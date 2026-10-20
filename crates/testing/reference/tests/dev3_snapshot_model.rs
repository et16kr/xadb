//! Independent D3-M02-P01 partial finite model; no production engine is executed.
//!
//! Frozen actions, fixtures, budgets, exclusions and expected mutants:
//! `devdoc/contracts/dev3-snapshot-model.md`.
use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::time::{Duration, Instant};

const SCOPE: &str = "D3-M02-P01";
const MAX_DEPTH: u8 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixture {
    Registration,
    Unpublished,
    TransientUndo,
    TransientStatus,
}

impl Fixture {
    fn name(self) -> &'static str {
        match self {
            Self::Registration => "registration",
            Self::Unpublished => "unpublished",
            Self::TransientUndo => "transient_undo",
            Self::TransientStatus => "transient_status",
        }
    }

    fn target(self) -> Option<ReferenceKind> {
        match self {
            Self::Registration | Self::Unpublished => None,
            Self::TransientUndo => Some(ReferenceKind::Undo),
            Self::TransientStatus => Some(ReferenceKind::Status),
        }
    }

    fn required_witnesses(self) -> &'static [&'static str] {
        match self {
            Self::Registration => &[
                "q10_selected",
                "registration_gate_blocks_aging",
                "q10_read_a",
                "registration_cancel_clean",
                "registration_failure_clean",
                "guard_overlap",
                "guard_transfer_complete",
                "guard_release_clean",
                "undo_reused",
                "status_reused",
            ],
            Self::Unpublished => &[
                "unpublished_empty_horizon10",
                "q10_read_a",
                "q11_read_b",
                "registration_cancel_clean",
                "registration_failure_clean",
                "guard_overlap",
                "guard_transfer_complete",
                "undo_reused",
                "status_reused",
            ],
            Self::TransientUndo => &[
                "ref_copied",
                "ref_pinned",
                "source_unlatched",
                "pinned_undo_blocks_reuse",
                "ref_loaded_same_generation",
                "reuse_after_blocked_pin_release",
                "undo_reused",
                "q11_read_b",
            ],
            Self::TransientStatus => &[
                "ref_copied",
                "ref_pinned",
                "source_unlatched",
                "pinned_status_blocks_reuse",
                "ref_loaded_same_generation",
                "reuse_after_blocked_pin_release",
                "status_reused",
                "q10_read_a",
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutant {
    None,
    EarlyGateRelease,
    UnpublishedHorizon,
    EmptyRegistryInfinity,
    GuardHandoffGap,
    PinAfterUnlatch,
    IgnoreTransientPin,
}

impl Mutant {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::EarlyGateRelease => "early_gate_release",
            Self::UnpublishedHorizon => "unpublished_horizon",
            Self::EmptyRegistryInfinity => "empty_registry_infinity",
            Self::GuardHandoffGap => "guard_handoff_gap",
            Self::PinAfterUnlatch => "pin_after_unlatch",
            Self::IgnoreTransientPin => "ignore_transient_pin",
        }
    }

    fn expected_error(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::EarlyGateRelease => Some("reader_history_lost"),
            Self::UnpublishedHorizon | Self::EmptyRegistryInfinity => {
                Some("published_history_lost")
            }
            Self::GuardHandoffGap => Some("guard_owner_lost"),
            Self::PinAfterUnlatch | Self::IgnoreTransientPin => {
                Some("transient_generation_changed")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Gate {
    Free,
    Reader,
    Ager,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ReaderPhase {
    Ready,
    Selected,
    Guard,
    Cancelled,
    Released,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ReferenceKind {
    Undo,
    Status,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum ReferencePhase {
    None,
    Copied,
    Pinned,
    Unlatched,
    Loaded,
    Released,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Value {
    A,
    B,
}

/// All mutable semantic and witness-monitor state participates in the key.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct State {
    gate: Gate,
    installed: bool,
    frontier: u8,
    reader: ReaderPhase,
    selected_q: Option<u8>,
    reserved: [bool; 2],
    registered: [Option<u8>; 2],
    transfer: u8,
    sampled_horizon: Option<u8>,
    row_undo_edge: bool,
    row_status_edge: bool,
    undo_incoming: u8,
    status_incoming: u8,
    undo_generation: u8,
    status_generation: u8,
    source_latch: bool,
    reference: ReferencePhase,
    copied_generation: Option<u8>,
    transient_pin: bool,
    blocked_with_pin: bool,
    observed: Option<Value>,
}

impl State {
    fn initial(fixture: Fixture) -> Self {
        let transient = fixture.target().is_some();
        let q = if fixture == Fixture::TransientUndo {
            11
        } else {
            10
        };
        Self {
            gate: if fixture == Fixture::Registration {
                Gate::Reader
            } else {
                Gate::Free
            },
            installed: fixture != Fixture::Registration,
            frontier: q,
            reader: if transient {
                ReaderPhase::Guard
            } else {
                ReaderPhase::Ready
            },
            selected_q: transient.then_some(q),
            reserved: [!transient, false],
            registered: [transient.then_some(q), None],
            transfer: 0,
            sampled_horizon: None,
            row_undo_edge: true,
            row_status_edge: true,
            undo_incoming: 1,
            status_incoming: 1,
            undo_generation: 0,
            status_generation: 0,
            source_latch: false,
            reference: ReferencePhase::None,
            copied_generation: None,
            transient_pin: false,
            blocked_with_pin: false,
            observed: None,
        }
    }

    fn generation(self, kind: ReferenceKind) -> u8 {
        match kind {
            ReferenceKind::Undo => self.undo_generation,
            ReferenceKind::Status => self.status_generation,
        }
    }

    fn edge(self, kind: ReferenceKind) -> bool {
        match kind {
            ReferenceKind::Undo => self.row_undo_edge,
            ReferenceKind::Status => self.row_status_edge,
        }
    }

    /// Candidate visibility follows physical links, independently of expected().
    fn candidate_value(self, q: u8) -> Option<Value> {
        let current_commit = if self.row_status_edge {
            if self.status_generation != 0 {
                // Reused status contains a different transaction's q=12.
                Some(12)
            } else {
                self.installed.then_some(11)
            }
        } else {
            Some(11)
        };
        if current_commit.is_some_and(|commit| commit <= q) {
            Some(Value::B)
        } else if self.row_undo_edge && self.undo_generation == 0 {
            Some(Value::A)
        } else {
            // A reused undo slot holds C, not the required A.
            None
        }
    }

    fn protected_target_without_pin(self, kind: ReferenceKind) -> bool {
        self.gate == Gate::Ager
            && self.sampled_horizon.is_some()
            && !self.edge(kind)
            && self.generation(kind) == 0
            && (kind == ReferenceKind::Status
                || self.sampled_horizon.is_some_and(|horizon| horizon >= 11))
    }
}

/// Literal logical history, never read from candidate storage or counts.
fn expected(q: u8) -> Value {
    match q {
        10 => Value::A,
        11 => Value::B,
        _ => panic!("fixture generated an unsupported snapshot"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    QRead,
    HorizonRegister,
    RegistrationCancel,
    RegistrationFail,
    StatusInstall,
    FrontierPublish,
    HorizonSample,
    Stamp,
    Cut,
    ReuseUndo,
    ReuseStatus,
    AgingFinish,
    GuardTransferBegin,
    GuardTransferRegister,
    GuardTransferReleaseOld,
    GuardRelease,
    CopyRef,
    PinRef,
    SourceUnlatch,
    LoadRef,
    ReleaseRef,
    ReadValue,
}

const ACTIONS: [Action; 22] = [
    Action::QRead,
    Action::HorizonRegister,
    Action::RegistrationCancel,
    Action::RegistrationFail,
    Action::StatusInstall,
    Action::FrontierPublish,
    Action::HorizonSample,
    Action::Stamp,
    Action::Cut,
    Action::ReuseUndo,
    Action::ReuseStatus,
    Action::AgingFinish,
    Action::GuardTransferBegin,
    Action::GuardTransferRegister,
    Action::GuardTransferReleaseOld,
    Action::GuardRelease,
    Action::CopyRef,
    Action::PinRef,
    Action::SourceUnlatch,
    Action::LoadRef,
    Action::ReleaseRef,
    Action::ReadValue,
];

impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::QRead => "q_read",
            Self::HorizonRegister => "horizon_register",
            Self::RegistrationCancel => "registration_cancel",
            Self::RegistrationFail => "registration_fail",
            Self::StatusInstall => "status_install",
            Self::FrontierPublish => "frontier_publish",
            Self::HorizonSample => "horizon_sample",
            Self::Stamp => "stamp",
            Self::Cut => "cut",
            Self::ReuseUndo => "reuse_undo",
            Self::ReuseStatus => "reuse_status",
            Self::AgingFinish => "aging_finish",
            Self::GuardTransferBegin => "guard_transfer_begin",
            Self::GuardTransferRegister => "guard_transfer_register",
            Self::GuardTransferReleaseOld => "guard_transfer_release_old",
            Self::GuardRelease => "guard_release",
            Self::CopyRef => "copy_ref",
            Self::PinRef => "pin_ref",
            Self::SourceUnlatch => "source_unlatch",
            Self::LoadRef => "load_ref",
            Self::ReleaseRef => "release_ref",
            Self::ReadValue => "read_value",
        }
    }
}

fn step(state: State, fixture: Fixture, mutant: Mutant, action: Action) -> Option<State> {
    let mut next = state;
    match action {
        Action::QRead
            if state.reader == ReaderPhase::Ready
                && state.reserved[0]
                && state.gate != Gate::Ager =>
        {
            next.reader = ReaderPhase::Selected;
            next.selected_q = Some(state.frontier);
            next.gate = if mutant == Mutant::EarlyGateRelease {
                Gate::Free
            } else {
                Gate::Reader
            };
        }
        Action::HorizonRegister
            if state.reader == ReaderPhase::Selected
                && (state.gate == Gate::Reader
                    || (mutant == Mutant::EarlyGateRelease && state.gate == Gate::Free)) =>
        {
            next.reader = ReaderPhase::Guard;
            next.registered[0] = state.selected_q;
            next.reserved[0] = false;
            next.gate = Gate::Free;
        }
        Action::RegistrationCancel
            if matches!(state.reader, ReaderPhase::Ready | ReaderPhase::Selected) =>
        {
            next.reader = ReaderPhase::Cancelled;
            next.selected_q = None;
            next.reserved = [false; 2];
            if state.gate == Gate::Reader {
                next.gate = Gate::Free;
            }
        }
        Action::RegistrationFail if state.reader == ReaderPhase::Ready => {
            next.reader = ReaderPhase::Cancelled;
            next.reserved = [false; 2];
            if state.gate == Gate::Reader {
                next.gate = Gate::Free;
            }
        }
        Action::StatusInstall
            if !state.installed && state.gate == Gate::Free && !state.source_latch =>
        {
            next.installed = true;
        }
        Action::FrontierPublish if state.installed && state.frontier == 10 => {
            next.frontier = 11;
        }
        Action::HorizonSample if state.gate == Gate::Free => {
            let mut horizon = if mutant == Mutant::UnpublishedHorizon && state.installed {
                11
            } else {
                state.frontier
            };
            if mutant == Mutant::EmptyRegistryInfinity
                && state.registered.iter().all(Option::is_none)
            {
                horizon = u8::MAX;
            }
            for q in state.registered.into_iter().flatten() {
                horizon = horizon.min(q);
            }
            next.gate = Gate::Ager;
            next.sampled_horizon = Some(horizon);
        }
        Action::Stamp
            if state.gate == Gate::Ager
                && state.installed
                && state.row_status_edge
                && !state.source_latch =>
        {
            next.row_status_edge = false;
            next.status_incoming -= 1;
        }
        Action::Cut
            if state.gate == Gate::Ager
                && state.installed
                && state.row_undo_edge
                && state.sampled_horizon.is_some_and(|horizon| horizon >= 11)
                && !state.source_latch =>
        {
            next.row_undo_edge = false;
            next.undo_incoming -= 1;
        }
        Action::ReuseUndo | Action::ReuseStatus => {
            let kind = if action == Action::ReuseUndo {
                ReferenceKind::Undo
            } else {
                ReferenceKind::Status
            };
            let pinned = fixture.target() == Some(kind) && state.transient_pin;
            if !state.protected_target_without_pin(kind)
                || (pinned && mutant != Mutant::IgnoreTransientPin)
            {
                return None;
            }
            match kind {
                ReferenceKind::Undo => next.undo_generation = 1,
                ReferenceKind::Status => next.status_generation = 1,
            }
        }
        Action::AgingFinish if state.gate == Gate::Ager => {
            next.gate = Gate::Free;
            next.sampled_horizon = None;
        }
        Action::GuardTransferBegin
            if state.reader == ReaderPhase::Guard
                && state.transfer == 0
                && state.gate == Gate::Free
                && !state.source_latch =>
        {
            next.transfer = 1;
            next.reserved[1] = true;
            next.gate = Gate::Reader;
            if mutant == Mutant::GuardHandoffGap {
                next.registered[0] = None;
            }
        }
        Action::GuardTransferRegister
            if state.reader == ReaderPhase::Guard
                && state.transfer == 1
                && state.gate == Gate::Reader =>
        {
            next.transfer = 2;
            next.registered[1] = state.selected_q;
            next.reserved[1] = false;
            next.gate = Gate::Free;
        }
        Action::GuardTransferReleaseOld
            if state.reader == ReaderPhase::Guard && state.transfer == 2 =>
        {
            next.transfer = 3;
            next.registered[0] = None;
        }
        Action::GuardRelease
            if state.reader == ReaderPhase::Guard
                && matches!(state.transfer, 0 | 3)
                && state.gate != Gate::Reader
                && !state.source_latch =>
        {
            next.reader = ReaderPhase::Released;
            next.selected_q = None;
            next.registered = [None; 2];
        }
        Action::CopyRef
            if fixture.target().is_some_and(|kind| state.edge(kind))
                && state.reader == ReaderPhase::Guard
                && state.transfer != 1
                && state.reference == ReferencePhase::None =>
        {
            next.source_latch = true;
            next.reference = ReferencePhase::Copied;
            next.copied_generation = fixture.target().map(|kind| state.generation(kind));
        }
        Action::PinRef
            if (state.reference == ReferencePhase::Copied
                && state.source_latch
                && mutant != Mutant::PinAfterUnlatch)
                || (state.reference == ReferencePhase::Unlatched
                    && !state.transient_pin
                    && mutant == Mutant::PinAfterUnlatch) =>
        {
            next.transient_pin = true;
            if state.source_latch {
                next.reference = ReferencePhase::Pinned;
            }
        }
        Action::SourceUnlatch
            if state.source_latch
                && (state.reference == ReferencePhase::Pinned
                    || (state.reference == ReferencePhase::Copied
                        && mutant == Mutant::PinAfterUnlatch)) =>
        {
            next.source_latch = false;
            next.reference = ReferencePhase::Unlatched;
        }
        Action::LoadRef if state.reference == ReferencePhase::Unlatched && state.transient_pin => {
            next.reference = ReferencePhase::Loaded;
        }
        Action::ReleaseRef if state.reference == ReferencePhase::Loaded => {
            next.reference = ReferencePhase::Released;
            next.transient_pin = false;
        }
        Action::ReadValue
            if state.reader == ReaderPhase::Guard
                && state.transfer != 1
                && state.observed.is_none() =>
        {
            next.observed = state.selected_q.and_then(|q| state.candidate_value(q));
        }
        _ => return None,
    }
    if fixture
        .target()
        .is_some_and(|kind| next.protected_target_without_pin(kind) && next.transient_pin)
    {
        next.blocked_with_pin = true;
    }
    Some(next)
}

fn invariant(state: State, fixture: Fixture) -> Result<(), &'static str> {
    // Reconstruct incoming edges independently of the stored counters.
    let undo_edges = u8::from(state.row_undo_edge);
    let status_edges = u8::from(state.row_status_edge);
    if (undo_edges, status_edges) != (state.undo_incoming, state.status_incoming) {
        return Err("stored_count_mismatch");
    }
    if state
        .reserved
        .into_iter()
        .zip(state.registered)
        .any(|(reserved, registered)| reserved && registered.is_some())
        || state
            .registered
            .into_iter()
            .flatten()
            .any(|q| Some(q) != state.selected_q)
    {
        return Err("registry_invariant");
    }
    if (state.gate == Gate::Ager) != state.sampled_horizon.is_some() {
        return Err("gate_invariant");
    }
    if state.reader == ReaderPhase::Guard && state.registered.iter().all(Option::is_none) {
        return Err("guard_owner_lost");
    }
    if matches!(state.reader, ReaderPhase::Cancelled | ReaderPhase::Released)
        && (state.selected_q.is_some()
            || state.reserved.iter().any(|value| *value)
            || state.registered.iter().any(Option::is_some))
    {
        return Err("released_registry_ownership");
    }
    if let Some(kind) = fixture.target()
        && !matches!(
            state.reference,
            ReferencePhase::None | ReferencePhase::Released
        )
        && state.copied_generation != Some(state.generation(kind))
    {
        return Err("transient_generation_changed");
    }
    if state.candidate_value(state.frontier) != Some(expected(state.frontier)) {
        return Err("published_history_lost");
    }
    if let Some(q) = state.selected_q
        && (state.candidate_value(q) != Some(expected(q))
            || state.observed.is_some_and(|actual| actual != expected(q)))
    {
        return Err("reader_history_lost");
    }
    Ok(())
}

fn witnessed(state: State, fixture: Fixture, action: Action) -> Vec<&'static str> {
    let mut found = Vec::new();
    match action {
        Action::QRead if state.selected_q == Some(10) => {
            found.push("q10_selected");
            if state.gate == Gate::Reader {
                found.push("registration_gate_blocks_aging");
            }
        }
        Action::RegistrationCancel => found.push("registration_cancel_clean"),
        Action::RegistrationFail => found.push("registration_failure_clean"),
        Action::HorizonSample
            if state.installed
                && state.frontier == 10
                && state.registered.iter().all(Option::is_none)
                && state.sampled_horizon == Some(10) =>
        {
            found.push("unpublished_empty_horizon10");
        }
        Action::GuardTransferRegister if state.registered.iter().all(Option::is_some) => {
            found.push("guard_overlap");
        }
        Action::GuardTransferReleaseOld => found.push("guard_transfer_complete"),
        Action::GuardRelease => found.push("guard_release_clean"),
        Action::CopyRef => found.push("ref_copied"),
        Action::PinRef => found.push("ref_pinned"),
        Action::SourceUnlatch => found.push("source_unlatched"),
        Action::LoadRef => found.push("ref_loaded_same_generation"),
        Action::ReadValue if state.selected_q == Some(10) && state.observed == Some(Value::A) => {
            found.push("q10_read_a");
        }
        Action::ReadValue if state.selected_q == Some(11) && state.observed == Some(Value::B) => {
            found.push("q11_read_b");
        }
        _ => (),
    }
    if let Some(kind) = fixture.target()
        && state.protected_target_without_pin(kind)
        && state.transient_pin
    {
        found.push(match kind {
            ReferenceKind::Undo => "pinned_undo_blocks_reuse",
            ReferenceKind::Status => "pinned_status_blocks_reuse",
        });
    }
    if action == Action::ReuseUndo {
        found.push("undo_reused");
    }
    if action == Action::ReuseStatus {
        found.push("status_reused");
    }
    if matches!(action, Action::ReuseUndo | Action::ReuseStatus)
        && state.blocked_with_pin
        && state.reference == ReferencePhase::Released
        && fixture
            .target()
            .is_some_and(|kind| (kind == ReferenceKind::Undo) == (action == Action::ReuseUndo))
    {
        found.push("reuse_after_blocked_pin_release");
    }
    found
}

#[derive(Clone, Copy)]
struct Budget {
    states: usize,
    transitions: usize,
    timeout: Duration,
}

const FROZEN_BUDGET: Budget = Budget {
    states: 1_000_000,
    transitions: 8_000_000,
    timeout: Duration::from_secs(60),
};

struct Node {
    state: State,
    parent: Option<(usize, Action)>,
    depth: u8,
}

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Complete,
    Counterexample,
    Inconclusive,
}

struct Report {
    outcome: Outcome,
    error: Option<&'static str>,
    states: usize,
    transitions: usize,
    max_depth: u8,
    elapsed_ms: u128,
    unexplored: usize,
    witnesses: BTreeMap<&'static str, Vec<Action>>,
    counterexample: Vec<Action>,
}

fn trace(nodes: &[Node], mut at: usize, last: Option<Action>) -> Vec<Action> {
    let mut actions = Vec::new();
    while let Some((parent, action)) = nodes[at].parent {
        actions.push(action);
        at = parent;
    }
    actions.reverse();
    actions.extend(last);
    actions
}

fn explore(fixture: Fixture, mutant: Mutant, budget: Budget) -> Report {
    let started = Instant::now();
    let initial = State::initial(fixture);
    let mut nodes = vec![Node {
        state: initial,
        parent: None,
        depth: 0,
    }];
    let mut seen = HashMap::from([(initial, 0)]);
    let mut report = Report {
        outcome: Outcome::Complete,
        error: None,
        states: 1,
        transitions: 0,
        max_depth: 0,
        elapsed_ms: 0,
        unexplored: 0,
        witnesses: BTreeMap::new(),
        counterexample: Vec::new(),
    };
    invariant(initial, fixture).expect("frozen fixture must be valid");
    let mut cursor = 0;
    'search: while cursor < nodes.len() {
        if started.elapsed() >= budget.timeout || report.states > budget.states {
            report.outcome = Outcome::Inconclusive;
            report.error = Some(if report.states > budget.states {
                "state_budget"
            } else {
                "time_budget"
            });
            report.unexplored = nodes.len() - cursor;
            break;
        }
        let state = nodes[cursor].state;
        let depth = nodes[cursor].depth;
        if depth == MAX_DEPTH {
            cursor += 1;
            continue;
        }
        for action in ACTIONS {
            let Some(next) = step(state, fixture, mutant, action) else {
                continue;
            };
            if report.transitions >= budget.transitions || started.elapsed() >= budget.timeout {
                report.outcome = Outcome::Inconclusive;
                report.error = Some(if report.transitions >= budget.transitions {
                    "transition_budget"
                } else {
                    "time_budget"
                });
                report.unexplored = nodes.len() - cursor;
                break 'search;
            }
            report.transitions += 1;
            report.max_depth = report.max_depth.max(depth + 1);
            if let Err(error) = invariant(next, fixture) {
                report.outcome = Outcome::Counterexample;
                report.error = Some(error);
                report.counterexample = trace(&nodes, cursor, Some(action));
                report.unexplored = nodes.len() - cursor;
                break 'search;
            }
            for name in witnessed(next, fixture, action) {
                report
                    .witnesses
                    .entry(name)
                    .or_insert_with(|| trace(&nodes, cursor, Some(action)));
            }
            if let Entry::Vacant(entry) = seen.entry(next) {
                if report.states >= budget.states {
                    report.outcome = Outcome::Inconclusive;
                    report.error = Some("state_budget");
                    report.unexplored = nodes.len() - cursor + 1;
                    break 'search;
                }
                entry.insert(nodes.len());
                nodes.push(Node {
                    state: next,
                    parent: Some((cursor, action)),
                    depth: depth + 1,
                });
                report.states += 1;
            }
        }
        cursor += 1;
    }
    if report.outcome == Outcome::Complete && started.elapsed() >= budget.timeout {
        report.outcome = Outcome::Inconclusive;
        report.error = Some("time_budget");
    }
    report.elapsed_ms = started.elapsed().as_millis();
    report
}

fn json_string(value: &str) -> String {
    let mut encoded = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            character => encoded.push(character),
        }
    }
    encoded.push('"');
    encoded
}

fn json_trace(actions: &[Action]) -> String {
    format!(
        "[{}]",
        actions
            .iter()
            .map(|action| json_string(action.name()))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn emit(fixture: Fixture, mutant: Mutant, report: &Report, status: &str) {
    let witnesses = report
        .witnesses
        .iter()
        .map(|(name, actions)| format!("{}:{}", json_string(name), json_trace(actions)))
        .collect::<Vec<_>>()
        .join(",");
    let violation = if report.outcome == Outcome::Counterexample {
        report.error
    } else {
        None
    };
    let execution_error = if report.outcome == Outcome::Counterexample {
        None
    } else {
        report.error
    };
    println!(
        "XADB_DEV3_SNAPSHOT_MODEL {{\"scope\":\"{SCOPE}\",\"fixture\":\"{}\",\"mutant\":\"{}\",\"status\":\"{status}\",\"states\":{},\"transitions\":{},\"max_depth\":{},\"bound_depth\":{MAX_DEPTH},\"elapsed_ms\":{},\"unexplored\":{},\"error\":{},\"violation\":{},\"witnesses\":{{{witnesses}}},\"counterexample\":{}}}",
        fixture.name(),
        mutant.name(),
        report.states,
        report.transitions,
        report.max_depth,
        report.elapsed_ms,
        report.unexplored,
        execution_error.map_or_else(|| "null".into(), json_string),
        violation.map_or_else(|| "null".into(), json_string),
        json_trace(&report.counterexample),
    );
}

fn replay(fixture: Fixture, mutant: Mutant, actions: &[Action]) -> Result<(), &'static str> {
    let mut state = State::initial(fixture);
    for action in actions {
        state = step(state, fixture, mutant, *action).ok_or("disabled_trace_action")?;
        invariant(state, fixture)?;
    }
    Ok(())
}

#[test]
fn snapshot_lifecycle_partial_model_exhaustive() {
    let fixtures = [
        Fixture::Registration,
        Fixture::Unpublished,
        Fixture::TransientUndo,
        Fixture::TransientStatus,
    ];
    for fixture in fixtures {
        let report = explore(fixture, Mutant::None, FROZEN_BUDGET);
        let missing = fixture
            .required_witnesses()
            .iter()
            .filter(|name| !report.witnesses.contains_key(*name))
            .collect::<Vec<_>>();
        let status = match report.outcome {
            Outcome::Complete if missing.is_empty() => "PASS",
            Outcome::Inconclusive => "INCONCLUSIVE",
            _ => "FAIL",
        };
        emit(fixture, Mutant::None, &report, status);
        assert_eq!(report.outcome, Outcome::Complete, "{status}");
        assert!(
            missing.is_empty(),
            "required witnesses missing: {missing:?}"
        );
        assert_eq!(report.unexplored, 0);
        for actions in report.witnesses.values() {
            assert_eq!(replay(fixture, Mutant::None, actions), Ok(()));
        }
    }
    let negatives = [
        (Fixture::Registration, Mutant::EarlyGateRelease),
        (Fixture::Unpublished, Mutant::UnpublishedHorizon),
        (Fixture::Unpublished, Mutant::EmptyRegistryInfinity),
        (Fixture::Registration, Mutant::GuardHandoffGap),
        (Fixture::TransientUndo, Mutant::PinAfterUnlatch),
        (Fixture::TransientStatus, Mutant::PinAfterUnlatch),
        (Fixture::TransientUndo, Mutant::IgnoreTransientPin),
        (Fixture::TransientStatus, Mutant::IgnoreTransientPin),
    ];
    for (fixture, mutant) in negatives {
        let report = explore(fixture, mutant, FROZEN_BUDGET);
        let status = if report.outcome == Outcome::Inconclusive {
            "INCONCLUSIVE"
        } else if report.outcome == Outcome::Counterexample
            && report.error == mutant.expected_error()
        {
            "EXPECTED_COUNTEREXAMPLE"
        } else {
            "FAIL"
        };
        emit(fixture, mutant, &report, status);
        assert_eq!(report.outcome, Outcome::Counterexample, "{status}");
        assert_eq!(report.error, mutant.expected_error());
        assert_eq!(
            replay(fixture, mutant, &report.counterexample),
            Err(report.error.expect("counterexample error"))
        );
    }
    println!(
        "XADB_DEV3_SNAPSHOT_COMPLETE {{\"scope\":\"{SCOPE}\",\"status\":\"PASS\",\"normal_cases\":4,\"negative_cases\":8}}"
    );
}

#[test]
fn snapshot_model_budget_exhaustion_is_inconclusive() {
    for (budget, reason) in [
        (
            Budget {
                states: 1,
                ..FROZEN_BUDGET
            },
            "state_budget",
        ),
        (
            Budget {
                transitions: 0,
                ..FROZEN_BUDGET
            },
            "transition_budget",
        ),
        (
            Budget {
                timeout: Duration::ZERO,
                ..FROZEN_BUDGET
            },
            "time_budget",
        ),
    ] {
        let report = explore(Fixture::Registration, Mutant::None, budget);
        assert_eq!(report.outcome, Outcome::Inconclusive);
        assert_eq!(report.error, Some(reason));
        assert!(report.unexplored > 0);
        assert!(report.counterexample.is_empty());
    }
}
