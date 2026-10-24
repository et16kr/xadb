#!/usr/bin/env python3
"""Focused new compiler-checker contract tests; no Cargo/product execution."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import check_dev3_mixed_init_borrows as subject


def transcript(code=None, line=None, *, success=False, path=subject.PROBE):
    rows = []
    if code is not None:
        rows.append(dict(reason='compiler-message', message=dict(
            level='error', code={'code': code}, spans=[dict(
                is_primary=True, file_name=path, line_start=line, line_end=line)])))
    rows.append(dict(reason='build-finished', success=success))
    return '\n'.join(json.dumps(row) for row in rows)


class MixedInitCompilerInputs(unittest.TestCase):
    def test_frozen_inventory_and_selective_subset(self):
        cases = subject.inputs()
        self.assertEqual(len(cases), 7)
        self.assertEqual([row['name'] for row in subject.select_cases(cases, ['owner_drop'])],
                         ['baseline', 'owner_drop'])
        for selection in ([], ['positive', 'positive'], ['unknown']):
            with self.assertRaises(ValueError):
                subject.select_cases(cases, selection)

    def test_input_mutation_and_unregistered_probe_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            shutil.copytree(subject.INPUTS, target)
            path = target / 'positive.rs'
            path.write_text(path.read_text() + '\n')
            with self.assertRaisesRegex(ValueError, 'source changed'):
                subject.inputs(target)
            shutil.copyfile(subject.INPUTS / 'positive.rs', path)
            (target / 'extra.rs').write_text('')
            with self.assertRaisesRegex(ValueError, 'unregistered'):
                subject.inputs(target)

    def test_manifest_identity_and_contract_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            shutil.copytree(subject.INPUTS, target)
            path = target / 'manifest.json'
            original = json.loads(path.read_text())
            for mutation in ('semantic_case_count', 'anchor_relative_path', 'negative_returncode'):
                changed = copy.deepcopy(original)
                changed[mutation] = 'invalid'
                path.write_text(json.dumps(changed))
                with self.assertRaisesRegex(ValueError, 'contract changed'):
                    subject.inputs(target, hashlib.sha256(path.read_bytes()).hexdigest())
            with self.assertRaisesRegex(ValueError, 'manifest changed'):
                subject.inputs(target)

    def test_exact_negative_origin_and_completion(self):
        for frozen in subject.inputs()[2:]:
            code, line = frozen['expected_code'], frozen['primary_line']
            good = transcript(code, line)
            subject.verify_diagnostics(good, 101, frozen)
            for output, status in [
                (good, 0), (good, -9), ('', 101),
                (transcript('E0603', line), 101),
                (transcript(code, line + 1), 101),
                (transcript(code, line, path='unrelated.rs'), 101),
                (transcript(code, line, success=True), 101),
                (good + '\n' + good, 101),
                (good.splitlines()[0], 101),
            ]:
                with self.assertRaises(ValueError):
                    subject.verify_diagnostics(output, status, frozen)

    def test_positive_needs_completed_success(self):
        for frozen in subject.inputs()[:2]:
            good = transcript(success=True)
            subject.verify_diagnostics(good, 0, frozen)
            for output, status in [(good, 101), ('', 0),
                                    (transcript('E0505', 36, success=True), 0),
                                    (good + '\n' + good, 0)]:
                with self.assertRaises(ValueError):
                    subject.verify_diagnostics(output, status, frozen)

    def test_timeout_budget_never_accepts_exhaustion(self):
        self.assertEqual(subject.remaining_budget(0), 60)
        self.assertEqual(subject.remaining_budget(59.5), 0.5)
        for elapsed in (60, 61):
            with self.assertRaises(RuntimeError):
                subject.remaining_budget(elapsed)


if __name__ == '__main__':
    unittest.main()
