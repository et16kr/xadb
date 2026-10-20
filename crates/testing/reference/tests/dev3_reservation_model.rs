//! M06 independent reservation/byte inventory and recovery bootstrap models.
//! Contract: devdoc/contracts/dev3-reservation-model.md. No production imports.
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::fmt::Debug;
use std::hash::Hash;
use std::time::{Duration, Instant};

const MAX_STATES: usize = 1_000_000;
const MAX_TRANSITIONS: usize = 8_000_000;
const TIME_LIMIT: Duration = Duration::from_secs(60);
const JOB_COSTS: [u8; 6] = [3, 3, 3, 3, 1, 1];
const WAL_CAPACITY: u16 = 24;
const OWNED_CAPACITY: u16 = 42;

fn remaining(cut: u8) -> u16 {
    JOB_COSTS[cut as usize..]
        .iter()
        .map(|cost| u16::from(*cost))
        .sum()
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Base {
    cut: u8,
    self_contained: bool,
    remaining: u16,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Bootstrap {
    slots: [Option<Base>; 2],
    pending: Option<(u8, Base)>,
    current: u8,
    previous: u8,
    base_only: bool,
    writer_base: Option<u8>,
    source_forward: bool,
    log: [bool; 6],
    cut: u8,
    descriptor: u16,
    home_cut: u8,
    home_forward: bool,
    headers: u8,
    charged_headers: u8,
    header_synced: bool,
    phase: u8,
    target: u8,
    ledger: Option<u16>,
    crash_count: u8,
    undo_live: [bool; 4],
    status_active: [bool; 4],
    current_status: [u8; 2],
    row_effects: [u8; 2],
    page_owners: [u8; 8],
    undo_generation: [u8; 4],
    status_generation: [u8; 4],
}

impl Bootstrap {
    fn initial(fixture: &str) -> Self {
        let cut = if fixture == "partial_clr" { 2 } else { 0 };
        let base = Base {
            cut: 0,
            self_contained: false,
            remaining: 14,
        };
        let mut log = [false; 6];
        for entry in log.iter_mut().take(cut as usize) {
            *entry = true;
        }
        Self {
            slots: [Some(base); 2],
            pending: None,
            current: 0,
            previous: u8::from(fixture != "active_begin_pruned"),
            base_only: false,
            writer_base: Some(0),
            source_forward: true,
            log,
            cut,
            descriptor: remaining(cut),
            home_cut: 0,
            home_forward: false,
            headers: 1,
            charged_headers: 1,
            header_synced: true,
            phase: 0,
            target: 1,
            ledger: None,
            crash_count: 0,
            undo_live: [true; 4],
            status_active: if fixture == "partial_clr" {
                [false, true, false, true]
            } else {
                [true, false, true, false]
            },
            current_status: if fixture == "partial_clr" {
                [1, 3]
            } else {
                [0, 2]
            },
            row_effects: [2; 2],
            page_owners: [1, 2, 3, 1, 1, 2, 2, 3],
            undo_generation: [1, 2, 1, 2],
            status_generation: [1, 2, 1, 2],
        }
    }

    fn wal_bytes(&self) -> u16 {
        u16::from(self.source_forward) * 8
            + u16::from(self.headers) * 2
            + self
                .log
                .iter()
                .zip(JOB_COSTS)
                .filter_map(|(exists, cost)| exists.then_some(u16::from(cost)))
                .sum::<u16>()
    }

    fn closure_valid(&self, slot: u8) -> bool {
        let Some(base) = self.slots[slot as usize] else {
            return false;
        };
        if base.cut > self.cut || base.remaining != remaining(base.cut) {
            return false;
        }
        if base.self_contained {
            if !self.home_forward || self.home_cut < base.cut {
                return false;
            }
        } else if !self.source_forward {
            return false;
        }
        self.log[base.cut as usize..self.cut as usize]
            .iter()
            .all(|exists| *exists)
    }

    fn apply_home(&mut self) {
        // Candidate replays complete work records. No oracle values are copied.
        while self.home_cut < self.cut {
            match self.home_cut {
                0..=3 => {
                    let slot = self.home_cut as usize;
                    let owner = slot / 2;
                    self.undo_live[slot] = false;
                    self.row_effects[owner] -= 1;
                }
                4..=5 => {
                    self.status_active[self.current_status[(self.home_cut - 4) as usize] as usize] =
                        false
                }
                _ => unreachable!(),
            }
            self.home_cut += 1;
        }
        self.home_forward = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BootMutant {
    None,
    AccumulateEpoch,
    OverwriteSelected,
    ReclaimPrevious,
    IgnoreBaseOnly,
    OmitEpochCharge,
    ResetDescriptor,
    DoubleRedoCharge,
    ReuseClrReservation,
}

impl BootMutant {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AccumulateEpoch => "accumulate_epoch",
            Self::OverwriteSelected => "overwrite_selected_slot",
            Self::ReclaimPrevious => "reclaim_previous_wal",
            Self::IgnoreBaseOnly => "ignore_base_only",
            Self::OmitEpochCharge => "omit_epoch_charge",
            Self::ResetDescriptor => "reset_descriptor",
            Self::DoubleRedoCharge => "double_redo_charge",
            Self::ReuseClrReservation => "reuse_clr_reservation",
        }
    }
}

fn bootstrap_steps(
    state: &Bootstrap,
    mutant: BootMutant,
    bounded: bool,
) -> Vec<(Bootstrap, &'static str)> {
    let mut out = Vec::new();
    let mut next = state.clone();
    let action = match state.phase {
        0 => {
            // Profile and owned control are sufficient before any home access.
            next.phase = 1;
            "boot_reserve"
        }
        1 => {
            next.phase = 2;
            "doublewrite_repair"
        }
        2 => {
            next.phase = 3;
            "analysis_complete_prefix"
        }
        3 => {
            if mutant == BootMutant::ResetDescriptor {
                next.descriptor = 0;
            }
            next.ledger = Some(if mutant == BootMutant::ResetDescriptor {
                0
            } else {
                state.descriptor
            });
            next.phase = 4;
            "reserve_restore"
        }
        4 => {
            next.apply_home();
            if mutant == BootMutant::DoubleRedoCharge && state.cut > 0 {
                next.ledger = next.ledger.map(|amount| amount.saturating_sub(3));
            }
            next.phase = 5;
            "redo_home"
        }
        5 => {
            next.phase = 6;
            "sync_source"
        }
        6 => {
            next.phase = 7;
            "sync_home"
        }
        7 => {
            next.phase = 8;
            "retire_doublewrite"
        }
        8 => {
            next.previous = state.current;
            next.target = 1 - state.current;
            next.phase = 9;
            "select_common_fallback"
        }
        9 => {
            if mutant == BootMutant::OverwriteSelected {
                next.target = state.current;
            }
            next.slots[next.target as usize] = None;
            next.pending = Some((
                next.target,
                Base {
                    cut: state.cut,
                    self_contained: true,
                    remaining: state.descriptor,
                },
            ));
            next.phase = 10;
            "write_free_slot"
        }
        10 => {
            let (slot, base) = state.pending.expect("pending slot write");
            next.slots[slot as usize] = Some(base);
            next.pending = None;
            next.phase = 11;
            "sync_free_slot"
        }
        11 => {
            next.previous = state.current;
            next.current = state.target;
            next.base_only = mutant != BootMutant::IgnoreBaseOnly;
            if mutant == BootMutant::ReclaimPrevious {
                next.source_forward = false;
                for entry in next.log.iter_mut().take(state.cut as usize) {
                    *entry = false;
                }
            }
            next.phase = 12;
            "select_new_base_old_previous"
        }
        12 => {
            next.previous = state.current;
            next.target = 1 - state.current;
            next.phase = 13;
            "select_new_common"
        }
        13 => {
            next.slots[state.target as usize] = None;
            next.pending = Some((
                state.target,
                Base {
                    cut: state.cut,
                    self_contained: true,
                    remaining: state.descriptor,
                },
            ));
            next.phase = 14;
            "write_second_slot"
        }
        14 => {
            let (slot, base) = state.pending.expect("pending second write");
            next.slots[slot as usize] = Some(base);
            next.pending = None;
            next.phase = 15;
            "sync_second_slot"
        }
        15 => {
            next.previous = state.current;
            next.current = state.target;
            next.phase = 16;
            "select_two_bases"
        }
        16 => {
            next.source_forward = false;
            for entry in next.log.iter_mut().take(state.cut as usize) {
                *entry = false;
            }
            if mutant != BootMutant::AccumulateEpoch {
                next.headers = 0;
                next.charged_headers = 0;
            }
            if mutant != BootMutant::IgnoreBaseOnly {
                next.writer_base = None;
            }
            next.header_synced = false;
            next.phase = 17;
            "reclaim_old_wal"
        }
        17 => {
            next.headers += 1;
            next.phase = 18;
            if mutant != BootMutant::OmitEpochCharge {
                next.charged_headers += 1;
            }
            "charge_and_create_epoch"
        }
        18 => {
            next.header_synced = true;
            next.phase = 19;
            "sync_epoch"
        }
        19 => {
            next.base_only = false;
            next.writer_base = Some(
                state.slots[state.current as usize]
                    .expect("selected base")
                    .cut,
            );
            next.phase = 20;
            "select_writer"
        }
        20 if state.cut < 6 => {
            let cost = u16::from(JOB_COSTS[state.cut as usize]);
            next.log[state.cut as usize] = true;
            next.cut += 1;
            next.descriptor -= cost;
            if mutant != BootMutant::ReuseClrReservation {
                next.ledger = state.ledger.map(|amount| amount - cost);
            }
            next.phase = 21;
            if state.cut < 4 {
                "append_clr_effect_progress_descriptor"
            } else {
                "append_terminal"
            }
        }
        20 => {
            next.phase = 22;
            "verify_recovery"
        }
        21 => {
            next.apply_home();
            next.phase = 20;
            "apply_complete_group"
        }
        22 => {
            next.phase = 23;
            "stabilize_ready"
        }
        _ => "",
    };
    if !action.is_empty() {
        out.push((next, action));
    }
    if !bounded || state.crash_count < 2 {
        let mut crash = state.clone();
        crash.phase = 0;
        crash.ledger = None;
        crash.pending = None; // Unsynced scratch was never selected.
        crash.target = 1 - crash.current;
        if bounded {
            crash.crash_count += 1;
        }
        out.push((crash, "crash"));
    }
    out
}

fn bootstrap_rank(state: &Bootstrap) -> u16 {
    let jobs = u16::from(6 - state.cut);
    match state.phase {
        0..=19 => 2 * jobs + 24 - u16::from(state.phase),
        20 => 2 * jobs + 4,
        21 => 2 * jobs + 5,
        22 => 2,
        23 => 0,
        _ => unreachable!(),
    }
}

fn bootstrap_invariant(state: &Bootstrap) -> Result<(), &'static str> {
    if !state.closure_valid(state.current) || !state.closure_valid(state.previous) {
        return Err("selected_recovery_closure_lost");
    }
    if state.descriptor != remaining(state.cut) {
        return Err("persistent_descriptor_mismatch");
    }
    if state
        .ledger
        .is_some_and(|ledger| ledger != remaining(state.cut))
    {
        return Err("remaining_obligation_mismatch");
    }
    let future_header = u16::from(state.headers == 0) * 2;
    let peak = state.wal_bytes() + remaining(state.cut) + future_header;
    if peak > WAL_CAPACITY || 18 + peak > OWNED_CAPACITY {
        return Err("bootstrap_peak");
    }
    if state.headers != state.charged_headers {
        return Err("unaccounted_epoch_charge");
    }
    if state
        .page_owners
        .iter()
        .filter(|owner| **owner == 1)
        .count()
        != 3
        || state
            .page_owners
            .iter()
            .filter(|owner| **owner == 2)
            .count()
            != 3
        || state
            .page_owners
            .iter()
            .filter(|owner| **owner == 3)
            .count()
            != 2
    {
        return Err("page_token_inventory");
    }
    if !state.base_only {
        if let Some(base) = state.writer_base {
            // A selected writer must still have its header and predecessor path.
            if state.headers == 0 || !state.header_synced {
                return Err("writer_continuation_lost");
            }
            if base
                < state.slots[state.current as usize]
                    .expect("valid current")
                    .cut
                && !state.source_forward
                && !state.log[base as usize..state.cut as usize]
                    .iter()
                    .all(|record| *record)
            {
                return Err("writer_continuation_lost");
            }
        } else {
            return Err("writer_continuation_lost");
        }
    }
    for slot in 0..4 {
        if state.undo_live[slot] != (slot >= state.home_cut.min(4) as usize) {
            return Err("home_undo_inventory");
        }
    }
    let expected_rows = [
        2 - state.home_cut.min(2),
        2 - state.home_cut.saturating_sub(2).min(2),
    ];
    if state.row_effects != expected_rows {
        return Err("home_logical_undo");
    }
    for owner in 0..2 {
        if state.status_active[state.current_status[owner] as usize]
            != (state.home_cut < 5 + owner as u8)
        {
            return Err("terminal_status");
        }
        let slot = state.current_status[owner] as usize;
        if slot / 2 != owner || state.status_generation[slot] != slot as u8 % 2 + 1 {
            return Err("status_slot_generation");
        }
    }
    if state
        .undo_generation
        .iter()
        .chain(state.status_generation.iter())
        .any(|generation| !(1..=2).contains(generation))
    {
        return Err("generation_outside_domain");
    }
    if state.phase == 23 && (state.cut != 6 || state.home_cut != 6 || state.ledger != Some(0)) {
        return Err("ready_with_unfinished_obligation");
    }
    Ok(())
}

struct BootReport {
    states: usize,
    transitions: usize,
    depth: usize,
    elapsed_ms: u128,
    peak: u16,
    paths: HashMap<&'static str, Vec<&'static str>>,
    counterexample: Vec<&'static str>,
    violation: Option<&'static str>,
    unexplored: usize,
    started: Instant,
}

fn trace(parents: &[Option<(usize, &'static str)>], mut at: usize) -> Vec<&'static str> {
    let mut actions = Vec::new();
    while let Some((parent, action)) = parents[at] {
        actions.push(action);
        at = parent;
    }
    actions.reverse();
    actions
}

fn explore_bootstrap(fixture: &str, mutant: BootMutant, bounded: bool, cap: usize) -> BootReport {
    let started = Instant::now();
    let initial = Bootstrap::initial(fixture);
    assert_eq!(bootstrap_invariant(&initial), Ok(()));
    let mut states = vec![initial.clone()];
    let mut parents = vec![None];
    let mut depths = vec![0];
    let mut seen = HashMap::from([(initial, 0)]);
    let mut result = BootReport {
        started,
        states: 1,
        transitions: 0,
        depth: 0,
        elapsed_ms: 0,
        peak: 0,
        paths: HashMap::new(),
        counterexample: vec![],
        violation: None,
        unexplored: 0,
    };
    let mut cursor = 0;
    'search: while cursor < states.len() {
        if started.elapsed() >= TIME_LIMIT {
            result.violation = Some("INCONCLUSIVE_time");
            result.unexplored = states.len() - cursor;
            break;
        }
        let state = states[cursor].clone();
        result.peak = result
            .peak
            .max(state.wal_bytes() + remaining(state.cut) + u16::from(state.headers == 0) * 2);
        if bounded && depths[cursor] == 16 {
            cursor += 1;
            continue;
        }
        for (next, action) in bootstrap_steps(&state, mutant, bounded) {
            if result.transitions >= MAX_TRANSITIONS {
                result.violation = Some("INCONCLUSIVE_transitions");
                result.unexplored = states.len() - cursor;
                break 'search;
            }
            result.transitions += 1;
            if action != "crash" {
                assert!(
                    bootstrap_rank(&next) < bootstrap_rank(&state),
                    "no-crash rank: {action}"
                );
            }
            result.depth = result.depth.max(depths[cursor] + 1);
            let mut path = trace(&parents, cursor);
            path.push(action);
            if let Err(violation) = bootstrap_invariant(&next) {
                result.violation = Some(violation);
                result.counterexample = path;
                result.unexplored = states.len() - cursor;
                break 'search;
            }
            result.paths.entry(action).or_insert_with(|| path.clone());
            if next.crash_count == 2 {
                result
                    .paths
                    .entry("two_crashes")
                    .or_insert_with(|| path.clone());
            }
            if action == "crash" && state.phase == 20 {
                result
                    .paths
                    .entry("crash_after_writer_selection")
                    .or_insert_with(|| path.clone());
            }
            if action == "crash" && state.base_only {
                result
                    .paths
                    .entry("base_only_restart")
                    .or_insert_with(|| path.clone());
            }
            if let Entry::Vacant(entry) = seen.entry(next.clone()) {
                if states.len() >= cap {
                    result.violation = Some("INCONCLUSIVE_states");
                    result.unexplored = states.len() - cursor + 1;
                    break 'search;
                }
                let index = states.len();
                entry.insert(index);
                states.push(next);
                parents.push(Some((cursor, action)));
                depths.push(depths[cursor] + 1);
            }
        }
        cursor += 1;
    }
    result.states = states.len();
    if started.elapsed() >= TIME_LIMIT {
        result.violation = Some("INCONCLUSIVE_time");
        result.counterexample.clear();
        result.unexplored = result.unexplored.max(1);
    }
    result.elapsed_ms = started.elapsed().as_millis();
    result
}

fn replay_bootstrap(
    fixture: &str,
    mutant: BootMutant,
    bounded: bool,
    actions: &[&str],
) -> Result<Bootstrap, &'static str> {
    let mut state = Bootstrap::initial(fixture);
    bootstrap_invariant(&state)?;
    for action in actions {
        state = bootstrap_steps(&state, mutant, bounded)
            .into_iter()
            .find(|(_, enabled)| enabled == action)
            .ok_or("disabled_action")?
            .0;
        bootstrap_invariant(&state)?;
    }
    Ok(state)
}

fn emit_bootstrap(fixture: &str, mutant: BootMutant, bounded: bool, report: &BootReport) {
    let elapsed_ms = report.started.elapsed().as_millis();
    let timed_out = elapsed_ms >= TIME_LIMIT.as_millis();
    let violation_value = if timed_out {
        Some("INCONCLUSIVE_time")
    } else {
        report.violation
    };
    let status = match violation_value {
        None => "PASS",
        Some(error) if error.starts_with("INCONCLUSIVE") => "INCONCLUSIVE",
        Some(_) if mutant != BootMutant::None => "EXPECTED_COUNTEREXAMPLE",
        Some(_) => "FAIL",
    };
    let mut witnesses: Vec<_> = report.paths.keys().copied().collect();
    witnesses.sort_unstable();
    let witness_paths: BTreeMap<_, _> = report
        .paths
        .iter()
        .map(|(name, path)| (*name, path))
        .collect();
    let violation = violation_value.map_or_else(|| "null".to_owned(), |value| format!("{value:?}"));
    println!(
        "XADB_DEV3_M06 {{\"suite\":\"bootstrap_{}\",\"fixture\":\"{fixture}\",\"mutant\":\"{}\",\"status\":\"{status}\",\"states\":{},\"transitions\":{},\"max_depth\":{},\"elapsed_ms\":{},\"unexplored\":{},\"wal_peak\":{},\"witnesses\":{witnesses:?},\"witness_paths\":{witness_paths:?},\"counterexample\":{:?},\"violation\":{violation}}}",
        if bounded { "bounded" } else { "closed" },
        mutant.name(),
        report.states,
        report.transitions,
        report.depth,
        elapsed_ms,
        report.unexplored.max(usize::from(timed_out)),
        report.peak,
        report.counterexample,
    );
    assert!(
        !timed_out,
        "INCONCLUSIVE_time including witness/SCC verification"
    );
}

#[test]
fn bootstrap_closed_crash_graph_keeps_both_closures_and_fixed_peak() {
    for fixture in ["active_begin_pruned", "partial_clr", "both_slots_selected"] {
        for bounded in [true, false] {
            let result = explore_bootstrap(fixture, BootMutant::None, bounded, MAX_STATES);
            assert_eq!(result.violation, None, "{:?}", result.counterexample);
            assert_eq!(result.unexplored, 0);
            assert_eq!(result.peak, 24);
            let required: &[&str] = if bounded {
                &[
                    "two_crashes",
                    "reserve_restore",
                    "redo_home",
                    "select_new_base_old_previous",
                ]
            } else {
                &[
                    "select_common_fallback",
                    "write_free_slot",
                    "select_new_base_old_previous",
                    "select_new_common",
                    "write_second_slot",
                    "select_two_bases",
                    "reclaim_old_wal",
                    "charge_and_create_epoch",
                    "select_writer",
                    "append_clr_effect_progress_descriptor",
                    "append_terminal",
                    "stabilize_ready",
                    "crash_after_writer_selection",
                    "base_only_restart",
                ]
            };
            for witness in required {
                assert!(result.paths.contains_key(witness), "missing {witness}");
            }
            for (name, actions) in &result.paths {
                let final_state = replay_bootstrap(fixture, BootMutant::None, bounded, actions)
                    .expect("bootstrap witness replay");
                match *name {
                    "select_common_fallback" | "select_new_common" => {
                        assert_eq!(final_state.current, final_state.previous);
                        assert_ne!(final_state.target, final_state.current);
                    }
                    "write_free_slot" | "write_second_slot" => {
                        let (slot, base) = final_state.pending.expect("unselected slot write");
                        assert_ne!(slot, final_state.current);
                        assert_ne!(slot, final_state.previous);
                        assert!(final_state.slots[slot as usize].is_none());
                        assert!(base.self_contained && base.cut == final_state.cut);
                    }
                    "select_new_base_old_previous" => {
                        assert_ne!(final_state.current, final_state.previous);
                        assert!(final_state.base_only);
                        assert!(
                            final_state.slots[final_state.current as usize].is_some_and(
                                |base| base.self_contained && base.cut == final_state.cut
                            )
                        );
                    }
                    "select_two_bases" => {
                        assert_ne!(final_state.current, final_state.previous);
                        assert!(
                            final_state
                                .slots
                                .iter()
                                .all(|base| base.is_some_and(|base| base.self_contained
                                    && base.cut == final_state.cut
                                    && base.remaining == remaining(final_state.cut)))
                        );
                        assert_eq!(final_state.home_cut, final_state.cut);
                    }
                    "reclaim_old_wal" => {
                        assert!(!final_state.source_forward);
                        assert!(
                            final_state.log[..final_state.cut as usize]
                                .iter()
                                .all(|exists| !exists)
                        );
                        assert_eq!((final_state.headers, final_state.charged_headers), (0, 0));
                        assert!(final_state.base_only && final_state.writer_base.is_none());
                    }
                    "charge_and_create_epoch" => {
                        assert_eq!((final_state.headers, final_state.charged_headers), (1, 1));
                        assert!(!final_state.header_synced && final_state.base_only);
                    }
                    "select_writer" => {
                        assert_eq!(final_state.phase, 20);
                        assert!(!final_state.base_only && final_state.header_synced);
                        assert_eq!(
                            final_state.writer_base,
                            final_state.slots[final_state.current as usize].map(|base| base.cut)
                        );
                    }
                    "reserve_restore" => {
                        assert_eq!(final_state.ledger, Some(remaining(final_state.cut)))
                    }
                    "redo_home" => assert_eq!(final_state.home_cut, final_state.cut),
                    "append_clr_effect_progress_descriptor" => {
                        assert_eq!(final_state.phase, 21);
                        assert!((1..=4).contains(&final_state.cut));
                        assert_eq!(final_state.ledger, Some(remaining(final_state.cut)));
                    }
                    "append_terminal" => {
                        assert!((5..=6).contains(&final_state.cut) && final_state.phase == 21)
                    }
                    "two_crashes" => assert_eq!(final_state.crash_count, 2),
                    "crash_after_writer_selection" => assert!(
                        final_state.phase == 0
                            && !final_state.base_only
                            && final_state.writer_base.is_some()
                    ),
                    "base_only_restart" => assert!(final_state.phase == 0 && final_state.base_only),
                    "stabilize_ready" => assert_eq!(
                        (final_state.phase, final_state.cut, final_state.home_cut),
                        (23, 6, 6)
                    ),
                    _ => assert_eq!(actions.last(), Some(name)),
                }
            }
            emit_bootstrap(fixture, BootMutant::None, bounded, &result);
        }
    }
    for (mutant, fixture, expected) in [
        (
            BootMutant::AccumulateEpoch,
            "active_begin_pruned",
            "bootstrap_peak",
        ),
        (
            BootMutant::OverwriteSelected,
            "active_begin_pruned",
            "selected_recovery_closure_lost",
        ),
        (
            BootMutant::ReclaimPrevious,
            "active_begin_pruned",
            "selected_recovery_closure_lost",
        ),
        (
            BootMutant::IgnoreBaseOnly,
            "partial_clr",
            "writer_continuation_lost",
        ),
        (
            BootMutant::OmitEpochCharge,
            "active_begin_pruned",
            "unaccounted_epoch_charge",
        ),
        (
            BootMutant::ResetDescriptor,
            "active_begin_pruned",
            "persistent_descriptor_mismatch",
        ),
        (
            BootMutant::DoubleRedoCharge,
            "partial_clr",
            "remaining_obligation_mismatch",
        ),
        (
            BootMutant::ReuseClrReservation,
            "active_begin_pruned",
            "remaining_obligation_mismatch",
        ),
    ] {
        let report = explore_bootstrap(fixture, mutant, false, MAX_STATES);
        assert_eq!(
            report.violation,
            Some(expected),
            "{:?}",
            report.counterexample
        );
        assert_eq!(
            replay_bootstrap(fixture, mutant, false, &report.counterexample).err(),
            Some(expected)
        );
        emit_bootstrap(fixture, mutant, false, &report);
    }
}

#[test]
fn bootstrap_budget_exhaustion_cannot_be_a_success_receipt() {
    let result = explore_bootstrap("active_begin_pruned", BootMutant::None, false, 1);
    assert_eq!(result.violation, Some("INCONCLUSIVE_states"));
    assert!(result.unexplored > 0);
    assert!(result.counterexample.is_empty());
    let initial = Boundary {
        charged: [0; 10],
        reserved: [0; 10],
        request: [1; 10],
        capacity: [1; 10],
        decision: None,
        effects: 0,
    };
    for (budget, reason) in [
        (
            Budget {
                states: 1,
                transitions: MAX_TRANSITIONS,
                time: TIME_LIMIT,
            },
            "INCONCLUSIVE_states",
        ),
        (
            Budget {
                states: MAX_STATES,
                transitions: 0,
                time: TIME_LIMIT,
            },
            "INCONCLUSIVE_transitions",
        ),
        (
            Budget {
                states: MAX_STATES,
                transitions: MAX_TRANSITIONS,
                time: Duration::ZERO,
            },
            "INCONCLUSIVE_time",
        ),
    ] {
        let stopped = machine_budget(
            initial.clone(),
            |s| boundary_steps(s, None),
            boundary_invariant,
            None,
            budget,
        );
        assert_eq!(stopped.violation, Some(reason));
        assert!(stopped.unexplored > 0);
        assert!(stopped.counterexample.is_empty());
    }
}

// The table is independent of bootstrap's descriptor/replay implementation.
const OPERATIONS: [[u16; 10]; 5] = [
    [1, 2, 0, 2, 2, 1, 1, 1, 0, 0],
    [1, 3, 0, 2, 2, 1, 0, 1, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 1, 0, 0],
    [1, 2, 0, 2, 2, 1, 1, 1, 0, 8],
    [0, 0, 0, 1, 1, 0, 0, 1, 1, 0],
];

fn admit(
    charged: [u16; 10],
    reserved: [u16; 10],
    requested: [u16; 10],
    capacity: [u16; 10],
    ignore: Option<usize>,
) -> bool {
    (0..10).all(|dimension| {
        ignore == Some(dimension)
            || charged[dimension]
                .checked_add(reserved[dimension])
                .and_then(|value| value.checked_add(requested[dimension]))
                .is_some_and(|value| value <= capacity[dimension])
    })
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Boundary {
    charged: [u16; 10],
    reserved: [u16; 10],
    request: [u16; 10],
    capacity: [u16; 10],
    decision: Option<bool>,
    effects: u8,
}
fn boundary_steps(state: &Boundary, ignore: Option<usize>) -> Vec<(Boundary, &'static str)> {
    if state.decision.is_some() {
        return vec![];
    }
    let mut next = state.clone();
    let accepted = admit(
        state.charged,
        state.reserved,
        state.request,
        state.capacity,
        ignore,
    );
    next.decision = Some(accepted);
    next.effects += u8::from(accepted);
    vec![(next, "admit_before_effect")]
}
fn boundary_invariant(state: &Boundary) -> Result<(), &'static str> {
    let fits = (0..10).all(|at| {
        u32::from(state.charged[at]) + u32::from(state.reserved[at]) + u32::from(state.request[at])
            <= u32::from(state.capacity[at])
    });
    if state.decision.is_some_and(|accepted| accepted != fits) || (!fits && state.effects != 0) {
        return Err("resource_admission");
    }
    Ok(())
}
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Tokens {
    fds: Vec<(u8, u8)>,
    pages: Vec<(u8, u8)>,
    frames: Vec<(u8, u8)>,
    assigned: bool,
}
fn token_steps(state: &Tokens, mutant: &str) -> Vec<(Tokens, &'static str)> {
    if state.assigned {
        return vec![];
    }
    let mut next = state.clone();
    next.assigned = true;
    next.pages = vec![
        (0, 0),
        (
            if mutant == "duplicate_page_token" {
                0
            } else {
                1
            },
            1,
        ),
    ];
    // frame0 belongs to fixed flusher; frame1/2 to selected rollback owner.
    next.frames = vec![(0, 2), (1, 1), (2, 1)];
    next.fds = vec![(0, 2), (1, 1)];
    if mutant == "borrow_protected" {
        next.frames[0] = (0, 0);
    }
    vec![(next, "assign_owned_tokens")]
}
fn token_invariant(state: &Tokens) -> Result<(), &'static str> {
    if state.assigned && state.fds != [(0, 2), (1, 1)] {
        return Err("fixed_fd_partition");
    }
    for (at, (page, _)) in state.pages.iter().enumerate() {
        if *page >= 8 || state.pages[..at].iter().any(|(other, _)| other == page) {
            return Err("duplicate_page_token");
        }
    }
    if state
        .frames
        .iter()
        .any(|(frame, owner)| (*frame == 0 && *owner != 2) || (*frame > 0 && *owner != 1))
    {
        return Err("protected_partition_borrowed");
    }
    Ok(())
}
#[test]
fn every_resource_boundary_rejects_before_effect_and_preserves_flush_partition() {
    let mut normal = 0;
    let mut na = 0;
    let mut negative = 0;
    for (operation, request) in OPERATIONS.into_iter().enumerate() {
        for dimension in 0..10 {
            for delta in [-1_i32, 0, 1] {
                if delta == -1 && request[dimension] == 0 {
                    na += 1;
                    continue;
                }
                let charged = [1, 2, 1, 1, 1, 0, 0, 1, 0, 8];
                let reserved = [1, 1, 1, 0, 0, 1, 1, 0, 0, 0];
                let mut capacity =
                    std::array::from_fn(|at| charged[at] + reserved[at] + request[at]);
                capacity[dimension] = (i32::from(capacity[dimension]) + delta) as u16;
                let initial = Boundary {
                    charged,
                    reserved,
                    request,
                    capacity,
                    decision: None,
                    effects: 0,
                };
                let report = machine(
                    initial.clone(),
                    |s| boundary_steps(s, None),
                    boundary_invariant,
                    Some(16),
                );
                assert_eq!(report.violation, None);
                witness(
                    &report,
                    |s| s.decision == Some(delta >= 0),
                    initial.clone(),
                    |s| boundary_steps(s, None),
                    boundary_invariant,
                );
                let fixture = format!("op{operation}_r{dimension}_d{delta}");
                emit_machine(
                    "admission",
                    &fixture,
                    "none",
                    &report,
                    &["boundary_decision_replayed"],
                );
                normal += 1;
                if delta < 0 {
                    let bad = machine(
                        initial.clone(),
                        |s| boundary_steps(s, Some(dimension)),
                        boundary_invariant,
                        Some(16),
                    );
                    assert_eq!(bad.violation, Some("resource_admission"));
                    assert_eq!(
                        replay_machine(
                            initial,
                            &bad.counterexample,
                            |s| boundary_steps(s, Some(dimension)),
                            boundary_invariant
                        )
                        .err(),
                        Some("resource_admission")
                    );
                    emit_machine(
                        "admission",
                        &fixture,
                        "ignore_dimension",
                        &bad,
                        &["counterexample_replayed"],
                    );
                    negative += 1;
                }
            }
        }
    }
    assert_eq!((normal, na, negative), (130, 20, 30));
    let initial = Tokens {
        fds: vec![],
        pages: vec![],
        frames: vec![],
        assigned: false,
    };
    let good = machine(
        initial.clone(),
        |s| token_steps(s, "none"),
        token_invariant,
        Some(16),
    );
    witness(
        &good,
        |s| s.assigned,
        initial.clone(),
        |s| token_steps(s, "none"),
        token_invariant,
    );
    emit_machine(
        "token_partition",
        "flusher_progress_and_pages",
        "none",
        &good,
        &["distinct_owned_tokens_replayed"],
    );
    for (mutant, reason) in [
        ("borrow_protected", "protected_partition_borrowed"),
        ("duplicate_page_token", "duplicate_page_token"),
    ] {
        let bad = machine(
            initial.clone(),
            |s| token_steps(s, mutant),
            token_invariant,
            Some(16),
        );
        assert_eq!(bad.violation, Some(reason));
        assert_eq!(
            replay_machine(
                initial.clone(),
                &bad.counterexample,
                |s| token_steps(s, mutant),
                token_invariant
            )
            .err(),
            Some(reason)
        );
        emit_machine(
            "token_partition",
            "flusher_progress_and_pages",
            mutant,
            &bad,
            &["counterexample_replayed"],
        );
    }
}

// Generic runtime explorer. The immutable raw journal is separate from candidate
// reservation descriptors, volatile ledger, applied rows and accounting totals.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Record {
    Forward {
        owner: u8,
        slot: u8,
        before: u8,
        after: u8,
        generation: u8,
    },
    Clr {
        owner: u8,
        slot: u8,
        restore: u8,
    },
    Terminal {
        owner: u8,
    },
}
impl Record {
    fn bytes(self) -> u16 {
        match self {
            Self::Forward { .. } => 2,
            Self::Clr { .. } => 3,
            Self::Terminal { .. } => 1,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Runtime {
    log: Vec<Record>,
    intents: Vec<(u8, i8)>,
    statuses: [u8; 4],
    rows: [u8; 2],
    index: [u8; 2],
    applied: usize,
    planned: [u8; 2],
    descriptor: [u16; 2],
    ledger: Option<[u16; 2]>,
    charged: u16,
    locks: [bool; 2],
    active: [bool; 2],
    aborting: [bool; 2],
    crashes: u8,
    recovery: u8, // 0 runtime,1 boot,2 restore,3 redo.
    statement: bool,
    statement_phase: u8, // 0 undo failed statement,1 retry admission,2 refused/abort.
    refused: bool,
    key: u8,
    generation: u8,
    split_retained: bool,
}

fn raw_obligations(state: &Runtime) -> [u16; 2] {
    // Immutable admission/cancellation inputs and raw complete effects are
    // independent of candidate heads, status cache, planned count and ledger.
    let mut future = [0_i16; 2];
    for (owner, count) in &state.intents {
        future[*owner as usize] += i16::from(*count);
    }
    for record in &state.log {
        if let Record::Forward { owner, .. } = record {
            future[*owner as usize] -= 1;
        }
    }
    assert!(future.iter().all(|count| *count >= 0));
    let mut outstanding = future.map(|count| count as u16 * 5);
    for record in &state.log {
        if let Record::Forward { owner, slot, .. } = *record {
            let compensated = state
                .log
                .iter()
                .any(|record| matches!(*record, Record::Clr { slot: done, .. } if done == slot));
            if !compensated {
                outstanding[owner as usize] += 3;
            }
        }
    }
    for (owner, amount) in outstanding.iter_mut().enumerate() {
        let admitted = state
            .intents
            .iter()
            .any(|(id, count)| *id as usize == owner && *count > 0);
        let terminal = state.log.iter().any(
            |record| matches!(*record, Record::Terminal { owner: id } if id as usize == owner),
        );
        *amount += u16::from(admitted && !terminal);
    }
    outstanding
}

impl Runtime {
    fn initial(fixture: &str, key: u8, generation: u8) -> Self {
        let statement = fixture == "statement_failure";
        let one_each = matches!(fixture, "cancel" | "deadlock_victim");
        let mut state = Self {
            log: vec![],
            intents: if statement {
                vec![(0, 2)]
            } else {
                vec![(0, 2), (1, 2)]
            },
            statuses: [0; 4],
            rows: [key; 2],
            index: [key; 2],
            applied: 0,
            planned: if one_each { [1; 2] } else { [0; 2] },
            descriptor: [0; 2],
            ledger: None,
            charged: 2,
            locks: [true; 2],
            active: [true, !statement],
            aborting: [false; 2],
            crashes: 0,
            recovery: 0,
            statement,
            statement_phase: 0,
            refused: false,
            key,
            generation,
            split_retained: statement,
        };
        if statement {
            state.locks[1] = false;
        }
        state.statuses[(generation - 1) as usize] = 1;
        if !statement {
            state.statuses[2 + (generation - 1) as usize] = 1;
        }
        for owner in 0..2_u8 {
            for ordinal in 0..if statement && owner == 1 {
                0
            } else if one_each {
                1
            } else {
                2
            } {
                let before = state.rows[owner as usize];
                let after = (before + 1) % 3;
                state.log.push(Record::Forward {
                    owner,
                    slot: owner * 2 + ordinal,
                    before,
                    after,
                    generation,
                });
                state.rows[owner as usize] = after;
            }
        }
        state.index = state.rows;
        state.applied = state.log.len();
        // Initial descriptors are explicit costs from the frozen workload.
        state.descriptor = if statement {
            [7, 0]
        } else if one_each {
            [9, 9]
        } else {
            [7, 7]
        };
        if fixture == "partial_clr" {
            state.log.push(Record::Clr {
                owner: 0,
                slot: 1,
                restore: (key + 1) % 3,
            });
            state.descriptor[0] -= 3; // Durable complete group, home still behind.
        }
        state.charged += state.log.iter().map(|record| record.bytes()).sum::<u16>();
        state.ledger = Some(state.descriptor);
        state
    }

    fn pending(&self, owner: usize) -> Option<(u8, u8)> {
        self.log.iter().rev().find_map(|record| match *record {
            Record::Forward {
                owner: found,
                slot,
                before,
                ..
            } if found as usize == owner
                && !self.log.iter().any(
                    |record| matches!(*record, Record::Clr { slot: undone, .. } if undone == slot),
                ) =>
            {
                Some((slot, before))
            }
            _ => None,
        })
    }

    fn apply_one(&mut self) {
        match self.log[self.applied] {
            Record::Forward { owner, after, .. } => self.rows[owner as usize] = after,
            Record::Clr { owner, restore, .. } => self.rows[owner as usize] = restore,
            Record::Terminal { .. } => (),
        }
        self.index = self.rows;
        self.applied += 1;
    }
}

fn runtime_steps(state: &Runtime, mutant: &str) -> Vec<(Runtime, &'static str)> {
    let mut out = vec![];
    if state.recovery > 0 {
        let mut next = state.clone();
        let action = match state.recovery {
            1 => {
                next.recovery = 2;
                "boot_reserve"
            }
            2 => {
                if mutant == "reset_descriptor" {
                    next.descriptor = [0; 2];
                }
                next.ledger = Some(if mutant == "reset_descriptor" {
                    [0; 2]
                } else {
                    state.descriptor
                });
                next.recovery = 3;
                "restore_reserve"
            }
            3 => {
                while next.applied < next.log.len() {
                    next.apply_one();
                }
                if mutant == "double_redo_charge"
                    && state
                        .log
                        .iter()
                        .any(|record| matches!(record, Record::Clr { .. }))
                {
                    next.ledger.as_mut().unwrap()[0] = next.ledger.unwrap()[0].saturating_sub(3);
                }
                next.recovery = 0;
                "redo_home"
            }
            _ => unreachable!(),
        };
        out.push((next, action));
    } else if state.applied < state.log.len() {
        let mut next = state.clone();
        next.apply_one();
        out.push((next, "home_apply"));
    } else {
        for owner in 0..2 {
            if !state.active[owner] {
                continue;
            }
            if state.statement && owner == 0 && state.statement_phase == 1 {
                let mut next = state.clone();
                if state.charged + state.descriptor.iter().sum::<u16>() + 5 > WAL_CAPACITY {
                    next.refused = true;
                    next.statement_phase = 2;
                    next.aborting[0] = true;
                    out.push((next, "admission_refused_retained_bytes"));
                } else {
                    let used = state
                        .log
                        .iter()
                        .filter(|record| matches!(record, Record::Forward { .. }))
                        .count() as u8;
                    if used < 4 {
                        next.intents.push((0, 1));
                        next.planned[0] += 1;
                        next.descriptor[0] += 5;
                        next.ledger.as_mut().unwrap()[0] += 5;
                        next.statement_phase = 0;
                        out.push((next, "admit_next_statement"));
                    }
                }
                continue;
            }
            if state.planned[owner] > 0 {
                // Both a normally admitted forward and cancellation before that
                // effect remain enabled. Cancellation only returns unused costs.
                let mut next = state.clone();
                let used = state
                    .log
                    .iter()
                    .filter(|record| matches!(record, Record::Forward { .. }))
                    .count() as u8;
                let slot = if state.statement {
                    used
                } else {
                    owner as u8 * 2 + 1
                };
                let before = state.rows[owner];
                next.log.push(Record::Forward {
                    owner: owner as u8,
                    slot,
                    before,
                    after: (before + 1) % 3,
                    generation: state.generation,
                });
                next.charged += 2;
                next.planned[owner] -= 1;
                next.descriptor[owner] -= 2;
                next.ledger.as_mut().unwrap()[owner] -= 2;
                out.push((
                    next,
                    if owner == 0 {
                        "forward_append_0"
                    } else {
                        "forward_append_1"
                    },
                ));
                if !state.statement {
                    let mut next = state.clone();
                    let unused = u16::from(next.planned[owner]) * 5;
                    next.intents
                        .push((owner as u8, -(next.planned[owner] as i8)));
                    next.planned[owner] = 0;
                    next.descriptor[owner] -= unused;
                    next.ledger.as_mut().unwrap()[owner] -= unused;
                    next.aborting[owner] = true;
                    out.push((
                        next,
                        if owner == 0 {
                            "cancel"
                        } else {
                            "deadlock_victim"
                        },
                    ));
                }
                continue;
            }
            if !state.aborting[owner] && !state.statement {
                let mut next = state.clone();
                next.aborting[owner] = true;
                out.push((next, if owner == 0 { "abort_0" } else { "abort_1" }));
                continue;
            }
            if let Some((slot, restore)) = state.pending(owner) {
                let mut next = state.clone();
                next.log.push(Record::Clr {
                    owner: owner as u8,
                    slot,
                    restore,
                });
                next.charged += 3;
                next.descriptor[owner] -= 3;
                if mutant != "reuse_clr_reservation" {
                    next.ledger.as_mut().unwrap()[owner] -= 3;
                }
                if state.statement && state.statement_phase == 0 {
                    next.statement_phase = 1;
                    if mutant == "reset_statement_ledger" {
                        next.charged = 6;
                    }
                    if mutant == "release_retained_lock" {
                        next.locks[owner] = false;
                    }
                }
                out.push((
                    next,
                    if owner == 0 {
                        "clr_append_0"
                    } else {
                        "clr_append_1"
                    },
                ));
            } else {
                let mut next = state.clone();
                next.log.push(Record::Terminal { owner: owner as u8 });
                next.charged += 1;
                next.descriptor[owner] -= 1;
                next.ledger.as_mut().unwrap()[owner] -= 1;
                next.active[owner] = false;
                next.statuses[owner * 2 + (state.generation - 1) as usize] = 2;
                next.locks[owner] = false;
                out.push((
                    next,
                    if owner == 0 {
                        "terminal_0"
                    } else {
                        "terminal_1"
                    },
                ));
            }
        }
    }
    if state.crashes < 2 {
        let mut next = state.clone();
        next.crashes += 1;
        next.recovery = 1;
        next.ledger = None;
        out.push((next, "crash"));
    }
    out
}

fn runtime_invariant(state: &Runtime) -> Result<(), &'static str> {
    let obligations = raw_obligations(state);
    if obligations != state.descriptor || state.ledger.is_some_and(|ledger| ledger != obligations) {
        return Err("remaining_obligation_mismatch");
    }
    let bytes = 2 + state.log.iter().map(|record| record.bytes()).sum::<u16>();
    if bytes != state.charged {
        return Err("retained_byte_charge");
    }
    if bytes + obligations.iter().sum::<u16>() > WAL_CAPACITY {
        return Err("wal_capacity");
    }
    for owner in 0..2 {
        if state.active[owner] && !state.locks[owner] {
            return Err("retained_lock_lost");
        }
        let admitted = state
            .intents
            .iter()
            .any(|(id, count)| *id as usize == owner && *count > 0);
        let terminal = state.log.iter().any(
            |record| matches!(*record, Record::Terminal { owner: id } if id as usize == owner),
        );
        if state.active[owner] != (admitted && !terminal)
            || state.statuses[owner * 2 + (state.generation - 1) as usize]
                != if !admitted {
                    0
                } else if terminal {
                    2
                } else {
                    1
                }
        {
            return Err("status_inventory");
        }
        // Independent algebra over the applied raw prefix, not candidate rows.
        let count = state.log[..state.applied]
            .iter()
            .fold(0_i16, |count, record| match record {
                Record::Forward { owner: id, .. } if *id as usize == owner => count + 1,
                Record::Clr { owner: id, .. } if *id as usize == owner => count - 1,
                _ => count,
            });
        if state.rows[owner] != (i16::from(state.key) + count).rem_euclid(3) as u8
            || state.index[owner] != state.rows[owner]
        {
            return Err("row_index_undo");
        }
    }
    if state.statement && !state.split_retained {
        return Err("completed_split_charge_lost");
    }
    for record in &state.log {
        if let Record::Forward {
            slot, generation, ..
        } = record
            && (*slot >= 4 || !(1..=2).contains(generation))
        {
            return Err("undo_identity_domain");
        }
    }
    if state.statement
        && state.statement_phase == 1
        && state.applied == state.log.len()
        && state.rows[0] != (state.key + 1) % 3
    {
        return Err("prior_statement_not_retained");
    }
    Ok(())
}

struct MachineReport<S> {
    states: Vec<S>,
    parents: Vec<Option<(usize, &'static str)>>,
    depths: Vec<usize>,
    edges: Vec<Vec<(usize, &'static str)>>,
    transitions: usize,
    elapsed_ms: u128,
    violation: Option<&'static str>,
    counterexample: Vec<&'static str>,
    unexplored: usize,
    witness_paths: RefCell<BTreeMap<String, Vec<&'static str>>>,
    started: Instant,
}
#[derive(Clone, Copy)]
struct Budget {
    states: usize,
    transitions: usize,
    time: Duration,
}
fn machine<S: Clone + Eq + Hash + Debug>(
    initial: S,
    steps: impl Fn(&S) -> Vec<(S, &'static str)>,
    check: impl Fn(&S) -> Result<(), &'static str>,
    bound: Option<usize>,
) -> MachineReport<S> {
    machine_budget(
        initial,
        steps,
        check,
        bound,
        Budget {
            states: MAX_STATES,
            transitions: MAX_TRANSITIONS,
            time: TIME_LIMIT,
        },
    )
}
fn machine_budget<S: Clone + Eq + Hash + Debug>(
    initial: S,
    steps: impl Fn(&S) -> Vec<(S, &'static str)>,
    check: impl Fn(&S) -> Result<(), &'static str>,
    bound: Option<usize>,
    budget: Budget,
) -> MachineReport<S> {
    let started = Instant::now();
    let mut result = MachineReport {
        started,
        states: vec![initial.clone()],
        parents: vec![None],
        depths: vec![0],
        edges: vec![vec![]],
        transitions: 0,
        elapsed_ms: 0,
        witness_paths: RefCell::new(BTreeMap::new()),
        violation: check(&initial).err(),
        counterexample: vec![],
        unexplored: 0,
    };
    let mut seen = HashMap::from([(initial, 0)]);
    let mut cursor = 0;
    'outer: while result.violation.is_none() && cursor < result.states.len() {
        if started.elapsed() >= budget.time {
            result.violation = Some("INCONCLUSIVE_time");
            break;
        }
        if bound.is_some_and(|depth| result.depths[cursor] >= depth) {
            cursor += 1;
            continue;
        }
        for (next, action) in steps(&result.states[cursor]) {
            if result.transitions >= budget.transitions {
                result.violation = Some("INCONCLUSIVE_transitions");
                break 'outer;
            }
            result.transitions += 1;
            if let Err(reason) = check(&next) {
                result.violation = Some(reason);
                result.counterexample = trace(&result.parents, cursor);
                result.counterexample.push(action);
                break 'outer;
            }
            let id = match seen.entry(next.clone()) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    if result.states.len() >= budget.states {
                        result.violation = Some("INCONCLUSIVE_states");
                        break 'outer;
                    }
                    let id = result.states.len();
                    entry.insert(id);
                    result.states.push(next);
                    result.parents.push(Some((cursor, action)));
                    result.depths.push(result.depths[cursor] + 1);
                    result.edges.push(vec![]);
                    id
                }
            };
            result.edges[cursor].push((id, action));
        }
        cursor += 1;
    }
    if result.violation.is_some() {
        result.unexplored = result.states.len().saturating_sub(cursor);
    }
    if started.elapsed() >= budget.time {
        result.violation = Some("INCONCLUSIVE_time");
        result.counterexample.clear();
        result.unexplored = result.unexplored.max(1);
    }
    result.elapsed_ms = started.elapsed().as_millis();
    result
}
fn replay_machine<S: Clone>(
    mut state: S,
    actions: &[&str],
    steps: impl Fn(&S) -> Vec<(S, &'static str)>,
    check: impl Fn(&S) -> Result<(), &'static str>,
) -> Result<S, &'static str> {
    check(&state)?;
    for action in actions {
        state = steps(&state)
            .into_iter()
            .find(|(_, name)| name == action)
            .ok_or("disabled_action")?
            .0;
        check(&state)?;
    }
    Ok(state)
}
fn witness<S: Clone + Eq + Debug>(
    report: &MachineReport<S>,
    predicate: impl Fn(&S) -> bool,
    initial: S,
    steps: impl Fn(&S) -> Vec<(S, &'static str)>,
    check: impl Fn(&S) -> Result<(), &'static str>,
) {
    let at = report
        .states
        .iter()
        .position(&predicate)
        .expect("required witness unreachable");
    let replayed = replay_machine(initial, &trace(&report.parents, at), steps, check)
        .expect("normal witness replay");
    assert_eq!(replayed, report.states[at]);
    assert!(predicate(&replayed));
    report
        .witness_paths
        .borrow_mut()
        .insert(format!("state{at}"), trace(&report.parents, at));
}
fn emit_machine<S>(
    suite: &str,
    fixture: &str,
    mutant: &str,
    report: &MachineReport<S>,
    witnesses: &[&str],
) {
    let elapsed_ms = report.started.elapsed().as_millis();
    let timed_out = elapsed_ms >= TIME_LIMIT.as_millis();
    let violation_value = if timed_out {
        Some("INCONCLUSIVE_time")
    } else {
        report.violation
    };
    let status = match violation_value {
        None => "PASS",
        Some(reason) if reason.starts_with("INCONCLUSIVE") => "INCONCLUSIVE",
        Some(_) if mutant != "none" => "EXPECTED_COUNTEREXAMPLE",
        _ => "FAIL",
    };
    let violation =
        violation_value.map_or_else(|| "null".to_owned(), |reason| format!("{reason:?}"));
    println!(
        "XADB_DEV3_M06 {{\"suite\":\"{suite}\",\"fixture\":\"{fixture}\",\"mutant\":\"{mutant}\",\"status\":\"{status}\",\"states\":{},\"transitions\":{},\"max_depth\":{},\"elapsed_ms\":{},\"unexplored\":{},\"witnesses\":{witnesses:?},\"witness_paths\":{:?},\"counterexample\":{:?},\"violation\":{violation}}}",
        report.states.len(),
        report.transitions,
        report
            .depths
            .iter()
            .max()
            .copied()
            .unwrap()
            .max(report.counterexample.len()),
        elapsed_ms,
        report.unexplored.max(usize::from(timed_out)),
        report.witness_paths.borrow(),
        report.counterexample
    );
    assert!(
        !timed_out,
        "INCONCLUSIVE_time including witness/SCC verification"
    );
}

#[test]
fn runtime_reservations_survive_crashes_and_repeated_statement_failures() {
    for fixture in [
        "begin_pruned_active",
        "partial_clr",
        "cancel",
        "deadlock_victim",
        "statement_failure",
    ] {
        for key in 0..3 {
            for generation in 1..=2 {
                let initial = Runtime::initial(fixture, key, generation);
                let report = machine(
                    initial.clone(),
                    |s| runtime_steps(s, "none"),
                    runtime_invariant,
                    None,
                );
                assert_eq!(
                    report.violation, None,
                    "{fixture}: {:?}",
                    report.counterexample
                );
                witness(
                    &report,
                    |s| s.active == [false; 2] && s.applied == s.log.len(),
                    initial.clone(),
                    |s| runtime_steps(s, "none"),
                    runtime_invariant,
                );
                witness(
                    &report,
                    |s| s.crashes == 2 && s.recovery == 0,
                    initial.clone(),
                    |s| runtime_steps(s, "none"),
                    runtime_invariant,
                );
                witness(
                    &report,
                    |s| s.recovery == 1 && s.applied < s.log.len(),
                    initial.clone(),
                    |s| runtime_steps(s, "none"),
                    runtime_invariant,
                );
                if fixture == "statement_failure" {
                    witness(
                        &report,
                        |s| s.refused && s.charged + s.descriptor.iter().sum::<u16>() == 23,
                        initial.clone(),
                        |s| runtime_steps(s, "none"),
                        runtime_invariant,
                    );
                }
                emit_machine(
                    "runtime",
                    &format!("{fixture}_key{key}_generation{generation}"),
                    "none",
                    &report,
                    &[
                        "terminal",
                        "two_crashes_restore_redo",
                        "append_before_home_crash",
                        "witnesses_replayed",
                    ],
                );
            }
        }
    }
    for (mutant, fixture, reason) in [
        (
            "reset_descriptor",
            "begin_pruned_active",
            "remaining_obligation_mismatch",
        ),
        (
            "reuse_clr_reservation",
            "begin_pruned_active",
            "remaining_obligation_mismatch",
        ),
        (
            "double_redo_charge",
            "partial_clr",
            "remaining_obligation_mismatch",
        ),
        (
            "reset_statement_ledger",
            "statement_failure",
            "retained_byte_charge",
        ),
        (
            "release_retained_lock",
            "statement_failure",
            "retained_lock_lost",
        ),
    ] {
        let initial = Runtime::initial(fixture, 0, 1);
        let report = machine(
            initial.clone(),
            |s| runtime_steps(s, mutant),
            runtime_invariant,
            Some(16),
        );
        assert_eq!(report.violation, Some(reason));
        assert_eq!(
            replay_machine(
                initial,
                &report.counterexample,
                |s| runtime_steps(s, mutant),
                runtime_invariant
            )
            .err(),
            Some(reason)
        );
        emit_machine(
            "runtime",
            fixture,
            mutant,
            &report,
            &["shortest_counterexample_replayed"],
        );
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Pressure {
    dirty: [bool; 3],
    page_lsn: [u8; 3],
    rec_lsn: [u8; 3],
    pinned: bool,
    reader: bool,
    releasable: bool,
    hot: u8,
    phase: u8,
    cursor: usize,
    scanned: u8,
    image: Option<(usize, u8)>,
    retained: u8,
    reclaimed: bool,
    admission: bool,
    refused: bool,
    flushes: u8,
}
impl Pressure {
    fn initial(fixture: &str, releasable: bool) -> Self {
        let dirty = match fixture {
            "no_dirty" => [false; 3],
            "hot_redirty" => [true, true, false],
            _ => [true, false, false],
        };
        Self {
            dirty,
            page_lsn: [10, 11, 12],
            rec_lsn: [10, 11, 12],
            pinned: fixture == "pinned_candidate",
            reader: true,
            releasable,
            hot: if fixture == "hot_redirty" { 2 } else { 0 },
            phase: if fixture == "dwb_inflight" { 2 } else { 0 },
            cursor: 0,
            scanned: 0,
            image: (fixture == "dwb_inflight").then_some((0, 10)),
            retained: 4,
            reclaimed: false,
            admission: false,
            refused: false,
            flushes: 0,
        }
    }
}
fn pressure_steps(state: &Pressure, mutant: &str) -> Vec<(Pressure, &'static str)> {
    if state.phase == 5 {
        return vec![];
    }
    let mut out = vec![];
    if state.reader && state.releasable {
        let mut next = state.clone();
        next.reader = false;
        next.pinned = false;
        out.push((next, "reader_release"));
    }
    if state.hot > 0 {
        let mut next = state.clone();
        next.hot -= 1;
        next.page_lsn[0] += 1;
        next.dirty[0] = true;
        next.retained += 1;
        out.push((next, "existing_hot_write"));
    }
    if mutant == "block_maintenance" {
        return out;
    }
    if mutant == "busy_noop" {
        out.push((state.clone(), "maintenance_noop"));
    }
    let mut next = state.clone();
    let action = match state.phase {
        0 => {
            if !state.dirty.iter().any(|dirty| *dirty) {
                next.phase = 4;
                "scan_no_dirty"
            } else if state.dirty[state.cursor] && !(state.cursor == 0 && state.pinned) {
                next.phase = 1;
                next.scanned = 0;
                "select_dirty"
            } else {
                next.cursor = (state.cursor + 1) % 3;
                next.scanned += 1;
                if next.scanned == 3 {
                    next.phase = 4;
                }
                "scan_next"
            }
        }
        1 => {
            next.image = Some((state.cursor, state.page_lsn[state.cursor]));
            next.phase = 2;
            "flush_copy"
        }
        2 => {
            next.phase = 3;
            "flush_io"
        }
        3 => {
            let (page, copied_lsn) = state.image.expect("copied image");
            if state.page_lsn[page] == copied_lsn {
                next.dirty[page] = false;
            }
            next.image = None;
            next.phase = 0;
            next.cursor = (state.cursor + 1) % 3;
            next.flushes += 1;
            "flush_complete"
        }
        4 if state.hot == 0 => {
            if state.reader && state.releasable {
                ""
            } else if !state.dirty.iter().any(|dirty| *dirty)
                && (!state.reader || mutant == "confuse_q_lsn")
            {
                // Wrong mutant uses q=2 < oldest retained LSN=10 as permission.
                next.retained = 0;
                next.reclaimed = true;
                next.admission = true;
                next.phase = 5;
                "checkpoint_reclaim_reopen"
            } else if state.dirty.iter().any(|dirty| *dirty) && !state.pinned {
                next.phase = 0;
                next.scanned = 0;
                "rescan_after_release"
            } else {
                next.refused = true;
                next.phase = 5;
                "quiescent_pressure_refusal"
            }
        }
        _ => "",
    };
    if !action.is_empty() {
        out.push((next, action));
    }
    out
}
fn pressure_invariant(state: &Pressure) -> Result<(), &'static str> {
    if state.reclaimed
        && (state.reader || state.dirty.iter().any(|dirty| *dirty) || state.image.is_some())
    {
        return Err("retention_floor_lost");
    }
    if state.admission && (!state.reclaimed || state.retained != 0) {
        return Err("reopen_without_real_reclaim");
    }
    if state.scanned > 3 {
        return Err("unbounded_scan");
    }
    if state.image.is_some() && !matches!(state.phase, 2 | 3) {
        return Err("doublewrite_owner");
    }
    // q is deliberately absent here: dirty/WAL retention is a LogPos relation.
    for page in 0..3 {
        if state.dirty[page] && state.rec_lsn[page] > state.page_lsn[page] {
            return Err("dirty_floor");
        }
    }
    Ok(())
}

fn pressure_actor(action: &str) -> u8 {
    match action {
        "reader_release" => 0,
        "existing_hot_write" => 1,
        _ => 2,
    }
}
// Independent closed SCC analysis. Returns prefix + a closed fair walk, or a
// reachable pending deadlock. Every member and actor edge is replayed below.
fn pressure_fair_counterexample(report: &MachineReport<Pressure>) -> Option<Vec<&'static str>> {
    let n = report.states.len();
    let active: Vec<_> = report.states.iter().map(|state| state.phase != 5).collect();
    let mut visited = vec![false; n];
    let mut order = vec![];
    for root in 0..n {
        if !active[root] || visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((at, edge)) = stack.last_mut() {
            if *edge == report.edges[*at].len() {
                order.push(*at);
                stack.pop();
                continue;
            }
            let next = report.edges[*at][*edge].0;
            *edge += 1;
            if active[next] && !visited[next] {
                visited[next] = true;
                stack.push((next, 0));
            }
        }
    }
    let mut reverse = vec![vec![]; n];
    for (from, edges) in report.edges.iter().enumerate() {
        if active[from] {
            for (to, _) in edges {
                if active[*to] {
                    reverse[*to].push(from);
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
        if members.len() == 1 && report.edges[root].is_empty() {
            return Some(trace(&report.parents, root));
        }
        if members.len() == 1 && !report.edges[root].iter().any(|(to, _)| *to == root) {
            continue;
        }
        let fair = (0..3).all(|actor| {
            !members.iter().all(|at| {
                report.edges[*at]
                    .iter()
                    .any(|(_, action)| pressure_actor(action) == actor)
            }) || members.iter().any(|at| {
                report.edges[*at]
                    .iter()
                    .any(|(to, action)| component[*to] == root && pressure_actor(action) == actor)
            })
        });
        if !fair {
            continue;
        }
        let mut path = trace(&report.parents, root);
        let mut at = root;
        let mut goals: Vec<_> = members.iter().map(|id| (*id, None)).collect();
        for actor in 0..3 {
            if let Some((from, edge)) = members.iter().find_map(|from| {
                report.edges[*from]
                    .iter()
                    .find(|(to, action)| component[*to] == root && pressure_actor(action) == actor)
                    .map(|edge| (*from, *edge))
            }) {
                goals.push((from, Some(edge)));
            }
        }
        goals.push((root, None));
        for (goal, edge) in goals {
            let mut reached = HashMap::from([(at, Vec::<&'static str>::new())]);
            let mut queue = std::collections::VecDeque::from([at]);
            while let Some(node) = queue.pop_front() {
                if node == goal {
                    break;
                }
                for (to, action) in &report.edges[node] {
                    if component[*to] == root && !reached.contains_key(to) {
                        let mut steps = reached[&node].clone();
                        steps.push(*action);
                        reached.insert(*to, steps);
                        queue.push_back(*to);
                    }
                }
            }
            path.extend(reached.remove(&goal).unwrap());
            at = goal;
            if let Some((to, action)) = edge {
                path.push(action);
                at = to;
            }
        }
        assert_eq!(at, root);
        let prefix = trace(&report.parents, root).len();
        let mut replay_at = 0;
        let mut cycle_states = vec![];
        let mut cycle_actors = vec![];
        for (step, action) in path.iter().enumerate() {
            let (to, action) = report.edges[replay_at]
                .iter()
                .find(|(_, name)| name == action)
                .expect("cycle edge replay");
            if step >= prefix {
                cycle_states.push(replay_at);
                cycle_actors.push(pressure_actor(action));
            }
            replay_at = *to;
        }
        assert!(!cycle_states.is_empty());
        assert_eq!(replay_at, cycle_states[0]);
        for actor in 0..3 {
            if cycle_states.iter().all(|at| {
                report.edges[*at]
                    .iter()
                    .any(|(_, action)| pressure_actor(action) == actor)
            }) {
                assert!(cycle_actors.contains(&actor));
            }
        }
        return Some(path);
    }
    None
}

#[test]
fn pressure_closed_graph_drains_or_preserves_reader_floor() {
    for fixture in [
        "old_dirty",
        "hot_redirty",
        "pinned_candidate",
        "dwb_inflight",
        "no_dirty",
    ] {
        for releasable in [false, true] {
            let initial = Pressure::initial(fixture, releasable);
            let report = machine(
                initial.clone(),
                |s| pressure_steps(s, "none"),
                pressure_invariant,
                None,
            );
            assert_eq!(report.violation, None, "{:?}", report.counterexample);
            assert_eq!(pressure_fair_counterexample(&report), None);
            witness(
                &report,
                |s| {
                    s.phase == 5
                        && if releasable {
                            s.admission && s.retained == 0
                        } else {
                            s.refused && s.reader && s.retained > 0
                        }
                },
                initial.clone(),
                |s| pressure_steps(s, "none"),
                pressure_invariant,
            );
            if fixture != "no_dirty" && (fixture != "pinned_candidate" || releasable) {
                witness(
                    &report,
                    |s| s.flushes > 0,
                    initial,
                    |s| pressure_steps(s, "none"),
                    pressure_invariant,
                );
            }
            emit_machine(
                "pressure",
                &format!("{fixture}_release{releasable}"),
                "none",
                &report,
                &[
                    "terminal_replayed",
                    "bounded_scan",
                    "no_weak_fair_pending_scc",
                ],
            );
        }
    }
    for mutant in ["block_maintenance", "busy_noop", "confuse_q_lsn"] {
        let initial = Pressure::initial("old_dirty", false);
        let mut report = machine(
            initial.clone(),
            |s| pressure_steps(s, mutant),
            pressure_invariant,
            None,
        );
        if mutant == "confuse_q_lsn" {
            assert_eq!(report.violation, Some("retention_floor_lost"));
            assert_eq!(
                replay_machine(
                    initial,
                    &report.counterexample,
                    |s| pressure_steps(s, mutant),
                    pressure_invariant
                )
                .err(),
                Some("retention_floor_lost")
            );
        } else {
            assert_eq!(report.violation, None);
            report.counterexample =
                pressure_fair_counterexample(&report).expect("fair liveness failure");
            assert!(
                replay_machine(
                    initial,
                    &report.counterexample,
                    |s| pressure_steps(s, mutant),
                    pressure_invariant
                )
                .is_ok()
            );
            report.violation = Some("weak_fair_noncompletion");
        }
        emit_machine(
            "pressure",
            "old_dirty_reader_held",
            mutant,
            &report,
            &["counterexample_replayed"],
        );
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Profile {
    valid: u8,          // 0 valid,1 missing,2 corrupt,3 unsupported.
    capacity: [u16; 4], // memory/frame/FD/owned.
    foreground: u16,
    descriptor: u16,
    reserve: u16,
    phase: u8, // 0 decode/admit,1 admitted,2 rejected,3 first effect.
    writes: u8,
}
fn profile_steps(state: &Profile, truncate: bool) -> Vec<(Profile, &'static str)> {
    let mut next = state.clone();
    match state.phase {
        0 => {
            if state.valid != 0
                || state
                    .capacity
                    .iter()
                    .zip([3, 3, 2, 42])
                    .any(|(have, need)| *have < need)
            {
                next.phase = 2;
                vec![(next, "reject_before_write")]
            } else {
                next.reserve = if truncate {
                    state.descriptor.min(state.foreground)
                } else {
                    state.descriptor
                };
                next.phase = 1;
                vec![(next, "restore_original_obligation")]
            }
        }
        1 => {
            next.writes += 1;
            next.phase = 3;
            vec![(next, "first_recovery_write")]
        }
        _ => vec![],
    }
}
fn profile_invariant(state: &Profile) -> Result<(), &'static str> {
    if matches!(state.phase, 1 | 3) && state.reserve != 4 * 3 + 2 {
        return Err("obligation_truncated");
    }
    if state.phase == 2 && state.writes != 0 {
        return Err("write_before_profile_rejection");
    }
    Ok(())
}
#[test]
fn recovery_profile_restores_old_obligation_or_rejects_before_writes() {
    let fixtures = [
        "foreground_limit_lowered",
        "memory_short",
        "frame_short",
        "fd_short",
        "owned_short",
        "missing",
        "corrupt",
        "unsupported",
    ];
    for (at, fixture) in fixtures.into_iter().enumerate() {
        let mut initial = Profile {
            valid: 0,
            capacity: [3, 3, 2, 42],
            foreground: 1,
            descriptor: 14,
            reserve: 0,
            phase: 0,
            writes: 0,
        };
        if (1..=4).contains(&at) {
            initial.capacity[at - 1] -= 1;
        }
        if at >= 5 {
            initial.valid = (at - 4) as u8;
        }
        let report = machine(
            initial.clone(),
            |s| profile_steps(s, false),
            profile_invariant,
            Some(16),
        );
        assert_eq!(report.violation, None);
        witness(
            &report,
            |s| s.phase == if at == 0 { 3 } else { 2 },
            initial.clone(),
            |s| profile_steps(s, false),
            profile_invariant,
        );
        emit_machine(
            "recovery_profile",
            fixture,
            "none",
            &report,
            &["first_effect_or_rejection_replayed"],
        );
        if at == 0 {
            let bad = machine(
                initial.clone(),
                |s| profile_steps(s, true),
                profile_invariant,
                Some(16),
            );
            assert_eq!(bad.violation, Some("obligation_truncated"));
            assert_eq!(
                replay_machine(
                    initial,
                    &bad.counterexample,
                    |s| profile_steps(s, true),
                    profile_invariant
                )
                .err(),
                Some("obligation_truncated")
            );
            emit_machine(
                "recovery_profile",
                fixture,
                "truncate_obligation_to_foreground",
                &bad,
                &["counterexample_replayed"],
            );
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Growth {
    physical: u8,
    intent_end: u8,
    selected_end: u8,
    synced_end: u8,
    reserve: Option<u8>,
    charged: Option<u8>,
    capacity: u8,
    phase: u8,
    crashes: u8,
    committed: bool,
}
fn growth_steps(state: &Growth, mutant: &str) -> Vec<(Growth, &'static str)> {
    let mut next = state.clone();
    let mut out = vec![];
    let action = match state.phase {
        0 => {
            if state.capacity < 16 {
                next.phase = 7;
                "admission_reject"
            } else {
                next.reserve = Some(8);
                next.charged = Some(8);
                next.intent_end = 16;
                next.phase = 1;
                "reserve_growth_and_intent"
            }
        }
        1 => {
            next.physical = 16;
            next.reserve = Some(0);
            next.charged = Some(16);
            next.phase = 2;
            "extend_and_convert_growth_to_charge"
        }
        2 => {
            next.synced_end = state.physical;
            next.phase = 3;
            "sync_file_length"
        }
        3 => {
            next.selected_end = state.synced_end;
            next.phase = 4;
            "select_filemap_and_dirsync"
        }
        4 => {
            next.committed = true;
            next.phase = 5;
            "commit_reference_to_extent"
        }
        6 => {
            next.charged = Some(state.physical);
            next.reserve = Some(match mutant {
                "double_growth" => state.intent_end - 8,
                "reset_growth" => 0,
                _ => state.intent_end.saturating_sub(state.physical),
            });
            next.phase = if state.intent_end == 8 {
                0
            } else if state.physical < state.intent_end {
                1
            } else if state.synced_end < state.physical {
                2
            } else if state.selected_end < state.physical {
                3
            } else if state.committed {
                5
            } else {
                4
            };
            "reconcile_physical_growth"
        }
        _ => "",
    };
    if !action.is_empty() {
        out.push((next, action));
    }
    if mutant == "commit_before_filemap" && state.phase == 2 {
        let mut next = state.clone();
        next.committed = true;
        out.push((next, "early_commit"));
    }
    if state.crashes < 2 && state.phase != 7 {
        let mut next = state.clone();
        next.reserve = None;
        next.charged = None;
        next.phase = 6;
        next.crashes += 1;
        out.push((next, "crash"));
    }
    out
}
fn growth_invariant(state: &Growth) -> Result<(), &'static str> {
    if let (Some(charge), Some(reserve)) = (state.charged, state.reserve) {
        let raw_missing = (8..state.intent_end)
            .filter(|page| *page >= state.physical)
            .count() as u8;
        if charge != state.physical || reserve != raw_missing {
            return Err("physical_growth_reconciliation");
        }
        if charge + reserve > state.capacity {
            return Err("owned_growth_capacity");
        }
    }
    if state.committed && (state.selected_end < 16 || state.synced_end < 16 || state.physical < 16)
    {
        return Err("reference_before_filemap_durable");
    }
    Ok(())
}
#[test]
fn nonzero_growth_reconciles_physical_bytes_after_each_crash_cut() {
    for capacity in [15, 16, 17] {
        let initial = Growth {
            physical: 8,
            intent_end: 8,
            selected_end: 8,
            synced_end: 8,
            reserve: Some(0),
            charged: Some(8),
            capacity,
            phase: 0,
            crashes: 0,
            committed: false,
        };
        let report = machine(
            initial.clone(),
            |s| growth_steps(s, "none"),
            growth_invariant,
            None,
        );
        assert_eq!(report.violation, None, "{:?}", report.counterexample);
        witness(
            &report,
            |s| {
                if capacity == 15 {
                    s.phase == 7 && s.physical == 8
                } else {
                    s.committed && s.crashes == 2 && s.reserve == Some(0)
                }
            },
            initial.clone(),
            |s| growth_steps(s, "none"),
            growth_invariant,
        );
        emit_machine(
            "growth",
            &format!("capacity{capacity}"),
            "none",
            &report,
            &["nonzero_growth_or_boundary_rejection_replayed"],
        );
        if capacity == 16 {
            for (mutant, reason) in [
                ("double_growth", "physical_growth_reconciliation"),
                ("reset_growth", "physical_growth_reconciliation"),
                ("commit_before_filemap", "reference_before_filemap_durable"),
            ] {
                let bad = machine(
                    initial.clone(),
                    |s| growth_steps(s, mutant),
                    growth_invariant,
                    Some(16),
                );
                assert_eq!(bad.violation, Some(reason));
                assert_eq!(
                    replay_machine(
                        initial.clone(),
                        &bad.counterexample,
                        |s| growth_steps(s, mutant),
                        growth_invariant
                    )
                    .err(),
                    Some(reason)
                );
                emit_machine(
                    "growth",
                    "capacity16",
                    mutant,
                    &bad,
                    &["shortest_counterexample_replayed"],
                );
            }
        }
    }
}
