//! Independent M03 logical observation, ordered preparation, and progress graphs.
//! Contract: devdoc/contracts/dev3-progress-model.md. No engine imports.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque, hash_map::Entry};
use std::fmt::Debug;
use std::hash::Hash;
use std::time::{Duration, Instant};

const LIMIT: usize = 1_000_000;
const EDGE_LIMIT: usize = 8_000_000;
const TIME_LIMIT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
struct Edge {
    to: usize,
    actor: u8,
    action: &'static str,
}

#[derive(Debug)]
struct Metrics {
    states: usize,
    transitions: usize,
    elapsed_ms: u128,
    unexplored: usize,
}

struct Graph<S> {
    states: Vec<S>,
    edges: Vec<Vec<Edge>>,
    parents: Vec<Option<(usize, u8, &'static str)>>,
    depths: Vec<u8>,
    transitions: usize,
    elapsed_ms: u128,
    witness_paths: RefCell<BTreeMap<String, Vec<String>>>,
    started: Instant,
}

impl<S> Graph<S> {
    fn path(&self, mut at: usize) -> Vec<String> {
        let mut path = Vec::new();
        while let Some((parent, actor, action)) = self.parents[at] {
            path.push(format!("{actor}:{action}"));
            at = parent;
        }
        path.reverse();
        path
    }
}

#[derive(Clone, Copy)]
struct Budget {
    states: usize,
    transitions: usize,
    time: Duration,
}
fn explore<S: Clone + Eq + Hash + Debug>(
    initial: S,
    successors: impl Fn(&S) -> Vec<(S, u8, &'static str)>,
    invariant: impl Fn(&S) -> Result<(), &'static str>,
    depth: Option<u8>,
    limit: usize,
) -> Result<Graph<S>, (&'static str, Vec<String>, Metrics)> {
    explore_budget(
        initial,
        successors,
        invariant,
        depth,
        Budget {
            states: limit,
            transitions: EDGE_LIMIT,
            time: TIME_LIMIT,
        },
    )
}
fn explore_budget<S: Clone + Eq + Hash + Debug>(
    initial: S,
    successors: impl Fn(&S) -> Vec<(S, u8, &'static str)>,
    invariant: impl Fn(&S) -> Result<(), &'static str>,
    depth: Option<u8>,
    budget: Budget,
) -> Result<Graph<S>, (&'static str, Vec<String>, Metrics)> {
    let start = Instant::now();
    invariant(&initial).map_err(|error| {
        (
            error,
            vec![],
            Metrics {
                states: 1,
                transitions: 0,
                elapsed_ms: start.elapsed().as_millis(),
                unexplored: 1,
            },
        )
    })?;
    let mut seen = HashMap::from([(initial.clone(), 0)]);
    let mut graph = Graph {
        states: vec![initial],
        edges: vec![vec![]],
        parents: vec![None],
        depths: vec![0],
        transitions: 0,
        elapsed_ms: 0,
        started: start,
        witness_paths: RefCell::new(BTreeMap::new()),
    };
    let mut cursor = 0;
    while cursor < graph.states.len() {
        if start.elapsed() >= budget.time {
            return Err((
                "INCONCLUSIVE_time",
                graph.path(cursor),
                metrics(&graph, start, cursor),
            ));
        }
        if depth.is_some_and(|bound| graph.depths[cursor] >= bound) {
            cursor += 1;
            continue;
        }
        for (next, actor, action) in successors(&graph.states[cursor]) {
            if graph.transitions >= budget.transitions {
                return Err((
                    "INCONCLUSIVE_transitions",
                    graph.path(cursor),
                    metrics(&graph, start, cursor),
                ));
            }
            graph.transitions += 1;
            if let Err(error) = invariant(&next) {
                let mut path = graph.path(cursor);
                path.push(format!("{actor}:{action}"));
                return Err((error, path, metrics(&graph, start, cursor)));
            }
            let to = match seen.entry(next.clone()) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    if graph.states.len() >= budget.states {
                        return Err((
                            "INCONCLUSIVE_states",
                            graph.path(cursor),
                            metrics(&graph, start, cursor),
                        ));
                    }
                    let id = graph.states.len();
                    entry.insert(id);
                    graph.states.push(next);
                    graph.edges.push(vec![]);
                    graph.parents.push(Some((cursor, actor, action)));
                    graph.depths.push(graph.depths[cursor].saturating_add(1));
                    id
                }
            };
            graph.edges[cursor].push(Edge { to, actor, action });
        }
        cursor += 1;
    }
    if start.elapsed() >= budget.time {
        return Err(("INCONCLUSIVE_time", vec![], metrics(&graph, start, cursor)));
    }
    graph.elapsed_ms = start.elapsed().as_millis();
    Ok(graph)
}

fn metrics<S>(graph: &Graph<S>, start: Instant, cursor: usize) -> Metrics {
    Metrics {
        states: graph.states.len(),
        transitions: graph.transitions,
        elapsed_ms: start.elapsed().as_millis(),
        unexplored: graph.states.len().saturating_sub(cursor),
    }
}

/// Closed nonterminal SCCs admitting a weak-fair infinite actor schedule.
fn fair_cycle<S>(
    graph: &Graph<S>,
    pending: impl Fn(&S) -> bool,
) -> Option<(usize, usize, Vec<String>)> {
    let n = graph.states.len();
    let active: Vec<_> = graph.states.iter().map(pending).collect();
    let mut visited = vec![false; n];
    let mut order = Vec::new();
    for root in 0..n {
        if !active[root] || visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((at, edge)) = stack.last_mut() {
            if *edge == graph.edges[*at].len() {
                order.push(*at);
                stack.pop();
                continue;
            }
            let next = graph.edges[*at][*edge].to;
            *edge += 1;
            if active[next] && !visited[next] {
                visited[next] = true;
                stack.push((next, 0));
            }
        }
    }
    let mut reverse = vec![Vec::new(); n];
    for (at, edges) in graph.edges.iter().enumerate() {
        if active[at] {
            for edge in edges {
                if active[edge.to] {
                    reverse[edge.to].push(at);
                }
            }
        }
    }
    let mut component = vec![usize::MAX; n];
    for root in order.into_iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        let mut members = vec![];
        let mut stack = vec![root];
        component[root] = root;
        while let Some(at) = stack.pop() {
            members.push(at);
            for &next in &reverse[at] {
                if component[next] == usize::MAX {
                    component[next] = root;
                    stack.push(next);
                }
            }
        }
        let cyclic = members.len() > 1 || graph.edges[root].iter().any(|edge| edge.to == root);
        if !cyclic {
            // A pending dead end also violates progress under fair scheduling.
            if members.len() == 1 && graph.edges[root].is_empty() {
                return Some((1, graph.path(root).len(), graph.path(root)));
            }
            continue;
        }
        let fair = (0..5).all(|actor| {
            let continuously_enabled = members
                .iter()
                .all(|&at| graph.edges[at].iter().any(|edge| edge.actor == actor));
            !continuously_enabled
                || members.iter().any(|&at| {
                    graph.edges[at]
                        .iter()
                        .any(|edge| edge.actor == actor && component[edge.to] == root)
                })
        });
        if fair {
            // A replayable prefix plus a closed walk visiting every SCC state
            // and an internal edge of every actor makes fairness inspectable.
            let mut trace = graph.path(root);
            let cycle_start = trace.len();
            let mut at = root;
            let mut goals: Vec<_> = members.iter().map(|&id| (id, None)).collect();
            for actor in 0..5 {
                if let Some((&from, edge)) = members.iter().find_map(|id| {
                    graph.edges[*id]
                        .iter()
                        .find(|edge| edge.actor == actor && component[edge.to] == root)
                        .map(|edge| (id, edge))
                }) {
                    goals.push((from, Some(edge.clone())));
                }
            }
            goals.push((root, None));
            for (target, extra) in goals {
                let mut queue = VecDeque::from([at]);
                let mut paths = HashMap::from([(at, Vec::<String>::new())]);
                while let Some(node) = queue.pop_front() {
                    if node == target {
                        break;
                    }
                    for edge in &graph.edges[node] {
                        if component[edge.to] == root && !paths.contains_key(&edge.to) {
                            let mut path = paths[&node].clone();
                            path.push(format!("{}:{}", edge.actor, edge.action));
                            paths.insert(edge.to, path);
                            queue.push_back(edge.to);
                        }
                    }
                }
                trace.extend(paths.remove(&target).expect("strongly connected"));
                at = target;
                if let Some(edge) = extra {
                    trace.push(format!("{}:{}", edge.actor, edge.action));
                    at = edge.to;
                }
            }
            validate_fair_walk(graph, &trace, cycle_start);
            return Some((members.len(), cycle_start, trace));
        }
    }
    None
}

fn validate_fair_walk<S>(graph: &Graph<S>, trace: &[String], prefix: usize) {
    let mut at = 0;
    let mut visited = vec![];
    let mut actors = vec![];
    for (step, action) in trace.iter().enumerate() {
        let edge = graph.edges[at]
            .iter()
            .find(|edge| format!("{}:{}", edge.actor, edge.action) == *action)
            .expect("SCC trace edge");
        if step >= prefix {
            visited.push(at);
            actors.push(edge.actor);
        }
        at = edge.to;
    }
    assert!(!visited.is_empty());
    assert_eq!(
        at, visited[0],
        "SCC closed walk must repeat same semantic state"
    );
    for actor in 0..5 {
        if visited
            .iter()
            .all(|at| graph.edges[*at].iter().any(|edge| edge.actor == actor))
        {
            assert!(
                actors.contains(&actor),
                "continuous actor must occur in repeated cycle"
            );
        }
    }
}
fn normal_witness<S: Clone + Eq + Debug>(
    graph: &Graph<S>,
    name: &str,
    predicate: impl Fn(&S) -> bool,
    initial: S,
    steps: impl Fn(&S) -> Vec<(S, u8, &'static str)>,
    check: impl Fn(&S) -> Result<(), &'static str>,
) {
    let at = graph
        .states
        .iter()
        .position(&predicate)
        .expect("normal witness missing");
    let path = graph.path(at);
    let replayed = replay(initial, &path, steps, check).expect("normal witness replay");
    assert_eq!(replayed, graph.states[at]);
    assert!(predicate(&replayed));
    graph
        .witness_paths
        .borrow_mut()
        .insert(name.to_owned(), path);
}

fn emit(suite: &str, cases: usize, states: usize, transitions: usize, negatives: usize) {
    println!(
        "XADB_DEV3_M03_SUMMARY {{\"suite\":\"{suite}\",\"status\":\"PASS\",\"cases\":{cases},\"states\":{states},\"transitions\":{transitions},\"negative_cases\":{negatives},\"unexplored\":0}}"
    );
}

fn emit_case<S>(suite: &str, fixture: &str, graph: &Graph<S>, witnesses: &[&str]) {
    let depth = graph.depths.iter().copied().max().unwrap_or(0);
    if graph.started.elapsed() >= TIME_LIMIT {
        println!(
            "XADB_DEV3_M03 {{\"suite\":{suite:?},\"fixture\":{fixture:?},\"mutant\":\"none\",\"status\":\"INCONCLUSIVE\",\"unexplored\":1,\"violation\":\"INCONCLUSIVE_time\"}}"
        );
        panic!("INCONCLUSIVE_time including witness/SCC verification");
    }
    println!(
        "XADB_DEV3_M03 {{\"suite\":\"{suite}\",\"fixture\":\"{fixture}\",\"mutant\":\"none\",\"status\":\"PASS\",\"states\":{},\"transitions\":{},\"max_depth\":{depth},\"elapsed_ms\":{},\"unexplored\":0,\"witnesses\":{witnesses:?},\"witness_paths\":{:?},\"counterexample\":[],\"violation\":null}}",
        graph.states.len(),
        graph.transitions,
        graph.started.elapsed().as_millis(),
        graph.witness_paths.borrow(),
    );
}

fn replay<S: Clone>(
    mut state: S,
    actions: &[String],
    successors: impl Fn(&S) -> Vec<(S, u8, &'static str)>,
    invariant: impl Fn(&S) -> Result<(), &'static str>,
) -> Result<S, &'static str> {
    for action in actions {
        state = successors(&state)
            .into_iter()
            .find(|(_, actor, name)| format!("{actor}:{name}") == *action)
            .ok_or("disabled_trace_action")?
            .0;
        invariant(&state)?;
    }
    Ok(state)
}

fn emit_negative(
    suite: &str,
    fixture: &str,
    mutant: &str,
    violation: &str,
    trace: &[String],
    stats: &Metrics,
) {
    assert!(
        stats.elapsed_ms < TIME_LIMIT.as_millis(),
        "INCONCLUSIVE_time before negative receipt"
    );
    println!(
        "XADB_DEV3_M03 {{\"suite\":\"{suite}\",\"fixture\":\"{fixture}\",\"mutant\":\"{mutant}\",\"status\":\"EXPECTED_COUNTEREXAMPLE\",\"states\":{},\"transitions\":{},\"max_depth\":{},\"elapsed_ms\":{},\"unexplored\":{},\"witnesses\":[\"counterexample_replayed\"],\"counterexample\":{trace:?},\"violation\":\"{violation}\"}}",
        stats.states,
        stats.transitions,
        trace.len(),
        stats.elapsed_ms,
        stats.unexplored,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObservationCase {
    Immediate,
    Waited,
    Delete,
    SameBytes,
    Abort,
    Own,
    Move,
    WrongRow,
    WrongStatement,
}

const OBSERVATION_CASES: [ObservationCase; 9] = [
    ObservationCase::Immediate,
    ObservationCase::Waited,
    ObservationCase::Delete,
    ObservationCase::SameBytes,
    ObservationCase::Abort,
    ObservationCase::Own,
    ObservationCase::Move,
    ObservationCase::WrongRow,
    ObservationCase::WrongStatement,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ObservationMutant {
    None,
    WaitOnly,
    PhysicalOnly,
    ValueOnly,
    IgnoreScope,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Row {
    key: u8, // 0,1,NULL=2; a separate bit represents deletion.
    version: u8,
    physical: u8,
    deleted: bool,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ObservedState {
    rows: [Row; 2],
    index: [Option<u8>; 2],
    undo: [Option<Row>; 4],
    status: [u8; 4],
    t1: u8,
    t2: u8,
    observed: Option<Row>,
    observation_row: u8,
    observation_statement: u8,
    waited: bool,
    rejected: bool,
    read: bool,
    serializable: bool,
    observed_table_epoch: u8,
    table_epoch: u8,
    committed: bool,
}

impl ObservedState {
    fn initial(case: ObservationCase, key: u8, generation: usize, serializable: bool) -> Self {
        let original = Row {
            key,
            version: 1,
            physical: 0,
            deleted: false,
        };
        let mut state = Self {
            rows: [original; 2],
            index: [Some(key); 2],
            undo: [None; 4],
            status: [0; 4],
            t1: 0,
            t2: 0,
            observed: None,
            observation_row: u8::from(case == ObservationCase::WrongRow),
            observation_statement: u8::from(case == ObservationCase::WrongStatement),
            waited: false,
            rejected: false,
            read: false,
            serializable,
            observed_table_epoch: 0,
            table_epoch: 0,
            committed: false,
        };
        state.status[generation] = 1;
        state.status[2 + generation] = 1;
        state.undo[0] = Some(original);
        state.rows[1].version = 3;
        state.rows[1].key = (key + 1) % 3;
        state.index[1] = Some(state.rows[1].key);
        if case == ObservationCase::Own {
            state.undo[1] = Some(original);
            state.rows[0].version = 4;
            state.rows[0].key = (key + 1) % 3;
            state.index[0] = Some(state.rows[0].key);
        }
        state
    }
}

fn observation_steps(
    state: &ObservedState,
    case: ObservationCase,
    mutant: ObservationMutant,
    generation: usize,
) -> Vec<(ObservedState, u8, &'static str)> {
    let mut result = Vec::new();
    let mut push = |actor, name, change: fn(&mut ObservedState)| {
        let mut next = state.clone();
        change(&mut next);
        result.push((next, actor, name));
    };
    if state.t1 == 0 {
        push(0, "observe", |next| {
            // Wrong-row fixture carries otherwise equal logical information.
            next.observed = Some(next.rows[0]);
            next.t1 = 1;
        });
    }
    if state.t1 >= 1 && state.t2 == 0 {
        let mut next = state.clone();
        if matches!(
            case,
            ObservationCase::Own | ObservationCase::WrongRow | ObservationCase::WrongStatement
        ) {
            next.t2 = 3;
            result.push((next, 1, "other_no_effect"));
        } else if case == ObservationCase::Move {
            next.rows[0].physical += 1;
            next.t2 = 3;
            result.push((next, 1, "split_move"));
        } else {
            next.t2 = 1;
            result.push((next, 1, "other_lock"));
        }
    }
    if state.t2 == 1 {
        let mut next = state.clone();
        next.undo[2] = Some(next.rows[0]);
        next.undo[3] = Some(next.rows[1]); // Prior ACK/other-row witness is independent.
        next.rows[0].version = 5;
        next.rows[0].physical += 1;
        if case != ObservationCase::SameBytes {
            next.rows[0].key = (next.rows[0].key + 1) % 3;
        }
        next.rows[0].deleted = case == ObservationCase::Delete;
        next.index[0] = (!next.rows[0].deleted).then_some(next.rows[0].key);
        next.t2 = 2;
        result.push((next, 1, "other_apply"));
    }
    if state.t2 == 2 && (case != ObservationCase::Waited || state.waited) {
        let mut next = state.clone();
        if case == ObservationCase::Abort {
            next.rows[0] = next.undo[2].expect("before image");
            next.index[0] = Some(next.rows[0].key);
            next.status[2 + generation] = 3;
            result.push((ObservedState { t2: 4, ..next }, 1, "other_abort"));
        } else {
            next.status[2 + generation] = 2;
            next.table_epoch += 1;
            result.push((ObservedState { t2: 3, ..next }, 1, "other_commit_unlock"));
        }
    }
    if state.t1 == 1 && case == ObservationCase::Waited && matches!(state.t2, 1 | 2) {
        let mut next = state.clone();
        next.waited = true;
        next.t1 = 2;
        result.push((next, 0, "lock_attempt_wait"));
    }
    if matches!(state.t1, 1 | 2)
        && state.t2 >= 3
        && (case != ObservationCase::Waited || state.waited)
    {
        let mut next = state.clone();
        next.t1 = 3;
        result.push((next, 0, "lock_acquired"));
    }
    if state.t1 == 3 {
        let observation = state.observed.expect("observed before lock");
        let same_scope = state.observation_row == 0 && state.observation_statement == 0;
        let accepted = match mutant {
            ObservationMutant::None => {
                same_scope && !state.rows[0].deleted && observation.version == state.rows[0].version
            }
            ObservationMutant::WaitOnly => {
                !state.waited || (same_scope && observation.version == state.rows[0].version)
            }
            ObservationMutant::PhysicalOnly => {
                same_scope && observation.physical == state.rows[0].physical
            }
            ObservationMutant::ValueOnly => {
                same_scope && !state.rows[0].deleted && observation.key == state.rows[0].key
            }
            ObservationMutant::IgnoreScope => {
                !state.rows[0].deleted && observation.version == state.rows[0].version
            }
        };
        let mut next = state.clone();
        next.rejected = !accepted;
        next.t1 = if accepted { 4 } else { 6 };
        result.push((next, 0, "validate"));
    }
    if state.t1 == 4 {
        let mut next = state.clone();
        next.rows[0].key = (next.rows[0].key + 1) % 3;
        next.rows[0].version = 7;
        next.index[0] = Some(next.rows[0].key);
        next.t1 = 5;
        result.push((next, 0, "apply"));
    }
    if state.t1 == 6 {
        let mut next = state.clone();
        next.rows[1] = next.undo[0].expect("earlier statement undo");
        next.index[1] = Some(next.rows[1].key);
        next.status[generation] = 3;
        next.t1 = 7;
        result.push((next, 0, "rollback_all_prior_own_writes"));
    }
    if !state.read {
        let mut next = state.clone();
        next.read = true;
        result.push((next, 2, "reader_check"));
    }
    if state.t1 == 5 && !state.committed {
        let mut next = state.clone();
        // Serializable has an additional table read/write-set check; native RC
        // uses the already-held row lock and logical observation validation.
        if !state.serializable || state.observed_table_epoch == state.table_epoch {
            next.committed = true;
            result.push((next, 0, "commit"));
        }
    }
    result
}

fn observation_invariant(
    state: &ObservedState,
    case: ObservationCase,
    original_key: u8,
) -> Result<(), &'static str> {
    for (row, index) in state.rows.iter().zip(state.index) {
        if index != (!row.deleted).then_some(row.key) {
            return Err("row_index_divergence");
        }
    }
    let conflict = matches!(
        case,
        ObservationCase::Immediate
            | ObservationCase::Waited
            | ObservationCase::Delete
            | ObservationCase::SameBytes
            | ObservationCase::WrongRow
            | ObservationCase::WrongStatement
    );
    if state.t1 >= 4 && state.rejected != conflict {
        return Err("observation_decision");
    }
    if state.t1 == 7 && (state.rows[1].key != original_key || state.rows[1].version != 1) {
        return Err("prior_own_write_survived_conflict");
    }
    if conflict
        && state.t2 == 3
        && state.t1 >= 4
        && !matches!(
            case,
            ObservationCase::WrongRow | ObservationCase::WrongStatement
        )
        && state.rows[0].version != 5
    {
        return Err("other_ack_lost");
    }
    if state.read && state.undo[0].is_none() {
        return Err("reader_snapshot_lost");
    }
    // Logical-event oracle: compute expected values/version/delete independently
    // of the candidate's row, index, observation or undo payload.
    let mut expected_key = if case == ObservationCase::Own {
        (original_key + 1) % 3
    } else {
        original_key
    };
    let mut expected_version = if case == ObservationCase::Own { 4 } else { 1 };
    let mut expected_deleted = false;
    if state.t2 >= 2
        && matches!(
            case,
            ObservationCase::Immediate
                | ObservationCase::Waited
                | ObservationCase::Delete
                | ObservationCase::SameBytes
                | ObservationCase::Abort
        )
        && !(case == ObservationCase::Abort && state.t2 == 4)
    {
        expected_key = if case == ObservationCase::SameBytes {
            original_key
        } else {
            (original_key + 1) % 3
        };
        expected_version = 5;
        expected_deleted = case == ObservationCase::Delete;
    }
    if state.t1 == 5 {
        expected_key = (expected_key + 1) % 3;
        expected_version = 7;
        expected_deleted = false;
    }
    if (
        state.rows[0].key,
        state.rows[0].version,
        state.rows[0].deleted,
    ) != (expected_key, expected_version, expected_deleted)
    {
        return Err("logical_row_oracle");
    }
    let expected_prior = if state.t1 == 7 {
        (original_key, 1)
    } else {
        ((original_key + 1) % 3, 3)
    };
    if (state.rows[1].key, state.rows[1].version) != expected_prior {
        return Err("prior_statement_oracle");
    }
    Ok(())
}

#[test]
fn observed_row_versions_cover_both_isolations_and_all_fixed_conflicts() {
    let mut totals = (0, 0, 0);
    for serializable in [false, true] {
        for case in OBSERVATION_CASES {
            for key in 0..3 {
                for generation in 0..2 {
                    let graph = explore(
                        ObservedState::initial(case, key, generation, serializable),
                        |state| observation_steps(state, case, ObservationMutant::None, generation),
                        |state| observation_invariant(state, case, key),
                        Some(16),
                        LIMIT,
                    )
                    .expect("normal observation model");
                    assert!(graph.states.iter().any(|state| matches!(state.t1, 5 | 7)));
                    if case == ObservationCase::Waited {
                        assert!(
                            graph
                                .states
                                .iter()
                                .any(|state| state.waited && state.t1 == 7)
                        );
                    }
                    if graph.states.iter().any(|state| state.t1 == 5) {
                        assert!(graph.states.iter().any(|state| state.committed));
                    }
                    normal_witness(
                        &graph,
                        "terminal",
                        |state| matches!(state.t1, 5 | 7),
                        ObservedState::initial(case, key, generation, serializable),
                        |state| observation_steps(state, case, ObservationMutant::None, generation),
                        |state| observation_invariant(state, case, key),
                    );
                    if case == ObservationCase::Waited {
                        normal_witness(
                            &graph,
                            "wait_then_conflict",
                            |state| state.waited && state.t1 == 7,
                            ObservedState::initial(case, key, generation, serializable),
                            |state| {
                                observation_steps(state, case, ObservationMutant::None, generation)
                            },
                            |state| observation_invariant(state, case, key),
                        );
                    }
                    if graph.states.iter().any(|state| state.t1 == 5) {
                        normal_witness(
                            &graph,
                            "commit",
                            |state| state.committed,
                            ObservedState::initial(case, key, generation, serializable),
                            |state| {
                                observation_steps(state, case, ObservationMutant::None, generation)
                            },
                            |state| observation_invariant(state, case, key),
                        );
                    }
                    emit_case(
                        "observed_row_version",
                        &format!(
                            "{case:?}_key{key}_generation{generation}_serializable{serializable}"
                        ),
                        &graph,
                        &["validation_before_effect", "terminal", "index_agrees"],
                    );
                    totals.0 += 1;
                    totals.1 += graph.states.len();
                    totals.2 += graph.transitions;
                }
            }
        }
    }
    let negatives = [
        (ObservationCase::Immediate, ObservationMutant::WaitOnly),
        (ObservationCase::Move, ObservationMutant::PhysicalOnly),
        (ObservationCase::SameBytes, ObservationMutant::ValueOnly),
        (ObservationCase::WrongRow, ObservationMutant::IgnoreScope),
        (
            ObservationCase::WrongStatement,
            ObservationMutant::IgnoreScope,
        ),
    ];
    for (case, mutant) in negatives {
        let error = explore(
            ObservedState::initial(case, 0, 0, false),
            |state| observation_steps(state, case, mutant, 0),
            |state| observation_invariant(state, case, 0),
            Some(16),
            LIMIT,
        )
        .err()
        .expect("mutant counterexample");
        assert_eq!(error.0, "observation_decision");
        assert!(matches!(
            replay(
                ObservedState::initial(case, 0, 0, false),
                &error.1,
                |state| observation_steps(state, case, mutant, 0),
                |state| observation_invariant(state, case, 0),
            ),
            Err("observation_decision")
        ));
        emit_negative(
            "observed_row_version",
            &format!("{case:?}"),
            &format!("{mutant:?}"),
            error.0,
            &error.1,
            &error.2,
        );
    }
    emit(
        "observed_row_version",
        totals.0,
        totals.1,
        totals.2,
        negatives.len(),
    );
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Pins {
    phase: [u8; 2],
    required: [u8; 2],
    discovery: [Vec<u8>; 2],
    pins: [u8; 2],
    held: [u8; 2],
    last_rank: [u8; 2],
    gate: u8,   // 0 none; actor+1.
    reader: u8, // 0 idle,1 pin,2 content latch.
    reader_page: u8,
    discovered: [bool; 2],
    incarnation: u8,
    expected_incarnation: u8,
}

fn pin_steps(state: &Pins, mutant: u8) -> Vec<(Pins, u8, &'static str)> {
    let mut out = Vec::new();
    for actor in 0..2 {
        let mut next = state.clone();
        match state.phase[actor] {
            0 => {
                let used = state.pins[0]
                    | state.pins[1]
                    | if state.reader != 0 {
                        1 << state.reader_page
                    } else {
                        0
                    };
                let requested = state.discovery[actor]
                    .iter()
                    .fold(0_u8, |mask, page| mask | (1 << page));
                assert_eq!(requested, state.required[actor]);
                if (used | requested).count_ones() <= 3 {
                    next.pins[actor] = state.required[actor];
                    next.phase[actor] = 1;
                    out.push((next, actor as u8, "reserve_all"));
                } else if mutant == 1 {
                    next.pins[actor] = 1 << state.required[actor].trailing_zeros();
                    next.phase[actor] = 5; // Waiting with an incomplete set.
                    out.push((next, actor as u8, "partial_pin_wait"));
                }
            }
            1 if state.gate == 0 => {
                if state.incarnation == state.expected_incarnation {
                    next.gate = actor as u8 + 1;
                    next.phase[actor] = 2;
                    out.push((next, actor as u8, "acquire_gate"));
                } else {
                    next.pins[actor] = 0;
                    next.phase[actor] = 6;
                    out.push((next, actor as u8, "reject_stale_incarnation"));
                }
            }
            2 => {
                let remaining = state.required[actor] & !state.held[actor];
                if remaining == 0 {
                    next.phase[actor] = 3;
                    out.push((next, actor as u8, "apply"));
                } else {
                    let mut acquisition = state.discovery[actor].clone();
                    if mutant != 2 {
                        acquisition.sort_unstable();
                    }
                    let page = *acquisition
                        .iter()
                        .find(|page| remaining & (1 << **page) != 0)
                        .unwrap();
                    if state.reader == 2 && page == state.reader_page {
                        next.phase[actor] = 0;
                        next.gate = 0;
                        next.held[actor] = 0;
                        next.last_rank[actor] = 0;
                        if mutant != 3 {
                            next.pins[actor] = 0;
                        }
                        out.push((next, actor as u8, "try_failed_cleanup"));
                    } else {
                        next.held[actor] |= 1 << page;
                        next.last_rank[actor] = page + 1;
                        out.push((next, actor as u8, "try_latch"));
                    }
                }
                if state.held[actor] != 0 && !state.discovered[actor] {
                    let mut found = state.clone();
                    let extra = (!state.required[actor]).trailing_zeros() as u8;
                    found.required[actor] |= 1 << extra;
                    found.discovery[actor].push(extra);
                    found.discovered[actor] = true;
                    found.phase[actor] = 0;
                    found.gate = 0;
                    found.pins[actor] = 0;
                    found.held[actor] = 0;
                    found.last_rank[actor] = 0;
                    out.push((found, actor as u8, "new_page_cleanup_and_reprepare"));
                }
            }
            3 => {
                next.phase[actor] = 4;
                next.gate = 0;
                next.pins[actor] = 0;
                next.held[actor] = 0;
                next.last_rank[actor] = 0;
                out.push((next, actor as u8, "release_all"));
            }
            _ => (),
        }
    }
    let mut next = state.clone();
    if state.reader == 0
        && (state.pins[0] | state.pins[1] | (1 << state.reader_page)).count_ones() <= 3
    {
        next.reader = 1;
        out.push((next, 2, "reader_pin"));
    } else if state.reader == 1
        && state
            .held
            .iter()
            .all(|mask| mask & (1 << state.reader_page) == 0)
    {
        next.reader = 2;
        out.push((next, 2, "reader_latch"));
    } else if state.reader == 2 {
        next.reader = 0;
        out.push((next, 2, "reader_release"));
    }
    out
}

fn pin_invariant(state: &Pins) -> Result<(), &'static str> {
    let used = (state.pins[0]
        | state.pins[1]
        | if state.reader != 0 {
            1 << state.reader_page
        } else {
            0
        })
    .count_ones();
    if used > 3 {
        return Err("frame_capacity");
    }
    for actor in 0..2 {
        if state.pins[actor] != 0 && state.pins[actor] != state.required[actor] {
            return Err("partial_pin_wait");
        }
        if state.phase[actor] == 0 && state.pins[actor] != 0 {
            return Err("retry_cleanup");
        }
        if state.held[actor] != 0 {
            if state.gate != actor as u8 + 1 {
                return Err("latch_without_gate");
            }
            let highest = 8 - state.held[actor].leading_zeros() as u8;
            if highest != state.last_rank[actor] {
                return Err("latch_rank");
            }
        }
    }
    Ok(())
}

#[test]
fn all_page_pairs_use_atomic_pin_reservation_sorted_latches_and_cleanup() {
    let mut totals = (0, 0, 0);
    for first in 0..8 {
        for second in first + 1..8 {
            for reverse in [false, true] {
                for incarnation in [1, 2] {
                    let initial = Pins {
                        phase: [0; 2],
                        required: [(1 << first) | (1 << second); 2],
                        discovery: if reverse {
                            [vec![second, first], vec![first, second]]
                        } else {
                            [vec![first, second], vec![second, first]]
                        },
                        pins: [0; 2],
                        held: [0; 2],
                        last_rank: [0; 2],
                        gate: 0,
                        reader: 0,
                        reader_page: (0..8)
                            .rev()
                            .find(|page| *page != first && *page != second)
                            .unwrap(),
                        discovered: [false; 2],
                        incarnation,
                        expected_incarnation: incarnation,
                    };
                    let graph = explore(
                        initial.clone(),
                        |s| pin_steps(s, 0),
                        pin_invariant,
                        Some(16),
                        LIMIT,
                    )
                    .expect("normal preparation model");
                    assert!(graph.states.iter().any(|state| state.phase[0] == 4));
                    assert!(graph.states.iter().any(|state| state.discovered[0]));
                    normal_witness(
                        &graph,
                        "apply",
                        |state| state.phase[0] == 4,
                        initial.clone(),
                        |s| pin_steps(s, 0),
                        pin_invariant,
                    );
                    normal_witness(
                        &graph,
                        "reprepare",
                        |state| state.discovered[0] && state.phase[0] == 0 && state.pins[0] == 0,
                        initial.clone(),
                        |s| pin_steps(s, 0),
                        pin_invariant,
                    );
                    normal_witness(
                        &graph,
                        "shared_frame_overlap",
                        |state| state.pins[0] != 0 && state.pins[0] == state.pins[1],
                        initial.clone(),
                        |s| pin_steps(s, 0),
                        pin_invariant,
                    );
                    emit_case(
                        "page_preparation",
                        &format!("pages{first}_{second}_reverse{reverse}_generation{incarnation}"),
                        &graph,
                        &["apply", "new_page_cleanup_and_reprepare"],
                    );
                    totals.0 += 1;
                    totals.1 += graph.states.len();
                    totals.2 += graph.transitions;
                }
            }
        }
    }
    for (mutant, error) in [
        (1, "partial_pin_wait"),
        (2, "latch_rank"),
        (3, "retry_cleanup"),
    ] {
        let initial = Pins {
            phase: [0; 2],
            required: [3; 2],
            discovery: [vec![1, 0], vec![0, 1]],
            pins: [0; 2],
            held: [0; 2],
            last_rank: [0; 2],
            gate: 0,
            reader: 0,
            reader_page: if mutant == 3 { 0 } else { 7 },
            discovered: [false; 2],
            incarnation: 1,
            expected_incarnation: 1,
        };
        if mutant == 3 {
            let good = explore(
                initial.clone(),
                |s| pin_steps(s, 0),
                pin_invariant,
                Some(16),
                LIMIT,
            )
            .expect("shared content contention");
            let (from, edge) = good
                .edges
                .iter()
                .enumerate()
                .find_map(|(from, edges)| {
                    edges
                        .iter()
                        .find(|edge| edge.action == "try_failed_cleanup")
                        .map(|edge| (from, edge))
                })
                .expect("cleanup edge");
            let mut path = good.path(from);
            path.push(format!("{}:{}", edge.actor, edge.action));
            let after = replay(initial.clone(), &path, |s| pin_steps(s, 0), pin_invariant)
                .expect("cleanup normal replay");
            assert_eq!(after, good.states[edge.to]);
            assert_eq!(
                (
                    after.gate,
                    after.pins[edge.actor as usize],
                    after.held[edge.actor as usize]
                ),
                (0, 0, 0)
            );
            good.witness_paths
                .borrow_mut()
                .insert("contention_cleanup".to_owned(), path);
            emit_case(
                "page_preparation",
                "shared_reader_contention",
                &good,
                &["try_failed_cleanup_replayed"],
            );
            totals.0 += 1;
            totals.1 += good.states.len();
            totals.2 += good.transitions;
        }
        let counterexample = explore(
            initial.clone(),
            |s| pin_steps(s, mutant),
            pin_invariant,
            Some(16),
            LIMIT,
        )
        .err()
        .expect("preparation mutant must fail");
        assert_eq!(counterexample.0, error);
        assert_eq!(
            replay(
                initial,
                &counterexample.1,
                |s| pin_steps(s, mutant),
                pin_invariant
            )
            .err(),
            Some(error)
        );
        emit_negative(
            "page_preparation",
            "pages0_1",
            &format!("mutant{mutant}"),
            error,
            &counterexample.1,
            &counterexample.2,
        );
    }
    let stale = Pins {
        phase: [0; 2],
        required: [3; 2],
        discovery: [vec![1, 0], vec![0, 1]],
        pins: [0; 2],
        held: [0; 2],
        last_rank: [0; 2],
        gate: 0,
        reader: 0,
        reader_page: 7,
        discovered: [false; 2],
        incarnation: 2,
        expected_incarnation: 1,
    };
    let rejected = explore(
        stale.clone(),
        |s| pin_steps(s, 0),
        pin_invariant,
        Some(16),
        LIMIT,
    )
    .expect("stale identity rejection");
    assert!(rejected.states.iter().any(|state| state.phase == [6, 6]));
    assert!(!rejected.states.iter().any(|state| state.held != [0, 0]));
    normal_witness(
        &rejected,
        "stale_before_latch",
        |state| state.phase == [6, 6],
        stale,
        |s| pin_steps(s, 0),
        pin_invariant,
    );
    emit_case(
        "page_preparation",
        "stale_incarnation",
        &rejected,
        &["rejected_before_latch"],
    );
    emit(
        "page_preparation",
        totals.0 + 1,
        totals.1 + rejected.states.len(),
        totals.2 + rejected.transitions,
        3,
    );
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Turn {
    phase: [u8; 2], // 0 try,1 queued,2 prepare,3 drain,4 done.
    retries: [u8; 2],
    queue: Vec<u8>,
    reader: u8, // 0 idle,1 q-selected,2 registered,3 content.
    flush: u8,  // 0 dirty,1 copied,2 IO,3 clean.
    workspace: u8,
    upper_lock: [bool; 2],
    completed: Vec<u8>,
    granted: Vec<u8>,
}

fn turn_steps(state: &Turn, mutant: u8) -> Vec<(Turn, u8, &'static str)> {
    let mut out = Vec::new();
    for actor in 0..2 {
        let mut next = state.clone();
        match state.phase[actor] {
            0 => {
                if state.reader != 0 || state.flush != 3 || state.workspace != 0 {
                    if state.retries[actor] < 2 || mutant == 1 {
                        next.retries[actor] = (state.retries[actor] + 1).min(2);
                        out.push((next, actor as u8, "failed_try"));
                    } else {
                        next.phase[actor] = 1;
                        next.queue.push(actor as u8);
                        if mutant == 3 {
                            next.upper_lock[actor] = true;
                        }
                        out.push((next, actor as u8, "enqueue_ticket"));
                    }
                } else {
                    next.phase[actor] = 1;
                    next.queue.push(actor as u8);
                    out.push((next, actor as u8, "enqueue_workspace_ticket"));
                }
            }
            1 if state.workspace == 0 && state.reader != 1 => {
                let selected = if mutant == 4 {
                    state.queue.last()
                } else {
                    state.queue.first()
                };
                if selected == Some(&(actor as u8)) {
                    next.phase[actor] = 2;
                    next.workspace = actor as u8 + 1;
                    next.granted.push(actor as u8);
                    out.push((next, actor as u8, "head_prepare"));
                }
            }
            2 if state.flush == 3 => {
                next.phase[actor] = 3;
                out.push((next, actor as u8, "close_content_and_drain"));
            }
            3 if state.reader != 3 => {
                next.phase[actor] = 4;
                next.workspace = 0;
                next.queue.retain(|ticket| *ticket != actor as u8);
                next.completed.push(actor as u8);
                out.push((next, actor as u8, "apply_and_release_turn"));
            }
            _ => (),
        }
    }
    let preparing = state.phase.contains(&2);
    let draining = state.phase.contains(&3);
    let mut next = state.clone();
    match state.reader {
        0 if !preparing && !draining && state.queue.is_empty() => {
            next.reader = 1;
            out.push((next, 2, "q_read"));
        }
        1 => {
            next.reader = 2;
            out.push((next, 2, "horizon_register"));
        }
        2 => {
            next.reader = 0;
            out.push((next, 2, "guard_release"));
            if !(draining || preparing && state.flush == 0) {
                let mut next = state.clone();
                next.reader = 3;
                out.push((next, 2, "content_enter"));
            }
        }
        3 => {
            next.reader = 2;
            out.push((next, 2, "content_exit"));
        }
        _ => (),
    }
    if state.flush < 3
        && !(mutant == 2 && preparing)
        && !draining
        && (state.flush > 0 || state.reader != 3)
    {
        let mut next = state.clone();
        next.flush += 1;
        out.push((
            next,
            3,
            ["flush_copy", "flush_io", "flush_complete"][state.flush as usize],
        ));
    }
    out
}

fn turn_invariant(state: &Turn) -> Result<(), &'static str> {
    if state.upper_lock.iter().any(|held| *held) {
        return Err("upper_lock_while_waiting_ticket");
    }
    if state.workspace != 0 {
        let owner = state.workspace - 1;
        if state.queue.first() != Some(&owner) {
            return Err("fifo_overtaking");
        }
        if !matches!(state.phase[owner as usize], 2 | 3) {
            return Err("workspace_owner");
        }
    }
    Ok(())
}

#[test]
fn fifo_turn_closed_graph_excludes_weak_fair_starvation() {
    let initial = Turn {
        phase: [0; 2],
        retries: [0; 2],
        queue: vec![],
        reader: 0,
        flush: 0,
        workspace: 0,
        upper_lock: [false; 2],
        completed: vec![],
        granted: vec![],
    };
    let graph = explore(
        initial.clone(),
        |s| turn_steps(s, 0),
        turn_invariant,
        None,
        LIMIT,
    )
    .expect("closed normal progress graph");
    assert!(graph.states.iter().any(|s| s.phase == [4, 4]));
    assert!(
        graph
            .states
            .iter()
            .any(|s| s.phase.contains(&2) && s.flush < 3)
    );
    assert!(
        graph
            .states
            .iter()
            .any(|s| s.reader == 3 && s.phase.contains(&3))
    );
    assert_eq!(
        fair_cycle(&graph, |state| state.phase != [4, 4]),
        None,
        "normal progress counterexample"
    );
    normal_witness(
        &graph,
        "both_done",
        |s| s.phase == [4, 4],
        initial.clone(),
        |s| turn_steps(s, 0),
        turn_invariant,
    );
    normal_witness(
        &graph,
        "prepare_dirty",
        |s| s.phase.contains(&2) && s.flush < 3,
        initial.clone(),
        |s| turn_steps(s, 0),
        turn_invariant,
    );
    normal_witness(
        &graph,
        "drain_reader",
        |s| s.reader == 3 && s.phase.contains(&3),
        initial.clone(),
        |s| turn_steps(s, 0),
        turn_invariant,
    );
    emit_case(
        "closed_progress",
        "two_writers_reader_flusher",
        &graph,
        &[
            "both_done",
            "prepare_allows_flusher",
            "drain_existing_reader",
            "no_fair_pending_scc",
        ],
    );
    for mutant in [1, 2] {
        let bad = explore(
            initial.clone(),
            |s| turn_steps(s, mutant),
            turn_invariant,
            None,
            LIMIT,
        )
        .expect("mutant graph must close before liveness judgment");
        let (size, cycle_start, trace) = fair_cycle(&bad, |state| state.phase != [4, 4])
            .expect("weak-fair starvation counterexample");
        assert!(
            replay(
                initial.clone(),
                &trace,
                |s| turn_steps(s, mutant),
                turn_invariant
            )
            .is_ok()
        );
        emit_negative(
            "closed_progress",
            &format!("fair_scc_size{size}_cycle_start{cycle_start}"),
            &format!("mutant{mutant}"),
            "weak_fair_noncompletion",
            &trace,
            &Metrics {
                states: bad.states.len(),
                transitions: bad.transitions,
                elapsed_ms: bad.started.elapsed().as_millis(),
                unexplored: 0,
            },
        );
    }
    for (mutant, reason) in [
        (3, "upper_lock_while_waiting_ticket"),
        (4, "fifo_overtaking"),
    ] {
        let error = explore(
            initial.clone(),
            |s| turn_steps(s, mutant),
            turn_invariant,
            Some(16),
            LIMIT,
        )
        .err()
        .expect("ticket mutant");
        assert_eq!(error.0, reason);
        assert_eq!(
            replay(
                initial.clone(),
                &error.1,
                |s| turn_steps(s, mutant),
                turn_invariant
            )
            .err(),
            Some(reason)
        );
        emit_negative(
            "closed_progress",
            "ticket",
            &format!("mutant{mutant}"),
            reason,
            &error.1,
            &error.2,
        );
    }
    emit(
        "closed_progress",
        1,
        graph.states.len(),
        graph.transitions,
        4,
    );
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Split {
    rows: [u8; 2],
    index: [u8; 2],
    pages: [u8; 8],
    before: [u8; 2],
    phase: u8,
    reader_seen: bool,
}

fn split_steps(state: &Split, stale: bool) -> Vec<(Split, u8, &'static str)> {
    let mut next = state.clone();
    let mut out = Vec::new();
    match state.phase {
        0 => {
            next.rows[0] = (next.rows[0] + 1) % 3;
            next.index[0] = next.rows[0];
            next.phase = 1;
            out.push((next, 0, "first_writer_change"));
        }
        1 => {
            next.pages.swap(1, 4);
            next.phase = 2;
            out.push((next, 0, "complete_split"));
        }
        2 => {
            next.rows[1] = (next.rows[1] + 1) % 3;
            next.index[1] = next.rows[1];
            next.phase = 3;
            out.push((next, 1, "other_writer_change"));
        }
        3 => {
            next.phase = 4;
            out.push((next, 1, "other_writer_commit"));
        }
        4 => {
            if stale {
                next.rows = next.before;
                next.index = next.before;
                next.pages = [0, 1, 2, 3, 4, 5, 6, 7];
            } else {
                next.rows[0] = next.before[0];
                next.index[0] = next.before[0];
            }
            next.phase = 5;
            out.push((next, 0, "logical_rollback"));
        }
        _ => (),
    }
    if !state.reader_seen {
        let mut next = state.clone();
        next.reader_seen = true;
        out.push((next, 2, "reader_check"));
    }
    out
}

fn split_invariant(state: &Split, initial: [u8; 2]) -> Result<(), &'static str> {
    if state.rows != state.index {
        return Err("row_index");
    }
    if state.phase >= 2 && state.pages != [0, 4, 2, 3, 1, 5, 6, 7] {
        return Err("completed_split_undone");
    }
    if state.phase >= 4 && state.rows[1] != (initial[1] + 1) % 3 {
        return Err("other_ack_lost");
    }
    if state.phase == 5 && state.rows[0] != initial[0] {
        return Err("own_undo_incomplete");
    }
    Ok(())
}

#[test]
fn logical_rollback_preserves_split_and_other_writer_ack() {
    let mut totals = (0, 0);
    for first in 0..3 {
        for second in 0..3 {
            let values = [first, second];
            let initial = Split {
                rows: values,
                index: values,
                before: values,
                pages: [0, 1, 2, 3, 4, 5, 6, 7],
                phase: 0,
                reader_seen: false,
            };
            let graph = explore(
                initial.clone(),
                |state| split_steps(state, false),
                |state| split_invariant(state, values),
                Some(16),
                LIMIT,
            )
            .expect("logical rollback");
            assert!(
                graph
                    .states
                    .iter()
                    .any(|state| state.phase == 5 && state.reader_seen)
            );
            normal_witness(
                &graph,
                "rollback_and_reader",
                |s| s.phase == 5 && s.reader_seen,
                initial.clone(),
                |s| split_steps(s, false),
                |s| split_invariant(s, values),
            );
            emit_case(
                "split_rollback",
                &format!("keys{first}_{second}"),
                &graph,
                &["split_retained", "other_ack_preserved", "logical_rollback"],
            );
            let error = explore(
                initial.clone(),
                |state| split_steps(state, true),
                |state| split_invariant(state, values),
                Some(16),
                LIMIT,
            )
            .err()
            .expect("stale page undo");
            assert_eq!(error.0, "completed_split_undone");
            assert_eq!(
                replay(
                    initial,
                    &error.1,
                    |state| split_steps(state, true),
                    |state| split_invariant(state, values)
                )
                .err(),
                Some(error.0)
            );
            emit_negative(
                "split_rollback",
                &format!("keys{first}_{second}"),
                "stale_page_restore",
                error.0,
                &error.1,
                &error.2,
            );
            totals.0 += graph.states.len();
            totals.1 += graph.transitions;
        }
    }
    emit("split_rollback", 9, totals.0, totals.1, 9);
}

#[test]
fn progress_exploration_caps_never_become_pass() {
    let initial = Turn {
        phase: [0; 2],
        retries: [0; 2],
        queue: vec![],
        reader: 0,
        flush: 0,
        workspace: 0,
        upper_lock: [false; 2],
        completed: vec![],
        granted: vec![],
    };
    let error = explore(
        initial.clone(),
        |s| turn_steps(s, 0),
        turn_invariant,
        None,
        1,
    )
    .err()
    .expect("cap must stop exploration");
    assert_eq!(error.0, "INCONCLUSIVE_states");
    for (budget, reason) in [
        (
            Budget {
                states: LIMIT,
                transitions: 0,
                time: TIME_LIMIT,
            },
            "INCONCLUSIVE_transitions",
        ),
        (
            Budget {
                states: LIMIT,
                transitions: EDGE_LIMIT,
                time: Duration::ZERO,
            },
            "INCONCLUSIVE_time",
        ),
    ] {
        let stopped = explore_budget(
            initial.clone(),
            |s| turn_steps(s, 0),
            turn_invariant,
            None,
            budget,
        )
        .err()
        .expect("budget rejection");
        assert_eq!(stopped.0, reason);
    }
}
