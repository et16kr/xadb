#!/usr/bin/env python3
"""Run only the Row UPDATE native fault cuts RUP-N01..06.

Each case is three processes of one immutable test ELF over its own
namespace (dev3-row-update.md#implementation-4): `produce` runs the actual
native writer (CREATE, A's autocommit INSERT, then the case's UPDATE steps)
and stops the process at its cut, before or after an UpdateForward,
UpdateClr or UpdatePrune group; `recover` runs C's recovery with A's
recovered CLR handler to READY, B's cursor read, and a new A row with its
Commit ACK; `reenter` runs the C+B recovery and B's cursor read of both
ACKs. Every expected value comes from the frozen `--input` inventory, which
is written before the run. Snapshots are stdlib SHA-256 only; no product
decoder or output-generated oracle is used here. The checks are the Row
DELETE controller's (`check_dev3_row_delete_native.py`).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

PHASES = ['produce', 'recover', 'reenter']
CASE_IDS = tuple(f'RUP-N0{n}' for n in range(1, 7))
# Each "after" cut runs its "before" partner's history and then the group:
# the partner's WAL file at its cut is a byte prefix of this one's, and the
# group is the one WAL write after it. A selected "after" case runs its
# partner first.
AFTER = {'RUP-N02': 'RUP-N01', 'RUP-N04': 'RUP-N03', 'RUP-N06': 'RUP-N05'}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(root):
    files = {p.name: {"bytes": p.stat().st_size, "sha256": digest(p.read_bytes())}
             for p in sorted(root.iterdir()) if p.is_file()}
    assert len(files) <= 24 and sum(f["bytes"] for f in files.values()) <= 48 * 1024 * 1024
    return files


def ints(text):
    return [int(value) for value in text.split(',')] if text else []


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--input', required=True, type=Path,
                        help='frozen RUP-N inventory whose hash is the run input')
    parser.add_argument('cases', nargs='*', help='subset of case ids (default: all)')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw_input = args.input.resolve(strict=True).read_bytes()
    inventory = json.loads(raw_input)
    assert inventory['schema'] == 'xadb.row-update-native-inventory.v1'
    assert tuple(inventory['cases']) == CASE_IDS, list(inventory['cases'])
    selected = args.cases or list(CASE_IDS)
    assert all(case in CASE_IDS for case in selected), selected
    selected = [case for case in CASE_IDS
                if case in selected or any(AFTER.get(s) == case for s in selected)]
    files = sorted(set(subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root
    ).decode().split('\0')) - {''})
    manifest = ''.join(f'{digest((root / p).read_bytes())}  {p}\n'
                       for p in files if (root / p).is_file())
    (out / 'source-files.sha256').write_text(manifest)
    source_hash, elf_hash = digest(manifest.encode()), digest(elf.read_bytes())
    input_hash = digest(raw_input)
    names = [line[:-6] for line in subprocess.check_output([str(elf), '--list', '--ignored'],
                                                           text=True).splitlines()
             if line.endswith('::rup_native_worker: test')]
    assert len(names) == 1, names
    assert names[0].endswith(inventory['worker']), names
    plan = dict(selector=names[0], cases={c: PHASES for c in selected}, source=source_hash,
                elf=elf_hash, input=input_hash, per_process_seconds=180,
                log_bytes_cap=16777216, rust_stack=33554432,
                expected='produce stops at its cut with the inventory result and q; '
                         'the last step changed the WAL and every write before the cut '
                         'is synced; recover is READY at the last ACKed q with the '
                         "inventory's CLRs and committed values, then a new ACK; "
                         'reenter reads both ACKs')
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    new = inventory['new_value']
    results, complete, cuts = [], {}, {}
    for case in selected:
        expected = inventory['cases'][case]
        db = out / case / 'db'
        db.mkdir(parents=True)
        pids, previous, acks = [], None, []
        for phase in PHASES:
            output = out / case / phase
            output.mkdir()
            env.update(XADB_RUP_NATIVE_CASE=case, XADB_RUP_NATIVE_PHASE=phase,
                       XADB_RUP_NATIVE_DB=str(db), XADB_RUP_NATIVE_OUTPUT=str(output))
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
            want = expected[phase]
            if phase == 'produce':
                assert receipt['result'] == want['result'], (receipt['result'], want)
                assert int(receipt['q']) == want['q'], (receipt['q'], want)
                assert ints(receipt['rows']) == want['rows'], (receipt['rows'], want)
                acks.append(int(receipt['q']))
                # The exact durable prefix at the cut: the in-process WAL
                # witness equals this controller's own digest of the WAL
                # file, the last step changed it, and no write follows the
                # last successful sync (Drop only releases the lease and
                # closes handles).
                wal = sorted(name for name in current if name.startswith('d3-wal-'))[-1]
                assert receipt['wal_cut'] == current[wal]['sha256'], 'WAL witness'
                assert int(receipt['wal_cut_bytes']) == current[wal]['bytes'], 'WAL length'
                assert receipt['wal_before'] != receipt['wal_cut'], 'the last step is in the WAL'
                effects = [line.split('\t') for line in
                           (output / 'effects.tsv').read_text().splitlines()]
                # File by file, a successful sync of that file follows its
                # last completed write or resize.
                written, synced = {}, {}
                for i, row in enumerate(effects):
                    if row[1] in ('write', 'resize') and row[5] != 'ATTEMPT':
                        written[row[2]] = i
                    if row[1] == 'sync' and row[5] == 'Ok(())':
                        synced[row[2]] = i
                assert wal in written, 'the WAL was written'
                unsynced = {f: i for f, i in written.items() if synced.get(f, -1) < i}
                assert not unsynced, ('unsynced writes', unsynced)
                tail = {row[1] for row in effects[max(synced.values()) + 1:]}
                assert tail <= {'drop_lease', 'close'}, tail
                raw = (db / wal).read_bytes()
                cuts[case] = (wal, len(raw), digest(raw))
                partner = AFTER.get(case)
                if partner:
                    name, size, sha = cuts[partner]
                    assert name == wal and size < len(raw), (cuts[partner], wal, len(raw))
                    assert digest(raw[:size]) == sha, f'{partner} WAL is a prefix of {case}'
                    later = [(int(row[3]), int(row[4])) for row in effects
                             if row[1] == 'write' and row[2] == wal and row[5] != 'ATTEMPT'
                             and int(row[3]) + int(row[4]) > size]
                    assert later == [(size, len(raw) - size)], ('one group after', later)
            else:
                assert receipt['result'] == 'ready'
                assert current != previous, 'the process wrote nothing'
                # MCI01-V11 across processes: the READY Q is the previous
                # process's last ACKed q, after every cleanup/CLR it ran.
                assert int(receipt['q_ready']) == acks[-1] == want['q_ready'], (
                    receipt['q_ready'], acks, want)
                assert int(receipt['clrs']) == want['clrs'], (receipt['clrs'], want)
                assert ints(receipt['values']) == want['values'], (receipt['values'], want)
                if phase == 'recover':
                    assert int(receipt['ack_q']) == want['ack_q'], (receipt['ack_q'], want)
                    acks.append(int(receipt['ack_q']))
                else:
                    assert 'ack_q' not in receipt and want['values'][-1] == new
            previous = current
        assert len(set(pids)) == len(PHASES), pids
        complete[case] = dict(pids=pids, acks=acks, result='PASS')
        if case in AFTER:
            complete[case]['wal_prefix_of_before_cut'] = AFTER[case]
            complete[case]['wal_group_bytes'] = cuts[case][1] - cuts[AFTER[case]][1]
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
