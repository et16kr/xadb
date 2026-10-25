//! Independent relational checker cases; every case first runs in a 256KiB worker.
//!
//! Expected edges are read directly from the hand-authored `literals.json` with this file's own
//! JSON expansion (states and inputs through the frozen test-only literal reader). Targeted
//! known-bad variants are derived from those literals or from hand-built post-states; no model
//! transition produces an expectation here. Deeper states for the self-walk and for rejection
//! variants on fence/continuation paths are reached through the checker's own test-only
//! materialized expectation; those cases check self-consistency, codec validity of every
//! expected post-state and agreement with the assembler's Env1, not model agreement.
//!
//! `XADB_CHECKER_WALKS` / `XADB_CHECKER_WALK_LENGTH` resize the self-walk (default 58 × 8);
//! `XADB_CHECKER_CLASSES` prints its per-family class coverage.
use super::checker::materialize::{expected, frame_bytes};
use super::literal::{Literals, SOURCE};
use super::workspace::Workspace;
use super::*;
use serde_json::{Map, Value};
use std::fmt::Debug;

fn worker(name: &str, case: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .name(name.into())
        .stack_size(256 * 1024)
        .spawn(case)
        .expect("spawn 256KiB checker worker")
        .join()
        .expect("checker worker");
}

fn vocab<T: TryFrom<u8> + Debug>(name: &str) -> T {
    (0..=u8::MAX)
        .filter_map(|tag| T::try_from(tag).ok())
        .find(|v| format!("{v:?}") == name)
        .unwrap_or_else(|| panic!("unknown vocabulary name {name}"))
}

struct Fixture {
    root: Value,
    literals: Literals,
}

impl Fixture {
    fn new() -> Self {
        Self {
            root: serde_json::from_str(SOURCE).expect("literal JSON"),
            literals: Literals::new().expect("literal reader"),
        }
    }

    fn identity(&self, v: &Value) -> Identity {
        let alias = v.as_str().expect("identity alias");
        let t = &self.root["identities"][&alias[1..]];
        let n = |i: usize| t[i].as_u64().expect("identity field") as u16;
        Identity {
            kind: IdentityKind::try_from(n(0) as u8).unwrap(),
            actor: n(1) as u8,
            slot: n(2) as u8,
            engine: n(3),
            session: n(4),
            nonce: n(5),
            generation: n(6),
        }
    }

    fn opt_identity(&self, v: &Value) -> Option<Identity> {
        (!v.is_null()).then(|| self.identity(v))
    }

    /// Exact base/remove/put expansion of one named literal plus `$value` substitution.
    fn named(&self, section: &str, name: &str) -> Value {
        let source = &self.root[section][name];
        let mut value = match source.get("base") {
            Some(base) => self.named(section, base.as_str().unwrap()),
            None => source.clone(),
        };
        if source.get("base").is_some()
            && let Some(put) = source.get("put")
        {
            put_all(&mut value, put);
        }
        self.expand(value)
    }

    fn expand(&self, value: Value) -> Value {
        match value {
            Value::Object(fields) if fields.contains_key("$value") => {
                let name = fields["$value"].as_str().unwrap();
                self.expand(self.root["values"][name].clone())
            }
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(k, v)| (k, self.expand(v)))
                    .collect::<Map<_, _>>(),
            ),
            Value::Array(items) => {
                Value::Array(items.into_iter().map(|v| self.expand(v)).collect())
            }
            other => other,
        }
    }

    fn diagnostic(&self, v: &Value) -> Diagnostic {
        let s = |k: &str| v[k].as_str().unwrap();
        Diagnostic {
            code: vocab(s("code")),
            phase: vocab(s("phase")),
            effect: vocab(s("effect")),
            certainty: vocab(s("certainty")),
            retry: vocab(s("retry")),
            engine_cause: vocab(s("engine_cause")),
            origin_cause: vocab(s("origin_cause")),
        }
    }

    fn response(&self, v: &Value) -> Response {
        let s = |k: &str| {
            v[k].as_str()
                .unwrap_or_else(|| panic!("response field {k}"))
        };
        let mut obligations = Records::default();
        for o in v["recovery_obligations"].as_array().unwrap() {
            obligations.try_push(self.identity(o)).unwrap();
        }
        Response {
            code: vocab(s("code")),
            phase: vocab(s("phase")),
            statement_effect: vocab(s("statement_effect")),
            certainty: vocab(s("certainty")),
            retry: vocab(s("retry")),
            engine_cause: vocab(s("engine_cause")),
            origin_cause: vocab(s("origin_cause")),
            diagnostic: (!v["diagnostic"].is_null()).then(|| self.diagnostic(&v["diagnostic"])),
            rows: match &v["rows"] {
                Value::Number(n) => Rows::Count(n.as_u64().unwrap() as u8),
                _ => Rows::NotApplicable,
            },
            complete: match &v["complete"] {
                Value::Bool(true) => Complete::Yes,
                Value::Bool(false) => Complete::No,
                _ => Complete::NotApplicable,
            },
            affected_count: v["affected_count"].as_u64().map(|n| n as u8),
            returned_statement: self.opt_identity(&v["returned_statement"]),
            returned_result: self.opt_identity(&v["returned_result"]),
            public_transaction: match s("public_transaction") {
                "Idle" => PublicTransaction::Idle,
                "Active" => PublicTransaction::Active,
                "RecoveryPending" => PublicTransaction::RecoveryPending,
                "Ended" => PublicTransaction::Ended,
                other => panic!("public transaction {other}"),
            },
            transaction_decision: vocab(s("transaction_decision")),
            client_knowledge: vocab(s("client_knowledge")),
            commit_progress: vocab(s("commit_progress")),
            completion: self.opt_identity(&v["completion"]),
            delivery: vocab(s("delivery")),
            recovery_obligations: obligations,
        }
    }

    fn check(v: &Value) -> Check {
        match v {
            Value::Bool(true) => Check::Yes,
            Value::Bool(false) => Check::No,
            _ => Check::NotChecked,
        }
    }

    /// Expected edge of one literal, with its pre/input/post names.
    fn edge(&self, name: &str) -> (String, String, String, Edge) {
        let e = &self.root["edges"][name];
        let text = |k: &str| e[k].as_str().unwrap().to_string();
        let class = match e["class"].as_str().unwrap() {
            "CheckedAdmitted" => EdgeClass::Admitted,
            "CheckedRejected" => EdgeClass::Rejected,
            "CheckedNoEffect" => EdgeClass::NoEffect,
            "CheckedNoContextIssue" => EdgeClass::NoContextIssue,
            "CheckedContinuation" => EdgeClass::Continuation,
            other => panic!("edge class {other}"),
        };
        let response = self.response(&self.named("responses", e["response"].as_str().unwrap()));
        let context_event = (!e["context_event"].is_null()).then(|| {
            let c = &e["context_event"];
            ContextEvent {
                identity: self.identity(&c["identity"]),
                before: c["before"].as_str().map(vocab),
                after: vocab(c["after"].as_str().unwrap()),
            }
        });
        let c = &e["controls"];
        let edge = Edge {
            class,
            response: Some(response),
            context_event,
            controls: Controls {
                entry_cancel: Self::check(&c["entry_cancel"]),
                entry_deadline: Self::check(&c["entry_deadline"]),
                observed_cancel: c["observed_cancel"].as_bool().unwrap(),
                observed_deadline: c["observed_deadline"].as_bool().unwrap(),
            },
            consumed_parts: e["consumed_fact_parts"].as_u64().unwrap() as u8,
            authenticated: e.get("authenticated_targets").map(|a| Authenticated {
                statement: self.opt_identity(&a["statement"]),
                result: self.opt_identity(&a["result"]),
                transaction: self.opt_identity(&a["transaction"]),
            }),
        };
        (text("pre"), text("input"), text("post"), edge)
    }

    /// One literal edge loaded into heap workspaces.
    fn load(&self, name: &str) -> Case {
        let (pre_name, input_name, post_name, edge) = self.edge(name);
        let mut pre = Workspace::try_new().unwrap();
        let mut post = Workspace::try_new().unwrap();
        self.literals.state(&pre_name, pre.state_mut()).unwrap();
        self.literals.state(&post_name, post.state_mut()).unwrap();
        let input = self.literals.input(&input_name, None).unwrap();
        Case {
            pre,
            input,
            edge,
            post,
        }
    }

    /// A state written as a base/put delta over a named literal state.
    fn delta_state(&self, delta: &Value, out: &mut State) {
        let mut root = self.root.clone();
        root["states"]
            .as_object_mut()
            .unwrap()
            .insert("checker.variant".into(), delta.clone());
        Literals::from_json(&root.to_string())
            .unwrap()
            .state("checker.variant", out)
            .unwrap();
    }
}

fn put_all(value: &mut Value, put: &Value) {
    for (pointer, replacement) in put.as_object().unwrap() {
        let mut target = &mut *value;
        let parts: Vec<&str> = pointer[1..].split('/').collect();
        for part in &parts[..parts.len() - 1] {
            target = target.get_mut(*part).expect("put parent");
        }
        target
            .as_object_mut()
            .unwrap()
            .insert(parts[parts.len() - 1].into(), replacement.clone());
    }
}

struct Case {
    pre: Workspace,
    input: ResolvedInput,
    edge: Edge,
    post: Workspace,
}

impl Case {
    fn verify(&self) -> Result<(), CheckFailure> {
        verify_edge(self.pre.state(), &self.input, &self.edge, self.post.state())
    }
    fn verify_with(&self, edge: &Edge, post: &State) -> Result<(), CheckFailure> {
        verify_edge(self.pre.state(), &self.input, edge, post)
    }
}

const EDGE_COUNT: usize = 29;

/// Owner decision 2026-10-25 on v5 §1: a rejection after the child gate passed carries the
/// authenticated targets. `sel.entry_cancel` (Fetch EntryCancelled on the authenticated R1)
/// predates the decision; its expected edge gains result R1 and descriptor transaction T1.
fn authenticated_override(fx: &Fixture, name: &str) -> Option<Authenticated> {
    (name == "sel.entry_cancel").then(|| Authenticated {
        statement: None,
        result: Some(fx.identity(&Value::from("@r1s0"))),
        transaction: Some(fx.identity(&Value::from("@t1"))),
    })
}

#[test]
fn literal_edges_are_accepted() {
    worker("checker-literal-edges-256k", || {
        let fx = Fixture::new();
        let names: Vec<String> = fx.root["edges"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(names.len(), EDGE_COUNT);
        let mut failures = Vec::new();
        let mut overridden = 0;
        for name in &names {
            let mut case = fx.load(name);
            if let Some(auth) = authenticated_override(&fx, name) {
                // The literal predates the owner decision and omits the targets.
                assert_eq!(
                    case.verify(),
                    Err(CheckFailure::Authenticated("authenticated targets")),
                    "{name}: unmodified literal"
                );
                case.edge.authenticated = Some(auth);
                overridden += 1;
            }
            if let Err(f) = case.verify() {
                failures.push(format!("{name}: {f:?}"));
            }
        }
        assert!(failures.is_empty(), "rejected literal edges: {failures:#?}");
        let (expect_bytes, plan_bytes) = frame_bytes();
        println!(
            "CHECKER literal_edges accepted={} of {EDGE_COUNT} authenticated_overrides={overridden} expect_bytes={expect_bytes} env_plan_bytes={plan_bytes} stack_budget=262144",
            names.len()
        );
    });
}

#[test]
fn known_bad_mutants_are_rejected() {
    worker("checker-known-bad-256k", || {
        let fx = Fixture::new();
        let mutants = fx.root["mutants"].as_array().unwrap().clone();
        assert_eq!(mutants.len(), 5);
        let mut rejected = 0;
        for m in &mutants {
            let id = m["id"].as_str().unwrap();
            let edge_name = m["first_required_divergence_edge"].as_str().unwrap();
            let case = fx.load(edge_name);
            assert_eq!(case.verify(), Ok(()), "{id} normal control must pass");
            let bad = &m["bad_response"];
            let mut bad_value = fx.named("responses", bad["base"].as_str().unwrap());
            put_all(&mut bad_value, &bad["put"]);
            let bad_value = fx.expand(bad_value);
            let mut bad_edge = case.edge;
            bad_edge.response = Some(fx.response(&bad_value));
            let mut bad_post = Workspace::try_new().unwrap();
            fx.delta_state(&m["bad_post"], bad_post.state_mut());
            let post_changed = bad_post.state() != case.post.state();
            let response_changed = bad_edge.response != case.edge.response;
            assert!(
                post_changed || response_changed,
                "{id} must change something"
            );
            if response_changed {
                assert!(
                    case.verify_with(&bad_edge, case.post.state()).is_err(),
                    "{id} bad response accepted"
                );
                rejected += 1;
            }
            if post_changed {
                assert!(
                    case.verify_with(&case.edge, bad_post.state()).is_err(),
                    "{id} bad post accepted"
                );
                rejected += 1;
            }
            let both = case.verify_with(&bad_edge, bad_post.state());
            assert!(both.is_err(), "{id} bad observation accepted");
            rejected += 1;
            println!("CHECKER mutant={id} rejected={both:?}");
        }
        println!("CHECKER known_bad mutants=5 rejected_observations={rejected}");
    });
}

/// Mutate a copy of the literal post-state and expect a rejection.
fn reject_post(case: &Case, edit: impl FnOnce(&mut State)) -> CheckFailure {
    let mut post = Workspace::try_new().unwrap();
    post.state_mut().copy_from(case.post.state());
    edit(post.state_mut());
    case.verify_with(&case.edge, post.state())
        .expect_err("mutated post-state must be rejected")
}

fn reject_edge(case: &Case, edit: impl FnOnce(&mut Edge)) -> CheckFailure {
    let mut edge = case.edge;
    edit(&mut edge);
    case.verify_with(&edge, case.post.state())
        .expect_err("mutated edge must be rejected")
}

fn owner_mut(s: &mut State, nonce: u16) -> &mut OwnerRecord {
    s.owners
        .as_mut_slice()
        .iter_mut()
        .find(|o| o.id.nonce == nonce)
        .unwrap()
}

fn txn_mut(s: &mut State, nonce: u16) -> &mut TransactionRecord {
    s.transactions
        .as_mut_slice()
        .iter_mut()
        .find(|t| t.id.nonce == nonce)
        .unwrap()
}

#[test]
fn targeted_literal_variants_are_rejected() {
    worker("checker-targeted-variants-256k", || {
        let fx = Fixture::new();
        let mut count = 0;
        let rows = fx.load("sel.rows");
        // Wrong fresh context nonce in the edge and in the post-state.
        let f = reject_edge(&rows, |e| {
            e.context_event.as_mut().unwrap().identity.nonce = 4;
        });
        assert!(matches!(f, CheckFailure::ContextEvent(_)), "{f:?}");
        count += 1;
        let f = reject_post(&rows, |s| {
            let c = s.contexts.as_mut_slice().last_mut().unwrap();
            c.id.nonce = 4;
        });
        assert!(matches!(f, CheckFailure::PostState { .. }), "{f:?}");
        count += 1;
        // Caller context counter not advanced.
        let f = reject_post(&rows, |s| {
            s.next_nonce[IdentityKind::Context as usize - 1] -= 1
        });
        assert_eq!(
            f,
            CheckFailure::PostState {
                table: "header",
                reason: "next nonce counters"
            }
        );
        count += 1;
        // Fact counter not advanced, and the Work fact missing.
        let f = reject_post(&rows, |s| {
            s.next_nonce[IdentityKind::Fact as usize - 1] -= 1
        });
        assert!(matches!(f, CheckFailure::PostState { .. }), "{f:?}");
        count += 1;
        // Position not advanced for a consumed Rows fact.
        let f = reject_post(&rows, |s| {
            s.private_descriptors.as_mut_slice()[0].position = 0
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "private_descriptors",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        // Wrong consumed parts and a forged observed cancel.
        let f = reject_edge(&rows, |e| e.consumed_parts = 3);
        assert!(matches!(f, CheckFailure::ConsumedParts(_)), "{f:?}");
        count += 1;
        let f = reject_edge(&rows, |e| e.controls.observed_cancel = true);
        assert!(matches!(f, CheckFailure::Controls(_)), "{f:?}");
        count += 1;
        // A legal input hidden as an environment exclusion.
        for rule in [
            Exclusion::Envelope,
            Exclusion::ResumeWork,
            Exclusion::ContextFresh,
        ] {
            let mut excluded = Workspace::try_new().unwrap();
            excluded.state_mut().copy_from(rows.pre.state());
            let edge = Edge {
                class: EdgeClass::Excluded(rule),
                ..Edge::default()
            };
            let f = rows
                .verify_with(&edge, excluded.state())
                .expect_err("legal input must not be excluded");
            assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
            count += 1;
        }

        // Cursor owner left Held although the EOF cleanup was consumed.
        let eof = fx.load("sel.eof");
        let f = reject_post(&eof, |s| {
            let o = owner_mut(s, 3);
            o.disposition = OwnerDisposition::Held;
            o.current_holder = o.previous_holder;
            o.previous_holder = None;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "owners",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        // Implicit read-only transaction left Active after EOF.
        let f = reject_post(&eof, |s| txn_mut(s, 1).life = TransactionLife::Active);
        assert!(matches!(f, CheckFailure::PostState { .. }), "{f:?}");
        count += 1;

        // Stale R1 accepted as a fetch of rows on the current R2.
        let stale = fx.load("sel.stale");
        let current = fx.load("sel.current_rows");
        let f = stale
            .verify_with(&current.edge, current.post.state())
            .expect_err("stale result accepted");
        assert!(
            !matches!(f, CheckFailure::PreState(_) | CheckFailure::Input(_)),
            "{f:?}"
        );
        count += 1;
        // Stale rejection must not authenticate a target.
        let f = reject_edge(&stale, |e| e.authenticated = None);
        assert!(matches!(f, CheckFailure::Authenticated(_)), "{f:?}");
        count += 1;

        // Resume timeout must not consume the supplied Rows work.
        let resume = fx.load("sel.resume");
        let f = reject_edge(&resume, |e| e.consumed_parts = 3);
        assert!(matches!(f, CheckFailure::ConsumedParts(_)), "{f:?}");
        count += 1;
        // The continuation reuses the parked identity and ordinal; it issues nothing new.
        let f = reject_post(&resume, |s| {
            let c = s
                .contexts
                .as_mut_slice()
                .iter_mut()
                .find(|c| c.id.nonce == 3)
                .unwrap();
            c.issue_ordinal = 4;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "contexts",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;

        // Rollback Incomplete: owner moved, engine left Admitting, obligation not Outstanding.
        let incomplete = fx.load("rw.rollback_incomplete");
        let f = reject_post(&incomplete, |s| {
            s.engines.as_mut_slice()[0].admission = EngineAdmission::Admitting
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "engines",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_post(&incomplete, |s| {
            let o = owner_mut(s, 1);
            o.disposition = OwnerDisposition::Released;
            o.previous_holder = o.current_holder;
            o.current_holder = None;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "owners",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_edge(&incomplete, |e| {
            e.response.as_mut().unwrap().recovery_obligations.clear();
        });
        assert!(matches!(f, CheckFailure::Response(_)), "{f:?}");
        count += 1;

        // Close of an explicit RW result must not commit the transaction.
        let close = fx.load("rw.close");
        let f = reject_post(&close, |s| {
            let t = txn_mut(s, 1);
            t.decision = Decision::Committed;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "transactions",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        println!("CHECKER targeted_literal_variants rejected={count}");
    });
}

/// Assemble one raw action at a heap state.
fn assemble(state: &State, action: RawAction) -> ResolvedInput {
    let mut out = ResolvedInput::default();
    assemble_input(state, &action, &mut out).expect("assemble");
    out
}

fn raw(api: Api, actor: u8, selector: Option<TargetSelector>, outcome: u8) -> RawAction {
    RawAction {
        api,
        actor,
        selector,
        prepare: None,
        autocommit: None,
        outcome,
    }
}

#[test]
fn environment_exclusions_are_judged() {
    worker("checker-exclusions-256k", || {
        let fx = Fixture::new();
        let mut count = 0;
        let mut pre = Workspace::try_new().unwrap();
        fx.literals.state("sel.prepared", pre.state_mut()).unwrap();
        let pre = pre.state();
        let excluded = |rule| Edge {
            class: EdgeClass::Excluded(rule),
            ..Edge::default()
        };
        // N0 Bind Type: only BindN0 is justified; an admitted Type error is wrong.
        let bind_type = assemble(
            pre,
            raw(Api::BindAll, 0, Some(TargetSelector::Issued(0)), 3),
        );
        assert_eq!(
            verify_edge(pre, &bind_type, &excluded(Exclusion::BindN0), pre),
            Ok(())
        );
        count += 1;
        let f = verify_edge(pre, &bind_type, &excluded(Exclusion::CommitNoWrite), pre).unwrap_err();
        assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
        count += 1;
        let f = verify_edge(pre, &bind_type, &Edge::default(), pre).unwrap_err();
        assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
        count += 1;
        // An excluded edge must not change the state or carry a response.
        let mut changed = Workspace::try_new().unwrap();
        changed.state_mut().copy_from(pre);
        changed.state_mut().next_nonce[IdentityKind::Context as usize - 1] += 1;
        let f = verify_edge(
            pre,
            &bind_type,
            &excluded(Exclusion::BindN0),
            changed.state(),
        )
        .unwrap_err();
        assert!(matches!(f, CheckFailure::PostState { .. }), "{f:?}");
        count += 1;
        let with_response = Edge {
            response: Some(Response::default()),
            ..excluded(Exclusion::BindN0)
        };
        let f = verify_edge(pre, &bind_type, &with_response, pre).unwrap_err();
        assert!(matches!(f, CheckFailure::Response(_)), "{f:?}");
        count += 1;
        // A wrong fresh context is ContextFresh, never an API rejection.
        let mut wrong = assemble(pre, raw(Api::Begin, 0, None, 0));
        if let Some(c) = wrong.context.as_mut() {
            c.0[8] = 6;
        }
        assert_eq!(
            verify_edge(pre, &wrong, &excluded(Exclusion::ContextFresh), pre),
            Ok(())
        );
        count += 1;
        // Execute Unknown on an implicit read-only Select is ExecuteCommit.
        let unknown = assemble(
            pre,
            raw(Api::Execute, 0, Some(TargetSelector::Issued(0)), 9),
        );
        assert_eq!(
            verify_edge(pre, &unknown, &excluded(Exclusion::ExecuteCommit), pre),
            Ok(())
        );
        count += 1;
        // Execute Incomplete on a read-only transaction is ReadOnlyCleanup.
        let incomplete = assemble(
            pre,
            raw(Api::Execute, 0, Some(TargetSelector::Issued(0)), 8),
        );
        assert_eq!(
            verify_edge(pre, &incomplete, &excluded(Exclusion::ReadOnlyCleanup), pre),
            Ok(())
        );
        count += 1;
        // A Signal DeadlineReached to a parked call without a deadline.
        let mut parked = Workspace::try_new().unwrap();
        fx.literals.state("sel.parked", parked.state_mut()).unwrap();
        let ctx = parked
            .state_mut()
            .contexts
            .as_mut_slice()
            .iter_mut()
            .find(|c| c.id.nonce == 3)
            .unwrap();
        if let ContextData::ParkedFetch(p) = &mut ctx.data {
            p.original_deadline = Deadline::Absent;
        }
        let signal = fx.literals.input("sel.signal_parked", None).unwrap();
        assert_eq!(
            verify_edge(
                parked.state(),
                &signal,
                &excluded(Exclusion::SignalNoDeadline),
                parked.state()
            ),
            Ok(())
        );
        count += 1;
        // Unsignalled Resume Rows+Incomplete is ResumeWork (or ReadOnlyCleanup on RO).
        let mut resume_pre = Workspace::try_new().unwrap();
        fx.literals
            .state("sel.parked", resume_pre.state_mut())
            .unwrap();
        let resume = assemble(resume_pre.state(), raw(Api::ResumeFetch, 0, None, 1));
        assert_eq!(
            verify_edge(
                resume_pre.state(),
                &resume,
                &excluded(Exclusion::ResumeWork),
                resume_pre.state()
            ),
            Ok(())
        );
        count += 1;
        // Recovery while the engine is still admitting: RecoveryPre, never RecoverySet alone.
        let recovery = assemble(pre, raw(Api::RecoveryConfirmedFreshEngine, 255, None, 0));
        assert_eq!(
            verify_edge(pre, &recovery, &excluded(Exclusion::RecoveryPre), pre),
            Ok(())
        );
        count += 1;
        println!("CHECKER exclusion_cases={count}");
    });
}

/// Hand-built CloseSession Complete on the already fenced engine of `rw.recovery_pending`:
/// local release, exact handoff of RW3+Recovery to O1, decision preserved.
#[test]
fn fenced_close_preserves_decision() {
    worker("checker-fenced-close-256k", || {
        let fx = Fixture::new();
        let mut pre = Workspace::try_new().unwrap();
        fx.literals
            .state("rw.recovery_pending", pre.state_mut())
            .unwrap();
        let input = assemble(pre.state(), raw(Api::CloseSession, 0, None, 0));
        let mut post = Workspace::try_new().unwrap();
        post.state_mut().copy_from(pre.state());
        let s = post.state_mut();
        let id = |kind, nonce| Identity {
            kind,
            actor: 0,
            slot: 255,
            engine: 1,
            session: 1,
            nonce,
            generation: 0,
        };
        let c5 = id(IdentityKind::Context, 5);
        let f3 = Identity {
            actor: 255,
            session: 0,
            ..id(IdentityKind::Fact, 3)
        };
        let o1 = HolderRef {
            kind: IdentityKind::Obligation,
            nonce: 1,
        };
        let t1 = HolderRef {
            kind: IdentityKind::Transaction,
            nonce: 1,
        };
        let p1 = HolderRef {
            kind: IdentityKind::Statement,
            nonce: 1,
        };
        s.next_nonce[IdentityKind::Context as usize - 1] = 6;
        s.next_nonce[IdentityKind::Fact as usize - 1] = 4;
        s.contexts
            .try_push(ContextRecord {
                id: c5,
                issue_ordinal: 5,
                data: ContextData::Completed,
            })
            .unwrap();
        for n in [1, 2, 3, 6] {
            let o = owner_mut(s, n);
            o.previous_holder = Some(t1);
            o.current_holder = Some(o1);
            o.disposition = OwnerDisposition::Transferred;
            o.transfer_count = 1;
            o.last_fact = Some(FactRef(3));
        }
        for n in [4, 5] {
            let o = owner_mut(s, n);
            o.previous_holder = Some(p1);
            o.current_holder = None;
            o.disposition = OwnerDisposition::Released;
            o.last_fact = Some(FactRef(3));
        }
        let ob = &mut s.obligations.as_mut_slice()[0];
        ob.disposition = OwnershipSummary::AllTransferred;
        ob.latest_fact = Some(FactRef(3));
        let st = &mut s.issued_statements.as_mut_slice()[0];
        st.life = StatementLife::Retired;
        st.retirement = Retirement::SessionClose;
        st.retirement_context = Some(ContextRef(5));
        let slot = &mut s.prepared_slots.as_mut_slice()[0];
        slot.state = SlotState::Vacant;
        slot.statement = None;
        slot.binding = None;
        slot.next_statement_generation += 1;
        s.bindings.as_mut_slice()[0].life = BindingLife::Released;
        let session = &mut s.session_incarnations.as_mut_slice()[0];
        session.lifecycle = Shell::Closed;
        session.closing_context = Some(ContextRef(5));
        let a = &mut s.actors.as_mut_slice()[0];
        a.shell = Shell::Closed;
        a.transaction = None;
        s.facts
            .try_push(FactRecord {
                id: f3,
                kind: FactKind::SessionCleanup,
                api: Api::CloseSession,
                outcome_class: 0,
                cleanup: Cleanup::LocalReleaseAndHandoff,
                decision: Decision::NotCommittedRecoveryRequired,
                delivery: Delivery::NotApplicable,
                next_work: NextWork::NotConsumed,
                consumed_parts: 2,
                engine: EngineRef(1),
                obligation_count: 1,
                owner_count: 6,
                context: Some(ContextRef(5)),
                transaction: Some(TransactionRef(1)),
                receipt: None,
                fresh_engine: None,
                anchor: None,
            })
            .unwrap();
        s.fact_obligations
            .try_push(FactObligation {
                fact: FactRef(3),
                obligation: ObligationRef(1),
                known_decision: Decision::NotCommittedRecoveryRequired,
                disposition: OwnershipSummary::AllTransferred,
                owner_count: 4,
            })
            .unwrap();
        for n in 1..=6u16 {
            let local = matches!(n, 4 | 5);
            s.fact_owners
                .try_push(FactOwner {
                    fact: FactRef(3),
                    resource: ResourceRef(n),
                    disposition: if local {
                        OwnerDisposition::Released
                    } else {
                        OwnerDisposition::Transferred
                    },
                    obligation: (!local).then_some(ObligationRef(1)),
                    before_holder: Some(if local { p1 } else { t1 }),
                    after_holder: (!local).then_some(o1),
                })
                .unwrap();
        }
        let mut obligations = Records::default();
        obligations
            .try_push(id(IdentityKind::Obligation, 1))
            .unwrap();
        let edge = Edge {
            class: EdgeClass::Admitted,
            response: Some(Response {
                code: ReturnCode::Ok,
                phase: Phase::Close,
                statement_effect: StatementEffect::NoEffect,
                certainty: Certainty::Known,
                retry: Retry::NotApplicable,
                public_transaction: PublicTransaction::Idle,
                transaction_decision: Decision::NotCommittedRecoveryRequired,
                client_knowledge: ClientKnowledge::KnownNotCommittedRecoveryRequired,
                recovery_obligations: obligations,
                ..Response::default()
            }),
            context_event: Some(ContextEvent {
                identity: c5,
                before: None,
                after: ContextStatus::Completed,
            }),
            controls: Controls {
                entry_cancel: Check::No,
                entry_deadline: Check::No,
                observed_cancel: false,
                observed_deadline: false,
            },
            consumed_parts: 2,
            authenticated: None,
        };
        let case = Case {
            pre,
            input,
            edge,
            post,
        };
        assert_eq!(case.verify(), Ok(()), "hand-built fenced close must pass");
        // A close on a fenced engine must not invent a whole-abort decision.
        let f = reject_post(&case, |s| {
            let t = txn_mut(s, 1);
            t.decision = Decision::Aborted;
            t.client_knowledge = ClientKnowledge::KnownAborted;
            t.life = TransactionLife::Ended;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "transactions",
                    ..
                }
            ),
            "{f:?}"
        );
        let f = reject_edge(&case, |e| {
            let r = e.response.as_mut().unwrap();
            r.transaction_decision = Decision::Aborted;
            r.client_knowledge = ClientKnowledge::KnownAborted;
        });
        assert!(matches!(f, CheckFailure::Response(_)), "{f:?}");
        // Owner left Held instead of the exact handoff.
        let f = reject_post(&case, |s| {
            let o = owner_mut(s, 6);
            o.current_holder = Some(t1);
            o.previous_holder = None;
            o.disposition = OwnerDisposition::Held;
            o.transfer_count = 0;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "owners",
                    ..
                }
            ),
            "{f:?}"
        );
        // The fence must survive the close.
        let f = reject_post(&case, |s| {
            s.engines.as_mut_slice()[0].admission = EngineAdmission::Admitting;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "engines",
                    ..
                }
            ),
            "{f:?}"
        );
        // Incomplete on an already fenced engine is FencedClose.
        let incomplete = assemble(case.pre.state(), raw(Api::CloseSession, 0, None, 1));
        let excluded = Edge {
            class: EdgeClass::Excluded(Exclusion::FencedClose),
            ..Edge::default()
        };
        assert_eq!(
            verify_edge(case.pre.state(), &incomplete, &excluded, case.pre.state()),
            Ok(())
        );
        println!("CHECKER fenced_close accepted=1 rejected=4 excluded=1");
    });
}

/// Totality over every assembled input at every literal state: the checker returns without
/// panicking, and no assembled input is judged an envelope mismatch only (which would mean the
/// checker's recipe and the assembler's differ).
#[test]
fn total_over_assembled_inputs() {
    worker("checker-totality-256k", || {
        let fx = Fixture::new();
        let names: Vec<String> = fx.root["states"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut state = Workspace::try_new().unwrap();
        let mut calls = 0usize;
        let mut assembled = 0usize;
        let mut envelope_only = Vec::new();
        let rules = [
            Exclusion::ContextFresh,
            Exclusion::SignalNoDeadline,
            Exclusion::BindN0,
            Exclusion::ExecuteCommit,
            Exclusion::ReadOnlyCleanup,
            Exclusion::CommitNoWrite,
            Exclusion::CommitProgress,
            Exclusion::CommitPreattemptGate,
            Exclusion::RecoveryPre,
            Exclusion::RecoverySet,
            Exclusion::Envelope,
            Exclusion::ResumeWork,
            Exclusion::FencedClose,
            Exclusion::PeerFence,
        ];
        for name in &names {
            fx.literals.state(name, state.state_mut()).unwrap();
            let s = state.state();
            for index in 0..RAW_ACTION_COUNT {
                let action = action_at(index).unwrap();
                let mut input = ResolvedInput::default();
                if assemble_input(s, &action, &mut input).is_err() {
                    continue;
                }
                assembled += 1;
                let _ = verify_edge(s, &input, &Edge::default(), s);
                calls += 1;
                let accepted: Vec<Exclusion> = rules
                    .iter()
                    .copied()
                    .filter(|r| {
                        let edge = Edge {
                            class: EdgeClass::Excluded(*r),
                            ..Edge::default()
                        };
                        calls += 1;
                        verify_edge(s, &input, &edge, s).is_ok()
                    })
                    .collect();
                if accepted == [Exclusion::Envelope] {
                    envelope_only.push(format!("{name} raw={index} {action:?}"));
                }
            }
        }
        assert!(
            envelope_only.is_empty(),
            "assembled inputs judged envelope-only ({}): {:#?}",
            envelope_only.len(),
            &envelope_only[..envelope_only.len().min(40)]
        );
        println!(
            "CHECKER totality states={} assembled_inputs={assembled} verify_calls={calls}",
            names.len()
        );
    });
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

fn count_from_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Deterministic walk over the checker's own expectations from every literal state.
///
/// At every visited state each assembled raw input must (1) be self-consistent
/// (`verify_edge` accepts the checker's own expected edge and post-state), (2) yield an expected
/// post-state that the codec's canonical state loader accepts, and (3) never be an
/// envelope-only exclusion, i.e. the checker's Env1 recipe equals the assembler's.
/// The walk then follows one admitted edge, choosing the API family first.
#[test]
fn self_walk_posts_are_canonical_states() {
    worker("checker-self-walk-256k", || {
        let fx = Fixture::new();
        let names: Vec<String> = fx.root["states"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let walks = count_from_env("XADB_CHECKER_WALKS", 58);
        let length = count_from_env("XADB_CHECKER_WALK_LENGTH", 8);
        let mut rng = Lcg(0x5eed_c0de);
        let mut current = Workspace::try_new().unwrap();
        let mut post = Workspace::try_new().unwrap();
        let mut bytes = vec![0u8; 16 * 1024];
        let mut failures: Vec<String> = Vec::new();
        let mut visited = 0usize;
        let mut checked = 0usize;
        let mut classes = std::collections::BTreeMap::<String, usize>::new();
        for walk in 0..walks {
            fx.literals
                .state(&names[walk % names.len()], current.state_mut())
                .unwrap();
            for step in 0..length {
                visited += 1;
                let mut by_family: Vec<Vec<u16>> = vec![Vec::new(); 21];
                for index in 0..RAW_ACTION_COUNT {
                    let action = action_at(index).unwrap();
                    let mut input = ResolvedInput::default();
                    if assemble_input(current.state(), &action, &mut input).is_err() {
                        continue;
                    }
                    checked += 1;
                    let mut edge = Edge::default();
                    let rules = match expected(current.state(), &input, &mut edge, post.state_mut())
                    {
                        Ok(rules) => rules,
                        Err(f) => {
                            failures.push(format!(
                                "walk={walk} step={step} raw={index} expectation {f:?}"
                            ));
                            continue;
                        }
                    };
                    if rules == [Exclusion::Envelope] {
                        failures.push(format!(
                            "walk={walk} step={step} raw={index} {action:?} envelope-only"
                        ));
                    }
                    if let Err(f) = verify_edge(current.state(), &input, &edge, post.state()) {
                        failures.push(format!(
                            "walk={walk} step={step} raw={index} self-check {f:?}"
                        ));
                    }
                    let label = match edge.class {
                        EdgeClass::Excluded(r) => format!("Excluded({r:?})"),
                        other => format!("{other:?}"),
                    };
                    *classes
                        .entry(format!("{:?}/{label}", action.api))
                        .or_default() += 1;
                    if rules.is_empty() {
                        if let Err(e) =
                            encode_state(post.state(), &WireIdentity::default(), &mut bytes)
                        {
                            failures.push(format!(
                                "walk={walk} step={step} raw={index} {action:?} post rejected by codec: {}",
                                e.reason
                            ));
                        }
                        if !matches!(edge.class, EdgeClass::Rejected | EdgeClass::NoEffect) {
                            by_family[action.api as usize].push(index);
                        }
                    }
                }
                let families: Vec<&Vec<u16>> = by_family.iter().filter(|f| !f.is_empty()).collect();
                if families.is_empty() {
                    break;
                }
                let family = families[rng.next(families.len())];
                let index = family[rng.next(family.len())];
                let mut input = ResolvedInput::default();
                assemble_input(current.state(), &action_at(index).unwrap(), &mut input).unwrap();
                let mut edge = Edge::default();
                expected(current.state(), &input, &mut edge, post.state_mut()).unwrap();
                current.state_mut().copy_from(post.state());
            }
            if failures.len() > 40 {
                break;
            }
        }
        if std::env::var("XADB_CHECKER_CLASSES").is_ok() {
            for (k, v) in &classes {
                println!("CLASS {k} {v}");
            }
        }
        assert!(
            failures.is_empty(),
            "self-walk failures ({}): {:#?}",
            failures.len(),
            &failures[..failures.len().min(40)]
        );
        println!(
            "CHECKER self_walk walks={walks} visited_states={visited} checked_inputs={checked}"
        );
    });
}

fn prepare(actor: u8, slot: u8, dml: bool) -> RawAction {
    RawAction {
        api: Api::Prepare,
        actor,
        selector: None,
        prepare: Some(PrepareChoice {
            slot,
            shape: if dml { Shape::Dml } else { Shape::Select },
            parameter_count: 0,
        }),
        autocommit: None,
        outcome: 0,
    }
}

/// Reach a deeper state by following admitted edges of the checker's own expectation.
/// Used only as the baseline of known-bad rejection variants on paths the literals lack.
fn follow(fx: &Fixture, start: &str, actions: &[RawAction]) -> Workspace {
    let mut state = Workspace::try_new().unwrap();
    let mut post = Workspace::try_new().unwrap();
    fx.literals.state(start, state.state_mut()).unwrap();
    for a in actions {
        let input = assemble(state.state(), *a);
        let mut edge = Edge::default();
        let rules = expected(state.state(), &input, &mut edge, post.state_mut()).unwrap();
        assert!(rules.is_empty(), "{a:?} unexpectedly excluded: {rules:?}");
        assert_ne!(
            edge.class,
            EdgeClass::Rejected,
            "{a:?} unexpectedly rejected"
        );
        state.state_mut().copy_from(post.state());
    }
    state
}

/// The checker's expected edge for one assembled action, packaged as a case.
fn expected_case(pre: Workspace, action: RawAction) -> (Case, Vec<Exclusion>) {
    let input = assemble(pre.state(), action);
    let mut post = Workspace::try_new().unwrap();
    let mut edge = Edge::default();
    let rules = expected(pre.state(), &input, &mut edge, post.state_mut()).unwrap();
    (
        Case {
            pre,
            input,
            edge,
            post,
        },
        rules,
    )
}

fn only_rule(case: &Case, rule: Exclusion) {
    let excluded = |r| Edge {
        class: EdgeClass::Excluded(r),
        ..Edge::default()
    };
    let pre = case.pre.state();
    assert_eq!(
        verify_edge(pre, &case.input, &excluded(rule), pre),
        Ok(()),
        "{rule:?} must be accepted"
    );
    for other in [
        Exclusion::Envelope,
        Exclusion::ContextFresh,
        Exclusion::BindN0,
    ] {
        if other != rule {
            let f = verify_edge(pre, &case.input, &excluded(other), pre).unwrap_err();
            assert!(matches!(f, CheckFailure::Class(_)), "{other:?}: {f:?}");
        }
    }
    let f = verify_edge(pre, &case.input, &Edge::default(), pre).unwrap_err();
    assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
}

#[test]
fn fence_and_continuation_variants_are_rejected() {
    worker("checker-fence-continuation-256k", || {
        let fx = Fixture::new();
        let mut count = 0;
        let stmt0 = Some(TargetSelector::Issued(0));
        // Peer fence: actor 1 holds an explicit RW; actor 0's implicit DML commit is Unknown.
        let pre = follow(
            &fx,
            "initial",
            &[raw(Api::Begin, 1, None, 0), prepare(0, 0, true)],
        );
        let (fence, rules) = expected_case(pre, raw(Api::Execute, 0, stmt0, 9));
        assert!(rules.is_empty());
        assert_eq!(fence.verify(), Ok(()));
        let peer = 1u16;
        let f = reject_post(&fence, |s| {
            txn_mut(s, peer).decision = Decision::NotCommittedRecoveryRequired;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "transactions",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_post(&fence, |s| txn_mut(s, peer).life = TransactionLife::Active);
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "transactions",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_post(&fence, |s| {
            let o = owner_mut(s, 1);
            o.previous_holder = o.current_holder;
            o.current_holder = None;
            o.disposition = OwnerDisposition::Released;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "owners",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_post(&fence, |s| {
            let last = s.owners.len() - 1;
            s.owners.remove(last);
        });
        assert!(matches!(f, CheckFailure::PostState { .. }), "{f:?}");
        count += 1;
        let f = reject_post(&fence, |s| {
            s.engines.as_mut_slice()[0].first_fence_context = None
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "engines",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_post(&fence, |s| {
            let o = s
                .obligations
                .as_mut_slice()
                .iter_mut()
                .find(|o| o.transaction == TransactionRef(peer))
                .unwrap();
            o.known_decision = Decision::Unknown;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "obligations",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_edge(&fence, |e| {
            e.response.as_mut().unwrap().certainty = Certainty::Known;
        });
        assert!(matches!(f, CheckFailure::Response(_)), "{f:?}");
        count += 1;
        let hidden = Edge {
            class: EdgeClass::Excluded(Exclusion::ExecuteCommit),
            ..Edge::default()
        };
        let f = fence
            .verify_with(&hidden, fence.pre.state())
            .expect_err("legal implicit-commit Unknown hidden as excluded");
        assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
        count += 1;

        // Commit continuation at Attempted: out-of-order facts are CommitProgress only.
        let attempted = [
            raw(Api::Begin, 0, None, 0),
            prepare(0, 0, true),
            raw(Api::Execute, 0, stmt0, 0),
            raw(Api::BeginCommit, 0, None, 1),
            raw(Api::CommitFact, 0, None, 0),
        ];
        let (published, rules) = expected_case(
            follow(&fx, "initial", &attempted),
            raw(Api::CommitFact, 0, None, 2),
        );
        assert_eq!(rules, [Exclusion::CommitProgress]);
        only_rule(&published, Exclusion::CommitProgress);
        count += 1;
        let (unknown, rules) = expected_case(
            follow(&fx, "initial", &attempted),
            raw(Api::FinishCommit, 0, None, 3),
        );
        assert!(rules.is_empty());
        assert_eq!(unknown.verify(), Ok(()));
        let f = reject_edge(&unknown, |e| e.context_event = None);
        assert!(matches!(f, CheckFailure::ContextEvent(_)), "{f:?}");
        count += 1;
        let f = reject_post(&unknown, |s| {
            s.actors.as_mut_slice()[0].parked_context = Some(ContextRef(4))
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "actors",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        let f = reject_edge(&unknown, |e| e.class = EdgeClass::Admitted);
        assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
        count += 1;

        // A signalled BeforeAttempt commit: pre-attempt facts are gated, Finish observes Cancel.
        let signalled = [
            raw(Api::Begin, 0, None, 0),
            prepare(0, 0, true),
            raw(Api::Execute, 0, stmt0, 0),
            raw(Api::BeginCommit, 0, None, 1),
            raw(Api::SignalContext, 0, Some(TargetSelector::Issued(3)), 0),
        ];
        let (gated, rules) = expected_case(
            follow(&fx, "initial", &signalled),
            raw(Api::CommitFact, 0, None, 0),
        );
        assert_eq!(rules, [Exclusion::CommitPreattemptGate]);
        only_rule(&gated, Exclusion::CommitPreattemptGate);
        count += 1;
        let (aborted, rules) = expected_case(
            follow(&fx, "initial", &signalled),
            raw(Api::FinishCommit, 0, None, 1),
        );
        assert!(rules.is_empty());
        assert_eq!(aborted.verify(), Ok(()));
        assert_eq!(aborted.edge.response.unwrap().code, ReturnCode::Cancelled);
        let f = reject_edge(&aborted, |e| e.controls.observed_cancel = false);
        assert!(matches!(f, CheckFailure::Controls(_)), "{f:?}");
        count += 1;
        let f = reject_edge(&aborted, |e| {
            let r = e.response.as_mut().unwrap();
            r.code = ReturnCode::Conflict;
            r.origin_cause = OriginCause::Conflict;
            r.diagnostic.as_mut().unwrap().code = ReturnCode::Conflict;
            r.diagnostic.as_mut().unwrap().origin_cause = OriginCause::Conflict;
        });
        assert!(matches!(f, CheckFailure::Response(_)), "{f:?}");
        count += 1;

        // A BeforeAttempt peer under a fence: Finish Aborted is PeerFence; NCR is the handoff.
        let peer_fence = [
            raw(Api::Begin, 1, None, 0),
            raw(Api::BeginCommit, 1, None, 0),
            prepare(0, 0, true),
            raw(Api::Execute, 0, stmt0, 9),
        ];
        let (peer_abort, rules) = expected_case(
            follow(&fx, "initial", &peer_fence),
            raw(Api::FinishCommit, 1, None, 1),
        );
        assert_eq!(rules, [Exclusion::PeerFence]);
        only_rule(&peer_abort, Exclusion::PeerFence);
        count += 1;
        let (handoff, rules) = expected_case(
            follow(&fx, "initial", &peer_fence),
            raw(Api::FinishCommit, 1, None, 2),
        );
        assert!(rules.is_empty());
        assert_eq!(handoff.verify(), Ok(()));
        let r = handoff.edge.response.unwrap();
        assert_eq!(
            (r.code, r.origin_cause, handoff.edge.consumed_parts),
            (ReturnCode::RecoveryRequired, OriginCause::PeerFence, 2)
        );
        let f = reject_post(&handoff, |s| {
            for o in s.owners.as_mut_slice() {
                if o.disposition == OwnerDisposition::Transferred {
                    o.disposition = OwnerDisposition::Held;
                    o.current_holder = o.previous_holder;
                    o.previous_holder = None;
                    o.transfer_count = 0;
                }
            }
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "owners",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        // CloseSession Incomplete keeps the session Open (owner decision, like Rollback
        // Incomplete): the shell, statements and the RecoveryPending transaction stay.
        let mut written = Workspace::try_new().unwrap();
        fx.literals
            .state("rw.written", written.state_mut())
            .unwrap();
        let (failed_close, rules) = expected_case(written, raw(Api::CloseSession, 0, None, 1));
        assert!(rules.is_empty());
        assert_eq!(failed_close.verify(), Ok(()));
        let post = failed_close.post.state();
        let a = post.actors.as_slice()[0];
        assert_eq!(
            (a.shell, a.transaction),
            (Shell::Open, Some(TransactionRef(1)))
        );
        assert_eq!(
            post.issued_statements.as_slice()[0].life,
            StatementLife::Current
        );
        assert_eq!(
            post.session_incarnations.as_slice()[0].lifecycle,
            Shell::Open
        );
        assert_eq!(
            post.transactions.as_slice()[0].life,
            TransactionLife::RecoveryPending
        );
        assert_eq!(
            post.engines.as_slice()[0].admission,
            EngineAdmission::Fenced
        );
        let r = failed_close.edge.response.unwrap();
        assert_eq!(
            (r.code, r.public_transaction),
            (
                ReturnCode::RecoveryRequired,
                PublicTransaction::RecoveryPending
            )
        );
        let f = reject_post(&failed_close, |s| {
            let a = &mut s.actors.as_mut_slice()[0];
            a.shell = Shell::Closed;
            a.transaction = None;
            s.session_incarnations.as_mut_slice()[0].lifecycle = Shell::Closed;
        });
        assert!(matches!(f, CheckFailure::PostState { .. }), "{f:?}");
        count += 1;
        let f = reject_post(&failed_close, |s| {
            let o = owner_mut(s, 4);
            o.previous_holder = o.current_holder;
            o.current_holder = None;
            o.disposition = OwnerDisposition::Released;
        });
        assert!(
            matches!(
                f,
                CheckFailure::PostState {
                    table: "owners",
                    ..
                }
            ),
            "{f:?}"
        );
        count += 1;
        println!("CHECKER fence_continuation_variants rejected_or_excluded={count}");
    });
}

/// Corrupted resolved inputs (outside the assembler's range) never panic the checker.
#[test]
fn total_over_corrupted_inputs() {
    worker("checker-corrupted-256k", || {
        let fx = Fixture::new();
        let names: Vec<String> = fx.root["states"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let mut state = Workspace::try_new().unwrap();
        let mut calls = 0usize;
        let mut rng = Lcg(0x0bad_5eed);
        for name in &names {
            fx.literals.state(name, state.state_mut()).unwrap();
            let s = state.state();
            for index in (0..RAW_ACTION_COUNT).step_by(7) {
                let mut input = ResolvedInput::default();
                if assemble_input(s, &action_at(index).unwrap(), &mut input).is_err() {
                    continue;
                }
                for variant in 0..8 {
                    let mut bad = input;
                    match variant {
                        0 => bad.outcome = rng.next(256) as u8,
                        1 => bad.actor = rng.next(256) as u8,
                        2 => bad.target = Some(RawIdentity([rng.next(256) as u8; 16])),
                        3 => bad.context = None,
                        4 => bad.environment = None,
                        5 => {
                            if let Some(env) = bad.environment.as_mut() {
                                env.kind = FactKind::RecoveryConfirmed;
                                env.cleanup = rng.next(256) as u8;
                            }
                        }
                        6 => {
                            bad.cancel = !bad.cancel;
                            bad.deadline_reached = !bad.deadline_reached;
                        }
                        _ => {
                            bad.prepare_slot = rng.next(256) as u8;
                            bad.set_autocommit = rng.next(256) as u8;
                        }
                    }
                    let _ = verify_edge(s, &bad, &Edge::default(), s);
                    calls += 1;
                }
            }
        }
        println!("CHECKER corrupted_inputs verify_calls={calls}");
    });
}

/// Owner decisions of 2026-10-25: authenticated targets after the child gate, the recovery
/// completion fact, and the RecoveryPre/Envelope split of recovery header mismatches.
#[test]
fn owner_decisions_are_applied() {
    worker("checker-owner-decisions-256k", || {
        let fx = Fixture::new();
        let mut count = 0;
        let stmt0 = Some(TargetSelector::Issued(0));
        let result0 = Some(TargetSelector::Issued(0));
        let id = |alias: &str| fx.identity(&Value::from(alias));
        let rejected_with = |case: &Case, auth: Option<Authenticated>| {
            assert_eq!(case.edge.class, EdgeClass::Rejected);
            assert_eq!(case.edge.authenticated, auth);
            assert_eq!(case.verify(), Ok(()));
            for other in [None, Some(Authenticated::default())] {
                if other != auth {
                    let f = reject_edge(case, |e| e.authenticated = other);
                    assert!(matches!(f, CheckFailure::Authenticated(_)), "{f:?}");
                }
            }
        };
        let at = |state: &str| {
            let mut w = Workspace::try_new().unwrap();
            fx.literals.state(state, w.state_mut()).unwrap();
            w
        };
        // Statement-targeted ResultBusy: statement P1 and the caller's transaction T1.
        let (busy, _) = expected_case(at("sel.open"), raw(Api::Execute, 0, stmt0, 0));
        rejected_with(
            &busy,
            Some(Authenticated {
                statement: Some(id("@p1")),
                result: None,
                transaction: Some(id("@t1")),
            }),
        );
        count += 1;
        // Statement-targeted entry Cancelled with no caller transaction.
        let (bind_cancel, _) = expected_case(at("sel.prepared"), raw(Api::BindAll, 0, stmt0, 8));
        rejected_with(
            &bind_cancel,
            Some(Authenticated {
                statement: Some(id("@p1")),
                result: None,
                transaction: None,
            }),
        );
        count += 1;
        // InspectStatement Busy under a parked fetch: it selects no transaction.
        let (inspect, _) = expected_case(at("sel.parked"), raw(Api::InspectStatement, 0, stmt0, 0));
        rejected_with(
            &inspect,
            Some(Authenticated {
                statement: Some(id("@p1")),
                result: None,
                transaction: None,
            }),
        );
        count += 1;
        // CloseStatement Busy: the slot descriptor's transaction T1 (wire-v2 §4 selection).
        let (close_busy, _) =
            expected_case(at("sel.parked"), raw(Api::CloseStatement, 0, stmt0, 0));
        rejected_with(
            &close_busy,
            Some(Authenticated {
                statement: Some(id("@p1")),
                result: None,
                transaction: Some(id("@t1")),
            }),
        );
        count += 1;
        // CloseStatement entry Cancelled without a slot descriptor: no transaction.
        let (close_cancel, _) =
            expected_case(at("sel.prepared"), raw(Api::CloseStatement, 0, stmt0, 1));
        rejected_with(
            &close_cancel,
            Some(Authenticated {
                statement: Some(id("@p1")),
                result: None,
                transaction: None,
            }),
        );
        count += 1;
        // Execute entry Timeout inside an explicit transaction: the caller's pre-state T1.
        let (execute_timeout, _) = expected_case(at("rw.prepared"), raw(Api::Execute, 0, stmt0, 2));
        rejected_with(
            &execute_timeout,
            Some(Authenticated {
                statement: Some(id("@p1")),
                result: None,
                transaction: Some(id("@t1")),
            }),
        );
        count += 1;
        // Result-targeted HandleClosed: result R1 and its descriptor transaction T1.
        let (closed, _) = expected_case(at("sel.closed"), raw(Api::Fetch, 0, result0, 0));
        rejected_with(
            &closed,
            Some(Authenticated {
                statement: None,
                result: Some(id("@r1s0")),
                transaction: Some(id("@t1")),
            }),
        );
        count += 1;
        // Child-authentication failure stays Some(default); no-child APIs and cached reads None.
        let (stale, _) = expected_case(
            at("sel.reexecuted"),
            raw(Api::Fetch, 0, Some(TargetSelector::FrozenUnissued(0)), 0),
        );
        rejected_with(&stale, Some(Authenticated::default()));
        count += 1;
        let (begin, _) = expected_case(at("sel.open"), raw(Api::Begin, 0, None, 0));
        rejected_with(&begin, None);
        count += 1;
        let (cached, _) = expected_case(at("sel.eof"), raw(Api::Fetch, 0, result0, 0));
        assert_eq!(
            (cached.edge.class, cached.edge.authenticated),
            (EdgeClass::Admitted, None)
        );
        let f = reject_edge(&cached, |e| {
            e.authenticated = Some(Authenticated::default())
        });
        assert!(matches!(f, CheckFailure::Authenticated(_)), "{f:?}");
        count += 1;

        // Recovery after a peer fence and both fenced closes.
        let closed_both = [
            raw(Api::Begin, 1, None, 0),
            prepare(0, 0, true),
            raw(Api::Execute, 0, stmt0, 9),
            raw(Api::CloseSession, 0, None, 0),
            raw(Api::CloseSession, 1, None, 0),
        ];
        let (recovery, rules) = expected_case(
            follow(&fx, "initial", &closed_both),
            raw(Api::RecoveryConfirmedFreshEngine, 255, None, 0),
        );
        assert!(rules.is_empty());
        assert_eq!(recovery.verify(), Ok(()));
        let fact = recovery
            .post
            .state()
            .facts
            .as_slice()
            .last()
            .unwrap()
            .id
            .nonce;
        let completion = |s: &State, n: u16| {
            s.transactions
                .as_slice()
                .iter()
                .find(|t| t.id.nonce == n)
                .unwrap()
                .completion_fact
        };
        // T1 is actor 1's fenced peer (no completion fact); T2 the Unknown cause. Recovery
        // changes neither completion fact; the obligations record the recovery fact instead.
        assert_eq!(completion(recovery.pre.state(), 1), None);
        let cause_fact = completion(recovery.pre.state(), 2);
        assert!(cause_fact.is_some());
        assert_eq!(completion(recovery.post.state(), 1), None);
        assert_eq!(completion(recovery.post.state(), 2), cause_fact);
        assert!(
            recovery
                .post
                .state()
                .obligations
                .as_slice()
                .iter()
                .all(|o| o.recovery_fact == Some(FactRef(fact)))
        );
        for t in [1, 2] {
            let f = reject_post(&recovery, |s| {
                txn_mut(s, t).completion_fact = Some(FactRef(fact))
            });
            assert!(
                matches!(
                    f,
                    CheckFailure::PostState {
                        table: "transactions",
                        ..
                    }
                ),
                "{f:?}"
            );
        }
        let mut bytes = vec![0u8; 16 * 1024];
        assert!(
            encode_state(recovery.post.state(), &WireIdentity::default(), &mut bytes).is_ok(),
            "recovery post-state must satisfy the codec loader"
        );
        count += 1;
        // Precondition holds; only a non-identity header byte differs: Envelope only.
        let excluded = |r| Edge {
            class: EdgeClass::Excluded(r),
            ..Edge::default()
        };
        let pre = recovery.pre.state();
        let mut header = recovery.input;
        header.environment.as_mut().unwrap().cleanup = 1;
        assert_eq!(
            verify_edge(pre, &header, &excluded(Exclusion::Envelope), pre),
            Ok(())
        );
        for other in [Exclusion::RecoveryPre, Exclusion::RecoverySet] {
            let f = verify_edge(pre, &header, &excluded(other), pre).unwrap_err();
            assert!(matches!(f, CheckFailure::Class(_)), "{other:?}: {f:?}");
        }
        count += 1;
        // A fresh-engine or fact identity mismatch falls under both ENV-RECOVERY-PRE and the
        // general envelope rule; either label is accepted, RecoverySet is not.
        let mut fresh = recovery.input;
        fresh
            .environment
            .as_mut()
            .unwrap()
            .fresh_engine
            .as_mut()
            .unwrap()
            .0[8] = 6;
        let mut fact_id = recovery.input;
        fact_id.environment.as_mut().unwrap().fact.0[8] = 6;
        for input in [&fresh, &fact_id] {
            for rule in [Exclusion::RecoveryPre, Exclusion::Envelope] {
                assert_eq!(
                    verify_edge(pre, input, &excluded(rule), pre),
                    Ok(()),
                    "{rule:?}"
                );
            }
            let f = verify_edge(pre, input, &excluded(Exclusion::RecoverySet), pre).unwrap_err();
            assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
            let f = verify_edge(pre, input, &Edge::default(), pre).unwrap_err();
            assert!(matches!(f, CheckFailure::Class(_)), "{f:?}");
        }
        count += 1;
        // A MissingFirst mutation is RecoverySet only.
        let (missing, rules) = expected_case(
            follow(&fx, "initial", &closed_both),
            raw(Api::RecoveryConfirmedFreshEngine, 255, None, 1),
        );
        assert_eq!(rules, [Exclusion::RecoverySet]);
        only_rule(&missing, Exclusion::RecoverySet);
        count += 1;
        println!("CHECKER owner_decision_cases={count}");
    });
}
