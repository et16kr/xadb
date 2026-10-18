//! Explore all possible linearizations between each invocation and response.
use crate::model::{Model, Pending};
use crate::validation::validate;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
use xadbtest_protocol::history::{Event, History};

/// Search limits; exceeding any is inconclusive, never an accepted/rejected history.
#[derive(Clone, Copy, Debug)]
pub struct SearchBudget {
    /// Total unique states admitted across all event closures.
    pub states: usize,
    /// Total attempted pending-operation linearizations.
    pub transitions: usize,
    /// Maximum unique states in one event closure.
    pub frontier_states: usize,
    /// Conservative retained logical bytes per closure; excludes caller input, the previous frontier and two scratch states.
    pub frontier_bytes: usize,
    /// Monotonic elapsed-time limit, checked during every closure step.
    pub timeout: Duration,
}

impl Default for SearchBudget {
    fn default() -> Self {
        Self {
            states: 1_000_000,
            transitions: 4_000_000,
            frontier_states: 4096,
            frontier_bytes: 64 * 1024 * 1024,
            timeout: Duration::from_secs(30),
        }
    }
}

/// Interpretation of the whole externally observed history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// At least one complete legal linearization matches every observation.
    Accepted,
    /// Exhaustive exploration found no legal matching linearization.
    Rejected,
    /// Invalid controller structure or an unsupported input bound.
    Invalid,
    /// A search/time/memory limit prevented a conclusion.
    Incomplete,
}

/// Bounded search result. Acceptance proves consistency, not unobserved durability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    /// Final classification.
    pub verdict: Verdict,
    /// Zero-based event where validation/exploration stopped; None on acceptance.
    pub event: Option<usize>,
    /// Stable diagnostic, independent of candidate-provided strings.
    pub reason: &'static str,
    /// Unique states admitted to closures over this run.
    pub states: usize,
    /// Attempted model transitions over this run.
    pub transitions: usize,
}

struct Search {
    budget: SearchBudget,
    start: Instant,
    states: usize,
    transitions: usize,
}

impl Search {
    fn result(&self, verdict: Verdict, event: Option<usize>, reason: &'static str) -> CheckResult {
        CheckResult {
            verdict,
            event,
            reason,
            states: self.states,
            transitions: self.transitions,
        }
    }

    fn closure(&mut self, input: Vec<Model>) -> Result<Vec<Model>, &'static str> {
        // Keep one owning copy per retained state and a separate next breadth layer.
        let mut seen = BTreeSet::new();
        let mut bytes = 0usize;
        for state in input {
            self.insert(&mut seen, &mut bytes, state)?;
        }
        // At most eight pending requests can linearize, each once. Scanning already
        // completed states avoids retaining a second full copy of the frontier.
        for _ in 0..8 {
            let mut additions = BTreeSet::new();
            let mut additions_bytes = 0usize;
            for state in &seen {
                if self.start.elapsed() >= self.budget.timeout {
                    return Err("search timeout");
                }
                for (&actor, pending) in &state.pending {
                    if pending.result.is_some() {
                        continue;
                    }
                    self.transitions += 1;
                    if self.transitions > self.budget.transitions {
                        return Err("transition budget");
                    }
                    for next in state.linearize(actor) {
                        if seen.contains(&next) || additions.contains(&next) {
                            continue;
                        }
                        if seen.len() + additions.len() >= self.budget.frontier_states
                            || bytes + additions_bytes + next.retained_bytes()
                                > self.budget.frontier_bytes
                        {
                            return Err("frontier budget");
                        }
                        self.insert(&mut additions, &mut additions_bytes, next)?;
                    }
                }
            }
            if additions.is_empty() {
                break;
            }
            bytes += additions_bytes;
            seen.append(&mut additions);
        }
        Ok(seen.into_iter().collect())
    }

    fn insert(
        &mut self,
        set: &mut BTreeSet<Model>,
        bytes: &mut usize,
        state: Model,
    ) -> Result<(), &'static str> {
        if set.contains(&state) {
            return Ok(());
        }
        if self.start.elapsed() >= self.budget.timeout {
            return Err("search timeout");
        }
        if self.states >= self.budget.states {
            return Err("state budget");
        }
        let size = state.retained_bytes();
        if set.len() >= self.budget.frontier_states || *bytes + size > self.budget.frontier_bytes {
            return Err("frontier budget");
        }
        self.states += 1;
        *bytes += size;
        set.insert(state);
        Ok(())
    }
}

/// Check a controller history against independent snapshot/statement/commit semantics.
///
/// No engine code, snapshot token or claimed sync witness is used. Every pending
/// operation may linearize anywhere inside its observed interval. An uncertain
/// commit's all-or-nothing inclusion is chosen once and retained across later crashes.
/// Resource exhaustion returns [`Verdict::Incomplete`], not a guessed result.
///
/// A history must contain an observation; pending calls must be closed by return or
/// Crash. A final-state match cannot establish preservation of overwritten commits;
/// product acceptance additionally needs independent commit identity/decoder evidence.
pub fn check(history: &History, budget: SearchBudget) -> CheckResult {
    let mut search = Search {
        budget,
        start: Instant::now(),
        states: 0,
        transitions: 0,
    };
    if let Err(reason) = validate(history) {
        return search.result(Verdict::Invalid, None, reason);
    }
    let mut states = vec![Model::new(history.initial.clone())];
    for (index, event) in history.events.iter().enumerate() {
        states = match search.closure(states) {
            Ok(states) => states,
            Err(reason) => return search.result(Verdict::Incomplete, Some(index), reason),
        };
        let mut next_states = Vec::new();
        let mut retained_bytes = 0usize;
        for mut state in states {
            if search.start.elapsed() >= budget.timeout {
                return search.result(Verdict::Incomplete, Some(index), "search timeout");
            }
            match event {
                Event::Invoke {
                    actor,
                    request,
                    action,
                } => {
                    state.pending.insert(
                        *actor,
                        Pending {
                            request: *request,
                            action: action.clone(),
                            result: None,
                        },
                    );
                }
                Event::Return {
                    actor,
                    request,
                    observation,
                } => {
                    let Some(pending) = state.pending.remove(actor) else {
                        continue;
                    };
                    if pending.request != *request || pending.result.as_ref() != Some(observation) {
                        continue;
                    }
                }
                Event::Crash => state.crash(),
            }
            retained_bytes += state.retained_bytes();
            if next_states.len() >= budget.frontier_states || retained_bytes > budget.frontier_bytes
            {
                return search.result(Verdict::Incomplete, Some(index), "event frontier budget");
            }
            next_states.push(state);
        }
        states = next_states;
        if states.is_empty() {
            return search.result(
                Verdict::Rejected,
                Some(index),
                "no legal linearization matches observation",
            );
        }
    }
    search.result(Verdict::Accepted, None, "legal observed history")
}
