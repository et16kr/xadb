#!/usr/bin/env python3
"""Run the S10 native case of MI03: C+B MCB01-N01 (consumers §9.2).

MCB01-N01 is the committed six-row corpus (MCB01) reopened by a second
process, with no A. Two processes of one immutable test ELF over one
namespace: `produce` (P1) is C's install, CREATE of T, the corpus statement
through C's checked ingress with the independent literal
(tests/fixtures/dev3-module-c2/row-read-six) and C's Commit (q 2), exiting
there without shutdown; `reenter` (P2) is C's recovery (RejectInitialLosers)
of those files to READY and B's point read of every RowId 1..7 and its
cursor over T under a read-only snapshot.

The expected reread is computed here from the author's expected.json (its
rows and gap), pinned by SHA-256, never from XADB output. The controller
runs from the full tree (source identity from git) or from a single-source
export (`--root` with the export's `--manifest`, checked file by file), as
check_dev3_s9_native.py does. Snapshots are stdlib SHA-256 only.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

CASE = 'MCB01-N01'
WORKER = 's10_corpus_native_worker'
PHASES = ['produce', 'reenter']
AUTHOR = 'tests/fixtures/dev3-module-c2/row-read-six/expected.json'
AUTHOR_SHA = '1d5a961628e7cd80b52742df100d267a4746920e3c08d5be7c5d9952c34e42dc'
SECONDS = 180


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(root):
    files = {p.name: {"bytes": p.stat().st_size, "sha256": digest(p.read_bytes())}
             for p in sorted(root.iterdir()) if p.is_file()}
    assert len(files) <= 24 and sum(f["bytes"] for f in files.values()) <= 48 * 1024 * 1024
    return files


def expected_reread(root):
    """(points of RowId 1..7, cursor rows) from the author's rows and gap."""
    raw = (root / AUTHOR).read_bytes()
    assert digest(raw) == AUTHOR_SHA, 'expected.json pin'
    author = json.loads(raw)
    values = {row: value for _, row, value in author['rows']}
    assert len(values) == 6 and not set(author['gap']) & set(values)
    points = ','.join(str(values[row]) if row in values else 'null' for row in range(1, 8))
    rows = ','.join(f'{row}:{values[row]}' for row in sorted(values))
    return points, rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--input', required=True, type=Path,
                        help='frozen test inventory whose hash is the run input')
    parser.add_argument('--root', type=Path,
                        help='source root to run from (default: this checkout)')
    parser.add_argument('--manifest', type=Path,
                        help="an export's source-files.sha256 (required with a root "
                             'that is not a git checkout)')
    args = parser.parse_args()
    root = (args.root or Path(__file__).resolve().parents[1]).resolve(strict=True)
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    if args.manifest:
        manifest = args.manifest.resolve(strict=True).read_text()
        listed = dict(reversed(line.split('  ', 1)) for line in manifest.splitlines())
        present = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()}
        assert present == set(listed), sorted(present ^ set(listed))[:8]
        for name, hashed in listed.items():
            assert digest((root / name).read_bytes()) == hashed, name
    else:
        files = sorted(set(subprocess.check_output(
            ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root
        ).decode().split('\0')) - {''})
        manifest = ''.join(f'{digest((root / p).read_bytes())}  {p}\n'
                           for p in files if (root / p).is_file())
    (out / 'source-files.sha256').write_text(manifest)
    source_hash, elf_hash = digest(manifest.encode()), digest(elf.read_bytes())
    input_hash = digest(args.input.resolve(strict=True).read_bytes())
    points, rows = expected_reread(root)
    listed = [line[:-6] for line in subprocess.check_output([str(elf), '--list', '--ignored'],
                                                            text=True).splitlines()]
    names = [name for name in listed if name.endswith(f'::{WORKER}')]
    assert len(names) == 1, names
    plan = dict(selector=names[0], root=str(root), case=CASE, phases=PHASES,
                source=source_hash, elf=elf_hash, input=input_hash,
                per_process_seconds=SECONDS, log_bytes_cap=16777216, rust_stack=33554432,
                expected=dict(points=points, rows=rows, q=2),
                summary='produce exits after C\'s Commit of the corpus (q 2) with its last '
                        'write synced; reenter is READY at q 2 and B reads the author\'s '
                        'rows by point (the gap Missing) and cursor; no A module is linked '
                        'or run')
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    db = out / CASE / 'db'
    db.mkdir(parents=True)
    results, pids, previous, changed = [], [], None, None
    for phase in PHASES:
        output = out / CASE / phase
        output.mkdir()
        env.update(XADB_S10_NATIVE_CASE=CASE, XADB_S10_NATIVE_PHASE=phase,
                   XADB_S10_NATIVE_DB=str(db), XADB_S10_NATIVE_OUTPUT=str(output))
        assert digest(elf.read_bytes()) == elf_hash
        start = time.monotonic()
        try:
            with (output / 'test.log').open('wb') as log:
                code = subprocess.run([str(elf), names[0], '--exact', '--ignored',
                                       '--nocapture', '--test-threads=1'], env=env, cwd=root,
                                      stdout=log, stderr=subprocess.STDOUT,
                                      timeout=SECONDS).returncode
        except subprocess.TimeoutExpired:
            code = 124
        seconds = time.monotonic() - start
        results.append(dict(case=CASE, phase=phase, exit=code, seconds=round(seconds, 2)))
        (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        assert (output / 'test.log').stat().st_size <= plan['log_bytes_cap']
        print(CASE, phase, code, round(seconds, 2), flush=True)
        if code:
            raise SystemExit(code)
        receipt = dict(line.split('\t', 1)
                       for line in (output / 'receipt.tsv').read_text().splitlines())
        assert receipt['source'] == source_hash and receipt['elf'] == elf_hash
        assert receipt['case'] == CASE and receipt['phase'] == phase
        pids.append(receipt['pid'])
        current = snapshot(db)
        (output / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
        if phase == 'produce':
            assert receipt['result'] == 'committed' and receipt['q'] == '2', receipt
            # The exact durable prefix at the cut: the in-process WAL witness
            # equals this controller's own digest of the WAL file, and only
            # lease release and closes follow the last successful sync.
            wal = sorted(name for name in current if name.startswith('d3-wal-'))[-1]
            assert receipt['wal_cut'] == current[wal]['sha256'], 'WAL witness'
            assert int(receipt['wal_cut_bytes']) == current[wal]['bytes'], 'WAL length'
            effects = [line.split('\t') for line in
                       (output / 'effects.tsv').read_text().splitlines()]
            writes = [i for i, row in enumerate(effects) if row[1] == 'write']
            syncs = [i for i, row in enumerate(effects)
                     if row[1] == 'sync' and row[5] == 'Ok(())']
            assert writes and syncs and writes[-1] < syncs[-1], 'last write synced'
            wal_writes = [i for i in writes if effects[i][2] == wal]
            wal_syncs = [i for i in syncs if effects[i][2] == wal]
            assert wal_writes and wal_syncs and wal_writes[-1] < wal_syncs[-1], \
                'last WAL write synced'
            tail = {row[1] for row in effects[syncs[-1] + 1:]}
            assert tail <= {'drop_lease', 'close'}, tail
        else:
            assert receipt['result'] == 'ready'
            assert (receipt['q_ready'], receipt['q']) == ('2', '2'), receipt
            assert receipt['points'] == points, (receipt['points'], points)
            assert receipt['rows'] == rows, (receipt['rows'], rows)
            changed = current != previous
        previous = current
    assert len(set(pids)) == len(PHASES), pids
    complete = {CASE: dict(pids=pids, points=points, rows=rows,
                           reenter_changed_files=changed, result='PASS')}
    (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
