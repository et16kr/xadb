#!/usr/bin/env python3
"""Select only the frozen Status bootstrap subset on a preserved engine binary."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from dev3_storage_native import native_objects
from dev3_storage_model import expected_trace

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / 'tests/fixtures/dev3-status-bootstrap'
PREFIX = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_bootstrap_tests::'
SELECTORS = {f'SB0{i}': PREFIX + name for i, name in enumerate((
    'status_bootstrap_sb01_literals_and_layout', 'status_bootstrap_sb02_plan_decoder',
    'status_bootstrap_sb03_role_health', 'status_bootstrap_sb04_incomplete_creation',
    'status_bootstrap_sb05_legacy_adapters'), 1)}
SELECTORS.update(PARSER='storage::wal_io::bootstrap::tests::bootstrap_descriptor_rejects_damage',
                 P1=PREFIX + 'native::status_bootstrap_native_p1',
                 S5=PREFIX + 'status_bootstrap_s5_expanded_matrix')
COUNTS = dict(SB01=4, SB02=12, SB03=5, SB04=7, SB05=4)
CASE_MARKER = 'XADB_STATUS_BOOTSTRAP_CASE '
VARIANT_MARKER = 'XADB_STATUS_BOOTSTRAP_VARIANT '


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check_runtime(output, case):
    if 'test result: ok. 1 passed; 0 failed;' not in output or output.count('test result:') != 1:
        raise ValueError('FAIL_EXACT_TEST_CENSUS')
    test_lines = [line for line in output.splitlines() if line.startswith('test ') and not line.startswith('test result:')]
    prefix = f'test {SELECTORS[case]} ...'
    if len(test_lines) != 1 or not test_lines[0].startswith(prefix) or test_lines[0][len(prefix):].strip() not in ('', 'ok'):
        raise ValueError('FAIL_EXACT_TEST_NAME')
    markers = [line[len(CASE_MARKER):] for line in output.splitlines() if line.startswith(CASE_MARKER)]
    if case in COUNTS:
        wanted = [f'{case}-{i:02}' for i in range(1, COUNTS[case] + 1)]
    elif case == 'S5':
        wanted = [row['id'] for row in json.loads((FIXTURES / 's5-matrix.json').read_text())
                  if not row['mandatory_alias']]
    else:
        wanted = []
    if markers != wanted:
        raise ValueError('FAIL_CASE_CENSUS')
    variants = [line[len(VARIANT_MARKER):] for line in output.splitlines() if line.startswith(VARIANT_MARKER)]
    if variants != (['SB03-05 healthy', 'SB03-05 workspace_poison', 'SB03-05 plan1'] if case == 'SB03' else []):
        raise ValueError('FAIL_VARIANT_CENSUS')
    resource_marker = 'XADB_STATUS_BOOTSTRAP_RESOURCE_VARIANT '
    resources = [line[len(resource_marker):] for line in output.splitlines() if line.startswith(resource_marker)]
    if resources != (['normal_admit', 'alignment_reject'] if case == 'SB01' else []):
        raise ValueError('FAIL_RESOURCE_VARIANT_CENSUS')
    natives = [line for line in output.splitlines() if line.startswith('XADB_STATUS_BOOTSTRAP_NATIVE ')]
    if natives != (['XADB_STATUS_BOOTSTRAP_NATIVE P1'] if case == 'P1' else []):
        raise ValueError('FAIL_NATIVE_MARKER')
    return {'cases': markers, 'variants': variants, 'resource_variants': resources}


def check_native(path, root=ROOT):
    fields = (path / 'role.tsv').read_text().strip().split('\t')
    if len(fields) != 14 or fields[:2] != ['P1', '1'] or fields[3:] != ['1', '2', '1', '9', '0', '2', '350', '3', '0', '3', '0']:
        raise ValueError('FAIL_ROLE_OR_HANDLES')
    database = bytes.fromhex(fields[2])
    if len(database) != 16 or not any(database):
        raise ValueError('FAIL_NAMESPACE')
    files = native_objects(root, 1, database)
    descriptor = bytearray(files['d3-bootstrap'])
    struct.pack_into('<Q', descriptor, 120, 2)
    struct.pack_into('<Q', descriptor, 280, 2)
    descriptor[4064:] = hashlib.sha256(descriptor[:4064]).digest()
    files['d3-bootstrap'] = bytes(descriptor)
    actual, total, rows = set(), 0, []
    for file in sorted((path / 'database').iterdir()):
        meta = file.lstat()
        if not stat.S_ISREG(meta.st_mode) or meta.st_nlink != 1:
            raise ValueError('FAIL_NATIVE_FILE_KIND')
        wanted = b'' if file.name == 'LOCK' else files.get(file.name)
        if wanted is None or file.read_bytes() != wanted:
            raise ValueError('FAIL_NATIVE_FILE_BYTES')
        actual.add(file.name)
        total += meta.st_size
        rows.append({'name': file.name, 'bytes': meta.st_size, 'sha256': digest(file),
                     'device': meta.st_dev, 'inode': meta.st_ino})
    if actual != set(files) | {'LOCK'} or total > 8 << 20:
        raise ValueError('FAIL_NATIVE_FILE_CENSUS')
    plan = json.loads((root / 'tests/fixtures/dev3-storage-session/effects.json').read_text())
    expected = expected_trace(plan, files, 1)
    lines = (path / 'effects.tsv').read_text().splitlines()
    wanted = [f'{e["op"]}\t{e["name"] or "-"}\t{e["offset"]}\t{e["length"]}\t{e["sha256"]}' for e in expected]
    if lines != wanted or len(lines) != 350:
        raise ValueError('FAIL_NATIVE_EFFECTS')
    return {'profile': 1, 'effects': 350, 'files': rows, 'total_bytes': total,
            'wrapped_file_peak': 3, 'wrapped_file_final': 0,
            'live_db_fd_observation': 3, 'final_db_fd_observation': 0,
            'scope': 'fresh creator and immutable role only; no recovery or INIT'}


def check_inputs():
    manifest = json.loads((FIXTURES / 'manifest.json').read_text())
    for name, row in manifest['files'].items():
        path = FIXTURES / name
        if path.stat().st_size != row['bytes'] or digest(path) != row['sha256']:
            raise ValueError('FAIL_INPUT_IDENTITY')
    for name, expected in manifest['inputs'].items():
        if digest(ROOT / name) != expected:
            raise ValueError('FAIL_INPUT_DEPENDENCY')
    return digest(FIXTURES / 'manifest.json')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', action='append', required=True, choices=SELECTORS)
    args = parser.parse_args()
    if len(set(args.case)) != len(args.case):
        parser.error('duplicate selections')
    args.output.mkdir(parents=True, exist_ok=False)
    build = json.loads(args.build_receipt.read_text())
    source = source_fingerprint(ROOT)
    binary = Path(build['binary'])
    if build['status'] != 'PASS' or source != build['source_sha256'] or digest(binary) != build['binary_sha256']:
        raise ValueError('FAIL_BUILD_BINDING')
    report = {'status': 'RUNNING', 'source_sha256': source, 'binary_sha256': digest(binary),
              'build_receipt_sha256': digest(args.build_receipt), 'inputs_sha256': check_inputs(),
              'selected': args.case, 'not_run': [case for case in SELECTORS if case not in args.case], 'results': []}
    environment = os.environ.copy()
    for name in list(environment):
        if name.startswith(('XADB', 'RUST', 'CARGO_PROFILE_')):
            environment.pop(name)
    try:
        for case in args.case:
            directory = args.output / case
            directory.mkdir()
            command = [str(binary), SELECTORS[case], '--exact', '--nocapture', '--test-threads=1']
            if case in ('P1', 'S5'):
                command.append('--ignored')
            child_env = environment.copy()
            if case == 'P1':
                child_env['XADB_STATUS_BOOTSTRAP_NATIVE_DIR'] = str(directory.resolve())
            row = {'case': case, 'command': command, 'status': 'RUNNING', 'timeout_seconds': 60 if case == 'PARSER' else 30, 'timed_out': False}
            report['results'].append(row)
            started = time.monotonic()
            with (directory / 'stdout.log').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
                with subprocess.Popen(command, cwd=ROOT, env=child_env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                    row['pid'] = child.pid
                    wait_process_group(child, row, row['timeout_seconds'])
            row['seconds'] = time.monotonic() - started
            row['stdout_sha256'] = digest(directory / 'stdout.log')
            row['stderr_sha256'] = digest(directory / 'stderr.log')
            if row['exit_code'] != 0 or row['timed_out']:
                raise ValueError(f'FAIL_WORKER_{case}')
            row.update(check_runtime((directory / 'stdout.log').read_text(), case))
            if case == 'P1':
                row['native'] = check_native(directory)
            row['status'] = 'PASS'
        if source_fingerprint(ROOT) != source or digest(binary) != report['binary_sha256']:
            raise ValueError('FAIL_POST_RUN_IDENTITY')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        if report['results'] and report['results'][-1]['status'] == 'RUNNING':
            report['results'][-1]['status'] = 'FAIL'
    (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({key: report.get(key) for key in ('status', 'error', 'selected', 'source_sha256')}))
    return int(report['status'] != 'PASS')


if __name__ == '__main__':
    sys.exit(main())
