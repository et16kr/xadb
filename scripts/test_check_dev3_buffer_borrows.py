"""Fail-closed compiler diagnostic parsing; synthetic rows are not compiler evidence."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from check_dev3_buffer_borrows import PROBE, check_diagnostics, probe_source, run_compiler


def output(errors=(), success=False):
    rows = [{'reason': 'compiler-message', 'message': error} for error in errors]
    rows.append({'reason': 'build-finished', 'success': success})
    return '\n'.join(json.dumps(row) for row in rows)


class BorrowDiagnosticTests(unittest.TestCase):
    def setUp(self):
        self.error = {'level': 'error', 'code': {'code': 'E0505'}, 'spans': [
            {'is_primary': True, 'file_name': PROBE, 'line_start': 7, 'line_end': 7}]}

    def test_positive_and_exact_negative(self):
        self.assertEqual(check_diagnostics(output(success=True), 0), [])
        self.assertEqual(check_diagnostics(output([self.error]), 101, 'E0505', 7)[0]['code'], 'E0505')
        for kind in ('pool', 'pin', 'frames'):
            self.assertIsNotNone(probe_source(kind, False)[1])
            self.assertIsNone(probe_source(kind, True)[1])

    def test_missing_duplicate_and_successful_negative(self):
        for text, rc in [(output(), 101), (output([self.error] * 2), 101), (output(success=True), 0), ('', 101), (output([self.error]) + '\n' + output(), 101)]:
            with self.assertRaises(ValueError):
                check_diagnostics(text, rc, 'E0505', 7)

    def test_wrong_code_and_wrong_primary_span(self):
        for field, value in [('file_name', 'crates/engine/src/lib.rs'), ('line_start', 6), ('line_end', 8), ('is_primary', False)]:
            error = copy.deepcopy(self.error)
            error['spans'][0][field] = value
            with self.assertRaises(ValueError):
                check_diagnostics(output([error]), 101, 'E0505', 7)
        for code in ('E0603', 'E0432', 'E0499', None):
            error = copy.deepcopy(self.error)
            error['code'] = {'code': code}
            with self.assertRaises(ValueError):
                check_diagnostics(output([error]), 101, 'E0505', 7)
        error = copy.deepcopy(self.error)
        error['spans'] *= 2
        with self.assertRaises(ValueError):
            check_diagnostics(output([error]), 101, 'E0505', 7)

    def test_failed_baseline_and_positive_never_pass(self):
        for text, rc in [(output([self.error]), 101), (output(success=True), 1), (output(), 0)]:
            with self.assertRaises(ValueError):
                check_diagnostics(text, rc)

    def test_timeout_preserves_both_partial_logs(self):
        with tempfile.TemporaryDirectory() as directory:
            root, row = Path(directory), {}
            command = [sys.executable, '-c', "import sys,time; print('started',flush=True); print('error-stream',file=sys.stderr,flush=True); time.sleep(30)"]
            with self.assertRaisesRegex(RuntimeError, 'exceeded timeout'):
                run_compiler(command, root, os.environ.copy(), root, row, timeout=0.2)
            self.assertTrue(row['interrupted'])
            self.assertTrue(row['remaining_group_signalled'])
            self.assertNotEqual(row['returncode'], 0)
            self.assertIn('started', Path(row['stdout']).read_text())
            self.assertIn('error-stream', Path(row['stderr']).read_text())

    def test_success_with_live_descendant_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root, row = Path(directory), {}
            command = [sys.executable, '-c', "import subprocess,sys; p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)']); print(p.pid,flush=True)"]
            with self.assertRaisesRegex(RuntimeError, 'left descendants'):
                run_compiler(command, root, os.environ.copy(), root, row, timeout=5)
            self.assertFalse(row['interrupted'])
            self.assertTrue(row['remaining_group_signalled'])
            self.assertEqual(row['returncode'], 0)
            self.assertTrue(Path(row['stdout']).read_text().strip().isdigit())

if __name__ == '__main__':
    unittest.main()
