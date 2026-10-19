#!/usr/bin/env python3
"""Run actual thirty-minute COMM campaigns against a retained release test binary."""
import argparse
from datetime import datetime, timezone
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

from check_dev0 import check_report_paths, digest, source_fingerprint
from comm_campaign import build_test, capture_source, identity, write_json

ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    'codec': {
        'suite': 'codec_fuzz', 'test': 'codec_fuzz_thirty_minutes', 'corpus': 13,
        'inputs': ('crates/transport/tests/codec_fuzz.rs', 'crates/transport/tests/soak_support/mod.rs',
                   'crates/transport/tests/support/mod.rs', 'crates/transport/tests/corpus/codec.hex',
                   'scripts/check_comm_soak.py'),
    },
    'events': {
        'suite': 'event_fuzz', 'test': 'event_sequence_fuzz_thirty_minutes', 'corpus': 8,
        'inputs': ('crates/transport/tests/event_fuzz.rs', 'crates/transport/tests/soak_support/mod.rs',
                   'crates/transport/tests/support/mod.rs', 'scripts/check_comm_soak.py'),
    },
    'threads': {
        'suite': 'thread_stress', 'test': 'producer_lease_thread_stress_thirty_minutes', 'corpus': 8,
        'inputs': ('crates/transport/tests/thread_stress.rs', 'crates/transport/tests/support/mod.rs',
                   'scripts/check_comm_soak.py'),
    },
}


def check_soak(output, code, target, controller_seconds):
    spec = TARGETS[target]
    if code != 0 or not re.search(rf'^test {spec["test"]} \.\.\. ok$', output, re.M):
        raise ValueError('soak test failed or did not execute')
    receipts = re.findall(r'^COMM_SOAK_COMPLETE target=(\w+) elapsed_ms=(\d+) iterations=(-?\d+) seed=(\d+) corpus=(\d+) crashes=(\d+)$', output, re.M)
    if len(receipts) != 1:
        raise ValueError('missing or repeated soak receipt')
    name, milliseconds, iterations, seed, corpus, crashes = receipts[0]
    milliseconds, iterations, seed, corpus, crashes = map(int, (milliseconds, iterations, seed, corpus, crashes))
    reported_iterations = iterations
    iteration_encoding = 'unsigned_decimal'
    if name != target or milliseconds < 1_800_000 or controller_seconds < 1800 or seed != 20_261_019 or corpus != spec['corpus'] or crashes != 0:
        raise ValueError('short/empty/wrong campaign or crash')
    event_counts = None
    thread_counts = None
    if target == 'events':
        matches = re.findall(r'^COMM_EVENT_COUNTS target=events counts=([0-9,]+)$', output, re.M)
        if len(matches) != 1:
            raise ValueError('missing/repeated event-kind coverage')
        event_counts = [int(value) for value in matches[0].split(',')]
        actual_iterations = sum(event_counts)
        if (len(event_counts) != 26 or min(event_counts) <= 0
                or max(event_counts) > 0xFFFF_FFFF_FFFF_FFFF
                or actual_iterations > 0xFFFF_FFFF_FFFF_FFFF):
            raise ValueError('event-kind coverage is incomplete or inconsistent')
        if iterations == actual_iterations:
            # The corrected fixture prints its u64 total directly.
            iteration_encoding = 'u64_exact'
        else:
            # Earlier release fixtures inferred an i32 counter. Reconstruct one
            # wrap only when the complete u64 event-count sum proves it exactly.
            if actual_iterations > 0xFFFF_FFFF:
                raise ValueError('legacy event iteration counter wrapped more than once')
            expected_i32 = actual_iterations if actual_iterations <= 0x7FFF_FFFF else actual_iterations - (1 << 32)
            if iterations != expected_i32:
                raise ValueError('event iteration receipt differs from complete event-kind totals')
            iteration_encoding = 'legacy_i32_wrapped'
        if actual_iterations < 10_000:
            raise ValueError('event iteration receipt differs from complete event-kind totals')
        iterations = actual_iterations
    elif iterations < 10_000:
        raise ValueError('short/empty/wrong campaign or crash')
    if target == 'threads':
        matches = re.findall(
            r'^COMM_THREAD_COUNTS target=threads rounds=(\d+) attempts=(\d+) accepted=(\d+) '
            r'backpressured=(\d+) rejected=(\d+) race_accepted=(\d+) race_rejected=(\d+) '
            r'postclose_rejected=(\d+) leases=(\d+) flushed=(\d+) failed=(\d+)$',
            output, re.M
        )
        if len(matches) != 1:
            raise ValueError('missing/repeated thread activity receipt')
        (rounds, attempts, accepted, backpressured, rejected, race_accepted, race_rejected,
         postclose_rejected, leases, flushed, failed) = map(int, matches[0])
        drain_rounds = (rounds + 1) // 3
        non_drain_rounds = rounds - drain_rounds
        thread_counts = {'rounds': rounds, 'attempts': attempts, 'accepted': accepted,
                         'backpressured': backpressured, 'rejected': rejected,
                         'race_accepted': race_accepted, 'race_rejected': race_rejected,
                         'postclose_rejected': postclose_rejected,
                         'leases': leases, 'flushed': flushed, 'failed': failed}
        if (rounds <= 0 or attempts != iterations or attempts != 16 * rounds
                or attempts != accepted + backpressured + rejected or accepted != 4 * rounds + race_accepted
                or backpressured != 4 * rounds or rejected != race_rejected + postclose_rejected
                or race_accepted + race_rejected != 4 * rounds or race_accepted < rounds
                or postclose_rejected != 4 * rounds or leases != 2 * rounds
                or flushed < 4 * rounds + drain_rounds or failed < non_drain_rounds
                or accepted != flushed + failed):
            raise ValueError('thread race coverage is incomplete or inconsistent')
    return {'target': name, 'elapsed_ms': milliseconds, 'controller_seconds': controller_seconds,
            'iterations': iterations, 'iterations_reported': reported_iterations,
            'iteration_encoding': iteration_encoding, 'seed': seed, 'corpus': corpus, 'crashes': crashes,
            'event_counts': event_counts, 'thread_counts': thread_counts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', choices=sorted(TARGETS), required=True)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/comm'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix=f'{args.target}-soak-', dir=parent))
    source = session / 'source'
    spec = TARGETS[args.target]
    campaign = 'C-V14 thread stress' if args.target == 'threads' else f'C-V17 {args.target} fuzz'
    report = {'schema_version': 1, 'status': 'RUNNING', 'scope': f'{campaign}, local only',
              'date': datetime.now(timezone.utc).isoformat(), 'origin': identity(ROOT), 'execution_root': str(source)}
    exit_code = 0
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        report['generator_sha256'] = {name: digest(source / name) for name in spec['inputs']}
        write_json(session / 'manifest.json', report)
        print(f'COMM {args.target} soak source: {source}', flush=True)
        binary, report['build'] = build_test(source, spec['suite'], session, release=True)
        command = [str(binary), spec['test'], '--exact', '--ignored', '--nocapture']
        report['command'] = command
        environment = os.environ.copy()
        environment['XADB_COMM_CRASH_DIRECTORY'] = str(session / 'crashes')
        started = time.monotonic()
        with (session / 'execution.log').open('wb') as log:
            process = subprocess.Popen(command, cwd=source, env=environment, stdout=log, stderr=subprocess.STDOUT)
            report['pid'] = process.pid
            write_json(session / 'running.json', report)
            print(f'COMM {args.target} soak pid={process.pid}', flush=True)
            code = process.wait(timeout=2100)
        report['exit_code'] = code
        report['log_sha256'] = digest(session / 'execution.log')
        report['campaign'] = check_soak((session / 'execution.log').read_text(), code, args.target, time.monotonic() - started)
        if source_fingerprint(source) != report['source_sha256']:
            raise ValueError('retained soak source changed during execution')
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        if 'process' in locals() and process.poll() is None:
            process.kill()
            process.wait(timeout=5)
        report['status'] = 'FAIL'
        report['error'] = str(error)
        exit_code = 1
    write_json(session / 'validation.json', report)
    if args.report:
        write_json(args.report, report)
    print(f'COMM {args.target} soak {report["status"]}: {session / "validation.json"}', flush=True)
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
