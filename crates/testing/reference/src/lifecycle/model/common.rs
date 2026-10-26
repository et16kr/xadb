//! Shared transition steps: target authentication, facts, fences and transaction endings.
use super::edit::Fact;
use super::{
    Complete, ContextEvent, ENVELOPE, EdgeClass, Exclusion, Flow, INVALID, Response, Rows, Run,
    Step, unpack,
};
use crate::lifecycle::input_schema::Environment;
use crate::lifecycle::schema::{
    Access, ActorRecord, Api, Certainty, Cleanup, ClientKnowledge, ContextData, ContextRef,
    ContextStatus, Decision, Delivery, Descriptor, Diagnostic, EngineAdmission, EngineCause,
    FactKind, Identity, IdentityKind, NextWork, ObligationReason, OriginCause, ParkedContext,
    Phase, ReceiptKind, ReceiptRecord, ResourceKind, ResultRecord, ResultState, Retirement, Retry,
    ReturnCode, SlotState, StatementEffect, StatementLife, StatementRecord, TransactionLife,
    TransactionRef,
};

impl Run<'_> {
    /// The caller's scope as an identity template.
    pub(super) fn scope(caller: &ActorRecord) -> Identity {
        Identity {
            actor: caller.actor,
            engine: caller.engine.0,
            session: caller.session.0,
            ..Identity::default()
        }
    }

    /// Same-caller exact current statement (Current or CurrentClosed).
    pub(super) fn authenticate_statement(&self) -> Step<Option<StatementRecord>> {
        let caller = self.caller()?;
        let Some(id) = self.input.target.and_then(unpack) else {
            return Ok(None);
        };
        if id.kind != IdentityKind::Statement
            || id.actor != caller.actor
            || id.engine != caller.engine.0
            || id.session != caller.session.0
        {
            return Ok(None);
        }
        let found = self
            .edit
            .state()
            .issued_statements
            .as_slice()
            .iter()
            .find(|s| s.id == id)
            .copied();
        Ok(found.filter(|s| s.life != StatementLife::Retired))
    }

    /// Same-caller exact public current result.
    pub(super) fn authenticate_result(&self) -> Step<Option<ResultRecord>> {
        let caller = self.caller()?;
        let Some(id) = self.input.target.and_then(unpack) else {
            return Ok(None);
        };
        if id.kind != IdentityKind::Result
            || id.actor != caller.actor
            || id.engine != caller.engine.0
            || id.session != caller.session.0
        {
            return Ok(None);
        }
        let found = self
            .edit
            .state()
            .issued_results
            .as_slice()
            .iter()
            .find(|r| r.id == id)
            .copied();
        Ok(found.filter(|r| r.public_generation_live))
    }

    /// The supplied environment of an admitted call; a missing or foreign-kind envelope is
    /// an exclusion.
    pub(super) fn require_environment(&self) -> Result<&Environment, Exclusion> {
        let env = self.input.environment.as_ref().ok_or(Exclusion::Envelope)?;
        let expected = match self.input.api {
            Api::Execute => FactKind::Execution,
            Api::Fetch | Api::ResumeFetch => FactKind::FetchWorkOrCleanup,
            Api::CloseResult | Api::CloseStatement => FactKind::LocalCleanup,
            Api::CommitAtomic => FactKind::CommitAtomic,
            Api::CommitFact => FactKind::CommitProgress,
            Api::FinishCommit => FactKind::CommitFinish,
            Api::Rollback => FactKind::RollbackCleanup,
            Api::CloseSession => FactKind::SessionCleanup,
            Api::RecoveryConfirmedFreshEngine => FactKind::RecoveryConfirmed,
            _ => return Err(Exclusion::Envelope),
        };
        if env.kind == expected {
            Ok(env)
        } else {
            Err(Exclusion::Envelope)
        }
    }

    /// The supplied transaction identity must be exactly the selected one.
    pub(super) fn envelope_transaction(&self, expected: Option<Identity>) -> Option<Exclusion> {
        let supplied = self
            .input
            .environment
            .as_ref()
            .and_then(|e| e.transaction)
            .and_then(unpack);
        (supplied != expected).then_some(Exclusion::Envelope)
    }

    /// Record the consumed envelope as a fact.
    // Each argument is one independent fact field supplied by the calling family.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn consume(
        &mut self,
        cleanup: Cleanup,
        next_work: NextWork,
        consumed: u8,
        transaction: Option<u16>,
        reason: Option<ObligationReason>,
        receipt: bool,
    ) -> Step<Identity> {
        let env = *self.input.environment.as_ref().ok_or(INVALID)?;
        let (decision, delivery) = match transaction {
            Some(t) => {
                let r = self.edit.transaction(t)?;
                (r.decision, r.delivery)
            }
            None => (Decision::NotApplicable, Delivery::NotApplicable),
        };
        let fact = Fact {
            kind: env.kind,
            api: self.input.api,
            outcome: self.input.outcome,
            cleanup,
            next_work,
            consumed,
            context: self.context,
            transaction,
            receipt,
            fresh_engine: env.fresh_engine.is_some()
                && self.input.api == Api::RecoveryConfirmedFreshEngine,
            anchor: receipt && env.anchor.is_some(),
            reason,
        };
        let new_transaction =
            transaction.map(|t| self.edit.transaction(t).map(|r| r.id).unwrap_or_default());
        let new_result = self.new_result;
        let holder = move |kind: ResourceKind| match kind {
            ResourceKind::Snapshot | ResourceKind::Scratch | ResourceKind::Temp => new_result,
            ResourceKind::Plan | ResourceKind::Binding => None,
            _ => new_transaction,
        };
        let id = self
            .edit
            .record_fact(&env, &fact, decision, delivery, &holder)?;
        self.fact = Some(id);
        self.edge.consumed_parts = consumed;
        // A fenced CloseSession handoff confirms neither the end nor the incompletion of the
        // transaction (owner decision 2026-10-25), so it is not its completion fact; the fenced
        // Finish NCR handoff confirms NCR and is.
        let handoff_only =
            self.input.api == Api::CloseSession && cleanup == Cleanup::LocalReleaseAndHandoff;
        if let Some(t) = transaction.filter(|_| !handoff_only) {
            let id_ref = crate::lifecycle::schema::FactRef(id.nonce);
            let record = self.edit.transaction_mut(t)?;
            if record.life != TransactionLife::Active && record.completion_fact.is_none() {
                record.completion_fact = Some(id_ref);
            }
        }
        Ok(id)
    }

    /// Mark the transaction's completion fact when it ends or enters recovery in this call.
    pub(super) fn settle_completion(&mut self, transaction: u16) -> Step<()> {
        if let Some(fact) = self.fact {
            let record = self.edit.transaction_mut(transaction)?;
            if record.life != TransactionLife::Active {
                record.completion_fact = Some(crate::lifecycle::schema::FactRef(fact.nonce));
            }
        }
        Ok(())
    }

    /// Install the receipt carried by the consumed envelope.
    pub(super) fn install_receipt(
        &mut self,
        transaction: u16,
        kind: ReceiptKind,
        delivery: Delivery,
    ) -> Step<Option<u16>> {
        let env = *self.input.environment.as_ref().ok_or(INVALID)?;
        let Some(id) = env.receipt.and_then(unpack) else {
            return Ok(None);
        };
        let anchor = env.anchor.and_then(unpack);
        self.edit.add_receipt(ReceiptRecord {
            id,
            transaction: TransactionRef(transaction),
            engine: crate::lifecycle::schema::EngineRef(id.engine),
            kind,
            delivery,
            anchor: if kind == ReceiptKind::DurableWrite {
                anchor
            } else {
                None
            },
            source_fact: None,
            delivery_context: None,
        })?;
        let record = self.edit.transaction_mut(transaction)?;
        record.receipt = Some(crate::lifecycle::schema::ReceiptRef(id.nonce));
        Ok(Some(id.nonce))
    }

    /// End a transaction, clearing the actor link when it is the caller's current one.
    pub(super) fn end_transaction(
        &mut self,
        transaction: u16,
        decision: Option<Decision>,
    ) -> Step<()> {
        let record = self.edit.transaction_mut(transaction)?;
        record.life = TransactionLife::Ended;
        if let Some(decision) = decision {
            record.decision = decision;
            record.client_knowledge = match decision {
                Decision::Aborted => ClientKnowledge::KnownAborted,
                Decision::Committed => ClientKnowledge::KnownCommitted,
                Decision::NotCommittedRecoveryRequired => {
                    ClientKnowledge::KnownNotCommittedRecoveryRequired
                }
                Decision::Unknown => ClientKnowledge::Unknown,
                _ => record.client_knowledge,
            };
        }
        for actor in self.edit.state_mut().actors.as_mut_slice() {
            if actor.transaction.is_some_and(|t| t.0 == transaction) {
                actor.transaction = None;
            }
        }
        Ok(())
    }

    /// Recoverable terminal error of the caller's own implicit transaction ends it.
    pub(super) fn end_if_implicit(&mut self, transaction: u16) -> Step<()> {
        let record = self.edit.transaction(transaction)?;
        match record.origin {
            crate::lifecycle::schema::TransactionOrigin::ImplicitReadOnly => {
                self.end_transaction(transaction, None)
            }
            crate::lifecycle::schema::TransactionOrigin::ImplicitReadWrite => {
                self.end_transaction(transaction, Some(Decision::Aborted))
            }
            _ => Ok(()),
        }
    }

    /// Whole abort of a transaction: RW Ended Aborted, read-only simply ended.
    pub(super) fn abort_transaction(&mut self, transaction: u16) -> Step<()> {
        let record = self.edit.transaction(transaction)?;
        if record.access == Access::ReadWrite {
            self.end_transaction(transaction, Some(Decision::Aborted))
        } else {
            self.end_transaction(transaction, None)
        }
    }

    /// First and repeated fences: the engine fence and the transaction's recovery state.
    pub(super) fn fence(
        &mut self,
        transaction: Option<u16>,
        cause: EngineCause,
        decision: Decision,
    ) -> Step<()> {
        let caller = self.caller()?;
        let engine = caller.engine.0;
        let context = self.context.ok_or(INVALID)?;
        if let Some(t) = transaction {
            let record = self.edit.transaction_mut(t)?;
            record.life = TransactionLife::RecoveryPending;
            record.decision = decision;
            record.client_knowledge = match decision {
                Decision::NotCommittedRecoveryRequired => {
                    ClientKnowledge::KnownNotCommittedRecoveryRequired
                }
                Decision::Unknown | Decision::Committed => ClientKnowledge::Unknown,
                _ => record.client_knowledge,
            };
        }
        let record = self.edit.engine_mut(engine)?;
        if record.admission != EngineAdmission::Admitting {
            return Ok(());
        }
        record.admission = EngineAdmission::Fenced;
        record.first_cause = cause;
        record.known_decision = decision;
        record.cause_transaction = transaction.map(TransactionRef);
        record.first_fence_context = Some(ContextRef(context.nonce));
        // The first fence retires every current public result of the engine once.
        let mut retired = [0u16; 6];
        let mut count = 0;
        for result in self.edit.state().issued_results.as_slice() {
            if result.id.engine == engine && result.public_generation_live {
                retired[count] = result.id.nonce;
                count += 1;
            }
        }
        for &nonce in &retired[..count] {
            self.retire_result(nonce, Retirement::EngineFence, false)?;
            let d = self.edit.descriptor_mut(nonce)?;
            d.blocked_by_fence = true;
            // The retired public result no longer holds its slot open (owner decision
            // 2026-10-25); the descriptor keeps its owner and position behind the fence.
            let id = self.edit.result_mut(nonce)?.id;
            let slot = self.edit.slot_mut(id.actor, id.slot)?;
            if slot.state == SlotState::ResultOpen {
                slot.state = SlotState::ReadyBound;
            }
        }
        Ok(())
    }

    /// Retire a current public result: token retired, slot generation advanced.
    pub(super) fn retire_result(
        &mut self,
        result: u16,
        reason: Retirement,
        drop_descriptor: bool,
    ) -> Step<()> {
        let context = self.context.ok_or(INVALID)?;
        let record = self.edit.result_mut(result)?;
        let live = record.public_generation_live;
        let (actor, slot) = (record.id.actor, record.id.slot);
        if live {
            record.public_generation_live = false;
            record.retirement = reason;
            record.retirement_context = Some(ContextRef(context.nonce));
            self.edit.slot_mut(actor, slot)?.next_result_generation += 1;
        }
        if let Ok(d) = self.edit.descriptor_mut(result) {
            d.public_generation_live = false;
        }
        for a in self.edit.state_mut().actors.as_mut_slice() {
            if a.active_public_result.is_some_and(|r| r.0 == result) {
                a.active_public_result = None;
            }
        }
        if drop_descriptor {
            self.edit.remove_descriptor(result)?;
            let s = self.edit.slot_mut(actor, slot)?;
            if s.descriptor_result.is_some_and(|r| r.0 == result) {
                s.descriptor_result = None;
            }
        }
        Ok(())
    }

    /// Close a cursor's public life after a terminal fetch/cleanup outcome.
    pub(super) fn finish_cursor(
        &mut self,
        result: u16,
        state: ResultState,
        terminal: Option<Diagnostic>,
    ) -> Step<()> {
        let d = self.edit.descriptor_mut(result)?;
        let was_open = d.state == ResultState::Open;
        d.state = state;
        // A terminal diagnostic exists exactly for Error descriptors.
        d.terminal = if state == ResultState::Error {
            terminal.or(d.terminal)
        } else {
            None
        };
        let statement = d.statement.0;
        if was_open {
            let st = self.edit.statement(statement)?;
            let slot = self.edit.slot_mut(st.id.actor, st.id.slot)?;
            if slot.state == SlotState::ResultOpen {
                slot.state = SlotState::ReadyBound;
            }
            for a in self.edit.state_mut().actors.as_mut_slice() {
                if a.active_public_result.is_some_and(|r| r.0 == result) {
                    a.active_public_result = None;
                }
            }
        }
        Ok(())
    }

    /// Fetch-family response shape.
    pub(super) fn fetch_response(mut response: Response, rows: u8, complete: bool) -> Response {
        response.rows = Rows::Count(rows);
        response.complete = if complete {
            Complete::Yes
        } else {
            Complete::No
        };
        response
    }

    /// Error response for an admitted call, with its own engine cause.
    pub(super) fn admitted_error(
        &self,
        code: ReturnCode,
        phase: Phase,
        effect: StatementEffect,
        retry: Retry,
        engine: EngineCause,
        origin: OriginCause,
    ) -> Response {
        let mut response = self.error(code, phase, effect, retry, origin);
        response.engine_cause = engine;
        if let Some(d) = response.diagnostic.as_mut() {
            d.engine_cause = engine;
        }
        if matches!(code, ReturnCode::ResultUnknown) {
            response.certainty = Certainty::Unknown;
            if let Some(d) = response.diagnostic.as_mut() {
                d.certainty = Certainty::Unknown;
            }
        }
        response
    }

    /// Complete the matched parked context of a continuation.
    pub(super) fn complete_parked(&mut self, context: Identity) -> Step<()> {
        let before = match self.edit.context(context.nonce)?.data {
            ContextData::ParkedFetch(_) => ContextStatus::ParkedFetch,
            ContextData::ParkedCommit(_) => ContextStatus::ParkedCommit,
            ContextData::Completed => return Err(INVALID),
        };
        self.edit.context_mut(context.nonce)?.data = ContextData::Completed;
        for a in self.edit.state_mut().actors.as_mut_slice() {
            if a.parked_context.is_some_and(|c| c.0 == context.nonce) {
                a.parked_context = None;
            }
        }
        for t in self.edit.state_mut().transactions.as_mut_slice() {
            if t.parked_context.is_some_and(|c| c.0 == context.nonce) {
                t.parked_context = None;
            }
        }
        self.edge.context_event = Some(ContextEvent {
            identity: context,
            before: Some(before),
            after: ContextStatus::Completed,
        });
        self.edge.class = EdgeClass::Continuation;
        Ok(())
    }

    /// The matched pending context of the requested kind, or the control-plane code.
    pub(super) fn pending(&self, kind: Api) -> Step<Result<(Identity, ParkedContext), ReturnCode>> {
        let caller = self.caller()?;
        let Some(reference) = caller.parked_context else {
            return Ok(Err(ReturnCode::NoPendingOperation));
        };
        let record = self.edit.context(reference.0)?;
        match (record.data, kind) {
            (ContextData::ParkedFetch(p), Api::ParkFetch)
            | (ContextData::ParkedCommit(p), Api::BeginCommit) => {
                // The supplied continuation identity is part of the envelope (owner decision
                // 2026-10-25): another identity is `Excluded(Envelope)`, not a continuation.
                if self.input.context.and_then(unpack) != Some(record.id) {
                    return Err(ENVELOPE);
                }
                Ok(Ok((record.id, p)))
            }
            (ContextData::Completed, _) => Err(INVALID),
            _ => Ok(Err(ReturnCode::ContinuationKind)),
        }
    }

    /// Control-plane response without context, state unchanged.
    pub(super) fn control_plane(&mut self, code: ReturnCode) -> Step<Flow> {
        self.edge.class = EdgeClass::NoEffect;
        let mut response = self.error(
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
        self.respond(response, None)
    }

    /// Park a fresh call with its original control and targets.
    pub(super) fn park(&mut self, payload: ParkedContext, fetch: bool) -> Step<()> {
        let context = self.context.ok_or(INVALID)?;
        let data = if fetch {
            ContextData::ParkedFetch(payload)
        } else {
            ContextData::ParkedCommit(payload)
        };
        self.edit.add_context(context, Some(data))?;
        let actor = self.input.actor;
        self.edit.actor_mut(actor)?.parked_context = Some(ContextRef(context.nonce));
        if let Some(t) = payload.transaction {
            self.edit.transaction_mut(t.0)?.parked_context = Some(ContextRef(context.nonce));
        }
        self.edge.context_event = Some(ContextEvent {
            identity: context,
            before: None,
            after: if fetch {
                ContextStatus::ParkedFetch
            } else {
                ContextStatus::ParkedCommit
            },
        });
        Ok(())
    }

    /// Terminal diagnostic stored in an Error descriptor.
    pub(super) fn terminal(code: ReturnCode, phase: Phase, origin: OriginCause) -> Diagnostic {
        Diagnostic {
            code,
            phase,
            effect: StatementEffect::CursorReleased,
            certainty: Certainty::Known,
            retry: Retry::Operation,
            engine_cause: EngineCause::None,
            origin_cause: origin,
        }
    }

    pub(super) fn descriptor_of(&self, result: u16) -> Step<Descriptor> {
        self.edit.descriptor(result)
    }

    /// Holder identity for a born resource of the given transaction.
    pub(super) fn transaction_identity(&self, transaction: u16) -> Step<Identity> {
        Ok(self.edit.transaction(transaction)?.id)
    }
}
