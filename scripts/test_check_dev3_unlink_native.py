"""Independent negative vectors for UNL-N's evidence checks (no engine run)."""
import csv
from pathlib import Path
import tempfile
import unittest

import check_dev3_unlink_native as check

WAL = 'd3-wal-00000001'
HOME = 'd3-data-0000000100000001'


def event(op, name, offset=0, length=0, outcome='Ok(())'):
    return ['0', op, name, str(offset), str(length), outcome]


def transferred(n):
    return f'OK:{n}:' + 'a' * 64


class UnlinkEvidenceTests(unittest.TestCase):
    def setUp(self):
        path = Path(__file__).resolve().parents[1] / 'tests/fixtures/dev3-08-unlink/native-inventory.tsv'
        with path.open() as source:
            self.inventory = {row['case']: row for row in csv.DictReader(source, delimiter='\t')}

    def cut(self, number):
        want = check.expected_of(self.inventory[f'UNL-N0{number}-R'])
        receipt = dict(shape='root', cut_at=want['cut'], q='5', wal_bytes='100',
                       armed=want['stop'][1])
        return want, receipt

    def test_hook_uses_completed_read_bytes(self):
        for number in (1, 2):
            want, receipt = self.cut(number)
            arm = event('arm', receipt['armed'], outcome='HOOK')
            # A requested 100 bytes with only 1 byte returned has not read the prefix.
            for n in (0, 1, 99, 101):
                with self.subTest(number=number, returned=n), self.assertRaises(AssertionError):
                    check.check_cut(want, receipt, [arm, event('read', WAL, 0, 100, transferred(n))], WAL)
            # Legal partial reads can collectively cover the prefix.
            check.check_cut(want, receipt, [arm, event('read', WAL, 0, 100, transferred(40)),
                                           event('read', WAL, 40, 60, transferred(60))], WAL)

    def test_wal_cut_requires_successful_sync_of_the_same_wal(self):
        want, receipt = self.cut(4)
        arm = event('arm', receipt['armed'], outcome='After')
        cut = event('cut', receipt['armed'], outcome=f'sync:{WAL}')
        sync = event('sync', WAL)
        check.check_cut(want, receipt, [arm, sync, cut], WAL)
        for bad in (event('sync', WAL, outcome='ATTEMPT'),
                    event('sync', WAL, outcome='Err(io)'), event('sync', 'd3-wal-other')):
            with self.subTest(bad=bad), self.assertRaises(AssertionError):
                check.check_cut(want, receipt, [arm, bad, cut], WAL)

    @staticmethod
    def crash_events():
        return [event('sync', 'd3-rebase-00000002'),
                event('publish', 'd3-current-stage', outcome='CURRENT:Ok(())'),
                event('publish', 'd3-current-stage', outcome='CURRENT:Ok(())'),
                event('publish', 'd3-current-stage', outcome='CURRENT:Ok(())'),
                event('dirsync', 'namespace'),
                event('cut', 'UNL_CURRENT_NEW_NEW', outcome='dirsync:namespace')]

    def test_current_cut_rejects_failed_barrier_effects(self):
        receipt = dict(armed='UNL_CURRENT_NEW_NEW')
        check.check_crash(receipt, self.crash_events())
        for index in range(5):
            effects = self.crash_events()
            effects[index][5] = 'CURRENT:Err(io)' if 1 <= index <= 3 else 'Err(io)'
            with self.subTest(index=index), self.assertRaises(AssertionError):
                check.check_crash(receipt, effects)

    def test_current_cut_requires_current_selector(self):
        effects = self.crash_events()
        effects[-3][5] = 'DWB:Ok(())'
        with self.assertRaises(AssertionError):
            check.check_crash(dict(armed='UNL_CURRENT_NEW_NEW'), effects)

    @staticmethod
    def repair_events():
        return [event('read', 'd3-dwb-images', 0, check.PAGE_BYTES, transferred(check.PAGE_BYTES)),
                event('write', HOME, 9 * check.PAGE_BYTES, check.PAGE_BYTES, transferred(check.PAGE_BYTES)),
                event('publish', 'd3-current-stage', outcome='CURRENT:Ok(())')]

    def test_repair_requires_complete_home_write(self):
        want, _ = self.cut(5)
        produced = dict(anchor_page='9')
        check.check_repair(want, 'recover', self.repair_events(), produced)
        for n in (0, 1, check.PAGE_BYTES - 1):
            effects = self.repair_events()
            effects[1][5] = transferred(n)
            with self.subTest(returned=n), self.assertRaises(AssertionError):
                check.check_repair(want, 'recover', effects, produced)
        # write_all may legally complete the same page with two syscalls.
        effects = self.repair_events()
        effects[1][5] = transferred(1)
        effects.insert(2, event('write', HOME, 9 * check.PAGE_BYTES + 1,
                                check.PAGE_BYTES - 1, transferred(check.PAGE_BYTES - 1)))
        check.check_repair(want, 'recover', effects, produced)

    def test_repair_rejects_empty_dwb_read(self):
        want, _ = self.cut(5)
        effects = self.repair_events()
        effects[0][5] = transferred(0)
        with self.assertRaises(AssertionError):
            check.check_repair(want, 'recover', effects, dict(anchor_page='9'))

    def test_repair_rejects_gap_and_other_file_completion(self):
        want, _ = self.cut(5)
        for name, offset in ((HOME, 9 * check.PAGE_BYTES + 2),
                             ('d3-data-other', 9 * check.PAGE_BYTES + 1)):
            effects = self.repair_events()
            effects[1][5] = transferred(1)
            effects.insert(2, event('write', name, offset, check.PAGE_BYTES - 1,
                                    transferred(check.PAGE_BYTES - 1)))
            with self.subTest(name=name, offset=offset), self.assertRaises(AssertionError):
                check.check_repair(want, 'recover', effects, dict(anchor_page='9'))

    def home_cut(self, page):
        want, receipt = self.cut(5)
        receipt.update(anchor_page='9', flushed_page='9', lsn='50',
                       wal_sha256=check.digest(b'p' * 100))
        frame = bytearray(want['frame'])
        frame[:4] = b'XWF1'
        for offset, size, value in ((12, 4, len(frame)), (56, 8, 51),
                                    (166, 2, 4), (170, 2, 4)):
            frame[offset:offset + size] = value.to_bytes(size, 'little')
        effects = [event('arm', receipt['armed']), event('sync', 'd3-dwb-images'),
                   event('write', HOME, 9 * check.PAGE_BYTES, check.PAGE_BYTES, 'ATTEMPT'),
                   event('write', HOME, 9 * check.PAGE_BYTES, 1, 'FIRST_BYTE:Ok(1)'),
                   event('cut', receipt['armed'], outcome=f'write:{HOME}')]
        with tempfile.TemporaryDirectory() as directory:
            db = Path(directory)
            (db / WAL).write_bytes(b'p' * 100 + frame)
            (db / HOME).write_bytes(bytes(9 * check.PAGE_BYTES) + page)
            return check.check_produce(want, receipt, effects, db)

    def test_home_cut_rejects_checksum_valid_or_truncated_page(self):
        # XDP1 has invariant magic: writing its first byte to an already
        # valid page may leave the page intact. A cut label is not damage.
        body = b'XDP1' + bytes(check.PAGE_BYTES - 32 - 4)
        valid = body + bytes.fromhex(check.digest(body))
        for page in (valid, valid[:-1], b'X'):
            with self.subTest(length=len(page)), self.assertRaises(AssertionError):
                self.home_cut(page)

    def test_home_cut_accepts_checksum_invalid_page(self):
        body = b'XDP1' + bytes(check.PAGE_BYTES - 32 - 4)
        page = bytearray(body + bytes.fromhex(check.digest(body)))
        page[64] = 1  # New pageLSN under the old checksum.
        self.home_cut(page)

    def test_unchanged_wal_and_ready_negative_vectors(self):
        import tempfile
        want, _ = self.cut(3)
        receipt = dict(wal_bytes='6', wal_sha256=check.digest(b'prefix'))
        with tempfile.TemporaryDirectory() as directory:
            db = Path(directory)
            (db / WAL).write_bytes(b'prefixX')
            check.check_wal(want, receipt, db, WAL)
            for contents in (b'prefix', b'prefixXX', b'prefixY', b'PREFIXX'):
                (db / WAL).write_bytes(contents)
                with self.subTest(contents=contents), self.assertRaises(AssertionError):
                    check.check_wal(want, receipt, db, WAL)
        want, _ = self.cut(4)
        produced = dict(emptied='10', root_page='12', lsn='50')
        receipt = dict(result='ready', handles='0', q_ready='5', route='5', level='0',
                       listed='10,12', high_water='51', rows='5', ack_q='6')
        check.check_ready(want, 'recover', receipt, produced)
        for field, value in dict(listed='10', high_water='50', rows='8', q_ready='6', handles='1').items():
            with self.subTest(field=field), self.assertRaises(AssertionError):
                check.check_ready(want, 'recover', dict(receipt, **{field: value}), produced)


if __name__ == '__main__':
    unittest.main()
