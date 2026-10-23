#!/usr/bin/env python3
"""Compile exact resident Status ownership/admission probes; never execute test binaries."""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_buffer_borrows import check_diagnostics, run_compiler
from comm_campaign import capture_source, write_json

ROOT = Path(__file__).resolve().parent.parent
INPUTS = ROOT / 'crates/engine/tests/fixtures/dev3-resident-status/compiler'
PROBE = 'crates/engine/src/storage/resident_status_borrow_probe.rs'
ANCHOR = 'crates/engine/src/storage/mod.rs'
BUDGET_SECONDS = 180
EXPECTATIONS = {'baseline': None, 'release': None, 'activation_retry': None, 'failed_result_reborrow': 'E0499', 'guard_reborrow': 'E0499', 'guard_sync': 'E0277', 'consumed_binding': 'E0382', 'split_prepare': 'E0499', 'frames_alias': 'E0499'}
PRIMARY_LINES = {'failed_result_reborrow': 6, 'guard_reborrow': 6, 'guard_sync': 5, 'consumed_binding': 6, 'split_prepare': 6, 'frames_alias': 7}

def inputs(directory=INPUTS):
    manifest = json.loads((directory / 'manifest.json').read_text())
    rows = manifest['cases']
    if ([x['name'] for x in rows] != list(EXPECTATIONS)
            or manifest['schema'] != 'xadb-resident-status-compiler-v1'
            or manifest['status'] != 'FROZEN_INPUTS_NOT_EXECUTION_EVIDENCE'
            or manifest['compiler_budget_seconds'] != BUDGET_SECONDS
            or manifest['probe_relative_path'] != PROBE
            or manifest['anchor_relative_path'] != ANCHOR):
        raise ValueError('compiler inventory changed')
    if {p.name for p in directory.iterdir()} != {'manifest.json', *(n + '.rs' for n in EXPECTATIONS if n != 'baseline')}:
        raise ValueError('unregistered or missing program')
    for row in rows:
        name = row['name']
        path = PROBE if EXPECTATIONS[name] else None
        if (row['expected_code'] != EXPECTATIONS[name]
                or row['primary_line'] != PRIMARY_LINES.get(name)
                or row['primary_path'] != path or not row['reason']):
            raise ValueError('compiler diagnostic expectation changed')
        if name == 'baseline':
            continue
        program = directory / (name + '.rs')
        if program.stat().st_size != row['bytes'] or digest(program) != row['sha256']:
            raise ValueError('compiler program changed')
        markers = [n for n, text in enumerate(program.read_text().splitlines(), 1)
                   if '// expected diagnostic' in text]
        if markers != ([] if row['primary_line'] is None else [row['primary_line']]):
            raise ValueError('compiler marker changed')
    return rows


def select(rows, names):
    if names is None:
        return rows
    if not names or len(names) != len(set(names)) or set(names) - EXPECTATIONS.keys():
        raise ValueError('unknown, empty or duplicate selection')
    return [row for row in rows if row['name'] in set(names) | {'baseline'}]


def verify(output, returncode, row):
    return check_diagnostics(output, returncode, row['expected_code'], row['primary_line'],
                             probe_path=row['primary_path'] or PROBE)


def checker_bindings():
    """Identify the executed runner and the actual imported local module files."""
    paths = {Path(__file__).resolve()}
    paths.update(Path(sys.modules[function.__module__].__file__).resolve()
                 for function in (digest, source_fingerprint, check_diagnostics,
                                  run_compiler, capture_source, write_json))
    expected = {ROOT / ('scripts/' + name) for name in (
        'check_dev3_resident_status_borrows.py', 'check_dev0.py',
        'check_dev3_buffer_borrows.py', 'comm_campaign.py')}
    if paths != expected:
        raise ValueError('executed checker module paths differ from repository')
    return {str(path.relative_to(ROOT)): {'path': str(path), 'sha256': digest(path)}
            for path in sorted(paths)}


def verify_checker_bindings(bindings, baseline):
    observed = {}
    for relative, row in bindings.items():
        observed[relative] = {'executed_sha256': digest(Path(row['path'])),
                              'baseline_sha256': digest(baseline / relative)}
        if any(value != row['sha256'] for value in observed[relative].values()):
            raise ValueError('executed checker differs from captured baseline or changed')
    return observed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact-parent', type=Path, default=ROOT / 'artifacts/dev3')
    parser.add_argument('--case', action='append', choices=list(EXPECTATIONS))
    args = parser.parse_args()
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='resident-status-compiler-', dir=args.artifact_parent)).resolve()
    report = {'status': 'RUNNING', 'cases': [], 'artifact_directory': str(session),
              'budget_seconds': BUDGET_SECONDS, 'compiler_elapsed_seconds': 0,
              'scope': 'resident binding and wide-read lifetime compile only; no product runtime'}
    code = 0
    try:
        bindings = checker_bindings()
        report['checker_bindings_before'] = bindings
        baseline = session / 'baseline'
        report['source_sha256'] = capture_source(ROOT, baseline)
        report['checker_binding_after_capture'] = verify_checker_bindings(bindings, baseline)
        frozen_inputs = baseline / INPUTS.relative_to(ROOT)
        rows = select(inputs(frozen_inputs), args.case)
        report['selected'] = [row['name'] for row in rows]
        report['not_run'] = [name for name in EXPECTATIONS if name not in report['selected']]
        original = (baseline / ANCHOR).read_text()
        report['toolchain'] = subprocess.check_output(['rustc', '-Vv'], cwd=baseline, text=True, timeout=10)
        report['inputs'] = {str(p): digest(p) for p in sorted(frozen_inputs.iterdir())}
        environment = os.environ.copy()
        for key in list(environment):
            if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
                environment.pop(key)
        environment['CARGO_TARGET_DIR'] = str(session / 'target')
        for frozen in rows:
            name = frozen['name']
            evidence = session / (name + '-logs')
            evidence.mkdir()
            source = baseline if name == 'baseline' else session / name / 'source'
            if name != 'baseline':
                if capture_source(baseline, source) != report['source_sha256']:
                    raise ValueError('probe base differs from baseline')
                program_path = frozen_inputs / (name + '.rs')
                if digest(program_path) != frozen['sha256']:
                    raise ValueError('captured program changed before injection')
                program = program_path.read_text()
                if hashlib.sha256(program.encode()).hexdigest() != frozen['sha256']:
                    raise ValueError('injected program differs from captured input')
                if (source / PROBE).exists():
                    raise ValueError('probe path exists')
                (source / PROBE).write_text(program)
                modified = original + '\n#[cfg(test)]\nmod resident_status_borrow_probe;\n'
                (source / ANCHOR).write_text(modified)
                patch = ''.join(difflib.unified_diff(original.splitlines(True), modified.splitlines(True), fromfile=ANCHOR, tofile=ANCHOR))
                patch += ''.join(difflib.unified_diff([], program.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                (evidence / 'probe.patch').write_text(patch)
            codegen = name == 'baseline'
            command = (['cargo', 'test', '-p', 'xadb-engine', '--lib', '--no-run'] if codegen else
                       ['cargo', 'check', '-p', 'xadb-engine', '--tests']) + ['--locked', '--offline', '--message-format=json']
            row = {**frozen, 'source': str(source), 'source_sha256': source_fingerprint(source),
                   'status': 'RUNNING', 'codegen': codegen}
            report['cases'].append(row)
            remaining = BUDGET_SECONDS - report['compiler_elapsed_seconds']
            if remaining <= 0:
                raise RuntimeError('compiler total budget exhausted')
            started = time.monotonic()
            try:
                output, returncode = run_compiler(command, source, environment, evidence, row, timeout=remaining)
            finally:
                row['elapsed_seconds'] = time.monotonic() - started
                report['compiler_elapsed_seconds'] += row['elapsed_seconds']
            row['diagnostics'] = verify(output, returncode, frozen)
            if report['compiler_elapsed_seconds'] > BUDGET_SECONDS:
                raise RuntimeError('compiler total budget exceeded')
            row['binary_artifacts'] = []
            for text in output.splitlines():
                if not text.startswith('{'):
                    continue
                message = json.loads(text)
                if message.get('reason') == 'compiler-artifact' and message.get('executable'):
                    binary = Path(message['executable'])
                    dest = evidence / binary.name
                    shutil.copy2(binary, dest)
                    row['binary_artifacts'].append({'path': str(dest), 'sha256': digest(dest)})
            if source_fingerprint(source) != row['source_sha256']:
                raise ValueError('probe source changed during compilation')
            row['status'] = 'PASS'
        if source_fingerprint(baseline) != report['source_sha256']:
            raise ValueError('baseline changed during compilation')
        if any(digest(Path(p)) != sha for p, sha in report['inputs'].items()):
            raise ValueError('compiler inputs changed during execution')
        report['checker_bindings_after'] = verify_checker_bindings(bindings, baseline)
        report['status'] = 'PASS'
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report['status'], report['error'], code = 'FAIL', str(error), 1
        for row in report['cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
    write_json(session / 'result.json', report)
    print(json.dumps({'status': report['status'], 'artifact_directory': str(session), 'cases': len(report['cases']), 'error': report.get('error')}))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
