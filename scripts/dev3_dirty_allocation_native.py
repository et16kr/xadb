#!/usr/bin/env python3
"""Three fresh Linux cases for the new guarded dirty allocator, no recovery claim."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from dev3_allocation_native import inventory, parse_manifest, check_files, sha, require, WAL
from dev3_doublewrite_native import native_objects
from check_dev3_dirty_runtime import verify_build

FIXTURES = Path('tests/fixtures/dev3-dirty-allocation')
NAMESPACE = bytes(range(1, 17))
CASES = ('normal', 'reserve-write-half', 'cancel-sync-after')
SELECTOR = ('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
            'dirty_allocation::native_tests::dirty_allocation_native_selected')
CASE_SECONDS = 30
DB_LIMIT = 24 << 20
TOTAL_LIMIT = 72 << 20


def check_census(cases):
    require(tuple(cases) == CASES, 'FAIL_NATIVE_CENSUS')


def frozen_inputs(root):
    manifest = json.loads((root / FIXTURES / 'manifest.json').read_text())
    require(len(manifest['literals']) == 63, 'FAIL_FIXTURE_CENSUS')
    seen = set()
    for row in manifest['literals']:
        require(row['path'] not in seen, 'FAIL_FIXTURE_CENSUS')
        seen.add(row['path'])
        data = (root / FIXTURES / row['path']).read_bytes()
        require(len(data) == row['bytes'] and sha(data) == row['sha256'], 'FAIL_FIXTURE_IDENTITY')
    plan = json.loads((root / FIXTURES / 'native-manifest.json').read_text())
    require(plan['cases'] == list(CASES) and plan['selector'] == SELECTOR
            and plan['case_timeout_seconds'] == CASE_SECONDS
            and plan['database_limit_bytes'] == DB_LIMIT
            and plan['total_limit_bytes'] == TOTAL_LIMIT, 'FAIL_PLAN_IDENTITY')
    return {'manifest_sha256': digest(root / FIXTURES / 'manifest.json'),
            'native_manifest_sha256': digest(root / FIXTURES / 'native-manifest.json'),
            'literals': manifest['literals']}


def literal(root, part, kind):
    return (root / FIXTURES / f'p1-n1-{part}.{kind}').read_bytes()


def expected_events(kind):
    if kind == 'normal':
        return [['write', '4096', '21200', '21200', ''], ['sync', '0', '0', '0', ''],
                ['write', '25296', '21200', '21200', ''], ['sync', '0', '0', '0', '']]
    if kind == 'reserve-write-half':
        return [['write', '4096', '21200', '10600', 'write-half']]
    return [['write', '25296', '21200', '21200', ''], ['sync', '0', '0', '0', 'sync-after']]


def check_worker(worker, kind, source):
    require(kind in CASES, 'FAIL_NATIVE_CENSUS')
    require(worker['summary'] == ['PASS', kind, str(int(kind != 'normal')), worker['peak'], source],
            'FAIL_WORKER_IDENTITY')
    require(1 <= int(worker['peak']) <= 5, 'FAIL_HANDLE_BUDGET')
    require(worker['markers'] == ['XADB_DIRTY_NATIVE_DONE'], 'FAIL_WORKER_MARKER')
    require(worker['events'] == expected_events(kind), 'FAIL_FAULT_PREFIX')
    for part, values in worker['dirty'].items():
        lsn = '2' if part == 'cancel' else '1'
        require(values == [lsn, lsn, '1', '1', 'true', '0'], 'FAIL_DIRTY_REGISTRATION')
    require(set(worker['dirty']) == ({'reserve'} if kind == 'reserve-write-half'
                                    else {'reserve', 'cancel'}), 'FAIL_DIRTY_CENSUS')
    if kind != 'normal':
        cancel = kind == 'cancel-sync-after'
        require(worker['error'] == ['Sync', 'CallerMustResolve', 'Sync' if cancel else 'Write',
                                   'CallerMustResolve', 'IoWriteUnknown', 'Other',
                                   'sync outcome unknown' if cancel else 'write outcome requires caller resolution'],
                'FAIL_ORIGINAL_CAUSE')
        require(worker['fence'] == ['true', '2' if cancel else '1', '1' if cancel else '0',
                                   '46496' if cancel else '25296', '1', str(cancel).lower(),
                                   'DptInstalled', '1'], 'FAIL_RETAINED_OBLIGATION')
        require(worker['obligation'] == ['1', '1' if cancel else '0', '2' if cancel else '1',
                                        '25296' if cancel else '4096', '21200',
                                        '46496' if cancel else '25296', 'true', 'true'],
                'FAIL_OBLIGATION_IDENTITY')


def evaluate_case(case, root, kind, source):
    summary = (case / 'worker.tsv').read_text().rstrip('\n').split('\t')
    require(len(summary) == 5, 'FAIL_WORKER_IDENTITY')
    parts = ('reserve',) if kind == 'reserve-write-half' else ('reserve', 'cancel')
    worker = {'summary': summary, 'peak': summary[3],
              'events': [line.split('\t') for line in (case / 'events.tsv').read_text().splitlines()],
              'markers': [line for line in (case / 'stdout.log').read_text().splitlines()
                          if line.startswith('XADB_DIRTY_')],
              'dirty': {part: (case / f'{part}.dirty.tsv').read_text().rstrip('\n').split('\t')
                        for part in parts}}
    names = {'before.tsv', 'after.tsv', 'worker.tsv', 'events.tsv', 'header.bin',
             'stdout.log', 'stderr.log', 'process.json', 'database'}
    if kind != 'normal':
        for part in ('error', 'fence', 'obligation'):
            worker[part] = (case / f'{part}.tsv').read_text().rstrip('\n').split('\t')
            names.add(f'{part}.tsv')
    check_worker(worker, kind, source)
    objects = native_objects(root, 1, NAMESPACE)
    expected = objects[WAL]
    require((case / 'header.bin').read_bytes() == expected, 'FAIL_INITIAL_WAL')
    for part in parts:
        frame = literal(root, part, 'frame')
        expected += frame[:10600] if kind == 'reserve-write-half' else frame
        names.add(f'{part}.dirty.tsv')
        for suffix in ('page', 'descriptor'):
            name = f'{part}.{suffix}'
            names.add(name)
            require((case / name).read_bytes() == literal(root, part, suffix), 'FAIL_DIRTY_AUTHORITY')
    require({p.name for p in case.iterdir()} == names, 'FAIL_ARTIFACT_CENSUS')
    require((case / 'database' / WAL).read_bytes() == expected, 'FAIL_WAL_PREFIX')
    before, after = parse_manifest(case / 'before.tsv'), parse_manifest(case / 'after.tsv')
    actual = inventory(case / 'database')
    check_files(before, after, actual, objects, expected)
    return {'status': 'PASS', 'kind': kind, 'database': str(case / 'database'),
            'database_bytes': sum(row[0] for row in actual.values()), 'files': actual, 'worker': worker,
            'evidence': {p.name: digest(p) for p in sorted(case.iterdir()) if p.is_file()}}


def run(root, binary, output, source, build_receipt):
    root, binary, output = root.resolve(), binary.resolve(), output.resolve()
    require(source_fingerprint(root) == source, 'FAIL_SOURCE_IDENTITY')
    frozen = frozen_inputs(root)
    output.mkdir(parents=True, exist_ok=False)
    binary_hash = digest(binary)
    build_receipt = build_receipt.resolve()
    build_hash = digest(build_receipt)
    verify_build(json.loads(build_receipt.read_text()), root, binary, source, binary_hash)
    checker_names = ('dev3_dirty_allocation_native.py', 'test_dev3_dirty_allocation_native.py',
                     'dev3_allocation_native.py', 'dev3_doublewrite_native.py',
                     'check_dev3_dirty_runtime.py', 'check_dev0.py', 'comm_campaign.py',
                     'check_dev3_allocation_stack.py')
    checker_hashes = {name: digest(Path(__file__).resolve().parent / name) for name in checker_names}
    require(all(digest(root / 'scripts' / name) == checksum for name, checksum in checker_hashes.items()),
            'FAIL_CHECKER_IDENTITY')
    retained = output / f'dirty-native-worker-{binary_hash}'
    shutil.copyfile(binary, retained)
    retained.chmod(0o500)
    with retained.open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'FAIL_BINARY_FORMAT')
    report = {'status': 'RUNNING', 'scope': 'dirty allocator native WAL; no recovery or power-loss claim',
              'source_root': str(root), 'source_sha256': source, 'binary': str(retained),
              'binary_sha256': binary_hash, 'fixtures': frozen, 'cases': [],
              'build_receipt': str(build_receipt), 'build_receipt_sha256': build_hash,
              'checker_sha256': checker_hashes,
              'case_timeout_seconds': CASE_SECONDS, 'database_limit_bytes': DB_LIMIT,
              'total_limit_bytes': TOTAL_LIMIT}
    try:
        for kind in CASES:
            case = output / kind
            case.mkdir()
            command = [str(retained), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
            env = dict(os.environ, XADB_DIRTY_NATIVE_CASE=str(case), XADB_DIRTY_NATIVE_KIND=kind)
            process = {'command': command, 'binary_sha256': binary_hash, 'timeout_seconds': CASE_SECONDS,
                       'timed_out': False, 'exit_code': None}
            start = time.monotonic()
            with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
                try:
                    child = subprocess.run(command, env=env, cwd=root, stdout=stdout, stderr=stderr,
                                           timeout=CASE_SECONDS, check=False)
                    process['exit_code'] = child.returncode
                except subprocess.TimeoutExpired:
                    process['timed_out'] = True
            process['elapsed_seconds'] = time.monotonic() - start
            (case / 'process.json').write_text(json.dumps(process, indent=2) + '\n')
            require(not process['timed_out'] and process['elapsed_seconds'] <= CASE_SECONDS, 'FAIL_TIMEOUT')
            require(process['exit_code'] == 0, 'FAIL_WORKER_EXIT')
            stdout = (case / 'stdout.log').read_text()
            require('test result: ok. 1 passed; 0 failed; 0 ignored;' in stdout, 'FAIL_TEST_CENSUS')
            report['cases'].append(evaluate_case(case, root, kind, source))
            require(sum(r['database_bytes'] for r in report['cases']) <= TOTAL_LIMIT, 'FAIL_TOTAL_BUDGET')
        check_census([r['kind'] for r in report['cases']])
        require(source_fingerprint(root) == source and digest(binary) == digest(retained) == binary_hash
                and digest(build_receipt) == build_hash
                and all(digest(Path(__file__).resolve().parent / name) == checksum
                        for name, checksum in checker_hashes.items()),
                'FAIL_SOURCE_OR_BINARY_DRIFT')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', failure=str(error))
        raise
    finally:
        report['databases'] = len(report['cases'])
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--build-receipt', type=Path, required=True)
    args = parser.parse_args()
    result = run(args.root, args.binary, args.output, args.source_sha256, args.build_receipt)
    print(json.dumps({'status': result['status'], 'databases': result['databases']}))


if __name__ == '__main__':
    main()
