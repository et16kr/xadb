"""Known-bad receipts for the new allocation checker; no product tests run here."""
from copy import deepcopy
import unittest

from dev3_allocation_native import (case_plan, check_census, check_files, check_worker,
                                   expected_events, sha, WAL)


class AllocationNativeCheckerTests(unittest.TestCase):
    def baseline(self):
        return {'summary': ['PASS', '1', 'cancel', 'write-half', '1', '3', 'source'],
                'peak': '3', 'markers': ['XADB_ALLOCATION_NATIVE_DONE'],
                'events': expected_events('cancel', 'write-half'),
                'error': ['Sync', 'CallerMustResolve', 'Write', 'CallerMustResolve',
                          'IoWriteUnknown', 'Other', 'write outcome requires caller resolution'],
                'fence': ['true', 'true', 'true', '4', '21259936']}

    def test_worker_negative_receipts(self):
        valid = self.baseline()
        check_worker(valid, 1, 'cancel', 'write-half', 'source')
        changes = [
            ('summary', 6, 'stale-source', 'FAIL_WORKER_IDENTITY'),
            ('summary', 4, '0', 'FAIL_WORKER_IDENTITY'),
            ('markers', 0, 'WRONG', 'FAIL_WORKER_MARKER'),
            ('error', 1, 'Unchanged', 'FAIL_ORIGINAL_CAUSE'),
            ('error', 2, 'Sync', 'FAIL_ORIGINAL_CAUSE'),
            ('error', 4, 'IoRead', 'FAIL_ORIGINAL_CAUSE'),
            ('error', 5, 'PermissionDenied', 'FAIL_ORIGINAL_CAUSE'),
            ('fence', 0, 'false', 'FAIL_COMMON_FENCE'),
            ('fence', 1, 'false', 'FAIL_COMMON_FENCE'),
            ('fence', 2, 'false', 'FAIL_COMMON_FENCE'),
            ('fence', 3, '3', 'FAIL_COMMON_FENCE'),
            ('fence', 4, '0', 'FAIL_COMMON_FENCE'),
        ]
        for field, index, replacement, code in changes:
            with self.subTest(field=field, index=index):
                bad = deepcopy(valid)
                bad[field][index] = replacement
                with self.assertRaisesRegex(ValueError, code):
                    check_worker(bad, 1, 'cancel', 'write-half', 'source')
        bad = deepcopy(valid)
        bad['events'][0][3] = '22656'
        with self.assertRaisesRegex(ValueError, 'FAIL_FAULT_PREFIX'):
            check_worker(bad, 1, 'cancel', 'write-half', 'source')

    def test_census_rejects_missing_and_duplicate(self):
        check_census(case_plan())
        for keys in (case_plan()[:-1], case_plan() + [case_plan()[0]], case_plan()[1:] + [case_plan()[1]]):
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_CENSUS'):
                check_census(keys)

    def test_actual_wal_home_and_dwb_identity(self):
        objects = {WAL: b'header', 'home': b'home', 'DWB': b'dwb'}
        before = {name: [len(data), 1, i + 1, 1, sha(data)] for i, (name, data) in enumerate(objects.items())}
        after = deepcopy(before)
        after[WAL][0], after[WAL][4] = 12, sha(b'headerprefix')
        check_files(before, after, after, objects, b'headerprefix')
        for name, index, replacement, code in [
                ('home', 4, 'corrupt', 'FAIL_HOME_OR_DWB_CHANGED'),
                ('DWB', 4, 'corrupt', 'FAIL_HOME_OR_DWB_CHANGED'),
                (WAL, 2, 999, 'FAIL_WAL_REPLACED'),
                (WAL, 4, sha(b'header'), 'FAIL_WAL_PREFIX')]:
            bad = deepcopy(after)
            bad[name][index] = replacement
            with self.assertRaisesRegex(ValueError, code):
                check_files(before, bad, bad, objects, b'headerprefix')
        with self.assertRaisesRegex(ValueError, 'FAIL_EXTERNAL_IMAGE'):
            check_files(before, before, after, objects, b'headerprefix')


if __name__ == '__main__':
    unittest.main()
