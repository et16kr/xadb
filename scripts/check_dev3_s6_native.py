#!/usr/bin/env python3
"""Run only the S6 native cases: MBC40-N01..03 (C4 Row recovery READY with a
restart in between) and MCB05-N01..06 (RejectInitialLosers refusals).

Every phase is a separate process of one immutable test ELF over its own
namespace. The produce process writes the actual memory writer run's durable
files as the crash cut; every restart process runs the actual recovery entry
on the native Writer/File. Snapshots are stdlib SHA-256 only; no product
decoder or output-generated oracle is used here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

CASES = {
    'MBC40-N01': ['produce', 'recover', 'reenter'],
    'MBC40-N02': ['produce', 'crash', 'recover'],
    'MBC40-N03': ['produce', 'crash', 'recover'],
    'MCB05-N01': ['produce', 'reject'],
    'MCB05-N02': ['produce', 'reject'],
    'MCB05-N03': ['produce', 'reject'],
    'MCB05-N04': ['produce', 'reject'],
    'MCB05-N05': ['produce', 'reject'],
    'MCB05-N06': ['produce', 'reject'],
}


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
             if line.endswith('::s6r_native_worker: test')]
    assert len(names) == 1, names
    plan = dict(selector=names[0], cases={c: CASES[c] for c in selected}, source=source_hash,
                elf=elf_hash, input=input_hash, per_process_seconds=300,
                log_bytes_cap=16777216, rust_stack=33554432,
                expected='MBC40: READY after a separate restart (N02/N03 after a crashed '
                         'recovery process); MCB05: Analysis/Unsupported, files unchanged')
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    results, complete = [], {}
    for case in selected:
        db = out / case / 'db'
        db.mkdir(parents=True)
        pids, produced = [], None
        for phase in CASES[case]:
            output = out / case / phase
            output.mkdir()
            env.update(XADB_S6_NATIVE_CASE=case, XADB_S6_NATIVE_PHASE=phase,
                       XADB_S6_NATIVE_DB=str(db), XADB_S6_NATIVE_OUTPUT=str(output))
            assert digest(elf.read_bytes()) == elf_hash
            start = time.monotonic()
            try:
                with (output / 'test.log').open('wb') as log:
                    code = subprocess.run([str(elf), names[0], '--exact', '--ignored',
                                           '--nocapture', '--test-threads=1'], env=env, cwd=root,
                                          stdout=log, stderr=subprocess.STDOUT,
                                          timeout=300).returncode
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
                produced = current
            elif phase == 'reject':
                assert receipt['result'] == 'refused'
                assert current == produced, 'refused restart changed database bytes'
            else:
                assert receipt['result'] == ('crashed' if phase == 'crash' else 'ready')
                assert current != produced, 'recovery wrote nothing'
        assert len(set(pids)) == len(CASES[case]), pids
        complete[case] = dict(pids=pids, result='PASS')
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
