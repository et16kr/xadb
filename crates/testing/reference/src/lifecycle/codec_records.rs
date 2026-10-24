//! Fixed-field encoding for bounded state records and raw environment envelopes.
use super::{CodecError, Field, Reader, Writer, ensure};
use crate::lifecycle::input_schema::{
    BindValue, Environment, InputObligation, InputOwner, RawIdentity,
};
use crate::lifecycle::schema::{
    Access, ActorRecord, AnchorRef, Api, Autocommit, BindingLife, BindingRecord, BindingRef,
    BindingValue, Certainty, Cleanup, ClientKnowledge, CommitProgress, CompletionPhase,
    ContextData, ContextRecord, ContextRef, ContextStatus, Deadline, Decision, Delivery,
    Descriptor, Diagnostic, EngineAdmission, EngineCause, EngineRecord, EngineRef, FactKind,
    FactObligation, FactOwner, FactRecord, FactRef, HolderRef, Identity, IdentityKind, Metadata,
    NextWork, ObligationReason, ObligationRecord, ObligationRef, OriginCause, OwnerDisposition,
    OwnerRecord, OwnershipSummary, ParkedContext, Phase, PreparedSlot, ReceiptKind, ReceiptRecord,
    ReceiptRef, Records, ReopenKind, ResourceKind, ResourceRef, ResultRecord, ResultRef,
    ResultState, Retirement, Retry, ReturnCode, SessionRecord, SessionRef, Settlement, Shape,
    Shell, SlotState, State, StatementEffect, StatementLife, StatementRecord, StatementRef,
    TransactionLife, TransactionOrigin, TransactionRecord, TransactionRef,
};

macro_rules! tags {
 ($($ty:ty),* $(,)?) => {$(impl Field for $ty {
 fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> { let at=r.pos; Self::try_from(r.byte()?).map_err(|_| CodecError::new(at,"unknown enum tag")) }
 fn write(&self,w: &mut Writer<'_>) -> Result<(),CodecError> {w.byte(*self as u8)}
 })*};
}
tags!(
    IdentityKind,
    EngineAdmission,
    EngineCause,
    Shell,
    Autocommit,
    ReopenKind,
    SlotState,
    Shape,
    Metadata,
    StatementLife,
    Retirement,
    ResultState,
    ContextStatus,
    Deadline,
    CompletionPhase,
    TransactionOrigin,
    Access,
    TransactionLife,
    CommitProgress,
    Decision,
    ClientKnowledge,
    Delivery,
    ObligationReason,
    OwnerDisposition,
    Settlement,
    BindingValue,
    BindingLife,
    ReceiptKind,
    ResourceKind,
    FactKind,
    Cleanup,
    NextWork,
    Api,
    ReturnCode,
    Phase,
    StatementEffect,
    Certainty,
    Retry,
    OriginCause,
    OwnershipSummary
);
macro_rules! references {
 ($($ty:ident),* $(,)?) => {$(impl Field for $ty {
 fn read(r:&mut Reader<'_>)->Result<Self,CodecError>{let at=r.pos;let n=r.word()?;ensure(n!=0,at,"zero typed reference")?;Ok(Self(n))}
 fn write(&self,w:&mut Writer<'_>)->Result<(),CodecError>{ensure(self.0!=0,w.pos,"zero typed reference")?;w.word(self.0)}
 })*};
}
references!(
    EngineRef,
    SessionRef,
    StatementRef,
    ResultRef,
    ContextRef,
    TransactionRef,
    BindingRef,
    ReceiptRef,
    ResourceRef,
    ObligationRef,
    FactRef,
    AnchorRef
);
impl Field for Identity {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let kind = Field::read(r)?;
        let actor = r.byte()?;
        let slot = r.byte()?;
        r.zero(1)?;
        let engine = r.word()?;
        let session = r.word()?;
        let nonce = r.word()?;
        let generation = r.word()?;
        r.zero(4)?;
        Ok(Self {
            kind,
            actor,
            slot,
            engine,
            session,
            nonce,
            generation,
        })
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.kind.write(w)?;
        w.byte(self.actor)?;
        w.byte(self.slot)?;
        w.zero(1)?;
        w.word(self.engine)?;
        w.word(self.session)?;
        w.word(self.nonce)?;
        w.word(self.generation)?;
        w.zero(4)
    }
}
impl Field for HolderRef {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let kind = Field::read(r)?;
        let nonce = r.word()?;
        Ok(Self { kind, nonce })
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.kind.write(w)?;
        w.word(self.nonce)
    }
}
impl Field for RawIdentity {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(r.take(16)?);
        Ok(Self(bytes))
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        w.put(&self.0)
    }
}
impl Field for Diagnostic {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let value = Self {
            code: Field::read(r)?,
            phase: Field::read(r)?,
            effect: Field::read(r)?,
            certainty: Field::read(r)?,
            retry: Field::read(r)?,
            engine_cause: Field::read(r)?,
            origin_cause: Field::read(r)?,
        };
        r.zero(9)?;
        Ok(value)
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.code.write(w)?;
        self.phase.write(w)?;
        self.effect.write(w)?;
        self.certainty.write(w)?;
        self.retry.write(w)?;
        self.engine_cause.write(w)?;
        self.origin_cause.write(w)?;
        w.zero(9)
    }
}
pub(super) trait Record: Sized {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError>;
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError>;
}
fn read_record<T: Record>(r: &mut Reader<'_>) -> Result<T, CodecError> {
    let start = r.pos;
    let len = usize::from(r.word()?);
    ensure(len >= 2, start, "short record length")?;
    let mut body = Reader::new(r.take(len - 2)?);
    let row = T::read_body(&mut body)?;
    body.end()?;
    Ok(row)
}
fn write_record<T: Record>(value: &T, w: &mut Writer<'_>) -> Result<(), CodecError> {
    let start = w.pos;
    w.word(0)?;
    value.write_body(w)?;
    let len = u16::try_from(w.pos - start)
        .map_err(|_| CodecError::new(start, "record length overflow"))?;
    w.patch_word(start, len)
}
pub(super) fn read_table<T: Record + Copy, const N: usize>(
    r: &mut Reader<'_>,
    out: &mut Records<T, N>,
    count: usize,
) -> Result<(), CodecError> {
    ensure(count <= N, r.pos, "section count cap")?;
    out.clear();
    for _ in 0..count {
        let row = read_record(r)?;
        out.try_push(row)
            .map_err(|_| CodecError::new(r.pos, "section count cap"))?;
    }
    Ok(())
}
pub(super) fn write_table<T: Record, const N: usize>(
    table: &Records<T, N>,
    w: &mut Writer<'_>,
) -> Result<(), CodecError> {
    for row in table.as_slice() {
        write_record(row, w)?;
    }
    Ok(())
}
impl Record for EngineRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let admission = Field::read(r)?;
        let first_cause = Field::read(r)?;
        let known_decision = Field::read(r)?;
        r.zero(1)?;
        let cause_transaction = Field::read(r)?;
        let first_fence_context = Field::read(r)?;
        let successor_engine = Field::read(r)?;
        let recovery_fact = Field::read(r)?;
        Ok(Self {
            id,
            admission,
            first_cause,
            known_decision,
            cause_transaction,
            first_fence_context,
            successor_engine,
            recovery_fact,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.admission.write(w)?;
        self.first_cause.write(w)?;
        self.known_decision.write(w)?;
        w.zero(1)?;
        self.cause_transaction.write(w)?;
        self.first_fence_context.write(w)?;
        self.successor_engine.write(w)?;
        self.recovery_fact.write(w)?;
        Ok(())
    }
}
impl Record for SessionRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let engine = Field::read(r)?;
        let actor = Field::read(r)?;
        let lifecycle = Field::read(r)?;
        let reopen_kind = Field::read(r)?;
        r.zero(1)?;
        let previous_session = Field::read(r)?;
        let closing_context = Field::read(r)?;
        Ok(Self {
            id,
            engine,
            actor,
            lifecycle,
            reopen_kind,
            previous_session,
            closing_context,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.engine.write(w)?;
        self.actor.write(w)?;
        self.lifecycle.write(w)?;
        self.reopen_kind.write(w)?;
        w.zero(1)?;
        self.previous_session.write(w)?;
        self.closing_context.write(w)?;
        Ok(())
    }
}
impl Record for ActorRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let actor = Field::read(r)?;
        let shell = Field::read(r)?;
        let autocommit = Field::read(r)?;
        r.zero(1)?;
        let session = Field::read(r)?;
        let engine = Field::read(r)?;
        let transaction = Field::read(r)?;
        let parked_context = Field::read(r)?;
        let active_public_result = Field::read(r)?;
        Ok(Self {
            actor,
            shell,
            autocommit,
            session,
            engine,
            transaction,
            parked_context,
            active_public_result,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.actor.write(w)?;
        self.shell.write(w)?;
        self.autocommit.write(w)?;
        w.zero(1)?;
        self.session.write(w)?;
        self.engine.write(w)?;
        self.transaction.write(w)?;
        self.parked_context.write(w)?;
        self.active_public_result.write(w)?;
        Ok(())
    }
}
impl Record for PreparedSlot {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let actor = Field::read(r)?;
        let slot = Field::read(r)?;
        let state = Field::read(r)?;
        r.zero(1)?;
        let next_statement_generation = Field::read(r)?;
        let next_result_generation = Field::read(r)?;
        let statement = Field::read(r)?;
        let binding = Field::read(r)?;
        let descriptor_result = Field::read(r)?;
        Ok(Self {
            actor,
            slot,
            state,
            next_statement_generation,
            next_result_generation,
            statement,
            binding,
            descriptor_result,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.actor.write(w)?;
        self.slot.write(w)?;
        self.state.write(w)?;
        w.zero(1)?;
        self.next_statement_generation.write(w)?;
        self.next_result_generation.write(w)?;
        self.statement.write(w)?;
        self.binding.write(w)?;
        self.descriptor_result.write(w)?;
        Ok(())
    }
}
impl Record for StatementRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let issue_ordinal = Field::read(r)?;
        let shape = Field::read(r)?;
        let parameter_count = Field::read(r)?;
        let metadata = Field::read(r)?;
        let life = Field::read(r)?;
        let retirement = Field::read(r)?;
        r.zero(1)?;
        let session = Field::read(r)?;
        let creation_context = Field::read(r)?;
        let retirement_context = Field::read(r)?;
        Ok(Self {
            id,
            issue_ordinal,
            shape,
            parameter_count,
            metadata,
            life,
            retirement,
            session,
            creation_context,
            retirement_context,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.issue_ordinal.write(w)?;
        self.shape.write(w)?;
        self.parameter_count.write(w)?;
        self.metadata.write(w)?;
        self.life.write(w)?;
        self.retirement.write(w)?;
        w.zero(1)?;
        self.session.write(w)?;
        self.creation_context.write(w)?;
        self.retirement_context.write(w)?;
        Ok(())
    }
}
impl Record for ResultRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let issue_ordinal = Field::read(r)?;
        let statement = Field::read(r)?;
        let retirement = Field::read(r)?;
        let public_generation_live = Field::read(r)?;
        r.zero(2)?;
        let creation_context = Field::read(r)?;
        let retirement_context = Field::read(r)?;
        Ok(Self {
            id,
            issue_ordinal,
            statement,
            retirement,
            public_generation_live,
            creation_context,
            retirement_context,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.issue_ordinal.write(w)?;
        self.statement.write(w)?;
        self.retirement.write(w)?;
        self.public_generation_live.write(w)?;
        w.zero(2)?;
        self.creation_context.write(w)?;
        self.retirement_context.write(w)?;
        Ok(())
    }
}
impl Record for Descriptor {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let result = Field::read(r)?;
        let statement = Field::read(r)?;
        let state = Field::read(r)?;
        let public_generation_live = Field::read(r)?;
        let blocked_by_fence = Field::read(r)?;
        r.zero(1)?;
        let position = Field::read(r)?;
        let transaction = Field::read(r)?;
        let terminal = Field::read(r)?;
        Ok(Self {
            result,
            statement,
            state,
            public_generation_live,
            blocked_by_fence,
            position,
            transaction,
            terminal,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.result.write(w)?;
        self.statement.write(w)?;
        self.state.write(w)?;
        self.public_generation_live.write(w)?;
        self.blocked_by_fence.write(w)?;
        w.zero(1)?;
        self.position.write(w)?;
        self.transaction.write(w)?;
        self.terminal.write(w)?;
        Ok(())
    }
}
impl Record for TransactionRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let origin = Field::read(r)?;
        let access = Field::read(r)?;
        let life = Field::read(r)?;
        let prior_successful_write = Field::read(r)?;
        let progress = Field::read(r)?;
        let decision = Field::read(r)?;
        let client_knowledge = Field::read(r)?;
        let delivery = Field::read(r)?;
        let birth_session = Field::read(r)?;
        r.zero(2)?;
        let receipt = Field::read(r)?;
        let parked_context = Field::read(r)?;
        let obligation = Field::read(r)?;
        let completion_fact = Field::read(r)?;
        Ok(Self {
            id,
            origin,
            access,
            life,
            prior_successful_write,
            progress,
            decision,
            client_knowledge,
            delivery,
            birth_session,
            receipt,
            parked_context,
            obligation,
            completion_fact,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.origin.write(w)?;
        self.access.write(w)?;
        self.life.write(w)?;
        self.prior_successful_write.write(w)?;
        self.progress.write(w)?;
        self.decision.write(w)?;
        self.client_knowledge.write(w)?;
        self.delivery.write(w)?;
        self.birth_session.write(w)?;
        w.zero(2)?;
        self.receipt.write(w)?;
        self.parked_context.write(w)?;
        self.obligation.write(w)?;
        self.completion_fact.write(w)?;
        Ok(())
    }
}
impl Record for ObligationRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let engine = Field::read(r)?;
        let transaction = Field::read(r)?;
        let origin_session = Field::read(r)?;
        let creation_reason = Field::read(r)?;
        let latest_reason = Field::read(r)?;
        let reason_mask = Field::read(r)?;
        let disposition = Field::read(r)?;
        let known_decision = Field::read(r)?;
        let settlement = Field::read(r)?;
        r.zero(2)?;
        let first_fact = Field::read(r)?;
        let latest_fact = Field::read(r)?;
        let recovery_fact = Field::read(r)?;
        Ok(Self {
            id,
            engine,
            transaction,
            origin_session,
            creation_reason,
            latest_reason,
            reason_mask,
            disposition,
            known_decision,
            settlement,
            first_fact,
            latest_fact,
            recovery_fact,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.engine.write(w)?;
        self.transaction.write(w)?;
        self.origin_session.write(w)?;
        self.creation_reason.write(w)?;
        self.latest_reason.write(w)?;
        self.reason_mask.write(w)?;
        self.disposition.write(w)?;
        self.known_decision.write(w)?;
        self.settlement.write(w)?;
        w.zero(2)?;
        self.first_fact.write(w)?;
        self.latest_fact.write(w)?;
        self.recovery_fact.write(w)?;
        Ok(())
    }
}
impl Record for BindingRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let statement = Field::read(r)?;
        let issue_ordinal = Field::read(r)?;
        let value = Field::read(r)?;
        let life = Field::read(r)?;
        r.zero(2)?;
        let replaced_by = Field::read(r)?;
        let creation_context = Field::read(r)?;
        Ok(Self {
            id,
            statement,
            issue_ordinal,
            value,
            life,
            replaced_by,
            creation_context,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.statement.write(w)?;
        self.issue_ordinal.write(w)?;
        self.value.write(w)?;
        self.life.write(w)?;
        w.zero(2)?;
        self.replaced_by.write(w)?;
        self.creation_context.write(w)?;
        Ok(())
    }
}
impl Record for ReceiptRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let transaction = Field::read(r)?;
        let engine = Field::read(r)?;
        let kind = Field::read(r)?;
        let delivery = Field::read(r)?;
        let anchor = Field::read(r)?;
        let source_fact = Field::read(r)?;
        let delivery_context = Field::read(r)?;
        Ok(Self {
            id,
            transaction,
            engine,
            kind,
            delivery,
            anchor,
            source_fact,
            delivery_context,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.transaction.write(w)?;
        self.engine.write(w)?;
        self.kind.write(w)?;
        self.delivery.write(w)?;
        self.anchor.write(w)?;
        self.source_fact.write(w)?;
        self.delivery_context.write(w)?;
        Ok(())
    }
}
impl Record for OwnerRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let resource_kind = Field::read(r)?;
        let disposition = Field::read(r)?;
        let transfer_count = Field::read(r)?;
        let original_holder = Field::read(r)?;
        r.zero(1)?;
        let current_holder = Field::read(r)?;
        let previous_holder = Field::read(r)?;
        let birth_context = Field::read(r)?;
        let last_fact = Field::read(r)?;
        Ok(Self {
            id,
            resource_kind,
            disposition,
            transfer_count,
            original_holder,
            current_holder,
            previous_holder,
            birth_context,
            last_fact,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.resource_kind.write(w)?;
        self.disposition.write(w)?;
        self.transfer_count.write(w)?;
        self.original_holder.write(w)?;
        w.zero(1)?;
        self.current_holder.write(w)?;
        self.previous_holder.write(w)?;
        self.birth_context.write(w)?;
        self.last_fact.write(w)?;
        Ok(())
    }
}
impl Record for FactRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let kind = Field::read(r)?;
        let api = Field::read(r)?;
        let outcome_class = Field::read(r)?;
        let cleanup = Field::read(r)?;
        let decision = Field::read(r)?;
        let delivery = Field::read(r)?;
        let next_work = Field::read(r)?;
        let consumed_parts = Field::read(r)?;
        r.zero(1)?;
        let engine = Field::read(r)?;
        let obligation_count = Field::read(r)?;
        let owner_count = Field::read(r)?;
        let context = Field::read(r)?;
        let transaction = Field::read(r)?;
        let receipt = Field::read(r)?;
        let fresh_engine = Field::read(r)?;
        let anchor = Field::read(r)?;
        Ok(Self {
            id,
            kind,
            api,
            outcome_class,
            cleanup,
            decision,
            delivery,
            next_work,
            consumed_parts,
            engine,
            obligation_count,
            owner_count,
            context,
            transaction,
            receipt,
            fresh_engine,
            anchor,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        self.kind.write(w)?;
        self.api.write(w)?;
        self.outcome_class.write(w)?;
        self.cleanup.write(w)?;
        self.decision.write(w)?;
        self.delivery.write(w)?;
        self.next_work.write(w)?;
        self.consumed_parts.write(w)?;
        w.zero(1)?;
        self.engine.write(w)?;
        self.obligation_count.write(w)?;
        self.owner_count.write(w)?;
        self.context.write(w)?;
        self.transaction.write(w)?;
        self.receipt.write(w)?;
        self.fresh_engine.write(w)?;
        self.anchor.write(w)?;
        Ok(())
    }
}
impl Record for FactObligation {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let fact = Field::read(r)?;
        let obligation = Field::read(r)?;
        let known_decision = Field::read(r)?;
        let disposition = Field::read(r)?;
        let owner_count = Field::read(r)?;
        r.zero(2)?;
        Ok(Self {
            fact,
            obligation,
            known_decision,
            disposition,
            owner_count,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.fact.write(w)?;
        self.obligation.write(w)?;
        self.known_decision.write(w)?;
        self.disposition.write(w)?;
        self.owner_count.write(w)?;
        w.zero(2)?;
        Ok(())
    }
}
impl Record for FactOwner {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let fact = Field::read(r)?;
        let resource = Field::read(r)?;
        let disposition = Field::read(r)?;
        r.zero(1)?;
        let obligation = Field::read(r)?;
        let before_holder = Field::read(r)?;
        let after_holder = Field::read(r)?;
        Ok(Self {
            fact,
            resource,
            disposition,
            obligation,
            before_holder,
            after_holder,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.fact.write(w)?;
        self.resource.write(w)?;
        self.disposition.write(w)?;
        w.zero(1)?;
        self.obligation.write(w)?;
        self.before_holder.write(w)?;
        self.after_holder.write(w)?;
        Ok(())
    }
}
impl Record for ContextRecord {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let id = Field::read(r)?;
        let issue_ordinal = r.word()?;
        let status = ContextStatus::read(r)?;
        let data = match status {
            ContextStatus::Completed => ContextData::Completed,
            other => {
                let api = Field::read(r)?;
                let original_deadline = Field::read(r)?;
                let cancel_supplied_or_signalled = Field::read(r)?;
                let deadline_reached = Field::read(r)?;
                let cancel_observed_at_completion = Field::read(r)?;
                let deadline_observed_at_completion = Field::read(r)?;
                let completion_phase = Field::read(r)?;
                r.zero(2)?;
                let statement = Field::read(r)?;
                let result = Field::read(r)?;
                let transaction = Field::read(r)?;
                let payload = ParkedContext {
                    api,
                    original_deadline,
                    cancel_supplied_or_signalled,
                    deadline_reached,
                    cancel_observed_at_completion,
                    deadline_observed_at_completion,
                    completion_phase,
                    statement,
                    result,
                    transaction,
                };
                if other == ContextStatus::ParkedFetch {
                    ContextData::ParkedFetch(payload)
                } else {
                    ContextData::ParkedCommit(payload)
                }
            }
        };
        Ok(Self {
            id,
            issue_ordinal,
            data,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.id.write(w)?;
        w.word(self.issue_ordinal)?;
        self.data.status().write(w)?;
        match &self.data {
            ContextData::Completed => {}
            ContextData::ParkedFetch(p) | ContextData::ParkedCommit(p) => {
                p.api.write(w)?;
                p.original_deadline.write(w)?;
                p.cancel_supplied_or_signalled.write(w)?;
                p.deadline_reached.write(w)?;
                p.cancel_observed_at_completion.write(w)?;
                p.deadline_observed_at_completion.write(w)?;
                p.completion_phase.write(w)?;
                w.zero(2)?;
                p.statement.write(w)?;
                p.result.write(w)?;
                p.transaction.write(w)?;
            }
        }
        Ok(())
    }
}
impl Field for BindValue {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let tag = r.byte()?;
        ensure(tag <= 2, r.pos, "binding tag")?;
        let byte_length = r.word()?;
        ensure(byte_length <= 8, r.pos, "binding payload cap")?;
        let mut bytes = [0; 8];
        bytes[..usize::from(byte_length)].copy_from_slice(r.take(usize::from(byte_length))?);
        Ok(Self {
            tag,
            byte_length,
            bytes,
        })
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        ensure(
            self.tag <= 2 && self.byte_length <= 8,
            w.pos,
            "binding shape",
        )?;
        w.byte(self.tag)?;
        w.word(self.byte_length)?;
        w.put(&self.bytes[..usize::from(self.byte_length)])
    }
}
impl Record for InputObligation {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        let value = Self {
            obligation: Field::read(r)?,
            known_decision: Field::read(r)?,
            after_ownership_summary: Field::read(r)?,
            associated_owner_count: r.word()?,
        };
        ensure(
            value.associated_owner_count <= 54,
            r.pos,
            "input owner count",
        )?;
        Ok(value)
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        ensure(
            self.associated_owner_count <= 54,
            w.pos,
            "input owner count",
        )?;
        self.obligation.write(w)?;
        self.known_decision.write(w)?;
        self.after_ownership_summary.write(w)?;
        w.word(self.associated_owner_count)
    }
}
impl Record for InputOwner {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            resource: Field::read(r)?,
            resource_kind: Field::read(r)?,
            after_disposition: Field::read(r)?,
            obligation: Field::read(r)?,
            before_holder: Field::read(r)?,
            after_holder: Field::read(r)?,
        })
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        self.resource.write(w)?;
        self.resource_kind.write(w)?;
        self.after_disposition.write(w)?;
        self.obligation.write(w)?;
        self.before_holder.write(w)?;
        self.after_holder.write(w)
    }
}
fn env_tags(e: &Environment, at: usize) -> Result<(), CodecError> {
    ensure(
        e.work_kind <= 3
            && matches!(e.cleanup, 1..=4 | 255)
            && matches!(e.commit_class, 0..=8 | 255),
        at,
        "environment container tag",
    )?;
    let valid = match e.work_kind {
        0 => e.work_class == 255,
        1 => e.work_class <= 10,
        2 => e.work_class <= 9,
        3 => e.work_class <= 4,
        _ => false,
    };
    ensure(valid, at, "environment work tag")
}
impl Record for Environment {
    fn read_body(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        ensure(r.byte()? == 1, r.pos, "environment schema")?;
        let kind = Field::read(r)?;
        let fact = Field::read(r)?;
        let work_kind = r.byte()?;
        let work_class = r.byte()?;
        let cleanup = r.byte()?;
        let commit_class = r.byte()?;
        r.zero(1)?;
        let transaction = Field::read(r)?;
        let receipt = Field::read(r)?;
        let anchor = Field::read(r)?;
        let fresh_engine = Field::read(r)?;
        let mut value = Self {
            kind,
            fact,
            work_kind,
            work_class,
            cleanup,
            commit_class,
            transaction,
            receipt,
            anchor,
            fresh_engine,
            obligations: Records::default(),
            owners: Records::default(),
        };
        env_tags(&value, r.pos)?;
        let n = usize::from(r.word()?);
        read_table(r, &mut value.obligations, n)?;
        let n = usize::from(r.word()?);
        read_table(r, &mut value.owners, n)?;
        Ok(value)
    }
    fn write_body(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        env_tags(self, w.pos)?;
        w.byte(1)?;
        self.kind.write(w)?;
        self.fact.write(w)?;
        w.byte(self.work_kind)?;
        w.byte(self.work_class)?;
        w.byte(self.cleanup)?;
        w.byte(self.commit_class)?;
        w.zero(1)?;
        self.transaction.write(w)?;
        self.receipt.write(w)?;
        self.anchor.write(w)?;
        self.fresh_engine.write(w)?;
        w.word(self.obligations.len() as u16)?;
        write_table(&self.obligations, w)?;
        w.word(self.owners.len() as u16)?;
        write_table(&self.owners, w)
    }
}
impl Field for Environment {
    fn read(r: &mut Reader<'_>) -> Result<Self, CodecError> {
        read_record(r)
    }
    fn write(&self, w: &mut Writer<'_>) -> Result<(), CodecError> {
        write_record(self, w)
    }
}
pub(super) fn counts(s: &State) -> [usize; 16] {
    [
        s.engines.len(),
        s.session_incarnations.len(),
        s.actors.len(),
        s.prepared_slots.len(),
        s.issued_statements.len(),
        s.issued_results.len(),
        s.private_descriptors.len(),
        s.contexts.len(),
        s.transactions.len(),
        s.obligations.len(),
        s.bindings.len(),
        s.receipts.len(),
        s.owners.len(),
        s.facts.len(),
        s.fact_obligations.len(),
        s.fact_owners.len(),
    ]
}
pub(super) fn read_sections(
    r: &mut Reader<'_>,
    s: &mut State,
    n: [usize; 16],
) -> Result<(), CodecError> {
    read_table(r, &mut s.engines, n[0])?;
    read_table(r, &mut s.session_incarnations, n[1])?;
    read_table(r, &mut s.actors, n[2])?;
    read_table(r, &mut s.prepared_slots, n[3])?;
    read_table(r, &mut s.issued_statements, n[4])?;
    read_table(r, &mut s.issued_results, n[5])?;
    read_table(r, &mut s.private_descriptors, n[6])?;
    read_table(r, &mut s.contexts, n[7])?;
    read_table(r, &mut s.transactions, n[8])?;
    read_table(r, &mut s.obligations, n[9])?;
    read_table(r, &mut s.bindings, n[10])?;
    read_table(r, &mut s.receipts, n[11])?;
    read_table(r, &mut s.owners, n[12])?;
    read_table(r, &mut s.facts, n[13])?;
    read_table(r, &mut s.fact_obligations, n[14])?;
    read_table(r, &mut s.fact_owners, n[15])?;
    Ok(())
}
pub(super) fn write_sections(s: &State, w: &mut Writer<'_>) -> Result<(), CodecError> {
    write_table(&s.engines, w)?;
    write_table(&s.session_incarnations, w)?;
    write_table(&s.actors, w)?;
    write_table(&s.prepared_slots, w)?;
    write_table(&s.issued_statements, w)?;
    write_table(&s.issued_results, w)?;
    write_table(&s.private_descriptors, w)?;
    write_table(&s.contexts, w)?;
    write_table(&s.transactions, w)?;
    write_table(&s.obligations, w)?;
    write_table(&s.bindings, w)?;
    write_table(&s.receipts, w)?;
    write_table(&s.owners, w)?;
    write_table(&s.facts, w)?;
    write_table(&s.fact_obligations, w)?;
    write_table(&s.fact_owners, w)?;
    Ok(())
}
