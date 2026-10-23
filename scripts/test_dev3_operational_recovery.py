import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import dev3_operational_recovery_literals as inputs
import dev3_operational_recovery_native as native

ROOT=Path(__file__).resolve().parent.parent


class OperationalRecoveryInputs(unittest.TestCase):
    def test_literal_sequence_matches_independent_predecessor(self):
        raw,_=inputs.wire.load_inputs(ROOT)
        result,work=inputs.history(raw['profile1-d3-profile.bin'],raw['profile1-allocation.bin'],2,1)
        self.assertEqual(work,3)
        self.assertEqual(result['wal.frames-only'],native.literal(ROOT,'wal.frames-only'))
        self.assertEqual(result['allocator.after'],native.literal(ROOT,'work3-allocator.after'))
        self.assertEqual(result['status-2.after'],native.literal(ROOT,'work3-status.after'))

    def test_oversized_combination_rejects_before_generation(self):
        with patch.object(inputs.operation,'allocator_after',side_effect=AssertionError('must not allocate')):
            with self.assertRaisesRegex(ValueError,'capacity preflight'):
                inputs.history(b'',b'',63,4096)

    def test_invalid_bounds_are_rejected(self):
        for pages,cancels in ((0,0),(64,0),(2,-1),(2,4097)):
            with self.assertRaisesRegex(ValueError,'history bounds'):
                inputs.history(b'',b'',pages,cancels)

    def test_native_protocol_rejects_wrong_pid_selector_and_missing_completion(self):
        text=f'\nrunning 1 test\ntest {native.SELECTOR} ... OPERATIONAL_RECOVERY_NATIVE_BEGIN mode=recover pid=42\nOPERATIONAL_RECOVERY_NATIVE_PASS mode=recover pid=42\nok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;\n'
        native.check_worker(text,'recover',42)
        for bad in (text.replace('pid=42','pid=43'),text.replace(native.SELECTOR,'wrong'),text.replace('OPERATIONAL_RECOVERY_NATIVE_PASS','missing')):
            with self.assertRaises(ValueError): native.check_worker(bad,'recover',42)

    def test_failure_oracle_keeps_pending_partial_page_until_reentry(self):
        before=native.producer_files(ROOT,False)
        failed=native.recover_files(before,ROOT,'partial_b',True)
        data=native.model.DATA
        page=native.literal(ROOT,'work3-status.after')
        self.assertEqual(failed[data][32768:40960],page[:8192])
        self.assertEqual(failed[data][40960:49152],bytes(8192))
        recovered=native.recover_files(failed,ROOT,'partial_b')
        self.assertEqual(recovered[data][32768:49152],page)
        self.assertEqual(recovered[native.model.WAL],before[native.model.WAL])

    def test_failure_requires_exact_new_target(self):
        with tempfile.TemporaryDirectory() as path:
            folder=Path(path)
            (folder/'error.tsv').write_text('phase\tFlush\ncause\tIo\neffect\tCallerMustResolve\n')
            (folder/'events.tsv').write_text('write\td3-data-0000000100000001\t32768\t16384\tERR_PARTIAL:8192:sha\n')
            native.check_fault(folder,'partial_b')
            (folder/'events.tsv').write_text('write\td3-data-0000000100000001\t16384\t16384\tERR_PARTIAL:8192:sha\n')
            with self.assertRaisesRegex(ValueError,'B_PARTIAL'): native.check_fault(folder,'partial_b')

    def test_wait_uses_callable_deadline_and_records_exit(self):
        child=Mock(); child.poll.return_value=0; item={'timed_out':False}
        def finish(actual, remaining, row):
            self.assertIs(actual, child)
            self.assertTrue(callable(remaining))
            self.assertGreater(remaining(),0)
        now=native.time.monotonic()
        with patch.object(native.prior,'finish_child',side_effect=finish):
            native.wait_worker(child,item,now,now)
        self.assertEqual(item['exit_code'],0)
        self.assertGreaterEqual(item['elapsed_seconds'],0)

    def test_failed_wait_still_records_exit_and_elapsed(self):
        child=Mock(); child.poll.return_value=-9; item={'timed_out':False}
        now=native.time.monotonic()
        with patch.object(native.prior,'finish_child',side_effect=RuntimeError('reaped')):
            with self.assertRaisesRegex(RuntimeError,'reaped'):
                native.wait_worker(child,item,now,now)
        self.assertEqual(item['exit_code'],-9)
        self.assertGreaterEqual(item['elapsed_seconds'],0)

    def test_new_target_wire_tables_match_old_scope_and_page2_fields(self):
        first=native.literal(ROOT,'work1-status.after')
        self.assertEqual(native.wire_control(3,1,first),native.model.wire.control(3,1,first))
        self.assertEqual(native.wire_slot(1,first),native.model.wire.slot(1,first))
        second=native.literal(ROOT,'work3-status.after')
        control=native.wire_control(12,4,second);slot=native.wire_slot(4,second)
        self.assertEqual(control[136:176],second[24:64])
        self.assertEqual(slot[40:80],second[24:64])
        self.assertEqual(slot[128:16512],second)
        self.assertEqual(native.model.sha(control[:-32]),control[-32:].hex())
        self.assertEqual(native.model.sha(slot[:-32]),slot[-32:].hex())

    def test_new_target_wire_rejects_outside_first_extent(self):
        page=bytearray(native.literal(ROOT,'work3-status.after'))
        page[32:40]=(64).to_bytes(8,'little');page[-32:]=bytes.fromhex(native.model.sha(page[:-32]))
        with self.assertRaisesRegex(ValueError,'STATUS_IDENTITY'):native.wire_slot(1,page)

    def test_full_target_observation_rejects_hidden_high_bit(self):
        before=native.producer_files(ROOT,False)
        after=native.recover_files(before,ROOT,'plain')
        with tempfile.TemporaryDirectory() as path:
            folder=Path(path)
            for dest,source in [('allocator.bin','work3-allocator.after'),('descriptor.bin','work3-terminal.descriptor'),('status-2.bin','work3-status.after')]:
                (folder/dest).write_bytes(native.literal(ROOT,source))
            initial=[2,2,None]+[None]*61;final=[6,2,6]+[None]*61
            rows=dict(mask=6,initial=initial,final=final,applied=[4,0,1]+[0]*61,skipped=[2,1,0]+[0]*61)
            import json
            def save(): (folder/'all-targets.txt').write_text(''.join(f'{k}={json.dumps(v)}\n' for k,v in rows.items()))
            save();native.check_inspection(folder,ROOT,'plain',before,after)
            rows['final'][63]=6;save()
            with self.assertRaisesRegex(ValueError,'ALL_TARGET_LSNS'):native.check_inspection(folder,ROOT,'plain',before,after)


if __name__=='__main__': unittest.main()
