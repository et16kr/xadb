#!/usr/bin/env python3
"""Compile actual private ReadBuffer lifetimes; diagnostics are not runtime evidence."""
import argparse
import difflib
import json
import os
import signal
from pathlib import Path
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from comm_campaign import capture_source, write_json

ROOT = Path(__file__).resolve().parent.parent
PROBE = 'crates/engine/src/buffer_borrow_probe.rs'
LIB = 'crates/engine/src/lib.rs'
PREFIX = '''#![allow(dead_code)]
use crate::storage::{page::{PageIdentity, PageKind, PAGE_BYTES}, page_io::PageFileReader, read_buffer::{ReadBuffer, ReadFrame}};
use xadb_foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(reader: PageFileReader<'_, W>, frames: &mut [ReadFrame], scratch: &mut [u8; PAGE_BYTES], identity: PageIdentity) {
    let buffer = ReadBuffer::new(reader, frames, scratch).unwrap();
'''

def probe_source(kind, positive):
    if kind == 'pool':
        body = '    let pin = buffer.pin(identity, PageKind::RowLeaf, 0).unwrap();\n'
        body += ('    drop(pin);\n    drop(buffer);\n' if positive else
                 '    drop(buffer); // expected diagnostic\n    drop(pin);\n')
    elif kind == 'pin':
        body = ('    let pin = buffer.pin(identity, PageKind::RowLeaf, 0).unwrap();\n'
                '    let guard = pin.read().unwrap();\n')
        body += ('    drop(guard);\n    drop(pin);\n' if positive else
                 '    drop(pin); // expected diagnostic\n    drop(guard);\n')
    elif kind == 'frames':
        second = '    let alias = &mut *frames;'
        body = ('    drop(buffer);\n' + second + '\n    std::hint::black_box(alias);\n' if positive else
                second + ' // expected diagnostic\n    std::hint::black_box(alias);\n    drop(buffer);\n')
    else:
        raise ValueError('unknown probe')
    text = PREFIX + body + '}\n'
    marker = next((i for i, line in enumerate(text.splitlines(), 1)
                   if '// expected diagnostic' in line), None)
    return text, marker


def check_diagnostics(output, returncode, expected_code=None, expected_line=None, *, probe_path=PROBE):
    """Require a successful baseline or exactly one coded primary probe error."""
    messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
    finishes = [m for m in messages if m.get('reason') == 'build-finished']
    errors = [m['message'] for m in messages if m.get('reason') == 'compiler-message'
              and m.get('message', {}).get('level') == 'error']
    if len(finishes) != 1:
        raise ValueError('missing/duplicate compiler completion')
    if expected_code is None:
        if returncode != 0 or finishes[0].get('success') is not True or errors:
            raise ValueError('positive/baseline did not compile')
        return []
    if returncode == 0 or finishes[0].get('success') is not False or len(errors) != 1:
        raise ValueError('negative needs exactly one actual compiler error')
    error = errors[0]
    if (error.get('code') or {}).get('code') != expected_code:
        raise ValueError('wrong compiler error code')
    primary = [s for s in error.get('spans', []) if s.get('is_primary') is True]
    if len(primary) != 1 or primary[0].get('file_name') != probe_path or (
            primary[0].get('line_start'), primary[0].get('line_end')) != (expected_line, expected_line):
        raise ValueError('error is not at the expected probe expression')
    return [{'code': expected_code, 'primary_span': primary[0]}]


def run_compiler(command, source, environment, evidence, row, timeout=180):
    """Stream separate JSON/stderr logs and own the whole compiler process group.

    Unlike runtime run_logged, an expected compiler failure is returned for exact
    code/span validation. Timeout, interrupt and surviving descendants always fail.
    """
    stdout, stderr = evidence / 'compiler.jsonl', evidence / 'compiler.stderr'
    interrupted = None
    remaining_group_signalled = False
    with stdout.open('xb') as out, stderr.open('xb') as err:
        process = subprocess.Popen(command, cwd=source, env=environment, stdout=out,
                                   stderr=err, start_new_session=True)
        try:
            process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt) as error:
            interrupted = error
        finally:
            try:
                os.killpg(process.pid, signal.SIGKILL)
                remaining_group_signalled = True
            except ProcessLookupError:
                pass
            process.wait(timeout=10)
    row.update({'command': list(map(str, command)), 'returncode': process.returncode,
                'stdout': str(stdout), 'stdout_sha256': digest(stdout),
                'stderr': str(stderr), 'stderr_sha256': digest(stderr),
                'interrupted': interrupted is not None,
                'remaining_group_signalled': remaining_group_signalled})
    if interrupted is not None:
        raise RuntimeError('compiler interrupted or exceeded timeout; partial logs retained') from interrupted
    if remaining_group_signalled:
        raise RuntimeError('compiler left descendants after completion; logs retained')
    return stdout.read_text(), process.returncode


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact-parent', type=Path, default=ROOT / 'artifacts' / 'dev3')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if args.report is not None and (args.report.exists() or args.report.is_symlink()):
        parser.error('report already exists')
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='buffer-borrows-', dir=args.artifact_parent)).resolve()
    report = {'scope': 'three private buffer lifetime compiler checks and three positive baselines',
              'status': 'RUNNING', 'artifact_directory': str(session), 'cases': []}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    environment['CARGO_TARGET_DIR'] = str(session / 'target')
    checker_paths = [Path(__file__).resolve(), ROOT / 'scripts/check_dev0.py', ROOT / 'scripts/comm_campaign.py']
    checker_hashes = {str(p): digest(p) for p in checker_paths}
    report['checker_sha256'] = checker_hashes
    code = 0
    try:
        baseline = session / 'baseline'
        report['source_sha256'] = capture_source(ROOT, baseline)
        original_lib = (baseline / LIB).read_text()
        command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--locked', '--offline', '--message-format=json']
        report['toolchain'] = subprocess.check_output(['rustc', '-Vv'], cwd=baseline, text=True)
        # Unmodified baseline always executes first; its failure cannot satisfy a negative probe.
        cases = [('baseline', None, True)] + [(f'{kind}-{mode}', kind, mode == 'positive')
                 for kind in ('pool', 'pin', 'frames') for mode in ('positive', 'negative')]
        for name, kind, positive in cases:
            source = baseline if kind is None else session / name / 'source'
            evidence = session / 'baseline-logs' if kind is None else source.parent
            evidence.mkdir(parents=True, exist_ok=True)
            expected_code = expected_line = None
            if kind is not None:
                captured = capture_source(baseline, source)
                if captured != report['source_sha256']:
                    raise ValueError('probe base differs from baseline')
                probe, marker = probe_source(kind, positive)
                if (source / PROBE).exists():
                    raise ValueError('probe path already exists')
                (source / PROBE).write_text(probe)
                modified_lib = original_lib + '\nmod buffer_borrow_probe;\n'
                (source / LIB).write_text(modified_lib)
                patch = ''.join(difflib.unified_diff(original_lib.splitlines(True), modified_lib.splitlines(True), fromfile=LIB, tofile=LIB))
                patch += ''.join(difflib.unified_diff([], probe.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                (evidence / 'probe.patch').write_text(patch)
                if not positive:
                    expected_code = 'E0499' if kind == 'frames' else 'E0505'
                    expected_line = marker
            fingerprint = source_fingerprint(source)
            row = {'name': name, 'source': str(source), 'source_sha256': fingerprint,
                   'expected_code': expected_code, 'expected_line': expected_line, 'status': 'RUNNING'}
            report['cases'].append(row)
            output, returncode = run_compiler(command, source, environment, evidence, row)
            row['diagnostics'] = check_diagnostics(output, returncode, expected_code, expected_line)
            if source_fingerprint(source) != fingerprint:
                raise ValueError('probe source changed during compilation')
            row['status'] = 'PASS'
        if source_fingerprint(baseline) != report['source_sha256'] or source_fingerprint(ROOT) != report['source_sha256']:
            raise ValueError('original source changed during verification')
        if any(digest(Path(p)) != h for p, h in checker_hashes.items()):
            raise ValueError('checker changed during verification')
        report['status'] = 'PASS'
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report['status'] = 'FAIL'
        report['error'] = str(error)
        for row in report['cases']:
            if row.get('status') == 'RUNNING':
                row['status'] = 'FAIL'
        code = 1
    write_json(session / 'result.json', report)
    if args.report is not None:
        write_json(args.report, report)
    print(json.dumps({'status': report['status'], 'cases': len(report['cases']), 'artifact_directory': str(session), 'error': report.get('error')}))
    return code

if __name__ == '__main__':
    raise SystemExit(main())
