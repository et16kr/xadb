"""New dirty native receipt rejection tests; no product execution."""
from copy import deepcopy
import unittest
from dev3_dirty_allocation_native import CASES, check_census, check_worker, expected_events


class DirtyNativeChecks(unittest.TestCase):
    def valid(self, kind='cancel-sync-after'):
        cancel = kind == 'cancel-sync-after'
        parts = ('reserve',) if kind == 'reserve-write-half' else ('reserve', 'cancel')
        return {'summary': ['PASS', kind, str(int(kind != 'normal')), '3', 'source'], 'peak': '3',
                'markers': ['XADB_DIRTY_NATIVE_DONE'], 'events': expected_events(kind),
                'dirty': {part: [str(1 + int(part == 'cancel'))] * 2 + ['1', '1', 'true', '0'] for part in parts},
                'error': ['Sync', 'CallerMustResolve', 'Sync' if cancel else 'Write',
                          'CallerMustResolve', 'IoWriteUnknown', 'Other',
                          'sync outcome unknown' if cancel else 'write outcome requires caller resolution'],
                'fence': ['true', '2' if cancel else '1', '1' if cancel else '0',
                          '46496' if cancel else '25296', '1', str(cancel).lower(), 'DptInstalled', '1'],
                'obligation': ['1', '1' if cancel else '0', '2' if cancel else '1',
                               '25296' if cancel else '4096', '21200',
                               '46496' if cancel else '25296', 'true', 'true']}

    def test_case_census_and_exact_identity(self):
        check_census(CASES)
        for cases in (CASES[:-1], CASES + CASES[:1], CASES[::-1]):
            with self.assertRaisesRegex(ValueError, 'FAIL_NATIVE_CENSUS'):
                check_census(cases)
        for kind in CASES:
            good = self.valid(kind)
            check_worker(good, kind, 'source')
            for field, index, value, code in [('summary', 4, 'stale', 'FAIL_WORKER_IDENTITY'),
                                             ('summary', 1, 'other', 'FAIL_WORKER_IDENTITY'),
                                             ('markers', 0, 'OLD_NATIVE_DONE', 'FAIL_WORKER_MARKER')]:
                bad = deepcopy(good)
                bad[field][index] = value
                with self.assertRaisesRegex(ValueError, code):
                    check_worker(bad, kind, 'source')

    def test_partial_write_and_sync_fault_prefix(self):
        for kind in ('reserve-write-half', 'cancel-sync-after'):
            good = self.valid(kind)
            for index, value in ((1, '0'), (3, '0'), (4, '')):
                bad = deepcopy(good)
                bad['events'][0][index] = value
                if bad == good:
                    bad['events'][0][index] = 'corrupt'
                with self.assertRaisesRegex(ValueError, 'FAIL_FAULT_PREFIX'):
                    check_worker(bad, kind, 'source')
            for index in (1, 2, 3, 4, 5, 6):
                bad = deepcopy(good)
                bad['error'][index] = 'wrong'
                with self.assertRaisesRegex(ValueError, 'FAIL_ORIGINAL_CAUSE'):
                    check_worker(bad, kind, 'source')

    def test_retained_obligation_every_field(self):
        for kind in ('reserve-write-half', 'cancel-sync-after'):
            good = self.valid(kind)
            for index in range(8):
                bad = deepcopy(good)
                bad['fence'][index] = 'wrong'
                with self.assertRaisesRegex(ValueError, 'FAIL_RETAINED_OBLIGATION'):
                    check_worker(bad, kind, 'source')
                bad = deepcopy(good)
                bad['obligation'][index] = 'wrong'
                with self.assertRaisesRegex(ValueError, 'FAIL_OBLIGATION_IDENTITY'):
                    check_worker(bad, kind, 'source')

    def test_dirty_first_lsn_generation_counter_and_pin(self):
        good = self.valid()
        for part in ('reserve', 'cancel'):
            for index in range(6):
                bad = deepcopy(good)
                bad['dirty'][part][index] = 'wrong'
                with self.assertRaisesRegex(ValueError, 'FAIL_DIRTY_REGISTRATION'):
                    check_worker(bad, 'cancel-sync-after', 'source')
        del good['dirty']['cancel']
        with self.assertRaisesRegex(ValueError, 'FAIL_DIRTY_CENSUS'):
            check_worker(good, 'cancel-sync-after', 'source')


if __name__ == '__main__':
    unittest.main()
