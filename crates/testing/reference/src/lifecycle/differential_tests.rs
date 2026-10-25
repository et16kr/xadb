//! Model output judged by the independent edge checker over reachable states.
//!
//! The checker never calls the model and the model never calls the checker. A rejection is a
//! disagreement to adjudicate against the sources, not an automatic model bug. Every raw slot
//! the assembler resolves is judged; assembly errors are counted by kind and a malformed state
//! or capacity error fails the run. `XADB_DIFF_COVERAGE` prints per-(API, outcome, class)
//! counts.
use super::checker::verify_edge;
use super::codec::{decode_state, encode_state};
use super::input_assembler::{AssemblyError, assemble_input};
use super::input_schema::{RawIdentity, ResolvedInput};
use super::literal::{Literals, SOURCE};
use super::model::{Edge, EdgeClass, Exclusion, transition};
use super::raw_action::{RAW_ACTION_COUNT, action_at};
use super::schema::{State, WireIdentity};
use serde_json::Value;
use std::collections::BTreeMap;

/// Fixed walk prefixes (2026-10-25). The first five ended where an adjudicated disagreement or
/// assembler gap appeared: a fenced CloseSession handoff, a recovery-ready state, a fenced
/// parked commit, a CloseStatement of a fenced cursor and a fenced Finish NCR of an already
/// ended no-write commit. The last two reach a Resume fence terminal and a signalled Resume with
/// Incomplete cleanup, families the default walks rarely visit. Every final state is also judged
/// with corrupted inputs.
const REGRESSION_PATHS: [&[u16]; 7] = [
    &[36, 1279, 579, 6, 419],
    &[36, 1267, 1315, 1314, 1310],
    &[42, 1267, 1506, 18, 421],
    &[1310, 1279, 36, 576, 1307],
    &[27, 1267, 228, 1264, 1505, 1512, 1315],
    &[24, 576, 1452, 6, 420],
    &[1264, 0, 400, 1420, 1362],
];

#[derive(Default)]
struct Tally {
    failures: BTreeMap<String, (usize, String)>,
    counts: BTreeMap<String, usize>,
    coverage: BTreeMap<String, usize>,
    assembly: BTreeMap<String, usize>,
    corrupt: BTreeMap<String, usize>,
}

impl Tally {
    fn fail(&mut self, key: String, example: impl FnOnce() -> String) {
        self.failures.entry(key).or_insert_with(|| (0, example())).0 += 1;
    }

    /// Runs the model on one input and judges the observed edge.
    fn judge(
        &mut self,
        name: &str,
        raw: u16,
        pre: &State,
        input: &ResolvedInput,
        post: &mut State,
    ) -> Option<EdgeClass> {
        let mut edge = Edge::default();
        if let Err(error) = transition(pre, input, post, &mut edge) {
            self.fail(
                format!("{:?}/{} model {error:?}", input.api, input.outcome),
                || format!("{name} raw={raw}"),
            );
            return None;
        }
        if let Err(why) = verify_edge(pre, input, &edge, post) {
            self.fail(
                format!("{:?}/{} {:?} {why:?}", input.api, input.outcome, edge.class),
                || format!("{name} raw={raw}"),
            );
        }
        Some(edge.class)
    }

    fn judge_state(
        &mut self,
        name: &str,
        pre: &State,
        successors: Option<&mut Vec<(u16, EdgeClass)>>,
    ) {
        let mut input = Box::<ResolvedInput>::default();
        let mut post = Box::<State>::default();
        let mut chosen = successors;
        for raw in 0..RAW_ACTION_COUNT {
            let action = action_at(raw).expect("raw");
            if let Err(error) = assemble_input(pre, &action, &mut input) {
                let kind = match error {
                    AssemblyError::InvalidAction(_) => "InvalidAction",
                    AssemblyError::Selector(_) => "Selector",
                    AssemblyError::InvalidState => "InvalidState",
                    AssemblyError::ProfileLimit => "ProfileLimit",
                    AssemblyError::Capacity => "Capacity",
                };
                *self.assembly.entry(kind.into()).or_default() += 1;
                if matches!(error, AssemblyError::InvalidState | AssemblyError::Capacity) {
                    self.fail(format!("{:?} assembly {kind}", action.api), || {
                        format!("{name} raw={raw}")
                    });
                }
                continue;
            }
            let Some(class) = self.judge(name, raw, pre, &input, &mut post) else {
                continue;
            };
            *self.counts.entry(format!("{class:?}")).or_default() += 1;
            *self
                .coverage
                .entry(format!("{:?}/{} {class:?}", action.api, action.outcome))
                .or_default() += 1;
            if matches!(
                class,
                EdgeClass::Excluded(Exclusion::Envelope | Exclusion::ContextFresh)
            ) {
                self.fail(
                    format!("{:?}/{} assembled {class:?}", action.api, action.outcome),
                    || format!("{name} raw={raw}"),
                );
            }
            if let Some(list) = chosen.as_deref_mut() {
                list.push((raw, class));
            }
        }
    }

    /// Envelope/ContextFresh inputs the assembler never produces: a later context nonce, a
    /// different supplied transaction and a different fact identity. This is not general Envelope
    /// coverage: other Env1 bytes are not corrupted.
    fn judge_corrupted(&mut self, name: &str, pre: &State) {
        let mut input = Box::<ResolvedInput>::default();
        let mut bad = Box::<ResolvedInput>::default();
        let mut post = Box::<State>::default();
        let bump = |id: &mut RawIdentity| {
            let nonce = u16::from_le_bytes([id.0[8], id.0[9]]).wrapping_add(1);
            id.0[8..10].copy_from_slice(&nonce.to_le_bytes());
        };
        for raw in 0..RAW_ACTION_COUNT {
            let action = action_at(raw).expect("raw");
            if assemble_input(pre, &action, &mut input).is_err() {
                continue;
            }
            for corruption in ["context", "transaction", "fact"] {
                *bad = *input;
                let changed = match corruption {
                    "context" => bad.context.as_mut().map(bump).is_some(),
                    "transaction" => bad
                        .environment
                        .as_mut()
                        .and_then(|e| e.transaction.as_mut())
                        .map(bump)
                        .is_some(),
                    _ => bad
                        .environment
                        .as_mut()
                        .map(|e| bump(&mut e.fact))
                        .is_some(),
                };
                if !changed {
                    continue;
                }
                let example = format!("{name} {corruption}");
                if let Some(class) = self.judge(&example, raw, pre, &bad, &mut post) {
                    *self
                        .corrupt
                        .entry(format!("{corruption} {:?} {class:?}", bad.api))
                        .or_default() += 1;
                }
            }
        }
    }

    /// Replays a raw path from `start`, judging every visited state including the last.
    fn follow(&mut self, label: &str, start: &State, path: &[u16], pre: &mut State) {
        let mut input = Box::<ResolvedInput>::default();
        let mut post = Box::<State>::default();
        pre.copy_from(start);
        for (step, &raw) in path.iter().enumerate() {
            self.judge_state(&format!("{label}{:?}", &path[..step]), pre, None);
            assemble_input(pre, &action_at(raw).expect("raw"), &mut input).expect("assemble");
            let mut edge = Edge::default();
            transition(pre, &input, &mut post, &mut edge).expect("model");
            pre.copy_from(&post);
        }
        self.judge_state(&format!("{label}{path:?}"), pre, None);
    }
}

fn env_count<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
fn differential_literal_states_and_walks() {
    let source: Value = serde_json::from_str(SOURCE).expect("literal JSON");
    let literals = Literals::new().expect("literals");
    let mut states = Vec::new();
    for name in source["states"].as_object().unwrap().keys() {
        let mut s = Box::<State>::default();
        literals.state(name, &mut s).expect("state");
        states.push((name.clone(), s));
    }
    let walks: u64 = env_count("XADB_DIFF_WALKS", 30);
    let depth: usize = env_count("XADB_DIFF_DEPTH", 6);
    let worker = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut tally = Tally::default();
            for (name, s) in &states {
                tally.judge_state(name, s, None);
                tally.judge_corrupted(name, s);
            }
            let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
            let mut pre = Box::<State>::default();
            let mut post = Box::<State>::default();
            let mut input = Box::<ResolvedInput>::default();
            let initial = &states
                .iter()
                .find(|(n, _)| n == "initial")
                .expect("initial")
                .1;
            for path in REGRESSION_PATHS {
                tally.follow("regression", initial, path, &mut pre);
                tally.judge_corrupted(&format!("regression{path:?}"), &pre);
            }
            let mut edges = Vec::new();
            for walk in 0..walks {
                pre.copy_from(initial);
                let mut path = Vec::new();
                for _ in 0..depth {
                    edges.clear();
                    tally.judge_state(&format!("walk{walk}{path:?}"), &pre, Some(&mut edges));
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    // One step in eight follows a rejection, whose fresh context still changes
                    // the state; the rest follow state-changing admitted edges.
                    let rejected = seed.is_multiple_of(8);
                    let candidates: Vec<u16> = edges
                        .iter()
                        .filter(|(_, c)| {
                            if rejected {
                                *c == EdgeClass::Rejected
                            } else {
                                matches!(
                                    c,
                                    EdgeClass::Admitted
                                        | EdgeClass::Continuation
                                        | EdgeClass::NoContextIssue
                                )
                            }
                        })
                        .map(|(r, _)| *r)
                        .collect();
                    if candidates.is_empty() {
                        break;
                    }
                    let raw = candidates[((seed >> 3) % candidates.len() as u64) as usize];
                    let action = action_at(raw).expect("raw");
                    assemble_input(&pre, &action, &mut input).expect("assemble");
                    let mut edge = Edge::default();
                    transition(&pre, &input, &mut post, &mut edge).expect("model");
                    path.push(raw);
                    pre.copy_from(&post);
                }
                if path.len() == depth {
                    tally.judge_state(&format!("walk{walk}{path:?}"), &pre, None);
                }
            }
            println!("COUNTS {:?}", tally.counts);
            println!("ASSEMBLY {:?}", tally.assembly);
            println!("CORRUPT {:?}", tally.corrupt);
            println!("COVERED {} api/outcome/class keys", tally.coverage.len());
            if std::env::var("XADB_DIFF_COVERAGE").is_ok() {
                for (key, n) in &tally.coverage {
                    println!("COVER {n:>8} {key}");
                }
            }
            for (key, (n, example)) in &tally.failures {
                println!("DISAGREE {n:>6} {key} first={example}");
            }
            assert!(
                tally.failures.is_empty(),
                "{} disagreement kinds",
                tally.failures.len()
            );
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}

/// Replays `XADB_DIFF_REPLAY="raw,raw,...:raw"` from the initial state and prints the final
/// edge with the changed tables, for adjudicating one disagreement.
#[test]
fn differential_replay() {
    let Ok(spec) = std::env::var("XADB_DIFF_REPLAY") else {
        return;
    };
    let (path, last) = spec.split_once(':').expect("path:raw");
    let path: Vec<u16> = path
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().parse().expect("raw"))
        .collect();
    let last: u16 = last.trim().parse().expect("raw");
    let worker = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let literals = Literals::new().expect("literals");
            let mut pre = Box::<State>::default();
            literals.state("initial", &mut pre).expect("initial");
            let mut post = Box::<State>::default();
            let mut input = Box::<ResolvedInput>::default();
            for &raw in &path {
                assemble_input(&pre, &action_at(raw).expect("raw"), &mut input).expect("assemble");
                let mut edge = Edge::default();
                transition(&pre, &input, &mut post, &mut edge).expect("model");
                pre.copy_from(&post);
            }
            let action = action_at(last).expect("raw");
            let mut bytes = vec![0u8; 16 * 1024];
            let mut back = Box::<State>::default();
            let loader = encode_state(&pre, &WireIdentity::default(), &mut bytes)
                .map(|n| decode_state(&bytes[..n], &WireIdentity::default(), &mut back));
            println!("PRE LOADER {loader:?}");
            if let Err(error) = assemble_input(&pre, &action, &mut input) {
                println!("ACTION {action:?}");
                println!("ASSEMBLY {error:?}");
                println!("PRE {pre:#?}");
                return;
            }
            let mut edge = Edge::default();
            transition(&pre, &input, &mut post, &mut edge).expect("model");
            println!("ACTION {action:?}");
            println!("INPUT {input:#?}");
            println!("EDGE {edge:#?}");
            println!("VERDICT {:?}", verify_edge(&pre, &input, &edge, &post));
            macro_rules! diff {
                ($($t:ident),*) => {$(
                    if pre.$t != post.$t {
                        println!("PRE {} {:#?}", stringify!($t), pre.$t.as_slice());
                        println!("POST {} {:#?}", stringify!($t), post.$t.as_slice());
                    }
                )*};
            }
            diff!(
                engines,
                session_incarnations,
                actors,
                prepared_slots,
                issued_statements,
                issued_results,
                private_descriptors,
                contexts,
                transactions,
                obligations,
                bindings,
                receipts,
                owners,
                facts,
                fact_obligations,
                fact_owners
            );
        })
        .expect("worker");
    if let Err(p) = worker.join() {
        std::panic::resume_unwind(p);
    }
}
