#!/usr/bin/env python3
"""Run the new plan3 native creator; audit exact files, effects and source identity."""
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
INPUT = Path('tests/fixtures/dev3-undo-bootstrap')
SELECTOR = ('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
            'status_bootstrap_tests::undo::native_undo::undo_bootstrap_native_p1')
TIMEOUT = 30


def remaining(started):
    seconds = TIMEOUT - (time.monotonic() - started)
    if seconds <= 0:
        raise ValueError('FAIL_TOTAL_TIME_BUDGET')
    return seconds


def expected_files(database, root=ROOT):
    files = native_objects(root, 1, database)
    raw = bytearray(files['d3-bootstrap'])
    struct.pack_into('<Q', raw, 120, 3)
    struct.pack_into('<Q', raw, 280, 3)
    raw[4064:] = hashlib.sha256(raw[:4064]).digest()
    files['d3-bootstrap'] = bytes(raw)
    return files


def check_inputs(root=ROOT):
    manifest = json.loads((root / INPUT / 'manifest.json').read_text())
    if manifest['unit'] != 'D3-UNDO-BOOTSTRAP-001':
        raise ValueError('FAIL_INPUT_UNIT')
    if {p.name for p in (root / INPUT).iterdir()} != set(manifest['files']) | {'manifest.json'}:
        raise ValueError('FAIL_INPUT_CENSUS')
    for name, row in manifest['files'].items():
        path = root / INPUT / name
        if path.stat().st_size != row['bytes'] or digest(path) != row['sha256']:
            raise ValueError('FAIL_INPUT_IDENTITY')
    for name, sha in manifest['inputs'].items():
        if digest(root / name) != sha:
            raise ValueError('FAIL_INPUT_DEPENDENCY')
    return digest(root / INPUT / 'manifest.json')


def check_runtime(output):
    if output.count('test result:') != 1 or 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in output:
        raise ValueError('FAIL_EXACT_TEST_CENSUS')
    lines = [line for line in output.splitlines() if line.startswith('test ') and not line.startswith('test result:')]
    prefix = f'test {SELECTOR} ...'
    if len(lines) != 1 or not lines[0].startswith(prefix) or lines[0][len(prefix):].strip() not in ('', 'ok'):
        raise ValueError('FAIL_EXACT_TEST_NAME')
    markers = [line for line in output.splitlines() if line.startswith('XADB_UNDO_BOOTSTRAP_NATIVE')]
    if markers != ['XADB_UNDO_BOOTSTRAP_NATIVE P1']:
        raise ValueError('FAIL_NATIVE_MARKER')


def check_native(path, root=ROOT):
    fields = (path / 'role.tsv').read_text().strip().split('\t')
    if len(fields) != 14 or fields[:2] != ['P1', '1'] or fields[3:] != ['1', '3', '1', '8', '0', '3', '350', '3', '0', '3', '0']:
        raise ValueError('FAIL_ROLE_OR_HANDLES')
    database = bytes.fromhex(fields[2])
    if len(database) != 16 or not any(database):
        raise ValueError('FAIL_NAMESPACE')
    files = expected_files(database, root)
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
    wanted = [f'{e["op"]}\t{e["name"] or "-"}\t{e["offset"]}\t{e["length"]}\t{e["sha256"]}' for e in expected]
    if (path / 'effects.tsv').read_text().splitlines() != wanted or len(wanted) != 350:
        raise ValueError('FAIL_NATIVE_EFFECTS')
    return {'files': rows, 'total_bytes': total, 'effects': 350, 'wrapped_file_peak': 3,
            'wrapped_file_final': 0, 'live_fd': 3, 'final_fd': 0}


def build_binding(directory):
    build = json.loads((directory / 'build-receipt.json').read_text())
    if (build['exit'] != 0 or source_fingerprint(directory / 'source') != build['source_sha256']
            or digest(directory / 'engine-tests') != build['binary_sha256']):
        raise ValueError('FAIL_BUILD_BINDING')
    return build


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-directory', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    directory, output = args.build_directory.resolve(), args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    row = {'status': 'RUNNING', 'timed_out': False,
           'scope': 'immutable plan3 creator only; INIT/recovery not supplied'}
    started = time.monotonic()
    try:
        build = build_binding(directory)
        source, binary = directory / 'source', directory / 'engine-tests'
        row.update(source_sha256=build['source_sha256'], binary_sha256=build['binary_sha256'],
                   build_receipt_sha256=digest(directory / 'build-receipt.json'), inputs_sha256=check_inputs(source))
        checker_paths = {Path(__file__).resolve()}
        for fn in (digest, source_fingerprint, wait_process_group, native_objects, expected_trace):
            checker_paths.add(Path(sys.modules[fn.__module__].__file__).resolve())
        row['checker_files'] = {str(p): digest(p) for p in checker_paths}
        env = os.environ.copy()
        for key in list(env):
            if key.startswith(('XADB', 'RUST', 'CARGO_PROFILE_')):
                env.pop(key)
        env['XADB_UNDO_BOOTSTRAP_NATIVE_DIR'] = str(output)
        command = [str(binary), SELECTOR, '--exact', '--ignored', '--nocapture', '--test-threads=1']
        row['command'] = command
        timeout = remaining(started)
        with (output / 'stdout.log').open('xb') as out, (output / 'stderr.log').open('xb') as err:
            with subprocess.Popen(command, cwd=source, env=env, stdout=out, stderr=err, start_new_session=True) as child:
                row['pid'] = child.pid
                row['timeout_seconds'] = timeout
                wait_process_group(child, row, timeout)
        if row['exit_code'] != 0 or row['timed_out']:
            raise ValueError('FAIL_WORKER')
        check_runtime((output / 'stdout.log').read_text())
        row['native'] = check_native(output, source)
        if build_binding(directory) != build or any(digest(Path(p)) != h for p, h in row['checker_files'].items()):
            raise ValueError('FAIL_POST_IDENTITY')
        remaining(started)
        row['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        row.update(status='FAIL', error=str(error))
    for name in ('stdout.log', 'stderr.log'):
        if (output / name).exists():
            row[name + '_sha256'] = digest(output / name)
    row['elapsed_seconds'] = time.monotonic() - started
    (output / 'result.json').write_text(json.dumps(row, indent=2) + '\n')
    print(json.dumps(row))
    return int(row['status'] != 'PASS')


if __name__ == '__main__':
    sys.exit(main())
