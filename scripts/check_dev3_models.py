#!/usr/bin/env python3
"""Retain and verify selected DEV3 finite models; never grant DEV3/S1 acceptance."""
import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
CHECKER_DIRECTORY = Path(__file__).resolve().parent
CHECKER_NAMES = ('check_dev3_models.py', 'check_dev3_leaf.py', 'check_dev0.py',
                 'comm_campaign.py')
# Hash before loading the local helper modules, then compare with the retained
# export. A changed working checker must not certify another checker's source.
LOADED_CHECKER_HASHES = {
    'scripts/' + name: hashlib.sha256((CHECKER_DIRECTORY / name).read_bytes()).hexdigest()
    for name in CHECKER_NAMES}
from check_dev0 import check_report_paths, digest, source_fingerprint
from check_dev3_leaf import run_logged
from comm_campaign import capture_source, identity, write_json

ROOT = Path(__file__).resolve().parent.parent


@dataclass(frozen=True)
class Model:
    scope: str
    target: str
    test: str
    auxiliary: tuple
    contract: str
    prefix: str
    normal: dict
    negative: dict
    actions: frozenset
    aggregate: frozenset = frozenset()
    completion: str | None = None
    replay_flags: bool = False
    bound_field: bool = False
    empty_witness: bool = True
    preludes: dict | None = None


M01_WITNESSES = frozenset({
    'four_undo_live', 'both_previous_links', 'reader_observed', 'reader_released',
    'rollback_before_cut', 'rollback_after_cut', 'rollback_complete', 'index_matches_history',
})
M01_NORMAL = {
    f'{layout}_{rollback}_q{q}_{start}':
        M01_WITNESSES | ({'parallel_edges_same_target'} if layout == 'same' else set())
    for layout, rollback, q, start in itertools.product(
        ('distinct', 'same'), ('statement', 'full'), (10, 11), ('before_b', 'after_b'))
}
M01_ERRORS = {
    'stale_before_link': 'dangling_edge', 'missing_increment': 'incoming_count',
    'double_decrement': 'incoming_count', 'cut_transaction_link': 'rollback_chain',
    'cut_needed_history': 'reader_history', 'free_with_incoming': 'dangling_edge',
}
M01_NEGATIVE = {
    (f'{layout}_{rollback}_q{q}_after_b', mutant): violation
    for mutant, violation in M01_ERRORS.items()
    for layout in ('distinct', 'same')
    for rollback in ('full' if layout == 'distinct' or mutant == 'stale_before_link'
                     else 'statement',)
    for q in (10 if mutant == 'cut_needed_history' else 11,)
}
M01_ACTIONS = {
    'b_update', 'rollback_begin', 'rollback_step', 'reader_observe', 'reader_release',
    'horizon_sample', 'queue_step', 'queue_finish', 'aging_finish',
}
for action, count in [('stamp', 8), ('drop_terminal_head', 4),
                      ('queue_begin', 4), ('reuse_undo', 4), ('reuse_status', 4)]:
    M01_ACTIONS.update(f'{action}:{i}' for i in range(count))
M01_ACTIONS.update(f'cut:{i}' for i in (0, 1, 2, 3, 8, 9, 11, 12, 14, 15, 17, 18))

M02_ACQUIRE = {
    'q10_selected', 'registration_gate_blocks_aging', 'q10_read',
    'registration_cancel_clean', 'registration_failure_clean', 'guard_overlap',
    'guard_transfer_complete', 'two_writers_published',
}
M02_CHAIN = {
    'root_copied', 'old_pin_before_unlatch', 'old_loaded', 'next_copied',
    'two_pins_overlap', 'old_released_after_next_pin', 'next_loaded', 'all_pins_released',
}
M02_HANDOFF = {
    'next_copied', 'two_pins_overlap', 'next_target_reuse_blocked',
    'next_loaded', 'next_target_reused_after_pin_release',
}
M02_ROUTES = ('undo_undo', 'status_undo', 'undo_status')
M02_NORMAL = {
    f'{family}_{layout}': M02_ACQUIRE | (
        {'q12_read', 'empty_registry_frontier10_retained'} if family == 'unpublished' else set())
    for family, layout in itertools.product(('registration', 'unpublished'), ('distinct', 'same'))
}
M02_NORMAL.update({f'chain_{route}_{root}': M02_CHAIN
                   for route, root in itertools.product(M02_ROUTES, ('row', 'index'))})
M02_NORMAL.update({f'handoff_{route}_row': M02_HANDOFF for route in M02_ROUTES})
M02_NEGATIVE = {
    (f'{family}_{layout}', mutant): violation
    for family, mutant, violation in [
        ('registration', 'early_gate_release', 'reader_history'),
        ('unpublished', 'unpublished_horizon', 'published_history'),
        ('unpublished', 'empty_registry_infinity', 'published_history'),
        ('registration', 'guard_handoff_gap', 'guard_owner_lost')]
    for layout in ('distinct', 'same')
}
M02_NEGATIVE.update({
    (f'chain_{route}_{root}', mutant): 'transient_lifetime'
    for route, root, mutant in itertools.product(
        ('undo_undo', 'status_undo'), ('row', 'index'),
        ('pin_after_unlatch', 'snapshot_only_pin', 'ignore_transient_pin'))
})
M02_NEGATIVE.update({(f'handoff_{route}_row', 'next_pin_handoff_gap'): 'transient_lifetime'
                     for route in M02_ROUTES})
M02_ACTIONS = (M01_ACTIONS - {
    'reader_release', 'rollback_begin', 'rollback_step',
}) | {
    'q_read', 'horizon_register', 'registration_cancel', 'registration_fail',
    'status_install:0', 'status_install:1', 'frontier_publish:0', 'frontier_publish:1',
    'guard_transfer_begin', 'guard_transfer_register', 'guard_transfer_release_old',
    'guard_release', 'copy_root', 'pin_old', 'unlatch_root', 'load_old', 'copy_next',
    'pin_next', 'release_old', 'unlatch_next', 'load_next', 'release_next',
}
# Exact prepared prefixes follow the contract's field-order cleanup for case ordinals
# 10/11/12. They are checked separately from the subsequent depth-16 exploration.
M02_PRELUDE_BASE = [
    'copy_root', 'pin_old', 'unlatch_root', 'load_old', 'horizon_sample',
    'stamp:0', 'stamp:1', 'stamp:2', 'stamp:3', 'cut:0', 'cut:1', 'cut:2', 'cut:3', 'cut:14',
]
M02_PRELUDES = {fixture: [] for fixture in M02_NORMAL}
M02_PRELUDES.update({
    'handoff_undo_undo_row': M02_PRELUDE_BASE + [
        'drop_terminal_head:2', 'drop_terminal_head:3', 'aging_finish'],
    'handoff_status_undo_row': M02_PRELUDE_BASE + [
        'cut:17', 'drop_terminal_head:3', 'aging_finish'],
    'handoff_undo_status_row': M02_PRELUDE_BASE + [
        'cut:17', 'drop_terminal_head:0', 'drop_terminal_head:1', 'aging_finish'],
})

M04_NORMAL = {
    'clr_chain': {'append_apply_gap', 'clr_atomic', 'home_flush', 'checkpoint_capture',
                  'reader_winner_loser', 'two_crashes', 'group_frames_three'},
    'partial_clr': {'recovery_writer_clr', 'partial_clr_recrash', 'remaining_three',
                    'reader_winner_loser', 'writer_recrash'},
    'cut_queue': {'cut_count_queue', 'cursor_one_generation', 'cursor_two_generation',
                  'home_flush', 'two_crashes'},
    'commit_capture': {'commit_capture_blocked', 'committed_capture',
                       'reader_winner_loser', 'two_crashes'},
    'begin_removed': {'analysis_without_begin', 'boot_reserve', 'reservation_restored',
                      'redo_all_pages', 'restore_recrash', 'boot_recrash', 'writer_recrash',
                      'recovery_writer_clr'},
}
M04_NEGATIVE = {
    ('clr_chain', 'progress_without_effect'): 'clr_effect_progress',
    ('cut_queue', 'count_without_cut'): 'edge_count',
    ('cut_queue', 'release_without_cursor'): 'queue_release_once',
    ('commit_capture', 'omit_commit_status'): 'commit_status',
    ('clr_chain', 'capture_missing_dpt'): 'capture_lsn_closure',
    ('begin_removed', 'zero_restored_ledger'): 'reservation_restore',
    ('partial_clr', 'reuse_consumed_clr'): 'remaining_obligation',
    ('begin_removed', 'stale_home_ownership'): 'reservation_restore',
    ('begin_removed', 'redo_double_charge'): 'remaining_obligation',
    ('begin_removed', 'clr_before_writer'): 'writer_not_selected',
}
M04_ACTIONS = {
    'append_group', 'prepare_group_frames', 'apply_group', 'wal_sync',
    'checkpoint_capture', 'checkpoint_select', 'boot_reserve', 'analysis',
    'reserve_restore', 'redo_page', 'rebase', 'writer_select', 'verify_ready', 'early_clr',
}
M04_ACTIONS.update(f'home_flush_{i}' for i in range(8))
M04_ACTIONS.update(f'crash_prefix_{i}' for i in range(5))

M05_NORMAL = {
    'batch': {'batch_retired', 'two_file_repair', 'two_crashes', 'slot_reused',
              'repair_recrash', 'reader_ack', 'capture_pins', 'repair_workspace',
              'address_reused_after_retirement'},
    'retired_newer': {'raw_retired_ignored', 'two_crashes', 'reader_ack'},
    'redirty': {'redirty_preserved', 'two_file_repair', 'repair_recrash'},
}
M05_NEGATIVE = {
    ('batch', 'pending_before_image_sync'): 'pending_image_invalid',
    ('batch', 'home_before_pending_sync'): 'home_without_durable_pending',
    ('batch', 'retire_after_first_file'): 'retirement_without_all_file_sync',
    ('batch', 'partial_retirement'): 'partial_batch_retirement',
    ('batch', 'reuse_before_retirement'): 'pending_image_invalid',
    ('retired_newer', 'raw_retired_resurrection'): 'older_image_over_newer_home',
    ('redirty', 'clear_redirty'): 'redirty_marked_clean',
    ('batch', 'address_reuse_before_retirement'): 'pending_address_reused',
    ('batch', 'omit_wal_redo'): 'ack_or_repair_order_lost',
}
M05_ACTIONS = {
    'capture', 'capture_finish', 'wal_redo', 'wal_sync', 'image_sync', 'select_pending',
    'control_sync', 'select_retired', 'slot_reuse', 'boot_select', 'redo_ready',
}
for action in ('repair_acquire', 'target_reuse', 'image_write', 'home_write',
               'home_torn', 'file_sync'):
    M05_ACTIONS.update(f'{action}_{i}' for i in range(2))
M05_ACTIONS.update(f'crash_{mask:02x}' for mask in range(128))

# Fixed case inventories come from the owner contracts, never from candidate output.
# Other models remain unregistered until their reviewed receipt schema is ready.
MODELS = {
    'M01': Model(
        scope='D3-M01', target='dev3_undo_model', test='undo_graph_m01_exhaustive',
        auxiliary=('undo_graph_budget_exhaustion_is_inconclusive',),
        contract='devdoc/contracts/dev3-undo-model.md', prefix='XADB_DEV3_UNDO_CASE',
        normal=M01_NORMAL, negative=M01_NEGATIVE, actions=frozenset(M01_ACTIONS),
        aggregate=frozenset(f'{kind}_reused_{i}' for kind in ('undo', 'status')
                            for i in range(4)),
        completion='XADB_DEV3_UNDO_COMPLETE', bound_field=True),
    'M02': Model(
        scope='D3-M02', target='dev3_undo_model',
        test='snapshot_graph::snapshot_graph_m02_exhaustive',
        auxiliary=('snapshot_graph::snapshot_graph_budget_exhaustion_is_inconclusive',),
        contract='devdoc/contracts/dev3-undo-model.md', prefix='XADB_DEV3_GRAPH_SNAPSHOT_CASE',
        normal=M02_NORMAL, negative=M02_NEGATIVE, actions=frozenset(M02_ACTIONS),
        completion='XADB_DEV3_GRAPH_SNAPSHOT_COMPLETE', bound_field=True,
        preludes=M02_PRELUDES),
    'M04': Model(
        scope='D3-M04-F01', target='dev3_recovery_model',
        test='recovery_atomic_groups_exhaustive',
        auxiliary=('recovery_model_budget_exhaustion_is_inconclusive',),
        contract='devdoc/contracts/dev3-recovery-model.md', prefix='XADB_DEV3_RECOVERY_MODEL',
        normal=M04_NORMAL, negative=M04_NEGATIVE, actions=frozenset(M04_ACTIONS),
        replay_flags=True, empty_witness=False),
    'M05': Model(
        scope='D3-M05-F01', target='dev3_doublewrite_model',
        test='doublewrite_two_file_exhaustive',
        auxiliary=('doublewrite_model_budget_exhaustion_is_inconclusive',
                   'doublewrite_repair_workspace_does_not_pin_completed_page'),
        contract='devdoc/contracts/dev3-doublewrite-model.md',
        prefix='XADB_DEV3_DOUBLEWRITE_MODEL', normal=M05_NORMAL, negative=M05_NEGATIVE,
        actions=frozenset(M05_ACTIONS), replay_flags=True, empty_witness=False),
}


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate JSON member: {key}')
        result[key] = value
    return result


def records(output, prefix):
    return [json.loads(line[len(prefix) + 1:], object_pairs_hook=unique_object)
            for line in output.splitlines() if line.startswith(prefix + ' ')]


def check_one_test(output, exit_code, name):
    tests = re.findall(r'^test (\S+) \.\.\. (\S+)$', output, re.M)
    summaries = re.findall(r'^test result: (\S+)\. (\d+) passed; (\d+) failed; (\d+) ignored;',
                           output, re.M)
    if exit_code or tests != [(name, 'ok')] or summaries != [('ok', '1', '0', '0')]:
        raise ValueError(f'missing, duplicate or failed test: {name}')


def valid_trace(trace, actions, *, empty=False):
    return (isinstance(trace, list) and (0 if empty else 1) <= len(trace) <= 16
            and all(isinstance(action, str) and action in actions for action in trace))


def check_model_output(output, exit_code, model):
    check_one_test(output, exit_code, model.test)
    cases = records(output, model.prefix)
    normal, negative, aggregate = set(), set(), set()
    if model.completion:
        expected = {'scope': model.scope, 'status': 'PASS', 'normal_cases': len(model.normal),
                    'negative_cases': len(model.negative)}
        if records(output, model.completion) != [expected]:
            raise ValueError('missing or inconsistent model completion')
    for case in cases:
        if not isinstance(case, dict) or case.get('scope') != model.scope:
            raise ValueError('invalid model case or scope')
        if case.get('error') is not None or case.get('missing_witnesses', []) != []:
            raise ValueError('model reports an execution error')
        for field, minimum, maximum in (
                ('states', 1, 1_000_000), ('transitions', 1, 8_000_000),
                ('max_depth', 1, 16), ('elapsed_ms', 0, 59_999),
                ('unexplored', 0, 1_000_000)):
            value = case.get(field)
            if type(value) is not int or not minimum <= value <= maximum:
                raise ValueError(f'invalid or exceeded {field}')
        if model.bound_field and (type(case.get('bound_depth')) is not int
                                  or case['bound_depth'] != 16):
            raise ValueError('changed exploration bound')
        fixture, mutant = case.get('fixture'), case.get('mutant')
        if not isinstance(fixture, str) or not isinstance(mutant, str):
            raise ValueError('fixture and mutant must be names')
        if model.preludes is not None and case.get('prelude') != model.preludes.get(fixture):
            raise ValueError('missing or changed prepared fixture prelude')
        witnesses = case.get('witnesses')
        if (not isinstance(witnesses, dict)
                or not all(isinstance(name, str) and name
                           and valid_trace(trace, model.actions, empty=model.empty_witness)
                           for name, trace in witnesses.items())):
            raise ValueError('invalid witness trace')
        if mutant == 'none':
            if fixture in normal or fixture not in model.normal:
                raise ValueError('duplicate or unexpected normal fixture')
            if (case.get('status') != 'PASS' or case['unexplored'] != 0
                    or case.get('violation') is not None or case.get('counterexample') != []
                    or not model.normal[fixture] <= witnesses.keys()):
                raise ValueError('normal model incomplete or missing a required witness')
            if model.replay_flags and case.get('witnesses_replayed') is not True:
                raise ValueError('normal witness replay not confirmed')
            normal.add(fixture)
            aggregate.update(witnesses)
        else:
            pair = fixture, mutant
            if pair in negative or pair not in model.negative:
                raise ValueError('duplicate or unexpected negative fixture')
            if (case.get('status') != 'EXPECTED_COUNTEREXAMPLE'
                    or case.get('violation') != model.negative[pair]
                    or not valid_trace(case.get('counterexample'), model.actions)):
                raise ValueError('mutant did not produce its specified counterexample')
            if model.replay_flags and case.get('counterexample_replayed') is not True:
                raise ValueError('counterexample replay not confirmed')
            negative.add(pair)
    if normal != model.normal.keys() or negative != model.negative.keys():
        raise ValueError('incomplete model case inventory')
    if not model.aggregate <= aggregate:
        raise ValueError('missing required campaign coverage')
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model', action='append',
                        choices=sorted(MODELS.keys() | {'M03', 'M06'}), required=True,
                        help='explicit model selection; no implied full-stage acceptance')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if len(args.model) != len(set(args.model)):
        parser.error('duplicate model selection')
    check_report_paths([args.report])
    parent = ROOT / 'artifacts/dev3'
    parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='models-', dir=parent))
    source = session / 'source'
    report = {
        'schema_version': 1, 'date': datetime.now(timezone.utc).isoformat(),
        'scope': 'selected DEV3 finite models only; no disk engine or S1 acceptance',
        'selected_models': args.model, 'status': 'RUNNING', 'origin': identity(ROOT),
        'artifact_directory': str(session), 'dev3_acceptance': 'INCOMPLETE',
        'checks': [], 'builds': [], 'models': {},
    }
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN',
                'RUST_TEST_THREADS'):
            environment.pop(key)
    environment['CARGO_TARGET_DIR'] = str(source / 'target')

    def run(name, command, timeout=600):
        return run_logged(name, command, source, environment, session, report['checks'], timeout)

    code = 0
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        report['loaded_checker_sha256'] = LOADED_CHECKER_HASHES
        for path, expected in LOADED_CHECKER_HASHES.items():
            if digest(source / path) != expected or digest(ROOT / path) != expected:
                raise ValueError(f'loaded checker differs from retained source: {path}')
        # Execute the exact retained helper that its self-tests exercise. It
        # imports only stdlib and never reads the live working tree.
        helper_path = source / 'scripts/dev3_progress_receipts.py'
        spec = importlib.util.spec_from_file_location('_xadb_retained_progress', helper_path)
        if spec is None or spec.loader is None:
            raise ImportError('cannot load retained progress receipt checker')
        helper = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = helper
        spec.loader.exec_module(helper)
        progress_models = helper.PROGRESS_MODELS
        check_group_output = helper.check_group_output
        checker_test_count = helper.CHECKER_TEST_COUNT
        if type(checker_test_count) is not int or checker_test_count != 24:
            raise ValueError('changed progress checker self-test inventory')
        report['retained_helper_sha256'] = digest(helper_path)
        contracts = {progress_models[name]['contract'] if name in progress_models
                     else MODELS[name].contract for name in args.model}
        report['contract_sha256'] = {path: digest(source / path) for path in contracts}
        write_json(session / 'manifest.json', report)
        report['toolchain'] = run('toolchain', ['rustc', '-Vv'])
        output = run('checker-self-tests', ['python3', '-B', '-m', 'unittest', 'discover',
                                            '-s', 'scripts', '-p', 'test_check_dev3_models.py'])
        if not re.search(r'^Ran 6 tests in .+\n\nOK$', output, re.M):
            raise ValueError('checker self-test inventory did not execute')
        if any(name in progress_models for name in args.model):
            output = run('progress-checker-self-tests', [
                'python3', '-B', '-m', 'unittest', 'discover', '-s', 'scripts',
                '-p', 'test_dev3_progress_receipts.py'])
            if not re.search(rf'^Ran {checker_test_count} tests in .+\n\nOK$', output, re.M):
                raise ValueError('progress checker self-test inventory did not execute')
        binaries = {}
        for name in args.model:
            progress = progress_models.get(name)
            model = MODELS.get(name)
            target = progress['target'] if progress else model.target
            if target not in binaries:
                output = run('build-' + target, [
                    'cargo', 'test', '-p', 'xadbtest-reference', '--release',
                    '--test', target, '--no-run', '--locked', '--offline',
                    '--message-format=json'])
                messages = [json.loads(line) for line in output.splitlines()
                            if line.startswith('{')]
                paths = [message['executable'] for message in messages
                         if message.get('reason') == 'compiler-artifact'
                         and message.get('target', {}).get('name') == target
                         and message.get('executable')]
                if len(paths) != 1 or not any(
                        m.get('reason') == 'build-finished' and m.get('success') is True
                        for m in messages):
                    raise ValueError('missing unique compiled model binary')
                binary = session / (target + '-binary')
                shutil.copy2(paths[0], binary)
                binaries[target] = str(binary)
                report['builds'].append({'target': target, 'binary': str(binary),
                                         'sha256': digest(binary)})
            binary = binaries[target]
            if progress:
                groups = []
                report['models'][name] = groups
                for index, test in enumerate(progress['tests']):
                    output = run(f'{name}-group-{index}',
                                 [binary, test, '--exact', '--nocapture'],
                                 timeout=60 * (progress['normal'] + progress['negative']) + 60)
                    group = check_group_output(name, test, output, 0)
                    groups.append(group)
                    if group.get('pass') is not True:
                        raise ValueError(f'{name}/{test}: {group.get("errors")}')
                auxiliary = progress['budget_tests']
            else:
                output = run(name, [binary, model.test, '--exact', '--nocapture'],
                             timeout=60 * (len(model.normal) + len(model.negative)) + 60)
                report['models'][name] = check_model_output(output, 0, model)
                auxiliary = model.auxiliary
            for index, test in enumerate(auxiliary):
                output = run(f'{name}-auxiliary-{index}', [binary, test, '--exact'])
                if progress:
                    group = check_group_output(name, test, output, 0)
                    groups.append(group)
                    if group.get('pass') is not True:
                        raise ValueError(f'{name}/{test}: {group.get("errors")}')
                else:
                    check_one_test(output, 0, test)
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
    except (OSError, ValueError, RuntimeError, ImportError, SyntaxError, AttributeError, KeyError,
            TypeError, AssertionError, subprocess.SubprocessError) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        code = 1
    write_json(session / 'validation.json', report)
    if args.report:
        write_json(args.report, report)
    print(f'DEV3 selected models {report["status"]}: {session / "validation.json"}', flush=True)
    return code


if __name__ == '__main__':
    raise SystemExit(main())
