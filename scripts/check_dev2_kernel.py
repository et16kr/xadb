#!/usr/bin/env python3
"""Freeze, build and verify the DEV2 kernel correctness leaf, not the DEV2 stage.

Native fault cells use three fresh process campaigns. Source, test harness,
worker, pre-run plan and actual executions remain bound for the external oracle.
Performance, complete V3/bounded exploration and lower gates stay separate.
"""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from check_dev0 import SOURCE_PATHS, check_report_paths, source_fingerprint, write_report
from check_dev2_extended import digest, read_receipt, run_command, unique_fields

ROOT = Path(__file__).resolve().parent.parent
NATIVE_TESTS = (
    'control_from_another_session_never_qualifies_or_cleans_up_its_process',
    'every_declared_error_cut_has_a_flushed_reached_and_release_receipt',
    'partial_write_continue_returns_short_prefix_then_real_commit_ack',
    'synced_unacknowledged_commit_survives_kill_and_recovery_sync_refault',
)
KERNEL_TESTS = (
    'backup_each_target_cut_preserves_source_and_only_complete_archives_restore',
    'bootstrap_each_boundary_is_incomplete_until_the_final_selector_is_durable',
    'checkpoint_every_mutating_boundary_preserves_the_selected_ack_closure',
    'checkpoint_pins_exact_root_while_commit_rotation_and_gc_progress',
    'commit_write_sync_cuts_preserve_atomicity_and_known_ack_witnesses',
    'corrupt_current_and_writer_never_fall_back_or_modify_the_source_image',
    'every_partial_commit_prefix_is_ignored_without_truncating_original_bytes',
    'every_stabilize_resume_and_writer_cut_blocks_ready_and_preserves_the_fixed_prefix',
    'fallback_reselection_every_cut_and_following_checkpoint_never_reselects_damage',
    'full_frame_corruption_and_missing_anchor_are_explicit_errors',
    'gc_each_unlink_and_directory_sync_cut_preserves_both_selected_closures',
    'gc_keeps_fallback_chain_and_stops_before_anchor_predecessor_after_reclaim',
    'restart_stabilizes_adopted_unsynced_t1_before_acknowledging_t2_and_cache_loss',
    'restore_each_target_cut_rejects_incomplete_selection_and_preserves_exact_cut',
    'rotation_seal_header_writer_and_commit_cuts_keep_the_predeclared_atomic_prefix',
)


def utc():
    return datetime.now(timezone.utc).isoformat()


def binding(path):
    path = Path(path).resolve()
    return {'path': str(path), 'sha256': digest(path)}


def test_inventory(output):
    names = [line[:-6] for line in output.splitlines() if line.endswith(': test')]
    if not names or len(names) != len(set(names)):
        raise ValueError('missing or duplicate test inventory')
    return set(names)


def require_test_summary(output, expected, ignored=()):
    if not expected:
        raise ValueError('zero required tests')
    started = [name for name, tail in re.findall(r'^test (\S+) \.\.\. (.*)$', output, re.MULTILINE)
               if not tail.startswith('ignored')]
    if set(started) != expected or len(started) != len(expected):
        raise ValueError('required test inventory was not executed exactly once')
    skipped = [name for name, tail in re.findall(r'^test (\S+) \.\.\. (.*)$', output, re.MULTILINE)
               if tail.startswith('ignored')]
    if set(skipped) != set(ignored) or len(skipped) != len(ignored):
        raise ValueError('undeclared ignored test inventory')
    summaries = re.findall(r'^test result: ok\. (\d+) passed; 0 failed; (\d+) ignored; 0 measured; (\d+) filtered out; finished in [0-9.]+s$', output, re.MULTILINE)
    if (len(summaries) != 1 or int(summaries[0][0]) != len(expected)
            or int(summaries[0][1]) != len(ignored) or int(summaries[0][2]) != 0):
        raise ValueError('test summary does not match required completed inventory')


def require_python_tests(output, minimum=75):
    counts = re.findall(r'^Ran (\d+) tests in [0-9.]+s$', output, re.MULTILINE)
    if len(counts) != 1 or int(counts[0]) < minimum or output.splitlines().count('OK') != 1:
        raise ValueError('independent oracle/bridge self-test inventory is incomplete')


def require_kernel_thread_receipts(output, elapsed_seconds):
    profile = read_receipt(output, 'XADB_KERNEL_THREAD_PROFILE ')
    result = read_receipt(output, 'XADB_KERNEL_THREAD_RESULT ')
    expected_profile = {'generator': 'dev2-durable-thread-v1', 'seed_count': 64,
                        'writer_attempts_per_seed': 128, 'readers': 4, 'writers': 2,
                        'min_seconds': 0, 'power_loss': False}
    if profile != expected_profile or any(type(profile[key]) is not type(value) for key, value in expected_profile.items()):
        raise ValueError('kernel thread profile changed')
    if (result.get('generator') != profile['generator'] or result.get('rounds') != 64
            or result.get('commits') != 4096 or result.get('conflicts') != 4096
            or result.get('seed_visits') != [1] * 64
            or any(type(value) is not int for value in result['seed_visits'])):
        raise ValueError('kernel thread seed/work coverage is incomplete')
    fields = ('elapsed_ms', 'sampler_checks', 'sampler_overlap_begin', 'sampler_overlap_end',
              'sampler_overlap_both', 'reader_row_checks')
    if any(type(result.get(field)) is not int or result[field] < 0 for field in fields):
        raise ValueError('invalid kernel thread counter')
    checks, both = result['sampler_checks'], result['sampler_overlap_both']
    if (checks == 0 or both == 0
            or not both <= result['sampler_overlap_begin'] <= checks
            or not both <= result['sampler_overlap_end'] <= checks
            or result['reader_row_checks'] != 3 * 64 * 64 * 4 + 2 * checks
            or result['elapsed_ms'] > elapsed_seconds * 1000 + 1):
        raise ValueError('kernel sampler work or timing conservation failed')
    return {'profile': profile, 'result': result}


def require_oracle_summary(path, repetition):
    with path.open() as stream:
        summary = json.load(stream, object_pairs_hook=unique_fields)
    if not isinstance(summary, dict):
        raise ValueError('oracle summary is not an object')
    inventory = summary.get('campaign_inventory', {})
    negative = summary.get('controller_negative', [])
    if (not isinstance(inventory, dict) or not isinstance(negative, list)
            or any(not isinstance(item, dict) for item in negative)
            or not isinstance(summary.get('formal_execution'), dict)):
        raise ValueError('invalid oracle summary inventory shape')
    if (summary.get('cases') != 33
            or summary.get('verdict_counts') != {'ACCEPTED': 33, 'REJECTED': 0, 'INVALID': 0, 'INCOMPLETE': 0}
            or inventory.get('verdict') != 'COMPLETE'
            or inventory.get('expected_cases') != 33 or inventory.get('actual_cases') != 33
            or any(inventory.get(field) != [] for field in ('missing', 'unexpected', 'duplicate', 'case_errors'))
            or summary.get('run_inventory') != {'unreferenced': [], 'missing': [], 'reused': []}
            or summary.get('controller_negative_complete') is not True
            or len(negative) != 1 or negative[0].get('verdict') != 'ACCEPTED'
            or summary.get('formal_execution', {}).get('repetition') != repetition):
        raise ValueError('strict independent oracle campaign is incomplete')
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    check_report_paths([args.report])
    if platform.system() != 'Linux':
        parser.error('the declared native fault profile requires Linux')
    parent = ROOT / 'artifacts'
    parent.mkdir(exist_ok=True)
    # Keep Unix fault socket paths below sockaddr_un's fixed pathname bound.
    session = Path(tempfile.mkdtemp(prefix='d2k-', dir=parent)).resolve()
    source = session / 'source'
    binary_directory = session / 'binaries'
    binary_directory.mkdir()
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    environment.update(CARGO_TARGET_DIR=str(session / 'target'),
                       CARGO_PROFILE_RELEASE_OPT_LEVEL='3',
                       CARGO_PROFILE_RELEASE_LTO='false',
                       CARGO_PROFILE_RELEASE_CODEGEN_UNITS='16',
                       CARGO_PROFILE_RELEASE_INCREMENTAL='false',
                       CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS='true',
                       CARGO_PROFILE_RELEASE_DEBUG='2',
                       CARGO_PROFILE_RELEASE_STRIP='none')
    report = {'schema_version': 1, 'date': utc(), 'status': 'RUNNING',
              'scope': 'DEV2 retained-source kernel correctness leaf',
              'dev2_stage_status': 'INCOMPLETE', 'performance': 'NOT_RUN',
              'full_v3': 'NOT_RUN', 'bounded_exhaustive': 'NOT_RUN',
              'lower_regression': 'NOT_RUN', 'protected_ci': 'NOT_RUN',
              'artifact_directory': str(session), 'checks': []}
    retained_binaries = {}

    def binary_hashes():
        return {name: digest(Path(name)) for name in retained_binaries}

    def verify_binary_hashes(hashes):
        if hashes != retained_binaries:
            raise ValueError('retained executable changed across execution')

    def execute(name, argv, cwd, env, timeout):
        log = session / f'{name}.log'
        entry = {'id': name, 'command': list(map(str, argv)), 'cwd': str(cwd),
                 'status': 'FAIL', 'started_at': utc(), 'timeout_seconds': timeout}
        report['checks'].append(entry)
        print(f'{name}: {" ".join(map(str, argv))}', flush=True)
        try:
            entry['binary_sha256_before'] = binary_hashes()
            verify_binary_hashes(entry['binary_sha256_before'])
            entry.update(run_command(entry['command'], cwd, env, log, timeout))
            entry['binary_sha256_after'] = binary_hashes()
            verify_binary_hashes(entry['binary_sha256_after'])
            if entry['exit_code'] != 0 or entry['timed_out'] or entry['remaining_group_signalled']:
                raise RuntimeError(f'{name} failed; original log: {log}')
            entry['status'] = 'PASS'
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
            entry['error'] = f'{type(error).__name__}: {error}'
            raise
        finally:
            entry['finished_at'] = utc()
            # Spawn/wait/interrupt failure still retains its original command,
            # log and whatever post-state identity can actually be observed.
            for field, read in (('log', lambda: binding(log)),
                                ('binary_sha256_after', binary_hashes)):
                try:
                    entry[field] = read()
                except OSError as error:
                    entry[field + '_error'] = f'{type(error).__name__}: {error}'
        return entry, log

    def command(name, argv, cwd=source, extra=None, timeout=1200):
        env = environment.copy()
        if extra:
            env.update(extra)
        _, log = execute(name, argv, cwd, env, timeout)
        return log.read_text()

    def build(name, argv, wanted):
        output = command(name, argv)
        found = {}
        for line in output.splitlines():
            if not line.startswith('{'):
                continue
            item = json.loads(line)
            if item.get('reason') != 'compiler-artifact' or not item.get('executable'):
                continue
            key = (item['target']['name'], bool(item['profile']['test']))
            if key in wanted:
                if key in found:
                    raise ValueError(f'ambiguous build artifact: {key}')
                path = binary_directory / (item['target']['name'] + ('-test' if key[1] else ''))
                if path.exists():
                    raise ValueError(f'retained executable already exists: {path}')
                compiled_hash = digest(item['executable'])
                shutil.copy2(item['executable'], path)
                if digest(path) != compiled_hash or digest(item['executable']) != compiled_hash:
                    raise ValueError('compiled executable changed during retention')
                retained_binaries[str(path)] = compiled_hash
                found[key] = path
        if set(found) != set(wanted):
            raise ValueError(f'missing build artifacts: {set(wanted) - set(found)}')
        return found

    try:
        fingerprint = source_fingerprint(ROOT)
        source.mkdir()
        for name in SOURCE_PATHS:
            original = ROOT / name
            if original.is_dir():
                shutil.copytree(original, source / name)
            else:
                shutil.copy2(original, source / name)
        if source_fingerprint(source) != fingerprint or source_fingerprint(ROOT) != fingerprint:
            raise ValueError('source changed during snapshot creation')
        report.update(source_sha256=fingerprint, source_root=str(source))
        output = command('kernel-runner-self-tests', ['python3', '-B', '-m', 'unittest',
            'discover', '-s', 'scripts', '-p', 'test_check_dev2_kernel.py', '-v'])
        require_python_tests(output, minimum=8)
        report['toolchain'] = command('toolchain', ['rustc', '-Vv'])
        engine = build('build-engine', ['cargo', 'test', '-p', 'xadb-engine', '--lib',
            '--test', 'transaction', '--test', 'durable_recovery', '--release', '--locked',
            '--offline', '--no-default-features', '--no-run', '--message-format=json'],
            {('engine', True), ('transaction', True), ('durable_recovery', True)})
        worker = build('build-native', ['cargo', 'test', '-p', 'xadbtest-engine-worker',
            '--test', 'native_fault', '--test', 'kernel_stress', '--release', '--locked',
            '--offline', '--no-default-features', '--no-run', '--message-format=json'],
            {('native_fault', True), ('kernel_stress', True), ('xadbtest-durable-worker', False)})
        output = command('independent-oracle-self-tests', ['python3', '-B', '-m', 'unittest',
            'discover', '-s', 'scripts', '-p', 'test_dev2_durable*.py', '-v'])
        require_python_tests(output)
        for name, target in (('engine-units', engine[('engine', True)]),
                             ('transactions', engine[('transaction', True)])):
            inventory = test_inventory(command(name + '-inventory', [target, '--list']))
            ignored = ({'durable::codec_fuzz::disk_record_mutation_v3_30_minutes'}
                       if name == 'engine-units' else set())
            if not ignored <= inventory:
                raise ValueError('declared extended test is missing')
            output = command(name, [target, '--test-threads=1'])
            require_test_summary(output, inventory - ignored, ignored)
        output = command('cache-loss-cuts', [engine[('durable_recovery', True)], '--test-threads=1'])
        require_test_summary(output, set(KERNEL_TESTS))
        failure_directory = session / 'kernel-thread-failures'
        failure_directory.mkdir()
        output = command('kernel-threads-v2', [worker[('kernel_stress', True)], '--test-threads=1', '--nocapture'],
                         extra={'XADBTEST_V3_FAILURE_DIR': str(failure_directory)})
        require_test_summary(output, {'durable_two_writers_four_readers_v2_64_seeds'},
                             {'durable_two_writers_four_readers_v3_30_minutes'})
        report['checks'][-1]['thread_receipts'] = require_kernel_thread_receipts(
            output, report['checks'][-1]['elapsed_seconds'])
        harness = worker[('native_fault', True)]
        native = worker[('xadbtest-durable-worker', False)]
        listing = command('native-inventory', [harness, '--list'])
        if test_inventory(listing) != set(NATIVE_TESTS):
            raise ValueError('native harness test inventory differs from the frozen four cases')
        campaigns = []
        for repetition in range(3):
            artifacts = session / f'native-{repetition}'
            artifacts.mkdir()
            campaigns.append({'repetition': repetition, 'artifacts': str(artifacts),
                'command': [str(harness), '--test-threads=1', '--nocapture'],
                'environment': {'XADBTEST_DURABLE_WORKER_BIN': str(native),
                                'XADBTEST_NATIVE_ARTIFACTS': str(artifacts)}})
        manifest = {'schema': 1, 'profile': 'dev2-native-fault-v1', 'created_at': utc(),
            'source_root': str(source), 'source_sha256': fingerprint, 'harness': binding(harness),
            'worker': binding(native), 'oracle': binding(source / 'scripts/dev2_durable_oracle.py'),
            'bridge': binding(source / 'scripts/dev2_durable_bridge.py'), 'campaigns': campaigns,
            'planned_cases_per_campaign': 33, 'tests': list(NATIVE_TESTS)}
        manifest_path = session / 'pre-run-manifest.json'
        write_report(manifest_path, manifest)
        for campaign in campaigns:
            repetition = campaign['repetition']
            evidence = {'manifest_sha256': digest(manifest_path), 'command': campaign['command'],
                'environment': campaign['environment'], 'started_at': utc(),
                'harness_sha256_before': digest(harness), 'worker_sha256_before': digest(native),
                'source_sha256_before': source_fingerprint(source)}
            began = time.monotonic()
            log = session / f'native-{repetition}.log'
            env = {**environment, **campaign['environment']}
            execution_path = session / f'execution-{repetition}.json'
            result = {}
            try:
                if evidence['source_sha256_before'] != fingerprint:
                    raise ValueError('frozen source changed before a native campaign')
                result, log = execute(f'native-{repetition}', campaign['command'], source, env, 1200)
                require_test_summary(log.read_text(), set(NATIVE_TESTS))
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
                evidence['error'] = f'{type(error).__name__}: {error}'
                raise
            finally:
                if not result and report['checks'][-1]['id'] == f'native-{repetition}':
                    result = report['checks'][-1]
                evidence.update(finished_at=utc(), elapsed_seconds=time.monotonic() - began,
                    exit_code=result.get('exit_code'), timed_out=result.get('timed_out'),
                    remaining_group_signalled=result.get('remaining_group_signalled'))
                for field, read in (('log', lambda: binding(log)),
                                    ('harness_sha256_after', lambda: digest(harness)),
                                    ('worker_sha256_after', lambda: digest(native)),
                                    ('source_sha256_after', lambda: source_fingerprint(source))):
                    try:
                        evidence[field] = read()
                    except OSError as error:
                        evidence[field + '_error'] = f'{type(error).__name__}: {error}'
                write_report(execution_path, evidence)
                if result:
                    result['execution'] = binding(execution_path)
            command(f'oracle-{repetition}', ['python3', '-B', 'scripts/dev2_durable_bridge.py',
                '--artifacts', campaign['artifacts'], '--output', session / f'oracle-{repetition}',
                '--manifest', manifest_path, '--execution', execution_path])
            summary_path = session / f'oracle-{repetition}' / 'summary.json'
            require_oracle_summary(summary_path, repetition)
            report['checks'][-1]['strict_oracle_summary'] = binding(summary_path)
        report['retained_source_sha256_after'] = source_fingerprint(source)
        report['live_source_sha256_after'] = source_fingerprint(ROOT)
        if report['retained_source_sha256_after'] != fingerprint:
            raise ValueError('retained source changed during execution')
        report['current_source_matches'] = report['live_source_sha256_after'] == fingerprint
        report['status'] = 'PASS'
        verify_binary_hashes(binary_hashes())
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report.update(status='FAIL', error=str(error))
        print(f'FAIL: {error}', flush=True)
    finally:
        report['retained_binary_sha256_before'] = retained_binaries
        try:
            report['retained_binary_sha256_after'] = binary_hashes()
            verify_binary_hashes(report['retained_binary_sha256_after'])
        except (OSError, ValueError) as error:
            report.update(status='FAIL', binary_identity_error=f'{type(error).__name__}: {error}')
        write_report(session / 'report.json', report)
        if args.report:
            write_report(args.report, report)
    print(f'report: {session / "report.json"}', flush=True)
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
