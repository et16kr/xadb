//! RDL-J11 (dev3-row-delete.md#classification, #implementation-3): C's
//! judgement of rows another transaction holds, over the actual supply.
//! Each transaction begins on its own Row Status (a second and a third
//! Status as in RDL-N05), so a transaction begins while another is Active
//! or before another's cleanup. Expectations are §5's table rows, never
//! values read back from the same producer.
use super::*;
use crate::storage::row::insert::driver::live::begin_transaction_live;

/// A's Begin on `status` with `isolation` into `holder`.
macro_rules! begin_at {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $status:expr, $isolation:expr, $private:expr, $holder:expr) => {{
        let mut out = Explicit::Idle;
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        begin_transaction_live(&mut call, $status, $isolation, $private, $holder, &mut out);
        begun(out);
    }};
}

/// A DELETE statement's refusal at C's preflight: no effect, nothing held,
/// the transaction goes on.
macro_rules! refused {
    ($out:expr, $slot:expr, $cause:pat) => {{
        let failure = statement_failed($out);
        let DriverError::Disk(error) = &failure.error else {
            panic!("expected C's refusal, got {failure:?}");
        };
        assert_eq!(failure.stage, Stage::Preflight);
        assert!(
            matches!(error.cause, $cause) && error.effect == Effect::Unchanged,
            "{error:?}"
        );
        assert!($slot.is_none());
    }};
}

/// RDL-J11: rows 100 and 101 are committed at q 2 and drained. A
/// (ReadCommitted) deletes 100 and Y (ReadCommitted, a second Status)
/// begins:
/// - Y's DELETE of 100 is Busy (A's tombstone, A Leased); Y goes on and
///   INSERTs 102.
/// - A's DELETE of 102 is 0 rows (Y's INSERT row, Y Leased).
/// - Y commits at q 3 with no drain. A's DELETE of 102 is Busy (Y's row,
///   committed at q 3 ≤ S and not yet pruned).
/// - The live cleanup's steps are Y's only (A is Active): Prune, three
///   ReleaseRoles, RootFree, TerminalFinish. After the Prune and after the
///   third ReleaseRole, 102 is `Committed(3)` and Y's root still holds its
///   RowId: Busy. After RootFree no root holds it and A's DELETE deletes
///   it (1 row).
/// - Z (Serializable, a third Status) begins at q 3. A commits at q 4 with
///   the caller's 2 rows. Z's DELETEs of 100 and 102 are
///   SerializationConflict (A's tombstones, committed at q 4 > Z's
///   snapshot 3); Z rolls back.
///
/// The drain cleans A's two delete roots (5 + 5 groups), its Terminal, and
/// Y's and Z's Terminals (one group each); a new statement reads only 101.
/// C's restart reaches READY at q 4 with no CLR and reads only 101.
#[test]
fn rdl_j11_rows_another_transaction_holds_are_busy_until_its_cleanup() {
    case_within("RDL-J11", std::time::Duration::from_secs(300), || {
        let oracle = Oracle::new();
        let (images, kept) = with_fresh!(
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
                let (target, kept) = (rows[0].0, rows[1]);

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
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    target
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), target);

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
                // A's tombstone, A Leased.
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
                    target
                );
                refused!(out, slot, RetentionCause::Busy);
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
                // Y's INSERT row, Y Leased: not in A's snapshot.
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    new
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
                        1
                    )),
                    (3, 1)
                );
                // Y's row, committed at q 3 ≤ S, before its Prune.
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    new
                );
                refused!(out, slot, RetentionCause::Busy);

                // Y's cleanup, one live step at a time (A's Jobs are not
                // finished, so every step is Y's).
                let mut steps = Vec::new();
                while let Some((ordinal, operation)) = writer.row_cleanup_next(&[]).unwrap() {
                    if let Err(error) =
                        retire_one!(writer, owner, private, flush, flusher, ordinal, operation)
                    {
                        panic!("live {operation:?}: {error:?}");
                    }
                    steps.push(operation);
                    // After the Prune and the third ReleaseRole (the order
                    // is asserted below), Y's root still holds the RowId.
                    if steps.len() == 1 || steps.len() == 4 {
                        let (out, slot) = delete_statement!(
                            &mut writer,
                            owner,
                            flush,
                            flusher,
                            reader,
                            reader_private,
                            &mut a,
                            private,
                            6,
                            new
                        );
                        refused!(out, slot, RetentionCause::Busy);
                    }
                }
                assert_eq!(
                    steps,
                    [
                        RowCleanup::Prune,
                        RowCleanup::ReleaseRole,
                        RowCleanup::ReleaseRole,
                        RowCleanup::ReleaseRole,
                        RowCleanup::RootFree,
                        RowCleanup::TerminalFinish,
                    ]
                );
                let (out, slot) = delete_statement!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    reader,
                    reader_private,
                    &mut a,
                    private,
                    6,
                    new
                );
                assert_eq!(done(out), 1);
                assert_eq!(slot.expect("deleted row").row(), new);

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
                // A's tombstones, committed at q 4 after Z's snapshot 3.
                for row in [target, new] {
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
                        row
                    );
                    refused!(out, slot, RetentionCause::SerializationConflict);
                }
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
                        [target, kept.0, new]
                    ),
                    [None, Some(101), None]
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
        assert_eq!((joined.ready.0, joined.clrs, joined.row_cells), (4, 0, 0));
        assert_eq!(joined.rows, [kept]);
        println!(
            "RDL_J11 busy=4 conflict=2 q=3,4 restart_steps={}",
            joined.steps
        );
    });
}
