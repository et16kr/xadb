#!/usr/bin/env python3
"""Run only S3-N01: bounded actual Commit, then current Row recovery gate.

Both processes use one immutable test ELF. Frame envelopes are checked here
with stdlib SHA-256; frames 15/16/17 use previously frozen independent literals.
No product encoder, decoder, output-generated oracle, or recovery READY claim.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(root):
    return {p.name: {"bytes": p.stat().st_size, "sha256": digest(p.read_bytes())}
            for p in sorted(root.iterdir()) if p.is_file()}


def verify_wal(path, fixture):
    wal = path.read_bytes()
    assert len(wal) <= 4 * 1024 * 1024
    literals = {
        15: (57336, '6e3badc82e1864c405f846a05372e2367f6a266a10e0a4e0c8c3d28267870fbe'),
        16: (91497, '0ee305722bff97a71161e2111bb7a15059b3a35b8a941c603a45e2fc5de81043'),
        17: (56712, '5a2e224034872faa810d76813c997732dd13babef9253dc435a7ef3be3799dc3'),
    }
    assert digest(fixture.read_bytes()) == literals[17][1]
    at = 4096
    frames = []
    for lsn in range(1, 18):
        length = int.from_bytes(wal[at + 12:at + 16], 'little')
        assert 192 <= length <= 1048576 and at + length <= len(wal)
        frame = wal[at:at + length]
        assert frame[:4] == b'XWF1'
        assert hashlib.sha256(frame[:128]).digest() == frame[128:160]
        assert hashlib.sha256(frame[:-32]).digest() == frame[-32:]
        assert int.from_bytes(frame[56:64], 'little') == lsn
        assert int.from_bytes(frame[64:72], 'little') == lsn - 1
        actual = (length, digest(frame))
        if lsn in literals:
            assert actual == literals[lsn], (lsn, actual)
        if lsn == 17:
            assert frame == fixture.read_bytes()
        frames.append({"lsn": lsn, "bytes": length, "sha256": actual[1],
                       "oracle": "independent literal" if lsn in literals else "envelope/order only"})
        at += length
    assert at == len(wal), "no unverified trailing bytes"
    return frames


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    db = out / 'db'
    db.mkdir()
    files = sorted(set(subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root
    ).decode().split('\0')) - {''})
    manifest = ''.join(f'{digest((root / p).read_bytes())}  {p}\n'
                       for p in files if (root / p).is_file())
    (out / 'source-files.sha256').write_text(manifest)
    source_hash, elf_hash = digest(manifest.encode()), digest(elf.read_bytes())
    fixture = root / 'tests/fixtures/dev3-module-c3/row-commit/commit.bin'
    names = [line[:-6] for line in subprocess.check_output([str(elf), '--list'], text=True).splitlines()
             if line.endswith('::s3_n01_native_worker: test')]
    assert len(names) == 1
    plan = dict(selector=names[0], phases=['produce', 'restart'], source=source_hash,
                elf=elf_hash, input=digest(fixture.read_bytes()), per_process_seconds=180,
                total_seconds=360, log_bytes_cap=16777216, rust_stack=33554432,
                expected='ACK q2 LSN17; separate restart Analysis/Unsupported unchanged; READY0')
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=plan['input'],
               XADB_S3_NATIVE_DB=str(db))
    results, pids = [], []
    for phase in plan['phases']:
        output = out / phase
        output.mkdir()
        env.update(XADB_S3_NATIVE_PHASE=phase, XADB_S3_NATIVE_OUTPUT=str(output))
        assert digest(elf.read_bytes()) == elf_hash
        start = time.monotonic()
        try:
            with (output / 'test.log').open('wb') as log:
                code = subprocess.run([str(elf), names[0], '--exact', '--ignored', '--nocapture',
                                       '--test-threads=1'], env=env, cwd=root,
                                      stdout=log, stderr=subprocess.STDOUT, timeout=180).returncode
        except subprocess.TimeoutExpired:
            code = 124
        results.append(dict(phase=phase, exit=code, seconds=time.monotonic()-start))
        (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        assert (output / 'test.log').stat().st_size <= plan['log_bytes_cap']
        print(phase, code, flush=True)
        if code:
            raise SystemExit(code)
        receipt = dict(line.split('\t', 1) for line in (output / 'receipt.tsv').read_text().splitlines())
        pids.append(receipt['pid'])
        assert receipt['source'] == source_hash and receipt['elf'] == elf_hash
        current = snapshot(db)
        (output / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
        if phase == 'produce':
            original = current
            frames = verify_wal(db / 'd3-wal-00000001', fixture)
            (output / 'frames.json').write_text(json.dumps(frames, indent=2) + '\n')
        else:
            assert current == original, 'restart changed database bytes'
    assert len(set(pids)) == 2
    (out / 'complete.json').write_text(json.dumps(dict(pids=pids, result='PASS', ready=False), indent=2)+'\n')


if __name__ == '__main__':
    main()
