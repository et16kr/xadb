//! Independent two-file D3-M05 model. Frozen scope: dev3-doublewrite-model.md.
//! Values 0/1/2/3 denote old/@10/@20/torn complete-image equivalence classes.
use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Mode {
    Live,
    Fenced,
    Restart,
    Repair,
    Redo,
    Readable,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixture {
    Batch,
    RetiredNewer,
    Redirty,
}

impl Fixture {
    fn name(self) -> &'static str {
        match self {
            Self::Batch => "batch",
            Self::RetiredNewer => "retired_newer",
            Self::Redirty => "redirty",
        }
    }
    fn ack(self) -> u8 {
        if self == Self::Batch { 1 } else { 2 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutant {
    None,
    PendingBeforeImageSync,
    HomeBeforePendingSync,
    RetireAfterFirstFile,
    PartialRetirement,
    ReuseBeforeRetirement,
    RawRetiredResurrection,
    ClearRedirty,
    AddressReuseBeforeRetirement,
    OmitWalRedo,
}

impl Mutant {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PendingBeforeImageSync => "pending_before_image_sync",
            Self::HomeBeforePendingSync => "home_before_pending_sync",
            Self::RetireAfterFirstFile => "retire_after_first_file",
            Self::PartialRetirement => "partial_retirement",
            Self::ReuseBeforeRetirement => "reuse_before_retirement",
            Self::RawRetiredResurrection => "raw_retired_resurrection",
            Self::ClearRedirty => "clear_redirty",
            Self::AddressReuseBeforeRetirement => "address_reuse_before_retirement",
            Self::OmitWalRedo => "omit_wal_redo",
        }
    }
    fn error(self) -> &'static str {
        match self {
            Self::None => "",
            Self::PendingBeforeImageSync | Self::ReuseBeforeRetirement => "pending_image_invalid",
            Self::HomeBeforePendingSync => "home_without_durable_pending",
            Self::RetireAfterFirstFile => "retirement_without_all_file_sync",
            Self::PartialRetirement => "partial_batch_retirement",
            Self::RawRetiredResurrection => "older_image_over_newer_home",
            Self::ClearRedirty => "redirty_marked_clean",
            Self::AddressReuseBeforeRetirement => "pending_address_reused",
            Self::OmitWalRedo => "ack_or_repair_order_lost",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct State {
    mode: Mode,
    captured: bool,
    wal_verified: bool,
    image: [u8; 2],
    disk_image: [u8; 2],
    home: [u8; 2],
    disk_home: [u8; 2],
    control: u8,
    disk_control: u8,
    synced: u8,
    unsynced_home: u8,
    retired: bool,
    reused: bool,
    dirty: bool,
    observed: Option<[u8; 2]>,
    crashes: u8,
    repairing: bool,
    repaired: u8,
    repaired_twice: bool,
    premature_home: bool,
    premature_retire: bool,
    buffer: [u8; 2],
    durable_wal: [u8; 2],
    snapshot: [u8; 2],
    modification: [u8; 2],
    captured_modification: [u8; 2],
    source_pins: u8,
    workspace: Option<usize>,
    frame_peak: u8,
    slot_sequence: u8,
    disk_sequence: u8,
    selected_sequence: u8,
    disk_selected_sequence: u8,
    target_incarnation: [u8; 2],
    selected_incarnation: [u8; 2],
    disk_selected_incarnation: [u8; 2],
    raw_selected: bool,
}

impl State {
    fn initial(fixture: Fixture) -> Self {
        let old = fixture == Fixture::RetiredNewer;
        Self {
            mode: if old { Mode::Restart } else { Mode::Live },
            captured: old || fixture == Fixture::Redirty,
            wal_verified: old,
            image: [u8::from(old); 2],
            disk_image: [u8::from(old); 2],
            home: [if old { 2 } else { 0 }; 2],
            disk_home: [if old { 2 } else { 0 }; 2],
            control: 0,
            disk_control: 0,
            synced: 0,
            unsynced_home: 0,
            retired: old,
            reused: false,
            dirty: fixture == Fixture::Redirty,
            observed: None,
            crashes: 0,
            repairing: false,
            repaired: 0,
            repaired_twice: false,
            premature_home: false,
            premature_retire: false,
            buffer: [if fixture == Fixture::Batch { 1 } else { 2 }; 2],
            durable_wal: [if fixture == Fixture::Batch { 1 } else { 2 }; 2],
            snapshot: [1; 2],
            modification: [if fixture == Fixture::Redirty { 2 } else { 1 }; 2],
            captured_modification: [1; 2],
            source_pins: 0,
            workspace: None,
            frame_peak: if fixture == Fixture::Redirty { 2 } else { 0 },
            slot_sequence: 0,
            disk_sequence: 0,
            selected_sequence: 0,
            disk_selected_sequence: 0,
            target_incarnation: [1; 2],
            selected_incarnation: [1; 2],
            disk_selected_incarnation: [1; 2],
            raw_selected: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Capture,
    CaptureFinish,
    RepairAcquire(usize),
    TargetReuse(usize),
    WalRedo,
    WalSync,
    ImageWrite(usize),
    ImageSync,
    SelectPending,
    ControlSync,
    HomeWrite(usize, bool),
    FileSync(usize),
    SelectRetired,
    SlotReuse,
    Crash(u8),
    BootSelect,
    RedoReady,
}

impl Action {
    fn name(self) -> String {
        match self {
            Self::Capture => "capture".into(),
            Self::CaptureFinish => "capture_finish".into(),
            Self::RepairAcquire(i) => format!("repair_acquire_{i}"),
            Self::TargetReuse(i) => format!("target_reuse_{i}"),
            Self::WalRedo => "wal_redo".into(),
            Self::WalSync => "wal_sync".into(),
            Self::ImageWrite(i) => format!("image_write_{i}"),
            Self::ImageSync => "image_sync".into(),
            Self::SelectPending => "select_pending".into(),
            Self::ControlSync => "control_sync".into(),
            Self::HomeWrite(i, torn) => format!("home_{}_{i}", if torn { "torn" } else { "write" }),
            Self::FileSync(i) => format!("file_sync_{i}"),
            Self::SelectRetired => "select_retired".into(),
            Self::SlotReuse => "slot_reuse".into(),
            Self::Crash(mask) => format!("crash_{mask:02x}"),
            Self::BootSelect => "boot_select".into(),
            Self::RedoReady => "redo_ready".into(),
        }
    }
}

fn actions() -> Vec<Action> {
    let mut actions = vec![
        Action::Capture,
        Action::CaptureFinish,
        Action::RepairAcquire(0),
        Action::RepairAcquire(1),
        Action::TargetReuse(0),
        Action::TargetReuse(1),
        Action::WalRedo,
        Action::WalSync,
        Action::ImageWrite(0),
        Action::ImageWrite(1),
        Action::ImageSync,
        Action::SelectPending,
        Action::ControlSync,
        Action::HomeWrite(0, false),
        Action::HomeWrite(1, false),
        Action::HomeWrite(0, true),
        Action::HomeWrite(1, true),
        Action::FileSync(0),
        Action::FileSync(1),
        Action::SelectRetired,
        Action::SlotReuse,
        Action::BootSelect,
        Action::RedoReady,
    ];
    actions.extend((0..128).map(Action::Crash));
    actions
}

fn step(mut s: State, _fixture: Fixture, mutant: Mutant, action: Action) -> Option<State> {
    let working = matches!(s.mode, Mode::Live | Mode::Repair);
    match action {
        Action::Capture if s.mode == Mode::Live && !s.captured && s.source_pins == 0 => {
            s.source_pins = 3;
            s.frame_peak = s.frame_peak.max(2);
        }
        Action::CaptureFinish if s.mode == Mode::Live && s.source_pins == 3 => {
            s.snapshot = s.buffer;
            s.captured_modification = s.modification;
            s.captured = true;
            s.source_pins = 0;
        }
        Action::RepairAcquire(i)
            if s.mode == Mode::Repair
                && s.workspace.is_none()
                && s.home[i] != s.disk_image[i]
                && (s.home[i] != 2 || s.raw_selected) =>
        {
            s.workspace = Some(i);
            s.frame_peak = s.frame_peak.max(1);
        }
        Action::TargetReuse(i)
            if s.target_incarnation[i] == 1
                && (s.retired && s.disk_control == 0
                    || mutant == Mutant::AddressReuseBeforeRetirement && s.disk_control == 3) =>
        {
            s.target_incarnation[i] = 2;
        }
        Action::WalRedo if s.mode == Mode::Redo && s.disk_control == 0 => {
            s.buffer = s.disk_home;
            for i in 0..2 {
                if mutant != Mutant::OmitWalRedo
                    && s.buffer[i] != 3
                    && s.durable_wal[i] > s.buffer[i]
                {
                    s.buffer[i] = s.durable_wal[i];
                }
            }
            s.mode = Mode::Readable;
        }
        Action::WalSync if s.mode == Mode::Live && s.captured && !s.wal_verified => {
            s.wal_verified = (0..2).all(|i| s.durable_wal[i] >= s.snapshot[i]);
        }
        Action::ImageWrite(i) if s.mode == Mode::Live && s.wal_verified && s.image[i] == 0 => {
            s.image[i] = s.snapshot[i];
        }
        Action::ImageSync
            if s.mode == Mode::Live && s.image == [1; 2] && s.disk_image != [1; 2] =>
        {
            s.disk_image = s.image;
            s.disk_sequence = s.slot_sequence;
        }
        Action::SelectPending
            if s.mode == Mode::Live
                && s.control == 0
                && !s.retired
                && (s.disk_image == [1; 2]
                    || (mutant == Mutant::PendingBeforeImageSync && s.image == [1; 2])) =>
        {
            s.control = 3;
            s.selected_sequence = s.slot_sequence;
            s.selected_incarnation = s.target_incarnation;
        }
        Action::ControlSync if working && s.control != s.disk_control => {
            s.disk_control = s.control;
            s.disk_selected_sequence = s.selected_sequence;
            s.disk_selected_incarnation = s.selected_incarnation;
            if s.control == 0 {
                s.retired = true;
                if s.modification == s.captured_modification || mutant == Mutant::ClearRedirty {
                    s.dirty = false;
                }
                if s.mode == Mode::Repair {
                    s.mode = Mode::Redo;
                }
            }
        }
        Action::HomeWrite(i, torn)
            if working
                && s.home[i] != 1
                && (s.mode != Mode::Repair || s.workspace == Some(i))
                && (s.home[i] != 2 || s.raw_selected)
                && (s.disk_control == 3
                    || s.raw_selected
                    || (mutant == Mutant::HomeBeforePendingSync && s.control == 3)) =>
        {
            if s.disk_control != 3 && !s.raw_selected {
                s.premature_home = true;
            }
            s.home[i] = if torn {
                3
            } else if s.mode == Mode::Repair {
                s.disk_image[i]
            } else {
                s.image[i]
            };
            s.workspace = None;
            s.unsynced_home |= 1 << i;
            s.synced &= !(1 << i);
            if torn {
                s.mode = Mode::Fenced;
            } else if s.repairing {
                s.repaired |= 1 << i;
            }
        }
        Action::FileSync(i)
            if working && s.disk_control == 3 && s.home[i] == 1 && s.synced & (1 << i) == 0 =>
        {
            s.disk_home[i] = s.home[i];
            s.synced |= 1 << i;
            s.unsynced_home &= !(1 << i);
        }
        Action::SelectRetired
            if working
                && s.disk_control == 3
                && s.control == 3
                && (s.synced == 3
                    || (s.synced != 0
                        && matches!(
                            mutant,
                            Mutant::RetireAfterFirstFile | Mutant::PartialRetirement
                        ))) =>
        {
            if mutant == Mutant::PartialRetirement {
                s.control = 3 & !s.synced;
                if s.control == 0 {
                    s.control = 1;
                }
            } else {
                s.control = 0;
                s.premature_retire = s.synced != 3;
            }
        }
        Action::SlotReuse
            if working
                && !s.reused
                && (s.retired
                    || (mutant == Mutant::ReuseBeforeRetirement && s.disk_control == 3)) =>
        {
            s.image = [2; 2];
            s.disk_image = [2; 2];
            s.reused = true;
            s.slot_sequence += 1;
            s.disk_sequence = s.slot_sequence;
        }
        Action::Crash(mask) if s.crashes < 2 => {
            for i in 0..2 {
                match (mask >> (i * 2)) & 3 {
                    0 => (),
                    1 => s.disk_home[i] = s.home[i],
                    2 if s.unsynced_home & (1 << i) != 0 => s.disk_home[i] = 3,
                    _ => return None,
                }
                if mask & (1 << (i + 4)) != 0 {
                    s.disk_image[i] = s.image[i];
                    s.disk_sequence = s.slot_sequence;
                }
            }
            if mask & 64 != 0 {
                s.disk_control = s.control;
                s.disk_selected_sequence = s.selected_sequence;
                s.disk_selected_incarnation = s.selected_incarnation;
            }
            s.home = s.disk_home;
            s.image = s.disk_image;
            s.control = s.disk_control;
            s.selected_sequence = s.disk_selected_sequence;
            s.selected_incarnation = s.disk_selected_incarnation;
            s.slot_sequence = s.disk_sequence;
            s.source_pins = 0;
            s.workspace = None;
            s.buffer = [0; 2];
            s.raw_selected = false;
            s.repaired_twice |= s.repairing && s.repaired != 0;
            s.mode = Mode::Restart;
            s.synced = 0;
            s.unsynced_home = 0;
            s.observed = None;
            s.repairing = false;
            s.repaired = 0;
            s.crashes += 1;
        }
        Action::BootSelect if s.mode == Mode::Restart => {
            if mutant == Mutant::RawRetiredResurrection
                && s.disk_control == 0
                && s.disk_image == [1; 2]
                && s.disk_home == [2; 2]
            {
                s.raw_selected = true;
            }
            s.repairing = s.disk_control != 0 || s.raw_selected;
            s.mode = if s.repairing {
                Mode::Repair
            } else {
                Mode::Redo
            };
        }
        Action::RedoReady
            if s.mode == Mode::Readable
                || (s.mode == Mode::Live && s.retired && s.disk_control == 0) =>
        {
            s.observed = Some(s.buffer);
            s.mode = Mode::Ready;
        }
        _ => return None,
    }
    Some(s)
}

fn invariant(s: State, fixture: Fixture) -> Result<(), &'static str> {
    if ![0, 3].contains(&s.control) || ![0, 3].contains(&s.disk_control) {
        return Err("partial_batch_retirement");
    }
    if s.disk_control == 3
        && (s.disk_image != [1; 2] || s.disk_sequence != s.disk_selected_sequence)
    {
        return Err("pending_image_invalid");
    }
    if s.premature_home {
        return Err("home_without_durable_pending");
    }
    if s.premature_retire {
        return Err("retirement_without_all_file_sync");
    }
    if (0..2).any(|i| s.home[i] != 3 && s.disk_home[i] != 3 && s.home[i] < s.disk_home[i]) {
        return Err("older_image_over_newer_home");
    }
    if s.disk_control == 3 && s.target_incarnation != s.disk_selected_incarnation {
        return Err("pending_address_reused");
    }
    if s.source_pins.count_ones() + u32::from(s.workspace.is_some()) > 3 {
        return Err("frame_ownership");
    }
    if (s.mode == Mode::Ready || s.retired && !s.repairing)
        && (s.workspace.is_some() || s.source_pins != 0)
    {
        return Err("frame_not_released");
    }
    if fixture == Fixture::Redirty && !s.dirty {
        return Err("redirty_marked_clean");
    }
    if s.mode == Mode::Ready && (s.disk_control != 0 || s.observed != Some([fixture.ack(); 2])) {
        return Err("ack_or_repair_order_lost");
    }
    if s.disk_control == 0 && s.disk_home.contains(&3) {
        return Err("torn_home_without_pending");
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Budget {
    states: usize,
    transitions: usize,
    time: Duration,
}
const BUDGET: Budget = Budget {
    states: 1_000_000,
    transitions: 8_000_000,
    time: Duration::from_secs(60),
};

struct Node {
    state: State,
    parent: Option<(usize, Action)>,
    depth: u8,
}
struct Report {
    states: usize,
    transitions: usize,
    depth: u8,
    elapsed_ms: u128,
    incomplete: bool,
    unexplored: usize,
    error: Option<&'static str>,
    trace: Vec<Action>,
    replayed: bool,
    witnesses_replayed: bool,
    witnesses: BTreeMap<&'static str, Vec<Action>>,
}

fn trace(nodes: &[Node], mut at: usize, last: Action) -> Vec<Action> {
    let mut result = vec![last];
    while let Some((parent, action)) = nodes[at].parent {
        result.push(action);
        at = parent;
    }
    result.reverse();
    result
}

fn witness_predicates(next: State, fixture: Fixture) -> Vec<(&'static str, bool)> {
    [
        (
            "batch_retired",
            next.retired && fixture != Fixture::RetiredNewer,
        ),
        ("two_file_repair", next.repairing && next.synced == 3),
        ("two_crashes", next.crashes == 2),
        ("capture_pins", next.source_pins == 3),
        ("repair_workspace", next.workspace.is_some()),
        (
            "address_reused_after_retirement",
            next.target_incarnation != [1; 2] && next.disk_control == 0,
        ),
        ("slot_reused", next.reused),
        ("repair_recrash", next.repaired_twice),
        (
            "redirty_preserved",
            fixture == Fixture::Redirty && next.retired && next.dirty,
        ),
        (
            "raw_retired_ignored",
            fixture == Fixture::RetiredNewer && next.mode == Mode::Ready,
        ),
        ("reader_ack", next.mode == Mode::Ready),
    ]
    .to_vec()
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
        states: 1,
        transitions: 0,
        depth: 0,
        elapsed_ms: 0,
        incomplete: false,
        unexplored: 0,
        error: None,
        trace: Vec::new(),
        replayed: false,
        witnesses_replayed: false,
        witnesses: BTreeMap::new(),
    };
    if let Err(error) = invariant(initial, fixture) {
        report.error = Some(error);
        return report;
    }
    let mut cursor = 0;
    let actions = actions();
    'search: while cursor < nodes.len() {
        if started.elapsed() >= budget.time || report.states > budget.states {
            report.incomplete = true;
            break;
        }
        let state = nodes[cursor].state;
        let depth = nodes[cursor].depth;
        if depth < 16 {
            for action in actions.iter().copied() {
                let Some(next) = step(state, fixture, mutant, action) else {
                    continue;
                };
                if report.transitions >= budget.transitions || started.elapsed() >= budget.time {
                    report.incomplete = true;
                    break 'search;
                }
                report.transitions += 1;
                report.depth = report.depth.max(depth + 1);
                if let Err(error) = invariant(next, fixture) {
                    report.error = Some(error);
                    report.trace = trace(&nodes, cursor, action);
                    break 'search;
                }
                let witnesses = witness_predicates(next, fixture);
                for (name, reached) in witnesses {
                    if reached {
                        report
                            .witnesses
                            .entry(name)
                            .or_insert_with(|| trace(&nodes, cursor, action));
                    }
                }
                if let Entry::Vacant(entry) = seen.entry(next) {
                    if report.states >= budget.states {
                        report.incomplete = true;
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
        }
        cursor += 1;
    }
    report.unexplored = if report.incomplete || report.error.is_some() {
        nodes.len() - cursor
    } else {
        0
    };
    report.elapsed_ms = started.elapsed().as_millis();
    report
}

fn emit(fixture: Fixture, mutant: Mutant, report: &Report) {
    let status = if report.incomplete {
        "INCONCLUSIVE"
    } else if report.error.is_some() {
        if mutant != Mutant::None && report.error == Some(mutant.error()) && report.replayed {
            "EXPECTED_COUNTEREXAMPLE"
        } else {
            "FAIL"
        }
    } else if mutant == Mutant::None && report.witnesses_replayed {
        "PASS"
    } else {
        "FAIL"
    };
    let trace: Vec<_> = report.trace.iter().map(|a| a.name()).collect();
    let witnesses = report
        .witnesses
        .iter()
        .map(|(name, actions)| {
            let names: Vec<_> = actions.iter().map(|a| a.name()).collect();
            format!("{name:?}:{names:?}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let error = report.error.map_or("null".to_owned(), |e| format!("{e:?}"));
    println!(
        "XADB_DEV3_DOUBLEWRITE_MODEL {{\"scope\":\"D3-M05-F01\",\"fixture\":{:?},\"mutant\":{:?},\"status\":{status:?},\"states\":{},\"transitions\":{},\"max_depth\":{},\"elapsed_ms\":{},\"unexplored\":{},\"violation\":{error},\"witnesses_replayed\":{},\"counterexample_replayed\":{},\"counterexample\":{trace:?},\"witnesses\":{{{witnesses}}}}}",
        fixture.name(),
        mutant.name(),
        report.states,
        report.transitions,
        report.depth,
        report.elapsed_ms,
        report.unexplored,
        report.witnesses_replayed,
        report.replayed,
    );
}

fn required_witnesses(fixture: Fixture) -> &'static [&'static str] {
    match fixture {
        Fixture::Batch => &[
            "batch_retired",
            "two_file_repair",
            "two_crashes",
            "slot_reused",
            "repair_recrash",
            "reader_ack",
            "capture_pins",
            "repair_workspace",
            "address_reused_after_retirement",
        ],
        Fixture::RetiredNewer => &["raw_retired_ignored", "two_crashes", "reader_ack"],
        Fixture::Redirty => &["redirty_preserved", "two_file_repair", "repair_recrash"],
    }
}

fn validate_witnesses(fixture: Fixture, report: &mut Report) {
    if report.incomplete || report.error.is_some() {
        return;
    }
    if required_witnesses(fixture)
        .iter()
        .any(|name| !report.witnesses.contains_key(name))
    {
        report.error = Some("required_witness_missing");
        return;
    }
    for (name, actions) in &report.witnesses {
        let mut state = State::initial(fixture);
        if invariant(state, fixture).is_err() || actions.is_empty() {
            report.error = Some("witness_replay_failed");
            return;
        }
        for action in actions {
            let Some(next) = step(state, fixture, Mutant::None, *action) else {
                report.error = Some("witness_replay_failed");
                return;
            };
            if invariant(next, fixture).is_err() {
                report.error = Some("witness_replay_failed");
                return;
            }
            state = next;
        }
        if !witness_predicates(state, fixture)
            .into_iter()
            .any(|(key, reached)| key == *name && reached)
        {
            report.error = Some("witness_replay_failed");
            return;
        }
    }
    report.witnesses_replayed = true;
}
fn replay_counterexample(fixture: Fixture, mutant: Mutant, actions: &[Action]) -> bool {
    let mut state = State::initial(fixture);
    if invariant(state, fixture).is_err() || actions.is_empty() {
        return false;
    }
    for (i, action) in actions.iter().enumerate() {
        let Some(next) = step(state, fixture, mutant, *action) else {
            return false;
        };
        let error = invariant(next, fixture).err();
        if i + 1 == actions.len() {
            return error == Some(mutant.error());
        }
        if error.is_some() {
            return false;
        }
        state = next;
    }
    false
}

#[test]
fn doublewrite_two_file_exhaustive() {
    for fixture in [Fixture::Batch, Fixture::RetiredNewer, Fixture::Redirty] {
        let mut report = explore(fixture, Mutant::None, BUDGET);
        validate_witnesses(fixture, &mut report);
        emit(fixture, Mutant::None, &report);
        assert!(
            !report.incomplete,
            "full frozen exploration is INCONCLUSIVE"
        );
        assert!(
            report.error.is_none(),
            "normal counterexample: {:?}",
            report.error
        );
        let required = required_witnesses(fixture);
        for witness in required {
            assert!(report.witnesses.contains_key(witness), "missing {witness}");
        }
    }
    for mutant in [
        Mutant::PendingBeforeImageSync,
        Mutant::HomeBeforePendingSync,
        Mutant::RetireAfterFirstFile,
        Mutant::PartialRetirement,
        Mutant::ReuseBeforeRetirement,
        Mutant::RawRetiredResurrection,
        Mutant::ClearRedirty,
        Mutant::AddressReuseBeforeRetirement,
        Mutant::OmitWalRedo,
    ] {
        let fixture = match mutant {
            Mutant::RawRetiredResurrection => Fixture::RetiredNewer,
            Mutant::ClearRedirty => Fixture::Redirty,
            _ => Fixture::Batch,
        };
        let mut report = explore(fixture, mutant, BUDGET);
        if report.error == Some(mutant.error())
            && replay_counterexample(fixture, mutant, &report.trace)
        {
            report.replayed = true;
        } else if !report.incomplete {
            report.error = Some("counterexample_replay_failed");
        }
        emit(fixture, mutant, &report);
        assert!(!report.incomplete, "mutant exploration is INCONCLUSIVE");
        assert_eq!(report.error, Some(mutant.error()));
        assert!(report.replayed);
    }
}

#[test]
fn doublewrite_model_budget_exhaustion_is_inconclusive() {
    for budget in [
        Budget {
            states: 1,
            ..BUDGET
        },
        Budget {
            transitions: 0,
            ..BUDGET
        },
        Budget {
            time: Duration::ZERO,
            ..BUDGET
        },
    ] {
        assert!(explore(Fixture::Batch, Mutant::None, budget).incomplete);
    }
}

#[test]
fn doublewrite_repair_workspace_does_not_pin_completed_page() {
    let fixture = Fixture::Batch;
    let mut state = State::initial(fixture);
    for action in [
        Action::Capture,
        Action::CaptureFinish,
        Action::WalSync,
        Action::ImageWrite(0),
        Action::ImageWrite(1),
        Action::ImageSync,
        Action::SelectPending,
        Action::ControlSync,
        Action::HomeWrite(0, false),
        Action::Crash(1),
        Action::BootSelect,
    ] {
        state = step(state, fixture, Mutant::None, action).expect("reachable mixed home cut");
        assert_eq!(invariant(state, fixture), Ok(()));
    }
    assert_eq!(state.home, [1, 0]);
    assert!(step(state, fixture, Mutant::None, Action::RepairAcquire(0)).is_none());
    for action in [
        Action::FileSync(0),
        Action::RepairAcquire(1),
        Action::HomeWrite(1, false),
        Action::FileSync(1),
        Action::SelectRetired,
        Action::ControlSync,
        Action::WalRedo,
        Action::RedoReady,
    ] {
        state =
            step(state, fixture, Mutant::None, action).expect("remaining page repair completes");
        assert_eq!(invariant(state, fixture), Ok(()));
    }
    assert_eq!(state.mode, Mode::Ready);
    assert_eq!(state.workspace, None);
}
