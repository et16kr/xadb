"""New guard runtime receipt checks; does not run any product test."""
import unittest
from pathlib import Path

import check_dev3_concurrency_runtime as checker


class RuntimeReceiptTests(unittest.TestCase):
    def test_narrow_selection_rejects_empty_duplicate_unknown(self):
        self.assertEqual(checker.select(['GA06']), ['GA06'])
        self.assertEqual(checker.select(None), list(checker.SELECTORS))
        for selected in ([], ['GA06', 'GA06'], ['GA08']):
            with self.assertRaises(ValueError):
                checker.select(selected)

    def test_fixture_census_rejects_malformed_or_missing_inputs(self):
        rows = [f'{group}-01\tcase_{group}\tNONE' for group in checker.SELECTORS]
        self.assertEqual(len(checker.cases('\n'.join(rows))), 7)
        for broken in (rows[:-1], rows + [rows[0]], rows + ['GA08-01\tx\tNONE'],
                       rows + ['GA01-02\tx\tDURABLE'], rows + ['bad']):
            with self.assertRaises(ValueError):
                checker.cases('\n'.join(broken))

    def test_receipt_requires_exact_success_and_complete_case_markers(self):
        expected = ['GA06-01\tstate_poison\tNONE', 'GA06-02\tversion_poison\tNONE']
        output = ('running 1 test\ntest ' + checker.PREFIX + checker.SELECTORS['GA06'] + ' ... '
                  'GUARD_CASE ' + expected[0] + '\nGUARD_CASE ' + expected[1]
                  + '\nGUARD_COMPLETE GA06 2\nok\n\ntest result: ok. 1 passed; '
                  '0 failed; 0 ignored; 0 measured; 255 filtered out; finished in 0.01s\n')
        process = {'exit_code': 0, 'timed_out': False}
        self.assertEqual(checker.evaluate(output, process, 'GA06', expected)['case_count'], 2)
        broken_outputs = [output.replace(expected[1], expected[0]),
                          output.replace('GUARD_CASE ' + expected[1] + '\n', ''),
                          output.replace('NONE', 'SIMULATED', 1),
                          output.replace('GUARD_COMPLETE GA06 2', 'GUARD_COMPLETE GA06 1'),
                          output.replace('1 passed', '0 passed'),
                          output.replace(checker.SELECTORS['GA06'], checker.SELECTORS['GA05']),
                          output + 'GUARD_COMPLETE GA06 2\n']
        for broken in broken_outputs:
            with self.assertRaises(ValueError):
                checker.evaluate(broken, process, 'GA06', expected)
        for failed in ({'exit_code': 1}, {'exit_code': 0, 'timed_out': True}):
            with self.assertRaises(ValueError):
                checker.evaluate(output, failed, 'GA06', expected)

    def test_build_receipt_binds_source_binary_and_no_run_command(self):
        root, binary = Path('/retained/source'), Path('/retained/test-binary')
        receipt = {'status': 'PASS', 'source_root': str(root), 'binary': str(binary),
                   'source_sha256': 'source', 'binary_sha256': 'binary',
                   'command': checker.BUILD_COMMAND, 'exit_code': 0}
        checker.verify_build(receipt, root, binary, 'source', 'binary')
        for key, value in (('source_sha256', 'wrong'), ('binary_sha256', 'wrong'),
                           ('source_root', '/elsewhere'), ('binary', '/wrong'),
                           ('command', ['cargo', 'test']), ('exit_code', 1), ('status', 'FAIL')):
            with self.assertRaises(ValueError):
                checker.verify_build({**receipt, key: value}, root, binary, 'source', 'binary')


if __name__ == '__main__':
    unittest.main()
