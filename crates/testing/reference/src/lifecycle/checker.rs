//! Independent relational edge checker for the DEV4 P5 lifecycle model (D4-LM-MODEL-001).
//!
//! `verify_edge` decides whether one observed `(pre, input, edge, post)` is allowed. It
//! recomputes the expected class, response, context event, controls, consumed parts,
//! authenticated targets and the exact post-state from the specification and the pre-state
//! only. It shares record declarations with the model but never calls the model, its helpers or
//! the input assembler. The product path allocates nothing on the heap, does not recurse and
//! never clones a `State`: the post-state is compared table by table with `pre ⊕ delta`.
//!
//! Environment exclusions are judged as well: an `Excluded(rule)` edge is accepted only when
//! the admitted input really matches an exclusion condition, and then nothing may change.
//! Where several exclusion conditions hold for one admitted input the contract fixes no
//! priority, so any applicable rule is accepted.
//!
//! The supplied Env1 is compared with an independently recomputed recipe (wire-v2 §3–§4 and
//! the assembler contract); rows are compared as sets. Any difference justifies
//! `Excluded(Envelope)`; for Recovery, row differences are `RecoverySet` and fact/fresh-engine
//! identity differences also justify `RecoveryPre` (see below). Owner/obligation updates of
//! the expected post-state come from the recipe, never from the supplied rows or the post.
//!
//! A full fixed checker table yields `CheckFailure::CheckerCapacity`: no verdict, distinct
//! from an unreadable pre-state. Product-path sorts are in place and unstable (no buffer).
//!
//! # Owner decisions applied (2026-10-25)
//!
//! - Authenticated targets (v5 §1): for BindAll/Execute/CloseStatement/InspectStatement and
//!   Fetch/ParkFetch/CloseResult, every rejection after the child gate passed carries
//!   `Some(Authenticated)`: statement-targeted = statement identity plus the transaction the
//!   API selects (wire-v2 §4: Execute the caller's pre-state transaction, CloseStatement the
//!   slot descriptor's transaction, BindAll/InspectStatement none); result-targeted = result
//!   identity plus its descriptor transaction. A child-authentication failure is
//!   `Some(default)`; every other edge has `None`.
//! - Recovery ends each confirmed obligation's transaction but changes no `completion_fact`
//!   (a fence-moved peer keeps `None`): the recovery fact is engine history without a
//!   transaction, and the obligation's `recovery_fact` records the end (revised decision).
//! - Recovery exclusions: a failed ENV-RECOVERY-PRE precondition (old engine not Fenced, a
//!   shell not Closed, a parked call, no fresh engine within the profile) is `RecoveryPre`;
//!   a fact/fresh-engine identity mismatch accepts both `RecoveryPre` and `Envelope`; any other
//!   header byte is `Envelope` only; obligation/owner rows and the four mutations are
//!   `RecoverySet`.
//! - CloseSession Incomplete keeps the session Open like Rollback Incomplete (see below).
//!
//! # Interpretation choices (contract open; decided from the sources)
//!
//! - Excluded edges carry no response, no context event, default controls, zero consumption
//!   and no authenticated targets; several applicable exclusion rules are all accepted.
//! - The response decision fields come from the call's selected transaction once its child is
//!   authenticated (Execute: caller T or the created T; Fetch/CloseResult/ParkFetch: descriptor
//!   T; CloseStatement: slot descriptor T; session family: caller T; continuation: parked T),
//!   for rejections too; otherwise from the caller's post transaction.
//! - `EngineFenced` (gate rejection, Reopen, Resume fence terminal) reports the attached
//!   engine's `first_cause` as `engine_cause`.
//! - Every non-success code (all but Ok/Eof/IgnoredCompleted) carries its Diagnostic, including
//!   the control-plane codes NoPendingOperation/ContinuationKind/InvalidContext.
//! - Fetch-like rows/complete: Fetch, ParkFetch and ResumeFetch rejections/cached/completions
//!   are `0/1`·`false/true`; the ParkFetch park success and control-plane replies are N/A.
//! - Inspect and Reopen successes use certainty Known; Recovery success, like CommitFact,
//!   certainty NotApplicable.
//! - First fence: current public results of the engine retire as EngineFence (slot counter +1,
//!   ResultOpen slot → ReadyBound, `active_public_result` cleared); descriptors keep state and
//!   position with `blocked_by_fence`; the cause cursor is not turned into an Error terminal.
//!   Peers get `life = RecoveryPending` and their new obligation link only.
//! - CloseSession Incomplete (owner decision 2026-10-25): like Rollback Incomplete the session
//!   stays Open with its RecoveryPending transaction; only first-fence effects apply. Every
//!   admitted Complete clears the caller's transaction and parked links.
//! - CloseStatement that closes an Open cursor of an implicit read-only transaction ends that
//!   transaction (as CloseResult does).
//! - Rollback/whole abort of an implicit read-only transaction ends it without an Aborted
//!   decision; Execute/CommitAtomic/Finish aborts keep commit progress unchanged.
//! - Obligation `disposition` is derived from all RW/Recovery owners of its transaction;
//!   `latest_fact` advances whenever the obligation row is in a consumed envelope.
//! - Fact `next_work` is NotConsumed for Execute facts, atomic terminal controls, pending
//!   continuation controls and Fetch UndoIncomplete; fact `context` of CommitFact is the parked
//!   commit context; fact `receipt`/`anchor` are recorded only when Receipt4 is consumed.
//! - Finish NCR on an already Fenced engine reports engine/origin cause PeerFence and sets the
//!   transaction decision/knowledge to NCR; Finish Unknown/PublicationPending there is a
//!   count-0 cause update of the existing obligation.
//! - Bind payload bytes and the outcome/deadline correspondence of Park/BeginCommit are not
//!   re-validated; an entry-class outcome without its entry bit is `Excluded(Envelope)`.
mod delta;
mod expect;
mod fence;
mod gates;
mod ident;
#[cfg(test)]
pub(super) mod materialize;
mod owners;
mod result;
mod session;
mod shared;
mod statement;
mod txn;

use super::input_schema::ResolvedInput;
use super::schema::{Api, State};
use super::{Edge, EdgeClass};
use expect::{Expect, bit};
use gates::Cx;

/// The part of an observed edge that the specification does not allow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckFailure {
    /// The pre-state lacks a reference this check needs (not a reachable state).
    PreState(&'static str),
    /// The resolved input lies outside the frozen raw profile.
    Input(&'static str),
    /// A fixed checker table (expected delta or Env1 recipe) is full; no verdict was reached.
    CheckerCapacity(&'static str),
    /// Wrong edge class, including a missing or unjustified environment exclusion.
    Class(&'static str),
    /// Response fields differ from the expected response row.
    Response(&'static str),
    /// The fresh/parked/completed context event differs.
    ContextEvent(&'static str),
    /// Entry checks or observed controls differ.
    Controls(&'static str),
    /// Consumed environment parts differ.
    ConsumedParts(&'static str),
    /// Authenticated targets differ (present only on rejections at or after the child gate).
    Authenticated(&'static str),
    /// A post-state table differs from `pre ⊕ expected change`.
    PostState {
        /// Affected state section.
        table: &'static str,
        /// What differs.
        reason: &'static str,
    },
}

/// Decide whether `edge` and `post` are the specified result of `input` at `pre`.
///
/// # Errors
///
/// Returns the first part of the observation that the specification does not allow.
pub fn verify_edge(
    pre: &State,
    input: &ResolvedInput,
    edge: &Edge,
    post: &State,
) -> Result<(), CheckFailure> {
    let mut exp = Expect::new(pre);
    let cx = Cx { pre, input };
    build(&cx, &mut exp)?;
    if exp.excluded != 0 {
        return check_excluded(&exp, edge, pre, post);
    }
    exp.finish(pre);
    if edge.class != exp.class {
        return Err(CheckFailure::Class(match edge.class {
            EdgeClass::Excluded(_) => "legal input hidden as an environment exclusion",
            _ => "edge class",
        }));
    }
    compare_response(&exp, edge)?;
    if edge.context_event != exp.context_event {
        return Err(CheckFailure::ContextEvent("context event"));
    }
    if edge.controls != exp.controls {
        return Err(CheckFailure::Controls("entry or observed controls"));
    }
    if edge.consumed_parts != exp.consumed {
        return Err(CheckFailure::ConsumedParts("consumed environment parts"));
    }
    if edge.authenticated != exp.authenticated {
        return Err(CheckFailure::Authenticated("authenticated targets"));
    }
    delta::compare_post(pre, post, &exp.delta)
}

fn build(cx: &Cx, exp: &mut Expect) -> Result<(), CheckFailure> {
    match cx.input.api {
        Api::Prepare => statement::prepare(cx, exp),
        Api::BindAll => statement::bind(cx, exp),
        Api::Execute => statement::execute(cx, exp),
        Api::Fetch => result::fetch(cx, exp),
        Api::CloseResult => result::close_result(cx, exp),
        Api::CloseStatement => statement::close_statement(cx, exp),
        Api::Begin => txn::begin(cx, exp),
        Api::SetAutocommit => txn::set_autocommit(cx, exp),
        Api::CommitAtomic => txn::commit_atomic(cx, exp),
        Api::Rollback => txn::rollback(cx, exp),
        Api::CloseSession => session::close_session(cx, exp),
        Api::ReopenSession => session::reopen(cx, exp),
        Api::InspectStatement => statement::inspect_statement(cx, exp),
        Api::InspectSession => session::inspect_session(cx, exp),
        Api::SignalContext => session::signal(cx, exp),
        Api::ParkFetch => result::park(cx, exp),
        Api::ResumeFetch => result::resume(cx, exp),
        Api::BeginCommit => txn::begin_commit(cx, exp),
        Api::CommitFact => txn::commit_fact(cx, exp),
        Api::FinishCommit => txn::finish(cx, exp),
        Api::RecoveryConfirmedFreshEngine => session::recovery(cx, exp),
    }
}

/// An exclusion carries no response, no context event, default controls, no consumption,
/// and leaves the whole state unchanged.
fn check_excluded(
    exp: &Expect,
    edge: &Edge,
    pre: &State,
    post: &State,
) -> Result<(), CheckFailure> {
    match edge.class {
        EdgeClass::Excluded(rule) if exp.excluded & bit(rule) != 0 => {}
        EdgeClass::Excluded(_) => {
            return Err(CheckFailure::Class("exclusion rule does not apply"));
        }
        _ => return Err(CheckFailure::Class("environment exclusion not applied")),
    }
    if edge.response.is_some() {
        return Err(CheckFailure::Response("excluded edge carries a response"));
    }
    if edge.context_event.is_some() {
        return Err(CheckFailure::ContextEvent(
            "excluded edge records a context",
        ));
    }
    if edge.controls != Default::default() {
        return Err(CheckFailure::Controls("excluded edge records controls"));
    }
    if edge.consumed_parts != 0 {
        return Err(CheckFailure::ConsumedParts("excluded edge consumed a fact"));
    }
    if edge.authenticated.is_some() {
        return Err(CheckFailure::Authenticated(
            "excluded edge authenticated targets",
        ));
    }
    delta::unchanged(pre, post)
}

fn compare_response(exp: &Expect, edge: &Edge) -> Result<(), CheckFailure> {
    let (Some(e), Some(a)) = (exp.response.as_ref(), edge.response.as_ref()) else {
        return match (exp.response.is_some(), edge.response.is_some()) {
            (true, false) => Err(CheckFailure::Response("missing response")),
            (false, true) => Err(CheckFailure::Response("unexpected response")),
            _ => Ok(()),
        };
    };
    let field = |same: bool, name: &'static str| {
        if same {
            Ok(())
        } else {
            Err(CheckFailure::Response(name))
        }
    };
    field(a.code == e.code, "code")?;
    field(a.phase == e.phase, "phase")?;
    field(a.statement_effect == e.statement_effect, "statement effect")?;
    field(a.certainty == e.certainty, "certainty")?;
    field(a.retry == e.retry, "retry")?;
    field(a.engine_cause == e.engine_cause, "engine cause")?;
    field(a.origin_cause == e.origin_cause, "origin cause")?;
    field(a.diagnostic == e.diagnostic, "diagnostic")?;
    field(a.rows == e.rows, "rows")?;
    field(a.complete == e.complete, "complete")?;
    field(a.affected_count == e.affected_count, "affected count")?;
    field(
        a.returned_statement == e.returned_statement,
        "returned statement",
    )?;
    field(a.returned_result == e.returned_result, "returned result")?;
    field(
        a.public_transaction == e.public_transaction,
        "public transaction",
    )?;
    field(
        a.transaction_decision == e.transaction_decision,
        "transaction decision",
    )?;
    field(a.client_knowledge == e.client_knowledge, "client knowledge")?;
    field(a.commit_progress == e.commit_progress, "commit progress")?;
    field(a.completion == e.completion, "completion receipt")?;
    field(a.delivery == e.delivery, "delivery")?;
    field(
        a.recovery_obligations == e.recovery_obligations,
        "recovery obligations",
    )
}
