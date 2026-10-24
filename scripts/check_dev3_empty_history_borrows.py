#!/usr/bin/env python3
"""Compile selected live-history/load probes in isolated exports; no tests run."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from comm_campaign import capture_source

ROOT = Path(__file__).resolve().parent.parent
INPUT = Path('crates/engine/tests/fixtures/dev3-empty-transactions/history-compiler')
ANCHOR = Path('crates/engine/src/storage/wal_io/status_home_flush.rs')
PROBE = ANCHOR.with_name('empty_history_borrow_probe.rs')
PROBES = ('flush_sequence', 'sealed_history', 'load_sequence', 'sealed_load')
PRIVATE_SPANS = {
    'sealed_history': [(4, 15, 22), (4, 38, 43)],
    'sealed_load': [(5, 24, 32), (5, 34, 39), (5, 44, 53),
                    (5, 62, 67), (5, 69, 76), (5, 78, 81)],
}


def verify(output, code, name):
    messages = [json.loads(line) for line in output.splitlines() if line.startswith('{')]
    finishes = [m for m in messages if m.get('reason') == 'build-finished']
    errors = [m['message'] for m in messages if m.get('reason') == 'compiler-message'
              and m['message'].get('level') == 'error']
    negative = name in PRIVATE_SPANS
    if name not in PROBES or len(finishes) != 1:
        raise ValueError('unknown probe or missing compiler completion')
    if finishes[0].get('success') is not (not negative) or (code != 0) != negative:
        raise ValueError('unexpected compiler completion')
    if not negative:
        if errors:
            raise ValueError('positive probe has compiler errors')
        return
    if len(errors) != 1 or (errors[0].get('code') or {}).get('code') != 'E0451':
        raise ValueError('one private-field diagnostic required')
    spans = [s for s in errors[0].get('spans', []) if s.get('is_primary') is True]
    actual = [(s['file_name'], s['line_start'], s['line_end'], s['column_start'], s['column_end']) for s in spans]
    expected = [(str(PROBE), line, line, start, end) for line, start, end in PRIVATE_SPANS[name]]
    if actual != expected:
        raise ValueError('private-field diagnostic must identify every exact field')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifact-parent', type=Path, default=ROOT / 'artifacts/dev3')
    parser.add_argument('--case', action='append', choices=PROBES)
    args = parser.parse_args()
    args.artifact_parent.mkdir(parents=True, exist_ok=True)
    out = Path(tempfile.mkdtemp(prefix='empty-history-compiler-', dir=args.artifact_parent)).resolve()
    baseline = out / 'baseline'
    source_sha = capture_source(ROOT, baseline)
    env = os.environ.copy()
    for key in list(env):
        if key.startswith(('XADBTEST_', 'CARGO_PROFILE_')) or key in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'RUSTDOC', 'RUSTUP_TOOLCHAIN'):
            env.pop(key)
    env['CARGO_TARGET_DIR'] = str(out / 'target')
    selected = args.case or list(PROBES)
    if len(selected) != len(set(selected)):
        raise ValueError('duplicate probe selection')
    for name in selected:
        source = out / name / 'source'
        assert capture_source(baseline, source) == source_sha
        program = (baseline / INPUT / (name + '.rs')).read_bytes()
        (source / PROBE).write_bytes(program)
        with (source / ANCHOR).open('a') as file:
            file.write('\n#[path = "empty_history_borrow_probe.rs"]\nmod empty_history_borrow_probe;\n')
        command = ['cargo', 'check', '-p', 'xadb-engine', '--lib', '--locked', '--offline', '--message-format=json']
        started = time.monotonic()
        result = subprocess.run(command, cwd=source, env=env, capture_output=True, timeout=180)
        directory = source.parent
        (directory / 'stdout.jsonl').write_bytes(result.stdout)
        (directory / 'stderr').write_bytes(result.stderr)
        receipt = {'source_sha256': source_sha, 'program_sha256': hashlib.sha256(program).hexdigest(), 'command': command, 'exit': result.returncode, 'elapsed_seconds': time.monotonic() - started, 'stdout_sha256': hashlib.sha256(result.stdout).hexdigest(), 'stderr_sha256': hashlib.sha256(result.stderr).hexdigest(), 'verdict': 'NOT_EVALUATED'}
        (directory / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        verify(result.stdout.decode(), result.returncode, name)
        receipt['verdict'] = 'PASS'
        (directory / 'verified.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(name, 'PASS', directory, flush=True)


if __name__ == '__main__':
    main()
