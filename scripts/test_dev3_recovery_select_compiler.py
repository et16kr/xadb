"""Known-bad frozen ownership diagnostics; never compile or run product code."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
import dev3_recovery_select_compiler as runner


class RecoveryCompilerHarnessTests(unittest.TestCase):
    def test_empty_typo_duplicate_selection(self):
        cases = [{'name': name} for name in runner.CASE_NAMES]
        for values in ([], ['typo'], ['positive', 'positive']):
            with self.subTest(values=values), self.assertRaises(ValueError): runner.select_cases(cases, values)
        self.assertEqual([r['name'] for r in runner.select_cases(cases, ['owner_move'])], ['baseline', 'owner_move'])

    def test_frozen_manifest_and_program_identity(self):
        cases = runner.inputs()
        self.assertEqual(len(cases), 6)
        with tempfile.TemporaryDirectory() as temp:
            base = Path(temp)
            for path in runner.INPUTS.iterdir(): (base/path.name).write_bytes(path.read_bytes())
            (base/'owner_move.rs').write_text('wrong\n')
            with self.assertRaises(ValueError): runner.inputs(base)

    def test_exact_negative_completion_path_line_and_code(self):
        frozen = next(row for row in runner.inputs() if row['name'] == 'owner_move')
        error = dict(reason='compiler-message', message=dict(level='error', code=dict(code='E0382'), spans=[dict(is_primary=True, file_name=runner.PROBE, line_start=6, line_end=6)]))
        finish = dict(reason='build-finished', success=False)
        good = '\n'.join(map(json.dumps, (error, finish)))
        runner.verify_diagnostics(good, 101, frozen)
        alternatives = []
        for key, value in (('file_name','wrong.rs'), ('line_start',5), ('line_end',7)):
            bad = copy.deepcopy(error); bad['message']['spans'][0][key] = value; alternatives.append((bad,finish))
        bad = copy.deepcopy(error); bad['message']['code']['code'] = 'E0425'; alternatives.append((bad,finish))
        alternatives.extend(((error,), (error,error,finish), (finish,)))
        for rows in alternatives:
            with self.subTest(rows=rows), self.assertRaises(ValueError): runner.verify_diagnostics('\n'.join(map(json.dumps, rows)), 101, frozen)
        with self.assertRaises(ValueError): runner.verify_diagnostics(good, 0, frozen)

    def test_positive_cannot_accept_failure_and_budget_exhaustion(self):
        positive = next(row for row in runner.inputs() if row['name'] == 'positive')
        runner.verify_diagnostics(json.dumps(dict(reason='build-finished', success=True)), 0, positive)
        with self.assertRaises(ValueError): runner.verify_diagnostics(json.dumps(dict(reason='build-finished', success=False)), 101, positive)
        with self.assertRaises(RuntimeError): runner.remaining_budget(60)


if __name__ == '__main__': unittest.main()
