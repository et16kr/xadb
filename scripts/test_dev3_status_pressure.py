import unittest
import tempfile
import json
from pathlib import Path

import dev3_status_pressure_native as native

ROOT = Path(__file__).resolve().parent.parent


class StatusPressureOracle(unittest.TestCase):
    def test_protocol_rejects_wrong_mode_pid_selector_and_extra_result(self):
        text = (f'\nrunning 1 test\ntest {native.SELECTOR} ... STATUS_PRESSURE_NATIVE_BEGIN mode=recover pid=42\n'
                'STATUS_PRESSURE_NATIVE_PASS mode=recover pid=42\nok\n'
                'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;\n')
        native.protocol(text, 'recover', 42)
        for bad in (text.replace('pid=42', 'pid=43'), text.replace('recover ', 'produce '),
                    text.replace('STATUS_PRESSURE_NATIVE_PASS', 'missing'),
                    text + '\ntest other ... ok\n', text + 'test result: failed\n'):
            with self.assertRaises(ValueError): native.protocol(bad, 'recover', 42)

    def test_distinct_producer_recovery_and_idempotent_files(self):
        before = native.producer(ROOT)
        after = native.recover(before, ROOT)
        self.assertEqual(native.recover(after, ROOT), after)
        self.assertEqual(before[native.model.WAL], after[native.model.WAL])
        for files, lsns, batch, generation in [(before, [0, 2, 9, 8], 6, 19), (after, [6, 2, 10, 8], 8, 25)]:
            self.assertEqual([native.model.number(files[native.model.DATA], i * 16384 + 64) for i in range(4)], lsns)
            control = files[f'd3-dwb-{files["DWB"][81]}']
            self.assertEqual((control[6], native.model.number(control, 32), native.model.number(control, 40)), (0, generation, batch))
        self.assertEqual(len(before[native.model.WAL]), 4096 + 3 * 59200 + 4 * 3040)
        bad = dict(before)
        raw = bytearray(bad[f'd3-dwb-{bad["DWB"][81]}']); raw[6] = 1
        bad[f'd3-dwb-{bad["DWB"][81]}'] = bytes(raw)
        with self.assertRaises(ValueError): native.recover(bad, ROOT)

    def test_exact_cold_actor_bank_retains_only_second_row(self):
        bank = native.retained_bank(ROOT)
        self.assertEqual(len(bank), 53248)
        self.assertFalse(any(bank[:1664]))
        self.assertEqual(bank[1664:3200], native.literal(ROOT, 'step2.descriptor'))
        self.assertEqual(native.model.number(bank, 1664 + 1544), 3)
        self.assertEqual(native.model.number(bank, 1664 + 1568), 2)
        self.assertEqual(native.model.number(bank, 1664 + 1576 + 32), 2)
        self.assertEqual(bank[1664 + 1624], 1)
        self.assertFalse(any(bank[1664 + 1625:]))

    def test_home_trace_rejects_omission_extra_wrong_hash_and_wrong_offset(self):
        expected = native.expected_home_writes(ROOT, 'recover')
        good = [['write', native.model.DATA, str(offset), '16384', f'16384:{native.model.sha(page)}'] for offset, page in expected]
        self.assertEqual(native.check_home_writes(good, expected), 2)
        for bad in (good[:1], good + good[:1], [good[0][:-1] + ['16384:bad'], good[1]],
                    [good[0][:2] + ['16384'] + good[0][3:], good[1]]):
            with self.assertRaises(ValueError): native.check_home_writes(bad, expected)
        with self.assertRaises(ValueError): native.check_home_writes(good, [])
        self.assertEqual(len(native.expected_home_writes(ROOT, 'produce')), 70)
        self.assertEqual(native.expected_home_writes(ROOT, 'reentry'), [])

    def test_ledger_reserves_all_four_pages_not_only_two_redo_writes(self):
        facts = {'complete_lsn': '10', 'remaining_wal': '6080', 'restored': '1',
                 'replay_topup': '65536', 'generation': '25', 'batch': '8'}
        native.check_ledger(facts)
        for key, bad in [('replay_topup', '32768'), ('complete_lsn', '9'), ('remaining_wal', '0'), ('restored', '0')]:
            with self.assertRaises(ValueError): native.check_ledger({**facts, key: bad})

    def test_resume_rejects_other_failures_completed_or_missing_processes(self):
        original = {'status': 'FAIL', 'error': 'FAIL_RECOVERED_LEDGER', 'not_run': ['reentry'],
                    'workers': [{'mode': 'produce', 'status': 'PASS', 'exit_code': 0},
                                {'mode': 'recover', 'status': 'FAIL', 'exit_code': 0}]}
        with tempfile.TemporaryDirectory() as path:
            folder = Path(path)
            for bad in [{**original, 'status': 'PASS'}, {**original, 'error': 'FAIL_WORKER'},
                        {**original, 'not_run': []}, {**original, 'workers': original['workers'][:1]}]:
                (folder / 'result.json').write_text(json.dumps(bad))
                with self.assertRaisesRegex(ValueError, 'FAIL_RESUME_BOUNDARY'):
                    native.resume_origin(ROOT, folder)

    def test_resume_snapshot_rejects_missing_changed_and_extra_files(self):
        with tempfile.TemporaryDirectory() as path:
            origin = Path(path) / 'origin'; origin.mkdir()
            file = origin / 'events.tsv'; file.write_bytes(b'original')
            m = file.stat()
            saved = {'root': str(origin), 'files': {'events.tsv': {'bytes': m.st_size,
                     'sha256': native.digest(file), 'device': m.st_dev, 'inode': m.st_ino, 'links': 1}}}
            snapshot = Path(path) / 'snapshot.json'; snapshot.write_text(json.dumps(saved))
            native.check_resume_snapshot(origin, snapshot)
            file.write_bytes(b'altered!')
            with self.assertRaisesRegex(ValueError, 'FAIL_RESUME_SNAPSHOT_DRIFT'):
                native.check_resume_snapshot(origin, snapshot)
            file.write_bytes(b'original'); extra = origin / 'extra'; extra.write_bytes(b'x')
            with self.assertRaisesRegex(ValueError, 'FAIL_RESUME_SNAPSHOT_DRIFT'):
                native.check_resume_snapshot(origin, snapshot)
            extra.unlink(); file.unlink()
            with self.assertRaisesRegex(ValueError, 'FAIL_RESUME_SNAPSHOT_DRIFT'):
                native.check_resume_snapshot(origin, snapshot)


if __name__ == '__main__':
    unittest.main()
