//! S2-b1: the Row write preflight (S2-0 Row write amendment §1 "order" and
//! "residency", S2-0b `#row-write-validate-once`) over an actual committed
//! CREATE of table T. Only the cases preflight alone can exercise are here
//! (S2W-R01, R02, R03, R05, R16, R17, R18, R19, R20 and the preflight half of
//! R22); every other S2W case needs reserve, prepare, apply or cancel. The
//! expected values are fixture facts (pages, LSNs, masks) and the refusal
//! table's (phase, cause, effect), never values copied from the body.
use super::super::super::super::plan4_pages::RetentionFlushPhase;
use super::row_transactions::second_status;
use super::*;
use crate::storage::{
    dirty_frame::status_load_test_support,
    disk::{
        IsolationLevel, RetentionCause, RetentionError, RetentionPhase, RowPreflightStep,
        TableStep, TransactionOptions,
    },
    page::{self, PageKind},
    undo_record,
};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use std::sync::mpsc::sync_channel;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);

/// One System INIT (Status or Undo) through the original exclusive owner
/// (row_gate's `system_init`); the page is the allocator's choice.
fn system_init<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    undo: bool,
) -> Plan4Target {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..32 {
        let step = if undo {
            live.prepare_undo_slot(owner, &mut cursor)
        } else {
            live.prepare_status_slot(owner, &mut cursor)
        }
        .unwrap();
        match step {
            None => {
                let reserved = if undo {
                    live.reserve_undo(owner)
                } else {
                    live.reserve_status(owner)
                }
                .unwrap();
                return match reserved.initialize() {
                    Ok(target) => target,
                    Err(RetentionReservationError::Before { error, .. })
                    | Err(RetentionReservationError::Fatal(error)) => {
                        panic!("actual INIT {error:?}")
                    }
                };
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected System slot preparation"),
        }
    }
    panic!("bounded System preparation exhausted");
}

/// The committed table T (object 6) of S1E-R05T: the Row actor's Status
/// (page 11) before the CREATE pair, then the Catalog actor's CREATE and
/// Commit on Status 9 with Undo 10 (its root keeps Undo 10). With
/// `free_undo`, one more Undo INIT after the Commit is the Row actor's Free
/// Undo container (row_gate's fixture order).
fn create_table<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    status: &Plan4Target,
    undo: &Plan4Target,
    private: &mut [u8; PRIVATE_BYTES],
    free_undo: bool,
) -> (Plan4Target, Option<Plan4Target>) {
    let row_status = second_status(live, owner, flusher);
    assert_eq!(row_status.identity().page, page_ref(11, 2));
    prepare_create(live, owner, flusher, status, undo);
    {
        let (mut writer, _, mut flush) = live.split();
        let tx = writer
            .begin_catalog_transaction(
                owner,
                IsolationLevel::Serializable,
                status,
                private,
                &mut flush,
            )
            .unwrap();
        let view = writer.begin_statement(owner, &tx).unwrap();
        writer
            .catalog_test_forward_in_statement(
                owner,
                &view,
                undo,
                &table_spec("T"),
                private,
                &mut flush,
            )
            .unwrap();
        assert!(writer.finish_statement(owner, view).is_ok());
        match writer.commit_catalog_transaction(owner, tx, private) {
            Ok(commit) => assert_eq!(commit.q(), 1),
            Err(_) => panic!("Catalog Commit"),
        }
    }
    let row_undo = free_undo.then(|| system_init(live, owner, flusher, true));
    (row_status, row_undo)
}

/// Makes a clean resident Undo or Status page non-resident: flush it, then
/// load a non-resident genesis Jobs page into its frame (resident_faults'
/// `evict`).
fn evict<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    flusher: &mut Owner<'c, 's>,
    target: &Plan4Target,
) {
    drive_flush(live, flusher, Some(target));
    let index = match live
        .residency_step(target, &mut Plan4ResidencyCursor::new())
        .unwrap()
    {
        RetentionResidencyStep::Resident { index, frame, .. } => {
            assert!(frame.rec_lsn.is_none());
            index
        }
        _ => panic!("resident victim"),
    };
    let jobs = (1..5)
        .map(|index| live.target(Plan4Page::Jobs { index }).unwrap())
        .find(|t| resident_index(live, t.identity().page.page_no).is_none())
        .unwrap();
    assert!(live.load_into(index, &jobs).unwrap().before.is_some());
    assert!(resident_index(live, target.identity().page.page_no).is_none());
}

/// The CheckedTable of T for a Row statement, through `finish_table`; a
/// residency FlushRequired of the reader is flushed and retried.
macro_rules! issue_table {
    ($reader:ident, $owner:ident, $view:ident, $reader_private:ident, $flush:ident, $flusher:ident) => {{
        let mut issued = None;
        for _ in 0..8 {
            let call = $reader.catalog_statement_call($owner, &$view).unwrap();
            match call
                .finish_table(
                    ObjectId::new(6).unwrap(),
                    &mut Plan4ResidencyCursor::new(),
                    $reader_private,
                )
                .unwrap()
            {
                TableStep::Table(table) => {
                    issued = Some(table);
                    break;
                }
                TableStep::FlushRequired { target } => flush_target!($flush, $flusher, target),
                TableStep::Missing => panic!("a committed visible Table issues"),
            }
        }
        issued.expect("bounded table issue")
    }};
}

/// The original flush of one target through the split MaintenancePort.
macro_rules! flush_target {
    ($flush:ident, $flusher:ident, $target:expr) => {{
        let mut step = $flush.try_flush($flusher, &$target).unwrap();
        for _ in 0..8 {
            if matches!(
                step,
                RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
            ) {
                break;
            }
            step = $flush.continue_flush($flusher).unwrap();
        }
        assert!(matches!(
            step,
            RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
        ));
    }};
}

/// Preflights until Ready, flushing each `FlushRequired` victim, then
/// finishes the Ready preflight. Returns its cut and the victims flushed.
macro_rules! settle {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident, $flusher:ident) => {{
        let mut flushed = Vec::new();
        loop {
            assert!(flushed.len() < 16, "bounded preflight residency");
            match $writer
                .preflight_row($owner, &$view, &$table, $private, &mut $flush)
                .unwrap()
            {
                RowPreflightStep::Ready(preflight) => {
                    let cut = preflight.test_cut();
                    preflight.finish().unwrap();
                    break (cut, flushed);
                }
                RowPreflightStep::FlushRequired { target } => {
                    flush_target!($flush, $flusher, target);
                    flushed.push(target);
                }
            }
        }
    }};
}

/// Runs `run` while a separate Foreground owner holds a target-set read of
/// the given frames (MCL03's pinned Root), so no LOAD may take one of them as
/// its victim.
fn with_pins<'c, 's, R>(
    pool: &crate::storage::dirty_frame::SharedPool<'c, 's, '_, '_>,
    holder: &mut Owner<'c, 's>,
    indices: &[usize],
    run: impl FnOnce() -> R,
) -> R {
    let mut requests: Vec<_> = indices
        .iter()
        .map(|index| pool.target_request(*index).unwrap())
        .collect();
    requests.sort_by_key(|request| request.identity.page.page_no);
    macro_rules! held {
        ($($n:literal)*) => {
            match requests.len() {
                $($n => {
                    let set: &[_; $n] = requests.as_slice().try_into().unwrap();
                    let read = pool.reserve_target_set(holder, set).unwrap().try_read().unwrap();
                    let result = run();
                    read.release_checked().unwrap();
                    result
                })*
                0 => run(),
                count => panic!("{count} pinned frames"),
            }
        };
    }
    held!(1 2 3 4 5 6 7 8 9)
}

/// Registered frames (index, page number, dirty) among the first `count`.
fn frames(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    count: usize,
) -> Vec<(usize, u64, bool)> {
    let mut bytes = boxed::<PAGE_BYTES>();
    (1..count)
        .filter_map(|index| {
            let request = pool.target_request(index).ok()?;
            let snapshot = probe::diagnose(pool, index, &mut bytes);
            Some((index, request.identity.page.page_no, snapshot.dirty))
        })
        .collect()
}

/// The Row transaction on the Row Status, its first statement and the
/// CheckedTable of T, over a new split of `live`.
macro_rules! row_statement {
    ($live:ident, $owner:ident, $flusher:ident, $private:ident, $reader_private:ident,
     $row_status:ident => $writer:ident, $reader:ident, $flush:ident, $row:ident, $view:ident,
     $table:ident) => {
        let (mut $writer, $reader, mut $flush) = $live.split();
        let $row = $writer
            .begin_row_transaction(
                $owner,
                IsolationLevel::Serializable,
                &$row_status,
                $private,
                &mut $flush,
            )
            .unwrap();
        let $view = $writer.begin_statement($owner, &$row).unwrap();
        let $table = issue_table!($reader, $owner, $view, $reader_private, $flush, $flusher);
    };
}

/// Ends the statement and the empty Row transaction (RollbackStart and
/// AbortEnd), then the owners are idle.
macro_rules! close_row {
    ($writer:ident, $owner:ident, $row:ident, $view:ident, $private:ident, $flush:ident) => {
        assert!($writer.finish_statement($owner, $view).is_ok());
        assert!(
            $writer
                .abort_empty_transaction($owner, $row, $private, &mut $flush)
                .is_ok()
        );
        $owner.check_idle().unwrap();
    };
}

/// (WAL complete LSN, every file image): a preflight's effect is none.
macro_rules! effects {
    ($writer:ident, $state:ident) => {
        (
            $writer.free_undo_test_state().unwrap().0.complete_lsn,
            $state.lock().unwrap().images(),
        )
    };
}

/// (catalog flag, table read mask, table write mask) of the transaction.
macro_rules! reads {
    ($view:ident) => {{
        let (read, write) = $view.test_table_masks();
        ($view.test_catalog_reads(), read, write)
    }};
}

/// An Unchanged refusal returned the admission and the gate, registered
/// nothing and changed no WAL or file.
macro_rules! returned_unchanged {
    ($writer:ident, $reader:ident, $view:ident, $state:ident, $core:ident, $fence:ident,
     $used:expr, $effects:expr) => {
        assert!(!hook::observe($core).read_gate_closed);
        assert_eq!(hook::observe($core).active_read_operations, 0);
        assert_eq!($reader.catalog_test_source_used(), $used);
        assert_eq!(reads!($view), (0, 0, 0));
        assert_eq!(effects!($writer, $state), $effects);
        assert!(!$fence.is_fenced());
    };
}

/// S2W-R03: preflight registers the read bit of the table's slot and the
/// catalog flag, holds the gate and the RowWrite charge while Ready, and
/// `finish` returns the charge, then the gate, then the lease, with no WAL or
/// file effect. The cut is the fixture's: T = allocator 0, Root 1, Jobs 2
/// (RootJob and Terminal), Status 11, anchor 12, RowLeaf 13 and the Free
/// Undo 14 in rank order, no head Undo, the empty leaf's 16352 − 128 − 32 −
/// 4 = 16188 free bytes, and the complete LSN of the WAL.
#[test]
fn s2w_r03_preflight_registers_reads_and_finish_returns_the_lease() {
    case("S2W-R03", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                assert_eq!(row_undo.unwrap().identity().page, page_ref(14, 2));
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let (_, slot, anchor) = table.test_identity();
                assert_eq!((slot, anchor.page), (0, page_ref(12, 2)));
                let (settled, flushed) =
                    settle!(writer, owner, view, table, private, flush, flusher);
                println!(
                    "S2W_R03 settle_flushed={:?}",
                    flushed
                        .iter()
                        .map(|target| target.identity().page.page_no)
                        .collect::<Vec<_>>()
                );
                view.test_clear_reads();
                // RowWrite reuses the CatalogProgressCharge (44304, the S2-0
                // Q06 record).
                let charge = writer.catalog_test_retirement_charge() as u64;
                assert_eq!(charge, 44_304);
                let used = reader.catalog_test_source_used();
                let before = effects!(writer, state);
                let RowPreflightStep::Ready(preflight) = writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                else {
                    panic!("every source is resident after the settle");
                };
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert_eq!(reads!(view), (1, 1 << slot, 0));
                let cut = preflight.test_cut();
                preflight.finish().unwrap();
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert_eq!(effects!(writer, state), before);
                assert_eq!(reads!(view), (1, 1 << slot, 0));
                let (source, complete_lsn, roles, leaf_free, root_job) = cut;
                println!(
                    "S2W_R03 cut source={} complete_lsn={complete_lsn} roles={roles:#x} \
                     leaf_free={leaf_free} root_job={root_job}",
                    hex(&source)
                );
                // The same sources observe the same cut.
                assert_eq!(cut, settled);
                assert_eq!(complete_lsn, before.0);
                // T in rank order is [allocator, Root 1, Jobs 2, Status 11,
                // anchor 12, RowLeaf 13, Undo 14]: the count 7 above one
                // position nibble per role, from PreviousUndo (absent, F)
                // down to Root (1).
                assert_eq!(roles, (7 << 36) | 0xF_4056_3221);
                assert_eq!(leaf_free, 16_188);
                // The first Free Job cell from the Root cursor (one of the
                // 64 cells); its ordinal is the fixture's and is printed.
                assert!(root_job < 64);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// The preflight sources (page numbers) of the fixture's row: Root, Jobs 0,
/// the Catalog leaf, the Row Status, the table anchor and the RowLeaf; the
/// Free Undo 14 is the one source a test makes non-resident.
const SOURCES: [u64; 6] = [1, 2, 8, 11, 12, 13];
const FREE_UNDO: u64 = 14;

/// S2W-R01: a Catalog view and a read-only view are the mixed refusal
/// (Prepare, Unsupported) and an ended view is the view boundary's (Census,
/// WrongState), all Unchanged before the gate and the RowWrite source, with
/// every read set as it was. The same CheckedTable then preflights Ready from
/// a live Row view.
#[test]
fn s2w_r01_catalog_read_only_and_stale_views_are_refused_before_the_gate() {
    case("S2W-R01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 13,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let catalog_status = system_init(live, owner, flusher, false);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                // The CheckedTable outlives the statement that issued it.
                assert!(writer.finish_statement(owner, view).is_ok());
                let refused = |error: RetentionError, phase, unsupported: bool| {
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert_eq!(error.phase, phase);
                    if unsupported {
                        assert!(matches!(error.cause, RetentionCause::Unsupported));
                    } else {
                        assert!(matches!(error.cause, RetentionCause::WrongState));
                    }
                };
                // A Catalog view.
                let catalog = writer
                    .begin_catalog_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &catalog_status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let catalog_view = writer.begin_statement(owner, &catalog).unwrap();
                let (used, before, catalog_reads) = (
                    reader.catalog_test_source_used(),
                    effects!(writer, state),
                    reads!(catalog_view),
                );
                let error = writer
                    .preflight_row(owner, &catalog_view, &table, private, &mut flush)
                    .err()
                    .unwrap();
                refused(error, RetentionPhase::Prepare, true);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(reads!(catalog_view), catalog_reads);
                assert_eq!(effects!(writer, state), before);
                assert!(writer.finish_statement(owner, catalog_view).is_ok());
                assert!(
                    writer
                        .abort_empty_transaction(owner, catalog, private, &mut flush)
                        .is_ok()
                );
                // A read-only view.
                let read_only = writer
                    .begin_transaction(
                        owner,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        private,
                    )
                    .unwrap();
                let read_view = writer.begin_statement(owner, &read_only).unwrap();
                let (used, before, read_reads) = (
                    reader.catalog_test_source_used(),
                    effects!(writer, state),
                    reads!(read_view),
                );
                let error = writer
                    .preflight_row(owner, &read_view, &table, private, &mut flush)
                    .err()
                    .unwrap();
                refused(error, RetentionPhase::Prepare, true);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(reads!(read_view), read_reads);
                assert_eq!(effects!(writer, state), before);
                assert!(writer.finish_statement(owner, read_view).is_ok());
                assert!(writer.finish_read_only(owner, read_only).is_ok());
                // An ended (stale) Row view.
                let view = writer.begin_statement(owner, &row).unwrap();
                let (used, before, row_reads) = (
                    reader.catalog_test_source_used(),
                    effects!(writer, state),
                    reads!(view),
                );
                assert_eq!(row_reads, (1, 1, 0));
                view.test_set_stale(true);
                let error = writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .err()
                    .unwrap();
                refused(error, RetentionPhase::Census, false);
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(reads!(view), row_reads);
                assert_eq!(effects!(writer, state), before);
                view.test_set_stale(false);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R02: with a test-only held loan one byte beyond what leaves the
/// CatalogProgressCharge, the RowWrite admission is Unchanged Capacity: the
/// gate is reopened and nothing is registered or written. One byte less and
/// the same preflight is Ready with the charge held until `finish`.
#[test]
fn s2w_r02_source_shortage_reopens_the_gate_with_no_effect() {
    case("S2W-R02", || {
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
                view.test_clear_reads();
                let shared = writer.catalog_test_commit_charges()[0];
                let charge = writer.catalog_test_retirement_charge();
                let used = reader.catalog_test_source_used();
                let held = 65_536 - shared - charge - used as usize + 1;
                let before = effects!(writer, state);
                reader.catalog_test_with_held_source(held, || {
                    let error = writer
                        .preflight_row(owner, &view, &table, private, &mut flush)
                        .err()
                        .unwrap();
                    println!("S2W_R02 held={held} refusal={error:?}");
                    assert_eq!(error.phase, RetentionPhase::Census);
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(matches!(error.cause, RetentionCause::Capacity));
                    returned_unchanged!(
                        writer,
                        reader,
                        view,
                        state,
                        core,
                        fence,
                        used + held as u64,
                        before
                    );
                });
                reader.catalog_test_with_held_source(held - 1, || {
                    let RowPreflightStep::Ready(preflight) = writer
                        .preflight_row(owner, &view, &table, private, &mut flush)
                        .unwrap()
                    else {
                        panic!("resident sources");
                    };
                    assert_eq!(reader.catalog_test_source_used(), (65_536 - shared) as u64);
                    assert!(hook::observe(core).read_gate_closed);
                    preflight.finish().unwrap();
                    assert!(!hook::observe(core).read_gate_closed);
                    assert_eq!(reader.catalog_test_source_used(), used + held as u64 - 1);
                });
                assert_eq!(reader.catalog_test_source_used(), used);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R05: with no Free Undo container (the Catalog root keeps Undo 10),
/// with the only candidate's page header incarnation off its allocator entry
/// generation, or with its slot0 not Free, preflight is Unchanged (Prepare,
/// Capacity) and returns the source and the gate with no registration or
/// effect. The two corruptions are re-encoded in the resident frame only.
#[test]
fn s2w_r05_no_free_undo_is_unchanged_capacity() {
    case("S2W-R05", || r05("none"));
}

#[test]
fn s2w_r05_incarnation_off_its_entry_is_unchanged_capacity() {
    case("S2W-R05", || r05("incarnation"));
}

#[test]
fn s2w_r05_slot0_not_free_is_unchanged_capacity() {
    case("S2W-R05", || r05("slot0"));
}

fn r05(variant: &str) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 12,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let (row_status, row_undo) = create_table(
                live,
                owner,
                flusher,
                status,
                undo,
                private,
                variant != "none",
            );
            row_statement!(live, owner, flusher, private, reader_private, row_status
                => writer, reader, flush, row, view, table);
            if let Some(row_undo) = row_undo {
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let identity = row_undo.identity();
                let index = resident_index(&writer, FREE_UNDO).unwrap();
                status_load_test_support::edit_content(
                    reader.catalog_test_pool(),
                    index,
                    |bytes| {
                        let lsn = undo_record::initial_undo_page_lsn(bytes, identity).unwrap();
                        if variant == "incarnation" {
                            let mut moved = identity;
                            moved.page.incarnation =
                                Generation::new(identity.page.incarnation.get() + 1).unwrap();
                            undo_record::encode_initial_undo_page(moved, lsn, bytes).unwrap();
                        } else {
                            let page = page::decode_page(bytes, identity, PageKind::Undo).unwrap();
                            let header = page.header();
                            let mut cell = page.cell(0).unwrap().to_vec();
                            cell[6] = 1;
                            page::encode_page(header, &[&cell], bytes).unwrap();
                        }
                    },
                );
            } else {
                // Settle residency up to the Free Undo selection.
                for _ in 0..16 {
                    match writer.preflight_row(owner, &view, &table, private, &mut flush) {
                        Ok(RowPreflightStep::FlushRequired { target }) => {
                            flush_target!(flush, flusher, target)
                        }
                        Ok(RowPreflightStep::Ready(_)) => panic!("no Free Undo"),
                        Err(_) => break,
                    }
                }
            }
            view.test_clear_reads();
            let used = reader.catalog_test_source_used();
            let before = effects!(writer, state);
            let error = writer
                .preflight_row(owner, &view, &table, private, &mut flush)
                .err()
                .unwrap();
            println!("S2W_R05 variant={variant} refusal={error:?}");
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Prepare, Effect::Unchanged)
            );
            assert!(matches!(error.cause, RetentionCause::Capacity));
            returned_unchanged!(writer, reader, view, state, core, fence, used, before);
            assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
            close_row!(writer, owner, row, view, private, flush);
        }
    );
}

/// S2W-R16: the Free Undo 14 is not resident and every clean frame that is
/// not a source is pinned by another owner, so its LOAD needs a dirty
/// victim: `FlushRequired{target}` names one of the unpinned dirty frames,
/// registers nothing, writes nothing and returns the source, the gate and
/// the lease. After that flush the same preflight loads the Undo and is
/// Ready.
#[test]
fn s2w_r16_dirty_victim_is_flush_required_then_ready() {
    case("S2W-R16", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                evict(live, flusher, &row_undo.unwrap());
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                view.test_clear_reads();
                let pool = reader.catalog_test_pool();
                let listed = frames(pool, 12);
                println!("S2W_R16 frames={listed:?}");
                let clean: Vec<usize> = listed
                    .iter()
                    .filter(|(_, page, dirty)| !dirty && !SOURCES.contains(page))
                    .map(|(index, ..)| *index)
                    .collect();
                let dirty: Vec<u64> = listed
                    .iter()
                    .filter(|(_, page, dirty)| *dirty && !SOURCES.contains(page))
                    .map(|(_, page, _)| *page)
                    .collect();
                assert!(!dirty.is_empty());
                assert!(resident_index(&writer, FREE_UNDO).is_none());
                let used = reader.catalog_test_source_used();
                let before = effects!(writer, state);
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let target = with_pins(pool, &mut holder, &clean, || {
                    match writer
                        .preflight_row(owner, &view, &table, private, &mut flush)
                        .unwrap()
                    {
                        RowPreflightStep::FlushRequired { target } => target,
                        RowPreflightStep::Ready(_) => panic!("a dirty victim is required"),
                    }
                });
                holder.check_idle().unwrap();
                println!(
                    "S2W_R16 victim={} dirty={dirty:?}",
                    target.identity().page.page_no
                );
                assert!(dirty.contains(&target.identity().page.page_no));
                returned_unchanged!(writer, reader, view, state, core, fence, used, before);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(resident_index(&writer, FREE_UNDO).is_none());
                flush_target!(flush, flusher, target);
                let (_, flushed) = settle!(writer, owner, view, table, private, flush, flusher);
                assert!(flushed.is_empty());
                assert!(resident_index(&writer, FREE_UNDO).is_some());
                assert_eq!(reads!(view), (1, 1, 0));
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R17: the flush lease is checked before the gate. A flush I/O origin
/// of another binding fences (CallerMustResolve, BindingMismatch), WaitingWal
/// is Unchanged Busy, a Ready phase over a retained capture fences
/// (CallerMustResolve, Invariant), and an I/O preflight refusal is its
/// `Flush(…)` (here Unchanged Capacity: no selector generation headroom).
/// None of them closes the gate, takes the source or registers anything.
#[test]
fn s2w_r17_flush_origin_mismatch_fences_before_the_gate() {
    case("S2W-R17", || r17(1));
}

#[test]
fn s2w_r17_flush_waiting_wal_is_unchanged_busy() {
    case("S2W-R17", || r17(2));
}

#[test]
fn s2w_r17_flush_ready_over_retained_capture_fences() {
    case("S2W-R17", || r17(0));
}

#[test]
fn s2w_r17_flush_io_preflight_refusal_is_its_own() {
    case("S2W-R17", || r17(3));
}

fn r17(kind: u8) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 12,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            let root = live.target(Plan4Page::Root).unwrap();
            row_statement!(live, owner, flusher, private, reader_private, row_status
                => writer, reader, flush, row, view, table);
            let _ = settle!(writer, owner, view, table, private, flush, flusher);
            if kind == 0 {
                // A pinned dirty Root retires its flush pending
                // finalize: the capture stays (MCL03).
                let index = resident_index(&writer, 1).unwrap();
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let step = with_pins(reader.catalog_test_pool(), &mut holder, &[index], || {
                    flush.try_flush(flusher, &root).unwrap()
                });
                assert!(matches!(step, RetentionFlushStep::RetiredPendingFinalize));
                holder.check_idle().unwrap();
            }
            flush.row_test_flush_fault(kind);
            view.test_clear_reads();
            let used = reader.catalog_test_source_used();
            let before = effects!(writer, state);
            let error = writer
                .preflight_row(owner, &view, &table, private, &mut flush)
                .err()
                .unwrap();
            println!("S2W_R17 kind={kind} refusal={error:?}");
            assert_eq!(error.phase, RetentionPhase::Prepare);
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(reader.catalog_test_source_used(), used);
            assert_eq!(reads!(view), (0, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before.1);
            match kind {
                1 | 0 => {
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    if kind == 1 {
                        assert!(matches!(error.cause, RetentionCause::BindingMismatch));
                    } else {
                        assert!(matches!(error.cause, RetentionCause::Invariant));
                    }
                    assert!(fence.is_fenced());
                    drop(view);
                    drop(row);
                }
                _ => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    if kind == 2 {
                        assert!(matches!(error.cause, RetentionCause::Busy));
                        flush.row_test_flush_fault(0);
                    } else {
                        assert!(matches!(error.cause, RetentionCause::Flush(_)));
                    }
                    assert!(!fence.is_fenced());
                    assert_eq!(effects!(writer, state), before);
                    if kind == 2 {
                        let _ = settle!(writer, owner, view, table, private, flush, flusher);
                    }
                    close_row!(writer, owner, row, view, private, flush);
                }
            }
        }
    );
}

/// S2W-R18: an actor whose Row Job is RootReserved (a one-shot cfg(test)
/// view of its decoded Jobs; no live Reserve exists before S2-b2) is
/// Unchanged (Prepare, WrongState) after the gate, and the refusal returns
/// the source, then the gate, then the lease. The next preflight is Ready.
#[test]
fn s2w_r18_root_reserved_actor_is_wrong_state() {
    case("S2W-R18", || {
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
                view.test_clear_reads();
                let used = reader.catalog_test_source_used();
                let before = effects!(writer, state);
                let once = view.test_root_reserved_once();
                let error = writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .err()
                    .unwrap();
                drop(once);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::WrongState));
                returned_unchanged!(writer, reader, view, state, core, fence, used, before);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                assert_eq!(reads!(view), (1, 1, 0));
                close_row!(writer, owner, row, view, private, flush);
            }
        );
    });
}

/// S2W-R19: the LOAD of the non-resident Free Undo 14. A claim with every
/// non-source frame pinned is refused Unchanged (Load, Busy) and returns the
/// source and the gate; a page read error is Fatal (Load, PageIo) and a
/// corrupt home page is Fatal (Load) of its own cause, each with the fence.
#[test]
fn s2w_r19_load_claim_busy_is_unchanged() {
    case("S2W-R19", || r19("busy"));
}

#[test]
fn s2w_r19_load_page_io_is_fatal() {
    case("S2W-R19", || r19("io"));
}

#[test]
fn s2w_r19_load_corrupt_page_keeps_its_cause() {
    case("S2W-R19", || r19("decode"));
}

fn r19(variant: &str) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 12,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let (row_status, row_undo) =
                create_table(live, owner, flusher, status, undo, private, true);
            evict(live, flusher, &row_undo.unwrap());
            row_statement!(live, owner, flusher, private, reader_private, row_status
                => writer, reader, flush, row, view, table);
            view.test_clear_reads();
            let pool = reader.catalog_test_pool();
            let others: Vec<usize> = frames(pool, 12)
                .iter()
                .filter(|(_, page, _)| !SOURCES.contains(page))
                .map(|(index, ..)| *index)
                .collect();
            let offset = FREE_UNDO * PAGE_BYTES as u64;
            match variant {
                "io" => {
                    let mut io = state.lock().unwrap();
                    io.events.clear();
                    io.fault_hits = 0;
                    io.fault = Some(port::Fault {
                        key: port::Key::new("read", HOME_NAME, offset, 1),
                        action: port::Action::BeforeError,
                    });
                }
                "decode" => {
                    let mut io = state.lock().unwrap();
                    io.files.get_mut(HOME_NAME).unwrap().bytes[offset as usize + 1000] ^= 1;
                }
                _ => {}
            }
            let used = reader.catalog_test_source_used();
            let before = effects!(writer, state);
            let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
            let pinned: &[usize] = if variant == "busy" { &others } else { &[] };
            let error = with_pins(pool, &mut holder, pinned, || {
                writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .err()
                    .unwrap()
            });
            // The fatal variants fence the core, so only the busy
            // holder is checked idle; a fenced core reports Fenced.
            let idle = holder.check_idle();
            if variant == "busy" {
                idle.unwrap();
            } else {
                assert!(idle.is_err());
            }
            println!("S2W_R19 variant={variant} refusal={error:?}");
            assert_eq!(error.phase, RetentionPhase::Load);
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(reads!(view), (0, 0, 0));
            if variant == "busy" {
                assert_eq!(error.effect, Effect::Unchanged);
                assert!(matches!(error.cause, RetentionCause::Frame(ref frame)
                    if frame.cause == crate::storage::dirty_frame::Cause::Busy));
                returned_unchanged!(writer, reader, view, state, core, fence, used, before);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                close_row!(writer, owner, row, view, private, flush);
            } else {
                assert_eq!(error.effect, Effect::CallerMustResolve);
                // A fenced ledger refuses the return, so the RowWrite
                // charge stays visible (Fatal keeps its bookkeeping).
                let charge = writer.catalog_test_retirement_charge() as u64;
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                if variant == "io" {
                    assert_eq!(state.lock().unwrap().fault_hits, 1);
                    assert!(matches!(error.cause, RetentionCause::PageIo(_)));
                } else {
                    // The home page fails its check before a decode:
                    // the original cause, a page decode or the
                    // verification Invariant, never a Busy or Stale.
                    assert!(matches!(
                        error.cause,
                        RetentionCause::Page(_) | RetentionCause::Invariant
                    ));
                }
                assert!(fence.is_fenced());
                drop(view);
                drop(row);
            }
        }
    );
}

/// S2W-R20: preflight opens no read operation under its gate. Paused after
/// its residency (the LOAD of the Free Undo 14 has claimed and installed),
/// the core shows the gate closed and no read operation; then it is Ready.
#[test]
fn s2w_r20_preflight_opens_no_read_operation() {
    case("S2W-R20", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                evict(live, flusher, &row_undo.unwrap());
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let pool = reader.catalog_test_pool();
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                hook::install(core, Point::RowPreflightResident, entered, release);
                std::thread::scope(|scope| {
                    let controller = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        let observed = hook::observe(core);
                        let loaded = probe::registered_index(pool, FREE_UNDO).is_some();
                        resume.send(Resume::Continue).unwrap();
                        (observed, loaded)
                    });
                    let RowPreflightStep::Ready(preflight) = writer
                        .preflight_row(owner, &view, &table, private, &mut flush)
                        .unwrap()
                    else {
                        panic!("a clean victim exists");
                    };
                    preflight.finish().unwrap();
                    let (observed, loaded) = controller.join().unwrap();
                    println!("S2W_R20 at_resident={observed:?} undo_loaded={loaded}");
                    assert!(observed.read_gate_closed);
                    assert_eq!(observed.active_read_operations, 0);
                    assert!(!observed.fenced);
                    assert!(loaded);
                });
                assert!(!hook::observe(core).read_gate_closed);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R22 (preflight half): a row whose every source is resident opens one
/// replay checkout, the one `validate_live`. A row whose Free Undo is loaded
/// first adds that LOAD's own checkout, printed separately. Replay scopes
/// opened by LOAD steps that find their source resident check nothing out.
#[test]
fn s2w_r22_preflight_checks_out_the_replay_once() {
    case("S2W-R22", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, row_undo) =
                    create_table(live, owner, flusher, status, undo, private, true);
                evict(live, flusher, &row_undo.unwrap());
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let start = writer.row_test_replay_counts();
                let (_, flushed) = settle!(writer, owner, view, table, private, flush, flusher);
                assert!(flushed.is_empty());
                let loaded = writer.row_test_replay_counts();
                let (_, flushed) = settle!(writer, owner, view, table, private, flush, flusher);
                assert!(flushed.is_empty());
                let resident = writer.row_test_replay_counts();
                println!(
                    "S2W_R22 preflight_with_load scopes={} checkouts={} \
                     preflight_all_resident scopes={} checkouts={}",
                    loaded.0 - start.0,
                    loaded.1 - start.1,
                    resident.0 - loaded.0,
                    resident.1 - loaded.1
                );
                assert_eq!(loaded.1 - start.1, 2);
                assert_eq!(resident.1 - loaded.1, 1);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}
