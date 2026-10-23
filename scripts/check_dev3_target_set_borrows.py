#!/usr/bin/env python3
"""Compile exact target-set capacity/lifetime probes; never execute test binaries."""
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
INPUTS = ROOT / 'crates/engine/tests/fixtures/dev3-target-set/compiler'
PROBE = 'crates/engine/src/storage/target_set_borrow_probe.rs'
ANCHOR = 'crates/engine/src/storage/mod.rs'
ASSERT_SOURCE = 'crates/engine/src/storage/dirty_frame.rs'
ASSERT_MARKER = '// target-set capacity assertion'
BUDGET_SECONDS = 120
EXPECTATIONS = {'baseline': None, 'release': None, 'owner_reborrow': 'E0499',
                'pins_reuse': 'E0382', 'read_sync': 'E0277',
                'capacity_0': 'E0080', 'capacity_65': 'E0080'}
PRIMARY_LINES = {'owner_reborrow': 6, 'pins_reuse': 7, 'read_sync': 6}


def inputs(directory=INPUTS):
    manifest = json.loads((directory / 'manifest.json').read_text())
    rows = manifest['cases']
    if ([x['name'] for x in rows] != list(EXPECTATIONS)
            or manifest['schema'] != 'xadb-target-set-compiler-v1'
            or manifest['status'] != 'FROZEN_INPUTS_NOT_EXECUTION_EVIDENCE'
            or manifest['compiler_budget_seconds'] != BUDGET_SECONDS
            or manifest['assertion_marker'] != ASSERT_MARKER
            or manifest['probe_relative_path'] != PROBE
            or manifest['anchor_relative_path'] != ANCHOR):
        raise ValueError('compiler inventory changed')
    if {p.name for p in directory.iterdir()} != {'manifest.json', *(n + '.rs' for n in EXPECTATIONS if n != 'baseline')}:
        raise ValueError('unregistered or missing program')
    for row in rows:
        name = row['name']
        path = ASSERT_SOURCE if name.startswith('capacity_') else PROBE if EXPECTATIONS[name] else None
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
        if name.startswith('capacity_'):
            capacity = name.split('_')[1]
            text = program.read_text()
            if '#[test]' not in text or f'&[request; {capacity}]' not in text:
                raise ValueError('capacity probe lacks concrete test instantiation')
    return rows


def select(rows, names):
    if names is None:
        return rows
    if not names or len(names) != len(set(names)) or set(names) - EXPECTATIONS.keys():
        raise ValueError('unknown, empty or duplicate selection')
    return [row for row in rows if row['name'] in set(names) | {'baseline'}]


def assertion_line(source):
    lines = (source / ASSERT_SOURCE).read_text().splitlines()
    positions = [n for n, text in enumerate(lines, 1) if ASSERT_MARKER in text]
    if len(positions) != 1 or 'assert!(N >= 1 && N <= MAX_FRAMES)' not in lines[positions[0] - 1]:
        raise ValueError('exact capacity assertion changed')
    return positions[0]


def verify(output, returncode, row, line):
    expected_line = line if row['name'].startswith('capacity_') else row['primary_line']
    result = check_diagnostics(output, returncode, row['expected_code'], expected_line,
                               probe_path=row['primary_path'] or PROBE)
    if row['name'].startswith('capacity_'):
        capacity = row['name'].split('_')[1]
        messages = [json.loads(text) for text in output.splitlines() if text.startswith('{')]
        errors = [message['message'] for message in messages
                  if message.get('reason') == 'compiler-message'
                  and message.get('message', {}).get('level') == 'error']
        # check_diagnostics already required this exact single coded primary error.
        rendered = errors[0].get('rendered') or ''
        if f'reserve_target_capacity::<{capacity}>' not in rendered:
            raise ValueError('capacity rejection lacks concrete production instantiation')
    return result



def checker_bindings():
    """Identify the executed runner and the actual imported local module files."""
    paths = {Path(__file__).resolve()}
    paths.update(Path(sys.modules[function.__module__].__file__).resolve()
                 for function in (digest, source_fingerprint, check_diagnostics,
                                  run_compiler, capture_source, write_json))
    expected = {ROOT / ('scripts/' + name) for name in (
        'check_dev3_target_set_borrows.py', 'check_dev0.py',
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
    session = Path(tempfile.mkdtemp(prefix='target-set-compiler-', dir=args.artifact_parent)).resolve()
    report = {'status': 'RUNNING', 'cases': [], 'artifact_directory': str(session),
              'budget_seconds': BUDGET_SECONDS, 'compiler_elapsed_seconds': 0,
              'scope': 'exact target capacity and lifetime compile only; no product runtime'}
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
        line = assertion_line(baseline)
        report['assertion_line'] = line
        report['assertion_source_sha256'] = digest(baseline / ASSERT_SOURCE)
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
                modified = original + '\n#[cfg(test)]\nmod target_set_borrow_probe;\n'
                (source / ANCHOR).write_text(modified)
                patch = ''.join(difflib.unified_diff(original.splitlines(True), modified.splitlines(True), fromfile=ANCHOR, tofile=ANCHOR))
                patch += ''.join(difflib.unified_diff([], program.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                (evidence / 'probe.patch').write_text(patch)
            codegen = name == 'baseline' or name.startswith('capacity_')
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
            row['diagnostics'] = verify(output, returncode, frozen, line)
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
