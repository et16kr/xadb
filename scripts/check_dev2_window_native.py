#!/usr/bin/env python3
"""Select one changed worker hook case. This is not a PF04 campaign or wrapper."""
import argparse
import copy
import json
import os
import signal
from pathlib import Path
import struct
import subprocess
import time
from unittest.mock import patch

import dev2_measurement_window as window
import dev2_durable_performance as durable

K0_PATH = 'k0.replace_snapshot_read.v1'


def frames(raw):
    values = []
    while raw:
        window.require(len(raw) >= 4, 'truncated frame header')
        length = struct.unpack('>I', raw[:4])[0]
        window.require(0 < length <= 4 * window.CAP and length <= len(raw) - 4, 'frame bound')
        values.append(json.loads(raw[4:4+length], object_pairs_hook=window.unique_object))
        raw = raw[4+length:]
    return values


def requests(workload):
    identity = dict(version=1, run=[7]*16, request=1, actor=0)
    if workload == 'durable':
        return [identity | {'input': durable.INPUT}]
    profile = dict(version=1, seed=20261018, row_count=128, pin_old=True, mode='off',
                   capacity=256, warmup=4096, measure=65536, latency_stride=16)
    return [identity | {'action': dict(operation='initialize', database=[8]*16, timeline=1,
                                      epoch=1, resident_bytes=64*1024*1024, snapshot_pins=32)},
            identity | {'request': 2, 'action': dict(operation='performance', input=profile)}]


def encode(values):
    return b''.join(struct.pack('>I', len(raw)) + raw for raw in
                    (json.dumps(value, separators=(',', ':')).encode() for value in values))


def plain(worker, input_file, source, workload, directory, partial):
    directory.mkdir(mode=0o700)
    (directory / 'input.bin').write_bytes(input_file.read_bytes())
    work = directory / 'work'
    work.mkdir()
    run = window.Runner(directory, 30)
    run.record.update(source_sha256=source, workload=workload, worker_sha256=window.digest(worker),
                      test_case='partial_configuration' if partial else 'disabled',
                      performance_acceptance='NOT_APPLICABLE')
    env = os.environ.copy()
    env.pop('XADBTEST_WINDOW_SOCKET', None)
    env.pop('XADBTEST_WINDOW_TOKEN', None)
    env.pop('XADBTEST_WINDOW_BOUNDARY', None)
    if partial:
        env['XADBTEST_WINDOW_TOKEN'] = '0'*32
    old_handlers = {}
    def interrupt(signum, _frame):
        run.cancel_signal = signum
    for sig in (signal.SIGINT, signal.SIGTERM):
        old_handlers[sig] = signal.signal(sig, interrupt)
    try:
        with input_file.open('rb') as stdin:
            run.record['invoke_ns'] = time.monotonic_ns()
            child = run.spawn(['taskset', '-c', '2', str(worker)], work, env, stdin, 'worker', 4*window.CAP)
            run.wait(child.exited)
            for capture in child.captures:
                run.wait(lambda c=capture: c.eof)
            run.record['observed_ns'] = time.monotonic_ns()
            result = os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOWAIT)
            window.require(result.si_code == os.CLD_EXITED and result.si_status == (2 if partial else 0), 'unexpected native exit')
            run.record['status'] = 'EXPECTED_NATIVE_EXIT'
    except Exception as error:
        run.record['error'] = repr(error)
    finally:
        for sig in old_handlers:
            signal.signal(sig, signal.SIG_IGN)
        try:
            run.cleanup()
        finally:
            for sig, handler in old_handlers.items():
                signal.signal(sig, handler)
        run.record['files'] = {name: {'sha256': window.digest(directory / name)}
                               for name in ('input.bin', 'worker.stdout', 'worker.stderr')}
        (directory / 'result.json').write_text(json.dumps(run.record, indent=2)+'\n')
    return run.record


def validate(directory, workload, case, build):
    record = json.loads((directory / 'result.json').read_text())
    window.require(record['status'] in ('EXPECTED_NATIVE_EXIT', 'COLLECTED_DIAGNOSTIC_ONLY'), 'collector failed')
    window.require(record['worker_sha256'] == build['workers'][workload]['sha256'], 'worker receipt mismatch')
    window.require(record['source_sha256'] == build['build']['source_sha256'], 'source receipt mismatch')
    for name, binding in record['files'].items():
        window.require(window.digest(directory / name) == binding['sha256'], 'raw evidence hash mismatch')
    inputs = frames((directory / 'input.bin').read_bytes())
    outputs = frames((directory / 'worker.stdout').read_bytes())
    if case == 'partial':
        window.require(len(outputs) == (1 if workload == 'k0' else 0), 'partial config performed workload')
        window.require(b'measurement window' in (directory / 'worker.stderr').read_bytes(), 'missing config error')
        return {'status': 'EXPECTED_CONFIG_FAILURE', 'performance_acceptance': 'NOT_APPLICABLE'}
    window.require(len(outputs) == len(inputs), 'request/response count mismatch')
    for request, response in zip(inputs, outputs):
        window.require(all(request[key] == response[key] for key in ('version', 'run', 'request', 'actor')), 'envelope mismatch')
    output = outputs[-1]['output'] if workload == 'durable' else outputs[-1]['observation']['output']
    original_path = durable.PATH if workload == 'durable' else K0_PATH
    window.require(output['build'] == build['build'], 'compiled build identity mismatch')
    prewarmup = case in ('prewarmup-continuous-v1', 'prewarmup-wait-v1')
    suffix = window.BOUNDARIES[case] if prewarmup else window.SUFFIX if case in ('active', 'stop-error') else ''
    window.require(output['path'] == original_path + suffix, 'diagnostic path mismatch')
    if prewarmup:
        window.require(workload == 'k0' and record['boundary'] == case, 'prewarmup mode/workload mismatch')
        window.require(record['pmu_scope'] == 'warmup-drain-measurement-and-control-bracket', 'prewarmup scope')
        window.require(record['window']['start_ns'] - record['window']['armed_ns'] >= output['warmup']['elapsed_ns'],
                       'warmup not enclosed by PMU arm boundary')
    if case == 'stop-error':
        window.require(output['error'] is not None and output['measurement']['completed'] == (512 if workload == 'durable' else 65536), 'gate failure lost completed measurement/error')
        window.require(b'measurement window' in (directory / 'worker.stderr').read_bytes(), 'gate error not retained')
        return {'status': 'EXPECTED_STOP_FAILURE_WITH_PARTIAL_OUTPUT', 'performance_acceptance': 'NOT_APPLICABLE'}
    if case == 'active' or prewarmup:
        window.require(record['window']['elapsed_ns'] == output['measurement']['elapsed_ns'], 'window bound to wrong measured output')
    if workload == 'durable':
        duration = record['observed_ns'] - record['invoke_ns']
        if case == 'active':
            try:
                durable.check_output(output, durable.INPUT, build['build'], duration, directory / 'work')
            except durable.oracle.OracleError as error:
                window.require('identity' in str(error), 'unexpected acceptance rejection')
            else:
                raise window.DiagnosticError('diagnostic output accepted as ordinary performance')
        normalized = copy.deepcopy(output)
        normalized['path'] = durable.PATH
        # Exact diagnostic identity verified above; raw output stays unchanged.
        semantic = durable.check_output(normalized, durable.INPUT, build['build'], duration, directory / 'work')
        return {'status': 'DIAGNOSTIC_SEMANTICS_CHECKED', 'performance_acceptance': 'NOT_APPLICABLE',
                'ack_members': semantic['observed_commits'], 'source_selected_files': semantic['source_selected_files']}
    # The Rust adapter uses the existing independent scalar/counter oracle on
    # retained bytes. It is a separate explicit selected test, not run here.
    return {'status': 'K0_IDENTITY_CHECKED_ORACLE_PENDING', 'performance_acceptance': 'NOT_APPLICABLE'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--workload', choices=('k0', 'durable'), required=True)
    parser.add_argument('--case', choices=('active', 'disabled', 'partial', 'stop-error',
                                         'prewarmup-continuous-v1', 'prewarmup-wait-v1'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    build = json.loads(args.build.read_text())
    worker = Path(build['workers'][args.workload]['path'])
    source = build['build']['source_sha256']
    args.output.mkdir(mode=0o700)
    input_file = args.output / 'request.bin'
    input_file.write_bytes(encode(requests(args.workload)))
    directory = args.output / 'run'
    if args.case in ('active', 'stop-error', 'prewarmup-continuous-v1', 'prewarmup-wait-v1'):
        original_send = window.Runner.send
        def send(runner, peer, message, end):
            if args.case == 'stop-error' and message.startswith('DONE '):
                message = 'WRONG completion'
            return original_send(runner, peer, message, end)
        with patch.object(window.Runner, 'send', send):
            window.collect(worker, input_file, source, args.workload, directory, affinity=2,
                           boundary=args.case if args.case.startswith('prewarmup-') else 'measurement')
    else:
        plain(worker, input_file, source, args.workload, directory, args.case == 'partial')
    verdict = validate(directory, args.workload, args.case, build)
    (args.output / 'semantic-result.json').write_text(json.dumps(verdict, indent=2)+'\n')
    print(json.dumps(verdict))


if __name__ == '__main__':
    main()
