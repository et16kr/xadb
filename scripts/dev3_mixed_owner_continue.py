#!/usr/bin/env python3
"""Approved harness-only continuation of one preserved mixed native failure.

The original successful producer is audited, never invoked. Its complete saved
disk bytes are copied into a new database with independent inodes. Only recover
and reentry run, both on the new frozen ELF. The original failed database and
all receipts remain unchanged. An explicit independently approved lineage is
required; this script cannot create its own approval or bless runtime changes.

The original history elapsed time carries into the 30-second limit, including
this invocation's audit/copy/run/retention. Original plus new immutable output
share the same 128-MiB limit. Existing matching disk snapshots are referenced
instead of copying another database at successful completion.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import time

sys.dont_write_bytecode = True
import dev3_mixed_owner_native as native
from check_dev0 import SOURCE_PATHS

require = native.require
WORKER = 'crates/engine/src/storage/wal_io/mixed_system_owner_native_tests.rs'
FORMAT_ONLY_TEST = 'crates/engine/src/storage/wal_io/mixed_system_recovery_tests.rs'
PACKAGES = ('engine', 'foundation', 'platform')
BUILD_FILES = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')
PHASES = ('recover', 'reentry')
APPROVAL = 'APPROVED_MIXED_OWNER_CONTINUATION'


def read_json(path):
    return json.loads(path.read_text())


def content_identity(inventory):
    return {name: {key: row[key] for key in ('bytes', 'sha256')} for name, row in inventory.items()}


def source_files(root):
    paths = []
    for name in SOURCE_PATHS:
        path = root / name
        paths.extend(path.rglob('*') if path.is_dir() else [path])
    return {str(path.relative_to(root)): native.digest(path) for path in sorted(paths) if path.is_file()}


def runtime_accounting(old_root, new_root, old_worker, new_worker, format_only_test=None):
    old, new = source_files(old_root), source_files(new_root)
    required = set(BUILD_FILES) | {f'crates/{package}/Cargo.toml' for package in PACKAGES}
    require(all(name in old and name in new for name in required), 'FAIL_RUNTIME_BUILD_INPUT_MISSING')
    def protected(name):
        return name in required or name.startswith('.cargo/') or any(
            name.startswith(f'crates/{package}/src/') or name == f'crates/{package}/build.rs'
            for package in PACKAGES)
    # .cargo is not in the common source fingerprint set, so bind it here too.
    for root, values in ((old_root, old), (new_root, new)):
        for path in sorted((root / '.cargo').rglob('*')):
            if path.is_file():
                values[str(path.relative_to(root))] = native.digest(path)
    changes = [{'path': name, 'old_sha256': old.get(name), 'new_sha256': new.get(name),
                'protected_runtime_or_build': protected(name)}
               for name in sorted(set(old) | set(new)) if old.get(name) != new.get(name)]
    require(old.get(WORKER) == old_worker and new.get(WORKER) == new_worker, 'FAIL_NATIVE_WORKER_LINEAGE')
    allowed = {WORKER}
    if format_only_test is not None:
        require(format_only_test.get('path') == FORMAT_ONLY_TEST
                and old.get(FORMAT_ONLY_TEST) == format_only_test.get('old_sha256')
                and new.get(FORMAT_ONLY_TEST) == format_only_test.get('new_sha256')
                and old.get(FORMAT_ONLY_TEST) is not None and new.get(FORMAT_ONLY_TEST) is not None,
                'FAIL_FORMAT_ONLY_TEST_LINEAGE')
        require(''.join((old_root / FORMAT_ONLY_TEST).read_text().split())
                == ''.join((new_root / FORMAT_ONLY_TEST).read_text().split()), 'HOLD_FORMAT_ONLY_TEST_CONTENT_CHANGED')
        allowed.add(FORMAT_ONLY_TEST)
    require(all(row['path'] in allowed or not row['protected_runtime_or_build'] for row in changes),
            'HOLD_RUNTIME_OR_BUILD_SOURCE_CHANGED')
    protected_files = {name: sha for name, sha in old.items() if protected(name) and name not in allowed}
    require(protected_files and all(new.get(name) == sha for name, sha in protected_files.items()), 'FAIL_RUNTIME_SOURCE_IDENTITY')
    return dict(unchanged_runtime_and_build=protected_files, changed_paths=changes,
                reviewed_exception={'path': WORKER, 'old_sha256': old_worker, 'new_sha256': new_worker},
                reviewed_format_only_test=format_only_test)


def build_binding(root, binary, receipt):
    build = read_json(receipt)
    require(root == receipt.parent / 'source' and build.get('exit', build.get('exit_code')) == 0
            and build['command'] == native.BUILD, 'FAIL_ORIGINAL_BUILD')
    source, elf = native.source_fingerprint(root), native.digest(binary)
    require(source == build['source_sha256'] and elf == build['binary_sha256'], 'FAIL_ORIGINAL_BUILD_IDENTITY')
    with binary.open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'FAIL_ORIGINAL_ELF')
    rows = [json.loads(line) for line in (receipt.parent / 'build.jsonl').read_text().splitlines()]
    require([row['success'] for row in rows if row.get('reason') == 'build-finished'] == [True], 'FAIL_ORIGINAL_COMPILATION')
    logs = {name: native.digest(receipt.parent / name) for name in ('build.jsonl', 'build.stderr')}
    for name, key in [('build.jsonl', 'stdout_sha256'), ('build.stderr', 'stderr_sha256')]:
        if key in build:
            require(logs[name] == build[key], 'FAIL_ORIGINAL_BUILD_LOG')
    return dict(source_sha256=source, binary_sha256=elf, build_receipt_sha256=native.digest(receipt), build_logs=logs)


def validate_lineage(lineage, origin, old_receipt, new_receipt, old_binding, new_identity, output):
    require(lineage.get('schema') == 1 and lineage.get('status') == APPROVAL
            and lineage.get('runtime_exception') == WORKER and lineage.get('allowed_phases') == list(PHASES), 'FAIL_LINEAGE_APPROVAL')
    for key, receipt, binding in [('original', old_receipt, old_binding), ('continuation', new_receipt, new_identity)]:
        declared = lineage[key]
        require(Path(declared['build_receipt']).is_absolute() and Path(declared['build_receipt']) == receipt,
                'FAIL_LINEAGE_BUILD_PATH')
        require(all(declared[field] == binding[field] for field in
                    ('source_sha256', 'binary_sha256', 'build_receipt_sha256')), 'FAIL_LINEAGE_BUILD_IDENTITY')
    require(Path(lineage['original']['result']).is_absolute()
            and Path(lineage['original']['result']) == origin
            and lineage['original']['result_sha256'] == native.digest(origin), 'FAIL_LINEAGE_ORIGIN')
    require(Path(lineage['continuation']['output']).is_absolute()
            and Path(lineage['continuation']['output']) == output, 'FAIL_LINEAGE_OUTPUT')


def audit_original(origin, old_root, old_binding, new_root, new_identity):
    report = read_json(origin)
    identity = report['identity']
    require(all(identity[key] == value for key, value in old_binding.items()), 'FAIL_ORIGINAL_RECEIPT_BUILD')
    native.verify_immutable_chain(origin, identity)
    require(report['status'] == 'FAIL' and report['not_run'] == ['recover', 'reentry']
            and [row['phase'] for row in report['completed']] == ['produce']
            and [(row['phase'], row['status']) for row in report['workers']] == [('produce', 'PASS'), ('recover', 'FAIL')],
            'FAIL_ORIGINAL_PHASE_BOUNDARY')
    require(all(native.digest(old_root / name) == sha for name, sha in identity['checker_sha256'].items()), 'FAIL_ORIGINAL_CHECKER_DRIFT')
    require(native.fixture_identity(old_root) == identity['fixture_sha256'] == new_identity['fixture_sha256'], 'FAIL_ORIGINAL_FIXTURE_DRIFT')
    old_expected, new_expected = native.expected_history(old_root), native.expected_history(new_root)
    require(old_expected == new_expected, 'FAIL_EXPECTED_MODEL_DRIFT')
    row = report['completed'][0]
    stage = Path(row['output'])
    require(stage == origin.parent / 'produce' and native.digest(stage / 'phase.json') == row['receipt_sha256'], 'FAIL_ORIGINAL_PRODUCER_RECEIPT')
    phase = read_json(stage / 'phase.json')
    require(phase['status'] == 'PASS' and phase['exit_code'] == 0 and not phase['timed_out']
            and phase['phase'] == 'produce' and phase['pid'] == row['pid'] and phase['identity'] == identity,
            'FAIL_ORIGINAL_PRODUCER_STATUS')
    require(native.file_receipts(stage, (stage / 'phase.json',)) == phase['files'], 'FAIL_ORIGINAL_PRODUCER_FILES')
    observed = native.inspect(stage, old_root, 'produce', old_expected['produce'], identity, row['pid'])
    require(observed == phase['oracle'], 'FAIL_ORIGINAL_PRODUCER_ORACLE')
    failed = report['failed_phase']
    failed_stage = Path(failed['output'])
    require(failed['phase'] == 'recover' and failed_stage == origin.parent / 'recover'
            and native.digest(failed_stage / 'phase.json') == failed['receipt_sha256'], 'FAIL_ORIGINAL_FAILED_RECEIPT')
    failed_phase = read_json(failed_stage / 'phase.json')
    require(failed_phase['status'] == 'FAIL' and failed_phase['phase'] == 'recover'
            and failed_phase['identity'] == identity and failed_phase['pid'] == failed['pid']
            and not failed_phase['worker_completed']
            and native.file_receipts(failed_stage, (failed_stage / 'phase.json',)) == failed_phase['files'], 'FAIL_ORIGINAL_FAILED_PHASE')
    require(row['pid'] != failed['pid'], 'FAIL_ORIGINAL_PID_REUSE')
    db = Path(report['database'])
    require(db == origin.parent / 'database' and native.prior.inventory(db) == report['database_inventory'], 'FAIL_ORIGINAL_FAILED_DB_DRIFT')
    require(0 <= report['elapsed_seconds'] < native.SECONDS, 'FAIL_ORIGINAL_TIME_BUDGET')
    return report, row, new_expected


def log_bytes(files):
    return sum(row['bytes'] for name, row in files.items() if Path(name).suffix in ('.log', '.tsv', '.txt'))


def immutable_origin_usage(origin):
    """Count each immutable ancestor tree once; the live DB and ELF are separate."""
    seen, total, logs = set(), 0, 0
    path = origin
    while path is not None:
        require(path not in seen, 'FAIL_ORIGIN_CYCLE')
        seen.add(path)
        report = read_json(path)
        files = native.file_receipts(path.parent, (path, path.parent / 'engine-tests', path.parent / 'database'))
        require(files == report['retention']['files'], 'FAIL_ORIGINAL_RETENTION_DRIFT')
        total += sum(row['bytes'] for row in files.values())
        logs += log_bytes(files)
        link = report.get('origin')
        path = Path(link['path']) if link else None
    return dict(bytes=total, log_bytes=logs)


def durable_copy_directory(source, destination):
    """Create independent files, sync their bytes, then the directory entries."""
    destination.mkdir(exist_ok=False)
    for path in sorted(source.iterdir()):
        with path.open('rb') as reader, (destination / path.name).open('xb') as writer:
            shutil.copyfileobj(reader, writer)
            writer.flush()
            os.fsync(writer.fileno())
    for directory in (destination, destination.parent):
        descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def copy_producer(source, destination, expected, original_disk):
    source_before = native.prior.compare_files(source, expected)
    durable_copy_directory(source, destination)
    copied = native.prior.compare_files(destination, expected)
    require(native.prior.inventory(source) == source_before, 'FAIL_PRODUCER_SNAPSHOT_CHANGED_DURING_COPY')
    for name, row in copied.items():
        for earlier in (source_before[name], original_disk[name]):
            require((row['device'], row['inode']) != (earlier['device'], earlier['inode']), 'FAIL_COPY_REUSED_INODE')
        require(row['links'] == 1, 'FAIL_COPY_HARDLINK')
    return dict(method='create_new_byte_copy_fsync_files_and_directories_no_hardlinks', original_disk=original_disk,
                source_snapshot=source_before, copied_database=copied, inodes_changed=True)


def original_unchanged(origin, identity, original_inventory, origin_sha):
    require(native.digest(origin) == origin_sha, 'FAIL_ORIGINAL_RESULT_DRIFT')
    native.verify_immutable_chain(origin, identity)
    report = read_json(origin)
    require(native.prior.inventory(Path(report['database'])) == original_inventory, 'FAIL_ORIGINAL_FAILED_DB_CHANGED')


def run_phases(root, binary, output, db, identity, producer, producer_identity, remaining, completed,
               workers=None, before_phase=None):
    """Only these two fixed phases can dispatch a new child process."""
    references = [producer]
    workers, failed = ([] if workers is None else workers), None
    for phase in PHASES:
        if before_phase is not None:
            before_phase()
        require(remaining() > 0, 'FAIL_HISTORY_TIME')
        stage = output / phase
        item = native.execute_phase(root, binary, stage, db, phase, identity, references, remaining,
                                    producer_identity=producer_identity)
        workers.append(dict(phase=phase, output=str(stage), status=item['status'], pid=item['pid']))
        reference = dict(phase=phase, output=str(stage), pid=item['pid'], receipt_sha256=native.digest(stage / 'phase.json'))
        require(item['pid'] not in {row['pid'] for row in references}, 'FAIL_CONTINUATION_PID_REUSE')
        if item['status'] != 'PASS':
            failed = dict(reference, status=item['status'], error=item['error'])
            break
        completed.append(reference)
        references.append(reference)
    return workers, failed


def retain_without_duplicate(output, binary, db, producer_snapshot, old_bytes, report, old_log_bytes=0):
    actual = native.prior.inventory(db)
    wanted = content_identity(actual)
    selected = None
    for candidate in (output / 'reentry/disk', output / 'recover/disk', producer_snapshot):
        if candidate.is_dir() and content_identity(native.prior.inventory(candidate)) == wanted:
            selected = candidate
            break
    if selected is None:
        selected = output / 'retained-database'
        durable_copy_directory(db, selected)
    require(content_identity(native.prior.inventory(selected)) == wanted, 'FAIL_RETAINED_DATABASE_BYTES')
    report['database_inventory'] = actual
    report['retained_database'] = dict(path=str(selected), content=wanted, snapshot_reused=selected.name == 'disk')
    files = native.file_receipts(output, (binary, db))
    new_bytes = sum(row['bytes'] for row in files.values())
    new_log_bytes = log_bytes(files)
    report['retention'] = dict(files=files, bytes=new_bytes, original_bytes=old_bytes, aggregate_bytes=old_bytes + new_bytes,
                               log_bytes=new_log_bytes, original_log_bytes=old_log_bytes,
                               aggregate_log_bytes=old_log_bytes + new_log_bytes)
    require(old_bytes + new_bytes <= native.RETAINED_LIMIT, 'FAIL_HISTORY_RETAINED_SPACE')
    require(old_log_bytes + new_log_bytes <= native.LOG_LIMIT, 'FAIL_HISTORY_LOG_SPACE')


def run(root, binary, receipt, origin, approval, output):
    started = time.monotonic()  # Includes all pre-effect audit/copy work below.
    require(all(path.is_absolute() for path in (root, binary, receipt, origin, approval, output)), 'FAIL_ABSOLUTE_PATH_REQUIRED')
    root, binary, receipt, origin, approval, output = (path.resolve() for path in (root, binary, receipt, origin, approval, output))
    require(not output.exists() and origin.parent not in output.parents
            and root not in output.parents, 'FAIL_CONTINUATION_OUTPUT_PATH')
    lineage = read_json(approval)
    approved_sha = native.digest(approval)
    old_receipt = Path(lineage['original']['build_receipt'])
    require(old_receipt.is_absolute(), 'FAIL_ORIGINAL_ABSOLUTE_PATH')
    old_root = old_receipt.parent / 'source'
    require(old_root not in output.parents, 'FAIL_OUTPUT_INSIDE_ORIGINAL_SOURCE')
    old_binding = build_binding(old_root, origin.parent / 'engine-tests', old_receipt)
    identity = native.immutable_identity(root, binary, receipt)
    validate_lineage(lineage, origin, old_receipt, receipt, old_binding, identity, output)
    accounting = runtime_accounting(old_root, root, lineage['original']['native_worker_sha256'],
                                    lineage['continuation']['native_worker_sha256'], lineage.get('format_only_test'))
    original, producer, expected = audit_original(origin, old_root, old_binding, root, identity)
    old_usage = immutable_origin_usage(origin)
    old_bytes = old_usage['bytes']
    elapsed_before = original['elapsed_seconds']
    def remaining():
        return native.SECONDS - elapsed_before - (time.monotonic() - started)
    require(remaining() > 0 and old_bytes <= native.RETAINED_LIMIT
            and old_usage['log_bytes'] <= native.LOG_LIMIT, 'FAIL_ORIGINAL_HISTORY_BUDGET')
    producer_stage = Path(producer['output'])
    origin_sha = native.digest(origin)
    output.mkdir(parents=True, exist_ok=False)
    inputs = output / 'input'
    inputs.mkdir()
    native.fresh_json(inputs / 'approved-lineage.json', lineage)
    native.fresh_json(inputs / 'runtime-source-accounting.json', accounting)
    native.fresh_json(inputs / 'new-identity.json', identity)
    retained_binary = output / 'engine-tests'
    with binary.open('rb') as reader, retained_binary.open('xb') as writer:
        shutil.copyfileobj(reader, writer)
    retained_binary.chmod(binary.stat().st_mode & 0o777)
    db = output / 'database'
    completed = []
    report = dict(schema=1, kind='approved_mixed_owner_continuation', status='RUNNING', identity=identity,
                  origin={'path': str(origin), 'sha256': origin_sha}, approved_lineage={'path': str(approval), 'sha256': approved_sha},
                  imported_producer={**producer, 'identity': original['identity'], 'executed_in_this_invocation': False},
                  database=str(db), completed=completed, workers=[], not_run=list(PHASES),
                  original_failed_database={'path': original['database'], 'inventory': original['database_inventory']},
                  original_elapsed_seconds=elapsed_before,
                  limits={'history_seconds': native.SECONDS, 'aggregate_retained_bytes': native.RETAINED_LIMIT,
                          'aggregate_log_bytes': native.LOG_LIMIT})
    try:
        require(native.digest(retained_binary) == identity['binary_sha256'], 'FAIL_CONTINUATION_ELF_COPY')
        copy = copy_producer(producer_stage / 'disk', db, expected['produce'], read_json(producer_stage / 'phase.json')['oracle']['disk'])
        native.fresh_json(inputs / 'producer-copy.json', copy)
        original_unchanged(origin, original['identity'], original['database_inventory'], origin_sha)
        def before_phase():
            require(native.immutable_identity(root, retained_binary, receipt) == identity
                    and native.digest(approval) == approved_sha, 'FAIL_PRE_PHASE_IDENTITY_DRIFT')
            original_unchanged(origin, original['identity'], original['database_inventory'], origin_sha)
        workers, failed = run_phases(root, retained_binary, output, db, identity, producer, original['identity'], remaining,
                                    completed, report['workers'], before_phase)
        report['workers'] = workers
        if failed:
            report['failed_phase'] = failed
            report['status'], report['error'] = failed['status'], failed['error']
        else:
            report['status'] = 'PASS'
        require(native.immutable_identity(root, retained_binary, receipt) == identity
                and native.digest(approval) == approved_sha, 'FAIL_CONTINUATION_IDENTITY_DRIFT')
    except BaseException as error:
        report['status'] = 'INTERRUPTED' if isinstance(error, KeyboardInterrupt) else 'FAIL'
        report['error'] = str(error) or type(error).__name__
    finally:
        failures = []
        def check_original_build():
            require(build_binding(old_root, origin.parent / 'engine-tests', old_receipt) == old_binding,
                    'FAIL_ORIGINAL_BUILD_DRIFT')
        def retain_new_output():
            if db.is_dir():
                retain_without_duplicate(output, retained_binary, db, producer_stage / 'disk', old_bytes, report,
                                         old_log_bytes=old_usage['log_bytes'])
        for action in (
            lambda: original_unchanged(origin, original['identity'], original['database_inventory'], origin_sha),
            check_original_build, retain_new_output,
            lambda: require(remaining() >= 0, 'FAIL_HISTORY_TIME'),
        ):
            try:
                action()
            except BaseException as error:
                failures.append(str(error) or type(error).__name__)
        if failures:
            if 'error' in report:
                report['prior_error'] = report['error']
            report['status'], report['error'] = 'FAIL', failures[0]
            report['finalization_errors'] = failures
        report['elapsed_seconds'] = elapsed_before + time.monotonic() - started
        report['not_run'] = [phase for phase in PHASES if phase not in {row['phase'] for row in completed}]
        native.fresh_json(output / 'result.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'binary', 'build-receipt', 'origin', 'approved-lineage', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    result = run(args.root, args.binary, args.build_receipt, args.origin, args.approved_lineage, args.output)
    print(json.dumps({'status': result['status'], 'imported_producer_pid': result['imported_producer']['pid'],
                      'completed': [row['phase'] for row in result['completed']], 'not_run': result['not_run']}))
    return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
