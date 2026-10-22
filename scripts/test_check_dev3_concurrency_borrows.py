#!/usr/bin/env python3
"""New concurrency manifest/selection boundaries only; no compiler invocation."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import check_dev3_concurrency_borrows as runner


class ConcurrencyCompilerChecks(unittest.TestCase):
    def test_frozen_inputs(self):
        rows = runner.inputs()
        self.assertEqual([row['name'] for row in rows], list(runner.CASE_NAMES))
        self.assertEqual(sum(row['expected_code'] is not None for row in rows), 10)
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
            lambda value: value.update(compiler_budget_seconds=31),
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
            fixture = target / 'owner_send.rs'
            fixture.write_text(fixture.read_text() + '\n')
            with self.assertRaises(ValueError):
                runner.inputs(target, expected)

    def test_narrow_selection_requires_one_baseline(self):
        rows = runner.inputs()
        self.assertEqual([row['name'] for row in runner.select_cases(rows, ['owner_drop'])],
                         ['baseline', 'owner_drop'])
        self.assertEqual([row['name'] for row in runner.select_cases(rows, ['release'])],
                         ['baseline', 'release'])
        for names in ([], ['owner_drop', 'owner_drop'], ['missing']):
            with self.subTest(names=names), self.assertRaises(ValueError):
                runner.select_cases(rows, names)

    def test_total_deadline_is_not_per_probe(self):
        self.assertEqual(runner.remaining_budget(29), 1)
        for used in (30, 31):
            with self.subTest(used=used), self.assertRaises(RuntimeError):
                runner.remaining_budget(used)


if __name__ == '__main__':
    unittest.main()
