#!/usr/bin/env python3
"""Retain and run DEV2 thread/codec V3 leaves and the finite P0 exploration.

Independent correctness targets run concurrently; each V3 target executes at
least 30 minutes and P0 completes its declared finite space. All code is built
from a retained source-only export. Performance, remaining codecs, native fault
qualification, lower regressions and protected promotion remain separate axes.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import resource
import shutil
import signal
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from check_dev0 import SOURCE_PATHS, check_report_paths, source_fingerprint, write_report
import dev2_extended_profiles as kernel_profiles

ROOT = Path(__file__).resolve().parent.parent
MIN_SECONDS = 1800
TIMEOUT_SECONDS = 2400
MAX_LOG_BYTES = 32 * 1024 * 1024
LANES = ('off', 'always', 'detailed_0', 'detailed_1', 'detailed_256')
FUZZ_OPERATORS = ('valid_identity_text', 'byte_flip', 'byte_insert', 'byte_delete', 'length',
                  'truncate', 'concatenate', 'unknown_field', 'duplicate_field', 'unknown_tag',
                  'identity_limit', 'invalid_utf8', 'nested_unknown', 'nested_duplicate',
                  'trailing_json', 'partial_suffix', 'invalid_suffix')
FUZZ_FAMILIES = ('valid', 'eof', 'length', 'identity', 'duplicate', 'unknown', 'utf8', 'json',
                 'tag', 'truncation', 'concatenation', 'serialization', 'differential')
FUZZ_VARIANTS = {
    'native_request_fuzz': ('initialize', 'create', 'lookup', 'drop_table', 'insert', 'replace',
                            'delete', 'snapshot', 'release_snapshot', 'read', 'open_scan',
                            'next_page', 'close_scan', 'metrics', 'performance_off',
                            'performance_always', 'performance_detailed'),
    'native_response_fuzz': ('ready', 'done', 'table', 'inserted', 'row_some', 'row_none', 'page',
                             'empty_page', 'metrics', 'performance_off', 'performance_always',
                             'performance_detailed', *(f'product_error_{i}' for i in range(1, 10))),
}
BASE_TARGETS = (
    {'target': 'k0_real_thread_stress', 'package': 'xadb-engine', 'binary': 'stress',
     'test': 'four_readers_keep_old_rows_and_release_pins_v3_30_minutes',
     'generator': 'k0-thread-v1', 'seed': 0},
    {'target': 'native_request_fuzz', 'package': 'xadbtest-protocol', 'binary': 'fuzz',
     'test': 'v3_request_parser_mutation_fuzz_30_minutes',
     'generator': 'k0-native-mutation-v1', 'seed': 0x5841444252455131},
    {'target': 'native_response_fuzz', 'package': 'xadbtest-protocol', 'binary': 'fuzz',
     'test': 'v3_response_parser_mutation_fuzz_30_minutes',
     'generator': 'k0-native-mutation-v1', 'seed': 0x5841444252455331},
)
TARGETS = BASE_TARGETS + kernel_profiles.TARGETS


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read_receipt(output, prefix):
    receipts = []
    for line in output.splitlines():
        if prefix in line:
            receipts.append(json.loads(line.split(prefix, 1)[1], object_pairs_hook=unique_fields))
    if len(receipts) != 1 or not isinstance(receipts[0], dict):
        raise ValueError(f'expected exactly one {prefix} object')
    return receipts[0]


def unique_fields(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate JSON field: {key}')
        result[key] = value
    return result


def require_one_test(output, name):
    # With --nocapture the receipt can occur between `test NAME ...` and `ok`.
    # The exact invocation, inventory and final harness summary jointly prove
    # that the requested test actually completed; a zero-test exit is rejected.
    if output.splitlines().count('running 1 test') != 1:
        raise ValueError('expected one executed test')
    if len(re.findall(r'^test ' + re.escape(name) + r' \.\.\. ', output, re.MULTILINE)) != 1:
        raise ValueError('missing/duplicate exact test name')
    summaries = re.findall(r'^test result: .*$', output, re.MULTILINE)
    if len(summaries) != 1 or not re.fullmatch(
            r'test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out; finished in [0-9.]+s',
            summaries[0]):
        raise ValueError('missing successful one-test harness summary')


def integer(record, field, minimum):
    value = record.get(field)
    if type(value) is not int or value < minimum:
        raise ValueError(f'invalid or missing {field}: {value!r}')
    return value


def coverage(record, field, expected):
    counts = record.get(field)
    if not isinstance(counts, dict) or set(counts) != set(expected):
        raise ValueError(f'missing/foreign coverage: {field}')
    for key in expected:
        integer(counts, key, 1)
    return counts


def validate_receipts(spec, output, elapsed_seconds):
    """Reject incomplete, foreign, short and zero-work V3 evidence."""
    require_one_test(output, spec['test'])
    if spec['target'] in {item['target'] for item in kernel_profiles.TARGETS}:
        return kernel_profiles.validate(spec, output, elapsed_seconds, read_receipt, integer, coverage)
    profile = read_receipt(output, 'XADB_V3_PROFILE ')
    result = read_receipt(output, 'XADB_V3_RESULT ')
    for field in ('target', 'generator', 'seed'):
        if (profile.get(field) != spec[field] or result.get(field) != spec[field]
                or type(profile.get(field)) is not type(spec[field])
                or type(result.get(field)) is not type(spec[field])):
            raise ValueError(f'profile/result identity mismatch: {field}')
    if integer(profile, 'min_seconds', MIN_SECONDS) != MIN_SECONDS:
        raise ValueError('unexpected duration contract')
    measured_ms = integer(result, 'elapsed_ms', MIN_SECONDS * 1000)
    if elapsed_seconds < MIN_SECONDS or measured_ms > elapsed_seconds * 1000 + 1:
        raise ValueError('impossible or short wall-clock containment')
    if spec['target'] == 'k0_real_thread_stress':
        if profile.get('seed_count') != 256:
            raise ValueError('stress seed inventory mismatch')
        rounds = integer(result, 'rounds', 1280)
        counts = result.get('lane_rounds')
        if not isinstance(counts, dict) or set(counts) != set(LANES):
            raise ValueError('missing/foreign stress lane')
        if any(type(counts[lane]) is not int or counts[lane] < 256 for lane in LANES):
            raise ValueError('stress lane below fixed seed coverage')
        if sum(counts.values()) != rounds or integer(result, 'missing_lanes', 0) != 0:
            raise ValueError('stress lane conservation failed')
        if integer(result, 'seed_count', 256) != 256 or integer(result, 'visited_seed_lanes', 1280) != 1280:
            raise ValueError('missing seed/lane pair coverage')
        readers = result.get('reader_operations_by_thread')
        if (not isinstance(readers, list) or len(readers) != 4 or
                any(type(value) is not int or value <= 0 for value in readers) or
                sum(readers) != integer(result, 'reader_operations', 1)):
            raise ValueError('missing reader work or incorrect work conservation')
        integer(result, 'writer_operations', 1)
    else:
        if profile.get('coverage_guided') is not False or integer(profile, 'input_cap', 8388612) != 8388612:
            raise ValueError('fuzz input/model profile changed')
        cases = integer(result, 'cases', 1)
        accepted = integer(result, 'accepted', 1)
        rejected = integer(result, 'rejected', 1)
        if accepted + rejected != cases:
            raise ValueError('fuzz case conservation failed')
        if integer(result, 'known_valid', 1) > accepted or integer(result, 'known_invalid', 1) > rejected:
            raise ValueError('fuzz known expectations exceed observed outcomes')
        families = (*FUZZ_FAMILIES, 'product_code') if spec['target'] == 'native_response_fuzz' else FUZZ_FAMILIES
        family_counts = coverage(result, 'family_visits', families)
        operators = coverage(result, 'operator_visits', FUZZ_OPERATORS)
        variants = coverage(result, 'variant_visits', FUZZ_VARIANTS[spec['target']])
        mutations = integer(result, 'mutations', 1024)
        corpus_cases = integer(result, 'corpus_cases', 322 if spec['target'] == 'native_request_fuzz' else 223)
        if (sum(family_counts.values()) != cases or sum(operators.values()) != mutations
                or corpus_cases + mutations != cases or any(value != 1 for value in variants.values())
                or result['known_valid'] + result['known_invalid'] + family_counts['differential'] != cases):
            raise ValueError('fuzz coverage conservation failed')
    return {'profile': profile, 'result': result}


def signal_group(pid):
    try:
        os.killpg(pid, signal.SIGKILL)
        return True
    except ProcessLookupError:
        return False


def run_command(argv, cwd, environment, log, timeout):
    """Own the entire build/regression process group, even on timeout or interrupt."""
    started = time.monotonic()
    timed_out = False
    leftovers = False
    with log.open('wb') as stream:
        child = subprocess.Popen(argv, cwd=cwd, env=environment, stdout=stream,
                                 stderr=subprocess.STDOUT, start_new_session=True)
        try:
            try:
                child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
        finally:
            # Kill children still using the owned group even when their parent
            # already exited. Reap our direct child; descendant reaping belongs
            # to its parent or Linux's adopter, rather than an invented waitpid.
            leftovers = signal_group(child.pid)
            child.wait(timeout=5)
    return {'exit_code': child.returncode, 'timed_out': timed_out,
            'remaining_group_signalled': leftovers, 'elapsed_seconds': time.monotonic() - started}


def child_limits():
    # Linux-only qualification: bounded address space and output files. CPU time
    # is deliberately not confused with elapsed time for the five-thread target.
    resource.setrlimit(resource.RLIMIT_AS, (1024 ** 3, 1024 ** 3))
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_LOG_BYTES, MAX_LOG_BYTES))


def run_targets(specs, binaries, session, environment, expected_hashes=None):
    """Run every fixed target, preserving failures instead of dropping a pair."""
    active = []
    results = []
    try:
        for spec in specs:
            name = spec['target']
            directory = session / name
            directory.mkdir()
            failure = directory / 'failures'
            failure.mkdir()
            binary = binaries[(spec['package'], spec['binary'])]
            log = directory / 'stdout.log'
            output = log.open('wb')
            argv = [str(binary), '--exact', spec['test'], '--ignored', '--nocapture',
                    '--test-threads=1']
            env = environment.copy()
            env['XADBTEST_V3_FAILURE_DIR'] = str(failure)
            entry = {'target': name, 'command': argv, 'cwd': str(directory),
                     'binary_sha256_before': digest(binary), 'log': str(log),
                     'timeout_seconds': spec.get('timeout_seconds', TIMEOUT_SECONDS), 'status': 'RUNNING'}
            if (expected_hashes is not None and entry['binary_sha256_before'] !=
                    expected_hashes[(spec['package'], spec['binary'])]):
                output.close()
                entry.update(status='FAIL', error='retained binary differs from build receipt',
                             exit_code=None, log_sha256=digest(log))
                results.append(entry)
                continue
            started = time.monotonic()
            try:
                child = subprocess.Popen(argv, cwd=directory, env=env, stdout=output,
                                         stderr=subprocess.STDOUT, start_new_session=True,
                                         preexec_fn=child_limits)
            except (OSError, subprocess.SubprocessError) as error:
                output.close()
                entry.update(status='FAIL', error=f'spawn failed: {error}', exit_code=None,
                             elapsed_seconds=time.monotonic() - started, log_sha256=digest(log))
                results.append(entry)
                continue
            active.append((spec, child, output, started, entry, binary, failure))
            print(f"started {name}: pid={child.pid}", flush=True)
        next_progress = time.monotonic() + 30
        while active:
            for item in active[:]:
                spec, child, output, started, entry, binary, failure = item
                elapsed = time.monotonic() - started
                needs_kill = elapsed > entry['timeout_seconds'] and child.poll() is None
                if needs_kill:
                    entry['timed_out'] = True
                    signal_group(child.pid)
                timed_out = entry.get('timed_out', False)
                code = child.poll()
                if code is None:
                    continue
                child.wait()
                if signal_group(child.pid):
                    entry['remaining_group_signalled'] = True
                output.close()
                entry.update(exit_code=code, timed_out=timed_out, elapsed_seconds=elapsed,
                             binary_sha256_after=digest(binary), log_sha256=digest(entry['log']))
                try:
                    if timed_out or code != 0 or entry.get('remaining_group_signalled', False):
                        raise ValueError('target failed or timed out')
                    if entry['binary_sha256_before'] != entry['binary_sha256_after']:
                        raise ValueError('binary changed during execution')
                    if any(failure.iterdir()):
                        raise ValueError('target retained a failure input')
                    log_path = Path(entry['log'])
                    if log_path.stat().st_size > MAX_LOG_BYTES:
                        raise ValueError('log bound exceeded')
                    entry.update(validate_receipts(spec, log_path.read_text(), elapsed))
                    entry['status'] = 'PASS'
                except (OSError, ValueError, RuntimeError) as error:
                    entry.update(status='FAIL', error=str(error))
                results.append(entry)
                active.remove(item)
                print(f"completed {entry['target']}: {entry['status']}", flush=True)
            if time.monotonic() >= next_progress:
                print('running: ' + ', '.join(
                    f'{item[0]["target"]} {int(time.monotonic() - item[3])}s'
                    for item in active), flush=True)
                next_progress = time.monotonic() + 30
            if active:
                time.sleep(1)
    finally:
        for spec, child, output, started, entry, binary, failure in active:
            signal_group(child.pid)
            child.wait()
            output.close()
            entry.update(status='FAIL', error='runner interrupted', exit_code=child.returncode,
                         elapsed_seconds=time.monotonic() - started,
                         log_sha256=digest(entry['log']))
            results.append(entry)
        # Incremental result also survives an exception in the outer runner.
        write_report(session / 'target-results.json', results)
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, help='new report; never overwrite')
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/dev2-extended'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='check-', dir=parent))
    report = {'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
              'scope': 'DEV2 retained-source thread/codec V3 and finite P0 leaves',
              'status': 'RUNNING', 'dev2_stage_status': 'INCOMPLETE',
              'k0_stage_status': 'INCOMPLETE', 'durability': 'NOT_RUN',
              'bounded_concurrent_exhaustion': 'NOT_RUN', 'performance': 'NOT_RUN',
              'lower_regression': 'NOT_RUN',
              'additional_parser_targets': 'NOT_RUN', 'protected_ci': 'NOT_RUN',
              'artifact_directory': str(session.relative_to(ROOT)), 'checks': [],
              'targets': [], 'parallel_correctness_execution': True,
              'environment': {'uname': list(platform.uname()), 'cpu_count': os.cpu_count()},
              'profile': {'targets': TARGETS, 'min_seconds': MIN_SECONDS,
                          'timeout_seconds': TIMEOUT_SECONDS, 'memory_bytes_per_target': 1024 ** 3,
                          'fuzz_variants': FUZZ_VARIANTS, 'fuzz_operators': FUZZ_OPERATORS,
                          'fuzz_families': FUZZ_FAMILIES, 'stress_lanes': LANES}}
    environment = os.environ.copy()
    for key in list(environment):
        if (key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or
                key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                        'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN')):
            environment.pop(key)
    environment.update(CARGO_TARGET_DIR=str(session / 'target'),
                       CARGO_PROFILE_RELEASE_OPT_LEVEL='3', CARGO_PROFILE_RELEASE_LTO='false',
                       CARGO_PROFILE_RELEASE_CODEGEN_UNITS='16', CARGO_PROFILE_RELEASE_INCREMENTAL='false',
                       CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS='true', CARGO_PROFILE_RELEASE_DEBUG='2',
                       CARGO_PROFILE_RELEASE_STRIP='none')
    binary_bindings = {}

    def verify_binaries():
        for path, expected in binary_bindings.items():
            if digest(path) != expected:
                raise ValueError('retained binary differs from compiled build receipt: ' + path)

    def command(name, argv, cwd, timeout=600):
        print(f'{name}: {" ".join(map(str, argv))}', flush=True)
        log = session / f'{name}.log'
        verify_binaries()
        try:
            execution = run_command(argv, cwd, environment, log, timeout)
        except (OSError, subprocess.SubprocessError, KeyboardInterrupt) as error:
            report['checks'].append({'id': name, 'command': list(map(str, argv)), 'cwd': str(cwd),
                                     'status': 'FAIL', 'error': str(error), 'log': str(log),
                                     'log_sha256': digest(log) if log.exists() else None})
            raise
        verify_binaries()
        passed = (execution['exit_code'] == 0 and not execution['timed_out']
                  and not execution['remaining_group_signalled'])
        report['checks'].append({'id': name, 'command': list(map(str, argv)), 'cwd': str(cwd),
                                 **execution, 'timeout_seconds': timeout,
                                 'status': 'PASS' if passed else 'FAIL',
                                 'log': str(log), 'log_sha256': digest(log)})
        if not passed:
            raise RuntimeError(f'{name} failed; see {log}')
        return log.read_text()

    try:
        source_before = source_fingerprint()
        report['source_sha256_before'] = source_before
        report['base_commit'] = command('base-commit', ['git', 'rev-parse', 'HEAD'], ROOT).strip()
        command('dirty-status', ['git', 'status', '--short'], ROOT)
        source = session / 'source-inputs'
        source.mkdir()
        for name in SOURCE_PATHS:
            original = ROOT / name
            if original.is_dir():
                shutil.copytree(original, source / name)
            else:
                shutil.copy2(original, source / name)
        if source_fingerprint(source) != source_before or source_fingerprint() != source_before:
            raise ValueError('source changed during export')
        report['retained_source'] = str(source)
        report['source_sha256'] = source_before
        write_report(session / 'profile.json', report['profile'])
        report['profile_sha256'] = digest(session / 'profile.json')
        report['toolchain'] = command('toolchain', ['rustc', '-Vv'], source)
        report['build_environment'] = {key: value for key, value in environment.items()
                                       if key.startswith('CARGO_PROFILE_')}
        command('runner-self-tests', ['python3', '-B', '-m', 'unittest', 'discover',
                                      '-s', 'scripts', '-p', 'test*dev2_extended*.py'], source)
        binaries = {}
        output_directory = session / 'binaries'
        output_directory.mkdir()
        for spec in TARGETS:
            key = (spec['package'], spec['binary'])
            if key in binaries:
                continue
            name = f'{key[0]}-{key[1]}'
            selection = ['--lib'] if spec.get('kind') == 'lib' else ['--test', key[1]]
            output = command(f'build-{name}', ['cargo', 'test', '-p', key[0], *selection,
                             '--release', '--no-default-features', '--locked', '--offline',
                             '--no-run', '--message-format=json'], source)
            executable = []
            for line in output.splitlines():
                if not line.startswith('{'):
                    continue
                item = json.loads(line)
                if item.get('reason') == 'compiler-artifact' and item.get('executable'):
                    if item['target']['name'] == key[1] and item['profile']['test']:
                        executable.append(Path(item['executable']))
            if len(executable) != 1:
                raise ValueError(f'missing/ambiguous test binary: {name}')
            destination = output_directory / name
            compiled_hash = digest(executable[0])
            shutil.copy2(executable[0], destination)
            if digest(destination) != compiled_hash or digest(executable[0]) != compiled_hash:
                raise ValueError('compiled executable changed while being retained')
            binary_bindings[str(destination)] = compiled_hash
            binaries[key] = destination
            listing = command(f'inventory-{name}', [str(destination), '--list'], session)
            for required in (item['test'] for item in TARGETS
                             if (item['package'], item['binary']) == key):
                if f'{required}: test' not in listing.splitlines():
                    raise ValueError(f'missing long target: {required}')
            required_smoke = ({spec['smoke']} if 'smoke' in spec else
                             {'four_readers_keep_old_rows_and_release_pins_smoke'} if key[1] == 'stress'
                             else {'request_mutation_corpus_smoke', 'response_mutation_corpus_smoke'})
            for test in sorted(required_smoke):
                smoke = command(f'smoke-{test}', [str(destination), '--exact', test,
                                '--nocapture', '--test-threads=1'], session)
                require_one_test(smoke, test)
        if source_fingerprint(source) != source_before:
            raise ValueError('retained source changed before long run')
        report['retained_binary_sha256'] = dict(binary_bindings)
        verify_binaries()
        report['targets'] = run_targets(TARGETS, binaries, session, environment,
                                       {key: binary_bindings[str(path)] for key, path in binaries.items()})
        verify_binaries()
        if len(report['targets']) != len(TARGETS) or any(item['status'] != 'PASS' for item in report['targets']):
            raise ValueError('one or more fixed V3 targets failed')
        report['bounded_concurrent_exhaustion'] = 'PASS_FIXED_SCALAR_SPACE'
        # The final integrated source gets one explicit lower regression gate;
        # independent V3 jobs do not silently repeat it.
        report['source_sha256_after'] = source_fingerprint()
        report['retained_source_sha256_after'] = source_fingerprint(source)
        if report['retained_source_sha256_after'] != source_before:
            raise ValueError('retained source changed during verification')
        report['current_source_matches'] = report['source_sha256_after'] == source_before
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report.update(status='FAIL', error=str(error))
        print(f'FAIL: {error}', flush=True)
    finally:
        write_report(session / 'report.json', report)
        if args.report:
            write_report(args.report, report)
    print(f'report: {session / "report.json"}', flush=True)
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
