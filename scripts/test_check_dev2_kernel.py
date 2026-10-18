#!/usr/bin/env python3
"""Known-bad receipt cases; never launch a product process in these tests."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import check_dev2_kernel as kernel


def summary():
    return {'cases': 33,
            'verdict_counts': {'ACCEPTED': 33, 'REJECTED': 0, 'INVALID': 0, 'INCOMPLETE': 0},
            'campaign_inventory': {'verdict': 'COMPLETE', 'expected_cases': 33, 'actual_cases': 33,
                                   'missing': [], 'unexpected': [], 'duplicate': [], 'case_errors': []},
            'run_inventory': {'unreferenced': [], 'missing': [], 'reused': []},
            'controller_negative_complete': True, 'controller_negative': [{'verdict': 'ACCEPTED'}],
            'formal_execution': {'repetition': 0}}


class KernelRunnerTests(unittest.TestCase):
    def test_inventory_rejects_zero_and_duplicate_tests(self):
        for output in ('0 tests, 0 benchmarks\n', 'one: test\none: test\n'):
            with self.assertRaises(ValueError):
                kernel.test_inventory(output)
        self.assertEqual(kernel.test_inventory('one: test\ntwo: test\n'), {'one', 'two'})

    def test_nocapture_output_and_declared_ignored_target_preserve_exact_completed_inventory(self):
        output = ('running 2 tests\ntest one ... PROFILE {}\nok\n'
                  'test long ... ignored, explicit V3\n'
                  'test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.10s\n')
        kernel.require_test_summary(output, {'one'}, {'long'})
        with self.assertRaises(ValueError):
            kernel.require_test_summary(output, {'one'})

    def test_zero_duplicate_missing_and_filtered_results_never_pass(self):
        good = ('test one ... ok\n'
                'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s\n')
        for output in (good.replace('1 passed', '0 passed'), good + 'test one ... ok\n',
                       good.replace('test one ... ok\n', ''), good.replace('0 filtered out', '1 filtered out')):
            with self.assertRaises(ValueError):
                kernel.require_test_summary(output, {'one'})
        with self.assertRaises(ValueError):
            kernel.require_test_summary(good, set())

    def test_python_zero_partial_or_skipped_suite_never_passes(self):
        kernel.require_python_tests('Ran 75 tests in 0.12s\n\nOK\n')
        for output in ('Ran 0 tests in 0.12s\n\nOK\n', 'Ran 74 tests in 0.12s\n\nOK\n',
                       'Ran 75 tests in 0.12s\n\nOK (skipped=1)\n'):
            with self.assertRaises(ValueError):
                kernel.require_python_tests(output)

    def test_thread_seed_overlap_and_work_receipts_are_required(self):
        profile = {'generator': 'dev2-durable-thread-v1', 'seed_count': 64,
                   'writer_attempts_per_seed': 128, 'readers': 4, 'writers': 2,
                   'min_seconds': 0, 'power_loss': False}
        result = {'generator': profile['generator'], 'rounds': 64, 'commits': 4096,
                  'conflicts': 4096, 'seed_visits': [1] * 64, 'elapsed_ms': 1000,
                  'sampler_checks': 100, 'sampler_overlap_begin': 20, 'sampler_overlap_end': 15,
                  'sampler_overlap_both': 10, 'reader_row_checks': 3 * 64 * 64 * 4 + 200}
        def output(record):
            return 'XADB_KERNEL_THREAD_PROFILE ' + json.dumps(profile) + '\nXADB_KERNEL_THREAD_RESULT ' + json.dumps(record)
        kernel.require_kernel_thread_receipts(output(result), 1.1)
        for field, value in (('seed_visits', [1] * 63), ('conflicts', 4095),
                             ('sampler_overlap_both', 0), ('sampler_overlap_end', 101),
                             ('reader_row_checks', 1), ('elapsed_ms', 1102)):
            with self.assertRaises(ValueError):
                kernel.require_kernel_thread_receipts(output({**result, field: value}), 1.1)

    def test_strict_oracle_requires_every_case_and_controller_negative_and_exact_repetition(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'summary.json'
            path.write_text(json.dumps(summary()))
            kernel.require_oracle_summary(path, 0)
            for field, value in (('cases', 32), ('controller_negative_complete', False),
                                 ('controller_negative', []), ('formal_execution', {'repetition': 1}),
                                 ('run_inventory', {'unreferenced': ['extra'], 'missing': [], 'reused': []})):
                changed = summary()
                changed[field] = value
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    kernel.require_oracle_summary(path, 0)
            changed = summary()
            changed['campaign_inventory']['missing'] = ['a cut']
            path.write_text(json.dumps(changed))
            with self.assertRaises(ValueError):
                kernel.require_oracle_summary(path, 0)
            for changed in ([], {**summary(), 'campaign_inventory': []},
                            {**summary(), 'controller_negative': [None]},
                            {**summary(), 'formal_execution': None}):
                path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    kernel.require_oracle_summary(path, 0)

    def test_duplicate_json_summary_fields_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'summary.json'
            path.write_text('{"cases":33,"cases":33}')
            with self.assertRaises(ValueError):
                kernel.require_oracle_summary(path, 0)

    def test_subprocess_error_preserves_failed_check_log_and_final_report(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('fixed fixture')

            def fail(argv, cwd, environment, log, timeout):
                log.write_text('retained spawn/wait failure')
                raise subprocess.SubprocessError('injected process failure')

            with mock.patch.object(kernel, 'ROOT', root), \
                 mock.patch.object(kernel, 'SOURCE_PATHS', ('Cargo.toml',)), \
                 mock.patch.object(kernel, 'source_fingerprint', return_value='frozen'), \
                 mock.patch.object(kernel, 'run_command', side_effect=fail), \
                 mock.patch.object(kernel.platform, 'system', return_value='Linux'), \
                 mock.patch.object(sys, 'argv', ['check_dev2_kernel.py']):
                self.assertEqual(kernel.main(), 1)
            reports = list((root / 'artifacts').glob('d2k-*/report.json'))
            self.assertEqual(len(reports), 1)
            report = json.loads(reports[0].read_text())
            self.assertEqual(report['status'], 'FAIL')
            check = report['checks'][0]
            self.assertEqual(check['status'], 'FAIL')
            self.assertIn('SubprocessError', check['error'])
            self.assertEqual(Path(check['log']['path']).read_text(), 'retained spawn/wait failure')
            self.assertEqual(check['log']['sha256'], kernel.digest(check['log']['path']))


if __name__ == '__main__':
    unittest.main()
