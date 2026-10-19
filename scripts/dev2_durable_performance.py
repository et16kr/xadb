#!/usr/bin/env python3
"""Bounded release Off/Always durability observation audit and bundle-only replay.

The native worker executes actual production calls; this controller owns literal
workload correctness, exact paired statistics and all verdicts. No optimization
promotion is supported. Run only in a centrally scheduled, otherwise idle slot.
"""
import argparse
from fractions import Fraction
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import struct
import sys
import time

sys.dont_write_bytecode = True
import dev2_durable_bridge as evidence
import dev2_durable_oracle as oracle

PROFILE = 'dev2-durable-observation-audit-v1'
PATH = 'dev2.retained_reader.sync_replace_readback.v1'
INPUT = {'version': 1, 'seed': 20261018, 'row_count': 128, 'warmup': 64, 'measure': 512, 'observations': False}
POLICY = {'primary': 'whole_cycle_measurement_elapsed_ns', 'cycle': 'RW Begin through sync Commit then fresh RO read and RO Commit',
          'old_reader': 'read-only transaction at q2 held through all measured cycles',
          'minimum_cost': 'forwarded namespace I/O counters are included in both modes',
          'pairs': 10, 'orders': 'seeded permutation of five AB and five BA',
          'interval_order_indices': [1, 8], 'nominal_interval_coverage': [1002, 1024],
          'effect_band': [[95, 100], [105, 100]], 'maximum_ab_ba_drift': [1, 10],
          'elapsed_cpu_external_guardrail': [115, 100], 'cycle_p99_guardrail': [125, 100],
          'rss_delta_guardrail_bytes': 4 << 20, 'affinity': [2],
          'maintenance': 'separate one-call costs, no percentile or response-time addition',
          'outer_timeout_seconds': 150, 'optimization_promotion': 'unsupported'}
SOURCE_INPUTS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'crates', 'tests', 'scripts', '.github')


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def orders(seed):
    value = ['AB'] * 5 + ['BA'] * 5
    for index in range(9, 0, -1):
        seed = (seed * 6364136223846793005 + 1442695040888963407) & ((1 << 64) - 1)
        other = seed % (index + 1)
        value[index], value[other] = value[other], value[index]
    return value


def run_id(campaign, index, side):
    return hashlib.sha256(bytes.fromhex(campaign) + bytes([index, side])).digest()[:16].hex()


def check_recipe(recipe):
    fields = {'schema', 'profile', 'kind', 'campaign_id', 'order_seed', 'input', 'policy', 'pairs',
              'worker', 'controller', 'hypothesis', 'original_recipe_sha256', 'prior_runs'}
    if not isinstance(recipe, dict) or set(recipe) != fields or recipe['schema'] != 1 or recipe['profile'] != PROFILE or recipe['kind'] not in ('instrumentation_audit', 'replay_audit'):
        oracle.invalid('durable performance recipe profile/fields')
    oracle.database_hex(list(bytes.fromhex(recipe['campaign_id'])))
    oracle.integer(recipe['order_seed'])
    if recipe['input'] != INPUT or recipe['policy'] != POLICY or not isinstance(recipe['hypothesis'], str) or not 1 <= len(recipe['hypothesis']) <= 4096:
        oracle.invalid('changed frozen workload, policy, or hypothesis')
    if not isinstance(recipe['pairs'], list) or len(recipe['pairs']) != 10:
        oracle.invalid('exactly ten durable pairs required')
    expected = [{'order': order, 'baseline_run': run_id(recipe['campaign_id'], index, 0),
                 'candidate_run': run_id(recipe['campaign_id'], index, 1)} for index, order in enumerate(orders(recipe['order_seed']))]
    if recipe['pairs'] != expected:
        oracle.invalid('run IDs or AB/BA order differ from predeclared campaign')
    fresh = {pair[side] for pair in expected for side in ('baseline_run', 'candidate_run')}
    prior = recipe['prior_runs']
    if not isinstance(prior, list) or len(prior) > 320 or len(prior) != len(set(prior)) or fresh.intersection(prior):
        oracle.invalid('run reuse/lineage bound')
    if recipe['kind'] == 'instrumentation_audit':
        if prior or recipe['original_recipe_sha256'] is not None:
            oracle.invalid('first audit has replay lineage')
    elif len(prior) < 20 or not isinstance(recipe['original_recipe_sha256'], str) or len(recipe['original_recipe_sha256']) != 64:
        oracle.invalid('replay lacks original recipe and prior runs')
    if set(recipe['worker']) != {'path', 'sha256', 'receipt', 'receipt_sha256', 'config', 'config_sha256', 'source_root', 'source_sha256'} or set(recipe['controller']) != {'path', 'sha256'}:
        oracle.invalid('build resource fields')


def check_resources(recipe, resources=None):
    worker, controller = recipe['worker'], recipe['controller']
    paths = resources or {'worker': worker['path'], 'receipt': worker['receipt'], 'config': worker['config'],
                          'source': worker['source_root'], 'controller': controller['path']}
    for key, expected in (('worker', worker['sha256']), ('receipt', worker['receipt_sha256']),
                          ('config', worker['config_sha256']), ('controller', controller['sha256'])):
        if evidence.file_hash(paths[key]) != expected:
            oracle.invalid('retained performance ' + key + ' identity')
    if evidence.source_hash(Path(paths['source'])) != worker['source_sha256']:
        oracle.invalid('retained performance source identity')
    receipt = evidence.document(paths['receipt'])
    build = receipt.get('build', {})
    if receipt.get('schema_version') != 1 or receipt.get('worker_sha256') != worker['sha256'] or receipt.get('config_sha256') != worker['config_sha256'] or build.get('source_sha256') != worker['source_sha256'] or build.get('profile') != 'release' or build.get('self_test') is not False or not build.get('toolchain') or not build.get('target'):
        oracle.invalid('external release build receipt mismatch')
    return build


def retained_resources(bundle):
    root = bundle / 'resources'
    return {name: root / filename for name, filename in {'worker': 'worker', 'controller': 'controller',
             'receipt': 'build-receipt.json', 'config': 'build-config.json', 'source': 'source'}.items()}


def retain_resources(recipe, bundle):
    paths = retained_resources(bundle)
    (bundle / 'resources').mkdir()
    source = recipe['worker']
    for role, original in (('worker', source['path']), ('receipt', source['receipt']), ('config', source['config']),
                           ('controller', recipe['controller']['path'])):
        shutil.copy2(original, paths[role])
    paths['source'].mkdir()
    for name in SOURCE_INPUTS:
        original, target = Path(source['source_root']) / name, paths['source'] / name
        shutil.copytree(original, target) if original.is_dir() else shutil.copy2(original, target)
    check_resources(recipe, paths)
    return paths


def host_observation():
    def optional(path):
        try:
            return evidence.bounded(path, 1 << 20).decode('utf-8', errors='replace')
        except OSError:
            return None
    return {'uname': list(platform.uname()), 'affinity': sorted(os.sched_getaffinity(0)),
            'loadavg': optional('/proc/loadavg'), 'cpuinfo': optional('/proc/cpuinfo'),
            'mountinfo': optional('/proc/self/mountinfo'), 'status': optional('/proc/self/status'),
            'cpu2_governor': optional('/sys/devices/system/cpu/cpu2/cpufreq/scaling_governor'),
            'cpu2_frequency_khz': optional('/sys/devices/system/cpu/cpu2/cpufreq/scaling_cur_freq'),
            'cpu2_siblings': optional('/sys/devices/system/cpu/cpu2/topology/thread_siblings_list')}


def invoke(command, stdout, stderr):
    """Bound only the owned controller process group; retain all partial output."""
    started = time.monotonic_ns()
    failure = None
    with stdout.open('xb') as out, stderr.open('xb') as err:
        process = subprocess.Popen(command, stdout=out, stderr=err, start_new_session=True)
        while process.poll() is None:
            if time.monotonic_ns() - started > POLICY['outer_timeout_seconds'] * 1_000_000_000:
                failure = 'outer controller timeout'
            elif max(stdout.stat().st_size, stderr.stat().st_size) > 8 << 20:
                failure = 'controller stdout/stderr byte bound'
            if failure:
                os.killpg(process.pid, signal.SIGKILL)
                break
            time.sleep(0.05)
        exit_code = process.wait()
    return {'command': command, 'exit_code': exit_code, 'error': failure,
            'controller_elapsed_ns': time.monotonic_ns() - started,
            'stdout': evidence.binding(stdout), 'stderr': evidence.binding(stderr)}


def ratio(value):
    return [value.numerator, value.denominator]


def analyze(pairs):
    result = {'validity': 'VALID', 'correctness': 'PASS', 'performance_effect': 'INCONCLUSIVE',
              'guardrails': 'PASS', 'cause_status': 'NOT_RUN', 'promotion': 'INCOMPLETE',
              'reasons': [], 'ratios': [], 'interval': None, 'median': None,
              'nominal_interval_coverage': [1002, 1024]}
    ratios, by_order = [], {'AB': [], 'BA': []}
    if len(pairs) != 10:
        result['validity'], result['correctness'] = 'INVALID', 'UNKNOWN'
        result['reasons'].append('missing/extra paired runs')
    for pair in pairs:
        baseline, candidate = pair.get('baseline'), pair.get('candidate')
        if not baseline or not candidate or any(item.get('verdict') != 'ACCEPTED' for item in (baseline, candidate)):
            result['validity'] = 'INVALID'
            result['correctness'] = 'FAIL' if any(item and item.get('verdict') == 'REJECTED' for item in (baseline, candidate)) else 'UNKNOWN'
            result['reasons'].append('failed, invalid, or incomplete run retained')
            continue
        if baseline['measurement_ns'] <= 0 or candidate['measurement_ns'] <= 0:
            result['validity'] = 'INVALID'
            continue
        elapsed = Fraction(candidate['measurement_ns'], baseline['measurement_ns'])
        ratios.append(elapsed)
        by_order[pair['order']].append(elapsed)
        for field, threshold in (('measurement_ns', Fraction(115, 100)), ('cpu_ns', Fraction(115, 100)),
                                 ('external_ns', Fraction(115, 100)), ('cycle_p99_ns', Fraction(125, 100))):
            left, right = baseline[field], candidate[field]
            if left is None or right is None or left <= 0:
                if result['guardrails'] != 'FAIL':
                    result['guardrails'] = 'UNKNOWN'
                result['reasons'].append('missing guardrail: ' + field)
            elif Fraction(right, left) > threshold:
                result['guardrails'] = 'FAIL'
                result['reasons'].append('guardrail failed: ' + field)
        if baseline['rss_bytes'] is None or candidate['rss_bytes'] is None:
            if result['guardrails'] != 'FAIL':
                result['guardrails'] = 'UNKNOWN'
        elif candidate['rss_bytes'] - baseline['rss_bytes'] > 4 << 20:
            result['guardrails'] = 'FAIL'
            result['reasons'].append('RSS high-water delta guardrail failed')
    if len(ratios) != 10 or any(len(values) != 5 for values in by_order.values()):
        result['validity'] = 'INVALID'
    else:
        ordered = sorted(ratios)
        low, high = ordered[1], ordered[8]
        result.update(ratios=[ratio(value) for value in ratios], interval=[ratio(low), ratio(high)],
                      median=ratio((ordered[4] + ordered[5]) / 2))
        if abs(sorted(by_order['AB'])[2] - sorted(by_order['BA'])[2]) > Fraction(1, 10):
            result['validity'] = 'INVALID'
            result['reasons'].append('AB/BA median drift exceeds 0.10')
        if result['validity'] == 'VALID':
            result['performance_effect'] = ('IMPROVED' if high < Fraction(95, 100) else
                'REGRESSED' if low > Fraction(105, 100) else
                'NO_MATERIAL_CHANGE' if low >= Fraction(95, 100) and high <= Fraction(105, 100) else 'INCONCLUSIVE')
    result['budget'] = 'PASS' if result['validity'] == 'VALID' and result['correctness'] == 'PASS' and result['guardrails'] == 'PASS' else 'INCOMPLETE'
    if result['guardrails'] == 'FAIL' or result['correctness'] == 'FAIL':
        result['budget'] = 'FAIL'
    return result


METRICS = ('dev2.wal.encode', 'dev2.wal.append', 'dev2.wal.direct_sync', 'dev2.wal.rotate', 'dev2.wal.epoch_install',
    'dev2.recovery.inspect', 'dev2.recovery.validate', 'dev2.recovery.apply', 'dev2.recovery.stabilize', 'dev2.recovery.direct_sync',
    'dev2.checkpoint.build', 'dev2.checkpoint.write', 'dev2.checkpoint.direct_sync', 'dev2.checkpoint.select',
    'dev2.backup.copy', 'dev2.backup.file', 'dev2.namespace.wait', 'dev2.namespace.hold', 'dev2.retention.inspect',
    'dev2.retention.collect', 'dev2.namespace.direct_sync')
EFFECTS = ('create', 'open', 'read', 'write', 'file_sync', 'resize', 'rename', 'directory_sync', 'remove')
STAGES = ('scan', 'checkpoint', 'garbage_collect', 'backup', 'restore', 'reopen')


def delta(before, after, name):
    old, new = oracle.integer(before[name]), oracle.integer(after[name])
    if new < old:
        oracle.invalid('cumulative counter reset: ' + name)
    return new - old


def metric_snapshot(snapshot, observed):
    if not isinstance(snapshot, dict) or set(snapshot) != {'identity', 'core', 'durability', 'effects'}:
        oracle.invalid('missing production/effect metrics snapshot')
    oracle.database_hex(snapshot['identity']['database'])
    oracle.integer(snapshot['identity']['timeline'], 1)
    core, durable, effects = snapshot['core'], snapshot['durability'], snapshot['effects']
    if core['resident_bytes'] > 64 << 20 or core['private_bytes'] > 8 << 20 or core['counters_saturated'] or core['coordinator_clock_invalid']:
        oracle.invalid('core accounting/clock/saturation')
    if durable['semantic_revision'] != 1 or durable['clock_invalid'] or durable['counters_saturated']:
        oracle.invalid('durability metric version/clock/saturation')
    gauges = durable['gauges']
    if gauges['control_reserved_bytes'] != 8 << 20 or not 0 <= gauges['control_used_bytes'] <= 8 << 20 or gauges['retained_wal_bytes'] > 256 << 20:
        oracle.invalid('durable gauges exceed registered budgets')
    operations = durable['operations']
    if not observed:
        if operations is not None:
            oracle.invalid('Off operations must be None, never fabricated zeros')
    else:
        if not isinstance(operations, list) or [item['metric_id'] for item in operations] != list(METRICS):
            oracle.invalid('missing, duplicate or stale metric IDs')
        for metric in operations:
            attempts, successes, failures = (oracle.integer(metric[key]) for key in ('attempts', 'successes', 'failures'))
            if successes + failures > attempts or failures or metric['unknown_partial_operations']:
                oracle.invalid('failed or inconsistent production scopes')
            timing = metric['elapsed']
            if timing is None or metric['timing_missing']:
                oracle.incomplete('required configured timing observation missing')
            buckets = timing['log2_ns']
            if len(buckets) != 64 or sum(oracle.integer(count) for count in buckets) != timing['samples'] or timing['samples'] != successes + failures or timing['max_ns'] > timing['sum_ns']:
                oracle.invalid('inclusive histogram sample accounting')
            for key in ('requested_bytes', 'confirmed_complete_bytes', 'items'):
                if metric[key] is not None:
                    oracle.integer(metric[key])
            if metric['requested_bytes'] is not None and metric['confirmed_complete_bytes'] is not None and metric['confirmed_complete_bytes'] > metric['requested_bytes']:
                oracle.invalid('confirmed wrapper bytes exceed requested bytes')
    if set(effects) != set(EFFECTS) | {'saturated'} or effects['saturated']:
        oracle.invalid('effect counter catalog/saturation')
    for name in EFFECTS:
        item = effects[name]
        if item['attempts'] != item['successes'] + item['failures'] or item['failures'] or item['unknown_partial_calls']:
            oracle.reject('failed or incomplete forwarded OS effects')
        if name in ('read', 'write'):
            if not 0 <= oracle.integer(item['confirmed_bytes']) <= oracle.integer(item['requested_bytes']):
                oracle.invalid('forwarded OS byte counter')
        elif item['requested_bytes'] is not None or item['confirmed_bytes'] is not None:
            oracle.invalid('non-byte effect has fabricated byte count')


def receipt(value, namespace, position, wrote, begin=None):
    if not isinstance(value, dict) or set(value) != {'transaction', 'position', 'wrote'}:
        oracle.invalid('actual commit receipt missing')
    identity = oracle.transaction_key(value['transaction'])
    if identity[:2] != namespace or value['position'] != position or value['wrote'] is not wrote or begin is not None and oracle.transaction_key(begin) != identity:
        oracle.reject('actual TxnId/q/commit meaning differs from external invocation')
    return identity


def check_output(output, input_value, build, external_ns, work):
    """Independently validate literal observations, actual ACK membership and bytes."""
    if output.get('version') != 1 or output.get('path') != PATH or output.get('input') != input_value or output.get('build') != build:
        oracle.invalid('performance input/path/source/build identity')
    if output.get('error') is not None:
        oracle.reject('product or fixture failure retained: ' + str(output['error']))
    if output.get('actual_affinity') != '2':
        oracle.incomplete('worker did not observe the predeclared CPU2 affinity')
    if output.get('directories') != {'source': 'source', 'backup': 'backup', 'restored': 'restored'}:
        oracle.invalid('unexpected fixture directories')
    setup = output['setup']
    namespace = oracle.database_hex(setup['identity']['database']), oracle.integer(setup['identity']['timeline'], 1)
    table = oracle.table_identity(setup['table'], namespace)
    spec = {'handle': table, 'name': 'items', 'columns': [{'name': 'value', 'data_type': 'int64', 'nullable': False}]}
    numbers = []
    for row in setup['rows']:
        if oracle.table_identity(row['table'], namespace) != table:
            oracle.reject('setup row table identity')
        number = oracle.integer(row['row'], 1)
        if number != (1 << 32) + len(numbers) + 1:
            oracle.reject('setup must observe all 128 unique issued row identities')
        numbers.append(number)
    if len(numbers) != 128 or table != (0x100000001, 1, 1):
        oracle.reject('initial table/row issuance differs from fixed fixture')
    batches = {}
    def record(value, q, changes, begin=None):
        key = receipt(value, namespace, q, True, begin)
        if key in batches:
            oracle.reject('duplicate actual ACK TxnId')
        batches[key] = {'q': q, 'changes': changes}
    def row_image(index, value):
        return {'table': table, 'row': numbers[index], 'values': [{'type': 'int64', 'value': value}]}
    record(setup['ddl'], 1, [{'change': 'create', 'table': spec}])
    record(setup['insert_commit'], 2, [{'change': 'put', 'row': row_image(index, index)} for index in range(128)], setup['insert_begin'])
    state, values, latencies = INPUT['seed'], list(range(128)), []
    for phase, count, offset in (('warmup', 64, 0), ('measurement', 512, 64)):
        batch = output[phase]
        if batch['completed'] != count or len(batch['iterations']) != count or oracle.integer(batch['elapsed_ns'], 1) == 0:
            oracle.reject('missing, retried, or incomplete offered cycles')
        for index, iteration in enumerate(batch['iterations']):
            ordinal = offset + index
            state = (state * 6364136223846793005 + 1442695040888963407) & ((1 << 64) - 1)
            value = state if state < 1 << 63 else state - (1 << 64)
            values[ordinal % 128] = value
            if iteration['ordinal'] != ordinal or iteration['completed'] is not True or iteration['read_value'] != value:
                oracle.reject('independent LCG/readback/ordinal mismatch')
            record(iteration['commit'], ordinal + 3, [{'change': 'put', 'row': row_image(ordinal % 128, value)}], iteration['begin'])
            receipt(iteration['read_commit'], namespace, ordinal + 3, False)
            elapsed = oracle.integer(iteration['elapsed_ns'], 1, batch['elapsed_ns'])
            for key in ('commit_ns', 'readback_ns'):
                oracle.integer(iteration[key], 0, elapsed)
            if phase == 'measurement':
                latencies.append(elapsed)
        if sum(item['elapsed_ns'] for item in batch['iterations']) > batch['elapsed_ns']:
            oracle.invalid('nonoverlapping cycle durations exceed enclosing batch')
    if output['old_values'] != list(range(128)):
        oracle.reject('retained q2 reader changed under replacement')
    receipt(output['old_commit'], namespace, 2, False)
    before, after = output['metrics_before'], output['metrics_after']
    for snapshot in (before, after):
        metric_snapshot(snapshot, input_value['observations'])
        if snapshot['identity'] != setup['identity']:
            oracle.invalid('measurement counter snapshot belongs to another engine')
    if before['durability']['gauges']['durable_position'] != 66 or after['durability']['gauges']['durable_position'] != 578:
        oracle.reject('measured WAL q delta differs from offered cycles')
    for name in EFFECTS:
        old, new = before['effects'][name], after['effects'][name]
        calls = delta(old, new, 'successes')
        if name == 'file_sync' and calls != 512 or name == 'write' and calls < 512 or name not in ('file_sync', 'write') and calls != 0:
            oracle.reject('forwarded OS effect coverage differs from 512 sync commits: ' + name)
    if delta(before['effects']['write'], after['effects']['write'], 'confirmed_bytes') != 512 * 175:
        oracle.reject('actual written batch bytes differ from frozen AXF1 row grammar')
    if input_value['observations']:
        for index in (0, 1, 2):
            old, new = before['durability']['operations'][index], after['durability']['operations'][index]
            if delta(old, new, 'successes') != 512:
                oracle.reject('production WAL encode/append/direct-sync coverage missing')
    phases = output['maintenance']
    if [item['stage'] for item in phases] != list(STAGES) or any(item['completed'] is not True for item in phases):
        oracle.reject('required production maintenance paths omitted or failed')
    for phase in phases:
        oracle.integer(phase['elapsed_ns'], 1)
        for snapshot in (phase['metrics_before'], phase['metrics_after']):
            if snapshot is not None:
                metric_snapshot(snapshot, input_value['observations'])
        if phase['stage'] in ('scan', 'restore', 'reopen') and phase['values'] != values:
            oracle.reject('maintenance actual full scan differs from independent rows')
        if phase['stage'] != 'garbage_collect' and phase['position'] != 578:
            oracle.reject('maintenance did not preserve exact q578 cut')
        old_effects, new_effects = phase['metrics_before']['effects'], phase['metrics_after']['effects']
        if phase['effects_before'] != old_effects or phase['effects_after'] != new_effects:
            oracle.invalid('maintenance forwarded effect snapshots disagree')
        if phase['metrics_before']['identity'] != setup['identity']:
            oracle.invalid('maintenance source counter identity')
        expected_identity = phase['identity'] if phase['stage'] == 'restore' else setup['identity']
        if phase['metrics_after']['identity'] != expected_identity:
            oracle.invalid('maintenance counter identity changed unexpectedly')
        for name in EFFECTS:
            delta(old_effects[name], new_effects[name], 'successes')
        if phase['stage'] in ('checkpoint', 'backup', 'restore', 'reopen') and delta(old_effects['file_sync'], new_effects['file_sync'], 'successes') == 0:
            oracle.reject('maintenance omitted actual forwarded file sync')
        if input_value['observations']:
            old_ops = {item['metric_id']: item for item in phase['metrics_before']['durability']['operations']}
            new_ops = {item['metric_id']: item for item in phase['metrics_after']['durability']['operations']}
            required = {'scan': (), 'checkpoint': ('dev2.checkpoint.build', 'dev2.checkpoint.select'),
                        'garbage_collect': ('dev2.retention.inspect', 'dev2.retention.collect'),
                        'backup': ('dev2.backup.copy',), 'restore': ('dev2.recovery.apply', 'dev2.wal.epoch_install'),
                        'reopen': ('dev2.recovery.inspect', 'dev2.recovery.apply', 'dev2.recovery.stabilize', 'dev2.wal.epoch_install')}[phase['stage']]
            for name in required:
                count = new_ops[name]['successes'] if phase['stage'] in ('restore', 'reopen') else delta(old_ops[name], new_ops[name], 'successes')
                if count < 1:
                    oracle.reject('required production path was not observed: ' + name)
    if setup['elapsed_ns'] + output['warmup']['elapsed_ns'] + output['measurement']['elapsed_ns'] + sum(item['elapsed_ns'] for item in phases) > external_ns:
        oracle.invalid('independent phase durations exceed external Invoke-to-Observe interval')
    expected_state = oracle.normalize_state({'tables': [spec], 'rows': [row_image(index, value) for index, value in enumerate(values)]})
    source = oracle.inspect_image(work / 'source')
    if source['identity'][:2] != namespace or source['q'] != 578 or source['state'] != expected_state:
        oracle.reject('actual source bytes/reopen q/state differ from external fixture')
    if set(source['batches']) != set(batches):
        oracle.reject('actual ACK transaction set differs from retained selected WAL')
    for key, expected in batches.items():
        actual = source['batches'][key]
        if actual['q'] != expected['q'] or oracle.canonical_changes(actual['changes']) != oracle.canonical_changes(expected['changes']):
            oracle.reject('whole ACK transaction afterimages/q differ from external commands')
    archive = oracle.Image(work / 'backup')
    current, _ = archive.metadata('CURRENT')
    if current['kind'] != 6 or current['previous'] is not None or 'WRITER' in archive.inventory:
        oracle.reject('backup is not an exact-cut checkpoint archive')
    manifest, archive_state = oracle.checkpoint(archive, current['current'], current['identity'])
    allowed = {'LOCK', 'CURRENT', oracle.manifest_name(manifest['generation'])} | {oracle.chunk_name(manifest['generation'], item['index']) for item in manifest['chunks']}
    if set(archive.inventory) - allowed or archive_state.snapshot() != expected_state or manifest['cut'] != 578:
        oracle.reject('archive contains wrong closure/cut/state')
    archive.confirm()
    restored = oracle.inspect_image(work / 'restored')
    provenance = restored['manifest']['provenance']
    if restored['identity'][0] != namespace[0] or restored['identity'][1] == namespace[1] or restored['q'] != 578 or restored['state'] != expected_state or provenance != {'database': namespace[0], 'timeline': namespace[1], 'generation': manifest['generation'], 'digest': current['current']['digest']}:
        oracle.reject('restore identity/exact-cut/provenance differs from source archive')
    return {'verdict': 'ACCEPTED', 'measurement_ns': output['measurement']['elapsed_ns'],
            'cpu_ns': output['cpu_ns'], 'external_ns': external_ns, 'cycle_p99_ns': sorted(latencies)[506],
            'rss_bytes': output['process_peak_rss_bytes'], 'observed_commits': len(batches),
            'maintenance': phases, 'source_selected_files': source['closure'], 'restore_selected_files': restored['closure']}


def inspect_run(directory, identifier, input_value, recipe, build):
    manifest, completion = evidence.document(directory / 'manifest.json'), evidence.document(directory / 'completion.json')
    if manifest.get('schema_version') != 1 or manifest.get('journal_format_version') != 1 or manifest.get('wire_protocol') != 'durable-performance-v1' or manifest.get('run') != list(bytes.fromhex(identifier)) or manifest.get('self_test') is not False or manifest.get('profile') != 'release' or manifest.get('source_sha256') != recipe['worker']['source_sha256'] or manifest.get('worker_sha256') != recipe['worker']['sha256']:
        oracle.invalid('native performance manifest identity')
    if manifest.get('work_directory') != completion.get('directory', '') + '/work' or completion.get('requests') != 1 or completion.get('observations') != 1 or completion.get('completion') != 'TRANSPORT_CLOSED_NO_PRODUCT_VERDICT':
        oracle.invalid('native completion/owned working directory')
    for filename, key in (('worker', 'worker_sha256'), ('history.bin', 'history_sha256'), ('stdout.bin', 'stdout_sha256'), ('stderr.bin', 'stderr_sha256')):
        if evidence.file_hash(directory / filename) != completion.get(key):
            oracle.invalid('retained raw native artifact digest: ' + filename)
    if completion['worker_sha256'] != recipe['worker']['sha256'] or evidence.document(directory / 'input.json') != input_value:
        oracle.invalid('worker or retained input differs from frozen recipe')
    journal = evidence.events(directory / 'history.bin')
    if [item['kind'] for item in journal] != [1, 2, 3, 5] or [item['sequence'] for item in journal] != list(range(4)) or any(left['nanos'] > right['nanos'] for left, right in zip(journal, journal[1:])):
        oracle.invalid('one-batch immediate native journal sequence')
    request, response = journal[0]['value'], journal[2]['value']
    identity = {'version': 1, 'run': list(bytes.fromhex(identifier)), 'request': 1, 'actor': 0}
    if request != {**identity, 'input': input_value} or any(response.get(key) != value for key, value in identity.items()) or set(response) != set(identity) | {'output'} or [item['request'] for item in journal] != [1, 1, 1, 0]:
        oracle.invalid('native request/response binding')
    raw = evidence.bounded(directory / 'stdout.bin', 1 << 20)
    if len(raw) < 4 or struct.unpack_from('>I', raw)[0] != len(raw) - 4 or oracle.json_bytes(raw[4:]) != response or evidence.document(directory / 'performance-output.json') != response:
        oracle.invalid('worker stdout differs from immediately retained observation')
    external_ns = journal[2]['nanos'] - journal[0]['nanos']
    checked = check_output(response['output'], input_value, build, external_ns, directory / 'work')
    checked.update(native_directory=directory.name, manifest=evidence.binding(directory / 'manifest.json'),
                   history=evidence.binding(directory / 'history.bin'), output=evidence.binding(directory / 'performance-output.json'))
    return checked


def failure(error):
    return {'verdict': error.verdict if isinstance(error, oracle.OracleError) else 'INVALID', 'detail': str(error)}


def prepare(builds, campaign, kind='instrumentation_audit', original=None, prior=()):
    worker = builds['worker']
    recipe = {'schema': 1, 'profile': PROFILE, 'kind': kind, 'campaign_id': campaign, 'order_seed': 20261018,
              'input': dict(INPUT), 'policy': POLICY, 'hypothesis': 'Measure the bounded production observation cost under an identical retained-reader workload; no code optimization is claimed.',
              'worker': {'path': str(Path(worker['path']).resolve()), 'sha256': evidence.file_hash(worker['path']),
                         'receipt': str(Path(worker['receipt']).resolve()), 'receipt_sha256': evidence.file_hash(worker['receipt']),
                         'config': str(Path(worker['config']).resolve()), 'config_sha256': evidence.file_hash(worker['config']),
                         'source_root': str(Path(worker['source_root']).resolve()), 'source_sha256': evidence.source_hash(Path(worker['source_root']).resolve())},
              'controller': {'path': str(Path(builds['controller']).resolve()), 'sha256': evidence.file_hash(builds['controller'])},
              'pairs': [{'order': order, 'baseline_run': run_id(campaign, index, 0), 'candidate_run': run_id(campaign, index, 1)}
                        for index, order in enumerate(orders(20261018))],
              'original_recipe_sha256': original, 'prior_runs': list(prior)}
    check_recipe(recipe)
    check_resources(recipe)
    return recipe


def run_campaign(recipe_path, recipe_sha256, output):
    recipe_path, output = Path(recipe_path).resolve(), Path(output).resolve()
    if evidence.file_hash(recipe_path) != recipe_sha256:
        oracle.invalid('predeclared performance recipe digest')
    recipe = evidence.document(recipe_path)
    check_recipe(recipe)
    check_resources(recipe)
    output.mkdir(parents=True, exist_ok=False)
    shutil.copy2(recipe_path, output / 'recipe.json')
    resources = retain_resources(recipe, output)
    build = check_resources(recipe, resources)
    before = host_observation()
    evidence.save(output / 'host-before.json', before)
    preflight = None
    taskset = shutil.which('taskset')
    if 2 not in before['affinity'] or taskset is None:
        preflight = 'CPU2 affinity or taskset is unavailable; no CPU fallback'
    index = {'schema': 1, 'profile': PROFILE, 'recipe_sha256': recipe_sha256, 'pairs': [],
             'checker': {'path': str(Path(__file__).resolve()), 'sha256': evidence.file_hash(__file__)},
             'oracle_sha256': evidence.file_hash(oracle.__file__), 'bridge_sha256': evidence.file_hash(evidence.__file__),
             'preflight_error': preflight}
    for pair_index, pair in enumerate(recipe['pairs']):
        retained = {'order': pair['order']}
        for side in (('baseline', 'candidate') if pair['order'] == 'AB' else ('candidate', 'baseline')):
            identifier = pair[side + '_run']
            slot = output / ('pair-' + str(pair_index).zfill(2) + '-' + side)
            slot.mkdir()
            input_value = {**INPUT, 'observations': side == 'candidate'}
            evidence.save(slot / 'input.json', input_value)
            record = {'run': identifier, 'slot': slot.name, 'execution': None, 'result': {'verdict': 'INCOMPLETE', 'detail': preflight}}
            if preflight is None:
                command = [taskset, '-c', '2', str(resources['controller']), 'run-durable-performance',
                           '--input', str(slot / 'input.json'), '--input-sha256', evidence.file_hash(slot / 'input.json'),
                           '--worker', str(resources['worker']), '--worker-sha256', recipe['worker']['sha256'],
                           '--artifacts', str(slot / 'native'), '--run-id', identifier]
                try:
                    record['execution'] = invoke(command, slot / 'controller.stdout', slot / 'controller.stderr')
                    if record['execution']['exit_code'] != 0 or record['execution']['error'] is not None:
                        oracle.incomplete('native controller failed or timed out; all raw output retained')
                    directories = list((slot / 'native').glob('native-*'))
                    if len(directories) != 1:
                        oracle.invalid('missing/extra native run directory')
                    record['native_directory'] = directories[0].name
                    record['result'] = inspect_run(directories[0], identifier, input_value, recipe, build)
                except (oracle.OracleError, OSError, KeyError, TypeError, ValueError) as error:
                    record['result'] = failure(error)
            evidence.save(slot / 'receipt.json', record)
            retained[side] = record
        index['pairs'].append(retained)
    evidence.save(output / 'host-after.json', host_observation())
    try:
        check_resources(recipe, resources)
    except (oracle.OracleError, OSError) as error:
        index['postflight_error'] = str(error)
    evidence.save(output / 'index.json', index)
    return verify_bundle(output, save_result=True)


def verify_bundle(bundle, save_result=False):
    bundle = Path(bundle).resolve()
    index, recipe = evidence.document(bundle / 'index.json'), evidence.document(bundle / 'recipe.json')
    check_recipe(recipe)
    if index.get('schema') != 1 or index.get('profile') != PROFILE or index.get('recipe_sha256') != evidence.file_hash(bundle / 'recipe.json') or len(index.get('pairs', [])) != 10:
        oracle.invalid('retained bundle/recipe identity or pair inventory')
    build = check_resources(recipe, retained_resources(bundle))
    if index['checker']['sha256'] != evidence.file_hash(__file__) or index['oracle_sha256'] != evidence.file_hash(oracle.__file__) or index['bridge_sha256'] != evidence.file_hash(evidence.__file__):
        oracle.invalid('checker source differs from registered bundle')
    checked = []
    for pair_index, (expected, retained) in enumerate(zip(recipe['pairs'], index['pairs'])):
        if retained['order'] != expected['order']:
            oracle.invalid('retained pair order changed')
        pair = {'order': expected['order']}
        for side in ('baseline', 'candidate'):
            record = retained[side]
            slot = bundle / ('pair-' + str(pair_index).zfill(2) + '-' + side)
            if record['slot'] != slot.name or record['run'] != expected[side + '_run'] or evidence.document(slot / 'receipt.json') != record:
                oracle.invalid('retained slot/run/receipt identity')
            try:
                if index.get('preflight_error') or index.get('postflight_error') or record['execution'] is None or record['execution']['exit_code'] != 0 or record['execution']['error'] is not None:
                    oracle.incomplete('retained execution/preflight/postflight failed')
                for filename, key in (('controller.stdout', 'stdout'), ('controller.stderr', 'stderr')):
                    if evidence.file_hash(slot / filename) != record['execution'][key]['sha256']:
                        oracle.invalid('outer raw execution digest changed')
                directories = list((slot / 'native').glob('native-*'))
                if len(directories) != 1 or directories[0].name != record['native_directory']:
                    oracle.invalid('retained native directory inventory changed')
                pair[side] = inspect_run(directories[0], record['run'], {**INPUT, 'observations': side == 'candidate'}, recipe, build)
            except (oracle.OracleError, OSError, KeyError, TypeError, ValueError) as error:
                pair[side] = failure(error)
        checked.append(pair)
    result = {'schema': 1, 'profile': PROFILE, 'recipe_sha256': index['recipe_sha256'], 'comparison': analyze(checked),
              'pairs': checked, 'scope': 'same-release-binary observation cost; native OS sync/bytes, not power-loss qualification',
              'source_sha256': recipe['worker']['source_sha256'], 'worker_sha256': recipe['worker']['sha256'],
              'not_qualified': ['optimization_adoption', 'power_loss_hardware', 'concurrent_load_saturation', 'maintenance_percentiles']}
    if save_result:
        evidence.save(bundle / 'comparison.json', result)
    return result


def replay(bundle, output, campaign):
    bundle = Path(bundle).resolve()
    verified = verify_bundle(bundle)
    original = evidence.document(bundle / 'recipe.json')
    paths = retained_resources(bundle)
    prior = original['prior_runs'] + [pair[side] for pair in original['pairs'] for side in ('baseline_run', 'candidate_run')]
    builds = {'worker': {'path': paths['worker'], 'receipt': paths['receipt'], 'config': paths['config'], 'source_root': paths['source']}, 'controller': paths['controller']}
    prepared = prepare(builds, campaign, 'replay_audit', verified['recipe_sha256'], prior)
    preparation = Path(str(output) + '.recipe.json').resolve()
    evidence.save(preparation, prepared)
    return run_campaign(preparation, evidence.file_hash(preparation), output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest='command', required=True)
    subcommands.add_parser('profile')
    preparer = subcommands.add_parser('prepare')
    preparer.add_argument('--builds', required=True)
    preparer.add_argument('--campaign-id', required=True)
    preparer.add_argument('--output', required=True)
    runner = subcommands.add_parser('run')
    runner.add_argument('--recipe', required=True)
    runner.add_argument('--recipe-sha256', required=True)
    runner.add_argument('--output', required=True)
    verifier = subcommands.add_parser('verify')
    verifier.add_argument('--bundle', required=True)
    replayer = subcommands.add_parser('replay')
    replayer.add_argument('--bundle', required=True)
    replayer.add_argument('--campaign-id', required=True)
    replayer.add_argument('--output', required=True)
    args = parser.parse_args()
    try:
        if args.command == 'profile':
            result = {'schema': 1, 'profile': PROFILE, 'path': PATH, 'input': INPUT, 'policy': POLICY,
                      'prepare_builds_schema': {'worker': {'path': 'absolute executable', 'receipt': 'external BuildReceipt JSON', 'config': 'bound build config JSON', 'source_root': 'complete retained source tree'}, 'controller': 'absolute release controller'}}
        elif args.command == 'prepare':
            result = prepare(evidence.document(args.builds), args.campaign_id)
            evidence.save(args.output, result)
            result = {'recipe': str(Path(args.output).resolve()), 'sha256': evidence.file_hash(args.output)}
        elif args.command == 'run':
            result = run_campaign(args.recipe, args.recipe_sha256, args.output)
        elif args.command == 'verify':
            result = verify_bundle(args.bundle)
        else:
            result = replay(args.bundle, args.output, args.campaign_id)
        print(json.dumps(result, indent=2, sort_keys=True))
        comparison = result.get('comparison')
        return 0 if comparison is None or comparison['budget'] == 'PASS' else 1 if comparison['budget'] == 'FAIL' else 3
    except (oracle.OracleError, OSError, KeyError, TypeError, ValueError) as error:
        print(json.dumps(failure(error), indent=2, sort_keys=True))
        return 2


if __name__ == '__main__':
    sys.exit(main())
