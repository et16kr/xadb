"""New compiler input/selection boundary tests; no database or compiler runs."""
import json
import tempfile
import unittest
from pathlib import Path
import check_dev3_operational_status_borrows as check
from check_dev0 import digest


class CompilerInputs(unittest.TestCase):
    def test_frozen_inventory(self):
        cases = check.inputs()
        self.assertEqual(len(cases), 6)
        self.assertEqual(sum(row['expected_code'] is not None for row in cases), 4)

    def test_narrow_selection_preserves_omissions(self):
        rows = check.select_cases(check.inputs(), ['workspace'])
        self.assertEqual([row['name'] for row in rows], ['baseline', 'workspace'])
        for names in ([], ['workspace', 'workspace'], ['unregistered']):
            with self.assertRaises(ValueError):
                check.select_cases(check.inputs(), names)

    def test_changed_program_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for source in check.INPUTS.iterdir():
                (target / source.name).write_bytes(source.read_bytes())
            (target / 'workspace.rs').write_text('fn fake() {}\n')
            with self.assertRaisesRegex(ValueError, 'source changed'):
                check.inputs(target)

    def test_diagnostic_expectation_cannot_be_relaxed(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for source in check.INPUTS.iterdir():
                (target / source.name).write_bytes(source.read_bytes())
            path = target / 'manifest.json'
            manifest = json.loads(path.read_text())
            manifest['cases'][2]['expected_code'] = None
            manifest['cases'][2]['primary_line'] = None
            path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'diagnostic identity'):
                check.inputs(target, digest(path))


if __name__ == '__main__':
    unittest.main()
