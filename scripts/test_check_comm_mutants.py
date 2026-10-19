"""Checker must distinguish semantic kills from builds, crashes and absent tests."""
from pathlib import Path
import tempfile
import unittest
from check_comm_mutants import MUTANTS, apply_edits, check_execution
from comm_campaign import write_json


class MutantCheckerTests(unittest.TestCase):
    def test_only_an_executed_named_assertion_failure_counts_as_killed(self):
        output = 'running 1 test\ntest witness ... FAILED\nthread x panicked at test.rs:1\nCOMM_MUTANT expected: violated contract\ntest result: FAILED. 0 passed; 1 failed; 0 ignored;\n'
        check_execution(output, 101, 'witness', mutant=True, marker='COMM_MUTANT expected:')
        for text, code in [(output, -9), (output, 0), (output.replace('witness', 'unrelated'), 101),
                           (output.replace('running 1 test', 'running 0 tests'), 101),
                           (output.replace('panicked at ', 'aborted '), 101),
                           ('error: compilation failed', 101),
                           (output.replace('COMM_MUTANT expected:', 'unrelated fixture panic:'), 101)]:
            with self.assertRaises(ValueError):
                check_execution(text, code, 'witness', mutant=True, marker='COMM_MUTANT expected:')

    def test_baseline_must_execute_and_pass_the_same_test(self):
        output = 'running 1 test\ntest witness ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        check_execution(output, 0, 'witness')
        for text, code in [(output, 101), (output.replace('witness', 'other'), 0), (output.replace(' ... ok', ' ... ignored'), 0)]:
            with self.assertRaises(ValueError):
                check_execution(text, code, 'witness')

    def test_ambiguous_or_missing_site_is_not_a_kill_and_restores_earlier_edits(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            path = source / 'production.rs'
            path.write_text('first\nambiguous ambiguous\n')
            mutant = {'id': 'ambiguous', 'edits': [('production.rs', 'first', 'changed'), ('production.rs', 'ambiguous', 'bad')]}
            with self.assertRaises(ValueError):
                apply_edits(source, mutant)
            self.assertEqual(path.read_text(), 'first\nambiguous ambiguous\n')
        self.assertEqual(len(MUTANTS), 11)
        self.assertEqual(len({m['id'] for m in MUTANTS}), 11)

    def test_immutable_receipts_cannot_overwrite_existing_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'evidence.json'
            write_json(path, {'status': 'FAIL'})
            original = path.read_bytes()
            with self.assertRaises(FileExistsError):
                write_json(path, {'status': 'PASS'})
            self.assertEqual(path.read_bytes(), original)
            progress = Path(directory) / 'progress.json'
            write_json(progress, {'iteration': 1}, replace=True)
            write_json(progress, {'iteration': 2}, replace=True)
            self.assertIn('2', progress.read_text())


if __name__ == '__main__':
    unittest.main()
