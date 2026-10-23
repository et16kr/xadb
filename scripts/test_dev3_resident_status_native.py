"""Known-bad new native protocols and full target/descriptor observations."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import dev3_resident_status_native as check

ROOT = Path(__file__).resolve().parent.parent


class ResidentNative(unittest.TestCase):
    def test_worker_identity_and_exact_execution_count(self):
        text = (f'OPERATIONAL_RECOVERY_NATIVE_BEGIN mode=produce pid=31\n'
                f'test {check.PRODUCER} ... ok\n'
                f'OPERATIONAL_RECOVERY_NATIVE_PASS mode=produce pid=31\n'
                'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 500 filtered out; finished in 0.00s\n')
        check.check_worker(text, check.PRODUCER, 'produce', 31)
        for wrong in (text.replace(check.PRODUCER, check.RECOVERY), text.replace('pid=31', 'pid=32'),
                      text.replace('1 passed', '2 passed'), text.replace('0 failed', '1 failed'),
                      text.replace('mode=produce', 'mode=recover'), text + f'test {check.PRODUCER} ... ok\n'):
            with self.assertRaises(Exception):
                check.check_worker(wrong, check.PRODUCER, 'produce', 31)

    def test_full_images_and_target_census_reject_known_bad(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            for actual, expected in [('allocator.bin', 'allocator.after'), ('descriptor.bin', 'terminal.descriptor'),
                                     *[(f'status-{n}.bin', f'status-{n}.after') for n in (1,2,3)]]:
                (folder / actual).write_bytes(check.literal(ROOT, expected))
            rows = {'mask': '14', 'initial': json.dumps([0,2,4,None] + [None]*60),
                    'final': json.dumps([6,2,4,6] + [None]*60),
                    'applied': json.dumps([6,0,0,1] + [0]*60), 'skipped': json.dumps([0,1,1,0] + [0]*60)}
            def write_rows():
                (folder / 'all-targets.txt').write_text(''.join(f'{key}={value}\n' for key,value in rows.items()))
            write_rows()
            with patch.object(check.prior, 'observation', return_value={'complete_lsn':'6','highwater':'3','replay_topup':'65536'}), patch.object(check.prior, 'check_census'):
                check.check_inspection(folder, ROOT, False)
                for name in ('allocator.bin','descriptor.bin','status-1.bin','status-2.bin','status-3.bin'):
                    path = folder / name
                    original = path.read_bytes()
                    bad = bytearray(original); bad[-1] ^= 1
                    path.write_bytes(bad)
                    with self.assertRaises(Exception):
                        check.check_inspection(folder, ROOT, False)
                    path.write_bytes(original)
                for key, bad in [('mask','6'), ('initial',json.dumps([0,2,4,0]+[None]*60)),
                                 ('applied',json.dumps([6,0,0,0]+[0]*60)), ('final',json.dumps([6,2,4,4]+[None]*60))]:
                    original = rows[key]; rows[key] = bad; write_rows()
                    with self.assertRaises(Exception):
                        check.check_inspection(folder, ROOT, False)
                    rows[key] = original; write_rows()
                (folder/'status-4.bin').write_bytes(check.literal(ROOT,'status-3.after'))
                with self.assertRaises(Exception):
                    check.check_inspection(folder, ROOT, False)

    def test_expected_history_keeps_dirty_homes_until_recovery(self):
        before = check.producer_files(ROOT)
        after = check.recover_files(ROOT, before)
        page = check.model.PAGE
        self.assertNotEqual(before[check.model.DATA][:page], check.literal(ROOT,'allocator.after'))
        self.assertEqual(before[check.model.DATA][3*page:4*page], bytes(page))
        self.assertEqual(after[check.model.DATA][:page], check.literal(ROOT,'allocator.after'))
        self.assertEqual(after[check.model.DATA][3*page:4*page], check.literal(ROOT,'status-3.after'))
        self.assertEqual(before[check.model.WAL], after[check.model.WAL])
        self.assertEqual(before[check.model.DATA][page:3*page], after[check.model.DATA][page:3*page])


    def test_retention_counts_failed_database_and_preserves_error(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp); db = output / 'database'; db.mkdir()
            retained = output / 'engine-tests'; retained.write_bytes(b'ELF')
            (db / 'data').write_bytes(b'12345'); (output / 'stdout.log').write_bytes(b'xy')
            report = dict(status='FAIL', error='FAIL_TIMEOUT', limits=dict(database_bytes=5, retained_database_bytes=5, logs_bytes=2))
            check.retain_inventory(output, db, retained, report)
            self.assertEqual(report['error'], 'FAIL_TIMEOUT')
            self.assertEqual(report['retention']['database_bytes'], 5)
            self.assertEqual(report['retention']['case_output_bytes'], 2)
            self.assertEqual(len(report['retention']['files']), 2)
            self.assertTrue(all(row['sha256'] and row['inode'] for row in report['retention']['files']))
            for limit in ('database_bytes', 'retained_database_bytes', 'logs_bytes'):
                old = report['limits'][limit]; report['limits'][limit] = 1
                check.retain_inventory(output, db, retained, report)
                self.assertEqual(report['error'], 'FAIL_RETAINED_SPACE')
                self.assertEqual(report['prior_error'], 'FAIL_TIMEOUT')
                self.assertEqual(report['retention']['status'], 'FAIL')
                report['limits'][limit] = old

    def test_build_log_hash_rejects_changed_diagnostics(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp); receipt = folder / 'build.json'
            stdout = folder / 'build.jsonl'; stderr = folder / 'build.stderr'
            stdout.write_bytes(b'compile'); stderr.write_bytes(b'diagnostic')
            build = dict(stdout_sha256=check.digest(stdout), stderr_sha256=check.digest(stderr))
            check.check_build_logs(receipt, build)
            for path in (stdout, stderr):
                original = path.read_bytes(); path.write_bytes(original + b'changed')
                with self.assertRaisesRegex(Exception, 'FAIL_BUILD_LOG_HASH'):
                    check.check_build_logs(receipt, build)
                path.write_bytes(original)

    def test_final_time_includes_inventory_and_keeps_prior_failure(self):
        report = dict(status='PASS', limits=dict(history_seconds=30, total_seconds=120))
        check.check_final_time(report, 30)
        self.assertEqual(report['status'], 'PASS')
        check.check_final_time(report, 30.001)
        self.assertEqual(report['status'], 'FAIL')
        self.assertEqual(report['error'], 'FAIL_FINAL_TIME_BUDGET')
        report = dict(status='FAIL', error='FAIL_TIMEOUT', limits=dict(history_seconds=30, total_seconds=20))
        check.check_final_time(report, 21)
        self.assertEqual(report['prior_error'], 'FAIL_TIMEOUT')
        self.assertEqual(report['elapsed_seconds'], 21)


if __name__ == '__main__':
    unittest.main()
