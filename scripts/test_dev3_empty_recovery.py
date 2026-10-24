import tempfile
import unittest
from pathlib import Path

import dev3_empty_recovery_native as native

ROOT = Path(__file__).resolve().parent.parent


class EmptyRecoveryOracle(unittest.TestCase):
    def test_protocol_rejects_missing_wrong_pid_and_extra_selector(self):
        text = (f'\nrunning 1 test\ntest {native.SELECTOR} ... EMPTY_RECOVERY_NATIVE_BEGIN mode=recover depth=2 pid=42\n'
                'EMPTY_RECOVERY_NATIVE_PASS mode=recover depth=2 pid=42\nok\n'
                'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;\n')
        native.protocol(text, 'recover', 2, 42)
        for bad in (text.replace('pid=42', 'pid=43'), text.replace('depth=2', 'depth=1'),
                    text.replace('EMPTY_RECOVERY_NATIVE_PASS', 'missing'),
                    text + '\ntest other ... ok\n'):
            with self.assertRaises(ValueError):
                native.protocol(bad, 'recover', 2, 42)

    def test_partial_home_and_retirement_have_distinct_reentry_states(self):
        before = native.initial(ROOT, 1)
        page = native.images(ROOT, 1)[1]
        partial = native.recover(before, ROOT, 1, 'redo_second_partial')
        self.assertEqual(partial[native.model.DATA][16384:24576], page[:8192])
        self.assertEqual(partial[native.model.DATA][24576:32768], bytes(8192))
        bank = partial[f'd3-dwb-{partial["DWB"][81]}']
        self.assertEqual((bank[6], native.model.number(bank, 40)), (1, 2))
        retired = native.recover(before, ROOT, 1, 'empty_status_retirement_after')
        bank = retired[f'd3-dwb-{retired["DWB"][81]}']
        self.assertEqual((bank[6], native.model.number(bank, 40)), (0, 2))
        for boundary in (partial, retired):
            after = native.recover(boundary, ROOT, 1)
            self.assertEqual(after[native.model.DATA][16384:32768], page)
            self.assertEqual(after[native.model.WAL], before[native.model.WAL])
            self.assertEqual(native.recover(after, ROOT, 1), after)

    def test_new_census_requires_both_lifetime_phases(self):
        good = ('outer\t100\nprefix_nested\t200\ninventory\t300\nwork_nested\t400\n'
                'entry_phase\t600\nreturn_phase\t550\npeak\t600\nwriter\t32\nfile\t32\n'
                'maintenance\t12582912\nflush\t4194304\n')
        with tempfile.TemporaryDirectory() as path:
            folder = Path(path)
            file = folder / 'source-census.tsv'
            file.write_text(good)
            native.prior.check_census(folder)
            for bad in (good.replace('entry_phase\t600\n', ''),
                        good.replace('peak\t600', 'peak\t550'),
                        good.replace('entry_phase\t600', 'entry_phase\t199').replace('peak\t600', 'peak\t550'),
                        good.replace('return_phase\t550', 'return_phase\t700')):
                file.write_text(bad)
                with self.assertRaises(ValueError):
                    native.prior.check_census(folder)

    def test_slot_failure_preserves_empty_issuance_and_old_home(self):
        before = native.initial(ROOT, 3)
        failed = native.recover(before, ROOT, 3, 'empty_status_slot_partial')
        self.assertEqual(failed[native.model.DATA][16384:32768], before[native.model.DATA][16384:32768])
        bank = failed[f'd3-dwb-{failed["DWB"][81]}']
        self.assertEqual((bank[6], native.model.number(bank, 32), native.model.number(bank, 40)), (0, 5, 2))
        image = native.images(ROOT, 3)[1]
        slot = native.wire.wire_slot(2, image)
        self.assertEqual(failed['d3-dwb-images'][:8192], slot[:8192])
        recovered = native.recover(failed, ROOT, 3)
        self.assertEqual(recovered[native.model.DATA][16384:32768], image)
        final = recovered[f'd3-dwb-{recovered["DWB"][81]}']
        self.assertEqual((native.model.number(final, 32), native.model.number(final, 40)), (10, 3))


if __name__ == '__main__':
    unittest.main()
