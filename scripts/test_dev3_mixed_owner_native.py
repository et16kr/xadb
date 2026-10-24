"""New runner's known-bad receipts and independent mixed bytes; no Rust worker.

These tests are individually selectable with unittest. Importing this module
never builds, spawns a product process or invokes an earlier native test suite.
"""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import dev3_mixed_owner_native as check

ROOT = Path(__file__).resolve().parent.parent


def text_for(phase, pid=41):
    return (f'test {check.SELECTORS[phase]} ... '
            f'MIXED_OWNER_CASE_BEGIN\t{check.CASE_IDS[phase]}\n'
            f'MIXED_OWNER_NATIVE_BEGIN\t{phase}\t{pid}\n'
            f'MIXED_OWNER_NATIVE_PASS\t{phase}\t{pid}\n'
            f'MIXED_OWNER_CASE_PASS\t{check.CASE_IDS[phase]}\n'
            'ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 500 filtered out; finished in 0.00s\n')


class MixedOwnerNative(unittest.TestCase):
    def test_original_fixture_manifest_and_every_raw_hash(self):
        hashes = check.fixture_identity(ROOT)
        manifest = json.loads((ROOT / check.INPUT / 'manifest.json').read_text())
        self.assertEqual(len(manifest['files']), 286)
        self.assertEqual(hashes[str(check.INPUT / 'manifest.json')], check.MANIFEST_SHA)
        for row in manifest['files']:
            self.assertEqual(hashes[str(check.INPUT / row['path'])], row['sha256'])
        with patch.object(check, 'MANIFEST_SHA', '0' * 64):
            with self.assertRaisesRegex(ValueError, 'FAIL_FIXTURE_MANIFEST'):
                check.fixture_identity(ROOT)

    def test_exact_phase_selector_markers_pid_and_count(self):
        original = text_for('produce')
        check.protocol(original, 'produce', 41)
        bad = [original.replace(check.SELECTORS['produce'], check.SELECTORS['recover']),
               original.replace('\t41', '\t42'), original.replace('1 passed', '2 passed'),
               original.replace('0 ignored', '1 ignored'), original.replace('NATIVE_PASS', 'NATIVE_FAILED'),
               original.replace('CASE_PASS', 'CASE_SKIP'), original + 'test result: ok. 1 passed;\n',
               original.replace('MO-NATIVE-PRODUCE', 'MO-NATIVE-RECOVER')]
        for value in bad:
            with self.subTest(value=value), self.assertRaises(ValueError):
                check.protocol(value, 'produce', 41)

    def test_mixed_expected_files_preserve_dirty_allocator_and_c_until_recovery(self):
        history = check.expected_history(ROOT)
        before, after = history['produce'], history['recover']
        page = check.model.PAGE
        self.assertEqual(before['d3-bootstrap'], (ROOT / 'tests/fixtures/dev3-undo-bootstrap/profile1-d3-bootstrap.bin').read_bytes())
        self.assertEqual(before[check.model.DATA][:page], check.literal(ROOT, 'allocator.genesis'))
        self.assertEqual(before[check.model.DATA][3 * page:], bytes(len(before[check.model.DATA]) - 3 * page))
        for index in (1, 2):
            self.assertEqual(before[check.model.DATA][index * page:(index + 1) * page], check.literal(ROOT, f'sequence/work{index}-payload.after'))
        self.assertEqual(after[check.model.DATA][:page], check.literal(ROOT, 'sequence/work3-allocator.after'))
        self.assertEqual(after[check.model.DATA][3 * page:4 * page], check.literal(ROOT, 'sequence/work3-payload.after'))
        self.assertEqual(before[check.model.WAL], after[check.model.WAL])
        self.assertEqual(after, history['reentry'])
        self.assertEqual(before['d3-dwb-images'][88:90], bytes([8, 0]))
        self.assertEqual(after['d3-dwb-images'][128:128 + page], check.literal(ROOT, 'sequence/work3-payload.after'))
        self.assertEqual(check.model.number(before['d3-dwb-0'], 32), 7)
        self.assertEqual(check.model.number(after['d3-dwb-0'], 32), 13)

    def test_topup_counts_all_four_canonical_pages_not_two_writes(self):
        expected = {check.model.WAL: bytes(182112)}
        inode = {'device': 2, 'inode': 3}
        facts = {'complete_lsn': '6', 'complete_offset': '182112', 'source_length': '182112',
                 'highwater': '3', 'active_mask': '0', 'terminal_hold_mask': '14', 'inventory': '0',
                 'remaining_wal': '0', 'remaining_pages': '0', 'remaining_protected': '0',
                 'replay_topup': '65536', 'restored': '1', 'generation': '13', 'batch': '4',
                 'bank': '0', 'state': 'Empty', 'phase': 'Snapshot',
                 'pending_repaired': 'false', 'pending_home_skipped': 'false', 'pending_retired': 'false',
                 'source_stabilized': 'true', 'slots_provisioned': 'true',
                 'lease_effect': 'MayHaveChanged', 'canonical_passes': '1', 'dirty_mask': '9',
                 'flushed_mask': '9', 'source_device': '2', 'source_inode': '3',
                 'actual_owned': '24344416', 'outstanding': '16384', 'owned_peak': '24360800',
                 'remaining_frames': '0', 'remaining_memory': '0', 'remaining_files': '0',
                 'protected': '23377760', 'epoch_credit': '0', 'inventory_digest': check.model.sha(b'')}
        check.check_ledger(facts, expected, 'recover', inode)
        for key, wrong in [('replay_topup', '32768'), ('remaining_wal', '59408'),
                           ('highwater', '2'), ('generation', '7'), ('dirty_mask', '1'),
                           ('source_inode', '4'), ('owned_peak', '121'), ('protected', '0'),
                           ('epoch_credit', '1'), ('terminal_hold_mask', '0'), ('inventory_digest', '00' * 32), ('phase', 'ReadOnly'),
                           ('pending_repaired', 'true'), ('pending_home_skipped', 'true'), ('pending_retired', 'true'),
                           ('remaining_frames', '1'), ('remaining_memory', '1'), ('remaining_files', '1')]:
            changed = dict(facts, **{key: wrong})
            with self.subTest(key=key), self.assertRaises(ValueError):
                check.check_ledger(changed, expected, 'recover', inode)
        changed = dict(facts, actual_owned='100', outstanding='20', owned_peak='120')
        with self.assertRaisesRegex(ValueError, 'FAIL_MIXED_LEDGER'):
            check.check_ledger(changed, expected, 'recover', inode)
        facts['dirty_mask'] = facts['flushed_mask'] = '0'
        check.check_ledger(facts, expected, 'reentry', inode)
        facts['replay_topup'] = '0'
        with self.assertRaisesRegex(ValueError, 'FAIL_MIXED_LEDGER'):
            check.check_ledger(facts, expected, 'reentry', inode)

    def test_full_snapshot_rejects_changed_bytes_missing_files_and_duplicate_rows(self):
        expected = {'a': b'first-page', 'b': b'whole-second-file'}
        with tempfile.TemporaryDirectory() as temp:
            stage = Path(temp)
            disk = stage / 'disk'
            disk.mkdir()
            lines = ['name\tdevice\tinode\tlength\tsha256']
            for index, (name, raw) in enumerate(expected.items()):
                (disk / name).write_bytes(raw)
                lines.append(f'{name}\t2\t{index + 10}\t{len(raw)}\t{check.model.sha(raw)}')
            table = stage / 'disk-inventory.tsv'
            table.write_text('\n'.join(lines) + '\n')
            self.assertEqual(check.snapshot(stage, expected)['a']['inode'], 10)
            (disk / 'b').write_bytes(b'whole-second-filE')
            with self.assertRaises(ValueError):
                check.snapshot(stage, expected)
            (disk / 'b').write_bytes(expected['b'])
            for bad in (lines[:-1], lines + [lines[-1]]):
                table.write_text('\n'.join(bad) + '\n')
                with self.assertRaises(ValueError):
                    check.snapshot(stage, expected)

    def test_exact_home_effects_and_zero_reentry_writes(self):
        with tempfile.TemporaryDirectory() as temp:
            stage = Path(temp)
            rows = [f'write\t{check.model.DATA}\t{offset}\t16384\t16384:{check.model.sha(raw)}'
                    for offset, raw in check.home_writes(ROOT, 'recover')]
            events = stage / 'events.tsv'
            events.write_text('\n'.join(rows) + '\n')
            effects = stage / 'effects.txt'
            keys = ('home_write_attempted', 'home_written', 'home_sync_attempted', 'home_synced')
            effects.write_text('RecoveryEffects {\n' + ''.join(f'    {key}: 9,\n' for key in keys) + '}\n')
            self.assertEqual(check.check_effects(stage, ROOT, 'recover')['home_writes'], 2)
            correct = effects.read_text()
            effects.write_text(correct.replace(': 9,', ': 2,'))
            with self.assertRaisesRegex(ValueError, 'FAIL_HOME_EFFECT_MASK'):
                check.check_effects(stage, ROOT, 'recover')
            effects.write_text(correct)
            for bad in (rows[:1], rows[::-1], rows + [rows[-1]],
                        rows + [f'write\t{check.model.WAL}\t0\t1\t1:00']):
                events.write_text('\n'.join(bad) + '\n')
                with self.assertRaises(ValueError):
                    check.check_effects(stage, ROOT, 'recover')
            events.write_text('read\tDWB\t0\t4096\t4096\n')
            effects.write_text('RecoveryEffects {\n' + ''.join(f'    {key}: 0,\n' for key in keys) + '}\n')
            self.assertEqual(check.check_effects(stage, ROOT, 'reentry')['home_writes'], 0)
            events.write_text(rows[-1] + '\n')
            with self.assertRaises(ValueError):
                check.check_effects(stage, ROOT, 'reentry')

    def test_phase_selection_never_replays_completed_producer(self):
        self.assertEqual(check.select_phases('all', []), ['produce', 'recover', 'reentry'])
        self.assertEqual(check.select_phases('all', ['produce']), ['recover', 'reentry'])
        self.assertEqual(check.select_phases('recover', ['produce']), ['recover'])
        self.assertEqual(check.select_phases('reentry', ['produce', 'recover']), ['reentry'])
        for phase, completed in [('produce', ['produce']), ('recover', []), ('reentry', ['produce']),
                                 ('all', ['recover']), ('all', list(check.PHASES))]:
            with self.subTest(phase=phase, completed=completed), self.assertRaises(ValueError):
                check.select_phases(phase, completed)

    def test_inode_policy_only_allows_dwb_selector_replacement(self):
        before = {name: {'device': 1, 'inode': index + 10} for index, name in enumerate(('DWB', 'LOCK', check.model.WAL, check.model.DATA))}
        after = copy.deepcopy(before)
        after['DWB']['inode'] = 999
        check.preserve_inodes(before, after)
        for name in ('LOCK', check.model.WAL, check.model.DATA):
            bad = copy.deepcopy(after)
            bad[name]['inode'] += 1
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'FAIL_UNEXPECTED_INODE_CHANGE'):
                check.preserve_inodes(before, bad)

    def test_resume_checks_saved_failure_and_refuses_partial_effects(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            db = folder / 'database'
            db.mkdir()
            stage = folder / 'produce'
            stage.mkdir()
            (stage / 'stdout.log').write_text('unfinished producer')
            phase = dict(status='FAIL', worker_completed=False, files=check.file_receipts(stage))
            check.fresh_json(stage / 'phase.json', phase)
            binary = folder / 'engine-tests'
            binary.write_bytes(b'not executed')
            identity = {'source_sha256': 'a' * 64, 'binary_sha256': check.digest(binary)}
            report = dict(identity=identity, status='FAIL', completed=[], database=str(db),
                          database_inventory={}, elapsed_seconds=0.1,
                          failed_phase=dict(output=str(stage), receipt_sha256=check.digest(stage / 'phase.json')))
            receipt = folder / 'result.json'
            def write_report():
                report['retention'] = {'files': check.file_receipts(folder, (receipt, binary, db))}
                receipt.write_text(json.dumps(report))
            write_report()
            self.assertEqual(check.check_resume(ROOT, receipt, identity, {})['completed'], [])
            (db / 'partial').write_bytes(b'changed')
            report['database_inventory'] = check.prior.inventory(db)
            write_report()
            with self.assertRaisesRegex(ValueError, 'FAIL_PARTIAL_EFFECTS_REQUIRE_SEPARATE_RECOVERY_INPUT'):
                check.check_resume(ROOT, receipt, identity, {})
            (db / 'partial').unlink()
            report['database_inventory'] = {}
            phase['worker_completed'] = True
            (stage / 'phase.json').write_text(json.dumps(phase))
            report['failed_phase']['receipt_sha256'] = check.digest(stage / 'phase.json')
            write_report()
            with self.assertRaisesRegex(ValueError, 'FAIL_COMPLETED_WORKER_REQUIRES_OUTPUT_AUDIT'):
                check.check_resume(ROOT, receipt, identity, {})

    def test_live_database_continuation_preserves_immutable_retention(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            db = folder / 'database'
            db.mkdir()
            (db / 'home').write_bytes(b'original image')
            binary = folder / 'engine-tests'
            binary.write_bytes(b'never executed')
            report = {}
            check.retain(folder, db, binary, report)
            frozen = dict(report['retention']['files'])
            self.assertFalse(any(name.startswith('database/') for name in frozen))
            self.assertIn('retained-database/home', frozen)
            (db / 'home').write_bytes(b'continued by next phase')
            self.assertEqual(check.file_receipts(folder, (binary, db)), frozen)
            self.assertEqual((folder / 'retained-database/home').read_bytes(), b'original image')
            (folder / 'retained-database/home').write_bytes(b'altered snapshot')
            self.assertNotEqual(check.file_receipts(folder, (binary, db)), frozen)

    def test_resume_validates_ancestor_raw_outputs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            identity = {'source_sha256': 'a' * 64}
            prior = None
            for name in ('first', 'second'):
                folder = root / name
                folder.mkdir()
                (folder / 'engine-tests').write_bytes(b'never executed')
                identity['binary_sha256'] = check.digest(folder / 'engine-tests')
                (folder / 'stderr.log').write_bytes(b'original failure stream')
                report = dict(identity=dict(identity), status='FAIL',
                              retention={'files': check.file_receipts(folder, (folder / 'engine-tests',))},
                              origin={'path': str(prior), 'sha256': check.digest(prior)} if prior else None)
                prior = folder / 'result.json'
                check.fresh_json(prior, report)
            check.verify_immutable_chain(prior, identity)
            (root / 'first/stderr.log').write_bytes(b'altered ancestor failure')
            with self.assertRaisesRegex(ValueError, 'FAIL_RESUME_RETAINED_OUTPUT_DRIFT'):
                check.verify_immutable_chain(prior, identity)

    def test_create_new_receipts_and_input_output_drift(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            path = folder / 'identity.json'
            check.fresh_json(path, {'source': 'a'})
            with self.assertRaises(FileExistsError):
                check.fresh_json(path, {'source': 'b'})
            before = check.file_receipts(folder)
            self.assertEqual(json.loads(path.read_text()), {'source': 'a'})
            path.write_text('{"source":"b"}')
            self.assertNotEqual(check.file_receipts(folder), before)

    def test_failed_worker_is_reaped_and_receipted_without_second_spawn(self):
        class FailedChild:
            pid = 57
            returncode = 101

            def __enter__(self):
                return self

            def __exit__(self, *_args):
                return False

        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            db = folder / 'database'
            db.mkdir()
            identity = {'source_sha256': 'a' * 64, 'binary_sha256': 'b' * 64}
            with patch.object(check.subprocess, 'Popen', return_value=FailedChild()) as spawn, \
                    patch.object(check.prior, 'finish_child') as finish:
                result = check.execute_phase(ROOT, folder / 'unexecuted-elf', folder / 'produce', db,
                                             'produce', identity, [], lambda: 10)
            self.assertEqual(spawn.call_count, 1)
            self.assertEqual(finish.call_count, 1)
            self.assertEqual(result['status'], 'FAIL')
            self.assertEqual(result['exit_code'], 101)
            self.assertFalse(result['worker_completed'])
            self.assertEqual(json.loads((folder / 'produce/phase.json').read_text())['error'], 'FAIL_PROCESS_EXIT_OR_TIMEOUT')
            env = spawn.call_args.kwargs['env']
            self.assertEqual(env['XADB_MIXED_OWNER_INCLUDE'], 'MO-NATIVE-PRODUCE')
            self.assertEqual(env['XADB_MIXED_OWNER_NATIVE_PRODUCER_OUT'], str(folder / 'produce'))
            self.assertEqual(spawn.call_args.args[0][2], check.SELECTORS['produce'])


if __name__ == '__main__':
    unittest.main()
