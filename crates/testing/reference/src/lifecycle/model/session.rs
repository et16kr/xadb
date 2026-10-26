//! Session, transaction, commit continuation, signal, reopen and recovery transitions.
use super::{EdgeClass, Exclusion, Flow, INVALID, OriginCause, Run, Step, unpack};
use crate::lifecycle::schema::{
    Access, Api, Autocommit, BindingLife, Certainty, Cleanup, ClientKnowledge, CommitProgress,
    CompletionPhase, ContextData, ContextRef, Deadline, Decision, Delivery, EngineAdmission,
    EngineCause, EngineRecord, EngineRef, IdentityKind, NextWork, ObligationReason, ParkedContext,
    Phase, PreparedSlot, ReceiptKind, ReopenKind, ResourceKind, ResultState, Retirement, Retry,
    ReturnCode, SessionRecord, SessionRef, Settlement, Shell, SlotState, StatementEffect,
    StatementLife, TransactionLife, TransactionOrigin, TransactionRecord, TransactionRef,
};

impl Run<'_> {
    pub(super) fn begin(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let caller = self.caller()?;
        if caller.transaction.is_some() {
            return self.reject(ReturnCode::TxnState);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        let context = self.context.ok_or(INVALID)?;
        let scope = Self::scope(&caller);
        let id = self.edit.issue(IdentityKind::Transaction, scope, 255, 0);
        self.edit.add_transaction(TransactionRecord {
            id,
            origin: TransactionOrigin::ExplicitBegin,
            access: Access::ReadWrite,
            life: TransactionLife::Active,
            prior_successful_write: false,
            progress: CommitProgress::None,
            decision: Decision::NotAttempted,
            client_knowledge: ClientKnowledge::NoDecisionObserved,
            delivery: Delivery::NotApplicable,
            birth_session: SessionRef(caller.session.0),
            receipt: None,
            parked_context: None,
            obligation: None,
            completion_fact: None,
        })?;
        for kind in [
            ResourceKind::Undo,
            ResourceKind::Locks,
            ResourceKind::AbortReserve,
        ] {
            self.edit.birth(kind, id, context)?;
        }
        self.edit.actor_mut(caller.actor)?.transaction = Some(TransactionRef(id.nonce));
        self.complete_fresh()?;
        let response = self.success(Phase::Entry, StatementEffect::NoEffect);
        self.respond(response, Some(id.nonce))
    }

    pub(super) fn set_autocommit(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let caller = self.caller()?;
        if caller.transaction.is_some() {
            return self.reject(ReturnCode::TxnState);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        self.edit.actor_mut(caller.actor)?.autocommit = if self.input.set_autocommit == 0 {
            Autocommit::On
        } else {
            Autocommit::Off
        };
        self.complete_fresh()?;
        let response = self.success(Phase::Entry, StatementEffect::NoEffect);
        self.respond(response, None)
    }

    /// Shared entry of CommitAtomic and BeginCommit; Some(transaction) when admitted.
    fn commit_entry(&mut self) -> Step<Result<u16, Flow>> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Err(Flow::Exclude(rule)));
        }
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code).map(Err);
        }
        let caller = self.caller()?;
        if caller.active_public_result.is_some() {
            return self.reject(ReturnCode::ResultBusy).map(Err);
        }
        let Some(t) = caller.transaction else {
            return self.reject(ReturnCode::TxnState).map(Err);
        };
        if let Some(code) = self.entry_controls() {
            return self.reject(code).map(Err);
        }
        Ok(Ok(t.0))
    }

    pub(super) fn commit_atomic(&mut self) -> Step<Flow> {
        let t = match self.commit_entry()? {
            Ok(t) => t,
            Err(flow) => return Ok(flow),
        };
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let record = self.edit.transaction(t)?;
        if let Some(rule) = self.envelope_transaction(Some(record.id)) {
            return Ok(Flow::Exclude(rule));
        }
        let changed = record.access == Access::ReadWrite && record.prior_successful_write;
        let context = self.context.ok_or(INVALID)?;
        match self.input.outcome {
            0 if changed => self.publish(t, context),
            0 => self.no_write(t, true, context),
            1 if changed => Ok(Flow::Exclude(Exclusion::CommitNoWrite)),
            1 => self.no_write(t, false, context),
            2 | 5 | 6 => {
                let (code, origin) = match self.input.outcome {
                    2 => (ReturnCode::Conflict, OriginCause::Conflict),
                    5 => (ReturnCode::Cancelled, OriginCause::Cancelled),
                    _ => (ReturnCode::Timeout, OriginCause::Timeout),
                };
                match self.input.outcome {
                    5 => self.edge.controls.observed_cancel = true,
                    6 => self.edge.controls.observed_deadline = true,
                    _ => {}
                }
                self.abort_transaction(t)?;
                self.consume(
                    Cleanup::Complete,
                    NextWork::NotConsumed,
                    2,
                    Some(t),
                    None,
                    false,
                )?;
                self.complete_fresh()?;
                let response = self.error(
                    code,
                    Phase::Commit,
                    StatementEffect::StatementRestored,
                    Retry::WholeTransactionDecision,
                    origin,
                );
                self.respond(response, Some(t))
            }
            3 => {
                if record.access != Access::ReadWrite {
                    return Ok(Flow::Exclude(Exclusion::ReadOnlyCleanup));
                }
                self.recovery_required(t, Phase::Commit)
            }
            4 | 7 if !changed => Ok(Flow::Exclude(Exclusion::CommitNoWrite)),
            4 => {
                self.edit.transaction_mut(t)?.progress = CommitProgress::Attempted;
                self.unknown(t, Phase::Commit)
            }
            7 => self.publication_pending(t, context),
            _ => Ok(Flow::Exclude(Exclusion::Envelope)),
        }
    }

    /// Changed RW atomic commit success: Published, durable receipt delivered, RW released.
    fn publish(&mut self, t: u16, context: crate::lifecycle::schema::Identity) -> Step<Flow> {
        if self
            .install_receipt(t, ReceiptKind::DurableWrite, Delivery::Delivered)?
            .is_none()
        {
            return Ok(Flow::Exclude(Exclusion::Envelope));
        }
        let record = self.edit.transaction_mut(t)?;
        record.progress = CommitProgress::Published;
        record.delivery = Delivery::Delivered;
        self.end_transaction(t, Some(Decision::Committed))?;
        self.consume(
            Cleanup::NotConsumed,
            NextWork::NotConsumed,
            4 | 8,
            Some(t),
            None,
            true,
        )?;
        self.link_receipt_fact(t, context)?;
        self.settle_drain(t)?;
        self.complete_fresh()?;
        let response = self.success(Phase::Commit, StatementEffect::NoEffect);
        self.respond(response, Some(t))
    }

    /// No-write validation success, with or without a supplied completion identity.
    fn no_write(
        &mut self,
        t: u16,
        identity: bool,
        context: crate::lifecycle::schema::Identity,
    ) -> Step<Flow> {
        let atomic = self.input.api == Api::CommitAtomic;
        if identity {
            let delivery = if atomic {
                Delivery::Delivered
            } else {
                Delivery::NotDelivered
            };
            if self
                .install_receipt(t, ReceiptKind::NoWriteCompletion, delivery)?
                .is_none()
            {
                return Ok(Flow::Exclude(Exclusion::Envelope));
            }
            self.edit.transaction_mut(t)?.delivery = delivery;
        }
        let record = self.edit.transaction_mut(t)?;
        record.progress = CommitProgress::NoWriteValidated;
        record.decision = Decision::Committed;
        record.life = TransactionLife::Ended;
        if atomic {
            self.end_transaction(t, Some(Decision::Committed))?;
        }
        let parts = if identity { 4 | 8 } else { 8 };
        self.consume(
            Cleanup::NotConsumed,
            NextWork::NotConsumed,
            parts,
            Some(t),
            None,
            identity,
        )?;
        if identity {
            self.link_receipt_fact(t, context)?;
        }
        if atomic {
            self.complete_fresh()?;
            let response = self.success(Phase::Commit, StatementEffect::NoEffect);
            self.respond(response, Some(t))
        } else {
            self.environment_response(t)
        }
    }

    fn recovery_required(&mut self, t: u16, phase: Phase) -> Step<Flow> {
        self.fence(
            Some(t),
            EngineCause::UndoIncomplete,
            Decision::NotCommittedRecoveryRequired,
        )?;
        self.consume(
            Cleanup::Incomplete,
            NextWork::NotConsumed,
            2,
            Some(t),
            Some(ObligationReason::KnownNotCommittedUndoPending),
            false,
        )?;
        self.finish_commit_call()?;
        let response = self.admitted_error(
            ReturnCode::RecoveryRequired,
            phase,
            StatementEffect::RecoveryRequired,
            Retry::AfterRecovery,
            EngineCause::UndoIncomplete,
            OriginCause::UndoIncomplete,
        );
        self.respond(response, Some(t))
    }

    fn unknown(&mut self, t: u16, phase: Phase) -> Step<Flow> {
        self.fence(Some(t), EngineCause::CommitUnknown, Decision::Unknown)?;
        self.consume(
            Cleanup::NotConsumed,
            NextWork::NotConsumed,
            8,
            Some(t),
            Some(ObligationReason::CommitDecisionUnknown),
            false,
        )?;
        self.finish_commit_call()?;
        let response = self.admitted_error(
            ReturnCode::ResultUnknown,
            phase,
            StatementEffect::RecoveryRequired,
            Retry::AfterRecovery,
            EngineCause::CommitUnknown,
            OriginCause::CommitUnknown,
        );
        self.respond(response, Some(t))
    }

    fn publication_pending(
        &mut self,
        t: u16,
        context: crate::lifecycle::schema::Identity,
    ) -> Step<Flow> {
        let atomic = self.input.api == Api::CommitAtomic;
        let existing = self.edit.transaction(t)?.receipt.is_some();
        if (atomic || !existing)
            && self
                .install_receipt(t, ReceiptKind::DurableWrite, Delivery::NotDelivered)?
                .is_none()
        {
            return Ok(Flow::Exclude(Exclusion::Envelope));
        }
        let record = self.edit.transaction_mut(t)?;
        record.progress = CommitProgress::DurableConfirmed;
        record.delivery = Delivery::NotDelivered;
        self.fence(
            Some(t),
            EngineCause::CommittedPublicationPending,
            Decision::Committed,
        )?;
        self.consume(
            Cleanup::NotConsumed,
            NextWork::NotConsumed,
            4 | 8,
            Some(t),
            Some(ObligationReason::KnownCommittedPublicationPending),
            true,
        )?;
        if atomic || !existing {
            self.link_receipt_fact(t, context)?;
        }
        self.finish_commit_call()?;
        let response = self.admitted_error(
            ReturnCode::ResultUnknown,
            Phase::Commit,
            StatementEffect::RecoveryRequired,
            Retry::AfterRecovery,
            EngineCause::CommittedPublicationPending,
            OriginCause::PublicationPending,
        );
        self.respond(response, Some(t))
    }

    /// Atomic commit calls complete their fresh context; Finish completes the parked one.
    fn finish_commit_call(&mut self) -> Step<()> {
        if self.input.api == Api::FinishCommit {
            let context = self.context.ok_or(INVALID)?;
            self.complete_parked(context)
        } else {
            self.complete_fresh()
        }
    }

    pub(super) fn begin_commit(&mut self) -> Step<Flow> {
        let t = match self.commit_entry()? {
            Ok(t) => t,
            Err(flow) => return Ok(flow),
        };
        self.edit.transaction_mut(t)?.progress = CommitProgress::BeforeAttempt;
        let payload = ParkedContext {
            api: Api::BeginCommit,
            original_deadline: self.input.original_deadline,
            cancel_supplied_or_signalled: false,
            deadline_reached: false,
            cancel_observed_at_completion: false,
            deadline_observed_at_completion: false,
            completion_phase: CompletionPhase::NotCompleted,
            statement: None,
            result: None,
            transaction: Some(TransactionRef(t)),
        };
        self.park(payload, false)?;
        let response = self.success(Phase::Commit, StatementEffect::NoEffect);
        self.respond(response, Some(t))
    }

    pub(super) fn commit_fact(&mut self) -> Step<Flow> {
        let (context, payload) = match self.pending(Api::BeginCommit)? {
            Ok(found) => found,
            Err(code) => return self.control_plane(code),
        };
        self.context = Some(context);
        let t = payload.transaction.ok_or(INVALID)?.0;
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let record = self.edit.transaction(t)?;
        if let Some(rule) = self.envelope_transaction(Some(record.id)) {
            return Ok(Flow::Exclude(rule));
        }
        let changed = record.access == Access::ReadWrite && record.prior_successful_write;
        let caller = self.caller()?;
        let fenced = self.edit.engine(caller.engine.0)?.admission != EngineAdmission::Admitting;
        let gated = payload.cancel_supplied_or_signalled || payload.deadline_reached || fenced;
        let progress = record.progress;
        self.edge.class = EdgeClass::NoContextIssue;
        match (self.input.outcome, progress) {
            (0 | 3 | 4, CommitProgress::BeforeAttempt) if gated => {
                Ok(Flow::Exclude(Exclusion::CommitPreattemptGate))
            }
            (0, CommitProgress::BeforeAttempt) if changed => {
                let row = self.edit.transaction_mut(t)?;
                row.progress = CommitProgress::Attempted;
                row.decision = Decision::Unknown;
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    8,
                    Some(t),
                    None,
                    false,
                )?;
                self.environment_response(t)
            }
            (1, CommitProgress::Attempted) => {
                if self
                    .install_receipt(t, ReceiptKind::DurableWrite, Delivery::NotDelivered)?
                    .is_none()
                {
                    return Ok(Flow::Exclude(Exclusion::Envelope));
                }
                let record = self.edit.transaction_mut(t)?;
                record.progress = CommitProgress::DurableConfirmed;
                record.decision = Decision::Committed;
                record.delivery = Delivery::NotDelivered;
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    4 | 8,
                    Some(t),
                    None,
                    true,
                )?;
                self.link_receipt_fact(t, context)?;
                self.environment_response(t)
            }
            (2, CommitProgress::DurableConfirmed) => {
                let record = self.edit.transaction_mut(t)?;
                record.progress = CommitProgress::Published;
                record.life = TransactionLife::Ended;
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    4 | 8,
                    Some(t),
                    None,
                    true,
                )?;
                self.settle_drain(t)?;
                self.environment_response(t)
            }
            (3, CommitProgress::BeforeAttempt) if !changed => self.no_write(t, true, context),
            (4, CommitProgress::BeforeAttempt) if !changed => self.no_write(t, false, context),
            (0..=2, _) if !changed => Ok(Flow::Exclude(Exclusion::CommitNoWrite)),
            (3 | 4, _) if changed => Ok(Flow::Exclude(Exclusion::CommitNoWrite)),
            _ => Ok(Flow::Exclude(Exclusion::CommitProgress)),
        }
    }

    /// Existing outstanding obligation drained by a publication fact releasing its owners.
    fn settle_drain(&mut self, t: u16) -> Step<()> {
        let fact = self.fact.ok_or(INVALID)?;
        if let Some(o) = self.edit.transaction(t)?.obligation {
            let record = self.edit.obligation_mut(o.0)?;
            if record.settlement == Settlement::Outstanding {
                record.settlement = Settlement::DrainedConfirmed;
                record.recovery_fact = Some(crate::lifecycle::schema::FactRef(fact.nonce));
            }
        }
        Ok(())
    }

    /// Response of an environment fact that keeps its context parked.
    fn environment_response(&mut self, t: u16) -> Step<Flow> {
        self.edge.class = EdgeClass::NoContextIssue;
        let mut response = self.success(Phase::Commit, StatementEffect::NoEffect);
        response.certainty = Certainty::NotApplicable;
        self.respond(response, Some(t))
    }

    pub(super) fn finish(&mut self) -> Step<Flow> {
        let (context, payload) = match self.pending(Api::BeginCommit)? {
            Ok(found) => found,
            Err(code) => return self.control_plane(code),
        };
        self.context = Some(context);
        let t = payload.transaction.ok_or(INVALID)?.0;
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let record = self.edit.transaction(t)?;
        if let Some(rule) = self.envelope_transaction(Some(record.id)) {
            return Ok(Flow::Exclude(rule));
        }
        let caller = self.caller()?;
        let admission = self.edit.engine(caller.engine.0)?.admission;
        let observed_cancel = payload.cancel_supplied_or_signalled;
        let observed_deadline = !observed_cancel && payload.deadline_reached;
        match (self.input.outcome, record.progress) {
            (0, CommitProgress::Published | CommitProgress::NoWriteValidated) => {
                let parts = if record.receipt.is_some() { 4 | 8 } else { 8 };
                if let Some(r) = record.receipt {
                    let receipt = self.edit.receipt_mut(r.0)?;
                    receipt.delivery = Delivery::Delivered;
                    receipt.delivery_context = Some(ContextRef(context.nonce));
                }
                let row = self.edit.transaction_mut(t)?;
                row.client_knowledge = ClientKnowledge::KnownCommitted;
                if row.receipt.is_some() {
                    row.delivery = Delivery::Delivered;
                }
                for actor in self.edit.state_mut().actors.as_mut_slice() {
                    if actor.transaction.is_some_and(|x| x.0 == t) {
                        actor.transaction = None;
                    }
                }
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    parts,
                    Some(t),
                    None,
                    record.receipt.is_some(),
                )?;
                self.complete_parked(context)?;
                let response = self.success(Phase::Commit, StatementEffect::NoEffect);
                self.respond(response, Some(t))
            }
            (1, CommitProgress::BeforeAttempt) if admission != EngineAdmission::Admitting => {
                Ok(Flow::Exclude(Exclusion::PeerFence))
            }
            (1, CommitProgress::BeforeAttempt) => {
                self.edge.controls.observed_cancel = observed_cancel;
                self.edge.controls.observed_deadline = observed_deadline;
                self.abort_transaction(t)?;
                self.consume(
                    Cleanup::Complete,
                    NextWork::NotConsumed,
                    2,
                    Some(t),
                    None,
                    false,
                )?;
                self.complete_parked(context)?;
                let (code, origin) = if observed_cancel {
                    (ReturnCode::Cancelled, OriginCause::Cancelled)
                } else if observed_deadline {
                    (ReturnCode::Timeout, OriginCause::Timeout)
                } else {
                    (ReturnCode::Conflict, OriginCause::Conflict)
                };
                let response = self.error(
                    code,
                    Phase::Commit,
                    StatementEffect::StatementRestored,
                    Retry::WholeTransactionDecision,
                    origin,
                );
                self.respond(response, Some(t))
            }
            (2, CommitProgress::BeforeAttempt) => {
                self.edge.controls.observed_cancel = observed_cancel;
                self.edge.controls.observed_deadline = observed_deadline;
                if admission == EngineAdmission::Fenced {
                    // Peer-fenced handoff keeps the existing obligation's decision.
                    let row = self.edit.transaction_mut(t)?;
                    row.life = TransactionLife::RecoveryPending;
                    row.decision = Decision::NotCommittedRecoveryRequired;
                    row.client_knowledge = ClientKnowledge::KnownNotCommittedRecoveryRequired;
                    self.consume(
                        Cleanup::LocalReleaseAndHandoff,
                        NextWork::NotConsumed,
                        2,
                        Some(t),
                        None,
                        false,
                    )?;
                    self.complete_parked(context)?;
                    let response = self.admitted_error(
                        ReturnCode::RecoveryRequired,
                        Phase::Commit,
                        StatementEffect::RecoveryRequired,
                        Retry::AfterRecovery,
                        EngineCause::PeerFence,
                        OriginCause::PeerFence,
                    );
                    self.respond(response, Some(t))
                } else {
                    self.recovery_required(t, Phase::Commit)
                }
            }
            (3, CommitProgress::Attempted) => self.unknown(t, Phase::Commit),
            (4, CommitProgress::DurableConfirmed) => {
                let context_id = context;
                self.publication_pending(t, context_id)
            }
            _ => Ok(Flow::Exclude(Exclusion::CommitProgress)),
        }
    }

    pub(super) fn rollback(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let caller = self.caller()?;
        let Some(t) = caller.transaction.map(|t| t.0) else {
            return self.reject(ReturnCode::TxnState);
        };
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let record = self.edit.transaction(t)?;
        if let Some(rule) = self.envelope_transaction(Some(record.id)) {
            return Ok(Flow::Exclude(rule));
        }
        if self.input.outcome == 1 {
            if record.access != Access::ReadWrite {
                return Ok(Flow::Exclude(Exclusion::ReadOnlyCleanup));
            }
            return self.recovery_required(t, Phase::Rollback);
        }
        self.abort_open_results(t)?;
        self.abort_transaction(t)?;
        self.consume(
            Cleanup::Complete,
            NextWork::NotConsumed,
            2,
            Some(t),
            None,
            false,
        )?;
        self.complete_fresh()?;
        let response = self.success(Phase::Rollback, StatementEffect::StatementRestored);
        self.respond(response, Some(t))
    }

    /// Whole abort retires the transaction's Open results and returns their statements.
    fn abort_open_results(&mut self, t: u16) -> Step<()> {
        let mut open = [0u16; 4];
        let mut count = 0;
        for d in self.edit.state().private_descriptors.as_slice() {
            if d.transaction.is_some_and(|x| x.0 == t) && d.state == ResultState::Open {
                open[count] = d.result.0;
                count += 1;
            }
        }
        for &result in &open[..count] {
            self.finish_cursor(result, ResultState::Closed, None)?;
            self.retire_result(result, Retirement::Rollback, true)?;
        }
        Ok(())
    }

    pub(super) fn close_session(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        let caller = self.caller()?;
        if caller.shell == Shell::Closed {
            self.complete_fresh()?;
            let response = self.success(Phase::Close, StatementEffect::NoEffect);
            return self.respond(response, None);
        }
        if caller.parked_context.is_some() {
            return self.reject(ReturnCode::Busy);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let transaction = caller.transaction.map(|t| t.0);
        let expected = match transaction {
            Some(t) => Some(self.transaction_identity(t)?),
            None => None,
        };
        if let Some(rule) = self.envelope_transaction(expected) {
            return Ok(Flow::Exclude(rule));
        }
        let admission = self.edit.engine(caller.engine.0)?.admission;
        if self.input.outcome == 1 {
            if admission != EngineAdmission::Admitting {
                return Ok(Flow::Exclude(Exclusion::FencedClose));
            }
            let read_write = match transaction {
                Some(t) => self.edit.transaction(t)?.access == Access::ReadWrite,
                None => false,
            };
            if !read_write {
                return Ok(Flow::Exclude(Exclusion::ReadOnlyCleanup));
            }
            let t = transaction.ok_or(INVALID)?;
            self.fence(
                Some(t),
                EngineCause::UndoIncomplete,
                Decision::NotCommittedRecoveryRequired,
            )?;
            self.consume(
                Cleanup::Incomplete,
                NextWork::NotConsumed,
                2,
                Some(t),
                Some(ObligationReason::KnownNotCommittedUndoPending),
                false,
            )?;
            self.complete_fresh()?;
            let response = self.admitted_error(
                ReturnCode::RecoveryRequired,
                Phase::Close,
                StatementEffect::RecoveryRequired,
                Retry::AfterRecovery,
                EngineCause::UndoIncomplete,
                OriginCause::UndoIncomplete,
            );
            return self.respond(response, Some(t));
        }
        let admitting = admission == EngineAdmission::Admitting;
        if admitting && let Some(t) = transaction {
            self.abort_transaction(t)?;
        }
        let cleanup = if admitting {
            Cleanup::Complete
        } else {
            Cleanup::LocalReleaseAndHandoff
        };
        self.consume(cleanup, NextWork::NotConsumed, 2, transaction, None, false)?;
        self.close_children(caller.actor)?;
        let context = self.context.ok_or(INVALID)?;
        let session = caller.session.0;
        let row = self
            .edit
            .state_mut()
            .session_incarnations
            .as_mut_slice()
            .iter_mut()
            .find(|s| s.id.nonce == session)
            .ok_or(INVALID)?;
        row.lifecycle = Shell::Closed;
        row.closing_context = Some(ContextRef(context.nonce));
        let actor = self.edit.actor_mut(caller.actor)?;
        actor.shell = Shell::Closed;
        actor.transaction = None;
        actor.active_public_result = None;
        self.complete_fresh()?;
        let response = self.success(Phase::Close, StatementEffect::NoEffect);
        self.respond(response, transaction)
    }

    /// CloseSession retires every current statement/result of the actor and clears its slots.
    fn close_children(&mut self, actor: u8) -> Step<()> {
        for slot_index in 0..2 {
            let slot = self.edit.slot(actor, slot_index)?;
            if let Some(r) = slot.descriptor_result {
                if let Ok(d) = self.descriptor_of(r.0)
                    && d.state == ResultState::Open
                    && let Some(t) = d.transaction
                    && self.edit.transaction(t.0)?.origin == TransactionOrigin::ImplicitReadOnly
                {
                    self.end_transaction(t.0, None)?;
                    self.settle_completion(t.0)?;
                }
                self.retire_result(r.0, Retirement::SessionClose, true)?;
            }
            if let Some(s) = slot.statement {
                let context = self.context.ok_or(INVALID)?;
                let record = self.edit.statement_mut(s.0)?;
                if record.life != StatementLife::Retired {
                    record.life = StatementLife::Retired;
                    record.retirement = Retirement::SessionClose;
                    record.retirement_context = Some(ContextRef(context.nonce));
                    self.edit
                        .slot_mut(actor, slot_index)?
                        .next_statement_generation += 1;
                }
            }
            if let Some(b) = slot.binding {
                self.edit.binding_mut(b.0)?.life = BindingLife::Released;
            }
            let s = self.edit.slot_mut(actor, slot_index)?;
            s.statement = None;
            s.binding = None;
            s.descriptor_result = None;
            s.state = SlotState::Vacant;
        }
        Ok(())
    }

    pub(super) fn reopen(&mut self) -> Step<Flow> {
        let caller = self.caller()?;
        if caller.shell == Shell::Open {
            return self.no_context_reject(ReturnCode::SessionState);
        }
        if caller.parked_context.is_some() {
            return self.no_context_reject(ReturnCode::Busy);
        }
        let current = self.edit.state().current_engine.0;
        let attached = self.edit.engine(caller.engine.0)?;
        let target = match self.input.outcome {
            0 if caller.engine.0 == current && attached.admission == EngineAdmission::Admitting => {
                current
            }
            1 if attached.admission == EngineAdmission::RecoveryConfirmedRetired
                && attached.successor_engine.is_some_and(|e| e.0 == current) =>
            {
                current
            }
            _ => return self.no_context_reject(ReturnCode::EngineFenced),
        };
        let nonce = self.edit.next(IdentityKind::Session);
        if nonce > 8 {
            return Err(super::CAPACITY);
        }
        let id = crate::lifecycle::schema::Identity {
            kind: IdentityKind::Session,
            actor: caller.actor,
            slot: 255,
            engine: target,
            session: nonce,
            nonce,
            generation: 0,
        };
        self.edit.state_mut().next_nonce[IdentityKind::Session as usize - 1] += 1;
        self.edit.add_session(SessionRecord {
            id,
            engine: EngineRef(target),
            actor: caller.actor,
            lifecycle: Shell::Open,
            reopen_kind: if self.input.outcome == 0 {
                ReopenKind::HealthySameEngine
            } else {
                ReopenKind::RecoveredFreshEngine
            },
            previous_session: Some(SessionRef(caller.session.0)),
            closing_context: None,
        })?;
        let actor = self.edit.actor_mut(caller.actor)?;
        actor.shell = Shell::Open;
        actor.autocommit = Autocommit::On;
        actor.session = SessionRef(nonce);
        actor.engine = EngineRef(target);
        actor.transaction = None;
        actor.parked_context = None;
        actor.active_public_result = None;
        for slot_index in 0..2 {
            *self.edit.slot_mut(caller.actor, slot_index)? = PreparedSlot {
                actor: caller.actor,
                slot: slot_index,
                state: SlotState::Vacant,
                next_statement_generation: 1,
                next_result_generation: 1,
                statement: None,
                binding: None,
                descriptor_result: None,
            };
        }
        self.edge.class = EdgeClass::NoContextIssue;
        let response = self.success(Phase::NotApplicable, StatementEffect::NoEffect);
        self.respond(response, None)
    }

    pub(super) fn inspect_session(&mut self) -> Step<Flow> {
        self.edge.class = EdgeClass::NoEffect;
        let response = self.success(Phase::NotApplicable, StatementEffect::NoEffect);
        self.respond(response, None)
    }

    pub(super) fn signal(&mut self) -> Step<Flow> {
        let caller = self.caller()?;
        let target = self.input.target.and_then(unpack);
        let record = target.and_then(|id| {
            self.edit
                .state()
                .contexts
                .as_slice()
                .iter()
                .find(|c| {
                    c.id == id && id.kind == IdentityKind::Context && id.actor == caller.actor
                })
                .copied()
        });
        let Some(record) = record else {
            return self.signal_response(ReturnCode::InvalidContext, EdgeClass::NoEffect);
        };
        let deadline = self.input.outcome == 1;
        match record.data {
            ContextData::Completed => {
                self.signal_response(ReturnCode::IgnoredCompleted, EdgeClass::NoEffect)
            }
            ContextData::ParkedFetch(mut p) | ContextData::ParkedCommit(mut p) => {
                let before = p;
                if deadline {
                    if p.original_deadline == Deadline::Absent {
                        return Ok(Flow::Exclude(Exclusion::SignalNoDeadline));
                    }
                    p.deadline_reached = true;
                } else {
                    p.cancel_supplied_or_signalled = true;
                }
                if p == before {
                    return self.signal_response(ReturnCode::Ok, EdgeClass::NoEffect);
                }
                let row = self.edit.context_mut(record.id.nonce)?;
                row.data = match row.data {
                    ContextData::ParkedFetch(_) => ContextData::ParkedFetch(p),
                    _ => ContextData::ParkedCommit(p),
                };
                self.signal_response(ReturnCode::Ok, EdgeClass::NoContextIssue)
            }
        }
    }

    fn signal_response(&mut self, code: ReturnCode, class: EdgeClass) -> Step<Flow> {
        self.edge.class = class;
        let mut response = self.success(Phase::NotApplicable, StatementEffect::NoEffect);
        response.code = code;
        response.certainty = Certainty::NotApplicable;
        if code == ReturnCode::InvalidContext {
            response = self.error(
                code,
                Phase::NotApplicable,
                StatementEffect::NoEffect,
                Retry::Never,
                OriginCause::None,
            );
            response.certainty = Certainty::NotApplicable;
            if let Some(d) = response.diagnostic.as_mut() {
                d.certainty = Certainty::NotApplicable;
            }
        }
        self.respond(response, None)
    }

    /// Supplied owners are exactly the live RW/Recovery owners of the confirmed obligations,
    /// each released, in canonical order.
    fn recovery_owners_exact(
        &self,
        env: &crate::lifecycle::input_schema::Environment,
        obligations: &[crate::lifecycle::schema::Identity],
    ) -> Step<bool> {
        let state = self.edit.state();
        let mut expected = [crate::lifecycle::schema::Identity::default(); 54];
        let mut count = 0;
        for o in obligations {
            let record = self.edit.obligation(o.nonce)?;
            for owner in state.owners.as_slice() {
                let rw = matches!(
                    owner.resource_kind,
                    ResourceKind::Undo
                        | ResourceKind::Locks
                        | ResourceKind::AbortReserve
                        | ResourceKind::Recovery
                );
                if rw
                    && owner.original_holder.kind == IdentityKind::Transaction
                    && owner.original_holder.nonce == record.transaction.0
                    && owner.disposition != crate::lifecycle::schema::OwnerDisposition::Released
                {
                    if count == expected.len() {
                        return Err(INVALID);
                    }
                    expected[count] = owner.id;
                    count += 1;
                }
            }
        }
        let expected = &mut expected[..count];
        expected.sort_unstable_by_key(|id| id.key());
        let supplied = env.owners.as_slice();
        Ok(supplied.len() == expected.len()
            && supplied.iter().zip(expected.iter()).all(|(row, id)| {
                unpack(row.resource) == Some(*id)
                    && row.after_disposition == crate::lifecycle::schema::OwnerDisposition::Released
            }))
    }

    pub(super) fn recovery(&mut self) -> Step<Flow> {
        let old = self.edit.state().current_engine.0;
        let engine = self.edit.engine(old)?;
        let state = self.edit.state();
        let pre_ok = engine.admission == EngineAdmission::Fenced
            && state
                .actors
                .as_slice()
                .iter()
                .all(|a| a.shell == Shell::Closed && a.parked_context.is_none());
        if !pre_ok {
            return Ok(Flow::Exclude(Exclusion::RecoveryPre));
        }
        let env = match self.require_environment() {
            Ok(env) => *env,
            Err(rule) => return Ok(Flow::Exclude(rule)),
        };
        // The supplied set must be exactly the outstanding obligations of the old engine.
        let mut expected = [crate::lifecycle::schema::Identity::default(); 6];
        let mut count = 0;
        for o in state.obligations.as_slice() {
            if o.engine.0 == old && o.settlement == Settlement::Outstanding {
                expected[count] = o.id;
                count += 1;
            }
        }
        let expected = &mut expected[..count];
        expected.sort_unstable_by_key(|id| id.key());
        let supplied = env.obligations.as_slice();
        let exact = self.input.outcome == 0
            && supplied.len() == expected.len()
            && supplied
                .iter()
                .zip(expected.iter())
                .all(|(row, id)| unpack(row.obligation) == Some(*id));
        if !exact || !self.recovery_owners_exact(&env, expected)? {
            return Ok(Flow::Exclude(Exclusion::RecoverySet));
        }
        let fresh = env.fresh_engine.and_then(unpack).ok_or(super::ENVELOPE)?;
        if fresh.nonce != self.edit.next(IdentityKind::Engine) || fresh.nonce > 7 {
            return Err(super::ENVELOPE);
        }
        self.consume(
            Cleanup::RecoveryConfirmed,
            NextWork::NotConsumed,
            2 | 16,
            None,
            None,
            false,
        )?;
        let fact = self.fact.ok_or(INVALID)?;
        let fact_ref = crate::lifecycle::schema::FactRef(fact.nonce);
        self.edit.add_engine(EngineRecord {
            id: fresh,
            admission: EngineAdmission::Admitting,
            first_cause: EngineCause::None,
            known_decision: Decision::NotApplicable,
            cause_transaction: None,
            first_fence_context: None,
            successor_engine: None,
            recovery_fact: None,
        })?;
        let row = self.edit.engine_mut(old)?;
        row.admission = EngineAdmission::RecoveryConfirmedRetired;
        row.successor_engine = Some(EngineRef(fresh.nonce));
        row.recovery_fact = Some(fact_ref);
        self.edit.state_mut().current_engine = EngineRef(fresh.nonce);
        for id in expected.iter() {
            let record = self.edit.obligation_mut(id.nonce)?;
            record.settlement = Settlement::RecoveryConfirmed;
            record.recovery_fact = Some(fact_ref);
            let t = record.transaction.0;
            let row = self.edit.transaction_mut(t)?;
            // The recovery fact names no transaction: it settles the obligation, not the
            // transaction's completion fact (owner decision 2026-10-25).
            row.life = TransactionLife::Ended;
        }
        self.edge.class = EdgeClass::NoContextIssue;
        // An external completion fact like CommitFact: no statement certainty (owner decision
        // 2026-10-25).
        let mut response = self.success(Phase::Recovery, StatementEffect::NoEffect);
        response.certainty = Certainty::NotApplicable;
        self.respond(response, None)
    }
}
