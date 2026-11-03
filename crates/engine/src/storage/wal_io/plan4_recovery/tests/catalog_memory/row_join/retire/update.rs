//! RUP-J (dev3-row-update.md#tests): the UPDATE first path over the actual
//! C supply, through A's `autocommit_update_live` and
//! `transaction_update_live`, from Begin to the transaction's end, then the
//! caller's cleanup (`retire_finished_live`), B's reads and C's restart.
//! Expectations are state-machine facts (the outcome, the rows updated, the
//! groups each drain installed, freed Jobs, the values a new statement and
//! a restart read), never values read back from the same producer.
use super::super::super::super::row_literal::NULL_INT;
use super::super::super::super::row_preflight::create_table_with;
use super::super::super::super::row_recovery::{Drive, PageRead, restart_with};
use super::*;
use crate::storage::{
    disk::RowDeleteStep,
    row::insert::{
        driver::live::{autocommit_update_live, transaction_delete_live, transaction_update_live},
        validate_insert,
    },
};

/// One autocommit UPDATE of `row` in `object` on `status` to `value`
/// through A's executor: its outcome and the row's slot.
macro_rules! update {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $status:expr, $private:expr, $object:expr, $row:expr, $value:expr) => {{
        let mut slot: Option<RowMutationObservation> = None;
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: $object,
        };
        let values: [ValueRef<'static>; 1] = $value;
        let mut out = Autocommit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        autocommit_update_live(
            &mut call,
            $status,
            IsolationLevel::Serializable,
            &mut tables,
            $private,
            ($row, &values[..]),
            &mut slot,
            &mut out,
        );
        (out, slot)
    }};
}

/// One UPDATE statement of `row` in `object` to `value` in the
/// transaction in `holder` through A's entry: the slot and the row's slot.
macro_rules! update_statement {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $reader:expr, $reader_private:expr, $holder:expr, $private:expr, $object:expr, $row:expr, $value:expr) => {{
        let mut slot: Option<RowMutationObservation> = None;
        let mut tables = Tables {
            reader: &$reader,
            private: $reader_private,
            object: $object,
        };
        let values: [ValueRef<'static>; 1] = $value;
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        transaction_update_live(
            &mut call,
            $holder,
            &mut tables,
            $private,
            ($row, &values[..]),
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

/// One drain of A's cleanup; it must not stop.
macro_rules! drained {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $private:expr) => {{
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        match retire_finished_live(&mut call, $private) {
            Ok(retired) => retired,
            Err(stop) => panic!("A's cleanup drain stopped {stop:?}"),
        }
    }};
}

/// A statement's refusal before any effect: the cause, no row, the
/// transaction goes on.
macro_rules! refused {
    ($out:expr, $slot:expr, $stage:expr, $cause:pat) => {{
        let failure = statement_failed($out);
        let DriverError::Disk(error) = &failure.error else {
            panic!("expected C's refusal, got {failure:?}");
        };
        assert_eq!(failure.stage, $stage);
        assert!(
            matches!(error.cause, $cause) && error.effect == Effect::Unchanged,
            "{error:?}"
        );
        assert!($slot.is_none());
    }};
}

fn int(value: i64) -> [ValueRef<'static>; 1] {
    [ValueRef::Int64(value)]
}
const NULL: [ValueRef<'static>; 1] = [ValueRef::Null];

/// T (object 6) with one nullable Int64 column: a NULL's canonical is 5 B
/// and an Int64's 13 B, so an UPDATE between them shrinks or grows the row
/// by 8 B (C' = C ∓ 8).
fn nullable() -> TableSpec {
    TableSpec {
        name: "T".to_owned(),
        columns: vec![Column {
            name: "i".to_owned(),
            data_type: DataType::Int64,
            nullable: true,
        }],
    }
}

/// The Row Jobs C's restart of `images` took into its base4 (the cell trace
/// at bind, before any action): one UPDATE root in state `root` and its
/// actor's Terminal in state `terminal`.
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

/// RUP-J01 (RUP-A01): rows 100 and 101 committed at q 2 are drained; A's
/// autocommit UPDATE sets the first to 200 at q 3 (1 row). The caller's
/// drain installs the UPDATE root's UpdatePrune, its three ReleaseRoles,
/// RootFree and TerminalFinish (5 + 1 groups) and no Row Job is left; a new
/// statement reads 200 and the other row. An UPDATE of a RowId with no key
/// is 0 rows (committed at q 4 with no root, one TerminalFinish), and the
/// row is updated again to 300 at q 5. C's recovery of the files reaches
/// READY at q 5 with no CLR and B's cursor reads 300 and 101.
#[test]
fn rup_j01_autocommit_update_commits_and_its_prune_keeps_the_new_value() {
    case_within("RUP-J01", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
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
                for (value, q) in [(200, 3), (300, 5)] {
                    let (out, slot) = update!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        target,
                        int(value)
                    );
                    assert_eq!(committed(out), (q, 1));
                    assert_eq!(slot.expect("updated row").row(), target);
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
                        [Some(value), Some(101)]
                    );
                    if q == 3 {
                        let (out, slot) = update!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &row_status,
                            private,
                            6,
                            RowId::new(999).unwrap(),
                            int(1)
                        );
                        assert_eq!(committed(out), (4, 0));
                        assert!(slot.is_none());
                        assert_eq!(
                            drains!(&mut writer, owner, flush, flusher, private),
                            [drain(1, true)]
                        );
                    }
                }
                assert!(!fence.is_fenced());
                let expected = vec![(target.get(), 300), (kept.0.get(), kept.1)];
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (5, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("RUP_J01 q=5 drains=6,1,6 restart_steps={}", joined.steps);
    });
}

/// RUP-J02 (#leaf-reservation, #update-states): a nullable T holds NULL and
/// 5, committed at q 2. One explicit transaction updates NULL to 9 (the row
/// grows by 8 B) and the caller rolls back: the UpdateClr restores NULL
/// and the drain frees the root (three ReleaseRoles, RootFree and
/// TerminalFinish: 4 + 1). A second updates 5 to NULL (it shrinks by 8 B,
/// so the leaf reserves 8 B for its UpdateClr) and rolls back the same way:
/// 5 is restored. A third updates both (the second root's H is the first,
/// effect 2) and commits at q 3 with the caller's 2 rows; the drain
/// installs both UpdatePrunes and frees both roots (5 + 5 + 1). New
/// statements read each state; C's restart reaches READY at q 3 with no
/// CLR and B's cursor reads 9 and NULL.
#[test]
fn rup_j02_growing_and_shrinking_updates_roll_back_then_commit() {
    case_within("RUP-J02", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) = create_table_with(
                    live,
                    owner,
                    flusher,
                    status,
                    undo,
                    private,
                    true,
                    &nullable(),
                );
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
                    vec![NULL, int(5)]
                );
                assert_eq!(committed(out), (2, 2));
                let rows = applied(slots, &[NULL_INT, 5]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                let ids = [rows[0].0, rows[1].0];
                for (at, value) in [(0, int(9)), (1, NULL)] {
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
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        ids[at],
                        value
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("updated row").row(), ids[at]);
                    let out = rollback!(&mut writer, owner, flush, flusher, &mut holder, private);
                    assert!(
                        matches!(out, Explicit::RolledBack),
                        "expected RolledBack, got {out:?}"
                    );
                    assert_eq!(
                        drains!(&mut writer, owner, flush, flusher, private),
                        [drain(5, true)]
                    );
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    assert_eq!(
                        read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                        [Some(NULL_INT), Some(5)]
                    );
                }
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
                for (at, value) in [(0, int(9)), (1, NULL)] {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        ids[at],
                        value
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("updated row").row(), ids[at]);
                }
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
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [Some(9), Some(NULL_INT)]
                );
                assert!(!fence.is_fenced());
                let expected = vec![(ids[0].get(), 9), (ids[1].get(), NULL_INT)];
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("RUP_J02 q=3 drains=5,5,11 restart_steps={}", joined.steps);
    });
}

/// RUP-J03 (RUP-A06, #previous-proof): one explicit transaction updates
/// row 100 to 200 (1 row); its next statement INSERTs 8 and then a NULL
/// that A refuses, so the statement rolls back its applied row (a CLR whose
/// row's H is the UPDATE root, effect 2) and the transaction goes on; it
/// updates 101 to 201 (1 row) and commits at q 3 with the caller's 2 rows.
/// The drain installs both UpdatePrunes and frees both roots, the
/// compensated INSERT root and the Terminal (5 + 4 + 5 + 1 = 15 groups). A
/// new statement reads 200, no row at the rolled back RowId and 201; C's
/// restart reaches READY at q 3 with no CLR and B's cursor reads 200 and
/// 201.
#[test]
fn rup_j03_statement_rollback_between_two_updates() {
    case_within("RUP-J03", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..4 {
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
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ids[0],
                    int(200)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[0]);
                let (out, failed) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    vec![[ValueRef::Int64(8)], [ValueRef::Null]]
                );
                let failure = statement_failed(out);
                assert!(
                    matches!(failure.error, DriverError::Insert(_)),
                    "{failure:?}"
                );
                assert!(holder.is_some(), "the transaction goes on");
                let rolled_back = failed[0].as_ref().expect("the row applied").row();
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ids[1],
                    int(201)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[1]);
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
                    [drain(15, true)]
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
                        [ids[0], rolled_back, ids[1]]
                    ),
                    [Some(200), None, Some(201)]
                );
                assert!(!fence.is_fenced());
                let expected = vec![(ids[0].get(), 200), (ids[1].get(), 201)];
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("RUP_J03 q=3 drain=15 restart_steps={}", joined.steps);
    });
}

/// RUP-J04 (RUP-A01..A03, #previous-proof, #classification): rows 100,
/// 101 and 102 are committed at q 2. One explicit transaction:
/// - updates 100 to 200 (1 row);
/// - INSERTs 103 (1 row; its H is the UPDATE root, effect 2);
/// - deletes 102 (1 row; its H is the INSERT root);
/// - updates 101 to 201 (1 row; its H is the DELETE root, effect 1);
/// - updates 102 again: its own tombstone, 0 rows;
/// - updates 103, its own INSERT row, and 100, its own UPDATE row: each
///   Unsupported at C's preflight with no effect, and the transaction goes
///   on;
/// - updates a RowId with no key: 0 rows.
///
/// It commits at q 3 with the caller's 4 rows; the drain installs each
/// root's Prune with its three ReleaseRoles and RootFree, and the
/// TerminalFinish (4 × 5 + 1 = 21 groups). A new statement reads 200, 201,
/// no row at 102's RowId and 103; C's restart reaches READY at q 3 with no
/// CLR and B's cursor reads the same three rows.
#[test]
fn rup_j04_update_then_insert_delete_and_update_in_one_transaction() {
    case_within("RUP-J04", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 1..5 {
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
                    ints(&[100, 101, 102])
                );
                assert_eq!(committed(out), (2, 3));
                let rows = applied(slots, &[100, 101, 102]);
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(16, true)]
                );
                let ids = [rows[0].0, rows[1].0, rows[2].0];
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
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ids[0],
                    int(200)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[0]);
                let (out, inserted) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ints(&[103])
                );
                assert_eq!(done(out), 1);
                let new = applied(inserted, &[103])[0].0;
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
                    ids[2]
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), ids[2]);
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ids[1],
                    int(201)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[1]);
                for row in [ids[2], RowId::new(999).unwrap()] {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        row,
                        int(7)
                    );
                    assert_eq!(done(out), 0);
                    assert!(slot.is_none());
                }
                let jobs = writer.catalog_test_jobs().to_vec();
                for row in [new, ids[0]] {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        row,
                        int(7)
                    );
                    refused!(out, slot, Stage::Preflight, RetentionCause::Unsupported);
                    assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
                }
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut holder,
                        private,
                        4
                    )),
                    (3, 4)
                );
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(21, true)]
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
                        [ids[0], ids[1], ids[2], new]
                    ),
                    [Some(200), Some(201), None, Some(103)]
                );
                assert!(!fence.is_fenced());
                let expected = vec![(ids[0].get(), 200), (ids[1].get(), 201), (new.get(), 103)];
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (3, 0, 0));
        assert_eq!(joined.rows, expected);
        println!("RUP_J04 q=3 drain=21 restart_steps={}", joined.steps);
    });
}

/// RUP-J05 (#read, #retirement): reads of an updated row before its
/// cleanup over the actual C supply. Rows 100 and 101 are committed at
/// q 2. A ReadCommitted reader's statement `old` begins (S = 2). An
/// explicit transaction updates 100 to 200 and is Active: a new
/// statement's capture takes the new row, C's `missing_before` reports a
/// value before and `value_before` reads U's old value 100 (q_old 2 <= S).
/// After the Commit at q 3, a new statement sees T committed at q 3 <= S:
/// 200, with no U read; `old` (S = 2) still reads 100. The drain then
/// holds the UpdatePrune (q 3 is above the horizon `old` keeps) and stages
/// nothing; once `old` finishes the drain installs it (5 + 1 groups) and a
/// new statement reads 200.
#[test]
fn rup_j05_reads_before_cleanup_and_a_held_update_prune() {
    case_within("RUP-J05", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
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
                let ids = [rows[0].0, rows[1].0];
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let read = writer
                    .begin_transaction(
                        b,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer.begin_statement(b, &read).unwrap();
                assert_eq!(old.q().unwrap(), 2);

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
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut holder,
                    private,
                    6,
                    ids[0],
                    int(200)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[0]);
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
                    [Some(200), Some(101)]
                );
                let old_table = issue_table!(reader, b, old, reader_private, flush, flusher);
                assert_eq!(
                    point_at!(reader, b, reader_private, &old, &old_table, ids[0]),
                    Some(100)
                );
                let jobs = writer.catalog_test_jobs().to_vec();
                let (wal, _) = files(state);
                let held = drained!(&mut writer, owner, flush, flusher, private);
                assert_eq!((held.groups, held.held, held.complete), (0, 1, true));
                assert_eq!(files(state).0, wal, "a held drain stages nothing");
                assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
                assert_eq!(
                    point_at!(reader, b, reader_private, &old, &old_table, ids[0]),
                    Some(100)
                );
                assert!(writer.finish_statement(b, old).is_ok());
                let done_drain = drained!(&mut writer, owner, flush, flusher, private);
                assert_eq!(
                    (done_drain.groups, done_drain.held, done_drain.complete),
                    (6, 0, true)
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert_eq!(
                    read_rows!(writer, reader, core, reader_private, flush, flusher, ids),
                    [Some(200), Some(101)]
                );
                assert!(!fence.is_fenced());
                println!("RUP_J05 active=old committed=new old_snapshot=old held=1 drained=6");
            }
        );
    });
}

/// Where `crash_images` takes the files (dev3-row-update.md#recovery).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Crash {
    /// A's autocommit UPDATE committed at q 3, before any cleanup: the
    /// UPDATE root is Retained beside its Ready Terminal.
    Committed,
    /// An explicit transaction's UPDATE statement applied (1 row) while the
    /// transaction is Active: the root is Retained beside its Leased
    /// Terminal.
    Active,
    /// That UPDATE rolled back by the caller (its UpdateClr and AbortEnd),
    /// before the drain: the root is ReleasingCompensated beside its Ready
    /// Terminal.
    RolledBack,
    /// C's UPDATE preflight and Reserve, before the UpdateForward: the root
    /// is Reserved beside its Leased Terminal.
    Reserved,
}

/// Two rows committed at q 2 by A's autocommit INSERT and drained, then
/// the first one's UPDATE to 200 up to `crash`: the files there and the
/// two rows (RowId, value) as committed at q 2.
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
            match crash {
                Crash::Committed => {
                    let (out, _) = update!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &row_status,
                        private,
                        6,
                        target,
                        int(200)
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
                    let mut preflight = match writer
                        .preflight_update(owner, &view, &table, target, private, &mut flush)
                        .unwrap()
                    {
                        RowDeleteStep::Ready(preflight) => preflight,
                        RowDeleteStep::FlushRequired { .. } => panic!("every source is resident"),
                        RowDeleteStep::NoRow => panic!("a committed row"),
                    };
                    let plan = validate_insert(
                        &[ValueRef::Int64(200)],
                        preflight.validation_lane().unwrap(),
                    )
                    .unwrap();
                    let reserved = match preflight.reserve_update(plan) {
                        Ok(reserved) => reserved,
                        Err(RowOwnedError::Before { error, .. })
                        | Err(RowOwnedError::Fatal(error)) => {
                            panic!("Reserve {error:?}")
                        }
                    };
                    assert!(!fence.is_fenced());
                    // RowReserve is durable, its UpdateForward is not staged.
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
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut holder,
                        private,
                        6,
                        target,
                        int(200)
                    );
                    assert_eq!(done(out), 1);
                    assert_eq!(slot.expect("updated row").row(), target);
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

/// RUP-R01..R04 over the actual files (dev3-row-update.md#recovery): C's
/// restart takes each effect-2 crash state into its base4 and reaches
/// READY with the rows the state machine says.
/// - Committed (winner): the Retained root beside its Ready Terminal; the
///   recovery selects UpdatePrune before READY at q 3, no CLR; 200 and 101.
/// - Active (loser): the Retained root beside its Leased Terminal; the
///   loser's RollbackStart hands its U to A's recovered handler (the same
///   `produce_clr`: the root's effect makes it an UpdateClr that restores
///   100 from U), then AbortEnd and the compensated cleanup; READY at q 2
///   with one CLR; 100 and 101.
/// - RolledBack: ReleasingCompensated beside its Ready Terminal; no CLR,
///   READY at q 2; 100 and 101.
/// - Reserved: the Reserved effect-2 root (the old committed row of cell C
///   in the leaf, U Free) beside its Leased Terminal; the loser's rollback
///   cancels it with no CLR; READY at q 2; 100 and 101.
#[test]
fn rup_r01_r04_recovery_of_each_update_state() {
    case_within("RUP-R01", std::time::Duration::from_secs(600), || {
        for (crash, ready, clrs, root, terminal) in [
            (
                Crash::Committed,
                3,
                0,
                JobState::RootRetained,
                JobState::TerminalReady,
            ),
            (
                Crash::Active,
                2,
                1,
                JobState::RootRetained,
                JobState::TerminalLeased,
            ),
            (
                Crash::RolledBack,
                2,
                0,
                JobState::RootReleasingCompensated,
                JobState::TerminalReady,
            ),
            (
                Crash::Reserved,
                2,
                0,
                JobState::RootReserved,
                JobState::TerminalLeased,
            ),
        ] {
            let (images, rows) = crash_images(crash);
            let joined = join_restart(
                &clone_directory(&images),
                RowRecoveryPolicy::WithRowClr,
                None,
            );
            assert_eq!(
                (joined.ready.0, joined.clrs, joined.row_cells),
                (ready, clrs, 0),
                "{crash:?}"
            );
            let mut expected = rows.clone();
            if crash == Crash::Committed {
                expected[0].1 = 200;
            }
            assert_eq!(joined.rows, expected, "{crash:?}");
            assert_bound(&images, root, terminal);
            println!(
                "RUP_R01_R04 {crash:?} q={ready} clrs={clrs} restart_steps={}",
                joined.steps
            );
        }
    });
}
