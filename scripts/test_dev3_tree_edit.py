"""Narrow integration checks for the new leaf-edit inventory and loan probes."""
import json
import unittest

from check_dev3_leaf import LEAF_EDIT_TESTS, check_named_tests, check_leaf_edit_output
from check_dev3_tree_edit_borrows import KINDS, PROBE, probe_source
from check_dev3_buffer_borrows import check_diagnostics


class TreeEditChecks(unittest.TestCase):
    def test_layout_interleaving_preserves_all_required_test_results(self):
        prefix = 'storage::tree_page::edit::tests::'
        witness = 'XADB_LEAF_EDIT_LAYOUT view=128 edit=24 plan=104 result=104 scratch=16208\n'
        lines = [f'test {prefix}{name} ... ok' for name in sorted(LEAF_EDIT_TESTS)]
        output = '\n'.join(lines + ['test result: ok. 5 passed; 0 failed; 0 ignored;'])
        interleaved = output.replace('leaf_edit_maximum_reference_scratch_is_bounded ... ',
                                     'leaf_edit_maximum_reference_scratch_is_bounded ... ' + witness)
        self.assertEqual(check_leaf_edit_output(interleaved, 0), sorted(LEAF_EDIT_TESTS))
        self.assertEqual(check_leaf_edit_output(output + '\n' + witness, 0), sorted(LEAF_EDIT_TESTS))
        for wrong in [output, interleaved + witness, interleaved.replace('16208', '16209'),
                      interleaved.replace(' ... ok', ' ... ignored', 1)]:
            with self.assertRaises(ValueError):
                check_leaf_edit_output(wrong, 0)

    def test_inventory_rejects_missing_duplicate_ignored_and_failed_results(self):
        prefix = 'storage::tree_page::edit::tests::'
        self.assertEqual(len(LEAF_EDIT_TESTS), 5)
        lines = [f'test {prefix}{name} ... ok' for name in sorted(LEAF_EDIT_TESTS)]
        summary = 'test result: ok. 5 passed; 0 failed; 0 ignored;'
        output = '\n'.join(lines + [summary])
        self.assertEqual(check_named_tests(output, 0, prefix, LEAF_EDIT_TESTS), sorted(LEAF_EDIT_TESTS))
        for wrong, code in [(output, 1), ('\n'.join(lines[1:] + [summary]), 0),
                            (output + '\n' + lines[0], 0),
                            (output.replace(' ... ok', ' ... ignored', 1), 0),
                            (output.replace(' ... ok', ' ... FAILED', 1), 0)]:
            with self.assertRaises(ValueError):
                check_named_tests(wrong, code, prefix, LEAF_EDIT_TESTS)

    def test_new_probe_span_and_diagnostic_binding(self):
        for kind, expected_code in KINDS.items():
            positive, marker = probe_source(kind, True)
            self.assertIsNone(marker)
            self.assertIn('drop(plan);', positive)
            negative, line = probe_source(kind, False)
            self.assertNotIn('drop(plan);', negative)
            self.assertIn('// expected diagnostic', negative.splitlines()[line - 1])
            message = {'reason': 'compiler-message', 'message': {
                'level': 'error', 'code': {'code': expected_code},
                'spans': [{'is_primary': True, 'file_name': PROBE,
                           'line_start': line, 'line_end': line}]}}
            finish = {'reason': 'build-finished', 'success': False}
            output = '\n'.join(map(json.dumps, [message, finish]))
            check_diagnostics(output, 101, expected_code, line, probe_path=PROBE)
            for changed in [output.replace(expected_code, 'E9999'),
                            output.replace(PROBE, 'unrelated.rs'),
                            json.dumps(message), output + '\n' + json.dumps(message)]:
                with self.assertRaises(ValueError):
                    check_diagnostics(changed, 101, expected_code, line, probe_path=PROBE)


if __name__ == '__main__':
    unittest.main()
