"""Allocation-only checker counterexamples; synthetic diagnostics are not rustc evidence."""
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from check_dev0 import digest
from check_dev3_allocation_borrows import (
    CASE_NAMES, INPUTS, PROBE, inputs, preserve_artifacts, remaining_budget,
    select_cases, verify_diagnostics,
)


def compiler_output(error=None, success=False):
    rows = [] if error is None else [{'reason': 'compiler-message', 'message': error}]
    rows.append({'reason': 'build-finished', 'success': success})
    return '\n'.join(map(json.dumps, rows))


class AllocationBorrowCheckerTests(unittest.TestCase):
    def test_frozen_complete_inventory_and_selection(self):
        cases = inputs()
        self.assertEqual([row['name'] for row in cases], list(CASE_NAMES))
        self.assertEqual([row['name'] for row in select_cases(cases, None)],
                         ['baseline', 'release', 'installed', 'frame', 'operation', 'flush', 'slot'])
        self.assertEqual([row['name'] for row in select_cases(cases, ['operation'])],
                         ['baseline', 'operation'])
        for selection in ([], ['frame', 'frame'], ['unknown']):
            with self.assertRaises(ValueError):
                select_cases(cases, selection)

    def test_changed_bytes_missing_and_extra_fixture_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            for corruption in ('changed', 'missing', 'extra'):
                shutil.copytree(INPUTS, target)
                if corruption == 'changed':
                    (target / 'frame.rs').write_text('unrelated invalid Rust\n')
                elif corruption == 'missing':
                    (target / 'frame.rs').unlink()
                else:
                    (target / 'extra.rs').write_text('')
                with self.assertRaises(ValueError):
                    inputs(target)
                shutil.rmtree(target)

    def test_manifest_tampering_and_inventory_loss_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            shutil.copytree(INPUTS, target)
            path = target / 'manifest.json'
            manifest = json.loads(path.read_text())
            for case_list in (manifest['cases'][:-1], manifest['cases'] + [manifest['cases'][1]]):
                candidate = dict(manifest, cases=case_list)
                path.write_text(json.dumps(candidate))
                with self.assertRaisesRegex(ValueError, 'manifest changed'):
                    inputs(target)
                with self.assertRaisesRegex(ValueError, 'compiler case'):
                    inputs(target, digest(path))

    def test_all_five_rejections_require_exact_code_and_primary_location(self):
        for frozen in inputs():
            if frozen['expected_code'] is None:
                continue
            error = {'level': 'error', 'code': {'code': frozen['expected_code']}, 'spans': [
                {'is_primary': True, 'file_name': PROBE, 'line_start': 10, 'line_end': 10}]}
            self.assertEqual(verify_diagnostics(compiler_output(error), 101, frozen)[0]['code'],
                             frozen['expected_code'])
            for field, value in (('file_name', 'crates/engine/src/lib.rs'),
                                 ('line_start', 9), ('line_end', 11), ('is_primary', False)):
                wrong = copy.deepcopy(error)
                wrong['spans'][0][field] = value
                with self.assertRaises(ValueError):
                    verify_diagnostics(compiler_output(wrong), 101, frozen)
            wrong = copy.deepcopy(error)
            wrong['code']['code'] = 'E0432'
            with self.assertRaises(ValueError):
                verify_diagnostics(compiler_output(wrong), 101, frozen)

    def test_missing_duplicate_and_unexpected_compiler_success_fail(self):
        frozen = inputs()[1]
        error = {'level': 'error', 'code': {'code': 'E0382'}, 'spans': [
            {'is_primary': True, 'file_name': PROBE, 'line_start': 10, 'line_end': 10}]}
        output = compiler_output(error)
        for text, code in (('', 101), (compiler_output(), 101),
                           (output + '\n' + output, 101),
                           (compiler_output(success=True), 0)):
            with self.assertRaises(ValueError):
                verify_diagnostics(text, code, frozen)

    def test_baseline_and_release_must_succeed(self):
        for frozen in (inputs()[0], inputs()[-1]):
            self.assertEqual(verify_diagnostics(compiler_output(success=True), 0, frozen), [])
            for text, code in ((compiler_output(), 101), (compiler_output(success=True), 1)):
                with self.assertRaises(ValueError):
                    verify_diagnostics(text, code, frozen)

    def test_suite_budget_is_not_reset_for_each_probe(self):
        self.assertEqual(remaining_budget(0), 30)
        self.assertEqual(remaining_budget(29), 1)
        for elapsed in (30, 31):
            with self.assertRaisesRegex(RuntimeError, '30 second budget'):
                remaining_budget(elapsed)

    def test_metadata_is_retained_and_positive_requires_engine_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target, evidence = root / 'target', root / 'evidence'
            target.mkdir()
            evidence.mkdir()
            binary = target / 'libxadb_engine.rmeta'
            binary.write_bytes(b'frozen compiler metadata')
            output = json.dumps({'reason': 'compiler-artifact',
                                 'target': {'name': 'xadb_engine'}, 'filenames': [str(binary)]})
            result = preserve_artifacts(output, evidence, target, True)
            self.assertEqual(result[0]['sha256'], digest(binary))
            self.assertEqual(Path(result[0]['retained_path']).read_bytes(), binary.read_bytes())
            binary.write_bytes(b'later compilation')
            self.assertNotEqual(digest(binary), result[0]['sha256'])
            with self.assertRaisesRegex(ValueError, 'no xadb-engine'):
                preserve_artifacts('', evidence, target, True)
            outside = root / 'outside.rmeta'
            outside.write_bytes(b'wrong target')
            output = json.dumps({'reason': 'compiler-artifact', 'filenames': [str(outside)]})
            with self.assertRaisesRegex(ValueError, 'outside isolated target'):
                preserve_artifacts(output, evidence, target, False)


if __name__ == '__main__':
    unittest.main()
