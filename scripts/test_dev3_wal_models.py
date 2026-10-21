"""Checker adversaries use synthetic input and do not count as product execution."""
import json
from pathlib import Path
import unittest
from dev3_wal_durable_model import ACTIONS, evaluate, input_bytes, parse_trace
from dev3_wal_slot_model import evaluate as slot_model

ROOT = Path(__file__).resolve().parent.parent


def synthetic_trace():
    payloads = input_bytes(ROOT)
    return '\n'.join(f'WAL_IO_TRACE|{at}|{kind}|{name}|{4096 if at == 12 else 0}|'
                     f'{len(payloads.get(at, b""))}|{payloads.get(at, b"").hex()}'
                     for at, (_, kind, name) in enumerate(ACTIONS))


class WalModelCheckerTests(unittest.TestCase):
    def test_fixed_corpus_and_slot_mutants(self):
        report = evaluate(synthetic_trace(), ROOT)
        self.assertEqual((report['cuts'], report['image_count']), (28, 82))
        self.assertEqual(slot_model()['normal']['transitions'], 1246)

    def test_missing_duplicate_and_reordered_effects_reject(self):
        lines = synthetic_trace().splitlines()
        for text in ('\n'.join(lines[:-1]), '\n'.join(lines + [lines[-1]]),
                     '\n'.join([lines[1], lines[0], *lines[2:]]),
                     synthetic_trace().replace('WAL_IO_TRACE|0|', 'wrongWAL_IO_TRACE|0|')):
            with self.assertRaises(ValueError):
                parse_trace(text, ROOT)

    def test_wrong_bytes_offsets_names_and_lengths_reject(self):
        lines = synthetic_trace().splitlines()
        for at, field, replacement in [(12, 4, '4095'), (4, 3, 'd3-writer-stage'),
                                       (8, 5, '4095'), (8, 6, '00' * 4096), (10, 6, '01')]:
            changed = list(lines)
            parts = changed[at].split('|')
            parts[field] = replacement
            changed[at] = '|'.join(parts)
            with self.assertRaises(ValueError):
                parse_trace('\n'.join(changed), ROOT)


if __name__ == '__main__':
    unittest.main()
