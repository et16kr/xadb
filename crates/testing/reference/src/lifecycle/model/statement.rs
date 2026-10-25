//! Prepare, BindAll, Execute, CloseStatement and InspectStatement transitions.
use super::{
    Authenticated, EdgeClass, Exclusion, Flow, INVALID, OriginCause, Response, Rows, Run, Step,
};
use crate::lifecycle::schema::{
    Access, Autocommit, BindingLife, BindingRecord, BindingValue, Cleanup, ClientKnowledge,
    CommitProgress, ContextRef, Decision, Delivery, Descriptor, EngineCause, HolderRef,
    IdentityKind, Metadata, NextWork, ObligationReason, OwnerDisposition, Phase, ReceiptKind,
    ResourceKind, ResultRecord, ResultRef, ResultState, Retirement, Retry, ReturnCode, Shape,
    SlotState, StatementEffect, StatementLife, StatementRecord, StatementRef, TransactionLife,
    TransactionOrigin, TransactionRecord, TransactionRef,
};

impl Run<'_> {
    pub(super) fn prepare(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let caller = self.caller()?;
        if caller.active_public_result.is_some() {
            return self.reject(ReturnCode::ResultBusy);
        }
        let slot_index = self.input.prepare_slot;
        let slot = self.edit.slot(caller.actor, slot_index)?;
        if !matches!(slot.state, SlotState::Vacant | SlotState::Closed) {
            return self.reject(ReturnCode::SlotBusy);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        let context = self.context.ok_or(INVALID)?;
        let context_ref = Some(ContextRef(context.nonce));
        if slot.state == SlotState::Closed
            && let Some(old) = slot.statement
        {
            let record = self.edit.statement_mut(old.0)?;
            record.life = StatementLife::Retired;
            record.retirement = Retirement::SlotReuse;
            record.retirement_context = context_ref;
            self.edit
                .slot_mut(caller.actor, slot_index)?
                .next_statement_generation += 1;
        }
        let generation = self
            .edit
            .slot(caller.actor, slot_index)?
            .next_statement_generation;
        let scope = Self::scope(&caller);
        let id = self
            .edit
            .issue(IdentityKind::Statement, scope, slot_index, generation);
        let shape = if self.input.prepare_shape == 0 {
            Shape::Select
        } else {
            Shape::Dml
        };
        let n = self.input.prepare_n;
        let metadata = match (shape, n) {
            (Shape::Select, 0) => Metadata::SelectN0,
            (Shape::Select, _) => Metadata::SelectN1Utf8NotNullMax1,
            (Shape::Dml, 0) => Metadata::DmlN0,
            (Shape::Dml, _) => Metadata::DmlN1Utf8NotNullMax1,
        };
        self.edit.add_statement(StatementRecord {
            id,
            issue_ordinal: id.nonce,
            shape,
            parameter_count: n,
            metadata,
            life: StatementLife::Current,
            retirement: Retirement::None,
            session: crate::lifecycle::schema::SessionRef(caller.session.0),
            creation_context: context_ref,
            retirement_context: None,
        })?;
        self.edit.birth(ResourceKind::Plan, id, context)?;
        let binding = if n == 0 {
            let b = self.edit.issue(IdentityKind::Binding, scope, 255, 0);
            self.edit.add_binding(BindingRecord {
                id: b,
                statement: StatementRef(id.nonce),
                issue_ordinal: b.nonce,
                value: BindingValue::Empty,
                life: BindingLife::Current,
                replaced_by: None,
                creation_context: context_ref,
            })?;
            self.edit.birth(ResourceKind::Binding, id, context)?;
            Some(crate::lifecycle::schema::BindingRef(b.nonce))
        } else {
            None
        };
        let slot = self.edit.slot_mut(caller.actor, slot_index)?;
        slot.statement = Some(StatementRef(id.nonce));
        slot.next_statement_generation += 1;
        slot.binding = binding;
        slot.descriptor_result = None;
        slot.state = if n == 0 {
            SlotState::ReadyBound
        } else {
            SlotState::ReadyUnbound
        };
        self.complete_fresh()?;
        let mut response = self.success(Phase::Prepare, StatementEffect::NoEffect);
        response.returned_statement = Some(id);
        self.respond(response, None)
    }

    pub(super) fn bind(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        let Some(statement) = self.authenticate_statement()? else {
            return self.reject_handle();
        };
        // BindAll selects no transaction (wire-v2 §4 selection).
        self.authenticated_child(Some(statement.id), None, None)?;
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let caller = self.caller()?;
        if caller.active_public_result.is_some() {
            return self.reject(ReturnCode::ResultBusy);
        }
        if statement.life == StatementLife::CurrentClosed {
            return self.reject(ReturnCode::HandleClosed);
        }
        let slot = self.edit.slot(statement.id.actor, statement.id.slot)?;
        if slot.state == SlotState::Invalid {
            return self.reject(ReturnCode::StalePlan);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        let n1 = statement.parameter_count == 1;
        let value = match (self.input.outcome, n1) {
            (0, true) => Ok(BindingValue::A61),
            (1, true) => Ok(BindingValue::B62),
            (0 | 1, false) | (2, false) => Ok(BindingValue::Empty),
            (2 | 10, _) => Err((ReturnCode::BindCount, Retry::Never)),
            (3..=7, false) => return Ok(Flow::Exclude(Exclusion::BindN0)),
            (3, true) => Err((ReturnCode::Type, Retry::Never)),
            (4, true) => Err((ReturnCode::NotNull, Retry::Never)),
            (5, true) => Err((ReturnCode::Range, Retry::Never)),
            (6, true) => Err((ReturnCode::SyntaxUtf8, Retry::Never)),
            (7, true) => Err((ReturnCode::Limit, Retry::Operation)),
            _ => return Ok(Flow::Exclude(Exclusion::Envelope)),
        };
        let value = match value {
            Ok(value) => value,
            Err((code, retry)) => {
                self.complete_fresh()?;
                let response = self.error(
                    code,
                    Phase::Bind,
                    StatementEffect::NoEffect,
                    retry,
                    OriginCause::None,
                );
                return self.respond(response, None);
            }
        };
        let context = self.context.ok_or(INVALID)?;
        let scope = Self::scope(&caller);
        let new = self.edit.issue(IdentityKind::Binding, scope, 255, 0);
        if let Some(old) = slot.binding {
            let record = self.edit.binding_mut(old.0)?;
            record.life = BindingLife::Replaced;
            record.replaced_by = Some(crate::lifecycle::schema::BindingRef(new.nonce));
        }
        let holder = HolderRef {
            kind: IdentityKind::Statement,
            nonce: statement.id.nonce,
        };
        let old_owner = self
            .edit
            .state()
            .owners
            .as_slice()
            .iter()
            .find(|o| {
                o.original_holder == holder
                    && o.resource_kind == ResourceKind::Binding
                    && o.disposition != OwnerDisposition::Released
            })
            .map(|o| o.id.nonce);
        if let Some(owner) = old_owner {
            self.edit.release_local(owner)?;
        }
        self.edit.add_binding(BindingRecord {
            id: new,
            statement: StatementRef(statement.id.nonce),
            issue_ordinal: new.nonce,
            value,
            life: BindingLife::Current,
            replaced_by: None,
            creation_context: Some(ContextRef(context.nonce)),
        })?;
        self.edit
            .birth(ResourceKind::Binding, statement.id, context)?;
        let slot = self.edit.slot_mut(statement.id.actor, statement.id.slot)?;
        slot.binding = Some(crate::lifecycle::schema::BindingRef(new.nonce));
        slot.state = SlotState::ReadyBound;
        self.complete_fresh()?;
        let response = self.success(Phase::Bind, StatementEffect::NoEffect);
        self.respond(response, None)
    }

    pub(super) fn execute(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        let Some(statement) = self.authenticate_statement()? else {
            return self.reject_handle();
        };
        let held = self.caller()?.transaction.map(|t| t.0);
        self.authenticated_child(Some(statement.id), None, held)?;
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let caller = self.caller()?;
        if caller.active_public_result.is_some() {
            return self.reject(ReturnCode::ResultBusy);
        }
        if statement.life == StatementLife::CurrentClosed {
            return self.reject(ReturnCode::HandleClosed);
        }
        let slot = self.edit.slot(statement.id.actor, statement.id.slot)?;
        match slot.state {
            SlotState::Invalid => return self.reject(ReturnCode::StalePlan),
            SlotState::ReadyUnbound => return self.reject(ReturnCode::BindCount),
            _ => {}
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let outcome = self.input.outcome;
        let scope = Self::scope(&caller);
        let (existing, origin, access) = match caller.transaction {
            Some(t) => {
                let r = self.edit.transaction(t.0)?;
                (Some(r), r.origin, r.access)
            }
            None => match (caller.autocommit, statement.shape) {
                (Autocommit::On, Shape::Select) => {
                    (None, TransactionOrigin::ImplicitReadOnly, Access::ReadOnly)
                }
                (Autocommit::On, Shape::Dml) => (
                    None,
                    TransactionOrigin::ImplicitReadWrite,
                    Access::ReadWrite,
                ),
                (Autocommit::Off, _) => (None, TransactionOrigin::OffStarted, Access::ReadWrite),
            },
        };
        let prospective_id = crate::lifecycle::schema::Identity {
            kind: IdentityKind::Transaction,
            slot: 255,
            nonce: self.edit.next(IdentityKind::Transaction),
            ..scope
        };
        let selected = existing.map_or(prospective_id, |t| t.id);
        if let Some(rule) = self.envelope_transaction(Some(selected)) {
            return Ok(Flow::Exclude(rule));
        }
        let implicit_rw_new = existing.is_none() && origin == TransactionOrigin::ImplicitReadWrite;
        match outcome {
            8 if access != Access::ReadWrite => {
                return Ok(Flow::Exclude(Exclusion::ReadOnlyCleanup));
            }
            9 | 10 if !implicit_rw_new => return Ok(Flow::Exclude(Exclusion::ExecuteCommit)),
            _ => {}
        }
        let context = self.context.ok_or(INVALID)?;
        let t = match existing {
            Some(t) => t.id.nonce,
            None => {
                let id = self.edit.issue(IdentityKind::Transaction, scope, 255, 0);
                self.edit.add_transaction(TransactionRecord {
                    id,
                    origin,
                    access,
                    life: TransactionLife::Active,
                    prior_successful_write: false,
                    progress: CommitProgress::None,
                    decision: if access == Access::ReadWrite {
                        Decision::NotAttempted
                    } else {
                        Decision::NotApplicable
                    },
                    client_knowledge: ClientKnowledge::NoDecisionObserved,
                    delivery: Delivery::NotApplicable,
                    birth_session: crate::lifecycle::schema::SessionRef(caller.session.0),
                    receipt: None,
                    parked_context: None,
                    obligation: None,
                    completion_fact: None,
                })?;
                self.edit.actor_mut(caller.actor)?.transaction = Some(TransactionRef(id.nonce));
                id.nonce
            }
        };
        // The previous terminal result of this statement is discarded at the admitted boundary.
        if let Some(old) = slot.descriptor_result {
            self.retire_result(old.0, Retirement::Reexecute, true)?;
        }
        let select = statement.shape == Shape::Select;
        match outcome {
            0 if select => {
                let generation = self
                    .edit
                    .slot(statement.id.actor, statement.id.slot)?
                    .next_result_generation;
                let r = self.edit.issue(
                    IdentityKind::Result,
                    statement.id,
                    statement.id.slot,
                    generation,
                );
                self.new_result = Some(r);
                self.edit.add_result(ResultRecord {
                    id: r,
                    issue_ordinal: r.nonce,
                    statement: StatementRef(statement.id.nonce),
                    retirement: Retirement::None,
                    public_generation_live: true,
                    creation_context: Some(ContextRef(context.nonce)),
                    retirement_context: None,
                })?;
                self.edit.add_descriptor(Descriptor {
                    result: ResultRef(r.nonce),
                    statement: StatementRef(statement.id.nonce),
                    state: ResultState::Open,
                    public_generation_live: true,
                    blocked_by_fence: false,
                    position: 0,
                    transaction: Some(TransactionRef(t)),
                    terminal: None,
                })?;
                let s = self.edit.slot_mut(statement.id.actor, statement.id.slot)?;
                s.next_result_generation += 1;
                s.descriptor_result = Some(ResultRef(r.nonce));
                s.state = SlotState::ResultOpen;
                self.edit.actor_mut(caller.actor)?.active_public_result = Some(ResultRef(r.nonce));
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    1,
                    Some(t),
                    None,
                    false,
                )?;
                self.complete_fresh()?;
                let mut response = self.success(Phase::Execute, StatementEffect::NoEffect);
                response.rows = Rows::Count(0);
                response.returned_result = Some(r);
                self.respond(response, Some(t))
            }
            0 => {
                self.edit.transaction_mut(t)?.prior_successful_write = true;
                let implicit = origin == TransactionOrigin::ImplicitReadWrite;
                let parts = if implicit { 1 | 4 | 8 } else { 1 };
                if implicit {
                    let receipt =
                        self.install_receipt(t, ReceiptKind::DurableWrite, Delivery::Delivered)?;
                    let record = self.edit.transaction_mut(t)?;
                    record.progress = CommitProgress::Published;
                    record.delivery = Delivery::Delivered;
                    if receipt.is_none() {
                        return Ok(Flow::Exclude(Exclusion::Envelope));
                    }
                    self.end_transaction(t, Some(Decision::Committed))?;
                }
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    parts,
                    Some(t),
                    None,
                    implicit,
                )?;
                if implicit {
                    self.link_receipt_fact(t, context)?;
                }
                self.complete_fresh()?;
                let mut response = self.success(Phase::Execute, StatementEffect::StatementApplied);
                response.affected_count = Some(1);
                self.respond(response, Some(t))
            }
            3..=7 => {
                let (code, effect, retry, origin_cause) = match outcome {
                    3 => (
                        ReturnCode::StalePlan,
                        StatementEffect::NoEffect,
                        Retry::Never,
                        OriginCause::None,
                    ),
                    4 => (
                        ReturnCode::ValueError,
                        StatementEffect::StatementRestored,
                        Retry::Operation,
                        OriginCause::ValueError,
                    ),
                    5 => (
                        ReturnCode::Cancelled,
                        StatementEffect::StatementRestored,
                        Retry::Operation,
                        OriginCause::Cancelled,
                    ),
                    6 => (
                        ReturnCode::Timeout,
                        StatementEffect::StatementRestored,
                        Retry::Operation,
                        OriginCause::Timeout,
                    ),
                    _ => (
                        ReturnCode::Conflict,
                        StatementEffect::StatementRestored,
                        Retry::WholeTransactionDecision,
                        OriginCause::Conflict,
                    ),
                };
                match outcome {
                    5 => self.edge.controls.observed_cancel = true,
                    6 => self.edge.controls.observed_deadline = true,
                    _ => {}
                }
                if outcome == 3 {
                    self.edit
                        .slot_mut(statement.id.actor, statement.id.slot)?
                        .state = SlotState::Invalid;
                }
                if outcome == 7 {
                    self.abort_transaction(t)?;
                } else if existing.is_none() {
                    self.end_if_implicit(t)?;
                }
                let parts = if matches!(outcome, 5 | 6) { 2 } else { 1 | 2 };
                self.consume(
                    Cleanup::Complete,
                    NextWork::NotConsumed,
                    parts,
                    Some(t),
                    None,
                    false,
                )?;
                self.complete_fresh()?;
                let mut response = self.error(code, Phase::Execute, effect, retry, origin_cause);
                if outcome == 7 {
                    response.statement_effect = StatementEffect::StatementRestored;
                }
                self.respond(response, Some(t))
            }
            8 => {
                self.fence(
                    Some(t),
                    EngineCause::UndoIncomplete,
                    Decision::NotCommittedRecoveryRequired,
                )?;
                self.consume(
                    Cleanup::Incomplete,
                    NextWork::NotConsumed,
                    1 | 2,
                    Some(t),
                    Some(ObligationReason::KnownNotCommittedUndoPending),
                    false,
                )?;
                self.complete_fresh()?;
                let response = self.admitted_error(
                    ReturnCode::RecoveryRequired,
                    Phase::Execute,
                    StatementEffect::RecoveryRequired,
                    Retry::AfterRecovery,
                    EngineCause::UndoIncomplete,
                    OriginCause::UndoIncomplete,
                );
                self.respond(response, Some(t))
            }
            9 => {
                self.edit.transaction_mut(t)?.prior_successful_write = true;
                self.edit.transaction_mut(t)?.progress = CommitProgress::Attempted;
                self.fence(Some(t), EngineCause::CommitUnknown, Decision::Unknown)?;
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::NotConsumed,
                    1 | 8,
                    Some(t),
                    Some(ObligationReason::CommitDecisionUnknown),
                    false,
                )?;
                self.complete_fresh()?;
                let response = self.admitted_error(
                    ReturnCode::ResultUnknown,
                    Phase::Commit,
                    StatementEffect::RecoveryRequired,
                    Retry::AfterRecovery,
                    EngineCause::CommitUnknown,
                    OriginCause::CommitUnknown,
                );
                self.respond(response, Some(t))
            }
            10 => {
                self.edit.transaction_mut(t)?.prior_successful_write = true;
                if self
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
                    1 | 4 | 8,
                    Some(t),
                    Some(ObligationReason::KnownCommittedPublicationPending),
                    true,
                )?;
                self.link_receipt_fact(t, context)?;
                self.complete_fresh()?;
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
            _ => Ok(Flow::Exclude(Exclusion::Envelope)),
        }
    }

    /// The receipt installed by this call records the consuming fact and delivery context.
    pub(super) fn link_receipt_fact(
        &mut self,
        transaction: u16,
        context: crate::lifecycle::schema::Identity,
    ) -> Step<()> {
        let fact = self.fact.ok_or(INVALID)?;
        let receipt = self.edit.transaction(transaction)?.receipt.ok_or(INVALID)?;
        let record = self.edit.receipt_mut(receipt.0)?;
        record.source_fact = Some(crate::lifecycle::schema::FactRef(fact.nonce));
        if record.delivery == Delivery::Delivered {
            record.delivery_context = Some(ContextRef(context.nonce));
        }
        Ok(())
    }

    pub(super) fn close_statement(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        let Some(statement) = self.authenticate_statement()? else {
            return self.reject_handle();
        };
        // CloseStatement selects its slot descriptor's transaction, if any (wire-v2 §4).
        let held = match self
            .edit
            .slot(statement.id.actor, statement.id.slot)?
            .descriptor_result
        {
            Some(r) => self.descriptor_of(r.0)?.transaction.map(|t| t.0),
            None => None,
        };
        self.authenticated_child(Some(statement.id), None, held)?;
        if let Some(code) = self.session_gates(true)? {
            return self.reject(code);
        }
        if statement.life == StatementLife::CurrentClosed {
            self.complete_fresh()?;
            let response = self.success(Phase::Close, StatementEffect::NoEffect);
            return self.respond(response, None);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        let slot = self.edit.slot(statement.id.actor, statement.id.slot)?;
        let mut operated = None;
        let mut released_cursor = false;
        if let Some(r) = slot.descriptor_result {
            let d = self.descriptor_of(r.0)?;
            operated = d.transaction.map(|t| t.0);
            if d.state == ResultState::Open {
                released_cursor = true;
            }
        }
        if let Some(rule) = self.envelope_transaction(match operated {
            Some(t) => Some(self.transaction_identity(t)?),
            None => None,
        }) {
            return Ok(Flow::Exclude(rule));
        }
        self.consume(
            Cleanup::Complete,
            NextWork::NotConsumed,
            2,
            operated,
            None,
            false,
        )?;
        if let Some(r) = slot.descriptor_result {
            let d = self.descriptor_of(r.0)?;
            // Only an Active implicit transaction ends here; a fenced RecoveryPending one keeps
            // its completion fact (owner decision 2026-10-25).
            if d.state == ResultState::Open
                && let Some(t) = d.transaction
                && self.edit.transaction(t.0)?.life == TransactionLife::Active
            {
                self.end_if_implicit(t.0)?;
                self.settle_completion(t.0)?;
            }
            self.retire_result(r.0, Retirement::StatementClose, true)?;
        }
        if let Some(b) = slot.binding {
            self.edit.binding_mut(b.0)?.life = BindingLife::Released;
        }
        self.edit.statement_mut(statement.id.nonce)?.life = StatementLife::CurrentClosed;
        let s = self.edit.slot_mut(statement.id.actor, statement.id.slot)?;
        s.state = SlotState::Closed;
        s.descriptor_result = None;
        s.binding = None;
        self.complete_fresh()?;
        let effect = if released_cursor {
            StatementEffect::CursorReleased
        } else {
            StatementEffect::NoEffect
        };
        let response = self.success(Phase::Close, effect);
        self.respond(response, operated)
    }

    pub(super) fn inspect_statement(&mut self) -> Step<Flow> {
        let Some(statement) = self.authenticate_statement()? else {
            self.edge.authenticated = Some(Authenticated::default());
            return self.no_context_reject(ReturnCode::InvalidHandle);
        };
        // InspectStatement selects no transaction (wire-v2 §4 selection).
        self.authenticated_child(Some(statement.id), None, None)?;
        let caller = self.caller()?;
        if caller.shell == crate::lifecycle::schema::Shell::Closed {
            return self.no_context_reject(ReturnCode::SessionState);
        }
        if caller.parked_context.is_some() {
            return self.no_context_reject(ReturnCode::Busy);
        }
        if statement.life == StatementLife::CurrentClosed {
            return self.no_context_reject(ReturnCode::HandleClosed);
        }
        self.edge.class = EdgeClass::NoEffect;
        let response = self.success(Phase::NotApplicable, StatementEffect::NoEffect);
        self.respond(response, None)
    }

    /// A context-free diagnostic rejected at a gate: no context and no state change.
    pub(super) fn no_context_reject(&mut self, code: ReturnCode) -> Step<Flow> {
        self.edge.class = EdgeClass::Rejected;
        if let Some(targets) = self.authenticated {
            self.edge.authenticated = Some(targets);
        }
        let retry = match code {
            ReturnCode::Busy => Retry::Operation,
            ReturnCode::EngineFenced => Retry::AfterRecovery,
            _ => Retry::Never,
        };
        let mut response: Response = self.error(
            code,
            Phase::Entry,
            StatementEffect::NoEffect,
            retry,
            OriginCause::None,
        );
        self.fenced_cause(&mut response)?;
        let selected = self
            .authenticated
            .and_then(|a| a.transaction)
            .map(|t| t.nonce);
        self.respond(response, selected)
    }
}
