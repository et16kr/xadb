"""Independent diagnostic/prior-receipt rejection literals; never invoke Cargo."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

import dev3_commit_continuation_compiler as compiler


# Literal expected evidence is independent of the manifest/parser under test.
PROBE = 'crates/engine/src/storage/commit_continuation_compiler_probe.rs'
EXPECTED = {'name': 'double_consume', 'expected_code': 'E0382', 'primary_spans': [
    {'line_start': 6, 'line_end': 6, 'column_start': 10, 'column_end': 15}]}
ERROR = {'reason': 'compiler-message', 'message': {
    'level': 'error', 'code': {'code': 'E0382'}, 'message': 'use of moved value: stage',
    'spans': [{'file_name': PROBE, 'is_primary': True,
               'line_start': 6, 'line_end': 6, 'column_start': 10, 'column_end': 15}]}}
REJECTED = {'reason': 'build-finished', 'success': False}
SUCCEEDED = {'reason': 'build-finished', 'success': True}


def transcript(*rows):
    return '\n'.join(json.dumps(row) for row in rows) + '\n'


def prior_pair():
    # These are hand-written receipt semantics, not output from the runner.
    initial = {
        'schema': 'XADB_COMMIT_CONTINUATION_COMPILER_RUN_V1',
        'status': 'RUNNING', 'finalized': False, 'cases': [],
        'source_sha256': 'a' * 64, 'manifest_sha256': 'b' * 64,
        'budget': {'case_seconds': 30},
        'selected': ['baseline', 'double_consume'], 'prior': [],
        'tool_bindings': {'scripts/example.py': 'c' * 64},
        'unit_root': '/independent/compiler-history', 'product_execution': False,
    }
    result = copy.deepcopy(initial)
    result.update(status='PASS', finalized=True, not_run=[], compiler_seconds=2.0,
                  elapsed_seconds=3.0, log_bytes=1000, artifact_bytes=2000,
                  cases=[{'name': 'baseline', 'status': 'PASS',
                          'diagnostic_completed': True, 'evidence_validated': True},
                         {'name': 'double_consume', 'status': 'PASS',
                          'diagnostic_completed': True, 'evidence_validated': True}])
    return initial, result


class CommitCompilerChecks(unittest.TestCase):
    def test_success_requires_exact_completed_success_without_errors(self):
        expected = {'expected_code': None, 'primary_spans': []}
        self.assertEqual(compiler.verify(transcript(SUCCEEDED), 0, expected), [])
        for output, code in [('', 0), (transcript(REJECTED), 0),
                             (transcript(SUCCEEDED), 101),
                             (transcript(SUCCEEDED, SUCCEEDED), 0),
                             (transcript(ERROR, SUCCEEDED), 0)]:
            with self.subTest(output=output, code=code), self.assertRaises(ValueError):
                compiler.verify(output, code, expected)

    def test_negative_requires_one_exact_error_code(self):
        self.assertEqual(compiler.verify(transcript(ERROR, REJECTED), 101, EXPECTED)[0]['code'],
                         'E0382')
        wrong_code = copy.deepcopy(ERROR)
        wrong_code['message']['code']['code'] = 'E0499'
        no_code = copy.deepcopy(ERROR)
        no_code['message']['code'] = None
        for rows in [(wrong_code, REJECTED), (no_code, REJECTED),
                     (REJECTED,), (ERROR, ERROR, REJECTED)]:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                compiler.verify(transcript(*rows), 101, EXPECTED)

    def test_negative_requires_exact_primary_file_and_full_span(self):
        # A real secondary note is allowed and cannot replace the primary span.
        secondary = copy.deepcopy(ERROR)
        secondary['message']['spans'].append({
            'file_name': 'crates/engine/src/storage/dirty_frame/commit.rs',
            'is_primary': False, 'line_start': 110, 'line_end': 110,
            'column_start': 1, 'column_end': 20})
        compiler.verify(transcript(secondary, REJECTED), 101, EXPECTED)
        for key, value in [('file_name', 'crates/engine/src/unrelated.rs'),
                           ('line_start', 5), ('line_end', 7),
                           ('column_start', 9), ('column_end', 16),
                           ('is_primary', False)]:
            wrong = copy.deepcopy(ERROR)
            wrong['message']['spans'][0][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                compiler.verify(transcript(wrong, REJECTED), 101, EXPECTED)
        duplicate = copy.deepcopy(ERROR)
        duplicate['message']['spans'].append(copy.deepcopy(duplicate['message']['spans'][0]))
        with self.assertRaises(ValueError):
            compiler.verify(transcript(duplicate, REJECTED), 101, EXPECTED)

    def test_rejection_is_not_pass_without_exact_cargo_completion(self):
        for output, code in [(transcript(ERROR), 101),
                             (transcript(ERROR, REJECTED, REJECTED), 101),
                             (transcript(ERROR, SUCCEEDED), 101),
                             (transcript(ERROR, REJECTED), 0),
                             (transcript(ERROR, REJECTED), -9),
                             (transcript(ERROR, {'reason': 'artifact', 'success': False}), 101)]:
            with self.subTest(output=output, code=code), self.assertRaises(ValueError):
                compiler.verify(output, code, EXPECTED)

    def test_completed_selection_and_partial_pass_receipts_are_rejected(self):
        self.assertEqual(compiler.select_cases(['double_consume'], {'baseline'}),
                         ['double_consume'])
        for selected, completed in [(['baseline'], {'baseline'}),
                                    (['double_consume'], {'baseline', 'double_consume'}),
                                    (['positive'], set()),
                                    (['positive', 'positive'], {'baseline'}),
                                    ([], {'baseline'})]:
            with self.subTest(selected=selected), self.assertRaises(ValueError):
                compiler.select_cases(selected, completed)
        initial, valid = prior_pair()
        compiler.validate_prior_summary(initial, valid)
        for field, value in [('diagnostic_completed', False),
                             ('evidence_validated', False), ('status', 'FAIL')]:
            wrong = copy.deepcopy(valid)
            wrong['cases'][1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                compiler.validate_prior_summary(initial, wrong)
        for cases in [valid['cases'][:1], list(reversed(valid['cases'])),
                      [valid['cases'][0], valid['cases'][0]]]:
            wrong = copy.deepcopy(valid)
            wrong['cases'] = cases
            with self.subTest(cases=cases), self.assertRaises(ValueError):
                compiler.validate_prior_summary(initial, wrong)

    def test_prior_discovery_requires_matching_original_run_and_final_inventory(self):
        with tempfile.TemporaryDirectory() as temporary:
            unit = Path(temporary)
            directory = unit / 'attempt-0'
            directory.mkdir()
            initial, valid = prior_pair()
            run = directory / 'run.json'
            result = directory / 'result.json'
            # An interrupted attempt cannot disappear when a new output is used.
            run.write_text(json.dumps(initial))
            with self.assertRaises(ValueError):
                compiler.prior_runs(unit)
            result.write_text(json.dumps(valid))
            self.assertEqual(compiler.prior_runs(unit), [result])
            for field, value in [('source_sha256', 'd' * 64),
                                 ('selected', ['baseline']), ('status', 'FAIL'),
                                 ('finalized', False), ('not_run', ['double_consume']),
                                 ('compiler_seconds', 4.0)]:
                wrong = copy.deepcopy(valid)
                wrong[field] = value
                result.write_text(json.dumps(wrong))
                with self.subTest(field=field), self.assertRaises(ValueError):
                    compiler.prior_runs(unit)
            result.write_text(json.dumps(valid))
            changed_initial = copy.deepcopy(initial)
            changed_initial['selected'] = ['positive']
            run.write_text(json.dumps(changed_initial))
            with self.assertRaises(ValueError):
                compiler.prior_runs(unit)


if __name__ == '__main__':
    unittest.main()
