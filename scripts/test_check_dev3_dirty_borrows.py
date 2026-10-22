#!/usr/bin/env python3
"""New dirty manifest/selection boundaries only; no compiler invocation."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import check_dev3_dirty_borrows as runner


class DirtyCompilerChecks(unittest.TestCase):
    def test_frozen_inputs(self):
        rows = runner.inputs()
        self.assertEqual([row['name'] for row in rows], list(runner.CASE_NAMES))
        self.assertEqual(sum(row['expected_code'] is not None for row in rows), 16)
        self.assertEqual([row['name'] for row in rows if row['expected_code'] is None],
                         ['baseline', 'release'])

    def test_changed_manifest_or_fixture_is_rejected(self):
        original = json.loads((runner.INPUTS / 'manifest.json').read_text())
        changes = [
            lambda value: value['cases'].pop(),
            lambda value: value['cases'].append(value['cases'][-1]),
            lambda value: value['cases'][2].update(expected_code='E0499'),
            lambda value: value['cases'][2].update(primary_line=9),
            lambda value: value.update(unrelated_diagnostics_are_failures=False),
            lambda value: value.update(compiler_budget_seconds=61),
            lambda value: value.update(anchor_relative_path='crates/engine/src/lib.rs'),
            lambda value: value['cases'][-2].update(primary_spans='one'),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / 'inputs'
            shutil.copytree(runner.INPUTS, target)
            manifest = target / 'manifest.json'
            for change in changes:
                with self.subTest(change=changes.index(change)):
                    value = copy.deepcopy(original)
                    change(value)
                    manifest.write_text(json.dumps(value))
                    # Pin the modified bytes to exercise semantic rejection too.
                    expected = hashlib.sha256(manifest.read_bytes()).hexdigest()
                    with self.assertRaises(ValueError):
                        runner.inputs(target, expected)
            manifest.write_text(json.dumps(original))
            expected = hashlib.sha256(manifest.read_bytes()).hexdigest()
            fixture = target / 'storage_drop.rs'
            fixture.write_text(fixture.read_text() + '\n')
            with self.assertRaises(ValueError):
                runner.inputs(target, expected)

    def test_narrow_selection_requires_one_baseline(self):
        rows = runner.inputs()
        self.assertEqual([row['name'] for row in runner.select_cases(rows, ['storage_drop'])],
                         ['baseline', 'storage_drop'])
        self.assertEqual([row['name'] for row in runner.select_cases(rows, ['release'])],
                         ['baseline', 'release'])
        for names in ([], ['storage_drop', 'storage_drop'], ['missing']):
            with self.subTest(names=names), self.assertRaises(ValueError):
                runner.select_cases(rows, names)

    def test_total_deadline_is_not_per_probe(self):
        self.assertEqual(runner.remaining_budget(59), 1)
        for used in (60, 61):
            with self.subTest(used=used), self.assertRaises(RuntimeError):
                runner.remaining_budget(used)


    def test_private_constructor_exact_code_and_all_primary_spans(self):
        frozen = next(row for row in runner.inputs() if row['name'] == 'permit_private')
        def message(spans, code='E0451'):
            return '\n'.join(json.dumps(value) for value in [
                {'reason': 'compiler-message', 'message': {
                    'level': 'error', 'code': {'code': code}, 'spans': spans}},
                {'reason': 'build-finished', 'success': False},
            ])
        span = {'is_primary': True, 'file_name': runner.PROBE, 'line_start': 10, 'line_end': 10}
        self.assertEqual(len(runner.verify_diagnostics(message([span, span]), 101, frozen)), 1)
        for spans, code in [([], 'E0451'), ([dict(span, file_name='wrong.rs')], 'E0451'),
                            ([dict(span, line_end=11)], 'E0451'), ([span], 'E0063')]:
            with self.subTest(spans=spans, code=code), self.assertRaises(ValueError):
                runner.verify_diagnostics(message(spans, code), 101, frozen)

    def test_private_constructor_rejects_extra_errors_and_success(self):
        frozen = next(row for row in runner.inputs() if row['name'] == 'permit_private')
        error = {'reason': 'compiler-message', 'message': {'level': 'error',
                 'code': {'code': 'E0451'}, 'spans': [{'is_primary': True,
                 'file_name': runner.PROBE, 'line_start': 10, 'line_end': 10}]}}
        extra = '\n'.join(map(json.dumps, [error, error, {'reason': 'build-finished', 'success': False}]))
        with self.assertRaises(ValueError):
            runner.verify_diagnostics(extra, 101, frozen)
        with self.assertRaises(ValueError):
            runner.verify_diagnostics(json.dumps({'reason': 'build-finished', 'success': True}), 0, frozen)


if __name__ == '__main__':
    unittest.main()
