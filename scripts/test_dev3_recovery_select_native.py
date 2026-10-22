"""Known-bad selection/receipt/process assertions for only the new native runner."""
import json
from pathlib import Path
import tempfile
import unittest
import dev3_recovery_select_native as runner


class RecoveryNativeHarnessTests(unittest.TestCase):
    def test_empty_typo_duplicate_selection(self):
        for values in ([], ['typo'], ['NP11', 'NP11']):
            with self.subTest(values=values), self.assertRaises(ValueError): runner.selected(values)
        self.assertEqual(runner.selected(['NC01', 'NP11']), ['NP11', 'NC01'])

    def test_zero_wrong_duplicate_test_and_missing_mode(self):
        good = f'test {runner.SELECTOR} ... RECOVERY_SELECT_NATIVE_COMPLETE mode=select pid=123 outcome=selected\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n'
        runner.check_stdout(good, 'select', 123, 'selected')
        for bad in ('', good.replace(runner.SELECTOR, 'wrong::test'), good+good, good.replace('mode=select', 'mode=hold'), good.replace('1 passed', '0 passed')):
            with self.subTest(output=bad), self.assertRaises(ValueError): runner.check_stdout(bad, 'select', 123, 'selected')

    def test_timeout_and_failed_exit_are_not_pass(self):
        good = dict(timed_out=False, exit_code=0, elapsed_seconds=1)
        runner.check_process(good, 30)
        for field, value in (('timed_out', True), ('exit_code', 1), ('elapsed_seconds', 31), ('elapsed_seconds', -1)):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError): runner.check_process(dict(good, **{field:value}), 30)

    def test_corrupt_or_wrong_source_binary_receipt(self):
        root = Path('/frozen')
        good = dict(status='PASS', source_sha256='source', binary_sha256='binary', source_root=str(root), command=runner.BUILD_COMMAND, exit_code=0, errors=0)
        runner.check_build_receipt(good, root, 'source', 'binary')
        for field, value in (('status', 'FAIL'), ('source_sha256', 'other'), ('binary_sha256', 'other'), ('source_root', '/other'), ('command', ['wrong']), ('exit_code', 1), ('errors', 1)):
            with self.subTest(field=field), self.assertRaises(ValueError): runner.check_build_receipt(dict(good, **{field:value}), root, 'source', 'binary')

    def test_wrong_binary_format(self):
        with tempfile.TemporaryDirectory() as temp:
            binary = Path(temp)/'binary'
            binary.write_bytes(b'not an executable')
            with self.assertRaises(ValueError): runner.check_binary_format(binary)
            binary.write_bytes(b'\x7fELFmetadata')
            runner.check_binary_format(binary)

    def test_manifest_identity_and_missing_dependency(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); path = root/runner.INPUT; path.parent.mkdir(parents=True)
            path.write_text('{}\n')
            with self.assertRaises(ValueError): runner.validate_inputs(root)

    def test_resource_counts_and_home_read_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root/'resources.tsv').write_text('handles_peak\t1\nhandles_final\t0\nfd_peak\t3\nfd_final\t0\nmutations\t0\n')
            (root/'fd.tsv').write_text('open\t3\nfinal\t0\n')
            (root/'reads.tsv').write_text('d3-profile\t0\t4096\t4096\n')
            runner.check_resources(root, True)
            (root/'reads.tsv').write_text('d3-data-0000000100000001\t0\t16384\t16384\n')
            with self.assertRaises(ValueError): runner.check_resources(root, True)


if __name__ == '__main__': unittest.main()
