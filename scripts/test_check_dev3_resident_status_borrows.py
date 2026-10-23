"""Reject forged selection, source binding and resident lifetime diagnostics."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import check_dev3_resident_status_borrows as check


class ResidentCompiler(unittest.TestCase):
    def test_inventory_selection_and_source_binding(self):
        rows = check.inputs()
        self.assertEqual([r['name'] for r in check.select(rows, ['failed_result_reborrow'])],
                         ['baseline', 'failed_result_reborrow'])
        for choice in ([], ['release', 'release'], ['unknown']):
            with self.assertRaises(ValueError):
                check.select(rows, choice)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for path in check.INPUTS.iterdir():
                (target / path.name).write_bytes(path.read_bytes())
            (target / 'failed_result_reborrow.rs').write_text('fn unrelated() {}')
            with self.assertRaisesRegex(ValueError, 'program changed'):
                check.inputs(target)
            bindings = check.checker_bindings()
            for relative, binding in bindings.items():
                dest = target / relative
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(Path(binding['path']).read_bytes())
            check.verify_checker_bindings(bindings, target)
            first = next(iter(bindings))
            (target / first).write_text('# altered captured checker')
            with self.assertRaisesRegex(ValueError, 'executed checker differs'):
                check.verify_checker_bindings(bindings, target)
            (target / first).write_bytes(Path(bindings[first]['path']).read_bytes())
            with patch.object(check, 'digest', side_effect=lambda p: 'changed' if str(p) == bindings[first]['path'] else __import__('hashlib').sha256(p.read_bytes()).hexdigest()):
                with self.assertRaisesRegex(ValueError, 'executed checker differs'):
                    check.verify_checker_bindings(bindings, target)

    def test_foreign_error_success_or_location_is_not_lifetime_evidence(self):
        row = next(r for r in check.inputs() if r['name'] == 'failed_result_reborrow')
        def output(code, path, line, success=False):
            return '\n'.join(json.dumps(item) for item in [
                {'reason': 'compiler-message', 'message': {'level': 'error', 'code': {'code': code},
                    'spans': [{'is_primary': True, 'file_name': path, 'line_start': line, 'line_end': line}],
                    'rendered': 'admission lifetime diagnostic'}},
                {'reason': 'build-finished', 'success': success}])
        good = output('E0499', check.PROBE, row['primary_line'])
        self.assertEqual(check.verify(good, 101, row)[0]['code'], 'E0499')
        for data, exit_code in [(good, 0), (output('E0603', check.PROBE, row['primary_line']), 101),
                                (output('E0499', 'elsewhere.rs', row['primary_line']), 101),
                                (output('E0499', check.PROBE, row['primary_line'] + 1), 101),
                                (output('E0499', check.PROBE, row['primary_line'], True), 101),
                                ('', 101)]:
            with self.assertRaises(ValueError):
                check.verify(data, exit_code, row)


if __name__ == '__main__':
    unittest.main()
