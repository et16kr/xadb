//! S5: B's point and cursor reads over the actual C Row read path, with
//! the rows written by A's INSERT driver. Moved out of `row_live.rs` in S8
//! (contract `#row-single-source-s8`) unchanged, so that `row_live` keeps
//! only A's S2-e driver cases (C+A) and this module is the C+A+B part: it
//! links both A and B and is in neither single-source export.
use super::row_forward::files;
use super::row_live::finished;
use super::row_preflight::{create_table, evict, frames, with_pins};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, RetentionCause, RetentionError, RetentionPhase, RowPreflightStep, TableStep,
    },
    row::insert::driver::live::{LiveCall, finish_statement_live, insert_live},
    row_record::ValueRef,
};
use crate::transaction::concurrency::catalog_test_support as hook;

/// S5 factory retains exactly one Full charge until checked finish. Reopening
/// the same original Owner proves both reference rows and read progress return.
#[test]
fn s5r_r01_read_admission_returns_original_resources() {
    case("S5R-R01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let used = reader.catalog_test_source_used();
                println!(
                    "S5_SOURCE_OBSERVATIONS full/load/operation/page/candidate={:?}",
                    reader.row_test_source_observations()
                );
                let before = files(state);
                for _ in 0..2 {
                    let operation = reader
                        .row_operation(owner, &view, reader_private)
                        .unwrap_or_else(|e| panic!("read entry: {e:?}"));
                    assert!(reader.catalog_test_source_used() > used);
                    operation.finish().unwrap();
                    assert_eq!(reader.catalog_test_source_used(), used);
                    owner.check_idle().unwrap();
                }
                assert_eq!(files(state), before);
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
                let _ = (core, table);
            }
        );
    });
}

#[test]
fn s5r_r02_empty_table_and_r04_actual_capture_copy() {
    use crate::storage::disk::{Plan4ResidencyCursor, ReadStep};
    use crate::storage::row::{
        insert::{produce_insert, validate_insert},
        read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R02-R04", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut residency = Plan4ResidencyCursor::new();
                let cancel = Cancellation::default();
                let mut output = [0xA5; 64];
                let before = files(state);
                let read = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &view,
                    &table,
                    RowId::new(1).unwrap(),
                    &mut output,
                    &cancel,
                )
                .unwrap();
                assert!(matches!(read, ReadStep::Ready(PointRead::Missing)));
                assert_eq!(output, [0xA5; 64]);
                assert_eq!(files(state), before);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(p) => p,
                    _ => panic!("resident"),
                };
                let input =
                    validate_insert(&[ValueRef::Int64(7)], preflight.validation_lane().unwrap())
                        .unwrap();
                let mut reserved = preflight
                    .reserve(input)
                    .unwrap_or_else(|_| panic!("reserve"));
                let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                let applied = reserved
                    .prepare(proposal)
                    .unwrap_or_else(|_| panic!("prepare"))
                    .apply()
                    .unwrap_or_else(|_| panic!("apply"));
                let (observation, _) = applied.into_parts();
                let read = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &view,
                    &table,
                    observation.row(),
                    &mut output,
                    &cancel,
                )
                .unwrap();
                let ReadStep::Ready(PointRead::Found(copied)) = read else {
                    panic!("own row")
                };
                assert_eq!(copied.version().row, observation.row());
                let expected = crate::storage::row_record::decode_canonical_row(
                    &output[..copied.len()],
                    &[crate::api::Column {
                        name: "i".into(),
                        data_type: crate::api::DataType::Int64,
                        nullable: false,
                    }],
                )
                .unwrap();
                assert_eq!(expected.value(0).unwrap(), ValueRef::Int64(7));
                // Capture is source-backed; short output, stale candidate and
                // sequence exhaustion do not publish bytes or leak references.
                let stale = {
                    let mut op = reader.row_operation(owner, &view, reader_private).unwrap();
                    let page = match op.page(&table, &mut residency).unwrap() {
                        ReadStep::Ready(p) => p,
                        _ => panic!("resident"),
                    };
                    let candidate = page.capture(0).unwrap();
                    assert!(matches!(
                        op.status(&candidate, &mut residency).unwrap(),
                        ReadStep::Ready(
                            crate::storage::transaction_record::TransactionState::Active
                        )
                    ));
                    assert!(matches!(
                        op.missing_before(&candidate, &mut residency).unwrap(),
                        ReadStep::Ready(_)
                    ));
                    let mut short = [0xBC; 1];
                    assert!(op.copy_candidate(&candidate, &mut short).is_err());
                    assert_eq!(short, [0xBC]);
                    let stale = candidate.test_duplicate();
                    op.release_candidate(candidate).unwrap();
                    op.finish().unwrap();
                    stale
                };
                {
                    let mut op = reader.row_operation(owner, &view, reader_private).unwrap();
                    assert!(op.copy_candidate(&stale, &mut output).is_err());
                    op.test_set_sequence(u64::MAX);
                    let page = match op.page(&table, &mut residency).unwrap() {
                        ReadStep::Ready(p) => p,
                        _ => panic!("resident"),
                    };
                    assert!(page.capture(0).is_err());
                    op.finish().unwrap();
                }
                {
                    // Receiver has a current capture with the same sequence;
                    // captured=None cannot mask the foreign identity check.
                    let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                    let mut second = reader
                        .row_operation(&mut other, &view, reader_private)
                        .unwrap();
                    let second_page = match second.page(&table, &mut residency).unwrap() {
                        ReadStep::Ready(page) => page,
                        _ => panic!("resident"),
                    };
                    let local = second_page.capture(0).unwrap();
                    let before = output;
                    assert!(second.copy_candidate(&stale, &mut output).is_err());
                    assert!(second.status(&stale, &mut residency).is_err());
                    assert!(second.release_candidate(stale.test_duplicate()).is_err());
                    assert_eq!(output, before);
                    second.copy_candidate(&local, &mut output).unwrap();
                    second.release_candidate(local).unwrap();
                    second.finish().unwrap();
                    other.check_idle().unwrap();
                }
                {
                    use crate::storage::row::read::driver::{
                        CursorStep,
                        live::{next_live, open_cursor_live},
                    };
                    let mut cursor =
                        open_cursor_live(&view, &table, Some(observation.row()), None).unwrap();
                    let mut short = [0xB1; 1];
                    assert!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency
                            },
                            &mut short,
                            &cancel
                        )
                        .is_err()
                    );
                    assert_eq!(short, [0xB1]);
                    let cancelled = Cancellation::default();
                    cancelled.cancel();
                    assert!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency
                            },
                            &mut output,
                            &cancelled
                        )
                        .is_err()
                    );
                    let found = next_live(
                        &mut cursor,
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency,
                        },
                        &mut output,
                        &cancel,
                    )
                    .unwrap();
                    assert!(
                        matches!(found,ReadStep::Ready(CursorStep::Row(ref copy)) if copy.version().row==observation.row())
                    );
                    assert!(matches!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency
                            },
                            &mut output,
                            &cancel
                        )
                        .unwrap(),
                        ReadStep::Ready(CursorStep::End)
                    ));
                    cursor.close().unwrap();
                }
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.commit_row(owner, row, private).is_ok());
                assert!(!fence.is_fenced());
                let _ = core;
            }
        );
    });
}

#[test]
fn s5r_r06_old_and_new_statement_snapshot() {
    use crate::storage::{
        disk::{Plan4ResidencyCursor, ReadStep, TransactionOptions},
        row::{
            insert::{produce_insert, validate_insert},
            read::driver::{
                PointRead,
                live::{LiveCall as ReadCall, read_point_live},
            },
        },
    };
    use foundation::runtime::Cancellation;
    case("S5R-R06", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(p) => p,
                    _ => panic!("resident"),
                };
                let input =
                    validate_insert(&[ValueRef::Int64(7)], preflight.validation_lane().unwrap())
                        .unwrap();
                let mut reserved = preflight
                    .reserve(input)
                    .unwrap_or_else(|_| panic!("reserve"));
                let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                let applied = reserved
                    .prepare(proposal)
                    .unwrap_or_else(|_| panic!("prepare"))
                    .apply()
                    .unwrap_or_else(|_| panic!("apply"));
                let (observation, _) = applied.into_parts();
                let mut read_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let transaction = writer
                    .begin_transaction(
                        &mut read_owner,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer
                    .begin_statement(&mut read_owner, &transaction)
                    .unwrap();
                let q = old.q().unwrap();
                let cancel = Cancellation::default();
                let mut output = [0xAA; 64];
                let mut residency = Plan4ResidencyCursor::new();
                // Another actor's still Active INSERT is invisible, proven Missing.
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner: &mut read_owner,
                            private: reader_private,
                            residency: &mut residency
                        },
                        &old,
                        &table,
                        observation.row(),
                        &mut output,
                        &cancel
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert!(writer.finish_statement(owner, view).is_ok());
                let committed = writer
                    .commit_row(owner, row, private)
                    .unwrap_or_else(|_| panic!("commit"));
                assert!(committed.q() > q);
                // The same ReadCommitted statement retains its original q.
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner: &mut read_owner,
                            private: reader_private,
                            residency: &mut residency
                        },
                        &old,
                        &table,
                        observation.row(),
                        &mut output,
                        &cancel
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert_eq!(old.q().unwrap(), q);
                assert_eq!(output, [0xAA; 64]);
                assert!(writer.finish_statement(&mut read_owner, old).is_ok());
                let new = writer
                    .begin_statement(&mut read_owner, &transaction)
                    .unwrap();
                assert_eq!(new.q().unwrap(), committed.q());
                assert!(
                    matches!(read_point_live(ReadCall{port:&reader,owner:&mut read_owner,private:reader_private,residency:&mut residency},&new,&table,observation.row(),&mut output,&cancel).unwrap(),ReadStep::Ready(PointRead::Found(ref copy)) if copy.version().row==observation.row())
                );
                assert!(writer.finish_statement(&mut read_owner, new).is_ok());
                assert!(
                    writer
                        .finish_read_only(&mut read_owner, transaction)
                        .is_ok()
                );
                assert!(!fence.is_fenced());
                let _ = state;
            }
        );
    });
}

/// C/boundary join. Original ports are already in shared_bytes; operation,
/// its caller frames and each C callee are added once. This is a source-value
/// bound, not compiler stack/RSS measurement.
fn row_read_census<W: WriterNamespace>(kind: &str) {
    use crate::storage::{disk::ReadPort, row::read::driver::live};
    use std::mem::size_of as n;
    let c = ReadPort::<W>::row_test_load_inventory();
    let leaf = ReadPort::<W>::row_test_nonload_inventory();
    let b = live::test_load_callers::<W>();
    let nonload_b = live::test_nonload_caller::<W>();
    // Same-error `?`/match moves stay in their counted Result payloads.
    // Explicit conversion leaf slack: two whole error carriers plus eight
    // reference/scalar temporaries; ViewSample, merge, decode, extraction and
    // publication bodies are separate never-inline callees.
    // Eight words: cancel/error branch scalars(1), Range::next Option(2),
    // protected slice(2), backend and loader reborrows(2), conversion borrow(1).
    // Error values already in named Result payloads are not additional slots;
    // the two conversion temporaries cover the Source→RowReadError boundary.
    assert!(n::<crate::storage::row::read::driver::RowReadError>() <= n::<RetentionError>());
    let load_temporaries = 2 * n::<RetentionError>() + 8 * n::<&()>();
    let operation = c[6];
    let load = c[7];
    let full = c[8];
    let mut phases = Vec::new();
    for (name, offset) in [("point", 0), ("cursor", 3)] {
        phases.push((
            format!("{name}.catalog_load"),
            operation + load + c[0] + c[1] + b[offset] + load_temporaries,
        ));
        phases.push((
            format!("{name}.row_load"),
            operation + load + c[0] + c[2] + b[offset] + load_temporaries,
        ));
        phases.push((
            format!("{name}.status_load"),
            operation + load + c[9] + b[offset + 1] + load_temporaries,
        ));
        phases.push((
            format!("{name}.undo_load"),
            operation + load + c[10] + b[offset + 2] + load_temporaries,
        ));
        phases.push((
            format!("{name}.status_location"),
            operation + leaf[3] + c[9] + b[offset + 1] + load_temporaries,
        ));
        phases.push((
            format!("{name}.undo_location"),
            operation + leaf[3] + c[10] + b[offset + 2] + load_temporaries,
        ));
    }
    let parent = c[0].max(c[9]).max(c[10]);
    for (at, name) in [
        (0, "admission"),
        (1, "prepare_pages"),
        (2, "capture"),
        (4, "decode"),
        (5, "copy"),
        (6, "cleanup"),
    ] {
        phases.push((name.into(), operation + parent + leaf[at] + nonload_b));
    }
    println!(
        "S5_ROW_CENSUS kind={kind} full={full} operation={operation} load={load} B={b:?} nonload_B={nonload_b} C={c:?} leaf={leaf:?} slack={load_temporaries}"
    );
    for (name, bytes) in phases {
        println!("S5_ROW_PHASE {kind} {name} {bytes} {full}");
        assert!(bytes <= full, "{kind} {name}: {bytes}>{full}");
    }
}

#[test]
fn s5r_c01_memory_source_inventory() {
    row_read_census::<port::Writer>("memory");
}

#[cfg(target_os = "linux")]
#[test]
fn s5r_c02_native_source_inventory() {
    use super::super::super::super::plan4_pages::native;
    row_read_census::<native::Writer>("instrumented-native");
}

#[test]
fn s5r_r08_corrupt_row_tree_fences_and_preserves_cause() {
    use crate::storage::{
        dirty_frame::status_load_test_support,
        page::{PageKind, decode_page, encode_page},
        row::read::driver::{
            RowReadError,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R08", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let index = frames(pool, 12)
                    .into_iter()
                    .find(|(_, page, _)| *page == 13)
                    .unwrap()
                    .0;
                let identity = pool.target_request(index).unwrap().identity;
                // Damage grammar while keeping the original page identity, LSN,
                // generic envelope and checksum valid. No replacement authority.
                status_load_test_support::edit_content(pool, index, |bytes| {
                    let page = decode_page(bytes, identity, PageKind::RowLeaf).unwrap();
                    assert_eq!(page.len(), 1);
                    let header = page.header();
                    let mut metadata = page.cell(0).unwrap().to_vec();
                    metadata[0] ^= 1;
                    encode_page(header, &[metadata.as_slice()], bytes).unwrap();
                    decode_page(bytes, identity, PageKind::RowLeaf).unwrap();
                });
                let full = reader.row_test_source_observations()[0] as u64;
                let before = files(state);
                let mut output = [0xA9; 32];
                let error = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner,
                        private: reader_private,
                        residency: &mut Plan4ResidencyCursor::new(),
                    },
                    &view,
                    &table,
                    RowId::new(1).unwrap(),
                    &mut output,
                    &Cancellation::default(),
                )
                .err()
                .unwrap();
                assert!(
                    matches!(
                        error,
                        RowReadError::Source(RetentionError {
                            phase: RetentionPhase::Source,
                            effect: Effect::CallerMustResolve,
                            cause: RetentionCause::Tree(_)
                        })
                    ),
                    "{error:?}"
                );
                assert!(fence.is_fenced());
                assert_eq!(output, [0xA9; 32]);
                assert_eq!(files(state), before);
                // Fatal source bookkeeping stays quarantined, as for Catalog
                // reads: release must not manufacture reusable fenced capacity.
                assert_eq!(reader.catalog_test_source_used(), full);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                drop(view);
                drop(row);
            }
        );
    });
}

#[test]
fn s5r_r03_dirty_victim_flush_retries_the_same_snapshot() {
    use crate::storage::{
        disk::ReadStep,
        row::read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let index = frames(live.catalog_test_pool(), 12)
                    .into_iter()
                    .find(|(_, page, _)| *page == 13)
                    .unwrap()
                    .0;
                let leaf = live.row_test_resident_target(index).unwrap();
                evict(live, flusher, &leaf);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                assert!(resident_index(&writer, 13).is_none());
                let q = view.q().unwrap();
                let pool = reader.catalog_test_pool();
                let clean: Vec<usize> = frames(pool, 12)
                    .into_iter()
                    .filter(|(_, _, dirty)| !*dirty)
                    .map(|(index, ..)| index)
                    .collect();
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let before = files(state);
                let mut output = [0xA3; 32];
                let mut residency = Plan4ResidencyCursor::new();
                let cancel = Cancellation::default();
                let target = with_pins(pool, &mut holder, &clean, || {
                    match read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency,
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &cancel,
                    )
                    .unwrap()
                    {
                        ReadStep::FlushRequired { target } => target,
                        ReadStep::Ready(_) => {
                            panic!("nonresident leaf with only dirty eligible victims")
                        }
                    }
                });
                holder.check_idle().unwrap();
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(files(state), before);
                assert_eq!(output, [0xA3; 32]);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                owner.check_idle().unwrap();
                flush_target!(flush, flusher, target);
                let before = files(state);
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &cancel
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(files(state), before);
                assert_eq!(output, [0xA3; 32]);
                assert!(resident_index(&writer, 13).is_some());
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert!(!fence.is_fenced());
                // The read LOAD may legitimately evict the writer's Status.
                // Restore the writer preflight residency before its legacy
                // empty-abort fixture cleanup; the read assertions are above.
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[cfg(target_os = "linux")]
#[path = "row_read_live/native.rs"]
mod native_runtime;

#[test]
fn s5r_c03_original_full_admission_boundary_and_foreign_owner() {
    case("S5R-C03", || {
        let foreign_fence = crate::storage::session_fence::SessionFence::new();
        let foreign_core = PageConcurrency::new(&foreign_fence).unwrap();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let [shared, full, _] = reader.catalog_test_fixed_charges();
                let available = 65536 - shared - full;
                let before = files(state);
                let bytes = hex(reader_private);
                for held in [available + 1, available, available - 1] {
                    reader.catalog_test_with_held_source(held, || {
                        let refs = reader.catalog_test_references();
                        let result = reader.row_operation(owner, &view, reader_private);
                        if held > available {
                            let error = result.err().expect("one byte short");
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert!(matches!(error.cause, RetentionCause::Capacity));
                        } else {
                            let operation = result.unwrap();
                            assert_eq!(reader.catalog_test_source_used(), (held + full) as u64);
                            operation.finish().unwrap();
                        }
                        assert_eq!(reader.catalog_test_source_used(), held as u64);
                        assert_eq!(reader.catalog_test_references(), refs);
                        assert_eq!(hook::observe(core).active_read_operations, 0);
                        owner.check_idle().unwrap();
                    });
                    assert_eq!(reader.catalog_test_source_used(), 0);
                }
                let mut foreign = foreign_core.try_owner(OwnerRole::Foreground).unwrap();
                let error = reader
                    .row_operation(&mut foreign, &view, reader_private)
                    .err()
                    .unwrap();
                assert_eq!(error.effect, Effect::Unchanged);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                foreign.check_idle().unwrap();
                assert_eq!(hex(reader_private), bytes);
                assert_eq!(files(state), before);
                assert!(!fence.is_fenced());
                close_live!(writer, owner, flush, flusher, row, view, private);
                let _ = table;
            }
        );
    });
}

#[test]
fn s5r_r01_pending_preflight_precedes_read_source_reservation() {
    use crate::storage::{
        disk::ReadStep,
        row::read::driver::{
            PointRead, RowReadError,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R01-PENDING", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                let q = view.q().unwrap();
                let preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(preflight) => preflight,
                    _ => panic!("settled preflight"),
                };
                let used = reader.catalog_test_source_used();
                let refs = reader.catalog_test_references();
                let before = files(state);
                let loan = hex(reader_private);
                let mut output = [0xD1; 32];
                assert!(hook::observe(core).read_gate_closed);
                let error = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner: &mut late,
                        private: reader_private,
                        residency: &mut Plan4ResidencyCursor::new(),
                    },
                    &view,
                    &table,
                    RowId::new(1).unwrap(),
                    &mut output,
                    &Cancellation::default(),
                )
                .err()
                .unwrap();
                assert!(
                    matches!(error,RowReadError::Source(RetentionError{effect:Effect::Unchanged,cause:RetentionCause::Concurrency(ref e),..}) if e.cause==crate::transaction::concurrency::Cause::InstallationPending),
                    "{error:?}"
                );
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(reader.catalog_test_references(), refs);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(hex(reader_private), loan);
                assert_eq!(files(state), before);
                assert_eq!(output, [0xD1; 32]);
                preflight.finish().unwrap();
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner: &mut late,
                            private: reader_private,
                            residency: &mut Plan4ResidencyCursor::new()
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &Cancellation::default()
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(reader.catalog_test_source_used(), 0);
                late.check_idle().unwrap();
                assert!(!fence.is_fenced());
                close_live!(writer, owner, flush, flusher, row, view, private);
            }
        );
    });
}

#[test]
fn s5r_r05_r07_retiring_capture_and_corrupt_undo() {
    use crate::storage::{
        dirty_frame::status_load_test_support,
        disk::{ReadStep, TransactionOptions},
        page::{PageKind, decode_page, encode_page},
        row::{
            insert::{produce_insert, validate_insert},
            read::driver::{
                RowReadError,
                live::{LiveCall as ReadCall, read_point_live},
            },
        },
    };
    use foundation::runtime::Cancellation;
    case("S5R-R05-R07", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(p) => p,
                    _ => panic!("resident"),
                };
                let input =
                    validate_insert(&[ValueRef::Int64(7)], preflight.validation_lane().unwrap())
                        .unwrap();
                let mut reserved = preflight
                    .reserve(input)
                    .unwrap_or_else(|_| panic!("reserve"));
                let proposal = produce_insert(reserved.producer_lane().unwrap()).unwrap();
                let applied = reserved
                    .prepare(proposal)
                    .unwrap_or_else(|_| panic!("prepare"))
                    .apply()
                    .unwrap_or_else(|_| panic!("apply"));
                let (observation, _) = applied.into_parts();

                let mut residency = Plan4ResidencyCursor::new();
                let mut op = reader.row_operation(owner, &view, reader_private).unwrap();
                reader.row_test_with_retiring_undo(observation.undo(), || {
                    let page = match op.page(&table, &mut residency).unwrap() {
                        ReadStep::Ready(page) => page,
                        _ => panic!("resident"),
                    };
                    let error = page.capture(0).err().unwrap();
                    assert!(
                        matches!(error, RetentionError {effect: Effect::Unchanged,
                        cause: RetentionCause::Visibility(ref e), ..}
                        if e.cause == crate::storage::wal_io::VisibilityCause::Busy),
                        "{error:?}"
                    );
                    // The leaf has both Txn and Undo references; neither is
                    // installed when the second target is retiring.
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), true));
                    assert!(!fence.is_fenced());
                });
                let page = match op.page(&table, &mut residency).unwrap() {
                    ReadStep::Ready(page) => page,
                    _ => panic!("resident"),
                };
                let candidate = page.capture(0).unwrap();
                assert_eq!(reader.catalog_test_references(), ((2, 0, 0), false));
                op.release_candidate(candidate).unwrap();
                op.finish().unwrap();
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                let pool = reader.catalog_test_pool();
                let index = frames(pool, 12)
                    .into_iter()
                    .find(|(_, page, _)| *page == 14)
                    .unwrap()
                    .0;
                let identity = pool.target_request(index).unwrap().identity;
                status_load_test_support::edit_content(pool, index, |bytes| {
                    let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
                    let header = page.header();
                    let mut cells = (0..page.len())
                        .map(|i| page.cell(i).unwrap().to_vec())
                        .collect::<Vec<_>>();
                    cells[observation.undo().slot() as usize][0] ^= 1;
                    let refs = cells.iter().map(Vec::as_slice).collect::<Vec<_>>();
                    encode_page(header, &refs, bytes).unwrap();
                    decode_page(bytes, identity, PageKind::Undo).unwrap();
                });
                let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                let transaction = writer
                    .begin_transaction(
                        &mut other,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let other_view = writer.begin_statement(&mut other, &transaction).unwrap();
                let before = files(state);
                let mut output = [0xB9; 64];
                let error = read_point_live(
                    ReadCall {
                        port: &reader,
                        owner: &mut other,
                        private: reader_private,
                        residency: &mut residency,
                    },
                    &other_view,
                    &table,
                    observation.row(),
                    &mut output,
                    &Cancellation::default(),
                )
                .err()
                .unwrap();
                assert!(
                    matches!(
                        error,
                        RowReadError::Source(RetentionError {
                            phase: RetentionPhase::Census,
                            effect: Effect::CallerMustResolve,
                            cause: RetentionCause::Undo(_),
                        })
                    ),
                    "{error:?}"
                );
                assert!(fence.is_fenced());
                assert_eq!(output, [0xB9; 64]);
                assert_eq!(files(state), before);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                drop(other_view);
                drop(transaction);
                drop(view);
                drop(row);
            }
        );
    });
}

#[test]
fn s5r_r03_cold_catalog_anchor_loads_before_schema() {
    use crate::storage::{
        disk::ReadStep,
        row::read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        },
    };
    use foundation::{identity::RowId, runtime::Cancellation};
    case("S5R-R03-CATALOG", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let anchor_index = frames(live.catalog_test_pool(), 12)
                    .into_iter()
                    .find(|(_, p, _)| *p == 7)
                    .unwrap()
                    .0;
                let anchor = live.row_test_resident_target(anchor_index).unwrap();
                let jobs = (1..5)
                    .map(|index| live.target(Plan4Page::Jobs { index }).unwrap())
                    .find(|t| resident_index(live, t.identity().page.page_no).is_none())
                    .unwrap();
                row_statement!(live,owner,flusher,private,reader_private,row_status=>writer,reader,flush,row,view,table);
                flush_target!(flush, flusher, anchor);
                let pool = reader.catalog_test_pool();
                let anchor_index = frames(pool, 12)
                    .into_iter()
                    .find(|(_, p, _)| *p == 7)
                    .unwrap()
                    .0;
                let pinned: Vec<usize> = frames(pool, 12)
                    .into_iter()
                    .filter(|(index, _, dirty)| *index != anchor_index && !*dirty)
                    .map(|(index, ..)| index)
                    .collect();
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let mut op = reader.row_operation(owner, &view, reader_private).unwrap();
                with_pins(pool, &mut holder, &pinned, || {
                    assert!(matches!(
                        op.test_load(&jobs).unwrap(),
                        RetentionResidencyStep::Resident { loaded: true, .. }
                    ));
                });
                op.finish().unwrap();
                holder.check_idle().unwrap();
                assert!(resident_index(&writer, 7).is_none());
                assert!(resident_index(&writer, 8).is_some());
                let before = files(state);
                let mutations = state.lock().unwrap().mutations();
                let q = view.q().unwrap();
                let mut output = [0xCD; 32];
                assert!(matches!(
                    read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut Plan4ResidencyCursor::new()
                        },
                        &view,
                        &table,
                        RowId::new(1).unwrap(),
                        &mut output,
                        &Cancellation::default()
                    )
                    .unwrap(),
                    ReadStep::Ready(PointRead::Missing)
                ));
                assert!(resident_index(&writer, 7).is_some());
                assert_eq!(view.q().unwrap(), q);
                assert_eq!(files(state), before);
                assert_eq!(state.lock().unwrap().mutations(), mutations);
                assert_eq!(output, [0xCD; 32]);
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                close_live!(writer, owner, flush, flusher, row, view, private);
                assert!(!fence.is_fenced());
            }
        );
    });
}
