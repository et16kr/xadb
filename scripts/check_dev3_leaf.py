#!/usr/bin/env python3
"""Verify DEV3 expressions, selector/metadata ports, bounded reads and M02-P01."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
CHECKER_DIRECTORY = Path(__file__).resolve().parent
# F07 freezes these files during execution. Disk hashes detect ordinary drift;
# this local check is not bytecode attestation or a defense against compile/read
# races, preloaded module injection, or a malicious writer.
LOADED_CHECKER_HASHES = {
    'scripts/' + name: hashlib.sha256((CHECKER_DIRECTORY / name).read_bytes()).hexdigest()
    for name in ('check_dev3_leaf.py', 'check_dev0.py', 'comm_campaign.py',
                 'dev3_wal_slot_model.py', 'dev3_wal_durable_model.py',
                 'dev3_storage_model.py', 'dev3_storage_native.py',
                 'dev3_doublewrite_model.py', 'dev3_doublewrite_native.py', 'dev3_doublewrite_checks.py')}
from check_dev0 import check_report_paths, digest, source_fingerprint
from comm_campaign import capture_source, identity, write_json
from dev3_wal_slot_model import evaluate as evaluate_wal_slot
from dev3_wal_durable_model import evaluate as evaluate_wal_durable
from dev3_storage_model import evaluate as evaluate_storage_model
from dev3_storage_native import evaluate as evaluate_storage_native
from dev3_doublewrite_model import evaluate as evaluate_doublewrite_model
from dev3_doublewrite_native import evaluate as evaluate_doublewrite_native

ROOT = Path(__file__).resolve().parent.parent
DOUBLEWRITE_INSTALL_TESTS = {
    'doublewrite_install_literal_profiles', 'doublewrite_install_pristine_preflight',
    'doublewrite_install_namespace_preflight', 'doublewrite_install_effect_failures',
    'doublewrite_install_effect_unwinds', 'doublewrite_install_short_io',
    'doublewrite_install_observation_failures', 'doublewrite_install_readback_rejects',
    'doublewrite_install_descriptor_and_workspace', 'doublewrite_install_resource_and_drop',
}
DOUBLEWRITE_INSTALL_NATIVE_TESTS = {
    'doublewrite_install_native_profiles', 'doublewrite_install_native_faults',
    'doublewrite_install_native_sigkill',
}
STORAGE_TESTS = {
    'bootstrap_literals_and_geometry', 'bootstrap_descriptor_rejects_damage',
    'bootstrap_requires_fresh_namespace', 'bootstrap_installs_exact_order',
    'bootstrap_faults_preserve_artifacts', 'bootstrap_short_io_is_bounded',
    'bootstrap_readback_binds_installed_objects', 'bootstrap_reader_and_wal_make_progress',
    'bootstrap_wal_requires_selected_mapping', 'bootstrap_buffer_preserves_shared_concurrency',
    'bootstrap_shared_fence_cleans_claims', 'bootstrap_workspace_and_handles_are_bounded',
}
STORAGE_NATIVE_TESTS = {'bootstrap_native_profiles_and_lease', 'bootstrap_native_fault_images',
                        'bootstrap_native_kill_images'}
WAL_IO_TESTS = {
    'genesis_codec_literals_and_damage',
    'genesis_requires_exact_empty_namespace',
    'genesis_installs_in_durable_order',
    'genesis_install_faults_preserve_partial_namespace',
    'wal_slot_writes_exact_logical_and_physical_prefixes',
    'wal_slot_rejects_boundaries_before_effect',
    'wal_io_faults_fence_without_replay',
    'wal_io_releases_gate_and_protects_slot',
    'wal_generated_prefixes_match_independent_bytes',
    'wal_workspace_and_handles_remain_bounded',
}
WAL_IO_NATIVE_TESTS = {
    'native_genesis_installs_and_appends_exact_bytes',
    'native_genesis_faults_preserve_partial_files',
    'native_genesis_process_kills_preserve_observed_prefixes',
}
READ_BUFFER_TESTS = {
    'read_buffer_pins_selected_bytes_and_hits',
    'read_buffer_capacity_and_profile_bounds',
    'read_buffer_clock_and_pins_bound_replacement',
    'read_buffer_identity_never_aliases_cached_pages',
    'read_buffer_load_failure_releases_claim',
    'read_buffer_loading_deduplicates_and_keeps_hits_live',
    'read_buffer_concurrent_pins_prevent_aba',
    'read_buffer_limits_and_poison_fence',
    'read_buffer_generated_sequences_match_reference_clock',
    'read_buffer_workspace_can_only_be_reused_after_release',
}
READ_BUFFER_NATIVE_TESTS = {
    'native_read_buffer_hits_evictions_and_reopen',
    'native_read_buffer_rejects_bad_load_and_recovers',
}
SEMANTIC_GROUP_TESTS = {
    'rollback_producer_matches_literal_frames_and_pages',
    'rollback_producer_rejects_invalid_before_context',
    'rollback_producer_budget_and_lsn_boundaries',
    'rollback_producer_generated_frames_match_independent_bytes',
    'rollback_producer_reuses_unpublished_outputs',
    'rollback_materialization_matches_literal_pages',
    'rollback_materialization_rejects_unbound_inputs',
    'rollback_materialization_keeps_semantic_cost_checks',
    'rollback_materialization_generated_pages_match_independent_bytes',
    'rollback_materialization_scratch_reuse_requires_success',
    'rollback_start_literals_bind_full_state',
    'rollback_start_lengths_and_damage',
    'rollback_start_group_sections_are_exact',
    'rollback_start_owner_chain_and_profile_are_bound',
    'rollback_start_operation_core_matches_head',
    'rollback_start_status_pages_are_complete',
    'rollback_start_redo_and_native_match_exact_cell',
    'rollback_start_preserves_other_page_bytes',
    'rollback_start_cost_transfer_is_exact',
    'rollback_start_ordering_uses_stream_and_page_positions',
    'rollback_start_legacy_frames_and_limits_are_preserved',
    'rollback_start_generated_profiles_match_independent_transition',
}
OWNERSHIP_TESTS = {
    'inventory_literals_bind_all_bytes_and_owners',
    'inventory_root_chunk_truncation_and_damage_reject',
    'inventory_reserved_and_header_damage_reject_after_checksum',
    'inventory_expected_owners_and_capture_are_required',
    'inventory_item_bounds_and_generations_are_checked',
    'inventory_canonical_order_spans_chunk_boundaries',
    'inventory_owner_digests_and_category_budgets_are_exact',
    'inventory_physical_aliases_and_intervals_match_pairwise_oracle',
    'inventory_all_lanes_drain_and_bind_repeated_bytes',
    'inventory_source_failures_and_workspace_are_bounded',
    'inventory_intent_uniqueness_is_explicitly_not_proven',
    'inventory_generated_profiles_match_independent_fields',
    'profile_inventory_binds_real_profiles',
    'profile_inventory_rejects_profile_mismatch_before_reads',
    'profile_inventory_enforces_growth_quota',
    'profile_inventory_rejects_owner_local_intent_aliases',
    'profile_inventory_checks_full_capacity_and_lying_claims',
    'profile_inventory_reuses_workspace_after_failures',
    'profile_inventory_keeps_all_validation_passes',
    'profile_inventory_intents_match_independent_sets',
}
OBLIGATION_TESTS = {
    'descriptor_literals_bind_exact_owner_profile_and_state',
    'descriptor_lengths_and_every_byte_damage_reject',
    'descriptor_reserved_and_semantic_damage_reject_after_checksum',
    'descriptor_budget_sums_peaks_and_growth_are_exact',
    'descriptor_expected_context_and_unsupported_work_root',
    'descriptor_encoder_errors_preserve_output',
    'descriptor_empty_inventory_and_chain_boundaries',
    'descriptor_generated_profiles_match_independent_fields',
}
STATUS_HOME_TESTS = {
    'status_home_literals_match_core_and_count',
    'status_home_rejects_lengths_headers_and_free_references',
    'status_home_encoder_errors_preserve_output',
    'status_home_count_delta_is_checked_and_atomic',
    'status_home_page_validates_all_slots_and_namespace',
    'status_home_lookup_rejects_stale_aliases',
    'status_home_generated_profiles_match_independent_fields',
}
PAGE_IO_TESTS = {
    'selected_map_reads_independent_page_and_all_declared_chunks',
    'selected_map_rejects_late_damage_and_never_scans_fallback',
    'selected_map_and_data_require_exact_handle_lengths',
    'page_request_binds_profile_owner_generation_kind_and_bounds',
    'every_read_fault_preserves_selection_and_bounded_handles',
}
METADATA_PLATFORM_TESTS = {
    'opened_metadata_binds_exact_handle_and_length',
    'replacement_keeps_old_handle_metadata_and_lease',
    'opened_metadata_rejects_invalid_names_and_types_without_effects',
}
METADATA_WORKER_TESTS = {'dev3_open_metadata_preserves_selector_roles'}
METADATA_PERFORMANCE_TESTS = {'open_metadata_counts_one_success_or_failure'}
PAGE_IO_NATIVE_TESTS = {'native_selected_map_reads_and_rejects_selected_damage'}
DISK_VECTOR_RESULT = {
    'dev3-io-profile.bin': (4096, '985651bdbfde061063f76265665c2aff331c9b01486e0be4de60223d6ec327d8'),
    'dev3-io-selector.bin': (4096, '4b383d1f2ad756630421555efcdef18ea40bba314b07a42333099ef75bf7fbae'),
    'dev3-io-root.bin': (416, '13e967b9db3b3e29d0bcf2bc6698de6f06b63e9a9b02e91a71bc5a624ba2de69'),
    'dev3-io-chunk0.bin': (6304, 'e2a132d9659f20a874339bf29328a9b529af734bfbdd72cae510601e744c81ed'),
    'dev3-io-chunk1.bin': (256, '0f186336d4102cbcc31474f3e3188f55cc3f4ae18c985aa0486e9909baa7e7d9'),
}
PAGE_TESTS = {
    'independent_page_vectors_match_every_byte_and_borrow_exact_cells',
    'every_truncation_trailing_byte_and_single_byte_damage_is_rejected',
    'valid_checksum_does_not_hide_invalid_structure_or_zero_identities',
    'expected_database_file_page_incarnation_owner_and_kind_are_required',
    'every_kind_and_internal_level_boundary_is_checked',
    'exact_page_capacity_and_all_encode_errors_preserve_output',
    'fixed_64_seed_128_operation_envelope_campaign_preserves_cells',
}
KEY_TESTS = {
    'composite_and_row_suffix_match_frozen_literal_vectors',
    'every_two_column_pair_and_row_tie_agree_with_independent_tuple_ordinals',
    'exact_escaped_key_limit_excludes_row_suffix_and_rejects_oversize',
    'invalid_arity_and_small_output_fail_before_any_write',
    'decoder_rejects_wrong_arity_malformed_scalar_trailing_bytes_and_zero_row',
}
SCALAR_KEY_TESTS = {
    'scalar_buffer_matches_literal_vectors_and_preserves_errors',
    'prefix_rejects_noncanonical_values_without_decoding_allocations',
    'decoded_value_limit_counts_escaped_zero_once_and_utf8_in_bytes',
}
ANCHOR_TESTS = {
    'anchor_cell_matches_independent_literal_and_validates_every_tree_level',
    'anchor_rejects_checksummed_semantic_damage_and_wrong_cell_count',
    'every_encode_validation_error_preserves_anchor_output',
    'root_load_requires_database_owner_reference_kind_and_level',
}
UNDO_REF_TESTS = {
    'page_and_undo_references_match_independent_literals_and_exact_lengths',
    'reference_decoders_reject_zero_identity_reserved_and_out_of_range_slots',
}
TREE_TESTS = {
    'tree_golden_pages_and_cells',
    'tree_leaf_lower_bound_matches_typed_oracle',
    'tree_internal_routing_and_child_expectations',
    'tree_rejects_checksummed_structural_damage',
    'tree_enforces_entry_page_and_output_limits',
    'tree_leaf_split_uses_bytes',
    'tree_internal_split_promotes_separator',
    'tree_child_alias_and_kind_level_guards',
    'tree_encoders_preserve_outputs',
}
OVERFLOW_TESTS = {
    'overflow_head_literal_and_limits',
    'overflow_page_literal',
    'overflow_chain_fixed_boundaries',
    'overflow_rejects_checksummed_damage',
    'overflow_rejects_links_and_identity',
    'overflow_verifier_requires_complete_order_and_digest',
    'overflow_streaming_profiles',
    'overflow_encoders_preserve_outputs',
}
TRANSACTION_TESTS = {
    'identity_literals_and_limits_are_canonical',
    'stamp_and_logical_version_identities_are_distinct',
    'status_literals_preserve_rollback_and_terminal_heads',
    'all_length_boundaries_reject_without_output_changes',
    'fixed_corrupt_fields_are_rejected',
    'status_lookup_binds_logical_and_physical_identity',
    'state_and_namespace_matrix_is_exact',
    'generated_transaction_records_roundtrip_without_effects',
}
WAL_TESTS = {
    'wal_semantic_finalizer_preserves_payload_and_legacy_boundary',
    'wal_semantic_finalizer_rejects_header_and_size_without_mutation',
    'wal_frame_literal_and_kind_matrix',
    'wal_frame_identity_and_lsn_chain',
    'wal_tail_classification_is_strict',
    'wal_rejects_complete_checksum_and_semantic_damage',
    'wal_patch_records_are_bounded_after_images',
    'wal_init_records_bind_complete_page_images',
    'wal_redo_groups_are_canonical_and_borrowed',
    'wal_exact_frame_and_record_limits',
    'wal_all_encoders_preserve_output',
    'wal_generated_profiles_match_independent_fields',
    'wal_stream_prefix_preserves_following_frames',
}
ALLOCATION_TESTS = {
    'allocation_golden_pages_and_literals',
    'allocation_geometry_and_metadata_ownership',
    'allocation_state_matrix_is_exact',
    'allocation_rejects_checksummed_semantic_damage',
    'allocation_lookup_binds_state_owner_and_generation',
    'allocation_reservation_owners_and_namespaces',
    'allocation_encoders_preserve_outputs_and_generations',
    'allocation_bounded_scan_profiles',
}
ROW_TESTS = {
    'canonical_row_literals_and_borrowed_values',
    'canonical_schema_and_size_boundaries',
    'versioned_row_and_index_literals',
    'every_short_buffer_and_trailing_input_is_rejected',
    'fixed_noncanonical_fields_are_rejected',
    'record_namespace_and_logical_identity_are_bound',
    'row_payload_and_index_presence_matrices',
    'materialized_overflow_requires_digest_and_schema',
    'tree_values_bind_expected_row_ids',
    'fixed_seed_rows_match_independent_wire_fields',
}
DOUBLEWRITE_TESTS = {
    'doublewrite_literal_controls_and_images',
    'doublewrite_lengths_and_checksum_damage',
    'doublewrite_checksummed_semantic_damage',
    'doublewrite_addresses_slots_and_expectations',
    'doublewrite_image_validation_binds_every_field',
    'doublewrite_exact_bounds_and_empty_retirement',
    'doublewrite_encoders_preserve_outputs',
    'doublewrite_fixed_profiles_have_bounded_inventory',
    'doublewrite_slot_sequence_and_envelope_are_bound',
    'doublewrite_slot_encoders_preserve_outputs',
}
CATALOG_TESTS = {
    'catalog_literals_and_borrowed_definitions',
    'catalog_table_name_column_and_size_bounds',
    'catalog_index_columns_and_parent_matrix',
    'catalog_lengths_damage_and_outputs',
    'catalog_namespace_handle_and_versions',
    'catalog_tree_values_and_fixed_profiles',
}
UNDO_RECORD_TESTS = {
    'undo_independent_literals_and_layouts',
    'undo_all_lengths_preserve_outputs',
    'undo_targets_bind_namespace_owner_and_key',
    'undo_before_state_matrix',
    'undo_edges_keep_roles_and_multiplicity',
    'undo_release_cursor_state_matrix',
    'undo_rejects_checksummed_damage',
    'undo_slot_lookup_checks_generation_and_kind',
    'undo_chunk_boundaries_and_verifier',
    'undo_chunk_ownership_and_release_resume',
    'undo_failed_shapes_have_no_effects',
    'undo_generated_profiles_match_independent_fields',
}
CONTROL_TESTS = {
    'control_profile_literals',
    'control_filemap_literals',
    'control_selector_bindings_and_lengths',
    'control_envelope_corruption_is_rejected',
    'control_profiles_require_exact_capacities',
    'control_filemap_cursor_rejects_mixed_inventory',
    'control_names_and_bootstrap_ownership',
    'control_filemap_exact_bounds',
    'control_encoders_preserve_outputs',
    'control_generated_inventory_profiles',
}
SELECTOR_PLATFORM_TESTS = {
    'typed_selectors_replace_reopen_and_preserve_reserved_names',
    'typed_selector_preflight_rejects_aliases_without_effects',
}
SELECTOR_PROTOCOL_TESTS = {'dev3_selector_cut_matrix_and_wire_names'}
SELECTOR_WORKER_TESTS = {
    'dev3_selector_faults_preserve_target_lineage_and_certainty',
    'dev3_selector_write_sync_faults_use_exact_names',
    'dev3_selector_repeated_unknown_reopens_do_not_cross_targets',
}
MODEL_TEST = 'snapshot_lifecycle_partial_model_exhaustive'
MODEL_BUDGET_TEST = 'snapshot_model_budget_exhaustion_is_inconclusive'
NORMAL_CASES = {'registration', 'unpublished', 'transient_undo', 'transient_status'}
REQUIRED_WITNESSES = {
    'registration': {'q10_selected', 'registration_gate_blocks_aging', 'q10_read_a',
                     'registration_cancel_clean', 'registration_failure_clean', 'guard_overlap',
                     'guard_transfer_complete', 'guard_release_clean', 'undo_reused', 'status_reused'},
    'unpublished': {'unpublished_empty_horizon10', 'q10_read_a', 'q11_read_b',
                    'registration_cancel_clean', 'registration_failure_clean', 'guard_overlap',
                    'guard_transfer_complete', 'undo_reused', 'status_reused'},
    'transient_undo': {'ref_copied', 'ref_pinned', 'source_unlatched', 'pinned_undo_blocks_reuse',
                       'ref_loaded_same_generation', 'reuse_after_blocked_pin_release',
                       'undo_reused', 'q11_read_b'},
    'transient_status': {'ref_copied', 'ref_pinned', 'source_unlatched', 'pinned_status_blocks_reuse',
                         'ref_loaded_same_generation', 'reuse_after_blocked_pin_release',
                         'status_reused', 'q10_read_a'},
}
VIOLATIONS = {
    'early_gate_release': 'reader_history_lost',
    'unpublished_horizon': 'published_history_lost',
    'empty_registry_infinity': 'published_history_lost',
    'guard_handoff_gap': 'guard_owner_lost',
    'pin_after_unlatch': 'transient_generation_changed',
    'ignore_transient_pin': 'transient_generation_changed',
}
ACTIONS = {
    'q_read', 'horizon_register', 'registration_cancel', 'registration_fail', 'status_install',
    'frontier_publish', 'horizon_sample', 'stamp', 'cut', 'reuse_undo', 'reuse_status', 'aging_finish',
    'guard_transfer_begin', 'guard_transfer_register', 'guard_transfer_release_old', 'guard_release',
    'copy_ref', 'pin_ref', 'source_unlatch', 'load_ref', 'release_ref', 'read_value',
}
VECTOR_RESULT = {
    'status': 'PASS',
    'fixture_sha256': {
        'dev3-page-v1-empty.bin': 'de44e1eda7207bbd7aac6310bb9e307cfe2b37d9bca788056a69569d55735867',
        'dev3-page-v1-cells.bin': '908281ca2193411014868af608a28cc6aa06a129e8f6eb4d554bd12de3f2659c'},
    'negative': {'short_lengths': 16384, 'oversized_lengths': 1, 'single_byte_damage': 16384,
                 'checksummed_semantic_damage': 27, 'expected_identity_or_kind_mismatch': 7},
    'boundaries': {'accepted_boundaries': 17, 'rejected_construction_boundaries': 2},
}
NEGATIVE_CASES = {
    ('registration', 'early_gate_release'),
    ('unpublished', 'unpublished_horizon'),
    ('unpublished', 'empty_registry_infinity'),
    ('registration', 'guard_handoff_gap'),
    ('transient_undo', 'pin_after_unlatch'),
    ('transient_status', 'pin_after_unlatch'),
    ('transient_undo', 'ignore_transient_pin'),
    ('transient_status', 'ignore_transient_pin'),
}


def check_page_output(output, exit_code):
    return check_named_tests(output, exit_code, 'storage::page::tests::', PAGE_TESTS)


def check_disk_vectors(output):
    expected = {f'{name} {length} {sha}' for name, (length, sha) in DISK_VECTOR_RESULT.items()}
    lines = output.splitlines()
    if len(lines) != len(expected) or set(lines) != expected:
        raise ValueError('disk literal fixture inventory/hash mismatch')
    return {name: {'bytes': length, 'sha256': sha}
            for name, (length, sha) in DISK_VECTOR_RESULT.items()}


def check_named_tests(output, exit_code, prefix, expected):
    names = re.findall(rf'^test {re.escape(prefix)}(\w+) \.\.\. ok$', output, re.M)
    if exit_code or len(names) != len(expected) or set(names) != expected:
        raise ValueError('test failure, missing test, or duplicate result')
    summaries = re.findall(r'^test result: ok\. (\d+) passed; 0 failed; 0 ignored;', output, re.M)
    if summaries != [str(len(expected))]:
        raise ValueError('test summary is missing, duplicate or inconsistent')
    return sorted(names)


def check_model_output(output, exit_code):
    if exit_code or not re.search(rf'^test {MODEL_TEST} \.\.\. ok$', output, re.M):
        raise ValueError('model test failed or did not execute')
    if not re.search(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;', output, re.M):
        raise ValueError('model harness summary missing or inconsistent')
    cases = [json.loads(line.removeprefix('XADB_DEV3_SNAPSHOT_MODEL '))
             for line in output.splitlines() if line.startswith('XADB_DEV3_SNAPSHOT_MODEL ')]
    summaries = [json.loads(line.removeprefix('XADB_DEV3_SNAPSHOT_COMPLETE '))
                 for line in output.splitlines() if line.startswith('XADB_DEV3_SNAPSHOT_COMPLETE ')]
    if summaries != [{'scope': 'D3-M02-P01', 'status': 'PASS', 'normal_cases': 4, 'negative_cases': 8}]:
        raise ValueError('missing or inconsistent partial-model completion')
    normal, negative = set(), set()
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError('model case must be an object')
        if case.get('scope') != 'D3-M02-P01' or case.get('error') not in (None, ''):
            raise ValueError('wrong scope or model error')
        for field, maximum in [('states', 1_000_000), ('transitions', 8_000_000),
                               ('max_depth', 16), ('elapsed_ms', 60_000)]:
            value = case.get(field)
            if type(value) is not int or not 0 <= value <= maximum:
                raise ValueError(f'invalid or exceeded {field}')
        if not case['states'] or not case['transitions']:
            raise ValueError('empty exploration')
        if case.get('bound_depth') != 16 or case['elapsed_ms'] >= 60_000:
            raise ValueError('changed exploration depth or exhausted time budget')
        if type(case.get('unexplored')) is not int or case['unexplored'] < 0:
            raise ValueError('invalid unexplored count')
        fixture, mutant = case.get('fixture'), case.get('mutant')
        if not isinstance(fixture, str) or not (mutant is None or isinstance(mutant, str)):
            raise ValueError('fixture and mutant must be names')
        if mutant in (None, 'none'):
            if fixture in normal or fixture not in NORMAL_CASES:
                raise ValueError('duplicate or unexpected normal fixture')
            if case.get('status') != 'PASS' or case.get('unexplored') != 0 or case.get('counterexample'):
                raise ValueError('normal model did not complete its declared bounded exploration')
            witnesses = case.get('witnesses')
            if (not isinstance(witnesses, dict) or not REQUIRED_WITNESSES[fixture] <= witnesses.keys()
                    or not all(valid_trace(trace) for trace in witnesses.values())):
                raise ValueError('missing or invalid required reachability traces')
            if case.get('violation') is not None:
                raise ValueError('normal model reports a violation')
            normal.add(fixture)
        else:
            pair = fixture, mutant
            if pair in negative or pair not in NEGATIVE_CASES:
                raise ValueError('duplicate or unexpected mutant')
            if (case.get('status') != 'EXPECTED_COUNTEREXAMPLE'
                    or not valid_trace(case.get('counterexample'))
                    or case.get('violation') != VIOLATIONS[mutant]):
                raise ValueError('mutant was not rejected by a counterexample')
            negative.add(pair)
    if normal != NORMAL_CASES or negative != NEGATIVE_CASES:
        raise ValueError('incomplete model case inventory')
    return cases


def valid_trace(trace):
    return (isinstance(trace, list) and 0 < len(trace) <= 16
            and all(isinstance(action, str) and action in ACTIONS for action in trace))


def check_vectors(output):
    results = [json.loads(line.removeprefix('XADB_DEV3_PAGE_VECTORS '))
               for line in output.splitlines() if line.startswith('XADB_DEV3_PAGE_VECTORS ')]
    if results != [VECTOR_RESULT]:
        raise ValueError('missing, duplicate or incomplete independent vector result')
    return results[0]


def run_logged(name, command, source, environment, session, checks, timeout=600):
    """Retain command output and reject any owned process left after completion."""
    print(f'{name}: {" ".join(map(str, command))}', flush=True)
    log = session / f'{name}.log'
    interrupted = None
    remaining_group_signalled = False
    with log.open('xb') as stream:
        process = subprocess.Popen(command, cwd=source, env=environment, stdout=stream,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt) as error:
            interrupted = error
        finally:
            # Include descendants even if the direct child has already exited.
            try:
                os.killpg(process.pid, signal.SIGKILL)
                remaining_group_signalled = True
            except ProcessLookupError:
                pass
            process.wait(timeout=10)
    checks.append({'id': name, 'command': list(map(str, command)),
                   'exit_code': process.returncode, 'log': str(log),
                   'log_sha256': digest(log), 'interrupted': interrupted is not None,
                   'remaining_group_signalled': remaining_group_signalled})
    if interrupted is not None:
        raise RuntimeError(f'{name} interrupted or exceeded timeout; see {log}') from interrupted
    if remaining_group_signalled:
        raise RuntimeError(f'{name} left descendants after completion; see {log}')
    if process.returncode:
        raise RuntimeError(f'{name} failed; see {log}')
    return log.read_text()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--with-regressions', action='store_true',
                        help='also run DEV2 P0/K0 and DEV0/1 in the same retained source')
    args = parser.parse_args()
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/dev3'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='leaf-', dir=parent))
    source = session / 'source'
    report = {
        'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
        'scope': 'DEV3 page/key/reference/tree/overflow/transaction/status-home/obligation-descriptor/ownership-observation/RollbackStart-full-state-observation/WAL/allocation/row/DWB/catalog/undo/control expressions + selector/metadata effect ports + bounded selected-page reads/clean buffer + fresh storage and initial Empty DWB installation + D3-M02-P01; no disk engine or S1 acceptance',
        'status': 'RUNNING', 'origin': identity(ROOT), 'artifact_directory': str(session),
        'dev3_acceptance': 'INCOMPLETE', 'checks': [], 'builds': [],
        'checker_identity_scope': 'F07 file freeze and before/after disk hashes; not bytecode attestation or protected CI',
    }
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'XADB_WAL_', 'XADB_STORAGE_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN',
                'RUST_TEST_THREADS'):
            environment.pop(key)
    environment['CARGO_TARGET_DIR'] = str(source / 'target')
    environment['XADB_WAL_NATIVE_ARTIFACT_PARENT'] = str(session / 'genesis-native-artifacts')
    environment['XADB_STORAGE_NATIVE_ARTIFACT_PARENT'] = str(session / 'storage-native-artifacts')

    def run(name, command, timeout=600):
        return run_logged(name, command, source, environment, session, report['checks'], timeout)

    def build(package, target_args, target_name, name):
        command = ['cargo', 'test', '-p', package, *target_args, '--no-run', '--locked',
                   '--offline', '--message-format=json']
        output = run(f'build-{name}', command)
        messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
        binaries = [m['executable'] for m in messages
                    if m.get('reason') == 'compiler-artifact'
                    and m.get('target', {}).get('name') == target_name and m.get('executable')]
        if len(binaries) != 1 or not any(
                m.get('reason') == 'build-finished' and m.get('success') is True for m in messages):
            raise ValueError(f'missing unique compiled {name} binary')
        binary = session / f'{name}-binary'
        shutil.copy2(binaries[0], binary)
        report['builds'].append({'name': name, 'binary': str(binary), 'sha256': digest(binary)})
        return str(binary)

    code = 0
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        report['loaded_checker_sha256'] = LOADED_CHECKER_HASHES
        for path, expected in LOADED_CHECKER_HASHES.items():
            if digest(source / path) != expected or digest(ROOT / path) != expected:
                raise ValueError(f'loaded checker differs from retained source: {path}')
        report['contract_sha256'] = {
            name: digest(source / name) for name in [
                'devdoc/contracts/dev3-page-format.md', 'devdoc/contracts/dev3-key-format.md',
                'devdoc/contracts/dev3-reference-format.md',
                'devdoc/contracts/dev3-tree-format.md', 'devdoc/contracts/dev3-overflow-format.md',
                'devdoc/contracts/dev3-transaction-format.md',
                'devdoc/contracts/dev3-status-home.md',
                'devdoc/contracts/dev3-obligation-format.md',
                'devdoc/contracts/dev3-ownership-format.md',
                'devdoc/contracts/dev3-wal-format.md', 'devdoc/contracts/dev3-allocation-format.md',
                'devdoc/contracts/dev3-row-format.md', 'devdoc/contracts/dev3-doublewrite-format.md',
                'devdoc/contracts/dev3-catalog-format.md', 'devdoc/contracts/dev3-undo-format.md',
                'devdoc/contracts/dev3-control-format.md',
                'devdoc/contracts/dev3-selector-port.md',
                'devdoc/contracts/dev3-page-io.md',
                'devdoc/contracts/dev3-read-buffer.md',
                'devdoc/contracts/dev3-wal-io.md',
                'devdoc/contracts/dev3-storage-session.md',
                'devdoc/contracts/dev3-doublewrite-install.md',
                'devdoc/contracts/dev3-snapshot-model.md']}
        write_json(session / 'manifest.json', report)
        report['toolchain'] = run('toolchain', ['rustc', '-Vv'])
        self_tests = run('checker-self-tests', ['python3', '-B', '-m', 'unittest', 'discover',
                                               '-s', 'scripts', '-p', 'test_check_dev3_leaf.py'])
        if not re.search(r'^Ran 7 tests in .+\n\nOK$', self_tests, re.M):
            raise ValueError('checker self-test inventory did not execute')
        borrow_self_tests = run('buffer-borrow-checker-self-tests', ['python3', '-B', '-m', 'unittest',
                                    'discover', '-s', 'scripts', '-p', 'test_check_dev3_buffer_borrows.py'])
        if not re.search(r'^Ran 7 tests in .+\n\nOK$', borrow_self_tests, re.M):
            raise ValueError('buffer borrow checker self-test inventory did not execute')
        borrow_report = session / 'buffer-borrows.json'
        run('buffer-borrow-compiler-checks', ['python3', '-B', 'scripts/check_dev3_buffer_borrows.py',
            '--artifact-parent', str(session), '--report', str(borrow_report)])
        borrow_receipt = json.loads(borrow_report.read_text())
        expected_cases = {'baseline', 'pool-positive', 'pool-negative', 'pin-positive', 'pin-negative',
                          'frames-positive', 'frames-negative'}
        if (borrow_receipt.get('status') != 'PASS'
                or borrow_receipt.get('source_sha256') != report['source_sha256']
                or len(borrow_receipt.get('cases', [])) != 7
                or {row['name'] for row in borrow_receipt['cases']} != expected_cases
                or any(row.get('status') != 'PASS' for row in borrow_receipt['cases'])):
            raise ValueError('buffer borrow compiler checks did not verify this source')
        report['buffer_borrows'] = {'report': str(borrow_report), 'sha256': digest(borrow_report)}
        wal_borrow_report = session / 'wal-borrows.json'
        run('wal-borrow-compiler-checks', ['python3', '-B', 'scripts/check_dev3_wal_borrows.py',
            '--artifact-parent', str(session), '--report', str(wal_borrow_report)])
        wal_receipt = json.loads(wal_borrow_report.read_text())
        if (wal_receipt.get('status') != 'PASS'
                or wal_receipt.get('source_sha256') != report['source_sha256']
                or len(wal_receipt.get('cases', [])) != 4
                or {row['name'] for row in wal_receipt['cases']} != {'baseline', 'prepared', 'slot', 'two_sessions'}
                or any(row.get('status') != 'PASS' for row in wal_receipt['cases'])):
            raise ValueError('WAL borrow compiler checks did not verify this source')
        report['wal_borrows'] = {'report': str(wal_borrow_report), 'sha256': digest(wal_borrow_report)}
        storage_self_tests = run('storage-checker-self-tests', ['python3', '-B', '-m', 'unittest',
                                 'discover', '-s', 'scripts', '-p', 'test_dev3_storage_checks.py'])
        if not re.search(r'^Ran 3 tests in .+\n\nOK$', storage_self_tests, re.M):
            raise ValueError('storage checker counterexamples did not execute')
        storage_borrow_report = session / 'storage-borrows.json'
        run('storage-borrow-compiler-checks', ['python3', '-B', 'scripts/check_dev3_storage_borrows.py',
            '--artifact-parent', str(session), '--report', str(storage_borrow_report)])
        storage_receipt = json.loads(storage_borrow_report.read_text())
        if (storage_receipt.get('status') != 'PASS'
                or storage_receipt.get('source_sha256') != report['source_sha256']
                or len(storage_receipt.get('cases', [])) != 8
                or {row['name'] for row in storage_receipt['cases']} !=
                   {'baseline', 'shared_thread', 'buffer_thread', 'session', 'slot', 'root', 'scratch', 'writer'}
                or any(row.get('status') != 'PASS' for row in storage_receipt['cases'])):
            raise ValueError('storage borrow compiler checks did not verify this source')
        report['storage_borrows'] = {'report': str(storage_borrow_report), 'sha256': digest(storage_borrow_report)}
        doublewrite_self_tests = run('doublewrite-checker-self-tests', ['python3', '-B', '-m', 'unittest',
                                     'discover', '-s', 'scripts', '-p', 'test_dev3_doublewrite_checks.py'])
        if not re.search(r'^Ran 4 tests in .+\n\nOK$', doublewrite_self_tests, re.M):
            raise ValueError('doublewrite checker counterexamples did not execute')
        doublewrite_borrow_report = session / 'doublewrite-borrows.json'
        run('doublewrite-borrow-compiler-checks', ['python3', '-B', 'scripts/check_dev3_doublewrite_borrows.py',
            '--artifact-parent', str(session), '--report', str(doublewrite_borrow_report)])
        doublewrite_receipt = json.loads(doublewrite_borrow_report.read_text())
        if (doublewrite_receipt.get('status') != 'PASS'
                or doublewrite_receipt.get('source_sha256') != report['source_sha256']
                or len(doublewrite_receipt.get('cases', [])) != 9
                or {row['name'] for row in doublewrite_receipt['cases']} !=
                   {'baseline', 'fresh', 'active_reader', 'workspace', 'slot', 'installed_reader', 'raw_stage', 'release', 'shared_thread'}
                or any(row.get('status') != 'PASS' for row in doublewrite_receipt['cases'])):
            raise ValueError('doublewrite borrow compiler checks did not verify this source')
        report['doublewrite_borrows'] = {'report': str(doublewrite_borrow_report), 'sha256': digest(doublewrite_borrow_report)}
        wal_self_tests = run('wal-model-checker-self-tests', ['python3', '-B', '-m', 'unittest',
                            'discover', '-s', 'scripts', '-p', 'test_dev3_wal_models.py'])
        if not re.search(r'^Ran 3 tests in .+\n\nOK$', wal_self_tests, re.M):
            raise ValueError('WAL model checker self-test inventory did not execute')
        vectors = run('independent-vectors', ['python3', '-B', 'scripts/dev3_page_vectors.py'])
        report['vector_result'] = check_vectors(vectors)
        vectors = run('independent-disk-vectors', ['python3', '-B', 'scripts/dev3_disk_vectors.py'])
        report['disk_vector_result'] = check_disk_vectors(vectors)
        page = build('xadb-engine', ['--lib'], 'engine', 'page')
        output = run('page', [page, 'storage::page::tests::', '--nocapture', '--test-threads=1'])
        report['page_tests'] = check_page_output(output, 0)
        output = run('key', [page, 'storage::key::tests::', '--nocapture', '--test-threads=1'])
        report['key_tests'] = check_named_tests(output, 0, 'storage::key::tests::', KEY_TESTS)
        for name, expected in [('anchor', ANCHOR_TESTS), ('undo_ref', UNDO_REF_TESTS),
                               ('tree_page', TREE_TESTS), ('overflow', OVERFLOW_TESTS),
                               ('transaction_record', TRANSACTION_TESTS),
                               ('status_home', STATUS_HOME_TESTS),
                               ('obligation', OBLIGATION_TESTS),
                               ('ownership', OWNERSHIP_TESTS),
                               ('semantic_group', SEMANTIC_GROUP_TESTS),
                               ('wal_frame', WAL_TESTS), ('allocation_page', ALLOCATION_TESTS),
                               ('row_record', ROW_TESTS), ('doublewrite_control', DOUBLEWRITE_TESTS),
                               ('catalog_record', CATALOG_TESTS), ('undo_record', UNDO_RECORD_TESTS),
                               ('control', CONTROL_TESTS), ('page_io', PAGE_IO_TESTS),
                               ('read_buffer', READ_BUFFER_TESTS), ('wal_io', WAL_IO_TESTS),
                               ('wal_io::bootstrap', STORAGE_TESTS),
                               ('wal_io::bootstrap::doublewrite_install', DOUBLEWRITE_INSTALL_TESTS)]:
            prefix = f'storage::{name}::tests::'
            # Intentional poison panics must not split libtest's named result line.
            output_mode = '--show-output' if name in ('read_buffer', 'wal_io', 'wal_io::bootstrap', 'wal_io::bootstrap::doublewrite_install') else '--nocapture'
            output = run(name, [page, prefix, output_mode, '--test-threads=1'])
            report[f'{name}_tests'] = check_named_tests(output, 0, prefix, expected)
            if name == 'wal_io':
                models = {'slot': evaluate_wal_slot(), 'durable': evaluate_wal_durable(output, source),
                          'source_sha256': report['source_sha256'],
                          'producer_binary': page, 'producer_binary_sha256': digest(Path(page)),
                          'producer_log': str(session / 'wal_io.log'), 'producer_log_sha256': digest(session / 'wal_io.log')}
                model_report = session / 'wal-models.json'
                write_json(model_report, models)
                report['wal_models'] = {'report': str(model_report), 'sha256': digest(model_report)}
            if name == 'wal_io::bootstrap':
                model_report = session / 'storage-model.json'
                model_result = evaluate_storage_model(output, source)
                model_result.update(source_sha256=report['source_sha256'], producer_binary=page,
                                    producer_binary_sha256=digest(Path(page)),
                                    producer_log=str(session / 'wal_io::bootstrap.log'),
                                    producer_log_sha256=digest(session / 'wal_io::bootstrap.log'))
                write_json(model_report, model_result)
                report['storage_model'] = {'report': str(model_report), 'sha256': digest(model_report)}
            if name == 'wal_io::bootstrap::doublewrite_install':
                model_report = session / 'doublewrite-model.json'
                model_result = evaluate_doublewrite_model(output, source)
                model_result.update(source_sha256=report['source_sha256'], producer_binary=page,
                                    producer_binary_sha256=digest(Path(page)),
                                    producer_log=str(session / f'{name}.log'),
                                    producer_log_sha256=digest(session / f'{name}.log'))
                write_json(model_report, model_result)
                report['doublewrite_model'] = {'report': str(model_report), 'sha256': digest(model_report)}
        scalar = build('xadb-foundation', ['--test', 'key_buffer'], 'key_buffer', 'scalar-key')
        output = run('scalar-key', [scalar, '--nocapture', '--test-threads=1'])
        report['scalar_key_tests'] = check_named_tests(output, 0, '', SCALAR_KEY_TESTS)
        for name, package, target_args, target_name, prefix, test_filter, expected in [
                ('selector-platform', 'xadb-platform', ['--test', 'contracts'], 'contracts',
                 '', 'typed_selector', SELECTOR_PLATFORM_TESTS),
                ('selector-protocol', 'xadbtest-protocol', ['--lib'], 'test_protocol',
                 'fault::tests::', 'fault::tests::dev3_selector_', SELECTOR_PROTOCOL_TESTS),
                ('selector-worker', 'xadbtest-engine-worker', ['--lib'], 'engine_worker',
                 'fault::tests::', 'fault::tests::dev3_selector_', SELECTOR_WORKER_TESTS),
                ('metadata-platform', 'xadb-platform', ['--test', 'namespace_metadata'],
                 'namespace_metadata', '', '', METADATA_PLATFORM_TESTS),
                ('metadata-worker', 'xadbtest-engine-worker', ['--lib'], 'engine_worker',
                 'fault::tests::', 'fault::tests::dev3_open_metadata_', METADATA_WORKER_TESTS),
                ('metadata-performance', 'xadbtest-engine-worker', ['--lib'], 'engine_worker',
                 'durable_performance::ports::tests::',
                 'durable_performance::ports::tests::open_metadata_', METADATA_PERFORMANCE_TESTS),
                ('page-io-native', 'xadbtest-engine-worker', ['--test', 'page_io'], 'page_io',
                 '', '', PAGE_IO_NATIVE_TESTS),
                ('read-buffer-native', 'xadbtest-engine-worker', ['--test', 'read_buffer'], 'read_buffer',
                 '', '', READ_BUFFER_NATIVE_TESTS),
                ('wal-io-native', 'xadbtest-engine-worker', ['--test', 'genesis_wal'], 'genesis_wal',
                 '', '', WAL_IO_NATIVE_TESTS),
                ('storage-native', 'xadbtest-engine-worker', ['--test', 'storage_session'], 'storage_session',
                 '', '', STORAGE_NATIVE_TESTS),
                ('doublewrite-native', 'xadbtest-engine-worker', ['--test', 'doublewrite_install'], 'doublewrite_install',
                 '', '', DOUBLEWRITE_INSTALL_NATIVE_TESTS)]:
            binary = build(package, target_args, target_name, name)
            # Keep witness prints out of libtest's in-progress `test ...` line.
            # --show-output preserves successful-test witnesses after each result.
            output = run(name, [binary, test_filter, '--show-output', '--test-threads=1'])
            report[f'{name}_tests'] = check_named_tests(output, 0, prefix, expected)
            if name == 'storage-native':
                native_report = session / 'storage-native.json'
                native_result = evaluate_storage_native(output, source, session / 'storage-native-artifacts',
                                                        binary, report['source_sha256'])
                native_result.update(producer_log=str(session / 'storage-native.log'),
                                     producer_log_sha256=digest(session / 'storage-native.log'))
                write_json(native_report, native_result)
                report['storage_native'] = {'report': str(native_report), 'sha256': digest(native_report)}
            if name == 'doublewrite-native':
                native_report = session / 'doublewrite-native.json'
                native_result = evaluate_doublewrite_native(output, source, session / 'storage-native-artifacts',
                                                            binary, report['source_sha256'])
                native_result.update(producer_log=str(session / 'doublewrite-native.log'),
                                     producer_log_sha256=digest(session / 'doublewrite-native.log'))
                write_json(native_report, native_result)
                report['doublewrite_native'] = {'report': str(native_report), 'sha256': digest(native_report)}
        model = build('xadbtest-reference', ['--test', 'dev3_snapshot_model'],
                      'dev3_snapshot_model', 'model')
        output = run('model', [model, MODEL_TEST, '--exact', '--nocapture'],
                     timeout=780)
        report['model_cases'] = check_model_output(output, 0)
        # Harness budget guards are ordinary tests in the same retained binary.
        budget = run('model-budget-tests', [model, MODEL_BUDGET_TEST, '--exact', '--test-threads=1'])
        if (not re.search(rf'^test {MODEL_BUDGET_TEST} \.\.\. ok$', budget, re.M)
                or not re.search(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;', budget, re.M)):
            raise ValueError('model budget guard test did not execute')
        if args.with_regressions:
            regression = session / 'regression.json'
            run('regressions', ['python3', '-B', 'scripts/check_dev2_reference.py',
                               '--with-substrate', '--report', str(regression)], timeout=3600)
            receipt = json.loads(regression.read_text())
            if receipt.get('status') != 'PASS' or receipt.get('source_sha256') != report['source_sha256']:
                raise ValueError('lower regression did not verify this source')
            report['regression'] = {'report': str(regression), 'sha256': digest(regression)}
        else:
            report['regression'] = {'status': 'NOT_RUN'}
        report['source_sha256_after'] = source_fingerprint(source)
        if report['source_sha256_after'] != report['source_sha256']:
            raise ValueError('retained source changed during verification')
        for path, expected in report['contract_sha256'].items():
            if digest(source / path) != expected:
                raise ValueError(f'retained contract changed: {path}')
        for binary in report['builds']:
            if digest(Path(binary['binary'])) != binary['sha256']:
                raise ValueError('retained binary changed during verification')
        for path, expected in LOADED_CHECKER_HASHES.items():
            if digest(ROOT / path) != expected:
                raise ValueError(f'loaded checker changed during verification: {path}')
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        code = 1
    write_json(session / 'validation.json', report)
    if args.report:
        write_json(args.report, report)
    print(f'DEV3 leaf {report["status"]}: {session / "validation.json"}', flush=True)
    return code


if __name__ == '__main__':
    raise SystemExit(main())
