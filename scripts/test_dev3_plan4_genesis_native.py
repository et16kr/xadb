"""Six new pure native-harness boundaries; no engine or native worker process."""
from pathlib import Path
import os
import tempfile
import unittest
from unittest.mock import Mock

import dev3_plan4_genesis_native as native


class NativeBoundaries(unittest.TestCase):
    def test_requires_entropy_lineage(self):
        """H04: the independent entropy request/result is the only namespace seed."""
        valid = '1\t41\t0102030405060708090a0b0c0d0e0f10\n'
        native.validate_entropy(valid, 'produce', 41)
        native.validate_entropy('', 'select', 42)
        for text in ('', valid * 2, valid.replace('01', 'ff', 1),
                     valid.replace('41', '42'), 'header\t0102030405060708090a0b0c0d0e0f10\n'):
            with self.subTest(text=text), self.assertRaises(ValueError):
                native.validate_entropy(text, 'produce', 41)
        with self.assertRaises(ValueError):
            native.validate_entropy(valid, 'reentry', 41)

    def test_requires_three_real_phase_pids(self):
        """H05: actual orchestration invokes three distinct process boundaries."""
        execute = Mock(side_effect=[{'pid': n} for n in (41, 42, 43)])
        inspect = Mock(side_effect=lambda phase, process: dict(process, phase=phase))
        rows = native.run_phases(native.planned_phases({}), {}, execute, inspect)
        self.assertEqual([c.args[0] for c in execute.call_args_list], list(native.PHASES))
        self.assertEqual([r['pid'] for r in rows.values()], [41, 42, 43])
        with self.assertRaises(ValueError):
            native.run_phases(native.PHASES, {}, Mock(side_effect=[{'pid': 41}] * 3), inspect)
        process = {'pid': 41, 'exact': True, 'exit_code': 0, 'truncated': False}
        case = 'P4-NATIVE-P1-PRODUCE'
        text = f'running 1 test\nP4_NATIVE_COMPLETE\t{case}\t41\n1 passed; 0 failed;\n'
        native.require_completion(text, case, process)
        for changed in (text.replace('P4_NATIVE_COMPLETE', 'OBSERVATION'),
                        text.replace('\t41', '\t42'), text + text,
                        text.replace('running 1 test', 'running 2 tests')):
            with self.assertRaises(ValueError):
                native.require_completion(changed, case, process)

    def test_rejects_missing_or_extra_files(self):
        """H06: stable15 does not include either publication stage or CURRENT."""
        names = ['LOCK', 'd3-profile', 'WRITER', 'd3-wal-00000001', 'FILEMAP',
                 'd3-bootstrap', 'd3-filemap-0', 'd3-filemap-1', 'd3-data-0000000100000001',
                 'DWB', 'd3-dwb-stage', 'd3-dwb-0', 'd3-dwb-1', 'd3-dwb-images', 'd3-dwb-reserved']
        expected = dict.fromkeys(names, b'x')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name, raw in expected.items(): (root / name).write_bytes(raw)
            self.assertEqual(len(native.compare_files(root, expected)['files']), 15)
            (root / names[0]).unlink()
            with self.assertRaises(ValueError): native.compare_files(root, expected)
            (root / names[0]).write_bytes(b'x')
            for extra in ('CURRENT', 'd3-rebase-0', 'd3-writer-stage', 'd3-filemap-select-stage', 'd3-dwb-select-stage'):
                (root / extra).write_bytes(b'x')
                with self.assertRaises(ValueError): native.compare_files(root, expected)
                (root / extra).unlink()

    def test_compares_unused_bytes_too(self):
        """H07: full comparisons cover unused DWB and the final Free page byte."""
        expected = {'d3-dwb-1': bytes(16384), 'd3-dwb-images': bytes(16384),
                    'd3-data-0000000100000001': bytes(1_048_576)}
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name, raw in expected.items(): (root / name).write_bytes(raw)
            receipt = native.compare_files(root, expected)
            self.assertEqual(receipt['compared_bytes'], sum(map(len, expected.values())))
            for name in expected:
                path = root / name
                with path.open('r+b') as stream:
                    stream.seek(len(expected[name]) - 1)
                    stream.write(b'!')
                with self.assertRaises(ValueError): native.compare_files(root, expected)
                path.write_bytes(expected[name])

    def test_requires_clone_and_stable_inodes(self):
        """H08: snapshot copy has separate inodes; reentry preserves live ones."""
        expected = {'LOCK': b'', 'data': b'canonical'}
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source, copied = root / 'source', root / 'snapshot'
            source.mkdir()
            for name, raw in expected.items(): (source / name).write_bytes(raw)
            first = native.compare_files(source, expected)
            clone = native.clone_snapshot(source, copied, expected)
            self.assertNotEqual(first['files']['data']['inode'], clone['files']['data']['inode'])
            replacement = root / 'replacement'
            replacement.write_bytes(expected['data'])
            os.replace(replacement, source / 'data')
            with self.assertRaises(ValueError):
                native.require_stable(first, native.compare_files(source, expected))
            (copied / 'data').unlink()
            os.link(source / 'data', copied / 'data')
            with self.assertRaises(ValueError): native.compare_files(copied, expected)
            (copied / 'data').unlink()
            (copied / 'data').symlink_to(source / 'data')
            with self.assertRaises(ValueError): native.compare_files(copied, expected)
            (copied / 'data').unlink()
            (copied / 'data').write_bytes(expected['data'])
            immutable = {str(copied): native.retained_index(copied)}
            (copied / 'data').write_bytes(b'corrupted')
            with self.assertRaises(ValueError): native.require_original_history(immutable)

    def test_rejects_readonly_effects(self):
        """H09: every mutation attempt fails, including sync and created LOCK."""
        events = [('acquire', 'LOCK', 'ATTEMPT'), ('acquire', 'LOCK', 'EXISTING'),
                  ('read', 'd3-data-0000000100000001', 'ATTEMPT'),
                  ('drop_lease', 'LOCK', 'OK')]
        def trace(rows):
            return ''.join(f'{i}\t{op}\t{name}\t0\t0\t{result}\n'
                           for i, (op, name, result) in enumerate(rows))
        native.validate_effects(trace(events), 'select')
        for operation in sorted(native.MUTATIONS):
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                native.validate_effects(trace(events + [(operation, 'data', 'ATTEMPT')]), 'select')
        with self.assertRaises(ValueError):
            native.validate_effects(trace(events).replace('EXISTING', 'CREATED'), 'reentry')

    def test_requires_audit_for_uncertain_native_phase(self):
        """H14: source-added continuation guard, separate from the original six."""
        case = 'P4-NATIVE-P1-SELECT'
        process = {'pid': 42, 'exit_code': 101, 'stop_reason': None, 'truncated': False,
                   'parsed': {'parser_error': None, 'active': None,
                              'markers': {'BEGIN': [case], 'PASS': [], 'FAIL': [case]}}}
        attempt = {'case': case, 'state': 'RETURNED', 'process': process}
        native.require_retryable_attempt(attempt, 'P4_FAIL\t' + case)
        variants = [{'case': case, 'state': 'INVOKING'}]
        import copy
        for key, value in [('exit_code', 0), ('stop_reason', 'OUTPUT_LIMIT'), ('truncated', True)]:
            changed = copy.deepcopy(attempt)
            changed['process'][key] = value
            variants.append(changed)
        for key, value in [('parser_error', 'bad marker'), ('active', case)]:
            changed = copy.deepcopy(attempt)
            changed['process']['parsed'][key] = value
            variants.append(changed)
        changed = copy.deepcopy(attempt)
        changed['process']['parsed']['markers']['PASS'] = [case]
        variants.append(changed)
        for changed in variants:
            with self.assertRaises(ValueError): native.require_retryable_attempt(changed, '')
        with self.assertRaises(ValueError):
            native.require_retryable_attempt(attempt, f'P4_NATIVE_COMPLETE\t{case}\t42')

    def test_rejects_fresh_chain_after_existing_profile(self):
        """H15: a new output/revision pathname does not reset completed IDs."""
        with tempfile.TemporaryDirectory() as temp:
            unit = Path(temp)
            native.require_lineage_selection(unit, 1, [])
            old = unit / 'old-revision' / 'native-a'
            old.mkdir(parents=True)
            native.fresh_json(old / 'planned.json', {'schema': native.SCHEMA, 'profile': 1})
            with self.assertRaises(ValueError): native.require_lineage_selection(unit, 1, [])
            native.require_lineage_selection(unit, 1, [str(old)])
            native.require_lineage_selection(unit, 2, [])
            hidden = unit / 'new-revision' / 'new-name'
            hidden.mkdir(parents=True)
            native.fresh_json(hidden / 'planned.json', {'schema': native.SCHEMA, 'profile': 1})
            with self.assertRaises(ValueError): native.require_lineage_selection(unit, 1, [str(old)])


if __name__ == '__main__':
    unittest.main()
