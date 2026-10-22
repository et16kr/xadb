"""Known-bad selection/receipt/process assertions for only the new native runner."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import dev3_recovery_prefix_native as runner
import dev3_recovery_prefix_long_native as long_runner


class RecoveryPrefixNativeHarnessTests(unittest.TestCase):
    def test_empty_typo_duplicate_selection(self):
        for values in ([], ['typo'], ['PN01', 'PN01']):
            with self.subTest(values=values), self.assertRaises(ValueError): runner.selected(values)
        self.assertEqual(runner.selected(['PC10', 'PN01']), ['PN01', 'PC10'])
        self.assertEqual(long_runner.selected(['PL01']),['PL01'])
        for values in (None,[],['PN01'],['PL01','PL01']):
            with self.subTest(long_values=values),self.assertRaises(ValueError):long_runner.selected(values)
        self.assertEqual(len(runner.CASES),31)

    def test_zero_wrong_duplicate_test_and_missing_mode(self):
        good = f'test {runner.SELECTOR} ... RECOVERY_PREFIX_NATIVE_BEGIN mode=analyze pid=123\nRECOVERY_PREFIX_NATIVE_CASE mode=analyze pid=123 outcome=analyzed PASS\nRECOVERY_PREFIX_NATIVE_COMPLETE count=1\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n'
        runner.check_stdout(good, 'analyze', 123, 'analyzed')
        for bad in ('', good.replace(runner.SELECTOR, 'wrong::test'), good+good, good.replace('mode=analyze', 'mode=hold'), good.replace('1 passed', '0 passed')):
            with self.subTest(output=bad), self.assertRaises(ValueError): runner.check_stdout(bad, 'analyze', 123, 'analyzed')
        long_output=good.replace(runner.SELECTOR,long_runner.SELECTOR)
        runner.check_stdout(long_output,'analyze',123,'analyzed',long_runner.SELECTOR)
        with self.assertRaises(ValueError):runner.check_stdout(good,'analyze',123,'analyzed',long_runner.SELECTOR)

    def test_timeout_and_failed_exit_are_not_pass(self):
        good = dict(timed_out=False, exit_code=0, elapsed_seconds=1)
        runner.check_process(good)
        for field, value in (('timed_out', True), ('exit_code', 1), ('elapsed_seconds', 31), ('elapsed_seconds', -1)):
            with self.subTest(field=field, value=value), self.assertRaises(ValueError): runner.check_process(dict(good, **{field:value}))
        runner.check_total_budget(300)
        with self.assertRaises(ValueError):runner.check_total_budget(300.001)
        for failure in (ValueError('expired before wait'),KeyboardInterrupt()):
            child=mock.Mock(pid=123,returncode=None)
            child.poll.side_effect=[None,-9]
            process={}
            with mock.patch.object(runner.os,'killpg') as kill:
                with self.assertRaises(type(failure)):
                    runner.wait_child(child,lambda:(_ for _ in ()).throw(failure),process)
                kill.assert_called_once_with(123,9)
                child.wait.assert_called_once_with()
            self.assertTrue(process['aborted_by_controller'])

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
            long_path=root/long_runner.INPUT;long_path.write_text('{}\n')
            with self.assertRaises(ValueError):long_runner.validate_inputs(root)

    def test_resource_counts_and_home_read_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); db=root/'database';db.mkdir()
            (db/'d3-profile').write_bytes(b'a');(db/'DWB').write_bytes(bytes(4096))
            (root/'resources.tsv').write_text('handles_peak\t1\nhandles_final\t0\nfd_peak\t3\nfd_final\t0\nmutations\t0\ncut_hits\t0\ncut_ordinal\t0\n')
            (root/'fd.tsv').write_text('open\t3\nfinal\t0\n')
            census='outer\t100\nbootstrap\t200\nreplay\t300\nstream\t400\npeak\t500\nwriter\t32\nfile\t32\ncaller\t262144\n'
            (root/'source-census.tsv').write_text(census)
            (root/'reads.tsv').write_text('d3-profile\t0\t1\t1\t'+runner.sha(b'a')+'\n')
            runner.check_resources(root, db, True)
            for bad in (census.replace('stream\t400\n',''),census.replace('peak\t500','peak\t499'),census.replace('outer\t100','outer\t65536').replace('peak\t500','peak\t65936'),census.replace('file\t32','file\t16385')):
                (root/'source-census.tsv').write_text(bad)
                with self.assertRaises(ValueError):runner.check_resources(root,db,True)
            (root/'source-census.tsv').write_text(census)
            (root/'reads.tsv').write_text('d3-data-0000000100000001\t0\t1\t1\t'+runner.sha(b'a')+'\n')
            with self.assertRaises(ValueError): runner.check_resources(root, db, True)
            (root/'reads.tsv').write_text('d3-profile\t0\t1\t1\twrong\n')
            with self.assertRaises(ValueError): runner.check_resources(root, db, True)
            (root/'producer-error.txt').write_text('CallerMustResolve Selection Publish Namespace\n')
            (root/'events.tsv').write_text('publish\t19\n')
            runner.check_producer_cut(root,'pending',dict(cut_hits=1,cut_ordinal=19))
            with self.assertRaises(ValueError):runner.check_producer_cut(root,'pending',dict(cut_hits=0,cut_ordinal=19))
            (root/'events.tsv').write_text('publish\t18\n')
            with self.assertRaises(ValueError):runner.check_producer_cut(root,'pending',dict(cut_hits=1,cut_ordinal=19))
            (db/long_runner.WAL).write_bytes(b'header')
            with self.assertRaises(ValueError):long_runner.check_files(db,{'d3-profile':b'a','DWB':bytes(4096),long_runner.WAL:b'header'})
            (root/'allocator.bin').write_bytes(b'a');(root/'descriptor.bin').write_bytes(b'd')
            meta=(db/long_runner.WAL).stat()
            (root/'observation.tsv').write_text(f'work_high_water\t1000\nsource_device\t{meta.st_dev}\nsource_inode\t{meta.st_ino}\n')
            long_runner.check_inspection(root,db,b'a',b'd',dict(work_high_water='1000'))
            with self.assertRaises(ValueError):long_runner.check_inspection(root,db,b'corrupt',b'd',dict(work_high_water='1000'))
            with self.assertRaises(ValueError):long_runner.check_inspection(root,db,b'a',b'd',dict(work_high_water='999'))


if __name__ == '__main__': unittest.main()
