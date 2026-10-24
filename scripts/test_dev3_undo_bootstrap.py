"""New plan3 checker routes only; synthetic negatives, no product tests."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import check_dev3_undo_role_borrows as compiler
import dev3_undo_bootstrap as native
from check_dev0 import digest, source_fingerprint
from dev3_storage_model import expected_trace


class UndoBootstrapCheckers(unittest.TestCase):
    def test_native_total_budget(self):
        with patch.object(native.time, 'monotonic', return_value=129):
            self.assertEqual(native.remaining(100), 1)
        for now in (130, 131):
            with patch.object(native.time, 'monotonic', return_value=now):
                with self.assertRaisesRegex(ValueError, 'TOTAL_TIME_BUDGET'):
                    native.remaining(100)

    def test_compiler_inputs_and_selected_inventory(self):
        rows = compiler.inputs()
        self.assertEqual(len(rows), 5)
        self.assertEqual([r['name'] for r in compiler.select_cases(rows, ['role_confusion'])], ['baseline', 'role_confusion'])
        for selection in ([], ['positive', 'positive'], ['unknown']):
            with self.assertRaises(ValueError):
                compiler.select_cases(rows, selection)
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'compiler'
            shutil.copytree(compiler.INPUTS, path)
            (path / 'role_confusion.rs').write_text('changed')
            with self.assertRaisesRegex(ValueError, 'source changed'):
                compiler.inputs(path)

    def test_compiler_diagnostics_are_exact(self):
        rows = {r['name']: r for r in compiler.inputs()}
        for name in ('private_constructor', 'owner_drop', 'role_confusion'):
            row = rows[name]
            span = {'is_primary': True, 'file_name': compiler.PROBE,
                    'line_start': row['primary_line'], 'line_end': row['primary_line']}
            error = {'reason': 'compiler-message', 'message': {'level': 'error',
                     'code': {'code': row['expected_code']}, 'spans': [span]}}
            finish = {'reason': 'build-finished', 'success': False}
            encode = lambda es: '\n'.join(map(json.dumps, es))
            compiler.verify_diagnostics(encode([error, finish]), 101, row)
            for code, messages in [(0, [error, finish]), (1, [error, finish]),
                                   (101, [finish]), (101, [error, error, finish]),
                                   (101, [error])]:
                with self.assertRaises(ValueError):
                    compiler.verify_diagnostics(encode(messages), code, row)
            span['file_name'] = 'unrelated.rs'
            with self.assertRaises(ValueError):
                compiler.verify_diagnostics(encode([error, finish]), 101, row)
        compiler.verify_diagnostics(json.dumps({'reason': 'build-finished', 'success': True}), 0, rows['positive'])
        with self.assertRaises(ValueError):
            compiler.verify_diagnostics('', 0, rows['positive'])

    def test_native_whole_files_effects_and_roles(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            db = path / 'database'
            db.mkdir()
            database = bytes([0x93]) * 16
            files = native.expected_files(database)
            for name, raw in {'LOCK': b'', **files}.items():
                (db / name).write_bytes(raw)
            role = f'P1\t1\t{database.hex()}\t1\t3\t1\t8\t0\t3\t350\t3\t0\t3\t0\n'
            (path / 'role.tsv').write_text(role)
            plan = json.loads((native.ROOT / 'tests/fixtures/dev3-storage-session/effects.json').read_text())
            events = expected_trace(plan, files, 1)
            effects = ''.join(f'{e["op"]}\t{e["name"] or "-"}\t{e["offset"]}\t{e["length"]}\t{e["sha256"]}\n' for e in events)
            (path / 'effects.tsv').write_text(effects)
            self.assertEqual(native.check_native(path)['effects'], 350)
            payload = db / 'd3-data-0000000100000001'
            raw = payload.read_bytes()
            payload.write_bytes(raw[:-1] + bytes([raw[-1] ^ 1]))
            with self.assertRaisesRegex(ValueError, 'FILE_BYTES'):
                native.check_native(path)
            payload.write_bytes(raw)
            (db / 'unexpected').write_bytes(b'')
            with self.assertRaisesRegex(ValueError, 'FILE_BYTES'):
                native.check_native(path)
            (db / 'unexpected').unlink()
            (path / 'effects.tsv').write_text('\n'.join(effects.splitlines()[:-1]) + '\n')
            with self.assertRaisesRegex(ValueError, 'EFFECTS'):
                native.check_native(path)
            (path / 'effects.tsv').write_text(effects)
            (path / 'role.tsv').write_text(role.replace('\t8\t', '\t9\t'))
            with self.assertRaisesRegex(ValueError, 'ROLE_OR_HANDLES'):
                native.check_native(path)

    def test_native_exact_execution(self):
        output = f'test {native.SELECTOR} ... ok\nXADB_UNDO_BOOTSTRAP_NATIVE P1\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n'
        native.check_runtime(output)
        for altered in [output.replace(native.SELECTOR, 'wrong'),
                        output.replace('1 passed', '0 passed'),
                        output.replace('0 ignored', '1 ignored'),
                        output + 'XADB_UNDO_BOOTSTRAP_NATIVE P1\n']:
            with self.assertRaises(ValueError):
                native.check_runtime(altered)

    def test_native_build_binding(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            (path / 'source').mkdir()
            (path / 'source/Cargo.toml').write_text('source')
            (path / 'engine-tests').write_bytes(b'not executed')
            row = {'exit': 0, 'source_sha256': source_fingerprint(path / 'source'),
                   'binary_sha256': digest(path / 'engine-tests')}
            (path / 'build-receipt.json').write_text(json.dumps(row))
            self.assertEqual(native.build_binding(path), row)
            (path / 'engine-tests').write_bytes(b'wrong executable')
            with self.assertRaisesRegex(ValueError, 'BUILD_BINDING'):
                native.build_binding(path)
            (path / 'engine-tests').write_bytes(b'not executed')
            (path / 'source/Cargo.toml').write_text('source drift')
            with self.assertRaisesRegex(ValueError, 'BUILD_BINDING'):
                native.build_binding(path)

    def test_native_literal_binding(self):
        native.check_inputs()
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)
            shutil.copytree(native.ROOT / native.INPUT, path / native.INPUT)
            manifest = json.loads((path / native.INPUT / 'manifest.json').read_text())
            for name in manifest['inputs']:
                destination = path / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(native.ROOT / name, destination)
            native.check_inputs(path)
            (path / native.INPUT / 'effects.tsv').write_text('changed')
            with self.assertRaisesRegex(ValueError, 'INPUT_IDENTITY'):
                native.check_inputs(path)


if __name__ == '__main__':
    unittest.main()
