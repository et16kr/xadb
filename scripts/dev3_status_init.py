#!/usr/bin/env python3
"""Run only frozen new INIT selections on a source-bound preserved test binary."""
import argparse
from collections import OrderedDict
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import subprocess
import sys
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from dev3_doublewrite_native import native_objects

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / 'tests/fixtures/dev3-status-init'
RUNTIME_INPUT = ROOT / 'crates/engine/tests/fixtures/dev3-status-init/runtime/manifest.json'
RUNTIME_SHA256 = '04a59a300db989ede8eb5ca0ef8c64f3150792530347bea750732a851a2a36f0'
BASE = 'storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
PREFIX = BASE + 'status_init::tests::'
WIRE = BASE + 'status_init_operation::tests::'
CF = {
    'CF01': (3, 'status_init_cf01_binding_preserves_closed_bytes'),
    'CF02': (3, 'status_init_cf02_unpublished_has_no_reader_admission'),
    'CF03': (9, 'protocol::status_init_cf03_exact_authority_and_returned_reservation'),
    'CF04': (3, 'protocol::status_init_cf04_whole_two_pin_capacity'),
    'CF05': (2, 'protocol::status_init_cf05_actual_content_busy_returns_whole_claim'),
    'CF06': (8, 'protocol::status_init_cf06_stale_and_closed_corruption'),
    'CF07': (3, 'status_init_cf07_profiles_publish_full_literal_images'),
    'CF08': (8, 'protocol::status_init_cf08_exact_partial_cuts_fence_before_release'),
    'CF09': (1, 'protocol::status_init_cf09_poststage_poison_preserves_old_images'),
    'CF10': (4, 'status_init_cf10_wal_failures_retain_installed_obligation'),
    'CF11': (4, 'protocol::status_init_cf11_checked_cleanup_preserves_first_fatal'),
    'CF12': (2, 'protocol::status_init_cf12_publication_and_concurrent_reader'),
    'CF15': (3, 'status_init_cf15_initialized_and_reused_binding_rejected'),
    'CF16': (2, 'status_init_cf16_retry_and_cancel_keep_exact_history'),
    'CF17': (5, 'status_init_cf17_actual_resource_and_capacity_bounds'),
    'CF18': (1, 'status_init_cf18_known_bad_witnesses'),
}
WI_NAMES = ('exact_init reserve_budget cancel_budget retry_init opcode work_kind birth_chain native_count status_before status_span redo_index consume_action token_birth token_incarnation memory_budget wal_budget frame_budget protected_budget role_generation role_kind role_duplicate payload_owner canonical_free cross_target_mismatch').split()
WM_NAMES = ('zero_output torn_output truncated inner_page_checksum swapped_target wrong_before_owner already_allocated newer_before').split()
SELECTORS = OrderedDict()
for group, (count, selector) in CF.items():
    for i in range(1, count + 1):
        SELECTORS[f'{group}-{i:02}'] = PREFIX + selector
for i in (2, 3):
    SELECTORS[f'CF17-{i:02}'] = PREFIX + 'resources::status_init_cf17_padded_readback_admission'
SELECTORS['CF17-05'] = PREFIX + 'resources::status_init_cf17_trusted_credit_arithmetic'
for family, names in (('WI', WI_NAMES), ('WM', WM_NAMES)):
    for i, suffix in enumerate(names, 1):
        SELECTORS[f'{family}-{i:02}'] = WIRE + f'status_init_{family.lower()}{i:02}_{suffix}'
MANDATORY = tuple(SELECTORS)
for profile in (2, 3):
    for variant in range(1, 10):
        SELECTORS[f'SA-P{profile}-{variant:02}'] = PREFIX + 'protocol::status_init_s5_authority_expanded'
for profile in (2, 3):
    for cut in [f'C{i}' for i in range(1, 7)] + ['SYNC-BEFORE', 'SYNC-AFTER-ERROR', 'SYNC-AFTER-PANIC']:
        SELECTORS[f'SS-P{profile}-{cut}'] = PREFIX + 'protocol::status_init_s5_install_schedules_expanded'
for profile in (1, 2, 3):
    for cancellations in (1, 2, 8):
        for terminal in ('INIT', 'CANCEL'):
            # Final actual CF16 aliases use profile3/count1, as frozen in source.
            if (profile, cancellations) != (3, 1):
                SELECTORS[f'SR-P{profile}-C{cancellations}-{terminal}'] = PREFIX + 'status_init_s5_retry_histories'
for family, count in (('WI', 24), ('WM', 8)):
    for profile in (2, 3):
        for i in range(1, count + 1):
            SELECTORS[f'S{family}-P{profile}-{i:02}'] = WIRE + f'status_init_{family.lower()}_s5_profiles'
EXPANDED = tuple(key for key in SELECTORS if key not in MANDATORY)
SELECTORS['SHARED'] = BASE + 'dirty_allocation::tests::dirty_shared_resource_after_closed_init'
for profile in (1, 2, 3):
    SELECTORS[f'P{profile}'] = PREFIX + 'native::status_init_native_worker'
NATIVE = ('P1', 'P2', 'P3')
GROUPS = {group: tuple(k for k in MANDATORY if k.startswith(group + '-')) for group in CF}
GROUPS.update(WI=tuple(k for k in MANDATORY if k.startswith('WI-')),
              WM=tuple(k for k in MANDATORY if k.startswith('WM-')),
              MANDATORY=MANDATORY, S5=EXPANDED, NATIVE=NATIVE)
CASE_MARKER = 'XADB_STATUS_INIT_CASE '
S5_MARKER = 'XADB_STATUS_INIT_S5_CASE '
VARIANT_MARKER = 'XADB_STATUS_INIT_VARIANT '
ORACLE = 'XADB_STATUS_INIT_ORACLE synthetic_valid=1 rejected_known_bad=8 runtime_interleaving=false'
BUILD_COMMAND = ['cargo', 'test', '-p', 'xadb-engine', '--lib', '--all-features',
                 '--no-run', '--locked', '--offline', '--message-format=json']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def check_build_receipt(build, source_root, source_sha256, binary_sha256):
    if (build.get('status') != 'PASS' or build.get('source_sha256') != source_sha256
            or build.get('binary_sha256') != binary_sha256
            or build.get('source_root') is None
            or Path(build['source_root']).resolve() != source_root.resolve()
            or build.get('command') != BUILD_COMMAND or build.get('exit_code') != 0
            or build.get('errors') != 0):
        raise ValueError('FAIL_BUILD_BINDING')


def selections(requested):
    selected = []
    for item in requested:
        rows = GROUPS.get(item, (item,))
        if any(row not in SELECTORS or row in selected for row in rows):
            raise ValueError('FAIL_DUPLICATE_OR_UNKNOWN_SELECTION')
        selected.extend(rows)
    if not selected:
        raise ValueError('FAIL_EMPTY_SELECTION')
    return [key for key in SELECTORS if key in selected]


def jobs(selected):
    result = OrderedDict()
    for key in selected:
        # Native profiles require separate processes despite sharing a selector.
        group = key if key in NATIVE else SELECTORS[key]
        result.setdefault(group, []).append(key)
    return list(result.values())


def check_runtime(output, selected):
    selector = SELECTORS[selected[0]]
    if any(SELECTORS[key] != selector for key in selected):
        raise ValueError('FAIL_MIXED_SELECTOR')
    if output.count('test result:') != 1 or 'test result: ok. 1 passed; 0 failed;' not in output:
        raise ValueError('FAIL_EXACT_TEST_CENSUS')
    tests = [line for line in output.splitlines() if line.startswith('test ') and not line.startswith('test result:')]
    prefix = f'test {selector} ...'
    if (len(tests) != 1 or not tests[0].startswith(prefix)
            or (len(tests[0]) > len(prefix) and tests[0][len(prefix)] != ' ')):
        raise ValueError('FAIL_EXACT_TEST_NAME')
    # libtest may append the first test diagnostic to its unfinished status line.
    # Strip only this exact selected-test prefix, retaining its entire suffix.
    lines = [line[len(prefix):].lstrip() if line.startswith(prefix) else line
             for line in output.splitlines()]
    cases = [line[len(CASE_MARKER):] for line in lines if line.startswith(CASE_MARKER)]
    enlarged = [line[len(S5_MARKER):] for line in lines if line.startswith(S5_MARKER)]
    wire_s5 = [key for key in selected if key.startswith(('SWI-', 'SWM-'))]
    wanted = [key for key in selected if key in MANDATORY]
    for order in runtime_input()['marker_order_overrides'].values():
        if any(key in wanted for key in order):
            wanted = [key for key in order if key in wanted]
    wanted += [key[1:3] + '-' + key[-2:] for key in wire_s5]
    if cases != wanted or enlarged != [key for key in selected if key in EXPANDED]:
        raise ValueError('FAIL_CASE_CENSUS')
    profiles = [line.removeprefix('STATUS_INIT_PROFILE ') for line in lines if line.startswith('STATUS_INIT_PROFILE ')]
    wanted_profiles = [f'{key} 1' for key in selected if key.startswith(('WI-', 'WM-'))]
    wanted_profiles += [f'{key[1:3]}-{key[-2:]} {key[5]}' for key in wire_s5]
    if profiles != wanted_profiles:
        raise ValueError('FAIL_PROFILE_CENSUS')
    variants = [line[len(VARIANT_MARKER):] for line in lines if line.startswith(VARIANT_MARKER)]
    expected_variants = []
    if 'CF11-02' in selected:
        expected_variants += ['CF11-02 count_mismatch', 'CF11-02 own_pins_flag']
    if 'CF12-02' in selected:
        # C6 publication Busy/retry is CF12-01; CF12-02 covers closed C1..C5.
        expected_variants += [f'CF12-02 C{i}' for i in range(1, 6)]
    if variants != expected_variants:
        raise ValueError('FAIL_VARIANT_CENSUS')
    oracles = [line for line in lines if line.startswith('XADB_STATUS_INIT_ORACLE')]
    if oracles != ([ORACLE] if 'CF18-01' in selected else []):
        raise ValueError('FAIL_ORACLE_CENSUS')
    native = [line for line in lines if line.startswith('XADB_STATUS_INIT_NATIVE_COMPLETE ')]
    expected_native = [f'XADB_STATUS_INIT_NATIVE_COMPLETE profile={key[1]} cancellations={int(key == "P3")} homes=unchanged' for key in selected if key in NATIVE]
    if native != expected_native:
        raise ValueError('FAIL_NATIVE_MARKER')
    wal = [line for line in lines if line.startswith('XADB_STATUS_INIT_WAL_FAILURE ')]
    expected_wal = [f'XADB_STATUS_INIT_WAL_FAILURE {key} actual_frame_bytes={19000 if key == "CF10-04" else 38000} sync_partial_bytes=not_applicable' for key in selected if key.startswith('CF10-')]
    if wal != expected_wal:
        raise ValueError('FAIL_WAL_FAULT_CENSUS')
    caps = runtime_input()['resource_caps']
    numeric = []
    for line in lines:
        if line.startswith(('XADB_STATUS_INIT_RESOURCE ', 'XADB_STATUS_INIT_NATIVE_LAYOUT ', 'DIRTY_TYPE ')):
            parts = line.split()
            if len(parts) != 4 or not parts[2].isdigit() or not parts[3].isdigit() or int(parts[2]) > int(parts[3]):
                raise ValueError('FAIL_RESOURCE_BOUND')
            if parts[0] == 'DIRTY_TYPE' and selected != ['SHARED']:
                raise ValueError('FAIL_FOREIGN_RESOURCE')
            if parts[0] != 'DIRTY_TYPE' and 'CF17-01' not in selected:
                raise ValueError('FAIL_FOREIGN_RESOURCE')
            name = (parts[0], parts[1])
            if caps.get(parts[0], {}).get(parts[1]) != int(parts[3]):
                raise ValueError('FAIL_FROZEN_RESOURCE_CAP')
            if name in [row[:2] for row in numeric]:
                raise ValueError('FAIL_DUPLICATE_RESOURCE')
            numeric.append((*name, int(parts[2]), int(parts[3])))
    wanted_numeric = []
    for category in (('XADB_STATUS_INIT_RESOURCE', 'XADB_STATUS_INIT_NATIVE_LAYOUT') if 'CF17-01' in selected else (('DIRTY_TYPE',) if selected == ['SHARED'] else ())):
        wanted_numeric += [(category, name) for name in caps[category]]
    if [row[:2] for row in numeric] != wanted_numeric:
        raise ValueError('FAIL_RESOURCE_CENSUS')
    padded = [line for line in lines if line.startswith('XADB_STATUS_INIT_PADDED ')]
    others = [line for line in lines if line.startswith('XADB_STATUS_INIT_PADDED_OTHER ')]
    wanted_padded = [key for key in selected if key in ('CF17-02', 'CF17-03')]
    if len(padded) != len(wanted_padded) or len(others) != len(wanted_padded):
        raise ValueError('FAIL_PADDED_CENSUS')
    for key, line, other in zip(wanted_padded, padded, others):
        match = re.fullmatch(r'XADB_STATUS_INIT_PADDED ' + key + r' owner=(\d+)/32768 reader=(\d+) file=(\d+)/16384 constructor=(\d+)/16384 carriers=32 total=(\d+)/65536 alignment=(\d+)', line)
        if match is None:
            raise ValueError('FAIL_PADDED_BOUND')
        owner, reader, file, constructor, total, alignment = map(int, match.groups())
        if not (owner <= 32768 and file <= 16384 and constructor <= 16384 and alignment > 0
                and total == owner + reader + file + constructor + 32 and total > 65536):
            raise ValueError('FAIL_PADDED_BOUND')
        match = re.fullmatch(r'XADB_STATUS_INIT_PADDED_OTHER ' + key + r' control=(\d+) bound=(\d+)/28672 result=(\d+)/28672 prepare=(\d+)/65536 stage=(\d+)/65536 install=(\d+)/65536 read=(\d+)/65536 wait=(\d+)/65536', other)
        if match is None:
            raise ValueError('FAIL_PADDED_OTHER_BOUND')
        _, bound, result, *chains = map(int, match.groups())
        if bound > 28672 or result > 28672 or any(value > 65536 for value in chains):
            raise ValueError('FAIL_PADDED_OTHER_BOUND')
    protected = [line for line in lines if line.startswith('XADB_STATUS_INIT_PROTECTED_LIMIT ')]
    exact_protected = 'XADB_STATUS_INIT_PROTECTED_LIMIT branch=NOT_APPLICABLE fixed=21024768 maximum=289488896 limit=1073741824 arithmetic_only=true'
    if protected != ([exact_protected] if 'CF17-05' in selected else []):
        raise ValueError('FAIL_PROTECTED_BOUND')
    # These diagnostics are source-bound; they never replace the successful case marker.
    allowed = ('XADB_STATUS_INIT_CASE ', 'XADB_STATUS_INIT_S5_CASE ', 'XADB_STATUS_INIT_VARIANT ',
               'XADB_STATUS_INIT_RESOURCE ', 'XADB_STATUS_INIT_NATIVE_LAYOUT ',
               'XADB_STATUS_INIT_NATIVE_COMPLETE ', 'XADB_STATUS_INIT_WAL_FAILURE ',
               'XADB_STATUS_INIT_ORACLE ', 'XADB_STATUS_INIT_PADDED ',
               'XADB_STATUS_INIT_PADDED_OTHER ', 'XADB_STATUS_INIT_PROTECTED_LIMIT ')
    for line in lines:
        if line.startswith('XADB_STATUS_INIT_') and not line.startswith(allowed):
            raise ValueError('FAIL_UNKNOWN_PROTOCOL')
    return {'cases': cases, 'enlarged': enlarged, 'variants': variants, 'profiles': profiles, 'resource_rows': numeric}


def runtime_input():
    if digest(RUNTIME_INPUT) != RUNTIME_SHA256:
        raise ValueError('FAIL_RUNTIME_INPUT_IDENTITY')
    return json.loads(RUNTIME_INPUT.read_text())


def check_inputs(root=ROOT):
    directory = root / 'tests/fixtures/dev3-status-init'
    manifest = json.loads((directory / 'manifest.json').read_text())
    if not manifest['canonical_fixture'] or len(manifest['literals']) != 858:
        raise ValueError('FAIL_INPUT_CENSUS')
    if digest(root / 'scripts/dev3_status_init_literals.py') != manifest['generator_sha256']:
        raise ValueError('FAIL_GENERATOR_IDENTITY')
    names = []
    for row in manifest['literals']:
        path = directory / row['path']
        if path.is_symlink() or path.stat().st_size != row['bytes'] or digest(path) != row['sha256']:
            raise ValueError('FAIL_INPUT_IDENTITY')
        names.append(row['path'])
    if len(set(names)) != 858 or set(names) != {p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file() and p.name != 'manifest.json'}:
        raise ValueError('FAIL_INPUT_CENSUS')
    for row in manifest['input_provenance']:
        path = root / row['path']
        if path.stat().st_size != row['bytes'] or digest(path) != row['sha256']:
            raise ValueError('FAIL_INPUT_DEPENDENCY')
    return digest(directory / 'manifest.json')


def native_expected(profile, root=ROOT):
    files = native_objects(root, profile, bytes(range(1, 17)))
    descriptor = bytearray(files['d3-bootstrap'])
    struct.pack_into('<Q', descriptor, 120, 2)
    struct.pack_into('<Q', descriptor, 280, 2)
    descriptor[4064:] = hashlib.sha256(descriptor[:4064]).digest()
    files['d3-bootstrap'] = bytes(descriptor)
    before = dict(files)
    cancellation = int(profile == 3)
    fixture = root / f'tests/fixtures/dev3-status-init/p{profile}-cancel{cancellation}-init'
    files['d3-wal-00000001'] += (fixture / 'wal.frames-only').read_bytes()
    return before, files, fixture


def inventory(path):
    rows = {}
    lines = path.read_text().splitlines()
    for line in lines:
        f = line.split('\t')
        if len(f) != 6 or f[0] in rows or '/' in f[0] or not re.fullmatch('[0-9a-f]{64}', f[5]):
            raise ValueError('FAIL_INVENTORY_SHAPE')
        rows[f[0]] = dict(length=int(f[1]), device=int(f[2]), inode=int(f[3]), links=int(f[4]), sha256=f[5])
        if rows[f[0]]['links'] != 1:
            raise ValueError('FAIL_INVENTORY_LINKS')
    if list(rows) != sorted(rows):
        raise ValueError('FAIL_INVENTORY_ORDER')
    return rows


def check_native(path, profile, root=ROOT):
    before, expected, fixture = native_expected(profile, root)
    prior, after = inventory(path / 'before.tsv'), inventory(path / 'after.tsv')
    if set(prior) != set(expected) or set(after) != set(expected):
        raise ValueError('FAIL_NATIVE_FILE_CENSUS')
    actual_names, total = set(), 0
    for file in sorted((path / 'database').iterdir()):
        meta = file.lstat()
        if not stat.S_ISREG(meta.st_mode) or meta.st_nlink != 1 or file.name not in expected:
            raise ValueError('FAIL_NATIVE_FILE_KIND')
        value = file.read_bytes()
        if value != expected[file.name]:
            raise ValueError('FAIL_NATIVE_FILE_BYTES')
        actual = dict(length=meta.st_size, device=meta.st_dev, inode=meta.st_ino, links=1, sha256=sha(value))
        if after[file.name] != actual:
            raise ValueError('FAIL_NATIVE_INVENTORY_ACTUAL')
        old = prior[file.name]
        if (old['length'], old['sha256']) != (len(before[file.name]), sha(before[file.name])):
            raise ValueError('FAIL_NATIVE_BEFORE_BYTES')
        if (old['device'], old['inode'], old['links']) != (actual['device'], actual['inode'], actual['links']):
            raise ValueError('FAIL_NATIVE_FILE_REPLACED')
        total += meta.st_size
        actual_names.add(file.name)
    if actual_names != set(expected) or total > 24 << 20:
        raise ValueError('FAIL_NATIVE_FILE_CENSUS')
    cancellation = int(profile == 3)
    work = cancellation + 1
    dumps = {'wal-header.bin': before['d3-wal-00000001'], 'wal.complete': expected['d3-wal-00000001'],
             'allocator.after': (fixture / f'work{work}-allocator.after').read_bytes(),
             'status.after': (fixture / f'work{work}-status.after').read_bytes(),
             'owner.descriptor': (fixture / f'work{work}-init.descriptor').read_bytes()}
    if any((path / name).read_bytes() != wanted for name, wanted in dumps.items()):
        raise ValueError('FAIL_NATIVE_RAM_WAL_DESCRIPTOR')
    events, offset = [], 4096
    lengths = [21200, 21200] * cancellation + [21200, 38000]
    for size in lengths:
        events += [f'write\td3-wal-00000001\t{offset}\t{size}\t{size}', 'sync\td3-wal-00000001\t0\t0\t0']
        offset += size
    if (path / 'events.tsv').read_text().splitlines() != events:
        raise ValueError('FAIL_NATIVE_EFFECTS')
    pairs = [line.split('\t') for line in (path / 'resources.tsv').read_text().splitlines()]
    if [pair[0] for pair in pairs] != ['wrapped_peak', 'wrapped_final', 'observed_fd_peak', 'observed_fd_final'] or any(len(pair) != 2 for pair in pairs):
        raise ValueError('FAIL_NATIVE_RESOURCE_SHAPE')
    resources = {k: int(v) for k, v in pairs}
    fd = []
    for line in (path / 'fd-events.tsv').read_text().splitlines():
        tag, count = line.split('\t')
        if tag not in ('baseline', 'read', 'write', 'sync', 'open', 'close') or not 0 <= int(count) <= 5:
            raise ValueError('FAIL_NATIVE_FD_EVENT')
        fd.append((tag, int(count)))
    # Bind/readback preceded enabled tracing. The current INIT-only worker has
    # no wrapped read/open afterwards: only these WAL writes/syncs and its drop.
    expected_tags = ['baseline'] + [line.split('\t')[0] for line in events] + ['close']
    if [tag for tag, _ in fd] != expected_tags or not fd or fd[-1] != ('close', 0):
        raise ValueError('FAIL_NATIVE_FD_SEQUENCE')
    if (fd[0] != ('baseline', 3) or not 1 <= resources['wrapped_peak'] <= 2
            or resources['wrapped_final'] != 0 or resources['observed_fd_final'] != 0
            or resources['observed_fd_peak'] != max(count for _, count in fd)
            or not 3 <= resources['observed_fd_peak'] <= 5):
        raise ValueError('FAIL_NATIVE_HANDLES')
    return {'profile': profile, 'cancellations': cancellation, 'effects': len(events),
            'files': after, 'total_bytes': total, 'resources': resources,
            'fd_scope': 'worker DB-anchored boundary observations; not continuous peak measurement',
            'homes': 'full initial home/DWB bytes unchanged; no flush/reopen credit'}


def remaining_seconds(selected, elapsed):
    family = 'native' if selected[0] in NATIVE else ('expanded' if selected[0] in EXPANDED else 'mandatory')
    limit = 30 if family == 'native' else (1200 if family == 'expanded' else 120)
    remaining = limit if family == 'native' else limit - elapsed[family]
    if remaining <= 0:
        raise ValueError('FAIL_AGGREGATE_BUDGET')
    return family, remaining


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-receipt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--case', action='append', required=True, choices=tuple(SELECTORS) + tuple(GROUPS))
    args = parser.parse_args()
    selected = selections(args.case)
    args.output.mkdir(parents=True, exist_ok=False)
    report = {'status': 'RUNNING', 'selected': selected, 'not_run': [k for k in SELECTORS if k not in selected],
              'inventory': {'mandatory': 93, 'expanded_additional': 116, 'shared_changed_resource': 1, 'native': 3},
              'results': [], 'elapsed': dict(mandatory=0.0, expanded=0.0, native=0.0)}
    environment = {k: v for k, v in os.environ.items() if not k.startswith(('XADB', 'RUST', 'CARGO_PROFILE_'))}
    try:
        build_receipt_sha256 = digest(args.build_receipt)
        build = json.loads(args.build_receipt.read_text())
        source, binary = source_fingerprint(ROOT), Path(build['binary'])
        check_build_receipt(build, ROOT, source, digest(binary))
        if digest(args.build_receipt) != build_receipt_sha256:
            raise ValueError('FAIL_BUILD_RECEIPT_IDENTITY')
        report.update(source_sha256=source, binary_sha256=digest(binary), build_receipt_sha256=build_receipt_sha256, inputs_sha256=check_inputs())
        runtime_input()
        report['runtime_input_sha256'] = RUNTIME_SHA256
        native_bytes = 0
        for number, chosen in enumerate(jobs(selected), 1):
            directory = args.output / f'{number:03}-{chosen[0]}'
            directory.mkdir()
            command = [str(binary), SELECTORS[chosen[0]], '--exact', '--nocapture', '--test-threads=1']
            if chosen[0] in NATIVE or chosen[0] in EXPANDED:
                command.append('--ignored')
            child_env = dict(environment, XADB_STATUS_INIT_CASES=','.join(chosen))
            if chosen[0] in NATIVE:
                child_env.update(XADB_STATUS_INIT_NATIVE_DIR=str(directory.resolve()), XADB_STATUS_INIT_NATIVE_PROFILE=chosen[0][1])
            family, timeout = remaining_seconds(chosen, report['elapsed'])
            row = dict(selected=chosen, selector=SELECTORS[chosen[0]], command=command, status='RUNNING', timeout_seconds=timeout, timed_out=False, artifact_directory=str(directory.resolve()))
            report['results'].append(row)
            started = time.monotonic()
            with (directory / 'stdout.log').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
                with subprocess.Popen(command, cwd=ROOT, env=child_env, stdout=stdout, stderr=stderr, start_new_session=True) as child:
                    row['pid'] = child.pid
                    wait_process_group(child, row, timeout)
            row['seconds'] = time.monotonic() - started
            report['elapsed'][family] += row['seconds']
            row.update(stdout_sha256=digest(directory / 'stdout.log'), stderr_sha256=digest(directory / 'stderr.log'))
            if row['exit_code'] != 0 or row['timed_out'] or row['seconds'] > timeout:
                raise ValueError('FAIL_WORKER_OR_BUDGET')
            row.update(check_runtime((directory / 'stdout.log').read_text(), chosen))
            if chosen[0] in NATIVE:
                row['native'] = check_native(directory, int(chosen[0][1]))
                native_bytes += row['native']['total_bytes']
                if native_bytes > 72 << 20:
                    raise ValueError('FAIL_NATIVE_TOTAL_BYTES')
            row['status'] = 'PASS'
        if (source_fingerprint(ROOT) != source or digest(binary) != report['binary_sha256']
                or check_inputs() != report['inputs_sha256'] or digest(RUNTIME_INPUT) != RUNTIME_SHA256
                or digest(args.build_receipt) != report['build_receipt_sha256']):
            raise ValueError('FAIL_POST_RUN_IDENTITY')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', error=str(error))
        if report['results'] and report['results'][-1]['status'] == 'RUNNING':
            report['results'][-1]['status'] = 'FAIL'
    attempted = {key for row in report['results'] for key in row['selected']}
    report['not_run'] = [key for key in SELECTORS if key not in attempted]
    report['completed_selected'] = [key for row in report['results'] if row['status'] == 'PASS' for key in row['selected']]
    report['unconfirmed_selected'] = [key for key in selected if key not in report['completed_selected'] and key not in report['not_run']]
    (args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report.get(k) for k in ('status', 'error', 'selected', 'source_sha256')}))
    return int(report['status'] != 'PASS')


if __name__ == '__main__':
    sys.exit(main())
