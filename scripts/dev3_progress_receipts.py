"""Strict, non-executing receipt checks for the frozen DEV3 M03/M06 models.

Inventories below are transcribed from dev3-progress-model.md,
dev3-reservation-model.md and their reviewed Rust tests. They do not depend on
candidate output or artifacts/. The caller owns source identity, clean export,
execution and retained logs. Acceptance here checks receipt structure and the
exact completed test; semantic path replay/SCC/rank checks run inside that test.
It does not independently replay Rust transitions or grant product DEV3 status.
"""

from dataclasses import dataclass
import itertools
import json
import re


PROGRESS_MODELS = {
    "M03": {
        "target": "dev3_progress_model",
        "contract": "devdoc/contracts/dev3-progress-model.md",
        "prefix": "XADB_DEV3_M03 ",
        "tests": (
            "observed_row_versions_cover_both_isolations_and_all_fixed_conflicts",
            "all_page_pairs_use_atomic_pin_reservation_sorted_latches_and_cleanup",
            "logical_rollback_preserves_split_and_other_writer_ack",
            "fifo_turn_closed_graph_excludes_weak_fair_starvation",
        ),
        "budget_tests": ("progress_exploration_caps_never_become_pass",),
        "normal": 232, "negative": 21, "not_applicable": 0,
    },
    "M06": {
        "target": "dev3_reservation_model",
        "contract": "devdoc/contracts/dev3-reservation-model.md",
        "prefix": "XADB_DEV3_M06 ",
        "tests": (
            "every_resource_boundary_rejects_before_effect_and_preserves_flush_partition",
            "runtime_reservations_survive_crashes_and_repeated_statement_failures",
            "pressure_closed_graph_drains_or_preserves_reader_floor",
            "bootstrap_closed_crash_graph_keeps_both_closures_and_fixed_peak",
            "recovery_profile_restores_old_obligation_or_rejects_before_writes",
            "nonzero_growth_reconciles_physical_bytes_after_each_crash_cut",
        ),
        "budget_tests": ("bootstrap_budget_exhaustion_cannot_be_a_success_receipt",),
        "normal": 188, "negative": 52, "not_applicable": 20,
    },
}
STATE_CAP = 1_000_000
TRANSITION_CAP = 8_000_000
TIME_LIMIT_MS = 60_000
SUMMARY_PREFIX = "XADB_DEV3_M03_SUMMARY "
CHECKER_TEST_COUNT = 24


@dataclass(frozen=True)
class Case:
    suite: str
    fixture: str
    mutant: str = "none"
    violation: str | None = None
    graph: str = "bounded_depth16"
    witnesses: frozenset = frozenset()
    # Named paths: exact names. State paths: exact count, IDs remain model output.
    paths: frozenset | int = frozenset()
    crash_bound: int | None = 0
    cycle_start: int | None = None
    initial_deadlock: bool = False

    @property
    def key(self):
        return self.suite, self.fixture, self.mutant


def words(value):
    return frozenset(value.split())


def actors(names, owners):
    return {f"{owner}:{name}" for owner in owners for name in names.split()}


ACTION_SETS = {
    "observed_row_version": frozenset(
        actors("observe lock_attempt_wait lock_acquired validate apply "
               "rollback_all_prior_own_writes commit", (0,))
        | actors("other_no_effect split_move other_lock other_apply "
                 "other_abort other_commit_unlock", (1,))
        | {"2:reader_check"}),
    "page_preparation": frozenset(
        actors("reserve_all partial_pin_wait acquire_gate reject_stale_incarnation "
               "apply try_failed_cleanup try_latch new_page_cleanup_and_reprepare "
               "release_all", (0, 1))
        | actors("reader_pin reader_latch reader_release", (2,))),
    "split_rollback": frozenset(
        actors("first_writer_change complete_split logical_rollback", (0,))
        | actors("other_writer_change other_writer_commit", (1,))
        | {"2:reader_check"}),
    "closed_progress": frozenset(
        actors("failed_try enqueue_ticket enqueue_workspace_ticket head_prepare "
               "close_content_and_drain apply_and_release_turn", (0, 1))
        | actors("q_read horizon_register guard_release content_enter content_exit", (2,))
        | actors("flush_copy flush_io flush_complete", (3,))),
    "admission": words("admit_before_effect"),
    "token_partition": words("assign_owned_tokens"),
    "runtime": words(
        "boot_reserve restore_reserve redo_home home_apply "
        "admission_refused_retained_bytes admit_next_statement "
        "forward_append_0 forward_append_1 cancel deadlock_victim abort_0 abort_1 "
        "clr_append_0 clr_append_1 terminal_0 terminal_1 crash"),
    "pressure": words(
        "reader_release existing_hot_write maintenance_noop scan_no_dirty "
        "select_dirty scan_next flush_copy flush_io flush_complete "
        "checkpoint_reclaim_reopen rescan_after_release quiescent_pressure_refusal"),
    "recovery_profile": words(
        "reject_before_write restore_original_obligation first_recovery_write"),
    "growth": words(
        "admission_reject reserve_growth_and_intent extend_and_convert_growth_to_charge "
        "sync_file_length select_filemap_and_dirsync commit_reference_to_extent "
        "reconcile_physical_growth early_commit crash"),
}

# Phase order is fixed in bootstrap_steps. These slices describe *successful*
# witness edges before a mutant's violating edge, not observations of its stdout.
BOOT_PHASES = (
    "boot_reserve", "doublewrite_repair", "analysis_complete_prefix",
    "reserve_restore", "redo_home", "sync_source", "sync_home", "retire_doublewrite",
    "select_common_fallback", "write_free_slot", "sync_free_slot",
    "select_new_base_old_previous", "select_new_common", "write_second_slot",
    "sync_second_slot", "select_two_bases", "reclaim_old_wal",
    "charge_and_create_epoch", "sync_epoch", "select_writer",
    "append_clr_effect_progress_descriptor", "append_terminal", "apply_complete_group",
    "verify_recovery", "stabilize_ready",
)
BOOT_ACTIONS = frozenset(BOOT_PHASES) | {"crash"}
BOOT_CLOSED_PATHS = BOOT_ACTIONS | {"base_only_restart", "crash_after_writer_selection"}
BOOT_BOUNDED_PATHS = frozenset(BOOT_PHASES[:16]) | {
    "crash", "base_only_restart", "two_crashes",
}
ACTION_SETS.update(bootstrap_closed=BOOT_ACTIONS, bootstrap_bounded=BOOT_ACTIONS)

GROUP_CASES = {(model, test): {} for model, meta in PROGRESS_MODELS.items()
               for test in meta["tests"]}


def register(model, test_index, case):
    group = GROUP_CASES[model, PROGRESS_MODELS[model]["tests"][test_index]]
    if case.key in group:
        raise AssertionError(f"duplicate fixed case: {case.key}")
    group[case.key] = case


for observation, key, generation, serializable in itertools.product(
        ("Immediate", "Waited", "Delete", "SameBytes", "Abort", "Own", "Move",
         "WrongRow", "WrongStatement"), range(3), range(2), ("false", "true")):
    paths = {"terminal"}
    if observation == "Waited":
        paths.add("wait_then_conflict")
    if observation in ("Abort", "Own", "Move"):
        paths.add("commit")
    register("M03", 0, Case(
        "observed_row_version",
        f"{observation}_key{key}_generation{generation}_serializable{serializable}",
        witnesses=words("validation_before_effect terminal index_agrees"),
        paths=frozenset(paths)))
for fixture, mutant in (("Immediate", "WaitOnly"), ("Move", "PhysicalOnly"),
                        ("SameBytes", "ValueOnly"), ("WrongRow", "IgnoreScope"),
                        ("WrongStatement", "IgnoreScope")):
    register("M03", 0, Case(
        "observed_row_version", fixture, mutant, "observation_decision",
        witnesses=words("counterexample_replayed")))
for first, second in itertools.combinations(range(8), 2):
    for reverse, generation in itertools.product(("false", "true"), (1, 2)):
        register("M03", 1, Case(
            "page_preparation", f"pages{first}_{second}_reverse{reverse}_generation{generation}",
            witnesses=words("apply new_page_cleanup_and_reprepare"),
            paths=words("apply reprepare shared_frame_overlap")))
for fixture, witness, path in (
        ("shared_reader_contention", "try_failed_cleanup_replayed", "contention_cleanup"),
        ("stale_incarnation", "rejected_before_latch", "stale_before_latch")):
    register("M03", 1, Case("page_preparation", fixture,
                            witnesses=words(witness), paths=words(path)))
for number, violation in enumerate(("partial_pin_wait", "latch_rank", "retry_cleanup"), 1):
    register("M03", 1, Case(
        "page_preparation", "pages0_1", f"mutant{number}", violation,
        witnesses=words("counterexample_replayed")))
for first, second in itertools.product(range(3), repeat=2):
    fixture = f"keys{first}_{second}"
    register("M03", 2, Case(
        "split_rollback", fixture,
        witnesses=words("split_retained other_ack_preserved logical_rollback"),
        paths=words("rollback_and_reader")))
    register("M03", 2, Case(
        "split_rollback", fixture, "stale_page_restore", "completed_split_undone",
        witnesses=words("counterexample_replayed")))
register("M03", 3, Case(
    "closed_progress", "two_writers_reader_flusher", graph="closed",
    witnesses=words("both_done prepare_allows_flusher drain_existing_reader no_fair_pending_scc"),
    paths=words("both_done prepare_dirty drain_reader")))
for number, size, start in ((1, 4, 9), (2, 2, 12)):
    register("M03", 3, Case(
        "closed_progress", f"fair_scc_size{size}_cycle_start{start}", f"mutant{number}",
        "weak_fair_noncompletion", graph="closed",
        witnesses=words("counterexample_replayed"), cycle_start=start))
for number, violation in ((3, "upper_lock_while_waiting_ticket"), (4, "fifo_overtaking")):
    register("M03", 3, Case(
        "closed_progress", "ticket", f"mutant{number}", violation,
        witnesses=words("counterexample_replayed")))

OPERATIONS = (
    (1, 2, 0, 2, 2, 1, 1, 1, 0, 0),
    (1, 3, 0, 2, 2, 1, 0, 1, 0, 0),
    (0, 1, 1, 1, 1, 0, 0, 1, 0, 0),
    (1, 2, 0, 2, 2, 1, 1, 1, 0, 8),
    (0, 0, 0, 1, 1, 0, 0, 1, 1, 0),
)
NOT_APPLICABLE_CASES = tuple(
    f"op{operation}_r{dimension}_d-1"
    for operation, request in enumerate(OPERATIONS)
    for dimension, demand in enumerate(request) if demand == 0)
PROGRESS_MODELS["M03"]["not_applicable_cases"] = ()
PROGRESS_MODELS["M06"]["not_applicable_cases"] = NOT_APPLICABLE_CASES
for operation, request in enumerate(OPERATIONS):
    for dimension, demand in enumerate(request):
        for delta in (-1, 0, 1):
            if delta == -1 and demand == 0:
                continue
            fixture = f"op{operation}_r{dimension}_d{delta}"
            register("M06", 0, Case("admission", fixture,
                                    witnesses=words("boundary_decision_replayed"), paths=1))
            if delta == -1:
                register("M06", 0, Case(
                    "admission", fixture, "ignore_dimension", "resource_admission",
                    witnesses=words("counterexample_replayed")))
register("M06", 0, Case(
    "token_partition", "flusher_progress_and_pages",
    witnesses=words("distinct_owned_tokens_replayed"), paths=1))
for mutant, violation in (("borrow_protected", "protected_partition_borrowed"),
                          ("duplicate_page_token", "duplicate_page_token")):
    register("M06", 0, Case(
        "token_partition", "flusher_progress_and_pages", mutant, violation,
        witnesses=words("counterexample_replayed")))
for fixture, key, generation in itertools.product(
        ("begin_pruned_active", "partial_clr", "cancel", "deadlock_victim",
         "statement_failure"), range(3), (1, 2)):
    register("M06", 1, Case(
        "runtime", f"{fixture}_key{key}_generation{generation}", graph="closed",
        witnesses=words("terminal two_crashes_restore_redo append_before_home_crash "
                        "witnesses_replayed"),
        paths=4 if fixture == "statement_failure" else 3, crash_bound=2))
for fixture, mutant, violation in (
        ("begin_pruned_active", "reset_descriptor", "remaining_obligation_mismatch"),
        ("begin_pruned_active", "reuse_clr_reservation", "remaining_obligation_mismatch"),
        ("partial_clr", "double_redo_charge", "remaining_obligation_mismatch"),
        ("statement_failure", "reset_statement_ledger", "retained_byte_charge"),
        ("statement_failure", "release_retained_lock", "retained_lock_lost")):
    register("M06", 1, Case(
        "runtime", fixture, mutant, violation,
        witnesses=words("shortest_counterexample_replayed"), crash_bound=2))
for fixture, release in itertools.product(
        ("old_dirty", "hot_redirty", "pinned_candidate", "dwb_inflight", "no_dirty"),
        ("false", "true")):
    flush_witness = fixture != "no_dirty" and (fixture != "pinned_candidate" or release == "true")
    register("M06", 2, Case(
        "pressure", f"{fixture}_release{release}", graph="closed",
        witnesses=words("terminal_replayed bounded_scan no_weak_fair_pending_scc"),
        paths=2 if flush_witness else 1))
for mutant in ("block_maintenance", "busy_noop", "confuse_q_lsn"):
    register("M06", 2, Case(
        "pressure", "old_dirty_reader_held", mutant,
        "retention_floor_lost" if mutant == "confuse_q_lsn" else "weak_fair_noncompletion",
        graph="closed", witnesses=words("counterexample_replayed"),
        initial_deadlock=mutant == "block_maintenance"))
for fixture, bounded in itertools.product(
        ("active_begin_pruned", "partial_clr", "both_slots_selected"), (True, False)):
    paths = BOOT_BOUNDED_PATHS if bounded else BOOT_CLOSED_PATHS
    register("M06", 3, Case(
        "bootstrap_bounded" if bounded else "bootstrap_closed", fixture,
        graph="bounded_depth16" if bounded else "closed", witnesses=paths, paths=paths,
        crash_bound=2 if bounded else None))
for mutant, fixture, violation, phases, extra in (
        ("accumulate_epoch", "active_begin_pruned", "bootstrap_peak", 25,
         {"base_only_restart", "crash_after_writer_selection"}),
        ("overwrite_selected_slot", "active_begin_pruned", "selected_recovery_closure_lost", 9, set()),
        ("reclaim_previous_wal", "active_begin_pruned", "selected_recovery_closure_lost", 11, set()),
        ("ignore_base_only", "partial_clr", "writer_continuation_lost", 16, set()),
        ("omit_epoch_charge", "active_begin_pruned", "unaccounted_epoch_charge", 17,
         {"base_only_restart"}),
        ("reset_descriptor", "active_begin_pruned", "persistent_descriptor_mismatch", 3, set()),
        ("double_redo_charge", "partial_clr", "remaining_obligation_mismatch", 4, set()),
        ("reuse_clr_reservation", "active_begin_pruned", "remaining_obligation_mismatch", 20,
         {"base_only_restart"})):
    paths = frozenset(BOOT_PHASES[:phases]) | {"crash"} | extra
    register("M06", 3, Case(
        "bootstrap_closed", fixture, mutant, violation, graph="closed",
        witnesses=paths, paths=paths, crash_bound=None))
for fixture in ("foreground_limit_lowered", "memory_short", "frame_short", "fd_short",
                "owned_short", "missing", "corrupt", "unsupported"):
    register("M06", 4, Case(
        "recovery_profile", fixture, witnesses=words("first_effect_or_rejection_replayed"),
        paths=1))
register("M06", 4, Case(
    "recovery_profile", "foreground_limit_lowered", "truncate_obligation_to_foreground",
    "obligation_truncated", witnesses=words("counterexample_replayed")))
for capacity in (15, 16, 17):
    register("M06", 5, Case(
        "growth", f"capacity{capacity}", graph="closed",
        witnesses=words("nonzero_growth_or_boundary_rejection_replayed"), paths=1, crash_bound=2))
for mutant in ("double_growth", "reset_growth", "commit_before_filemap"):
    register("M06", 5, Case(
        "growth", "capacity16", mutant,
        "reference_before_filemap_durable" if mutant == "commit_before_filemap"
        else "physical_growth_reconciliation",
        witnesses=words("shortest_counterexample_replayed"), crash_bound=2))

# Detect inventory edits that silently alter a denominator at import time.
for model, metadata in PROGRESS_MODELS.items():
    cases = [case for (name, _), group in GROUP_CASES.items() if name == model
             for case in group.values()]
    assert sum(case.mutant == "none" for case in cases) == metadata["normal"]
    assert sum(case.mutant != "none" for case in cases) == metadata["negative"]
    assert len(metadata["not_applicable_cases"]) == metadata["not_applicable"]


def _unique_object(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def _invalid_constant(value):
    raise ValueError(f"non-finite JSON value: {value}")


def _integer(value, low, high):
    return type(value) is int and low <= value <= high


def _trace(value, case, maximum):
    if not isinstance(value, list) or len(value) > maximum:
        return False
    if any(type(action) is not str or action not in ACTION_SETS[case.suite] for action in value):
        return False
    return case.crash_bound is None or value.count("crash") <= case.crash_bound


def _path_evidence(case, paths):
    """Check names/end actions/required cuts; semantic replay remains in Rust."""
    errors = []
    for name, path in paths.items():
        last = path[-1]
        action = last.split(":", 1)[-1]
        if case.suite.startswith("bootstrap_"):
            if name in BOOT_ACTIONS and last != name:
                errors.append(f"witness {name} does not end at its action")
            if name == "two_crashes" and path.count("crash") != 2:
                errors.append("two_crashes witness lacks exactly two crashes")
            if name == "base_only_restart" and (
                    last != "crash" or "select_new_base_old_previous" not in path):
                errors.append("base_only_restart witness lacks the selected base crash")
            if name == "crash_after_writer_selection" and (
                    len(path) < 2 or path[-2:] != ["select_writer", "crash"]):
                errors.append("writer-selection crash witness lacks its adjacent cut")
        elif case.suite == "observed_row_version":
            endings = {"terminal": {"apply", "rollback_all_prior_own_writes"},
                       "wait_then_conflict": {"rollback_all_prior_own_writes"},
                       "commit": {"commit"}}
            if action not in endings[name] or "0:validate" not in path:
                errors.append(f"witness {name} lacks validation/terminal action")
            if name == "wait_then_conflict" and "0:lock_attempt_wait" not in path:
                errors.append("wait_then_conflict witness lacks wait")
        elif case.suite == "page_preparation":
            endings = {"apply": "release_all", "reprepare": "new_page_cleanup_and_reprepare",
                       "shared_frame_overlap": "reserve_all",
                       "contention_cleanup": "try_failed_cleanup",
                       "stale_before_latch": "reject_stale_incarnation"}
            if action != endings[name]:
                errors.append(f"witness {name} has the wrong final action")
        elif case.suite == "split_rollback":
            if not {"0:logical_rollback", "1:other_writer_commit", "2:reader_check"} <= set(path):
                errors.append("rollback witness omits another ACK or reader")
        elif case.suite == "closed_progress":
            endings = {"both_done": "apply_and_release_turn",
                       "prepare_dirty": "head_prepare",
                       "drain_reader": "close_content_and_drain"}
            if action != endings[name]:
                errors.append(f"witness {name} has the wrong final action")
            if name == "both_done" and not {
                    "0:apply_and_release_turn", "1:apply_and_release_turn"} <= set(path):
                errors.append("both_done witness omits a writer")
        elif case.suite == "admission" and path != ["admit_before_effect"]:
            errors.append("admission witness must be the single decision")
        elif case.suite == "token_partition" and path != ["assign_owned_tokens"]:
            errors.append("token witness must be the single assignment")
        elif case.suite == "recovery_profile":
            expected = (["restore_original_obligation", "first_recovery_write"]
                        if case.fixture == "foreground_limit_lowered"
                        else ["reject_before_write"])
            if path != expected:
                errors.append("profile witness has the wrong rejection/write path")
        elif case.suite == "growth":
            if case.fixture == "capacity15":
                if path != ["admission_reject"]:
                    errors.append("growth boundary did not reject before effect")
            elif path.count("crash") != 2 or "commit_reference_to_extent" not in path:
                errors.append("growth witness lacks two crashes and committed reference")
    traces = list(paths.values())
    if case.suite == "runtime" and case.mutant == "none":
        # Statement failure begins with writer 1 already inactive; its fixed
        # history has no new terminal obligation. Other runtime fixtures do.
        terminals = {"terminal_0"} if case.fixture.startswith("statement_failure") else {
            "terminal_0", "terminal_1"}
        if not any(path[-1] in ("terminal_0", "terminal_1", "home_apply", "redo_home")
                   and terminals <= set(path) for path in traces):
            errors.append("runtime witnesses omit required terminal records")
        if not any(path.count("crash") == 2 and path[-1] == "redo_home" for path in traces):
            errors.append("runtime witnesses omit two-crash redo completion")
        # partial_clr starts with a durable CLR whose home effect is still absent.
        if not any(path[-1] == "crash"
                   and (case.fixture.startswith("partial_clr")
                        or any(action.startswith(("forward_append_", "clr_append_")) for action in path))
                   for path in traces):
            errors.append("runtime witnesses omit append-before-home crash")
        if case.fixture.startswith("statement_failure") and not any(
                path[-1] == "admission_refused_retained_bytes" for path in traces):
            errors.append("statement witness omits retained-byte refusal")
    if case.suite == "pressure" and case.mutant == "none":
        end = ("checkpoint_reclaim_reopen" if case.fixture.endswith("releasetrue")
               else "quiescent_pressure_refusal")
        if not any(path[-1] == end for path in traces):
            errors.append("pressure witnesses omit their required terminal decision")
        if case.paths == 2 and not any(path[-1] == "flush_complete" for path in traces):
            errors.append("pressure witnesses omit completed flush")
    return errors


def _check_case(model, row, case):
    errors = []
    required = {
        "suite", "fixture", "mutant", "status", "states", "transitions", "max_depth",
        "elapsed_ms", "unexplored", "witnesses", "counterexample", "violation",
    }
    if model == "M06" or case.mutant == "none":
        required.add("witness_paths")
    if case.suite.startswith("bootstrap_"):
        required.add("wal_peak")
    if set(row) != required:
        errors.append("receipt fields do not match the fixed schema")
    normal = case.mutant == "none"
    status = "PASS" if normal else "EXPECTED_COUNTEREXAMPLE"
    if row.get("status") != status or row.get("violation") != case.violation:
        errors.append("wrong status or expected violation")
    limits = {"states": (1, STATE_CAP), "transitions": (0, TRANSITION_CAP),
              "max_depth": (0, TRANSITION_CAP), "elapsed_ms": (0, TIME_LIMIT_MS - 1),
              "unexplored": (0, STATE_CAP)}
    valid_numbers = all(_integer(row.get(name), *limit) for name, limit in limits.items())
    if not valid_numbers:
        errors.append("invalid integer, exhausted budget, or timeout")
    if case.suite.startswith("bootstrap_") and (
            type(row.get("wal_peak")) is not int or row["wal_peak"] != 24):
        errors.append("bootstrap WAL peak must be 24")
    witness = row.get("witnesses")
    if (not isinstance(witness, list) or any(type(item) is not str for item in witness)
            or len(witness) != len(set(witness)) or set(witness) != case.witnesses):
        errors.append("missing, duplicate, or unknown witnesses")
    if not valid_numbers:
        return errors
    depth, states, transitions = row["max_depth"], row["states"], row["transitions"]
    if case.graph == "bounded_depth16" and depth > 16:
        errors.append("bounded safety graph exceeds depth 16")
    if depth > transitions:
        errors.append("depth exceeds the explored transition count")
    if states > transitions + 1:
        errors.append("state count exceeds initial state plus explored transitions")
    if case.cycle_start is None and depth > states:
        errors.append("shortest path depth exceeds the retained state bound")
    if row["unexplored"] > states:
        errors.append("unexplored count exceeds retained states")
    closed_counterexample = case.violation == "weak_fair_noncompletion"
    if normal or closed_counterexample:
        if row["unexplored"] != 0:
            errors.append("normal/closed-progress receipt has unexplored work")
    elif row["unexplored"] == 0:
        errors.append("safety counterexample must retain its unexplored frontier")
    counterexample = row.get("counterexample")
    if not _trace(counterexample, case, depth):
        errors.append("invalid counterexample action, crash bound, or depth")
    elif normal:
        if counterexample:
            errors.append("normal receipt contains a counterexample")
    elif case.initial_deadlock:
        if counterexample or (states, transitions, depth) != (1, 0, 0):
            errors.append("initial deadlock must be exactly one state and no transitions")
    else:
        if not counterexample or transitions == 0:
            errors.append("expected counterexample is empty")
        if model == "M03" and len(counterexample) != depth:
            errors.append("M03 counterexample length differs from reported depth")
        if case.cycle_start is not None and not case.cycle_start < len(counterexample):
            errors.append("fair SCC trace lacks its nonempty cycle suffix")
    if not case.initial_deadlock and (transitions == 0 or depth == 0):
        errors.append("only the declared initial deadlock permits zero exploration")
    if "witness_paths" in required:
        paths = row.get("witness_paths")
        if not isinstance(paths, dict):
            errors.append("witness_paths must be an object")
        else:
            if type(case.paths) is int:
                if len(paths) != case.paths or any(
                        re.fullmatch(r"state(?:0|[1-9][0-9]*)", name) is None
                        or int(name[5:]) >= states for name in paths):
                    errors.append("missing/unknown state witness path or invalid state ID")
            elif set(paths) != case.paths:
                errors.append("missing or unknown named witness paths")
            paths_valid = all(path and _trace(path, case, depth)
                              and len(path) <= states for path in paths.values())
            if not paths_valid:
                errors.append("empty/invalid witness path, action, crash bound, or depth")
            # Only call semantic-shape checks after names and actions are valid.
            if paths_valid and ((type(case.paths) is int and len(paths) == case.paths)
                                or set(paths) == case.paths):
                errors.extend(_path_evidence(case, paths))
    return errors


def _test_envelope(metadata, test_name, output, exit_code):
    errors = []
    if type(exit_code) is not int or exit_code != 0:
        errors.append("test process did not exit successfully")
    lines = output.splitlines()
    if lines.count("running 1 test") != 1:
        errors.append("expected exactly one named libtest run")
    test_lines = [line for line in lines if line.startswith("test ") and not line.startswith("test result:")]
    if test_lines != [f"test {test_name} ... ok"]:
        errors.append("expected only the exact completed test")
    summaries = [line for line in lines if line.startswith("test result:")]
    filtered = len(metadata["tests"]) + len(metadata["budget_tests"]) - 1
    pattern = (rf"test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; "
               rf"{filtered} filtered out; finished in [0-9]+(?:\.[0-9]+)?s")
    if len(summaries) != 1 or re.fullmatch(pattern, summaries[0]) is None:
        errors.append("expected exactly one successful one-test summary")
    return errors


def check_group_output(model_id, test_name, output, exit_code):
    """Validate one ``cargo test --test TARGET NAME -- --exact --nocapture`` log.

    No I/O or subprocess is performed. All errors produce ``pass=False``; every
    decoded case (including duplicates/unknowns) is retained with its own errors.
    A budget test accepts no case receipts. N/A entries are declarations, never
    successful executions. Caller must remove inherited RUST_TEST_THREADS and
    use libtest's default scheduler, so receipts start on their own lines.
    """
    result = {"model": model_id, "test": test_name, "pass": False, "errors": [],
              "cases": [], "counts": {"normal": 0, "negative": 0},
              "not_applicable": []}
    errors = result["errors"]
    metadata = PROGRESS_MODELS.get(model_id) if isinstance(model_id, str) else None
    if metadata is None or test_name not in metadata["tests"] + metadata["budget_tests"]:
        errors.append("unregistered model or exact test")
        return result
    if type(output) is not str:
        errors.append("output must be text")
        return result
    errors.extend(_test_envelope(metadata, test_name, output, exit_code))
    expected = GROUP_CASES.get((model_id, test_name), {})
    seen, summaries = set(), []
    for line in output.splitlines():
        is_case = line.startswith(metadata["prefix"])
        is_summary = model_id == "M03" and line.startswith(SUMMARY_PREFIX)
        if not is_case and not is_summary:
            if "XADB_DEV3_" in line:
                errors.append("unknown or non-line-aligned model receipt")
            continue
        prefix = metadata["prefix"] if is_case else SUMMARY_PREFIX
        try:
            row = json.loads(line[len(prefix):], object_pairs_hook=_unique_object,
                             parse_constant=_invalid_constant)
            if not isinstance(row, dict):
                raise ValueError("receipt must be a JSON object")
        except (ValueError, RecursionError) as exc:
            errors.append(f"invalid receipt JSON: {exc}")
            continue
        if is_summary:
            summaries.append(row)
            continue
        identity = tuple(row.get(field) for field in ("suite", "fixture", "mutant"))
        case_errors = []
        if any(type(field) is not str for field in identity):
            case_errors.append("case identity fields must be strings")
        elif identity not in expected:
            case_errors.append("unknown case for the selected test")
        else:
            if identity in seen:
                case_errors.append("duplicate case")
            seen.add(identity)
            case_errors.extend(_check_case(model_id, row, expected[identity]))
        result["cases"].append({"receipt": row, "pass": not case_errors, "errors": case_errors})
        if case_errors:
            errors.append(f"case {identity}: {'; '.join(case_errors)}")
    missing = set(expected) - seen
    if missing:
        errors.append(f"missing cases: {sorted(missing)}")
    result["counts"] = {
        "normal": sum(key[2] == "none" for key in seen),
        "negative": sum(key[2] != "none" for key in seen),
    }
    suites = sorted({case.suite for case in expected.values()})
    if model_id == "M03":
        if len(summaries) != len(suites):
            errors.append("missing or duplicate suite summary")
        summary_suites = set()
        for summary in summaries:
            suite = summary.get("suite")
            if type(suite) is not str or suite not in suites or suite in summary_suites:
                errors.append("unknown or duplicate suite summary")
                continue
            summary_suites.add(suite)
            fields = {"suite", "status", "cases", "negative_cases", "states", "transitions", "unexplored"}
            numeric = ("cases", "negative_cases", "states", "transitions", "unexplored")
            if set(summary) != fields or any(type(summary.get(name)) is not int for name in numeric):
                errors.append("invalid suite summary schema/integer")
                continue
            normal = [item["receipt"] for item in result["cases"]
                      if item["pass"] and item["receipt"]["suite"] == suite
                      and item["receipt"]["mutant"] == "none"]
            fixed = [case for case in expected.values() if case.suite == suite]
            if (summary["status"] != "PASS" or summary["unexplored"] != 0
                    or summary["cases"] != sum(case.mutant == "none" for case in fixed)
                    or summary["negative_cases"] != sum(case.mutant != "none" for case in fixed)
                    or summary["states"] != sum(row["states"] for row in normal)
                    or summary["transitions"] != sum(row["transitions"] for row in normal)):
                errors.append("suite summary disagrees with fixed denominator or valid cases")
    if model_id == "M06" and test_name == metadata["tests"][0]:
        result["not_applicable"] = [
            {"suite": "admission", "fixture": fixture, "status": "N_A_ZERO_DEMAND",
             "evidence": "fixed_declaration_not_executed"}
            for fixture in NOT_APPLICABLE_CASES]
    result["pass"] = not errors
    return result
