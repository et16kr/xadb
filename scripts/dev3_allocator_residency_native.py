#!/usr/bin/env python3
"""One fresh Linux allocator reserve/flush/reload/cancel/flush/reload history.

Frozen p1/n1 bytes are reusable inputs, not repeated old producer evidence.
Expected bytes and reads use literals and standard Python; no product codec or
residency semantic helper participates in the external acceptance decision.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_allocation_stack import wait_process_group
from check_dev3_allocator_flush_runtime import verify_build
from dev3_allocator_flush_native import (
    HOME, WAL, FIXTURES, DIRTY, INSTALL, FD_LIMIT, DB_LIMIT, FIXED_OWNED,
    check_events, check_fd, check_inventory, check_resources, expected_after,
    initial_expected, inventory, parse_manifest, require, sample_process, sha,
)

SELECTOR = ('storage::wal_io::bootstrap::doublewrite_install::mutable_storage::'
            'allocator_residency::native_tests::worker_allocator_residency_native')
CASE_NAMES = ('reserve-reload-cancel-reload',)
CASE_SECONDS = 30
TOTAL_LIMIT = DB_LIMIT
GENESIS = Path('crates/engine/tests/fixtures/dev3-storage-session/literal')
MANIFEST_HASHES = {
    str(INSTALL / 'manifest.json'): 'b794e062669d4e7442d1398a484dadfaafd193ab5ff9de70788a6a8177139811',
    str(FIXTURES / 'manifest.json'): '9e68f75570434055172dbb33368278a4cd9fec553229e4cac7939d8683731ed1',
    str(DIRTY / 'manifest.json'): '17a3b3565f93809795b36eec3eb438442d215709f49bdd16ee4209d4e769db12',
}
CHECKER_NAMES = ('dev3_allocator_residency_native.py',
                 'test_dev3_allocator_residency_acceptance.py',
                 'dev3_allocator_flush_native.py', 'check_dev3_allocator_flush_runtime.py',
                 'check_dev3_allocation_stack.py', 'check_dev0.py', 'comm_campaign.py')


def select_cases(requested=None):
    selected = list(CASE_NAMES) if requested is None else requested
    require(selected and len(selected) == len(set(selected))
            and not set(selected) - set(CASE_NAMES), 'FAIL_NATIVE_CENSUS')
    return [name for name in CASE_NAMES if name in selected]


def frozen_inputs(root):
    identities = dict(MANIFEST_HASHES)
    for name, checksum in identities.items():
        require(digest(root / name) == checksum, 'FAIL_LITERAL_MANIFEST_IDENTITY')
    for directory, suffixes in (
        (FIXTURES, ('home.after', 'slot.bin', 'pending.bank', 'retirement.bank', 'retirement.selector')),
        (DIRTY, ('frame', 'descriptor')),
    ):
        rows = {row['path']: row for row in json.loads((root / directory / 'manifest.json').read_text())['literals']}
        for part in ('reserve', 'cancel'):
            for suffix in suffixes:
                name = f'p1-n1-{part}' + ('-' if directory == FIXTURES else '.') + suffix
                row = rows[name]
                path = root / directory / name
                require(path.stat().st_size == row['bytes'] and digest(path) == row['sha256'],
                        'FAIL_LITERAL_IDENTITY')
                identities[str(directory / name)] = row['sha256']
    # The installed FILEMAP selects this independently recorded initial layout.
    # These sizes are read from raw frozen inputs, never from production metadata.
    for suffix, length, checksum in (
        ('root', 320, 'c8ddca13874f35180d5b80ddfe856837506e764e1326fdc2c11b3427effe78d8'),
        ('chunk', 256, '37746fe4d861925812f7f18134fe53a85e71371700d00fb4306051e2f18924b4'),
    ):
        name = GENESIS / f'profile1-{suffix}.bin'
        require((root / name).stat().st_size == length and digest(root / name) == checksum,
                'FAIL_READ_INPUT_IDENTITY')
        identities[str(name)] = checksum
    return identities


def expected_reload_reads(root):
    root_length = (root / GENESIS / 'profile1-root.bin').stat().st_size
    chunk_length = (root / GENESIS / 'profile1-chunk.bin').stat().st_size
    return [['read', name, str(offset), str(length), str(length)] for name, offset, length in (
        ('d3-profile', 0, 4096), ('FILEMAP', 0, 4096), ('d3-filemap-0', 0, root_length),
        ('d3-filemap-0', 16384, chunk_length), ('d3-filemap-0', 16384, chunk_length),
        (HOME, 0, 16384),
    )]


def check_reload_reads(rows, root):
    require(rows == expected_reload_reads(root), 'FAIL_RELOAD_READ_SEQUENCE')


def check_reload(values, batch):
    # The second change starts from the first reload's clean counter1.
    reserved = ['4294967297', '1', '2'] if batch == 1 else ['none'] * 3
    expected = [str(batch), str(batch + 1), str(batch), str(batch), str(batch), '1',
                str(batch == 1).lower(), str(batch == 1 and 1 or 0), '1', str(batch),
                str(4096 + 21200 * batch), str(3 * batch + 1), *reserved]
    require(values == expected, 'FAIL_RELOAD_STATE')


def check_stdout(text, db_bytes):
    summaries = re.findall(r'^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; \d+ filtered out; finished in [\d.]+s$', text, re.M)
    require(summaries == [('1', '0', '0', '0')], 'FAIL_TEST_CENSUS')
    require(re.findall(r'^test ([\w:]+) \.\.\.', text, re.M) == [SELECTOR], 'FAIL_TEST_SELECTOR')
    prefix = r'^(?:test [\w:]+ \.\.\. )?'
    matches = re.findall(prefix + r'ALLOCATOR_RESIDENCY_NATIVE_COMPLETE profile=(\d+) batches=(\d+) reloads=(\d+) effects=(\d+) bytes=(\d+) peak_files=(\d+)$', text, re.M)
    protocol = [re.sub(r'^test [\w:]+ \.\.\. ', '', line) for line in text.splitlines()]
    require(len(matches) == 1 and sum(line.startswith('ALLOCATOR_RESIDENCY_NATIVE') for line in protocol) == 1,
            'FAIL_NATIVE_MARKER')
    profile, batches, reloads, effects, size, handles = map(int, matches[0])
    require((profile, batches, reloads, effects, size) == (1, 2, 2, 62, db_bytes), 'FAIL_NATIVE_MARKER')
    require(1 <= handles <= 3, 'FAIL_TRACKED_FILE_CENSUS')
    return {'profile': profile, 'batches': batches, 'reloads': reloads,
            'logical_effects': effects, 'tracked_file_peak': handles}


def check_payloads(page, descriptor, root, part):
    require(part in ('reserve', 'cancel'), 'FAIL_PART')
    expected_page = (root / FIXTURES / f'p1-n1-{part}-home.after').read_bytes()
    expected_descriptor = (root / DIRTY / f'p1-n1-{part}.descriptor').read_bytes()
    require(page == expected_page, 'FAIL_RAM_PAGE')
    require(descriptor == expected_descriptor, 'FAIL_ACTIVE_DESCRIPTOR')


def check_reload_inventory(old, new):
    require(old == new, 'FAIL_RELOAD_DISK_MUTATION')


def evaluate_case(case, root):
    expected_names = {'database', 'header.bin', 'before.tsv', 'after.tsv', 'fd.tsv', 'fd-events.tsv',
                      'stdout.log', 'stderr.log', 'process.json'}
    for part in ('reserve', 'cancel'):
        expected_names.update(f'{part}.{suffix}' for suffix in (
            'before.tsv', 'after.tsv', 'before.page', 'after.page', 'before.descriptor',
            'after.descriptor', 'reload.tsv', 'flush.events.tsv', 'flush.reads.tsv',
            'reload.events.tsv', 'reload.reads.tsv'))
    require({path.name for path in case.iterdir()} == expected_names, 'FAIL_ARTIFACT_CENSUS')
    initial = initial_expected(root, 1)
    header = (case / 'header.bin').read_bytes()
    require((len(header), sha(header)) == initial[WAL], 'FAIL_INITIAL_WAL')
    before = parse_manifest(case / 'before.tsv')
    actual = inventory(case / 'database')
    require(parse_manifest(case / 'after.tsv') == actual, 'FAIL_EXTERNAL_IMAGE')
    for batch, part in ((1, 'reserve'), (2, 'cancel')):
        old = parse_manifest(case / f'{part}.before.tsv')
        new = parse_manifest(case / f'{part}.after.tsv')
        check_reload_inventory(old, new)
        check_inventory(before, new, expected_after(root, 1, batch, header), initial)
        if batch == 2:
            require(new == actual, 'FAIL_FINAL_IMAGE')
        rows = lambda suffix: [line.split('\t') for line in (case / f'{part}.{suffix}.tsv').read_text().splitlines()]
        check_events(rows('flush.events'), batch)
        check_reload_reads(rows('reload.reads'), root)
        require(rows('reload.events') == [], 'FAIL_RELOAD_MUTATION')
        # Whole bytes distinguish a correct registration witness from stale RAM.
        for boundary in ('before', 'after'):
            check_payloads((case / f'{part}.{boundary}.page').read_bytes(),
                           (case / f'{part}.{boundary}.descriptor').read_bytes(), root, part)
        check_reload((case / f'{part}.reload.tsv').read_text().rstrip('\n').split('\t'), batch)
    fd = check_fd((case / 'fd.tsv').read_text(),
                  [line.split('\t') for line in (case / 'fd-events.tsv').read_text().splitlines()])
    database_bytes = sum(row[0] for row in actual.values())
    return {'status': 'PASS', 'database_bytes': database_bytes, 'files': actual,
            'worker': check_stdout((case / 'stdout.log').read_text(), database_bytes),
            'resources': check_resources(database_bytes, actual[WAL][0]), 'fd': fd,
            'evidence': {p.name: digest(p) for p in sorted(case.iterdir()) if p.is_file()}}


def run(root, binary, output, source, build_receipt, requested=None):
    root, binary, output = root.resolve(), binary.resolve(), output.resolve()
    selected = select_cases(requested)
    require(sys.platform.startswith('linux'), 'FAIL_PLATFORM')
    require(source_fingerprint(root) == source, 'FAIL_SOURCE_IDENTITY')
    identities = frozen_inputs(root)
    binary_hash = digest(binary)
    build_receipt = build_receipt.resolve()
    build_hash = digest(build_receipt)
    verify_build(json.loads(build_receipt.read_text()), root, binary, source, binary_hash)
    scripts = Path(__file__).resolve().parent
    checkers = {name: digest(scripts / name) for name in CHECKER_NAMES}
    require(all(digest(root / 'scripts' / name) == checksum for name, checksum in checkers.items()),
            'FAIL_CHECKER_IDENTITY')
    output.mkdir(parents=True, exist_ok=False)
    retained = output / f'allocator-residency-worker-{binary_hash}'
    shutil.copyfile(binary, retained)
    retained.chmod(0o500)
    with retained.open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'FAIL_BINARY_FORMAT')
    report = {'status': 'RUNNING', 'scope': 'one fresh Linux allocator residency history; no recovery/power-loss claim',
              'source_root': str(root), 'source_sha256': source, 'binary': str(retained),
              'binary_sha256': binary_hash, 'build_receipt': str(build_receipt),
              'build_receipt_sha256': build_hash, 'checker_sha256': checkers,
              'fixture_identities': identities, 'selected_cases': selected, 'native_cases': [],
              'case_timeout_seconds': CASE_SECONDS, 'database_limit_bytes': DB_LIMIT,
              'total_limit_bytes': TOTAL_LIMIT, 'fixed_owned_budget_bytes': FIXED_OWNED,
              'heap_payload_bytes': 5373952, 'database_fd_limit': FD_LIMIT,
              'tracked_file_handles_limit': 3,
              'not_selected': [name for name in CASE_NAMES if name not in selected]}
    try:
        for name in selected:
            case = output / name
            case.mkdir()
            command = [str(retained), '--exact', SELECTOR, '--ignored', '--nocapture', '--test-threads=1']
            environment = os.environ.copy()
            for key in list(environment):
                if key.startswith(('XADB_', 'XADBTEST_', 'RUST_TEST_')):
                    environment.pop(key)
            environment['XADB_ALLOCATOR_RESIDENCY_NATIVE_DIR'] = str(case)
            process = {'command': command, 'binary_sha256': binary_hash, 'timeout_seconds': CASE_SECONDS,
                       'timed_out': False, 'exit_code': None, 'fd_samples': 0,
                       'sampled_database_fd_peak': 0, 'sampled_database_bytes_peak': 0,
                       'sample_errors': [], 'proc_terminal_unavailable': []}
            row = {'name': name, 'status': 'RUNNING', 'process': process}
            report['native_cases'].append(row)
            started = time.monotonic()
            stop = threading.Event()
            sampler = None
            try:
                with (case / 'stdout.log').open('wb') as stdout, (case / 'stderr.log').open('wb') as stderr:
                    with subprocess.Popen(command, cwd=root, env=environment, stdout=stdout,
                                          stderr=stderr, start_new_session=True) as child:
                        process['pid'] = child.pid
                        sampler = threading.Thread(target=sample_process,
                                                   args=(child, case / 'database', stop, process), daemon=True)
                        sampler.start()
                        wait_process_group(child, process, CASE_SECONDS)
            finally:
                stop.set()
                if sampler:
                    sampler.join(timeout=1)
                    if sampler.is_alive():
                        process['sample_errors'].append('FD sampler did not terminate')
                process['elapsed_seconds'] = time.monotonic() - started
                (case / 'process.json').write_text(json.dumps(process, indent=2) + '\n')
            require(not process['timed_out'] and process['elapsed_seconds'] <= CASE_SECONDS, 'FAIL_TIMEOUT')
            require(process['exit_code'] == 0, 'FAIL_WORKER_EXIT')
            require(not process['sample_errors'] and process['fd_samples'] > 0, 'FAIL_PROC_OBSERVATION')
            require(process['sampled_database_fd_peak'] <= FD_LIMIT, 'FAIL_SAMPLED_FD_BOUND')
            require(process['sampled_database_bytes_peak'] <= DB_LIMIT, 'FAIL_SAMPLED_DATABASE_BOUND')
            row.update(evaluate_case(case, root))
        require(source_fingerprint(root) == source and digest(binary) == digest(retained) == binary_hash
                and digest(build_receipt) == build_hash, 'FAIL_SOURCE_BINARY_BUILD_DRIFT')
        require(all(digest(root / name) == checksum for name, checksum in identities.items()), 'FAIL_FIXTURE_DRIFT')
        require(all(digest(scripts / name) == checksum for name, checksum in checkers.items()), 'FAIL_CHECKER_DRIFT')
        report['status'] = 'PASS'
    except BaseException as error:
        report.update(status='FAIL', failure=str(error))
        for row in report['native_cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
        raise
    finally:
        attempted = {row['name'] for row in report['native_cases']}
        report['not_run'] = [name for name in selected if name not in attempted]
        report['databases_completed'] = sum(row['status'] == 'PASS' for row in report['native_cases'])
        report['runtime_elapsed_seconds'] = sum(row['process'].get('elapsed_seconds', 0) for row in report['native_cases'])
        (output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    for name in ('binary', 'output', 'build-receipt'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--source-sha256', required=True)
    parser.add_argument('--case', action='append', choices=CASE_NAMES)
    args = parser.parse_args()
    result = run(args.root, args.binary, args.output, args.source_sha256, args.build_receipt, args.case)
    print(json.dumps({'status': result['status'], 'databases_completed': result['databases_completed']}))


if __name__ == '__main__':
    main()
