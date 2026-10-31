#!/usr/bin/env python3
"""Run only the S9 native cases of MI03: C+A S9A-N01..03 and C+B S9B-N01.

S9A (consumers §8.2 native P1-P3 of C+A) replays S7's MCI02-N01..03 with
no B. Each case is three processes of one immutable test ELF over its own
namespace: `produce` (P1) runs the actual native writer (CREATE, A's
driver) and stops the process at its cut (after the Commit ACK, or after
the first durable CLR of a full rollback); `recover` (P2) runs C's recovery
with A's recovered CLR handler to READY, C's page reader over the READY
files, and a new A row with its Commit ACK; `reenter` (P3) runs the
recovery again (RejectInitialLosers) and the page read of both ACKs.

MCB05-N01..06 (consumers §8.3 `#acceptance`, C+B) are two processes each:
`produce` is C's actual native writer run to the no-loser cut, exiting the
process there; `reject` presents the files to RejectInitialLosers, which
refuses at Analysis with no file change.

S9B-N01 (consumers §8.3 native P1-P3 of C+B) is MCB05-L03 with no A:
`produce` is C's install, CREATE and Commit (q 1), stopped there; `recover`
is C's recovery to READY, the live pins, the author's late-bound RowReserve,
the independent literal Forward and C's Commit (q 2); `reenter` recovers
again and B reads the row by point and cursor.

The controller runs from the full tree (source identity from git) or from a
single-source export (`--root` with the export's `--manifest`, checked file
by file against the root). Snapshots are stdlib SHA-256 only; no product
decoder or output-generated oracle is used here.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

PHASES = ['produce', 'recover', 'reenter']
# Trace events that change the namespace.
MUTATIONS = {'create', 'write', 'sync', 'dirsync', 'publish', 'resize', 'remove'}
# case -> (worker, phases, produce result)
CASES = {
    'S9A-N01': ('s9a_native_worker', PHASES, 'acked'),
    'S9A-N02': ('s9a_native_worker', PHASES, 'acked'),
    'S9A-N03': ('s9a_native_worker', PHASES, 'crashed'),
    'S9B-N01': ('s9b_native_worker', PHASES, 'created'),
    **{f'MCB05-N0{n}': ('s9b_no_loser_worker', ['produce', 'reject'], 'cut')
       for n in range(1, 7)},
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
    parser.add_argument('--root', type=Path,
                        help='source root to run from (default: this checkout)')
    parser.add_argument('--manifest', type=Path,
                        help="an export's source-files.sha256 (required with a root "
                             'that is not a git checkout)')
    parser.add_argument('cases', nargs='*', help='subset of case ids (default: all)')
    args = parser.parse_args()
    root = (args.root or Path(__file__).resolve().parents[1]).resolve(strict=True)
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    selected = args.cases or list(CASES)
    assert all(case in CASES for case in selected), selected
    if args.manifest:
        # The export's manifest is its identity: every listed file has that
        # hash under the root, and the root holds no other file.
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
    listed = [line[:-6] for line in subprocess.check_output([str(elf), '--list', '--ignored'],
                                                            text=True).splitlines()]
    selectors = {}
    for worker in sorted({CASES[case][0] for case in selected}):
        names = [name for name in listed if name.endswith(f'::{worker}')]
        assert len(names) == 1, (worker, names)
        selectors[worker] = names[0]
    plan = dict(selectors=selectors, root=str(root),
                cases={c: CASES[c][1] for c in selected},
                source=source_hash,
                elf=elf_hash, input=input_hash, per_process_seconds=180,
                log_bytes_cap=16777216, rust_stack=33554432,
                expected='S9A: produce stops at its cut with the ACK (N01/N02) or CLR1 '
                         '(N03) receipt; recover is READY with the page reader '
                         'finding only committed rows and a new ACK; reenter '
                         'reads both ACKs; no B module is linked or run. '
                         'S9B: produce stops after the CREATE Commit (q 1); recover '
                         'is READY at q 1, the literal row commits at q 2; reenter '
                         'is READY at q 2 and B reads (1, 7); MCB05-N0x: produce exits '
                         'at the cut with its last write synced, reject refuses with '
                         'every file unchanged; no A module is linked or run')
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    results, complete = [], {}
    for case in selected:
        worker, phases, produced = CASES[case]
        db = out / case / 'db'
        db.mkdir(parents=True)
        pids, previous, acks = [], None, []
        for phase in phases:
            output = out / case / phase
            output.mkdir()
            env.update(XADB_S9_NATIVE_CASE=case, XADB_S9_NATIVE_PHASE=phase,
                       XADB_S9_NATIVE_DB=str(db), XADB_S9_NATIVE_OUTPUT=str(output))
            assert digest(elf.read_bytes()) == elf_hash
            start = time.monotonic()
            try:
                with (output / 'test.log').open('wb') as log:
                    code = subprocess.run([str(elf), selectors[worker], '--exact', '--ignored',
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
                assert receipt['result'] == produced
                if 'q' in receipt:
                    acks.append(receipt['q'])
                # The exact durable prefix at the cut: the in-process WAL
                # witness equals this controller's own digest of the WAL
                # file, and no write follows the last successful sync (Drop
                # only releases the lease and closes handles).
                wal = sorted(name for name in current if name.startswith('d3-wal-'))[-1]
                assert receipt['wal_cut'] == current[wal]['sha256'], 'WAL witness'
                assert int(receipt['wal_cut_bytes']) == current[wal]['bytes'], 'WAL length'
                if case == 'S9A-N03':
                    assert receipt['wal_before_clr'] != receipt['wal_cut'], 'CLR1 durable'
                effects = [line.split('\t') for line in
                           (output / 'effects.tsv').read_text().splitlines()]
                writes = [i for i, row in enumerate(effects) if row[1] == 'write']
                syncs = [i for i, row in enumerate(effects)
                         if row[1] == 'sync' and row[5] == 'Ok(())']
                assert writes and syncs and writes[-1] < syncs[-1], 'last write synced'
                tail = {row[1] for row in effects[syncs[-1] + 1:]}
                if produced == 'cut':
                    # The process exits at the cut: reads may follow the
                    # last sync, no mutation does.
                    assert not tail & MUTATIONS, tail
                else:
                    assert tail <= {'drop_lease', 'close'}, tail
            elif phase == 'reject':
                # RejectInitialLosers at Analysis: database_effect Unchanged,
                # every file byte-equal to the cut.
                assert receipt['result'] == 'refused'
                assert current == previous, 'the refusal changed a file'
            else:
                assert receipt['result'] == 'ready'
                assert current != previous, 'the process wrote nothing'
                # MCI01-V11 across processes: the READY Q is the previous
                # process's last ACKed q, after every cleanup/CLR it ran.
                assert receipt['q_ready'] == acks[-1], (receipt['q_ready'], acks)
                if phase == 'recover':
                    acks.append(receipt['ack_q'])
            previous = current
        assert len(set(pids)) == len(phases), pids
        complete[case] = dict(pids=pids, acks=acks, result='PASS')
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
