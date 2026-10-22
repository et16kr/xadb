#!/usr/bin/env python3
"""Run only the two remaining allocation native groups, retaining 13 Linux DBs.

The libtest ELF must be built with XADB_ALLOCATION_SOURCE_SHA256 equal to the
frozen source fingerprint. Expected bytes come from reviewed literal fixtures;
production encoders and decoders are never used as the oracle.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import time

from check_dev0 import digest, source_fingerprint
from dev3_doublewrite_native import native_objects

FIXTURES = Path('tests/fixtures/dev3-allocation-reservation')
WAL = 'd3-wal-00000001'
NAMESPACE = bytes(range(1, 17))
CUTS = ('write-before', 'write-half', 'write-after', 'sync-before', 'sync-after')
MODULE = 'storage::wal_io::bootstrap::doublewrite_install::allocation_reservation::native_tests::'
CASE_SECONDS = 30
DB_LIMIT = 24 << 20
TOTAL_LIMIT = 312 << 20


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, code):
    if not condition:
        raise ValueError(code)


def case_plan(group='all'):
    profiles = [(p, 'profiles', '') for p in (1, 2, 3)]
    faults = [(1, operation, cut) for operation in ('reserve', 'cancel') for cut in CUTS]
    return profiles if group == 'profiles' else faults if group == 'faults' else profiles + faults


def check_census(keys, group='all'):
    require(len(keys) == len(set(keys)) and set(keys) == set(case_plan(group)), 'FAIL_NATIVE_CENSUS')


def frozen_inputs(root):
    manifest_path = root / FIXTURES / 'manifest.json'
    manifest = json.loads(manifest_path.read_text())
    require(len(manifest['literals']) == 63, 'FAIL_FIXTURE_CENSUS')
    paths = set()
    for row in manifest['literals']:
        require(row['path'] not in paths, 'FAIL_FIXTURE_CENSUS')
        paths.add(row['path'])
        data = (root / FIXTURES / row['path']).read_bytes()
        require(len(data) == row['bytes'] and sha(data) == row['sha256'], 'FAIL_FIXTURE_IDENTITY')
    return {'manifest_sha256': digest(manifest_path),
            'literals': manifest['literals'],
            'count': len(paths), 'bytes': sum(row['bytes'] for row in manifest['literals'])}


def literal(root, profile, n, part):
    return (root / FIXTURES / f'p{profile}-n{n}-{part}').read_bytes()


def inventory(database):
    rows = {}
    for path in sorted(database.iterdir()):
        meta = path.lstat()
        require(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1, 'FAIL_FILE_TYPE')
        rows[path.name] = [meta.st_size, meta.st_dev, meta.st_ino, meta.st_nlink, digest(path)]
    return rows


def parse_manifest(path):
    rows = {}
    for line in path.read_text().splitlines():
        name, size, device, inode, links, checksum = line.split('\t')
        require(name not in rows, 'FAIL_FILE_CENSUS')
        rows[name] = [int(size), int(device), int(inode), int(links), checksum]
    return rows


def check_files(before, after, actual, objects, expected_wal):
    require(after == actual, 'FAIL_EXTERNAL_IMAGE')
    require(set(before) == set(after) == set(objects), 'FAIL_FILE_CENSUS')
    for name, data in objects.items():
        require(before[name][0] == len(data) and before[name][4] == sha(data), 'FAIL_INITIAL_IMAGE')
        if name != WAL:
            require(after[name] == before[name], 'FAIL_HOME_OR_DWB_CHANGED')
        else:
            require(after[name][1:4] == before[name][1:4], 'FAIL_WAL_REPLACED')
            require(after[name][0] == len(expected_wal) and after[name][4] == sha(expected_wal),
                    'FAIL_WAL_PREFIX')
    require(sum(row[0] for row in after.values()) <= DB_LIMIT, 'FAIL_DATABASE_BUDGET')


def expected_events(operation, cut):
    if operation == 'profiles':
        events, offset = [], 4096
        for n in (1, 8, 63):
            length = 20992 + 208 * n
            for _ in (False, True):
                events.extend([['write', str(offset), str(length), str(length), ''],
                               ['sync', '0', '0', '0', '']])
                offset += length
        return events
    offset = 4096 + 21200 * 2 + (22656 if operation == 'cancel' else 0)
    written = {'write-before': 0, 'write-half': 11328}.get(cut, 22656)
    if cut.startswith('write-'):
        return [['write', str(offset), '22656', str(written), cut]]
    return [['write', str(offset), '22656', '22656', ''], ['sync', '0', '0', '0', cut]]


def check_worker(worker, profile, operation, cut, source):
    require(worker['summary'] == ['PASS', str(profile), operation, cut,
                                 str(int(operation != 'profiles')), worker['peak'], source],
            'FAIL_WORKER_IDENTITY')
    require(1 <= int(worker['peak']) <= 5, 'FAIL_HANDLE_BUDGET')
    require(worker['markers'] == ['XADB_ALLOCATION_NATIVE_DONE'], 'FAIL_WORKER_MARKER')
    require(worker['events'] == expected_events(operation, cut), 'FAIL_FAULT_PREFIX')
    if operation != 'profiles':
        write = cut.startswith('write-')
        require(worker['error'] == ['Sync', 'CallerMustResolve', 'Write' if write else 'Sync',
                                   'CallerMustResolve', 'IoWriteUnknown', 'Other',
                                   'write outcome requires caller resolution' if write else 'sync outcome unknown'],
                'FAIL_ORIGINAL_CAUSE')
        end = 4096 + 21200 * 2 + 22656 * (2 if operation == 'cancel' else 1)
        retained = 21024768 + end - 4096 + 16384 + 8 * 16384 + (0 if operation == 'cancel' else 22656)
        require(worker['fence'] == ['true', 'true', 'true', '4', str(retained)], 'FAIL_COMMON_FENCE')


def evaluate_case(case, root, key, source):
    profile, operation, cut = key
    summary = (case / 'worker.tsv').read_text().rstrip('\n').split('\t')
    require(len(summary) == 7, 'FAIL_WORKER_IDENTITY')
    worker = {'summary': summary, 'peak': summary[5],
              'events': [line.split('\t') for line in (case / 'events.tsv').read_text().splitlines()],
              'markers': [line for line in (case / 'stdout.log').read_text().splitlines()
                          if line.startswith('XADB_ALLOCATION_')]}
    if operation != 'profiles':
        worker['error'] = (case / 'error.tsv').read_text().rstrip('\n').split('\t')
        worker['fence'] = (case / 'fence.tsv').read_text().rstrip('\n').split('\t')
    check_worker(worker, profile, operation, cut, source)
    objects = native_objects(root, profile, NAMESPACE)
    expected = objects[WAL]
    require((case / 'header.bin').read_bytes() == expected, 'FAIL_INITIAL_WAL')
    steps = [(n, part) for n in (1, 8, 63) for part in ('reserve', 'cancel')]
    if operation != 'profiles':
        steps = [(1, 'reserve'), (1, 'cancel'), (8, 'reserve')]
        if operation == 'cancel':
            steps.append((8, 'cancel'))
    expected_artifacts = {'before.tsv', 'after.tsv', 'worker.tsv', 'events.tsv', 'header.bin',
                          'stdout.log', 'stderr.log', 'process.json', 'database'}
    if operation != 'profiles':
        expected_artifacts.update(('error.tsv', 'fence.tsv'))
    for n, part in steps:
        data = literal(root, profile, n, f'{part}.frame')
        if operation != 'profiles' and (n, part) == steps[-1]:
            data = data[:{'write-before': 0, 'write-half': 11328}.get(cut, 22656)]
        expected += data
        for kind in ('page', 'descriptor'):
            name = f'n{n}-{part}.{kind}'
            expected_artifacts.add(name)
            require((case / name).read_bytes() == literal(root, profile, n, f'{part}.{kind}'),
                    'FAIL_DIRTY_AUTHORITY')
    require({p.name for p in case.iterdir()} == expected_artifacts, 'FAIL_ARTIFACT_CENSUS')
    require((case / 'database' / WAL).read_bytes() == expected, 'FAIL_WAL_PREFIX')
    before, after = parse_manifest(case / 'before.tsv'), parse_manifest(case / 'after.tsv')
    actual = inventory(case / 'database')
    check_files(before, after, actual, objects, expected)
    return {'status': 'PASS', 'profile': profile, 'operation': operation, 'cut': cut,
            'database': str(case / 'database'), 'database_bytes': sum(row[0] for row in actual.values()),
            'files': actual, 'worker': worker,
            'evidence': {path.name: digest(path) for path in sorted(case.iterdir()) if path.is_file()}}


def run(root, binary, output, source, group):
    root, binary, output = root.resolve(), binary.resolve(), output.resolve()
    require(source_fingerprint(root) == source, 'FAIL_SOURCE_IDENTITY')
    frozen = frozen_inputs(root)
    output.mkdir(parents=True, exist_ok=False)
    binary_hash = digest(binary)
    retained = output / f'allocation-native-worker-{binary_hash}'
    shutil.copyfile(binary, retained)
    retained.chmod(0o500)
    require(retained.read_bytes()[:4] == b'\x7fELF', 'FAIL_BINARY_FORMAT')
    report = {'status': 'RUNNING', 'scope': 'native allocation WAL; no recovery or power-loss claim',
              'source_root': str(root), 'source_sha256': source,
              'binary': str(retained), 'binary_sha256': binary_hash,
              'fixture_namespace': NAMESPACE.hex(), 'fixtures': frozen, 'group': group,
              'case_timeout_seconds': CASE_SECONDS, 'database_limit_bytes': DB_LIMIT,
              'total_limit_bytes': TOTAL_LIMIT, 'cases': []}
    try:
        for profile, operation, cut in case_plan(group):
            case = output / f'p{profile}-{operation}{"-" + cut if cut else ""}'
            case.mkdir()
            test = 'allocation_native_profiles' if operation == 'profiles' else 'allocation_native_faults'
            command = [str(retained), '--exact', MODULE + test, '--ignored', '--nocapture', '--test-threads=1']
            env = dict(os.environ, XADB_ALLOCATION_CASE=str(case), XADB_ALLOCATION_PROFILE=str(profile),
                       XADB_ALLOCATION_OPERATION=operation, XADB_ALLOCATION_CUT=cut)
            started = time.monotonic()
            process = {'command': command, 'binary_sha256': binary_hash, 'timeout_seconds': CASE_SECONDS,
                       'timed_out': False, 'exit_code': None}
            with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
                try:
                    result = subprocess.run(command, env=env, cwd=root, stdout=stdout, stderr=stderr,
                                            timeout=CASE_SECONDS, check=False)
                    process['exit_code'] = result.returncode
                except subprocess.TimeoutExpired:
                    process['timed_out'] = True
            process['elapsed_seconds'] = time.monotonic() - started
            (case / 'process.json').write_text(json.dumps(process, indent=2) + '\n')
            require(not process['timed_out'] and process['elapsed_seconds'] <= CASE_SECONDS, 'FAIL_TIMEOUT')
            require(process['exit_code'] == 0, 'FAIL_WORKER_EXIT')
            result = evaluate_case(case, root, (profile, operation, cut), source)
            report['cases'].append(result)
            require(sum(row['database_bytes'] for row in report['cases']) <= TOTAL_LIMIT, 'FAIL_TOTAL_BUDGET')
        check_census([(r['profile'], r['operation'], r['cut']) for r in report['cases']], group)
        require(source_fingerprint(root) == source and digest(binary) == digest(retained) == binary_hash,
                'FAIL_SOURCE_OR_BINARY_DRIFT')
        report['status'] = 'PASS'
    except Exception as error:
        report['status'] = 'FAIL'
        report['failure'] = str(error)
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
    parser.add_argument('--group', choices=('all', 'profiles', 'faults'), default='all')
    args = parser.parse_args()
    result = run(args.root, args.binary, args.output, args.source_sha256, args.group)
    print(json.dumps({'status': result['status'], 'databases': result['databases'], 'report': str(args.output / 'result.json')}))


if __name__ == '__main__':
    main()
