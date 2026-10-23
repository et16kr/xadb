#!/usr/bin/env python3
"""Residency-only checker selftests; synthetic diagnostics are not rustc evidence."""
from copy import deepcopy
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from check_dev0 import digest
import check_dev3_allocator_residency_borrows as c
import dev3_allocator_residency_native as n

ROOT = Path(__file__).resolve().parent.parent


def compiler_output(error=None, success=False):
    rows = [] if error is None else [{'reason': 'compiler-message', 'message': error}]
    rows.append({'reason': 'build-finished', 'success': success})
    return '\n'.join(map(json.dumps, rows))


def native_stdout(size=22111648):
    return ('running 1 test\ntest ' + n.SELECTOR
            + f' ... ALLOCATOR_RESIDENCY_NATIVE_COMPLETE profile=1 batches=2 reloads=2 effects=62 bytes={size} peak_files=3\n'
            'ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 10 filtered out; finished in 0.01s\n')


class AllocatorResidencyAcceptance(unittest.TestCase):
    def test_compiler_census_selection_and_budget(self):
        cases = c.inputs()
        self.assertEqual([row['name'] for row in cases], ['baseline', 'release', 'live_split'])
        self.assertEqual([row['name'] for row in c.select_cases(cases, ['live_split'])],
                         ['baseline', 'live_split'])
        self.assertEqual([row['name'] for row in c.select_cases(cases, ['release'])],
                         ['baseline', 'release'])
        for bad in ([], ['release', 'release'], ['unknown']):
            with self.assertRaises(ValueError):
                c.select_cases(cases, bad)
        self.assertEqual(c.remaining_budget(29), 1)
        for consumed in (30, 31):
            with self.assertRaises(RuntimeError):
                c.remaining_budget(consumed)

    def test_compiler_frozen_bytes_and_inventory_tampering(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'inputs'
            for corruption in ('changed', 'missing', 'extra', 'manifest'):
                shutil.copytree(c.INPUTS, target)
                if corruption == 'changed':
                    (target / 'live_split.rs').write_text('unrelated invalid Rust\n')
                elif corruption == 'missing':
                    (target / 'live_split.rs').unlink()
                elif corruption == 'extra':
                    (target / 'extra.rs').write_text('')
                else:
                    path = target / 'manifest.json'
                    manifest = json.loads(path.read_text())
                    manifest['cases'].append(manifest['cases'][-1])
                    path.write_text(json.dumps(manifest))
                with self.assertRaises((ValueError, FileNotFoundError)):
                    c.inputs(target)
                if corruption == 'manifest':
                    with self.assertRaises(ValueError):
                        c.inputs(target, digest(target / 'manifest.json'))
                shutil.rmtree(target)

    def test_compiler_requires_exact_e0499_and_primary_span(self):
        frozen = c.inputs()[-1]
        error = {'level': 'error', 'code': {'code': 'E0499'}, 'spans': [
            {'is_primary': True, 'file_name': c.PROBE, 'line_start': 8, 'line_end': 8}]}
        self.assertEqual(c.verify_diagnostics(compiler_output(error), 101, frozen)[0]['code'], 'E0499')
        for field, value in (('file_name', 'crates/engine/src/lib.rs'), ('line_start', 7),
                             ('line_end', 9), ('is_primary', False)):
            wrong = deepcopy(error)
            wrong['spans'][0][field] = value
            with self.assertRaises(ValueError):
                c.verify_diagnostics(compiler_output(wrong), 101, frozen)
        wrong = deepcopy(error)
        wrong['code']['code'] = 'E0432'
        with self.assertRaises(ValueError):
            c.verify_diagnostics(compiler_output(wrong), 101, frozen)
        for text, code in (('', 101), (compiler_output(), 101),
                           (compiler_output(error) + '\n' + compiler_output(error), 101),
                           (compiler_output(success=True), 0)):
            with self.assertRaises(ValueError):
                c.verify_diagnostics(text, code, frozen)

    def test_compiler_positive_needs_actual_success(self):
        for frozen in c.inputs()[:2]:
            self.assertEqual(c.verify_diagnostics(compiler_output(success=True), 0, frozen), [])
            for text, code in (('', 0), (compiler_output(), 101),
                               (compiler_output(success=True), 101),
                               (compiler_output(success=True) + '\n' + compiler_output(success=True), 0)):
                with self.assertRaises(ValueError):
                    c.verify_diagnostics(text, code, frozen)

    def test_native_census_and_exact_history_marker(self):
        self.assertEqual(n.select_cases(), list(n.CASE_NAMES))
        for bad in ([], [n.CASE_NAMES[0]] * 2, ['unrelated-old-native']):
            with self.assertRaises(ValueError):
                n.select_cases(bad)
        text = native_stdout()
        self.assertEqual(n.check_stdout(text, 22111648)['reloads'], 2)
        for old, new in (('profile=1', 'profile=2'), ('batches=2', 'batches=1'),
                         ('reloads=2', 'reloads=1'), ('effects=62', 'effects=31'),
                         ('bytes=22111648', 'bytes=0'), ('peak_files=3', 'peak_files=4'),
                         ('1 passed;', '2 passed;'), (n.SELECTOR, 'wrong::selector')):
            with self.assertRaises(ValueError):
                n.check_stdout(text.replace(old, new), 22111648)
        for bad in (text + text, text + 'ALLOCATOR_RESIDENCY_NATIVE malformed\n'):
            with self.assertRaises(ValueError):
                n.check_stdout(bad, 22111648)

    def test_native_exact_reads_include_actual_home_once(self):
        expected = n.expected_reload_reads(ROOT)
        self.assertEqual(len(expected), 6)
        self.assertEqual(expected[-1], ['read', n.HOME, '0', '16384', '16384'])
        n.check_reload_reads(expected, ROOT)
        for index in range(len(expected)):
            for column in range(5):
                damaged = deepcopy(expected)
                damaged[index][column] = 'wrong'
                with self.assertRaises(ValueError):
                    n.check_reload_reads(damaged, ROOT)
        for bad in ([], expected[:-1], expected + [expected[-1]], expected[::-1]):
            with self.assertRaises(ValueError):
                n.check_reload_reads(bad, ROOT)

    def test_native_stamps_original_reservation_and_control_are_exact(self):
        for batch in (1, 2):
            reserved = ['4294967297', '1', '2'] if batch == 1 else ['none'] * 3
            expected = [str(batch), str(batch + 1), str(batch), str(batch), str(batch), '1',
                        'true' if batch == 1 else 'false', '1' if batch == 1 else '0',
                        '1', str(batch), str(4096 + 21200 * batch), str(3 * batch + 1), *reserved]
            n.check_reload(expected, batch)
            for index in range(len(expected)):
                damaged = expected.copy()
                damaged[index] = 'wrong'
                with self.assertRaises(ValueError):
                    n.check_reload(damaged, batch)
            with self.assertRaises(ValueError):
                n.check_reload(expected[:-1], batch)

    def test_native_full_page_and_descriptor_reject_stale_or_malformed_bytes(self):
        for part in ('reserve', 'cancel'):
            page = (ROOT / n.FIXTURES / f'p1-n1-{part}-home.after').read_bytes()
            descriptor = (ROOT / n.DIRTY / f'p1-n1-{part}.descriptor').read_bytes()
            n.check_payloads(page, descriptor, ROOT, part)
            for offset in (0, 24, 64, 256, len(page) - 1):
                damaged = bytearray(page)
                damaged[offset] ^= 1
                with self.assertRaisesRegex(ValueError, 'FAIL_RAM_PAGE'):
                    n.check_payloads(damaged, descriptor, ROOT, part)
            for offset in (0, 24, 72, 968, len(descriptor) - 1):
                damaged = bytearray(descriptor)
                damaged[offset] ^= 1
                with self.assertRaisesRegex(ValueError, 'FAIL_ACTIVE_DESCRIPTOR'):
                    n.check_payloads(page, damaged, ROOT, part)
            for short_page, short_descriptor in ((page[:-1], descriptor), (page, descriptor[:-1])):
                with self.assertRaises(ValueError):
                    n.check_payloads(short_page, short_descriptor, ROOT, part)
        reserve = (ROOT / n.FIXTURES / 'p1-n1-reserve-home.after').read_bytes()
        cancel = (ROOT / n.DIRTY / 'p1-n1-cancel.descriptor').read_bytes()
        with self.assertRaises(ValueError):
            n.check_payloads(reserve, cancel, ROOT, 'reserve')

    def test_native_reload_preserves_every_inventory_field(self):
        old = {'home': [16384, 7, 8, 1, 'a' * 64], 'wal': [25296, 7, 9, 1, 'b' * 64]}
        n.check_reload_inventory(old, deepcopy(old))
        for name in old:
            for index in range(5):
                changed = deepcopy(old)
                changed[name][index] = 'changed'
                with self.assertRaisesRegex(ValueError, 'FAIL_RELOAD_DISK_MUTATION'):
                    n.check_reload_inventory(old, changed)
        with self.assertRaises(ValueError):
            n.check_reload_inventory(old, {**old, 'unexpected': old['home']})

    def test_native_pinned_inputs_and_build_receipt_cannot_substitute_source(self):
        inputs = n.frozen_inputs(ROOT)
        self.assertEqual(len(inputs), 19)
        source, binary_hash = 'a' * 64, 'b' * 64
        root, binary = Path('/frozen/source'), Path('/frozen/engine-worker')
        command = ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--all-features', '--no-run',
                   '--locked', '--offline', '--message-format=json']
        receipt = {'status': 'PASS', 'source_root': str(root), 'binary': str(binary),
                   'source_sha256': source, 'binary_sha256': binary_hash, 'command': command, 'exit_code': 0}
        n.verify_build(receipt, root, binary, source, binary_hash)
        for key in receipt:
            wrong = deepcopy(receipt)
            wrong[key] = 'wrong'
            with self.assertRaises(ValueError):
                n.verify_build(wrong, root, binary, source, binary_hash)


if __name__ == '__main__':
    unittest.main()
