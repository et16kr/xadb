"""New .b input checks only; synthetic checks are not product evidence."""
import csv
import hashlib
import json
from pathlib import Path
import struct
import unittest

import dev3_status_home_flush_literals as oracle

BASE = oracle.ROOT / 'tests/fixtures/dev3-status-home-flush'


def table(name):
    with (BASE / name).open() as stream:
        return list(csv.DictReader(stream, delimiter='\t'))


def literal(logical):
    rows = {r['logical']: r for r in table('literals.tsv')}
    row = rows[logical]
    data = (BASE / row['blob']).read_bytes()
    if len(data) != int(row['bytes']) or hashlib.sha256(data).hexdigest() != row['sha256']:
        raise ValueError('literal identity')
    return data


class StatusHomeFlushLiteralChecks(unittest.TestCase):
    def test_independent_target_wire_and_full_bytes(self):
        for profile in (1, 2, 3):
            for order in ('AS', 'SA'):
                for batch, target in enumerate(order, 1):
                    stem = f'p{profile}-c0-{order}-b{batch}'
                    page, slot = literal(stem + '/capture'), literal(stem + '/slot')
                    self.assertEqual((len(page), len(slot)), (16384, 16544))
                    self.assertEqual(page[6], 9 if target == 'S' else 10)
                    self.assertEqual(struct.unpack_from('<Q', page, 32)[0], int(target == 'S'))
                    self.assertEqual(slot[128:-32], page)
                    self.assertEqual(slot[-32:], hashlib.sha256(slot[:-32]).digest())
                    self.assertEqual(slot[96:128], hashlib.sha256(page).digest())
                    for offset, phase in enumerate(('issuance', 'pending', 'retirement')):
                        control = literal(stem + '/' + phase + '.control')
                        self.assertEqual(len(control), 248 if phase == 'pending' else 160)
                        self.assertEqual(struct.unpack_from('<QQ', control, 32), (3*batch-1+offset, batch))
                        self.assertEqual(control[-32:], hashlib.sha256(control[:-32]).digest())
                        if phase == 'pending':
                            self.assertEqual(control[130], page[6])
                            self.assertEqual(control[184:216], hashlib.sha256(page).digest())

    def test_partial_regions_and_distinct_batch_campaign_facts(self):
        facts = {r['id']: r for r in json.loads((BASE / 'fault-facts.json').read_text())}
        half = facts['SF-P1-SA-PREFIX-21']
        expected = literal('p1-c0-SA-b1/capture')
        self.assertEqual(literal('SF-P1-SA-PREFIX-21/home:page1'), expected[:8192] + bytes(8192))
        self.assertEqual(half['injected_batch_facts']['completed'], 20)
        self.assertEqual(half['final_campaign_facts']['known_generation'], 3)
        for mode in ('SHORT', 'INTERRUPTED'):
            row = facts[f'SF-P1-SA-{mode}-21']
            self.assertEqual(row['injected_batch'], 1)
            self.assertEqual(row['injected_batch_facts']['known_selection_index'], 2)
            self.assertEqual(row['final_campaign_facts']['final_batch'], 2)
            self.assertEqual(row['final_campaign_facts']['known_generation'], 7)
            self.assertEqual(row['final_campaign_facts']['last_issued'], 2)
        for cut in (8, 9, 19, 20, 30, 31):
            row = facts[f'SF-P1-SA-BEFORE-{cut:02}']
            self.assertEqual(len(row['injected_batch_facts']['possible_selection_indices']), 2)

    def test_exact_alias_union_and_new31_history(self):
        manifest = json.loads((BASE / 'manifest.json').read_text())
        self.assertEqual((manifest['mandatory_cases'], manifest['rust_mandatory'], manifest['python_witness_cases']), (43, 42, 1))
        faults, histories = table('faults.tsv'), table('histories.tsv')
        self.assertEqual((len(faults), len(histories)), (570, 24))
        self.assertEqual(sum(r['mandatory_alias'] != '-' for r in faults), 6)
        self.assertEqual(43 + 570 - 6 + 24, manifest['unique_scenarios'])
        self.assertEqual({r['mandatory_alias']: r['id'] for r in faults if r['mandatory_alias'] != '-'}, oracle.ALIASES)
        for p in (1, 2, 3):
            page = literal(f'p{p}-c31/work32-status.after')
            self.assertEqual(struct.unpack_from('<Q', page, 40)[0], 33)
            self.assertEqual(struct.unpack_from('<Q', page, 64)[0], 64)
            self.assertEqual(len(literal(f'p{p}-c31/wal.frames-only')), 31 * 42400 + 59200)

    def test_twelve_named_witnesses_and_required_order_edges(self):
        oracle.inspect_witness(oracle.valid_witness())
        rows = oracle.negative_witnesses()
        self.assertEqual([r['id'] for r in rows], [f'N{i:02}' for i in range(1, 13)])
        for row in rows:
            with self.assertRaisesRegex(ValueError, '^' + row['expected_rejection'] + '$'):
                oracle.inspect_witness(row['witness'])
        for before, after in (('capture', 'image_write'), ('image_write', 'image_sync')):
            w = oracle.valid_witness()
            w['events'].remove(before)
            w['events'].insert(w['events'].index(after) + 1, before)
            with self.assertRaises(ValueError):
                oracle.inspect_witness(w)
        w = oracle.valid_witness()
        w['events'].remove('wal_durable'); w['events'].insert(0, 'wal_durable')
        oracle.inspect_witness(w)  # Durable fact may precede capture.


if __name__ == '__main__':
    unittest.main()
