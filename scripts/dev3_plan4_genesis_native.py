#!/usr/bin/env python3
"""Exact plan4 native lineage; only unfinished phases run against one frozen ELF.

The original producer snapshot and the working database are separate durable
byte copies. Read-only phases retain full comparisons and identities, not extra
22 MB snapshots. All elapsed and output bounds include resumed invocations.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import time

sys.dont_write_bytecode = True
import dev3_plan4_genesis_cases as core

SCHEMA = 'XADB_PLAN4_NATIVE_V1'
PHASES = ('produce', 'select', 'reentry')
SELECTOR = 'storage::recovery_select::plan4::native::worker_plan4_genesis'
INPUT = Path('tests/fixtures/dev3-plan4-genesis')
PAGE, TOTAL, SECONDS = 16384, 22_069_248, 30.0
DB_LIMIT, HISTORY_LIMIT, LOG_LIMIT = 24 << 20, 128 << 20, 8 << 20
require, sha = core.require, core.sha
MUTATIONS = {'create', 'write', 'resize', 'sync', 'dirsync', 'publish', 'remove'}


def fresh_json(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def fixture(root):
    base = Path(root) / INPUT
    rows, inputs = {}, {}
    lines = (base / 'raw-index.tsv').read_text().splitlines()
    require(lines[0] == 'id\tpath\tbytes\tsha256', 'raw index header')
    for line in lines[1:]:
        key, relative, length, checksum = line.split('\t')
        path = Path(relative)
        require(not path.is_absolute() and '..' not in path.parts and key not in rows,
                'duplicate or unsafe raw reference')
        raw = (base / path).read_bytes()
        require(len(raw) == int(length) and digest(raw) == checksum, 'raw input drift')
        rows[key], inputs[str(path)] = raw, checksum
    require(len(rows) == 42 and sum(map(len, rows.values())) == 274720, 'raw input census')
    inputs['manifest.json'] = sha(base / 'manifest.json')
    inputs['raw-index.tsv'] = sha(base / 'raw-index.tsv')
    return rows, inputs


def expected_files(raw, profile):
    """Independent published extents and zero tails; no product encoder import."""
    require(profile in (1, 2, 3), 'unsupported profile')
    ref = lambda name: raw[f'REF-P{profile}-{name}']
    bank = ref('root') + bytes(PAGE - 320) + ref('chunk') + bytes(2_113_536 - PAGE - 256)
    values = {
        'LOCK': b'', 'd3-profile': ref('d3-profile'), 'WRITER': ref('WRITER'),
        'd3-wal-00000001': ref('d3-wal-00000001'), 'FILEMAP': ref('FILEMAP'),
        'd3-bootstrap': raw[f'XBS-P{profile}'], 'd3-filemap-0': bank,
        'd3-filemap-1': bytes(2_113_536),
        'd3-data-0000000100000001': b''.join(raw[f'P{i:02}'] for i in range(9)) + bytes(55 * PAGE),
        'DWB': ref('DWB'), 'd3-dwb-stage': ref('d3-dwb-stage'),
        'd3-dwb-0': ref('empty') + bytes(PAGE - 160), 'd3-dwb-1': bytes(PAGE),
        'd3-dwb-images': bytes(16_711_680), 'd3-dwb-reserved': bytes(8192),
    }
    require(len(values) == 15 and sum(map(len, values.values())) == TOTAL, 'expected census')
    return values


def compare_files(directory, expected):
    directory = Path(directory)
    require(directory.is_dir() and not directory.is_symlink(), 'database directory type')
    paths = {p.name: p for p in directory.iterdir()}
    require(set(paths) == set(expected), 'missing or extra database files')
    receipt, count, inodes = {}, 0, set()
    for name in sorted(paths):
        path, model = paths[name], expected[name]
        before = path.lstat()
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1, 'nonregular or hardlinked database file')
        require(before.st_size == len(model), 'database length mismatch')
        identity = (before.st_dev, before.st_ino)
        require(identity not in inodes, 'duplicate inode')
        inodes.add(identity)
        hasher = hashlib.sha256()
        with path.open('rb') as stream:
            opened = os.fstat(stream.fileno())
            require((opened.st_dev, opened.st_ino) == identity, 'opened identity mismatch')
            for offset in range(0, len(model), 65536):
                actual = stream.read(min(65536, len(model) - offset))
                require(actual == model[offset:offset + 65536], f'full byte mismatch {name}@{offset}')
                hasher.update(actual)
                count += len(actual)
            require(stream.read(1) == b'', 'trailing bytes')
        after = path.lstat()
        require((after.st_dev, after.st_ino, after.st_size, after.st_nlink) ==
                (before.st_dev, before.st_ino, before.st_size, 1), 'identity drift during compare')
        receipt[name] = {'device': before.st_dev, 'inode': before.st_ino,
                         'bytes': before.st_size, 'links': 1, 'sha256': hasher.hexdigest()}
    require(count == sum(map(len, expected.values())) and count <= DB_LIMIT, 'full byte accounting')
    return {'files': receipt, 'compared_bytes': count, 'full_byte_compare': True}


def require_stable(before, after):
    require(before == after, 'read-only file bytes or identities changed')


def clone_snapshot(source, destination, expected):
    source, destination = Path(source), Path(destination)
    before = compare_files(source, expected)
    destination.mkdir()
    for name in sorted(expected):
        with (source / name).open('rb') as src, (destination / name).open('xb') as dst:
            while block := src.read(65536):
                dst.write(block)
            dst.flush()
            os.fsync(dst.fileno())
    fd = os.open(destination, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)
    copied = compare_files(destination, expected)
    require_stable(before, compare_files(source, expected))
    for name in expected:
        require((before['files'][name]['device'], before['files'][name]['inode']) !=
                (copied['files'][name]['device'], copied['files'][name]['inode']), 'snapshot shares inode')
    return copied


def tsv(path):
    rows = [line.split('\t') for line in Path(path).read_text().splitlines()]
    require(all(len(row) == 2 for row in rows), 'key/value receipt shape')
    result = dict(rows)
    require(len(result) == len(rows), 'duplicate receipt field')
    return result


def validate_entropy(text, phase, pid):
    rows = text.splitlines()
    if phase == 'produce':
        require(rows == [f'1\t{pid}\t0102030405060708090a0b0c0d0e0f10'], 'entropy lineage')
    else:
        require(not rows, 'read-only entropy request')


def validate_effects(text, phase):
    events = []
    for index, line in enumerate(text.splitlines()):
        row = line.split('\t')
        require(len(row) == 6 and int(row[0]) == index, 'effect trace ordinal/shape')
        int(row[3]); int(row[4])
        require(not row[5].startswith('ERR'), 'native operation error')
        events.append(row)
    require(events, 'missing actual effect trace')
    acquired = [e for e in events if e[1] == 'acquire' and e[5] != 'ATTEMPT']
    require(len(acquired) == 1 and acquired[0][5] == ('CREATED' if phase == 'produce' else 'EXISTING'), 'lease lineage')
    entropy = [e for e in events if e[1] == 'entropy']
    require(len(entropy) == int(phase == 'produce'), 'entropy trace count')
    require(any(e[1] == 'drop_lease' for e in events), 'lease not released')
    if phase != 'produce':
        require(not any(e[1] in MUTATIONS for e in events), 'read-only native effect')
        require(any(e[1] == 'read' and e[2] == 'd3-data-0000000100000001' for e in events), 'selector data operation absent')
    return len(events)


def require_completion(text, case, process):
    require(process['exact'] and process['exit_code'] == 0 and not process['truncated'], 'incomplete worker')
    complete = [line.removeprefix('test ' + SELECTOR + ' ... ') for line in text.splitlines()
                if 'P4_NATIVE_COMPLETE\t' in line]
    require(complete == [f'P4_NATIVE_COMPLETE\t{case}\t{process["pid"]}'], 'missing/foreign reached operation')
    require('running 1 test' in text and '1 passed; 0 failed;' in text, 'non-exact libtest invocation')


def validate_phase(output, phase, profile, identity, process, epoch_digest):
    output = Path(output)
    case = f'P4-NATIVE-P{profile}-{phase.upper()}'
    text = (output / 'stdout.log').read_text()
    require_completion(text, case, process)
    actual = tsv(output / 'execution.tsv')
    expected = {'id': case, 'phase': phase, 'pid': str(process['pid']), 'profile': str(profile),
                **dict(zip(('source_sha256', 'elf_sha256', 'input_sha256'), identity))}
    require(actual == expected, 'execution identity')
    validate_entropy((output / 'entropy.tsv').read_text(), phase, process['pid'])
    count = validate_effects((output / 'effects.tsv').read_text(), phase)
    resources = tsv(output / 'resources.tsv')
    require(int(resources['handles_peak']) <= (3 if phase == 'produce' else 2)
            and int(resources['fd_peak']) <= 5, 'native handle bound')
    require(resources['handles_final'] == resources['fd_final'] == '0', 'native handle retained')
    require(int(resources['events']) == count and int(resources['entropy_calls']) == int(phase == 'produce'), 'resource trace accounting')
    if phase != 'produce':
        require(resources['mutations'] == '0', 'read-only mutation count')
        observed = tsv(output / 'observation.tsv')
        require(observed == {'plan': '4', 'profile': str(profile), 'high_water': '5',
                'allocated': '9', 'free': '55', 'jobs_free': '64', 'anchor': '7', 'root': '8',
                'entries': '0', 'epoch_digest': epoch_digest}, 'selection observation')
    return {'case': case, 'pid': process['pid'], 'execution': actual, 'resources': resources,
            'files': {p.name: sha(p) for p in output.iterdir() if p.is_file()}}


def planned_phases(completed, requested='all'):
    require(set(completed) <= set(PHASES), 'unknown completed phase')
    prefix = PHASES[:len(completed)]
    require(set(completed) == set(prefix), 'partial ancestor chain')
    available = list(PHASES)
    chosen = [p for p in PHASES if p not in completed] if requested == 'all' else [requested]
    selected = core.select_cases(available, chosen, None, completed)
    require(set(PHASES[:PHASES.index(chosen[0])]) <= set(completed), 'missing completed ancestor')
    return [p for p in PHASES if p in selected]


def execute_phase(source, binary, identity, profile, phase, database, output, allowance, log_allowance):
    output.mkdir()
    case = f'P4-NATIVE-P{profile}-{phase.upper()}'
    env = os.environ.copy()
    env.pop('XADB_PLAN4_EXCLUDE', None)
    env['XADB_PLAN4_INCLUDE'] = case
    for key, value in zip(('SOURCE', 'ELF', 'INPUT'), identity):
        env['XADB_PLAN4_GENESIS_' + key + '_SHA256'] = value
    env.update(XADB_PLAN4_GENESIS_PHASE=phase, XADB_PLAN4_GENESIS_PROFILE=str(profile),
               XADB_PLAN4_GENESIS_DB=str(database), XADB_PLAN4_GENESIS_OUT=str(output))
    command = [str(binary), SELECTOR, '--exact', '--ignored', '--nocapture', '--test-threads=1']
    invocation = {'command': command, 'cwd': str(source), 'env': {k: v for k, v in env.items() if k.startswith('XADB_PLAN4')},
                  'timeout_seconds': allowance, 'output_limit': log_allowance}
    fresh_json(output / 'invocation.json', invocation)
    result = core.invoke(command, source, env, output / 'stdout.log', output / 'stderr.log',
                         SELECTOR, [case], identity, allowance, allowance, log_allowance)
    fresh_json(output / 'process.json', result)
    return result


def tree_totals(roots):
    seen, total, logs = set(), 0, 0
    for root in roots:
        for path in Path(root).rglob('*'):
            if path.is_file():
                require(not path.is_symlink(), 'symlink in history')
                key = str(path.resolve())
                if key in seen: continue
                seen.add(key)
                length = path.stat().st_size
                total += length
                if path.suffix in ('.log', '.tsv', '.txt', '.json'): logs += length
    return total, logs


def retained_index(root):
    """Index partial bytes too; invalid/unfinished DBs are evidence, never PASS."""
    root = Path(root)
    return {str(p.relative_to(root)): {'bytes': p.lstat().st_size, 'sha256': sha(p)}
            for p in sorted(root.rglob('*')) if p.is_file() and not p.is_symlink()}


def require_retryable_attempt(attempt, stdout):
    """A failed oracle never authorizes repeating a completed worker operation."""
    if attempt['state'] == 'VERIFIED':
        return
    process = attempt.get('process')
    require(attempt['state'] == 'RETURNED' and process is not None,
            'uncertain native invocation requires audit')
    parsed = process.get('parsed', {})
    markers = parsed.get('markers', {})
    case = attempt['case']
    require(process['exit_code'] != 0 and process['stop_reason'] is None
            and not process['truncated'] and parsed.get('parser_error') is None
            and parsed.get('active') is None and markers.get('PASS') == []
            and markers.get('BEGIN') == [case] and markers.get('FAIL') == [case]
            and 'P4_NATIVE_COMPLETE\t' not in stdout,
            'uncertain or completed native operation requires audit')


def require_original_history(indexes):
    for root, before in indexes.items():
        require(retained_index(root) == before, 'original history changed during continuation')


def require_lineage_selection(unit_root, profile, resume_roots):
    """A new pathname/build cannot restart an existing profile's producer."""
    existing = set()
    for path in Path(unit_root).rglob('planned.json'):
        planned = json.loads(path.read_text())
        if planned.get('schema') == SCHEMA and planned.get('profile') == profile:
            existing.add(str(path.parent.resolve()))
    require(existing == set(map(str, resume_roots)),
            'existing native profile requires explicit complete lineage resume/audit')


def all_native_roots(unit_root):
    return sorted({str(path.parent.resolve()) for path in Path(unit_root).rglob('planned.json')
                   if json.loads(path.read_text()).get('schema') == SCHEMA})


def run_phases(phases, completed, execute, inspect):
    """The real orchestration loop; checker mocks only execute/inspect boundaries."""
    result = dict(completed)
    for phase in phases:
        require(phase not in result and set(PHASES[:PHASES.index(phase)]) <= set(result), 'phase order/replay')
        process = execute(phase)
        require(process['pid'] not in {p['pid'] for p in result.values()}, 'reused native PID')
        result[phase] = inspect(phase, process)
    return result


def main():
    cli = argparse.ArgumentParser()
    cli.add_argument('revision', type=Path)
    cli.add_argument('--output', required=True, type=Path)
    cli.add_argument('--resume', type=Path)
    cli.add_argument('--phase', choices=('all', *PHASES), default='all')
    cli.add_argument('--profile', type=int, choices=(1, 2, 3), default=1)
    cli.add_argument('--s5', action='store_true', help='explicit final S5 expansion budget, all native history192MiB')
    args = cli.parse_args()
    started = time.monotonic()
    revision, output = args.revision.resolve(), args.output.resolve()
    require(args.profile == 1 or args.s5, 'profiles2/3 require explicit S5 selection')
    history_limit = 192 << 20 if args.s5 else HISTORY_LIMIT
    require(output.parent == revision, 'output must be a new revision child')
    source, binary, _, build, identity = core.validate_build(revision)
    raw, raw_identity = fixture(source)
    expected = expected_files(raw, args.profile)
    completed, history, prior_elapsed, prior_roots = {}, [], 0.0, []
    original_indexes = {}
    database = output / 'database'
    snapshot = output / 'produce' / 'disk'
    if args.resume:
        prior_path = args.resume.resolve()
        old = json.loads(prior_path.read_text())
        require(old['schema'] == SCHEMA and old['finalized'] and old['identity'] == list(identity)
                and old['profile'] == args.profile, 'resume identity or finalization')
        require(not (prior_path.parent / 'AUDIT-HOLD.json').exists(), 'prior final audit requires HOLD')
        final_accounting = json.loads((prior_path.parent / 'final-accounting.json').read_text())
        require(final_accounting['result_sha256'] == sha(prior_path)
                and final_accounting['within_budget'], 'missing final budget accounting')
        for attempt in old['attempts']:
            stdout = Path(attempt['output']) / 'stdout.log'
            require_retryable_attempt(attempt, stdout.read_text() if stdout.exists() else '')
        completed = old['completed']
        history = old['history'] + [{'path': str(prior_path), 'sha256': sha(prior_path)}]
        for record in history:
            require(sha(record['path']) == record['sha256'], 'ancestor receipt drift')
        for old_root, index in old['retained_indexes'].items():
            actual = retained_index(old_root)
            actual.pop('result.json', None)
            actual.pop('final-accounting.json', None)
            require(actual == index, 'prior raw/log/failed database drift')
        prior_roots = old['output_roots']
        prior_elapsed = final_accounting['aggregate_elapsed_seconds']
        original_indexes = {root: retained_index(root) for root in prior_roots}
        database, snapshot = Path(old['database']), Path(old['producer_snapshot'])
        require('produce' in completed, 'failed producer cannot be silently restarted')
        require_stable(old['working_identity'], compare_files(database, expected))
        require_stable(old['producer_snapshot_identity'], compare_files(snapshot, expected))
        for record in completed.values():
            for name, checksum in record['files'].items():
                require(sha(Path(record['output']) / name) == checksum, 'completed phase input/log drift')
    phases = planned_phases(completed, args.phase)
    require_lineage_selection(revision.parent, args.profile, prior_roots)
    aggregate_roots = all_native_roots(revision.parent)
    for root in aggregate_roots:
        original_indexes.setdefault(root, retained_index(root))
    core.budget_remaining(prior_elapsed, time.monotonic() - started, SECONDS)
    output.mkdir()
    if not args.resume: database.mkdir()
    report = {'schema': SCHEMA, 'identity': list(identity), 'profile': args.profile,
              'revision': str(revision), 'database': str(database), 'producer_snapshot': str(snapshot),
              'history': history, 'output_roots': prior_roots + [str(output)],
              'aggregate_output_roots': aggregate_roots + [str(output)],
              'history_limit': history_limit, 's5': args.s5,
              'raw_identity': raw_identity, 'completed': completed, 'new_completed': [],
              'attempts': [],
              'selected_phases': phases, 'status': 'NOT_RUN', 'finalized': False,
              'runner_sha256': sha(Path(__file__)), 'build_receipt_sha256': sha(revision / 'build-receipt.json')}
    fresh_json(output / 'planned.json', report)
    current_before = compare_files(database, expected) if completed else None
    snapshot_identity = compare_files(snapshot, expected) if completed else None

    def execute(phase):
        nonlocal current_before
        core.validate_build(revision)
        require(fixture(source)[1] == raw_identity, 'fixture changed between phases')
        if phase != 'produce':
            require_stable(current_before, compare_files(database, expected))
            require_stable(snapshot_identity, compare_files(snapshot, expected))
        total, logs = tree_totals(report['aggregate_output_roots'])
        require(total < history_limit and logs < LOG_LIMIT - 65536, 'aggregate retained budget')
        allowance = min(10.0, core.budget_remaining(prior_elapsed, time.monotonic() - started, SECONDS))
        attempt = {'phase': phase, 'case': f'P4-NATIVE-P{args.profile}-{phase.upper()}',
                   'output': str(output / phase), 'state': 'INVOKING'}
        report['attempts'].append(attempt)
        fresh_json(output / f'{phase}-invoking.json', attempt)
        process = execute_phase(source, binary, identity, args.profile, phase, database, output / phase,
                                allowance, LOG_LIMIT - logs - 65536)
        attempt.update(state='RETURNED', process=process)
        fresh_json(output / f'{phase}-returned.json', attempt)
        return process

    def inspect(phase, process):
        nonlocal current_before, snapshot_identity
        row = validate_phase(output / phase, phase, args.profile, identity, process,
                             digest(expected['d3-wal-00000001']))
        after = compare_files(database, expected)
        if phase == 'produce': snapshot_identity = clone_snapshot(database, snapshot, expected)
        else:
            require_stable(current_before, after)
            require_stable(snapshot_identity, compare_files(snapshot, expected))
            require_original_history(original_indexes)
        current_before = after
        core.validate_build(revision)
        require(fixture(source)[1] == raw_identity, 'fixture changed after phase')
        row.update(output=str(output / phase), working_identity=after,
                   input_snapshot_ref=str(snapshot), producer_snapshot_identity=snapshot_identity)
        fresh_json(output / phase / 'receipt.json', row)
        row['files']['receipt.json'] = sha(output / phase / 'receipt.json')
        report['new_completed'].append(phase)
        report['completed'][phase] = row
        report['attempts'][-1]['state'] = 'VERIFIED'
        return row

    try:
        report['completed'] = run_phases(phases, completed, execute, inspect)
        report['status'] = 'PASS' if set(report['completed']) == set(PHASES) else 'PARTIAL_COMPLETE'
    except (ValueError, OSError, KeyError, AssertionError) as error:
        report['status'], report['error'] = 'PARTIAL_OR_FAIL', str(error)
    finally:
        report['working_identity'] = current_before
        report['producer_snapshot_identity'] = snapshot_identity
        try:
            require_original_history(original_indexes)
            if snapshot_identity is not None:
                require_stable(snapshot_identity, compare_files(snapshot, expected))
            for attempt in report['attempts']:
                text = Path(attempt['output']) / 'stdout.log'
                require_retryable_attempt(attempt, text.read_text() if text.exists() else '')
        except (ValueError, OSError, KeyError) as error:
            report['status'], report['audit_hold'] = 'AUDIT_HOLD', str(error)
        report['retained_indexes'] = {}
        for old_root in report['output_roots']:
            index = retained_index(old_root)
            index.pop('result.json', None)
            index.pop('final-accounting.json', None)
            report['retained_indexes'][old_root] = index
        # Full retained hashing is charged before budget finalization. Reserve
        # 64 KiB for these last two receipts, then check their actual size.
        reserve = 65536
        report['aggregate_elapsed_seconds'] = prior_elapsed + time.monotonic() - started
        report['retained_bytes'], report['log_bytes'] = tree_totals(report['aggregate_output_roots'])
        report['final_receipt_reserve_bytes'] = reserve
        if report['aggregate_elapsed_seconds'] > SECONDS or report['retained_bytes'] + reserve > history_limit or report['log_bytes'] + reserve > LOG_LIMIT:
            report['status'], report['budget_exceeded'] = 'AUDIT_HOLD', True
        report['finalized'] = True
        fresh_json(output / 'result.json', report)
        actual_total, actual_logs = tree_totals(report['aggregate_output_roots'])
        observed_through_result = prior_elapsed + time.monotonic() - started
        accounting = {'result_sha256': sha(output / 'result.json'),
                      'observed_through_result_seconds': observed_through_result,
                      'final_accounting_reserve_seconds': 1.0,
                      'aggregate_elapsed_seconds': observed_through_result + 1.0,
                      'retained_bytes_through_result': actual_total,
                      'log_bytes_through_result': actual_logs,
                      'final_receipt_reserve_bytes': reserve,
                      'within_budget': report['status'] != 'AUDIT_HOLD' and observed_through_result + 1.0 <= SECONDS}
        fresh_json(output / 'final-accounting.json', accounting)
        final_total, final_logs = tree_totals(report['aggregate_output_roots'])
        elapsed = prior_elapsed + time.monotonic() - started
        receipt_bytes = sum((output / name).stat().st_size for name in ('result.json', 'final-accounting.json'))
        if not accounting['within_budget'] or elapsed > accounting['aggregate_elapsed_seconds'] or final_total > history_limit or final_logs > LOG_LIMIT or receipt_bytes > reserve:
            report['status'] = 'AUDIT_HOLD'
            fresh_json(output / 'AUDIT-HOLD.json', {'reason': 'final audit/receipt bound',
                       'elapsed_seconds': elapsed, 'retained_bytes': final_total,
                       'log_bytes': final_logs, 'receipt_bytes': receipt_bytes})
    print(json.dumps({'status': report['status'], 'new_completed': report['new_completed'],
                      'elapsed': report['aggregate_elapsed_seconds']}))
    return 0 if report['status'] in ('PASS', 'PARTIAL_COMPLETE') else 1


if __name__ == '__main__':
    raise SystemExit(main())
