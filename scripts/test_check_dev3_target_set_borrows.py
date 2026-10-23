"""Adversarial boundaries of the new compiler selection and diagnostic contract."""
import json
import tempfile
from unittest.mock import patch
import unittest
from pathlib import Path
import check_dev3_target_set_borrows as check


class TargetSetCompiler(unittest.TestCase):
    def test_invalid_selection_and_program_tampering(self):
        rows = check.inputs()
        self.assertEqual([row['name'] for row in check.select(rows, ['capacity_65'])], ['baseline', 'capacity_65'])
        for names in ([], ['release', 'release'], ['unknown']):
            with self.assertRaises(ValueError):
                check.select(rows, names)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for source in check.INPUTS.iterdir():
                (target / source.name).write_bytes(source.read_bytes())
            (target / 'capacity_65.rs').write_text('fn fake() {}')
            with self.assertRaisesRegex(ValueError, 'program changed'):
                check.inputs(target)
            bindings = check.checker_bindings()
            for relative, binding in bindings.items():
                copied = target / relative
                copied.parent.mkdir(parents=True, exist_ok=True)
                copied.write_bytes(Path(binding['path']).read_bytes())
            self.assertEqual(set(check.verify_checker_bindings(bindings, target)), set(bindings))
            changed = next(iter(bindings))
            (target / changed).write_text('# changed captured checker')
            with self.assertRaisesRegex(ValueError, 'executed checker differs'):
                check.verify_checker_bindings(bindings, target)
            (target / changed).write_bytes(Path(bindings[changed]['path']).read_bytes())
            with patch.object(check, 'digest', side_effect=lambda p: 'changed' if str(p) == bindings[changed]['path'] else __import__('hashlib').sha256(p.read_bytes()).hexdigest()):
                with self.assertRaisesRegex(ValueError, 'executed checker differs'):
                    check.verify_checker_bindings(bindings, target)


    def test_wrong_capacity_error_or_instantiation_is_not_pass(self):
        row = next(x for x in check.inputs() if x['name'] == 'capacity_65')
        def output(path, rendered, code='E0080'):
            return '\n'.join(json.dumps(x) for x in [
                {'reason': 'compiler-message', 'message': {'level': 'error', 'code': {'code': code}, 'spans': [{'is_primary': True, 'file_name': path, 'line_start': 1, 'line_end': 1}], 'rendered': rendered}},
                {'reason': 'build-finished', 'success': False}])
        for path, rendered, code in [(check.PROBE, 'reserve_target_capacity::<65>', 'E0080'), (check.ASSERT_SOURCE, 'reserve_target_capacity::<0>', 'E0080'), (check.ASSERT_SOURCE, 'reserve_target_capacity::<0> ... unrelated::<65>', 'E0080'), (check.ASSERT_SOURCE, 'reserve_target_capacity::<65>', 'E0603')]:
            with self.assertRaises(ValueError):
                check.verify(output(path, rendered, code), 101, row, 1)
        for level in ('warning', 'note'):
            foreign = json.dumps({'reason': 'compiler-message', 'message': {
                'level': level, 'rendered': 'reserve_target_capacity::<65>'}})
            forged = output(check.ASSERT_SOURCE, 'reserve_target_capacity::<0>') + '\n' + foreign
            with self.assertRaisesRegex(ValueError, 'concrete production instantiation'):
                check.verify(forged, 101, row, 1)
        self.assertEqual(check.verify(output(check.ASSERT_SOURCE, 'reserve_target_capacity::<65>'), 101, row, 1)[0]['code'], 'E0080')

    def test_missing_or_weakened_assertion_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory)
            path = source / check.ASSERT_SOURCE
            path.parent.mkdir(parents=True)
            for text in ['', 'const { assert!(N <= 64) }; '+check.ASSERT_MARKER]:
                path.write_text(text)
                with self.assertRaises(ValueError):
                    check.assertion_line(source)


if __name__ == '__main__':
    unittest.main()
