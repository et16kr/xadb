//! P0 scalar-program exploration with a checked semantic schedule reduction.
//!
//! The grammar is exactly twelve BeginRW/Replace/Read/Commit programs and one
//! BeginRO/Scan(1)/Scan(2)/Commit program, with two actors and four scalar slots.
//! Insert, Delete, multi-mutation statements, rollback, UNKNOWN, crashes and
//! catalog operations are outside this exhaustive space. No engine is executed.
//! Every raw event merge is classified, but the checker runs nineteen canonical
//! interval schedules, not all 12,870 raw event orderings. See `schedule_space`.
//! V2 validates stale RW transactions before treating final logical equality as
//! a no-op. Successful equal-value replacement does not advance the version.

use std::time::{Duration, Instant};
use test_protocol::history::{
    Action, Cell, CommitFault, Event, History, Key, Mode, Mutation, Observation, Rejection,
};
use test_reference::{SearchBudget, Verdict, check};

const GENERATOR: &str = "p0-bounded-net-zero-v2";
const EXCLUDED: &str = "Insert,Delete,multi-mutation,rollback,UNKNOWN,crash,catalog";
const MAX_SECONDS: u64 = 7_200;
const CRITICAL_SHAPES: [[u8; 4]; 6] = [
    [0, 3, 4, 7],
    [0, 4, 3, 7],
    [0, 4, 7, 3],
    [4, 0, 3, 7],
    [4, 0, 7, 3],
    [4, 7, 0, 3],
];

// Independently enumerated before implementation. Each entry is the four
// cross-actor Begin/Commit interval relations, raw member count and legal
// critical-order bitset. Classes appear at their first lexicographic raw merge.
const CLASS_VECTORS: [([u8; 4], usize, u8); 19] = [
    ([1, 1, 1, 1], 1, 0b000001),
    ([1, 1, 0, 1], 36, 0b000011),
    ([1, 1, 0, 0], 12, 0b000111),
    ([1, 1, 2, 1], 887, 0b000010),
    ([1, 1, 2, 0], 1572, 0b000110),
    ([1, 1, 2, 2], 495, 0b000100),
    ([0, 1, 0, 1], 12, 0b001011),
    ([0, 1, 0, 0], 4, 0b011111),
    ([0, 1, 2, 1], 1572, 0b001010),
    ([0, 1, 2, 0], 3688, 0b011110),
    ([0, 1, 2, 2], 1572, 0b010100),
    ([0, 0, 2, 0], 4, 0b111110),
    ([0, 0, 2, 2], 12, 0b110100),
    ([2, 1, 2, 1], 495, 0b001000),
    ([2, 1, 2, 0], 1572, 0b011000),
    ([2, 1, 2, 2], 887, 0b010000),
    ([2, 0, 2, 0], 12, 0b111000),
    ([2, 0, 2, 2], 36, 0b110000),
    ([2, 2, 2, 2], 1, 0b100000),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Absent,
    Null,
    Zero,
    One,
}

impl Slot {
    fn row(self) -> Option<Vec<Cell>> {
        match self {
            Self::Absent => None,
            Self::Null => Some(vec![Cell::Null]),
            Self::Zero => Some(vec![Cell::Int64(0)]),
            Self::One => Some(vec![Cell::Int64(1)]),
        }
    }
}

fn initial_slots(mut index: usize) -> [Slot; 4] {
    assert!(index < 256);
    std::array::from_fn(|_| {
        let value = [Slot::Absent, Slot::Null, Slot::Zero, Slot::One][index % 4];
        index /= 4;
        value
    })
}

fn key(slot: usize) -> Key {
    Key {
        table: (slot / 2 + 1) as u64,
        row: (slot % 2 + 1) as u64,
    }
}

/// Numeric program 0..11 selects one of four keys and {NULL,0,1}; 12 is read-only.
fn action(program: u8, step: u8) -> Action {
    assert!(program < 13 && step < 4);
    match (program == 12, step) {
        (read_only, 0) => Action::Begin(if read_only {
            Mode::ReadOnly
        } else {
            Mode::ReadWrite
        }),
        (true, 1) => Action::Scan(1),
        (true, 2) => Action::Scan(2),
        (false, 1) => Action::Statement(vec![Mutation::Replace(
            key(usize::from(program / 3)),
            [Slot::Null, Slot::Zero, Slot::One][usize::from(program % 3)]
                .row()
                .unwrap(),
        )]),
        (false, 2) => Action::Read(key(usize::from(program / 3))),
        (_, 3) => Action::Commit(CommitFault::None),
        _ => unreachable!(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expected {
    Begun,
    Applied,
    Missing,
    Row(Slot),
    Scan(u8, [Slot; 2]),
    Committed,
    Conflict,
}

impl Expected {
    fn observation(self) -> Observation {
        match self {
            Self::Begun => Observation::Begun,
            Self::Applied => Observation::StatementApplied,
            Self::Missing => Observation::Rejected(Rejection::RowNotFound),
            Self::Row(slot) => Observation::Row(slot.row()),
            Self::Scan(table, slots) => Observation::Scan(
                slots
                    .into_iter()
                    .enumerate()
                    .filter_map(|(row, slot)| {
                        slot.row().map(|value| {
                            (
                                Key {
                                    table: u64::from(table),
                                    row: row as u64 + 1,
                                },
                                value,
                            )
                        })
                    })
                    .collect(),
            ),
            Self::Committed => Observation::Committed,
            Self::Conflict => Observation::Rejected(Rejection::SerializationConflict),
        }
    }
}

/// Four scalar slots and arithmetic versions independently interpret the frozen
/// grammar. This never invokes or imports the reference model/execute algorithm.
fn scalar_signature(initial: [Slot; 4], programs: [u8; 2], order: [u8; 8]) -> [Expected; 8] {
    let mut committed = initial;
    let mut private = [initial; 2];
    let mut start_version = [0_u8; 2];
    let mut version = 0_u8;
    let mut result = [Expected::Begun; 8];
    let mut visited = 0_u8;
    for operation in order {
        let actor = usize::from(operation / 4);
        let step = operation % 4;
        let program = programs[actor];
        let expected = match step {
            0 => {
                private[actor] = committed;
                start_version[actor] = version;
                Expected::Begun
            }
            1 if program == 12 => Expected::Scan(1, [private[actor][0], private[actor][1]]),
            1 => {
                let slot = usize::from(program / 3);
                if private[actor][slot] == Slot::Absent {
                    Expected::Missing
                } else {
                    private[actor][slot] =
                        [Slot::Null, Slot::Zero, Slot::One][usize::from(program % 3)];
                    Expected::Applied
                }
            }
            2 if program == 12 => Expected::Scan(2, [private[actor][2], private[actor][3]]),
            2 => Expected::Row(private[actor][usize::from(program / 3)]),
            3 if program != 12 && start_version[actor] != version => Expected::Conflict,
            3 => {
                // A successful equal-value Replace is observed as Applied but
                // does not publish another version. Stale RW validation above
                // still runs before this comparison.
                if program != 12 && private[actor] != committed {
                    committed = private[actor];
                    version += 1;
                }
                Expected::Committed
            }
            _ => unreachable!(),
        };
        assert_eq!(visited & (1 << operation), 0);
        visited |= 1 << operation;
        result[usize::from(operation)] = expected;
    }
    assert_eq!(visited, u8::MAX);
    result
}

/// All merges of two independently ordered chains, in lexicographic order.
fn interleavings<const N: usize>(per_actor: usize) -> Vec<[u8; N]> {
    fn append<const N: usize>(
        per_actor: usize,
        position: usize,
        used: [usize; 2],
        current: &mut [u8; N],
        result: &mut Vec<[u8; N]>,
    ) {
        if position == N {
            result.push(*current);
            return;
        }
        for actor in 0..2 {
            if used[actor] < per_actor {
                current[position] = (actor * per_actor + used[actor]) as u8;
                let mut next = used;
                next[actor] += 1;
                append(per_actor, position + 1, next, current, result);
            }
        }
    }
    assert_eq!(N, per_actor * 2);
    let mut result = Vec::new();
    append(per_actor, 0, [0; 2], &mut [0; N], &mut result);
    assert!(result.windows(2).all(|pair| pair[0] < pair[1]));
    result
}

struct LinearOrder {
    operations: [u8; 8],
    positions: [u8; 8],
    critical: usize,
}

struct IntervalClass {
    relations: [u8; 4],
    canonical: [u8; 16],
    members: usize,
    legal_critical: u8,
}

struct ScheduleSpace {
    linear: Vec<LinearOrder>,
    critical: [[u8; 8]; 6],
    intervals: Vec<IntervalClass>,
}

/// Begin fixes a snapshot and Commit is the only publication/validation effect.
/// Intervening operations touch their actor's private four-slot state only.
/// Consequently their observations depend on the six Begin/Commit orders, and
/// any legal choice of critical points can fill private operations inside their
/// own non-overlapping per-actor intervals. We check this projection for *every*
/// raw event merge and all seventy operation orders, before reducing to nineteen
/// representative schedules. The scalar outcome premise is also checked for all
/// seventy orders of every initial-state/program pair during execution.
fn schedule_space() -> ScheduleSpace {
    let mut critical = [[0; 8]; 6];
    let mut seen_critical = [false; 6];
    let linear: Vec<_> = interleavings::<8>(4)
        .into_iter()
        .map(|operations| {
            let shape: Vec<_> = operations
                .iter()
                .copied()
                .filter(|operation| matches!(operation % 4, 0 | 3))
                .collect();
            let index = CRITICAL_SHAPES
                .iter()
                .position(|expected| expected.as_slice() == shape.as_slice())
                .unwrap();
            if !seen_critical[index] {
                critical[index] = operations;
                seen_critical[index] = true;
            }
            let mut positions = [0; 8];
            for (position, operation) in operations.iter().enumerate() {
                positions[usize::from(*operation)] = position as u8;
            }
            LinearOrder {
                operations,
                positions,
                critical: index,
            }
        })
        .collect();
    assert_eq!(linear.len(), 70);
    assert!(seen_critical.iter().all(|seen| *seen));

    let raw = interleavings::<16>(8);
    assert_eq!(raw.len(), 12_870);
    let mut intervals: Vec<IntervalClass> = Vec::new();
    for events in raw {
        let mut positions = [0; 16];
        for (position, event) in events.iter().enumerate() {
            positions[usize::from(*event)] = position as u8;
        }
        let relation = |left: usize, right: usize| {
            if positions[left * 2 + 1] < positions[right * 2] {
                1
            } else if positions[right * 2 + 1] < positions[left * 2] {
                2
            } else {
                0
            }
        };
        let relations = [
            relation(0, 4),
            relation(0, 7),
            relation(3, 4),
            relation(3, 7),
        ];
        let mut legal_critical = 0;
        for order in &linear {
            let legal = (0..8).all(|left| {
                (0..8).all(|right| {
                    positions[left * 2 + 1] >= positions[right * 2]
                        || order.positions[left] < order.positions[right]
                })
            });
            if legal {
                legal_critical |= 1 << order.critical;
            }
        }
        assert_ne!(legal_critical, 0);
        if let Some(class) = intervals
            .iter_mut()
            .find(|class| class.relations == relations)
        {
            assert_eq!(
                class.legal_critical, legal_critical,
                "unsound schedule reduction"
            );
            class.members += 1;
        } else {
            intervals.push(IntervalClass {
                relations,
                canonical: events,
                members: 1,
                legal_critical,
            });
        }
    }
    assert_eq!(intervals.len(), CLASS_VECTORS.len());
    assert_eq!(
        intervals.iter().map(|class| class.members).sum::<usize>(),
        12_870
    );
    for (class, expected) in intervals.iter().zip(CLASS_VECTORS) {
        assert_eq!(
            (class.relations, class.members, class.legal_critical),
            expected
        );
    }
    ScheduleSpace {
        linear,
        critical,
        intervals,
    }
}

fn history(
    initial: [Slot; 4],
    programs: [u8; 2],
    events: [u8; 16],
    expected: [Expected; 8],
) -> History {
    History {
        initial: initial
            .into_iter()
            .enumerate()
            .filter_map(|(slot, value)| value.row().map(|row| (key(slot), row)))
            .collect(),
        events: events
            .into_iter()
            .map(|event| {
                let operation = event / 2;
                let actor = operation / 4;
                let request = u64::from(operation) + 1;
                if event % 2 == 0 {
                    Event::Invoke {
                        actor,
                        request,
                        action: action(programs[usize::from(actor)], operation % 4),
                    }
                } else {
                    Event::Return {
                        actor,
                        request,
                        observation: expected[usize::from(operation)].observation(),
                    }
                }
            })
            .collect(),
    }
}

fn serial_events(order: [u8; 8]) -> [u8; 16] {
    std::array::from_fn(|index| order[index / 2] * 2 + (index % 2) as u8)
}

#[derive(Default)]
struct Counts {
    accepted: u64,
    rejected: u64,
    invalid: u64,
    incomplete: u64,
    max_states: usize,
    max_transitions: usize,
}

impl Counts {
    fn total(&self) -> u64 {
        self.accepted + self.rejected + self.invalid + self.incomplete
    }

    fn verify(&mut self, history: &History, expected: Verdict, budget: SearchBudget, label: &str) {
        let result = check(history, budget);
        if result.verdict != expected {
            let status = if result.verdict == Verdict::Incomplete {
                "INCOMPLETE"
            } else {
                "FAIL"
            };
            println!(
                "XADB_BOUNDED_FAILURE {{\"status\":\"{status}\",\"completed_cases\":{},\"case\":\"{label}\"}}",
                self.total()
            );
            panic!("{label}: expected {expected:?}, actual {result:?}; history={history:?}");
        }
        match result.verdict {
            Verdict::Accepted => self.accepted += 1,
            Verdict::Rejected => self.rejected += 1,
            Verdict::Invalid => self.invalid += 1,
            Verdict::Incomplete => self.incomplete += 1,
        }
        self.max_states = self.max_states.max(result.states);
        self.max_transitions = self.max_transitions.max(result.transitions);
    }
}

struct LiteralTrace {
    history: History,
    next: u64,
}

impl LiteralTrace {
    fn new() -> Self {
        Self {
            history: History {
                initial: [
                    (key(0), vec![Cell::Int64(0)]),
                    (key(2), vec![Cell::Null]),
                    (key(3), vec![Cell::Int64(1)]),
                ]
                .into(),
                events: Vec::new(),
            },
            next: 0,
        }
    }

    fn invoke(&mut self, actor: u8, action: Action) -> u64 {
        self.next += 1;
        self.history.events.push(Event::Invoke {
            actor,
            request: self.next,
            action,
        });
        self.next
    }

    fn reply(&mut self, actor: u8, request: u64, observation: Observation) {
        self.history.events.push(Event::Return {
            actor,
            request,
            observation,
        });
    }

    fn call(&mut self, actor: u8, action: Action, observation: Observation) {
        let request = self.invoke(actor, action);
        self.reply(actor, request, observation);
    }

    fn write(&mut self, actor: u8, slot: usize, value: i64) {
        self.call(
            actor,
            Action::Statement(vec![Mutation::Replace(key(slot), vec![Cell::Int64(value)])]),
            Observation::StatementApplied,
        );
        self.call(
            actor,
            Action::Read(key(slot)),
            Observation::Row(Some(vec![Cell::Int64(value)])),
        );
    }

    fn scans(&mut self, first: i64) {
        self.call(
            1,
            Action::Scan(1),
            Observation::Scan(vec![(key(0), vec![Cell::Int64(first)])]),
        );
        self.call(
            1,
            Action::Scan(2),
            Observation::Scan(vec![
                (key(2), vec![Cell::Null]),
                (key(3), vec![Cell::Int64(1)]),
            ]),
        );
    }
}

fn literals() -> Counts {
    let mut counts = Counts::default();
    let mut fixed = LiteralTrace::new();
    fixed.call(0, Action::Begin(Mode::ReadWrite), Observation::Begun);
    fixed.call(1, Action::Begin(Mode::ReadOnly), Observation::Begun);
    fixed.write(0, 0, 1);
    fixed.call(0, Action::Commit(CommitFault::None), Observation::Committed);
    fixed.scans(0);
    fixed.call(1, Action::Commit(CommitFault::None), Observation::Committed);
    counts.verify(
        &fixed.history,
        Verdict::Accepted,
        SearchBudget::default(),
        "literal-old-snapshot",
    );

    let mut wrong = fixed.history.clone();
    if let Event::Return { observation, .. } = &mut wrong.events[11] {
        *observation = Observation::Scan(vec![(key(0), vec![Cell::Int64(1)])]);
    } else {
        panic!("literal event index changed");
    }
    counts.verify(
        &wrong,
        Verdict::Rejected,
        SearchBudget::default(),
        "literal-mixed-snapshot",
    );
    let mut wrong = fixed.history.clone();
    if let Event::Return { observation, .. } = &mut wrong.events[13] {
        *observation = Observation::Scan(vec![(key(3), vec![Cell::Int64(1)])]);
    } else {
        panic!("literal event index changed");
    }
    counts.verify(
        &wrong,
        Verdict::Rejected,
        SearchBudget::default(),
        "literal-null-is-not-absence",
    );

    for observed in [0, 1] {
        let mut overlap = LiteralTrace::new();
        overlap.call(0, Action::Begin(Mode::ReadWrite), Observation::Begun);
        overlap.write(0, 0, 1);
        let commit = overlap.invoke(0, Action::Commit(CommitFault::None));
        overlap.call(1, Action::Begin(Mode::ReadOnly), Observation::Begun);
        overlap.scans(observed);
        overlap.reply(0, commit, Observation::Committed);
        overlap.call(1, Action::Commit(CommitFault::None), Observation::Committed);
        counts.verify(
            &overlap.history,
            Verdict::Accepted,
            SearchBudget::default(),
            "literal-overlapping-begin-commit",
        );
    }

    let mut writers = LiteralTrace::new();
    writers.call(0, Action::Begin(Mode::ReadWrite), Observation::Begun);
    writers.call(1, Action::Begin(Mode::ReadWrite), Observation::Begun);
    writers.write(0, 0, 1);
    writers.write(1, 3, 0);
    writers.call(0, Action::Commit(CommitFault::None), Observation::Committed);
    writers.call(
        1,
        Action::Commit(CommitFault::None),
        Observation::Rejected(Rejection::SerializationConflict),
    );
    counts.verify(
        &writers.history,
        Verdict::Accepted,
        SearchBudget::default(),
        "literal-coarse-conflict",
    );
    if let Event::Return { observation, .. } = writers.history.events.last_mut().unwrap() {
        *observation = Observation::Committed;
    }
    counts.verify(
        &writers.history,
        Verdict::Rejected,
        SearchBudget::default(),
        "literal-two-dirty-commits",
    );

    let mut missing = fixed.history.clone();
    missing.events.pop();
    counts.verify(
        &missing,
        Verdict::Invalid,
        SearchBudget::default(),
        "literal-missing-return",
    );
    for budget in [
        SearchBudget {
            states: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            transitions: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            frontier_states: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            frontier_bytes: 0,
            ..SearchBudget::default()
        },
        SearchBudget {
            timeout: Duration::ZERO,
            ..SearchBudget::default()
        },
    ] {
        counts.verify(
            &fixed.history,
            Verdict::Incomplete,
            budget,
            "literal-budget-is-incomplete",
        );
    }
    assert_eq!(
        (
            counts.accepted,
            counts.rejected,
            counts.invalid,
            counts.incomplete
        ),
        (4, 3, 1, 5)
    );
    assert_eq!(counts.total(), 13);
    counts
}

fn contaminate_read(history: &mut History, actor: u8, program: u8) {
    let step = if program == 12 { 1 } else { 2 };
    let request = u64::from(actor * 4 + step) + 1;
    let mut changed = 0;
    for event in &mut history.events {
        if let Event::Return {
            request: current,
            observation,
            ..
        } = event
            && *current == request
        {
            *observation = if program == 12 {
                Observation::Scan(vec![(key(0), vec![Cell::Int64(2)])])
            } else {
                Observation::Row(Some(vec![Cell::Int64(2)]))
            };
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
}

fn run_profile(exhaustive: bool) {
    let initial_indices: Vec<_> = if exhaustive {
        (0..256).collect()
    } else {
        vec![210]
    };
    let programs: Vec<_> = if exhaustive {
        (0..13).collect()
    } else {
        vec![0, 4, 11, 12]
    };
    let expected_pairs = (initial_indices.len() * programs.len() * programs.len()) as u64;
    let expected_serial = expected_pairs * 70;
    let expected_signatures = expected_pairs * 19 * 6;
    let expected_mutants = expected_pairs * 19 * 2;
    let expected_cases = expected_serial + expected_signatures + expected_mutants;
    // Frozen by an independent Begin/Commit-only scalar calculation before
    // executing this model profile. It imports no reference/product helpers.
    // Retained source/results: artifacts/dev2-bounded-net-zero-v2/count.py and
    // counts.json. Main serial=3,028,480; signature A/R=3,975,168/956,928;
    // out-of-domain mutants=1,644,032. The original v1 arithmetic was also
    // reproduced (6,599,296/3,005,312) as a cross-check of the counting method.
    let expected_verdicts = if exhaustive {
        (7_003_648, 2_600_960)
    } else {
        (2_756, 796)
    };
    assert_eq!(expected_cases, expected_verdicts.0 + expected_verdicts.1);
    let profile = if exhaustive { "exhaustive" } else { "smoke" };
    println!(
        "XADB_BOUNDED_PROFILE {{\"generator\":\"{GENERATOR}\",\"profile\":\"{profile}\",\"grammar\":\"12 BeginRW-Replace-Read-Commit;1 BeginRO-Scan1-Scan2-Commit\",\"excluded\":\"{EXCLUDED}\",\"schedule_scope\":\"all raw merges classified; checker runs semantic representatives\",\"initial_states\":{},\"programs_per_actor\":{},\"raw_event_schedules\":12870,\"interval_classes\":19,\"linear_orders\":70,\"critical_orders\":6,\"expected_cases\":{expected_cases},\"expected_accepted\":{},\"expected_rejected\":{},\"literal_cases\":13,\"max_seconds\":{MAX_SECONDS}}}",
        initial_indices.len(),
        programs.len(),
        expected_verdicts.0,
        expected_verdicts.1
    );
    let started = Instant::now();
    let space = schedule_space();
    let literal_counts = literals();
    let mut counts = Counts::default();
    let mut pairs = 0;
    let mut serial = 0;
    let mut signatures = 0;
    let mut mutants = 0;
    let mut scalar_reduction_checks = 0;
    for initial_index in initial_indices {
        let initial = initial_slots(initial_index);
        for &left in &programs {
            for &right in &programs {
                if started.elapsed() >= Duration::from_secs(MAX_SECONDS) {
                    println!(
                        "XADB_BOUNDED_FAILURE {{\"status\":\"INCOMPLETE\",\"reason\":\"wall cap\",\"completed_cases\":{},\"expected_cases\":{expected_cases}}}",
                        counts.total()
                    );
                    panic!("bounded enumeration exceeded its predeclared wall cap");
                }
                let pair = [left, right];
                let critical = space
                    .critical
                    .map(|order| scalar_signature(initial, pair, order));
                for (index, order) in space.linear.iter().enumerate() {
                    let expected = scalar_signature(initial, pair, order.operations);
                    assert_eq!(
                        expected, critical[order.critical],
                        "private-operation reduction mismatch"
                    );
                    scalar_reduction_checks += 1;
                    let history = history(initial, pair, serial_events(order.operations), expected);
                    counts.verify(
                        &history,
                        Verdict::Accepted,
                        SearchBudget::default(),
                        &format!("initial{initial_index}-programs{left},{right}-serial{index}"),
                    );
                    serial += 1;
                }
                for (class_index, class) in space.intervals.iter().enumerate() {
                    for (signature_index, signature) in critical.iter().enumerate() {
                        let permitted = critical.iter().enumerate().any(|(index, candidate)| {
                            class.legal_critical & (1 << index) != 0 && candidate == signature
                        });
                        let history = history(initial, pair, class.canonical, *signature);
                        counts.verify(&history, if permitted { Verdict::Accepted } else { Verdict::Rejected }, SearchBudget::default(), &format!("initial{initial_index}-programs{left},{right}-class{class_index}-signature{signature_index}"));
                        signatures += 1;
                    }
                    let permitted = class.legal_critical.trailing_zeros() as usize;
                    for actor in 0..2 {
                        let mut history =
                            history(initial, pair, class.canonical, critical[permitted]);
                        contaminate_read(&mut history, actor as u8, pair[actor]);
                        counts.verify(&history, Verdict::Rejected, SearchBudget::default(), &format!("initial{initial_index}-programs{left},{right}-class{class_index}-mutant{actor}"));
                        mutants += 1;
                    }
                }
                pairs += 1;
            }
        }
        if exhaustive && (initial_index + 1).is_multiple_of(16) {
            println!(
                "XADB_BOUNDED_PROGRESS {{\"initial_states_completed\":{},\"cases_completed\":{},\"elapsed_ms\":{}}}",
                initial_index + 1,
                counts.total(),
                started.elapsed().as_millis()
            );
        }
    }
    assert_eq!(pairs, expected_pairs, "INCOMPLETE program pairs");
    assert_eq!(serial, expected_serial, "INCOMPLETE serial cases");
    assert_eq!(
        signatures, expected_signatures,
        "INCOMPLETE signature cases"
    );
    assert_eq!(mutants, expected_mutants, "INCOMPLETE mutant cases");
    assert_eq!(
        scalar_reduction_checks, expected_serial,
        "INCOMPLETE scalar reduction checks"
    );
    assert_eq!(
        counts.total(),
        expected_cases,
        "INCOMPLETE case enumeration"
    );
    assert_eq!((counts.accepted, counts.rejected), expected_verdicts);
    assert_eq!((counts.invalid, counts.incomplete), (0, 0));
    assert!(
        started.elapsed() < Duration::from_secs(MAX_SECONDS),
        "INCOMPLETE wall cap"
    );
    println!(
        "XADB_BOUNDED_RESULT {{\"status\":\"PASS\",\"generator\":\"{GENERATOR}\",\"profile\":\"{profile}\",\"excluded\":\"{EXCLUDED}\",\"schedule_scope\":\"all raw merges classified; checker runs semantic representatives\",\"elapsed_ms\":{},\"raw_event_schedules\":12870,\"interval_classes\":19,\"critical_orders\":6,\"program_pairs\":{pairs},\"serial_cases\":{serial},\"signature_cases\":{signatures},\"mutant_cases\":{mutants},\"scalar_reduction_checks\":{scalar_reduction_checks},\"expected_cases\":{expected_cases},\"completed_cases\":{},\"accepted\":{},\"rejected\":{},\"invalid\":0,\"incomplete\":0,\"literal_cases\":{},\"missing_cases\":0,\"max_states\":{},\"max_transitions\":{}}}",
        started.elapsed().as_millis(),
        counts.total(),
        counts.accepted,
        counts.rejected,
        literal_counts.total(),
        counts.max_states,
        counts.max_transitions
    );
}

#[test]
fn bounded_two_actor_scalar_programs_smoke() {
    run_profile(false);
}

#[test]
#[ignore = "D2 frozen exhaustive profile; execute with retained source/binary evidence"]
fn bounded_two_actor_scalar_programs_exhaustive() {
    run_profile(true);
}
