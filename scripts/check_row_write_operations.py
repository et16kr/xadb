#!/usr/bin/env python3
"""Six bounded compiler probes for the sealed Row write operation protocol."""
import argparse
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
from comm_campaign import capture_source

ROOT = Path(__file__).resolve().parent.parent
MODULE = 'crates/engine/src/storage/mod.rs'
PROBE = 'crates/engine/src/storage/row_write_operation_probe.rs'
PREFIX = '''#![allow(dead_code)]
use super::disk::{RowPreflightOperation, RowReservedOperation, RowPreparedOperation, RowInputPlan, RowProposal};
'''
POSITIVE = '''fn preflight<P: RowPreflightOperation>(mut value: P, input: RowInputPlan) {
    drop(RowPreflightOperation::validation_lane(&mut value));
    let _ = RowPreflightOperation::reserve(value, input);
}
fn reserved<R: RowReservedOperation>(mut value: R, proposal: RowProposal) {
    drop(RowReservedOperation::producer_lane(&mut value));
    let _ = RowReservedOperation::prepare(value, proposal);
}
fn finish<P: RowPreflightOperation>(value: P) { let _ = RowPreflightOperation::finish(value); }
fn cancel<R: RowReservedOperation>(value: R) { let _ = RowReservedOperation::cancel(value); }
fn apply<P: RowPreparedOperation>(value: P) { let _ = RowPreparedOperation::apply(value); }
fn reopen<P: RowPreparedOperation>(value: P) {
    let reserved = RowPreparedOperation::reopen(value);
    let _ = RowReservedOperation::cancel(reserved);
}
'''


def probes():
    yield 'positive', PREFIX + POSITIVE, None
    for name, trait, method in (
            ('finish-reuse', 'RowPreflightOperation', 'finish'),
            ('cancel-reuse', 'RowReservedOperation', 'cancel'),
            ('apply-reuse', 'RowPreparedOperation', 'apply')):
        yield name, PREFIX + f'''fn probe<T: {trait}>(value: T) {{
    let _ = {trait}::{method}(value);
    let _ = {trait}::{method}(value); // expected diagnostic
}}
''', 'E0382'
    for name, trait, lane, method, argument in (
            ('validation-borrow', 'RowPreflightOperation', 'validation_lane', 'reserve', 'RowInputPlan'),
            ('producer-borrow', 'RowReservedOperation', 'producer_lane', 'prepare', 'RowProposal')):
        yield name, PREFIX + f'''fn probe<T: {trait}>(mut value: T, argument: {argument}) {{
    let lane = {trait}::{lane}(&mut value);
    let _ = {trait}::{method}(value, argument); // expected diagnostic
    drop(lane);
}}
''', 'E0505'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact-parent', type=Path)
    args = parser.parse_args()
    # The Rust entry point supplies the existing CI receipt directory. Local
    # standalone invocations keep the same logs in an exclusive temp directory.
    session = Path(tempfile.mkdtemp(prefix='xadb-row-operations-',
                                    dir=args.artifact_parent)).resolve()
    report = {'scope': 'F4 Row operation compiler probes', 'status': 'RUNNING', 'cases': []}
    source, target = session / 'source', session / 'target'
    started = time.monotonic()
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    environment['CARGO_TARGET_DIR'] = str(target)
    checkers = [Path(__file__).resolve(), ROOT / 'scripts/check_dev0.py',
                ROOT / 'scripts/comm_campaign.py', ROOT / 'scripts/check_dev3_buffer_borrows.py']
    report['checker_sha256'] = {str(p.relative_to(ROOT)): digest(p) for p in checkers}
    try:
        report['source_sha256'] = capture_source(ROOT, source)
        original = (source / MODULE).read_text()
        if (source / PROBE).exists():
            raise ValueError('probe path already exists')
        (source / MODULE).write_text(original + '\nmod row_write_operation_probe;\n')
        report['toolchain'] = subprocess.check_output(['rustc', '-Vv'], cwd=source, text=True,
                                                     env=environment, timeout=10)
        command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--locked', '--offline', '--message-format=json']
        for name, probe, expected in probes():
            # One positive control first; five negatives cannot mask a broken baseline.
            (source / PROBE).write_text(probe)
            logs = session / name
            logs.mkdir()
            (logs / 'probe.rs').write_text(probe)
            marker = next((i for i, line in enumerate(probe.splitlines(), 1)
                           if '// expected diagnostic' in line), None)
            fingerprint = source_fingerprint(source)
            row = {'name': name, 'status': 'RUNNING', 'source_sha256': fingerprint,
                   'probe_sha256': digest(logs / 'probe.rs'), 'expected_code': expected,
                   'expected_line': marker}
            report['cases'].append(row)
            remaining = 420 - (time.monotonic() - started)
            if remaining <= 0:
                raise RuntimeError('420 second total compiler budget exhausted')
            output, code = run_compiler(command, source, environment, logs, row,
                                        timeout=min(180, remaining))
            row['diagnostics'] = check_diagnostics(output, code, expected, marker, probe_path=PROBE)
            if source_fingerprint(source) != fingerprint:
                raise ValueError('probe source changed during compilation')
            row['status'] = 'PASS'
        (source / PROBE).unlink()
        (source / MODULE).write_text(original)
        if source_fingerprint(source) != report['source_sha256'] or source_fingerprint(ROOT) != report['source_sha256']:
            raise ValueError('original source changed during verification')
        if any(digest(ROOT / p) != h for p, h in report['checker_sha256'].items()):
            raise ValueError('checker changed during verification')
        report['status'] = 'PASS'
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        for row in report['cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
    finally:
        report['seconds'] = time.monotonic() - started
        (session / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
        print('F4_COMPILER_RESULT ' + json.dumps(report, sort_keys=True), flush=True)
        print('F4_COMPILER_LOG_DIRECTORY ' + str(session), flush=True)
        # Retain only logs/results/hashes. Source and ELF/build products are disposable.
        shutil.rmtree(source, ignore_errors=True)
        shutil.rmtree(target, ignore_errors=True)
    return 0 if report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
