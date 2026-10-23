#!/usr/bin/env python3
"""Run explicit C-V18 cell batches within a deadline; never qualify full C1."""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time

from check_comm_performance import (
    INSTRUMENTATION, MEASURE_SECONDS, PAIRS, WARMUP_SECONDS,
    matrix, run_case, slow_peer_matrix, summarize_pair_noise, summarize_pairs,
)
from check_dev0 import check_report_paths, digest, source_fingerprint
from comm_campaign import capture_source, write_json

ROOT = Path(__file__).resolve().parent.parent
CATALOGUE = matrix() + slow_peer_matrix()
BUILD_SECONDS = 600
MODE_SECONDS = 120
CLEANUP_SECONDS = 10
FINALIZE_SECONDS = 120
EIGHT_HOURS = 8 * 60 * 60


def plan_batch(selected, deadline, batch_seconds, now):
    """Freeze selection and conservative bounds before build or measurement."""
    if (not selected or any(type(index) is not int or not 0 <= index < len(CATALOGUE)
                            for index in selected) or len(set(selected)) != len(selected)):
        raise ValueError('cell indices must be unique integers in 0..134')
    if deadline.tzinfo is None or deadline.utcoffset() is None:
        raise ValueError('deadline requires an explicit timezone')
    if deadline <= now:
        raise ValueError('deadline has already passed')
    if type(batch_seconds) is not int or not 0 < batch_seconds < EIGHT_HOURS:
        raise ValueError('batch budget must be positive and less than eight hours')
    modes = len(selected) * PAIRS * 2
    conservative = BUILD_SECONDS + modes * (MODE_SECONDS + CLEANUP_SECONDS) + FINALIZE_SECONDS
    available = min(batch_seconds, (deadline - now).total_seconds())
    return {
        'selected_cell_indices': list(selected), 'selected_mode_runs': modes,
        'full_profile_cells': len(CATALOGUE), 'full_profile_mode_runs': len(CATALOGUE) * PAIRS * 2,
        'deadline': deadline.isoformat(), 'planned_at': now.isoformat(),
        'batch_seconds': batch_seconds, 'available_seconds': available,
        'nominal_measurement_seconds': modes * (WARMUP_SECONDS + MEASURE_SECONDS),
        'conservative_seconds': conservative, 'mode_seconds': MODE_SECONDS,
        'build_seconds': BUILD_SECONDS, 'cleanup_seconds': CLEANUP_SECONDS,
        'finalize_reserve_seconds': FINALIZE_SECONDS,
        'can_start': conservative < EIGHT_HOURS and conservative <= available,
        'skip_reason': None if conservative < EIGHT_HOURS and conservative <= available
                       else 'selected batch exceeds eight-hour/batch/deadline budget; split it',
    }


def require_time(deadline, monotonic_end, minimum_seconds):
    remaining = min((deadline - datetime.now(timezone.utc)).total_seconds(),
                    monotonic_end - time.monotonic())
    if remaining < minimum_seconds + FINALIZE_SECONDS:
        raise TimeoutError('insufficient time for next operation plus final evidence reserve')
    return remaining - FINALIZE_SECONDS


@contextmanager
def mode_timeout(seconds):
    """Linux main-thread one-shot alarm; run_case owns worker kill/reap on error."""
    if signal.getitimer(signal.ITIMER_REAL) != (0.0, 0.0):
        raise ValueError('an existing real-time alarm must not be replaced')
    previous = signal.getsignal(signal.SIGALRM)

    def expired(_signum, _frame):
        signal.setitimer(signal.ITIMER_REAL, 0)
        raise TimeoutError('selected performance mode exceeded its time budget')

    signal.signal(signal.SIGALRM, expired)
    try:
        signal.setitimer(signal.ITIMER_REAL, seconds)
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


def batch_summary(records, selected, source_hash, binary_hash):
    """Require all ten original pairs; partial coverage can never become C1 PASS."""
    for record in records:
        index = record.get('cell_index')
        if (type(index) is not int or index not in selected or record.get('cell') != CATALOGUE[index]
                or record.get('source_sha256') != source_hash
                or record.get('binary_sha256') != binary_hash
                or type(record.get('pair')) is not int):
            raise ValueError('batch receipt has wrong cell/source/binary/pair identity')
    comparisons = summarize_pairs(records, len(selected), PAIRS, [CATALOGUE[i] for i in selected])
    noise = summarize_pair_noise(comparisons, PAIRS)
    passed = all(row['throughput_guardrail_pass'] and row['p99_guardrail_pass']
                 and row['quota_guardrail_pass'] for row in noise)
    return {'status': 'BATCH_PASS' if passed else 'FAIL', 'c1_status': 'INCOMPLETE',
            'full_profile_mode_runs': len(CATALOGUE) * PAIRS * 2,
            'selected_cell_indices': list(selected), 'completed_mode_runs': len(records),
            'comparisons': comparisons, 'paired_noise_summary': noise}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cell-index', type=int, action='append', required=True,
                        help='original zero-based profile index; repeat for an explicit batch')
    parser.add_argument('--deadline', required=True, help='timezone-aware ISO deadline for this job')
    parser.add_argument('--batch-seconds', type=int, default=3600,
                        help='wall budget including build and cleanup; must be less than eight hours')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    try:
        check_report_paths([args.report])
        deadline = datetime.fromisoformat(args.deadline)
        plan = plan_batch(args.cell_index, deadline, args.batch_seconds, datetime.now(timezone.utc))
    except (OSError, ValueError) as error:
        parser.error(str(error))
    parent = ROOT / 'artifacts/comm'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='performance-batch-', dir=parent))
    report = {'schema_version': 1, 'scope': 'C-V18 selected original cell batches only',
              'status': 'RUNNING', 'c1_status': 'INCOMPLETE', 'plan': plan,
              'environment': {'uname': list(platform.uname()), 'python': sys.version,
                              'cpu_count': os.cpu_count(), 'initial_load_average': os.getloadavg()},
              'attempted_mode_runs': 0, 'completed_mode_runs': 0, 'record_paths': []}
    write_json(session / 'plan.json', report)
    if not plan['can_start']:
        report.update(status='SKIPPED_TIME_BUDGET', reason=plan['skip_reason'])
        write_json(session / 'validation.json', report)
        if args.report:
            write_json(args.report, report)
        print(f'C-V18 batch SKIPPED_TIME_BUDGET: {session / "validation.json"}', flush=True)
        return 0
    source = session / 'source'
    monotonic_end = time.monotonic() + plan['available_seconds']
    records = []
    try:
        require_time(deadline, monotonic_end, plan['conservative_seconds'] - FINALIZE_SECONDS)
        report['source_sha256'] = capture_source(ROOT, source)
        report['generator_sha256'] = {name: digest(source / name) for name in (
            'scripts/check_comm_performance_batch.py', 'scripts/check_comm_performance.py',
            'scripts/comm_campaign.py', 'crates/testing/transport-worker/src/bin/comm_performance.rs')}
        report['execution_root'] = str(source)
        command = ['cargo', 'build', '--release', '-p', 'xadbtest-transport-worker',
                   '--bin', 'comm_performance', '--locked', '--offline',
                   '--target-dir', str(session / 'build')]
        require_time(deadline, monotonic_end, BUILD_SECONDS)
        with (session / 'build.log').open('xb') as log:
            built = subprocess.run(command, cwd=source, stdout=log, stderr=subprocess.STDOUT,
                                   timeout=BUILD_SECONDS)
        if built.returncode:
            raise ValueError('performance fixture build failed')
        binary = session / 'build/release/comm_performance'
        report['worker'] = {'command': command, 'sha256': digest(binary),
                            'build_log_sha256': digest(session / 'build.log')}
        write_json(session / 'manifest.json', report)
        print(f'C-V18 batch source: {source}', flush=True)
        for pair in range(1, PAIRS + 1):
            order = INSTRUMENTATION if pair % 2 else tuple(reversed(INSTRUMENTATION))
            for index in plan['selected_cell_indices']:
                for instrumentation in order:
                    require_time(deadline, monotonic_end, MODE_SECONDS + CLEANUP_SECONDS)
                    report['attempted_mode_runs'] += 1
                    directory = session / 'cases' / f'pair-{pair:02}' / f'cell-{index:03}' / instrumentation.lower()
                    with mode_timeout(MODE_SECONDS):
                        record = run_case(binary, directory, CATALOGUE[index], instrumentation,
                                          report['source_sha256'])
                    record.update(pair=pair, cell_index=index, binary_sha256=report['worker']['sha256'])
                    write_json(directory / 'validation.json', record)
                    records.append(record)
                    report['completed_mode_runs'] = len(records)
                    report['record_paths'].append(str((directory / 'validation.json').relative_to(session)))
                    write_json(session / 'progress.json', report, replace=True)
                    print(f'cell={index} pair={pair} {instrumentation}: PASS '
                          f'({len(records)}/{plan["selected_mode_runs"]})', flush=True)
        if (source_fingerprint(source) != report['source_sha256']
                or digest(binary) != report['worker']['sha256']):
            raise ValueError('retained source or binary changed during batch')
        report.update(batch_summary(records, plan['selected_cell_indices'],
                                    report['source_sha256'], report['worker']['sha256']))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        report.update(status='FAIL', error=str(error), completed_mode_runs=len(records))
    write_json(session / 'validation.json', report)
    if args.report:
        write_json(args.report, report)
    print(f'C-V18 {report["status"]}; C1 INCOMPLETE: {session / "validation.json"}', flush=True)
    return int(report['status'] != 'BATCH_PASS')


if __name__ == '__main__':
    raise SystemExit(main())
