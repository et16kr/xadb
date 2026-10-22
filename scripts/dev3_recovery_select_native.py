#!/usr/bin/env python3
"""Run only new closed recovery-selection producers and separate reopen processes."""
import argparse
import csv
import hashlib
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
from dev3_doublewrite_native import native_objects
from dev3_status_home_flush import BUILD_COMMAND, check_build_receipt

ROOT = Path(__file__).resolve().parent.parent
SELECTOR = 'storage::recovery_select::native::worker_recovery_select_native'
CASES = ('NP11', 'NP12', 'NP21', 'NP22', 'NP31', 'NP32', 'NW01', 'NC01')
INPUT = Path('crates/engine/tests/fixtures/recovery_select/native/manifest.json')
INPUT_SHA256 = 'b2a4ebd1b8a965b817dc4cb3c8b7b1478b3fef573e99b7f896c4b3a6ef2a87ee'
CASE_SECONDS, TOTAL_SECONDS, DB_LIMIT, TOTAL_LIMIT, FD_LIMIT = 30, 240, 24 << 20, 192 << 20, 5
DATABASE = bytes(range(1, 17))


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def selected(names):
    values = list(CASES) if names is None else names
    require(values and len(values) == len(set(values)) and not set(values)-set(CASES), 'FAIL_NATIVE_SELECTION')
    return [name for name in CASES if name in values]


def parameters(case):
    return (int(case[2]), int(case[3])) if case.startswith('NP') else (1, 2)


def initial_files(root, profile, plan):
    files = native_objects(root, profile, DATABASE)
    if plan == 2:
        data = bytearray(files['d3-bootstrap'])
        data[120:128] = (2).to_bytes(8, 'little')
        data[280:288] = (2).to_bytes(8, 'little')
        data[-32:] = hashlib.sha256(data[:-32]).digest()
        files['d3-bootstrap'] = bytes(data)
    return files


def literal(root, logical):
    base = root/'tests/fixtures/dev3-status-home-flush'
    with (base/'literals.tsv').open() as stream:
        rows = [r for r in csv.DictReader(stream, delimiter='\t') if r['logical'] == logical]
    require(len(rows) == 1, 'FAIL_LITERAL_CENSUS')
    row = rows[0]
    value = (base/row['blob']).read_bytes()
    require(len(value) == int(row['bytes']) and hashlib.sha256(value).hexdigest() == row['sha256'], 'FAIL_LITERAL_IDENTITY')
    return value


def expected_files(root, case):
    profile, plan = parameters(case)
    files = initial_files(root, profile, plan)
    if case == 'NW01':
        files['d3-wal-00000001'] += literal(root, 'p1-c1/work1-reserve.wal')+literal(root, 'p1-c1/work1-cancel.wal')
    if case == 'NC01':
        raw = bytearray(files['d3-profile']); raw[4000] ^= 1
        files['d3-profile'] = bytes(raw)
    return files


def inventory(db):
    values = {}
    for path in sorted(db.iterdir()):
        meta = path.lstat()
        require(path.is_file() and not path.is_symlink() and meta.st_nlink == 1, 'FAIL_DATABASE_FILE_TYPE')
        values[path.name] = dict(bytes=meta.st_size, sha256=digest(path), device=meta.st_dev, inode=meta.st_ino, links=meta.st_nlink)
    require(sum(r['bytes'] for r in values.values()) <= DB_LIMIT, 'FAIL_DATABASE_BUDGET')
    return values


def check_files(db, expected):
    observed = inventory(db)
    require(set(observed) == set(expected), 'FAIL_DATABASE_CENSUS')
    for name, raw in expected.items():
        require((db/name).read_bytes() == raw, 'FAIL_DATABASE_BYTES:'+name)
    return observed


def check_stdout(output, mode, pid, outcome):
    require(output.count('test result:') == 1, 'FAIL_SUMMARY_CENSUS')
    tests = re.findall(r'^test ([\w:]+) \.\.\.', output, re.M)
    require(tests == [SELECTOR], 'FAIL_EXACT_NATIVE_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', output, re.M) == [('1', '0', '0', '0')], 'FAIL_NATIVE_CENSUS')
    markers = re.findall(r'RECOVERY_SELECT_NATIVE_COMPLETE [^\r\n]+', output)
    require(markers == [f'RECOVERY_SELECT_NATIVE_COMPLETE mode={mode} pid={pid} outcome={outcome}'], 'FAIL_NATIVE_MODE_NOT_REACHED')


def check_binary_format(binary):
    with binary.open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'FAIL_BINARY_FORMAT')


def check_process(row, remaining):
    require(row['timed_out'] is False and row['exit_code'] == 0 and 0 <= row['elapsed_seconds'] <= remaining, 'FAIL_PROCESS_TIMEOUT_OR_EXIT')


def validate_inputs(root):
    require(digest(root/INPUT) == INPUT_SHA256, 'FAIL_NATIVE_MANIFEST_IDENTITY')
    spec = json.loads((root/INPUT).read_text())
    require(spec['cases'] == list(CASES) and spec['selector'] == SELECTOR and spec['case_seconds'] == CASE_SECONDS and spec['total_seconds'] == TOTAL_SECONDS and spec['fd_limit'] == FD_LIMIT, 'FAIL_NATIVE_MANIFEST_SCHEMA')
    for row in spec['dependencies']:
        path = root/row['path']
        require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'FAIL_NATIVE_DEPENDENCY')
    return spec


def check_resources(folder, closed):
    rows = [line.split('\t') for line in (folder/'resources.tsv').read_text().splitlines()]
    require(len(rows) == len({r[0] for r in rows}) == 5, 'FAIL_RESOURCE_CENSUS')
    values = {k: int(v) for k, v in rows}
    require(values['handles_final'] == values['fd_final'] == 0 and values['fd_peak'] <= FD_LIMIT, 'FAIL_RESOURCE_BOUND')
    trace = [line.split('\t') for line in (folder/'fd.tsv').read_text().splitlines()]
    require(trace and trace[-1] == ['final', '0'] and max(int(r[1]) for r in trace) == values['fd_peak'], 'FAIL_FD_BOUNDARY_EVIDENCE')
    if closed:
        require(values['handles_peak'] <= 2 and values['mutations'] == 0, 'FAIL_CLOSED_EFFECTS')
        reads = [line.split('\t') for line in (folder/'reads.tsv').read_text().splitlines()]
        allowed = {'d3-profile', 'FILEMAP', 'd3-filemap-0', 'd3-bootstrap', 'd3-dwb-stage', 'DWB', 'd3-dwb-0', 'd3-wal-00000001', 'WRITER'}
        require(all(len(r) == 4 and r[0] in allowed and int(r[2]) == int(r[3]) for r in reads), 'FAIL_CLOSED_READ_SET')
    return values


def checker_identities(root):
    paths = {Path(m.__file__).resolve() for m in list(sys.modules.values()) if getattr(m, '__file__', None) and Path(m.__file__).suffix == '.py' and Path(m.__file__).resolve().parent == Path(__file__).resolve().parent}
    paths.add(Path(__file__).resolve())
    paths.add(Path(__file__).with_name('test_dev3_recovery_select_native.py'))
    result = {str(p.relative_to(ROOT)): digest(p) for p in paths}
    require(all(digest(root/p) == sha for p, sha in result.items()), 'FAIL_NATIVE_CHECKER_IDENTITY')
    return result


def run(root, binary, output, receipt, names=None):
    root, binary, output, receipt = (p.resolve() for p in (root, binary, output, receipt))
    require(sys.platform.startswith('linux'), 'FAIL_PLATFORM')
    chosen = selected(names); spec = validate_inputs(root)
    source, binary_hash = source_fingerprint(root), digest(binary)
    check_build_receipt(json.loads(receipt.read_text()), root, source, binary_hash)
    check_binary_format(binary)
    checkers = checker_identities(root)
    receipt_hash = digest(receipt)
    output.mkdir(parents=True, exist_ok=False)
    retained = output/'native-binary'; shutil.copyfile(binary, retained); retained.chmod(0o500)
    report = dict(status='RUNNING', unit='D3-RECOVERY-SELECT-001', scope='initial closed selection; no READY or repair', source_root=str(root), source_sha256=source, binary_sha256=binary_hash, build_receipt_sha256=receipt_hash, native_manifest_sha256=INPUT_SHA256, checker_sha256=checkers, selected=chosen, not_selected=[c for c in CASES if c not in chosen], cases=[])
    campaign_start = time.monotonic()
    active_jobs = []
    try:
        for case in chosen:
            folder = output/case; folder.mkdir(); db = folder/'database'; db.mkdir()
            profile, plan = parameters(case)
            row = dict(case=case, profile=profile, plan=plan, status='RUNNING', processes=[]); report['cases'].append(row)
            began = time.monotonic()
            def remaining():
                left = min(CASE_SECONDS-(time.monotonic()-began), TOTAL_SECONDS-(time.monotonic()-campaign_start))
                require(left > 0, 'FAIL_NATIVE_AGGREGATE_TIMEOUT')
                return left
            def start(mode):
                phase = folder/(mode+'-'+str(len(row['processes']))); phase.mkdir()
                env = {k: v for k, v in os.environ.items() if not k.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
                env.update(XADB_RECOVERY_SELECT_NATIVE_DIR=str(phase), XADB_RECOVERY_SELECT_NATIVE_DB=str(db), XADB_RECOVERY_SELECT_NATIVE_MODE=mode, XADB_RECOVERY_SELECT_NATIVE_PROFILE=str(profile), XADB_RECOVERY_SELECT_NATIVE_PLAN=str(plan))
                command = [str(retained), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
                stdout = (phase/'stdout.log').open('wb'); stderr = (phase/'stderr.log').open('wb')
                p = subprocess.Popen(command, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
                item = dict(mode=mode, pid=p.pid, command=command, binary_sha256=binary_hash, source_sha256=source, timed_out=False, exit_code=None, started=time.monotonic())
                row['processes'].append(item)
                job = (p, phase, item, stdout, stderr)
                active_jobs.append(job)
                return job
            def finish(job):
                p, phase, item, stdout, stderr = job
                allowance = remaining()
                try:
                    p.wait(timeout=allowance)
                except subprocess.TimeoutExpired:
                    item['timed_out'] = True
                    os.killpg(p.pid, 9); p.wait()
                finally:
                    stdout.close(); stderr.close()
                    item['elapsed_seconds'] = time.monotonic()-item.pop('started')
                    item['exit_code'] = p.returncode
                    (phase/'process.json').write_text(json.dumps(item, indent=2)+'\n')
                    active_jobs.remove(job)
                check_process(item, CASE_SECONDS)
                outcome = 'produced' if item['mode'].startswith('produce') else ('rejected' if item['mode'] in ('reject', 'contend') else 'selected')
                check_stdout((phase/'stdout.log').read_text(), item['mode'], p.pid, outcome)
                item['resources'] = check_resources(phase, not item['mode'].startswith('produce'))
                return phase
            producer = finish(start('produce_nonempty' if case == 'NW01' else 'produce'))
            if case == 'NC01':
                path = db/'d3-profile'; raw = bytearray(path.read_bytes()); before_hash = digest(path); raw[4000] ^= 1; path.write_bytes(raw)
                row['harness_corruption'] = dict(path=path.name, offset=4000, before_sha256=before_hash, after_sha256=digest(path))
            expected = expected_files(root, case)
            require({name: dict(bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest()) for name, raw in expected.items()} == spec['expected'][case], 'FAIL_EXPECTED_BYTES_IDENTITY')
            before = check_files(db, expected); row['before'] = before
            if case.startswith('NP'):
                hold = start('hold')
                try:
                    while not (hold[1]/'holding').exists():
                        require(hold[0].poll() is None, 'FAIL_OWNER_HOLD_NOT_REACHED'); remaining(); time.sleep(.005)
                    require((hold[1]/'holding').read_text() == str(hold[0].pid), 'FAIL_HOLD_PID')
                    contender = finish(start('contend'))
                    require('Busy' in (contender/'error.txt').read_text(), 'FAIL_LEASE_BUSY')
                finally:
                    (hold[1]/'release').write_text('release\n')
                    if hold[0].poll() is None and CASE_SECONDS-(time.monotonic()-began) <= 0:
                        os.killpg(hold[0].pid, 9)
                selected_phase = finish(hold)
                final_phase = finish(start('select'))
                epoch = hashlib.sha256(expected['d3-wal-00000001']).hexdigest()
                observation = f'{DATABASE.hex()}\t1\t{profile}\t{plan}\t1\t1\t{epoch}\n'
                require(all((p/'observation.tsv').read_text() == observation for p in (selected_phase, final_phase)), 'FAIL_SELECTION_OBSERVATION')
            else:
                rejection = finish(start('reject'))
                text = (rejection/'error.txt').read_text()
                require(('UnsupportedClosure' in text and 'Epoch' in text) if case == 'NW01' else ('Profile' in text and 'Corrupt' in text), 'FAIL_REJECTION_CAUSE')
                require('Unchanged' in text, 'FAIL_REJECTION_EFFECT')
            after = check_files(db, expected); require(after == before, 'FAIL_NAMESPACE_INODE_BYTES_CHANGED')
            row['after'] = after
            pids = [p['pid'] for p in row['processes']]
            require(len(pids) == len(set(pids)) and producer.name.startswith('produce'), 'FAIL_FRESH_PROCESS_IDENTITY')
            row.update(status='PASS', elapsed_seconds=time.monotonic()-began, database_bytes=sum(r['bytes'] for r in after.values()))
            require(row['elapsed_seconds'] <= CASE_SECONDS, 'FAIL_NATIVE_CASE_TIMEOUT')
            require(sum(r.get('database_bytes', 0) for r in report['cases']) <= TOTAL_LIMIT, 'FAIL_NATIVE_RETAINED_BUDGET')
        require(source_fingerprint(root) == source and digest(binary) == digest(retained) == binary_hash and digest(receipt) == receipt_hash, 'FAIL_SOURCE_BINARY_RECEIPT_DRIFT')
        validate_inputs(root); require(checker_identities(root) == checkers, 'FAIL_CHECKER_DRIFT')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        for row in report['cases']:
            if row['status'] == 'RUNNING': row['status'] = 'FAIL'
        raise
    finally:
        for p, phase, item, stdout, stderr in active_jobs:
            if p.poll() is None:
                os.killpg(p.pid, 9)
            p.wait()
            stdout.close(); stderr.close()
            item.update(exit_code=p.returncode, aborted_by_controller=True, elapsed_seconds=time.monotonic()-item.pop('started'))
            (phase/'process.json').write_text(json.dumps(item, indent=2)+'\n')
        report['not_run'] = [c for c in chosen if c not in {r['case'] for r in report['cases']}]
        report['elapsed_seconds'] = time.monotonic()-campaign_start
        (output/'result.json').write_text(json.dumps(report, indent=2)+'\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    for name in ('binary', 'output', 'build-receipt'): parser.add_argument('--'+name, type=Path, required=True)
    parser.add_argument('--case', action='append')
    args = parser.parse_args()
    report = run(args.root, args.binary, args.output, args.build_receipt, args.case)
    print(json.dumps(dict(status=report['status'], completed=len(report['cases']))))


if __name__ == '__main__': main()
