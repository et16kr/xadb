#!/usr/bin/env python3
"""Retain and verify the complete frozen COMM depth-eight model campaign."""
import argparse
from datetime import datetime, timezone
from pathlib import Path
import re
import subprocess
import tempfile

from check_dev0 import check_report_paths, digest, source_fingerprint
from comm_campaign import build_test, capture_source, identity, write_json

ROOT = Path(__file__).resolve().parent.parent
TEST = 'two_connections_two_slots_depth_eight_exhaustive_model'
WITNESSES = {'rx_complete_take_release', 'tx_flush_collect', 'partial_rx_cancel',
             'partial_tx_cancel', 'full_queue_rejected', 'both_connections_progress', 'two_frame_order'}


def check_model(output, code):
    if code != 0 or not re.search(rf'^test {TEST} \.\.\. ok$', output, re.M):
        raise ValueError('model test failed or did not execute')
    if 'INCOMPLETE' in output or 'COUNTEREXAMPLE' in output:
        raise ValueError('incomplete exploration or counterexample')
    layers = [tuple(map(int, values)) for values in re.findall(
        r'^COMM_MODEL_LAYER depth=(\d+) states=(\d+) total_states=(\d+) transitions=(\d+) elapsed_ms=(\d+)$', output, re.M)]
    if [row[0] for row in layers] != list(range(1, 9)):
        raise ValueError('missing, repeated or wrong exploration depth')
    total, transitions, elapsed = 1, 0, 0
    for depth, count, cumulative, edges, ms in layers:
        total += count
        if count <= 0 or cumulative != total or edges <= transitions or ms < elapsed:
            raise ValueError('inconsistent model progress counters')
        transitions, elapsed = edges, ms
    complete = re.findall(r'^COMM_MODEL_COMPLETE depth=(\d+) states=(\d+) transitions=(\d+) witnesses=(\d+)$', output, re.M)
    if len(complete) != 1 or tuple(map(int, complete[0])) != (8, total, transitions, len(WITNESSES)):
        raise ValueError('missing or inconsistent complete receipt')
    names = re.findall(r'^COMM_MODEL_WITNESS (\w+) \[Event ', output, re.M)
    if len(names) != len(WITNESSES) or set(names) != WITNESSES:
        raise ValueError('missing, duplicate or unexpected reachability witness')
    if total > 360_000_000 or transitions > 5_000_000_000 or elapsed >= 21_600_000:
        raise ValueError('exploration safety budget exceeded')
    return {'depth': 8, 'states': total, 'transitions': transitions, 'elapsed_ms': elapsed,
            'witnesses': sorted(names), 'layers': layers}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/comm'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='model-', dir=parent))
    source = session / 'source'
    report = {'schema_version': 1, 'status': 'RUNNING', 'scope': 'C-V14 complete finite model, local only',
              'date': datetime.now(timezone.utc).isoformat(), 'origin': identity(ROOT),
              'execution_root': str(source), 'key_version': 'comm-model-key-v1'}
    exit_code = 0
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        report['generator_sha256'] = {name: digest(source / name) for name in (
            'crates/transport/tests/model.rs', 'crates/transport/tests/exploration/mod.rs',
            'crates/transport/tests/support/mod.rs', 'scripts/check_comm_model.py')}
        report['contract_sha256'] = {name: digest(source / name) for name in (
            'devdoc/contracts/comm-design.md', 'devdoc/plans/comm-implementation-plan.md')}
        write_json(session / 'manifest.json', report)
        print(f'C-V14 source: {source}', flush=True)
        binary, report['build'] = build_test(source, 'model', session, features=['test-support'], release=True)
        # Only this exact test executes. The default libtest scheduler emits the
        # named result on its own line, separately from --nocapture progress.
        command = [str(binary), TEST, '--exact', '--ignored', '--nocapture']
        report['command'] = command
        with (session / 'execution.log').open('wb') as log:
            process = subprocess.Popen(command, cwd=source, stdout=log, stderr=subprocess.STDOUT)
            report['pid'] = process.pid
            write_json(session / 'running.json', report)
            print(f'C-V14 pid={process.pid} binary={report["build"]["binary_sha256"]}', flush=True)
            code = process.wait(timeout=21_660)
        report['exit_code'] = code
        report['log_sha256'] = digest(session / 'execution.log')
        report['model'] = check_model((session / 'execution.log').read_text(), code)
        if source_fingerprint(source) != report['source_sha256']:
            raise ValueError('retained model source changed during execution')
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
    print(f'C-V14 {report["status"]}: {session / "validation.json"}', flush=True)
    return exit_code


if __name__ == '__main__':
    raise SystemExit(main())
