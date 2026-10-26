#!/usr/bin/env python3
"""Compile private leaf-candidate loans against an explicitly preserved source."""
import argparse
import difflib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
from check_dev0 import digest, source_fingerprint
from check_dev3_buffer_borrows import check_diagnostics, run_compiler
from comm_campaign import capture_source, write_json

PROBE = 'crates/engine/src/tree_edit_borrow_probe.rs'
LIB = 'crates/engine/src/lib.rs'
KINDS = {'source': 'E0502', 'cell': 'E0502', 'scratch': 'E0499'}
PREFIX = '''#![allow(dead_code)]
use crate::storage::{page::PageLsn, tree_page::{decode_tree_page, TreeExpectation,
    edit::{LeafEdit, MAX_LEAF_EDIT_CELLS}}};
fn probe(source: &mut [u8], cell: &mut [u8], expected: TreeExpectation) {
    let view = decode_tree_page(source, expected).unwrap();
    let mut scratch = [&[][..]; MAX_LEAF_EDIT_CELLS];
    let plan = view.prepare_leaf_edit(LeafEdit::Insert(cell), &mut scratch).unwrap();
'''


def probe_source(kind, positive):
    mutation = {'source': 'source.fill(0);', 'cell': 'cell.fill(0);',
                'scratch': 'scratch.fill(&[]);'}[kind]
    text = PREFIX
    if positive:
        text += '    std::hint::black_box(plan.entry_bytes());\n    drop(plan);\n'
    text += '    ' + mutation + ('\n' if positive else ' // expected diagnostic\n')
    if not positive:
        text += '    std::hint::black_box(plan.entry_bytes());\n'
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
    session = Path(tempfile.mkdtemp(prefix='tree-edit-borrows-', dir=args.artifact_parent)).resolve()
    report = {'status': 'RUNNING', 'source': str(source), 'cases': [],
              'scope': 'leaf candidate source, supplied cell and scratch compiler loans'}
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
                    modified = original + '\nmod tree_edit_borrow_probe;\n'
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
