#!/usr/bin/env python3
"""Run only the live unlink native fault cuts UNL-N01..N06 (dev3-tree-smo.md#unlink-u4).

Each case is one immutable test ELF over its own namespace, one process per
phase (`unlink_native_worker`):

- produce: the actual native writer (CREATE of T, A's autocommit INSERT of
  11 or 8 of the largest rows at q 2, A's autocommit DELETEs of the leftmost
  leaf's three rows at q 3..5, each retired through C's Row-only cleanup),
  then A's cleanup drain, whose live unlink the case's cut stops. The cut is
  C's cfg(test) hook (stage_before, ram_install), an armed I/O event of the
  test adapter (wal_first_byte, wal_synced, and home_first_byte on T's
  anchor flushed after the unlink) or a worker cut after the durable unlink
  (current_published). Every produce ends by SIGABRT: no Drop, flush or
  shutdown write.
- crash (current_published only): C's recovery aborted after CURRENT (new
  base, new base) is published and its directory synced.
- recover: C's recovery to READY, the worker's tree, list, allocator,
  claim, bank and B-read checks, then an empty Commit at q 6.
- reenter: a further recovery and the same checks at q 6.

Every expectation here is this controller's: the inventory `--input` (route
key counts, level, list size, WAL bytes the cut adds), its own rule for
which cuts keep the old tree, and its own XWF1 header reader (stdlib) for
the one frame a durable unlink appends. Receipts and the effect trace are
the worker's reports, checked against these.
"""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

WORKER = 'unlink_native_worker'
SECONDS = 180
LOG_CAP = 16 * 1024 * 1024
PAGE_BYTES = 16384
HEADER = ['case', 'shape', 'cut', 'state', 'route', 'level', 'listed', 'wal_added']
CASE_IDS = [f'UNL-N0{n}-{s}' for n in range(1, 7) for s in ('L', 'R')]
CUTS = {
    # cut: (inventory number, READY tree after the unlink, produce stop)
    'stage_before': (1, False, ('hook', 'UNL_STAGE_BEFORE')),
    'ram_install': (2, False, ('hook', 'UNL_RAM_INSTALL')),
    'wal_first_byte': (3, False, ('io', 'UNL_WAL_FIRST_BYTE', 'write:d3-wal-')),
    'wal_synced': (4, True, ('io', 'UNL_WAL_SYNCED', 'sync:d3-wal-')),
    'home_first_byte': (5, True, ('io', 'UNL_HOME_FIRST_BYTE', 'write:d3-data-')),
    'current_published': (6, True, ('worker', 'UNL_AFTER_DURABLE')),
}
# Frame bytes and structural op of a durable unlink (XWF1 scope 4).
FRAME = {'leaf': (35248, 3), 'root': (19032, 4)}
ROWS = {'leaf': 11, 'root': 8}
MUTATIONS = {'create', 'write', 'sync', 'dirsync', 'publish', 'resize', 'remove'}


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def snapshot(root):
    files = {p.name: {'bytes': p.stat().st_size, 'sha256': digest(p.read_bytes())}
             for p in sorted(root.iterdir()) if p.is_file()}
    assert len(files) <= 24 and sum(f['bytes'] for f in files.values()) <= 64 * 1024 * 1024
    return files


def fields(path):
    return dict(line.split('\t', 1) for line in path.read_text().splitlines())


def trace(output):
    rows = [line.split('\t') for line in (output / 'effects.tsv').read_text().splitlines()]
    assert all(len(row) == 6 for row in rows), 'trace shape'
    return rows


def expected_of(row):
    """The controller's own expectation of one inventory row."""
    shape, cut = row['shape'], row['cut']
    number, after, stop = CUTS[cut]
    assert row['case'] == f'UNL-N0{number}-{shape[0].upper()}', row
    assert row['state'] == ('after' if after else 'before'), row
    # The leftmost leaf of [3, 3, 5] or [3, 5] is emptied before the cut and
    # stays routed until the unlink.
    route = ([3, 5] if shape == 'leaf' else [5]) if after else \
        ([0, 3, 5] if shape == 'leaf' else [0, 5])
    level = 0 if after and shape == 'root' else 1
    listed = (2 if shape == 'root' else 1) if after else 0
    frame, op = FRAME[shape]
    added = 0 if cut in ('stage_before', 'ram_install') else 1 if cut == 'wal_first_byte' else frame
    assert row['route'] == ','.join(map(str, route)), row
    assert (int(row['level']), int(row['listed']), int(row['wal_added'])) == (level, listed, added), row
    phases = ['produce', 'crash', 'recover', 'reenter'] if cut == 'current_published' else \
        ['produce', 'recover', 'reenter']
    return dict(shape=shape, cut=cut, after=after, stop=stop, route=row['route'], level=level,
                listed=listed, added=added, frame=frame, op=op, rows=ROWS[shape] - 3,
                phases=phases)


def check_produce(want, receipt, effects, db):
    """The cut and the WAL it left, from the trace and this reader."""
    assert receipt['shape'] == want['shape'] and receipt['cut_at'] == want['cut']
    assert int(receipt['q']) == 5
    kind = want['stop'][0]
    label = want['stop'][1]
    arms = [i for i, row in enumerate(effects) if row[1] == 'arm']
    if kind == 'io':
        assert receipt['armed'] == label and len(arms) == 1 and effects[arms[0]][2] == label
        last = effects[-1]
        assert last[1] == 'cut' and last[2] == label and last[5].startswith(want['stop'][2]), last
        if want['cut'] == 'home_first_byte':
            # T's anchor: its DWB image synced after the arm, then the first
            # byte of its home write at the anchor's page.
            after = effects[arms[0]:]
            assert any(row[1] == 'sync' and row[2] == 'd3-dwb-images' and row[5] == 'Ok(())'
                       for row in after)
            write = effects[-2]
            assert write[1] == 'write' and write[5] == 'FIRST_BYTE:Ok(1)', write
            assert int(write[3]) == int(receipt['flushed_page']) * PAGE_BYTES == \
                int(receipt['anchor_page']) * PAGE_BYTES, write
        elif want['cut'] == 'wal_first_byte':
            assert effects[-2][1] == 'write' and effects[-2][5] == 'FIRST_BYTE:Ok(1)', effects[-2]
    elif kind == 'hook':
        # C's hook aborts with no trace row: nothing of the group reached
        # the WAL after the arm row.
        assert receipt['armed'] == label and len(arms) == 1
        assert effects[arms[0]][2] == label and effects[arms[0]][5] == 'HOOK'
        assert not any(row[1] in MUTATIONS and row[2].startswith('d3-wal-')
                       for row in effects[arms[0]:]), 'a WAL mutation after the hook arm'
        assert 'cut' not in receipt
    else:
        assert receipt['cut'] == label and not arms and 'armed' not in receipt
        # After the unlink's WAL sync: no mutation (no Drop either).
        syncs = [i for i, row in enumerate(effects) if row[1] == 'sync' and row[5] == 'Ok(())']
        assert effects[syncs[-1]][2].startswith('d3-wal-')
        assert not {row[1] for row in effects[syncs[-1] + 1:]} & MUTATIONS
    wal_name = sorted(p.name for p in db.iterdir() if p.name.startswith('d3-wal-'))[-1]
    wal = (db / wal_name).read_bytes()
    start = int(receipt['wal_bytes'])
    assert digest(wal[:start]) == receipt['wal_sha256'], 'the WAL before the drain is a prefix'
    assert len(wal) - start == want['added'], (len(wal) - start, want['added'])
    if want['added'] == 1:
        assert wal[start:] == b'X'
    elif want['added']:
        frame = wal[start:]
        length = int.from_bytes(frame[12:16], 'little')
        lsn = int.from_bytes(frame[56:64], 'little')
        op = int.from_bytes(frame[166:168], 'little')
        scope = int.from_bytes(frame[170:172], 'little')
        assert frame[:4] == b'XWF1' and length == want['frame'], (frame[:4], length)
        assert (lsn, op, scope) == (int(receipt['lsn']) + 1, want['op'], 4), (lsn, op, scope)
    return dict(wal=wal_name, start=start, added=len(wal) - start)


def check_crash(receipt, effects):
    last = effects[-1]
    assert receipt['armed'] == 'UNL_CURRENT_NEW_NEW'
    assert last[1] == 'cut' and last[2] == 'UNL_CURRENT_NEW_NEW' and last[5] == 'dirsync:namespace'
    done = [row for row in effects[:-1] if row[5] != 'ATTEMPT' and row[1] in MUTATIONS]
    assert done[-1][1] == 'dirsync' and done[-2][1] == 'publish', done[-2:]
    published = sum(1 for row in done if row[1] == 'publish' and row[2] == 'd3-current-stage')
    assert published == 3, published
    assert sum(1 for row in done if row[1] == 'sync' and row[2].startswith('d3-rebase-')) == 1
    assert not any(row[1] == 'remove' for row in done), 'the old WAL is still named'


def check_ready(want, phase, receipt, produced):
    assert receipt['result'] == 'ready' and receipt['handles'] == '0'
    assert int(receipt['q_ready']) == (5 if phase == 'recover' else 6)
    assert receipt['route'] == want['route'] and int(receipt['level']) == want['level']
    listed = [int(page) for page in receipt['listed'].split(',') if page]
    if want['after']:
        pages = [int(produced['emptied'])]
        if want['shape'] == 'root':
            pages = sorted(pages + [int(produced['root_page'])])
        assert listed == pages, (listed, pages)
        assert int(receipt['high_water']) == int(produced['lsn']) + 1
    else:
        assert listed == []
        assert receipt['high_water'] == produced['high_water']
    assert int(receipt['rows']) == want['rows']
    if phase == 'recover':
        assert receipt['ack_q'] == '6'
    else:
        assert 'ack_q' not in receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--elf', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--input', required=True, type=Path,
                        help='frozen UNL-N inventory whose hash is the run input')
    parser.add_argument('cases', nargs='*', help='subset of case ids (default: all)')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    elf = args.elf.resolve(strict=True)
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw_input = args.input.resolve(strict=True).read_bytes()
    lines = raw_input.decode().splitlines()
    assert lines[0].split('\t') == HEADER, lines[0]
    inventory = {row['case']: row for row in csv.DictReader(lines, delimiter='\t')}
    assert list(inventory) == CASE_IDS, list(inventory)
    wants = {case: expected_of(row) for case, row in inventory.items()}
    selected = args.cases or CASE_IDS
    assert len(set(selected)) == len(selected) and all(c in CASE_IDS for c in selected), selected
    files = sorted(set(subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=root
    ).decode().split('\0')) - {''})
    manifest = ''.join(f'{digest((root / p).read_bytes())}  {p}\n'
                       for p in files if (root / p).is_file())
    (out / 'source-files.sha256').write_text(manifest)
    source_hash, elf_hash, input_hash = digest(manifest.encode()), digest(elf.read_bytes()), \
        digest(raw_input)
    names = [line[:-6] for line in subprocess.check_output([str(elf), '--list', '--ignored'],
                                                           text=True).splitlines()
             if line.endswith(f'::{WORKER}: test')]
    assert len(names) == 1, names
    plan = dict(selector=names[0], cases={c: wants[c]['phases'] for c in selected},
                source=source_hash, elf=elf_hash, input=input_hash,
                controller=digest(Path(__file__).read_bytes()),
                per_process_seconds=SECONDS, log_bytes_cap=LOG_CAP, rust_stack=33554432,
                expected={c: {k: wants[c][k] for k in ('after', 'route', 'level', 'listed',
                                                       'added', 'rows')} for c in selected})
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    results, complete = [], {}
    for case in selected:
        want = wants[case]
        db = out / case / 'db'
        db.mkdir(parents=True)
        pids, previous, facts, produced = [], None, {}, None
        for phase in want['phases']:
            output = out / case / phase
            output.mkdir()
            env.update(XADB_UNL_NATIVE_CASE=case, XADB_UNL_NATIVE_PHASE=phase,
                       XADB_UNL_NATIVE_DB=str(db), XADB_UNL_NATIVE_OUTPUT=str(output))
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
            results.append(dict(case=case, phase=phase, exit=code, seconds=round(seconds, 2)))
            (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
            assert (output / 'test.log').stat().st_size <= LOG_CAP
            print(case, phase, code, round(seconds, 2), flush=True)
            cut_phase = phase in ('produce', 'crash')
            if code != (-signal.SIGABRT if cut_phase else 0):
                raise SystemExit(f'{case} {phase}: exit {code}')
            receipt = fields(output / 'receipt.tsv')
            assert receipt['source'] == source_hash and receipt['elf'] == elf_hash
            assert receipt['input'] == input_hash
            assert receipt['case'] == case and receipt['phase'] == phase
            pids.append(receipt['pid'])
            effects = trace(output)
            current = snapshot(db)
            (output / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
            if phase == 'produce':
                facts[phase] = check_produce(want, receipt, effects, db)
                produced = receipt
            elif phase == 'crash':
                check_crash(receipt, effects)
                assert current != previous, 'the crash wrote nothing'
            else:
                check_ready(want, phase, receipt, produced)
                assert current != previous, 'the process wrote nothing'
                facts[phase] = dict(log_bytes=int(receipt['log_bytes']))
            previous = current
        assert len(set(pids)) == len(want['phases']), pids
        complete[case] = dict(result='PASS', pids=pids, after=want['after'], facts=facts)
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
