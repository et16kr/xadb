"""New subset-checker checks; synthetic files are not product execution evidence."""
import hashlib
from pathlib import Path
import struct
import tempfile
import unittest

import dev3_status_bootstrap as check


class StatusBootstrapChecks(unittest.TestCase):
    def test_exact_runtime_census(self):
        good = f'test {check.SELECTORS["SB01"]} ...\n' + '\n'.join(check.CASE_MARKER + f'SB01-{i:02}' for i in range(1, 5))
        good += '\nXADB_STATUS_BOOTSTRAP_RESOURCE_VARIANT normal_admit\nXADB_STATUS_BOOTSTRAP_RESOURCE_VARIANT alignment_reject'
        good += '\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 99 filtered out\n'
        self.assertEqual(len(check.check_runtime(good, 'SB01')['cases']), 4)
        for bad in (good.replace('SB01-04', 'SB01-03'),
                    good.replace(check.CASE_MARKER + 'SB01-04\n', ''),
                    good.replace('1 passed', '0 passed'),
                    good + check.CASE_MARKER + 'SB01-04\n',
                    good + check.VARIANT_MARKER + 'SB03-05 healthy\n',
                    good.replace(check.SELECTORS['SB01'], check.SELECTORS['SB02']),
                    good.replace('RESOURCE_VARIANT alignment_reject', 'RESOURCE_VARIANT normal_admit')):
            with self.assertRaises(ValueError):
                check.check_runtime(bad, 'SB01')

    def test_subvariants_and_native_marker(self):
        rows = [f'test {check.SELECTORS["SB03"]} ...'] + [check.CASE_MARKER + f'SB03-{i:02}' for i in range(1, 6)]
        rows += [check.VARIANT_MARKER + 'SB03-05 ' + name for name in ('healthy', 'workspace_poison', 'plan1')]
        rows += ['test result: ok. 1 passed; 0 failed;']
        good = '\n'.join(rows)
        check.check_runtime(good, 'SB03')
        with self.assertRaises(ValueError):
            check.check_runtime(good.replace('workspace_poison', 'healthy'), 'SB03')
        native = f'test {check.SELECTORS["P1"]} ...\nXADB_STATUS_BOOTSTRAP_NATIVE P1\ntest result: ok. 1 passed; 0 failed;\n'
        check.check_runtime(native, 'P1')
        for bad in (native.replace('P1', 'P2'), native.replace('XADB_STATUS_BOOTSTRAP_NATIVE P1\n', '')):
            with self.assertRaises(ValueError):
                check.check_runtime(bad, 'P1')
        parser = f'test {check.SELECTORS["PARSER"]} ... ok\ntest result: ok. 1 passed; 0 failed;\n'
        check.check_runtime(parser, 'PARSER')
        with self.assertRaisesRegex(ValueError, 'FAIL_EXACT_TEST_NAME'):
            check.check_runtime(parser.replace(check.SELECTORS['PARSER'], check.SELECTORS['SB01']), 'PARSER')

    def test_native_full_bytes_effects_and_handles(self):
        # Hand assembly from frozen literals, independent of native_objects().
        old = check.ROOT / 'crates/engine/tests/fixtures/dev3-storage-session/literal'
        def literal(name):
            return (old / f'profile1-{name}.bin').read_bytes()
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            db = out / 'database'
            db.mkdir()
            files = {name: literal(name) for name in ('d3-profile', 'd3-wal-00000001', 'WRITER', 'FILEMAP')}
            files['d3-bootstrap'] = (check.FIXTURES / 'profile1-d3-bootstrap.bin').read_bytes()
            files['LOCK'] = b''
            files['d3-data-0000000100000001'] = literal('allocation') + bytes(1048576 - 16384)
            files['d3-filemap-0'] = literal('root') + bytes(16384 - 320) + literal('chunk') + bytes(2113536 - 16384 - 256)
            files['d3-filemap-1'] = bytes(2113536)
            for name, data in files.items():
                (db / name).write_bytes(data)
            role = 'P1\t1\t0102030405060708090a0b0c0d0e0f10\t1\t2\t1\t9\t0\t2\t350\t3\t0\t3\t0\n'
            effects = (check.FIXTURES / 'effects.tsv').read_text()
            (out / 'role.tsv').write_text(role)
            (out / 'effects.tsv').write_text(effects)
            self.assertEqual(check.check_native(out)['effects'], 350)
            wrong = bytearray(files['d3-bootstrap'])
            struct.pack_into('<Q', wrong, 120, 1)
            wrong[4064:] = hashlib.sha256(wrong[:4064]).digest()
            (db / 'd3-bootstrap').write_bytes(wrong)
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_FILE_BYTES'):
                check.check_native(out)
            (db / 'd3-bootstrap').write_bytes(files['d3-bootstrap'])
            (out / 'effects.tsv').write_text('\n'.join(effects.splitlines()[:-1]) + '\n')
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_EFFECTS'):
                check.check_native(out)
            (out / 'effects.tsv').write_text(effects)
            (out / 'role.tsv').write_text(role.replace('\t3\t0\t3\t0\n', '\t3\t1\t3\t0\n'))
            with self.assertRaisesRegex(ValueError, 'FAIL_ROLE_OR_HANDLES'):
                check.check_native(out)
            (out / 'role.tsv').write_text(role)
            (db / 'unexpected').write_bytes(b'')
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_FILE_BYTES'):
                check.check_native(out)


if __name__ == '__main__':
    unittest.main()
