#!/usr/bin/env python3
"""Compile only StatusRole's three frozen ownership cases, without runtime calls."""
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
from check_dev3_buffer_borrows import run_compiler
from check_dev3_allocation_borrows import preserve_artifacts
from comm_campaign import capture_source, write_json

ROOT = Path(__file__).resolve().parent.parent
INPUT_RELATIVE = 'crates/engine/tests/fixtures/dev3-status-bootstrap/compiler'
INPUTS = ROOT / INPUT_RELATIVE
INPUTS_SHA256 = 'd330eb54cc4dab87f274ef5bfcf193b867181ebe7d50eeeb9e8a46c399f8362f'
PROBE = 'crates/engine/src/storage/status_role_borrow_probe.rs'
ANCHOR = 'crates/engine/src/storage/mod.rs'
EXPECTATIONS = {
    'baseline': (None, None, 'none', None),
    'positive': (None, None, 'none', 'NC00'),
    'private_constructor': ('E0451', 5, 'all_on_line', 'NC01'),
    'owner_drop': ('E0505', 7, 'one', 'NC02'),
}
CASE_NAMES = tuple(EXPECTATIONS)
COMPILER_BUDGET_SECONDS = 60


def inputs(directory=INPUTS, expected_sha256=INPUTS_SHA256):
    """Validate the entire input identity before selecting a changed-code subset."""
    if digest(directory / 'manifest.json') != expected_sha256:
        raise ValueError('frozen compiler manifest changed')
    manifest = json.loads((directory / 'manifest.json').read_text())
    if (manifest['schema_version'] != 1 or manifest['unit'] != 'D3-STATUS-BOOTSTRAP-001'
            or manifest['probe_relative_path'] != PROBE
            or manifest['anchor_relative_path'] != ANCHOR
            or manifest['requires_actual_unmodified_baseline'] is not True
            or manifest['unrelated_error_diagnostics_are_failures'] is not True
            or manifest['compiler_budget_seconds'] != COMPILER_BUDGET_SECONDS
            or manifest['semantic_case_count'] != 3
            or manifest['infrastructure_control_count'] != 1
            or manifest['negative_returncode'] != 101):
        raise ValueError('frozen compiler contract changed')
    cases = manifest['cases']
    if [row['name'] for row in cases] != list(CASE_NAMES):
        raise ValueError('missing, duplicate or unexpected compiler case')
    expected_files = {'manifest.json', *(name + '.rs' for name in CASE_NAMES[1:])}
    if {path.name for path in directory.iterdir()} != expected_files:
        raise ValueError('missing or unregistered compiler fixture')
    for row in cases:
        expected = EXPECTATIONS[row['name']]
        actual = tuple(row[key] for key in ('expected_code', 'primary_line',
                                           'primary_spans', 'semantic_id'))
        if actual != expected or not row['reason']:
            raise ValueError('wrong frozen diagnostic identity')
        if row['name'] == 'baseline':
            continue
        path = directory / (row['name'] + '.rs')
        if path.stat().st_size != row['bytes'] or digest(path) != row['sha256']:
            raise ValueError('frozen compiler source changed')
        markers = [i for i, line in enumerate(path.read_text().splitlines(), 1)
                   if '// expected diagnostic' in line]
        if markers != ([] if expected[1] is None else [expected[1]]):
            raise ValueError('diagnostic marker differs from frozen location')
    return cases


def select_cases(cases, requested):
    """Only the unmodified control is implicit; passed semantic cases stay omitted."""
    names = list(CASE_NAMES) if requested is None else requested
    if not names or len(set(names)) != len(names) or set(names) - set(CASE_NAMES):
        raise ValueError('empty, duplicate or unknown compiler selection')
    selected = set(names) | {'baseline'}
    return [row for row in cases if row['name'] in selected]


def verify_diagnostics(output, returncode, frozen):
    """Require completed Cargo success or one exact coded primary expression."""
    messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
    finishes = [row for row in messages if row.get('reason') == 'build-finished']
    errors = [row['message'] for row in messages if row.get('reason') == 'compiler-message'
              and row.get('message', {}).get('level') == 'error']
    if len(finishes) != 1:
        raise ValueError('missing/duplicate compiler completion')
    code = frozen['expected_code']
    if code is None:
        if returncode != 0 or finishes[0].get('success') is not True or errors:
            raise ValueError('positive/baseline did not compile')
        return []
    if returncode != 101 or finishes[0].get('success') is not False or len(errors) != 1:
        raise ValueError('negative needs exactly one completed Cargo rejection')
    error = errors[0]
    if (error.get('code') or {}).get('code') != code:
        raise ValueError('wrong compiler error code')
    primary = [span for span in error.get('spans', []) if span.get('is_primary') is True]
    if (not primary or (frozen['primary_spans'] == 'one' and len(primary) != 1)
            or any(span.get('file_name') != PROBE or
                   (span.get('line_start'), span.get('line_end')) !=
                   (frozen['primary_line'], frozen['primary_line']) for span in primary)):
        raise ValueError('error is not at the exact frozen probe expression')
    return [{'code': code, 'primary_spans': primary}]


def remaining_budget(consumed):
    remaining = COMPILER_BUDGET_SECONDS - consumed
    if remaining <= 0:
        raise RuntimeError('StatusRole compiler selection exhausted its 60 second budget')
    return remaining


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-root', type=Path, default=ROOT,
                        help='source to preserve; may be an already frozen export')
    parser.add_argument('--artifact-parent', type=Path, default=ROOT / 'artifacts/dev3')
    parser.add_argument('--report', type=Path)
    parser.add_argument('--case', action='append', choices=CASE_NAMES,
                        help='repeat for narrow selection; only unmodified control is implicit')
    args = parser.parse_args()
    if args.report is not None and (args.report.exists() or args.report.is_symlink()):
        parser.error('report already exists')
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='status-role-borrows-',
                                    dir=args.artifact_parent)).resolve()
    report = {'scope': 'D3-STATUS-BOOTSTRAP-001 compiler semantic3 plus control1',
              'status': 'RUNNING', 'artifact_directory': str(session), 'cases': [],
              'inventory': list(CASE_NAMES), 'requested': args.case,
              'semantic_inventory': ['NC00', 'NC01', 'NC02'],
              'infrastructure_inventory': ['baseline'],
              'compiler_budget_seconds': COMPILER_BUDGET_SECONDS,
              'compiler_elapsed_seconds': 0.0,
              'source_policy': 'all cases compile copies of one preserved source export',
              'binary_kind': 'cargo-check metadata; no database code or test execution',
              'positive_scope': 'type-check only; healthy create/get semantics are runtime-owned'}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    target = session / 'target'
    environment['CARGO_TARGET_DIR'] = str(target)
    checker_paths = [Path(__file__).resolve(), ROOT / 'scripts/check_dev3_allocation_borrows.py',
                     ROOT / 'scripts/check_dev3_buffer_borrows.py', ROOT / 'scripts/check_dev0.py',
                     ROOT / 'scripts/comm_campaign.py']
    code = 0
    started = time.monotonic()
    try:
        input_path = args.source_root.resolve() / INPUT_RELATIVE
        cases = select_cases(inputs(input_path), args.case)
        report['selected'] = [row['name'] for row in cases]
        report['checker_sha256'] = {str(path): digest(path) for path in checker_paths}
        report['fixture_sha256'] = {str(path): digest(path) for path in sorted(input_path.iterdir())}
        baseline = session / 'baseline'
        report['source_sha256'] = capture_source(args.source_root.resolve(), baseline)
        # Read programs only from the preserved source used by the unmodified control.
        preserved_inputs = baseline / INPUT_RELATIVE
        if inputs(preserved_inputs) != inputs(input_path):
            raise ValueError('preserved compiler inputs differ')
        original = (baseline / ANCHOR).read_text()
        if (baseline / PROBE).exists():
            raise ValueError('probe path already exists in unmodified baseline')
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
                probe = (preserved_inputs / (name + '.rs')).read_text()
                (source / PROBE).write_text(probe)
                modified = original + '\nmod status_role_borrow_probe;\n'
                (source / ANCHOR).write_text(modified)
                patch = ''.join(difflib.unified_diff(
                    original.splitlines(True), modified.splitlines(True),
                    fromfile=ANCHOR, tofile=ANCHOR))
                patch += ''.join(difflib.unified_diff(
                    [], probe.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                patch_path = evidence / 'probe.patch'
                patch_path.write_text(patch)
            fingerprint = source_fingerprint(source)
            row = {'name': name, 'semantic_id': frozen['semantic_id'], 'source': str(source),
                   'source_sha256': fingerprint, 'expected_code': frozen['expected_code'],
                   'expected_line': frozen['primary_line'], 'expected_path': PROBE,
                   'primary_spans': frozen['primary_spans'], 'reason': frozen['reason'],
                   'status': 'RUNNING'}
            if name != 'baseline':
                row['patch'] = str(patch_path)
                row['patch_sha256'] = digest(patch_path)
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
            # Preserve every emitted metadata artifact, including failed probes,
            # before the next invocation can reuse the isolated target directory.
            row['binary_artifacts'] = preserve_artifacts(output, evidence, target, require_engine=False)
            remaining_budget(report['compiler_elapsed_seconds'])
            row['diagnostics'] = verify_diagnostics(output, returncode, frozen)
            if frozen['expected_code'] is None and not any(
                    item['target'] == 'engine' for item in row['binary_artifacts']):
                raise ValueError('successful compile has no xadb-engine metadata artifact')
            if source_fingerprint(source) != fingerprint:
                raise ValueError('probe source changed during compilation')
            row['status'] = 'PASS'
        if source_fingerprint(baseline) != report['source_sha256']:
            raise ValueError('preserved baseline changed during verification')
        for identity in ('checker_sha256', 'fixture_sha256', 'tool_binary_sha256'):
            if any(digest(Path(path)) != expected for path, expected in report[identity].items()):
                raise ValueError(identity + ' changed during verification')
        report['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        report['status'], report['error'], code = 'FAIL', str(error), 1
        for row in report['cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
    report['total_elapsed_seconds'] = time.monotonic() - started
    attempted = {row['name'] for row in report['cases']}
    report['not_run'] = [name for name in CASE_NAMES if name not in attempted]
    report['semantic_pass'] = [row['semantic_id'] for row in report['cases']
                               if row['semantic_id'] is not None and row['status'] == 'PASS']
    report['infrastructure_pass'] = [row['name'] for row in report['cases']
                                     if row['semantic_id'] is None and row['status'] == 'PASS']
    write_json(session / 'result.json', report)
    if args.report is not None:
        write_json(args.report, report)
    print(json.dumps({'status': report['status'], 'attempts': len(report['cases']),
                      'semantic_pass': report['semantic_pass'],
                      'infrastructure_pass': report['infrastructure_pass'],
                      'compiler_elapsed_seconds': report['compiler_elapsed_seconds'],
                      'artifact_directory': str(session), 'error': report.get('error')}))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
