#!/usr/bin/env python3
"""Run only the S7 native C5 join cases MCI02-N01..03.

Each case is three processes of one immutable test ELF over its own
namespace: `produce` runs the actual native writer (CREATE, A's driver) and
stops the process at its cut (after the Commit ACK, or after the first
durable CLR of a full rollback); `recover` runs C's recovery with A's
recovered CLR handler to READY, B's cursor read, and a new A row with its
Commit ACK; `reenter` runs the C+B recovery and B's cursor read of both
ACKs. Snapshots are stdlib SHA-256 only; no product decoder or
output-generated oracle is used here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

PHASES = ['produce', 'recover', 'reenter']
CASES = {case: PHASES for case in ('MCI02-N01', 'MCI02-N02', 'MCI02-N03')}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(root):
    files = {p.name: {"bytes": p.stat().st_size, "sha256": digest(p.read_bytes())}
             for p in sorted(root.iterdir()) if p.is_file()}
    assert len(files) <= 24 and sum(f["bytes"] for f in files.values()) <= 48 * 1024 * 1024
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--input', required=True, type=Path,
                        help='frozen test inventory whose hash is the run input')
    parser.add_argument('cases', nargs='*', help='subset of case ids (default: all)')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    selected = args.cases or list(CASES)
    assert all(case in CASES for case in selected), selected
    files = sorted(set(subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root
    ).decode().split('\0')) - {''})
    manifest = ''.join(f'{digest((root / p).read_bytes())}  {p}\n'
                       for p in files if (root / p).is_file())
    (out / 'source-files.sha256').write_text(manifest)
    source_hash, elf_hash = digest(manifest.encode()), digest(elf.read_bytes())
    input_hash = digest(args.input.resolve(strict=True).read_bytes())
    names = [line[:-6] for line in subprocess.check_output([str(elf), '--list', '--ignored'],
                                                           text=True).splitlines()
             if line.endswith('::s7j_native_worker: test')]
    assert len(names) == 1, names
    plan = dict(selector=names[0], cases={c: CASES[c] for c in selected}, source=source_hash,
                elf=elf_hash, input=input_hash, per_process_seconds=180,
                log_bytes_cap=16777216, rust_stack=33554432,
                expected='produce stops at its cut with the ACK (N01/N02) or CLR1 '
                         '(N03) receipt; recover is READY with B reading only '
                         'committed rows and a new ACK; reenter reads both ACKs')
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    results, complete = [], {}
    for case in selected:
        db = out / case / 'db'
        db.mkdir(parents=True)
        pids, previous, acks = [], None, []
        for phase in CASES[case]:
            output = out / case / phase
            output.mkdir()
            env.update(XADB_S7_NATIVE_CASE=case, XADB_S7_NATIVE_PHASE=phase,
                       XADB_S7_NATIVE_DB=str(db), XADB_S7_NATIVE_OUTPUT=str(output))
            assert digest(elf.read_bytes()) == elf_hash
            start = time.monotonic()
            try:
                with (output / 'test.log').open('wb') as log:
                    code = subprocess.run([str(elf), names[0], '--exact', '--ignored',
                                           '--nocapture', '--test-threads=1'], env=env, cwd=root,
                                          stdout=log, stderr=subprocess.STDOUT,
                                          timeout=180).returncode
            except subprocess.TimeoutExpired:
                code = 124
            seconds = time.monotonic() - start
            results.append(dict(case=case, phase=phase, exit=code, seconds=round(seconds, 2)))
            (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
            assert (output / 'test.log').stat().st_size <= plan['log_bytes_cap']
            print(case, phase, code, round(seconds, 2), flush=True)
            if code:
                raise SystemExit(code)
            receipt = dict(line.split('\t', 1)
                           for line in (output / 'receipt.tsv').read_text().splitlines())
            assert receipt['source'] == source_hash and receipt['elf'] == elf_hash
            assert receipt['case'] == case and receipt['phase'] == phase
            pids.append(receipt['pid'])
            current = snapshot(db)
            (output / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
            if phase == 'produce':
                assert receipt['result'] == ('crashed' if case == 'MCI02-N03' else 'acked')
                acks.append(receipt['q'])
            else:
                assert receipt['result'] == 'ready'
                assert current != previous, 'the process wrote nothing'
                if phase == 'recover':
                    acks.append(receipt['ack_q'])
            previous = current
        assert len(set(pids)) == len(CASES[case]), pids
        complete[case] = dict(pids=pids, acks=acks, result='PASS')
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
