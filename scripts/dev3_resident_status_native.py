#!/usr/bin/env python3
"""Three resident frames/four addresses, followed by separate recovery/reentry.

Only the new history is run. Wire encoders and Linux-process receipt readers are
shared with prior owners; no prior completed scenario is invoked.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
import dev3_operational_recovery_native as wire
prior, model, require = wire.prior, wire.model, wire.require
PRODUCER = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::effect::native::resident::worker_resident_status_native'
RECOVERY = wire.SELECTOR
BUILD = ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--no-run', '--locked', '--offline', '--message-format=json']


def literal(root, name):
    return (root / 'tests/fixtures/dev3-resident-status/p1-n3' / name).read_bytes()


def producer_files(root):
    files = model.prefix.initial_files(root, 1, 2)
    files['LOCK'] = b''
    files[model.WAL] += literal(root, 'wal.frames-only')
    wire.flush(files, literal(root, 'status-1.after'), 1)
    wire.flush(files, literal(root, 'status-2.after'), 2)
    return files


def recover_files(root, before):
    files = dict(before)
    for name in model.SLOTS:
        files[name] = bytes(model.Q)
    # Producer completed two batches. Allocator and C are the only dirty homes.
    wire.flush(files, literal(root, 'allocator.after'), 3)
    wire.flush(files, literal(root, 'status-3.after'), 4)
    return files


def check_worker(text, selector, mode, pid):
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [selector], 'FAIL_EXACT_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', text, re.M) == [('1','0','0','0')], 'FAIL_WORKER_CENSUS')
    require(re.findall(r'OPERATIONAL_RECOVERY_NATIVE_[^\r\n]+', text) == [
        f'OPERATIONAL_RECOVERY_NATIVE_BEGIN mode={mode} pid={pid}',
        f'OPERATIONAL_RECOVERY_NATIVE_PASS mode={mode} pid={pid}'], 'FAIL_NATIVE_PROTOCOL')


def check_inspection(folder, root, reentry):
    require((folder / 'allocator.bin').read_bytes() == literal(root, 'allocator.after'), 'FAIL_ALLOCATOR_BYTES')
    require((folder / 'descriptor.bin').read_bytes() == literal(root, 'terminal.descriptor'), 'FAIL_DESCRIPTOR_BYTES')
    require({p.name for p in folder.glob('status-*.bin')} == {f'status-{n}.bin' for n in (1,2,3)}, 'FAIL_STATUS_INVENTORY')
    for n in (1,2,3):
        require((folder / f'status-{n}.bin').read_bytes() == literal(root, f'status-{n}.after'), 'FAIL_STATUS_BYTES')
    rows = dict(line.split('=', 1) for line in (folder / 'all-targets.txt').read_text().splitlines())
    require(set(rows) == {'mask','initial','final','applied','skipped'} and rows['mask'] == '14', 'FAIL_TARGET_CENSUS')
    initial = ([6,2,4,6] if reentry else [0,2,4,None]) + [None] * 60
    final = [6,2,4,6] + [None] * 60
    applied = ([0,0,0,0] if reentry else [6,0,0,1]) + [0] * 60
    skipped = ([6,1,1,1] if reentry else [0,1,1,0]) + [0] * 60
    require(wire.option_array(rows['initial']) == initial and wire.option_array(rows['final']) == final, 'FAIL_ALL_TARGET_LSNS')
    require(json.loads(rows['applied']) == applied and json.loads(rows['skipped']) == skipped, 'FAIL_ALL_TARGET_REDO_COUNTS')
    facts = prior.observation(folder)
    require(facts['complete_lsn'] == '6' and facts['highwater'] == '3' and facts['replay_topup'] == '65536', 'FAIL_PREFIX_FACTS')
    prior.check_census(folder)
    if reentry:
        require(facts['dirty_mask'] == '0' and not any(row[0] == 'write' and row[1] == model.DATA for row in prior.events(folder)), 'FAIL_REENTRY_HOME_WRITE')


def check_build_logs(receipt, build):
    require(digest(receipt.parent / 'build.jsonl') == build['stdout_sha256']
            and digest(receipt.parent / 'build.stderr') == build['stderr_sha256'],
            'FAIL_BUILD_LOG_HASH')


def retain_inventory(output, db, retained, report):
    """Retain all actual files, including failed/timeout databases and logs."""
    rows = []
    for path in sorted(output.rglob('*')):
        if not path.is_file() or path == retained or path.name == 'result.json':
            continue
        info = path.stat()
        rows.append(dict(path=str(path.relative_to(output)), bytes=info.st_size,
                         inode=info.st_ino, sha256=digest(path),
                         kind='database' if db in path.parents else 'case_output'))
    database_bytes = sum(row['bytes'] for row in rows if row['kind'] == 'database')
    logs_bytes = sum(row['bytes'] for row in rows if row['kind'] == 'case_output')
    limits = report['limits']
    fits = (database_bytes <= limits['database_bytes']
            and database_bytes <= limits['retained_database_bytes']
            and logs_bytes <= limits['logs_bytes'])
    report['retention'] = dict(files=rows, database_bytes=database_bytes,
                               retained_database_bytes=database_bytes,
                               case_output_bytes=logs_bytes, status='PASS' if fits else 'FAIL')
    if not fits:
        report.setdefault('prior_error', report.get('error'))
        report.update(status='FAIL', error='FAIL_RETAINED_SPACE')


def check_final_time(report, elapsed):
    report['elapsed_seconds'] = elapsed
    if elapsed > min(report['limits']['history_seconds'], report['limits']['total_seconds']):
        report.setdefault('prior_error', report.get('error'))
        report.update(status='FAIL', error='FAIL_FINAL_TIME_BUDGET')


def run(root, binary, receipt, output):
    root, binary, receipt, output = (p.resolve() for p in (root, binary, receipt, output))
    require(not output.exists(), 'FAIL_OUTPUT_EXISTS')
    source, binary_sha = source_fingerprint(root), digest(binary)
    build = json.loads(receipt.read_text())
    require(Path(__file__).resolve() == root / 'scripts/dev3_resident_status_native.py', 'FAIL_CHECKER_SOURCE')
    require(root == (receipt.parent / 'source').resolve() and build['status'] == 'PASS'
            and build['exit_code'] == 0 and build['source_sha256'] == source
            and Path(build['source_root']).resolve() == root and build['binary_sha256'] == binary_sha
            and build['command'] == BUILD, 'FAIL_BUILD_BINDING')
    check_build_logs(receipt, build)
    records = [json.loads(line) for line in (receipt.parent / 'build.jsonl').read_text().splitlines() if line.startswith('{')]
    require([r.get('success') for r in records if r.get('reason') == 'build-finished'] == [True]
            and not any(r.get('reason') == 'compiler-message' and r.get('message', {}).get('level') == 'error' for r in records), 'FAIL_COMPILE_RECEIPT')
    checkers = wire.checker_identities(root)
    receipt_sha = digest(receipt)
    output.mkdir(parents=True)
    retained = output / 'engine-tests'
    shutil.copy2(binary, retained)
    db = output / 'database'
    db.mkdir()
    report = dict(status='RUNNING', source_sha256=source, binary_sha256=binary_sha,
                  build_receipt_sha256=receipt_sha, checker_sha256=checkers, workers=[],
                  history='three frames, allocator+Status1/2/3, evict A/reload A, recover/reentry',
                  limits=dict(history_seconds=30, total_seconds=120, database_bytes=24<<20, retained_database_bytes=96<<20, logs_bytes=32<<20))
    started = time.monotonic()
    try:
        def worker(mode, label):
            selector = PRODUCER if mode == 'produce' else RECOVERY
            stage = output / label
            stage.mkdir()
            env = {k:v for k,v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
            env.update(XADB_OPERATIONAL_RECOVERY_DB=str(db), XADB_OPERATIONAL_RECOVERY_OUT=str(stage), XADB_OPERATIONAL_RECOVERY_MODE=mode, XADB_OPERATIONAL_RECOVERY_FAULT='')
            command = [str(retained), '--exact', selector, '--ignored', '--nocapture', '--test-threads=1']
            item = dict(label=label, mode=mode, selector=selector, command=command, status='RUNNING', timed_out=False,
                        environment={k:v for k,v in env.items() if k.startswith('XADB_')})
            report['workers'].append(item)
            with (stage / 'stdout.log').open('x') as stdout, (stage / 'stderr.log').open('x') as stderr:
                with subprocess.Popen(command, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                    item['pid'] = child.pid
                    try:
                        wire.wait_worker(child, item, started, started)
                    finally:
                        stdout.flush(); stderr.flush()
                        item.update(stdout_sha256=digest(stage / 'stdout.log'), stderr_sha256=digest(stage / 'stderr.log'))
            require(item['exit_code'] == 0 and not item['timed_out'], 'FAIL_WORKER_OR_TIMEOUT')
            check_worker((stage / 'stdout.log').read_text(), selector, mode, item['pid'])
            item['resources'] = prior.check_resources(stage, mode)
            require(item['resources']['cut_hits'] == 0, 'FAIL_UNREQUESTED_CUT')
            item['status'] = 'PASS'
            return stage
        expected = producer_files(root)
        produced = worker('produce', 'producer')
        before = prior.compare_files(db, expected)
        (output / 'producer-files.json').write_text(json.dumps(before, indent=2) + '\n')
        for name in ('allocator.after','status-1.after','status-3.after'):
            require((produced / name).read_bytes() == literal(root, name), 'FAIL_LIVE_READ_BYTES')
        expected = recover_files(root, expected)
        recovery = worker('recover', 'recovery')
        after = prior.compare_files(db, expected)
        prior.preserved_inodes(before, after)
        check_inspection(recovery, root, False)
        reentry = worker('recover', 'reentry')
        final = prior.compare_files(db, expected)
        prior.preserved_inodes(after, final)
        check_inspection(reentry, root, True)
        require(len({row['pid'] for row in report['workers']}) == 3, 'FAIL_SAME_PROCESS_REOPEN')
        require(source_fingerprint(root) == source and digest(binary) == binary_sha == digest(retained)
                and digest(receipt) == receipt_sha and wire.checker_identities(root) == checkers, 'FAIL_SOURCE_BINARY_CHANGED')
        require(sum(p.stat().st_size for p in output.rglob('*') if p.is_file() and p != retained and db not in p.parents) <= 32<<20, 'FAIL_LOG_SPACE')
        require(time.monotonic() - started <= 30, 'FAIL_HISTORY_BUDGET')
        report.update(status='PASS', final_files=final)
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        for item in report['workers']:
            if item['status'] == 'RUNNING':
                item['status'] = 'FAIL'
    finally:
        try:
            retain_inventory(output, db, retained, report)
        except BaseException as error:
            report.setdefault('prior_error', report.get('error'))
            report.update(status='FAIL', error='FAIL_RETENTION_INVENTORY: ' + str(error))
        check_final_time(report, time.monotonic() - started)
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root','binary','build-receipt','output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    report = run(args.root, args.binary, args.build_receipt, args.output)
    print(json.dumps({'status': report['status'], 'output': str(args.output), 'error': report.get('error')}))
    return int(report['status'] != 'PASS')


if __name__ == '__main__':
    raise SystemExit(main())
