#!/usr/bin/env python3
"""Literal policies and known-bad evidence for the independent durable audit."""
import copy
from pathlib import Path
import sys
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import dev2_durable_performance as perf
import dev2_durable_oracle as oracle

DB = [4] * 16
IDENTITY = {'database': DB, 'timeline': 1}
TABLE_ID = {**IDENTITY, 'object': 0x100000001, 'generation': 1, 'schema_epoch': 1}
HANDLE = (0x100000001, 1, 1)
SPEC = {'handle': HANDLE, 'name': 'items', 'columns': [{'name': 'value', 'data_type': 'int64', 'nullable': False}]}
BUILD = {'source_sha256': 'a' * 64, 'toolchain': 'rustc-test', 'target': 'x86_64-unknown-linux-gnu', 'profile': 'release', 'self_test': False}


def transaction(local):
    return {**IDENTITY, 'writer_epoch': 1, 'local': local}


def receipt(local, position, wrote=True):
    return {'transaction': transaction(local), 'position': position, 'wrote': wrote}


def effects(sync=0, written=0):
    result = {'saturated': False}
    for name in perf.EFFECTS:
        count = sync if name == 'file_sync' else written // 175 if name == 'write' else 0
        result[name] = {'attempts': count, 'successes': count, 'failures': 0, 'unknown_partial_calls': 0,
                        'requested_bytes': written if name == 'write' else 0 if name == 'read' else None,
                        'confirmed_bytes': written if name == 'write' else 0 if name == 'read' else None}
    return result


def metrics(q, effect, timeline=1, observed=False):
    operation = lambda name: {'metric_id': name, 'attempts': 0, 'successes': 0, 'failures': 0,
        'requested_bytes': None, 'confirmed_complete_bytes': None, 'items': None,
        'unknown_partial_operations': 0, 'timing_missing': 0,
        'elapsed': {'samples': 0, 'sum_ns': 0, 'max_ns': 0, 'log2_ns': [0] * 64}}
    return {'identity': {'database': DB, 'timeline': timeline},
            'core': {'resident_bytes': 8 << 20, 'private_bytes': 0, 'counters_saturated': False, 'coordinator_clock_invalid': False},
            'durability': {'semantic_revision': 1, 'clock_configured': True, 'clock_invalid': False, 'counters_saturated': False,
                'operations': [operation(name) for name in perf.METRICS] if observed else None,
                'gauges': {'control_reserved_bytes': 8 << 20, 'control_used_bytes': 4096, 'retained_wal_bytes': 1048576,
                           'durable_position': q, 'checkpoint_position': 0}}, 'effects': copy.deepcopy(effect)}


def literal_output():
    numbers = [0x100000001 + index for index in range(128)]
    def row(index, value):
        return {'table': HANDLE, 'row': numbers[index], 'values': [{'type': 'int64', 'value': value}]}
    batches = {oracle.transaction_key(transaction(1)): {'q': 1, 'changes': [{'change': 'create', 'table': SPEC}]},
               oracle.transaction_key(transaction(2)): {'q': 2, 'changes': [{'change': 'put', 'row': row(i, i)} for i in range(128)]}}
    output = {'version': 1, 'path': perf.PATH, 'input': dict(perf.INPUT), 'build': BUILD,
              'actual_affinity': '2', 'error': None, 'directories': {'source': 'source', 'backup': 'backup', 'restored': 'restored'},
              'setup': {'elapsed_ns': 1, 'identity': IDENTITY, 'table': TABLE_ID, 'ddl': receipt(1, 1),
                        'insert_begin': transaction(2), 'insert_commit': receipt(2, 2),
                        'rows': [{'table': TABLE_ID, 'row': number} for number in numbers]},
              'old_values': list(range(128)), 'old_commit': receipt(3, 2, False), 'maintenance': [],
              'cpu_ns': 50000, 'process_peak_rss_bytes': 16 << 20}
    state, values = perf.INPUT['seed'], list(range(128))
    for name, offset, count in (('warmup', 0, 64), ('measurement', 64, 512)):
        batch = {'completed': count, 'elapsed_ns': count * 100, 'iterations': []}
        for index in range(offset, offset + count):
            state = (state * 6364136223846793005 + 1442695040888963407) % (2 ** 64)
            value = state - 2 ** 64 if state >= 2 ** 63 else state
            values[index % 128] = value
            local = index * 2 + 4
            batch['iterations'].append({'ordinal': index, 'begin': transaction(local), 'commit': receipt(local, index + 3),
                'read_value': value, 'read_commit': receipt(local + 1, index + 3, False), 'completed': True,
                'elapsed_ns': 90, 'commit_ns': 50, 'readback_ns': 20})
            batches[oracle.transaction_key(transaction(local))] = {'q': index + 3, 'changes': [{'change': 'put', 'row': row(index % 128, value)}]}
        output[name] = batch
    output['metrics_before'] = metrics(66, effects(64, 64 * 175))
    output['metrics_after'] = metrics(578, effects(576, 576 * 175))
    cumulative = effects(576, 576 * 175)
    for stage in perf.STAGES:
        before = copy.deepcopy(cumulative)
        if stage in ('checkpoint', 'backup', 'restore', 'reopen'):
            cumulative['file_sync']['attempts'] += 1
            cumulative['file_sync']['successes'] += 1
        output['maintenance'].append({'stage': stage, 'elapsed_ns': 100, 'completed': True,
            'position': None if stage == 'garbage_collect' else 578, 'generation': 2 if stage in ('checkpoint', 'backup') else None,
            'files': 0, 'bytes': 0, 'identity': {'database': DB, 'timeline': 2 if stage == 'restore' else 1},
            'values': list(values) if stage in ('scan', 'restore', 'reopen') else [],
            'metrics_before': metrics(578, before), 'metrics_after': metrics(578, cumulative, 2 if stage == 'restore' else 1),
            'effects_before': before, 'effects_after': copy.deepcopy(cumulative)})
    expected = oracle.normalize_state({'tables': [SPEC], 'rows': [row(i, value) for i, value in enumerate(values)]})
    source = {'identity': (bytes(DB).hex(), 1, 2), 'q': 578, 'state': expected, 'batches': batches, 'closure': {}}
    manifest = {'generation': 2, 'cut': 578, 'chunks': []}
    restored = {'identity': (bytes(DB).hex(), 2, 2), 'q': 578, 'state': expected, 'closure': {},
                'manifest': {'provenance': {'database': bytes(DB).hex(), 'timeline': 1, 'generation': 2, 'digest': 'd' * 64}}}
    return output, source, manifest, restored, expected


def validate_fixture(fixture):
    output, source, manifest, restored, expected = fixture
    image = mock.Mock()
    image.inventory = {'LOCK': (), 'CURRENT': (), 'manifest-0000000000000002.axf': ()}
    image.metadata.return_value = ({'kind': 6, 'previous': None, 'identity': (bytes(DB).hex(), 1, 1),
                                   'current': {'generation': 2, 'digest': 'd' * 64}}, b'')
    archive_state = mock.Mock()
    archive_state.snapshot.return_value = expected
    with mock.patch.object(oracle, 'inspect_image', side_effect=[source, restored]), \
         mock.patch.object(oracle, 'Image', return_value=image), \
         mock.patch.object(oracle, 'checkpoint', return_value=(manifest, archive_state)):
        return perf.check_output(output, perf.INPUT, BUILD, 100000, Path('/work'))


class OutputTests(unittest.TestCase):
    def rejected(self, fixture, verdict='REJECTED'):
        with self.assertRaises(oracle.OracleError) as caught:
            validate_fixture(fixture)
        self.assertEqual(caught.exception.verdict, verdict)

    def test_literal_578_ack_observations(self):
        self.assertEqual(validate_fixture(literal_output())['observed_commits'], 578)

    def test_missing_cycle_is_rejected(self):
        fixture = literal_output()
        fixture[0]['measurement']['iterations'].pop()
        self.rejected(fixture)

    def test_wrong_readback_is_rejected(self):
        fixture = literal_output()
        fixture[0]['measurement']['iterations'][20]['read_value'] = 0
        self.rejected(fixture)

    def test_ack_q_mismatch_is_rejected(self):
        fixture = literal_output()
        fixture[0]['measurement']['iterations'][0]['commit']['position'] += 1
        self.rejected(fixture)

    def test_commit_different_from_begin_is_rejected(self):
        fixture = literal_output()
        fixture[0]['measurement']['iterations'][0]['commit']['transaction']['local'] += 1
        self.rejected(fixture)

    def test_sync_omission_is_rejected(self):
        fixture = literal_output()
        for field in ('attempts', 'successes'):
            fixture[0]['metrics_after']['effects']['file_sync'][field] -= 1
        self.rejected(fixture)

    def test_lost_ack_same_final_rows_is_rejected(self):
        fixture = literal_output()
        fixture[1]['batches'].pop(oracle.transaction_key(transaction(4)))
        self.rejected(fixture)

    def test_wal_wrong_afterimage_is_rejected(self):
        fixture = literal_output()
        fixture[1]['batches'][oracle.transaction_key(transaction(4))]['changes'][0]['row']['values'][0]['value'] = 0
        self.rejected(fixture)

    def test_restore_provenance_mismatch_is_rejected(self):
        fixture = literal_output()
        fixture[3]['manifest']['provenance']['digest'] = 'e' * 64
        self.rejected(fixture)

    def test_worker_affinity_missing_is_incomplete(self):
        fixture = literal_output()
        fixture[0]['actual_affinity'] = None
        self.rejected(fixture, 'INCOMPLETE')

    def test_old_reader_changes_are_rejected(self):
        fixture = literal_output()
        fixture[0]['old_values'][0] = 1
        self.rejected(fixture)

    def test_overlapping_time_not_added_to_response(self):
        fixture = literal_output()
        item = fixture[0]['measurement']['iterations'][0]
        item['commit_ns'], item['readback_ns'] = 80, 80
        # These subinterval observations do not define an exclusive decomposition.
        self.assertEqual(validate_fixture(fixture)['verdict'], 'ACCEPTED')

    def test_impossible_cycle_containment_is_invalid(self):
        fixture = literal_output()
        fixture[0]['measurement']['iterations'][0]['elapsed_ns'] = 100000
        self.rejected(fixture, 'INVALID')


class MetricTests(unittest.TestCase):
    def test_enabled_empty_catalog_is_measured_zero(self):
        perf.metric_snapshot(metrics(0, effects(), observed=True), True)

    def test_off_is_none_not_zero_catalog(self):
        with self.assertRaises(oracle.OracleError):
            perf.metric_snapshot(metrics(0, effects(), observed=True), False)

    def test_duplicate_label_rejected(self):
        value = metrics(0, effects(), observed=True)
        value['durability']['operations'][1]['metric_id'] = perf.METRICS[0]
        with self.assertRaises(oracle.OracleError):
            perf.metric_snapshot(value, True)

    def test_missing_clock_sample_not_zero_filled(self):
        value = metrics(0, effects(), observed=True)
        value['durability']['operations'][0]['elapsed'] = None
        with self.assertRaises(oracle.OracleError) as caught:
            perf.metric_snapshot(value, True)
        self.assertEqual(caught.exception.verdict, 'INCOMPLETE')

    def test_histogram_count_mismatch_is_invalid(self):
        value = metrics(0, effects(), observed=True)
        value['durability']['operations'][0]['elapsed']['log2_ns'][0] = 1
        with self.assertRaises(oracle.OracleError):
            perf.metric_snapshot(value, True)


def sample(duration=100, p99=100):
    return {'verdict': 'ACCEPTED', 'measurement_ns': duration, 'cpu_ns': duration, 'external_ns': duration,
            'cycle_p99_ns': p99, 'rss_bytes': 16 << 20}


def pairs(ratio=1):
    return [{'order': order, 'baseline': sample(), 'candidate': sample(int(100 * ratio))} for order in perf.orders(20261018)]


class AnalysisTests(unittest.TestCase):
    def test_known_equivalent(self):
        result = perf.analyze(pairs())
        self.assertEqual(result['performance_effect'], 'NO_MATERIAL_CHANGE')
        self.assertEqual(result['budget'], 'PASS')
        self.assertEqual(result['promotion'], 'INCOMPLETE')

    def test_known_improved_does_not_promote_code(self):
        result = perf.analyze(pairs(.9))
        self.assertEqual(result['performance_effect'], 'IMPROVED')
        self.assertEqual(result['promotion'], 'INCOMPLETE')

    def test_known_regression(self):
        result = perf.analyze(pairs(1.3))
        self.assertEqual(result['performance_effect'], 'REGRESSED')
        self.assertEqual(result['budget'], 'FAIL')

    def test_one_bad_p99_pair_is_not_averaged_away(self):
        values = pairs()
        values[3]['candidate']['cycle_p99_ns'] = 126
        self.assertEqual(perf.analyze(values)['guardrails'], 'FAIL')

    def test_exact_p99_boundary_passes(self):
        values = pairs()
        values[3]['candidate']['cycle_p99_ns'] = 125
        self.assertEqual(perf.analyze(values)['guardrails'], 'PASS')

    def test_unknown_cpu_stays_unknown(self):
        values = pairs()
        values[3]['candidate']['cpu_ns'] = None
        self.assertEqual(perf.analyze(values)['guardrails'], 'UNKNOWN')

    def test_order_drift_invalidates_effect(self):
        values = pairs()
        for value in values:
            value['candidate'] = sample(80 if value['order'] == 'AB' else 100)
        result = perf.analyze(values)
        self.assertEqual(result['validity'], 'INVALID')
        self.assertEqual(result['performance_effect'], 'INCONCLUSIVE')

    def test_failed_run_cannot_disappear(self):
        values = pairs()
        values[1]['baseline'] = {'verdict': 'INCOMPLETE'}
        self.assertEqual(perf.analyze(values)['validity'], 'INVALID')
        self.assertEqual(perf.analyze(values[:-1])['validity'], 'INVALID')


if __name__ == '__main__':
    unittest.main()
