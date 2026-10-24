#!/usr/bin/env python3
"""One fresh N+1 Status history: producer, recovery, reentry, exact independent files.

30s history, 24MiB database, 8MiB receipts. Source/ELF/cache retained separately.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

from check_dev0 import digest, source_fingerprint
import dev3_operational_recovery_native as wire
import dev3_recovery_effects_native as prior
import dev3_recovery_effects_oracle as model

SELECTOR = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::effect::native::pressure::worker_status_pressure_native'
require = model.require


def literal(root, name):
    return (root / 'crates/engine/tests/fixtures/dev3-status-pressure' / name).read_bytes()


def base(root, name):
    return (root / 'tests/fixtures/dev3-resident-status/p1-n3' / name).read_bytes()


def producer(root):
    files = model.prefix.initial_files(root, 1, 2)
    files['LOCK'] = b''
    files[model.WAL] += base(root, 'wal.frames-only') + literal(root, 'transaction.frames-only')
    pages = [base(root, f'status-{i}.after') for i in (1, 2, 3)]
    pages += [literal(root, f'step{i}.page') for i in (1, 2, 3)]
    for batch, page in enumerate(pages, 1):
        wire.flush(files, page, batch)
    return files


def recover(before, root):
    files = dict(before)
    for name in model.SLOTS:
        files[name] = bytes(model.Q)
    selected = files[f'd3-dwb-{files["DWB"][81]}']
    require(selected[6] == 0, 'FAIL_EXPECTED_EMPTY_DWB')
    batch = model.number(selected, 40)
    for page in [base(root, 'allocator.after'), literal(root, 'step4.page')]:
        index = model.number(page, 32)
        if files[model.DATA][index * model.PAGE:(index + 1) * model.PAGE] != page:
            batch += 1
            wire.flush(files, page, batch)
    return files


def protocol(text, mode, pid):
    require(text.count('test result:') == 1, 'FAIL_SUMMARY_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [SELECTOR], 'FAIL_EXACT_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', text, re.M) == [('1', '0', '0', '0')], 'FAIL_TEST_CENSUS')
    require(re.findall(r'STATUS_PRESSURE_NATIVE_[^\r\n]+', text) == [
        f'STATUS_PRESSURE_NATIVE_BEGIN mode={mode} pid={pid}',
        f'STATUS_PRESSURE_NATIVE_PASS mode={mode} pid={pid}'], 'FAIL_WORKER_PROTOCOL')


def retained_bank(root):
    data = bytearray(53248)
    offset = 1664
    descriptor = literal(root, 'step2.descriptor')
    page = literal(root, 'step2.page')
    data[offset:offset + 1536] = descriptor
    data[offset + 1536:offset + 1560] = page[24:48]
    data[offset + 1568:offset + 1576] = (2).to_bytes(8, 'little')
    data[offset + 1576:offset + 1624] = descriptor[24:72]
    data[offset + 1624] = 1
    return bytes(data)


def expected_home_writes(root, mode):
    if mode == 'reentry': return []
    if mode == 'produce':
        original = model.prefix.initial_files(root, 1, 2)[model.DATA]
        rows = [(at, original[at:at + model.PAGE]) for at in range(0, len(original), model.PAGE)]
        pages = [base(root, f'status-{i}.after') for i in (1, 2, 3)]
        pages += [literal(root, f'step{i}.page') for i in (1, 2, 3)]
    else:
        rows = []
        pages = [base(root, 'allocator.after'), literal(root, 'step4.page')]
    return rows + [(model.number(page, 32) * model.PAGE, page) for page in pages]


def check_home_writes(rows, expected):
    writes = [r for r in rows if r[0] == 'write' and r[1] == model.DATA]
    require([(int(r[2]), int(r[3]), r[4]) for r in writes] == [
        (offset, model.PAGE, f'{model.PAGE}:{model.sha(page)}') for offset, page in expected], 'FAIL_EXACT_HOME_WRITES')
    return len(writes)


def check_ledger(facts):
    # Canonical replay reserves allocator plus all three Status pages, including
    # already-clean homes; the number of actual REDO writes is a separate fact.
    require(facts['complete_lsn'] == '10' and facts['remaining_wal'] == '6080'
            and facts['restored'] == '1' and facts['replay_topup'] == str(4 * model.PAGE)
            and facts['generation'] == '25' and facts['batch'] == '8', 'FAIL_RECOVERED_LEDGER')


def inspection(stage, root, mode):
    rows = prior.events(stage)
    if mode == 'produce':
        for name, step in [('active-loaded.page', 1), ('aborting-loaded.page', 3), ('aborted-dirty.page', 4)]:
            require((stage / name).read_bytes() == literal(root, f'step{step}.page'), 'FAIL_LOADED_PAGE')
        require((stage / 'retained-bank.bin').read_bytes() == retained_bank(root), 'FAIL_LIVE_OWNER_BANK')
    else:
        require(not any(r[0] == 'write' and r[1] == model.WAL for r in rows), 'FAIL_WAL_WRITE')
        facts = prior.observation(stage)
        check_ledger(facts)
        require((stage / 'allocator.page').read_bytes() == base(root, 'allocator.after'), 'FAIL_ALLOCATOR_IMAGE')
        for i, page in [(1, base(root, 'status-1.after')), (2, literal(root, 'step4.page')), (3, literal(root, 'step2.page'))]:
            require((stage / f'status-{i}.page').read_bytes() == page, 'FAIL_CANONICAL_IMAGE')
        require((stage / 'system.descriptor').read_bytes() == base(root, 'terminal.descriptor'), 'FAIL_SYSTEM_OWNER')
        descriptors = list(stage.glob('transaction-*.descriptor'))
        require(len(descriptors) == 1 and descriptors[0].read_bytes() == literal(root, 'step2.descriptor'), 'FAIL_RETAINED_OWNER')
        require((stage / 'transactions.tsv').read_text() == 'highwater\t2\nactive\t1\nallocator_lsn\t6\n', 'FAIL_TRANSACTION_CENSUS')
        prior.check_census(stage)
        if mode == 'reentry': require(facts['dirty_mask'] == '0', 'FAIL_REENTRY_DIRTY')
    return check_home_writes(rows, expected_home_writes(root, mode))


def check_resume_snapshot(origin, snapshot):
    saved = json.loads(snapshot.read_text())
    require(saved.get('root') == str(origin), 'FAIL_RESUME_SNAPSHOT_ROOT')
    actual = {}
    for p in sorted(origin.rglob('*')):
        if not p.is_file(): continue
        m = p.lstat()
        require(not p.is_symlink() and m.st_nlink == 1, 'FAIL_RESUME_SNAPSHOT_TYPE')
        actual[str(p.relative_to(origin))] = {'bytes': m.st_size, 'sha256': digest(p),
            'device': m.st_dev, 'inode': m.st_ino, 'links': m.st_nlink}
    require(saved['files'] == actual, 'FAIL_RESUME_SNAPSHOT_DRIFT')


def resume_origin(root, origin, snapshot=None):
    report_path = origin / 'result.json'
    report = json.loads(report_path.read_text())
    require(report.get('status') == 'FAIL' and report.get('error') == 'FAIL_RECOVERED_LEDGER'
            and report.get('not_run') == ['reentry']
            and [(w.get('mode'), w.get('status'), w.get('exit_code')) for w in report.get('workers', [])]
                == [('produce', 'PASS', 0), ('recover', 'FAIL', 0)], 'FAIL_RESUME_BOUNDARY')
    require(snapshot is not None, 'FAIL_RESUME_SNAPSHOT_MISSING')
    check_resume_snapshot(origin, snapshot)
    old_root = origin.parent / 'source'
    old_receipt = origin.parent / 'build-receipt.json'
    build = json.loads(old_receipt.read_text())
    require(build['exit'] == 0 and source_fingerprint(old_root) == build['source_sha256'] == report['source_sha256']
            and digest(origin / 'engine-tests') == build['binary_sha256'] == report['binary_sha256']
            and digest(old_receipt) == report['build_receipt_sha256'], 'FAIL_RESUME_SOURCE_BINDING')
    require(all(digest(old_root / name) == sha for name, sha in report['checker_sha256'].items()), 'FAIL_RESUME_CHECKER_DRIFT')
    require(producer(old_root) == producer(root), 'FAIL_RESUME_INPUT_DRIFT')
    records = []
    before = None
    expected = producer(root)
    for item in report['workers']:
        mode = item['mode']
        stage = origin / mode
        require(not item['timed_out'] and 0 <= item['elapsed_seconds'] <= 30, 'FAIL_RESUME_PROCESS')
        require(digest(stage / 'stdout.log') == item['stdout_sha256']
                and digest(stage / 'stderr.log') == item['stderr_sha256'], 'FAIL_RESUME_LOG_DRIFT')
        protocol((stage / 'stdout.log').read_text(), mode, item['pid'])
        require(prior.check_resources(stage, mode) == item['resources'], 'FAIL_RESUME_RESOURCES')
        if mode == 'recover': expected = recover(expected, root)
        files = json.loads((stage / 'files.json').read_text())
        require({n: {'bytes': v['bytes'], 'sha256': v['sha256']} for n, v in files.items()} == prior.wire_identity(expected), 'FAIL_RESUME_FILE_RECEIPT')
        if before: prior.preserved_inodes(before, files)
        before = files
        records.append({'mode': mode, 'original_status': item['status'], 'original_pid': item['pid'],
                        'saved_output_audit': 'PASS', 'home_writes': inspection(stage, root, mode)})
    current = prior.compare_files(origin / 'database', expected)
    require(current == before, 'FAIL_RESUME_ORIGINAL_DB_DRIFT')
    require(len({w['pid'] for w in report['workers']}) == 2, 'FAIL_RESUME_PID')
    return {'path': str(origin), 'report_sha256': digest(report_path), 'original_status': report['status'],
            'original_error': report['error'], 'elapsed_seconds': report['elapsed_seconds'],
            'execution_source_sha256': report['source_sha256'], 'execution_binary_sha256': report['binary_sha256'],
            'saved_output_audits': records, 'snapshot': str(snapshot), 'snapshot_sha256': digest(snapshot)}, expected


def run(root, binary, receipt, output, resume=None, resume_snapshot=None):
    root, binary, receipt, output = (p.resolve() for p in (root, binary, receipt, output))
    require(not output.exists(), 'FAIL_OUTPUT_EXISTS')
    source, elf = source_fingerprint(root), digest(binary)
    build = json.loads(receipt.read_text())
    require(root == receipt.parent / 'source' and build['exit'] == 0
            and build['source_sha256'] == source and build['binary_sha256'] == elf
            and build['command'] == ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--no-run', '--locked', '--offline', '--message-format=json'], 'FAIL_BUILD_BINDING')
    rows = [json.loads(line) for line in (receipt.parent / 'build.jsonl').read_text().splitlines()]
    require([r['success'] for r in rows if r.get('reason') == 'build-finished'] == [True], 'FAIL_COMPILATION')
    require(Path(__file__).resolve() == root / 'scripts/dev3_status_pressure_native.py', 'FAIL_CHECKER_SOURCE')
    checkers, receipt_sha = wire.checker_identities(root), digest(receipt)
    started = time.monotonic()
    require((resume is None) == (resume_snapshot is None), 'FAIL_RESUME_ARGUMENTS')
    origin_audit, resumed_files = resume_origin(root, resume.resolve(), resume_snapshot.resolve()) if resume else (None, None)
    output.mkdir(parents=True)
    retained = output / 'engine-tests'
    execution_binary = resume / 'engine-tests' if resume else binary
    execution_elf = origin_audit['execution_binary_sha256'] if resume else elf
    shutil.copy2(execution_binary, retained)
    require(digest(retained) == execution_elf, 'FAIL_EXECUTION_BINARY_BINDING')
    db = output / 'database'
    if resume:
        shutil.copytree(resume / 'database', db, copy_function=shutil.copy2)
    else:
        db.mkdir()
    modes = ['reentry'] if resume else ['produce', 'recover', 'reentry']
    report = dict(status='RUNNING', source_sha256=source, binary_sha256=elf,
                  build_receipt_sha256=receipt_sha, checker_sha256=checkers, execution_binary_sha256=execution_elf,
                  execution_source_sha256=origin_audit['execution_source_sha256'] if resume else source, workers=[], not_run=list(modes), origin=origin_audit)
    try:
        expected = resumed_files if resume else producer(root)
        previous = prior.compare_files(db, expected) if resume else None
        if resume:
            (output / 'resumed-input-files.json').write_text(json.dumps(previous, indent=2) + '\n')
            original = prior.inventory(resume / 'database')
            require(all((v['device'], v['inode']) != (original[n]['device'], original[n]['inode']) for n, v in previous.items()), 'FAIL_CLONE_IS_NOT_INDEPENDENT')
        for mode in modes:
            report['not_run'].remove(mode)
            stage = output / mode
            stage.mkdir()
            env = {k: v for k, v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
            env.update(XADB_STATUS_PRESSURE_DB=str(db), XADB_STATUS_PRESSURE_OUT=str(stage), XADB_STATUS_PRESSURE_MODE=mode)
            cmd = [str(retained), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
            item = dict(mode=mode, command=cmd, status='RUNNING', timed_out=False)
            report['workers'].append(item)
            began = time.monotonic()
            with (stage / 'stdout.log').open('x') as stdout, (stage / 'stderr.log').open('x') as stderr:
                with subprocess.Popen(cmd, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                    item['pid'] = child.pid
                    def remaining():
                        limit = 30 - (time.monotonic() - started) - (origin_audit['elapsed_seconds'] if origin_audit else 0)
                        require(limit > 0, 'FAIL_TIME_BUDGET')
                        return limit
                    try:
                        prior.finish_child(child, remaining, item)
                    finally:
                        item.update(exit_code=child.poll(), elapsed_seconds=time.monotonic() - began)
            item.update(stdout_sha256=digest(stage / 'stdout.log'), stderr_sha256=digest(stage / 'stderr.log'))
            require(item['exit_code'] == 0 and not item['timed_out'], 'FAIL_WORKER')
            protocol((stage / 'stdout.log').read_text(), mode, item['pid'])
            item['resources'] = prior.check_resources(stage, mode)
            require(item['resources']['cut_hits'] == 0, 'FAIL_UNEXPECTED_FAULT')
            if mode != 'produce': expected = recover(expected, root)
            current = prior.compare_files(db, expected)
            (stage / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
            if previous: prior.preserved_inodes(previous, current)
            previous = current
            item['home_writes'] = inspection(stage, root, mode)
            item['status'] = 'PASS'
        pids = [w['pid'] for w in report['workers']]
        if origin_audit: pids += [r['original_pid'] for r in origin_audit['saved_output_audits']]
        require(len(set(pids)) == 3, 'FAIL_DISTINCT_PID')
        require(source_fingerprint(root) == source and digest(binary) == elf and digest(retained) == execution_elf
                and digest(receipt) == receipt_sha and wire.checker_identities(root) == checkers, 'FAIL_SOURCE_DRIFT')
        report['receipt_bytes'] = sum(p.stat().st_size for p in output.rglob('*') if p.is_file() and p != retained and 'database' not in p.parts)
        report['database_bytes'] = sum(v['bytes'] for v in current.values())
        require(report['receipt_bytes'] <= 8 << 20 and report['database_bytes'] <= 24 << 20, 'FAIL_RETAINED_SPACE')
        if resume:
            require(digest(resume_snapshot) == origin_audit['snapshot_sha256'], 'FAIL_RESUME_SNAPSHOT_CHANGE')
            check_resume_snapshot(resume.resolve(), resume_snapshot.resolve())
        require(time.monotonic() - started + (origin_audit['elapsed_seconds'] if origin_audit else 0) <= 30, 'FAIL_HISTORY_BUDGET')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        for item in report['workers']:
            if item['status'] == 'RUNNING': item['status'] = 'FAIL'
    finally:
        report['elapsed_seconds'] = time.monotonic() - started
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'binary', 'build-receipt', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--resume', type=Path)
    parser.add_argument('--resume-snapshot', type=Path)
    args = parser.parse_args()
    report = run(args.root, args.binary, args.build_receipt, args.output, args.resume, args.resume_snapshot)
    print(json.dumps({k: report[k] for k in ('status', 'elapsed_seconds', 'not_run')}))
    raise SystemExit(0 if report['status'] == 'PASS' else 1)
