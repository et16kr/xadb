//! RDL-J (dev3-row-delete.md#tests): the DELETE first path over the actual
//! C supply, through A's `autocommit_delete_live` and
//! `transaction_delete_live`, from Begin to the transaction's end, then the
//! caller's cleanup (`retire_finished_live`), B's reads and C's restart.
//! Expectations are state-machine facts (the outcome, the rows deleted, the
//! groups each drain installed, freed Jobs, the rows a new statement and a
//! restart read), never values read back from the same producer.
use super::super::super::super::row_recovery::{Drive, PageRead, restart_with};
use super::*;
use crate::storage::disk::RowDeleteStep;
use crate::storage::row::insert::driver::live::{autocommit_delete_live, transaction_delete_live};

/// One autocommit DELETE of `row` in `object` on `status` through A's
/// executor: its outcome and the row's slot.
macro_rules! delete {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $status:expr, $private:expr, $object:expr, $row:expr) => {{
        let mut slot: Option<RowMutationObservation> = None;
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: $object,
        };
        let mut out = Autocommit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        autocommit_delete_live(
            &mut call,
            $status,
            IsolationLevel::Serializable,
            &mut tables,
            $private,
            $row,
            &mut slot,
            &mut out,
        );
        (out, slot)
    }};
}

/// One DELETE statement of `row` in `object` in the transaction in
/// `holder` through A's entry: the slot and the row's slot.
macro_rules! delete_statement {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $holder:expr, $private:expr, $object:expr, $row:expr) => {{
        let mut slot: Option<RowMutationObservation> = None;
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: $object,
        };
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        transaction_delete_live(
            &mut call,
            $holder,
            &mut tables,
            $private,
            $row,
            &mut slot,
            &mut out,
        );
        (out, slot)
    }};
}

/// The Row Jobs C's restart of `images` took into its base4 (the cell trace
/// at bind, before any action; the restart runs to READY): one DELETE root
/// in state `root` and its actor's Terminal in state `terminal`.
fn assert_bound(images: &Images, root: JobState, terminal: JobState) {
    let restarted = restart_with(
        images,
        Some(RowRecoveryPolicy::WithRowClr),
        Drive::Action,
        0,
        &PageRead,
    );
    let states: Vec<_> = restarted.trace[0]
        .iter()
        .map(|(_, state, _)| *state)
        .collect();
    assert!(
        states.len() == 2 && states.contains(&root) && states.contains(&terminal),
        "{states:?}"
    );
}

/// RDL-J01: a row inserted and committed at q 2 is deleted by A's
/// autocommit executor at q 3 (1 row). The caller's drain installs the
/// DELETE root's DeletePrune, its three ReleaseRoles, RootFree and
/// TerminalFinish (5 + 1 groups) and no Row Job is left. A new statement
/// reads no row at the RowId and another row of the table is untouched.
/// A DELETE of the same RowId again finds no row (0 rows, committed with no
/// root, one TerminalFinish). C's recovery of the files reaches READY at q 4
/// with no CLR and B's cursor reads only the other row.
#[test]
fn rdl_j01_autocommit_delete_commits_and_its_prune_removes_the_row() {
    case_within("RDL-J01", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, kept) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                system_init(live, owner, flusher, true);
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let (target, kept) = (rows[0].0, rows[1]);

                let (out, slot) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    target
                );
                assert_eq!(committed(out), (3, 1));
                assert_eq!(slot.expect("deleted row").row(), target);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        [target, kept.0]
                    ),
                    [None, Some(101)]
                );

                let (out, slot) = delete!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    target
                );
                assert_eq!(committed(out), (4, 0));
                assert!(slot.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(1, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), (kept.0.get(), kept.1))
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (4, 0, 0));
        assert_eq!(joined.rows, [kept]);
        println!("RDL_J01 q=4 drains=6,1 restart_steps={}", joined.steps);
    });
}

/// RDL-J02: an explicit transaction's DELETE statement deletes one row
/// (1 row), the caller rolls back: the DeleteClr restores the row and the
/// drain frees the root (its three ReleaseRoles, RootFree and TerminalFinish:
/// 4 + 1 groups), after which a new statement reads both rows again.
/// A second transaction deletes the row, deletes it again (its own tombstone:
/// 0 rows) and commits at q 3 with the caller's count of 1 row; the drain
/// installs DeletePrune and frees the root (5 + 1 groups) and a new
/// statement reads only the other row. C's recovery
/// of the files reaches READY at q 3 with no CLR and B's cursor reads only
/// the other row.
#[test]
fn rdl_j02_explicit_delete_rolls_back_then_commits() {
    case_within("RDL-J02", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, kept) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let (target, kept) = (rows[0].0, rows[1]);

                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    target
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), target);
                let out = rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                assert!(
                    matches!(out, Explicit::RolledBack),
                    "expected RolledBack, got {out:?}"
                );
                assert!(holder.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(5, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        [target, kept.0]
                    ),
                    [Some(100), Some(101)]
                );

                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                for expected in [1, 0] {
                    let (out, slot) = delete_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        target
                    );
                    assert_eq!(done(out), expected);
                    assert_eq!(
                        slot.map(|slot| slot.row()),
                        (expected == 1).then_some(target)
                    );
                }
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        1
                    )),
                    (3, 1)
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        [target, kept.0]
                    ),
                    [None, Some(101)]
                );
                assert!(!fence.is_fenced());
                (state.lock().unwrap().images(), (kept.0.get(), kept.1))
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, [kept]);
        println!("RDL_J02 q=3 drains=5,6 restart_steps={}", joined.steps);
    });
}

/// RDL-J03 (dev3-row-delete.md#read): reads of a tombstone before its
/// cleanup over the actual C supply. While the deleting transaction is
/// Active, a new statement's capture takes the tombstone and C's
/// `value_before` reads U's RowValue before: the old value (q_old 2 <= S).
/// After the Commit at q 3 and before the drain, a new statement sees T
/// committed at q 3 <= S: no row, with no U read. The drain then installs
/// DeletePrune (5 + 1 groups) and the read is the same.
#[test]
fn rdl_j03_reads_before_cleanup_see_the_old_value_then_no_row() {
    case_within("RDL-J03", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let ids = [rows[0].0, rows[1].0];

                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ids[0]
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), ids[0]);
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [Some(100), Some(101)]
                );
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        1
                    )),
                    (3, 1)
                );
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [None, Some(101)]
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [None, Some(101)]
                );
                assert!(!fence.is_fenced());
                println!("RDL_J03 active=old committed=none drains=6");
            }
        );
    });
}

/// Where `crash_images` takes the files (dev3-row-delete.md#recovery).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Crash {
    /// A's autocommit DELETE committed at q 3, before any cleanup: the
    /// DELETE root is Retained beside its Ready Terminal.
    Committed,
    /// An explicit transaction's DELETE statement applied (1 row) while the
    /// transaction is Active: the root is Retained beside its Leased
    /// Terminal.
    Active,
    /// That DELETE rolled back by the caller (its DeleteClr and AbortEnd),
    /// before the drain: the root is ReleasingCompensated beside its Ready
    /// Terminal.
    RolledBack,
    /// C's DELETE preflight and Reserve, before the DeleteForward: the root
    /// is Reserved beside its Leased Terminal.
    Reserved,
}

/// Two rows committed at q 2 by A's autocommit INSERT and drained, then the
/// first one's DELETE up to `crash`: the files there and the two rows
/// (RowId, value).
fn crash_images(crash: Crash) -> (Images, Vec<(u64, i64)>) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 28,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let _ = core;
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            // An explicit transaction's Begin takes a second System Undo.
            let inits = if crash == Crash::Committed { 1 } else { 2 };
            for _ in 0..inits {
                system_init(live, owner, flusher, true);
            }
            let (mut writer, reader, mut flush) = live.split();
            let (out, slots) = autocommit!(
                &mut writer,
                owner,
                flush,
                flusher,
                reader,
                reader_private,
                &row_status,
                private,
                6,
                ints(&[100, 101])
            );
            assert_eq!(committed(out), (2, 2));
            let rows = applied(slots, &[100, 101]);
            assert_eq!(
                drains!(&mut writer, owner, flush, flusher, private),
                [drain(11, true)]
            );
            let target = rows[0].0;
            let rows: Vec<_> = rows
                .iter()
                .map(|(row, value)| (row.get(), *value))
                .collect();
            // The body is the macro's block: each arm yields its value.
            match crash {
                Crash::Committed => {
                    let (out, _) = delete!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        target
                    );
                    assert_eq!(committed(out), (3, 1));
                    assert!(!fence.is_fenced());
                    (state.lock().unwrap().images(), rows)
                }
                Crash::Reserved => {
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    let preflight = match writer
                        .preflight_delete(owner, &view, &table, target, private, &mut flush)
                        .unwrap()
                    {
                        RowDeleteStep::Ready(preflight) => preflight,
                        RowDeleteStep::FlushRequired { .. } => panic!("every source is resident"),
                        RowDeleteStep::NoRow => panic!("a committed row"),
                    };
                    let reserved = match preflight.reserve_delete() {
                        Ok(reserved) => reserved,
                        Err(RowOwnedError::Before { error, .. })
                        | Err(RowOwnedError::Fatal(error)) => {
                            panic!("Reserve {error:?}")
                        }
                    };
                    assert!(!fence.is_fenced());
                    // RowReserve is durable, its DeleteForward is not staged.
                    let images = state.lock().unwrap().images();
                    drop(reserved);
                    drop(view);
                    drop(tx);
                    (images, rows)
                }
                Crash::Active | Crash::RolledBack => {
                    let mut holder = None;
                    begun(begin!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &row_status,
                        private,
                        &mut holder
                    ));
                    let (out, slot) = delete_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        target
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("deleted row").row(), target);
                    if crash == Crash::RolledBack {
                        let out =
                            rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                        assert!(
                            matches!(out, Explicit::RolledBack),
                            "expected RolledBack, got {out:?}"
                        );
                        assert!(holder.is_none());
                    }
                    assert!(!fence.is_fenced());
                    let images = state.lock().unwrap().images();
                    // An Active holder's drop fences this session, after the files.
                    drop(holder);
                    (images, rows)
                }
            }
        }
    )
}

/// RDL-J04 (dev3-row-delete.md#recovery, winner): A's autocommit DELETE
/// commits at q 3 and the files are taken before any cleanup
/// (`Crash::Committed`). C's restart takes the effect-1 Retained root, the
/// tombstone and the Live RowValue U into its base4, its recovery selects
/// DeletePrune for the winner before READY, no CLR runs, and B's cursor
/// reads only the other row.
#[test]
fn rdl_j04_committed_delete_is_pruned_by_recovery() {
    case_within("RDL-J04", std::time::Duration::from_secs(300), || {
        let (images, rows) = crash_images(Crash::Committed);
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::WithRowClr,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, rows[1..]);
        assert_bound(&images, JobState::RootRetained, JobState::TerminalReady);
        println!("RDL_J04 q=3 restart_steps={}", joined.steps);
    });
}

/// RDL-J05 (dev3-row-delete.md#recovery, loser): the files are taken while
/// the deleting transaction is Active (`Crash::Active`). C's restart takes
/// the Retained root into its base4, runs the loser's full RollbackStart,
/// hands its one U to A's recovered handler (the same `produce_clr`; the
/// root's effect makes it a DeleteClr that restores the row from U), then
/// AbortEnd and the compensated cleanup to READY at the old q 2 with no Row
/// Job left. B's cursor reads both rows.
#[test]
fn rdl_j05_active_delete_loser_is_restored_by_recovery() {
    case_within("RDL-J05", std::time::Duration::from_secs(300), || {
        let (images, rows) = crash_images(Crash::Active);
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::WithRowClr,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 1, 0));
        assert_eq!(joined.rows, rows);
        assert_bound(&images, JobState::RootRetained, JobState::TerminalLeased);
        println!("RDL_J05 q=2 clrs=1 restart_steps={}", joined.steps);
    });
}

/// RDL-J06 (dev3-row-delete.md#recovery, compensated): the caller's
/// rollback of the DELETE is durable and the files are taken before the
/// drain (`Crash::RolledBack`), so the root is ReleasingCompensated (its U
/// Releasing without the value). C's restart takes it into its base4, runs
/// no CLR, and frees the root and Terminal before READY at q 2; B's cursor
/// reads both rows.
#[test]
fn rdl_j06_rolled_back_delete_is_freed_by_recovery() {
    case_within("RDL-J06", std::time::Duration::from_secs(300), || {
        let (images, rows) = crash_images(Crash::RolledBack);
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::WithRowClr,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        assert_eq!(joined.rows, rows);
        assert_bound(
            &images,
            JobState::RootReleasingCompensated,
            JobState::TerminalReady,
        );
        println!("RDL_J06 q=2 clrs=0 restart_steps={}", joined.steps);
    });
}

/// RDL-J07 (dev3-row-delete.md#recovery, Reserved): the files are taken
/// after C's DELETE Reserve (`Crash::Reserved`). C's restart takes the
/// Reserved effect-1 root (the replaced committed row of cell C in the
/// leaf, U Free) into its base4; the Active loser's rollback cancels the
/// root with no CLR and READY is at q 2 with no Row Job left. B's cursor
/// reads both rows.
#[test]
fn rdl_j07_reserved_delete_is_cancelled_by_recovery() {
    case_within("RDL-J07", std::time::Duration::from_secs(300), || {
        let (images, rows) = crash_images(Crash::Reserved);
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::WithRowClr,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (2, 0, 0));
        assert_eq!(joined.rows, rows);
        assert_bound(&images, JobState::RootReserved, JobState::TerminalLeased);
        println!("RDL_J07 q=2 clrs=0 restart_steps={}", joined.steps);
    });
}

/// RDL-J08 (dev3-row-delete.md#effect-1-h, #leaf-reservation): one
/// explicit transaction deletes a row (1 row) and then INSERTs a new one
/// into the same leaf (1 row). The INSERT root's previous root H is the
/// Retained DELETE root (effect 1, proved through the metadata entry), and
/// its Reserve keeps the DELETE root's leaf reservation. The transaction
/// commits at q 3 with the caller's 2 rows; the drain installs DeletePrune
/// and Prune with each root's three ReleaseRoles and RootFree and the
/// TerminalFinish (5 + 5 + 1 groups). A new statement reads no row at the
/// deleted RowId, the kept row and the new one; C's restart reaches READY at
/// q 3 with no CLR and B's cursor reads the same two rows.
#[test]
fn rdl_j08_delete_then_insert_in_one_transaction() {
    case_within("RDL-J08", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, rows) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let (target, kept) = (rows[0].0, rows[1]);
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    target
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), target);
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[102])
                );
                assert_eq!(done(out), 1);
                let new = applied(slots, &[102])[0];
                assert!(new.0 != target && new.0 != kept.0);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        2
                    )),
                    (3, 2)
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        [target, kept.0, new.0]
                    ),
                    [None, Some(101), Some(102)]
                );
                assert!(!fence.is_fenced());
                (
                    state.lock().unwrap().images(),
                    vec![(kept.0.get(), kept.1), (new.0.get(), new.1)],
                )
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, rows);
        println!("RDL_J08 q=3 drains=11 restart_steps={}", joined.steps);
    });
}

/// RDL-J09 (dev3-row-delete.md#classification, #api): in one explicit
/// transaction, a DELETE of the row its own INSERT statement just wrote
/// (undo Some, the actor's `Transaction` stamp) is Unsupported at C's
/// preflight: the statement fails holding nothing and the transaction goes
/// on. Its next DELETE of a committed row deletes it (1 row) and the
/// transaction commits at q 3 with the caller's 2 rows; the drain cleans
/// both roots and the Terminal (5 + 5 + 1 groups). Rows another
/// transaction holds (Busy) need a second Row Status: RDL-J11 (`busy`).
#[test]
fn rdl_j09_own_insert_row_is_unsupported_and_the_transaction_goes_on() {
    case_within("RDL-J09", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let (target, kept) = (rows[0].0, rows[1]);
                let mut holder = None;
                begun(begin!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &row_status,
                    private,
                    &mut holder
                ));
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[102])
                );
                assert_eq!(done(out), 1);
                let new = applied(slots, &[102])[0];
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    new.0
                );
                let failure = statement_failed(out);
                let DriverError::Disk(error) = &failure.error else {
                    panic!("expected C's refusal, got {failure:?}");
                };
                assert_eq!(failure.stage, Stage::Preflight);
                assert!(
                    matches!(error.cause, RetentionCause::Unsupported)
                        && error.effect == Effect::Unchanged,
                    "{error:?}"
                );
                assert!(slot.is_none());
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    target
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), target);
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        2
                    )),
                    (3, 2)
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert_eq!(
                    read_rows!(
                        writer,
                        reader,
                        core,
                        reader_private,
                        flush,
                        flusher,
                        [target, kept.0, new.0]
                    ),
                    [None, Some(101), Some(102)]
                );
                assert!(!fence.is_fenced());
                println!("RDL_J09 unsupported then q=3 drains=11");
            }
        );
    });
}

/// RDL-J10 (dev3-row-delete.md#delete-states): two DELETE statements of one
/// explicit transaction (1 row each) chain their roots (the second root's H
/// is the first, effect 1). A new statement of the same transaction reads
/// neither row (its own tombstones) while another statement reads both old
/// values. The caller's rollback compensates both with DeleteClr (the
/// second first) and its drain frees both roots and the Terminal (4 + 4 + 1
/// groups); both rows read again. A second transaction deletes both and
/// commits at q 3 with 2 rows; its drain installs both DeletePrunes (5 + 5
/// + 1 groups). C's restart reaches READY at q 3 with no row left.
#[test]
fn rdl_j10_two_deletes_roll_back_then_commit() {
    case_within("RDL-J10", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let images = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..3 {
                    system_init(live, owner, flusher, true);
                }
                let (mut writer, reader, mut flush) = live.split();
                let (out, slots) = autocommit!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &row_status,
                    private,
                    6,
                    ints(&[100, 101])
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[100, 101]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let ids = [rows[0].0, rows[1].0];
                for commit in [false, true] {
                    let mut holder = None;
                    begun(begin!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &row_status,
                        private,
                        &mut holder
                    ));
                    for id in ids {
                        let (out, slot) = delete_statement!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut holder,
                            private,
                            6,
                            id
                        );
                        assert_eq!(done(out), 1);
                        assert_eq!(slot.expect("deleted row").row(), id);
                    }
                    assert_eq!(
                        read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                        [Some(100), Some(101)]
                    );
                    if commit {
                        assert_eq!(
                            explicit_committed(commit!(
                                &mut writer,
                                owner,
                                flush,
                                flusher,
                                &mut holder,
                                private,
                                2
                            )),
                            (3, 2)
                        );
                        assert_eq!(
                            drains!(&mut writer, owner, flush, flusher, private),
                            [drain(11, true)]
                        );
                    } else {
                        let out =
                            rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                        assert!(
                            matches!(out, Explicit::RolledBack),
                            "expected RolledBack, got {out:?}"
                        );
                        assert_eq!(
                            drains!(&mut writer, owner, flush, flusher, private),
                            [drain(9, true)]
                        );
                    }
                    assert!(holder.is_none());
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    let expected = if commit {
                        [None, None]
                    } else {
                        [Some(100), Some(101)]
                    };
                    assert_eq!(
                        read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                        expected
                    );
                }
                assert!(!fence.is_fenced());
                state.lock().unwrap().images()
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert!(joined.rows.is_empty());
        println!("RDL_J10 drains=9,11 q=3 restart_steps={}", joined.steps);
    });
}

/// RDL-R: C's base4 of the DELETE crash states, over actual captured
/// bases.
#[path = "delete_base.rs"]
mod base;

#[path = "delete_read.rs"]
mod read;

#[path = "delete_busy.rs"]
mod busy;

/// RDL-N: native crashes on either side of each DELETE group, and their
/// memory twins.
#[cfg(target_os = "linux")]
#[path = "delete_native.rs"]
mod native;
