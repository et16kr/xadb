//! RDL-A01..A07 (dev3-row-delete.md#api, #tests) over the driver's literal
//! stand-ins: A's DELETE statement (`delete_statement`) and the executor
//! steps the autocommit and explicit DELETE entries run after it, for each
//! answer C's DELETE preflight and its Reserve give. The stand-ins classify
//! by a literal list of deletable rows (`State::visible`); C's actual
//! classification is RDL-C's (`plan4_retention/row_delete_tests.rs`). Frame
//! lengths are the contract's literals: DeleteForward `91304+2C+128h`,
//! DeleteClr `91096+2C+128h` with `C=180+L` (dev3-row-delete.md#delete-costs).
use super::*;

/// Fresh literal C state whose deletable rows are `visible` (RowId,
/// canonical length) over the actor's `chain`.
fn deleting(
    visible: &[(u64, usize)],
    chain: &[(u64, usize)],
    faults: &[(Call, Fault)],
) -> RefCell<State> {
    let state = state(chain, faults);
    state.borrow_mut().visible = visible.to_vec();
    state
}

fn row(row: u64) -> RowId {
    RowId::new(row).unwrap()
}

#[allow(
    clippy::result_large_err,
    reason = "the test harness returns the driver Failure unchanged"
)]
fn delete<'s>(
    call: &mut LiteralCall<'s>,
    target: u64,
    slot: &mut Option<LiteralApplied>,
    loan: &mut Loan,
) -> Outcome<'s> {
    let statement = statement(call.state);
    delete_statement(call, statement, &LiteralTable, loan, row(target), slot)
}

/// The calls of one deleted row after the statement check.
fn deleted(row: u64) -> [Call; 7] {
    [
        PreflightDelete(row),
        ReserveDelete(row),
        Call::ProducerLane(row),
        Forward(row),
        Prepare(row),
        Apply(row),
        FinishStatement,
    ]
}

/// One autocommit DELETE of `target`, as `autocommit_delete_live` runs it
/// after Begin and the table.
fn autocommit_delete(
    state: &RefCell<State>,
    target: u64,
) -> (Autocommit<u64>, Option<LiteralApplied>) {
    let mut loan = loan();
    let mut slot = None;
    let mut call = LiteralCall { state };
    let ran = delete(&mut call, target, &mut slot, &mut loan);
    (ended(&mut call, ran, &mut loan), slot)
}

/// One DELETE statement of `target` in the literal transaction, as
/// `transaction_delete_live` runs it after the statement start and the
/// table.
fn explicit_delete(
    call: &mut LiteralCall<'_>,
    transaction: &LiteralTransaction,
    target: u64,
    loan: &mut Loan,
) -> (Explicit<u64>, Option<LiteralApplied>) {
    let mut out = Explicit::Idle;
    let mut slot = None;
    let ran = delete(call, target, &mut slot, loan);
    if let Some(rows) = statement_ended(call, transaction, ran, loan, &mut out) {
        out = Explicit::Done { rows };
    }
    (out, slot)
}

/// RDL-A01: one row: C's preflight classifies the target deletable, the
/// delete Reserve takes it (no new RowId, no validation, nothing written to
/// the candidate lane), A's producer gives DeleteForward over the old cell,
/// and the statement is finished with 1. The autocommit entry commits it.
#[test]
fn rdl_a01_one_row_is_reserved_produced_applied_and_finished() {
    let state = deleting(&[(5, 22)], &[], &[]);
    let mut loan = loan();
    let mut slot = None;
    let mut call = LiteralCall { state: &state };
    assert_eq!(delete(&mut call, 5, &mut slot, &mut loan).unwrap(), 1);
    assert_eq!(slot, Some(LiteralApplied(5)));
    let state = state.into_inner();
    assert_eq!(state.log, calls(&[&[Check], &deleted(5)]));
    assert!(state.inputs.is_empty(), "a DELETE validates nothing");
    // C = 180 + 22, h = 0.
    assert_eq!(state.proposals, [proposal(RowOperation::Forward, 91_708)]);
    assert_eq!((state.row_high, &state.chain[..]), (6, &[(5, 22)][..]));
    assert!(state.visible.is_empty() && state.deleted == [5]);
    assert_eq!((state.outstanding, state.violations.len()), (None, 0));
    assert!(loan.iter().all(|byte| *byte == FILL));

    // Under a prior root of the actor (h = 1), through the autocommit steps.
    let state = deleting(&[(5, 21)], &[(4, 18)], &[]);
    let (out, slot) = autocommit_delete(&state, 5);
    match out {
        Autocommit::Committed { committed, rows } => assert_eq!((committed, rows), (42, 1)),
        other => panic!("expected Committed, got {other:?}"),
    }
    assert_eq!(slot, Some(LiteralApplied(5)));
    assert_eq!(take_log(&state), calls(&[&[Check], &deleted(5), &[Commit]]));
    assert_eq!(
        state.borrow().proposals,
        [proposal(RowOperation::Forward, 91_304 + 2 * 201 + 128)]
    );
    settled(&state);
}

/// RDL-A02: zero rows: a target C answers NoRow for (an absent key, the
/// actor's own tombstone, a row the snapshot does not see: all one answer
/// at A) finishes the statement with 0, no Reserve and the slot left None.
/// The autocommit entry commits the transaction with no root.
#[test]
fn rdl_a02_no_row_finishes_the_statement_with_zero() {
    let state = deleting(&[(5, 22)], &[], &[]);
    let (out, slot) = autocommit_delete(&state, 9);
    match out {
        Autocommit::Committed { committed, rows } => assert_eq!((committed, rows), (42, 0)),
        other => panic!("expected Committed, got {other:?}"),
    }
    assert_eq!(slot, None);
    assert_eq!(
        take_log(&state),
        [Check, PreflightDelete(9), FinishStatement, Commit]
    );
    settled(&state);

    // The actor's own tombstone: a second DELETE of the row it deleted in
    // the same transaction.
    let state = deleting(&[(5, 22)], &[], &[]);
    let mut loan = loan();
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let (out, slot) = explicit_delete(&mut call, &transaction, 5, &mut loan);
    assert_eq!((explicit_done(out), slot), (1, Some(LiteralApplied(5))));
    let (out, slot) = explicit_delete(&mut call, &transaction, 5, &mut loan);
    assert_eq!((explicit_done(out), slot), (0, None));
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check],
            &deleted(5),
            &[Check, PreflightDelete(5), FinishStatement]
        ])
    );
    settled(&state);
}

/// RDL-A03..A05: C's DELETE preflight refusals (Busy for a row a root Job
/// holds or a Leased T' tombstone, Unsupported for the actor's own INSERT
/// row, SerializationConflict for a T' tombstone committed after S) stay
/// distinct and unchanged, and end the row holding nothing: no finish, no
/// Reserve. The statement is `Returned`, so the autocommit entry finishes
/// it and rolls back and ends the transaction.
#[test]
fn rdl_a03_a05_preflight_refusals_stay_distinct_and_hold_nothing() {
    for cause in [
        Cause::Busy,
        Cause::Unsupported,
        Cause::SerializationConflict,
    ] {
        let fault = [(PreflightDelete(5), Fault::Before(cause))];
        let state = deleting(&[(5, 22)], &[], &fault);
        let mut slot = None;
        let mut call = LiteralCall { state: &state };
        let error = failed(delete(&mut call, 5, &mut slot, &mut loan()));
        assert_eq!(error.failure.stage, Stage::Preflight);
        assert_eq!(
            disk(&error.failure.error),
            (RetentionPhase::Prepare, cause, Effect::Unchanged)
        );
        assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
        returned(error.after);
        assert_eq!(slot, None);
        assert_eq!(take_log(&state), [Check, PreflightDelete(5)]);
        settled(&state);

        let state = deleting(&[(5, 22)], &[], &fault);
        let (out, slot) = autocommit_delete(&state, 5);
        let failure = aborted(out);
        assert_eq!(
            (failure.stage, disk(&failure.error).1),
            (Stage::Preflight, cause)
        );
        assert_eq!(slot, None);
        assert_eq!(
            take_log(&state),
            calls(&[
                &[Check, PreflightDelete(5), FinishStatement],
                &full_abort(&[])
            ])
        );
        settled(&state);
    }
}

/// RDL-A06: an explicit transaction goes on after a failed DELETE
/// statement: the refused statement is finished with nothing to roll back,
/// a later DELETE of another row applies, and the Commit carries the
/// caller's count. The caller's rollback of a transaction that deleted a row
/// compensates the DELETE root (DeleteClr), which restores the row.
#[test]
fn rdl_a06_explicit_transaction_goes_on_after_a_failed_delete() {
    let fault = [(PreflightDelete(5), Fault::Before(Cause::Busy))];
    let state = deleting(&[(5, 22), (6, 18)], &[], &fault);
    let mut loan = loan();
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let (out, _) = explicit_delete(&mut call, &transaction, 5, &mut loan);
    assert_eq!(explicit_failed(out).stage, Stage::Preflight);
    let (out, slot) = explicit_delete(&mut call, &transaction, 6, &mut loan);
    assert_eq!((explicit_done(out), slot), (1, Some(LiteralApplied(6))));
    match explicit_commit(&mut call, transaction, &mut loan, 1) {
        Explicit::Committed { committed, rows } => assert_eq!((committed, rows), (42, 1)),
        other => panic!("expected Committed, got {other:?}"),
    }
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check, PreflightDelete(5), FinishStatement],
            &[Check],
            &deleted(6),
            &[Commit],
        ])
    );
    settled(&state);

    // The caller's rollback: the DELETE root's CLR (C = 180 + 22, h = 0).
    let state = deleting(&[(5, 22)], &[], &[]);
    let mut call = LiteralCall { state: &state };
    let transaction = LiteralTransaction(1);
    let (out, _) = explicit_delete(&mut call, &transaction, 5, &mut loan);
    assert_eq!(explicit_done(out), 1);
    assert!(matches!(
        explicit_rollback(&mut call, transaction, &mut loan),
        Explicit::RolledBack
    ));
    assert_eq!(
        take_log(&state),
        calls(&[&[Check], &deleted(5), &full_abort(&[5])])
    );
    let state = state.into_inner();
    assert_eq!(
        state.proposals,
        [
            proposal(RowOperation::Forward, 91_708),
            proposal(RowOperation::Clr, 91_500)
        ]
    );
    assert_eq!((&state.visible[..], state.chain.len()), (&[(5, 22)][..], 0));
    assert!(state.violations.is_empty(), "{:?}", state.violations);
}

/// RDL-A07: the DELETE's steps after its preflight follow the INSERT's
/// rules. A FlushRequired is flushed and the same target tried again from
/// preflight; a refused Reserve finishes the preflight; a failure after the
/// Reserve cancels the one reservation (apply's Before reopens it first); a
/// Fatal answer calls nothing more. A DELETE applies at most its one row,
/// so a failed statement is `Returned` (or `MustResolve`), never
/// `RollbackRequired`.
#[test]
fn rdl_a07_steps_after_the_preflight_follow_the_insert_rules() {
    // FlushRequired twice, then the row.
    let state = deleting(&[(5, 22)], &[], &[(TryFlush(78), Fault::Steps(1))]);
    state.borrow_mut().flushes = vec![(5, 77), (5, 78)];
    let mut slot = None;
    let mut call = LiteralCall { state: &state };
    assert_eq!(delete(&mut call, 5, &mut slot, &mut loan()).unwrap(), 1);
    assert_eq!(
        take_log(&state),
        calls(&[
            &[Check, PreflightDelete(5), TryFlush(77)],
            &[PreflightDelete(5), TryFlush(78), ContinueFlush],
            &deleted(5),
        ])
    );
    settled(&state);

    let reserved = [Check, PreflightDelete(5), ReserveDelete(5)];
    let cases = [
        (
            ReserveDelete(5),
            Fault::Before(Cause::Capacity),
            Stage::Reserve,
            vec![FinishPreflight],
        ),
        (
            Call::ProducerLane(5),
            Fault::Before(Cause::Busy),
            Stage::ProducerLane,
            vec![Call::ProducerLane(5), Cancel(5)],
        ),
        (
            Forward(5),
            Fault::Producer(InsertError::IdentityMismatch),
            Stage::Produce,
            vec![Call::ProducerLane(5), Forward(5), Cancel(5)],
        ),
        (
            Prepare(5),
            Fault::Before(Cause::Unsupported),
            Stage::Prepare,
            vec![Call::ProducerLane(5), Forward(5), Prepare(5), Cancel(5)],
        ),
        (
            Apply(5),
            Fault::Before(Cause::Busy),
            Stage::Apply,
            vec![
                Call::ProducerLane(5),
                Forward(5),
                Prepare(5),
                Apply(5),
                Reopen(5),
                Cancel(5),
            ],
        ),
    ];
    for (at, fault, stage, tail) in cases {
        let state = deleting(&[(5, 22)], &[], &[(at, fault)]);
        let mut slot = None;
        let mut call = LiteralCall { state: &state };
        let error = failed(delete(&mut call, 5, &mut slot, &mut loan()));
        assert_eq!(error.failure.stage, stage);
        match fault {
            Fault::Producer(expected) => assert_eq!(insert(&error.failure.error), expected),
            Fault::Before(cause) => assert_eq!(
                disk(&error.failure.error),
                (RetentionPhase::Prepare, cause, Effect::Unchanged)
            ),
            _ => unreachable!(),
        }
        assert!(error.failure.cleanup.is_none() && !error.failure.must_resolve);
        returned(error.after);
        assert_eq!(take_log(&state), calls(&[&reserved, &tail]));
        assert_eq!(slot, None);
        assert!(state.borrow().chain.is_empty() && state.borrow().visible == [(5, 22)]);
        settled(&state);
    }

    // A Fatal Reserve: C keeps the obligation; nothing more is called.
    let state = deleting(&[(5, 22)], &[], &[(ReserveDelete(5), Fault::Fatal)]);
    let mut slot = None;
    let mut call = LiteralCall { state: &state };
    let error = failed(delete(&mut call, 5, &mut slot, &mut loan()));
    assert_eq!(error.failure.stage, Stage::Reserve);
    assert!(error.failure.must_resolve);
    assert!(must_resolve(error.after).is_some());
    assert_eq!(take_log(&state), reserved);
}
