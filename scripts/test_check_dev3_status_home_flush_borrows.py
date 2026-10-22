#!/usr/bin/env python3
"""Status flush compiler-input checker tests; never invoke a compiler or product."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
import check_dev3_status_home_flush_borrows as subject


def transcript(code=None, line=10, *, success=False, path=subject.PROBE, count=1):
    rows = []
    if code is not None:
        span = dict(is_primary=True, file_name=path, line_start=line, line_end=line)
        rows.append(dict(reason='compiler-message', message=dict(
            level='error', code={'code': code}, spans=[dict(span) for _ in range(count)])))
    rows.append(dict(reason='build-finished', success=success))
    return '\n'.join(json.dumps(row) for row in rows)


class StatusHomeFlushCompilerInputs(unittest.TestCase):
    def test_inventory_two_injection_scopes_and_diff_selection(self):
        cases = subject.inputs()
        self.assertEqual(len(cases), 7)
        self.assertEqual([r['semantic_id'] for r in cases],
                         [None] + [f'BC{i:02}' for i in range(6)])
        self.assertEqual([r['name'] for r in subject.select_cases(cases, ['raw_authority'])],
                         ['baseline', 'raw_authority'])
        self.assertEqual(subject.injection('raw_authority'),
                         (subject.PRODUCER_PROBE, subject.PRODUCER_ANCHOR))
        self.assertEqual(subject.injection('authority_private'),
                         (subject.PRODUCER_PROBE, subject.PRODUCER_ANCHOR))
        self.assertEqual(subject.injection('storage_borrow'), (subject.PROBE, subject.ANCHOR))
        for names in ([], ['release', 'release'], ['old_unrelated_probe']):
            with self.assertRaises(ValueError):
                subject.select_cases(cases, names)

    def test_frozen_source_scope_and_manifest_changes_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            shutil.copytree(subject.INPUTS, target)
            source = target / 'release.rs'
            source.write_text(source.read_text() + '\n')
            with self.assertRaisesRegex(ValueError, 'source changed'):
                subject.inputs(target)
            shutil.copyfile(subject.INPUTS / source.name, source)
            path = target / 'manifest.json'
            original = json.loads(path.read_text())
            for change, reason in (
                (lambda m: m['cases'].__setitem__(1, copy.deepcopy(m['cases'][0])), 'compiler case'),
                (lambda m: m.__setitem__('semantic_case_count', 7), 'contract changed'),
                (lambda m: m['cases'][4].__setitem__('probe_relative_path', subject.PROBE), 'injection path'),
                (lambda m: m['cases'][3].__setitem__('primary_line', 11), 'diagnostic identity'),
                (lambda m: m.__setitem__('producer_anchor_relative_path', subject.ANCHOR), 'contract changed'),
            ):
                bad = copy.deepcopy(original)
                change(bad)
                path.write_text(json.dumps(bad))
                with self.assertRaisesRegex(ValueError, reason):
                    subject.inputs(target, hashlib.sha256(path.read_bytes()).hexdigest())
            path.write_text(json.dumps(original))
            with self.assertRaisesRegex(ValueError, 'manifest changed'):
                subject.inputs(target)

    def test_completion_single_diagnostic_and_both_exact_paths(self):
        cases = {r['name']: r for r in subject.inputs()}
        positive = transcript(success=True)
        subject.verify_diagnostics(positive, 0, cases['release'])
        for output, status in ((positive, 101), ('', 0),
                               (positive + '\n' + positive, 0),
                               (transcript('E0505', success=True), 0)):
            with self.assertRaises(ValueError):
                subject.verify_diagnostics(output, status, cases['release'])
        for name in subject.CASE_NAMES[2:]:
            frozen = cases[name]
            code, path = frozen['expected_code'], frozen['probe_relative_path']
            good = transcript(code, path=path)
            subject.verify_diagnostics(good, 101, frozen)
            for output, status in (
                (good, 0), (good, -9), (good, 1),
                (transcript('E0603', path=path), 101),
                (transcript(code, line=11, path=path), 101),
                (transcript(code, path='wrong.rs'), 101),
                (transcript(code, path=path, success=True), 101),
                (transcript(code, path=path, count=0), 101),
                (good + '\n' + good.splitlines()[0], 101),
                (good + '\n' + good.splitlines()[-1], 101),
                (good.splitlines()[0], 101),
            ):
                with self.assertRaises(ValueError, msg=name):
                    subject.verify_diagnostics(output, status, frozen)
            many = transcript(code, path=path, count=2)
            if frozen['primary_spans'] == 'all_on_line':
                subject.verify_diagnostics(many, 101, frozen)
            else:
                with self.assertRaises(ValueError):
                    subject.verify_diagnostics(many, 101, frozen)
        with self.assertRaisesRegex(ValueError, 'exact frozen probe expression'):
            subject.verify_diagnostics(transcript('E0308', path=subject.PROBE),
                                       101, cases['raw_authority'])

    def test_aggregate_budget_is_not_per_probe(self):
        self.assertEqual(subject.remaining_budget(29.75), .25)
        for elapsed in (30, 31):
            with self.assertRaises(RuntimeError):
                subject.remaining_budget(elapsed)


if __name__ == '__main__':
    unittest.main()
