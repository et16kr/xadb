"""Six mocked boundaries for the new plan4 case collector; no product runs."""
from pathlib import Path
import io
import json
import shutil
import tempfile
import unittest
from unittest.mock import Mock, patch

import dev3_plan4_genesis_cases as runner


class Plan4CaseRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.identity = ('a' * 64, 'b' * 64, 'c' * 64)

    def build(self):
        revision = self.root / 'r0'
        source = revision / 'source'
        (source / 'scripts').mkdir(parents=True)
        for directory in ('crates', 'tests', '.github'):
            (source / directory).mkdir()
        for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'):
            (source / name).write_text(name)
        shutil.copy2(Path(__file__).with_name('check_dev0.py'), source / 'scripts/check_dev0.py')
        owned = source / 'crates/owned.rs'
        owned.write_text('// synthetic checker source\n')
        fixture = source / 'tests/fixtures/dev3-plan4-genesis'
        fixture.mkdir(parents=True)
        runner.save(fixture / 'manifest.json', {'cases': {
            'P4-T01': {'phase': 'mandatory', 'category': 'memory', 'selector': 'fake::case'}}})
        (revision / 'xadb-engine-tests').write_bytes(b'\x7fELFsynthetic-checker-only')
        (revision / 'build.log').write_text('synthetic compiler receipt')
        build = {'exit_code': 0, 'command': ['cargo', 'test', '-p', 'xadb-engine', '--lib',
                 '--all-features', '--locked', '--no-run', '--message-format=json'],
                 'source_sha256': runner.source_fingerprint(source),
                 'toolchain_file_sha256': runner.sha(source / 'rust-toolchain.toml'),
                 'cargo_lock_sha256': runner.sha(source / 'Cargo.lock'),
                 'build_log_sha256': runner.sha(revision / 'build.log'),
                 'binary_sha256': runner.sha(revision / 'xadb-engine-tests'),
                 'manifest_sha256': runner.sha(fixture / 'manifest.json'),
                 'owned_files': {'crates/owned.rs': runner.sha(owned)}}
        runner.save(revision / 'build-receipt.json', build)
        return revision, build

    def receipt(self, name='prior', elapsed=1.0):
        output = self.root / name
        output.mkdir()
        raw = ('test fake::case ... P4_CONTEXT\tproducer\t' + '\t'.join(self.identity)
               + '\nP4_BEGIN\tproducer\nP4_PASS\tproducer\n')
        (output / '01.stdout').write_text(raw)
        (output / '01.stderr').write_text('')
        parsed = runner.parse_markers(raw, 'fake::case', ['producer'], self.identity)
        data = {'schema': runner.SCHEMA, 'category': 'memory', 'finalized': True,
                'execution_wall_seconds': elapsed, 'identity': self.identity,
                'status': 'PARTIAL_OR_FAIL', 'jobs': [{'selector': 'fake::case',
                'selected_ids': ['producer'], 'stdout': '01.stdout', 'stderr': '01.stderr',
                'stdout_sha256': runner.sha(output / '01.stdout'),
                'stderr_sha256': runner.sha(output / '01.stderr'), 'parsed': parsed}]}
        runner.save(output / 'result.json', data)
        return output, data

    def test_rejects_invalid_exact_selection(self):
        spawn = Mock()
        for include, exclude, prior in [('', None, []), ('x', None, []), ('a,a', None, []),
                                       ('a', 'a', []), ('a', None, ['a'])]:
            with self.assertRaises(ValueError):
                selected = runner.select_cases(['a', 'b'], include, exclude, prior)
                spawn(selected)
        spawn.assert_not_called()
        self.assertEqual(runner.select_cases(['a', 'b'], 'b', None, ['a']), {'b'})

    def test_rejects_source_or_build_drift(self):
        revision, original = self.build()
        runner.validate_build(revision)
        for key, changed in [('source_sha256', '0' * 64), ('command', ['cargo', 'test']),
                             ('toolchain_file_sha256', '0' * 64), ('exit_code', 101),
                             ('owned_files', {'crates/owned.rs': '0' * 64})]:
            runner.save(revision / 'build-receipt.json', {**original, key: changed})
            with self.assertRaises(ValueError):
                runner.validate_build(revision)
        runner.save(revision / 'build-receipt.json', original)
        (revision / 'source/crates/owned.rs').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'source drift'):
            runner.validate_build(revision)

    def test_rejects_elf_or_manifest_drift(self):
        revision, original = self.build()
        binary = revision / 'xadb-engine-tests'
        before = binary.read_bytes()
        binary.write_bytes(b'not-an-elf')
        with self.assertRaisesRegex(ValueError, 'ELF drift'):
            runner.validate_build(revision)
        runner.save(revision / 'build-receipt.json', {**original, 'binary_sha256': runner.sha(binary)})
        with self.assertRaisesRegex(ValueError, 'not an ELF'):
            runner.validate_build(revision)
        binary.write_bytes(before)
        runner.save(revision / 'build-receipt.json', {**original, 'manifest_sha256': '0' * 64})
        with self.assertRaisesRegex(ValueError, 'manifest drift'):
            runner.validate_build(revision)

    def test_does_not_replay_completed_phase(self):
        self.receipt()
        passed, _, _, _ = runner.load_prior(self.root, 'memory')
        self.assertEqual(passed, {'producer'})
        with self.assertRaises(ValueError):
            runner.select_cases(['producer', 'select', 'reentry'], 'producer', None, passed)
        selected = runner.select_cases(['producer', 'select', 'reentry'], None, list(passed), passed)
        worker = Mock()
        for phase in ('producer', 'select', 'reentry'):
            if phase in selected:
                worker(phase)
        self.assertEqual([call.args[0] for call in worker.call_args_list], ['select', 'reentry'])

    def test_preserves_failed_inputs_and_partial_status(self):
        output, _ = self.receipt()
        original = (output / '01.stdout').read_bytes()
        (output / 'unindexed.stdout').write_text('P4_PASS\tselect\n')
        with self.assertRaisesRegex(ValueError, 'unindexed'):
            runner.load_prior(self.root, 'memory')
        self.assertEqual((output / '01.stdout').read_bytes(), original)
        # Collector interruption after a PASS must leave a pending indexed job.
        revision, _ = self.build()
        run = revision / 'memory-a'
        def interrupted(*args):
            Path(args[3]).write_text('P4_BEGIN\tP4-T01\nP4_PASS\tP4-T01\n')
            Path(args[4]).write_text('collector interrupted after product output')
            raise OSError('injected collector error')
        with patch.object(runner, 'load_prior', return_value=(set(), [], 0, 0)), \
             patch.object(runner, 'invoke', side_effect=interrupted), \
             patch('sys.argv', ['collector', str(revision), 'memory', '--output', str(run)]):
            with self.assertRaises(ValueError):
                runner.main()
        pending = json.loads((run / 'result.json').read_text())
        self.assertFalse(pending['finalized'])
        self.assertEqual(pending['jobs'][0]['state'], 'INVOKING')
        self.assertEqual(pending['status'], 'PARTIAL_OR_FAIL')
        with self.assertRaisesRegex(ValueError, 'unfinished'):
            runner.load_prior(revision, 'memory')
        # A low-level collector exception still terminates/reaps its own group.
        child = Mock(pid=43210, stdout=io.BytesIO(), stderr=io.BytesIO())
        with patch.object(runner.subprocess, 'Popen', return_value=child), \
             patch.object(runner.selectors, 'DefaultSelector', side_effect=OSError('collector')), \
             patch.object(runner.os, 'killpg') as kill:
            with self.assertRaises(OSError):
                runner.invoke(['fake'], self.root, {}, self.root / 'partial.stdout',
                              self.root / 'partial.stderr', 'fake', ['x'], None, 1, 1, 1024)
            kill.assert_called_once_with(43210, runner.signal.SIGKILL)
            child.wait.assert_called_once()

    def test_enforces_aggregate_budgets(self):
        self.receipt(elapsed=179.0)
        _, _, elapsed, retained = runner.load_prior(self.root, 'memory')
        self.assertGreater(retained, 0)
        self.assertEqual(runner.budget_remaining(elapsed, 0.5, 180.0), 0.5)
        for current in (1.0, 2.0):
            with self.assertRaises(ValueError):
                runner.budget_remaining(elapsed, current, 180.0)
        raw = 'P4_BEGIN\ta\nP4_PASS\ta\nP4_BEGIN\tb\nP4_PASS\ta\n'
        parsed = runner.parse_markers(raw, 'fake', ['a', 'b'])
        self.assertEqual(parsed['markers']['PASS'], ['a'])
        self.assertIsNotNone(parsed['parser_error'])

    def test_requires_audit_for_uncertain_output(self):
        original_root = self.root
        for label in ('truncated', 'malformed'):
            with self.subTest(label=label):
                self.root = original_root / label
                self.root.mkdir()
                output, receipt = self.receipt()
                job = receipt['jobs'][0]
                if label == 'truncated':
                    job['truncated'] = True
                else:
                    with (output / '01.stdout').open('a') as log:
                        log.write('P4_PASS\tproducer\nP4_BEGIN\tunknown\n')
                    job['stdout_sha256'] = runner.sha(output / '01.stdout')
                    job['parsed'] = runner.parse_markers(
                        (output / '01.stdout').read_text(), job['selector'],
                        job['selected_ids'], receipt['identity'])
                runner.save(output / 'result.json', receipt)
                originals = {p.name: p.read_bytes() for p in output.iterdir()}
                with self.assertRaisesRegex(ValueError, 'uncertain prior output'):
                    runner.load_prior(self.root, 'memory')
                self.assertEqual(job['parsed']['markers']['PASS'], ['producer'])
                self.assertEqual(originals, {p.name: p.read_bytes() for p in output.iterdir()})
        self.root = original_root


if __name__ == '__main__':
    unittest.main()
