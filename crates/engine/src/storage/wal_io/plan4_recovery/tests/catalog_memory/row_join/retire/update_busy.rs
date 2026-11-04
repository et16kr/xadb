//! RUP-J06..J08 (dev3-row-update.md#classification, #tests; RUP-A01..A04):
//! C's judgement of rows another transaction holds, the new value's
//! length refusal and the new value's checks after the judgement, over the
//! actual supply. Each transaction begins on its own Row Status (as
//! RDL-J11). Expectations are §5's table rows, never values read back from
//! the same producer.
use super::*;
use crate::storage::{row::insert::driver::live::begin_transaction_live, row_record::RowError};

/// A's Begin on `status` with `isolation` into `holder`.
macro_rules! begin_at {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $status:expr, $isolation:expr, $private:expr, $holder:expr) => {{
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        begin_transaction_live(&mut call, $status, $isolation, $private, $holder, &mut out);
        begun(out);
    }};
}

/// RUP-J06 (RUP-A01, A02, A04): rows 100 and 101 are committed at q 2 and
/// drained. A (ReadCommitted) updates 100 to 200 and Y (ReadCommitted, a
/// second Status) begins:
/// - Y's UPDATE and DELETE of 100 are Busy (A's UPDATE row: Inline
///   `Transaction(A)`, its effect-2 root, A Leased). Y goes on, INSERTs 102
///   and updates 101 to 111.
/// - A's UPDATE of 102 is 0 rows (Y's INSERT row, Y Leased: invisible).
/// - Y commits at q 3 with no drain. A's UPDATEs of 101 (Y's UPDATE row,
///   effect 2) and of 102 (Y's INSERT row) are Busy: committed at q 3 <= S
///   and not yet pruned.
/// - Y's live cleanup, one step at a time (A is Active): UpdatePrune and
///   Prune (head first), each root's three ReleaseRoles and RootFree, then
///   TerminalFinish. While Y's UPDATE root holds 101's RowId (after each of
///   the first nine steps) A's UPDATE of 101 is Busy; after its RootFree
///   it updates 111 to 121 (1 row).
/// - Z (Serializable, a third Status) begins at q 3. A commits at q 4 with
///   the caller's 2 rows. Z's UPDATEs of 100 and 101 and its DELETE of 100
///   are SerializationConflict (A's UPDATE rows, committed at q 4 > Z's
///   snapshot 3); Z rolls back.
///
/// The drain cleans A's two update roots (5 + 5 groups), A's Terminal and
/// Z's Terminal: 12 groups. A new statement reads 200, 121 and 102; C's
/// restart reaches READY at q 4 with no CLR and reads the same.
#[test]
fn rup_j06_rows_another_transaction_updates_are_busy_until_its_cleanup() {
    case_within("RUP-J06", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, expected) = with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (a_status, _) = create_table(live, owner, flusher, status, undo, private, true);
                let y_status = system_init(live, owner, flusher, false);
                let z_status = system_init(live, owner, flusher, false);
                for _ in 0..4 {
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
                    &a_status,
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

                let (mut a, mut y, mut z) = (None, None, None);
                begin_at!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &a_status,
                    IsolationLevel::ReadCommitted,
                    private,
                    &mut a
                );
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    ids[0],
                    int(200)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[0]);

                begin_at!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &y_status,
                    IsolationLevel::ReadCommitted,
                    private,
                    &mut y
                );
                // A's UPDATE row, A Leased.
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut y,
                    private,
                    6,
                    ids[0],
                    int(7)
                );
                refused!(out, slot, Stage::Preflight, RetentionCause::Busy);
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut y,
                    private,
                    6,
                    ids[0]
                );
                refused!(out, slot, Stage::Preflight, RetentionCause::Busy);
                let (out, slots) = statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut y,
                    private,
                    6,
                    ints(&[102])
                );
                assert_eq!(done(out), 1);
                let new = applied(slots, &[102])[0].0;
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut y,
                    private,
                    6,
                    ids[1],
                    int(111)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[1]);
                // Y's INSERT row, Y Leased: not in A's snapshot.
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    new,
                    int(7)
                );
                assert_eq!(done(out), 0);
                assert!(slot.is_none());
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut y,
                        private,
                        2
                    )),
                    (3, 2)
                );
                // Y's rows, committed at q 3 <= S, before their prunes.
                for row in [ids[1], new] {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut a,
                        private,
                        6,
                        row,
                        int(7)
                    );
                    refused!(out, slot, Stage::Preflight, RetentionCause::Busy);
                }

                // Y's cleanup, one live step at a time (A's Jobs are not
                // finished, so every step is Y's). The head root prunes
                // first (the UPDATE root, Y's later undo, then the INSERT
                // root); ReleaseRole and RootFree then take the Releasing
                // roots in ordinal order (the INSERT root's first). While
                // Y's UPDATE root holds 101's RowId, A's UPDATE of it is
                // Busy: after each of the first nine steps.
                let mut steps = Vec::new();
                let mut busy = 0;
                while let Some((ordinal, operation)) = writer.row_cleanup_next(&[]).unwrap() {
                    if let Err(error) =
                        retire_one!(writer, owner, private, flush, flusher, ordinal, operation)
                    {
                        panic!("live {operation:?}: {error:?}");
                    }
                    steps.push(operation);
                    if holds(writer.catalog_test_jobs(), ids[1]) {
                        let (out, slot) = update_statement!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut a,
                            private,
                            6,
                            ids[1],
                            int(7)
                        );
                        refused!(out, slot, Stage::Preflight, RetentionCause::Busy);
                        busy += 1;
                    }
                }
                let release = [
                    RowCleanup::ReleaseRole,
                    RowCleanup::ReleaseRole,
                    RowCleanup::ReleaseRole,
                    RowCleanup::RootFree,
                ];
                let mut expected_steps = vec![RowCleanup::UpdatePrune, RowCleanup::Prune];
                expected_steps.extend(release);
                expected_steps.extend(release);
                expected_steps.push(RowCleanup::TerminalFinish);
                assert_eq!((steps, busy), (expected_steps, 9));
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    ids[1],
                    int(121)
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[1]);

                begin_at!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &z_status,
                    IsolationLevel::Serializable,
                    private,
                    &mut z
                );
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut a,
                        private,
                        2
                    )),
                    (4, 2)
                );
                // A's UPDATE rows, committed at q 4 after Z's snapshot 3.
                for row in ids {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut z,
                        private,
                        6,
                        row,
                        int(7)
                    );
                    refused!(
                        out,
                        slot,
                        Stage::Preflight,
                        RetentionCause::SerializationConflict
                    );
                }
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut z,
                    private,
                    6,
                    ids[0]
                );
                refused!(
                    out,
                    slot,
                    Stage::Preflight,
                    RetentionCause::SerializationConflict
                );
                let out = rollback!(&mut writer, owner, flush, flusher, &mut z, private);
                assert!(
                    matches!(out, Explicit::RolledBack),
                    "expected RolledBack, got {out:?}"
                );
                assert!(a.is_none() && y.is_none() && z.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(12, true)]
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
                        [ids[0], ids[1], new]
                    ),
                    [Some(200), Some(121), Some(102)]
                );
                assert!(!fence.is_fenced());
                let expected = vec![(ids[0].get(), 200), (ids[1].get(), 121), (new.get(), 102)];
                (state.lock().unwrap().images(), expected)
            }
        );
        let joined = join_restart(
            &clone_directory(&images),
            RowRecoveryPolicy::RejectInitialLosers,
            None,
        );
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (4, 0, 0));
        assert_eq!(joined.rows, expected);
        println!(
            "RUP_J06 busy=6 conflict=3 q=3,4 restart_steps={}",
            joined.steps
        );
    });
}

/// Whether a Row root Job of effect 2 (UPDATE) holds `row`.
fn holds(jobs: &[[u8; 1024]; 64], row: RowId) -> bool {
    jobs.iter().any(|bytes| {
        let job = job3::decode_job(
            bytes,
            super::super::super::super::super::super::support::namespace(),
        )
        .unwrap();
        job.scope() == Some(JobScope::Row)
            && job.state().is_root()
            && job.effect() == job3::RowEffect::Update
            && job.row_id() == Some(row)
    })
}

/// A value whose canonical row is above the 2048 B inline bound.
static LONG: [u8; 2100] = [7; 2100];

/// RUP-J07 (RUP-A03, #classification "새 값 검사"): over a table of one
/// Binary column, the row x'01' is committed at q 2 and drained. A's
/// autocommit UPDATE to a 2,100 B value is refused by A's validation over
/// C's lane, the INSERT's length check (Validate, Unsupported), before any
/// Reserve: the transaction aborts with no Row Job of it but its Terminal,
/// which the drain finishes (1 group). In an explicit transaction the same
/// refusal fails the statement only; the transaction goes on, updates the
/// row to a 16 B value (1 row) and commits at q 3 (the aborted one took no
/// q); the drain cleans its root and its Terminal (5 + 1 groups).
#[test]
fn rup_j07_long_new_value_is_refused_before_any_effect() {
    case_within("RUP-J07", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &binary());
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
                    vec![[ValueRef::Binary(&[1])]]
                );
                assert_eq!(committed(out), (2, 1));
                let target = slots[0].as_ref().expect("applied row").row();
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(6, true)]
                );
                let long = |failure: &Failure| {
                    assert_eq!(failure.stage, Stage::Validate);
                    assert!(
                        matches!(failure.error, DriverError::Insert(InsertError::Unsupported)),
                        "{failure:?}"
                    );
                };
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
                    [ValueRef::Binary(&LONG)]
                );
                match out {
                    Autocommit::Aborted(failure) => long(&failure),
                    other => panic!("expected Aborted, got {other:?}"),
                }
                assert!(slot.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(1, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());

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
                let jobs = writer.catalog_test_jobs().to_vec();
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
                    [ValueRef::Binary(&LONG)]
                );
                long(&statement_failed(out));
                assert!(slot.is_none());
                assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
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
                    [ValueRef::Binary(&[9; 16])]
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), target);
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
                assert!(!fence.is_fenced());
                println!("RUP_J07 autocommit=Aborted(Validate) explicit=Failed,1 q=3");
            }
        );
    });
}

/// A one-column Binary T, so a value can be of the wrong type (an Int64)
/// or longer than the inline bound (`LONG`).
fn binary() -> TableSpec {
    TableSpec {
        name: "T".to_owned(),
        columns: vec![Column {
            name: "x".to_owned(),
            data_type: DataType::Binary,
            nullable: false,
        }],
    }
}

/// A statement refused at A's check of the new value over C's validation
/// lane: `bad` is the cause, nothing changed and the transaction goes on.
fn invalid(out: Explicit<RowCommitObservation>, bad: fn(&DriverError) -> bool) {
    let failure = statement_failed(out);
    assert_eq!(failure.stage, Stage::Validate);
    assert!(bad(&failure.error), "{failure:?}");
}

fn mismatch(error: &DriverError) -> bool {
    matches!(
        error,
        DriverError::Insert(InsertError::Row(RowError::SchemaMismatch))
    )
}

fn too_long(error: &DriverError) -> bool {
    matches!(error, DriverError::Insert(InsertError::Unsupported))
}

/// RUP-J08 (RUP-A01..A04, #classification "새 값 검사", §17): A checks the
/// new value over C's validation lane, which only a Ready preflight opens
/// (as an INSERT's value follows its preflight), so C's judgement of the
/// target row comes first. `WRONG` (an Int64 for the Binary column) and
/// `LONG` (canonical row over 2048 B):
/// - Autocommit UPDATEs of a RowId with no key commit 0 rows (q 3, q 4).
/// - A (ReadCommitted) updates row 1 to a 16 B value; its own UPDATE row
///   with `WRONG` is Unsupported at the preflight.
/// - Y (ReadCommitted, a second Status): A's row with either value is Busy,
///   a RowId with no key is 0 rows, and the committed row 2 is refused at
///   validation (SchemaMismatch, Unsupported). No Job changes.
/// - Z (Serializable, a third Status) begins at q 4; A commits at q 5; Z's
///   UPDATEs of A's row with either value are SerializationConflict.
///
/// Y and Z roll back; the drain cleans A's root and the three Terminals.
#[test]
fn rup_j08_the_new_value_is_checked_after_the_row_judgement() {
    case_within("RUP-J08", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (a_status, _) =
                    create_table_with(live, owner, flusher, status, undo, private, true, &binary());
                let y_status = system_init(live, owner, flusher, false);
                let z_status = system_init(live, owner, flusher, false);
                for _ in 0..4 {
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
                    &a_status,
                    private,
                    6,
                    vec![[ValueRef::Binary(&[1])], [ValueRef::Binary(&[2])]]
                );
                assert_eq!(committed(out), (2, 2));
                let ids = [
                    slots[0].as_ref().expect("applied row").row(),
                    slots[1].as_ref().expect("applied row").row(),
                ];
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(11, true)]
                );
                const WRONG: [ValueRef<'static>; 1] = [ValueRef::Int64(5)];
                let no_key = RowId::new(999).unwrap();
                for (q, value) in [(3, WRONG), (4, [ValueRef::Binary(&LONG)])] {
                    let (out, slot) = update!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &a_status,
                        private,
                        6,
                        no_key,
                        value
                    );
                    assert_eq!(committed(out), (q, 0));
                    assert!(slot.is_none());
                    assert_eq!(
                        drains!(&mut writer, owner, flush, flusher, private),
                        [drain(1, true)]
                    );
                }

                let (mut a, mut y, mut z) = (None, None, None);
                begin_at!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &a_status,
                    IsolationLevel::ReadCommitted,
                    private,
                    &mut a
                );
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    ids[0],
                    [ValueRef::Binary(&[9; 16])]
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("updated row").row(), ids[0]);
                // A's own UPDATE row.
                let (out, slot) = update_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    ids[0],
                    WRONG
                );
                refused!(out, slot, Stage::Preflight, RetentionCause::Unsupported);

                begin_at!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &y_status,
                    IsolationLevel::ReadCommitted,
                    private,
                    &mut y
                );
                let jobs = writer.catalog_test_jobs().to_vec();
                for value in [WRONG, [ValueRef::Binary(&LONG)]] {
                    // A's UPDATE row, A Leased.
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut y,
                        private,
                        6,
                        ids[0],
                        value
                    );
                    refused!(out, slot, Stage::Preflight, RetentionCause::Busy);
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut y,
                        private,
                        6,
                        no_key,
                        value
                    );
                    assert_eq!(done(out), 0);
                    assert!(slot.is_none());
                }
                // The committed row 2: the judgement is Proceed, so the value
                // is checked.
                for (value, bad) in [
                    (WRONG, mismatch as fn(&DriverError) -> bool),
                    ([ValueRef::Binary(&LONG)], too_long),
                ] {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut y,
                        private,
                        6,
                        ids[1],
                        value
                    );
                    invalid(out, bad);
                    assert!(slot.is_none());
                }
                assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);

                begin_at!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    &z_status,
                    IsolationLevel::Serializable,
                    private,
                    &mut z
                );
                assert_eq!(
                    explicit_committed(commit!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        &mut a,
                        private,
                        1
                    )),
                    (5, 1)
                );
                // A's UPDATE row, committed at q 5 after Z's snapshot 4.
                for value in [WRONG, [ValueRef::Binary(&LONG)]] {
                    let (out, slot) = update_statement!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        reader,
                        reader_private,
                        &mut z,
                        private,
                        6,
                        ids[0],
                        value
                    );
                    refused!(
                        out,
                        slot,
                        Stage::Preflight,
                        RetentionCause::SerializationConflict
                    );
                }
                for holder in [&mut y, &mut z] {
                    let out = rollback!(&mut writer, owner, flush, flusher, holder, private);
                    assert!(
                        matches!(out, Explicit::RolledBack),
                        "expected RolledBack, got {out:?}"
                    );
                }
                assert!(a.is_none() && y.is_none() && z.is_none());
                assert_eq!(
                    drains!(&mut writer, owner, flush, flusher, private),
                    [drain(8, true)]
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                assert!(!fence.is_fenced());
                println!("RUP_J08 no_key=0,0 busy=2 invalid=2 conflict=2 q=5");
            }
        );
    });
}
