#!/usr/bin/env python3
"""Explicit mixed-owner native phases, immutable receipts and full-file oracle.

Run from the frozen source export with Python -B. Each invocation needs a new
--output; --resume names the previous result.json and keeps its original DB.
--phase all selects only unfinished phases. A failed phase may run again only
when its DB still equals its exact pre-phase image. Partial effects are retained
and rejected, never repaired by restarting the already completed producer.

Bounds: 30 seconds of active history across invocations; DB 24 MiB; each new
receipt tree (including full snapshots) 128 MiB; logs 8 MiB. Frozen source/ELF
are reported separately. These are harness bounds, not an RSS claim.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
import dev3_operational_recovery_native as wire
import dev3_recovery_effects_native as prior
import dev3_recovery_effects_oracle as model

PHASES = ('produce', 'recover', 'reentry')
PREFIX = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::status_init::status_flush::mixed::tests::native::'
SELECTORS = dict(zip(PHASES, (PREFIX + 'mixed_owner_native_' + suffix
                            for suffix in ('producer', 'recovery', 'reentry'))))
CASE_IDS = dict(zip(PHASES, ('MO-NATIVE-PRODUCE', 'MO-NATIVE-RECOVER', 'MO-NATIVE-REENTRY')))
BUILD = ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--no-run', '--locked', '--offline', '--message-format=json']
INPUT = Path('tests/fixtures/dev3-mixed-system-init')
MANIFEST_SHA = 'd42f4474b2e03b4e28b1fe11dd4d5d431743c664725308fb8d007d961aa01a8e'
SECONDS, RETAINED_LIMIT, LOG_LIMIT = 30, 128 << 20, 8 << 20
require = model.require


def fresh_json(path, value):
    with path.open('x') as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def literal(root, name):
    return (root / INPUT / 'p1' / name).read_bytes()


def fixture_identity(root):
    manifest = root / INPUT / 'manifest.json'
    require(digest(manifest) == MANIFEST_SHA, 'FAIL_FIXTURE_MANIFEST')
    rows = json.loads(manifest.read_text())['files']
    require(len(rows) == len({r['path'] for r in rows}) == 286, 'FAIL_FIXTURE_CENSUS')
    hashes = {str(INPUT / 'manifest.json'): MANIFEST_SHA}
    for row in rows:
        relative = Path(row['path'])
        require(not relative.is_absolute() and '..' not in relative.parts, 'FAIL_FIXTURE_PATH')
        path = root / INPUT / relative
        require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'], 'FAIL_FIXTURE_BYTES')
        hashes[str(INPUT / relative)] = row['sha256']
    # Bootstrap and pre-existing install literals are independently authored
    # inputs; the complete source fingerprint also binds every imported model.
    for folder in ('tests/fixtures/dev3-undo-bootstrap',
                   'tests/fixtures/dev3-doublewrite-install/literal',
                   'crates/engine/tests/fixtures/dev3-storage-session/literal'):
        for path in sorted((root / folder).rglob('*')):
            if path.is_file():
                hashes[str(path.relative_to(root))] = digest(path)
    return hashes


def initial_files(root):
    files = model.prefix.initial_files(root, 1, 1)
    files['d3-bootstrap'] = (root / 'tests/fixtures/dev3-undo-bootstrap/profile1-d3-bootstrap.bin').read_bytes()
    require(files[model.DATA][:model.PAGE] == literal(root, 'allocator.genesis'), 'FAIL_GENESIS_INPUT')
    require(files['d3-profile'] == literal(root, 'profile.bin'), 'FAIL_PROFILE_INPUT')
    return files


def control(generation, batch, page=None):
    """DWB wire envelope for the fixed Status/Undo/allocator raw images."""
    require(generation > 0 and batch >= 0, 'FAIL_CONTROL_COUNTERS')
    raw = bytearray(128 + (88 if page is not None else 0))
    struct.pack_into('<4sHBB16sQQQH', raw, 0, b'XDW1', 1, int(page is not None), 0,
                     model.wire.DATABASE, 1, generation, batch, int(page is not None))
    if page is not None:
        require(len(page) == model.PAGE and page[6] in (8, 9, 10)
                and model.sha(page[:-32]) == page[-32:].hex(), 'FAIL_TARGET_IMAGE')
        struct.pack_into('<Q', raw, 56, model.number(page, 64))
        struct.pack_into('<HBB', raw, 128, 0, page[6], page[7])
        raw[136:176], raw[176:184] = page[24:64], page[64:72]
        raw[184:216] = bytes.fromhex(model.sha(page))
    return model.wire.seal(raw)


def select(files, generation, batch, page=None):
    logical = control(generation, batch, page)
    bank = (generation - 1) % 2
    full = logical + bytes(model.PAGE - len(logical))
    files['d3-dwb-stage'] = full
    files[f'd3-dwb-{bank}'] = full
    files['DWB'] = model.wire.selector(files['d3-profile'], generation, bank, logical)


def flush(files, page, batch):
    select(files, 3 * batch - 1, batch)
    header = bytearray(128)
    struct.pack_into('<4sHH16sQQ', header, 0, b'XDS1', 1, 0, model.wire.DATABASE, 1, batch)
    header[40:80], header[80:88], header[88:90] = page[24:64], page[64:72], page[6:8]
    header[96:128] = bytes.fromhex(model.sha(page))
    slot = model.wire.seal(header + page)
    files['d3-dwb-images'] = slot + files['d3-dwb-images'][len(slot):]
    select(files, 3 * batch, batch, page)
    model.put_home(files, page)
    select(files, 3 * batch + 1, batch)


def expected_history(root):
    produced = initial_files(root)
    produced[model.WAL] += literal(root, 'sequence/wal.frames-only')
    flush(produced, literal(root, 'sequence/work1-payload.after'), 1)
    flush(produced, literal(root, 'sequence/work2-payload.after'), 2)
    recovered = dict(produced)
    for slot in model.SLOTS:
        recovered[slot] = bytes(model.Q)
    flush(recovered, literal(root, 'sequence/work3-allocator.after'), 3)
    flush(recovered, literal(root, 'sequence/work3-payload.after'), 4)
    return {'produce': produced, 'recover': recovered, 'reentry': dict(recovered)}


def protocol(text, phase, pid):
    require(text.count('test result:') == 1, 'FAIL_SUMMARY_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [SELECTORS[phase]], 'FAIL_EXACT_SELECTOR')
    require(re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured;', text, re.M)
            == [('1', '0', '0', '0')], 'FAIL_TEST_CENSUS')
    require(re.findall(r'MIXED_OWNER_(?:CASE|NATIVE)_[^\r\n]+', text) == [
        f'MIXED_OWNER_CASE_BEGIN\t{CASE_IDS[phase]}', f'MIXED_OWNER_NATIVE_BEGIN\t{phase}\t{pid}',
        f'MIXED_OWNER_NATIVE_PASS\t{phase}\t{pid}', f'MIXED_OWNER_CASE_PASS\t{CASE_IDS[phase]}'], 'FAIL_PHASE_PROTOCOL')


def pairs(path):
    rows = [line.split('\t') for line in path.read_text().splitlines()]
    require(all(len(row) == 2 for row in rows) and len(rows) == len({r[0] for r in rows}), 'FAIL_PAIR_CENSUS')
    return dict(rows)


def resources(stage):
    values = {k: int(v) for k, v in pairs(stage / 'resources.tsv').items()}
    require(set(values) == {'writer_bytes', 'file_bytes', 'handles_peak', 'handles_final', 'fd_peak', 'fd_final'}, 'FAIL_RESOURCE_CENSUS')
    require(values['handles_final'] == values['fd_final'] == 0 and 0 < values['fd_peak'] <= 5
            and 0 < values['writer_bytes'] <= 16384 and 0 < values['file_bytes'] <= 16384, 'FAIL_RESOURCE_BOUND')
    rows = [line.split('\t') for line in (stage / 'fd.tsv').read_text().splitlines()]
    require(rows and all(len(row) == 2 for row in rows) and rows[-1] == ['final', '0']
            and max(int(row[1]) for row in rows) == values['fd_peak'], 'FAIL_FD_WITNESS')
    return values


def snapshot(stage, expected):
    rows = [line.split('\t') for line in (stage / 'disk-inventory.tsv').read_text().splitlines()]
    require(rows and rows[0] == ['name', 'device', 'inode', 'length', 'sha256'], 'FAIL_DISK_HEADER')
    require(all(len(row) == 5 for row in rows[1:]) and len(rows) - 1 == len({row[0] for row in rows[1:]}), 'FAIL_DISK_CENSUS')
    values = {name: dict(device=int(device), inode=int(inode), bytes=int(length), sha256=sha)
              for name, device, inode, length, sha in rows[1:]}
    require({name: {key: row[key] for key in ('bytes', 'sha256')} for name, row in values.items()}
            == prior.wire_identity(expected), 'FAIL_DISK_INVENTORY_BYTES')
    prior.compare_files(stage / 'disk', expected)
    return values


def home_writes(root, phase):
    if phase == 'reentry':
        return []
    if phase == 'produce':
        original = initial_files(root)[model.DATA]
        rows = [(at, original[at:at + model.PAGE]) for at in range(0, len(original), model.PAGE)]
        pages = [literal(root, f'sequence/work{i}-payload.after') for i in (1, 2)]
    else:
        rows = []
        pages = [literal(root, 'sequence/work3-allocator.after'), literal(root, 'sequence/work3-payload.after')]
    return rows + [(model.number(page, 32) * model.PAGE, page) for page in pages]


def check_effects(stage, root, phase):
    events = prior.events(stage)
    writes = [(int(row[2]), int(row[3]), row[4]) for row in events if row[:2] == ['write', model.DATA]]
    expected = [(offset, model.PAGE, f'{model.PAGE}:{model.sha(page)}') for offset, page in home_writes(root, phase)]
    require(writes == expected, 'FAIL_EXACT_HOME_WRITES')
    if phase != 'produce':
        require(not any(row[:2] == ['write', model.WAL] for row in events), 'FAIL_RECOVERY_WAL_WRITE')
        text = (stage / 'effects.txt').read_text()
        # RecoveryEffects records target-index bitmasks, not write counts.
        mask = sum(1 << (offset // model.PAGE) for offset, _ in home_writes(root, phase))
        for key in ('home_write_attempted', 'home_written', 'home_sync_attempted', 'home_synced'):
            require(re.findall(r'^\s*' + key + r': (\d+),$', text, re.M) == [str(mask)], 'FAIL_HOME_EFFECT_MASK')
    return dict(events=len(events), home_writes=len(writes), events_sha256=digest(stage / 'events.tsv'))


def check_ledger(facts, expected, phase, wal_identity):
    want = {'complete_lsn': '6', 'complete_offset': str(len(expected[model.WAL])),
            'source_length': str(len(expected[model.WAL])), 'highwater': '3', 'active_mask': '0',
            'terminal_hold_mask': '14', 'inventory': '0', 'remaining_wal': '0', 'remaining_pages': '0',
            'remaining_protected': '0', 'remaining_frames': '0', 'remaining_memory': '0',
            'remaining_files': '0', 'epoch_credit': '0',
            'inventory_digest': model.sha(b''),
            'actual_owned': str(22069248 + len(expected[model.WAL]) - 4096 + 2097152),
            'outstanding': '16384', 'owned_peak': str(24182784 + len(expected[model.WAL]) - 4096),
            'protected': str(23134208 + len(expected[model.WAL]) - 4096 + 65536),
            'replay_topup': '65536', 'restored': '1', 'generation': '13',
            'batch': '4', 'bank': '0', 'state': 'Empty', 'phase': 'Snapshot',
            'pending_repaired': 'false', 'pending_home_skipped': 'false', 'pending_retired': 'false',
            'source_stabilized': 'true',
            'slots_provisioned': 'true', 'lease_effect': 'MayHaveChanged', 'canonical_passes': '1',
            'dirty_mask': '0' if phase == 'reentry' else '9',
            'flushed_mask': '0' if phase == 'reentry' else '9',
            'source_device': str(wal_identity['device']), 'source_inode': str(wal_identity['inode'])}
    require(all(facts.get(key) == value for key, value in want.items()), 'FAIL_MIXED_LEDGER')
    require(int(facts['actual_owned']) + int(facts['outstanding']) == int(facts['owned_peak']), 'FAIL_LEDGER_BALANCE')


def inspect(stage, root, phase, expected, identity, pid):
    protocol((stage / 'stdout.log').read_text(), phase, pid)
    require(pairs(stage / 'execution.tsv') == {'source_sha256': identity['source_sha256'],
            'elf_sha256': identity['binary_sha256']} and (stage / 'pid.txt').read_text() == f'{pid}\n', 'FAIL_WORKER_IDENTITY')
    disk = snapshot(stage, expected)
    observed_resources = resources(stage)
    if phase == 'produce':
        outputs = {'status-a.loaded.page': 'sequence/work1-payload.after',
                   'undo-b.loaded.page': 'sequence/work2-payload.after',
                   'allocator.dirty.page': 'sequence/work3-allocator.after',
                   'undo-c.dirty.page': 'sequence/work3-payload.after'}
    else:
        outputs = {'allocator.page': 'sequence/work3-allocator.after',
                   'status-1.page': 'sequence/work1-payload.after',
                   'undo-2.page': 'sequence/work2-payload.after',
                   'undo-3.page': 'sequence/work3-payload.after'}
        check_ledger(prior.observation(stage), expected, phase, disk[model.WAL])
        census = prior.check_census(stage)
        require(census['writer'] == observed_resources['writer_bytes']
                and census['file'] == observed_resources['file_bytes'], 'FAIL_ACTUAL_TYPE_CENSUS')
    outputs['system.descriptor'] = 'sequence/work3-terminal.descriptor'
    require({p.name for p in stage.glob('*.page')} == {name for name in outputs if name.endswith('.page')}, 'FAIL_PAGE_OUTPUT_CENSUS')
    for name, raw in outputs.items():
        require((stage / name).read_bytes() == literal(root, raw), 'FAIL_RESIDENT_CANONICAL_IMAGE:' + name)
    return dict(resources=observed_resources, effects=check_effects(stage, root, phase), disk=disk)


def file_receipts(folder, excluded=()):
    values = {}
    for path in sorted(folder.rglob('*')):
        if any(path == entry or entry in path.parents for entry in excluded) or not path.is_file():
            continue
        metadata = path.lstat()
        require(not path.is_symlink() and metadata.st_nlink == 1, 'FAIL_RECEIPT_FILE_TYPE')
        values[str(path.relative_to(folder))] = dict(bytes=metadata.st_size, sha256=digest(path))
    return values


def immutable_identity(root, binary, receipt):
    build = json.loads(receipt.read_text())
    source, elf = source_fingerprint(root), digest(binary)
    require(root == receipt.parent / 'source' and build.get('exit', build.get('exit_code')) == 0
            and build['command'] == BUILD and build['source_sha256'] == source
            and build['binary_sha256'] == elf, 'FAIL_BUILD_BINDING')
    rows = [json.loads(line) for line in (receipt.parent / 'build.jsonl').read_text().splitlines()]
    require([row['success'] for row in rows if row.get('reason') == 'build-finished'] == [True], 'FAIL_COMPILATION')
    with binary.open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'FAIL_ELF')
    logs = {name: digest(receipt.parent / name) for name in ('build.jsonl', 'build.stderr')}
    for name, field in [('build.jsonl', 'stdout_sha256'), ('build.stderr', 'stderr_sha256')]:
        if field in build:
            require(logs[name] == build[field], 'FAIL_BUILD_LOG_HASH')
    require(Path(__file__).resolve() == root / 'scripts/dev3_mixed_owner_native.py', 'FAIL_CHECKER_SOURCE')
    return dict(source_sha256=source, binary_sha256=elf, build_receipt_sha256=digest(receipt),
                build_logs=logs, checker_sha256=wire.checker_identities(root), fixture_sha256=fixture_identity(root))


def select_phases(phase, completed):
    require(completed == list(PHASES[:len(completed)]) and len(completed) <= 3, 'FAIL_COMPLETION_ORDER')
    pending = list(PHASES[len(completed):])
    require(pending, 'FAIL_HISTORY_ALREADY_COMPLETE')
    if phase == 'all':
        return pending
    require(phase == pending[0], 'FAIL_PHASE_ALREADY_COMPLETE_OR_PREREQUISITE')
    return [phase]


def verify_immutable_chain(path, identity, seen=None):
    path = path.resolve()
    seen = set() if seen is None else seen
    require(path not in seen, 'FAIL_ORIGIN_CYCLE')
    seen.add(path)
    report = json.loads(path.read_text())
    require(report['identity'] == identity, 'FAIL_RESUME_IDENTITY')
    require(report['status'] in ('PARTIAL', 'PASS', 'FAIL', 'INTERRUPTED'), 'FAIL_RESUME_STATUS')
    require(digest(path.parent / 'engine-tests') == identity['binary_sha256'], 'FAIL_RESUME_ELF_DRIFT')
    require(file_receipts(path.parent, (path, path.parent / 'engine-tests', path.parent / 'database'))
            == report['retention']['files'], 'FAIL_RESUME_RETAINED_OUTPUT_DRIFT')
    if report.get('origin'):
        origin = Path(report['origin']['path'])
        require(digest(origin) == report['origin']['sha256'], 'FAIL_ORIGIN_RECEIPT_DRIFT')
        verify_immutable_chain(origin, identity, seen)
    return report


def check_resume(root, path, identity, expected):
    # Every ancestor's frozen evidence survives continuation of the live DB.
    report = verify_immutable_chain(path, identity)
    completed = report['completed']
    require([row['phase'] for row in completed] == list(PHASES[:len(completed)]), 'FAIL_COMPLETION_ORDER')
    previous = None
    pids = []
    for row in completed:
        stage = Path(row['output'])
        require(digest(stage / 'phase.json') == row['receipt_sha256'], 'FAIL_PHASE_RECEIPT_DRIFT')
        receipt = json.loads((stage / 'phase.json').read_text())
        require(receipt['status'] == 'PASS' and receipt['identity'] == identity
                and receipt['phase'] == row['phase'] and receipt['pid'] == row['pid'], 'FAIL_PHASE_RECEIPT')
        require(file_receipts(stage, (stage / 'phase.json',)) == receipt['files'], 'FAIL_PHASE_OUTPUT_DRIFT')
        result = inspect(stage, root, row['phase'], expected[row['phase']], identity, row['pid'])
        require(result == receipt['oracle'], 'FAIL_PHASE_ORACLE_DRIFT')
        if previous is not None:
            preserve_inodes(previous, result['disk'])
        previous = result['disk']
        pids.append(row['pid'])
    require(len(pids) == len(set(pids)), 'FAIL_PID_REUSE')
    db = Path(report['database'])
    actual = prior.inventory(db)
    require(actual == report['database_inventory'], 'FAIL_RESUME_DATABASE_DRIFT')
    failed = report.get('failed_phase')
    if failed:
        stage = Path(failed['output'])
        require(digest(stage / 'phase.json') == failed['receipt_sha256'], 'FAIL_FAILED_RECEIPT_DRIFT')
        failed_receipt = json.loads((stage / 'phase.json').read_text())
        require(file_receipts(stage, (stage / 'phase.json',)) == failed_receipt['files'], 'FAIL_FAILED_OUTPUT_DRIFT')
        # A completed worker with a failing oracle must be reviewed without a
        # new product execution. Its original FAIL cannot become a silent retry.
        require(not failed_receipt['worker_completed'], 'FAIL_COMPLETED_WORKER_REQUIRES_OUTPUT_AUDIT')
    before = expected[completed[-1]['phase']] if completed else {}
    require(prior.wire_identity(before) == {name: {key: row[key] for key in ('bytes', 'sha256')}
            for name, row in actual.items()}, 'FAIL_PARTIAL_EFFECTS_REQUIRE_SEPARATE_RECOVERY_INPUT')
    if previous is not None:
        require(all(all(actual[name][key] == row[key] for key in ('device', 'inode', 'bytes', 'sha256'))
                    for name, row in previous.items()), 'FAIL_PREDECESSOR_INODE_DRIFT')
    return report


def preserve_inodes(before, after):
    # DWB is atomically replaced by the canonical flush protocol. All other
    # common files, including WAL, home, LOCK and installed banks, must persist.
    for name in set(before) & set(after) - {'DWB'}:
        require(all(before[name][key] == after[name][key] for key in ('device', 'inode')), 'FAIL_UNEXPECTED_INODE_CHANGE:' + name)


def execute_phase(root, binary, stage, db, phase, identity, completed, remaining, producer_identity=None):
    if producer_identity is not None:
        require(phase in ('recover', 'reentry') and all(
            re.fullmatch(r'[0-9a-f]{64}', producer_identity[key])
            for key in ('source_sha256', 'binary_sha256')), 'FAIL_APPROVED_PRODUCER_IDENTITY')
    stage.mkdir()
    before = prior.inventory(db)
    fresh_json(stage / 'before.json', before)
    command = [str(binary), '--exact', SELECTORS[phase], '--ignored', '--nocapture', '--test-threads=1']
    env = {key: value for key, value in os.environ.items() if not key.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_'))}
    outputs = {row['phase']: row['output'] for row in completed}
    env.update(PYTHONDONTWRITEBYTECODE='1', XADB_MIXED_OWNER_NATIVE_DB=str(db),
               XADB_MIXED_OWNER_NATIVE_OUT=str(stage), XADB_MIXED_OWNER_NATIVE_PRODUCER_OUT=outputs.get('produce', str(stage)),
               XADB_MIXED_OWNER_SOURCE_SHA256=identity['source_sha256'], XADB_MIXED_OWNER_INCLUDE=CASE_IDS[phase])
    if phase == 'reentry':
        env['XADB_MIXED_OWNER_NATIVE_RECOVERY_OUT'] = outputs['recover']
    if producer_identity is not None:
        env['XADB_MIXED_OWNER_APPROVED_PRODUCER_SOURCE_SHA256'] = producer_identity['source_sha256']
        env['XADB_MIXED_OWNER_APPROVED_PRODUCER_ELF_SHA256'] = producer_identity['binary_sha256']
    item = dict(phase=phase, status='RUNNING', command=command, identity=identity, timed_out=False,
                worker_completed=False, pid=None, exit_code=None)
    if producer_identity is not None:
        item['approved_producer_identity'] = {key: producer_identity[key] for key in ('source_sha256', 'binary_sha256')}
    fresh_json(stage / 'begin.json', dict(phase=phase, case_id=CASE_IDS[phase], command=command,
               identity=identity, approved_producer_identity=item.get('approved_producer_identity'),
               database=str(db), before_sha256=digest(stage / 'before.json')))
    started = time.monotonic()
    print(f'MIXED_OWNER_RUNNER_BEGIN\t{phase}', flush=True)
    try:
        require(remaining() > 0, 'FAIL_HISTORY_TIME')
        with (stage / 'stdout.log').open('x') as stdout, (stage / 'stderr.log').open('x') as stderr:
            with subprocess.Popen(command, cwd=root, env=env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                item['pid'] = child.pid
                try:
                    prior.finish_child(child, remaining, item)
                finally:
                    item['exit_code'] = child.returncode
        item['elapsed_seconds'] = time.monotonic() - started
        text = (stage / 'stdout.log').read_text()
        item['worker_completed'] = f'MIXED_OWNER_NATIVE_PASS\t{phase}\t{item["pid"]}' in text
        require(item['exit_code'] == 0 and not item['timed_out'], 'FAIL_PROCESS_EXIT_OR_TIMEOUT')
        require(item['pid'] not in {row['pid'] for row in completed}, 'FAIL_PID_REUSE')
        expected = expected_history(root)[phase]
        item['oracle'] = inspect(stage, root, phase, expected, identity, item['pid'])
        if producer_identity is not None:
            require(pairs(stage / 'producer-execution.tsv') == {
                'source_sha256': producer_identity['source_sha256'],
                'elf_sha256': producer_identity['binary_sha256']}, 'FAIL_IMPORTED_PRODUCER_OBSERVATION')
        after = prior.compare_files(db, expected)
        preserve_inodes(before, after)
        require(all(all(after[name][key] == row[key] for key in ('device', 'inode', 'bytes', 'sha256'))
                    for name, row in item['oracle']['disk'].items()), 'FAIL_SNAPSHOT_ACTUAL_IDENTITY')
        item['status'] = 'PASS'
    except BaseException as error:
        item['status'] = 'INTERRUPTED' if isinstance(error, KeyboardInterrupt) else 'FAIL'
        item['error'] = str(error) or type(error).__name__
    finally:
        item['elapsed_seconds'] = time.monotonic() - started
        stdout_path = stage / 'stdout.log'
        if stdout_path.exists() and item['pid'] is not None:
            item['worker_completed'] = f'MIXED_OWNER_NATIVE_PASS\t{phase}\t{item["pid"]}' in stdout_path.read_text()
        try:
            item['database_inventory'] = prior.inventory(db)
            item['files'] = file_receipts(stage)
        except BaseException as error:
            if 'error' in item:
                item['prior_error'] = item['error']
            item['status'], item['error'] = 'FAIL', str(error) or type(error).__name__
        fresh_json(stage / 'phase.json', item)
    print(f'MIXED_OWNER_RUNNER_{item["status"]}\t{phase}\t{item["pid"]}', flush=True)
    return item


def retain(output, db, binary, report):
    destination = output / 'retained-database'
    destination.mkdir()
    # The child has exited/reaped before this point, including timeout paths.
    for path in sorted(db.iterdir()):
        require(path.is_file() and not path.is_symlink(), 'FAIL_DATABASE_FILE_TYPE')
        with (destination / path.name).open('xb') as stream:
            stream.write(path.read_bytes())
    report['database_inventory'] = prior.inventory(db)
    # The original live DB may be continued by a later phase. Its immutable
    # retained-database copy, not the live subtree, belongs in this receipt.
    files = file_receipts(output, (binary, output / 'database'))
    report['retention'] = dict(files=files, bytes=sum(row['bytes'] for row in files.values()))
    log_bytes = sum(row['bytes'] for name, row in files.items() if name.endswith(('.log', '.tsv', '.txt')))
    require(report['retention']['bytes'] <= RETAINED_LIMIT and log_bytes <= LOG_LIMIT, 'FAIL_RETAINED_SPACE')


def run(root, binary, receipt, output, phase='all', resume=None):
    root, binary, receipt, output = (path.resolve() for path in (root, binary, receipt, output))
    require(not output.exists(), 'FAIL_OUTPUT_EXISTS')
    identity = immutable_identity(root, binary, receipt)
    expected = expected_history(root)
    old = check_resume(root, resume.resolve(), identity, expected) if resume else None
    completed = list(old['completed']) if old else []
    phases = select_phases(phase, [row['phase'] for row in completed])
    output.mkdir(parents=True, exist_ok=False)
    inputs = output / 'input'
    inputs.mkdir()
    fresh_json(inputs / 'identity.json', identity)
    fresh_json(inputs / 'expected-files.json', {mode: prior.wire_identity(files) for mode, files in expected.items()})
    retained_binary = output / 'engine-tests'
    with binary.open('rb') as source, retained_binary.open('xb') as destination:
        shutil.copyfileobj(source, destination)
    retained_binary.chmod(binary.stat().st_mode & 0o777)
    require(digest(retained_binary) == identity['binary_sha256'], 'FAIL_RETAINED_ELF')
    db = Path(old['database']) if old else output / 'database'
    if old is None:
        db.mkdir()
    fresh_json(inputs / 'database-before.json', prior.inventory(db))
    report = dict(schema=1, status='RUNNING', identity=identity, database=str(db), completed=completed,
                  selected=phases, not_run=list(phases), workers=[], elapsed_seconds=0,
                  origin={'path': str(resume.resolve()), 'sha256': digest(resume.resolve())} if resume else None,
                  limits=dict(history_seconds=SECONDS, database_bytes=prior.DB_LIMIT,
                              retained_bytes=RETAINED_LIMIT, log_bytes=LOG_LIMIT))
    started = time.monotonic()
    elapsed_before = old['elapsed_seconds'] if old else 0
    def remaining():
        return SECONDS - elapsed_before - (time.monotonic() - started)
    try:
        for mode in phases:
            require(immutable_identity(root, retained_binary, receipt) == identity, 'FAIL_PRE_PHASE_IDENTITY_DRIFT')
            require(remaining() > 0, 'FAIL_HISTORY_TIME')
            stage = output / mode
            report['not_run'].remove(mode)
            item = execute_phase(root, retained_binary, stage, db, mode, identity, completed, remaining)
            report['workers'].append({'phase': mode, 'output': str(stage), 'status': item['status'], 'pid': item['pid']})
            reference = dict(phase=mode, output=str(stage), pid=item['pid'], receipt_sha256=digest(stage / 'phase.json'))
            if item['status'] != 'PASS':
                report['failed_phase'] = reference
                report['status'], report['error'] = item['status'], item['error']
                break
            completed.append(reference)
        else:
            report['status'] = 'PASS' if len(completed) == 3 else 'PARTIAL'
        require(immutable_identity(root, retained_binary, receipt) == identity, 'FAIL_POST_PHASE_IDENTITY_DRIFT')
    except BaseException as error:
        if 'error' in report:
            report['prior_error'] = report['error']
        report['status'] = 'INTERRUPTED' if isinstance(error, KeyboardInterrupt) else 'FAIL'
        report['error'] = str(error) or type(error).__name__
    finally:
        try:
            retain(output, db, retained_binary, report)
            require(remaining() >= 0, 'FAIL_HISTORY_TIME')
        except BaseException as error:
            if 'error' in report:
                report['prior_error'] = report['error']
            report['status'], report['error'] = 'FAIL', str(error) or type(error).__name__
        report['elapsed_seconds'] = elapsed_before + time.monotonic() - started
        report['completed'] = completed
        report['not_run'] = [mode for mode in PHASES if mode not in {row['phase'] for row in completed}]
        fresh_json(output / 'result.json', report)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'binary', 'build-receipt', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--phase', choices=(*PHASES, 'all'), default='all')
    parser.add_argument('--resume', type=Path, help='previous immutable result.json; continue its original DB')
    args = parser.parse_args()
    result = run(args.root, args.binary, args.build_receipt, args.output, args.phase, args.resume)
    print(json.dumps({'status': result['status'], 'completed': [row['phase'] for row in result['completed']],
                      'not_run': result['not_run'], 'receipt': str(args.output.resolve() / 'result.json')}))
    return 0 if result['status'] in ('PASS', 'PARTIAL') else 1


if __name__ == '__main__':
    raise SystemExit(main())
