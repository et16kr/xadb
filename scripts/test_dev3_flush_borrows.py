#!/usr/bin/env python3
"""New flush compiler input/checker boundaries; never invokes a compiler."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
import check_dev3_flush_borrows as subject


class FlushCompilerInputs(unittest.TestCase):
    def test_census_and_exact_narrow_selection(self):
        cases = subject.inputs()
        self.assertEqual(len(cases), 12)
        self.assertEqual([r['name'] for r in subject.select_cases(cases, ['capture_private'])],
                         ['baseline', 'capture_private'])
        for names in ([], ['capture_private', 'capture_private'], ['unknown']):
            with self.assertRaises(ValueError):
                subject.select_cases(cases, names)

    def test_mutated_fixture_and_duplicate_inventory_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            shutil.copytree(subject.INPUTS, target)
            path = target / 'release.rs'
            path.write_text(path.read_text() + '\n')
            with self.assertRaisesRegex(ValueError, 'source changed'):
                subject.inputs(target)
            shutil.copyfile(subject.INPUTS / 'release.rs', path)
            path = target / 'manifest.json'
            manifest = json.loads(path.read_text())
            manifest['cases'][1] = copy.deepcopy(manifest['cases'][0])
            path.write_text(json.dumps(manifest))
            sha = hashlib.sha256(path.read_bytes()).hexdigest()
            with self.assertRaisesRegex(ValueError, 'unexpected compiler case'):
                subject.inputs(target, sha)

    def test_private_rejection_needs_only_exact_primary_spans(self):
        frozen = next(r for r in subject.inputs() if r['name'] == 'capture_private')
        primary = dict(is_primary=True, file_name=subject.PROBE, line_start=10, line_end=10)
        diagnostic = dict(reason='compiler-message', message=dict(
            level='error', code={'code': 'E0451'}, spans=[primary, dict(primary)]))
        finish = dict(reason='build-finished', success=False)
        def output():
            return '\n'.join(json.dumps(row) for row in (diagnostic, finish))
        subject.verify_diagnostics(output(), 101, frozen)
        diagnostic['message']['spans'][1]['line_end'] = 11
        with self.assertRaises(ValueError):
            subject.verify_diagnostics(output(), 101, frozen)
        diagnostic['message']['spans'][1]['line_end'] = 10
        with self.assertRaises(ValueError):
            subject.verify_diagnostics(output() + '\n' + json.dumps(diagnostic), 101, frozen)
        with self.assertRaises(ValueError):
            subject.verify_diagnostics(output(), 0, frozen)

    def test_budget_is_aggregate_and_exhaustion_is_not_success(self):
        self.assertEqual(subject.remaining_budget(59.75), 0.25)
        for consumed in (60, 61):
            with self.assertRaises(RuntimeError):
                subject.remaining_budget(consumed)


if __name__ == '__main__':
    unittest.main()
