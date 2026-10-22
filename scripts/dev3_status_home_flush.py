#!/usr/bin/env python3
"""Select only frozen new Status flush cases on a source-bound test binary."""
import argparse
from collections import OrderedDict
import csv
import hashlib
import io
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
from dev3_status_home_flush_literals import inspect_witness

ROOT = Path(__file__).resolve().parent.parent
FIXTURE_REL = Path('tests/fixtures/dev3-status-home-flush')
INPUT_REL = Path('crates/engine/tests/fixtures/dev3-status-home-flush/runtime/manifest.json')
FIXTURE_SHA256 = '245dfef3871ee84ac86e2a681266a8b0ca8807c3e3bbd069d60d001d367255e1'
RUNTIME_SHA256 = '6bfeb4a276eb055e6b5914b7324920bab89f20d0e815f4de23650575110f1d54'
PREFIX = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_init::status_flush::tests::'
BUILD_COMMAND = ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--all-features', '--no-run', '--locked', '--offline', '--message-format=json']
BEGIN, CASE, VARIANT = 'STATUS_FLUSH_CASE_BEGIN\t', 'STATUS_FLUSH_CASE\t', 'STATUS_FLUSH_VARIANT\t'
COMPLETE, NEGATIVES = 'STATUS_FLUSH_COMPLETE ', 'STATUS_FLUSH_NEGATIVES '


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def read_tsv(path):
    with path.open() as stream:
        return list(csv.DictReader(stream, delimiter='\t'))


def runtime_input(root=ROOT, expected=RUNTIME_SHA256):
    path = root / INPUT_REL
    require(digest(path) == expected, 'FAIL_RUNTIME_INPUT_IDENTITY')
    result = json.loads(path.read_text())
    require(result['schema'] == 'xadb-status-home-flush-runtime-v1' and result['unit'] == 'D3-STATUS-FLUSH-001', 'FAIL_RUNTIME_INPUT_SCHEMA')
    return result


def check_inputs(root=ROOT):
    base = root / FIXTURE_REL
    require(digest(base / 'manifest.json') == FIXTURE_SHA256, 'FAIL_FIXTURE_MANIFEST_IDENTITY')
    manifest = json.loads((base / 'manifest.json').read_text())
    expected = {'manifest.json', *(row['path'] for row in manifest['outputs'])}
    actual = {str(path.relative_to(base)) for path in base.rglob('*') if path.is_file()}
    require(expected == actual and not any(path.is_symlink() for path in base.rglob('*')), 'FAIL_FIXTURE_CENSUS')
    for row in manifest['outputs']:
        path = base / row['path']
        require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'FAIL_LITERAL_IDENTITY')
    require(manifest['generator']['sha256'] == digest(root / manifest['generator']['path']), 'FAIL_GENERATOR_IDENTITY')
    for row in manifest['provenance_inputs']:
        # Ignored historical design artifacts are hash-labelled provenance, not
        # dependencies of a clean source export or a newly executed test.
        if row['path'].startswith('artifacts/'):
            continue
        path = root / row['path']
        require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'FAIL_PROVENANCE_IDENTITY')
    require((manifest['mandatory_cases'], manifest['rust_mandatory'], manifest['python_witness_cases'],
             manifest['fault_tuples'], manifest['mandatory_fault_aliases'], manifest['s5_history_extra'],
             manifest['unique_scenarios']) == (43, 42, 1, 570, 6, 24, 631), 'FAIL_INPUT_DENOMINATOR')
    return digest(base / 'manifest.json')


def selections(requested, spec):
    ordered = list(spec['cases'])
    selected = []
    for item in requested:
        names = spec['groups'].get(item, [item])
        require(bool(names) and all(name in ordered and name not in selected for name in names), 'FAIL_DUPLICATE_OR_UNKNOWN_SELECTION')
        selected.extend(names)
    require(bool(selected), 'FAIL_EMPTY_SELECTION')
    return [name for name in ordered if name in selected]


def jobs(selected, spec):
    groups = OrderedDict()
    for name in selected:
        selector = spec['cases'][name]['selector']
        groups.setdefault(selector, []).append(name)
    return list(groups.values())


def check_build_receipt(build, source_root, source_sha256, binary_sha256):
    require(build.get('status') == 'PASS' and build.get('source_sha256') == source_sha256
            and build.get('binary_sha256') == binary_sha256
            and build.get('source_root') is not None
            and Path(build['source_root']).resolve() == source_root.resolve()
            and build.get('command') == BUILD_COMMAND and build.get('exit_code') == 0
            and build.get('errors') == 0, 'FAIL_BUILD_BINDING')


def remaining_seconds(chosen, consumed, spec):
    classes = {spec['cases'][name]['budget'] for name in chosen}
    require(len(classes) == 1, 'FAIL_MIXED_BUDGET_JOB')
    group = classes.pop()
    left = spec['budget_seconds'][group] - consumed.get(group, 0)
    require(left > 0, 'FAIL_AGGREGATE_BUDGET')
    return group, left


def check_runtime(output, selected, spec, *, python=False):
    cases = spec['cases']
    selector = cases[selected[0]]['selector']
    require(all(cases[name]['selector'] == selector for name in selected), 'FAIL_MIXED_SELECTOR')
    lines = output.splitlines()
    if not python:
        require(output.count('test result:') == 1 and 'test result: ok. 1 passed; 0 failed;' in output, 'FAIL_EXACT_TEST_CENSUS')
        tests = [line for line in lines if line.startswith('test ') and not line.startswith('test result:')]
        prefix = 'test ' + selector + ' ...'
        require(len(tests) == 1 and tests[0].startswith(prefix) and
                (len(tests[0]) == len(prefix) or tests[0][len(prefix)] == ' '), 'FAIL_EXACT_TEST_NAME')
        lines = [line[len(prefix):].lstrip() if line.startswith(prefix) else line for line in lines]
    expected = list(selected)
    active, complete, passed, variants, resources, negative_rows = None, [], [], {}, {}, []
    for line in lines:
        if line.startswith(BEGIN):
            name = line[len(BEGIN):]
            require(active is None and len(passed) < len(expected) and name == expected[len(passed)] and not complete, 'FAIL_CASE_BEGIN_ORDER')
            active = name
        elif line.startswith(VARIANT):
            parts = line[len(VARIANT):].split('\t')
            require(len(parts) == 2 and parts[0] == active, 'FAIL_VARIANT_OWNER')
            variants.setdefault(parts[0], []).append(parts[1])
        elif line.startswith(CASE):
            name = line[len(CASE):]
            require(name == active, 'FAIL_CASE_COMPLETION_ORDER')
            require(variants.get(name, []) == cases[name]['variants'], 'FAIL_VARIANT_CENSUS')
            passed.append(name); active = None
        elif line.startswith(COMPLETE):
            complete.append(line[len(COMPLETE):])
            require(active is None and passed == expected, 'FAIL_GROUP_BEFORE_CASES')
        elif line.startswith(NEGATIVES):
            require(active == 'RF03', 'FAIL_NEGATIVES_OWNER')
            negative_rows.append(line[len(NEGATIVES):])
        elif line.startswith('RESOURCE_STATUS_FLUSH '):
            parts = line.split()
            require(active == 'RF01' and len(parts) == 4 and parts[1] not in resources,
                    'FAIL_RESOURCE_CENSUS')
            name = parts[1]
            require(name in spec['resource_caps'], 'FAIL_RESOURCE_NAME')
            actual, cap = int(parts[2]), int(parts[3])
            require(0 <= actual <= cap == spec['resource_caps'][name], 'FAIL_RESOURCE_CAP')
            resources[name] = actual
        elif line.startswith(('STATUS_FLUSH_', 'RESOURCE_STATUS_FLUSH')):
            raise ValueError('FAIL_UNKNOWN_PROTOCOL')
    group = cases[selected[0]]['group']
    require(active is None and passed == expected and complete == [f'{group} {len(expected)}'], 'FAIL_FINAL_CASE_CENSUS')
    require(negative_rows == (['12'] if 'RF03' in selected else []), 'FAIL_NEGATIVE_CENSUS')
    require(set(resources) == (set(spec['resource_caps']) if 'RF01' in selected else set()), 'FAIL_RESOURCE_CENSUS')
    return dict(cases=passed, variants=variants, resources=resources, known_bad=12 if negative_rows else 0)


def raw_progress(output, selector):
    """Journal raw reach/completion claims even when later checks fail.

    These arrays are deliberately not labelled PASS. A selected selector may
    abort before later selected cases are reached; dispatch is not execution.
    """
    prefix = 'test ' + selector + ' ...'
    lines = [line[len(prefix):].lstrip() if line.startswith(prefix) else line
             for line in output.splitlines()]
    return dict(raw_case_begin=[line[len(BEGIN):] for line in lines if line.startswith(BEGIN)],
                raw_case_completed=[line[len(CASE):] for line in lines if line.startswith(CASE)])


def run_witness(root=ROOT):
    frozen = json.loads((root / FIXTURE_REL / 'known-bad.json').read_text())
    inspect_witness(frozen['valid'])
    require([row['id'] for row in frozen['negatives']] == [f'N{i:02}' for i in range(1, 13)], 'FAIL_WITNESS_CENSUS')
    rejected = []
    for row in frozen['negatives']:
        try:
            inspect_witness(row['witness'])
        except ValueError as error:
            require(str(error) == row['expected_rejection'] == row['id'], 'FAIL_WITNESS_REASON')
            rejected.append(row['id'])
        else:
            raise ValueError('FAIL_WITNESS_FALSE_ACCEPT')
    return BEGIN + 'RF03\n' + NEGATIVES + '12\n' + CASE + 'RF03\n' + COMPLETE + 'RF 1\n', rejected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', action='append', required=True)
    args = parser.parse_args()
    require(not args.output.exists(), 'FAIL_EXISTING_ARTIFACT_DIRECTORY')
    spec = runtime_input()
    require(spec['freeze_status'] == 'FROZEN_INPUT_NOT_RUN', 'FAIL_RUNTIME_INPUT_NOT_FROZEN')
    selected = selections(args.case, spec)
    args.output.mkdir(parents=True)
    binary = args.binary.resolve()
    source = source_fingerprint(ROOT)
    report = dict(unit='D3-STATUS-FLUSH-001', status='RUNNING', source_root=str(ROOT), source_sha256=source,
                  binary=str(binary), binary_sha256=digest(binary), selected=selected,
                  build_receipt=str(args.build_receipt.resolve()), build_receipt_sha256=digest(args.build_receipt),
                  runtime_input_sha256=digest(ROOT / INPUT_REL), results=[], elapsed_seconds={},
                  witness_scope='independent Python RF03; never relabelled as Rust product test',
                  artifact_directory=str(args.output.resolve()))
    start = time.monotonic()
    try:
        report['inputs_sha256'] = check_inputs()
        check_build_receipt(json.loads(args.build_receipt.read_text()), ROOT, source, report['binary_sha256'])
        for chosen in jobs(selected, spec):
            budget, timeout = remaining_seconds(chosen, report['elapsed_seconds'], spec)
            directory = args.output / chosen[0]
            directory.mkdir()
            selector = spec['cases'][chosen[0]]['selector']
            python = selector == 'python:RF03'
            row = dict(selected=chosen, selector=selector, status='RUNNING', timeout_seconds=timeout,
                       executor='python-independent-witness' if python else 'rust-product', directory=str(directory.resolve()))
            report['results'].append(row)
            began = time.monotonic()
            if python:
                output, reasons = run_witness()
                (directory / 'stdout.log').write_text(output)
                (directory / 'stderr.log').write_text('')
                row.update(exit_code=0, rejected=reasons)
            else:
                command = [str(binary), '--exact', selector, '--nocapture', '--test-threads=1']
                if spec['cases'][chosen[0]]['ignored']:
                    command.append('--ignored')
                environment = os.environ.copy()
                for key in tuple(environment):
                    if key.startswith('XADB_'):
                        environment.pop(key)
                environment['XADB_STATUS_FLUSH_CASES'] = ','.join(chosen)
                row['command'] = command
                row['environment'] = {'XADB_STATUS_FLUSH_CASES': environment['XADB_STATUS_FLUSH_CASES']}
                with (directory / 'stdout.log').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
                    child = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, start_new_session=True)
                    row['pid'] = child.pid
                    row['timed_out'] = False
                    wait_process_group(child, row, timeout)
                output = (directory / 'stdout.log').read_text()
            elapsed = time.monotonic() - began
            row['elapsed_seconds'] = elapsed
            report['elapsed_seconds'][budget] = report['elapsed_seconds'].get(budget, 0) + elapsed
            row.update(raw_progress(output, selector))
            require(report['elapsed_seconds'][budget] <= spec['budget_seconds'][budget], 'FAIL_AGGREGATE_BUDGET')
            require(row['exit_code'] == 0 and not row.get('timed_out', False), 'FAIL_WORKER_OR_TIMEOUT')
            row.update(check_runtime(output, chosen, spec, python=python))
            row['stdout_sha256'] = digest(directory / 'stdout.log')
            row['stderr_sha256'] = digest(directory / 'stderr.log')
            row['status'] = 'PASS'
        require(source_fingerprint(ROOT) == source and digest(binary) == report['binary_sha256']
                and digest(args.build_receipt) == report['build_receipt_sha256']
                and runtime_input() == spec and check_inputs() == report['inputs_sha256'], 'FAIL_POST_RUN_IDENTITY')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        if report['results'] and report['results'][-1]['status'] == 'RUNNING':
            failed = report['results'][-1]
            failed['status'] = 'FAIL'
            if 'elapsed_seconds' not in failed:
                failed['elapsed_seconds'] = time.monotonic() - began
                report['elapsed_seconds'][budget] = report['elapsed_seconds'].get(budget, 0) + failed['elapsed_seconds']
            for name in ('stdout', 'stderr'):
                path = Path(failed['directory']) / (name + '.log')
                if path.is_file():
                    failed[name + '_sha256'] = digest(path)
                    if name == 'stdout':
                        failed.update(raw_progress(path.read_text(), failed['selector']))
    dispatched = {key for row in report['results'] for key in row['selected']}
    reached = {key for row in report['results'] for key in row.get('raw_case_begin', [])}
    report['selected_for_dispatch'] = [key for key in spec['cases'] if key in dispatched]
    report['reached_case_begin'] = [key for key in spec['cases'] if key in reached]
    report['dispatched_not_reached'] = [key for key in spec['cases'] if key in dispatched - reached]
    report['not_run'] = [key for key in spec['cases'] if key not in reached]
    report['completed_selected'] = [key for row in report['results'] if row['status'] == 'PASS' for key in row['selected']]
    report['total_elapsed_seconds'] = time.monotonic() - start
    (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report.get(k) for k in ('status', 'error', 'selected', 'source_sha256')}))
    return int(report['status'] != 'PASS')


if __name__ == '__main__':
    sys.exit(main())
