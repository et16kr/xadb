#!/usr/bin/env python3
"""Independent literal grammar and known-bad ACK-oracle tests.

Fixtures below are built from explicit frozen field widths, not a product
encoder. Every positive state/receipt is separately literal. These tests alone
are oracle self-tests and do not qualify the production recovery path.
"""
import copy
import hashlib
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
from unittest import mock

sys.dont_write_bytecode = True
import dev2_durable_oracle as oracle

DB = [1] * 16
RUN = [9] * 16
HANDLE = [0x100000001, 1, 1]
ROW = 0x100000001
SPEC = {'name': 't', 'columns': [{'name': 'v', 'data_type': 'int64', 'nullable': False}]}
TABLE = {'handle': HANDLE, **SPEC}
VALUE = {'table': HANDLE, 'row': ROW, 'values': [{'type': 'int64', 'value': 7}]}
STATE = {'tables': [TABLE], 'rows': [VALUE]}
EMPTY = {'tables': [], 'rows': []}


def u16(n): return struct.pack('<H', n)
def u32(n): return struct.pack('<I', n)
def u64(n): return struct.pack('<Q', n)
def digest(data): return hashlib.sha256(data).digest()
def handle(value=HANDLE): return b''.join(u64(n) for n in value)
def name(value): return u16(len(value.encode())) + value.encode()


def common(kind, epoch=1):
    return b'XDK1' + u16(1) + u64(1) + bytes([kind]) + bytes(DB) + u64(1) + u32(epoch)


def frame(payload):
    prefix = b'AXF1' + u16(1) + u32(len(payload)) + payload
    return prefix + digest(prefix)


def schema(value=HANDLE):
    return handle(value) + name('t') + u16(1) + name('v') + bytes([2, 0])


def row(value=7, row_id=ROW, table=HANDLE):
    return handle(table) + u64(row_id) + u32(1) + bytes([2]) + struct.pack('<q', value)


def batch(local, q, changes, epoch=1):
    body = common(2, epoch) + u64(local) + u64(q - 1) + u64(q) + u16(len(changes)) + b''.join(changes)
    return frame(body + b'XDCM' + u64(q) + u16(len(changes)) + u32(len(body)))


def header(epoch=1, segment=1, start=0, origin=0, previous=None, resume=bytes(32)):
    return frame(common(1, epoch) + u64(segment) + u64(start) + bytes([origin, previous is not None]) + (previous or b'') + resume)


def link(epoch, segment, raw, complete, q):
    return u32(epoch) + u64(segment) + u64(complete) + u64(len(raw)) + u64(q) + digest(raw[:complete])


def anchor(head, offset=None, raw=None, q=0, epoch=1, segment=1):
    raw = head if raw is None else raw
    offset = len(head) if offset is None else offset
    return u32(epoch) + u64(segment) + digest(head) + u64(offset) + digest(raw[:offset]) + u64(q)


def manifest(generation, point, cut=0, tables=(), chunks=(), epoch=1):
    payload = common(7, epoch) + u64(generation) + u64(cut) + u32(epoch) + point + u16(len(tables)) + b''.join(tables) + u32(len(chunks))
    for index, raw, count in chunks:
        payload += u32(index) + u64(len(raw)) + digest(raw) + u32(count)
    return frame(payload + b'\0')


def current(generation, raw, previous=None, epoch=1):
    payload = common(6, epoch) + u64(generation) + digest(raw) + bytes([previous is not None])
    if previous is not None:
        payload += u64(previous[0]) + digest(previous[1])
    return frame(payload)


def writer(head, epoch=1, segment=1):
    return frame(common(5, epoch) + u64(segment) + digest(head))


def tx(local, epoch=1):
    return {'database': DB, 'timeline': 1, 'writer_epoch': epoch, 'local': local}


def table_identity():
    return {'database': DB, 'timeline': 1, 'object': HANDLE[0], 'generation': 1, 'schema_epoch': 1}


def receipt(local, q):
    return {'transaction': tx(local), 'position': q, 'wrote': True}


class Journal:
    def __init__(self):
        self.events, self.requests = [], 0

    def event(self, kind, request=0, payload=b''):
        seq = len(self.events)
        self.events.append(struct.pack('>BQQQI', kind, seq, seq * 100, request, len(payload)) + payload)

    @staticmethod
    def wire(value):
        raw = json.dumps(value, separators=(',', ':')).encode()
        return struct.pack('>I', len(raw)) + raw

    def invoke(self, operation, **fields):
        self.requests += 1
        request = self.requests
        self.event(1, request, self.wire({'version': 1, 'run': RUN, 'request': request, 'actor': 0,
                                        'command': {'gate': None, 'action': {'operation': operation, **fields}}}))
        self.event(2, request)
        return request

    def observe(self, request, **observation):
        self.event(3, request, self.wire({'version': 1, 'run': RUN, 'request': request, 'actor': 0,
                                        'passed_gate': None, 'observation': observation}))

    def call(self, operation, fields, observation):
        request = self.invoke(operation, **fields)
        self.observe(request, **observation)

    def close(self, killed=False):
        if killed:
            self.event(4)
            self.event(6)
        else:
            self.event(5)
        return b''.join(self.events)


def history(outcome='ACK', ddl_local=1, duplicate_ddl=False, configure_fault=False, empty_statement=False):
    journal = Journal()
    if configure_fault:
        journal.call('configure_fault', {'token': RUN, 'plan': {'cut': {'target': 'wal_commit', 'boundary': 'write_after'}, 'occurrence': 1, 'actor': 0, 'directory': 0}}, {'result': 'done'})
    create = {'result': 'table', 'identity': table_identity(), 'spec': SPEC, 'commit': receipt(ddl_local, 1)}
    journal.call('create_table', {'table': 1, 'spec': SPEC}, create)
    if duplicate_ddl:
        journal.call('create_table', {'table': 2, 'spec': SPEC}, create)
    journal.call('begin', {'transaction': 1, 'mode': 'read_write'}, {'result': 'begun', 'identity': tx(2)})
    if empty_statement:
        journal.call('statement', {'transaction': 1, 'mutations': []}, {'result': 'statement', 'inserted': [], 'affected_rows': 0})
    journal.call('statement', {'transaction': 1, 'mutations': [{'mutation': 'insert', 'table': 1, 'row': 1, 'values': [{'type': 'int64', 'value': 7}]}]},
                 {'result': 'statement', 'inserted': [{'alias': 1, 'identity': {'table': table_identity(), 'row': ROW}}], 'affected_rows': 1})
    request = journal.invoke('commit', transaction=1)
    if outcome == 'PENDING':
        return journal.close(killed=True)
    if outcome == 'ACK':
        journal.observe(request, result='committed', **receipt(2, 2))
    else:
        journal.observe(request, result='error', failure={'domain': 'storage', 'code': 'io',
                         'outcome': 'aborted' if outcome == 'ABORT' else 'unknown', 'fenced': True, 'transaction': tx(2)})
    return journal.close()


class Case:
    def __init__(self, root, include_put=True, local=2, ddl_local=1):
        self.root = Path(root)
        self.image = self.root / 'image'
        self.image.mkdir()
        self.head = header()
        self.create = batch(ddl_local, 1, [b'\1' + schema()])
        self.put = batch(local, 2, [b'\3' + row()])
        self.wal = self.head + self.create + (self.put if include_put else b'')
        self.base_manifest = manifest(1, anchor(self.head))
        self.write('LOCK', b'')
        self.write('CURRENT', current(1, self.base_manifest))
        self.write('WRITER', writer(self.head))
        self.write('manifest-0000000000000001.axf', self.base_manifest)
        self.write('wal-00000001-0000000000000001.axf', self.wal)
        self.journal = self.root / 'history.bin'
        self.journal.write_bytes(history(ddl_local=ddl_local))
        self.expect = {'version': 1, 'expected_outcome': 'RECOVER_OK', 'fault_model': 'process_kill',
                       'allow_incomplete_tail': True, 'database': DB, 'timeline': 1, 'history_complete': True}
        self.observed = {'status': 'READY', 'q': 2 if include_put else 1,
                         'state': STATE if include_put else {'tables': [TABLE], 'rows': []}}

    def write(self, name, raw):
        (self.image / name).write_bytes(raw)

    def check(self):
        return oracle.check(self.image, [self.journal], self.expect, self.observed)


class LiteralGrammar(unittest.TestCase):
    def test_literal_axf1_checksum_vector(self):
        # Published DEV1 fixed vector, not a production encoder invocation.
        literal = bytes.fromhex('41584631010003000000616263c7f1e99df28fe3fd54487543c592b6980d856f9a581e90bb7f7566d655714ce8')
        self.assertEqual(oracle.decode_envelope(literal), b'abc')
        self.assertEqual(frame(b'abc'), literal)
        for cut in range(len(literal)):
            with self.assertRaises(oracle.Corruption):
                oracle.decode_envelope(literal[:cut])

    def test_literal_writer_payload_fields(self):
        payload = bytes.fromhex(
            '58444b310100010000000000000005'
            '01010101010101010101010101010101'
            '010000000000000001000000'
            '0100000000000000'
            '0202020202020202020202020202020202020202020202020202020202020202')
        decoded = oracle.decode_payload(payload)
        self.assertEqual(decoded, {'kind': 5, 'identity': ('01' * 16, 1, 1), 'segment': 1, 'header': '02' * 32})

    def test_literal_null_bool_signed_text_binary_row(self):
        values = b'\0\1\1\2' + struct.pack('<q', -(1 << 63)) + b'\3' + u32(3) + b'a\0b' + b'\4' + u32(2) + b'\0\xff'
        payload = common(8) + u64(1) + u32(0) + u32(1) + handle() + u64(ROW) + u32(5) + values
        record = oracle.decode_payload(payload)
        self.assertEqual(record['rows'][0]['values'], [{'type': 'null'}, {'type': 'bool', 'value': True},
                         {'type': 'int64', 'value': -(1 << 63)}, {'type': 'utf8', 'value': 'a\0b'}, {'type': 'binary', 'value': '00ff'}])

    def test_rechecksummed_bad_trailer_is_rejected(self):
        raw = batch(2, 2, [b'\3' + row()])
        payload = bytearray(raw[10:-32])
        payload[-4:] = u32(1)
        with self.assertRaises(oracle.Corruption):
            oracle.decode_frame(frame(payload))

    def test_full_checksum_damage_never_becomes_partial(self):
        raw = bytearray(header())
        raw[-1] ^= 1
        with self.assertRaises(oracle.Corruption):
            oracle.decode_frame(raw)

    def test_noncanonical_bool_unknown_tag_and_trailing_bytes(self):
        samples = [common(9), common(5) + u64(1) + bytes(32) + b'\0',
                   common(1) + u64(1) + u64(0) + b'\0\2' + bytes(32)]
        for sample in samples:
            with self.subTest(sample=sample.hex()), self.assertRaises(oracle.Corruption):
                oracle.decode_frame(frame(sample))

    def test_duplicate_global_afterimage_is_rejected_across_tables(self):
        with self.assertRaises(oracle.Corruption):
            oracle.decode_frame(batch(2, 2, [b'\3' + row(), b'\4' + handle([0x100000002, 1, 1]) + u64(ROW)]))

    def test_unsupported_features_are_distinct(self):
        payload = bytearray(common(5) + u64(1) + bytes(32))
        payload[6:14] = u64(2)
        with self.assertRaises(oracle.OracleError) as caught:
            oracle.decode_frame(frame(payload))
        self.assertEqual(caught.exception.code, 'UNSUPPORTED_FORMAT')

    def test_new_object_and_row_must_use_current_batch_epoch(self):
        state = oracle.State()
        table = oracle.normalize_spec(SPEC, tuple(HANDLE))
        with self.assertRaises(oracle.Corruption):
            state.apply({'identity': ('01' * 16, 1, 2), 'changes': [{'change': 'create', 'table': table}]})
        state.create(table, 2)  # Checkpoint loading permits older live identities.
        value = oracle.normalize_row(VALUE)
        with self.assertRaises(oracle.Corruption):
            state.apply({'identity': ('01' * 16, 1, 2), 'changes': [{'change': 'put', 'row': value}]})
        state.put(value, 2, checkpoint=True)
        state.apply({'identity': ('01' * 16, 1, 2), 'changes': [{'change': 'put', 'row': value}]})


class MembershipAndClosure(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.case = Case(self.directory.name)

    def tearDown(self):
        self.directory.cleanup()

    def verdict(self, expected):
        result = self.case.check()
        self.assertEqual(result['verdict'], expected, result)
        return result

    def test_literal_success_checks_two_actual_transactions(self):
        result = self.verdict('ACCEPTED')
        self.assertEqual([(c['transaction']['local'], c['q']) for c in result['commits']], [(1, 1), (2, 2)])
        self.assertEqual(result['checkpoint_cut'], 0)

    def test_same_final_rows_do_not_hide_missing_ack_identity(self):
        self.case.write('wal-00000001-0000000000000001.axf', self.case.head + self.case.create + batch(99, 2, [b'\3' + row()]))
        self.verdict('REJECTED')

    def test_aborted_transaction_must_be_absent(self):
        self.case.journal.write_bytes(history('ABORT'))
        self.verdict('REJECTED')

    def test_unknown_whole_commit_or_absence(self):
        self.case.journal.write_bytes(history('UNKNOWN'))
        self.verdict('ACCEPTED')
        self.case.write('wal-00000001-0000000000000001.axf', self.case.head + self.case.create)
        self.case.observed = {'status': 'READY', 'q': 1, 'state': {'tables': [TABLE], 'rows': []}}
        self.verdict('ACCEPTED')

    def test_pending_commit_only_after_owned_kill(self):
        self.case.journal.write_bytes(history('PENDING'))
        self.verdict('ACCEPTED')
        self.case.journal.write_bytes(history('PENDING')[:-29])
        self.verdict('INCOMPLETE')

    def test_duplicate_ack_cannot_reuse_one_ddl_batch(self):
        self.case.journal.write_bytes(history(duplicate_ddl=True))
        self.verdict('INVALID')

    def test_configure_fault_is_an_observed_control_without_commit_effect(self):
        self.case.journal.write_bytes(history(configure_fault=True))
        self.verdict('ACCEPTED')

    def test_empty_success_statement_has_no_private_effect(self):
        self.case.journal.write_bytes(history(empty_statement=True))
        self.verdict('ACCEPTED')

    def test_transaction_local_order_need_not_follow_commit_q(self):
        self.case.write('wal-00000001-0000000000000001.axf', self.case.head + batch(10, 1, [b'\1' + schema()]) + self.case.put)
        self.case.journal.write_bytes(history(ddl_local=10))
        self.verdict('ACCEPTED')

    def test_payload_afterimage_mismatch_rejects_even_with_same_txn_q(self):
        self.case.write('wal-00000001-0000000000000001.axf', self.case.head + self.case.create + batch(2, 2, [b'\3' + row(8)]))
        observed = copy.deepcopy(STATE)
        observed['rows'][0]['values'][0]['value'] = 8
        self.case.observed['state'] = observed
        self.verdict('REJECTED')

    def test_partial_tail_requires_predeclared_profile(self):
        self.case.write('wal-00000001-0000000000000001.axf', self.case.wal + b'AXF1\1\0')
        result = self.verdict('ACCEPTED')
        self.assertEqual(len(result['tails']), 1)
        self.case.expect['allow_incomplete_tail'] = False
        self.verdict('INCOMPLETE')

    def test_short_declared_payload_and_known_partial_version_reject(self):
        self.case.write('wal-00000001-0000000000000001.axf', self.case.wal + b'AXF1' + u16(1) + u32(1))
        self.verdict('REJECTED')
        self.case.write('wal-00000001-0000000000000001.axf', self.case.wal + b'AXF1\2')
        result = self.verdict('REJECTED')
        self.assertEqual(result['code'], 'UNSUPPORTED_FORMAT')

    def test_oversized_declared_payload_is_incomplete_not_fallback(self):
        self.case.write('wal-00000001-0000000000000001.axf', self.case.wal + b'AXF1' + u16(1) + u32(oracle.MAX_PAYLOAD + 1))
        self.verdict('INCOMPLETE')

    def test_full_bad_tail_is_corruption_even_if_last_unacknowledged(self):
        bad = bytearray(batch(3, 3, [b'\3' + row(8)]))
        bad[-1] ^= 1
        self.case.write('wal-00000001-0000000000000001.axf', self.case.wal + bad)
        self.verdict('REJECTED')
        self.case.expect['expected_outcome'] = 'REJECT_CORRUPTION'
        self.case.observed = {'status': 'REJECTED', 'normal_queries': 0, 'new_acks': 0}
        self.verdict('ACCEPTED')

    def test_missing_required_wal_rejects(self):
        (self.case.image / 'wal-00000001-0000000000000001.axf').unlink()
        self.verdict('REJECTED')

    def test_high_number_orphan_is_not_authority(self):
        self.case.write('manifest-ffffffffffffffff.axf', b'unselected junk')
        self.case.write('wal-ffffffff-ffffffffffffffff.axf', b'unselected junk')
        self.verdict('ACCEPTED')

    def test_exact_previous_fallback_only_for_corruption(self):
        bad = b'bad-current'
        self.case.write('manifest-0000000000000002.axf', bad)
        self.case.write('CURRENT', current(2, bad, (1, self.case.base_manifest)))
        result = self.verdict('ACCEPTED')
        self.assertTrue(result['fallback'])
        unsupported = bytearray(self.case.base_manifest[10:-32])
        unsupported[4:6] = u16(2)
        bad = frame(unsupported)
        self.case.write('manifest-0000000000000002.axf', bad)
        self.case.write('CURRENT', current(2, bad, (1, self.case.base_manifest)))
        result = self.verdict('REJECTED')
        self.assertEqual(result['code'], 'UNSUPPORTED_FORMAT')

    def test_corrupt_selector_does_not_search_for_other_manifest(self):
        self.case.write('CURRENT', b'bad')
        self.verdict('REJECTED')

    def test_anchor_inside_a_frame_rejects(self):
        point = anchor(self.case.head, len(self.case.head) + 1, self.case.wal)
        changed = manifest(1, point)
        self.case.write('manifest-0000000000000001.axf', changed)
        self.case.write('CURRENT', current(1, changed))
        self.verdict('REJECTED')

    def test_sealed_tail_rejects(self):
        seal = frame(common(3) + u64(len(self.case.wal)) + u64(2) + digest(self.case.wal))
        self.case.write('wal-00000001-0000000000000001.axf', self.case.wal + seal + b'A')
        self.verdict('REJECTED')

    def test_authenticated_resume_preserves_old_partial_tail(self):
        old = self.case.wal + b'AXF1'
        previous = link(1, 1, old, len(self.case.wal), 2)
        resume = frame(common(4, 2) + u64(1) + previous)
        head = header(2, start=2, origin=2, previous=previous, resume=digest(resume))
        self.case.write('wal-00000001-0000000000000001.axf', old)
        self.case.write('resume-00000002.axf', resume)
        self.case.write('wal-00000002-0000000000000001.axf', head)
        self.case.write('WRITER', writer(head, 2))
        self.verdict('ACCEPTED')
        (self.case.image / 'resume-00000002.axf').unlink()
        self.verdict('REJECTED')

    def test_checkpoint_with_retained_prefix_uses_command_replay(self):
        chunk = frame(common(8) + u64(2) + u32(0) + u32(1) + row())
        checkpoint = manifest(2, anchor(self.case.head, len(self.case.wal), self.case.wal, 2), 2, [schema()], [(0, chunk, 1)])
        self.case.write('chunk-0000000000000002-00000000.axf', chunk)
        self.case.write('manifest-0000000000000002.axf', checkpoint)
        self.case.write('CURRENT', current(2, checkpoint))
        result = self.verdict('ACCEPTED')
        self.assertEqual(result['checkpoint_cut'], 2)
        wrong = frame(common(8) + u64(2) + u32(0) + u32(1) + row(8))
        bad = manifest(2, anchor(self.case.head, len(self.case.wal), self.case.wal, 2), 2, [schema()], [(0, wrong, 1)])
        self.case.write('chunk-0000000000000002-00000000.axf', wrong)
        self.case.write('manifest-0000000000000002.axf', bad)
        self.case.write('CURRENT', current(2, bad))
        observed = copy.deepcopy(STATE)
        observed['rows'][0]['values'][0]['value'] = 8
        self.case.observed['state'] = observed
        self.verdict('REJECTED')

    def test_actual_open_failure_cannot_pass_valid_image(self):
        self.case.observed = {'status': 'REJECTED', 'normal_queries': 0, 'new_acks': 0}
        self.verdict('REJECTED')

    def test_rejected_corruption_must_not_expose_queries(self):
        self.case.write('CURRENT', b'bad')
        self.case.expect['expected_outcome'] = 'REJECT_CORRUPTION'
        self.case.observed = {'status': 'REJECTED', 'normal_queries': 1, 'new_acks': 0}
        self.verdict('REJECTED')

    def test_unknown_file_and_symlink_are_not_trusted(self):
        self.case.write('foreign', b'')
        self.verdict('REJECTED')
        (self.case.image / 'foreign').unlink()
        (self.case.image / 'manifest-0000000000000002.axf').symlink_to(self.case.image / 'CURRENT')
        self.verdict('REJECTED')

    def test_duplicate_json_field_is_invalid(self):
        with self.assertRaises(oracle.OracleError) as caught:
            oracle.json_bytes(b'{"a":1,"a":2}')
        self.assertEqual(caught.exception.verdict, 'INVALID')

    def test_journal_response_without_sent_is_invalid(self):
        raw = self.case.journal.read_bytes()
        first_payload = struct.unpack_from('>I', raw, 25)[0]
        end = 29 + first_payload
        self.case.journal.write_bytes(raw[:end] + raw[end + 29:])
        self.verdict('INVALID')

    def test_bounded_inventory_returns_incomplete(self):
        with mock.patch.object(oracle, 'MAX_FILES', 2):
            self.verdict('INCOMPLETE')

    def test_absent_selector_is_distinct_from_corruption(self):
        (self.case.image / 'CURRENT').unlink()
        result = self.verdict('REJECTED')
        self.assertEqual(result['code'], 'INCOMPLETE_CREATE')
        self.case.expect['expected_outcome'] = 'REJECT_INCOMPLETE_CREATE'
        self.case.observed = {'status': 'REJECTED', 'normal_queries': 0, 'new_acks': 0}
        self.verdict('ACCEPTED')

    def test_pruned_prefix_without_membership_proof_is_incomplete(self):
        head = header(2, start=2, origin=3)
        chunk = frame(common(8, 2) + u64(2) + u32(0) + u32(1) + row())
        checkpoint = manifest(2, anchor(head, q=2, epoch=2), 2, [schema()], [(0, chunk, 1)], epoch=2)
        self.case.write('chunk-0000000000000002-00000000.axf', chunk)
        self.case.write('manifest-0000000000000002.axf', checkpoint)
        self.case.write('CURRENT', current(2, checkpoint, epoch=2))
        self.case.write('WRITER', writer(head, 2))
        self.case.write('wal-00000002-0000000000000001.axf', head)
        (self.case.image / 'wal-00000001-0000000000000001.axf').unlink()
        self.verdict('INCOMPLETE')

    def test_net_zero_receipt_without_baseline_is_incomplete_not_rejected(self):
        history_model = oracle.History(('01' * 16, 1))
        value = oracle.normalize_row(VALUE)
        with self.assertRaises(oracle.OracleError) as caught:
            history_model.commit({'transaction': tx(2), 'position': 1, 'wrote': False},
                                 [{'change': 'put', 'row': value}], 1, 2, 'literal')
        self.assertEqual(caught.exception.verdict, 'INCOMPLETE')

    def test_overwritten_identical_witness_cannot_prove_missing_ack(self):
        history_model = oracle.History(('01' * 16, 1))
        key_a, key_b = oracle.transaction_key(tx(2)), oracle.transaction_key(tx(3))
        value = oracle.normalize_row(VALUE)
        changes = [{'change': 'put', 'row': value}]
        history_model.add(oracle.Attempt(key_a, 'ACK', 2, changes, 1, 2, 'A', issued_rows=(ROW,)))
        history_model.add(oracle.Attempt(key_b, 'ACK', 3, changes, 3, 4, 'B'))
        state = oracle.normalize_state(STATE)
        plan = {'reference': {'generation': 1, 'digest': '00' * 32}, 'manifest': {'cut': 3},
                'checkpoint_state': state, 'batches': {key_b: {'q': 3, 'changes': changes}}, 'q': 3}
        expectation = {'history_complete': True, 'checkpoint_evidence': [
            {'generation': 1, 'digest': '00' * 32, 'cut': 3, 'state': STATE,
             'prefix': [{'transaction': tx(2), 'q': 2, 'witness': VALUE}]}]}
        with self.assertRaises(oracle.OracleError) as caught:
            oracle.verify_history(plan, history_model, expectation)
        self.assertEqual(caught.exception.verdict, 'INCOMPLETE')
        self.assertIn('another transaction', str(caught.exception))


if __name__ == '__main__':
    unittest.main()
