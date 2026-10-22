#!/usr/bin/env python3
"""Compile only frozen dirty-frame ownership probes; never execute database code."""
import argparse
import difflib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_buffer_borrows import check_diagnostics, run_compiler
from check_dev3_allocation_borrows import preserve_artifacts
from comm_campaign import capture_source, write_json

ROOT = Path(__file__).resolve().parent.parent
INPUTS = ROOT / 'crates/engine/tests/fixtures/dev3-dirty-allocation/compiler'
INPUTS_SHA256 = 'cf3633e970aed4994635d5fa40959387f466422649ad44460bb81fcd57086cd8'
PROBE = 'crates/engine/src/storage/dirty_borrow_probe.rs'
ANCHOR = 'crates/engine/src/storage/mod.rs'
EXPECTATIONS = {
    'baseline': None, 'release': None,
    'old_reader_consume': 'E0505', 'storage_drop': 'E0505', 'core_drop': 'E0505',
    'frames_reborrow': 'E0499', 'payload_reborrow': 'E0499', 'workspace_reborrow': 'E0499',
    'bound_move': 'E0505', 'bound_resplit': 'E0499', 'owner_reborrow': 'E0499',
    'pins_reuse': 'E0382', 'read_mutable': 'E0308',
    'read_sync': 'E0277', 'write_sync': 'E0277',
    'raw_lsn': 'E0308', 'permit_private': 'E0451', 'permit_clone': 'E0599',
}
CASE_NAMES = tuple(EXPECTATIONS)
COMPILER_BUDGET_SECONDS = 60


def inputs(directory=INPUTS, expected_sha256=INPUTS_SHA256):
    """Check the adopted programs and complete inventory before any compiler."""
    if digest(directory / 'manifest.json') != expected_sha256:
        raise ValueError('frozen compiler manifest changed')
    manifest = json.loads((directory / 'manifest.json').read_text())
    cases = manifest['cases']
    if [row['name'] for row in cases] != list(CASE_NAMES):
        raise ValueError('missing, duplicate or unexpected compiler case')
    if (manifest['probe_relative_path'] != PROBE
            or manifest['anchor_relative_path'] != ANCHOR
            or manifest['requires_actual_unmodified_baseline'] is not True
            or manifest['unrelated_diagnostics_are_failures'] is not True
            or manifest['compiler_budget_seconds'] != COMPILER_BUDGET_SECONDS):
        raise ValueError('frozen compiler contract changed')
    expected_files = {'manifest.json', *(name + '.rs' for name in CASE_NAMES[1:])}
    if {path.name for path in directory.iterdir()} != expected_files:
        raise ValueError('missing or unregistered compiler fixture')
    for row in cases:
        name = row['name']
        expected_code = EXPECTATIONS[name]
        expected_line = 10 if expected_code else None
        if (row['expected_code'], row['primary_line']) != (expected_code, expected_line):
            raise ValueError('wrong frozen diagnostic identity')
        expected_spans = 'none' if expected_code is None else ('all_on_line' if name == 'permit_private' else 'one')
        if row['primary_spans'] != expected_spans:
            raise ValueError('wrong frozen primary-span policy')
        if not row['reason']:
            raise ValueError('missing ownership reason')
        if name == 'baseline':
            continue
        path = directory / (name + '.rs')
        if path.stat().st_size != row['bytes'] or digest(path) != row['sha256']:
            raise ValueError('frozen compiler source changed')
        markers = [i for i, line in enumerate(path.read_text().splitlines(), 1)
                   if '// expected diagnostic' in line]
        if markers != ([] if expected_line is None else [expected_line]):
            raise ValueError('diagnostic marker differs from frozen location')
    return cases


def select_cases(cases, requested):
    """A selected probe needs one unmodified baseline; omitted probes stay NOT_RUN."""
    names = list(CASE_NAMES) if requested is None else requested
    if not names or len(set(names)) != len(names) or set(names) - set(CASE_NAMES):
        raise ValueError('empty, duplicate or unknown compiler selection')
    selected = set(names) | {'baseline'}
    # The actual assembly/release success precedes selected rejection programs.
    return sorted((row for row in cases if row['name'] in selected),
                  key=lambda row: row['expected_code'] is not None)


def verify_diagnostics(output, returncode, frozen):
    if frozen['primary_spans'] != 'all_on_line':
        return check_diagnostics(output, returncode, frozen['expected_code'],
                                 frozen['primary_line'], probe_path=PROBE)
    messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
    finishes = [m for m in messages if m.get('reason') == 'build-finished']
    errors = [m['message'] for m in messages if m.get('reason') == 'compiler-message'
              and m.get('message', {}).get('level') == 'error']
    if returncode == 0 or len(finishes) != 1 or finishes[0].get('success') is not False or len(errors) != 1:
        raise ValueError('private construction needs exactly one completed compiler rejection')
    error = errors[0]
    if (error.get('code') or {}).get('code') != frozen['expected_code']:
        raise ValueError('wrong private-construction compiler code')
    primary = [span for span in error.get('spans', []) if span.get('is_primary') is True]
    if not primary or any(span.get('file_name') != PROBE or
                          (span.get('line_start'), span.get('line_end')) !=
                          (frozen['primary_line'], frozen['primary_line']) for span in primary):
        raise ValueError('private field diagnostic escaped its exact frozen expression')
    return [{'code': frozen['expected_code'], 'primary_spans': primary}]


def remaining_budget(consumed):
    remaining = COMPILER_BUDGET_SECONDS - consumed
    if remaining <= 0:
        raise RuntimeError('dirty compiler suite exhausted its 60 second budget')
    return remaining



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact-parent', type=Path, default=ROOT / 'artifacts/dev3')
    parser.add_argument('--report', type=Path)
    parser.add_argument('--case', action='append', choices=CASE_NAMES,
                        help='repeat for a narrow selection; baseline is always included')
    args = parser.parse_args()
    if args.report is not None and (args.report.exists() or args.report.is_symlink()):
        parser.error('report already exists')
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='dirty-borrows-',
                                    dir=args.artifact_parent)).resolve()
    report = {'scope': 'D3-DIRTY-ALLOCATION-001 frozen compiler probes',
              'status': 'RUNNING', 'artifact_directory': str(session), 'cases': [],
              'inventory': list(CASE_NAMES), 'requested': args.case,
              'compiler_budget_seconds': COMPILER_BUDGET_SECONDS,
              'compiler_elapsed_seconds': 0.0,
              'source_policy': 'all cases compile copies of one preserved source export',
              'binary_kind': 'cargo-check compiler metadata; no runtime executable'}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    target = session / 'target'
    environment['CARGO_TARGET_DIR'] = str(target)
    checkers = [Path(__file__).resolve(), ROOT / 'scripts/check_dev3_allocation_borrows.py',
                ROOT / 'scripts/check_dev3_buffer_borrows.py',
                ROOT / 'scripts/check_dev0.py', ROOT / 'scripts/comm_campaign.py']
    code = 0
    started = time.monotonic()
    try:
        cases = select_cases(inputs(), args.case)
        report['selected'] = [row['name'] for row in cases]
        report['not_run'] = [name for name in CASE_NAMES if name not in report['selected']]
        report['checker_sha256'] = {str(path): digest(path) for path in checkers}
        report['fixture_sha256'] = {str(path): digest(path) for path in sorted(INPUTS.iterdir())}
        baseline = session / 'baseline'
        report['source_sha256'] = capture_source(ROOT, baseline)
        original_lib = (baseline / ANCHOR).read_text()
        command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--locked',
                   '--offline', '--message-format=json']
        report['toolchain'] = subprocess.check_output(
            ['rustc', '-Vv'], cwd=baseline, env=environment, text=True, timeout=10)
        sysroot = Path(subprocess.check_output(
            ['rustc', '--print', 'sysroot'], cwd=baseline, env=environment,
            text=True, timeout=10).strip())
        report['tool_binary_sha256'] = {str(sysroot / 'bin' / name):
                                       digest(sysroot / 'bin' / name)
                                       for name in ('rustc', 'cargo')}
        for frozen in cases:
            name = frozen['name']
            source = baseline if name == 'baseline' else session / name / 'source'
            evidence = session / 'baseline-logs' if name == 'baseline' else source.parent
            evidence.mkdir(parents=True, exist_ok=True)
            if name != 'baseline':
                if capture_source(baseline, source) != report['source_sha256']:
                    raise ValueError('probe base differs from baseline')
                probe = (INPUTS / (name + '.rs')).read_text()
                if (source / PROBE).exists():
                    raise ValueError('probe path already exists')
                (source / PROBE).write_text(probe)
                modified = original_lib + '\nmod dirty_borrow_probe;\n'
                (source / ANCHOR).write_text(modified)
                patch = ''.join(difflib.unified_diff(
                    original_lib.splitlines(True), modified.splitlines(True),
                    fromfile=ANCHOR, tofile=ANCHOR))
                patch += ''.join(difflib.unified_diff(
                    [], probe.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                (evidence / 'probe.patch').write_text(patch)
            fingerprint = source_fingerprint(source)
            row = {'name': name, 'source': str(source), 'source_sha256': fingerprint,
                   'expected_code': frozen['expected_code'],
                   'expected_line': frozen['primary_line'], 'expected_path': PROBE,
                   'reason': frozen['reason'], 'status': 'RUNNING'}
            report['cases'].append(row)
            timeout = remaining_budget(report['compiler_elapsed_seconds'])
            row['timeout_seconds'] = timeout
            compile_started = time.monotonic()
            try:
                output, returncode = run_compiler(command, source, environment,
                                                 evidence, row, timeout=timeout)
            finally:
                row['elapsed_seconds'] = time.monotonic() - compile_started
                report['compiler_elapsed_seconds'] += row['elapsed_seconds']
            remaining_budget(report['compiler_elapsed_seconds'])
            row['diagnostics'] = verify_diagnostics(output, returncode, frozen)
            row['binary_artifacts'] = preserve_artifacts(
                output, evidence, target, require_engine=frozen['expected_code'] is None)
            if source_fingerprint(source) != fingerprint:
                raise ValueError('probe source changed during compilation')
            row['status'] = 'PASS'
        # Independent work may continue in ROOT. Only this preserved export is
        # execution evidence; an unrelated workspace edit does not invalidate it.
        if source_fingerprint(baseline) != report['source_sha256']:
            raise ValueError('preserved baseline changed during verification')
        for identity in ('checker_sha256', 'fixture_sha256', 'tool_binary_sha256'):
            if any(digest(Path(path)) != expected for path, expected in report[identity].items()):
                raise ValueError(identity + ' changed during verification')
        report['status'] = 'PASS'
    except (ValueError, KeyError, OSError, RuntimeError,
            subprocess.SubprocessError, KeyboardInterrupt) as error:
        report['status'], report['error'], code = 'FAIL', str(error), 1
        for row in report['cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
    report['total_elapsed_seconds'] = time.monotonic() - started
    attempted = {row['name'] for row in report['cases']}
    report['not_run'] = [name for name in CASE_NAMES if name not in attempted]
    write_json(session / 'result.json', report)
    if args.report is not None:
        write_json(args.report, report)
    print(json.dumps({'status': report['status'], 'cases': len(report['cases']),
                      'compiler_elapsed_seconds': report['compiler_elapsed_seconds'],
                      'artifact_directory': str(session), 'error': report.get('error')}))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
