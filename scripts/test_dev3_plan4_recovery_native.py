"""New recovery native-controller negative cases; never starts an engine."""
import contextlib
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import dev3_plan4_recovery_native as native

IDENTITY = ('1' * 64, '2' * 64, '3' * 64)


def trace(phase):
    rows = []
    def event(operation, name, outcome, length=0):
        rows.append(f'{len(rows)}\t{operation}\t{name}\t0\t{length}\t{outcome}')
    def pair(operation, name, outcome='Ok(())', length=0):
        event(operation, name, 'ATTEMPT', length)
        event(operation, name, outcome, length)
    pair('acquire', 'LOCK', 'CREATED' if phase == 'produce' else 'EXISTING')
    if phase == 'produce':
        event('entropy', 'namespace', '0102030405060708090a0b0c0d0e0f10', 16)
    pair('open_meta', native.HOME, '1:2:16384')
    event('retain_file', native.HOME, 'OK')
    pair('read', native.HOME, 'OK:1:' + hashlib.sha256(b'x').hexdigest(), 1)
    pair('sync', native.HOME)
    event('close', native.HOME, 'OK')
    if phase != 'produce':
        epoch = 2 if phase == 'recover' else 3
        pair('create', f'd3-wal-{epoch:08x}', 'OK')
        pair('remove', f'd3-wal-{epoch - 1:08x}')
        pair('publish', 'd3-current-stage', 'CURRENT:Ok(())')
        pair('publish', 'd3-writer-stage', 'WRITER:Ok(())')
    event('drop_lease', 'LOCK', 'OK')
    event('drop_directory', 'namespace', 'OK')
    return '\n'.join(rows) + '\n'


def worker_files(output, phase, pid, identity=IDENTITY):
    case = native.CASES[phase]
    text = (f'running 1 test\nP4_RECOVERY_CONTEXT\t{case}\t' + '\t'.join(identity)
            + f'\nP4_RECOVERY_BEGIN\t{case}\nP4_RECOVERY_NATIVE_COMPLETE\t{case}\t{pid}'
            + f'\nP4_RECOVERY_PASS\t{case}\ntest result: ok. 1 passed; 0 failed;\n')
    (output / 'stdout.log').write_text(text)
    (output / 'stderr.log').write_text('')
    effects = trace(phase)
    (output / 'effects.tsv').write_text(effects)
    (output / 'entropy.tsv').write_text(
        f'1\t{pid}\t0102030405060708090a0b0c0d0e0f10\n' if phase == 'produce' else '')
    counts = native.validate_effects(effects, phase)
    worker = {'case': case, 'phase': phase, 'pid': str(pid), 'handles': '0',
              'peak': str(counts['peak']), 'fd_peak': '3', 'events': str(counts['events']),
              'mutations': str(counts['mutations']), 'entropy': str(int(phase == 'produce')),
              'log_bytes': str(len(effects.encode())),
              **dict(zip(('source', 'elf', 'input'), identity))}
    (output / 'worker.tsv').write_text(''.join(f'{k}\t{v}\n' for k, v in worker.items()))
    return {'pid': pid, 'exact': True, 'exit_code': 0, 'stop_reason': None,
            'truncated': False, 'elapsed_seconds': 0.0,
            'parsed': native.core.parse_markers(text, native.SELECTOR, [case], identity)}


def input_value():
    return {'schema': native.INPUT_SCHEMA,
            'cases': {case: {'phase': phase, 'selector': native.SELECTOR}
                      for phase, case in native.CASES.items()},
            'budget': native.BUDGET.copy()}


def failed_prior(unit, retire=False):
    """A tiny complete failed receipt, with independent disk copy and source IDs."""
    revision = unit / 'old-build'
    prior = revision / 'native'
    phase = prior / 'produce'
    phase.mkdir(parents=True)
    (revision / 'source').mkdir()
    (revision / 'xadb-engine-tests').write_bytes(b'\x7fELFprior-fixture')
    build = {'exit_code': 0, 'source_sha256': IDENTITY[0],
             'binary_sha256': native.sha(revision / 'xadb-engine-tests')}
    (revision / 'build-receipt.json').write_text(json.dumps(build))
    build_hash = native.sha(revision / 'build-receipt.json')
    value = {'approved_source_sha256': build['source_sha256'],
             'approved_binary_sha256': build['binary_sha256'],
             'approved_build_receipt_sha256': build_hash}
    input_path = unit / 'prior-input.json'
    input_path.write_text(json.dumps(value))
    identity = [build['source_sha256'], build['binary_sha256'], native.sha(input_path)]
    stdout = ('P4_RECOVERY_CONTEXT\tP4-REC-N01\t' + '\t'.join(identity)
              + '\nP4_RECOVERY_BEGIN\tP4-REC-N01\nP4_RECOVERY_FAIL\tP4-REC-N01\n')
    (phase / 'stdout.log').write_text(stdout)
    (phase / 'stderr.log').write_text('original assertion failure\n')
    process = {'pid': 39, 'exact': False, 'exit_code': 101, 'stop_reason': None,
               'truncated': False, 'elapsed_seconds': 0.9,
               'parsed': native.core.parse_markers(stdout, native.SELECTOR, ['P4-REC-N01'], identity)}
    (phase / 'process.json').write_text(json.dumps(process))
    database = prior / 'database'
    database.mkdir()
    (database / 'LOCK').write_bytes(b'')
    (database / 'bytes').write_bytes(b'interrupted-bytes')
    working = native.snapshot_index(database, phase / 'disk')
    copied = native.snapshot_index(phase / 'disk')
    (phase / 'snapshot.json').write_text(json.dumps({'working': working, 'copy': copied}))
    (prior / 'planned.json').write_text(json.dumps({'schema': native.SCHEMA}))
    result = {'schema': native.SCHEMA, 'status': 'PARTIAL_OR_FAIL', 'finalized': True,
              'post_execution_verified': True, 'completed': {}, 'identity': identity,
              'budget': native.BUDGET.copy(), 'input_path': str(input_path), 'revision': str(revision),
              'build_receipt_sha256': build_hash,
              'attempts': [{'state': 'RETURNED', 'phase': 'produce', 'case': 'P4-REC-N01',
                            'process': process}], 'snapshot_indexes': {'produce': copied},
              'retained_database_bytes': working['bytes'] * 2,
              'execution_wall_seconds': 1.5, 'worker_seconds': 0.9, 'audit_seconds': 0.6}
    (prior / 'result.json').write_text(json.dumps(result))
    admission = {'schema': native.SUCCESSOR_SCHEMA, 'prior_run': 'old-build/native',
                 'prior_result_sha256': native.sha(prior / 'result.json'), 'prior_identity': identity,
                 'failed_ids': ['P4-REC-N01'], 'never_completed_ids': list(native.CASES.values()),
                 'retry_ids': list(native.CASES.values())}
    if retire:
        review = unit / 'retirement-review.json'
        review.write_text('{"status":"ACCEPTED"}')
        receipt = {'schema': native.RETIREMENT_SCHEMA, 'status': 'RETIRED', 'independent_review': 'ACCEPTED',
                   'prior_run': admission['prior_run'], 'prior_result_sha256': admission['prior_result_sha256'],
                   'comparison': 'ALL_FILES_STREAM_EQUAL', 'working_index': working,
                   'snapshot_index': copied, 'retired_bytes': working['bytes'],
                   'review_path': review.name, 'review_sha256': native.sha(review)}
        receipt_path = unit / 'retirement.json'
        receipt_path.write_text(json.dumps(receipt))
        for path in database.iterdir():
            path.unlink()
        database.rmdir()
        admission['retired_working_copy'] = {'receipt_path': receipt_path.name,
                                            'receipt_sha256': native.sha(receipt_path)}
    admission['preserved_file_bindings'] = native.file_bindings(prior)
    return prior, admission, result


class RecoveryNativeBoundaries(unittest.TestCase):
    def test_rejects_inventory_and_budget_changes(self):
        """HN01: an altered selector/count/budget cannot authorize a worker."""
        native.validate_inventory(input_value())
        changes = []
        for key, value in [('wall_seconds', 31), ('worker_seconds', 30),
                           ('audit_seconds', True), ('database_bytes', 1 << 30)]:
            item = input_value()
            item['budget'][key] = value
            changes.append(item)
        item = input_value()
        item['cases']['P4-REC-N02']['selector'] += '_other'
        changes.append(item)
        item = input_value()
        del item['cases']['P4-REC-N03']
        changes.append(item)
        for changed in changes:
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                native.validate_inventory(changed)

    def test_rejects_forged_completion_context_and_worker(self):
        """HN02: reached marker, actual process PID and all three hashes agree."""
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            process = worker_files(output, 'recover', 42)
            native.validate_phase(output, 'recover', IDENTITY, process)
            original = (output / 'stdout.log').read_text()
            for changed in (original.replace('\t42\n', '\t43\n'),
                            original.replace(IDENTITY[0], '4' * 64),
                            original.replace('P4_RECOVERY_NATIVE_COMPLETE', 'OBSERVATION'),
                            original + 'P4_RECOVERY_NATIVE_COMPLETE\tP4-REC-N02\t42\n',
                            original.replace('P4_RECOVERY_BEGIN', 'P4_BEGIN')):
                (output / 'stdout.log').write_text(changed)
                with self.assertRaises(ValueError):
                    native.validate_phase(output, 'recover', IDENTITY, process)
            (output / 'stdout.log').write_text(original)
            worker = (output / 'worker.tsv').read_text()
            for changed in (worker.replace('pid\t42', 'pid\t43'),
                            worker.replace('handles\t0', 'handles\t1'),
                            worker.replace('fd_peak\t3', 'fd_peak\t7'), worker + 'pid\t42\n'):
                (output / 'worker.tsv').write_text(changed)
                with self.assertRaises(ValueError):
                    native.validate_phase(output, 'recover', IDENTITY, process)

    def test_rejects_wrong_entropy_and_unfinished_process(self):
        """HN03: native namespace seed and exact process termination stay separate."""
        native.validate_entropy('1\t41\t0102030405060708090a0b0c0d0e0f10\n', 'produce', 41)
        native.validate_entropy('', 'reentry', 43)
        for phase, text in [('produce', ''), ('produce', '1\t42\t0102030405060708090a0b0c0d0e0f10\n'),
                            ('recover', '1\t42\t0102030405060708090a0b0c0d0e0f10\n')]:
            with self.assertRaises(ValueError):
                native.validate_entropy(text, phase, 41 if phase == 'produce' else 42)
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            process = worker_files(output, 'produce', 41)
            for key, value in [('pid', True), ('exact', False), ('exit_code', 101),
                               ('truncated', True), ('stop_reason', 'TIMEOUT')]:
                with self.subTest(key=key), self.assertRaises(ValueError):
                    native.validate_phase(output, 'produce', IDENTITY, dict(process, **{key: value}))

    def test_rejects_forged_and_unpaired_effect_traces(self):
        """HN04 changed: only owned closes may follow writer wrapper release."""
        for phase in native.PHASES:
            native.validate_effects(trace(phase), phase)
        rows = [line.split('\t') for line in trace('produce').splitlines()]
        close = next(row for row in rows if row[1] == 'close')
        rows.remove(close)
        rows.insert(-1, close)
        def render(items):
            return '\n'.join('\t'.join([str(i)] + row[1:]) for i, row in enumerate(items)) + '\n'
        native.validate_effects(render(rows), 'produce')
        for operation in ('read', 'write', 'sync', 'open', 'retain_file', 'metadata', 'drop_lease'):
            changed = copy.deepcopy(rows)
            changed[-2][1] = operation
            with self.subTest(after_wrapper_drop=operation), self.assertRaises(ValueError):
                native.validate_effects(render(changed), 'produce')
        with self.assertRaises(ValueError):
            native.validate_effects(render(rows[:-2] + rows[-1:]), 'produce')
        original = trace('reentry')
        for changed in (original.replace('0\tacquire', '9\tacquire', 1),
                        original.replace('WRITER:Ok(())', 'WRITER:Err(Io)'),
                        original.replace('d3-wal-00000003', 'd3-wal-00000004'),
                        original.replace('\tread\t', '\twrite\t'),
                        original.replace('\tclose\t', '\tretain_file\t'),
                        original.replace('\tdrop_lease\t', '\tdrop_directory\t'),
                        original.replace('\tsync\t' + native.HOME + '\t0\t0\tOk(())',
                                         '\tsync\tother\t0\t0\tOk(())'),
                        original.replace('OK:1:', 'OK:2:', 1)):
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                native.validate_effects(changed, 'reentry')

    def test_snapshot_checks_full_bytes_and_real_file_identity(self):
        """HN05: all bytes copied, no hardlinks/symlinks or same-size substitutions."""
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            database, copied = root / 'database', root / 'copy'
            database.mkdir()
            (database / 'LOCK').write_bytes(b'')
            (database / 'data').write_bytes(bytes(65536) + b'last-byte')
            original = native.snapshot_index(database, copied)
            snapshot = native.snapshot_index(copied)
            self.assertNotEqual(original['files']['data']['inode'], snapshot['files']['data']['inode'])
            self.assertEqual(original['files']['data']['sha256'], snapshot['files']['data']['sha256'])
            native.snapshot_index(database, expected=original)
            with (database / 'data').open('r+b') as stream:
                stream.seek(-1, 2)
                stream.write(b'!')
            with self.assertRaises(ValueError): native.snapshot_index(database, expected=original)
            (database / 'data').unlink()
            os.link(copied / 'data', database / 'data')
            with self.assertRaises(ValueError): native.snapshot_index(database)
            (database / 'data').unlink()
            (database / 'data').symlink_to(copied / 'data')
            with self.assertRaises(ValueError): native.snapshot_index(database)
            (database / 'data').unlink()
            (database / 'data').write_bytes(bytes(65536) + b'last-byte')
            with self.assertRaises(ValueError): native.snapshot_index(database, expected=original)

    def test_previous_chain_blocks_fresh_path_and_unfinished_restart(self):
        """HN06: both completed and uncertain existing operations need an audit."""
        with tempfile.TemporaryDirectory() as temp:
            unit = Path(temp)
            native.no_previous_chain(unit)
            run = unit / 'old-build' / 'old-run'
            run.mkdir(parents=True)
            for state in ('INVOKING', 'RETURNED', 'VERIFIED'):
                (run / 'planned.json').write_text(json.dumps({'schema': native.SCHEMA,
                                                             'attempts': [{'state': state}]}))
                with self.assertRaises(ValueError): native.no_previous_chain(unit)
                with self.assertRaises(ValueError): native.failed_chain_history(unit, unit / 'new/native')

    def test_full_chain_preserves_each_snapshot_and_distinct_pid(self):
        """HN07: the actual controller loop invokes and audits all three phases."""
        self._chain(None)
        self._chain('reused_pid')

    def test_failed_suffix_preserves_original_completed_phase(self):
        """HN08: a failed second worker leaves N01 and its bytes intact; N03 stays unrun."""
        self._chain('failed_recover')

    def _chain(self, fault, successor=False):
        with tempfile.TemporaryDirectory() as temp:
            unit = Path(temp)
            revision = unit / 'build'
            revision.mkdir()
            output = revision / 'native'
            value = input_value()
            value['approved_build_receipt_sha256'] = '5' * 64
            prior = None
            if successor:
                prior, admission, old_result = failed_prior(unit, retire=True)
                value['failed_chain_successor'] = admission
            input_path = unit / 'native-input.json'
            input_path.write_text(json.dumps(value))
            verifier = Mock()
            invocations = []
            def invoke(command, source, env, stdout, stderr, selector, selected, identity,
                       timeout, case_timeout, output_limit):
                phase = env['XADB_PLAN4_RECOVERY_NATIVE_PHASE']
                invocations.append(phase)
                pid = 41 if fault == 'reused_pid' else 41 + native.PHASES.index(phase)
                process = worker_files(stdout.parent, phase, pid)
                database = Path(env['XADB_PLAN4_RECOVERY_NATIVE_DB'])
                (database / 'bytes').write_bytes(phase.encode())
                if fault == 'failed_recover' and phase == 'recover':
                    process.update(exact=False, exit_code=101)
                return process
            with patch.object(native, 'identity_verifier', return_value=(revision, revision / 'elf', IDENTITY, verifier)), \
                    patch.object(native.core, 'source_fingerprint', return_value=IDENTITY[0]), \
                    patch.object(native.core, 'invoke', side_effect=invoke), contextlib.redirect_stdout(io.StringIO()):
                code = native.execute_chain(revision, input_path, output)
            result = json.loads((output / 'result.json').read_text())
            self.assertTrue(result['post_execution_verified'])
            self.assertEqual(result['audit_budget_mode'], 'RESERVED_FINAL_AUDIT')
            self.assertAlmostEqual(result['worker_seconds'], result['invocation_worker_seconds'] + (0.9 if successor else 0))
            self.assertAlmostEqual(result['execution_wall_seconds'], result['invocation_wall_seconds'] + (1.5 if successor else 0))
            self.assertEqual(invocations, list(native.PHASES if fault is None else native.PHASES[:2]))
            self.assertEqual(code, 0 if fault is None else 1)
            self.assertEqual(set(result['completed']), set(native.PHASES if fault is None else ['produce']))
            self.assertEqual((output / 'produce/disk/bytes').read_bytes(), b'produce')
            if fault == 'failed_recover':
                self.assertEqual((output / 'recover/disk/bytes').read_bytes(), b'recover')
            if successor:
                self.assertEqual(json.loads((prior / 'result.json').read_text()), old_result)
                self.assertEqual((prior / 'produce/disk/bytes').read_bytes(), b'interrupted-bytes')
                self.assertEqual(result['history']['retired_duplicate_bytes'], len(b'interrupted-bytes'))
                self.assertEqual(result['retained_database_bytes'],
                                 native.output_totals(output)[1] + len(b'interrupted-bytes'))
                self.assertGreater(result['output_bytes_before_receipt'], result['history']['output_bytes'])
            with self.assertRaises(ValueError): native.no_previous_chain(unit)

    def test_budget_and_identity_gate_tampering_is_rejected(self):
        """HN09 changed: audit reserve, cumulative wall and same-build gate."""
        with patch.object(native.time, 'monotonic', return_value=2):
            self.assertEqual(native.allowance(0, 0), 27)
            self.assertEqual(native.allowance(0, 1, prior_wall=1.5), 25.5)
        with patch.object(native.time, 'monotonic', return_value=29):
            with self.assertRaises(ValueError): native.allowance(0, 0)
        with tempfile.TemporaryDirectory() as temp:
            unit = Path(temp)
            revision = unit / 'build'
            source = revision / 'source'
            source.mkdir(parents=True)
            binary = revision / 'xadb-engine-tests'
            binary.write_bytes(b'\x7fELFstatic-unit-fixture')
            for name in ('Cargo.lock', 'rust-toolchain.toml', 'owner.rs'):
                (source / name).write_text(name)
            (revision / 'build.log').write_text('build log')
            (unit / 'evidence.json').write_text('{}')
            build = {'exit_code': 0, 'engine_artifact_fresh': False,
                     'command': ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--all-features',
                                 '--locked', '--no-run', '--message-format=json'],
                     'source_sha256': IDENTITY[0], 'binary_sha256': native.sha(binary),
                     'cargo_lock_sha256': native.sha(source / 'Cargo.lock'),
                     'toolchain_file_sha256': native.sha(source / 'rust-toolchain.toml'),
                     'build_log_sha256': native.sha(revision / 'build.log'),
                     'owned_files': {'owner.rs': native.sha(source / 'owner.rs')}}
            (revision / 'build-receipt.json').write_text(json.dumps(build))
            value = input_value()
            value.update(approved_source_sha256=build['source_sha256'], approved_binary_sha256=build['binary_sha256'],
                         approved_build_receipt_sha256=native.sha(revision / 'build-receipt.json'),
                         source_input_bindings=build['owned_files'],
                         tool_bindings={str(Path(p).resolve().relative_to(native.ROOT)): native.sha(p)
                                        for p in (native.__file__, native.core.__file__)},
                         effect_admission_path='native-gate.json')
            gate = {key: value[key] for key in ('cases', 'budget', 'source_input_bindings', 'tool_bindings')}
            gate.update(schema=native.GATE_SCHEMA, status='ACCEPTED', f05='ACCEPTED', independent=True,
                        source_sha256=build['source_sha256'], binary_sha256=build['binary_sha256'],
                        build_receipt_sha256=value['approved_build_receipt_sha256'],
                        allowed_ids=list(native.CASES.values()),
                        evidence_bindings={'evidence.json': native.sha(unit / 'evidence.json')})
            (unit / 'native-gate.json').write_text(json.dumps(gate))
            value['effect_admission_sha256'] = native.sha(unit / 'native-gate.json')
            input_path = unit / 'input.json'
            input_path.write_text(json.dumps(value))
            _, _, _, verify = native.identity_verifier(revision, input_path, value)
            with patch.object(native.core, 'source_fingerprint', return_value=IDENTITY[0]):
                verify()
                gate['failed_chain_successor'] = {'unbound': True}
                (unit / 'native-gate.json').write_text(json.dumps(gate))
                value['effect_admission_sha256'] = native.sha(unit / 'native-gate.json')
                input_path.write_text(json.dumps(value))
                _, _, _, wrong_gate = native.identity_verifier(revision, input_path, value)
                with self.assertRaises(ValueError): wrong_gate()
                del gate['failed_chain_successor']
                (unit / 'native-gate.json').write_text(json.dumps(gate))
                value['effect_admission_sha256'] = native.sha(unit / 'native-gate.json')
                input_path.write_text(json.dumps(value))
                _, _, _, verify = native.identity_verifier(revision, input_path, value)
                (unit / 'evidence.json').write_text('{"changed":true}')
                with self.assertRaises(ValueError): verify()
                (unit / 'evidence.json').write_text('{}')
                (source / 'owner.rs').write_text('changed')
                with self.assertRaises(ValueError): verify()
                (source / 'owner.rs').write_text('owner.rs')
                (revision / 'build-receipt.json').write_text(json.dumps(dict(build, exit_code=1)))
                with self.assertRaises(ValueError): verify()

    def test_native_fd_six_is_accepted_and_seven_is_rejected(self):
        """HN10 new: original source contract permits six actual database FDs."""
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp)
            process = worker_files(output, 'produce', 41)
            original = (output / 'worker.tsv').read_text()
            (output / 'worker.tsv').write_text(original.replace('fd_peak\t3', 'fd_peak\t6'))
            row = native.validate_phase(output, 'produce', IDENTITY, process)
            self.assertEqual(row['worker']['fd_peak'], '6')
            (output / 'worker.tsv').write_text(original.replace('fd_peak\t3', 'fd_peak\t7'))
            with self.assertRaises(ValueError):
                native.validate_phase(output, 'produce', IDENTITY, process)

    def test_failed_successor_admission_rejects_uncertain_or_completed_history(self):
        """HN11 new: only one source-bound final FAIL with zero native PASS can restart."""
        variants = ('valid', 'completed', 'invoking', 'pid', 'stopped', 'truncated', 'pass',
                    'active', 'extra_chain', 'source', 'elf', 'manifest', 'result_hash', 'timing')
        for variant in variants:
            with self.subTest(variant=variant), tempfile.TemporaryDirectory() as temp:
                unit = Path(temp)
                prior, admission, result = failed_prior(unit)
                current = unit / 'next/native'
                source_hash = IDENTITY[0]
                process = result['attempts'][0]['process']
                if variant == 'completed': result['completed']['produce'] = {'pid': 39}
                elif variant == 'invoking': result['attempts'][0]['state'] = 'INVOKING'
                elif variant == 'pid': process['pid'] = True
                elif variant == 'stopped': process['stop_reason'] = 'TIMEOUT'
                elif variant == 'truncated': process['truncated'] = True
                elif variant == 'pass':
                    text = (prior / 'produce/stdout.log').read_text().replace('P4_RECOVERY_FAIL', 'P4_RECOVERY_PASS')
                    (prior / 'produce/stdout.log').write_text(text)
                    process['parsed'] = native.core.parse_markers(text, native.SELECTOR, ['P4-REC-N01'], result['identity'])
                elif variant == 'active': process['parsed']['active'] = 'P4-REC-N01'
                elif variant == 'extra_chain':
                    extra = unit / 'unaccounted/native'
                    extra.mkdir(parents=True)
                    (extra / 'planned.json').write_text(json.dumps({'schema': native.SCHEMA}))
                elif variant == 'source': source_hash = '9' * 64
                elif variant == 'elf': (prior.parent / 'xadb-engine-tests').write_bytes(b'changed')
                elif variant == 'timing': result['worker_seconds'] = -1
                (prior / 'result.json').write_text(json.dumps(result))
                (prior / 'produce/process.json').write_text(json.dumps(process))
                admission['prior_result_sha256'] = native.sha(prior / 'result.json')
                admission['preserved_file_bindings'] = native.file_bindings(prior)
                if variant == 'manifest': admission['preserved_file_bindings'].pop('produce/stderr.log')
                if variant == 'result_hash': admission['prior_result_sha256'] = 'f' * 64
                with patch.object(native.core, 'source_fingerprint', return_value=source_hash):
                    if variant == 'valid':
                        history = native.failed_chain_history(unit, current, admission)
                        self.assertEqual(history['wall_seconds'], 1.5)
                        self.assertEqual(history['worker_seconds'], 0.9)
                        self.assertEqual(history['retained_database_bytes'], 2 * len(b'interrupted-bytes'))
                    else:
                        with self.assertRaises((ValueError, OSError)):
                            native.failed_chain_history(unit, current, admission)

    def test_retirement_requires_independent_exact_duplicate_proof(self):
        """HN12 new: preserved whole snapshot and reviewed duplicate retirement stay bound."""
        for variant in ('valid', 'missing_receipt', 'wrong_bytes', 'wrong_comparison', 'wrong_snapshot',
                        'review_drift', 'disk_drift', 'working_reappears'):
            with self.subTest(variant=variant), tempfile.TemporaryDirectory() as temp:
                unit = Path(temp)
                prior, admission, result = failed_prior(unit, retire=True)
                binding = admission['retired_working_copy']
                receipt_path = unit / binding['receipt_path']
                receipt = json.loads(receipt_path.read_text())
                if variant == 'missing_receipt': del admission['retired_working_copy']
                elif variant == 'wrong_bytes': receipt['retired_bytes'] += 1
                elif variant == 'wrong_comparison': receipt['comparison'] = 'SAMPLE_EQUAL'
                elif variant == 'wrong_snapshot': receipt['snapshot_index']['files']['bytes']['sha256'] = 'f' * 64
                elif variant == 'review_drift': (unit / receipt['review_path']).write_text('changed')
                elif variant == 'disk_drift': (prior / 'produce/disk/bytes').write_bytes(b'changed')
                elif variant == 'working_reappears':
                    (prior / 'database').mkdir()
                    (prior / 'database/bytes').write_bytes(b'interrupted-bytes')
                    admission['preserved_file_bindings'] = native.file_bindings(prior)
                receipt_path.write_text(json.dumps(receipt))
                binding['receipt_sha256'] = native.sha(receipt_path)
                with patch.object(native.core, 'source_fingerprint', return_value=IDENTITY[0]):
                    if variant == 'valid':
                        history = native.failed_chain_history(unit, unit / 'next/native', admission)
                        self.assertEqual(history['retained_database_bytes'], len(b'interrupted-bytes'))
                        self.assertEqual(history['retired_duplicate_bytes'], len(b'interrupted-bytes'))
                        self.assertEqual(history['original_retained_database_bytes'], result['retained_database_bytes'])
                    else:
                        with self.assertRaises((ValueError, OSError)):
                            native.failed_chain_history(unit, unit / 'next/native', admission)

    def test_successor_chain_charges_original_failure_and_preserves_it(self):
        """HN13 new: new same-identity three-PID chain retains original failure costs/bytes."""
        self._chain(None, successor=True)
        self._chain('failed_recover', successor=True)

    def test_prior_costs_cannot_reset_worker_wall_or_audit_reserve(self):
        """HN14 new: audit beyond one second is charged; final one-second reserve persists."""
        with patch.object(native.time, 'monotonic', return_value=3):
            self.assertEqual(native.allowance(0, 1, prior_wall=2), 24)
            with self.assertRaises(ValueError): native.allowance(0, 29, prior_wall=27)
            with self.assertRaises(ValueError): native.allowance(0, 1, prior_wall=26)
            with self.assertRaises(ValueError): native.allowance(0, 6, prior_wall=2)


if __name__ == '__main__':
    unittest.main()
