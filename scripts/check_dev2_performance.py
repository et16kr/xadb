#!/usr/bin/env python3
"""Verify K0 diagnostics, independent paired analysis and retained release replay.

Execution correctness is separate from observed effect/guardrails. This first
instrumentation audit never promotes an optimization or completes K0/DEV2.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import uuid

sys.dont_write_bytecode = True
from check_dev0 import SOURCE_PATHS, check_report_paths, source_fingerprint, verify_source_identity, write_report
from check_dev2_reference import require_passed_tests

ROOT = Path(__file__).resolve().parent.parent
TESTS = {
    'platform-observers': ('xadb-platform', ['--lib'], {
        'performance::tests::cpu_vectors_reject_partial_invalid_and_extra_fields',
        'performance::tests::rss_vectors_preserve_units_missing_duplicates_and_overflow',
        'performance::tests::linux_observations_are_optional_and_never_fabricated'}),
    'protocol-performance': ('xadbtest-protocol', ['--test', 'performance'], {
        'performance_profile_is_fixed_before_fixture_execution',
        'performance_json_rejects_unknown_duplicate_and_unknown_mode',
        'performance_action_roundtrip_preserves_maximum_seed_and_exact_input',
        'maximum_performance_observation_fits_frame_and_oversized_output_is_rejected',
        'performance_output_nested_unknown_and_duplicate_fields_are_rejected'}),
    'catalog-performance-boundary': ('xadbtest-reference', ['--test', 'performance_boundary'], {
        'catalog_oracle_never_qualifies_performance_actions',
        'catalog_oracle_rejects_performance_observation_for_catalog_action'}),
    'diagnostics-unit': ('xadb-engine', ['--lib'], {
        'diagnostics::tests::contention_and_poison_drop_without_reusing_poisoned_queue',
        'diagnostics::tests::counters_and_sequence_never_wrap_or_duplicate'}),
    'engine-performance': ('xadb-engine', ['--test', 'performance'], {
        'modes_and_queue_limits_preserve_rows_ids_errors_and_row_quota',
        'every_operation_records_success_failure_and_actual_completion_error',
        'bounded_collection_exposes_missing_and_dropped_observations_and_retains_capacity',
        'invalid_capacity_is_rejected_before_a_store_is_returned',
        'concurrent_readers_keep_counts_and_loss_accounting_after_bounded_drains'}),
    'worker-performance': ('xadbtest-engine-worker', ['--test', 'performance'], {
        'real_fixture_matrix_preserves_all_read_values_old_roots_and_exact_measurement_cuts',
        'native_dispatch_accepts_only_one_isolated_batch_and_output_fits_frame',
        'invalid_profile_or_position_is_fatal_without_running_a_batch'}),
    'worker-performance-admission': ('xadbtest-engine-worker', ['--test', 'performance_admission'], {
        'performance_requires_actor_zero_request_two_and_no_intermediate_request',
        'invalid_performance_profile_poison_is_permanent',
        'successful_performance_observation_closes_session_before_later_effects'}),
}
CONTROLLER_TESTS = {
    'fixed_effect_vectors_and_exact_boundaries',
    'order_drift_and_overflow_cannot_make_a_valid_comparison',
    'exact_guardrail_boundaries_unknown_observations_and_p99_rank',
    'independent_scalar_rejects_changed_seed_warmup_reads_final_and_old_rows',
    'all_mode_coverage_and_gauges_preserve_meaning',
    'missing_failed_or_saturated_runs_never_disappear',
    'strict_recipe_limit_fields_order_identity_and_policy_are_frozen',
    'winner_reuse_reconfirmation_and_changed_instrumentation_are_rejected',
    'bundle_verifier_reanalyzes_raw_streams_and_rejects_missing_history_identity_and_failure',
    'replay_audit_preserves_original_ids_and_cannot_promote',
    'impossible_timing_is_invalid_but_equal_duration_and_missing_external_are_distinct',
    'detailed_events_reject_wrong_labels_stale_warmup_sequences_and_gaps',
}


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, help='new report path; never overwrite')
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/dev2-performance'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='check-', dir=parent))
    environment = os.environ.copy()
    environment['CARGO_TARGET_DIR'] = str(ROOT / 'target')
    for key in list(environment):
        if key.startswith('XADBTEST_'):
            environment.pop(key)
    report = {
        'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
        'scope': 'DEV2 K0-005 current-capability diagnostics and local instrumentation/replay audit; no optimization promotion',
        'status': 'RUNNING', 'dev2_stage_status': 'INCOMPLETE', 'k0_stage_status': 'INCOMPLETE',
        'durability': 'NOT_RUN', 'protected_ci': 'NOT_RUN', 'pf12_optimization': 'NOT_RUN',
        'performance_qualification': 'INCOMPLETE', 'artifact_directory': str(session.relative_to(ROOT)),
        'checks': [], 'campaigns': [],
    }

    def command(name, argv, cwd=ROOT, accepted=(0,)):
        print(f'{name}: {" ".join(map(str, argv))}', flush=True)
        # The inherited native/controller/catalog V3 profiles alone take about
        # eight minutes; lower stage builds and regressions are additional.
        timeout = 1800 if name == 'lower-regression' else 600
        try:
            result = subprocess.run(list(map(str, argv)), cwd=cwd, env=environment,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout)
            output, returncode = result.stdout, result.returncode
        except subprocess.TimeoutExpired as error:
            output, returncode = error.stdout or b'', None
        log = session / f'{name}.log'
        log.write_bytes(output)
        report['checks'].append({'id': name, 'command': list(map(str, argv)), 'cwd': str(cwd),
                                 'exit_code': returncode, 'accepted_exit_codes': list(accepted),
                                 'status': 'PASS' if returncode in accepted else 'FAIL',
                                 'timed_out': returncode is None,
                                 'timeout_seconds': timeout,
                                 'log': str(log.relative_to(ROOT)), 'log_sha256': digest(log)})
        if returncode not in accepted:
            print(output.decode(errors='replace'), flush=True)
            raise RuntimeError(f'{name} failed')
        return output

    def audit_result(data):
        value = json.loads(data)
        comparison = value['comparison']
        if comparison['correctness'] != 'PASS' or comparison['promotion'] != 'INCOMPLETE':
            raise RuntimeError('audit correctness failed or audit attempted promotion')
        if comparison['validity'] != 'VALID':
            # Preserve an observed order effect without treating it as a valid
            # performance comparison. Missing/failed evidence is never exempted.
            if (comparison['validity'] != 'INVALID'
                    or comparison['reasons'] != ['AB/BA median drift exceeds 0.10']):
                raise RuntimeError('audit lacks complete valid workload evidence')
        directory = Path(value['directory'])
        index = json.loads((directory / 'index.json').read_text())
        if len(index['runs']) != 20 or any(run['failure'] or not run['directory'] for run in index['runs']):
            raise RuntimeError('audit did not retain all twenty complete runs')
        if digest(directory / 'index.json') != value['index_sha256']:
            raise RuntimeError('audit index identity mismatch')
        report['campaigns'].append(value)
        return value

    code = 0
    try:
        source = source_fingerprint()
        report['source_sha256_before'] = source
        report['source_sha256'] = source
        source_copy = session / 'source-inputs'
        source_copy.mkdir()
        for name in SOURCE_PATHS:
            origin = ROOT / name
            if origin.is_dir():
                shutil.copytree(origin, source_copy / name)
            else:
                shutil.copy2(origin, source_copy / name)
        if source_fingerprint(source_copy) != source:
            raise RuntimeError('retained source copy differs')
        report['retained_source'] = str(source_copy.relative_to(ROOT))
        regression_path = session / 'dev2-regression.json'
        command('lower-regression', ['python3', 'scripts/check_dev2_reference.py', '--with-substrate', '--report', regression_path])
        regression = json.loads(regression_path.read_text())
        if regression['status'] != 'PASS' or len(regression['checks']) != 28:
            raise RuntimeError('current P0/K0 regression did not complete')
        verify_source_identity(source, regression['source_sha256'])
        report['regression'] = str(regression_path.relative_to(ROOT))
        report['regression_sha256'] = digest(regression_path)
        for name, (package, target, required) in TESTS.items():
            output = command(name, ['cargo', 'test', '-p', package, '--no-default-features', '--locked', '--offline', *target])
            require_passed_tests(output, required)
        # Controller inventory is fixed in this script before final verification.
        output = command('controller-performance', ['cargo', 'test', '-p', 'xadbtest', '--no-default-features', '--locked', '--offline', '--test', 'performance'])
        require_passed_tests(output, CONTROLLER_TESTS)
        binaries = session / 'binaries'
        binaries.mkdir()
        toolchain = subprocess.check_output(['rustc', '--version'], text=True).strip()
        rustc = subprocess.check_output(['rustc', '-vV'], text=True)
        host = next(line.removeprefix('host: ') for line in rustc.splitlines() if line.startswith('host: '))
        with tempfile.TemporaryDirectory(prefix='xadb-performance-release-') as target:
            argv = ['cargo', 'build', '--release', '--locked', '--offline', '--target-dir', target,
                    '--target', host, '-p', 'xadbtest', '-p', 'xadbtest-engine-worker']
            command('release-build', argv)
            for binary in ('xadbtest', 'xadbtest-engine-worker'):
                shutil.copy2(Path(target) / host / 'release' / binary, binaries / binary)
        controller = binaries / 'xadbtest'
        worker = binaries / 'xadbtest-engine-worker'
        verify_source_identity(source, source)
        build = {'source_sha256': source, 'toolchain': toolchain, 'target': host, 'profile': 'release', 'self_test': False}
        config = {'profile': 'release', 'features': [], 'target': host,
                  'environment': {key: value for key, value in environment.items()
                                  if key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'CARGO_BUILD_TARGET')
                                  or key.startswith('CARGO_PROFILE_RELEASE_')}}
        config_path = session / 'build-config.json'
        write_report(config_path, config)
        receipt_path = session / 'build-receipt.json'
        receipt = {'schema_version': 1, 'worker_sha256': digest(worker), 'build': build,
                   'command': ' '.join(argv), 'features': [], 'config_sha256': digest(config_path)}
        write_report(receipt_path, receipt)
        report['build_receipt'] = {'path': str(receipt_path.relative_to(ROOT)), 'sha256': digest(receipt_path)}
        report['controller_sha256'] = digest(controller)
        report['worker_sha256'] = digest(worker)
        report['environment'] = {'uname': list(platform.uname()), 'cpu_count': os.cpu_count(),
                                 'loadavg_before': list(os.getloadavg()), 'parallel_builds_during_measurement': False}
        write_report(session / 'profile.json', json.loads(command('profile', [controller, 'performance-profile'])))
        original = None
        for mode in ('always', 'detailed'):
            recipe_path = session / f'{mode}-recipe.json'
            prepared = json.loads(command(f'prepare-{mode}', [controller, 'prepare-performance', '--receipt', receipt_path,
                '--receipt-sha256', digest(receipt_path), '--campaign-id', uuid.uuid4().hex, '--seed', '20261018',
                '--mode', mode, '--output', recipe_path]))
            recipe_hash = prepared['recipe_sha256']
            value = audit_result(command(f'audit-{mode}', [controller, 'run-performance', '--recipe', recipe_path,
                '--recipe-sha256', recipe_hash, '--baseline-worker', worker, '--candidate-worker', worker,
                '--baseline-receipt', receipt_path, '--candidate-receipt', receipt_path,
                '--baseline-config', config_path, '--candidate-config', config_path,
                '--artifacts', session / mode], accepted=(0, 2)))
            checked = json.loads(command(f'verify-{mode}', [controller, 'verify-performance', '--directory', value['directory'],
                '--recipe-sha256', recipe_hash], accepted=(0, 2)))
            if checked['comparison'] != value['comparison']:
                raise RuntimeError('saved campaign reanalysis differs')
            if original is None:
                original = value
        cwd = session / 'replay-workspace'
        cwd.mkdir()
        copied = cwd / 'original'
        shutil.copytree(original['directory'], copied)
        if digest(copied / 'controller') != report['controller_sha256']:
            raise RuntimeError('copied replay controller differs')
        replay = audit_result(command('replay', [copied / 'controller', 'replay-performance', '--directory', copied,
            '--recipe-sha256', original['recipe_sha256'], '--campaign-id', uuid.uuid4().hex,
            '--artifacts', session / 'replay'], cwd=cwd, accepted=(0, 2)))
        replay_controller = Path(replay['directory']) / 'controller'
        checked = json.loads(command('verify-replay', [replay_controller, 'verify-performance', '--directory', replay['directory'],
            '--recipe-sha256', replay['recipe_sha256']], cwd=cwd, accepted=(0, 2)))
        if checked['comparison'] != replay['comparison']:
            raise RuntimeError('replay reanalysis differs')
        report['environment']['loadavg_after'] = list(os.getloadavg())
        report['source_sha256_after'] = source_fingerprint()
        verify_source_identity(source, report['source_sha256_after'])
        if digest(worker) != receipt['worker_sha256'] or digest(controller) != report['controller_sha256']:
            raise RuntimeError('retained release binary changed')
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        code = 1
    write_report(session / 'validation.json', report)
    if args.report:
        write_report(args.report, report)
    print(f'K0 performance leaf {report["status"]}; K0/DEV2 INCOMPLETE: {session / "validation.json"}')
    return code


if __name__ == '__main__':
    raise SystemExit(main())
