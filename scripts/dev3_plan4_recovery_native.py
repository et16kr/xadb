#!/usr/bin/env python3
"""Bounded, independently audited N01/N02/N03 process chain for plan4 recovery.

Only a fresh admitted chain or an explicitly audited failed-chain successor
starts here. Completed or unfinished native phases cannot be replayed.
The worker's literal byte oracle and this controller's file/trace observations
are separate: a successful libtest exit alone is never a native acceptance.
"""
import argparse
import collections
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import stat
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location(
    '_plan4_recovery_native_process', ROOT / 'scripts/dev3_plan4_genesis_cases.py')
core = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(core)
require, sha = core.require, core.sha
SCHEMA = 'XADB_PLAN4_RECOVERY_NATIVE_V1'
INPUT_SCHEMA = 'XADB_PLAN4_RECOVERY_NATIVE_INPUT_V1'
GATE_SCHEMA = 'XADB_PLAN4_RECOVERY_NATIVE_ADMISSION_V1'
SUCCESSOR_SCHEMA = 'XADB_PLAN4_RECOVERY_NATIVE_FAILED_SUCCESSOR_V1'
RETIREMENT_SCHEMA = 'XADB_PLAN4_RECOVERY_NATIVE_DUPLICATE_RETIREMENT_V1'
SELECTOR = ('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
            'plan4_recovery::tests::native::recovery_three_process_worker')
PHASES = ('produce', 'recover', 'reentry')
CASES = dict(zip(PHASES, ('P4-REC-N01', 'P4-REC-N02', 'P4-REC-N03')))
BUDGET = {'wall_seconds': 30, 'worker_seconds': 29, 'audit_seconds': 1,
          'output_bytes': 8 << 20, 'database_bytes': 32 << 20,
          'retained_database_bytes': 128 << 20}
RECEIPT_RESERVE = 131072
WORKER_FILE_RESERVE = (4 << 20) + 8192
HOME = 'd3-data-0000000100000001'
MUTATIONS = {'create', 'write', 'resize', 'sync', 'dirsync', 'publish', 'remove'}
PAIRED = MUTATIONS | {'read', 'open', 'open_meta', 'acquire'}
UNPAIRED = {'retain_file', 'close', 'drop_directory', 'drop_lease', 'metadata', 'list', 'entropy'}


class MarkerParser(core.MarkerParser):
    def feed(self, line, now=None):
        line = line.removeprefix('test ' + self.selector + ' ... ')
        if line.startswith('P4_RECOVERY_'):
            super().feed('P4_' + line[len('P4_RECOVERY_'):], now)
        elif line.startswith(('P4_', 'G6_', 'XB_', 'G6RC_')):
            self.error = self.error or 'foreign native marker namespace'


# This is a private module instance; the existing runners are not modified.
core.MarkerParser = MarkerParser


def fresh_json(path, value):
    with Path(path).open('x') as stream:
        json.dump(value, stream, sort_keys=True, indent=2, allow_nan=False)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def relative_file(root, value):
    relative = Path(value)
    path = root / relative
    require(not relative.is_absolute() and '..' not in relative.parts
            and path.resolve().is_relative_to(root.resolve()), 'unsafe bound path')
    return path


def validate_inventory(inputs):
    require(inputs.get('schema') == INPUT_SCHEMA, 'foreign native input schema')
    expected = {case: {'phase': phase, 'selector': SELECTOR}
                for phase, case in CASES.items()}
    require(inputs.get('cases') == expected, 'native exact inventory mismatch')
    budget = inputs.get('budget')
    require(budget == BUDGET and all(type(v) in (int, float) and math.isfinite(v)
                                    for v in budget.values()), 'native frozen budget changed')


def identity_verifier(revision, input_path, inputs):
    build_path = revision / 'build-receipt.json'
    build = json.loads(build_path.read_text())
    source, binary = revision / 'source', revision / 'xadb-engine-tests'
    identity = (build['source_sha256'], build['binary_sha256'], sha(input_path))
    gate_path = relative_file(input_path.parent, inputs['effect_admission_path'])

    def verify():
        require(build.get('exit_code') == 0 and build.get('engine_artifact_fresh') is False,
                'unsuccessful or unproved fresh native build')
        require(build.get('command') == ['cargo', 'test', '-p', 'xadb-engine', '--lib',
                '--all-features', '--locked', '--no-run', '--message-format=json'],
                'unexpected native build command')
        require(identity[:2] == (inputs['approved_source_sha256'], inputs['approved_binary_sha256']),
                'unapproved native source or ELF')
        require(sha(build_path) == inputs['approved_build_receipt_sha256'], 'build receipt drift')
        require(sha(binary) == identity[1] and sha(input_path) == identity[2], 'ELF or input drift')
        with binary.open('rb') as stream:
            require(stream.read(4) == b'\x7fELF', 'not an ELF')
        require(core.source_fingerprint(source) == identity[0], 'source drift')
        require(sha(revision / 'build.log') == build['build_log_sha256'], 'build log drift')
        require(sha(source / 'Cargo.lock') == build['cargo_lock_sha256']
                and sha(source / 'rust-toolchain.toml') == build['toolchain_file_sha256'],
                'toolchain or lockfile drift')
        require(build['owned_files'] and inputs['source_input_bindings'], 'empty source binding')
        for bindings in (build['owned_files'], inputs['source_input_bindings']):
            for relative, digest in bindings.items():
                require(sha(relative_file(source, relative)) == digest, 'source input drift')
        for loaded in (Path(__file__), Path(core.__file__)):
            require(str(loaded.resolve().relative_to(ROOT)) in inputs['tool_bindings'],
                    'loaded native tool not bound')
        for relative, digest in inputs['tool_bindings'].items():
            require(sha(relative_file(ROOT, relative)) == digest, 'native tool drift')
        for relative, digest in inputs.get('immutable_fixture_bindings', {}).items():
            require(sha(relative_file(ROOT, relative)) == digest, 'immutable fixture drift')
        require(sha(gate_path) == inputs['effect_admission_sha256'], 'native gate drift')
        gate = json.loads(gate_path.read_text())
        require(gate.get('schema') == GATE_SCHEMA and gate.get('status') == 'ACCEPTED'
                and gate.get('f05') == 'ACCEPTED' and gate.get('independent') is True,
                'independent native F05/input gate not accepted')
        require((gate.get('source_sha256'), gate.get('binary_sha256')) == identity[:2]
                and gate.get('build_receipt_sha256') == inputs['approved_build_receipt_sha256'],
                'native gate build mismatch')
        for key in ('cases', 'budget', 'source_input_bindings', 'tool_bindings'):
            require(gate.get(key) == inputs[key], 'native gate closure mismatch: ' + key)
        require(gate.get('failed_chain_successor') == inputs.get('failed_chain_successor'),
                'native failed-chain gate closure mismatch')
        require(set(gate.get('allowed_ids', [])) == set(CASES.values()), 'native cases not admitted')
        evidence = gate.get('evidence_bindings', {})
        require(evidence, 'native gate lacks static/oracle evidence')
        for relative, digest in evidence.items():
            require(sha(relative_file(input_path.parent, relative)) == digest, 'native evidence drift')

    return source, binary, identity, verify


def previous_chains(unit, current=None):
    # Inspect only actual run roots, not exported source trees. A renamed output
    # or new build does not authorize starting N01 again.
    result = []
    for revision in Path(unit).iterdir():
        if not revision.is_dir() or revision.is_symlink():
            continue
        for run in revision.iterdir():
            planned = run / 'planned.json'
            if run.is_dir() and planned.is_file():
                old = json.loads(planned.read_text())
                if old.get('schema') == SCHEMA and run != current:
                    result.append(run)
    return sorted(result)


def no_previous_chain(unit):
    require(not previous_chains(unit),
            'prior native chain requires audit; completed phases cannot replay')


def read_tsv(path):
    rows = [line.split('\t') for line in Path(path).read_text().splitlines()]
    require(rows and all(len(row) == 2 for row in rows), 'worker receipt shape')
    result = dict(rows)
    require(len(result) == len(rows), 'duplicate worker receipt field')
    return result


def validate_entropy(text, phase, pid):
    expected = [f'1\t{pid}\t0102030405060708090a0b0c0d0e0f10'] if phase == 'produce' else []
    require(text.splitlines() == expected, 'native entropy lineage mismatch')


def validate_effects(text, phase):
    events, pending, active = [], None, collections.Counter()
    peak, mutations = 0, 0
    lease_dropped, directory_dropped = False, False
    for ordinal, line in enumerate(text.splitlines()):
        row = line.split('\t')
        require(len(row) == 6 and row[0] == str(ordinal), 'native trace ordinal/shape')
        _, operation, name, offset, length, outcome = row
        require(operation in PAIRED | UNPAIRED and offset.isdecimal() and length.isdecimal()
                and name and '/' not in name and name not in ('.', '..'), 'native trace fields')
        require(not directory_dropped and (not lease_dropped or operation in ('close', 'drop_directory')),
                'native operation after owner release')
        require('ERR' not in outcome and 'Err(' not in outcome, 'native trace contains failure')
        if outcome == 'ATTEMPT':
            require(operation in PAIRED and pending is None, 'overlapping native attempt')
            pending = row[1:5]
            mutations += operation in MUTATIONS
        elif operation in PAIRED:
            require(pending == row[1:5], 'unpaired native completion')
            pending = None
            if operation in ('read', 'write'):
                match = re.fullmatch(r'OK:(\d+):([0-9a-f]{64})', outcome)
                require(match is not None and int(match[1]) <= int(length), 'native byte result')
            elif operation == 'open_meta':
                require(re.fullmatch(r'\d+:\d+:\d+', outcome), 'native open metadata result')
            elif operation == 'acquire':
                require(outcome == ('CREATED' if phase == 'produce' else 'EXISTING'), 'native lease lineage')
            elif operation == 'publish':
                require(outcome in ('CURRENT:Ok(())', 'WRITER:Ok(())', 'DWB:Ok(())',
                                    'FILEMAP:Ok(())'), 'native publication outcome')
            else:
                require(outcome in ('OK', 'Ok(())'), 'native effect outcome')
        else:
            require(pending is None, 'missing native completion')
            if operation == 'retain_file':
                active[name] += 1
                peak = max(peak, sum(active.values()))
            elif operation == 'close':
                require(active[name] > 0, 'unowned native close')
                active[name] -= 1
            elif operation == 'drop_lease':
                # This event drops the writer wrapper, not the underlying flock:
                # each LinuxNamespaceFile retains Arc<LeaseState> until close.
                lease_dropped = True
            elif operation == 'drop_directory':
                require(lease_dropped and not any(active.values()),
                        'native directory released before lease/files')
                directory_dropped = True
            if operation in ('retain_file', 'close', 'drop_directory', 'drop_lease'):
                require(outcome == 'OK', 'native lifetime outcome')
            elif operation == 'entropy':
                require(outcome == '0102030405060708090a0b0c0d0e0f10', 'native entropy trace')
            else:
                require(outcome.startswith('Ok(') and outcome.endswith(')'), 'native query outcome')
        events.append(row)
    require(events and pending is None and sum(active.values()) == 0 and peak <= 3,
            'native lifetime incomplete or over bound')
    require(sum(e[1] == 'acquire' and e[5] != 'ATTEMPT' for e in events) == 1
            and sum(e[1] == 'drop_lease' for e in events) == 1
            and sum(e[1] == 'drop_directory' for e in events) == 1, 'native lease closure')
    require(sum(e[1] == 'entropy' for e in events) == int(phase == 'produce'), 'native entropy count')
    require(any(e[1] == 'read' and e[2] == HOME for e in events), 'native home observation absent')
    if phase != 'produce':
        epoch = 2 if phase == 'recover' else 3
        for operation, name in [('create', f'd3-wal-{epoch:08x}'),
                                ('remove', f'd3-wal-{epoch - 1:08x}'),
                                ('publish', 'd3-current-stage'), ('publish', 'd3-writer-stage')]:
            require(any(e[1:3] == [operation, name] and e[5] == 'ATTEMPT' for e in events),
                    'native selected epoch effect absent')
    if phase == 'reentry':
        require(not any(e[1] == 'write' and e[2] == HOME for e in events),
                'clean reentry rewrote home')
    return {'events': len(events), 'mutations': mutations, 'peak': peak}


def validate_phase(output, phase, identity, process):
    case = CASES[phase]
    require(type(process['pid']) is int and process['pid'] > 0 and process['exact']
            and process['exit_code'] == 0 and process['stop_reason'] is None
            and not process['truncated'], 'unfinished native worker')
    text = (output / 'stdout.log').read_text()
    parsed = core.parse_markers(text, SELECTOR, [case], identity)
    require(parsed == process['parsed'] and parsed['markers']['BEGIN'] == [case]
            and parsed['markers']['PASS'] == [case] and not parsed['markers']['FAIL']
            and not parsed['markers']['SKIP'], 'native process markers/context mismatch')
    lines = [line.removeprefix('test ' + SELECTOR + ' ... ') for line in text.splitlines()]
    complete = [line for line in lines if 'P4_RECOVERY_NATIVE_COMPLETE' in line]
    require(complete == [f'P4_RECOVERY_NATIVE_COMPLETE\t{case}\t{process["pid"]}']
            and 'running 1 test' in lines and '1 passed; 0 failed;' in text,
            'native reached completion/PID mismatch')
    worker = read_tsv(output / 'worker.tsv')
    expected_keys = {'case', 'phase', 'pid', 'handles', 'peak', 'fd_peak', 'events',
                     'mutations', 'entropy', 'log_bytes', 'source', 'elf', 'input'}
    require(set(worker) == expected_keys, 'unknown or missing native worker field')
    require(all(worker[k] == v for k, v in {'case': case, 'phase': phase,
            'pid': str(process['pid']), **dict(zip(('source', 'elf', 'input'), identity))}.items()),
            'native worker identity mismatch')
    for key in expected_keys - {'case', 'phase', 'source', 'elf', 'input'}:
        require(worker[key].isdecimal(), 'invalid native numeric receipt')
    validate_entropy((output / 'entropy.tsv').read_text(), phase, process['pid'])
    trace = validate_effects((output / 'effects.tsv').read_text(), phase)
    require(worker['handles'] == '0' and 0 < int(worker['peak']) <= 3
            and int(worker['peak']) <= int(worker['fd_peak']) <= 6,
            'native FD/handle bound')
    require(all(int(worker[k]) == trace[k] for k in trace) and trace['mutations'] > 0
            and int(worker['entropy']) == int(phase == 'produce')
            and int(worker['log_bytes']) == (output / 'effects.tsv').stat().st_size
            and int(worker['log_bytes']) <= 4 << 20, 'native resource/trace mismatch')
    return {'case': case, 'pid': process['pid'], 'worker': worker, 'trace': trace}


def snapshot_index(directory, copy_to=None, expected=None):
    """Stream every byte; reject aliases and changes during full copy/compare."""
    directory = Path(directory)
    require(directory.is_dir() and not directory.is_symlink(), 'snapshot directory type')
    paths = sorted(directory.iterdir())
    require(len(paths) <= 22, 'snapshot namespace bound')
    if expected is not None:
        require({p.name for p in paths} == set(expected['files']), 'snapshot file set drift')
    if copy_to is not None:
        copy_to.mkdir()
    rows, inodes, count = {}, set(), 0
    for path in paths:
        before = path.lstat()
        key = (before.st_dev, before.st_ino)
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and key not in inodes,
                'snapshot nonregular/hardlinked/duplicate inode')
        count += before.st_size
        require(count <= BUDGET['database_bytes'], 'snapshot database byte bound')
        inodes.add(key)
        digest = hashlib.sha256()
        with path.open('rb') as source:
            opened = os.fstat(source.fileno())
            require((opened.st_dev, opened.st_ino, opened.st_size) == (*key, before.st_size),
                    'snapshot opened identity drift')
            destination = (copy_to / path.name).open('xb') if copy_to is not None else None
            try:
                remaining = before.st_size
                while remaining:
                    block = source.read(min(65536, remaining))
                    require(block, 'snapshot truncated during read')
                    digest.update(block)
                    remaining -= len(block)
                    if destination is not None:
                        destination.write(block)
                require(source.read(1) == b'', 'snapshot grew during read')
                if destination is not None:
                    destination.flush()
                    os.fsync(destination.fileno())
            finally:
                if destination is not None:
                    destination.close()
        after = path.lstat()
        require((after.st_dev, after.st_ino, after.st_size, after.st_nlink) == (*key, before.st_size, 1),
                'snapshot named identity drift')
        rows[path.name] = {'device': key[0], 'inode': key[1], 'bytes': before.st_size,
                           'links': 1, 'sha256': digest.hexdigest()}
        if copy_to is not None:
            other = copy_to / path.name
            require((other.stat().st_dev, other.stat().st_ino) != key, 'snapshot reused source inode')
            with path.open('rb') as left, other.open('rb') as right:
                while block := left.read(65536):
                    require(right.read(len(block)) == block, 'snapshot full byte compare mismatch')
                require(right.read(1) == b'', 'snapshot trailing bytes')
            final = path.lstat()
            require((final.st_dev, final.st_ino, final.st_size, final.st_nlink) == (*key, before.st_size, 1),
                    'snapshot compare identity drift')
    result = {'files': rows, 'bytes': count, 'all_files_hashed': True,
              'stream_compared': copy_to is not None}
    if expected is not None:
        require(rows == expected['files'] and count == expected['bytes'], 'snapshot content/identity drift')
    return result


def output_totals(output):
    logs, database = 0, 0
    for path in output.rglob('*'):
        info = path.lstat()
        require(not stat.S_ISLNK(info.st_mode), 'symlink in native output')
        if stat.S_ISREG(info.st_mode):
            if path.relative_to(output).parts[0] == 'database' or 'disk' in path.relative_to(output).parts:
                database += info.st_size
            else:
                logs += info.st_size
    return logs, database


def file_bindings(directory):
    result = {}
    for path in sorted(directory.rglob('*')):
        info = path.lstat()
        require(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode),
                'nonregular preserved native evidence')
        if stat.S_ISREG(info.st_mode):
            require(info.st_nlink == 1, 'aliased preserved native evidence')
            result[str(path.relative_to(directory))] = sha(path)
    return result


def failed_chain_history(unit, current, admission=None):
    """Read-only admission for one terminated, wholly failed native attempt.

    The accepted gate binds this object separately from the new build. Neither
    changing a revision nor consuming the old failure can erase its resources.
    Only an independently audited duplicate working copy may be absent.
    """
    empty = {'wall_seconds': 0, 'worker_seconds': 0, 'audit_seconds': 0,
             'output_bytes': 0, 'retained_database_bytes': 0,
             'original_retained_database_bytes': 0, 'retired_duplicate_bytes': 0}
    if admission is None:
        require(not previous_chains(unit, current), 'prior native chain requires explicit audit')
        return empty
    required = {'schema', 'prior_run', 'prior_result_sha256', 'prior_identity',
                'preserved_file_bindings', 'failed_ids', 'never_completed_ids', 'retry_ids'}
    require(required <= set(admission) <= required | {'retired_working_copy'}
            and admission['schema'] == SUCCESSOR_SCHEMA, 'failed-chain admission shape')
    prior = relative_file(unit, admission['prior_run'])
    require(prior.parent.parent == unit and prior != current and not prior.is_symlink()
            and previous_chains(unit, current) == [prior], 'unaccounted prior native chain')
    result_path = prior / 'result.json'
    require(sha(result_path) == admission['prior_result_sha256'], 'prior result drift')
    result = json.loads(result_path.read_text())
    require(result.get('schema') == SCHEMA and result.get('status') == 'PARTIAL_OR_FAIL'
            and result.get('finalized') is True and result.get('post_execution_verified') is True
            and result.get('completed') == {} and not result.get('budget_exceeded')
            and not (prior / 'finalization-failure.json').exists(), 'prior chain not wholly failed/finalized')
    require(result.get('budget') == BUDGET and result.get('identity') == admission['prior_identity']
            and len(result['identity']) == 3
            and all(re.fullmatch('[0-9a-f]{64}', item) for item in result['identity']),
            'prior native identity/budget mismatch')
    require(admission['failed_ids'] == [CASES['produce']]
            and admission['never_completed_ids'] == list(CASES.values())
            and admission['retry_ids'] == list(CASES.values()), 'native completed IDs cannot replay')
    attempts = result.get('attempts', [])
    require(len(attempts) == 1 and attempts[0].get('state') == 'RETURNED'
            and attempts[0].get('phase') == 'produce' and attempts[0].get('case') == CASES['produce'],
            'unknown or completed prior native invocation')
    process = attempts[0]['process']
    require(type(process.get('pid')) is int and process['pid'] > 0
            and process.get('exact') is False and type(process.get('exit_code')) is int
            and process['exit_code'] != 0 and process.get('stop_reason') is None
            and process.get('truncated') is False, 'prior worker is unfinished or uncertain')
    prior_input = Path(result['input_path'])
    revision = prior.parent
    require(Path(result['revision']) == revision and prior_input.parent == unit,
            'prior native source placement')
    old_input = json.loads(prior_input.read_text())
    old_build = json.loads((revision / 'build-receipt.json').read_text())
    require(core.source_fingerprint(revision / 'source') == result['identity'][0]
            and sha(revision / 'xadb-engine-tests') == result['identity'][1]
            and sha(prior_input) == result['identity'][2]
            and sha(revision / 'build-receipt.json') == result['build_receipt_sha256']
            and old_build.get('exit_code') == 0
            and old_build.get('source_sha256') == result['identity'][0]
            and old_build.get('binary_sha256') == result['identity'][1]
            and old_input.get('approved_source_sha256') == result['identity'][0]
            and old_input.get('approved_binary_sha256') == result['identity'][1]
            and old_input.get('approved_build_receipt_sha256') == result['build_receipt_sha256'],
            'prior source/ELF/input/build proof drift')
    text = (prior / 'produce/stdout.log').read_text()
    parsed = core.parse_markers(text, SELECTOR, [CASES['produce']], result['identity'])
    require(parsed == process.get('parsed') and parsed.get('active') is None
            and parsed.get('parser_error') is None and parsed.get('contexts') == [CASES['produce']]
            and parsed.get('markers') == {'BEGIN': [CASES['produce']], 'FAIL': [CASES['produce']],
                                           'PASS': [], 'SKIP': []}
            and 'P4_RECOVERY_NATIVE_COMPLETE' not in text
            and json.loads((prior / 'produce/process.json').read_text()) == process,
            'prior native PASS/context/termination cannot be retried')
    require(admission['preserved_file_bindings'] and file_bindings(prior) == admission['preserved_file_bindings'],
            'prior native evidence missing, changed or added')
    original_snapshot = json.loads((prior / 'produce/snapshot.json').read_text())
    working, copied = original_snapshot['working'], original_snapshot['copy']
    require(result['snapshot_indexes'] == {'produce': copied} and working['stream_compared'] is True
            and working['bytes'] == copied['bytes'] and set(working['files']) == set(copied['files']),
            'prior full snapshot proof mismatch')
    for name, original in working['files'].items():
        retained = copied['files'][name]
        require(all(original[key] == retained[key] for key in ('bytes', 'links', 'sha256'))
                and (original['device'], original['inode']) != (retained['device'], retained['inode']),
                'prior snapshot was not an independent complete copy')
    snapshot_index(prior / 'produce/disk', expected=copied)
    retired, extra_output = 0, 0
    if 'retired_working_copy' in admission:
        binding = admission['retired_working_copy']
        require(set(binding) == {'receipt_path', 'receipt_sha256'}, 'retirement binding shape')
        receipt_path = relative_file(unit, binding['receipt_path'])
        require(sha(receipt_path) == binding['receipt_sha256'], 'retirement receipt drift')
        receipt = json.loads(receipt_path.read_text())
        require(receipt.get('schema') == RETIREMENT_SCHEMA and receipt.get('status') == 'RETIRED'
                and receipt.get('independent_review') == 'ACCEPTED'
                and receipt.get('prior_run') == admission['prior_run']
                and receipt.get('prior_result_sha256') == admission['prior_result_sha256']
                and receipt.get('comparison') == 'ALL_FILES_STREAM_EQUAL'
                and receipt.get('working_index') == working and receipt.get('snapshot_index') == copied
                and receipt.get('retired_bytes') == working['bytes'], 'unproved duplicate retirement')
        require(sha(relative_file(unit, receipt['review_path'])) == receipt['review_sha256'],
                'duplicate retirement review drift')
        database = prior / 'database'
        require(not database.exists() or (database.is_dir() and not database.is_symlink()
                                         and not list(database.iterdir())), 'retired working copy still exists')
        retired = working['bytes']
        if not receipt_path.is_relative_to(prior):
            extra_output = receipt_path.stat().st_size
    else:
        snapshot_index(prior / 'database', expected=working)
    logs, retained = output_totals(prior)
    require(retained + retired == result['retained_database_bytes'], 'prior retained byte accounting drift')
    wall, worker, audit = (result[key] for key in ('execution_wall_seconds', 'worker_seconds', 'audit_seconds'))
    require(all(type(value) in (int, float) and math.isfinite(value) and value >= 0
                for value in (wall, worker, audit, process['elapsed_seconds']))
            and worker == process['elapsed_seconds'] and worker <= wall
            and math.isclose(worker + audit, wall, abs_tol=1e-9)
            and wall <= BUDGET['wall_seconds'] and worker <= BUDGET['worker_seconds'],
            'prior cumulative timing invalid')
    return {'wall_seconds': wall, 'worker_seconds': worker, 'audit_seconds': audit,
            'output_bytes': logs + extra_output, 'retained_database_bytes': retained,
            'original_retained_database_bytes': result['retained_database_bytes'],
            'retired_duplicate_bytes': retired, 'prior_run': admission['prior_run'],
            'prior_result_sha256': admission['prior_result_sha256']}


def allowance(started, worker_seconds, prior_wall=0):
    elapsed = prior_wall + time.monotonic() - started
    require(0 <= worker_seconds <= elapsed, 'native cumulative timing invalid')
    # The manifest reserves one second for final audit. All intervening audit
    # time is charged to the same 30-second aggregate; it has no separate cap.
    left = min(BUDGET['worker_seconds'] - worker_seconds,
               BUDGET['wall_seconds'] - elapsed - BUDGET['audit_seconds'])
    require(left > 0, 'native process budget exhausted')
    return left


def execute_chain(revision, input_path, output):
    started = time.monotonic()
    inputs = json.loads(input_path.read_text())
    validate_inventory(inputs)
    unit = input_path.parent
    require(revision.parent == unit and output.parent == revision and not output.exists(),
            'native output/revision/input placement')
    lock = unit / '.recovery-native.lock'
    with lock.open('x') as stream:
        stream.write(str(os.getpid()) + '\n')
    report, verifier, final_hold, history = None, None, None, None
    try:
        source, binary, identity, current_verifier = identity_verifier(revision, input_path, inputs)
        current_verifier()
        history = failed_chain_history(unit, output, inputs.get('failed_chain_successor'))

        def verifier():
            current_verifier()
            require(failed_chain_history(unit, output, inputs.get('failed_chain_successor')) == history,
                    'native preserved history changed')

        def totals():
            logs, retained = output_totals(output)
            return logs + history['output_bytes'], retained + history['retained_database_bytes']

        allowance(started, history['worker_seconds'], history['wall_seconds'])
        output.mkdir()
        database = output / 'database'
        database.mkdir()
        report = {'schema': SCHEMA, 'identity': list(identity), 'status': 'NOT_RUN',
                  'revision': str(revision), 'input_path': str(input_path), 'budget': BUDGET,
                  'build_receipt_sha256': inputs['approved_build_receipt_sha256'],
                  'attempts': [], 'completed': {}, 'finalized': False,
                  'post_execution_verified': False, 'snapshot_indexes': {},
                  'history': history, 'audit_budget_mode': 'RESERVED_FINAL_AUDIT',
                  'failed_chain_successor': inputs.get('failed_chain_successor')}
        fresh_json(output / 'planned.json', report)
        try:
            for phase in PHASES:
                if report['completed']:
                    verifier()
                    previous = PHASES[PHASES.index(phase) - 1]
                    snapshot_index(database, expected=report['completed'][previous]['working'])
                process_seconds = sum(a.get('process', {}).get('elapsed_seconds', 0) for a in report['attempts'])
                timeout = allowance(started, process_seconds + history['worker_seconds'], history['wall_seconds'])
                logs, retained = totals()
                capacity = BUDGET['output_bytes'] - logs - WORKER_FILE_RESERVE - RECEIPT_RESERVE
                require(capacity > 0 and retained <= BUDGET['retained_database_bytes'],
                        'native output/snapshot budget exhausted')
                phase_out = output / phase
                phase_out.mkdir()
                env = {k: v for k, v in os.environ.items()
                       if not k.startswith(('XADB_PLAN4_', 'XADB_G6_', 'XADB_XB_'))}
                env.update(XADB_PLAN4_RECOVERY_INCLUDE=CASES[phase],
                           XADB_PLAN4_RECOVERY_NATIVE_PHASE=phase,
                           XADB_PLAN4_RECOVERY_NATIVE_DB=str(database),
                           XADB_PLAN4_RECOVERY_NATIVE_OUTPUT=str(phase_out))
                for key, value in zip(('SOURCE', 'ELF', 'INPUT'), identity):
                    env['XADB_PLAN4_RECOVERY_' + key + '_SHA256'] = value
                command = [str(binary), SELECTOR, '--exact', '--ignored', '--nocapture', '--test-threads=1']
                attempt = {'phase': phase, 'case': CASES[phase], 'state': 'INVOKING',
                           'command': command, 'cwd': str(source), 'timeout_seconds': timeout,
                           'env': {k: v for k, v in env.items() if k.startswith('XADB_PLAN4_RECOVERY_')}}
                report['attempts'].append(attempt)
                fresh_json(phase_out / 'invocation.json', attempt)
                process = core.invoke(command, source, env, phase_out / 'stdout.log', phase_out / 'stderr.log',
                                      SELECTOR, [CASES[phase]], identity, timeout, timeout, capacity)
                attempt.update(state='RETURNED', process=process)
                fresh_json(phase_out / 'process.json', process)
                require(process['pid'] not in {r['pid'] for r in report['completed'].values()},
                        'native PID reused across phases')
                # Preserve the actual database even when the worker oracle or
                # markers fail. A failed phase never enters completed.
                working_bytes = snapshot_index(database)['bytes']
                _, retained = totals()
                require(retained + working_bytes <= BUDGET['retained_database_bytes'],
                        'native snapshot would exceed retained history bound')
                working = snapshot_index(database, phase_out / 'disk')
                copied = snapshot_index(phase_out / 'disk')
                report['snapshot_indexes'][phase] = copied
                fresh_json(phase_out / 'snapshot.json', {'working': working, 'copy': copied})
                row = validate_phase(phase_out, phase, identity, process)
                row['working'] = working
                row['files'] = {p.name: sha(p) for p in phase_out.iterdir() if p.is_file()}
                report['completed'][phase] = row
                attempt['state'] = 'VERIFIED'
                fresh_json(phase_out / 'receipt.json', row)
                logs, retained = totals()
                require(logs + RECEIPT_RESERVE <= BUDGET['output_bytes']
                        and retained <= BUDGET['retained_database_bytes'], 'native aggregate bytes exceeded')
            verifier()
            report['post_execution_verified'] = True
            for phase, expected in report['snapshot_indexes'].items():
                snapshot_index(output / phase / 'disk', expected=expected)
            report['status'] = 'PASS'
        except Exception as error:
            report['status'], report['error'] = 'PARTIAL_OR_FAIL', str(error)
        finally:
            try:
                if not report['post_execution_verified']:
                    verifier()
                    report['post_execution_verified'] = True
            except Exception as error:
                report['status'], report['identity_audit_error'] = 'PARTIAL_OR_FAIL', str(error)
            process_seconds = sum(a.get('process', {}).get('elapsed_seconds', 0) for a in report['attempts'])
            elapsed = time.monotonic() - started
            report['invocation_worker_seconds'] = process_seconds
            report['invocation_wall_seconds'] = math.ceil((elapsed + 0.1) * 1000) / 1000
            report['worker_seconds'] = process_seconds + history['worker_seconds']
            report['execution_wall_seconds'] = report['invocation_wall_seconds'] + history['wall_seconds']
            report['audit_seconds'] = report['execution_wall_seconds'] - report['worker_seconds']
            logs, retained = totals()
            if (report['execution_wall_seconds'] > BUDGET['wall_seconds']
                    or report['worker_seconds'] > BUDGET['worker_seconds']
                    or logs + RECEIPT_RESERVE > BUDGET['output_bytes']
                    or retained > BUDGET['retained_database_bytes']):
                report['status'], report['budget_exceeded'] = 'AUDIT_HOLD', True
            report['retained_database_bytes'], report['output_bytes_before_receipt'] = retained, logs
            report['finalized'] = all(a['state'] in ('RETURNED', 'VERIFIED') for a in report['attempts'])
            fresh_json(output / 'result.json', report)
    finally:
        lock.unlink()
        if report is not None:
            elapsed = history['wall_seconds'] + time.monotonic() - started
            logs, retained = totals()
            if (elapsed > report['execution_wall_seconds'] or logs > BUDGET['output_bytes']
                    or retained > BUDGET['retained_database_bytes']):
                final_hold = {'status': 'HOLD', 'reason': 'finalization exceeded charged bound',
                              'result_sha256': sha(output / 'result.json'), 'elapsed_seconds': elapsed,
                              'output_bytes': logs, 'retained_database_bytes': retained}
                fresh_json(output / 'finalization-failure.json', final_hold)
    print(json.dumps({'status': 'HOLD' if final_hold else report['status'],
                      'receipt': str(output / 'result.json'), 'sha256': sha(output / 'result.json')}))
    return 0 if report['status'] == 'PASS' and final_hold is None else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('revision', 'input', 'output'):
        parser.add_argument('--' + flag, type=Path, required=True)
    args = parser.parse_args(argv)
    return execute_chain(args.revision.resolve(), args.input.resolve(), args.output.resolve())


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(json.dumps({'status': 'REJECTED_BEFORE_INVOKE_OR_HOLD', 'error': str(error)}))
        raise SystemExit(2)
