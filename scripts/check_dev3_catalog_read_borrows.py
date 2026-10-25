#!/usr/bin/env python3
"""Compile catalog view/guard/owner loans against an explicitly preserved source."""
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

PROBE = 'crates/engine/src/storage/wal_io/plan4_retention/read/catalog_borrow_probe.rs'
LIB = 'crates/engine/src/storage/wal_io/plan4_retention/read.rs'
KINDS = {'release': 'E0505', 'drop': 'E0505', 'owner': 'E0499'}
PREFIX = """#![allow(dead_code)]
use super::RetentionReader;
use crate::transaction::concurrency::Owner;
use foundation::{identity::ObjectId, namespace::WriterNamespace};
fn probe<'b, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>(
    reader: &'b RetentionReader<'b, 's, 'c, 'p, 'd, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>, object: ObjectId,
) {
    let guard = reader.read_catalog(owner).unwrap();
"""
MODULE = '\n#[path = "read/catalog_borrow_probe.rs"]\nmod catalog_borrow_probe;\n'


def probe_source(kind, positive):
    if kind not in KINDS:
        raise ValueError('unknown probe')
    text = PREFIX
    if kind in ('release', 'drop'):
        text += '    let record = guard.lookup(object).unwrap();\n'
        use = '    std::hint::black_box(record.map(|view| view.header()));\n'
        mutation = 'guard.release_checked().unwrap();' if kind == 'release' else 'drop(guard);'
        if positive:
            text += use
        text += '    ' + mutation + ('\n' if positive else ' // expected diagnostic\n')
        if not positive:
            text += use
    else:
        if positive:
            text += '    guard.release_checked().unwrap();\n'
        text += '    let alias = &mut *owner;' + ('\n' if positive else ' // expected diagnostic\n')
        text += '    std::hint::black_box(alias);\n'
        if not positive:
            text += '    std::hint::black_box(guard.lookup(object).unwrap());\n'
    text += '}\n'
    marker = next((i for i, line in enumerate(text.splitlines(), 1)
                   if '// expected diagnostic' in line), None)
    return text, marker


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True,
                        help='retained immutable source; this tool never modifies it')
    parser.add_argument('--artifact-parent', type=Path, required=True)
    args = parser.parse_args()
    source = args.source.resolve()
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    session = Path(tempfile.mkdtemp(prefix='catalog-read-borrows-', dir=args.artifact_parent)).resolve()
    report = {'status': 'RUNNING', 'source': str(source), 'cases': [],
              'scope': 'catalog record loan prevents guard release/drop; live guard prevents owner reuse'}
    environment = os.environ.copy()
    for key in list(environment):
        if key.startswith(('CARGO_PROFILE_', 'XADBTEST_')) or key in (
                'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER',
                'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            environment.pop(key)
    environment.update(CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2')
    command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--locked', '--offline',
               '--message-format=json']
    checker = Path(__file__).resolve()
    report['checker_sha256'] = {str(path): digest(path) for path in (
        checker, checker.with_name('check_dev3_buffer_borrows.py'),
        checker.with_name('check_dev0.py'), checker.with_name('comm_campaign.py'))}
    try:
        report['source_sha256'] = source_fingerprint(source)
        report['toolchain'] = subprocess.check_output(['rustc', '-Vv'], cwd=source, text=True)
        # Only one disposable export/target is live. Each probe is retained as a
        # patch against the immutable source, with exact logs and source hash.
        with tempfile.TemporaryDirectory(prefix='compiler-work-', dir=session) as temporary:
            work = Path(temporary)
            checkout = work / 'source'
            if capture_source(source, checkout) != report['source_sha256']:
                raise ValueError('compiler export differs from retained source')
            environment['CARGO_TARGET_DIR'] = str(work / 'target')
            original = (checkout / LIB).read_text()
            for name, kind, positive in [('baseline', None, True)] + [
                    (f'{kind}-{mode}', kind, mode == 'positive')
                    for kind in KINDS for mode in ('positive', 'negative')]:
                evidence = session / name
                evidence.mkdir()
                expected_code = marker = None
                if kind is not None:
                    text, marker = probe_source(kind, positive)
                    (checkout / PROBE).write_text(text)
                    modified = original + MODULE
                    (checkout / LIB).write_text(modified)
                    patch = ''.join(difflib.unified_diff(original.splitlines(True), modified.splitlines(True), fromfile=LIB, tofile=LIB))
                    patch += ''.join(difflib.unified_diff([], text.splitlines(True), fromfile='/dev/null', tofile=PROBE))
                    (evidence / 'probe.patch').write_text(patch)
                    expected_code = None if positive else KINDS[kind]
                fingerprint = source_fingerprint(checkout)
                row = {'name': name, 'status': 'RUNNING', 'source_sha256': fingerprint,
                       'expected_code': expected_code, 'expected_line': marker}
                report['cases'].append(row)
                output, code = run_compiler(command, checkout, environment, evidence, row)
                row['diagnostics'] = check_diagnostics(output, code, expected_code, marker,
                                                       probe_path=PROBE)
                if source_fingerprint(checkout) != fingerprint:
                    raise ValueError('probe source changed during compilation')
                row['status'] = 'PASS'
            report['removed_cache_bytes'] = sum(p.stat().st_size for p in work.rglob('*') if p.is_file())
        report['cleanup'] = 'disposable source and target removed; original source, patches and logs retained'
        if source_fingerprint(source) != report['source_sha256']:
            raise ValueError('retained source changed during verification')
        if any(digest(Path(path)) != value for path, value in report['checker_sha256'].items()):
            raise ValueError('checker changed during verification')
        report['status'] = 'PASS'
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, KeyboardInterrupt) as error:
        report.update(status='FAIL', error=str(error))
        for row in report['cases']:
            if row['status'] == 'RUNNING':
                row['status'] = 'FAIL'
    write_json(session / 'result.json', report)
    print(json.dumps({'status': report['status'], 'cases': len(report['cases']),
                      'result': str(session / 'result.json'), 'error': report.get('error')}))
    return int(report['status'] != 'PASS')


if __name__ == '__main__':
    raise SystemExit(main())
