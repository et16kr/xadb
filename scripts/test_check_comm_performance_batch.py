"""New deadline/selection/partial-evidence boundaries, no legacy suite execution."""
from datetime import datetime, timedelta, timezone
from contextlib import nullcontext
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

from check_comm_performance import MEASURE_SECONDS, run_case
from check_comm_performance_batch import (
    CATALOGUE, EIGHT_HOURS, FINALIZE_SECONDS, PAIRS, batch_summary,
    main, mode_timeout, plan_batch, require_time,
)


def receipts():
    rows = []
    for pair in range(1, PAIRS + 1):
        for mode in ('ALWAYS', 'DETAILED'):
            frames = [100] * MEASURE_SECONDS
            byte_counts = [100 * 65536] * MEASURE_SECONDS
            rows.append({'status': 'PASS', 'cell': CATALOGUE[37].copy(), 'cell_index': 37,
                         'pair': pair, 'instrumentation': mode,
                         'source_sha256': 'source', 'binary_sha256': 'binary',
                         'peer': {'measured_frames_per_second': frames,
                                  'measured_payload_bytes_per_second': byte_counts,
                                  'latency_ns': {'p99': None}},
                         'worker': {'totals': {'measured_received_frames_per_second': frames,
                                              'measured_received_payload_bytes_per_second': byte_counts},
                                    'diagnostics': {'peak_payload_bytes': 1000}}})
    return rows


class PerformanceBatchTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 23, 3, tzinfo=timezone.utc)
        self.deadline = datetime(2026, 10, 23, 14, tzinfo=timezone.utc)

    def test_explicit_original_cell_and_ten_pairs_preserve_the_full_denominator(self):
        plan = plan_batch([37], self.deadline, 3600, self.now)
        self.assertEqual(CATALOGUE[37], {'family': 'tcp6', 'connections': 1,
                                       'payload_bytes': 65536, 'workload': 'BULK', 'slow_peer': False})
        self.assertTrue(plan['can_start'])
        self.assertEqual(plan['selected_mode_runs'], 20)
        self.assertEqual(plan['full_profile_mode_runs'], 2700)
        self.assertEqual(plan['nominal_measurement_seconds'], 700)

    def test_invalid_selection_deadline_and_eight_hour_budget_are_rejected(self):
        for selected in ([], [37, 37], [-1], [135], [True]):
            with self.subTest(selected=selected), self.assertRaises(ValueError):
                plan_batch(selected, self.deadline, 3600, self.now)
        for deadline in (self.deadline.replace(tzinfo=None), self.now, self.now - timedelta(seconds=1)):
            with self.subTest(deadline=deadline), self.assertRaises(ValueError):
                plan_batch([37], deadline, 3600, self.now)
        for budget in (0, -1, EIGHT_HOURS, EIGHT_HOURS + 1, True):
            with self.subTest(budget=budget), self.assertRaises(ValueError):
                plan_batch([37], self.deadline, budget, self.now)

    def test_full_campaign_or_short_deadline_is_skipped_before_build(self):
        self.assertFalse(plan_batch(list(range(135)), self.deadline, EIGHT_HOURS - 1, self.now)['can_start'])
        self.assertFalse(plan_batch([37], self.now + timedelta(minutes=20), 3600, self.now)['can_start'])
        self.assertFalse(plan_batch([37], self.deadline, 700, self.now)['can_start'])
        with patch('check_comm_performance_batch.datetime') as clock:
            clock.now.return_value = self.deadline - timedelta(seconds=FINALIZE_SECONDS + 9)
            with self.assertRaises(TimeoutError):
                require_time(self.deadline, time.monotonic() + 3600, 10)

    def test_partial_duplicate_wrong_source_or_guardrail_fail_cannot_pass_batch(self):
        rows = receipts()
        summary = batch_summary(rows, [37], 'source', 'binary')
        self.assertEqual(summary['status'], 'BATCH_PASS')
        self.assertEqual(summary['c1_status'], 'INCOMPLETE')
        self.assertEqual(summary['full_profile_mode_runs'], 2700)
        for bad in (rows[:-1], rows[:-1] + [rows[0]],
                    [rows[0] | {'source_sha256': 'other'}] + rows[1:],
                    [rows[0] | {'cell_index': 36}] + rows[1:],
                    [rows[0] | {'binary_sha256': 'other'}] + rows[1:]):
            with self.subTest(bad=bad[0].get('source_sha256')), self.assertRaises(ValueError):
                batch_summary(bad, [37], 'source', 'binary')
        failed = receipts()
        for row in failed:
            if row['instrumentation'] == 'DETAILED':
                row['worker']['totals']['measured_received_frames_per_second'] = [50] * MEASURE_SECONDS
                row['worker']['totals']['measured_received_payload_bytes_per_second'] = [50 * 65536] * MEASURE_SECONDS
        self.assertEqual(batch_summary(failed, [37], 'source', 'binary')['status'], 'FAIL')

    def test_active_alarm_is_rejected_without_replacing_its_handler_or_timer(self):
        original = signal.getsignal(signal.SIGALRM)
        signal.setitimer(signal.ITIMER_REAL, 30)
        try:
            with self.assertRaises(ValueError):
                with mode_timeout(1):
                    self.fail('active alarm must not enter the guarded operation')
            self.assertGreater(signal.getitimer(signal.ITIMER_REAL)[0], 0)
            self.assertEqual(signal.getsignal(signal.SIGALRM), original)
        finally:
            signal.setitimer(signal.ITIMER_REAL, 0)

    def test_short_deadline_writes_skip_without_capturing_or_building(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            deadline = (datetime.now(timezone.utc) + timedelta(minutes=20)).isoformat()
            destination = root / 'result.json'
            with (patch('check_comm_performance_batch.ROOT', root),
                  patch('sys.argv', ['batch', '--cell-index', '37', '--deadline', deadline,
                                     '--report', str(destination)]),
                  patch('check_comm_performance_batch.capture_source') as capture,
                  patch('check_comm_performance_batch.subprocess.run') as build):
                self.assertEqual(main(), 0)
            report = json.loads(destination.read_text())
            self.assertEqual(report['status'], 'SKIPPED_TIME_BUDGET')
            self.assertEqual(report['completed_mode_runs'], 0)
            self.assertEqual(report['c1_status'], 'INCOMPLETE')
            capture.assert_not_called()
            build.assert_not_called()

    def test_failed_mode_is_attempted_but_not_counted_as_completed(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            deadline = (datetime.now(timezone.utc) + timedelta(hours=2)).isoformat()
            destination = root / 'result.json'
            responses = iter([receipts()[0], TimeoutError('mode budget')])

            def fake_mode(_binary, directory, *_args):
                directory.mkdir(parents=True)
                response = next(responses)
                if isinstance(response, Exception):
                    raise response
                return response

            with (patch('check_comm_performance_batch.ROOT', root),
                  patch('sys.argv', ['batch', '--cell-index', '37', '--deadline', deadline,
                                     '--report', str(destination)]),
                  patch('check_comm_performance_batch.capture_source', return_value='source'),
                  patch('check_comm_performance_batch.digest', return_value='binary'),
                  patch('check_comm_performance_batch.subprocess.run', return_value=Mock(returncode=0)),
                  patch('check_comm_performance_batch.mode_timeout', return_value=nullcontext()),
                  patch('check_comm_performance_batch.run_case',
                        side_effect=fake_mode)):
                self.assertEqual(main(), 1)
            report = json.loads(destination.read_text())
            self.assertEqual(report['status'], 'FAIL')
            self.assertEqual(report['completed_mode_runs'], 1)
            self.assertEqual(report['attempted_mode_runs'], 2)
            self.assertEqual(len(report['record_paths']), 1)

    def test_real_timeout_reaps_dummy_worker_and_preserves_failure_and_unix_cleanup(self):
        original = signal.getsignal(signal.SIGALRM)
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            pid_file = root / 'pid'
            binary = root / 'blocked-worker'
            binary.write_text(f'#!{sys.executable}\nimport os,time\n'
                              'from pathlib import Path\n'
                              'Path(os.environ["XADB_COMM_TIMEOUT_PID_PATH"]).write_text(str(os.getpid()))\n'
                              'time.sleep(30)\n')
            binary.chmod(0o700)
            with patch.dict(os.environ, {'XADB_COMM_TIMEOUT_PID_PATH': str(pid_file)}):
                with self.assertRaises(TimeoutError):
                    with mode_timeout(0.5):
                        run_case(binary, root / 'case', CATALOGUE[60], 'ALWAYS', 'source')
            receipt = json.loads((root / 'case/validation.json').read_text())
            self.assertEqual(receipt['status'], 'FAIL')
            self.assertEqual(receipt['worker_exit_code'], -signal.SIGKILL)
            self.assertFalse(Path(receipt['command'][-1]).parent.exists())
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_file.read_text()), 0)
        self.assertEqual(signal.getitimer(signal.ITIMER_REAL), (0.0, 0.0))
        self.assertEqual(signal.getsignal(signal.SIGALRM), original)


if __name__ == '__main__':
    unittest.main()
