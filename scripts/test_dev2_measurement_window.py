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
    def test_privilege_rejects_usable_caller_capabilities(self):
        for name in ('CapPrm', 'CapEff', 'CapAmb'):
            caller = {'caps': dict.fromkeys(('CapPrm', 'CapEff', 'CapAmb'), 0)}
            caller['caps'][name] = 1
            with patch.object(window, 'process_identity', return_value=caller), self.assertRaises(window.DiagnosticError):
                window.perf_invocation(['/usr/bin/perf'], 'sudo-cap-perfmon', Path('/tmp'))

    def test_privilege_command_is_explicit_and_restricted_to_installed_perf(self):
        plain = [str(PERF), 'stat']
        work = Path('/tmp/xadb-window-policy-vector')
        self.assertEqual(window.perf_invocation(plain, 'none', work), (plain, {'mode': 'none'}))
        with self.assertRaises(window.DiagnosticError):
            window.perf_invocation(plain, 'sudo-cap-perfmon', work)
        with patch.object(window.os, 'getuid', return_value=0), self.assertRaises(window.DiagnosticError):
            window.perf_invocation(['/usr/bin/perf'], 'sudo-cap-perfmon', work)
        command, policy = window.perf_invocation(['/usr/bin/perf', 'stat', '--tid', '123'], 'sudo-cap-perfmon', work)
        self.assertEqual(command[1:4], ['-n', '--preserve-env=PATH,LC_ALL,HOME,PERF_CONFIG', '--'])
        self.assertEqual(policy['environment'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C', 'HOME': str(work), 'PERF_CONFIG': '/dev/null'})
        self.assertEqual(command[5:-4], [f'--reuid={os.getuid()}', f'--regid={os.getgid()}',
            '--clear-groups', '--inh-caps=-all,+perfmon', '--ambient-caps=-all,+perfmon',
            '--bounding-set=-all,+perfmon', '--no-new-privs', '--pdeathsig=KILL', '--'])
        self.assertEqual(policy['perf_command'], ['/usr/bin/perf', 'stat', '--tid', '123'])
        with tempfile.TemporaryDirectory() as temp, self.assertRaises(window.DiagnosticError):
            window.trusted_tool(temp)

    def test_privilege_identity_rejects_extra_caps_root_worker_and_escaped_group(self):
        policy = {'uid': 1000, 'gid': 1000, 'perf_command': ['/usr/bin/perf', 'stat']}
        launcher = dict(pid=10, pgid=10, sid=10, uids=[1000, 0, 0, 0])
        collector = dict(pid=11, ppid=10, pgid=10, sid=10, uids=[1000]*4, gids=[1000]*4,
                         groups=[], no_new_privs=1, command=policy['perf_command'],
                         caps={k: 1 << 38 for k in ('CapInh', 'CapPrm', 'CapEff', 'CapBnd', 'CapAmb')})
        worker = dict(uids=[1000]*4, gids=[1000]*4, groups=[1000],
                      caps={k: 0 for k in ('CapInh', 'CapPrm', 'CapEff', 'CapAmb')})
        policy['caller'] = {'caps': worker['caps'], 'groups': worker['groups']}
        window.validate_perf_identity(launcher, collector, worker, policy)
        for change in ({'uids': [0]*4}, {'pgid': 12}, {'sid': 12}, {'ppid': 12},
                       {'groups': [0]}, {'no_new_privs': 0}, {'command': ['/tmp/perf']},
                       {'caps': collector['caps'] | {'CapEff': (1 << 38) | 1}}):
            with self.assertRaises(window.DiagnosticError):
                window.validate_perf_identity(launcher, collector | change, worker, policy)
        for changed in (worker | {'uids': [0]*4}, worker | {'caps': worker['caps'] | {'CapEff': 1}}):
            with self.assertRaises(window.DiagnosticError):
                window.validate_perf_identity(launcher, collector, changed, policy)
        with self.assertRaises(window.DiagnosticError):
            window.validate_perf_identity(launcher | {'uids': [0]*4}, collector, worker, policy)

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

    def test_privilege_rejects_fixture_before_sudo_and_reaps_worker(self):
        base = self.directory()
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE}))
        result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'result', perf=PERF,
                                perf_privilege='sudo-cap-perfmon')
        self.assertEqual(result['status'], 'FAILED')
        self.assertIn('only supports installed', result['error'])
        self.assertEqual(len(result['cleanup']), 1)
        self.assertTrue(result['cleanup'][0]['group_gone'])
        self.assertNotIn('perf', result['commands'])

    def test_pinned_collector_outside_launcher_group_is_terminated(self):
        result = self.pinned_collector_cleanup()
        self.assertTrue(result['cleanup'][0]['collector']['exited'])
        self.assertTrue(result['cleanup'][0]['group_gone'])

    def test_pinned_collector_cleanup_survives_launcher_cleanup_error(self):
        finish = window.Owned.finish
        def fail_after_launcher(child):
            finish(child)
            raise PermissionError('injected launcher cleanup failure')
        with patch.object(window.Owned, 'finish', fail_after_launcher):
            result = self.pinned_collector_cleanup()
        self.assertTrue(result['cleanup'][0]['collector']['exited'])
        self.assertEqual(result['status'], 'FAILED')
        self.assertIn('injected launcher cleanup failure', result['cleanup'][0]['capture_errors'][0])

    def pinned_collector_cleanup(self):
        base = self.directory()
        code = ('import os,time; from pathlib import Path; pid=os.fork(); '
                'os.setsid() if pid == 0 else None; '
                'Path("collector.pid").write_text(str(os.getpid())) if pid == 0 else None; time.sleep(5)')
        run = window.Runner(base, 3)
        try:
            child = run.spawn([sys.executable, '-B', '-c', code], base, os.environ.copy(),
                              subprocess.DEVNULL, 'launcher', window.CAP, window.PrivilegedPerf)
            run.wait(lambda: (base / 'collector.pid').exists())
            identity = window.process_identity(int((base / 'collector.pid').read_text()))
            child.pin_collector(identity)
            self.assertNotEqual(identity['pgid'], child.pid)
        finally:
            run.cleanup()
            (base / 'cleanup.json').write_text(json.dumps(run.record, indent=2) + '\n')
        return run.record

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


class WindowProfiles(unittest.TestCase):
    directory = WindowProcesses.directory
    EVENTS = ('cycles:u', 'cycles:k', 'instructions:u', 'instructions:k', 'ref-cycles:u',
              'context-switches', 'cpu-migrations', 'page-faults')

    def test_literal_user_kernel_profile_quality_and_wrong_events(self):
        lines = [f'{200+i};;{event};100000;100.00;' for i, event in enumerate(self.EVENTS)]
        raw = '\n'.join(lines)
        result = window.parse_perf(raw, 'user-kernel-v1')
        self.assertEqual(result['profile'], 'user-kernel-v1')
        self.assertEqual(tuple(result['events']), self.EVENTS)
        self.assertEqual(result['events']['instructions:k']['count'], 203)
        self.assertTrue(result['all_events_reported_full_coverage'])
        self.assertIsNone(result['ratios'])
        for altered, event, status in (
                ('\n'.join(lines[:3] + lines[4:]), 'instructions:k', 'MISSING'),
                (raw.replace('201;;cycles:k;100000;100.00', '201;;cycles:k;100000;97.50'),
                 'cycles:k', 'PARTIAL_OR_EMPTY'),
                (raw.replace('203;;instructions:k;100000;100.00', '<not supported>;;instructions:k;0;'),
                 'instructions:k', 'UNAVAILABLE')):
            value = window.parse_perf(altered, 'user-kernel-v1')
            self.assertEqual(value['events'][event]['status'], status)
            self.assertFalse(value['all_events_reported_full_coverage'])
            self.assertIsNone(value['ratios'])
        for altered, profile in ((raw + '\n' + lines[1], 'user-kernel-v1'),
                                 (raw.replace('cycles:k', 'cycles:h'), 'user-kernel-v1'),
                                 (raw, 'user-v1')):
            with self.assertRaises(window.DiagnosticError):
                window.parse_perf(altered, profile)

    def test_invalid_profile_rejected_before_input_output_or_spawn(self):
        base = self.directory()
        for profile in ('unknown', 'cycles:k', None, []):
            with patch.object(window, 'Runner') as runner, self.assertRaisesRegex(
                    window.DiagnosticError, 'unsupported perf event profile'):
                window.collect(PEER, base / 'missing-input', SOURCE, 'synthetic',
                               base / 'result', perf_profile=profile)
            runner.assert_not_called()
            self.assertFalse((base / 'result').exists())

    def test_cli_user_kernel_profile_reaches_fake_perf_and_parser(self):
        base = self.directory()
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE}))
        command = [sys.executable, '-B', str(HERE / 'dev2_measurement_window.py'),
                   '--worker', str(PEER), '--input', str(source), '--source', SOURCE,
                   '--workload', 'synthetic', '--output', str(base / 'result'),
                   '--perf', str(PERF), '--perf-profile', 'user-kernel-v1']
        completed = subprocess.run(command, capture_output=True, timeout=15, check=False)
        (base / 'cli.stdout').write_bytes(completed.stdout)
        (base / 'cli.stderr').write_bytes(completed.stderr)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        result = json.loads((base / 'result/result.json').read_text())
        self.assertEqual(result['perf_profile'], 'user-kernel-v1')
        self.assertEqual(result['pmu']['profile'], 'user-kernel-v1')
        self.assertTrue(result['pmu']['all_events_reported_full_coverage'])
        self.assertEqual(tuple(result['pmu']['events']), self.EVENTS)
        for i, event in enumerate(self.EVENTS):
            self.assertEqual(result['pmu']['events'][event]['count'], 100 + i)
        argv = result['commands']['perf']
        self.assertEqual(argv[argv.index('-e') + 1], ','.join(self.EVENTS))

    def test_selected_profile_survives_unavailable_and_pre_attach_failure(self):
        for config, expected_status, expected_mode in (
                ({'perf': 'permission'}, 'COLLECTED_DIAGNOSTIC_ONLY', 'RUSAGE_ONLY'),
                ({'behavior': 'bad_identity'}, 'FAILED', None)):
            base = self.directory()
            source = base / 'input.json'
            source.write_text(json.dumps({'source': SOURCE, **config}))
            result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'result',
                                    perf=PERF, perf_profile='user-kernel-v1')
            self.assertEqual(result['perf_profile'], 'user-kernel-v1')
            self.assertEqual(result['status'], expected_status)
            self.assertEqual(result.get('mode'), expected_mode)


@unittest.skipUnless(os.environ.get('XADBTEST_WINDOW_PRIVILEGED_TESTS') == '1',
                     'explicit sudo/real-PMU diagnostic validation only')
class WindowPrivilegeIntegration(unittest.TestCase):
    directory = WindowProcesses.directory

    def test_privileged_spawn_cancelled_before_first_control_ack(self):
        base = self.directory()
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE}))
        original = window.Runner.perf_command
        def interrupt_before_control(runner, *args, **kwargs):
            runner.cancel_signal = signal.SIGTERM
            return original(runner, *args, **kwargs)
        with patch.object(window.Runner, 'perf_command', interrupt_before_control):
            result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'result',
                perf=Path('/usr/bin/perf'), perf_privilege='sudo-cap-perfmon')
        self.assertEqual(result['status'], 'FAILED')
        self.assertIn('interrupted by signal 15', result['error'])
        self.assertNotIn('mode', result)
        self.assertNotIn('observed', result['perf_privilege'])
        self.assertEqual(len(result['cleanup']), 2)
        self.assertTrue(all(r['group_gone'] for r in result['cleanup']), result)

    def test_privileged_pre_ready_rejection_and_interruption_cleanup(self):
        original = window.Runner.perf_command
        def interrupt_during_prepare(runner, command, ctl, ack, child, timeout=2):
            # Cancel after the real disabled collector's ACK, before READY.
            value = original(runner, command, ctl, ack, child, timeout)
            runner.cancel_signal = signal.SIGTERM
            runner.tick()
            return value
        for label, target, replacement in (
                ('prepare-interrupt', 'perf_command', interrupt_during_prepare),
                ('identity-rejection', None, None)):
            base = self.directory()
            source = base / 'input.json'
            source.write_text(json.dumps({'source': SOURCE}))
            context = (patch.object(window.Runner, target, replacement) if target else
                       patch.object(window, 'validate_perf_identity', side_effect=window.DiagnosticError('injected topology rejection')))
            with context:
                result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'result',
                    perf=Path('/usr/bin/perf'), perf_privilege='sudo-cap-perfmon')
            self.assertEqual(result['status'], 'FAILED', result)
            self.assertIn('interrupted' if target else 'topology rejection', result['error'])
            self.assertNotIn('mode', result)
            self.assertEqual(len(result['cleanup']), 2)
            self.assertTrue(all(r['group_gone'] for r in result['cleanup']), result)
            if label == 'identity-rejection':
                self.assertTrue(result['cleanup'][0]['collector']['exited'])

    def test_actual_pmu_window_keeps_worker_unprivileged(self):
        base = self.directory()
        source = base / 'input.json'
        source.write_text(json.dumps({'source': SOURCE}))
        result = window.collect(PEER, source, SOURCE, 'synthetic', base / 'result', perf=Path('/usr/bin/perf'),
                                perf_privilege='sudo-cap-perfmon', affinity=min(os.sched_getaffinity(0)))
        self.assertEqual(result['status'], 'COLLECTED_DIAGNOSTIC_ONLY', result)
        self.assertEqual(result['mode'], 'PMU')
        self.assertEqual(result['pmu']['status'], 'COLLECTED')
        self.assertTrue(result['pmu']['all_events_reported_full_coverage'])
        for event in ('cycles:u', 'instructions:u', 'ref-cycles:u'):
            self.assertGreater(result['pmu']['events'][event]['count'], 0)
        observed = result['perf_privilege']['observed']
        self.assertEqual(observed['worker']['uids'], [os.getuid()]*4)
        self.assertEqual(observed['worker']['caps']['CapEff'], 0)
        self.assertEqual(observed['collector']['caps']['CapEff'], 1 << 38)
        self.assertTrue(all(r['group_gone'] for r in result['cleanup']), result)
        self.assertEqual(result['performance_acceptance'], 'NOT_APPLICABLE')

    def test_privileged_active_sigint_sigterm_and_deadline_cleanup(self):
        for sig in (signal.SIGINT, signal.SIGTERM, None):
            with self.subTest(signal=sig):
                base = self.directory()
                source = base / 'input.json'
                source.write_text(json.dumps({'source': SOURCE, 'behavior': 'active-hang'}))
                command = [sys.executable, '-B', str(HERE / 'dev2_measurement_window.py'), '--worker', str(PEER),
                    '--input', str(source), '--source', SOURCE, '--workload', 'synthetic',
                    '--output', str(base / 'result'), '--perf', '/usr/bin/perf',
                    '--perf-privilege', 'sudo-cap-perfmon', '--deadline', '2' if sig is None else '10']
                with (base / 'runner.stdout').open('wb') as out, (base / 'runner.stderr').open('wb') as err:
                    process = subprocess.Popen(command, stdout=out, stderr=err)
                    try:
                        marker = base / 'result/worker.stdout'
                        end = time.monotonic() + 5
                        while not marker.exists() or b'ACTIVE_WINDOW' not in marker.read_bytes():
                            self.assertIsNone(process.poll())
                            self.assertLess(time.monotonic(), end)
                            time.sleep(.01)
                        if sig is not None:
                            process.send_signal(sig)
                        self.assertEqual(process.wait(timeout=7), 1)
                    finally:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=7)
                result = json.loads((base / 'result/result.json').read_text())
                self.assertEqual(result['status'], 'FAILED')
                self.assertEqual(result['pmu']['status'], 'ATTACHED')
                self.assertIn('deadline exceeded' if sig is None else f'interrupted by signal {sig.value}', result['error'])
                self.assertEqual(len(result['cleanup']), 2)
                self.assertTrue(all(r['group_gone'] for r in result['cleanup']), result)
                self.assertTrue(all(r['returncode'] is not None for r in result['cleanup']))


if __name__ == '__main__':
    unittest.main()
