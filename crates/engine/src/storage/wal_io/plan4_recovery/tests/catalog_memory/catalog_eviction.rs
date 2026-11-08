//! CEV (dev3-visibility-retirement.md#catalog-capture-load): C's Catalog
//! lookup and the CheckedTable issuer after the Catalog anchor (page 7)
//! or leaf (page 8) left the pool. The pool's victim selection may replace
//! either page once it is clean, as any other page; a Full-class lookup
//! LOADs a missing one through the reader's own lookup LOAD before its
//! capture, keeping the anchor while the leaf loads, and returns a dirty
//! victim as FlushRequired. The resident class never LOADs and keeps its
//! retryable Busy. Expectations are fixture facts (the issued handle, the
//! pages resident, files and registrations unchanged), never values read
//! back from the same producer.
use super::row_forward::files;
use super::row_preflight::{create_table, evict, frames, with_pins};
use super::*;
use crate::storage::disk::{
    IsolationLevel, RetentionCause, RetentionError, RetentionPhase, TableStep, TransactionOptions,
};
use crate::transaction::concurrency::catalog_test_support::observe;

const ANCHOR: u64 = 7;
const LEAF: u64 = 8;

fn read_only() -> TransactionOptions {
    TransactionOptions {
        isolation: IsolationLevel::Serializable,
        read_only: true,
    }
}

/// Flush the Catalog page `page` (7 or 8) and load a non-resident genesis
/// Jobs page into its frame (`evict`), so it is no longer resident.
fn evict_catalog<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    flusher: &mut Owner<'c, 's>,
    page: u64,
) {
    let target = live
        .target(match page {
            ANCHOR => Plan4Page::CatalogAnchor,
            LEAF => Plan4Page::CatalogLeaf,
            other => panic!("Catalog page {other}"),
        })
        .unwrap();
    evict(live, flusher, &target);
    assert!(resident_index(live, page).is_none());
}

/// CEV-J01: after T's CREATE committed (q 1), the Catalog anchor, the leaf
/// or both are evicted. A statement's table issue (`finish_table`, Full
/// class) LOADs each missing page into a clean victim and issues T's
/// CheckedTable (object 6, generation 1, epoch 1, slot 0) with the catalog
/// flag and T's read bit registered. Both pages are resident afterwards,
/// the files are unchanged (a LOAD only reads), and the same statement's
/// ordinary lookup finds T. Before the fix this issue was refused
/// `Frame(MissingTarget)` Unchanged by the resident-only `read_catalog`.
#[test]
fn cev_j01_full_table_issue_loads_an_evicted_catalog_page() {
    for evicted in [&[ANCHOR][..], &[LEAF], &[ANCHOR, LEAF]] {
        case(&format!("CEV-J01 evicted={evicted:?}"), || {
            let oracle = Oracle::new();
            with_fresh!(
                oracle,
                frames = 12,
                |live,
                 owner,
                 flusher,
                 status,
                 undo,
                 private,
                 reader_private,
                 core,
                 fence,
                 state| {
                    create_table(live, owner, flusher, status, undo, private, false);
                    for page in evicted {
                        evict_catalog(live, flusher, *page);
                    }
                    let before = files(state);
                    let (mut writer, reader, _) = live.split();
                    let transaction = writer
                        .begin_transaction(owner, read_only(), reader_private)
                        .unwrap();
                    let view = writer.begin_statement(owner, &transaction).unwrap();
                    let call = reader.catalog_statement_call(owner, &view).unwrap();
                    let step = call
                        .finish_table(
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private,
                        )
                        .unwrap();
                    let TableStep::Table(table) = step else {
                        panic!("a committed visible Table issues after the LOAD")
                    };
                    let (handle, slot, _) = table.test_identity();
                    assert_eq!(
                        (
                            handle.object.get(),
                            handle.generation.get(),
                            handle.schema_epoch.get(),
                            slot
                        ),
                        (6, 1, 1, 0)
                    );
                    assert!(table.test_same_origin(core, fence));
                    assert_eq!(
                        (view.test_catalog_reads(), view.test_table_masks()),
                        (1, (1, 0))
                    );
                    assert!(resident_index(&writer, ANCHOR).is_some());
                    assert!(resident_index(&writer, LEAF).is_some());
                    assert_eq!(files(state), before);
                    assert_eq!(reader.catalog_test_source_used(), 0);
                    assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                    assert_eq!(writer.catalog_test_physical_pins(), 0);
                    assert_eq!(observe(core).active_read_operations, 0);
                    let mut call = reader.catalog_statement_call(owner, &view).unwrap();
                    let CatalogLookupStep::Found(record) = call
                        .lookup(
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private,
                        )
                        .unwrap()
                    else {
                        panic!("the same statement finds T")
                    };
                    assert_eq!(record.header().identity, handle);
                    call.finish_checked().unwrap();
                    assert!(writer.finish_statement(owner, view).is_ok());
                    assert!(writer.finish_read_only(owner, transaction).is_ok());
                    owner.check_idle().unwrap();
                    flusher.check_idle().unwrap();
                    assert!(!fence.is_fenced());
                }
            );
        });
    }
}

/// CEV-J02: the anchor evicted and every clean frame pinned by a separate
/// owner, so the anchor's LOAD has only dirty victims. The table issue
/// returns FlushRequired naming one of them, never the dirty leaf (the
/// anchor's LOAD keeps the leaf), with no effect: files, registrations,
/// source charge, reference pins and physical pins unchanged, the anchor
/// still not resident. After the caller flushes it, the same statement's
/// retry LOADs the anchor and issues T.
#[test]
fn cev_j02_dirty_victim_of_the_catalog_load_is_flush_required() {
    case("CEV-J02", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                create_table(live, owner, flusher, status, undo, private, false);
                evict_catalog(live, flusher, ANCHOR);
                let (mut writer, reader, mut flush) = live.split();
                let transaction = writer
                    .begin_transaction(owner, read_only(), reader_private)
                    .unwrap();
                let view = writer.begin_statement(owner, &transaction).unwrap();
                let pool = reader.catalog_test_pool();
                let all = frames(pool, 12);
                let clean: Vec<usize> = all
                    .iter()
                    .filter(|(_, _, dirty)| !*dirty)
                    .map(|(index, ..)| *index)
                    .collect();
                let dirty: Vec<u64> = all
                    .iter()
                    .filter(|(_, _, dirty)| *dirty)
                    .map(|(_, page, _)| *page)
                    .collect();
                assert!(!clean.is_empty() && !dirty.is_empty(), "{all:?}");
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let before = files(state);
                let pins = writer.catalog_test_physical_pins();
                let target = with_pins(pool, &mut holder, &clean, || {
                    let call = reader.catalog_statement_call(owner, &view).unwrap();
                    match call
                        .finish_table(
                            ObjectId::new(6).unwrap(),
                            &mut Plan4ResidencyCursor::new(),
                            reader_private,
                        )
                        .unwrap()
                    {
                        TableStep::FlushRequired { target } => target,
                        _ => panic!("only dirty victims: the anchor LOAD names one"),
                    }
                });
                holder.check_idle().unwrap();
                assert!(dirty.contains(&LEAF), "{all:?}");
                let victim = target.identity().page.page_no;
                assert!(dirty.contains(&victim) && victim != LEAF, "{target:?}");
                assert_eq!(files(state), before);
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (0, (0, 0))
                );
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(writer.catalog_test_physical_pins(), pins);
                assert!(resident_index(&writer, ANCHOR).is_none());
                owner.check_idle().unwrap();
                flush_target!(flush, flusher, target);
                let call = reader.catalog_statement_call(owner, &view).unwrap();
                let TableStep::Table(table) = call
                    .finish_table(
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .unwrap()
                else {
                    panic!("the retry after the flush issues T")
                };
                assert_eq!(table.test_identity().0.object.get(), 6);
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (1, (1, 0))
                );
                assert!(resident_index(&writer, ANCHOR).is_some());
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.finish_read_only(owner, transaction).is_ok());
                owner.check_idle().unwrap();
                flusher.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// CEV-J03: the resident class does not LOAD. With the anchor evicted, a
/// resident statement call's table issue is refused Busy Unchanged (the
/// class's retryable miss), registers nothing and leaves the anchor
/// non-resident and the files unchanged. A Full-class call of the same
/// statement then LOADs it and issues T.
#[test]
fn cev_j03_resident_class_keeps_its_retryable_miss() {
    case("CEV-J03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                create_table(live, owner, flusher, status, undo, private, false);
                evict_catalog(live, flusher, ANCHOR);
                let before = files(state);
                let (mut writer, reader, _) = live.split();
                let transaction = writer
                    .begin_transaction(owner, read_only(), reader_private)
                    .unwrap();
                let view = writer.begin_statement(owner, &transaction).unwrap();
                let call = reader
                    .catalog_resident_statement_call(owner, &view)
                    .unwrap();
                let error = match call.finish_table(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    reader_private,
                ) {
                    Err(error) => error,
                    Ok(_) => panic!("the resident class does not LOAD the anchor"),
                };
                assert!(
                    matches!(
                        error,
                        RetentionError {
                            phase: RetentionPhase::Census,
                            cause: RetentionCause::Busy,
                            effect: Effect::Unchanged,
                        }
                    ),
                    "{error:?}"
                );
                assert!(resident_index(&writer, ANCHOR).is_none());
                assert_eq!(files(state), before);
                assert_eq!(
                    (view.test_catalog_reads(), view.test_table_masks()),
                    (0, (0, 0))
                );
                assert_eq!(reader.catalog_test_source_used(), 0);
                assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
                assert_eq!(observe(core).active_read_operations, 0);
                let call = reader.catalog_statement_call(owner, &view).unwrap();
                let TableStep::Table(table) = call
                    .finish_table(
                        ObjectId::new(6).unwrap(),
                        &mut Plan4ResidencyCursor::new(),
                        reader_private,
                    )
                    .unwrap()
                else {
                    panic!("the Full class LOADs the anchor and issues T")
                };
                assert_eq!(table.test_identity().0.object.get(), 6);
                assert!(resident_index(&writer, ANCHOR).is_some());
                assert_eq!(files(state), before);
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.finish_read_only(owner, transaction).is_ok());
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}
