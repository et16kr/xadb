//! Independent finite graph models for DEV3 M01/M02.
//! Exact fixtures/actions/budgets: devdoc/contracts/dev3-undo-model.md.
//! This is neither production code nor durability/thread execution evidence.
use std::collections::{BTreeMap, BTreeSet, HashMap, hash_map::Entry};
use std::time::{Duration, Instant};

const DEPTH: u8 = 16;
const STATE_CAP: usize = 1_000_000;
const TRANSITION_CAP: usize = 8_000_000;
const TIME_CAP: Duration = Duration::from_secs(60);
const VACANT: u8 = 0;
const LIVE: u8 = 1;
const RELEASING: u8 = 2;
const FREE: u8 = 3;
const REUSED: u8 = 4;
const ACTIVE: u8 = 1;
const COMMITTED: u8 = 2;
const ABORTED: u8 = 3;
const ROW_UNDO: usize = 0;
const INDEX_UNDO: usize = 2;
const ROW_STATUS: usize = 4;
const INDEX_STATUS: usize = 6;
const UNDO_FIELDS: usize = 8;
const STATUS_HEAD: usize = 20;
const NULL_KEY: u8 = 2;

fn previous_version(slot: usize) -> usize {
    UNDO_FIELDS + 3 * slot
}
fn previous_transaction(slot: usize) -> usize {
    UNDO_FIELDS + 3 * slot + 1
}
fn before_status(slot: usize) -> usize {
    UNDO_FIELDS + 3 * slot + 2
}

#[derive(Clone, Copy, Debug)]
struct Fixture {
    same_row: bool,
    full: bool,
    reader_q: u8,
    after_b: bool,
    ordinal: usize,
}

impl Fixture {
    fn name(self) -> String {
        format!(
            "{}_{}_q{}_{}",
            if self.same_row { "same" } else { "distinct" },
            if self.full { "full" } else { "statement" },
            self.reader_q,
            if self.after_b { "after_b" } else { "before_b" }
        )
    }
    fn status(self, writer: usize) -> usize {
        (self.ordinal + writer) % 4
    }
    fn row(self, write: usize) -> usize {
        if write == 0 || write == 2 || (write == 3 && self.same_row) {
            0
        } else {
            1
        }
    }
    fn key(self, write: usize) -> u8 {
        match write {
            0 => 1,
            1 => NULL_KEY,
            2 => 0,
            3 if self.same_row => NULL_KEY,
            3 => 1,
            _ => unreachable!(),
        }
    }
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = Vec::new();
    for same_row in [false, true] {
        for full in [false, true] {
            for reader_q in [10, 11] {
                for after_b in [false, true] {
                    fixtures.push(Fixture {
                        same_row,
                        full,
                        reader_q,
                        after_b,
                        ordinal: fixtures.len(),
                    });
                }
            }
        }
    }
    fixtures
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutant {
    None,
    StaleBeforeLink,
    MissingIncrement,
    DoubleDecrement,
    CutTransactionLink,
    CutNeededHistory,
    FreeWithIncoming,
}

impl Mutant {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::StaleBeforeLink => "stale_before_link",
            Self::MissingIncrement => "missing_increment",
            Self::DoubleDecrement => "double_decrement",
            Self::CutTransactionLink => "cut_transaction_link",
            Self::CutNeededHistory => "cut_needed_history",
            Self::FreeWithIncoming => "free_with_incoming",
        }
    }
    fn expected_error(self) -> &'static str {
        match self {
            Self::None => "",
            Self::StaleBeforeLink => "dangling_edge",
            Self::MissingIncrement | Self::DoubleDecrement => "incoming_count",
            Self::CutTransactionLink => "rollback_chain",
            Self::CutNeededHistory => "reader_history",
            Self::FreeWithIncoming => "dangling_edge",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Reference {
    slot: u8,
    generation: u8,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Version {
    key: u8,
    /// Zero means the corresponding explicit status edge owns the stamp.
    commit: u8,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct State {
    edges: [Option<Reference>; 24],
    count: [i8; 8],
    generation: [u8; 8],
    lifecycle: [u8; 8],
    status_phase: [u8; 4],
    status_q: [u8; 4],
    frontier: u8,
    /// row0/row1/index0/index1 and undo0..3 before versions.
    versions: [Version; 8],
    captured_previous: [Option<Reference>; 4],
    b_applied: u8,
    /// Independent logical history bits, separate from candidate undo contents.
    history_live: u8,
    rollback_phase: u8,
    rollback_cursor: Option<Reference>,
    horizon: Option<u8>,
    reader_q: Option<u8>,
    queue: Option<u8>,
    queue_cursor: u8,
    cut_seen: bool,
    rollback_before_cut: bool,
    rollback_after_cut: bool,
    reader_observed: bool,
}

impl State {
    fn reference(self, slot: usize) -> Reference {
        Reference {
            slot: slot as u8,
            generation: self.generation[slot],
        }
    }

    /// Candidate accounting is deliberately incremental. The checker below
    /// rebuilds the complete multiset and does not call this routine.
    fn replace_edge(&mut self, field: usize, new: Option<Reference>, mode: Mutant) {
        let old = self.edges[field];
        if old == new {
            return;
        }
        if let Some(old) = old {
            self.count[old.slot as usize] -= 1;
            if mode == Mutant::DoubleDecrement {
                self.count[old.slot as usize] -= 1;
            }
        }
        if let Some(new) = new
            && mode != Mutant::MissingIncrement
        {
            self.count[new.slot as usize] += 1;
        }
        self.edges[field] = new;
    }

    fn status_field(version: usize) -> usize {
        match version {
            0 | 1 => ROW_STATUS + version,
            2 | 3 => INDEX_STATUS + version - 2,
            4..=7 => before_status(version - 4),
            _ => unreachable!(),
        }
    }

    fn resolved_commit(self, version: usize) -> Option<u8> {
        let value = self.versions[version];
        if value.commit != 0 {
            return Some(value.commit);
        }
        let target = self.edges[Self::status_field(version)]?;
        let slot = usize::from(target.slot).checked_sub(4)?;
        (slot < 4
            && self.generation[slot + 4] == target.generation
            && self.status_phase[slot] == COMMITTED)
            .then_some(self.status_q[slot])
    }

    fn write(&mut self, fixture: Fixture, write: usize) {
        let writer = usize::from(write >= 2);
        let status = fixture.status(writer);
        let status_ref = self.reference(status + 4);
        let row = fixture.row(write);
        let mut before = self.versions[row];
        let mut before_txn = self.edges[ROW_STATUS + row];
        // Committed values of another transaction are captured by q, never by
        // retaining an unrelated status slot.
        if before_txn != Some(status_ref)
            && let Some(commit) = self.resolved_commit(row)
        {
            before.commit = commit;
            before_txn = None;
        }
        let old_version = self.edges[ROW_UNDO + row];
        let old_transaction = self.edges[STATUS_HEAD + status];
        self.lifecycle[write] = LIVE;
        self.versions[4 + write] = before;
        self.captured_previous[write] = old_version;
        self.replace_edge(previous_version(write), old_version, Mutant::None);
        self.replace_edge(previous_transaction(write), old_transaction, Mutant::None);
        self.replace_edge(before_status(write), before_txn, Mutant::None);
        let target = self.reference(write);
        for header in [row, INDEX_UNDO + row] {
            self.versions[header] = Version {
                key: fixture.key(write),
                commit: 0,
            };
            self.replace_edge(header, Some(target), Mutant::None);
            self.replace_edge(Self::status_field(header), Some(status_ref), Mutant::None);
        }
        self.replace_edge(STATUS_HEAD + status, Some(target), Mutant::None);
        // These are command-history facts, not derived from stored payload.
        self.history_live |= 1 << write;
        if writer == 1 {
            self.b_applied += 1;
        }
    }

    fn initial(fixture: Fixture) -> Self {
        let mut state = Self {
            edges: [None; 24],
            count: [0; 8],
            generation: [0; 8],
            lifecycle: [VACANT; 8],
            status_phase: [0; 4],
            status_q: [0; 4],
            frontier: 11,
            versions: [Version { key: 0, commit: 10 }; 8],
            captured_previous: [None; 4],
            b_applied: 0,
            history_live: 0,
            rollback_phase: 0,
            rollback_cursor: None,
            horizon: None,
            reader_q: Some(fixture.reader_q),
            queue: None,
            queue_cursor: 0,
            cut_seen: false,
            rollback_before_cut: false,
            rollback_after_cut: false,
            reader_observed: false,
        };
        state.versions[1].key = 1;
        state.versions[3].key = 1;
        for writer in 0..2 {
            let status = fixture.status(writer);
            state.lifecycle[status + 4] = LIVE;
            state.status_phase[status] = ACTIVE;
            state.status_q[status] = 11 + writer as u8;
        }
        state.write(fixture, 0);
        state.write(fixture, 1);
        state.status_phase[fixture.status(0)] = COMMITTED;
        if fixture.after_b {
            state.write(fixture, 2);
            state.write(fixture, 3);
        }
        state
    }

    fn candidate_read(self, row: usize, index: bool, q: u8, own: Option<usize>) -> Option<u8> {
        let header = row + if index { INDEX_UNDO } else { ROW_UNDO };
        let mut version = header;
        let mut next = self.edges[header];
        for _ in 0..=4 {
            let stamp = self.edges[Self::status_field(version)];
            let own_visible = own.is_some_and(|status| {
                stamp == Some(self.reference(status + 4)) && self.status_phase[status] == ACTIVE
            });
            if own_visible
                || self
                    .resolved_commit(version)
                    .is_some_and(|commit| commit <= q)
            {
                return Some(self.versions[version].key);
            }
            let target = next?;
            let slot = usize::from(target.slot);
            if slot >= 4
                || self.generation[slot] != target.generation
                || self.lifecycle[slot] != LIVE
            {
                return None;
            }
            version = 4 + slot;
            next = self.edges[previous_version(slot)];
        }
        None
    }
}

/// The logical expectation applies the four frozen commands, independently of
/// physical headers, status phases, payloads, head traversal or incoming counts.
fn expected(state: State, fixture: Fixture, row: usize, q: u8, own_b: bool) -> u8 {
    let mut value = row as u8;
    for write in 0..4 {
        if fixture.row(write) == row
            && state.history_live & (1 << write) != 0
            && ((write < 2 && q >= 11) || (write >= 2 && (own_b || q >= 12)))
        {
            value = fixture.key(write);
        }
    }
    value
}

fn invariant(state: State, fixture: Fixture) -> Result<(), &'static str> {
    let mut graph_counts = [0_i8; 8];
    for target in state.edges.into_iter().flatten() {
        let slot = usize::from(target.slot);
        if slot >= 8 || state.generation[slot] != target.generation || state.lifecycle[slot] != LIVE
        {
            return Err("dangling_edge");
        }
        graph_counts[slot] += 1;
    }
    if graph_counts != state.count {
        return Err("incoming_count");
    }
    for slot in 0..4 {
        for field in [previous_version(slot), previous_transaction(slot)] {
            if state.edges[field].is_some_and(|target| usize::from(target.slot) >= slot) {
                return Err("undo_order");
            }
        }
    }
    let b_status = fixture.status(1);
    if state.status_phase[b_status] == ACTIVE {
        let mut cursor = state.edges[STATUS_HEAD + b_status];
        let mut actual = 0_u8;
        for _ in 0..4 {
            let Some(target) = cursor else { break };
            let slot = usize::from(target.slot);
            if slot < 2 {
                return Err("rollback_chain");
            }
            actual |= 1 << slot;
            cursor = state.edges[previous_transaction(slot)];
        }
        if actual != state.history_live & 0b1100 {
            return Err("rollback_chain");
        }
    }
    for row in 0..2 {
        for index in [false, true] {
            if state.candidate_read(row, index, state.frontier, None)
                != Some(expected(state, fixture, row, state.frontier, false))
            {
                return Err("published_history");
            }
            if let Some(q) = state.reader_q
                && state.candidate_read(row, index, q, None)
                    != Some(expected(state, fixture, row, q, false))
            {
                return Err("reader_history");
            }
            if state.status_phase[b_status] == ACTIVE
                && state.candidate_read(row, index, state.frontier, Some(b_status))
                    != Some(expected(state, fixture, row, state.frontier, true))
            {
                return Err("own_history");
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    BUpdate,
    RollbackBegin,
    RollbackStep,
    ReaderObserve,
    ReaderRelease,
    HorizonSample,
    Stamp(usize),
    Cut(usize),
    DropHead(usize),
    QueueBegin(usize),
    QueueStep,
    QueueFinish,
    ReuseUndo(usize),
    ReuseStatus(usize),
    AgingFinish,
}

impl Action {
    fn name(self) -> String {
        match self {
            Self::BUpdate => "b_update".into(),
            Self::RollbackBegin => "rollback_begin".into(),
            Self::RollbackStep => "rollback_step".into(),
            Self::ReaderObserve => "reader_observe".into(),
            Self::ReaderRelease => "reader_release".into(),
            Self::HorizonSample => "horizon_sample".into(),
            Self::Stamp(version) => format!("stamp:{version}"),
            Self::Cut(field) => format!("cut:{field}"),
            Self::DropHead(slot) => format!("drop_terminal_head:{slot}"),
            Self::QueueBegin(slot) => format!("queue_begin:{slot}"),
            Self::QueueStep => "queue_step".into(),
            Self::QueueFinish => "queue_finish".into(),
            Self::ReuseUndo(slot) => format!("reuse_undo:{slot}"),
            Self::ReuseStatus(slot) => format!("reuse_status:{slot}"),
            Self::AgingFinish => "aging_finish".into(),
        }
    }
}

fn action_inventory() -> Vec<Action> {
    let mut actions = vec![
        Action::BUpdate,
        Action::RollbackBegin,
        Action::RollbackStep,
        Action::ReaderObserve,
        Action::ReaderRelease,
        Action::HorizonSample,
        Action::QueueStep,
        Action::QueueFinish,
        Action::AgingFinish,
    ];
    actions.extend((0..8).map(Action::Stamp));
    actions.extend((0..4).map(Action::Cut));
    for slot in 0..4 {
        actions.push(Action::Cut(previous_version(slot)));
        actions.push(Action::Cut(previous_transaction(slot)));
        actions.push(Action::DropHead(slot));
        actions.push(Action::QueueBegin(slot));
        actions.push(Action::ReuseUndo(slot));
        actions.push(Action::ReuseStatus(slot));
    }
    actions
}

fn step(state: State, fixture: Fixture, mutant: Mutant, action: Action) -> Option<State> {
    let mut next = state;
    let b = fixture.status(1);
    match action {
        Action::BUpdate
            if state.b_applied < 2 && state.rollback_phase == 0 && state.horizon.is_none() =>
        {
            next.write(fixture, usize::from(state.b_applied) + 2);
        }
        Action::RollbackBegin
            if state.b_applied == 2 && state.rollback_phase == 0 && state.horizon.is_none() =>
        {
            next.rollback_phase = 1;
            next.rollback_cursor = state.edges[STATUS_HEAD + b];
        }
        Action::RollbackStep if state.rollback_phase == 1 && state.horizon.is_none() => {
            let target = state.rollback_cursor?;
            let slot = usize::from(target.slot);
            if slot >= 4 || state.lifecycle[slot] != LIVE {
                return None;
            }
            let row = fixture.row(slot);
            let previous = if mutant == Mutant::StaleBeforeLink {
                state.captured_previous[slot]
            } else {
                state.edges[previous_version(slot)]
            };
            let accounting = if mutant == Mutant::MissingIncrement {
                mutant
            } else {
                Mutant::None
            };
            for header in [row, INDEX_UNDO + row] {
                next.versions[header] = state.versions[slot + 4];
                next.replace_edge(header, previous, accounting);
                next.replace_edge(
                    State::status_field(header),
                    state.edges[before_status(slot)],
                    accounting,
                );
            }
            let previous_transaction = state.edges[previous_transaction(slot)];
            next.replace_edge(STATUS_HEAD + b, previous_transaction, accounting);
            next.rollback_cursor = previous_transaction;
            // Expected undo order comes from the logical command program,
            // independent of the candidate's chosen slot and transaction head.
            let expected_write = if state.history_live & 8 != 0 { 3 } else { 2 };
            next.history_live &= !(1 << expected_write);
            next.rollback_before_cut |= !state.cut_seen;
            next.rollback_after_cut |= state.cut_seen;
            if !fixture.full || next.history_live & 0b1100 == 0 {
                next.rollback_phase = 2;
                next.rollback_cursor = None;
                if fixture.full {
                    next.status_phase[b] = ABORTED;
                }
            }
        }
        Action::ReaderObserve if state.reader_q.is_some() && !state.reader_observed => {
            next.reader_observed = true;
        }
        Action::ReaderRelease if state.reader_q.is_some() => next.reader_q = None,
        Action::HorizonSample if state.horizon.is_none() => {
            next.horizon = Some(state.reader_q.map_or(11, |q| q.min(11)));
        }
        Action::Stamp(version)
            if state.horizon.is_some()
                && state.versions[version].commit == 0
                && (version < 4 || state.lifecycle[version - 4] == LIVE)
                && state.resolved_commit(version).is_some() =>
        {
            next.versions[version].commit = state.resolved_commit(version)?;
            next.replace_edge(State::status_field(version), None, Mutant::None);
        }
        Action::Cut(field) if state.horizon.is_some() && state.edges[field].is_some() => {
            let version = if field < 4 {
                field
            } else if (field - UNDO_FIELDS).is_multiple_of(3) {
                4 + (field - UNDO_FIELDS) / 3
            } else {
                // Only the explicit negative mode cuts an active transaction
                // chain. Normal aging never treats this as a visibility edge.
                if mutant != Mutant::CutTransactionLink
                    || (field - UNDO_FIELDS) % 3 != 1
                    || state.status_phase[b] != ACTIVE
                    || (field - UNDO_FIELDS) / 3 < 2
                {
                    return None;
                }
                next.replace_edge(field, None, Mutant::None);
                next.cut_seen = true;
                return Some(next);
            };
            if !state
                .resolved_commit(version)
                .is_some_and(|q| q <= state.horizon.unwrap() || mutant == Mutant::CutNeededHistory)
            {
                return None;
            }
            next.replace_edge(field, None, Mutant::None);
            next.cut_seen = true;
        }
        Action::DropHead(slot)
            if state.horizon.is_some()
                && matches!(state.status_phase[slot], COMMITTED | ABORTED)
                && state.edges[STATUS_HEAD + slot].is_some() =>
        {
            next.replace_edge(STATUS_HEAD + slot, None, Mutant::None);
        }
        Action::QueueBegin(slot)
            if state.horizon.is_some()
                && state.queue.is_none()
                && state.lifecycle[slot] == LIVE
                && (state.count[slot] == 0 || mutant == Mutant::FreeWithIncoming) =>
        {
            next.lifecycle[slot] = RELEASING;
            next.queue = Some(slot as u8);
            next.queue_cursor = 0;
        }
        Action::QueueStep if state.horizon.is_some() && state.queue_cursor < 3 => {
            let slot = usize::from(state.queue?);
            let field = UNDO_FIELDS + 3 * slot + usize::from(state.queue_cursor);
            next.replace_edge(
                field,
                None,
                if mutant == Mutant::DoubleDecrement {
                    mutant
                } else {
                    Mutant::None
                },
            );
            next.queue_cursor += 1;
        }
        Action::QueueFinish if state.horizon.is_some() && state.queue_cursor == 3 => {
            let slot = usize::from(state.queue?);
            if state.count[slot] != 0 {
                return None;
            }
            next.lifecycle[slot] = FREE;
            next.queue = None;
            next.queue_cursor = 0;
        }
        Action::ReuseUndo(slot)
            if state.horizon.is_some()
                && state.lifecycle[slot] == FREE
                && state.generation[slot] == 0 =>
        {
            next.lifecycle[slot] = REUSED;
            next.generation[slot] = 1;
        }
        Action::ReuseStatus(slot)
            if state.horizon.is_some()
                && matches!(state.status_phase[slot], COMMITTED | ABORTED)
                && state.count[slot + 4] == 0
                && state.edges[STATUS_HEAD + slot].is_none()
                && state.generation[slot + 4] == 0 =>
        {
            next.lifecycle[slot + 4] = REUSED;
            next.generation[slot + 4] = 1;
        }
        Action::AgingFinish if state.horizon.is_some() => next.horizon = None,
        _ => return None,
    }
    Some(next)
}

fn witnesses(state: State, fixture: Fixture) -> Vec<String> {
    let mut names = vec!["index_matches_history".into()];
    if state.lifecycle[..4].iter().all(|phase| *phase == LIVE) {
        names.push("four_undo_live".into());
    }
    if (0..4).any(|slot| {
        state.edges[previous_version(slot)].is_some()
            && state.edges[previous_transaction(slot)].is_some()
    }) {
        names.push("both_previous_links".into());
    }
    if fixture.same_row
        && state.edges[previous_version(3)].is_some()
        && state.edges[previous_version(3)] == state.edges[previous_transaction(3)]
    {
        names.push("parallel_edges_same_target".into());
    }
    if state.reader_observed {
        names.push("reader_observed".into());
    }
    if state.reader_q.is_none() {
        names.push("reader_released".into());
    }
    if state.rollback_before_cut {
        names.push("rollback_before_cut".into());
    }
    if state.rollback_after_cut {
        names.push("rollback_after_cut".into());
    }
    if state.rollback_phase == 2 {
        names.push("rollback_complete".into());
    }
    for slot in 0..4 {
        if state.generation[slot] == 1 {
            names.push(format!("undo_reused_{slot}"));
        }
        if state.generation[slot + 4] == 1 {
            names.push(format!("status_reused_{slot}"));
        }
    }
    names
}

#[derive(Clone)]
struct Node {
    state: State,
    parent: Option<(usize, Action)>,
    depth: u8,
}

fn trace(nodes: &[Node], mut at: usize, last: Option<Action>) -> Vec<Action> {
    let mut path = Vec::new();
    while let Some((parent, action)) = nodes[at].parent {
        path.push(action);
        at = parent;
    }
    path.reverse();
    path.extend(last);
    path
}

struct Report {
    status: &'static str,
    error: Option<&'static str>,
    violation: Option<&'static str>,
    states: usize,
    transitions: usize,
    max_depth: u8,
    elapsed_ms: u128,
    unexplored: usize,
    witnesses: BTreeMap<String, Vec<Action>>,
    counterexample: Vec<Action>,
}

#[derive(Clone, Copy)]
struct Budget {
    states: usize,
    transitions: usize,
    timeout: Duration,
}

const FROZEN_BUDGET: Budget = Budget {
    states: STATE_CAP,
    transitions: TRANSITION_CAP,
    timeout: TIME_CAP,
};

fn explore(fixture: Fixture, mutant: Mutant, budget: Budget) -> Report {
    let started = Instant::now();
    let initial = State::initial(fixture);
    invariant(initial, fixture).expect("invalid frozen fixture");
    let mut nodes = vec![Node {
        state: initial,
        parent: None,
        depth: 0,
    }];
    let mut seen = HashMap::from([(initial, 0)]);
    let mut report = Report {
        status: "PASS",
        error: None,
        violation: None,
        states: 1,
        transitions: 0,
        max_depth: 0,
        elapsed_ms: 0,
        unexplored: 0,
        witnesses: witnesses(initial, fixture)
            .into_iter()
            .map(|name| (name, Vec::new()))
            .collect(),
        counterexample: Vec::new(),
    };
    let actions = action_inventory();
    let mut cursor = 0;
    'search: while cursor < nodes.len() {
        let node = nodes[cursor].clone();
        if node.depth == DEPTH {
            cursor += 1;
            continue;
        }
        for &action in &actions {
            let Some(next) = step(node.state, fixture, mutant, action) else {
                continue;
            };
            if report.transitions >= budget.transitions || started.elapsed() >= budget.timeout {
                report.status = "INCONCLUSIVE";
                report.error = Some(if report.transitions >= budget.transitions {
                    "transition_cap"
                } else {
                    "time_cap"
                });
                report.unexplored = nodes.len() - cursor;
                break 'search;
            }
            report.transitions += 1;
            report.max_depth = report.max_depth.max(node.depth + 1);
            if let Err(violation) = invariant(next, fixture) {
                report.status = if violation == mutant.expected_error() {
                    "EXPECTED_COUNTEREXAMPLE"
                } else {
                    "FAIL"
                };
                report.violation = Some(violation);
                report.counterexample = trace(&nodes, cursor, Some(action));
                report.unexplored = nodes.len() - cursor;
                break 'search;
            }
            for name in witnesses(next, fixture) {
                report
                    .witnesses
                    .entry(name)
                    .or_insert_with(|| trace(&nodes, cursor, Some(action)));
            }
            if let Entry::Vacant(entry) = seen.entry(next) {
                if report.states >= budget.states {
                    report.status = "INCONCLUSIVE";
                    report.error = Some("state_cap");
                    report.unexplored = nodes.len() - cursor + 1;
                    break 'search;
                }
                entry.insert(nodes.len());
                nodes.push(Node {
                    state: next,
                    parent: Some((cursor, action)),
                    depth: node.depth + 1,
                });
                report.states += 1;
            }
        }
        cursor += 1;
    }
    if report.status == "PASS" && started.elapsed() >= budget.timeout {
        report.status = "INCONCLUSIVE";
        report.error = Some("time_cap");
    }
    report.elapsed_ms = started.elapsed().as_millis();
    report
}

fn json_path(path: &[Action]) -> String {
    format!(
        "[{}]",
        path.iter()
            .map(|action| format!("\"{}\"", action.name()))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_optional(value: Option<&str>) -> String {
    value.map_or_else(|| "null".into(), |value| format!("\"{value}\""))
}

fn emit(fixture: Fixture, mutant: Mutant, report: &Report) {
    let names = report
        .witnesses
        .iter()
        .map(|(name, path)| format!("\"{name}\":{}", json_path(path)))
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "XADB_DEV3_UNDO_CASE {{\"scope\":\"D3-M01\",\"fixture\":\"{}\",\"mutant\":\"{}\",\"status\":\"{}\",\"states\":{},\"transitions\":{},\"max_depth\":{},\"bound_depth\":16,\"elapsed_ms\":{},\"unexplored\":{},\"error\":{},\"violation\":{},\"witnesses\":{{{names}}},\"counterexample\":{}}}",
        fixture.name(),
        mutant.name(),
        report.status,
        report.states,
        report.transitions,
        report.max_depth,
        report.elapsed_ms,
        report.unexplored,
        json_optional(report.error),
        json_optional(report.violation),
        json_path(&report.counterexample),
    );
}

fn replay(fixture: Fixture, mutant: Mutant, path: &[Action]) -> Result<State, &'static str> {
    let mut state = State::initial(fixture);
    for &action in path {
        state = step(state, fixture, mutant, action).ok_or("disabled_trace_action")?;
        invariant(state, fixture)?;
    }
    Ok(state)
}

#[test]
fn undo_graph_m01_exhaustive() {
    let all = fixtures();
    let mut aggregate = BTreeSet::new();
    for &fixture in &all {
        let report = explore(fixture, Mutant::None, FROZEN_BUDGET);
        assert_eq!(report.status, "PASS", "{}", fixture.name());
        for required in [
            "four_undo_live",
            "both_previous_links",
            "reader_observed",
            "reader_released",
            "rollback_before_cut",
            "rollback_after_cut",
            "rollback_complete",
            "index_matches_history",
        ] {
            assert!(
                report.witnesses.contains_key(required),
                "{} missing {required}",
                fixture.name()
            );
        }
        if fixture.same_row {
            assert!(report.witnesses.contains_key("parallel_edges_same_target"));
        }
        for (name, path) in &report.witnesses {
            let terminal = replay(fixture, Mutant::None, path).expect("valid witness trace");
            assert!(
                witnesses(terminal, fixture).contains(name),
                "replayed witness predicate is false: {name}"
            );
            aggregate.insert(name.clone());
        }
        emit(fixture, Mutant::None, &report);
    }
    for slot in 0..4 {
        for kind in ["undo", "status"] {
            assert!(
                aggregate.contains(&format!("{kind}_reused_{slot}")),
                "missing actual {kind} generation reuse {slot}"
            );
        }
    }
    for mutant in [
        Mutant::StaleBeforeLink,
        Mutant::MissingIncrement,
        Mutant::DoubleDecrement,
        Mutant::CutTransactionLink,
        Mutant::CutNeededHistory,
        Mutant::FreeWithIncoming,
    ] {
        for same_row in [false, true] {
            let fixture = *all
                .iter()
                .find(|fixture| {
                    fixture.after_b
                        && fixture.same_row == same_row
                        && fixture.full == (!same_row || mutant == Mutant::StaleBeforeLink)
                        && fixture.reader_q
                            == if mutant == Mutant::CutNeededHistory {
                                10
                            } else {
                                11
                            }
                })
                .unwrap();
            let report = explore(fixture, mutant, FROZEN_BUDGET);
            assert_eq!(report.status, "EXPECTED_COUNTEREXAMPLE");
            assert_eq!(report.violation, Some(mutant.expected_error()));
            assert_eq!(
                replay(fixture, mutant, &report.counterexample),
                Err(mutant.expected_error())
            );
            emit(fixture, mutant, &report);
        }
    }
    println!(
        "XADB_DEV3_UNDO_COMPLETE {{\"scope\":\"D3-M01\",\"status\":\"PASS\",\"normal_cases\":16,\"negative_cases\":12}}"
    );
}

#[test]
fn undo_graph_budget_exhaustion_is_inconclusive() {
    let fixture = fixtures()[0];
    for (budget, error) in [
        (
            Budget {
                states: 1,
                ..FROZEN_BUDGET
            },
            "state_cap",
        ),
        (
            Budget {
                transitions: 0,
                ..FROZEN_BUDGET
            },
            "transition_cap",
        ),
        (
            Budget {
                timeout: Duration::ZERO,
                ..FROZEN_BUDGET
            },
            "time_cap",
        ),
    ] {
        let report = explore(fixture, Mutant::None, budget);
        assert_eq!(report.status, "INCONCLUSIVE");
        assert_eq!(report.error, Some(error));
        assert!(report.unexplored > 0);
        assert!(report.violation.is_none() && report.counterexample.is_empty());
    }
}

/// M02 adds registry acquisition and moving reference pins to the independently
/// checked graph. It does not replace the earlier D3-M02-P01 test binary.
mod snapshot_graph {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Family {
        Registration,
        Unpublished,
        Chain,
        Handoff,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Route {
        UndoUndo,
        StatusUndo,
        UndoStatus,
    }

    #[derive(Clone, Copy, Debug)]
    struct Case {
        family: Family,
        route: Route,
        index: bool,
        same: bool,
        ordinal: usize,
    }

    impl Case {
        fn core(self) -> Fixture {
            Fixture {
                same_row: self.same,
                full: false,
                reader_q: if matches!(self.family, Family::Chain | Family::Handoff) {
                    12
                } else {
                    10
                },
                after_b: matches!(self.family, Family::Chain | Family::Handoff),
                ordinal: self.ordinal,
            }
        }
        fn name(self) -> String {
            let family = match self.family {
                Family::Registration => "registration",
                Family::Unpublished => "unpublished",
                Family::Chain => "chain",
                Family::Handoff => "handoff",
            };
            if matches!(self.family, Family::Registration | Family::Unpublished) {
                format!("{family}_{}", if self.same { "same" } else { "distinct" })
            } else {
                format!(
                    "{family}_{}_{}",
                    match self.route {
                        Route::UndoUndo => "undo_undo",
                        Route::StatusUndo => "status_undo",
                        Route::UndoStatus => "undo_status",
                    },
                    if self.index { "index" } else { "row" }
                )
            }
        }
        fn root_header(self) -> usize {
            let row = usize::from(!self.same);
            row + if self.index { INDEX_UNDO } else { ROW_UNDO }
        }
        fn root_field(self) -> usize {
            let header = self.root_header();
            if self.route == Route::StatusUndo {
                State::status_field(header)
            } else {
                header
            }
        }
        fn next_field(self, old: Reference) -> usize {
            let slot = usize::from(old.slot);
            match self.route {
                Route::UndoUndo => previous_version(slot),
                Route::StatusUndo => STATUS_HEAD + slot - 4,
                Route::UndoStatus => before_status(slot),
            }
        }
        fn required(self) -> Vec<&'static str> {
            match self.family {
                Family::Registration | Family::Unpublished => {
                    let mut names = vec![
                        "q10_selected",
                        "registration_gate_blocks_aging",
                        "q10_read",
                        "registration_cancel_clean",
                        "registration_failure_clean",
                        "guard_overlap",
                        "guard_transfer_complete",
                        "two_writers_published",
                    ];
                    if self.family == Family::Unpublished {
                        names.extend(["q12_read", "empty_registry_frontier10_retained"]);
                    }
                    names
                }
                Family::Chain => vec![
                    "root_copied",
                    "old_pin_before_unlatch",
                    "old_loaded",
                    "next_copied",
                    "two_pins_overlap",
                    "old_released_after_next_pin",
                    "next_loaded",
                    "all_pins_released",
                ],
                Family::Handoff => vec![
                    "next_copied",
                    "two_pins_overlap",
                    "next_target_reuse_blocked",
                    "next_loaded",
                    "next_target_reused_after_pin_release",
                ],
            }
        }
    }

    fn cases() -> Vec<Case> {
        let mut all = Vec::new();
        for family in [Family::Registration, Family::Unpublished] {
            for same in [false, true] {
                all.push(Case {
                    family,
                    route: if same {
                        Route::UndoStatus
                    } else {
                        Route::UndoUndo
                    },
                    index: false,
                    same,
                    ordinal: all.len(),
                });
            }
        }
        for route in [Route::UndoUndo, Route::StatusUndo, Route::UndoStatus] {
            for index in [false, true] {
                all.push(Case {
                    family: Family::Chain,
                    route,
                    index,
                    same: route == Route::UndoStatus,
                    ordinal: all.len(),
                });
            }
        }
        for route in [Route::UndoUndo, Route::StatusUndo, Route::UndoStatus] {
            all.push(Case {
                family: Family::Handoff,
                route,
                index: false,
                same: route == Route::UndoStatus,
                ordinal: all.len(),
            });
        }
        all
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Fault {
        None,
        EarlyGate,
        InstalledHorizon,
        InfiniteHorizon,
        GuardGap,
        LateOldPin,
        SnapshotOnlyPin,
        IgnorePin,
        NextPinGap,
    }

    impl Fault {
        fn name(self) -> &'static str {
            match self {
                Self::None => "none",
                Self::EarlyGate => "early_gate_release",
                Self::InstalledHorizon => "unpublished_horizon",
                Self::InfiniteHorizon => "empty_registry_infinity",
                Self::GuardGap => "guard_handoff_gap",
                Self::LateOldPin => "pin_after_unlatch",
                Self::SnapshotOnlyPin => "snapshot_only_pin",
                Self::IgnorePin => "ignore_transient_pin",
                Self::NextPinGap => "next_pin_handoff_gap",
            }
        }
        fn expected(self) -> &'static str {
            match self {
                Self::None => "",
                Self::EarlyGate => "reader_history",
                Self::InstalledHorizon | Self::InfiniteHorizon => "published_history",
                Self::GuardGap => "guard_owner_lost",
                _ => "transient_lifetime",
            }
        }
    }

    #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
    struct Snapshot {
        core: State,
        // 0 Ready, 1 Selected, 2 live guard, 3 released, 4 cancel, 5 failure.
        reader: u8,
        registry: [Option<u8>; 2],
        reserved: [bool; 2],
        reader_gate: bool,
        transfer: u8,
        observed_q: Option<u8>,
        // Source identities: headers0..3, undo4..7, status8..11.
        source_latch: Option<usize>,
        old: Option<Reference>,
        next: Option<Reference>,
        pins: [Option<Reference>; 2],
        pc: u8,
        blocked_next: bool,
    }

    #[derive(Clone, Copy, Debug)]
    enum Event {
        QRead,
        Register,
        Cancel,
        Fail,
        Install(usize),
        Publish(usize),
        Sample,
        TransferBegin,
        TransferRegister,
        TransferRelease,
        GuardRelease,
        Observe,
        CopyRoot,
        PinOld,
        UnlatchRoot,
        LoadOld,
        CopyNext,
        PinNext,
        ReleaseOld,
        UnlatchNext,
        LoadNext,
        ReleaseNext,
        Core(Action),
    }

    impl Event {
        fn name(self) -> String {
            match self {
                Self::QRead => "q_read".into(),
                Self::Register => "horizon_register".into(),
                Self::Cancel => "registration_cancel".into(),
                Self::Fail => "registration_fail".into(),
                Self::Install(writer) => format!("status_install:{writer}"),
                Self::Publish(writer) => format!("frontier_publish:{writer}"),
                Self::Sample => "horizon_sample".into(),
                Self::TransferBegin => "guard_transfer_begin".into(),
                Self::TransferRegister => "guard_transfer_register".into(),
                Self::TransferRelease => "guard_transfer_release_old".into(),
                Self::GuardRelease => "guard_release".into(),
                Self::Observe => "reader_observe".into(),
                Self::CopyRoot => "copy_root".into(),
                Self::PinOld => "pin_old".into(),
                Self::UnlatchRoot => "unlatch_root".into(),
                Self::LoadOld => "load_old".into(),
                Self::CopyNext => "copy_next".into(),
                Self::PinNext => "pin_next".into(),
                Self::ReleaseOld => "release_old".into(),
                Self::UnlatchNext => "unlatch_next".into(),
                Self::LoadNext => "load_next".into(),
                Self::ReleaseNext => "release_next".into(),
                Self::Core(action) => action.name(),
            }
        }
    }

    fn events() -> Vec<Event> {
        let mut all = vec![
            Event::QRead,
            Event::Register,
            Event::Cancel,
            Event::Fail,
            Event::Install(0),
            Event::Install(1),
            Event::Publish(0),
            Event::Publish(1),
            Event::Sample,
            Event::TransferBegin,
            Event::TransferRegister,
            Event::TransferRelease,
            Event::GuardRelease,
            Event::Observe,
            Event::CopyRoot,
            Event::PinOld,
            Event::UnlatchRoot,
            Event::LoadOld,
            Event::CopyNext,
            Event::PinNext,
            Event::ReleaseOld,
            Event::UnlatchNext,
            Event::LoadNext,
            Event::ReleaseNext,
        ];
        all.extend(action_inventory().into_iter().filter_map(|action| {
            (!matches!(
                action,
                Action::HorizonSample
                    | Action::ReaderObserve
                    | Action::ReaderRelease
                    | Action::RollbackBegin
                    | Action::RollbackStep
            ))
            .then_some(Event::Core(action))
        }));
        all
    }

    fn source_of(reference: Reference) -> usize {
        usize::from(reference.slot) + 4
    }

    fn source_changed(before: State, after: State, source: usize) -> bool {
        if source < 4 {
            before.versions[source] != after.versions[source]
                || before.edges[source] != after.edges[source]
                || before.edges[State::status_field(source)]
                    != after.edges[State::status_field(source)]
        } else if source < 8 {
            let slot = source - 4;
            before.versions[source] != after.versions[source]
                || before.edges[previous_version(slot)..=before_status(slot)]
                    != after.edges[previous_version(slot)..=before_status(slot)]
                || before.lifecycle[slot] != after.lifecycle[slot]
                || before.generation[slot] != after.generation[slot]
                || before.count[slot] != after.count[slot]
        } else {
            let slot = source - 8;
            before.edges[STATUS_HEAD + slot] != after.edges[STATUS_HEAD + slot]
                || before.lifecycle[slot + 4] != after.lifecycle[slot + 4]
                || before.generation[slot + 4] != after.generation[slot + 4]
                || before.status_phase[slot] != after.status_phase[slot]
                || before.count[slot + 4] != after.count[slot + 4]
        }
    }

    fn eligible_except_pin(state: Snapshot, reference: Reference) -> bool {
        let slot = usize::from(reference.slot);
        state.core.horizon.is_some()
            && state.core.lifecycle[slot] == LIVE
            && state.core.generation[slot] == reference.generation
            && state.core.count[slot] == 0
            && if slot < 4 {
                state.core.queue.is_none()
            } else {
                matches!(state.core.status_phase[slot - 4], COMMITTED | ABORTED)
                    && state.core.edges[STATUS_HEAD + slot - 4].is_none()
            }
    }

    fn transition(state: Snapshot, case: Case, fault: Fault, event: Event) -> Option<Snapshot> {
        let mut next = state;
        let fixture = case.core();
        match event {
            Event::QRead
                if state.reader == 0 && state.reserved[0] && state.core.horizon.is_none() =>
            {
                next.reader = 1;
                next.core.reader_q = Some(state.core.frontier);
                next.reader_gate = fault != Fault::EarlyGate;
            }
            Event::Register
                if state.reader == 1
                    && state.core.horizon.is_none()
                    && (state.reader_gate || fault == Fault::EarlyGate) =>
            {
                next.reader = 2;
                next.registry[0] = state.core.reader_q;
                next.reserved[0] = false;
                next.reader_gate = false;
            }
            Event::Cancel if state.reader <= 1 => {
                next.reader = 4;
                next.core.reader_q = None;
                next.reserved = [false; 2];
                next.reader_gate = false;
            }
            Event::Fail if state.reader == 0 => {
                next.reader = 5;
                next.reserved = [false; 2];
                next.reader_gate = false;
            }
            Event::Install(writer)
                if !state.reader_gate
                    && state.core.horizon.is_none()
                    && state.core.status_phase[fixture.status(writer)] == ACTIVE
                    && (writer == 0
                        || (state.core.b_applied == 2 && state.core.frontier >= 11)) =>
            {
                next.core.status_phase[fixture.status(writer)] = COMMITTED;
            }
            Event::Publish(writer)
                if state.core.status_phase[fixture.status(writer)] == COMMITTED
                    && state.core.frontier == 10 + writer as u8 =>
            {
                next.core.frontier += 1;
            }
            Event::Sample if !state.reader_gate && state.core.horizon.is_none() => {
                let mut horizon = state.core.frontier;
                if fault == Fault::InstalledHorizon {
                    for writer in 0..2 {
                        if state.core.status_phase[fixture.status(writer)] == COMMITTED {
                            horizon = horizon.max(11 + writer as u8);
                        }
                    }
                }
                if fault == Fault::InfiniteHorizon && state.registry == [None; 2] {
                    horizon = u8::MAX;
                }
                for q in state.registry.into_iter().flatten() {
                    horizon = horizon.min(q);
                }
                next.core.horizon = Some(horizon);
            }
            Event::TransferBegin
                if state.reader == 2
                    && state.transfer == 0
                    && !state.reader_gate
                    && state.core.horizon.is_none()
                    && state.source_latch.is_none() =>
            {
                next.transfer = 1;
                next.reader_gate = true;
                next.reserved[1] = true;
                if fault == Fault::GuardGap {
                    next.registry[0] = None;
                }
            }
            Event::TransferRegister if state.transfer == 1 && state.reader_gate => {
                next.transfer = 2;
                next.registry[1] = state.core.reader_q;
                next.reserved[1] = false;
                next.reader_gate = false;
            }
            Event::TransferRelease if state.transfer == 2 => {
                next.transfer = 3;
                next.registry[0] = None;
            }
            Event::GuardRelease
                if state.reader == 2
                    && matches!(state.transfer, 0 | 3)
                    && !state.reader_gate
                    && state.source_latch.is_none() =>
            {
                next.reader = 3;
                next.registry = [None; 2];
                next.core.reader_q = None;
            }
            Event::Observe
                if state.reader == 2 && state.transfer != 1 && state.observed_q.is_none() =>
            {
                next.observed_q = state.core.reader_q;
            }
            Event::CopyRoot
                if state.reader == 2
                    && state.transfer != 1
                    && state.source_latch.is_none()
                    && state.pc == 0
                    && state.core.edges[case.root_field()].is_some() =>
            {
                next.source_latch = Some(case.root_header());
                next.old = state.core.edges[case.root_field()];
                next.pc = 1;
            }
            Event::PinOld
                if (state.pc == 1 && fault != Fault::LateOldPin)
                    || (state.pc == 3 && fault == Fault::LateOldPin && state.pins[0].is_none()) =>
            {
                if fault != Fault::SnapshotOnlyPin {
                    next.pins[0] = state.old;
                }
                if state.pc == 1 {
                    next.pc = 2;
                }
            }
            Event::UnlatchRoot
                if state.pc == 2 || (state.pc == 1 && fault == Fault::LateOldPin) =>
            {
                next.source_latch = None;
                next.pc = 3;
            }
            Event::LoadOld
                if state.pc == 3
                    && (state.pins[0].is_some() || fault == Fault::SnapshotOnlyPin) =>
            {
                next.pc = 4;
            }
            Event::CopyNext if state.pc == 4 && state.source_latch.is_none() => {
                let old = state.old?;
                let target = state.core.edges[case.next_field(old)]?;
                next.source_latch = Some(source_of(old));
                next.next = Some(target);
                next.pc = 5;
            }
            Event::PinNext
                if state.pc == 5
                    || (state.pc == 8 && fault == Fault::NextPinGap && state.pins[1].is_none()) =>
            {
                if fault != Fault::SnapshotOnlyPin {
                    next.pins[1] = state.next;
                }
                if state.pc == 5 {
                    next.pc = 6;
                }
            }
            Event::ReleaseOld if state.pc == 6 || (state.pc == 5 && fault == Fault::NextPinGap) => {
                next.pins[0] = None;
                next.pc = 7;
            }
            Event::UnlatchNext
                if state.pc == 7
                    && (state.pins[1].is_some()
                        || matches!(fault, Fault::NextPinGap | Fault::SnapshotOnlyPin)) =>
            {
                next.source_latch = None;
                next.pc = 8;
            }
            Event::LoadNext
                if state.pc == 8
                    && (state.pins[1].is_some() || fault == Fault::SnapshotOnlyPin) =>
            {
                next.pc = 9;
            }
            Event::ReleaseNext if state.pc == 9 => {
                next.pins[1] = None;
                next.pc = 10;
            }
            Event::Core(action) if !state.reader_gate => {
                if action == Action::BUpdate && state.core.frontier < 11 {
                    return None;
                }
                next.core = step(state.core, fixture, Mutant::None, action)?;
                if fault != Fault::IgnorePin
                    && state.pins.into_iter().flatten().any(|target| {
                        let slot = usize::from(target.slot);
                        next.core.lifecycle[slot] != LIVE
                            || next.core.generation[slot] != target.generation
                    })
                {
                    return None;
                }
            }
            _ => return None,
        }
        if state
            .source_latch
            .is_some_and(|source| source_changed(state.core, next.core, source))
        {
            return None;
        }
        if next.pins[1].is_some_and(|target| eligible_except_pin(next, target)) {
            next.blocked_next = true;
        }
        Some(next)
    }

    fn check(state: Snapshot, case: Case) -> Result<(), &'static str> {
        if state.reader == 2 && state.registry == [None; 2] {
            return Err("guard_owner_lost");
        }
        if state
            .registry
            .into_iter()
            .flatten()
            .any(|q| Some(q) != state.core.reader_q)
            || state
                .registry
                .into_iter()
                .zip(state.reserved)
                .any(|(q, reserved)| q.is_some() && reserved)
            || (state.reader_gate && state.core.horizon.is_some())
        {
            return Err("registry_invariant");
        }
        for (target, needed) in [
            (state.old, (1..=7).contains(&state.pc)),
            (state.next, (5..=9).contains(&state.pc)),
        ] {
            if let Some(target) = target
                && needed
                && (state.core.generation[usize::from(target.slot)] != target.generation
                    || state.core.lifecycle[usize::from(target.slot)] != LIVE)
            {
                return Err("transient_lifetime");
            }
        }
        invariant(state.core, case.core())
    }

    fn initial(case: Case) -> (Snapshot, Vec<Event>) {
        let fixture = case.core();
        let mut core = State::initial(fixture);
        let live = matches!(case.family, Family::Chain | Family::Handoff);
        if live {
            core.status_phase[fixture.status(1)] = COMMITTED;
            core.frontier = 12;
        } else {
            core.frontier = 10;
            core.reader_q = None;
            if case.family == Family::Registration {
                core.status_phase[fixture.status(0)] = ACTIVE;
            }
        }
        let mut state = Snapshot {
            core,
            reader: if live { 2 } else { 0 },
            registry: [live.then_some(12), None],
            reserved: [!live, false],
            reader_gate: case.family == Family::Registration,
            transfer: 0,
            observed_q: None,
            source_latch: None,
            old: None,
            next: None,
            pins: [None; 2],
            pc: 0,
            blocked_next: false,
        };
        check(state, case).expect("frozen initial snapshot graph");
        let mut prelude = Vec::new();
        if case.family == Family::Handoff {
            for event in [
                Event::CopyRoot,
                Event::PinOld,
                Event::UnlatchRoot,
                Event::LoadOld,
                Event::Sample,
            ] {
                state = transition(state, case, Fault::None, event).expect("frozen prelude");
                check(state, case).expect("prelude invariant");
                prelude.push(event);
            }
            let keep = case.next_field(state.old.unwrap());
            let mut cleanup = (0..8)
                .filter(|version| State::status_field(*version) != keep)
                .map(|version| Event::Core(Action::Stamp(version)))
                .collect::<Vec<_>>();
            cleanup.extend(
                (0..4)
                    .chain((0..4).map(previous_version))
                    .filter(|field| *field != keep)
                    .map(|field| Event::Core(Action::Cut(field))),
            );
            cleanup.extend(
                (0..4)
                    .filter(|slot| STATUS_HEAD + slot != keep)
                    .map(|slot| Event::Core(Action::DropHead(slot))),
            );
            cleanup.push(Event::Core(Action::AgingFinish));
            for event in cleanup {
                if let Some(next) = transition(state, case, Fault::None, event) {
                    check(next, case).expect("cleanup prelude invariant");
                    state = next;
                    prelude.push(event);
                }
            }
        }
        (state, prelude)
    }

    fn observed(state: Snapshot) -> Vec<String> {
        let mut names = Vec::new();
        if state.reader == 1 && state.core.reader_q == Some(10) {
            names.push("q10_selected");
            if state.reader_gate {
                names.push("registration_gate_blocks_aging");
            }
        }
        if state.observed_q == Some(10) {
            names.push("q10_read");
        }
        if state.observed_q == Some(12) {
            names.push("q12_read");
        }
        if state.reader == 4 {
            names.push("registration_cancel_clean");
        }
        if state.reader == 5 {
            names.push("registration_failure_clean");
        }
        if state.registry.iter().all(Option::is_some) {
            names.push("guard_overlap");
        }
        if state.transfer == 3 {
            names.push("guard_transfer_complete");
        }
        if state.core.frontier == 12 {
            names.push("two_writers_published");
        }
        if state.core.frontier == 10
            && state.registry == [None; 2]
            && state.core.horizon == Some(10)
        {
            names.push("empty_registry_frontier10_retained");
        }
        match state.pc {
            1 => names.push("root_copied"),
            2 if state.pins[0].is_some() => names.push("old_pin_before_unlatch"),
            4 => names.push("old_loaded"),
            5 => names.push("next_copied"),
            6 if state.pins.iter().all(Option::is_some) => names.push("two_pins_overlap"),
            7 if state.pins[1].is_some() => names.push("old_released_after_next_pin"),
            9 => names.push("next_loaded"),
            10 => names.push("all_pins_released"),
            _ => (),
        }
        if state.pins[1].is_some_and(|target| eligible_except_pin(state, target)) {
            names.push("next_target_reuse_blocked");
        }
        if state.blocked_next
            && state.pc == 10
            && state.next.is_some_and(|target| {
                state.core.generation[usize::from(target.slot)] != target.generation
                    && state.core.lifecycle[usize::from(target.slot)] == REUSED
            })
        {
            names.push("next_target_reused_after_pin_release");
        }
        names.into_iter().map(str::to_owned).collect()
    }

    struct SnapshotNode {
        state: Snapshot,
        parent: Option<(usize, Event)>,
        depth: u8,
    }

    fn path(nodes: &[SnapshotNode], mut at: usize, last: Option<Event>) -> Vec<Event> {
        let mut events = Vec::new();
        while let Some((parent, event)) = nodes[at].parent {
            events.push(event);
            at = parent;
        }
        events.reverse();
        events.extend(last);
        events
    }

    struct SnapshotReport {
        status: &'static str,
        error: Option<&'static str>,
        violation: Option<&'static str>,
        states: usize,
        transitions: usize,
        depth: u8,
        elapsed_ms: u128,
        unexplored: usize,
        prelude: Vec<Event>,
        witnesses: BTreeMap<String, Vec<Event>>,
        counterexample: Vec<Event>,
    }

    fn explore_snapshot(case: Case, fault: Fault, budget: Budget) -> SnapshotReport {
        let started = Instant::now();
        let (state, prelude) = initial(case);
        let mut report = SnapshotReport {
            status: "PASS",
            error: None,
            violation: None,
            states: 1,
            transitions: 0,
            depth: 0,
            elapsed_ms: 0,
            unexplored: 0,
            prelude,
            witnesses: observed(state)
                .into_iter()
                .map(|name| (name, Vec::new()))
                .collect(),
            counterexample: Vec::new(),
        };
        let mut nodes = vec![SnapshotNode {
            state,
            parent: None,
            depth: 0,
        }];
        let mut seen = HashMap::from([(snapshot_key(state), 0)]);
        let inventory = events();
        let mut cursor = 0;
        'search: while cursor < nodes.len() {
            let state = nodes[cursor].state;
            let depth = nodes[cursor].depth;
            if depth == DEPTH {
                cursor += 1;
                continue;
            }
            for &event in &inventory {
                let Some(next) = transition(state, case, fault, event) else {
                    continue;
                };
                if report.transitions >= budget.transitions || started.elapsed() >= budget.timeout {
                    report.status = "INCONCLUSIVE";
                    report.error = Some(if report.transitions >= budget.transitions {
                        "transition_cap"
                    } else {
                        "time_cap"
                    });
                    report.unexplored = nodes.len() - cursor;
                    break 'search;
                }
                report.transitions += 1;
                report.depth = report.depth.max(depth + 1);
                if let Err(violation) = check(next, case) {
                    report.status = if violation == fault.expected() {
                        "EXPECTED_COUNTEREXAMPLE"
                    } else {
                        "FAIL"
                    };
                    report.violation = Some(violation);
                    report.counterexample = path(&nodes, cursor, Some(event));
                    report.unexplored = nodes.len() - cursor;
                    break 'search;
                }
                for name in observed(next) {
                    report
                        .witnesses
                        .entry(name)
                        .or_insert_with(|| path(&nodes, cursor, Some(event)));
                }
                if let Entry::Vacant(entry) = seen.entry(snapshot_key(next)) {
                    if report.states >= budget.states {
                        report.status = "INCONCLUSIVE";
                        report.error = Some("state_cap");
                        report.unexplored = nodes.len() - cursor + 1;
                        break 'search;
                    }
                    entry.insert(nodes.len());
                    nodes.push(SnapshotNode {
                        state: next,
                        parent: Some((cursor, event)),
                        depth: depth + 1,
                    });
                    report.states += 1;
                }
            }
            cursor += 1;
        }
        if report.status == "PASS" && started.elapsed() >= budget.timeout {
            report.status = "INCONCLUSIVE";
            report.error = Some("time_cap");
        }
        if report.status == "PASS" && fault != Fault::None {
            report.status = "FAIL";
            report.error = Some("expected_counterexample_missing");
        }
        if report.status == "PASS"
            && case
                .required()
                .iter()
                .any(|name| !report.witnesses.contains_key(*name))
        {
            report.status = "FAIL";
            report.error = Some("required_witness_missing");
        }
        report.elapsed_ms = started.elapsed().as_millis();
        report
    }

    // Only this observation ghost is erased. Every non-Observe successor and
    // safety predicate is independent of it. Observe is a stutter on that
    // state; its actual successor witness is collected before key merging.
    // Nodes and replay traces retain the unmodified state.
    fn snapshot_key(mut state: Snapshot) -> Snapshot {
        state.observed_q = None;
        state
    }

    fn json_events(events: &[Event]) -> String {
        format!(
            "[{}]",
            events
                .iter()
                .map(|event| format!("\"{}\"", event.name()))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn emit_snapshot(case: Case, fault: Fault, report: &SnapshotReport) {
        let names = report
            .witnesses
            .iter()
            .map(|(name, path)| format!("\"{name}\":{}", json_events(path)))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "XADB_DEV3_GRAPH_SNAPSHOT_CASE {{\"scope\":\"D3-M02\",\"fixture\":\"{}\",\"mutant\":\"{}\",\"status\":\"{}\",\"states\":{},\"transitions\":{},\"max_depth\":{},\"bound_depth\":16,\"elapsed_ms\":{},\"unexplored\":{},\"error\":{},\"violation\":{},\"prelude\":{},\"witnesses\":{{{names}}},\"counterexample\":{}}}",
            case.name(),
            fault.name(),
            report.status,
            report.states,
            report.transitions,
            report.depth,
            report.elapsed_ms,
            report.unexplored,
            json_optional(report.error),
            json_optional(report.violation),
            json_events(&report.prelude),
            json_events(&report.counterexample),
        );
    }

    fn replay_snapshot(
        case: Case,
        fault: Fault,
        trace: &[Event],
    ) -> Result<Snapshot, &'static str> {
        let (mut state, _) = initial(case);
        for &event in trace {
            state = transition(state, case, fault, event).ok_or("disabled_trace_action")?;
            check(state, case)?;
        }
        Ok(state)
    }

    #[test]
    fn snapshot_graph_m02_exhaustive() {
        let all = cases();
        assert_eq!(all.len(), 13);
        let mut failures = Vec::new();
        for &case in &all {
            let report = explore_snapshot(case, Fault::None, FROZEN_BUDGET);
            if report.status != "PASS" {
                failures.push(format!("{} {}", case.name(), report.status));
            }
            for (name, trace) in &report.witnesses {
                let terminal = replay_snapshot(case, Fault::None, trace).expect("valid witness");
                assert!(
                    observed(terminal).contains(name),
                    "witness predicate {name}"
                );
            }
            emit_snapshot(case, Fault::None, &report);
        }
        let mut negatives = Vec::new();
        for fault in [
            Fault::EarlyGate,
            Fault::InstalledHorizon,
            Fault::InfiniteHorizon,
            Fault::GuardGap,
        ] {
            let family = if matches!(fault, Fault::EarlyGate | Fault::GuardGap) {
                Family::Registration
            } else {
                Family::Unpublished
            };
            negatives.extend(
                all.iter()
                    .filter(|case| case.family == family)
                    .map(|case| (*case, fault)),
            );
        }
        for fault in [Fault::LateOldPin, Fault::SnapshotOnlyPin, Fault::IgnorePin] {
            negatives.extend(
                all.iter()
                    .filter(|case| case.family == Family::Chain && case.route != Route::UndoStatus)
                    .map(|case| (*case, fault)),
            );
        }
        negatives.extend(
            all.iter()
                .filter(|case| case.family == Family::Handoff)
                .map(|case| (*case, Fault::NextPinGap)),
        );
        assert_eq!(negatives.len(), 23);
        for (case, fault) in negatives {
            let report = explore_snapshot(case, fault, FROZEN_BUDGET);
            if report.status == "EXPECTED_COUNTEREXAMPLE" {
                assert_eq!(report.violation, Some(fault.expected()));
                assert_eq!(
                    replay_snapshot(case, fault, &report.counterexample),
                    Err(fault.expected())
                );
            } else {
                failures.push(format!(
                    "{} {} {}",
                    case.name(),
                    fault.name(),
                    report.status
                ));
            }
            emit_snapshot(case, fault, &report);
        }
        assert!(failures.is_empty(), "{}", failures.join("; "));
        println!(
            "XADB_DEV3_GRAPH_SNAPSHOT_COMPLETE {{\"scope\":\"D3-M02\",\"status\":\"PASS\",\"normal_cases\":13,\"negative_cases\":23}}"
        );
    }

    #[test]
    fn snapshot_graph_budget_exhaustion_is_inconclusive() {
        for (budget, error) in [
            (
                Budget {
                    states: 1,
                    ..FROZEN_BUDGET
                },
                "state_cap",
            ),
            (
                Budget {
                    transitions: 0,
                    ..FROZEN_BUDGET
                },
                "transition_cap",
            ),
            (
                Budget {
                    timeout: Duration::ZERO,
                    ..FROZEN_BUDGET
                },
                "time_cap",
            ),
        ] {
            let report = explore_snapshot(cases()[0], Fault::None, budget);
            assert_eq!(report.status, "INCONCLUSIVE");
            assert_eq!(report.error, Some(error));
            assert!(report.unexplored > 0 && report.violation.is_none());
        }
    }
}
