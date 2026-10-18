#!/usr/bin/env python3
"""Verify DEV2 P0 and optional K0 substrate leaves, never DEV2/B2 or durability."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from check_dev0 import check_report_paths, source_fingerprint, verify_source_identity, write_report

ROOT = Path(__file__).resolve().parent.parent
# Frozen acceptance inventory: do not derive it from whichever tests happen to build.
REQUIRED_TESTS = {
    'literal_private_changes_statement_rollback_null_and_absence',
    'input_order_and_duplicate_insert_do_not_partially_change_statement',
    'lost_update_and_write_skew_known_bad_are_rejected',
    'phantom_and_mixed_snapshot_known_bad_are_rejected',
    'partial_batch_and_ack_loss_known_bad_are_rejected_after_crash',
    'unjustified_unknown_is_not_a_wildcard_for_wrong_results',
    'uncertain_commit_has_one_whole_outcome_across_repeated_crashes',
    'pending_unacknowledged_commit_may_be_included_or_absent',
    'reversed_responses_and_begin_interval_use_legal_linearizations',
    'fenced_existing_readers_can_read_and_rollback_but_cannot_write_or_commit',
    'before_attempt_abort_and_stale_noop_writer_cannot_publish',
    'malformed_trace_and_empty_assertions_never_pass',
    'every_search_budget_is_incomplete_instead_of_false_pass_or_rejection',
    'input_limits_are_invalid_and_not_silently_truncated',
    'retained_mutation_and_scan_buffers_respect_the_frontier_budget',
    'logical_input_budget_includes_cell_key_and_event_costs',
    'v2_fixed_64_seeds_each_reach_128_operations_and_reject_mutated_observation',
    'maximum_length_1024_operations_is_supported',
}
EXTENDED_TEST = 'v3_fixed_256_seeds_each_reach_1024_operations'
SUBSTRATE_TESTS = {
    'xadb_engine': {
        'identity::tests::epoch_local_vectors_and_exhaustion_never_wrap_or_reuse',
        'substrate::tests::unchanged_row_and_schema_allocations_are_shared_across_cow_roots',
    },
    'substrate': {
        'actual_rows_crud_and_hand_calculated_epoch_id_vectors',
        'schema_rejects_invalid_names_duplicates_types_and_null_without_effects',
        'fixed_schema_and_table_count_boundaries_are_enforced',
        'value_and_canonical_row_exact_byte_boundaries',
        'foreign_instance_row_namespace_and_wrong_table_are_rejected',
        'old_reader_survives_drop_recreate_and_views_share_one_pin',
        'fixed_cursor_orders_pages_reports_eof_and_charges_result_arrays',
        'scan_byte_limit_and_owning_cursor_survive_store_drop',
        'failed_preparation_releases_budget_but_never_reuses_issued_ids',
        'quota_failures_across_mutations_preserve_root_and_ledger',
        'invalid_namespace_and_resource_configuration_is_rejected',
        'repeated_scan_results_are_charged_and_failure_does_not_advance_cursor',
    },
    'model': {
        'v2_64_fixed_seeds_cover_128_mutation_positions',
        'maximum_1024_mutation_positions_has_independent_full_scan_expectations',
        'real_reader_threads_keep_one_snapshot_across_publications',
    },
}
SUBSTRATE_EXTENDED_TEST = 'v3_256_fixed_seeds_cover_1024_mutation_positions'
FRAME_TESTS = {
    'hand_written_big_endian_and_json_vector_matches_exact_bytes',
    'every_short_prefix_is_rejected_and_poisons_incremental_decoder',
    'length_limit_precedes_body_read_and_full_wire_boundary_is_exact',
    'escaping_is_bounded_after_serialization_and_not_by_input_length',
    'malformed_utf8_json_fields_versions_and_envelopes_never_dispatch',
    'one_byte_and_multiple_frames_preserve_unconsumed_tail',
    'scalar_tags_distinguish_null_empty_and_signed_boundaries',
    'v2_64_seeds_of_128_frames_preserve_chunk_boundaries',
    'maximum_1024_frames_preserve_chunk_boundaries',
}
FRAME_EXTENDED_TEST = 'v3_256_seeds_of_1024_frames_preserve_chunk_boundaries'
WORKER_TESTS = {
    'adapter': {
        'adapter_uses_actual_crud_and_old_snapshot_capabilities_after_drop_recreate',
        'adapter_preserves_schema_scalar_tags_and_missing_table_error',
        'protocol_failures_poison_session_before_any_later_effect',
        'failed_and_released_aliases_are_permanently_consumed',
        'initialization_failure_returns_product_error_then_permanently_closes',
        'pin_quota_and_cursor_limit_are_bounded_without_silent_eviction',
    },
    'process': {
        'process_streams_real_observations_before_stdin_eof',
        'malformed_partial_and_oversized_frames_terminate_without_response',
        'initialize_failure_is_reported_then_worker_exits_before_a_second_request',
        'duplicate_request_terminates_without_processing_later_frames',
        'native_v2_64_seeds_each_execute_128_requests_against_independent_values',
        'native_maximum_1024_requests_preserves_independent_values',
        'actual_64_mib_run_input_limit_rejects_next_frame_before_response',
        'actual_64_mib_run_output_limit_never_emits_a_partial_next_frame',
        'exact_64_mib_input_succeeds_and_one_extra_byte_is_rejected',
        'maximum_canonical_rows_survive_wire_escaping_and_page_byte_limits',
    },
}
NATIVE_WORKER_TESTS = {
    'actual_worker_kill_receipt_follows_recorded_insert_and_read',
    'controller_records_actual_old_snapshot_after_delete_and_drop_recreate',
    'controller_v2_64_seeds_execute_128_requests_and_verify_external_history',
    'controller_maximum_1024_requests_verify_external_history',
}
NATIVE_WORKER_EXTENDED_TEST = 'controller_v3_256_seeds_execute_1024_requests_and_verify_external_history'
WORKER_TESTS['native'] = NATIVE_WORKER_TESTS
CONTROLLER_TESTS = {
    'external_journal_orders_invoke_sent_observe_and_retains_exact_worker',
    'reversed_responses_match_two_actors_and_full_32_pending_boundary',
    'identity_duplicate_partial_oversized_malformed_and_unsolicited_responses_are_rejected',
    'request_timeout_and_stdin_backpressure_do_not_block_cleanup',
    'early_response_before_complete_large_request_is_rejected',
    'bounded_output_error_and_input_budgets_are_not_silent_truncation',
    'journal_failure_before_send_or_observe_stays_failed_even_when_cleanup_also_fails',
    'journal_byte_limit_and_unconsumed_response_prevent_successful_close',
    'descendant_holding_streams_hits_drain_deadline_and_is_signalled',
    'planned_kill_records_intent_and_confirmed_cleanup_after_response',
    'bad_worker_hash_and_same_actor_pending_are_rejected',
    'valid_response_is_preserved_when_same_write_exceeds_stdout_budget',
    'responses_present_before_invoke_cannot_be_retroactively_matched',
    'partial_response_start_cannot_bind_a_later_request_identity',
    'admission_drains_more_than_one_quantum_before_registering_new_request',
}
CATALOG_TESTS = {
    'literal_catalog_scalar_identity_old_cursor_and_pin_vectors',
    'failed_schema_and_rows_consume_aliases_but_preserve_root_and_id_issuance',
    'malformed_envelopes_duplicate_missing_and_retired_capabilities_never_pass',
    'fixed_profile_and_logical_eight_mib_boundary_are_checked_before_copy',
    'catalog_v2_64_seeds_execute_128_pairs_and_reject_mixed_snapshots',
    'catalog_maximum_1024_pairs_preserves_fixed_snapshots',
    'canonical_page_budget_orders_all_128_rows_and_rejects_permutation',
    'input_budget_accounts_schema_lookup_and_observed_cell_bytes',
    'table_cursor_name_value_and_metric_profile_boundaries_are_explicit',
    'incremental_input_budget_remains_failed_and_matches_pair_limit',
}
CATALOG_EXTENDED_TEST = 'catalog_v3_256_seeds_execute_1024_pairs'
WORKER_TESTS['catalog'] = {
    'saved_recipe_replays_into_distinct_owned_runs_and_verifies_without_live_source',
    'wrong_input_identity_unknown_duplicate_version_and_byte_limits_never_start_worker',
    'tampered_complete_history_is_rejected_even_after_stream_hashes_are_refreshed',
    'actual_catalog_v2_64_seeds_execute_128_requests',
    'actual_catalog_maximum_1024_requests',
    'exact_sixteen_mib_recipe_with_json_whitespace_is_valid',
}
CATALOG_WORKER_EXTENDED_TEST = 'actual_catalog_v3_256_seeds_execute_1024_requests'

WORKER_EXTENDED_TEST = 'native_v3_256_seeds_each_execute_1024_requests_against_independent_values'


def require_passed_tests(output, required):
    passed = {line[len('test '):-len(' ... ok')] for line in output.decode().splitlines()
              if line.startswith('test ') and line.endswith(' ... ok')}
    missing = required - passed
    if missing:
        raise RuntimeError(f'required tests absent or not passed: {sorted(missing)}')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, help='new report path; never overwritten')
    parser.add_argument('--with-substrate', action='store_true',
                        help='also build and verify the actual volatile K0 substrate')
    args = parser.parse_args()
    try:
        check_report_paths([args.report])
    except (OSError, ValueError) as error:
        parser.error(str(error))
    parent = ROOT / 'artifacts/dev2-reference'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='check-', dir=parent))
    environment = os.environ.copy()
    for name in ('XADBTEST_ENGINE_WORKER_BIN', 'XADBTEST_NATIVE_MUTANT_BIN',
                 'XADBTEST_NATIVE_ARTIFACTS', 'XADBTEST_NATIVE_MUTANT_ARTIFACTS',
                 'XADBTEST_CATALOG_ARTIFACTS'):
        environment.pop(name, None)
    environment['CARGO_TARGET_DIR'] = str(ROOT / 'target')
    checks = []
    report = {
        'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
        'scope': 'DEV2 P0 independent history reference self-tests; no complete K0/SC01 or durability acceptance',
        'status': 'RUNNING', 'dev2_stage_status': 'INCOMPLETE',
        'product_durability': 'NOT_RUN', 'protected_ci': 'NOT_RUN',
        'bounded_concurrent_exploration': 'NOT_RUN',
        'artifact_directory': str(session.relative_to(ROOT)), 'checks': checks,
    }
    if args.with_substrate:
        report['scope'] += '; actual K0 volatile substrate leaf, not complete K0'
        report['k0_stage_status'] = 'INCOMPLETE'
        report['native_worker'] = 'TRUSTED_AND_EXTERNAL_CONTROLLER_SEQUENTIAL'
        report['streaming_controller'] = 'ADVERSARIAL_SELF_TEST_AND_ACTUAL_WORKER'
        report['catalog_oracle'] = 'BOUNDED_SEQUENTIAL_RECIPE_AND_EXTERNAL_HISTORY'
        report['performance_acceptance'] = 'NOT_RUN'
        report['v3_30_minute_thread_stress'] = 'NOT_RUN'
        report['native_frame_codec'] = 'IN_SCOPE_SELF_TEST_ONLY'
        report['v3_30_minute_codec_fuzz'] = 'NOT_RUN'

    def command(name, argv, cwd=None):
        print(f'{name}: {" ".join(argv)}', flush=True)
        result = subprocess.run(argv, cwd=cwd or ROOT, env=environment, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=600)
        log = session / f'{name}.log'
        log.write_bytes(result.stdout)
        checks.append({'id': name, 'command': argv, 'cwd': str(cwd or ROOT), 'exit_code': result.returncode,
                       'status': 'PASS' if result.returncode == 0 else 'FAIL',
                       'log': str(log.relative_to(ROOT)), 'log_sha256': digest(log)})
        if result.returncode != 0:
            print(result.stdout.decode(errors='replace'))
            raise RuntimeError(f'{name} failed')
        return result.stdout

    def dependency_closure(metadata, package_name):
        packages = {package['id']: package for package in metadata['packages']}
        nodes = {node['id']: node for node in metadata['resolve']['nodes']}
        pending = [next(key for key, package in packages.items()
                        if package['name'] == package_name)]
        closure = set()
        while pending:
            package = pending.pop()
            if package in closure:
                continue
            closure.add(package)
            # Includes normal, build, dev and conditional paths on the tested target.
            pending.extend(dependency['pkg'] for dependency in nodes[package]['deps'])
        return sorted(packages[key]['name'] for key in closure if packages[key]['source'] is None)

    def build_tests(package_name, targets, production_target=None, features=None, variant="", command_only=False):
        label = package_name + variant
        binaries = {}
        with tempfile.TemporaryDirectory(prefix=f'{package_name}-build-') as target:
            output = command(f'{label}-independent-build', [
                'cargo', 'build' if command_only else 'test', '-p', package_name,
                '--no-default-features', '--locked', '--offline', '--target-dir', target,
                '--message-format=json']
                + (['--bin', production_target] if command_only else ['--no-run'])
                + (['--features', features] if features else []))
            for line in output.splitlines():
                if not line.startswith(b'{'):
                    continue
                item = json.loads(line)
                if item.get('reason') != 'compiler-artifact' or not item.get('executable'):
                    continue
                if (production_target and item['target']['name'] == production_target
                        and 'bin' in item['target']['kind'] and not item['profile']['test']):
                    name = 'production'
                elif item['profile']['test'] and item['target']['name'] in targets:
                    name = item['target']['name']
                else:
                    continue
                if name in binaries:
                    raise RuntimeError(f'ambiguous {package_name}/{name} test binary')
                binary = session / f'{label}-{name}-tests'
                shutil.copy2(item['executable'], binary)
                binaries[name] = binary
        expected = set(targets) | ({'production'} if production_target else set())
        if set(binaries) != expected:
            raise RuntimeError(f'missing {package_name} binaries: {expected - set(binaries)}')
        return binaries

    code = 0
    try:
        report['source_sha256_before'] = source_fingerprint()
        generator = ROOT / 'crates/testing/reference/tests/history.rs'
        report['profiles'] = {
            'generator_source': str(generator.relative_to(ROOT)),
            'generator_sha256': digest(generator),
            'v2': {'seeds': list(range(64)), 'operations_per_seed': 128},
            'v3': {'seeds': list(range(256)), 'operations_per_seed': 1024},
            'maximum_length_seed': 2**64 - 1,
            'required_tests': sorted(REQUIRED_TESTS), 'extended_test': EXTENDED_TEST,
        }
        if args.with_substrate:
            substrate_generator = ROOT / 'crates/engine/tests/model.rs'
            report['profiles']['substrate'] = {
                'generator_source': str(substrate_generator.relative_to(ROOT)),
                'generator_sha256': digest(substrate_generator),
                'v2': {'seeds': list(range(64)), 'mutation_positions_per_seed': 128},
                'v3': {'seeds': list(range(256)), 'mutation_positions_per_seed': 1024},
                'maximum_length_seed': 2**64 - 1,
                'observation': 'independent complete sorted scan after every mutation position',
                'real_threads': {'readers': 2, 'writer_replacements': 1000},
                'required_tests': {key: sorted(value) for key, value in SUBSTRATE_TESTS.items()},
                'extended_test': SUBSTRATE_EXTENDED_TEST,
            }
            frame_generator = ROOT / 'crates/testing/protocol/tests/engine_frame.rs'
            report['profiles']['frame'] = {
                'generator_source': str(frame_generator.relative_to(ROOT)),
                'generator_sha256': digest(frame_generator),
                'v2': {'seeds': list(range(64)), 'frames_per_seed': 128},
                'v3': {'seeds': list(range(256)), 'frames_per_seed': 1024},
                'maximum_length_seed': 2**64 - 1,
                'required_tests': sorted(FRAME_TESTS), 'extended_test': FRAME_EXTENDED_TEST,
            }
            worker_generator = ROOT / 'crates/testing/engine-worker/tests/support/mod.rs'
            report['profiles']['worker'] = {
                'generator_source': str(worker_generator.relative_to(ROOT)),
                'generator_sha256': digest(worker_generator),
                'v2': {'seeds': list(range(64)), 'wire_requests_per_seed': 128},
                'v3': {'seeds': list(range(256)), 'wire_requests_per_seed': 1024},
                'maximum_length_seed': 2**64 - 1,
                'required_tests': {key: sorted(value) for key, value in WORKER_TESTS.items()},
                'extended_test': WORKER_EXTENDED_TEST,
                'controller_extended_test': NATIVE_WORKER_EXTENDED_TEST,
                'drivers': {name: digest(ROOT / name) for name in (
                    'crates/testing/engine-worker/tests/process.rs',
                    'crates/testing/engine-worker/tests/native.rs')},
            }
            report['profiles']['controller'] = {
                'sources': {name: digest(ROOT / name) for name in (
                    'crates/testing/controller/tests/native.rs',
                    'crates/testing/controller/tests/fixtures/native_mutant.rs')},
                'required_tests': sorted(CONTROLLER_TESTS),
                'scope': 'reduced limits in self-test; fixed production limits in actual worker runs',
            }
            report['profiles']['catalog'] = {
                'sources': {name: digest(ROOT / name) for name in (
                    'crates/testing/reference/tests/catalog.rs',
                    'crates/testing/reference/tests/support/catalog_cases.rs',
                    'crates/testing/engine-worker/tests/catalog.rs')},
                'v2': {'seeds': list(range(64)), 'requests_per_seed': 128},
                'v3': {'seeds': list(range(256)), 'requests_per_seed': 1024},
                'maximum_length_seed': 2**64 - 1,
                'required_reference_tests': sorted(CATALOG_TESTS),
                'required_worker_tests': sorted(WORKER_TESTS['catalog']),
                'extended_reference_test': CATALOG_EXTENDED_TEST,
                'extended_worker_test': CATALOG_WORKER_EXTENDED_TEST,
                'scope': 'actor-zero sequential small catalog; no PF or durability qualification',
            }
        # Freeze profiles in the owned run before compilation or execution.
        write_report(session / 'profiles.json', report['profiles'])
        regression_path = session / 'dev1-regression.json'
        command('dev1-regression', ['python3', 'scripts/check_dev1.py', '--report', str(regression_path)])
        regression = json.loads(regression_path.read_text())
        if regression['status'] != 'PASS':
            raise RuntimeError('lower regression did not pass')
        report['source_sha256'] = regression['source_sha256']
        verify_source_identity(report['source_sha256_before'], report['source_sha256'])
        report['regression'] = str(regression_path.relative_to(ROOT))
        report['regression_sha256'] = digest(regression_path)
        metadata = json.loads(command('dependency-boundary', [
            'cargo', 'metadata', '--format-version', '1', '--locked', '--offline', '--all-features',
            '--filter-platform', regression['target']]))
        local = dependency_closure(metadata, 'xadbtest-reference')
        if local != ['xadbtest-protocol', 'xadbtest-reference']:
            raise RuntimeError(f'production dependency in independent oracle: {local}')
        report['all_kind_workspace_dependency_closure'] = local
        report['dependency_target'] = regression['target']
        reference_binaries = build_tests('xadbtest-reference', {'history', 'catalog'} if args.with_substrate else {'history'})
        binary = reference_binaries['history']
        report['test_binary'] = str(binary.relative_to(ROOT))
        report['test_binary_sha256'] = digest(binary)
        require_passed_tests(command('literal-v2-maximum', [str(binary)]), REQUIRED_TESTS)
        require_passed_tests(command('v3-extended', [str(binary), '--ignored']), {EXTENDED_TEST})
        if args.with_substrate:
            binary = reference_binaries['catalog']
            report['catalog_reference'] = {'test_binary': str(binary.relative_to(ROOT)),
                                           'test_binary_sha256': digest(binary)}
            require_passed_tests(command('catalog-reference-literal-v2-maximum', [str(binary)]), CATALOG_TESTS)
            require_passed_tests(command('catalog-reference-v3-extended', [str(binary), '--ignored']),
                                 {CATALOG_EXTENDED_TEST})
            local = dependency_closure(metadata, 'xadb-engine')
            if local != ['xadb-engine', 'xadb-foundation']:
                raise RuntimeError(f'wrong engine dependency boundary: {local}')
            binaries = build_tests('xadb-engine', SUBSTRATE_TESTS)
            report['substrate'] = {
                'all_kind_workspace_dependency_closure': local,
                'test_binaries': {name: {'path': str(path.relative_to(ROOT)), 'sha256': digest(path)}
                                  for name, path in binaries.items()},
            }
            for name, required in SUBSTRATE_TESTS.items():
                require_passed_tests(command(f'substrate-{name}', [str(binaries[name])]), required)
            require_passed_tests(command('substrate-v3-extended', [
                str(binaries['model']), '--ignored']), {SUBSTRATE_EXTENDED_TEST})
            binary = build_tests('xadbtest-protocol', {'engine_frame'})['engine_frame']
            report['frame'] = {'test_binary': str(binary.relative_to(ROOT)),
                               'test_binary_sha256': digest(binary)}
            require_passed_tests(command('frame-literal-v2-maximum', [str(binary)]), FRAME_TESTS)
            require_passed_tests(command('frame-v3-extended', [str(binary), '--ignored']),
                                 {FRAME_EXTENDED_TEST})
            local = dependency_closure(metadata, 'xadbtest-engine-worker')
            if local != ['xadb-engine', 'xadb-foundation', 'xadbtest', 'xadbtest-engine-worker', 'xadbtest-protocol', 'xadbtest-reference']:
                raise RuntimeError(f'wrong native worker dependency boundary: {local}')
            binaries = build_tests('xadbtest-engine-worker', WORKER_TESTS, 'xadbtest-engine-worker')
            report['worker'] = {
                'all_kind_workspace_dependency_closure': local,
                'binaries': {name: {'path': str(path.relative_to(ROOT)), 'sha256': digest(path)}
                             for name, path in binaries.items()},
            }
            # The retained process tests must launch the retained production executable,
            # not the deleted temporary build path embedded by Cargo at compilation.
            environment['XADBTEST_ENGINE_WORKER_BIN'] = str(binaries['production'])
            environment['XADBTEST_NATIVE_ARTIFACTS'] = str(session / 'native-runs')
            environment['XADBTEST_CATALOG_ARTIFACTS'] = str(session / 'catalog-runs')
            for name, required in WORKER_TESTS.items():
                require_passed_tests(command(f'worker-{name}', [str(binaries[name])]), required)
            require_passed_tests(command('worker-v3-extended', [str(binaries['process']), '--ignored']),
                                 {WORKER_EXTENDED_TEST})
            require_passed_tests(command('worker-controller-v3-extended', [
                str(binaries['native']), '--ignored']), {NATIVE_WORKER_EXTENDED_TEST})
            require_passed_tests(command('worker-catalog-v3-extended', [
                str(binaries['catalog']), '--ignored']), {CATALOG_WORKER_EXTENDED_TEST})
            local = dependency_closure(metadata, 'xadbtest')
            if local != ['xadbtest', 'xadbtest-protocol', 'xadbtest-reference']:
                raise RuntimeError(f'wrong controller dependency boundary: {local}')
            binaries = build_tests('xadbtest', {'native'}, 'xadbtest-native-mutant-worker', 'self-test')
            report['controller'] = {
                'all_kind_workspace_dependency_closure': local,
                'binaries': {name: {'path': str(path.relative_to(ROOT)), 'sha256': digest(path)}
                             for name, path in binaries.items()},
            }
            environment['XADBTEST_NATIVE_MUTANT_BIN'] = str(binaries['production'])
            environment['XADBTEST_NATIVE_MUTANT_ARTIFACTS'] = str(session / 'native-mutants')
            require_passed_tests(command('controller-adversarial', [str(binaries['native'])]),
                                 CONTROLLER_TESTS)
            # Inventory every retained native session before declaring this leaf PASS.
            runs = []
            for directory in sorted((session / 'native-runs').iterdir()):
                completed = directory / 'completion.json'
                killed = directory / 'planned-kill.json'
                if completed.is_file() == killed.is_file():
                    raise RuntimeError('native session must have exactly one terminal report')
                terminal = completed if completed.is_file() else killed
                completion = json.loads(terminal.read_text())
                manifest = json.loads((directory / 'manifest.json').read_text())
                if (manifest['source_sha256'] != report['source_sha256']
                        or manifest['self_test'] is not False
                        or manifest['worker_sha256'] != report['worker']['binaries']['production']['sha256']
                        or completion['requests'] != completion['observations']
                        or completion['completion'] != ('TRANSPORT_CLOSED_NO_PRODUCT_VERDICT'
                            if completed.is_file() else 'WORKER_KILLED_NO_PRODUCT_VERDICT')):
                    raise RuntimeError('native session identity/completion mismatch')
                for key, filename in (('worker_sha256', 'worker'), ('history_sha256', 'history.bin'),
                                      ('stdout_sha256', 'stdout.bin'), ('stderr_sha256', 'stderr.bin')):
                    if completion[key] != digest(directory / filename):
                        raise RuntimeError(f'native session digest mismatch: {directory}/{filename}')
                runs.append({'directory': str(directory.relative_to(ROOT)),
                             'requests': completion['requests'],
                             'terminal_report': terminal.name, 'completion_sha256': digest(terminal),
                             'manifest_sha256': digest(directory / 'manifest.json')})
            if (len(runs) != 323 or sorted(run['requests'] for run in runs)
                    != [5, 10] + [128] * 64 + [1024] * 257
                    or sum(run['terminal_report'] == 'planned-kill.json' for run in runs) != 1):
                raise RuntimeError('native session inventory is incomplete')
            write_report(session / 'native-inventory.json', runs)
            report['native_inventory'] = {
                'path': str((session / 'native-inventory.json').relative_to(ROOT)),
                'sha256': digest(session / 'native-inventory.json'), 'sessions': len(runs),
            }
            mutants = [{'path': str(path.relative_to(ROOT)), 'sha256': digest(path)}
                       for path in sorted((session / 'native-mutants').rglob('*')) if path.is_file()]
            write_report(session / 'native-mutants.json', mutants)
            report['native_mutants'] = {
                'path': str((session / 'native-mutants.json').relative_to(ROOT)),
                'sha256': digest(session / 'native-mutants.json'), 'files': len(mutants),
            }
            # Catalog positive runs and deliberate history mutations have separate paths.
            catalog_parent = session / 'catalog-runs'
            expected_seeds = {f'seed-{seed}-128' for seed in range(64)} | {
                f'seed-{seed}-1024' for seed in range(256)} | {f'seed-{2**64 - 1}-1024'}
            actual_seeds = {p.name for p in catalog_parent.glob('seed-*')}
            if actual_seeds != expected_seeds:
                raise RuntimeError('catalog seed directory inventory mismatch')
            catalog_runs = []
            for path in sorted(catalog_parent.glob('**/catalog-result.json')):
                if any(part.startswith('mutant-') for part in path.parts):
                    continue
                directory = path.parent
                verdict = json.loads(path.read_text())
                receipt = json.loads((directory / 'completion.json').read_text())
                manifest = json.loads((directory / 'manifest.json').read_text())
                recipe_path = directory / 'recipe.json'
                recipe = json.loads(recipe_path.read_text())
                if (verdict['verdict'] != 'ACCEPTED' or verdict['checked_pairs'] != len(recipe['actions'])
                        or manifest['source_sha256'] != report['source_sha256'] or manifest['self_test'] is not False
                        or manifest['worker_sha256'] != report['worker']['binaries']['production']['sha256']
                        or verdict['recipe_sha256'] != digest(recipe_path)
                        or receipt['requests'] != len(recipe['actions']) or receipt['observations'] != len(recipe['actions'])):
                    raise RuntimeError('catalog run identity/count/verdict mismatch')
                for key, filename in (('worker_sha256', 'worker'), ('history_sha256', 'history.bin'),
                                      ('stdout_sha256', 'stdout.bin'), ('stderr_sha256', 'stderr.bin')):
                    if receipt[key] != digest(directory / filename):
                        raise RuntimeError('catalog retained artifact hash mismatch')
                seed_directory = next((part for part in path.parts if part.startswith('seed-')), None)
                if seed_directory:
                    _, seed, requests = seed_directory.split('-')
                    first = next(action for action in recipe['actions'] if action['operation'] == 'replace')
                    value = (int(seed) * 6364136223846793005 + 1442695040888963407) % 2**64
                    if value >= 2**63:
                        value -= 2**64
                    if first['values'] != [{'type': 'int64', 'value': value}] or len(recipe['actions']) != int(requests):
                        raise RuntimeError('catalog recorded seed/request identity mismatch')
                catalog_runs.append({'directory': str(directory.relative_to(ROOT)),
                                     'pairs': len(recipe['actions']), 'result_sha256': digest(path)})
            if len(catalog_runs) != 325:
                raise RuntimeError(f'catalog run inventory incomplete: {len(catalog_runs)}')
            write_report(session / 'catalog-inventory.json', catalog_runs)
            report['catalog_inventory'] = {'path': str((session / 'catalog-inventory.json').relative_to(ROOT)),
                                          'sha256': digest(session / 'catalog-inventory.json'), 'sessions': len(catalog_runs)}
            catalog_files = [{'path': str(p.relative_to(ROOT)), 'sha256': digest(p)}
                             for p in sorted(catalog_parent.rglob('*')) if p.is_file()]
            write_report(session / 'catalog-files.json', catalog_files)
            report['catalog_files'] = {'path': str((session / 'catalog-files.json').relative_to(ROOT)),
                                      'sha256': digest(session / 'catalog-files.json'), 'files': len(catalog_files)}
            # Exercise the default command with only retained input/binaries and an unrelated cwd.
            controller = build_tests('xadbtest', set(), 'xadbtest', variant='-default-command',
                                     command_only=True)['production']
            report['catalog_command'] = {'binary': str(controller.relative_to(ROOT)), 'sha256': digest(controller)}
            recipe = catalog_parent / 'literal-replay' / 'input.json'
            worker = ROOT / report['worker']['binaries']['production']['path']
            identity = ['--recipe-sha256', digest(recipe), '--worker-sha256', digest(worker)]
            with tempfile.TemporaryDirectory(prefix='catalog-command-cwd-') as cwd:
                output = command('catalog-command-run', [str(controller), 'run-catalog', '--recipe', str(recipe),
                    '--worker', str(worker), '--artifacts', str(session / 'catalog-command')] + identity, cwd=cwd)
                executed = json.loads(output)
                if executed['verdict'] != 'ACCEPTED' or executed['checked_pairs'] != 23:
                    raise RuntimeError('catalog command did not execute the complete literal recipe')
                output = command('catalog-command-verify', [str(controller), 'verify-catalog',
                    '--directory', executed['directory']] + identity, cwd=cwd)
                verified = json.loads(output)
                if verified != executed:
                    raise RuntimeError('catalog command verification differs from executed result')
            report['catalog_command']['run'] = executed
        report['source_sha256_after'] = source_fingerprint()
        verify_source_identity(report['source_sha256_before'], report['source_sha256_after'])
        if report['profiles']['generator_sha256'] != digest(generator):
            raise RuntimeError('profile generator changed during run')
        if (args.with_substrate and report['profiles']['substrate']['generator_sha256']
                != digest(substrate_generator)):
            raise RuntimeError('substrate profile generator changed during run')
        if (args.with_substrate and report['profiles']['frame']['generator_sha256']
                != digest(frame_generator)):
            raise RuntimeError('frame profile generator changed during run')
        if (args.with_substrate and report['profiles']['worker']['generator_sha256']
                != digest(worker_generator)):
            raise RuntimeError('worker profile generator changed during run')
        if args.with_substrate:
            for inputs in (report['profiles']['worker']['drivers'],
                           report['profiles']['controller']['sources'],
                           report['profiles']['catalog']['sources']):
                for name, expected in inputs.items():
                    if digest(ROOT / name) != expected:
                        raise RuntimeError(f'profile source changed during run: {name}')
        report['status'] = 'PASS'
    except (RuntimeError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        code = 1
    write_report(session / 'validation.json', report)
    if args.report:
        try:
            write_report(args.report, report)
        except (OSError, ValueError) as error:
            print(f'report export failed: {error}', file=sys.stderr)
            return 1
    print(f'DEV2 leaves {report["status"]}; DEV2 INCOMPLETE: {session / "validation.json"}')
    return code


if __name__ == '__main__':
    raise SystemExit(main())
