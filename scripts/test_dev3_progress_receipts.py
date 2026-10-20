"""Receipt-parser mutation tests; synthetic logs are not model execution evidence.

Positive fixtures exercise the fixed inventory and structural schema. They do not
prove model semantics. Each negative test corrupts that fixture at a meaningful
acceptance boundary, independently of executing any Rust model or reading saved
artifacts. The clean runner separately retains actual Rust execution evidence.
"""

import copy
import json
import unittest

import dev3_progress_receipts as checker


def example_paths(case):
    """Small schema fixtures with the required action cuts, not replay claims."""
    if type(case.paths) is not int:
        result = {}
        for name in sorted(case.paths):
            if case.suite.startswith("bootstrap_"):
                if name == "base_only_restart":
                    path = ["select_new_base_old_previous", "crash"]
                elif name == "two_crashes":
                    path = ["crash", "crash"]
                elif name == "crash_after_writer_selection":
                    path = ["select_writer", "crash"]
                else:
                    path = [name]
            else:
                path = {
                    "terminal": ["0:validate", "0:apply"],
                    "wait_then_conflict": ["0:lock_attempt_wait", "0:validate",
                                           "0:rollback_all_prior_own_writes"],
                    "commit": ["0:validate", "0:commit"],
                    "apply": ["0:apply", "0:release_all"],
                    "reprepare": ["0:new_page_cleanup_and_reprepare"],
                    "shared_frame_overlap": ["0:reserve_all", "1:reserve_all"],
                    "contention_cleanup": ["0:try_failed_cleanup"],
                    "stale_before_latch": ["0:reject_stale_incarnation"],
                    "rollback_and_reader": ["1:other_writer_commit", "0:logical_rollback",
                                            "2:reader_check"],
                    "both_done": ["0:apply_and_release_turn", "1:apply_and_release_turn"],
                    "prepare_dirty": ["0:head_prepare"],
                    "drain_reader": ["0:close_content_and_drain"],
                }[name]
            result[name] = path
        return result
    if case.suite == "runtime":
        paths = [
            ["terminal_0", "terminal_1", "home_apply"],
            ["crash", "boot_reserve", "restore_reserve", "redo_home",
             "crash", "boot_reserve", "restore_reserve", "redo_home"],
            ["forward_append_0", "crash"],
        ]
        if case.paths == 4:
            paths.append(["admission_refused_retained_bytes"])
    elif case.suite == "pressure":
        terminal = ("checkpoint_reclaim_reopen" if case.fixture.endswith("releasetrue")
                    else "quiescent_pressure_refusal")
        paths = [[terminal]]
        if case.paths == 2:
            paths.append(["flush_complete"])
    elif case.suite == "recovery_profile":
        paths = [["restore_original_obligation", "first_recovery_write"]
                 if case.fixture == "foreground_limit_lowered" else ["reject_before_write"]]
    elif case.suite == "growth":
        paths = [["admission_reject"] if case.fixture == "capacity15"
                 else ["crash", "crash", "commit_reference_to_extent"]]
    else:
        paths = [[next(iter(checker.ACTION_SETS[case.suite]))]]
    return {f"state{index + 1}": path for index, path in enumerate(paths)}


def example_row(case, model):
    row = {
        "suite": case.suite, "fixture": case.fixture, "mutant": case.mutant,
        "status": "PASS" if case.mutant == "none" else "EXPECTED_COUNTEREXAMPLE",
        "states": 1000, "transitions": 2000, "max_depth": 16,
        "elapsed_ms": 1, "unexplored": 0 if case.mutant == "none"
        or case.violation == "weak_fair_noncompletion" else 1,
        "witnesses": sorted(case.witnesses), "counterexample": [],
        "violation": case.violation,
    }
    if model == "M06" or case.mutant == "none":
        row["witness_paths"] = example_paths(case)
    if case.suite.startswith("bootstrap_"):
        row["wal_peak"] = 24
    if case.mutant != "none":
        row["counterexample"] = [sorted(checker.ACTION_SETS[case.suite])[0]]
        if case.cycle_start is not None:
            row["counterexample"] *= case.cycle_start + 2
        row["max_depth"] = len(row["counterexample"])
        # Bootstrap records successful action witnesses prior to the failure;
        # max_depth covers both those paths and the final violating path.
        if case.suite.startswith("bootstrap_"):
            row["max_depth"] = max(3, row["max_depth"])
        if case.initial_deadlock:
            row.update(states=1, transitions=0, max_depth=0, counterexample=[])
    return row


def fixture(model, test_index=0):
    test = checker.PROGRESS_MODELS[model]["tests"][test_index]
    return test, [example_row(case, model)
                  for case in checker.GROUP_CASES[model, test].values()]


def log(model, test, rows=(), summaries=None):
    metadata = checker.PROGRESS_MODELS[model]
    lines = ["running 1 test"]
    lines += [metadata["prefix"] + json.dumps(row) for row in rows]
    if model == "M03" and summaries is None:
        summaries = []
        suites = sorted({row["suite"] for row in rows})
        for suite in suites:
            normal = [row for row in rows if row["suite"] == suite and row["mutant"] == "none"]
            summaries.append({
                "suite": suite, "status": "PASS", "cases": len(normal),
                "negative_cases": sum(row["suite"] == suite and row["mutant"] != "none"
                                      for row in rows),
                "states": sum(row["states"] for row in normal),
                "transitions": sum(row["transitions"] for row in normal), "unexplored": 0,
            })
    lines += [checker.SUMMARY_PREFIX + json.dumps(row) for row in (summaries or [])]
    lines += [f"test {test} ... ok",
              "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "
              f"{len(metadata['tests']) + len(metadata['budget_tests']) - 1} "
              "filtered out; finished in 0.01s"]
    return "\n".join(lines) + "\n"


class ReceiptChecks(unittest.TestCase):
    def assert_rejected(self, model, test, output, exit_code=0, phrase=None):
        result = checker.check_group_output(model, test, output, exit_code)
        self.assertIs(result["pass"], False, result)
        self.assertTrue(result["errors"])
        if phrase:
            self.assertIn(phrase, "\n".join(result["errors"]))
        json.dumps(result)  # Even malformed receipt diagnostics remain serializable.
        return result

    def test_01_inventory_denominators_and_test_names(self):
        expected = {
            "M03": {"observed_row_version": (108, 5), "page_preparation": (114, 3),
                    "split_rollback": (9, 9), "closed_progress": (1, 4)},
            "M06": {"admission": (130, 30), "token_partition": (1, 2),
                    "runtime": (30, 5), "pressure": (10, 3),
                    "bootstrap_bounded": (3, 0), "bootstrap_closed": (3, 8),
                    "recovery_profile": (8, 1), "growth": (3, 3)},
        }
        for model, suites in expected.items():
            cases = [case for (name, _), group in checker.GROUP_CASES.items() if name == model
                     for case in group.values()]
            self.assertEqual(set(suites), {case.suite for case in cases})
            for suite, counts in suites.items():
                selected = [case for case in cases if case.suite == suite]
                self.assertEqual(counts, (sum(case.mutant == "none" for case in selected),
                                          sum(case.mutant != "none" for case in selected)))
            meta = checker.PROGRESS_MODELS[model]
            self.assertEqual(len(meta["tests"]), 4 if model == "M03" else 6)
            self.assertEqual(len(meta["budget_tests"]), 1)
            self.assertFalse(set(meta["tests"]) & set(meta["budget_tests"]))
        names = unittest.defaultTestLoader.getTestCaseNames(type(self))
        self.assertEqual(len(names), checker.CHECKER_TEST_COUNT)

    def test_02_complete_synthetic_groups_and_budget_tests(self):
        for model, meta in checker.PROGRESS_MODELS.items():
            for index, test in enumerate(meta["tests"]):
                with self.subTest(model=model, test=test):
                    _, rows = fixture(model, index)
                    result = checker.check_group_output(model, test, log(model, test, rows), 0)
                    self.assertTrue(result["pass"], result["errors"])
                    self.assertTrue(all(case["pass"] for case in result["cases"]))
            for test in meta["budget_tests"]:
                self.assertTrue(checker.check_group_output(model, test, log(model, test), 0)["pass"])

    def test_03_process_exit_exact_test_and_one_passed_summary(self):
        test, rows = fixture("M03", 2)
        output = log("M03", test, rows)
        for code in (1, -9, True, None):
            self.assert_rejected("M03", test, output, code, "exit")
        for changed in (
                output.replace(f"test {test} ... ok", "test unrelated ... ok"),
                output.replace("1 passed", "0 passed"),
                output.replace("0 failed", "1 failed"),
                output.replace("0 ignored", "1 ignored"),
                output.replace("4 filtered out", "0 filtered out"),
                output + "test unrelated ... ok\n",
                output + output.splitlines()[-1] + "\n",
                output.replace("running 1 test", "running 0 tests")):
            self.assert_rejected("M03", test, changed)

    def test_04_missing_duplicate_unknown_and_wrong_group_cases(self):
        test, rows = fixture("M06", 4)
        for changed in (rows[1:], rows + [rows[0]],
                        rows + [dict(rows[0], fixture="unregistered")]):
            self.assert_rejected("M06", test, log("M06", test, changed))
        other = fixture("M06", 5)[1][0]
        self.assert_rejected("M06", test, log("M06", test, rows + [other]), phrase="unknown case")

    def test_05_duplicate_json_members_at_all_levels(self):
        test, rows = fixture("M06", 4)
        output = log("M06", test, rows)
        for old, new in (
                ('"states": 1000', '"states": 1, "states": 1000'),
                ('"state1": [', '"state1": [], "state1": ['),
                ('"status": "PASS"', '"status": "FAIL", "status": "PASS"')):
            self.assert_rejected("M06", test, output.replace(old, new, 1), phrase="duplicate JSON key")

    def test_06_malformed_json_and_nonfinite_values(self):
        test, rows = fixture("M06", 4)
        output = log("M06", test, rows)
        for value in ("NaN", "Infinity", "-Infinity", '"1"', "[1]", "{}"):
            self.assert_rejected("M06", test, output.replace('"elapsed_ms": 1',
                                                            f'"elapsed_ms": {value}', 1))
        for payload in ("[]", "null", "{broken", '{"suite":"x"} trailing'):
            self.assert_rejected("M06", test, output + checker.PROGRESS_MODELS["M06"]["prefix"]
                                 + payload + "\n")

    def test_07_boolean_and_fractional_integers(self):
        test, rows = fixture("M06", 3)
        for field in ("states", "transitions", "max_depth", "elapsed_ms", "unexplored", "wal_peak"):
            for value in (True, False, 1.5, "1", None):
                with self.subTest(field=field, value=value):
                    altered = copy.deepcopy(rows)
                    altered[0][field] = value
                    self.assert_rejected("M06", test, log("M06", test, altered))

    def test_08_caps_and_timeout_are_exclusive_of_inconclusive(self):
        test, rows = fixture("M06", 4)
        for field, values in (
                ("states", (0, -1, 1_000_001)), ("transitions", (-1, 8_000_001)),
                ("elapsed_ms", (-1, 60_000, 60_001)), ("unexplored", (1, 1_000_001))):
            for value in values:
                changed = copy.deepcopy(rows)
                changed[0][field] = value
                self.assert_rejected("M06", test, log("M06", test, changed))
        changed = copy.deepcopy(rows)
        changed[0].update(states=1_000_000, transitions=8_000_000, elapsed_ms=59_999)
        self.assertTrue(checker.check_group_output("M06", test, log("M06", test, changed), 0)["pass"])

    def test_09_status_violation_and_unknown_fields(self):
        test, rows = fixture("M06", 4)
        for patch in ({"status": "INCONCLUSIVE"}, {"status": "EXPECTED_COUNTEREXAMPLE"},
                      {"violation": "INCONCLUSIVE_states"}, {"unexpected": True}):
            changed = copy.deepcopy(rows)
            changed[0].update(patch)
            self.assert_rejected("M06", test, log("M06", test, changed))
        for violation in (None, "wrong_failure", "INCONCLUSIVE_time"):
            changed = copy.deepcopy(rows)
            changed[-1]["violation"] = violation
            self.assert_rejected("M06", test, log("M06", test, changed))

    def test_10_witness_missing_unknown_duplicate_and_wrong_type(self):
        test, rows = fixture("M03", 2)
        for witness in ([], ["invented"], rows[0]["witnesses"] * 2, [True], {}, None):
            changed = copy.deepcopy(rows)
            changed[0]["witnesses"] = witness
            self.assert_rejected("M03", test, log("M03", test, changed), phrase="witnesses")

    def test_11_named_path_missing_unknown_and_empty(self):
        test, rows = fixture("M03", 2)
        for paths in ({}, {"invented": ["0:logical_rollback"]},
                      {"rollback_and_reader": []}, []):
            changed = copy.deepcopy(rows)
            changed[0]["witness_paths"] = paths
            self.assert_rejected("M03", test, log("M03", test, changed), phrase="path")

    def test_12_state_path_ids_count_and_types(self):
        test, rows = fixture("M06", 4)
        for paths in (
                {}, {"state1000": ["reject_before_write"]}, {"state01": ["reject_before_write"]},
                {"terminal": ["reject_before_write"]}, {"state1": "reject_before_write"},
                {"state1": [], "state2": []}):
            changed = copy.deepcopy(rows)
            changed[0]["witness_paths"] = paths
            self.assert_rejected("M06", test, log("M06", test, changed))

    def test_13_unknown_actions_and_wrong_actor_are_rejected(self):
        test, rows = fixture("M03", 2)
        for action in ("0:reader_check", "2:logical_rollback", "9:reader_check",
                       "0:made_up", "reader_check", True):
            changed = copy.deepcopy(rows)
            changed[0]["witness_paths"]["rollback_and_reader"][0] = action
            self.assert_rejected("M03", test, log("M03", test, changed))
        test, rows = fixture("M06", 5)
        rows[-1]["counterexample"] = ["unregistered_crash_cut"]
        self.assert_rejected("M06", test, log("M06", test, rows))

    def test_14_normal_path_required_cuts_and_final_action(self):
        for model, index, path, replacement in (
                ("M03", 0, "terminal", ["0:apply"]),
                ("M03", 1, "apply", ["0:apply"]),
                ("M03", 2, "rollback_and_reader", ["0:logical_rollback"]),
                ("M03", 3, "both_done", ["0:apply_and_release_turn"]),
                ("M06", 4, "state1", ["first_recovery_write"])):
            test, rows = fixture(model, index)
            rows[0]["witness_paths"][path] = replacement
            self.assert_rejected(model, test, log(model, test, rows))

    def test_15_bound16_and_closed_depth_are_distinct(self):
        test, rows = fixture("M03", 2)
        rows[0]["max_depth"] = 17
        self.assert_rejected("M03", test, log("M03", test, rows), phrase="depth 16")
        test, rows = fixture("M03", 3)
        rows[0]["max_depth"] = 91
        self.assertTrue(checker.check_group_output("M03", test, log("M03", test, rows), 0)["pass"])
        test, rows = fixture("M06", 3)
        bounded = next(row for row in rows if row["suite"] == "bootstrap_bounded")
        bounded["max_depth"] = 17
        self.assert_rejected("M06", test, log("M06", test, rows), phrase="depth 16")

    def test_16_crash_bound_and_two_crash_witness(self):
        test, rows = fixture("M06", 3)
        bounded = next(row for row in rows if row["suite"] == "bootstrap_bounded")
        bounded["witness_paths"]["two_crashes"] = ["crash"]
        self.assert_rejected("M06", test, log("M06", test, rows), phrase="two_crashes")
        bounded["witness_paths"]["two_crashes"] = ["crash"] * 3
        self.assert_rejected("M06", test, log("M06", test, rows), phrase="crash bound")

    def test_17_negative_trace_frontier_and_exact_m03_length(self):
        test, rows = fixture("M03", 2)
        index = next(i for i, row in enumerate(rows) if row["mutant"] != "none")
        for patch in ({"counterexample": []}, {"unexplored": 0},
                      {"max_depth": 2}, {"max_depth": 0},
                      {"counterexample": ["0:logical_rollback"] * 17, "max_depth": 17}):
            changed = copy.deepcopy(rows)
            changed[index].update(patch)
            self.assert_rejected("M03", test, log("M03", test, changed))

    def test_18_initial_deadlock_exception_is_one_exact_case(self):
        test, rows = fixture("M06", 2)
        deadlock = next(row for row in rows if row["mutant"] == "block_maintenance")
        self.assertEqual((deadlock["states"], deadlock["transitions"], deadlock["max_depth"]), (1, 0, 0))
        for field, value in (("states", 2), ("transitions", 1),
                             ("counterexample", ["maintenance_noop"])):
            changed = copy.deepcopy(rows)
            row = next(row for row in changed if row["mutant"] == "block_maintenance")
            row[field] = value
            self.assert_rejected("M06", test, log("M06", test, changed))
        other = next(row for row in rows if row["mutant"] == "busy_noop")
        other.update(states=1, transitions=0, max_depth=0, counterexample=[])
        self.assert_rejected("M06", test, log("M06", test, rows), phrase="empty")

    def test_19_fair_cycle_prefix_replay_flag_and_closed_frontier(self):
        test, rows = fixture("M03", 3)
        index = next(i for i, row in enumerate(rows) if row["mutant"] == "mutant1")
        for patch in (
                {"counterexample": ["0:failed_try"] * 9, "max_depth": 9},
                {"fixture": "fair_scc_size4_cycle_start999"},
                {"witnesses": []}, {"unexplored": 1}):
            changed = copy.deepcopy(rows)
            changed[index].update(patch)
            self.assert_rejected("M03", test, log("M03", test, changed))

    def test_20_bootstrap_peak_path_inventory_and_adjacent_writer_cut(self):
        test, rows = fixture("M06", 3)
        for peak in (16, 23, 25, True):
            changed = copy.deepcopy(rows)
            changed[0]["wal_peak"] = peak
            self.assert_rejected("M06", test, log("M06", test, changed), phrase="peak")
        closed = next(row for row in rows if row["suite"] == "bootstrap_closed"
                      and row["mutant"] == "none")
        closed["witness_paths"]["crash_after_writer_selection"] = ["select_writer", "boot_reserve", "crash"]
        self.assert_rejected("M06", test, log("M06", test, rows), phrase="adjacent")
        test, rows = fixture("M06", 3)
        mutant = next(row for row in rows if row["mutant"] == "reset_descriptor")
        self.assertEqual(mutant["violation"], "persistent_descriptor_mismatch")
        mutant["witness_paths"].pop("analysis_complete_prefix")
        self.assert_rejected("M06", test, log("M06", test, rows), phrase="missing")

    def test_21_summaries_exact_denominator_totals_duplicate_and_json_types(self):
        test, rows = fixture("M03", 2)
        output = log("M03", test, rows)
        summary_line = next(line for line in output.splitlines() if line.startswith(checker.SUMMARY_PREFIX))
        for old, new in (('"cases": 9', '"cases": 8'),
                         ('"negative_cases": 9', '"negative_cases": true'),
                         ('"states": 9000', '"states": 9001'),
                         ('"transitions": 18000', '"transitions": 1'),
                         ('"unexplored": 0', '"unexplored": 1'),
                         ('"status": "PASS"', '"status": "FAIL"')):
            altered = summary_line.replace(old, new)
            self.assertNotEqual(altered, summary_line)
            self.assert_rejected("M03", test, output.replace(summary_line, altered), phrase="summary")
        self.assert_rejected("M03", test, output.replace(summary_line, ""), phrase="summary")
        self.assert_rejected("M03", test, output + summary_line + "\n", phrase="summary")

    def test_22_na20_are_declarations_and_cannot_be_receipts(self):
        expected = {
            0: (2, 8, 9), 1: (2, 6, 8, 9), 2: (0, 5, 6, 8, 9),
            3: (2, 8), 4: (0, 1, 2, 5, 6, 9),
        }
        names = {f"op{operation}_r{dimension}_d-1"
                 for operation, dimensions in expected.items() for dimension in dimensions}
        self.assertEqual(names, set(checker.NOT_APPLICABLE_CASES))
        self.assertEqual(len(names), 20)
        test, rows = fixture("M06", 0)
        result = checker.check_group_output("M06", test, log("M06", test, rows), 0)
        self.assertTrue(result["pass"])
        self.assertEqual(result["counts"], {"normal": 131, "negative": 32})
        self.assertEqual({row["fixture"] for row in result["not_applicable"]}, names)
        self.assertTrue(all(row["evidence"] == "fixed_declaration_not_executed"
                            for row in result["not_applicable"]))
        for status in ("PASS", "N_A", "N_A_ZERO_DEMAND"):
            extra = dict(rows[0], fixture=sorted(names)[0], status=status)
            self.assert_rejected("M06", test, log("M06", test, rows + [extra]), phrase="unknown")

    def test_23_unknown_prefix_inline_receipt_and_budget_receipts(self):
        test, rows = fixture("M06", 4)
        output = log("M06", test, rows)
        for changed in (
                output + 'XADB_DEV3_M99 {}\n',
                output.replace("XADB_DEV3_M06 ", "XADB_DEV3_M03 ", 1),
                output.replace("XADB_DEV3_M06 ", f"test {test} ... XADB_DEV3_M06 ", 1)):
            self.assert_rejected("M06", test, changed)
        budget = checker.PROGRESS_MODELS["M06"]["budget_tests"][0]
        self.assert_rejected("M06", budget, log("M06", budget, rows[:1]), phrase="unknown case")

    def test_24_invalid_api_input_and_corrupt_identity_are_errors(self):
        test, rows = fixture("M06", 4)
        for model, name, output in (("M99", test, ""), ("M06", "unknown", ""),
                                    ("M06", test, None)):
            self.assert_rejected(model, name, output)
        for field in ("suite", "fixture", "mutant"):
            for value in (True, [], {}):
                changed = copy.deepcopy(rows)
                changed[0][field] = value
                # Do not synthesize M03 summaries from deliberately malformed IDs.
                self.assert_rejected("M06", test, log("M06", test, changed), phrase="identity")


if __name__ == "__main__":
    unittest.main()
