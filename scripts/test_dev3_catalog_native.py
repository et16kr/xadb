#!/usr/bin/env python3
"""New controller rejection cases; no product process, grant or database I/O."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location('catalog_native', Path(__file__).with_name('dev3_catalog_native.py'))
controller = importlib.util.module_from_spec(spec)
spec.loader.exec_module(controller)


def rows(*events):
    return [[str(i), *event] for i, event in enumerate(events)]


def selected_fixture(root):
    """Independent fixed-offset controller fixture, not a product encoder.

    Its opaque prior WAL header is only a prefix binding input. This fixture
    tests the controller's Base3 projection and cannot be opened as a database.
    """
    oracle = controller.Oracle(Path(__file__).resolve().parents[1])
    db, previous = root / 'db', root / 'previous'
    db.mkdir()
    previous.mkdir()
    raw = bytearray(2097152)
    put = lambda target, at, value, width=8: target.__setitem__(slice(at, at + width), value.to_bytes(width, 'little'))
    raw[:8] = b'XRB1\x03\x00\x01\x00'
    put(raw, 8, len(raw), 4)
    put(raw, 12, 4096, 4)
    raw[16:32] = bytes(range(1, 17))
    put(raw, 32, 1)
    put(raw, 88, 1)
    put(raw, 96, 9)
    put(raw, 144, 1, 4)
    put(raw, 192, 1)
    put(raw, 208, 2)
    put(raw, 224, 1)
    put(raw, 248, 9)
    put(raw, 260, 1, 4)
    put(raw, 264, 512)
    point = oracle.points['N01', 9]
    for at, value in ((272, point['root_masks'][1]), (280, point['root_masks'][2]),
                      (288, point['scalar']['revision']), (296, point['scalar']['aging_groups'])):
        put(raw, at, value)
    wal = bytearray(4096)
    wal[:24] = b'opaque controller prefix'
    for lsn in range(1, 10):
        item = oracle.rows['N01', lsn]
        assert len(wal) == item['wal_start']
        wal.extend(oracle.raw[item['frame']])
    (previous / 'd3-wal-00000001').write_bytes(wal)
    put(raw, 152, len(wal))
    raw[160:192] = controller.digest(wal)
    raw[1192:1224] = controller.digest(wal[:4096])
    for wire, key in ((32, 'system'), (34, 'aging')):
        raw[4096 + wire * 1536:4096 + (wire + 1) * 1536] = oracle.raw[point[key]]
    for page_number in range(64):
        page = oracle.page('N01', 9, page_number)
        raw[262144 + page_number * 16384:262144 + (page_number + 1) * 16384] = page
        if not any(page):
            continue
        cert = bytearray(96)
        cert[0], cert[1] = 1, page[6]
        cert[8:32] = page[24:48]
        cert[32:56] = page[48:72]
        cert[56:88] = controller.digest(page)
        raw[65536 + page_number * 96:65536 + (page_number + 1) * 96] = cert
    # Four explicit initial sources. This is independent of Oracle.provenance's
    # generic scan and typed-field construction.
    for page_number, lsn, role, owner_kind, work, generation in (
            (9, 3, '4', 1, 1, 1), (10, 5, '5', 1, 2, 1),
            (11, 8, '6', 2, 1, 2), (12, 8, '7', 2, 1, 2)):
        row = oracle.rows['N01', lsn]
        image = oracle.raw[row['after'][role]]
        entry = bytearray(352)
        entry[:5] = bytes((1, page_number - 8, image[6], 0, owner_kind))
        entry[8:48] = image[24:64]
        put(entry, 48, lsn)
        put(entry, 56, 1, 4)
        entry[64:96] = bytes.fromhex(row['frame'])
        entry[96:128] = controller.digest(image)
        entry[128:144] = bytes(range(1, 17))
        put(entry, 144, 1)
        put(entry, 152, 1, 4)
        put(entry, 160, work)
        put(entry, 168, generation)
        put(entry, 176, lsn - 1)
        if owner_kind == 2:
            for at, value in ((184, 6), (192, 1), (200, 1), (208, 0x100000001),
                              (216, 11), (224, 2), (232, 0x100000001), (240, 12), (248, 2)):
                put(entry, at, value)
        raw[73728 + page_number * 352:73728 + (page_number + 1) * 352] = entry
    inventory = bytearray(16384)
    inventory[:8] = b'XOI1\x01\x00\x01\x02'
    for at, value, width in ((8, 128, 2), (10, 96, 2), (12, 216, 4), (32, 1, 8), (40, 1, 8), (48, 9, 8), (56, 1, 2)):
        put(inventory, at, value, width)
    inventory[16:32] = bytes(range(1, 17))
    inventory[88:120] = controller.digest(b'')
    inventory[128:184] = oracle.raw[point['aging']][16:72]
    inventory[184:216] = controller.digest(inventory[:184])
    raw[98304:114688] = inventory
    current = bytearray(4096)
    current[:4] = b'XRC1'
    current[16:32] = bytes(range(1, 17))
    put(current, 32, 1)
    current[256:260] = b'\x02\x01\x00\x01'
    for at, value, width in ((260, 2, 4), (264, 1, 8), (272, 9, 8), (280, 2097152, 4), (352, 4096, 8)):
        put(current, at, value, width)
    def publish():
        for index, (offset, length, count, width) in enumerate(((4096, 55296, 36, 1536), (65536, 6144, 64, 96), (73728, 22528, 64, 352), (98304, 16384, 1, 16384), (262144, 1048576, 64, 16384))):
            at = 1280 + index * 64
            put(raw, at, index + 1, 2)
            for field, value in ((4, offset), (8, length), (12, count), (16, width)):
                put(raw, at + field, value, 4)
            raw[at + 32:at + 64] = controller.digest(raw[offset:offset + length])
        raw[-32:] = controller.digest(raw[:-32])
        current[288:320] = controller.digest(raw)
        current[-32:] = controller.digest(current[:-32])
        (db / 'd3-rebase-0').write_bytes(raw)
        (db / 'CURRENT').write_bytes(current)
    publish()
    return oracle, db, previous, raw, current, publish


class ControllerRejections(unittest.TestCase):
    def test_marker_pid_occurrence_and_resource_bounds(self):
        cut = 'A_CREATE_ACK_T'
        self.assertEqual(controller.marker_resources(f'{cut}\t100\t1\t2\t3\t6\n', cut, 100)['peak'], 3)
        for raw in (f'{cut}\t101\t1\t2\t3\t6\n', f'{cut}\t100\t2\t2\t3\t6\n',
                    f'{cut}\t100\t1\t4\t4\t6\n', f'{cut}\t100\t1\t2\t3\t7\n',
                    f'{cut}\t100\t1\t-1\t3\t6\n'):
            with self.assertRaises(ValueError):
                controller.marker_resources(raw, cut, 100)

    def test_barrier_must_be_single_final_event(self):
        cut = 'A_CREATE_ACK_T'
        event = ('barrier', cut, '0', '0', 'REACHED')
        controller.audit_cut(rows(event), cut, 1000)
        for raw in (rows(event, event), rows(event, ('write', 'file', '0', '1', 'OK:1'))):
            with self.assertRaises(ValueError):
                controller.audit_cut(raw, cut, 1000)

    def test_pending_publication_requires_directory_sync_before_home(self):
        cut = 'A_CREATE_BEFORE_CATALOG_HOME'
        good = rows(('arm', cut, '131072', '16384', 'ARMED:1'),
                    ('publish', 'd3-dwb-select-stage', '0', '0', 'DWB:Ok(())'),
                    ('dirsync', 'namespace', '0', '0', 'Ok(())'),
                    ('cut', cut, '131072', '16384', 'REACHED:1'),
                    ('barrier', cut, '0', '0', 'REACHED'))
        controller.audit_cut(good, cut, 1000)
        for bad in (good[:2] + good[3:], good[:3] + [['3', 'write', controller.HOME, '131072', '16384', 'OK:16384']] + good[3:]):
            with self.assertRaises(ValueError):
                controller.audit_cut(bad, cut, 1000)

    def test_after_error_requires_exact_end_and_real_successful_sync(self):
        cut, name = 'A_CREATE_UNKNOWN_AFTER_SYNC', 'A_CREATE_COMMIT_SYNC_AFTER_ERROR'
        good = rows(('arm', name, '1000', '0', 'ARMED:1'),
                    ('sync', 'd3-wal-00000001', '0', '0', 'Ok(())'),
                    ('fault', name, '1000', '0', 'AFTER_SUCCESS:1'),
                    ('barrier', cut, '0', '0', 'REACHED'))
        controller.audit_cut(good, cut, 1000)
        for raw, end in ((good[:1] + good[2:], 1000), (good, 999)):
            with self.assertRaises(ValueError):
                controller.audit_cut(raw, cut, end)

    def test_old_wal_unlink_requires_directory_sync(self):
        good = rows(('publish', 'd3-current-stage', '0', '0', 'CURRENT:Ok(())'),
                    ('dirsync', 'namespace', '0', '0', 'Ok(())'),
                    ('remove', 'd3-wal-00000002', '0', '0', 'Ok(())'),
                    ('dirsync', 'namespace', '0', '0', 'Ok(())'),
                    ('create', 'd3-wal-00000003', '0', '0', 'OK'))
        controller.audit_unlink(good, 2)
        for raw in (good[:3] + good[4:], good[:1] + good[2:], good + good[2:3], good[:3] + good[4:] + good[3:4]):
            with self.assertRaises(ValueError):
                controller.audit_unlink(raw, 2)

    def test_ack_identity_and_unknown_no_ack(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            identity = {'case': 'N01', 'phase': 'produce', 'pid': '100', 'source_sha256': 's'}
            controller.audit_ack(path, 'A_CREATE_UNKNOWN_AFTER_SYNC', identity, 9)
            with self.assertRaises(ValueError):
                controller.audit_ack(path, 'A_CREATE_ACK_T', identity, 9)
            receipt = {**identity, 'kind': 'AckObserved', 'cut': 'A_CREATE_ACK_T', 'lsn': 9}
            (path / 'ack-observed.json').write_text(json.dumps(receipt))
            controller.audit_ack(path, 'A_CREATE_ACK_T', identity, 9)
            with self.assertRaises(ValueError):
                controller.audit_ack(path, 'A_CREATE_UNKNOWN_AFTER_SYNC', identity, 9)
            with self.assertRaises(ValueError):
                controller.audit_ack(path, 'A_CREATE_ACK_T', {**identity, 'pid': '101'}, 9)

    def test_source_hash_inventory_and_symlink_drift(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'member').write_bytes(b'original')
            manifest = {'files': {'member': {'bytes': 8, 'sha256': controller.sha(root / 'member')}}}
            controller.verify_source(root, manifest)
            (root / 'member').write_bytes(b'modified')
            with self.assertRaises(ValueError):
                controller.verify_source(root, manifest)
            (root / 'member').write_bytes(b'original')
            (root / 'extra').write_bytes(b'x')
            with self.assertRaises(ValueError):
                controller.verify_source(root, manifest)
            (root / 'extra').unlink()
            (root / 'link').symlink_to(root / 'member')
            with self.assertRaises(ValueError):
                controller.verify_source(root, manifest)

    def test_duplicate_receipt_and_trace_sequence_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'data'
            path.write_text('pid\t100\npid\t101\n')
            with self.assertRaises(ValueError):
                controller.fields(path)
            path.write_text('1\tsync\tfile\t0\t0\tOk(())\n')
            with self.assertRaises(ValueError):
                controller.trace_rows(path)

    def test_fast_child_output_and_database_caps(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'N01'
            (output / 'produce').mkdir(parents=True)
            (output / 'database').mkdir()
            budget = {**controller.BUDGET, 'output_bytes': 4, 'database_bytes': 4, 'free_floor_bytes': 0}
            with patch.object(controller, 'BUDGET', budget):
                (output / 'produce/stdout.log').write_bytes(b'12345')
                with self.assertRaises(ValueError):
                    controller.enforce_bounds(output, root)
                (output / 'produce/stdout.log').unlink()
                (output / 'database/file').write_bytes(b'12345')
                with self.assertRaises(ValueError):
                    controller.enforce_bounds(output, root)

    def test_inventory_actual_short_seal_not_page_tail(self):
        raw = bytearray(16384)
        raw[:8] = b'XOI1\x01\x00\x01\x02'
        for at, width, value in ((8, 2, 128), (10, 2, 96), (12, 4, 160), (32, 8, 1), (40, 8, 2), (48, 8, 9)):
            raw[at:at + width] = value.to_bytes(width, 'little')
        raw[16:32] = bytes(range(1, 17))
        raw[88:120] = controller.digest(b'')
        raw[128:160] = controller.digest(raw[:128])
        controller.audit_inventory(raw, 2, 9)
        for at in (48, 128, 3000):
            broken = raw.copy()
            broken[at] ^= 1
            with self.assertRaises(ValueError):
                controller.audit_inventory(broken, 2, 9)

    def test_malformed_selected_current_is_not_accepted(self):
        oracle = object.__new__(controller.Oracle)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'CURRENT').write_bytes(bytes(4096))
            with self.assertRaises(ValueError):
                oracle.base(root, root, 'N01', 9, 2)

    def test_exact_inventory_owners_and_no_unexpected_reservations(self):
        raw = bytearray(16384)
        expected = bytes(range(56))
        raw[56:58] = (1).to_bytes(2, 'little')
        raw[128:184] = expected
        controller.audit_inventory_owners(raw, expected)
        for at in (56, 72, 64, 128):
            broken = raw.copy()
            broken[at] ^= 1
            with self.assertRaises(ValueError):
                controller.audit_inventory_owners(broken, expected)

    def test_reap_race_and_cleanup_failure_preserve_original_failure(self):
        active = Mock(pid=100)
        active.poll.return_value = None
        active.wait.return_value = -9
        records = [{'pid': 100}]
        result = {'status': 'FAIL', 'error': 'original failure'}
        with patch.object(controller.os, 'killpg', side_effect=ProcessLookupError):
            controller.abort_and_reap(active, records, result)
        self.assertTrue(records[-1]['aborted_and_reaped'])
        self.assertEqual(result['error'], 'original failure')
        active.wait.side_effect = TimeoutError('unreaped')
        with patch.object(controller.os, 'killpg'):
            controller.abort_and_reap(active, records, result)
        self.assertEqual(result['error'], 'original failure')
        self.assertIn('unreaped', result['cleanup_error'])

    def test_selected_base_accepts_wire_bank_and_rejects_resealed_mutants(self):
        with tempfile.TemporaryDirectory() as directory:
            oracle, db, previous, raw, current, publish = selected_fixture(Path(directory))
            self.assertEqual(oracle.base(db, previous, 'N01', 9, 2)['capture'], 9)
            original = raw.copy()
            # Wrong wire bank and forged previous identity stay self-sealed.
            for mutation in ('bank', 'predecessor', 'generation', 'owner', 'provenance', 'certificate', 'root_revision'):
                raw[:] = original
                if mutation == 'bank':
                    left, right = 4096 + 32 * 1536, 4096 + 34 * 1536
                    raw[left:left + 1536], raw[right:right + 1536] = raw[right:right + 1536], raw[left:left + 1536]
                elif mutation == 'owner':
                    raw[98304 + 128] ^= 1
                    raw[98304 + 184:98304 + 216] = controller.digest(raw[98304:98304 + 184])
                else:
                    at = {'predecessor': 112, 'generation': 88, 'provenance': 73728 + 11 * 352 + 168,
                          'certificate': 65536 + 11 * 96 + 48, 'root_revision': 288}[mutation]
                    raw[at] ^= 1
                publish()
                with self.assertRaises(ValueError, msg=mutation):
                    oracle.base(db, previous, 'N01', 9, 2)


if __name__ == '__main__':
    unittest.main()
