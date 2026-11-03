//! RDL-J (dev3-row-delete.md#tests): the DELETE first path over the actual
//! C supply, through A's `autocommit_delete_live` and
//! `transaction_delete_live`, from Begin to the transaction's end, then the
//! caller's cleanup (`retire_finished_live`), B's reads and C's restart.
//! Expectations are state-machine facts (the outcome, the rows deleted, the
//! groups each drain installed, freed Jobs, the rows a new statement and a
//! restart read), never values read back from the same producer.
use super::*;
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
