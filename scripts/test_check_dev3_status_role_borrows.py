#!/usr/bin/env python3
"""New StatusRole compiler checker cases; never invokes Cargo or product code."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
import check_dev3_status_role_borrows as subject


def transcript(code=None, line=None, *, success=False, path=subject.PROBE, count=1):
    rows = []
    if code is not None:
        primary = dict(is_primary=True, file_name=path, line_start=line, line_end=line)
        rows.append(dict(reason='compiler-message', message=dict(
            level='error', code={'code': code}, spans=[dict(primary) for _ in range(count)])))
    rows.append(dict(reason='build-finished', success=success))
    return '\n'.join(json.dumps(row) for row in rows)


class StatusRoleCompilerInputs(unittest.TestCase):
    def test_inventory_and_diff_only_selection(self):
        cases = subject.inputs()
        self.assertEqual(len(cases), 4)
        self.assertEqual([row['semantic_id'] for row in cases], [None, 'NC00', 'NC01', 'NC02'])
        self.assertEqual([row['name'] for row in subject.select_cases(cases, ['owner_drop'])],
                         ['baseline', 'owner_drop'])
        self.assertEqual([row['name'] for row in subject.select_cases(cases, None)],
                         list(subject.CASE_NAMES))
        for names in ([], ['positive', 'positive'], ['unknown']):
            with self.assertRaises(ValueError):
                subject.select_cases(cases, names)

    def test_mutated_sources_manifest_census_and_contract_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            shutil.copytree(subject.INPUTS, target)
            path = target / 'positive.rs'
            path.write_text(path.read_text() + '\n')
            with self.assertRaisesRegex(ValueError, 'source changed'):
                subject.inputs(target)
            shutil.copyfile(subject.INPUTS / 'positive.rs', path)
            path = target / 'manifest.json'
            original = json.loads(path.read_text())
            for edit, reason in (
                (lambda m: m['cases'].__setitem__(1, copy.deepcopy(m['cases'][0])), 'compiler case'),
                (lambda m: m.__setitem__('semantic_case_count', 4), 'contract changed'),
                (lambda m: m['cases'][3].__setitem__('primary_line', 8), 'diagnostic identity'),
            ):
                manifest = copy.deepcopy(original)
                edit(manifest)
                path.write_text(json.dumps(manifest))
                sha = hashlib.sha256(path.read_bytes()).hexdigest()
                with self.assertRaisesRegex(ValueError, reason):
                    subject.inputs(target, sha)
            path.write_text(json.dumps(original))
            with self.assertRaisesRegex(ValueError, 'manifest changed'):
                subject.inputs(target)

    def test_exact_diagnostics_and_completion_are_required(self):
        cases = {row['name']: row for row in subject.inputs()}
        good_positive = transcript(success=True)
        subject.verify_diagnostics(good_positive, 0, cases['positive'])
        for output, status in ((good_positive, 101), ('', 0),
                               (good_positive + '\n' + good_positive, 0),
                               (transcript('E0505', 7, success=True), 0)):
            with self.assertRaises(ValueError):
                subject.verify_diagnostics(output, status, cases['positive'])
        for name, code, line in (('private_constructor', 'E0451', 5),
                                  ('owner_drop', 'E0505', 7)):
            frozen = cases[name]
            good = transcript(code, line)
            subject.verify_diagnostics(good, 101, frozen)
            for output, status in (
                (good, 0), (good, -9), (good, 1),
                (transcript('E0603', line), 101),
                (transcript(code, line + 1), 101),
                (transcript(code, line, path='unrelated.rs'), 101),
                (transcript(code, line, success=True), 101),
                (transcript(code, line, count=0), 101),
                (good + '\n' + good.splitlines()[0], 101),
                (good + '\n' + good.splitlines()[-1], 101),
                (good.splitlines()[0], 101),
            ):
                with self.assertRaises(ValueError):
                    subject.verify_diagnostics(output, status, frozen)
        subject.verify_diagnostics(transcript('E0451', 5, count=2), 101,
                                   cases['private_constructor'])
        with self.assertRaises(ValueError):
            subject.verify_diagnostics(transcript('E0505', 7, count=2), 101,
                                       cases['owner_drop'])

    def test_budget_exhaustion_is_failure(self):
        self.assertEqual(subject.remaining_budget(59.75), .25)
        for consumed in (60, 61):
            with self.assertRaises(RuntimeError):
                subject.remaining_budget(consumed)


if __name__ == '__main__':
    unittest.main()
