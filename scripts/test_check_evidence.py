#!/usr/bin/env python3
"""Regression tests for evidence binding, not product behavior."""
import hashlib
import contextlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import check_dev0
import check_dev1
from check_dev0 import check_report_paths, source_fingerprint, verify_source_identity, write_report


class EvidenceIdentityTests(unittest.TestCase):
    def test_actual_timeout_retains_partial_output_and_failed_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / 'timeout.log'
            record = {}
            with self.assertRaises(subprocess.TimeoutExpired):
                check_dev0.run_logged_command(
                    [sys.executable, '-B', '-c',
                     'import time; print("retained before timeout", flush=True); time.sleep(30)'],
                    cwd=temporary, environment=os.environ.copy(), log=log,
                    record=record, timeout_seconds=1)
            self.assertEqual(log.read_bytes(), b'retained before timeout\n')
            self.assertEqual(record['status'], 'FAIL')
            self.assertTrue(record['timed_out'])
            self.assertIsNone(record['exit_code'])
            self.assertEqual(record['timeout_seconds'], 1)
            self.assertGreaterEqual(record['elapsed_seconds'], 1)
            self.assertEqual(record['log_sha256'], hashlib.sha256(log.read_bytes()).hexdigest())

    def test_logged_command_preserves_nonzero_exit_and_never_overwrites_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / 'failure.log'
            record = {}
            result = check_dev0.run_logged_command(
                [sys.executable, '-B', '-c', 'import sys; print("failed"); sys.exit(7)'],
                cwd=temporary, environment=os.environ.copy(), log=log, record=record)
            self.assertEqual(result.returncode, 7)
            self.assertEqual(result.stdout, b'failed\n')
            self.assertEqual(record['exit_code'], 7)
            self.assertEqual(record['status'], 'FAIL')
            self.assertFalse(record['timed_out'])
            with self.assertRaises(FileExistsError):
                check_dev0.run_logged_command(
                    [sys.executable, '-B', '-c', 'print("replacement")'], cwd=temporary,
                    environment=os.environ.copy(), log=log, record={})
            self.assertEqual(log.read_bytes(), b'failed\n')

    def test_both_cli_entrypoints_reject_existing_reports_before_any_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            saved = root / 'saved.json'
            saved.write_bytes(b'original evidence')
            link = root / 'alias.json'
            link.symlink_to(saved)
            for module in (check_dev0, check_dev1):
                for path in (saved, link):
                    with self.subTest(module=module.__name__, path=path.name):
                        with patch.object(module, 'ROOT', root), patch.object(sys, 'argv', ['check', '--report', str(path)]), patch('subprocess.run') as command, contextlib.redirect_stderr(io.StringIO()):
                            with self.assertRaises(SystemExit) as caught:
                                module.main()
                            self.assertEqual(caught.exception.code, 2)
                            command.assert_not_called()
            self.assertEqual(saved.read_bytes(), b'original evidence')
            self.assertFalse((root / 'artifacts').exists())

    def test_dev1_cli_rejects_same_report_for_both_stages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'same.json'
            with patch.object(check_dev1, 'ROOT', root), patch.object(sys, 'argv', ['check', '--report', str(path), '--dev0-report', str(path)]), patch('subprocess.run') as command, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit) as caught:
                    check_dev1.main()
                self.assertEqual(caught.exception.code, 2)
                command.assert_not_called()
            self.assertFalse(path.exists())

    def test_new_report_is_saved_without_overwriting_existing_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'report.json'
            check_report_paths([path])
            write_report(path, {'status': 'FAIL'})
            original = path.read_bytes()
            with self.assertRaises(FileExistsError):
                write_report(path, {'status': 'PASS'})
            self.assertEqual(path.read_bytes(), original)

    def test_existing_report_is_rejected_before_running_checks(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'report.json'
            path.write_bytes(b'preserved failure')
            with self.assertRaises(FileExistsError):
                check_report_paths([path])
            self.assertEqual(path.read_bytes(), b'preserved failure')

    def test_symlink_report_does_not_replace_or_create_its_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary) / 'target.json'
            link = Path(temporary) / 'report.json'
            link.symlink_to(target)
            with self.assertRaises(FileExistsError):
                check_report_paths([link])
            with self.assertRaises(FileExistsError):
                write_report(link, {'status': 'PASS'})
            self.assertFalse(target.exists())
            self.assertTrue(link.is_symlink())

    def test_duplicate_report_destinations_are_rejected_before_running_checks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'child').mkdir()
            with self.assertRaises(ValueError):
                check_report_paths([root / 'report.json', root / 'child/../report.json'])
            self.assertFalse((root / 'report.json').exists())

    def test_report_created_after_preflight_is_still_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'report.json'
            check_report_paths([path, None])
            path.write_bytes(b'other run')
            with self.assertRaises(FileExistsError):
                write_report(path, {'status': 'PASS'})
            self.assertEqual(path.read_bytes(), b'other run')

    def test_path_length_content_encoding_has_an_independent_literal_vector(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_bytes(b'x')
            literal = b'\x0a\0\0\0\0\0\0\0Cargo.toml\x01\0\0\0\0\0\0\0x'
            self.assertEqual(source_fingerprint(root), hashlib.sha256(literal).hexdigest())

    def test_source_changed_after_compilation_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'Cargo.toml'
            path.write_text('before')
            old = source_fingerprint(root)
            verify_source_identity(old, old, root)
            path.write_text('after')
            with self.assertRaises(RuntimeError):
                verify_source_identity(old, old, root)

    def test_stale_compiled_identity_is_rejected_even_if_disk_is_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'Cargo.toml').write_text('current')
            current = source_fingerprint(root)
            with self.assertRaises(RuntimeError):
                verify_source_identity(current, '0' * 64, root)


if __name__ == '__main__':
    unittest.main()
