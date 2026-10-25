//! Fetch, ParkFetch, ResumeFetch and CloseResult transitions.
use super::{Exclusion, Flow, INVALID, OriginCause, Run, Step};
use crate::lifecycle::schema::{
    Access, Api, Cleanup, CompletionPhase, Decision, EngineAdmission, EngineCause, NextWork,
    ObligationReason, ParkedContext, Phase, ResultState, Retirement, Retry, ReturnCode,
    StatementEffect,
};

/// What a fetch-like call applies, after controls and fence gates.
#[derive(Clone, Copy)]
enum Work {
    Rows,
    Eof,
    ReadError,
    Conflict,
    Buffer,
    /// Pending or atomic Cancelled/Timeout: cleanup only.
    Control(ReturnCode),
}

impl Run<'_> {
    pub(super) fn fetch(&mut self, park: bool) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        let Some(result) = self.authenticate_result()? else {
            return self.reject_handle();
        };
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let d = self.descriptor_of(result.id.nonce)?;
        match d.state {
            ResultState::Closed => return self.reject(ReturnCode::HandleClosed),
            ResultState::Eof => {
                self.complete_fresh()?;
                let mut response = self.success(Phase::Fetch, StatementEffect::NoEffect);
                response.code = ReturnCode::Eof;
                let response = Self::fetch_response(response, 0, true);
                return self.respond(response, d.transaction.map(|t| t.0));
            }
            ResultState::Error => {
                self.complete_fresh()?;
                let diagnostic = d.terminal.ok_or(INVALID)?;
                let mut response = self.error(
                    diagnostic.code,
                    diagnostic.phase,
                    diagnostic.effect,
                    diagnostic.retry,
                    diagnostic.origin_cause,
                );
                response.diagnostic = Some(diagnostic);
                response.engine_cause = diagnostic.engine_cause;
                response.certainty = diagnostic.certainty;
                let response = Self::fetch_response(response, 0, false);
                return self.respond(response, d.transaction.map(|t| t.0));
            }
            ResultState::Open => {}
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        let transaction = d.transaction.map(|t| t.0);
        if park {
            let payload = ParkedContext {
                api: Api::ParkFetch,
                original_deadline: self.input.original_deadline,
                cancel_supplied_or_signalled: false,
                deadline_reached: false,
                cancel_observed_at_completion: false,
                deadline_observed_at_completion: false,
                completion_phase: CompletionPhase::NotCompleted,
                statement: Some(d.statement),
                result: Some(d.result),
                transaction: d.transaction,
            };
            self.park(payload, true)?;
            let response = self.success(Phase::Fetch, StatementEffect::NoEffect);
            return self.respond(response, transaction);
        }
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(rule) = self.envelope_transaction(match transaction {
            Some(t) => Some(self.transaction_identity(t)?),
            None => None,
        }) {
            return Ok(Flow::Exclude(rule));
        }
        let (work, complete) = match self.input.outcome {
            0 => (Work::Rows, true),
            1 => (Work::Eof, true),
            2 => (Work::Buffer, true),
            5 => (Work::Control(ReturnCode::Cancelled), true),
            6 => (Work::Control(ReturnCode::Timeout), true),
            7 => (Work::ReadError, true),
            8 => (Work::Conflict, true),
            9 => (Work::Conflict, false),
            _ => return Ok(Flow::Exclude(Exclusion::Envelope)),
        };
        self.apply_fetch(result.id.nonce, transaction, work, complete)
    }

    pub(super) fn resume(&mut self) -> Step<Flow> {
        let (context, payload) = match self.pending(Api::ParkFetch)? {
            Ok(found) => found,
            Err(code) => return self.control_plane(code),
        };
        self.context = Some(context);
        let result = payload.result.ok_or(INVALID)?.0;
        let transaction = payload.transaction.map(|t| t.0);
        let caller = self.caller()?;
        let d = self.descriptor_of(result)?;
        let fenced = d.blocked_by_fence
            || self.edit.engine(caller.engine.0)?.admission != EngineAdmission::Admitting;
        if fenced {
            // The original call ends as a fenced terminal; work and cleanup are not consumed.
            self.complete_parked(context)?;
            let response = self.error(
                ReturnCode::EngineFenced,
                Phase::Fetch,
                StatementEffect::NoEffect,
                Retry::AfterRecovery,
                OriginCause::None,
            );
            let response = Self::fetch_response(response, 0, false);
            return self.respond(response, transaction);
        }
        let work_index = self.input.outcome / 2;
        let complete = self.input.outcome.is_multiple_of(2);
        let work = if payload.cancel_supplied_or_signalled {
            self.edge.controls.observed_cancel = true;
            Work::Control(ReturnCode::Cancelled)
        } else if payload.deadline_reached {
            self.edge.controls.observed_deadline = true;
            Work::Control(ReturnCode::Timeout)
        } else {
            match (work_index, complete) {
                (0, true) => Work::Rows,
                (1, true) => Work::Eof,
                (2, _) => Work::ReadError,
                (3, _) => Work::Conflict,
                (4, true) => Work::Buffer,
                _ => return Ok(Flow::Exclude(Exclusion::ResumeWork)),
            }
        };
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(rule) = self.envelope_transaction(match transaction {
            Some(t) => Some(self.transaction_identity(t)?),
            None => None,
        }) {
            return Ok(Flow::Exclude(rule));
        }
        self.apply_fetch(result, transaction, work, complete)
    }

    /// Shared Fetch/Resume application of one work class and cleanup.
    fn apply_fetch(
        &mut self,
        result: u16,
        transaction: Option<u16>,
        work: Work,
        complete: bool,
    ) -> Step<Flow> {
        let read_write = match transaction {
            Some(t) => self.edit.transaction(t)?.access == Access::ReadWrite,
            None => false,
        };
        if !complete && !read_write {
            return Ok(Flow::Exclude(Exclusion::ReadOnlyCleanup));
        }
        let resume = self.input.api == Api::ResumeFetch;
        let next = |w: Work| match w {
            Work::Rows => NextWork::Rows,
            Work::Eof => NextWork::Eof,
            Work::ReadError => NextWork::ReadError,
            Work::Conflict => NextWork::Conflict,
            Work::Buffer => NextWork::RowBufferTooSmall,
            Work::Control(_) => NextWork::NotConsumed,
        };
        if !complete {
            // Cleanup incomplete: the RW transaction enters recovery; the cursor is not released.
            let parts = match work {
                Work::Control(_) => 2,
                _ => 1 | 2,
            };
            self.fence(
                transaction,
                EngineCause::UndoIncomplete,
                Decision::NotCommittedRecoveryRequired,
            )?;
            self.consume(
                Cleanup::Incomplete,
                next(work),
                parts,
                transaction,
                Some(ObligationReason::KnownNotCommittedUndoPending),
                false,
            )?;
            self.finish_call(resume)?;
            let response = self.admitted_error(
                ReturnCode::RecoveryRequired,
                Phase::Fetch,
                StatementEffect::RecoveryRequired,
                Retry::AfterRecovery,
                EngineCause::UndoIncomplete,
                OriginCause::UndoIncomplete,
            );
            let response = Self::fetch_response(response, 0, false);
            return self.respond(response, transaction);
        }
        match work {
            Work::Rows => {
                self.edit.descriptor_mut(result)?.position += 1;
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::Rows,
                    1,
                    transaction,
                    None,
                    false,
                )?;
                self.finish_call(resume)?;
                let response = self.success(Phase::Fetch, StatementEffect::NoEffect);
                let response = Self::fetch_response(response, 1, false);
                self.respond(response, transaction)
            }
            Work::Buffer => {
                self.consume(
                    Cleanup::NotConsumed,
                    NextWork::RowBufferTooSmall,
                    1,
                    transaction,
                    None,
                    false,
                )?;
                self.finish_call(resume)?;
                let response = self.error(
                    ReturnCode::RowBufferTooSmall,
                    Phase::Fetch,
                    StatementEffect::NoEffect,
                    Retry::Operation,
                    OriginCause::None,
                );
                let response = Self::fetch_response(response, 0, false);
                self.respond(response, transaction)
            }
            Work::Eof => {
                self.finish_cursor(result, ResultState::Eof, None)?;
                if let Some(t) = transaction {
                    self.end_if_implicit(t)?;
                }
                self.consume(
                    Cleanup::Complete,
                    NextWork::Eof,
                    1 | 2,
                    transaction,
                    None,
                    false,
                )?;
                self.finish_call(resume)?;
                let mut response = self.success(Phase::Fetch, StatementEffect::CursorReleased);
                response.code = ReturnCode::Eof;
                let response = Self::fetch_response(response, 0, true);
                self.respond(response, transaction)
            }
            Work::ReadError | Work::Control(_) => {
                let (code, origin, parts, applied) = match work {
                    Work::Control(code) => (
                        code,
                        if code == ReturnCode::Cancelled {
                            OriginCause::Cancelled
                        } else {
                            OriginCause::Timeout
                        },
                        2,
                        NextWork::NotConsumed,
                    ),
                    _ => (
                        ReturnCode::ReadError,
                        OriginCause::ReadError,
                        1 | 2,
                        NextWork::ReadError,
                    ),
                };
                if matches!(work, Work::Control(ReturnCode::Cancelled)) {
                    self.edge.controls.observed_cancel = true;
                } else if matches!(work, Work::Control(_)) {
                    self.edge.controls.observed_deadline = true;
                }
                let terminal = Self::terminal(code, Phase::Fetch, origin);
                self.finish_cursor(result, ResultState::Error, Some(terminal))?;
                if let Some(t) = transaction {
                    self.end_if_implicit(t)?;
                }
                self.consume(Cleanup::Complete, applied, parts, transaction, None, false)?;
                self.finish_call(resume)?;
                let response = self.error(
                    code,
                    Phase::Fetch,
                    StatementEffect::CursorReleased,
                    Retry::Operation,
                    origin,
                );
                let response = Self::fetch_response(response, 0, false);
                self.respond(response, transaction)
            }
            Work::Conflict => {
                // Whole abort retires the current Open result and returns its statement.
                self.finish_cursor(result, ResultState::Closed, None)?;
                self.retire_result(result, Retirement::Rollback, true)?;
                if let Some(t) = transaction {
                    self.abort_transaction(t)?;
                }
                self.consume(
                    Cleanup::Complete,
                    NextWork::Conflict,
                    1 | 2,
                    transaction,
                    None,
                    false,
                )?;
                self.finish_call(resume)?;
                let response = self.error(
                    ReturnCode::Conflict,
                    Phase::Fetch,
                    StatementEffect::CursorReleased,
                    Retry::WholeTransactionDecision,
                    OriginCause::Conflict,
                );
                let response = Self::fetch_response(response, 0, false);
                self.respond(response, transaction)
            }
        }
    }

    /// A fresh call completes its new context; a continuation completes the parked one.
    fn finish_call(&mut self, resume: bool) -> Step<()> {
        if resume {
            let context = self.context.ok_or(INVALID)?;
            self.complete_parked(context)
        } else {
            self.complete_fresh()
        }
    }

    pub(super) fn close_result(&mut self) -> Step<Flow> {
        if let Some(rule) = self.fresh_context()? {
            return Ok(Flow::Exclude(rule));
        }
        let Some(result) = self.authenticate_result()? else {
            return self.reject_handle();
        };
        if let Some(code) = self.session_gates(false)? {
            return self.reject(code);
        }
        let d = self.descriptor_of(result.id.nonce)?;
        let transaction = d.transaction.map(|t| t.0);
        if d.state == ResultState::Closed {
            self.complete_fresh()?;
            let response = self.success(Phase::Close, StatementEffect::NoEffect);
            return self.respond(response, transaction);
        }
        if let Some(code) = self.entry_controls() {
            return self.reject(code);
        }
        if let Err(rule) = self.require_environment() {
            return Ok(Flow::Exclude(rule));
        }
        if let Some(rule) = self.envelope_transaction(match transaction {
            Some(t) => Some(self.transaction_identity(t)?),
            None => None,
        }) {
            return Ok(Flow::Exclude(rule));
        }
        let was_open = d.state == ResultState::Open;
        self.finish_cursor(result.id.nonce, ResultState::Closed, None)?;
        if was_open && let Some(t) = transaction {
            let origin = self.edit.transaction(t)?.origin;
            if origin == crate::lifecycle::schema::TransactionOrigin::ImplicitReadOnly {
                self.end_transaction(t, None)?;
            }
        }
        self.consume(
            Cleanup::Complete,
            NextWork::NotConsumed,
            2,
            transaction,
            None,
            false,
        )?;
        self.complete_fresh()?;
        let effect = if was_open {
            StatementEffect::CursorReleased
        } else {
            StatementEffect::NoEffect
        };
        let response = self.success(Phase::Close, effect);
        self.respond(response, transaction)
    }
}
