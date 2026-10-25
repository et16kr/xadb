#!/usr/bin/env python3
"""Compile frozen fresh-storage ownership probes; no database execution."""
import argparse
import difflib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_buffer_borrows import check_diagnostics, run_compiler
from comm_campaign import capture_source, write_json

ROOT = Path(__file__).resolve().parent.parent
INPUTS = ROOT / 'crates/engine/tests/fixtures/dev3-storage-session/compiler'
INPUTS_SHA256 = 'dbe99337f15eb8a0d4d57de2097062b302a8b8a1627aa84a365d42d9a310278d'
PROBE = 'crates/engine/src/bootstrap_borrow_probe.rs'
LIB = 'crates/engine/src/lib.rs'


def inputs():
    if digest(INPUTS / 'manifest.json') != INPUTS_SHA256:
        raise ValueError('frozen compiler manifest changed')
    manifest = json.loads((INPUTS / 'manifest.json').read_text())
    cases = manifest['cases']
    if [c['name'] for c in cases] != ['baseline', 'shared_thread', 'buffer_thread', 'session', 'slot', 'root', 'scratch', 'writer']:
        raise ValueError('frozen compiler case identities changed')
    for case in cases[1:]:
        source = INPUTS / (case['name'] + '.rs')
        if source.stat().st_size != case['bytes'] or digest(source) != case['sha256']:
            raise ValueError('frozen compiler source changed')
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact-parent', type=Path, default=ROOT / 'artifacts/dev3')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if args.report is not None and (args.report.exists() or args.report.is_symlink()):
        parser.error('report already exists')
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='storage-borrows-', dir=args.artifact_parent)).resolve()
    report = {'scope': 'private fresh storage baseline1/positive2/negative5 compiler probes',
              'status': 'RUNNING', 'artifact_directory': str(session), 'cases': []}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    environment['CARGO_TARGET_DIR'] = str(session / 'target')
    checkers = [Path(__file__).resolve(), ROOT / 'scripts/check_dev3_buffer_borrows.py',
                ROOT / 'scripts/check_dev0.py', ROOT / 'scripts/comm_campaign.py',
                INPUTS / 'manifest.json', *sorted(INPUTS.glob('*.rs'))]
    report['checker_sha256'] = {str(p): digest(p) for p in checkers}
    code = 0
    try:
        baseline = session / 'baseline'
        report['source_sha256'] = capture_source(ROOT, baseline)
        original_lib = (baseline / LIB).read_text()
        command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--locked', '--offline', '--message-format=json']
        report['toolchain'] = subprocess.check_output(['rustc', '-Vv'], cwd=baseline, text=True)
        for frozen in inputs():
            kind = frozen['name']
            source = baseline if kind == 'baseline' else session / kind / 'source'
            evidence = session / 'baseline-logs' if kind == 'baseline' else source.parent
            evidence.mkdir(parents=True, exist_ok=True)
            expected_code = expected_line = None
            if kind != 'baseline':
                if capture_source(baseline, source) != report['source_sha256']:
                    raise ValueError('probe base differs from baseline')
                probe = (INPUTS / (kind + '.rs')).read_text()
                if (source / PROBE).exists():
                    raise ValueError('probe path already exists')
                (source / PROBE).write_text(probe)
                modified = original_lib + '\nmod bootstrap_borrow_probe;\n'
                (source / LIB).write_text(modified)
                patch = ''.join(difflib.unified_diff(original_lib.splitlines(True), modified.splitlines(True), fromfile=LIB, tofile=LIB))
                patch += ''.join(difflib.unified_diff([], probe.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                (evidence / 'probe.patch').write_text(patch)
                expected_code, expected_line = frozen['expected_code'], frozen['primary_line']
            fingerprint = source_fingerprint(source)
            row = {'name': kind, 'source': str(source), 'source_sha256': fingerprint,
                   'expected_code': expected_code, 'expected_line': expected_line, 'status': 'RUNNING'}
            report['cases'].append(row)
            output, returncode = run_compiler(command, source, environment, evidence, row)
            row['diagnostics'] = check_diagnostics(output, returncode, expected_code, expected_line, probe_path=PROBE)
            if source_fingerprint(source) != fingerprint:
                raise ValueError('probe source changed during compilation')
            row['status'] = 'PASS'
        if source_fingerprint(baseline) != report['source_sha256'] or source_fingerprint(ROOT) != report['source_sha256']:
            raise ValueError('original source changed during verification')
        if any(digest(Path(p)) != h for p, h in report['checker_sha256'].items()):
            raise ValueError('checker changed during verification')
        report['status'] = 'PASS'
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report['status'], report['error'], code = 'FAIL', str(error), 1
        for row in report['cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
    write_json(session / 'result.json', report)
    if args.report is not None:
        write_json(args.report, report)
    print(json.dumps({'status': report['status'], 'cases': len(report['cases']), 'artifact_directory': str(session), 'error': report.get('error')}))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
