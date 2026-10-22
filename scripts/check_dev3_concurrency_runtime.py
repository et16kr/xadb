#!/usr/bin/env python3
"""Run selected new admission tests with a cumulative process-group watchdog."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from comm_campaign import write_json

PREFIX = 'transaction::concurrency::tests::'
SELECTORS = {
    'GA01': 'concurrency_ga01_capacity_and_generations',
    'GA02': 'concurrency_ga02_real_version_exclusion',
    'GA03': 'concurrency_ga03_simulated_admission_recheck',
    'GA04': 'concurrency_ga04_simulated_pending_owner',
    'GA05': 'concurrency_ga05_cleanup_fences_before_release',
    'GA06': 'concurrency_ga06_poison_and_blocked_drop',
    'GA07': 'concurrency_ga07_resource_and_oracle_negatives',
}
FIXTURE = 'tests/fixtures/dev3-concurrency/normal-cases.tsv'
BUDGET_SECONDS = 120
BUILD_COMMAND = ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--all-features',
                 '--no-run', '--locked', '--offline', '--message-format=json']
CHECKER_NAMES = ('check_dev3_concurrency_runtime.py', 'check_dev3_allocation_stack.py',
                 'check_dev0.py', 'comm_campaign.py')


def select(requested):
    names = list(SELECTORS) if requested is None else requested
    if not names or len(names) != len(set(names)) or set(names) - SELECTORS.keys():
        raise ValueError('empty, duplicate or unknown runtime selection')
    return names


def cases(text):
    groups = {key: [] for key in SELECTORS}
    identifiers, names = set(), set()
    for line in text.splitlines():
        if not line or line.startswith('#'):
            continue
        fields = line.split('\t')
        if len(fields) != 3:
            raise ValueError('invalid normal fixture columns')
        identifier, name, stage = fields
        group = identifier.split('-')[0]
        if (group not in groups or not re.fullmatch(r'GA0[1-7]-\d{2}', identifier)
                or not name or stage not in ('NONE', 'SIMULATED')
                or identifier in identifiers or name in names):
            raise ValueError('invalid, duplicate or unknown normal fixture case')
        identifiers.add(identifier)
        names.add(name)
        groups[group].append(line)
    if any(not rows for rows in groups.values()):
        raise ValueError('normal fixture omits a mandatory group')
    return groups


def evaluate(stdout, process, group, expected):
    if process.get('timed_out') or process.get('exit_code') != 0:
        raise ValueError('runtime process failed or timed out')
    summaries = re.findall(
        r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; '
        r'(\d+) measured; \d+ filtered out; finished in [\d.]+s$', stdout, re.M)
    if summaries != [('1', '0', '0', '0')]:
        raise ValueError('missing unique exact-test success census')
    names = re.findall(r'^test ([\w:]+) \.\.\.', stdout, re.M)
    if names != [PREFIX + SELECTORS[group]]:
        raise ValueError('executed selector differs from selected test')
    markers = re.findall(r'^(?:test [\w:]+ \.\.\. )?GUARD_CASE ([^\r\n]+)$', stdout, re.M)
    if Counter(markers) != Counter(expected):
        raise ValueError('missing, duplicate, unexpected or changed case marker')
    completed = re.findall(r'^GUARD_COMPLETE (GA\d+) (\d+)$', stdout, re.M)
    if completed != [(group, str(len(expected)))]:
        raise ValueError('missing or wrong completion marker')
    return {'status': 'PASS', 'cases': [row.split('\t')[0] for row in markers],
            'case_count': len(markers)}


def verify_build(receipt, root, binary, source_sha256, binary_sha256):
    expected = {'status': 'PASS', 'source_root': str(root), 'binary': str(binary),
                'source_sha256': source_sha256, 'binary_sha256': binary_sha256,
                'command': BUILD_COMMAND, 'exit_code': 0}
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError('build receipt does not bind this source and executable')


def run(root, binary, output, source_sha256, binary_sha256, normal_sha256,
        build_receipt, requested=None):
    root, binary, output = root.resolve(), binary.resolve(), output.resolve()
    selected = select(requested)
    fixture = root / FIXTURE
    if (source_fingerprint(root) != source_sha256 or digest(binary) != binary_sha256
            or digest(fixture) != normal_sha256):
        raise ValueError('source, binary or fixture identity mismatch')
    inventory = cases(fixture.read_text())
    build_receipt = build_receipt.resolve()
    verify_build(json.loads(build_receipt.read_text()), root, binary, source_sha256, binary_sha256)
    checker_root = Path(__file__).resolve().parent
    checker_files = {str(checker_root / name): digest(checker_root / name) for name in CHECKER_NAMES}
    for path, expected in checker_files.items():
        if digest(root / 'scripts' / Path(path).name) != expected:
            raise ValueError('executing checker differs from retained source')
    build_sha256 = digest(build_receipt)
    output.mkdir(parents=True, exist_ok=False)
    report = {'status': 'RUNNING', 'scope': 'DEV3 3.10.1.a.1 mandatory changed-code runtime',
              'source_root': str(root), 'source_sha256': source_sha256,
              'binary': str(binary), 'binary_sha256': binary_sha256,
              'build_receipt': str(build_receipt), 'build_receipt_sha256': build_sha256,
              'executing_checker_files': checker_files,
              'normal_fixture_sha256': normal_sha256, 'selected': selected,
              'not_run': [name for name in SELECTORS if name not in selected],
              'expanded_s5_status': 'NOT_RUN', 'runtime_budget_seconds': BUDGET_SECONDS,
              'runtime_elapsed_seconds': 0.0, 'groups': []}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_')):
            environment.pop(key)
    environment['PYTHONDONTWRITEBYTECODE'] = '1'
    try:
        for group in selected:
            remaining = BUDGET_SECONDS - report['runtime_elapsed_seconds']
            if remaining <= 0:
                raise RuntimeError('selected runtime bundle exhausted its 120 second budget')
            directory = output / group
            directory.mkdir()
            command = [str(binary), '--exact', PREFIX + SELECTORS[group],
                       '--nocapture', '--test-threads=1']
            process = {'command': command, 'exit_code': None, 'timed_out': False,
                       'timeout_seconds': remaining}
            row = {'group': group, 'status': 'RUNNING', 'process': process}
            report['groups'].append(row)
            started = time.monotonic()
            try:
                with (directory / 'stdout.log').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
                    with subprocess.Popen(command, cwd=root, env=environment, stdout=stdout,
                                          stderr=stderr, start_new_session=True) as child:
                        process['pid'] = child.pid
                        wait_process_group(child, process, remaining)
            finally:
                process['elapsed_seconds'] = time.monotonic() - started
                report['runtime_elapsed_seconds'] += process['elapsed_seconds']
                row['logs'] = {str(path): digest(path) for path in directory.iterdir() if path.is_file()}
            row.update(evaluate((directory / 'stdout.log').read_text(), process, group, inventory[group]))
            if report['runtime_elapsed_seconds'] > BUDGET_SECONDS:
                raise RuntimeError('selected runtime bundle exceeded its 120 second budget')
        if (source_fingerprint(root) != source_sha256 or digest(binary) != binary_sha256
                or digest(fixture) != normal_sha256 or digest(build_receipt) != build_sha256
                or any(digest(Path(path)) != expected for path, expected in checker_files.items())):
            raise ValueError('source, binary or fixture changed during execution')
        report['status'] = 'PASS'
    except BaseException as error:
        report['status'], report['error'] = 'FAIL', str(error)
        for row in report['groups']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
        raise
    finally:
        attempted = {row['group'] for row in report['groups']}
        report['not_run'] = [name for name in SELECTORS if name not in attempted]
        write_json(output / 'result.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--binary-sha256', required=True)
    parser.add_argument('--normal-sha256', required=True)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--case', action='append', choices=SELECTORS)
    args = parser.parse_args()
    report = run(args.root, args.binary, args.output, args.source_sha256,
                 args.binary_sha256, args.normal_sha256, args.build_receipt, args.case)
    print(json.dumps({'status': report['status'], 'selected': report['selected'],
                      'seconds': report['runtime_elapsed_seconds']}))


if __name__ == '__main__':
    main()
