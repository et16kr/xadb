"""New window tool contract tests; retained fixtures are not acceptance evidence."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import dev2_measurement_window as window

HERE = Path(__file__).resolve().parent
PEER = HERE / 'fixtures/measurement_window_peer.py'
PERF = HERE / 'fixtures/measurement_window_perf.py'
SOURCE = 'a' * 64


class WindowVectors(unittest.TestCase):
    def test_literal_pmu_coverage_missing_duplicates_and_invalid_counts(self):
        lines = [f'{100+i};;{event};100000;100.00;' for i, event in enumerate(window.EVENTS)]
        result = window.parse_perf('\n'.join(lines))
        self.assertTrue(result['all_events_reported_full_coverage'])
        self.assertEqual(result['events']['instructions:u']['count'], 101)
        self.assertIsNone(result['ratios'])
        self.assertFalse(window.parse_perf(lines[0].replace('100.00', '50.00'))['all_events_reported_full_coverage'])
        self.assertFalse(window.parse_perf(lines[0].replace(';100000;', ';0;'))['all_events_reported_full_coverage'])
        self.assertEqual(window.parse_perf('<not supported>;;cycles:u;0;;')['events']['cycles:u']['status'], 'UNAVAILABLE')
        for raw in (lines[0] + '\n' + lines[0], lines[0].replace('100;;', '-1;;'),
                    lines[0].replace('100.00', 'nan'), lines[0].replace('cycles:u', 'wrong')):
            with self.assertRaises(window.DiagnosticError):
                window.parse_perf(raw)

    def test_literal_stop_rejects_order_token_types_and_decreasing_usage(self):
        value = dict(phase='STOP', token='t', start_ns=11, end_ns=20, elapsed_ns=8,
                     before=[0, 1, 2, 3, 4, 5], after=[0, 2, 4, 6, 8, 10])
        result = window.validate_stop(json.dumps(value), 't', 10, 21)
        self.assertEqual(result['usage_delta'], [0, 1, 2, 3, 4, 5])
        self.assertEqual(result['boundary_excess_ns'], 1)
        for change in ({'start_ns': 9}, {'end_ns': 22}, {'elapsed_ns': 10},
                       {'elapsed_ns': True}, {'token': 'wrong'}, {'phase': 'START'},
                       {'after': [0]*6}, {'before': [0]}, {'extra': 0}):
            with self.assertRaises(window.DiagnosticError):
                window.validate_stop(json.dumps(value | change), 't', 10, 21)
        self.assertIsNone(window.validate_stop(json.dumps(value | {'before': None}), 't', 10, 21)['usage_delta'])

    def test_owned_identity_rejects_tid_source_peer_and_starttime(self):
        pid = os.getpid()
        start = window.starttime(pid)
        hello = f'HELLO 1 t {pid} {pid} {SOURCE} synthetic'
        window.validate_hello(hello, pid, pid, start, SOURCE, 'synthetic', 't')
        for line, peer, epoch in ((hello.replace(SOURCE, 'b'*64), pid, start),
                                  (hello, pid+1, start), (hello, pid, start+1),
                                  (hello.replace(f'{pid} {pid}', f'{pid} {pid+1}'), pid, start)):
            with self.assertRaises(window.DiagnosticError):
                window.validate_hello(line, peer, pid, epoch, SOURCE, 'synthetic', 't')

    def test_duplicate_stop_and_split_nul_or_unsolicited_perf_ack(self):
        with self.assertRaises(window.DiagnosticError):
            window.validate_stop('{"phase":"wrong","phase":"STOP"}', 't', 0, 1)
        ack = window.PerfAck()
        self.assertFalse(ack.feed(b'a'))
        self.assertTrue(ack.feed(b'ck\n'))
        self.assertFalse(ack.feed(b'\0'))
        self.assertTrue(ack.feed(b'ack\n\0'))
        self.assertTrue(ack.feed(b'ack\n'))
        ack.unsolicited(b'\0')
        with self.assertRaises(window.DiagnosticError):
            ack.unsolicited(b'ack\n')
        with self.assertRaises(window.DiagnosticError):
            ack.feed(b'ack\nack\n')



class WindowProcesses(unittest.TestCase):
    def directory(self):
        root = os.environ.get('XADBTEST_WINDOW_TEST_ARTIFACTS')
        if root:
            Path(root).mkdir(parents=True, exist_ok=True)
        return Path(tempfile.mkdtemp(prefix=self._testMethodName + '-', dir=root))

    def collect(self, settings=None, *, perf=None, deadline=5):
        base = self.directory()
        input_file = base / 'input.json'
        input_file.write_text(json.dumps({'source': SOURCE} | (settings or {})))
        result = window.collect(PEER, input_file, SOURCE, 'synthetic', base / 'result',
                                perf=perf, deadline=deadline)
        self.assertEqual(result['performance_acceptance'], 'NOT_APPLICABLE')
        self.assertTrue(all(r['group_gone'] for r in result['cleanup']), result)
        return result, base

    def test_normal_rusage_only_and_raw_bracket(self):
        result, _ = self.collect()
        self.assertEqual(result['status'], 'COLLECTED_DIAGNOSTIC_ONLY', result)
        self.assertEqual(result['mode'], 'RUSAGE_ONLY')
        self.assertEqual(result['pmu']['status'], 'UNAVAILABLE')
        self.assertGreaterEqual(result['window']['usage_delta'][4], 1)

    def test_fake_pmu_controls_collect_literal_counts(self):
        result, _ = self.collect(perf=PERF)
        self.assertEqual(result['status'], 'COLLECTED_DIAGNOSTIC_ONLY', result)
        self.assertEqual(result['mode'], 'PMU')
        self.assertEqual(result['pmu']['events']['cycles:u']['count'], 100)

    def test_installed_perf_restricted_message_is_preserved_unavailable(self):
        result, _ = self.collect({'perf': 'restricted'}, perf=PERF)
        self.assertEqual(result['status'], 'COLLECTED_DIAGNOSTIC_ONLY', result)
        self.assertEqual(result['mode'], 'RUSAGE_ONLY')
        self.assertIn('No supported events found', result['pmu']['reason'])

    def test_finalization_without_counter_rows_is_failure(self):
        for behavior in ('sigint-no-output', 'empty-finalization'):
            result, _ = self.collect({'perf': behavior}, perf=PERF)
            self.assertEqual(result['status'], 'FAILED', result)
            self.assertIn('no counter rows', result['error'])
        with self.assertRaises(window.DiagnosticError):
            window.parse_perf('')

    def test_permission_is_unavailable_but_lost_active_perf_fails(self):
        unavailable, _ = self.collect({'perf': 'permission'}, perf=PERF)
        self.assertEqual(unavailable['status'], 'COLLECTED_DIAGNOSTIC_ONLY', unavailable)
        self.assertEqual(unavailable['pmu']['status'], 'UNAVAILABLE')
        lost, _ = self.collect({'perf': 'lost'}, perf=PERF)
        self.assertEqual(lost['status'], 'FAILED', lost)
        self.assertIn('perf exited', lost['error'])

    def test_identity_midstream_exit_and_deadline_failures_retained(self):
        for behavior in ('bad_identity', 'exit', 'hang', 'partial'):
            with self.subTest(behavior=behavior):
                result, _ = self.collect({'behavior': behavior}, deadline=.4)
                self.assertEqual(result['status'], 'FAILED', result)
                self.assertIn('error', result)

    def test_output_limit_is_hard_saved_bound(self):
        result, base = self.collect({'behavior': 'overflow'})
        self.assertEqual(result['status'], 'FAILED')
        self.assertEqual((base / 'result/worker.stdout').stat().st_size, 4 * window.CAP)
        self.assertTrue(result['cleanup'][0]['output_overflow'])

    def test_overwrite_and_large_input_rejected_before_spawn(self):
        result, base = self.collect()
        with self.assertRaises(FileExistsError):
            window.collect(PEER, base / 'input.json', SOURCE, 'synthetic', base / 'result')
        huge = base / 'huge'
        huge.write_bytes(b'x' * 65537)
        with self.assertRaises(window.DiagnosticError):
            window.collect(PEER, huge, SOURCE, 'synthetic', base / 'new')
        self.assertFalse((base / 'new').exists())
        self.assertEqual(result['status'], 'COLLECTED_DIAGNOSTIC_ONLY')

    def test_regular_input_bound_and_capture_setup_or_drain_failure_cleanup(self):
        base = self.directory()
        fifo = base / 'input.fifo'
        os.mkfifo(fifo)
        with self.assertRaises(window.DiagnosticError):
            window.bounded_input(fifo)
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE, 'behavior': 'hang'}))
        with patch.object(window, 'Capture', side_effect=OSError('injected capture open failure')):
            result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'open-error', deadline=1)
        self.assertEqual(result['status'], 'FAILED')
        self.assertEqual(len(result['cleanup']), 1)
        self.assertTrue(result['cleanup'][0]['group_gone'])
        with patch.object(window.Capture, 'drain', side_effect=OSError('injected capture write failure')):
            result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'drain-error', deadline=1)
        self.assertEqual(result['status'], 'FAILED')
        self.assertTrue(result['cleanup'][0]['group_gone'])
        self.assertTrue(result['cleanup'][0]['capture_errors'])


    def test_second_fifo_open_failure_closes_first_and_owned_child(self):
        base = self.directory()
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE}))
        original_open = os.open
        opened = []
        def open_file(path, flags, *args, **kwargs):
            if Path(path).name == 'ack':
                raise OSError('injected second FIFO open failure')
            fd = original_open(path, flags, *args, **kwargs)
            if Path(path).name == 'ctl':
                opened.append(fd)
            return fd
        with patch.object(window.os, 'open', open_file):
            result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'result', perf=PERF)
        self.assertEqual(result['status'], 'FAILED')
        self.assertEqual(len(opened), 1)
        for fd in opened:
            with self.assertRaises(OSError):
                os.fstat(fd)
        self.assertTrue(all(r['group_gone'] for r in result['cleanup']))

    def test_plain_native_helper_sigterm_reaps_synthetic_child(self):
        base = self.directory()
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE, 'behavior': 'hang'}))
        code = ('import sys; from pathlib import Path; from check_dev2_window_native import plain; '
                'plain(Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], "synthetic", Path(sys.argv[4]), False)')
        with (base / 'helper.stdout').open('wb') as stdout, (base / 'helper.stderr').open('wb') as stderr:
            process = subprocess.Popen([sys.executable, '-B', '-c', code, str(PEER), str(source), SOURCE, str(base / 'result')],
                                       env=os.environ | {'PYTHONPATH': str(HERE)}, stdout=stdout, stderr=stderr)
            try:
                end = time.monotonic() + 5
                while not (base / 'result/worker.stderr').exists():
                    self.assertIsNone(process.poll())
                    self.assertLess(time.monotonic(), end)
                    time.sleep(.01)
                process.terminate()
                self.assertEqual(process.wait(timeout=6), 0)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.wait()
        result = json.loads((base / 'result/result.json').read_text())
        self.assertEqual(result['status'], 'FAILED')
        self.assertIn('interrupted by signal 15', result['error'])
        self.assertTrue(all(r['group_gone'] for r in result['cleanup']))

    def test_cooperative_signals_preserve_receipt_and_reap_child(self):
        for sig in (signal.SIGINT, signal.SIGTERM):
            base = self.directory()
            input_file = base / 'input.json'
            input_file.write_text(json.dumps({'source': SOURCE, 'behavior': 'hang'}))
            with (base / 'runner.stdout').open('wb') as stdout, (base / 'runner.stderr').open('wb') as stderr:
                process = subprocess.Popen([sys.executable, '-B', str(HERE / 'dev2_measurement_window.py'),
                    '--worker', str(PEER), '--input', str(input_file), '--source', SOURCE,
                    '--workload', 'synthetic', '--output', str(base / 'result')], stdout=stdout, stderr=stderr)
                try:
                    end = time.monotonic() + 5
                    while not (base / 'result/worker.stderr').exists():
                        self.assertIsNone(process.poll())
                        self.assertLess(time.monotonic(), end)
                        time.sleep(.01)
                    process.send_signal(sig)
                    self.assertEqual(process.wait(timeout=6), 1)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
            result = json.loads((base / 'result/result.json').read_text())
            self.assertEqual(result['status'], 'FAILED')
            self.assertIn('interrupted by signal', result['error'])
            self.assertTrue(all(r['group_gone'] for r in result['cleanup']))


if __name__ == '__main__':
    unittest.main()
