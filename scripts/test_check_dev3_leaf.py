"""Fail-closed receipt checks; synthetic output is never product execution."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

from check_dev3_leaf import (
    ANCHOR_TESTS, KEY_TESTS, MODEL_TEST, NEGATIVE_CASES, NORMAL_CASES, PAGE_TESTS, REQUIRED_WITNESSES,
    OVERFLOW_TESTS, SCALAR_KEY_TESTS, TRANSACTION_TESTS, TREE_TESTS, UNDO_REF_TESTS,
    VECTOR_RESULT, VIOLATIONS, check_model_output, check_named_tests,
    check_page_output, check_vectors, run_logged,
)


def case(fixture, mutant=None):
    return {
        'scope': 'D3-M02-P01', 'fixture': fixture, 'mutant': mutant,
        'status': 'PASS' if mutant is None else 'EXPECTED_COUNTEREXAMPLE',
        'states': 10, 'transitions': 20, 'max_depth': 16, 'bound_depth': 16, 'elapsed_ms': 1,
        'unexplored': 0, 'witnesses': {name: ['q_read'] for name in REQUIRED_WITNESSES[fixture]},
        'counterexample': [] if mutant is None else ['q_read'], 'error': None,
        'violation': None if mutant is None else VIOLATIONS[mutant],
    }


def model_output(cases):
    lines = ['XADB_DEV3_SNAPSHOT_MODEL ' + json.dumps(row) for row in cases]
    lines.append('XADB_DEV3_SNAPSHOT_COMPLETE ' + json.dumps({
        'scope': 'D3-M02-P01', 'status': 'PASS', 'normal_cases': 4, 'negative_cases': 8}))
    lines.append(f'test {MODEL_TEST} ... ok')
    lines.append('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;')
    return '\n'.join(lines)


class ReceiptTests(unittest.TestCase):
    def setUp(self):
        self.cases = [case(name) for name in sorted(NORMAL_CASES)]
        self.cases += [case(name, mutant) for name, mutant in sorted(NEGATIVE_CASES)]

    def test_full_named_inventory_is_accepted(self):
        self.assertEqual(check_model_output(model_output(self.cases), 0), self.cases)
        output = '\n'.join(f'test storage::page::tests::{name} ... ok' for name in PAGE_TESTS)
        output += '\ntest result: ok. 7 passed; 0 failed; 0 ignored; 0 measured;\n'
        self.assertEqual(check_page_output(output, 0), sorted(PAGE_TESTS))
        self.assertEqual(check_vectors('XADB_DEV3_PAGE_VECTORS ' + json.dumps(VECTOR_RESULT)),
                         VECTOR_RESULT)

    def test_missing_duplicate_wrong_scope_and_budget_exhaustion_are_rejected(self):
        for rows in [self.cases[:-1], self.cases + [self.cases[0]]]:
            with self.assertRaises(ValueError):
                check_model_output(model_output(rows), 0)
        for invalid in [[], None, 7, 'case']:
            rows = copy.deepcopy(self.cases)
            rows[0] = invalid
            with self.subTest(case=invalid), self.assertRaises(ValueError):
                check_model_output(model_output(rows), 0)
        for field, value in [('scope', 'D3-M02'), ('status', 'INCONCLUSIVE'),
                             ('states', 1_000_001), ('transitions', 8_000_001),
                             ('max_depth', 17), ('elapsed_ms', 60_001),
                             ('unexplored', 1), ('witnesses', []),
                             ('counterexample', ['bad']), ('error', 'failure'),
                             ('states', True), ('transitions', 0), ('bound_depth', 15),
                             ('witnesses', {'unrelated': ['q_read']}), ('fixture', []), ('mutant', {})]:
            rows = copy.deepcopy(self.cases)
            rows[0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                check_model_output(model_output(rows), 0)

    def test_mutant_without_a_counterexample_is_not_a_pass(self):
        rows = copy.deepcopy(self.cases)
        rows[-1]['counterexample'] = []
        with self.assertRaises(ValueError):
            check_model_output(model_output(rows), 0)
        rows[-1]['counterexample'] = ['q_read']
        rows[-1]['status'] = 'PASS'
        with self.assertRaises(ValueError):
            check_model_output(model_output(rows), 0)
        for field, value in [('counterexample', 'not a replayable trace'),
                             ('counterexample', ['unknown_action']), ('violation', 'wrong_error')]:
            rows = copy.deepcopy(self.cases)
            rows[-1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                check_model_output(model_output(rows), 0)

    def test_wrong_binary_exit_or_missing_execution_and_completion_are_rejected(self):
        output = model_output(self.cases)
        for changed, code in [(output, 1), (output.replace(f'test {MODEL_TEST} ... ok', ''), 0),
                              (output.replace('XADB_DEV3_SNAPSHOT_COMPLETE', 'OTHER'), 0),
                              (output.replace('test result: ok.', 'omitted'), 0)]:
            with self.assertRaises(ValueError):
                check_model_output(changed, code)
        for output in ['', 'test result: ok. 7 passed; 0 failed; 0 ignored;',
                       '\n'.join(f'test storage::page::tests::{name} ... ok'
                                 for name in PAGE_TESTS)]:
            with self.assertRaises(ValueError):
                check_page_output(output, 0)
        for output in ['', 'XADB_DEV3_PAGE_VECTORS {"status":"PASS"}',
                       ('XADB_DEV3_PAGE_VECTORS ' + json.dumps(VECTOR_RESULT) + '\n') * 2]:
            with self.assertRaises(ValueError):
                check_vectors(output)

    def test_key_inventory_requires_each_name_once_and_one_exact_summary(self):
        for prefix, expected in [
            ('storage::key::tests::', KEY_TESTS), ('', SCALAR_KEY_TESTS),
            ('storage::anchor::tests::', ANCHOR_TESTS),
            ('storage::undo_ref::tests::', UNDO_REF_TESTS),
            ('storage::tree_page::tests::', TREE_TESTS),
            ('storage::overflow::tests::', OVERFLOW_TESTS),
            ('storage::transaction_record::tests::', TRANSACTION_TESTS),
        ]:
            lines = [f'test {prefix}{name} ... ok' for name in sorted(expected)]
            summary = f'test result: ok. {len(expected)} passed; 0 failed; 0 ignored;'
            output = '\n'.join(lines + [summary])
            self.assertEqual(check_named_tests(output, 0, prefix, expected), sorted(expected))
            for wrong, code in [
                (output, 1), ('\n'.join(lines[1:] + [summary]), 0),
                (output + '\n' + lines[0], 0), (output + '\n' + summary, 0),
                (output.replace(' ... ok', ' ... ignored', 1), 0),
                ('\n'.join(lines), 0), (summary, 0),
            ]:
                with self.subTest(prefix=prefix, output=wrong, code=code):
                    with self.assertRaises(ValueError):
                        check_named_tests(wrong, code, prefix, expected)

    def test_successful_parent_with_live_child_is_failed_and_cleaned_up(self):
        with tempfile.TemporaryDirectory() as directory:
            root, checks = Path(directory), []
            command = [sys.executable, '-c',
                       "import subprocess,sys; "
                       "p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); "
                       "print(p.pid,flush=True)"]
            with self.assertRaisesRegex(RuntimeError, 'left descendants'):
                run_logged('background', command, root, os.environ.copy(), root, checks, 5)
            self.assertTrue(checks[0]['remaining_group_signalled'])
            self.assertEqual(checks[0]['exit_code'], 0)
            self.assertFalse(checks[0]['interrupted'])
            self.assertTrue((root / 'background.log').read_text().strip().isdigit())

    def test_timeout_is_failed_and_keeps_the_partial_log(self):
        with tempfile.TemporaryDirectory() as directory:
            root, checks = Path(directory), []
            command = [sys.executable, '-c',
                       "import time; print('started',flush=True); time.sleep(30)"]
            with self.assertRaisesRegex(RuntimeError, 'exceeded timeout'):
                run_logged('timeout', command, root, os.environ.copy(), root, checks, 0.2)
            self.assertTrue(checks[0]['interrupted'])
            self.assertTrue(checks[0]['remaining_group_signalled'])
            self.assertNotEqual(checks[0]['exit_code'], 0)
            self.assertIn('started', (root / 'timeout.log').read_text())


if __name__ == '__main__':
    unittest.main()
