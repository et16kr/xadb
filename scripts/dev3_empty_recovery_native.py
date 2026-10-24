#!/usr/bin/env python3
"""Six frozen current-build empty histories, exact raw-file oracle and PID receipts.

Each history <=30s; whole selection <=180s; database <=24MiB, retained databases
<=144MiB; logs/snapshots <=16MiB. Source/ELF/cache are reported separately.
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

SELECTOR = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::effect::native::empty::worker_empty_recovery_native'
CASES = {'active': (1, ''), 'aborting': (2, ''), 'aborted': (3, ''),
         'active_partial': (1, 'redo_second_partial'),
         'aborting_retirement': (2, 'empty_status_retirement_after'),
         'aborted_slot_partial': (3, 'empty_status_slot_partial')}
require = model.require


def literal(root, name):
    return (root / 'crates/engine/tests/fixtures/dev3-empty-transactions/live' / name).read_bytes()


def initial(root, depth):
    files = model.prefix.initial_files(root, 1, 2)
    files['LOCK'] = b''
    files[model.WAL] = literal(root, 'wal-through-abort.bin')[:4096 + 59200 + depth * 3040]
    return files


def images(root, depth):
    base = root / 'tests/fixtures/dev3-status-init/p1-cancel0-init'
    return ((base / 'work1-allocator.after').read_bytes(),
            literal(root, ('begin', 'rollback', 'abort')[depth - 1] + '.after.page'))


def recover(before, root, depth, fault=''):
    files = dict(before)
    for name in model.SLOTS:
        files[name] = bytes(model.Q)
    selected = files[f'd3-dwb-{files["DWB"][81]}']
    batch = model.number(selected, 40)
    if selected[6] == 1:
        image = files['d3-dwb-images'][128:128 + model.PAGE]
        model.put_home(files, image)
        wire.select(files, 3 * batch + 1, batch)
    for page in images(root, depth):
        index = model.number(page, 32)
        if files[model.DATA][index * model.PAGE:(index + 1) * model.PAGE] == page:
            continue
        batch += 1
        if fault and index == 1:
            if fault == 'empty_status_slot_partial':
                wire.flush(files, page, batch, cut=9)
                slot = wire.wire_slot(batch, page)
                files['d3-dwb-images'] = slot[:8192] + files['d3-dwb-images'][8192:]
                return files
            wire.flush(files, page, batch, cut=21 if fault == 'redo_second_partial' else 30,
                       partial=8192 if fault == 'redo_second_partial' else None)
            return files
        wire.flush(files, page, batch)
    require(not fault, 'FAIL_EXPECTED_FAULT_NOT_REACHED')
    return files


def protocol(text, mode, depth, pid):
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [SELECTOR], 'FAIL_EXACT_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', text, re.M) == [('1', '0', '0', '0')], 'FAIL_TEST_CENSUS')
    require(re.findall(r'EMPTY_RECOVERY_NATIVE_[^\r\n]+', text) == [
        f'EMPTY_RECOVERY_NATIVE_BEGIN mode={mode} depth={depth} pid={pid}',
        f'EMPTY_RECOVERY_NATIVE_PASS mode={mode} depth={depth} pid={pid}'], 'FAIL_WORKER_PROTOCOL')


def inspection(stage, root, depth, reentry):
    allocator, status = images(root, depth)
    require((stage / 'allocator.page').read_bytes() == allocator
            and (stage / 'status.page').read_bytes() == status, 'FAIL_EXACT_PAGES')
    base = root / 'tests/fixtures/dev3-status-init/p1-cancel0-init'
    require((stage / 'system.descriptor').read_bytes() == (base / 'work1-init.descriptor').read_bytes(), 'FAIL_SYSTEM_OWNER')
    descriptor = stage / 'transaction.descriptor'
    require(descriptor.exists() == (depth < 3), 'FAIL_TERMINAL_RETIREMENT')
    if depth < 3:
        require(descriptor.read_bytes() == literal(root, ('begin', 'rollback')[depth - 1] + '.descriptor'), 'FAIL_RETAINED_OWNER')
    facts = prior.observation(stage)
    require(facts['complete_lsn'] == str(2 + depth) and facts['remaining_wal'] == str((3 - depth) * 3040)
            and facts['restored'] == '1' and facts['replay_topup'] == '32768', 'FAIL_LEDGER')
    require((stage / 'transactions.tsv').read_text() == f'highwater\t1\nactive\t{int(depth < 3)}\nallocator_lsn\t2\n', 'FAIL_TRANSACTION_CENSUS')
    prior.check_census(stage)
    require(not any(row[0] == 'write' and row[1] == model.WAL for row in prior.events(stage)), 'FAIL_WAL_WRITE')
    if reentry:
        require(facts['dirty_mask'] == '0' and not any(row[0] == 'write' and row[1] == model.DATA for row in prior.events(stage)), 'FAIL_REENTRY_HOME_WRITE')


def run(root, binary, receipt, output, selected):
    root, binary, receipt, output = (p.resolve() for p in (root, binary, receipt, output))
    require(selected and len(selected) == len(set(selected)) and set(selected) <= CASES.keys(), 'FAIL_SELECTION')
    require(not output.exists(), 'FAIL_OUTPUT_EXISTS')
    source, elf = source_fingerprint(root), digest(binary)
    build = json.loads(receipt.read_text())
    require(root == receipt.parent / 'source' and build['exit'] == 0
            and build['source_sha256'] == source and build['binary_sha256'] == elf
            and build['command'] == ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--no-run', '--locked', '--offline', '--message-format=json'], 'FAIL_BUILD_BINDING')
    rows = [json.loads(line) for line in (receipt.parent / 'build.jsonl').read_text().splitlines()]
    require([r['success'] for r in rows if r.get('reason') == 'build-finished'] == [True], 'FAIL_COMPILATION')
    require(Path(__file__).resolve() == root / 'scripts/dev3_empty_recovery_native.py', 'FAIL_CHECKER_SOURCE')
    checkers, receipt_sha = wire.checker_identities(root), digest(receipt)
    output.mkdir(parents=True)
    retained = output / 'engine-tests'
    shutil.copy2(binary, retained)
    report = dict(status='RUNNING', source_sha256=source, binary_sha256=elf,
                  build_receipt_sha256=receipt_sha, checker_sha256=checkers, cases=[], not_run=list(selected))
    total = time.monotonic()
    try:
        for case in selected:
            report['not_run'].remove(case)
            began = time.monotonic()
            depth, fault = CASES[case]
            folder = output / case
            db = folder / 'database'
            db.mkdir(parents=True)
            row = dict(case=case, depth=depth, fault=fault, status='RUNNING', workers=[])
            report['cases'].append(row)

            def worker(mode, label, cut=''):
                stage = folder / label
                stage.mkdir()
                env = {k: v for k, v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
                env.update(XADB_EMPTY_RECOVERY_DB=str(db), XADB_EMPTY_RECOVERY_OUT=str(stage),
                           XADB_EMPTY_RECOVERY_MODE=mode, XADB_EMPTY_RECOVERY_DEPTH=str(depth), XADB_EMPTY_RECOVERY_FAULT=cut)
                cmd = [str(retained), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
                item = dict(mode=mode, command=cmd, fault=cut, status='RUNNING', timed_out=False)
                row['workers'].append(item)
                started = time.monotonic()
                with (stage / 'stdout.log').open('x') as stdout, (stage / 'stderr.log').open('x') as stderr:
                    with subprocess.Popen(cmd, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                        item['pid'] = child.pid
                        def remaining():
                            limit = min(30 - (time.monotonic() - began), 180 - (time.monotonic() - total))
                            require(limit > 0, 'FAIL_TIME_BUDGET')
                            return limit
                        try:
                            prior.finish_child(child, remaining, item)
                        finally:
                            item.update(exit_code=child.poll(), elapsed_seconds=time.monotonic() - started)
                item.update(stdout_sha256=digest(stage / 'stdout.log'), stderr_sha256=digest(stage / 'stderr.log'))
                require(item['exit_code'] == 0 and not item['timed_out'], 'FAIL_WORKER')
                protocol((stage / 'stdout.log').read_text(), mode, depth, item['pid'])
                item['resources'] = prior.check_resources(stage, mode)
                require(item['resources']['cut_hits'] == int(mode == 'fail'), 'FAIL_FAULT_COUNT')
                item['status'] = 'PASS'
                return stage

            expected = initial(root, depth)
            worker('produce', 'producer')
            before = prior.compare_files(db, expected)
            (folder / 'producer-files.json').write_text(json.dumps(before, indent=2) + '\n')
            if fault:
                stage = worker('fail', 'failure', fault)
                events = prior.events(stage)
                if fault == 'redo_second_partial':
                    require(events[-1][:4] == ['write', model.DATA, '16384', '16384']
                            and events[-1][4].startswith('ERR_PARTIAL:8192:'), 'FAIL_PARTIAL_BOUNDARY')
                elif fault == 'empty_status_slot_partial':
                    require(events[-1][:4] == ['write', 'd3-dwb-images', '0', '16544']
                            and events[-1][4].startswith('ERR_PARTIAL:8192:'), 'FAIL_SLOT_BOUNDARY')
                else:
                    require(events[-1] == ['publish', 'd3-dwb-select-stage', '0', '0', 'OK'], 'FAIL_RETIREMENT_BOUNDARY')
                expected = recover(expected, root, depth, fault)
                failed = prior.compare_files(db, expected)
                (folder / 'failed-files.json').write_text(json.dumps(failed, indent=2) + '\n')
                for name in ('DWB', 'd3-dwb-0', 'd3-dwb-1', 'd3-dwb-images', model.DATA):
                    raw = (db / name).read_bytes()
                    (stage / (name + '.snapshot')).write_bytes(raw[:2 * model.PAGE] if name == model.DATA else raw[:16544])
            expected = recover(expected, root, depth)
            recovered = worker('recover', 'recovery')
            after = prior.compare_files(db, expected)
            prior.preserved_inodes(before, after)
            inspection(recovered, root, depth, False)
            reentry = worker('reentry', 'reentry')
            final = prior.compare_files(db, expected)
            prior.preserved_inodes(after, final)
            inspection(reentry, root, depth, True)
            require(len({w['pid'] for w in row['workers']}) == len(row['workers']), 'FAIL_DISTINCT_PID')
            row.update(elapsed_seconds=time.monotonic() - began, files=final)
            require(row['elapsed_seconds'] <= 30, 'FAIL_HISTORY_BUDGET')
            row['status'] = 'PASS'
        require(source_fingerprint(root) == source and digest(binary) == elf == digest(retained)
                and digest(receipt) == receipt_sha and wire.checker_identities(root) == checkers, 'FAIL_SOURCE_DRIFT')
        sizes = [p.stat().st_size for p in output.rglob('*') if p.is_file() and p != retained and 'database' not in p.parts]
        report['receipt_bytes'] = sum(sizes)
        report['database_bytes'] = sum(v['bytes'] for row in report['cases'] for v in row['files'].values())
        require(report['receipt_bytes'] <= 16 << 20 and report['database_bytes'] <= 144 << 20, 'FAIL_RETAINED_SPACE')
        require(time.monotonic() - total <= 180, 'FAIL_BATCH_BUDGET')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        for row in report['cases']:
            if row['status'] == 'RUNNING': row['status'] = 'FAIL'
            for item in row['workers']:
                if item['status'] == 'RUNNING': item['status'] = 'FAIL'
    finally:
        report['elapsed_seconds'] = time.monotonic() - total
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'binary', 'build-receipt', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--select', choices=CASES, nargs='+', default=list(CASES))
    args = parser.parse_args()
    report = run(args.root, args.binary, args.build_receipt, args.output, args.select)
    print(json.dumps({k: report[k] for k in ('status', 'elapsed_seconds', 'not_run')}))
    raise SystemExit(0 if report['status'] == 'PASS' else 1)


if __name__ == '__main__':
    main()
