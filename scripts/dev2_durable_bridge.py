#!/usr/bin/env python3
"""Read retained native fault runs into the independent DEV2 oracle.

This is a post-run evidence bridge, not an engine runner. It preserves the
original fault-case, raw control frames and native journals, verifies the fixed
one-table fault profile, and derives actual observations from complete scans.
Strict acceptance requires pre-open captured images plus a retained pre-run
manifest and execution identity. Legacy POST_REOPEN observations remain separate
byte/ACK consistency evidence. Generated expectation hashes are post-run content
bindings, never invented pre-campaign receipts.
"""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import struct
import sys

sys.dont_write_bytecode = True
import dev2_durable_oracle as oracle

SPEC = {'name': 'items', 'columns': [{'name': 'value', 'data_type': 'int64', 'nullable': False}]}
LIMIT = 64 << 20
CONTROL_BYTES = 128 << 10
CONTROL_FRAME_BYTES = 4096
RETURN_ERROR_CUTS = frozenset(
    [('any', boundary) for boundary in ('create_before', 'create_after')] +
    [('wal_commit', boundary) for boundary in ('write_before', 'write_partial', 'write_after', 'sync_before', 'sync_after')] +
    [('recovery_old_file', boundary) for boundary in ('sync_before', 'sync_after')] +
    [(target, boundary) for target in ('current', 'writer') for boundary in
     ('write_before', 'write_partial', 'write_after', 'sync_before', 'sync_after', 'rename_before', 'rename_after', 'directory_sync_before', 'directory_sync_after')] +
    [('retirement', boundary) for boundary in ('unlink_before', 'unlink_after', 'directory_sync_before', 'directory_sync_after')])
EXPECTED_CASES = frozenset('return_error_matrix:' + target + ':' + boundary for target, boundary in RETURN_ERROR_CUTS) | {
    'partial_continue:wal_commit:write_partial', 'synced_unacknowledged_then_refault:wal_commit:sync_after'}
FORMAL_TESTS = frozenset(('partial_write_continue_returns_short_prefix_then_real_commit_ack',
    'synced_unacknowledged_commit_survives_kill_and_recovery_sync_refault',
    'every_declared_error_cut_has_a_flushed_reached_and_release_receipt',
    'control_from_another_session_never_qualifies_or_cleans_up_its_process'))


def bounded(path, maximum=LIMIT):
    with Path(path).open('rb') as stream:
        data = stream.read(maximum + 1)
    if len(data) > maximum:
        oracle.incomplete('bridge input bound: ' + str(path))
    return data


def document(path):
    return oracle.json_bytes(bounded(path, oracle.MAX_PROOF))


def binding(path):
    data = bounded(path)
    return {'path': str(path), 'bytes': len(data), 'sha256': oracle.sha(data)}


def file_hash(path):
    with Path(path).open('rb') as stream:
        before = os.fstat(stream.fileno())
        if before.st_size > 512 << 20:
            oracle.incomplete('retained binary/source file bound')
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if oracle.Image.stamp(before) != oracle.Image.stamp(os.fstat(stream.fileno())):
            oracle.invalid('retained file changed during hash')
    return digest


def source_hash(root):
    """Independent implementation of the documented source identity byte layout."""
    paths = []
    for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tests', 'scripts', '.github'):
        path = root / name
        paths.extend(item for item in path.rglob('*') if item.is_file()) if path.is_dir() else paths.append(path)
    if len(paths) > 16384:
        oracle.incomplete('formal source inventory bound')
    digest, total = hashlib.sha256(), 0
    for path in sorted(paths):
        relative = str(path.relative_to(root)).encode()
        data = bounded(path, 64 << 20)
        total += len(data)
        if total > 512 << 20 or path.is_symlink():
            oracle.incomplete('formal source byte/link bound')
        digest.update(struct.pack('<Q', len(relative)) + relative + struct.pack('<Q', len(data)) + data)
    return digest.hexdigest()


def formal_binding(manifest_path, execution_path, campaign_root):
    if manifest_path is None and execution_path is None:
        return None
    if manifest_path is None or execution_path is None:
        oracle.invalid('formal provenance requires manifest and execution together')
    manifest, execution = document(manifest_path), document(execution_path)
    if manifest.get('schema') != 1 or manifest.get('profile') != 'dev2-native-fault-v1' or manifest.get('planned_cases_per_campaign') != 33 or set(manifest.get('tests', [])) != FORMAL_TESTS or len(manifest['tests']) != 4:
        oracle.invalid('formal manifest frozen profile')
    if execution.get('manifest_sha256') != file_hash(manifest_path) or execution.get('exit_code') != 0:
        oracle.incomplete('formal execution lacks bound successful harness exit')
    campaigns = manifest.get('campaigns', [])
    if len(campaigns) != 3 or {item['repetition'] for item in campaigns} != {0, 1, 2} or len({item['artifacts'] for item in campaigns}) != 3:
        oracle.invalid('formal campaign repetition plan')
    matches = [item for item in campaigns if Path(item['artifacts']) == campaign_root]
    if len(matches) != 1:
        oracle.invalid('formal execution campaign root')
    campaign = matches[0]
    command = [manifest['harness']['path'], '--test-threads=1', '--nocapture']
    environment = {'XADBTEST_DURABLE_WORKER_BIN': manifest['worker']['path'], 'XADBTEST_NATIVE_ARTIFACTS': str(campaign_root)}
    if campaign['command'] != command or execution.get('command') != command or campaign['environment'] != environment or execution.get('environment') != environment:
        oracle.invalid('formal command/environment binding')
    created = datetime.fromisoformat(manifest['created_at'].replace('Z', '+00:00'))
    started = datetime.fromisoformat(execution['started_at'].replace('Z', '+00:00'))
    finished = datetime.fromisoformat(execution['finished_at'].replace('Z', '+00:00'))
    if not created <= started <= finished:
        oracle.invalid('pre-run manifest must precede execution')
    for role in ('harness', 'worker', 'oracle', 'bridge'):
        item = manifest[role]
        if not Path(item['path']).is_absolute() or file_hash(item['path']) != item['sha256']:
            oracle.invalid('formal retained ' + role + ' hash')
        if role in ('harness', 'worker') and any(execution.get(role + '_sha256_' + when) != item['sha256'] for when in ('before', 'after')):
            oracle.invalid('formal binary changed across execution')
    if manifest['oracle']['sha256'] != file_hash(oracle.__file__) or manifest['bridge']['sha256'] != file_hash(__file__):
        oracle.invalid('running independent checker differs from formal source')
    root = Path(manifest['source_root'])
    if not root.is_absolute() or source_hash(root) != manifest['source_sha256'] or any(execution.get('source_sha256_' + when) != manifest['source_sha256'] for when in ('before', 'after')):
        oracle.invalid('formal frozen source identity')
    if file_hash(execution['log']['path']) != execution['log']['sha256']:
        oracle.invalid('formal execution log hash')
    return {'manifest': binding(manifest_path), 'execution': binding(execution_path),
            'source_sha256': manifest['source_sha256'], 'worker_sha256': manifest['worker']['sha256'],
            'harness_sha256': manifest['harness']['sha256'], 'repetition': campaign['repetition']}


def within(path, root):
    resolved = Path(path).resolve()
    if not resolved.is_relative_to(root.resolve()):
        oracle.invalid('retained path escapes campaign root')
    return resolved


def frames(data):
    if len(data) > CONTROL_BYTES:
        oracle.incomplete('control raw byte bound')
    result, at = [], 0
    while at < len(data):
        if len(result) >= 64 or len(data) - at < 4:
            oracle.invalid('control frame count/header')
        length = struct.unpack_from('>I', data, at)[0]
        at += 4
        if length > CONTROL_FRAME_BYTES - 4 or at + length > len(data):
            oracle.invalid('control frame length')
        result.append(oracle.json_bytes(data[at:at + length]))
        at += length
    return result


def events(path):
    """Extract only; the oracle subsequently validates the complete raw journal."""
    raw = bounded(path)
    at, result = 0, []
    while at < len(raw):
        if len(result) > 4096 or len(raw) - at < 29:
            oracle.invalid('bridge native journal header/count')
        kind, sequence, nanos, request, length = struct.unpack_from('>BQQQI', raw, at)
        at += 29
        if length > 4 * oracle.MAX_PAYLOAD + 4 or at + length > len(raw):
            oracle.invalid('bridge native journal payload')
        payload = raw[at:at + length]
        at += length
        value = None
        if kind in (1, 3):
            if len(payload) < 4 or struct.unpack_from('>I', payload)[0] != len(payload) - 4:
                oracle.invalid('bridge native frame length')
            value = oracle.json_bytes(payload[4:])
        result.append({'kind': kind, 'sequence': sequence, 'nanos': nanos, 'request': request, 'value': value})
    return result


def fixed_case(case):
    if not isinstance(case, dict) or case.get('schema_version') != 1 or case.get('scope') != 'NATIVE_OS_FAULT_NOT_POWER_LOSS' or case.get('initial_position') != 2 or case.get('initial_values') != [0]:
        oracle.invalid('foreign native fault fixture')
    plan = case.get('plan')
    if not isinstance(plan, dict) or set(plan) != {'actor', 'directory', 'occurrence', 'cut'} or set(plan.get('cut', {})) != {'target', 'boundary'} or plan.get('actor') != 0 or plan.get('directory') != 0 or plan.get('occurrence') != 1 or case.get('case_kind', 'recovery') != 'recovery':
        oracle.invalid('foreign native fault plan')
    target, boundary = plan['cut']['target'], plan['cut']['boundary']
    name = case.get('case')
    if name == 'return_error_matrix' and (target, boundary) in RETURN_ERROR_CUTS:
        value = 7 if target == 'wal_commit' and boundary in ('write_after', 'sync_before', 'sync_after') else 0
    elif name == 'partial_continue' and target == 'wal_commit' and boundary == 'write_partial':
        value = 7
    elif name == 'synced_unacknowledged_then_refault' and target == 'wal_commit' and boundary == 'sync_after':
        value = 7
    elif name == 'recovery_stabilize_refault' and target == 'recovery_old_file' and boundary == 'sync_after':
        value = 7
    else:
        oracle.invalid('case outside frozen native fixture family')
    if case.get('expected_reopen_values') != [value]:
        oracle.reject('retained expectation differs from independent literal cut rule')
    return value


def case_key(case):
    fixed_case(case)
    cut = case['plan']['cut']
    return ':'.join((case['case'], cut['target'], cut['boundary']))


def campaign_inventory(cases):
    counts = {}
    for case in cases:
        key = case_key(case)
        counts[key] = counts.get(key, 0) + 1
    actual = set(counts)
    missing, unexpected = sorted(EXPECTED_CASES - actual), sorted(actual - EXPECTED_CASES)
    duplicate = sorted(key for key, count in counts.items() if count != 1)
    return {'verdict': 'COMPLETE' if not (missing or unexpected or duplicate) else 'INCOMPLETE',
            'expected_cases': len(EXPECTED_CASES), 'actual_cases': sum(counts.values()),
            'expected_unique_cuts': sorted(EXPECTED_CASES), 'actual_unique_cuts': sorted(actual),
            'missing': missing, 'unexpected': unexpected, 'duplicate': duplicate}


def verify_witness(witness, case):
    target, boundary = case['plan']['cut']['target'], case['plan']['cut']['boundary']
    phase = {'any': 'checkpoint', 'wal_commit': 'commit', 'recovery_old_file': 'open',
             'current': 'checkpoint', 'writer': 'open', 'retirement': 'garbage_collect'}[target]
    if witness.get('phase') != phase:
        oracle.invalid('reached phase differs from frozen action/cut')
    name = witness.get('file')
    if not isinstance(name, str) or len(name) > 128 or not name.isascii():
        oracle.invalid('reached filename bound')
    matched = oracle.FILE_PATTERN.fullmatch(name)
    if not matched or name == 'LOCK' or (name in ('CURRENT', 'WRITER') and target != 'recovery_old_file') or any(value is not None and int(value, 16) == 0 for index, value in enumerate(matched.groups()) if index != 5):
        oracle.invalid('reached filename/identity')
    if target == 'wal_commit' and not name.startswith('wal-') or target in ('current', 'writer') and not name.startswith('select-'):
        oracle.invalid('reached file does not implement target')
    fields = ('offset', 'requested_bytes', 'completed_bytes', 'frame_start', 'frame_end', 'frame_written_bytes')
    if any(field not in witness for field in fields):
        oracle.invalid('reached extent fields missing')
    limit = oracle.MAX_SEGMENT if name.startswith('wal-') else oracle.MAX_PAYLOAD + 42
    for field in fields:
        if witness[field] is not None:
            oracle.integer(witness[field], 0, limit)
    if boundary.startswith('write_'):
        offset = oracle.integer(witness['offset'], 0, limit)
        requested = oracle.integer(witness['requested_bytes'], 1, oracle.MAX_PAYLOAD + 42)
        completed = oracle.integer(witness['completed_bytes'], 0, requested)
        start = oracle.integer(witness['frame_start'], 0, limit)
        end = oracle.integer(witness['frame_end'], start + 85, limit)
        written = oracle.integer(witness['frame_written_bytes'], 0, end - start)
        if end - start > oracle.MAX_PAYLOAD + 42 or offset < start or offset + requested != end or offset + completed != start + written:
            oracle.invalid('reached write extent is inconsistent')
        if boundary == 'write_before' and (completed != 0 or written != 0 or offset != start):
            oracle.invalid('write-before must precede first frame byte')
        if boundary == 'write_partial' and (completed != 1 or requested <= 1 or written != 1 or offset != start):
            oracle.invalid('partial-write cut must preserve exactly one frame byte')
        if boundary == 'write_after' and completed == 0:
            oracle.invalid('write-after lacks positive completed extent')
    elif any(witness[field] is not None for field in ('offset', 'requested_bytes', 'completed_bytes')):
        oracle.invalid('non-write cut has spurious write extent')
    if (target == 'wal_commit' and boundary in ('write_after', 'sync_before', 'sync_after')) or (target in ('current', 'writer') and boundary in ('sync_before', 'sync_after')):
        start = oracle.integer(witness['frame_start'], 0, limit)
        end = oracle.integer(witness['frame_end'], start + 85, limit)
        if end - start > oracle.MAX_PAYLOAD + 42 or witness['frame_written_bytes'] != end - start:
            oracle.invalid('complete-WAL cut lacks a fully written bounded frame')
    elif not boundary.startswith('write_') and not (target in ('current', 'writer') and boundary in ('sync_before', 'sync_after')):
        if any(witness[field] is not None for field in ('frame_start', 'frame_end', 'frame_written_bytes')):
            oracle.invalid('non-frame boundary has spurious frame state')


def verify_control(lines, case, journal, manifest):
    """Require the exact completed handshake, independently of frame transport."""
    if not lines or len(lines) > 16 or not journal:
        oracle.invalid('fault control journal count')
    first = lines[0]
    if first.get('kind') != 'plan' or first.get('version') != 1 or first.get('plan') != case['plan'] or first.get('run') != manifest['run']:
        oracle.invalid('fault plan/case/run binding')
    token = first.get('token')
    oracle.database_hex(token)
    observed, releases, reached, kill, state = [], [], None, False, 'connected'
    last_ns = 0
    for index, entry in enumerate(lines):
        if entry.get('sequence') != index or oracle.integer(entry.get('elapsed_ns')) < last_ns:
            oracle.invalid('control sequence/time')
        last_ns = entry['elapsed_ns']
        kind = entry.get('kind')
        if index == 0:
            continue
        if kind == 'observe':
            message = entry['value']
            if message.get('version') != 1 or message.get('run') != manifest['run'] or message.get('token') != token:
                oracle.invalid('control observation identity')
            observed.append(message)
            event = message['event']
            if event['event'] != state or state not in ('connected', 'reached', 'released'):
                oracle.invalid('control handshake observation order')
            if state == 'connected':
                oracle.integer(event.get('process_id'), 1)
                state = 'reached'
            elif state == 'reached':
                reached = event['witness']
                if reached.get('plan') != case['plan'] or reached.get('matched') != case['plan']['occurrence'] or reached.get('actor') != 0 or reached.get('directory') != 0:
                    oracle.invalid('reached witness plan/actor/directory')
                target = case['plan']['cut']['target']
                if target != 'any' and reached.get('observed_target') != target:
                    oracle.invalid('reached target differs from plan')
                verify_witness(reached, case)
                state = 'decision'
            else:
                if event != {'event': 'released', 'decision': releases[0]['decision']}:
                    oracle.invalid('released decision differs from intent')
                state = 'complete'
        elif kind == 'release_intent':
            if state != 'decision':
                oracle.invalid('release lacks unique prior reached')
            value = entry['value']
            if value.get('version') != 1 or value.get('run') != manifest['run'] or value.get('token') != token:
                oracle.invalid('release identity')
            releases.append(value)
            state = 'released'
        elif kind == 'kill_intent':
            if state != 'decision' or entry.get('value') != reached:
                oracle.invalid('kill lacks exact prior reached')
            kill = True
            state = 'complete'
        elif kind == 'image':
            if state != 'complete' or index != len(lines) - 1:
                oracle.invalid('image must follow one completed control sequence')
            state = 'captured'
        else:
            oracle.invalid('unknown control journal event')
    if state not in ('complete', 'captured') or reached is None:
        oracle.invalid('incomplete control handshake')
    expected_decision = 'continue' if case['case'] == 'partial_continue' else 'return_error'
    if kill:
        if [item['kind'] for item in journal[-2:]] != [4, 6]:
            oracle.invalid('qualified control kill lacks native killed receipt')
    else:
        if len(releases) != 1 or releases[0].get('decision') != expected_decision or not any(m['event'] == {'event': 'released', 'decision': expected_decision} for m in observed):
            oracle.invalid('control decision lacks completed release acknowledgement')
    invocations = {item['request']: item for item in journal if item['kind'] == 1}
    cut_invoke = invocations.get(reached['request'])
    request = cut_invoke['value'] if cut_invoke else None
    if request is None or request['actor'] != reached['actor']:
        oracle.invalid('reached request is absent from external Invoke history')
    target = case['plan']['cut']['target']
    expected_operation = {'wal_commit': 'commit', 'recovery_old_file': 'open_database', 'writer': 'open_database',
                          'current': 'checkpoint', 'any': 'checkpoint', 'retirement': 'collect_garbage'}.get(target)
    if request['command']['action'].get('operation') != expected_operation:
        oracle.invalid('reached boundary is not the fixture operation')
    configurations = [item for item in invocations.values() if item['value']['command']['action'].get('operation') == 'configure_fault']
    if len(configurations) != 1:
        oracle.invalid('fault control requires one ConfigureFault invocation')
    configured = configurations[0]
    action = configured['value']['command']['action']
    if configured['value']['actor'] != 0 or action.get('token') != token or action.get('plan') != case['plan']:
        oracle.invalid('ConfigureFault differs from retained plan/token')
    returned = [item for item in journal if item['kind'] == 3 and item['request'] == configured['request']]
    if len(returned) != 1 or returned[0]['value'].get('observation') != {'result': 'done'} or not configured['sequence'] < returned[0]['sequence'] < cut_invoke['sequence']:
        oracle.invalid('ConfigureFault must complete before cut invocation')
    if kill and any(item['kind'] == 3 and item['request'] == cut_invoke['request'] for item in journal):
        oracle.invalid('blocked qualified kill cannot follow cut response')
    return observed, releases, reached, kill


def control(run, case, journal, manifest):
    raw_jsonl = bounded(run / 'fault-control.jsonl', CONTROL_BYTES)
    lines = [oracle.json_bytes(line) for line in raw_jsonl.splitlines()]
    observed, releases, reached, kill = verify_control(lines, case, journal, manifest)
    if frames(bounded(run / 'fault-control.bin', CONTROL_BYTES)) != observed:
        oracle.invalid('raw control frames differ from controller-flushed observations')
    release_path = run / 'fault-release.bin'
    sent = frames(bounded(release_path, CONTROL_BYTES)) if release_path.exists() else []
    if sent != releases:
        oracle.invalid('release raw frames differ from controller intents')
    case_hash = oracle.sha(bounded(run / 'fault-case.json', oracle.MAX_PROOF))
    first = lines[0]
    provenance = first.get('case_sha256') == case_hash
    return {'witness': reached, 'decision': 'owned_process_kill' if kill else releases[0]['decision'],
            'pre_effect_case_binding': bool(provenance),
            'connected_process': observed[0]['event']['process_id'],
            'image_event': lines[-1]['value'] if lines[-1]['kind'] == 'image' else None,
            'files': [binding(run / filename) for filename in ('fault-case.json', 'fault-control.jsonl', 'fault-control.bin')] +
                     ([binding(release_path)] if release_path.exists() else [])}


def captured_image(run, case, journal, manifest, control_evidence, link):
    receipt_path = run / 'crash-image.json'
    directory = run / 'crash-image'
    receipt, event = document(receipt_path), control_evidence['image_event']
    boundary = 'blocked_reached_owned_kill' if control_evidence['decision'] == 'owned_process_kill' else 'released_response_closed_owned_kill'
    if receipt.get('schema_version') != 1 or receipt.get('scope') != 'NATIVE_PROCESS_LOSS_IMAGE_NOT_POWER_LOSS' or receipt.get('boundary') != boundary or receipt.get('run') != manifest['run'] or receipt.get('worker_id') != control_evidence['connected_process']:
        oracle.invalid('captured image owner/boundary')
    if receipt.get('case_sha256') != file_hash(run / 'fault-case.json') or not control_evidence['pre_effect_case_binding'] or receipt.get('source_work_directory') != manifest['work_directory'] or receipt.get('directory_slot') != 0 or receipt.get('image') != 'crash-image' or receipt.get('witness') != control_evidence['witness']:
        oracle.invalid('captured image case/source/witness')
    if receipt.get('inventory_hash_format') != 'name_len_le64+name_ascii+size_le64+sha256_bytes_sorted_names_v1':
        oracle.invalid('captured image inventory format')
    if [item['kind'] for item in journal[-2:]] != [4, 6]:
        oracle.invalid('captured image lacks owned kill completion')
    if boundary == 'released_response_closed_owned_kill':
        cut_return = [item for item in journal if item['kind'] == 3 and item['request'] == control_evidence['witness']['request']]
        if len(cut_return) != 1:
            oracle.invalid('released capture lacks consumed cut response')
        invocations = {item['request']: item for item in journal if item['kind'] == 1}
        closed = [item for item in journal if item['kind'] == 3 and item['sequence'] > cut_return[0]['sequence'] and
                  item['value']['observation'] == {'result': 'done'} and
                  invocations[item['request']]['value']['command']['action']['operation'] ==
                  ('configure_engine' if control_evidence['witness']['phase'] == 'open' else 'close_database')]
        if len(closed) != 1 or closed[0]['sequence'] >= journal[-2]['sequence']:
            oracle.invalid('released capture lacks completed closed-state handshake')
    image = oracle.Image(directory)
    entries = []
    digest, total = hashlib.sha256(), 0
    for name in sorted(image.inventory):
        with image.open(name) as stream:
            hashed = hashlib.file_digest(stream, 'sha256').hexdigest()
        length = image.inventory[name][2]
        entries.append({'name': name, 'bytes': length, 'sha256': hashed})
        total += length
        encoded_name = name.encode('ascii')
        digest.update(struct.pack('<Q', len(encoded_name)) + encoded_name + struct.pack('<Q', length) + bytes.fromhex(hashed))
    image.confirm()
    if receipt.get('files') != entries or receipt.get('total_bytes') != total or receipt.get('inventory_sha256') != digest.hexdigest():
        oracle.invalid('captured image inventory differs from immutable bytes')
    expected_event = {'boundary': boundary, 'receipt': 'crash-image.json', 'receipt_sha256': file_hash(receipt_path),
                      'inventory_sha256': digest.hexdigest(), 'files': len(entries), 'bytes': total}
    if event != expected_event or link != {'directory': str(directory), 'receipt': str(receipt_path),
            'receipt_sha256': expected_event['receipt_sha256'], 'inventory_sha256': digest.hexdigest(), 'files': len(entries), 'bytes': total}:
        oracle.invalid('captured image control/reopen link binding')
    return {'directory': str(directory), 'receipt': binding(receipt_path), 'boundary': boundary,
            'inventory_sha256': digest.hexdigest(), 'files': len(entries), 'bytes': total}


def extract_observation(journals):
    namespace, table_fixture, row_fixture, ready_after_open, recovered = None, None, None, False, None
    for journal in journals:
        ready_after_open, recovered = False, None
        pending, views, cursors = {}, {}, {}
        for event in journal:
            if event['kind'] == 1:
                request = event['value']
                pending[event['request']] = request
            elif event['kind'] == 3:
                request = pending.pop(event['request'], None)
                if request is None:
                    oracle.invalid('bridge Observe without Invoke')
                actor, action = request['actor'], request['command']['action']
                observation = event['value']['observation']
                operation, result = action['operation'], observation['result']
                alias = actor, action.get('transaction')
                if operation in ('create_database', 'open_database') and result == 'ready':
                    observed_namespace = oracle.database_hex(observation['database']), observation['timeline']
                    if namespace is not None and namespace != observed_namespace:
                        oracle.invalid('native run changed database namespace')
                    namespace = observed_namespace
                    ready_after_open = operation == 'open_database'
                elif operation == 'create_table' and result == 'table':
                    if action['spec'] != SPEC or observation['spec'] != SPEC or table_fixture is not None:
                        oracle.invalid('native fixture is not one literal items table')
                    table_fixture = oracle.table_identity(observation['identity'], namespace)
                elif operation == 'statement' and result == 'statement':
                    for item in observation['inserted']:
                        if row_fixture is not None:
                            oracle.invalid('native fixture has multiple inserted rows')
                        row_fixture = oracle.table_identity(item['identity']['table'], namespace), item['identity']['row']
                elif operation == 'begin' and result == 'begun' and action['mode'] == 'read_only' and ready_after_open:
                    views[alias] = {'tables': [], 'rows': [], 'scanned': set(), 'eof': set()}
                elif operation == 'lookup' and result == 'table' and alias in views:
                    views[alias]['tables'].append(oracle.normalize_spec(observation['spec'], oracle.table_identity(observation['identity'], namespace)))
                elif operation == 'open_scan' and result == 'done' and alias in views:
                    cursor = actor, action['cursor']
                    cursors[cursor] = alias
                    views[alias]['scanned'].add(cursor)
                elif operation == 'next_page' and result == 'page':
                    cursor = actor, action['cursor']
                    alias = cursors.get(cursor)
                    if alias in views:
                        if cursor in views[alias]['eof']:
                            oracle.invalid('page after EOF in recovered scan')
                        for row in observation['rows']:
                            views[alias]['rows'].append({'table': oracle.table_identity(row['identity']['table'], namespace),
                                                        'row': row['identity']['row'], 'values': row['values']})
                        if observation['eof']:
                            views[alias]['eof'].add(cursor)
                elif operation == 'commit' and result == 'committed' and not observation['wrote'] and alias in views:
                    view = views.pop(alias)
                    if not view['scanned'] or view['eof'] != view['scanned'] or len(view['tables']) != 1:
                        oracle.incomplete('actual reopened scan did not cover the complete fixture table')
                    recovered = {'status': 'READY', 'q': observation['position'], 'state': {'tables': view['tables'], 'rows': view['rows']}}
    if namespace is None or table_fixture is None or row_fixture is None or recovered is None:
        oracle.incomplete('original identities or complete post-open scan are unavailable')
    if row_fixture[0] != table_fixture:
        oracle.invalid('fixture row/table binding')
    return namespace, table_fixture, row_fixture[1], recovered


def verify_refault(runs, manifests, journals, controls):
    if len(runs) != 3 or len(set(runs)) != 3 or len({oracle.database_hex(item['run']) for item in manifests}) != 3:
        oracle.invalid('refault requires three distinct paths and run identities')
    if len(controls) != 2 or any(item['decision'] != 'owned_process_kill' for item in controls):
        oracle.invalid('refault requires exactly two qualified kills')
    if controls[0]['witness']['plan']['cut'] != {'target': 'wal_commit', 'boundary': 'sync_after'} or controls[1]['witness']['plan']['cut'] != {'target': 'recovery_old_file', 'boundary': 'sync_after'}:
        oracle.invalid('refault requires WAL sync then old-file stabilization cuts')
    second_request = controls[1]['witness']['request']
    second_invoked = [item for item in journals[1] if item['kind'] == 1 and item['request'] == second_request]
    if len(second_invoked) != 1 or second_invoked[0]['value']['command']['action']['operation'] != 'open_database' or any(item['kind'] == 3 and item['request'] == second_request for item in journals[1]):
        oracle.invalid('second refault Open must remain pending at qualified kill')
    pending = {}
    opened = False
    for item in journals[2]:
        if item['kind'] == 1:
            pending[item['request']] = item['value']['command']['action']['operation']
        elif item['kind'] == 3:
            opened |= pending.pop(item['request'], None) == 'open_database' and item['value']['observation'].get('result') == 'ready'
    if not opened:
        oracle.invalid('third refault run lacks actual successful Open')


def strict_verdict(consistency, image_phase, provenance):
    # Reopened bytes cannot prove what recovery was presented with, even if a
    # later receipt happens to bind the same harness and literal expectation.
    if consistency != 'ACCEPTED':
        return consistency
    return 'ACCEPTED' if image_phase == 'pre_open' and provenance else 'INCOMPLETE'


def controller_negative(run, campaign_root, formal):
    case = document(run / 'fault-case.json')
    expected_plan = {'actor': 0, 'directory': 0, 'occurrence': 1, 'cut': {'target': 'wal_commit', 'boundary': 'sync_after'}}
    if case != {'schema_version': 1, 'scope': 'CONTROLLER_OWNER_NEGATIVE', 'case_kind': 'controller_negative',
                'case': 'owner_mismatch_does_not_kill', 'plan': expected_plan, 'required_reached': 0, 'qualified_kills': 0}:
        oracle.invalid('unknown controller-negative profile')
    manifest = document(run / 'manifest.json')
    lines = [oracle.json_bytes(line) for line in bounded(run / 'fault-control.jsonl', CONTROL_BYTES).splitlines()]
    if len(lines) != 2 or lines[0].get('kind') != 'plan' or lines[0].get('version') != 1 or lines[0].get('plan') != expected_plan or lines[0].get('run') != [120] * 16 or lines[0].get('token') != [120] * 16 or manifest.get('run') != [120] * 16:
        oracle.invalid('negative control plan identity')
    for index, line in enumerate(lines):
        if line.get('sequence') != index:
            oracle.invalid('negative control sequence')
        oracle.integer(line.get('elapsed_ns'))
    connection = lines[1].get('value', {})
    if lines[1].get('kind') != 'observe' or connection.get('version') != 1 or connection.get('run') != manifest['run'] or connection.get('token') != [120] * 16 or connection.get('event', {}).get('event') != 'connected' or frames(bounded(run / 'fault-control.bin', CONTROL_BYTES)) != [connection]:
        oracle.invalid('negative control must connect without reaching a cut')
    if (run / 'fault-release.bin').exists() and bounded(run / 'fault-release.bin', CONTROL_BYTES):
        oracle.invalid('negative controller unexpectedly released a cut')
    companions = [path.parent for path in campaign_root.glob('native-*/manifest.json') if document(path).get('run') == [121] * 16]
    if len(companions) != 1:
        oracle.incomplete('negative unrelated-process receipt unavailable')
    journals = []
    for path in (run, companions[0]):
        metadata, journal = document(path / 'manifest.json'), events(path / 'history.bin')
        if formal and (metadata.get('source_sha256') != formal['source_sha256'] or metadata.get('worker_sha256') != formal['worker_sha256']):
            oracle.invalid('negative run identity differs from frozen harness')
        if not journal or journal[-1]['kind'] != 5 or any(item['kind'] in (4, 6) for item in journal):
            oracle.invalid('negative ownership check killed a process')
        requests = {item['request']: item for item in journal if item['kind'] == 1}
        ready, closed, configured = None, False, False
        for item in journal:
            if item['kind'] != 3:
                continue
            action = requests[item['request']]['value']['command']['action']
            observation = item['value']['observation']
            if action['operation'] == 'create_database' and observation['result'] == 'ready':
                ready = oracle.database_hex(observation['database']), observation['timeline']
            if action['operation'] == 'close_database' and observation == {'result': 'done'}:
                closed = True
            if action['operation'] == 'configure_fault':
                if action.get('plan') != expected_plan or action.get('token') != [120] * 16 or observation != {'result': 'done'}:
                    oracle.invalid('negative ConfigureFault does not match plan')
                configured = True
        if ready is None or not closed or (path == run and not configured):
            oracle.invalid('negative process usability/close evidence absent')
        history = oracle.History(ready)
        history.journal(path / 'history.bin')
        journals.append(binding(path / 'history.bin'))
    return {'schema': 1, 'scope': 'controller_owner_negative',
            'verdict': 'ACCEPTED' if formal and lines[0].get('case_sha256') == file_hash(run / 'fault-case.json') else 'INCOMPLETE',
            'coverage': ['exact_frozen_harness_assertion', 'both_owned_processes_remain_usable', 'zero_reached_and_qualified_kills'],
            'runs': [str(run), str(companions[0])], 'journals': journals}


def bridge_case(run, campaign_root, output, formal=None):
    original_case = document(run / 'fault-case.json')
    expected_value = fixed_case(original_case)
    runs = [run]
    chain_path = run / 'refault-chain.json'
    chain_binding, reopen_binding, image_links = None, None, []
    if chain_path.exists():
        chain = document(chain_path)
        if chain.get('schema_version') != 1 or chain.get('expected_values') != [7] or chain.get('required_reached') != 2 or chain.get('qualified_kills') != 2 or len(chain.get('runs', [])) != 3:
            oracle.invalid('refault chain profile')
        runs = [within(path, campaign_root) for path in chain['runs']]
        if runs[0] != run or original_case['case'] != 'synced_unacknowledged_then_refault':
            oracle.invalid('refault first run binding')
        chain_binding = binding(chain_path)
        image_links = chain.get('images', [])
        if image_links and len(image_links) != 2:
            oracle.invalid('refault requires both pre-open images')
    elif original_case['case'] in ('synced_unacknowledged_then_refault', 'recovery_stabilize_refault'):
        oracle.invalid('refault case lacks its complete chain')
    elif (run / 'reopen-run.json').exists():
        reopen_path = run / 'reopen-run.json'
        reopened = document(reopen_path)
        if reopened.get('schema_version') != 1 or reopened.get('origin_run') != str(run) or reopened.get('expected_values') != [expected_value]:
            oracle.invalid('reopen link profile')
        runs.append(within(reopened['reopen_run'], campaign_root))
        if runs[0] == runs[1]:
            oracle.invalid('reopen requires a separate owned process run')
        image_links = [reopened['image']]
        reopen_binding = binding(reopen_path)
    manifests = [document(path / 'manifest.json') for path in runs]
    for manifest in manifests:
        if manifest.get('schema_version') != 1 or manifest.get('journal_format_version') != 1 or manifest.get('wire_protocol') != 'durable-v1' or manifest.get('self_test') is not False:
            oracle.invalid('retained native manifest profile')
        if formal and (manifest.get('source_sha256') != formal['source_sha256'] or manifest.get('worker_sha256') != formal['worker_sha256']):
            oracle.invalid('native worker/source differs from formal execution identity')
    if len({oracle.database_hex(item['run']) for item in manifests}) != len(manifests):
        oracle.invalid('reopen process reused native run identity')
    work = within(manifests[0]['work_directory'], campaign_root)
    if any(within(manifest['work_directory'], campaign_root) != work for manifest in manifests):
        oracle.invalid('refault runs do not share the owned database')
    journals = [events(path / 'history.bin') for path in runs]
    controls, captures = [], []
    for index, (path, manifest, journal) in enumerate(zip(runs, manifests, journals)):
        if (path / 'fault-case.json').exists():
            case = document(path / 'fault-case.json')
            if fixed_case(case) != expected_value:
                oracle.invalid('refault expected state changed')
            if chain_binding and (index > 1 or case['case'] != ('synced_unacknowledged_then_refault', 'recovery_stabilize_refault')[index]):
                oracle.invalid('refault control case order')
            controls.append(control(path, case, journal, manifest))
            if image_links:
                if index >= len(image_links):
                    oracle.invalid('unexpected controlled run after captured image')
                captures.append(captured_image(path, case, journal, manifest, controls[-1], image_links[index]))
            elif controls[-1]['image_event'] is not None or (path / 'crash-image.json').exists():
                oracle.invalid('captured image lacks its exact reopen/chain link')
    if chain_binding:
        verify_refault(runs, manifests, journals, controls)
    namespace, table, row, recovered = extract_observation(journals)
    expected_state = {'tables': [{'handle': table, **SPEC}],
                      'rows': [{'table': table, 'row': row, 'values': [{'type': 'int64', 'value': expected_value}]}]}
    expectation = {'version': 1, 'expected_outcome': 'RECOVER_OK', 'fault_model': 'process_kill' if chain_binding else 'deterministic_io',
                   'allow_incomplete_tail': True, 'database': list(bytes.fromhex(namespace[0])), 'timeline': namespace[1],
                   'history_complete': True, 'expected_state': expected_state}
    derived, observed = output / (run.name + '.expectation-derived.json'), output / (run.name + '.recovered.json')
    save(derived, expectation)
    save(observed, recovered)
    consistency = oracle.check(work / 'database-0', [path / 'history.bin' for path in runs], expectation, recovered)
    pre_open_checks = [oracle.check(item['directory'], [path / 'history.bin' for path in runs], expectation, recovered) for item in captures]
    consistency_verdict = next((item['verdict'] for item in [consistency, *pre_open_checks] if item['verdict'] != 'ACCEPTED'), 'ACCEPTED')
    image_phase = 'pre_open' if captures else 'post_reopen'
    provenance = formal is not None and all(item['pre_effect_case_binding'] for item in controls)
    verdict = strict_verdict(consistency_verdict, image_phase, provenance)
    limitations = ['actual scan covers all tables in this controlled one-table fixture, not a general catalog enumeration',
                   'native OS error/process-kill profile is not a power-loss model']
    if not captures:
        limitations.append('post_reopen image; the original pre-open crash image was not separately retained')
    if not provenance:
        limitations.append('pre-effect expectation/harness digest binding absent; derived expectation hashes are post-run only')
    result = {'schema': 1, 'verdict': verdict, 'consistency_verdict': consistency_verdict, 'consistency': consistency,
              'image_phase': image_phase, 'pre_open_checks': pre_open_checks, 'captured_images': captures,
              'runs': [str(path) for path in runs], 'controls': controls,
              'expectation_provenance': 'derived_post_run_from_retained_literal_fixture_and_raw_plan',
              'derived_expectation': binding(derived), 'actual_recovered_observation': binding(observed),
              'manifests': [binding(path / 'manifest.json') for path in runs], 'refault_chain': chain_binding,
              'reopen_link': reopen_binding, 'formal_execution': formal,
              'limitations': limitations}
    save(output / (run.name + '.result.json'), result)
    return result


def save(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2, sort_keys=True)
        stream.write('\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--manifest', help='retained pre-run formal manifest')
    parser.add_argument('--execution', help='matching completed harness execution receipt')
    args = parser.parse_args()
    root, output = Path(args.artifacts).resolve(), Path(args.output).resolve()
    if output.is_relative_to(root):
        oracle.invalid('bridge output must be outside retained raw campaign')
    output.mkdir(parents=True, exist_ok=False)
    try:
        formal = formal_binding(args.manifest, args.execution, root)
    except (oracle.OracleError, OSError, KeyError, TypeError, ValueError) as error:
        verdict = error.verdict if isinstance(error, oracle.OracleError) else 'INVALID'
        result = {'schema': 1, 'verdict': verdict, 'scope': 'formal_provenance', 'detail': str(error)}
        save(output / 'summary.json', result)
        print(json.dumps(result, indent=2, sort_keys=True))
        return {'REJECTED': 1, 'INVALID': 2, 'INCOMPLETE': 3}[verdict]
    candidates = sorted(path.parent for path in root.glob('native-*/fault-case.json'))
    if len(candidates) > 128:
        oracle.incomplete('native campaign case count')
    secondary = set()
    for run in candidates:
        chain = run / 'refault-chain.json'
        if chain.exists():
            try:
                linked = document(chain).get('runs', [])
                if len(linked) == 3 and within(linked[0], root) == run and len(set(linked)) == 3:
                    secondary.add(within(linked[1], root))
            except (oracle.OracleError, OSError, KeyError, TypeError, ValueError):
                pass  # The complete per-case validation below retains the error.
    results, negatives = [], []
    selected_cases = []
    inventory_errors = []
    for run in candidates:
        if run in secondary:
            continue
        try:
            case = document(run / 'fault-case.json')
            if case.get('case_kind') == 'controller_negative':
                negative = controller_negative(run, root, formal)
                negatives.append(negative)
                save(output / (run.name + '.controller-negative.json'), negative)
                continue
            case_key(case)
            selected_cases.append(case)
            results.append(bridge_case(run, root, output, formal))
        except (oracle.OracleError, OSError, KeyError, TypeError, ValueError) as error:
            verdict = error.verdict if isinstance(error, oracle.OracleError) else 'INVALID'
            result = {'schema': 1, 'verdict': verdict, 'consistency_verdict': 'NOT_RUN', 'run': str(run), 'detail': str(error)}
            save(output / (run.name + '.result.json'), result)
            results.append(result)
            inventory_errors.append({'run': str(run), 'detail': str(error)})
    inventory = campaign_inventory(selected_cases)
    inventory['case_errors'] = inventory_errors
    if inventory_errors:
        inventory['verdict'] = 'INCOMPLETE'
    negative_complete = len(negatives) == 1 and negatives[0]['verdict'] == 'ACCEPTED'
    run_counts = {}
    for result in [*results, *negatives]:
        for path in result.get('runs', []):
            run_counts[path] = run_counts.get(path, 0) + 1
    retained_runs = {str(path.parent) for path in root.glob('native-*/manifest.json')}
    run_inventory = {'unreferenced': sorted(retained_runs - set(run_counts)),
                     'missing': sorted(set(run_counts) - retained_runs),
                     'reused': sorted(path for path, count in run_counts.items() if count != 1)}
    if formal and any(run_inventory.values()):
        inventory['verdict'] = 'INCOMPLETE'
    counts = {verdict: sum(result['verdict'] == verdict for result in results) for verdict in ('ACCEPTED', 'REJECTED', 'INVALID', 'INCOMPLETE')}
    consistency = {verdict: sum(result['consistency_verdict'] == verdict for result in results) for verdict in ('ACCEPTED', 'REJECTED', 'INVALID', 'INCOMPLETE', 'NOT_RUN')}
    summary = {'schema': 1, 'scope': 'native_fault_selected_closure_and_ack_consistency',
               'cases': len(results), 'verdict_counts': counts, 'consistency_counts': consistency,
               'campaign_inventory': inventory,
               'formal_execution': formal, 'controller_negative': negatives,
               'controller_negative_complete': negative_complete,
               'run_inventory': run_inventory,
               'oracle_source': binding(Path(oracle.__file__)), 'bridge_source': binding(Path(__file__))}
    save(output / 'summary.json', summary)
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if results and counts['ACCEPTED'] == len(results) and inventory['verdict'] == 'COMPLETE' and negative_complete else (1 if counts['REJECTED'] else (2 if counts['INVALID'] else 3))


if __name__ == '__main__':
    sys.exit(main())
