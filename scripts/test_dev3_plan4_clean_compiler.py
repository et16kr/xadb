"""Four G5 compiler-runner checks; never invoke Cargo, rustc, or product code."""
from contextlib import ExitStack
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import dev3_plan4_clean_compiler as runner


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value) + '\n')


def fingerprint(root):
    digest = hashlib.sha256()
    for path in sorted(Path(root).rglob('*')):
        if path.is_file():
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()


def messages(row, code=None):
    if row['expected_code'] is None:
        return [{'reason': 'build-finished', 'success': True}]
    primary = [{'file_name': runner.PROBE, 'is_primary': True, **span}
               for span in row['primary_spans']]
    return [{'reason': 'compiler-message', 'message': {
        'level': 'error', 'code': {'code': code or row['expected_code']}, 'spans': primary}},
        {'reason': 'build-finished', 'success': False}]


def lines(rows):
    return ''.join(json.dumps(row) + '\n' for row in rows)


def prior_fixture(root, identifier='CP01', wrong_code=None):
    """Synthetic runner records with real frozen probe bytes; no compiler work."""
    history, revision = root / 'history', root / 'revision'
    session, source = history / 'first', revision / 'source'
    session.mkdir(parents=True)
    source.mkdir(parents=True)
    shutil.copytree(runner.ROOT / runner.INPUT, source / runner.INPUT)
    (revision / 'build-receipt.json').write_text('{}\n')
    binding = source / 'scripts/compiler-fixture.py'
    binding.parent.mkdir()
    binding.write_text('# inert synthetic runner identity\n')
    budget = root / 'budget.json'
    budget.write_text('{}\n')
    identity = ['a' * 64, 'b' * 64, 'c' * 64]
    manifest = json.loads((source / runner.INPUT / 'manifest.json').read_text())
    frozen = next(row for row in manifest['cases'] if row['id'] == identifier)
    start = {'schema': runner.SCHEMA, 'identity': identity, 'selected': [identifier],
             'revision': str(revision), 'compiler_manifest_sha256': runner.sha(source / runner.INPUT / 'manifest.json'),
             'build_receipt_sha256': runner.sha(revision / 'build-receipt.json'),
             'runner_bindings': {'scripts/compiler-fixture.py': {'sha256': runner.sha(binding)}},
             'budget_path': str(budget), 'budget_sha256': runner.sha(budget)}
    write_json(session / 'run.json', start)
    evidence = session / identifier
    evidence.mkdir()
    overlay = evidence / 'source'
    probe = overlay / runner.PROBE
    probe.parent.mkdir(parents=True)
    probe.write_bytes((source / runner.INPUT / frozen['path']).read_bytes())
    stdout, stderr, patch_path = (evidence / name for name in ('compiler.jsonl', 'compiler.stderr', 'probe.patch'))
    stdout.write_text(lines(messages(frozen, wrong_code)))
    stderr.write_text('synthetic compiler capture\n')
    patch_path.write_text('synthetic overlay patch\n')
    metadata = evidence / 'engine.rmeta'
    metadata.write_bytes(b'synthetic metadata, not a product binary')
    row = {'id': identifier, 'status': 'FAIL' if wrong_code else 'PASS',
           'expectation': frozen, 'source': str(overlay), 'source_sha256': fingerprint(overlay),
           'probe_sha256': runner.sha(probe), 'stop_reason': None, 'truncated': False,
           'exit_code': 0 if frozen['expected_code'] is None else 101,
           'metadata': [{'target': 'xadb_engine', 'retained_path': str(metadata), 'sha256': runner.sha(metadata)}]}
    for key, path in [('stdout', stdout), ('stderr', stderr), ('patch', patch_path)]:
        row[key], row[key + '_sha256'] = str(path), runner.sha(path)
    report = {'schema': runner.SCHEMA, 'run_sha256': runner.sha(session / 'run.json'),
              'identity': identity, 'selected': [identifier], 'status': 'FAIL' if wrong_code else 'PASS',
              'cases': [row], 'compiler_elapsed_seconds': 0.005}
    lock = root / 'lock'
    lock.write_text('synthetic lock\n')
    with patch.object(runner.time, 'monotonic', side_effect=[0.01, 0.02]):
        runner.finalize_report(session, lock, report, 0.0, 0.0, 0.0, 0,
                               stdout.stat().st_size + stderr.stat().st_size)
    return history, source, identity, session, start, report


def prior_mocks(source, identity):
    stack = ExitStack()
    stack.enter_context(patch.object(runner.case, 'validate_build',
                                    return_value=(source, None, None, {}, tuple(identity))))
    stack.enter_context(patch.object(runner.common, 'source_fingerprint', side_effect=fingerprint))
    return stack


class G5CompilerHarness(unittest.TestCase):
    def test_h09_exact_diagnostic_contract(self):
        """G5-H09: code, one-error count, exact file and every primary span."""
        manifest = json.loads((runner.ROOT / runner.INPUT / 'manifest.json').read_text())
        for frozen in manifest['cases']:
            with self.subTest(identifier=frozen['id']):
                program = (runner.ROOT / runner.INPUT / frozen['path']).read_text()
                self.assertEqual(runner.diagnostic_spans(program, frozen['id']),
                                 (frozen['expected_code'], frozen['primary_spans']))
                rows = messages(frozen)
                code = 0 if frozen['expected_code'] is None else 101
                runner.verify(lines(rows), code, frozen)
                for changed, returncode in [(rows, 1), (rows[:-1], code), (rows + [rows[-1]], code)]:
                    with self.assertRaises(ValueError):
                        runner.verify(lines(changed), returncode, frozen)
                if frozen['expected_code'] is None:
                    error = {'reason': 'compiler-message', 'message': {'level': 'error'}}
                    with self.assertRaises(ValueError):
                        runner.verify(lines([error, *rows]), code, frozen)
                    continue
                changed = copy.deepcopy(rows)
                changed[0]['message']['code']['code'] = 'E0000'
                with self.assertRaisesRegex(ValueError, 'code'):
                    runner.verify(lines(changed), code, frozen)
                with self.assertRaisesRegex(ValueError, 'one completed'):
                    runner.verify(lines([rows[0], *rows]), code, frozen)
                for index in range(len(frozen['primary_spans'])):
                    for key in ('file_name', 'line_start', 'line_end', 'column_start', 'column_end', 'is_primary'):
                        changed = copy.deepcopy(rows)
                        span = changed[0]['message']['spans'][index]
                        span[key] = ('wrong.rs' if key == 'file_name' else False if key == 'is_primary' else span[key] + 1)
                        with self.assertRaisesRegex(ValueError, 'span'):
                            runner.verify(lines(changed), code, frozen)

    def test_h10_overlay_unlinks_anchor(self):
        """G5-H10: hard-linked immutable baseline survives anchor injection."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source, destination, evidence = root / 'source', root / 'overlay', root / 'evidence'
            anchor = source / runner.ANCHOR
            anchor.parent.mkdir(parents=True)
            original = '// original source\n'
            anchor.write_text(original)
            sibling = anchor.parent / 'untouched.rs'
            sibling.write_text('// untouched\n')
            (source / 'Cargo.toml').write_text('[workspace]\n')
            evidence.mkdir()
            before, inode = fingerprint(source), anchor.stat().st_ino
            with patch.object(runner, 'SOURCE_PATHS', ['crates', 'Cargo.toml']), \
                    patch.object(runner.common, 'source_fingerprint', side_effect=fingerprint):
                runner.clone_overlay(source, destination, '// inert probe\n', evidence)
            self.assertEqual(fingerprint(source), before)
            self.assertEqual(anchor.stat().st_ino, inode)
            self.assertEqual(anchor.read_text(), original)
            self.assertNotEqual((destination / runner.ANCHOR).stat().st_ino, inode)
            self.assertEqual((destination / runner.ANCHOR).read_text(), original + runner.DECLARATION)
            self.assertEqual((destination / sibling.relative_to(source)).stat().st_ino, sibling.stat().st_ino)
            self.assertEqual((destination / runner.PROBE).read_text(), '// inert probe\n')
            self.assertIn(runner.PROBE, (evidence / 'probe.patch').read_text())
            self.assertFalse((source / runner.PROBE).exists())

    def test_h11_completed_and_uncertain_history(self):
        """G5-H11: prior PASS excluded; completed/audit failure and uncertainty HOLD."""
        with tempfile.TemporaryDirectory() as temp:
            history, source, identity, _, _, _ = prior_fixture(Path(temp))
            with prior_mocks(source, identity):
                completed, references, elapsed, audit, output = runner.load_prior(history)
            self.assertEqual(completed, {'CP01'})
            self.assertEqual(len(references), 1)
            self.assertGreater(elapsed, 0)
            self.assertGreater(audit, 0)
            self.assertGreater(output, runner.RECEIPT_RESERVE)
            with self.assertRaisesRegex(ValueError, 'completed case replay'):
                runner.common.select_cases(runner.IDS, ['CP01'], None, completed)
            self.assertNotIn('CP01', runner.common.select_cases(runner.IDS, list(runner.IDS[1:]), None, completed))
        # A definite completed *unexpected* diagnostic remains a failed case,
        # unlike a correct diagnostic followed by a failed evidence audit.
        with tempfile.TemporaryDirectory() as temp:
            history, source, identity, _, _, _ = prior_fixture(Path(temp), 'CP03', 'E0308')
            with prior_mocks(source, identity):
                self.assertEqual(runner.load_prior(history)[0], set())
        for mutation in ('after_compile', 'metadata_after_compile', 'truncated', 'stopped',
                         'missing_completion', 'expectation', 'probe', 'metadata_drift',
                         'manifest_drift', 'unfinished', 'late_receipt'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temp:
                history, source, identity, session, start, report = prior_fixture(Path(temp))
                row = report['cases'][0]
                if mutation in ('after_compile', 'metadata_after_compile'):
                    row['status'], report['status'], report['passed'] = 'FAIL', 'FAIL', []
                    if mutation == 'metadata_after_compile':
                        row['metadata_error'] = 'preservation failed after successful compile'
                elif mutation == 'truncated':
                    row['truncated'] = True
                elif mutation == 'stopped':
                    row['stop_reason'] = 'TIMEOUT'
                elif mutation == 'missing_completion':
                    Path(row['stdout']).write_text('')
                    row['stdout_sha256'] = runner.sha(row['stdout'])
                elif mutation == 'expectation':
                    row['expectation'] = {**row['expectation'], 'expected_code': 'E0000'}
                elif mutation == 'probe':
                    row['probe_sha256'] = 'd' * 64
                elif mutation == 'metadata_drift':
                    Path(row['metadata'][0]['retained_path']).write_text('mutated')
                elif mutation == 'manifest_drift':
                    start['compiler_manifest_sha256'] = 'd' * 64
                    write_json(session / 'run.json', start)
                    report['run_sha256'] = runner.sha(session / 'run.json')
                elif mutation == 'late_receipt':
                    write_json(session / 'finalization-failure.json', {'status': 'HOLD'})
                write_json(session / 'result.json', report)
                if mutation == 'unfinished':
                    (session / 'result.json').unlink()
                with prior_mocks(source, identity), self.assertRaises(ValueError):
                    runner.load_prior(history)

    def test_h12_final_receipt_budget(self):
        """G5-H12: prior attempts, independent audit limit, final fsync and bytes."""
        self.assertEqual((runner.RECEIPT_RESERVE, runner.RECEIPT_SECONDS, runner.DIAGNOSTICS,
                          runner.SECONDS, runner.AUDIT_SECONDS, runner.TOTAL_SECONDS),
                         (131072, 0.05, 8 * 1024 * 1024, 120.0, 30.0, 150.0))
        self.assertGreater(runner.remaining_budget(119.0, 29.0, 0.25, 0.5), 0)
        for prior_execution, prior_audit, execution, wall in (
                (119.9, 0.0, 0.2, 0.2), (0.0, 29.9, 0.0, 0.2),
                (119.9, 29.9, 0.1, 0.2), (0.0, 0.0, 1.0, 0.5)):
            with self.assertRaises(ValueError):
                runner.remaining_budget(prior_execution, prior_audit, execution, wall)
        for mode in ('normal', 'receipt_91400', 'receipt_limit', 'late_write', 'oversized',
                     'prior_bytes', 'audit_limit', 'lock_failure'):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temp:
                session = Path(temp) / 'first'
                session.mkdir()
                write_json(session / 'run.json', {'schema': runner.SCHEMA})
                lock = session / 'lock'
                if mode != 'lock_failure':
                    lock.write_text('lease\n')
                report = {'schema': runner.SCHEMA, 'status': 'PASS', 'cases': [],
                          'run_sha256': runner.sha(session / 'run.json'), 'compiler_elapsed_seconds': 0.0}
                receipt_size = {'receipt_91400': 91400, 'receipt_limit': 131072,
                                'oversized': 131073}.get(mode)
                original_write = runner.fresh_json

                def write_sized_receipt(path, value):
                    if Path(path).name == 'result.json' and receipt_size is not None:
                        value = {**value, 'padding': ''}
                        encoded = json.dumps(value, ensure_ascii=False, indent=2) + '\n'
                        value['padding'] = 'x' * (receipt_size - len(encoded.encode('utf-8')))
                    original_write(path, value)

                clock = [1.0, 1.2] if mode == 'late_write' else [1.0, 1.01]
                if mode == 'audit_limit':
                    clock = [29.96, 29.97]
                prior_bytes = runner.DIAGNOSTICS if mode == 'prior_bytes' else 11
                with patch.object(runner.time, 'monotonic', side_effect=clock), \
                        patch.object(runner, 'fresh_json', side_effect=write_sized_receipt):
                    runner.finalize_report(session, lock, report, 0.0, 0.0, 0.0, prior_bytes, 7)
                saved = json.loads((session / 'result.json').read_text())
                if receipt_size is not None:
                    self.assertEqual((session / 'result.json').stat().st_size, receipt_size)
                self.assertFalse(lock.exists())
                self.assertEqual(saved['budgeted_output_bytes'],
                                 7 + (session / 'run.json').stat().st_size + runner.RECEIPT_RESERVE)
                self.assertAlmostEqual(saved['audit_elapsed_seconds'], clock[0] + runner.RECEIPT_SECONDS)
                if mode in ('normal', 'receipt_91400', 'receipt_limit'):
                    self.assertEqual(report['status'], 'PASS')
                    self.assertFalse((session / 'finalization-failure.json').exists())
                else:
                    self.assertEqual(report['status'], 'FAIL')
                    self.assertTrue(report['audit_hold'])
                if mode in ('late_write', 'oversized'):
                    # Original candidate receipt stays intact; marker wins at resume.
                    self.assertEqual(saved['status'], 'PASS')
                    marker = json.loads((session / 'finalization-failure.json').read_text())
                    self.assertEqual(marker['result_sha256'], runner.sha(session / 'result.json'))
                    with self.assertRaisesRegex(ValueError, 'finalization failure'):
                        runner.load_prior(session.parent)


if __name__ == '__main__':
    unittest.main()
