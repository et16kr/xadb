"""New continuation checker cases only; every product process is mocked."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import dev3_mixed_owner_continue as check


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def make_tree(root, worker):
    for name in check.BUILD_FILES:
        write(root / name, b'fixed build input')
    for package in check.PACKAGES:
        write(root / f'crates/{package}/Cargo.toml', b'fixed package')
        write(root / f'crates/{package}/src/lib.rs', b'fixed runtime')
    write(root / check.WORKER, worker)
    write(root / 'crates/engine/src/storage/wal_io/recovery_effect/native_tests.rs', b'actual native wrapper shape')


class MixedOwnerContinue(unittest.TestCase):
    def test_lineage_requires_exact_origin_build_source_and_binary(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            origin = root / 'result.json'
            origin.write_text('original failure')
            old_receipt, new_receipt = root / 'old.json', root / 'new.json'
            output = root / 'approved-new-output'
            old = dict(source_sha256='a' * 64, binary_sha256='b' * 64, build_receipt_sha256='c' * 64)
            new = dict(source_sha256='d' * 64, binary_sha256='e' * 64, build_receipt_sha256='f' * 64)
            lineage = dict(schema=1, status=check.APPROVAL, runtime_exception=check.WORKER, allowed_phases=list(check.PHASES),
                           original={**old, 'build_receipt': str(old_receipt), 'result': str(origin), 'result_sha256': check.native.digest(origin)},
                           continuation={**new, 'build_receipt': str(new_receipt), 'output': str(output)})
            check.validate_lineage(lineage, origin, old_receipt, new_receipt, old, new, output)
            for section, key, bad in [(None, 'status', 'DRAFT'), (None, 'allowed_phases', ['produce', 'recover']),
                                      (None, 'runtime_exception', 'crates/engine/src/lib.rs'),
                                      ('original', 'result_sha256', '0' * 64), ('original', 'source_sha256', '0' * 64),
                                      ('continuation', 'binary_sha256', '0' * 64), ('continuation', 'build_receipt', 'relative.json'),
                                      ('continuation', 'output', str(root / 'different-output'))]:
                changed = copy.deepcopy(lineage)
                (changed if section is None else changed[section])[key] = bad
                with self.subTest(section=section, key=key), self.assertRaises(ValueError):
                    check.validate_lineage(changed, origin, old_receipt, new_receipt, old, new, output)

    def test_runtime_build_and_native_wrapper_drift_hold_before_effects(self):
        with tempfile.TemporaryDirectory() as temp:
            old, new = Path(temp) / 'old', Path(temp) / 'new'
            make_tree(old, b'old worker')
            make_tree(new, b'new reviewed worker')
            old_sha, new_sha = check.native.digest(old / check.WORKER), check.native.digest(new / check.WORKER)
            write(old / 'crates/frontend/src/lib.rs', b'old independent frontend')
            write(new / 'crates/frontend/src/lib.rs', b'new independent frontend')
            result = check.runtime_accounting(old, new, old_sha, new_sha)
            self.assertEqual({row['path'] for row in result['changed_paths']}, {check.WORKER, 'crates/frontend/src/lib.rs'})
            for name in (*check.BUILD_FILES, 'crates/engine/Cargo.toml', 'crates/foundation/src/lib.rs',
                         'crates/platform/src/lib.rs', 'crates/engine/src/storage/wal_io/recovery_effect/native_tests.rs'):
                path = new / name
                original = path.read_bytes()
                path.write_bytes(original + b' drift')
                with self.subTest(path=name), self.assertRaisesRegex(ValueError, 'HOLD_RUNTIME_OR_BUILD_SOURCE_CHANGED'):
                    check.runtime_accounting(old, new, old_sha, new_sha)
                path.write_bytes(original)
            write(new / '.cargo/config.toml', b'changed rustflags')
            with self.assertRaisesRegex(ValueError, 'HOLD_RUNTIME_OR_BUILD_SOURCE_CHANGED'):
                check.runtime_accounting(old, new, old_sha, new_sha)
            (new / '.cargo/config.toml').unlink()
            write(old / check.FORMAT_ONLY_TEST, b'const { assert!(3 < 4); }')
            write(new / check.FORMAT_ONLY_TEST, b'const {\n    assert!(3 < 4);\n}')
            formatting = dict(path=check.FORMAT_ONLY_TEST, old_sha256=check.native.digest(old / check.FORMAT_ONLY_TEST),
                              new_sha256=check.native.digest(new / check.FORMAT_ONLY_TEST))
            with self.assertRaisesRegex(ValueError, 'HOLD_RUNTIME_OR_BUILD_SOURCE_CHANGED'):
                check.runtime_accounting(old, new, old_sha, new_sha)
            observed = check.runtime_accounting(old, new, old_sha, new_sha, formatting)
            self.assertEqual(observed['reviewed_format_only_test'], formatting)
            with self.assertRaisesRegex(ValueError, 'FAIL_FORMAT_ONLY_TEST_LINEAGE'):
                check.runtime_accounting(old, new, old_sha, new_sha, {**formatting, 'new_sha256': '0' * 64})
            write(new / check.FORMAT_ONLY_TEST, b'const { assert!(3 < 5); }')
            formatting['new_sha256'] = check.native.digest(new / check.FORMAT_ONLY_TEST)
            with self.assertRaisesRegex(ValueError, 'HOLD_FORMAT_ONLY_TEST_CONTENT_CHANGED'):
                check.runtime_accounting(old, new, old_sha, new_sha, formatting)

    def test_copy_uses_complete_bytes_new_inodes_and_preserves_source(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / 'original-producer/disk'
            source.mkdir(parents=True)
            expected = {'LOCK': b'', 'home': b'full page bytes', 'wal': b'complete WAL prefix'}
            for name, raw in expected.items():
                (source / name).write_bytes(raw)
            before = check.native.prior.inventory(source)
            destination = root / 'new/database'
            destination.parent.mkdir()
            with patch.object(check.os, 'fsync', wraps=check.os.fsync) as sync:
                result = check.copy_producer(source, destination, expected, before)
            self.assertEqual(sync.call_count, len(expected) + 2)
            self.assertTrue(result['inodes_changed'])
            self.assertEqual(check.native.prior.inventory(source), before)
            for name in expected:
                self.assertEqual((destination / name).read_bytes(), expected[name])
                self.assertNotEqual((destination / name).stat().st_ino, (source / name).stat().st_ino)
                self.assertEqual((destination / name).stat().st_nlink, 1)
            with self.assertRaises(FileExistsError):
                check.copy_producer(source, destination, expected, before)

    def test_dispatches_exactly_two_new_pids_and_never_producer(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            identity = {'source_sha256': 'a' * 64, 'binary_sha256': 'b' * 64}
            old_identity = {'source_sha256': 'c' * 64, 'binary_sha256': 'd' * 64}
            producer = dict(phase='produce', pid=41, output=str(root / 'original/produce'))
            seen = []
            def fake_execute(_root, binary, stage, db, phase, observed_identity, completed, remaining, producer_identity=None):
                seen.append((phase, binary, observed_identity, copy.deepcopy(completed), producer_identity))
                stage.mkdir()
                item = {'pid': 42 + len(seen) - 1, 'status': 'PASS', 'phase': phase}
                check.native.fresh_json(stage / 'phase.json', item)
                return item
            completed = []
            with patch.object(check.native, 'execute_phase', side_effect=fake_execute) as execute:
                workers, failed = check.run_phases(root, root / 'new-elf', root, root / 'database', identity,
                                                  producer, old_identity, lambda: 10, completed)
            self.assertEqual(execute.call_count, 2)
            self.assertEqual([row[0] for row in seen], ['recover', 'reentry'])
            self.assertEqual([row['pid'] for row in completed], [42, 43])
            self.assertEqual([row['phase'] for row in completed], ['recover', 'reentry'])
            self.assertIsNone(failed)
            self.assertEqual(len(workers), 2)
            self.assertEqual(seen[0][3], [producer])
            self.assertEqual([row['phase'] for row in seen[1][3]], ['produce', 'recover'])
            self.assertTrue(all(row[1] == root / 'new-elf' and row[2] == identity and row[4] == old_identity for row in seen))
            self.assertEqual(producer['pid'], 41)

    def test_failed_recovery_stops_before_reentry(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            producer = dict(phase='produce', pid=1, output='/original/produce')
            def failed_execute(_root, _binary, stage, *_args, **_kwargs):
                stage.mkdir()
                item = dict(status='FAIL', pid=2, error='original phase failure')
                check.native.fresh_json(stage / 'phase.json', item)
                return item
            completed = []
            with patch.object(check.native, 'execute_phase', side_effect=failed_execute) as execute:
                workers, failed = check.run_phases(root, root / 'elf', root, root / 'db', {}, producer, {}, lambda: 10, completed)
            self.assertEqual(execute.call_count, 1)
            self.assertEqual(completed, [])
            self.assertEqual(failed['phase'], 'recover')
            self.assertEqual(failed['error'], 'original phase failure')
            self.assertEqual(workers[0]['status'], 'FAIL')

    def test_ancestor_and_original_failed_database_are_immutable(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            binary, receipt = root / 'engine-tests', root / 'result.json'
            binary.write_bytes(b'original ELF bytes')
            db = root / 'database'
            db.mkdir()
            (db / 'home').write_bytes(b'preserved failed DB')
            write(root / 'produce/phase.json', b'original producer receipt')
            write(root / 'produce/stdout.log', b'original producer stdout')
            identity = {'binary_sha256': check.native.digest(binary)}
            inventory = check.native.prior.inventory(db)
            report = dict(status='FAIL', identity=identity, database=str(db), origin=None,
                          retention={'files': check.native.file_receipts(root, (binary, receipt, db))})
            check.native.fresh_json(receipt, report)
            receipt_sha = check.native.digest(receipt)
            check.original_unchanged(receipt, identity, inventory, receipt_sha)
            usage = check.immutable_origin_usage(receipt)
            self.assertEqual(usage['bytes'], sum(row['bytes'] for row in report['retention']['files'].values()))
            self.assertEqual(usage['log_bytes'], len(b'original producer stdout'))
            for path in (root / 'produce/phase.json', root / 'produce/stdout.log', db / 'home'):
                original = path.read_bytes()
                path.write_bytes(original + b' tampered')
                with self.subTest(path=path.name), self.assertRaises(ValueError):
                    check.original_unchanged(receipt, identity, inventory, receipt_sha)
                path.write_bytes(original)
            receipt.write_text('{}')
            with self.assertRaisesRegex(ValueError, 'FAIL_ORIGINAL_RESULT_DRIFT'):
                check.original_unchanged(receipt, identity, inventory, receipt_sha)

    def test_final_snapshot_reuse_and_aggregate_cap_do_not_reset_history(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            db, disk = root / 'database', root / 'reentry/disk'
            db.mkdir()
            disk.mkdir(parents=True)
            (db / 'home').write_bytes(b'final home')
            (disk / 'home').write_bytes(b'final home')
            binary = root / 'engine-tests'
            binary.write_bytes(b'ELF outside cap')
            report = {}
            check.retain_without_duplicate(root, binary, db, root / 'absent', 100, report)
            self.assertFalse((root / 'retained-database').exists())
            self.assertEqual(report['retained_database']['path'], str(disk))
            self.assertEqual(report['retention']['aggregate_bytes'], 100 + len(b'final home'))
            with self.assertRaisesRegex(ValueError, 'FAIL_HISTORY_RETAINED_SPACE'):
                check.retain_without_duplicate(root, binary, db, root / 'absent', check.native.RETAINED_LIMIT, {})
            write(root / 'reentry/stdout.log', b'new worker stdout')
            report = {}
            check.retain_without_duplicate(root, binary, db, root / 'absent', 100, report, old_log_bytes=50)
            self.assertEqual(report['retention']['aggregate_log_bytes'], 50 + len(b'new worker stdout'))
            with self.assertRaisesRegex(ValueError, 'FAIL_HISTORY_LOG_SPACE'):
                check.retain_without_duplicate(root, binary, db, root / 'absent', 100, {}, old_log_bytes=check.native.LOG_LIMIT)

    def test_approved_producer_env_preserves_old_identity_and_rejects_produce(self):
        class Child:
            pid, returncode = 52, 101
            def __enter__(self):
                return self
            def __exit__(self, *_args):
                return False
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            db = root / 'database'
            db.mkdir()
            identity = dict(source_sha256='a' * 64, binary_sha256='b' * 64)
            old = dict(source_sha256='c' * 64, binary_sha256='d' * 64)
            producer = dict(phase='produce', pid=41, output='/actual/original/produce')
            with patch.object(check.native.subprocess, 'Popen', return_value=Child()) as spawn, \
                    patch.object(check.native.prior, 'finish_child'):
                item = check.native.execute_phase(root, root / 'elf', root / 'recover', db, 'recover',
                                                  identity, [producer], lambda: 10, producer_identity=old)
            self.assertEqual(item['status'], 'FAIL')
            env = spawn.call_args.kwargs['env']
            self.assertEqual(env['XADB_MIXED_OWNER_SOURCE_SHA256'], identity['source_sha256'])
            self.assertEqual(env['XADB_MIXED_OWNER_APPROVED_PRODUCER_SOURCE_SHA256'], old['source_sha256'])
            self.assertEqual(env['XADB_MIXED_OWNER_APPROVED_PRODUCER_ELF_SHA256'], old['binary_sha256'])
            self.assertEqual(env['XADB_MIXED_OWNER_NATIVE_PRODUCER_OUT'], producer['output'])
            with self.assertRaisesRegex(ValueError, 'FAIL_APPROVED_PRODUCER_IDENTITY'):
                check.native.execute_phase(root, root / 'elf', root / 'produce', db, 'produce',
                                           identity, [], lambda: 10, producer_identity=old)
            self.assertFalse((root / 'produce').exists())


if __name__ == '__main__':
    unittest.main()
