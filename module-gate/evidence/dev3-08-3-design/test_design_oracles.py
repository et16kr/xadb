"""Independent O0 arithmetic/state oracles, not XADB runtime verification.

Run: python3 -B module-gate/evidence/dev3-08-3-design/test_design_oracles.py
Only Python's standard library is used. Wire constants come from the contracts,
not the production encoders or the earlier frames-r*.py calculations. These
bounded cases are inputs for O1--O3; passing them cannot qualify those stages.
"""

from collections import Counter
from dataclasses import dataclass, replace
import hashlib
import struct
import unittest


PAYLOAD = 16_092
MAX_ROW = 131_072


def count(total):
    return (total + PAYLOAD - 1) // PAYLOAD


def native(before, after):
    return 128 + before + after


def listed_size(entries):
    return 16 + 40 * entries if entries else 0


def return_frame(handoff, before, added):
    return (
        (3776 if handoff else 2176)
        + native(512, 512)
        + native(1024, 1024)
        + native(listed_size(before), listed_size(before + added))
        + (208 if handoff else 0)
        + 3 * (64 + 16280)
    )


@dataclass(frozen=True)
class Ownership:
    """Sets are deliberately separate from allocation; overlaps must be caught."""

    allocated: frozenset = frozenset()
    pending: frozenset = frozenset()
    leaf: frozenset = frozenset()
    releasing: frozenset = frozenset()
    retired: frozenset = frozenset()

    def valid(self):
        owners = Counter()
        for group in (self.pending, self.leaf, self.releasing, self.retired):
            owners.update(group)
        return set(owners) == set(self.allocated) and all(
            occurrences == 1 for occurrences in owners.values()
        )


class OverflowDesignOracles(unittest.TestCase):
    def test_every_supported_length_has_a_complete_reverse_written_suffix(self):
        for total in range(2049, MAX_ROW + 1):
            chunks = [
                (start, min(start + PAYLOAD, total))
                for start in range(0, total, PAYLOAD)
            ]
            self.assertEqual(len(chunks), count(total))
            self.assertLessEqual(len(chunks), 9)
            self.assertEqual(sum(end - start for start, end in chunks), total)
            for written in range(1, len(chunks) + 1):
                first = len(chunks) - written
                self.assertEqual(chunks[first][0], first * PAYLOAD)
                self.assertEqual(chunks[-1][1], total)
                self.assertTrue(all(0 < end - start <= PAYLOAD
                                    for start, end in chunks[first:]))

    def test_return_frames_fit_for_every_list_and_segment_length(self):
        # The contract uses 55 as a conservative bound, including combinations
        # that a live table's anchor/root occupancy would exclude.
        cancel, retire = [], []
        for before in range(55):
            for added in range(1, min(9, 55 - before) + 1):
                cancel.append(return_frame(True, before, added))
                retire.append(return_frame(False, before, added))
        self.assertEqual(max(cancel), 60864)
        self.assertEqual(max(retire), 59056)
        self.assertTrue(all(frame <= 98304 for frame in cancel + retire))
        self.assertEqual(return_frame(True, 0, 1), 56528)
        self.assertEqual(return_frame(False, 0, 1), 54720)

    def test_debt_conservation_at_every_chunk_and_cleanup_cut(self):
        write = (3776 + native(512, 512) + native(1024, 1024)
                 + native(96, 96) + native(0, 128)
                 + 2 * (64 + 16280) + 64 + 96 + 64 + 16384)
        self.assertEqual(write, 56976)
        release = 55528
        for previous in (0, 1):
            forward = 91304 + 244 + 128 * previous
            prune = 89496 + 2 * 244 + 128 * previous
            cleanup = prune + 4 * release
            self.assertEqual(cleanup, 312096 + 128 * previous)
            self.assertGreaterEqual(cleanup, 59056 + 4 * release)
            for chunks in range(1, 10):
                for written in range(chunks + 1):
                    remaining = (chunks - written) * write + forward
                    self.assertEqual(written * write + remaining,
                                     chunks * write + forward)
                    self.assertGreaterEqual(remaining, 60864)
                    if written < chunks:
                        self.assertEqual(remaining - write,
                                         (chunks - written - 1) * write + forward)
            for before in range(55):
                for added in range(1, min(9, 55 - before) + 1):
                    actual = return_frame(False, before, added)
                    self.assertLessEqual(actual + 4 * release, 59056 + 4 * release)
        # Root-specific WAL subtotal; excludes transaction/control/terminal work.
        self.assertEqual(57336 + 9 * write + 91676 + 90112 + 4 * release,
                         974020)

    def test_atomic_owner_transfers_and_crash_prefixes(self):
        for chunks in range(1, 10):
            state = Ownership()
            self.assertTrue(state.valid())
            # Page ordinals rise as logical chunk indices fall. No reservation
            # or reuse is assumed; a crash exposes a whole group prefix only.
            for index in reversed(range(chunks)):
                page = 9 + chunks - 1 - index
                pages = state.allocated | {page}
                state = replace(state, allocated=pages, pending=pages)
                self.assertTrue(state.valid())
                canceled = replace(state, pending=frozenset(), retired=pages)
                self.assertTrue(canceled.valid())
                self.assertEqual(canceled.allocated, state.allocated)
                # Missing old-owner deletion and missing new-owner insertion
                # must both fail; checking only total allocation is insufficient.
                self.assertFalse(replace(state, retired=pages).valid())
                self.assertFalse(replace(state, pending=frozenset()).valid())
            forwarded = replace(state, pending=frozenset(), leaf=state.pending)
            self.assertTrue(forwarded.valid())
            # Two leaf chains may share a page: keep occurrences, not only the
            # union, when checking one owner category. A set would hide this.
            leaf_occurrences = tuple(forwarded.leaf)
            self.assertFalse(replace(forwarded,
                                     leaf=leaf_occurrences + leaf_occurrences[:1]).valid())
            compensated = replace(forwarded, leaf=frozenset(),
                                  releasing=forwarded.leaf)
            self.assertTrue(compensated.valid())
            retired = replace(compensated, releasing=frozenset(),
                              retired=compensated.releasing)
            self.assertTrue(retired.valid())
            self.assertEqual(retired.allocated, forwarded.allocated)

    def test_root_zero_projection_preserves_value_commitment(self):
        # Independent head fixtures: root(24), total(4), count(2), zero(2), SHA(32).
        total = 16100
        digest = hashlib.sha256(b"v" * total).digest()
        suffix = (total.to_bytes(4, "little") + count(total).to_bytes(2, "little")
                  + bytes(2) + digest)
        heads = [struct.pack("<QQQ", 0x100000001, page, 2) + suffix
                 for page in (9, 23, 63)]
        projected = [bytes(24) + head[24:] for head in heads]
        self.assertEqual(len(heads[0]), 64)
        self.assertEqual(len(set(projected)), 1)
        self.assertEqual(len({hashlib.sha256(head).digest() for head in heads}), 3)
        for offset in range(24, 64):
            changed = bytearray(projected[0])
            changed[offset] ^= 1
            self.assertNotEqual(hashlib.sha256(changed).digest(),
                                hashlib.sha256(projected[0]).digest())


if __name__ == "__main__":
    unittest.main(verbosity=2)
