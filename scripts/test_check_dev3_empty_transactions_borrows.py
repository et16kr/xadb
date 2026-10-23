"""Reject forged selection, source binding and empty handoff lifetime diagnostics."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import check_dev3_empty_transactions_borrows as check


class EmptyHandoffCompiler(unittest.TestCase):
    def test_inventory_selection_and_source_binding(self):
        rows = check.inputs()
        self.assertEqual([r['name'] for r in check.select(rows, ['result_bank_alias'])],
                         ['baseline', 'result_bank_alias'])
        for choice in ([], ['bank_release', 'bank_release'], ['unknown']):
            with self.assertRaises(ValueError):
                check.select(rows, choice)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for path in check.INPUTS.iterdir():
                (target / path.name).write_bytes(path.read_bytes())
            (target / 'result_bank_alias.rs').write_text('fn unrelated() {}')
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

    def test_live_seal_inventory_and_diagnostic(self):
        rows = check.select(check.inputs(), ['live_sequence', 'sealed_install'])
        self.assertEqual([r['name'] for r in rows], ['baseline', 'live_sequence', 'sealed_install'])
        row = rows[-1]
        def messages(code='E0451', line=5):
            return [
                {'reason': 'compiler-message', 'message': {'level': 'error', 'code': {'code': code},
                 'spans': [{'is_primary': True, 'file_name': check.PROBE,
                            'line_start': line, 'line_end': line,
                            'column_start': start, 'column_end': end}
                           for start, end in [(43, 47), (48, 53), (54, 65), (66, 71)]],
                 'rendered': 'private seal'}},
                {'reason': 'build-finished', 'success': False}]
        def output(items):
            return '\n'.join(json.dumps(item) for item in items)
        self.assertEqual(len(check.verify(output(messages()), 101, row)[0]['primary_spans']), 4)
        negatives = [messages('E0603'), messages(line=6), [], messages() + [messages()[0]],
                     messages() + [messages()[1]], messages()[:-1]]
        for field, value in [('file_name', 'foreign.rs'), ('column_start', 42),
                             ('column_end', 48), ('line_end', 6), ('is_primary', False)]:
            bad = messages()
            bad[0]['message']['spans'][0][field] = value
            negatives.append(bad)
        for count in [0, 1, 3, 5]:
            bad = messages()
            bad[0]['message']['spans'] = (bad[0]['message']['spans'] * 2)[:count]
            negatives.append(bad)
        bad = messages()
        bad[-1]['success'] = True
        negatives.append(bad)
        for data, code in [(items, 101) for items in negatives] + [(messages(), 0)]:
            with self.assertRaises(ValueError):
                check.verify(output(data), code, row)

    def test_foreign_error_success_or_location_is_not_lifetime_evidence(self):
        row = next(r for r in check.inputs() if r['name'] == 'result_bank_alias')
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
