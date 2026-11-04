#!/usr/bin/env python3
"""Run selected frozen SMO E10 histories in three separate processes with explicit source/ELF identities.

The input fixes Root/Child and five cuts each. A failed recovery may reuse its
unchanged producer input under a corrected ELF; identities remain separate. Expectations come from its
partitions, new-page counts and the literal rows 1..4 / values 7..10. Each
produce retains independent before images; recovery compares allocator and
tree state against those images, commits an empty transaction, and reentry
checks that ACK. No B cursor or new multi-level Row CRUD is claimed.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(directory):
    result = {p.name: {'bytes': p.stat().st_size, 'sha256': digest(p.read_bytes())}
              for p in sorted(directory.iterdir()) if p.is_file()}
    assert len(result) <= 24 and sum(v['bytes'] for v in result.values()) <= 48 * 1024 * 1024
    return result


def fields(path):
    return dict(line.split('\t', 1) for line in path.read_text().splitlines())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--source-manifest', type=Path, required=True)
    parser.add_argument('--resume-produced', type=Path)
    parser.add_argument('cases', nargs='+')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw_input = args.input.read_bytes()
    inventory = {r['case']: r for r in csv.DictReader(raw_input.decode().splitlines(), delimiter='\t')}
    assert list(inventory) == [f'SMO-N{s}{i}' for s in ('R', 'C') for i in range(1, 6)]
    assert len(set(args.cases)) == len(args.cases) and all(c in inventory for c in args.cases)
    # Preserve the build's identity when later independent tests or controller
    # fixes change the working tree. Controller identity is recorded separately.
    manifest = args.source_manifest.read_text()
    (out / 'source-files.sha256').write_text(manifest)
    source, elf_hash, input_hash = digest(manifest.encode()), digest(elf.read_bytes()), digest(raw_input)
    names = [line[:-6] for line in subprocess.check_output([str(elf), '--list', '--ignored'], text=True).splitlines()
             if line.endswith('::structural_effect_native_worker: test')]
    assert len(names) == 1, names
    plan = dict(worker=names[0], cases={c: inventory[c] for c in args.cases},
                phases=['produce', 'recover', 'reenter'], source=source, elf=elf_hash,
                input=input_hash, controller=digest(Path(__file__).read_bytes()),
                resumed_from=str(args.resume_produced) if args.resume_produced else None,
                per_process_seconds=180, log_bytes_cap=16777216,
                expected_rows=[1, 2, 3, 4], expected_values=[7, 8, 9, 10], q_ready=[2, 3])
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    results, complete = [], {}
    for case in args.cases:
        expected = inventory[case]
        db = out / case / 'db'
        if args.resume_produced:
            prior = args.resume_produced / case
            prior_plan = json.loads((args.resume_produced / 'execution-plan.json').read_text())
            assert prior_plan['input'] == plan['input']
            prior_results = json.loads((args.resume_produced / 'results.json').read_text())
            prior_case = [r for r in prior_results if r['case'] == case]
            assert prior_case[0]['phase'] == 'produce' and prior_case[0]['exit'] == 0
            assert len(prior_case) == 1 or (len(prior_case) == 2 and
                                          prior_case[1]['phase'] == 'recover' and prior_case[1]['exit'] != 0)
            original_db = prior / ('recover/input-db' if len(prior_case) == 2 else 'db')
            assert snapshot(original_db) == json.loads((prior / 'produce/files.json').read_text())
            shutil.copytree(original_db, db)
            shutil.copytree(prior / 'produce', out / case / 'produce')
        else:
            db.mkdir(parents=True)
        pids, previous, ready_arena = [], None, None
        for phase in plan['phases']:
            output = out / case / phase
            reused = bool(args.resume_produced and phase == 'produce')
            output.mkdir(exist_ok=reused)
            # Preserve the exact pre-phase input for a failed process; a later
            # correction need not rerun already completed producer histories.
            if not reused:
                shutil.copytree(db, output / 'input-db')
            env.update(XADB_SMO_NATIVE_CASE=case, XADB_SMO_NATIVE_PHASE=phase,
                       XADB_SMO_NATIVE_DB=str(db), XADB_SMO_NATIVE_OUTPUT=str(output))
            assert digest(elf.read_bytes()) == elf_hash
            start = time.monotonic()
            try:
                if reused:
                    results.append(dict(case=case, phase=phase, exit=0, reused_from=str(args.resume_produced)))
                    code = 0
                else:
                    with (output / 'test.log').open('wb') as log:
                        code = subprocess.run([str(elf), names[0], '--exact', '--ignored', '--nocapture', '--test-threads=1'],
                                              cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=180).returncode
            except subprocess.TimeoutExpired:
                code = 124
            if not reused:
                results.append(dict(case=case, phase=phase, exit=code, seconds=round(time.monotonic() - start, 2)))
            (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
            print(results[-1], flush=True)
            assert (output / 'test.log').stat().st_size <= plan['log_bytes_cap']
            if code:
                raise SystemExit(code)
            receipt = fields(output / 'receipt.tsv')
            assert receipt['case'] == case and receipt['phase'] == phase
            identity = prior_plan if reused else plan
            assert receipt['source'] == identity['source'] and receipt['elf'] == identity['elf']
            assert receipt['input'] == input_hash
            assert receipt['child'] == expected['child'] and receipt['cut'] == expected['cut']
            pids.append(receipt['pid'])
            current = snapshot(db)
            (output / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
            before = fields(out / case / 'produce/before.tsv')
            if phase == 'produce':
                assert receipt['result'] == 'cut' and receipt['q'] == '2'
                wal = sorted(n for n in current if n.startswith('d3-wal-'))[-1]
                assert receipt['wal_sha256'] == current[wal]['sha256']
                assert int(receipt['wal_bytes']) == current[wal]['bytes']
                split = int(expected['new_pages']) != 0
                assert (int(receipt['wal_bytes']) > int(before['wal_bytes'])) == split
                assert (receipt['wal_sha256'] != before['wal_sha256']) == split
                effects = [r.split('\t') for r in (output / 'effects.tsv').read_text().splitlines()]
                written, synced = {}, {}
                for i, row in enumerate(effects):
                    if row[1] in ('write', 'resize') and row[5] != 'ATTEMPT':
                        written[row[2]] = i
                    if row[1] == 'sync' and row[5] == 'Ok(())':
                        synced[row[2]] = i
                assert not {f: i for f, i in written.items() if synced.get(f, -1) < i}
                assert {row[1] for row in effects[max(synced.values()) + 1:]} <= {'drop_lease', 'close', 'read', 'metadata'}
            else:
                assert receipt['result'] == 'ready' and receipt['handles'] == '0'
                assert receipt['partitions'] == expected['partitions']
                assert receipt['rows'] == '1,2,3,4' and receipt['values'] == '7,8,9,10'
                assert int(receipt['q_ready']) == (2 if phase == 'recover' else 3)
                high = int(before['lsn']) + 1 if int(expected['new_pages']) else int(before['high_water'])
                assert int(receipt['high_water']) == high
                if phase == 'recover':
                    assert receipt['ack_q'] == '3'
                    ready_arena = (out / case / 'recover-arena.bin').read_bytes()
                else:
                    assert 'ack_q' not in receipt
                    # Row tree pages themselves were checked against the fixed
                    # literals and original before image in both processes.
                    assert ready_arena is not None
                assert current != previous, 'restart made no durable progress'
            previous = current
        assert len(set(pids)) == 3
        complete[case] = dict(result='PASS', pids=pids, expected=expected,
                             producer_identity={k: (prior_plan if args.resume_produced else plan)[k]
                                                for k in ('source', 'elf')},
                             recovery_identity={k: plan[k] for k in ('source', 'elf')})
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
