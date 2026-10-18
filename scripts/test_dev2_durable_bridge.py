#!/usr/bin/env python3
"""Literal control-protocol counterexamples, independent of production helpers."""
import copy
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import dev2_durable_bridge as bridge
import dev2_durable_oracle as oracle

RUN = [7] * 16
TOKEN = [8] * 16


def case(target='wal_commit', boundary='sync_after', name='return_error_matrix'):
    return {'schema_version': 1, 'scope': 'NATIVE_OS_FAULT_NOT_POWER_LOSS', 'case': name,
            'initial_position': 2, 'initial_values': [0],
            'plan': {'actor': 0, 'directory': 0, 'occurrence': 1,
                     'cut': {'target': target, 'boundary': boundary}},
            'expected_reopen_values': [7 if (target == 'wal_commit' and boundary in ('write_after', 'sync_before', 'sync_after')) or name != 'return_error_matrix' else 0]}


def event(kind, sequence, request, value=None):
    return {'kind': kind, 'sequence': sequence, 'nanos': sequence, 'request': request, 'value': value}


def invocation(operation, **fields):
    return {'actor': 0, 'command': {'action': {'operation': operation, **fields}}}


def response(result):
    return {'observation': {'result': result}}


def fixture(kill=False):
    value = case(name='synced_unacknowledged_then_refault' if kill else 'return_error_matrix')
    plan = value['plan']
    witness = {'actor': 0, 'request': 2, 'directory': 0, 'plan': plan, 'matched': 1, 'observed_target': 'wal_commit',
               'phase': 'commit', 'file': 'wal-00000001-0000000000000001.axf',
               'offset': None, 'requested_bytes': None, 'completed_bytes': None,
               'frame_start': 400, 'frame_end': 575, 'frame_written_bytes': 175}
    common = {'version': 1, 'run': RUN, 'token': TOKEN}
    lines = [{'kind': 'plan', **common, 'plan': plan},
             {'kind': 'observe', 'value': {**common, 'event': {'event': 'connected', 'process_id': 123}}},
             {'kind': 'observe', 'value': {**common, 'event': {'event': 'reached', 'witness': witness}}}]
    if kill:
        lines.append({'kind': 'kill_intent', 'value': witness})
    else:
        lines.extend([{'kind': 'release_intent', 'value': {**common, 'decision': 'return_error'}},
                      {'kind': 'observe', 'value': {**common, 'event': {'event': 'released', 'decision': 'return_error'}}}])
    for index, line in enumerate(lines):
        line.update(sequence=index, elapsed_ns=index)
    journal = [event(1, 0, 1, invocation('configure_fault', token=TOKEN, plan=plan)),
               event(3, 1, 1, response('done')), event(1, 2, 2, invocation('commit'))]
    if kill:
        journal.extend([event(4, 3, 0), event(6, 4, 0)])
    return lines, value, journal, {'run': RUN}


def chain_fixture():
    runs = [Path('/campaign/first'), Path('/campaign/second'), Path('/campaign/third')]
    manifests = [{'run': [index] * 16} for index in (1, 2, 3)]
    controls = [{'decision': 'owned_process_kill', 'witness': {'request': 2,
                 'plan': {'cut': {'target': target, 'boundary': 'sync_after'}}}}
                for target in ('wal_commit', 'recovery_old_file')]
    journals = [[], [event(1, 0, 2, invocation('open_database')), event(4, 1, 0), event(6, 2, 0)],
                [event(1, 0, 1, invocation('open_database')), event(3, 1, 1, response('ready'))]]
    return runs, manifests, journals, controls


class BridgeTests(unittest.TestCase):
    def invalid(self, function, *args):
        with self.assertRaises(oracle.OracleError) as caught:
            function(*args)
        self.assertEqual(caught.exception.verdict, 'INVALID')

    def test_control_release_literal(self):
        self.assertFalse(bridge.verify_control(*fixture())[-1])

    def test_control_qualified_kill_literal(self):
        self.assertTrue(bridge.verify_control(*fixture(kill=True))[-1])

    def test_released_before_reached_is_invalid(self):
        lines, value, journal, manifest = fixture()
        lines[2]['value']['event'] = {'event': 'released', 'decision': 'return_error'}
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_duplicate_connected_is_invalid(self):
        lines, value, journal, manifest = fixture()
        lines[2]['value']['event'] = copy.deepcopy(lines[1]['value']['event'])
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_missing_release_ack_is_invalid(self):
        lines, value, journal, manifest = fixture()
        self.invalid(bridge.verify_control, lines[:-1], value, journal, manifest)

    def test_mismatched_configure_token_is_invalid(self):
        lines, value, journal, manifest = copy.deepcopy(fixture())
        journal[0]['value']['command']['action']['token'] = [9] * 16
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_configure_return_after_cut_invoke_is_invalid(self):
        lines, value, journal, manifest = fixture()
        journal[1]['sequence'] = 3
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_response_then_qualified_kill_is_invalid(self):
        lines, value, journal, manifest = fixture(kill=True)
        journal.insert(3, event(3, 3, 2, response('committed')))
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_wal_sync_missing_frame_extent_is_invalid(self):
        lines, value, journal, manifest = fixture()
        del lines[2]['value']['event']['witness']['frame_end']
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_sync_after_without_complete_frame_is_invalid(self):
        lines, value, journal, manifest = fixture()
        lines[2]['value']['event']['witness']['frame_written_bytes'] = 1
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_commit_cut_in_wrong_phase_is_invalid(self):
        lines, value, journal, manifest = fixture()
        lines[2]['value']['event']['witness']['phase'] = 'open'
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def test_partial_cut_requires_exact_one_byte(self):
        value = case('wal_commit', 'write_partial', 'partial_continue')
        witness = copy.deepcopy(fixture()[0][2]['value']['event']['witness'])
        witness.update(offset=400, requested_bytes=175, completed_bytes=1, frame_written_bytes=1)
        bridge.verify_witness(witness, value)
        witness['completed_bytes'] = 2
        witness['frame_written_bytes'] = 2
        self.invalid(bridge.verify_witness, witness, value)

    def test_stabilize_sync_current_selector_literal(self):
        value = case('recovery_old_file', 'sync_after')
        witness = {'phase': 'open', 'file': 'CURRENT', 'offset': None,
                   'requested_bytes': None, 'completed_bytes': None,
                   'frame_start': None, 'frame_end': None, 'frame_written_bytes': None}
        bridge.verify_witness(witness, value)
        witness['file'] = 'LOCK'
        self.invalid(bridge.verify_witness, witness, value)

    def test_reopened_scan_cannot_be_reused_from_an_earlier_run(self):
        table = {'database': RUN, 'timeline': 1, 'object': 0x100000001, 'generation': 1, 'schema_epoch': 1}
        row = {'table': table, 'row': 0x100000001}
        entries = [
            (invocation('create_database'), {'result': 'ready', 'database': RUN, 'timeline': 1}),
            (invocation('create_table', spec=bridge.SPEC), {'result': 'table', 'identity': table, 'spec': bridge.SPEC}),
            (invocation('statement'), {'result': 'statement', 'inserted': [{'identity': row}]}),
            (invocation('open_database'), {'result': 'ready', 'database': RUN, 'timeline': 1}),
            (invocation('begin', transaction=3, mode='read_only'), {'result': 'begun'}),
            (invocation('lookup', transaction=3), {'result': 'table', 'spec': bridge.SPEC, 'identity': table}),
            (invocation('open_scan', transaction=3, cursor=1), {'result': 'done'}),
            (invocation('next_page', cursor=1), {'result': 'page', 'rows': [{'identity': row, 'values': [{'type': 'int64', 'value': 7}]}], 'eof': True}),
            (invocation('commit', transaction=3), {'result': 'committed', 'wrote': False, 'position': 3}),
        ]
        journal = []
        for index, (request, observation) in enumerate(entries):
            journal += [event(1, index * 2, index, request), event(3, index * 2 + 1, index, {'observation': observation})]
        self.assertEqual(bridge.extract_observation([journal])[-1]['q'], 3)
        with self.assertRaises(oracle.OracleError) as caught:
            bridge.extract_observation([journal, []])
        self.assertEqual(caught.exception.verdict, 'INCOMPLETE')

    def test_any_unknown_boundary_is_invalid(self):
        self.invalid(bridge.fixed_case, case('any', 'sync_after'))

    def test_unknown_target_is_invalid(self):
        self.invalid(bridge.fixed_case, case('future_target', 'create_before'))

    def test_closed_campaign_has_33_distinct_cases(self):
        cases = [case(target, boundary) for target, boundary in bridge.RETURN_ERROR_CUTS]
        cases += [case('wal_commit', 'write_partial', 'partial_continue'),
                  case(name='synced_unacknowledged_then_refault')]
        inventory = bridge.campaign_inventory(cases)
        self.assertEqual(inventory['expected_cases'], 33)
        self.assertEqual(inventory['verdict'], 'COMPLETE')
        missing = bridge.campaign_inventory(cases[:-1])
        self.assertEqual(missing['verdict'], 'INCOMPLETE')
        self.assertEqual(len(missing['missing']), 1)
        duplicate = bridge.campaign_inventory(cases + [cases[0]])
        self.assertEqual(duplicate['verdict'], 'INCOMPLETE')
        self.assertEqual(len(duplicate['duplicate']), 1)

    def test_refault_literal(self):
        bridge.verify_refault(*chain_fixture())

    def test_refault_reuses_run_identity(self):
        runs, manifests, journals, controls = chain_fixture()
        manifests[1] = manifests[0]
        self.invalid(bridge.verify_refault, runs, manifests, journals, controls)

    def test_refault_requires_two_qualified_controls(self):
        runs, manifests, journals, controls = chain_fixture()
        controls[1]['decision'] = 'return_error'
        self.invalid(bridge.verify_refault, runs, manifests, journals, controls)

    def test_refault_second_open_cannot_already_return(self):
        runs, manifests, journals, controls = chain_fixture()
        journals[1].insert(1, event(3, 1, 2, response('ready')))
        self.invalid(bridge.verify_refault, runs, manifests, journals, controls)

    def test_refault_third_must_open(self):
        runs, manifests, journals, controls = chain_fixture()
        journals[2][1]['value'] = response('error')
        self.invalid(bridge.verify_refault, runs, manifests, journals, controls)

    def test_post_reopen_never_strict_accepted(self):
        self.assertEqual(bridge.strict_verdict('ACCEPTED', 'post_reopen', True), 'INCOMPLETE')
        self.assertEqual(bridge.strict_verdict('REJECTED', 'post_reopen', True), 'REJECTED')

    def test_control_frame_4k_includes_prefix(self):
        maximum = json.dumps('x' * 4090).encode()
        self.assertEqual(len(maximum), 4092)
        self.assertEqual(len(bridge.frames(struct.pack('>I', len(maximum)) + maximum)), 1)
        self.invalid(bridge.frames, struct.pack('>I', len(maximum) + 1) + maximum + b' ')

    def test_control_raw_128k_bound(self):
        with self.assertRaises(oracle.OracleError) as caught:
            bridge.frames(b'x' * (bridge.CONTROL_BYTES + 1))
        self.assertEqual(caught.exception.verdict, 'INCOMPLETE')

    def test_image_event_only_after_completed_control(self):
        lines, value, journal, manifest = fixture(kill=True)
        lines.append({'kind': 'image', 'value': {}, 'sequence': 4, 'elapsed_ns': 4})
        self.assertTrue(bridge.verify_control(lines, value, journal, manifest)[-1])
        lines[2]['kind'] = 'image'
        self.invalid(bridge.verify_control, lines, value, journal, manifest)

    def image_fixture(self, run):
        value = case(name='synced_unacknowledged_then_refault')
        bridge.save(run / 'fault-case.json', value)
        (run / 'crash-image').mkdir()
        (run / 'crash-image' / 'LOCK').write_bytes(b'')
        empty_hash = oracle.sha(b'')
        inventory = oracle.sha(struct.pack('<Q', 4) + b'LOCK' + struct.pack('<Q', 0) + bytes.fromhex(empty_hash))
        witness = {'request': 2}
        boundary = 'blocked_reached_owned_kill'
        receipt = {'schema_version': 1, 'scope': 'NATIVE_PROCESS_LOSS_IMAGE_NOT_POWER_LOSS', 'boundary': boundary,
                   'run': RUN, 'worker_id': 123, 'case_sha256': bridge.file_hash(run / 'fault-case.json'),
                   'source_work_directory': str(run / 'work'), 'directory_slot': 0, 'image': 'crash-image', 'witness': witness,
                   'inventory_hash_format': 'name_len_le64+name_ascii+size_le64+sha256_bytes_sorted_names_v1',
                   'inventory_sha256': inventory, 'files': [{'name': 'LOCK', 'bytes': 0, 'sha256': empty_hash}], 'total_bytes': 0}
        bridge.save(run / 'crash-image.json', receipt)
        digest = bridge.file_hash(run / 'crash-image.json')
        control = {'decision': 'owned_process_kill', 'connected_process': 123, 'witness': witness, 'pre_effect_case_binding': True,
                   'image_event': {'boundary': boundary, 'receipt': 'crash-image.json', 'receipt_sha256': digest,
                                   'inventory_sha256': inventory, 'files': 1, 'bytes': 0}}
        link = {'directory': str(run / 'crash-image'), 'receipt': str(run / 'crash-image.json'), 'receipt_sha256': digest,
                'inventory_sha256': inventory, 'files': 1, 'bytes': 0}
        return value, [event(4, 0, 0), event(6, 1, 0)], {'run': RUN, 'work_directory': str(run / 'work')}, control, link

    def test_pre_open_inventory_literal(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            result = bridge.captured_image(run, *self.image_fixture(run))
            self.assertEqual(result['files'], 1)

    def test_changed_captured_bytes_are_invalid(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            args = self.image_fixture(run)
            (run / 'crash-image' / 'LOCK').write_bytes(b'changed')
            self.invalid(bridge.captured_image, run, *args)

    def test_pre_open_requires_pre_effect_case_hash(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            value, journal, manifest, control, link = self.image_fixture(run)
            control['pre_effect_case_binding'] = False
            self.invalid(bridge.captured_image, run, value, journal, manifest, control, link)

    def test_pre_open_link_cannot_select_other_image(self):
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            value, journal, manifest, control, link = self.image_fixture(run)
            link['directory'] = str(run / 'other-image')
            self.invalid(bridge.captured_image, run, value, journal, manifest, control, link)

    def formal_fixture(self):
        paths = {role: {'path': '/' + role, 'sha256': role + '-digest'} for role in ('harness', 'worker', 'oracle', 'bridge')}
        campaigns = [{'repetition': index, 'artifacts': '/campaign' + str(index),
                      'command': ['/harness', '--test-threads=1', '--nocapture'],
                      'environment': {'XADBTEST_DURABLE_WORKER_BIN': '/worker', 'XADBTEST_NATIVE_ARTIFACTS': '/campaign' + str(index)}}
                     for index in range(3)]
        manifest = {'schema': 1, 'profile': 'dev2-native-fault-v1', 'planned_cases_per_campaign': 33,
                    'tests': list(bridge.FORMAL_TESTS), 'created_at': '2026-10-18T00:00:00Z',
                    'source_root': '/source', 'source_sha256': 'source-digest', 'campaigns': campaigns, **paths}
        execution = {'manifest_sha256': 'manifest-digest', 'exit_code': 0, 'command': campaigns[0]['command'],
                     'environment': campaigns[0]['environment'], 'started_at': '2026-10-18T00:00:01Z',
                     'finished_at': '2026-10-18T00:00:02Z', 'log': {'path': '/log', 'sha256': 'log-digest'}}
        for role in ('source', 'worker', 'harness'):
            for when in ('before', 'after'):
                execution[role + '_sha256_' + when] = role + '-digest'
        hashes = {'/' + role: role + '-digest' for role in ('manifest', 'log', 'harness', 'worker', 'oracle', 'bridge')}
        hashes[oracle.__file__], hashes[bridge.__file__] = 'oracle-digest', 'bridge-digest'
        return manifest, execution, hashes

    def verify_formal_fixture(self, manifest, execution, hashes):
        with mock.patch.object(bridge, 'document', side_effect=[manifest, execution]), \
             mock.patch.object(bridge, 'file_hash', side_effect=lambda path: hashes[str(path)]), \
             mock.patch.object(bridge, 'source_hash', return_value='source-digest'), \
             mock.patch.object(bridge, 'binding', side_effect=lambda path: {'path': str(path)}):
            return bridge.formal_binding('/manifest', '/execution', Path('/campaign0'))

    def test_formal_predeclared_identity_literal(self):
        self.assertEqual(self.verify_formal_fixture(*self.formal_fixture())['repetition'], 0)

    def test_formal_changed_harness_is_invalid(self):
        manifest, execution, hashes = self.formal_fixture()
        execution['harness_sha256_after'] = 'different'
        self.invalid(self.verify_formal_fixture, manifest, execution, hashes)

    def test_formal_changed_environment_is_invalid(self):
        manifest, execution, hashes = copy.deepcopy(self.formal_fixture())
        execution['environment'] = {'XADBTEST_NATIVE_ARTIFACTS': '/different'}
        self.invalid(self.verify_formal_fixture, manifest, execution, hashes)

    def test_postrun_manifest_is_invalid(self):
        manifest, execution, hashes = self.formal_fixture()
        manifest['created_at'] = '2026-10-18T00:00:03Z'
        self.invalid(self.verify_formal_fixture, manifest, execution, hashes)


if __name__ == '__main__':
    unittest.main()
