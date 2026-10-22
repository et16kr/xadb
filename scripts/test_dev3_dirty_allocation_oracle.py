#!/usr/bin/env python3
"""New oracle checks only. No Rust compiler, product code or old selector runs."""
import hashlib
from pathlib import Path
import unittest

import dev3_dirty_allocation_oracle as oracle

ROOT = Path(__file__).resolve().parent.parent
DIRTY = ROOT / oracle.OUTPUT
LEGACY = ROOT / oracle.LEGACY


def load(part, dirty=True):
    return ((DIRTY if dirty else LEGACY) / ('p1-n1-' + part)).read_bytes()


def seal_descriptor_in_frame(descriptor, frame):
    descriptor[-32:] = hashlib.sha256(descriptor[:-32]).digest()
    frame[352:1888] = descriptor
    frame[-32:] = hashlib.sha256(frame[:-32]).digest()


class DirtyOracleChecks(unittest.TestCase):
    def test_generated_context_fields_and_hashes(self):
        before = (ROOT / oracle.BASE / 'profile1-allocation.bin').read_bytes()
        for cancel, lsn in [(False, 1), (True, 2)]:
            operation = 'cancel' if cancel else 'reserve'
            frame, page, descriptor = (load(operation + '.' + suffix) for suffix in ('frame', 'page', 'descriptor'))
            previous = load('reserve.descriptor') if cancel else None
            oracle.inspect_operation(frame, before, page, descriptor, previous, 1, lsn, cancel)
            self.assertEqual(int.from_bytes(descriptor[968:976], 'little'), 0 if cancel else 98304)
            self.assertEqual(int.from_bytes(descriptor[1288:1296], 'little'), 0 if cancel else 98304)
            self.assertEqual(page, load(operation + '.page', dirty=False))
            before = page
        self.assertNotEqual(load('reserve.descriptor'), load('reserve.descriptor', dirty=False))
        self.assertNotEqual(load('cancel.frame'), load('cancel.frame', dirty=False))

    def test_coherent_wrong_context_rejected(self):
        descriptor, frame = bytearray(load('reserve.descriptor')), bytearray(load('reserve.frame'))
        for offset in (968, 1288):
            descriptor[offset:offset + 8] = (81920).to_bytes(8, 'little')
        seal_descriptor_in_frame(descriptor, frame)
        before = (ROOT / oracle.BASE / 'profile1-allocation.bin').read_bytes()
        with self.assertRaisesRegex(ValueError, '^trusted Terminal memory$'):
            oracle.inspect_operation(frame, before, load('reserve.page'), descriptor, None, 1, 1, False)

    def test_wrong_aggregate_rejected(self):
        descriptor, frame = bytearray(load('reserve.descriptor')), bytearray(load('reserve.frame'))
        descriptor[1288:1296] = (81920).to_bytes(8, 'little')
        seal_descriptor_in_frame(descriptor, frame)
        before = (ROOT / oracle.BASE / 'profile1-allocation.bin').read_bytes()
        with self.assertRaisesRegex(ValueError, '^trusted aggregate memory$'):
            oracle.inspect_operation(frame, before, load('reserve.page'), descriptor, None, 1, 1, False)

    def test_cancel_legacy_before_hash_rejected(self):
        frame = bytearray(load('cancel.frame'))
        frame[248:280] = hashlib.sha256(load('reserve.descriptor', dirty=False)).digest()
        frame[-32:] = hashlib.sha256(frame[:-32]).digest()
        with self.assertRaisesRegex(ValueError, '^before descriptor hash$'):
            oracle.inspect_operation(frame, load('reserve.page'), load('cancel.page'),
                                     load('cancel.descriptor'), load('reserve.descriptor'), 1, 2, True)
        with self.assertRaisesRegex(ValueError, '^trusted before Terminal memory$'):
            oracle.inspect_operation(frame, load('reserve.page'), load('cancel.page'),
                                     load('cancel.descriptor'), load('reserve.descriptor', dirty=False), 1, 2, True)


if __name__ == '__main__':
    unittest.main()
