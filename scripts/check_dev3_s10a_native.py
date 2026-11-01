#!/usr/bin/env python3
"""Run the S10 C+A native cases of MI03: MCA03-N01..N09 (16 cuts),
MCA03-R01..R12 and MCA04-N01 (consumers §9.1, cuts of §9.4).

Every case is one immutable test ELF over its own namespace, one process per
phase (`s10a_native_worker`, boundaries `#row-mi03-s10`):

- produce (P1): the actual native writer (CREATE of T, A's driver) stopped at
  the case's cut. The cut is an armed I/O event of the test adapter (before
  or after one syscall, a first-byte write, an injected write or sync error)
  or C's cfg(test) hook; the process aborts there (SIGABRT), with no Drop,
  flush or further namespace syscall. A worker-side cut (after an ACK, an
  UNKNOWN Commit or the R input) aborts the same way.
- crash (R01..R12): C's recovery aborted at one REBASE/epoch/UNDO barrier
  (R12: after READY and A's new ACK).
- recover: C's recovery (WithRowClr) to READY, the page read, A's new row
  and its ACK.
- again (MCA04-N01): a further reopen, then A's next row and its ACK.
- reenter: a further recovery (RejectInitialLosers) and page read.

The expectations here are this controller's own: the rows, CLR counts, READY
Q and RowId non-reuse are fixed by each history below; the frames at a cut
and the winner/loser of an UNKNOWN Commit come from this file's WAL frame
reader (XWF1 header fields only, stdlib), never from XADB output. The
controller runs from the full tree (source identity from git) or from a
single-source export (`--root` with the export's `--manifest`, checked file
by file), as check_dev3_s9_native.py does.

Each case's `input-plan.json` holds its `fault.preservation` (consumers
§9.4): the mode and the frame F the cut stops (op and length, this history's
own frame sizes), and for a home-page cut the page whose flushed image is
the partial home. After the produce phase the controller's witness
(`witness.json`) records D (the synced WAL prefix before F: end, last
frame LSN, SHA-256 of the bytes) and F (offset, length, LSN, digest, and
whether its bytes are absent, one byte, present or synced) from the trace
and the WAL file, checks every successful WAL write's bytes against the
trace digest, and for a home cut reads every data page's LSN: exactly the
flushed page carries F's LSN and no page is newer. After the recover phase
(R12: its crash phase) it copies the newest WAL into the phase output and
counts the Row CLR and Cancel frames of that epoch with its own reader.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time

WORKER = 's10a_native_worker'
SECONDS = 180
VALUE, NEW = 7, 17
# Scope3 (Row) group ops of the XWF1 header.
BEGIN, FORWARD, ROLLBACK_START, CLR, COMMIT, ABORT_END, RESERVE = 1, 2, 3, 4, 6, 7, 11
CANCEL = 12
# This history's Row frame lengths (one Int64 row of T; XWF1 header length).
# A case's `keep` may name another length: N02's F is A2's second Forward,
# which also moves the head H (91625); N09-V03's is the CLR of A2's first row.
FRAME_BYTES = {RESERVE: 57336, FORWARD: 91497, CLR: 91417, COMMIT: 56712}
PAGE = 16384
PRUNE, RELEASE_ROLE, ROOT_FREE, TERMINAL_FINISH = 20, 21, 22, 17
MUTATIONS = {'create', 'write', 'sync', 'dirsync', 'publish', 'resize', 'remove'}
N3 = ['produce', 'recover', 'reenter']
R4 = ['produce', 'crash', 'recover', 'reenter']


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def wal_frames(wal):
    """(offset, bytes, LSN, group op, scope) of each complete XWF1 frame, and
    the bytes after the last complete frame."""
    at, frames = wal.index(b'XWF1'), []
    while at + 192 <= len(wal) and wal[at:at + 4] == b'XWF1':
        length = int.from_bytes(wal[at + 12:at + 16], 'little')
        if at + length > len(wal):
            break
        frames.append((at, length, int.from_bytes(wal[at + 56:at + 64], 'little'),
                       int.from_bytes(wal[at + 166:at + 168], 'little'),
                       int.from_bytes(wal[at + 170:at + 172], 'little')))
        at += length
    return frames, len(wal) - at


def row_ops(frames):
    return [op for _, _, _, op, scope in frames if scope == 3]


# case -> (phases, cut at produce, expected state). `cut` is the trace's
# `cut` row (label, operation:name prefix) or the receipt's worker cut.
# `ram`: the ops the WAL's Row frames end with at the cut (complete frames);
# `tail`: bytes after the last complete frame. `rows`: the READY rows after
# the recover phase as (RowId, value); `clrs`: A's recovered CLRs there;
# `winner`: the unacknowledged A2 Commit is a winner (q moves by one);
# `ack_row`: the new row's RowId (an int, or ('>=', n)).
CASES = {
    'MCA03-N01': dict(phases=N3, worker_cut='N01_AFTER_ACK', ends=[FORWARD, COMMIT],
                      rows=[(1, 7), (2, 8)], clrs=0, ack_row=3),
    'MCA03-N02': dict(phases=N3, cut=('N02_ACTIVE_LEAF_HOME_SYNCED', 'sync:d3-data-'),
                      ends=[RESERVE, FORWARD], home_after_wal=True,
                      keep=('Durable', FORWARD, 91625), home=13,
                      rows=[(1, 7)], clrs=2, ack_row=4),
    'MCA03-N03-RamOnly': dict(phases=N3, cut=('N03_RESERVE_RAM_ONLY', 'write:d3-wal-'),
                              ends=[COMMIT, BEGIN], keep=('RamOnly', RESERVE),
                              rows=[(1, 7)], clrs=0, ack_row=('>=', 2)),
    'MCA03-N03-Durable': dict(phases=N3, cut=('N03_RESERVE_DURABLE', 'sync:d3-wal-'),
                              ends=[BEGIN, RESERVE], keep=('Durable', RESERVE),
                              cancels=1, rows=[(1, 7)], clrs=0, ack_row=3),
    'MCA03-N04': dict(phases=N3, cut=('N04_FORWARD_FIRST_BYTE', 'write:d3-wal-'),
                      ends=[BEGIN, RESERVE], tail=1, keep=('FirstByte', FORWARD),
                      cancels=1, rows=[(1, 7)], clrs=0, ack_row=3),
    'MCA03-N05-RamOnly': dict(phases=N3, hook='N05_FORWARD_FIRST_RAM_PAGE',
                              ends=[BEGIN, RESERVE], keep=('Hook', FORWARD),
                              cancels=1, rows=[(1, 7)], clrs=0, ack_row=3),
    'MCA03-N05-Durable': dict(phases=N3, cut=('N05_FIRST_FORWARD_HOME_SYNCED',
                                              'sync:d3-data-'),
                              ends=[RESERVE, FORWARD], home_after_wal=True,
                              keep=('Durable', FORWARD), home=None,
                              rows=[(1, 7)], clrs=1, ack_row=3),
    'MCA03-N06': dict(phases=N3, worker_cut='COMMIT_UNKNOWN', fault='WRITE_ERROR',
                      ends=[RESERVE, FORWARD], unknown=True, keep=('WriteError', COMMIT),
                      rows=[(1, 7)], clrs=1, ack_row=3),
    'MCA03-N07': dict(phases=N3, worker_cut='COMMIT_UNKNOWN', fault='SYNC_ERROR',
                      ends=[FORWARD, COMMIT], unknown=True, keep=('SyncError', COMMIT),
                      rows=[(1, 7), (2, 8)], clrs=0, ack_row=3),
    'MCA03-N08': dict(phases=N3, hook='N08_COMMIT_BEFORE_PUBLISH', ends=[FORWARD, COMMIT],
                      unknown=True, keep=('Durable', COMMIT), rows=[(1, 7), (2, 8)],
                      clrs=0, ack_row=3),
    **{f'MCA03-N09-V0{k}-{mode}': dict(
        phases=N3,
        cut=('N09_CLR_RAM_ONLY' if mode == 'RamOnly' else 'N09_CLR_DURABLE',
             'write:d3-wal-' if mode == 'RamOnly' else 'sync:d3-wal-'),
        clr_frames=k - 1 if mode == 'RamOnly' else k,
        # The k-th CLR compensates A2's (4-k)-th row; the first row's CLR
        # has no predecessor head H to restore (91289).
        keep=(mode, CLR, 91289 if k == 3 else 91417),
        rows=[(4, 10)], clrs=4 - k if mode == 'RamOnly' else 3 - k, ack_row=5)
       for k in (1, 2, 3) for mode in ('RamOnly', 'Durable')},
    **{f'MCA03-R{n:02}': dict(phases=R4 if n < 12 else ['produce', 'crash', 'reenter'],
                              worker_cut='R_INPUT', ends=[RESERVE, FORWARD],
                              rows=[(1, 7)], clrs=0 if n == 11 else 2, ack_row=4)
       for n in range(1, 13)},
    'MCA04-N01': dict(phases=['produce', 'recover', 'again', 'reenter'],
                      worker_cut='N01_AFTER_ACK', ends=[FORWARD, COMMIT],
                      rows=[(1, 7), (2, 8)], clrs=0, ack_row=3),
}
# R01..R11: the crash phase's barrier (label, operation:name prefix of the
# cut row, the mutations that must precede the cut in the crash trace).
BARRIERS = {
    'MCA03-R01': ('R01_BASE_WRITTEN', 'sync:d3-rebase-', [('write', 'd3-rebase-')]),
    'MCA03-R02': ('R02_BASE_READBACK', 'create:d3-current-stage',
                  [('sync', 'd3-rebase-'), ('dirsync', 'namespace')]),
    'MCA03-R03': ('R03_CURRENT_NEW_OLD', 'dirsync:namespace',
                  [('publish', 'd3-current-stage')]),
    'MCA03-R04': ('R04_CURRENT_NEW_NEW', 'dirsync:namespace',
                  [('publish', 'd3-current-stage')]),
    'MCA03-R05': ('R05_SECOND_COPY_CLOSURE', 'remove:d3-wal-',
                  [('publish', 'd3-current-stage'), ('dirsync', 'namespace')]),
    'MCA03-R06': ('R06_OLD_WAL_UNLINKED', 'remove:d3-wal-', []),
    'MCA03-R07': ('R07_OLD_WAL_DIRSYNC', 'dirsync:namespace', [('remove', 'd3-wal-')]),
    'MCA03-R08': ('R08_EPOCH_INTENT', 'create:d3-wal-',
                  [('publish', 'd3-current-stage'), ('dirsync', 'namespace')]),
    'MCA03-R09': ('R09_EPOCH_HEADER', 'publish:d3-writer-stage',
                  [('sync', 'd3-writer-stage')]),
    'MCA03-R10': ('R10_WAL_OPEN', 'write:d3-wal-',
                  [('publish', 'd3-current-stage'), ('dirsync', 'namespace')]),
    'MCA03-R11': ('R11_UNDO_COMPLETE', 'sync:d3-wal-', [('write', 'd3-wal-')]),
}
AFTER = {'MCA03-R03', 'MCA03-R04', 'MCA03-R06', 'MCA03-R07', 'MCA03-R11'}
# The publications of CURRENT before each barrier in one recovery: seal (1),
# new/old (2), new/new (3), second copy (4), retired source (5), epoch
# intent (6), WalOpen (7).
CURRENT_BEFORE = {'MCA03-R01': 1, 'MCA03-R02': 1, 'MCA03-R03': 2, 'MCA03-R04': 3,
                  'MCA03-R05': 4, 'MCA03-R06': 4, 'MCA03-R07': 4, 'MCA03-R08': 6,
                  'MCA03-R09': 6, 'MCA03-R10': 7, 'MCA03-R11': 7}


def snapshot(root):
    files = {p.name: {"bytes": p.stat().st_size, "sha256": digest(p.read_bytes())}
             for p in sorted(root.iterdir()) if p.is_file()}
    assert len(files) <= 24 and sum(f["bytes"] for f in files.values()) <= 64 * 1024 * 1024
    return files


def trace(output):
    rows = [line.split('\t') for line in (output / 'effects.tsv').read_text().splitlines()]
    assert all(len(row) == 6 for row in rows), 'trace shape'
    return rows


def receipt_of(output):
    lines = [line.split('\t', 1) for line in (output / 'receipt.tsv').read_text().splitlines()]
    return dict(lines)


def rows_of(text):
    return [tuple(int(x) for x in item.split(':')) for item in text.split(',') if item]


def check_produce(case, spec, receipt, effects, db):
    """The cut and the durable prefix it left, from the trace and the WAL."""
    done = [row for row in effects if row[5] != 'ATTEMPT']
    if 'cut' in spec:
        label, at = spec['cut']
        last = effects[-1]
        assert last[1] == 'cut' and last[2] == label, last
        assert last[5].startswith(at), (last, at)
    elif 'hook' in spec:
        # C's hook aborts with no trace row: the arm row is the case's, and
        # the last mutation is a successful one before the hook's group.
        assert any(row[1] == 'arm' and row[2] == spec['hook'] for row in effects)
        assert receipt.get('armed') == spec['hook']
        assert 'cut' not in receipt
    else:
        assert receipt.get('cut') == spec['worker_cut'], receipt
        if 'fault' in spec:
            faults = [row for row in effects if row[1] == 'fault']
            assert len(faults) == 1 and faults[0][5].startswith(spec['fault']), faults
            assert effects[-1] == faults[0], 'nothing after the injected fault'
            assert receipt['commit'].startswith('unknown'), 'Commit outcome UNKNOWN'
        else:
            # After the last successful sync: no mutation (no Drop either).
            syncs = [i for i, row in enumerate(effects)
                     if row[1] == 'sync' and row[5] == 'Ok(())']
            assert not {row[1] for row in effects[syncs[-1] + 1:]} & MUTATIONS
    wal_name = sorted(p.name for p in db.iterdir() if p.name.startswith('d3-wal-'))[-1]
    wal = (db / wal_name).read_bytes()
    frames, tail = wal_frames(wal)
    ops = row_ops(frames)
    assert tail == spec.get('tail', 0), (case, tail)
    if 'ends' in spec:
        assert ops[-len(spec['ends']):] == spec['ends'], (case, ops[-4:])
    if 'clr_frames' in spec:
        # N09: RollbackStart, then exactly D's CLRs (RamOnly k-1, Durable k).
        start = len(ops) - 1 - ops[::-1].index(ROLLBACK_START)
        assert ops[start + 1:] == [CLR] * spec['clr_frames'], (case, ops[start:])
    if spec.get('home_after_wal'):
        # The home page's write and sync follow the last WAL sync; the
        # home write is one page.
        last_wal = max(i for i, row in enumerate(done)
                       if row[1] == 'sync' and row[2].startswith('d3-wal-'))
        homes = [row for row in done[last_wal:] if row[1] == 'write'
                 and row[2].startswith('d3-data-')]
        assert len(homes) == 1 and homes[0][4] == '16384', homes
    if spec.get('unknown'):
        # The independent outcome of the unacknowledged Commit: a winner
        # iff the WAL holds its complete frame.
        winner = ops[-1] == COMMIT and ops.count(COMMIT) == 2
        assert winner == (len(spec['rows']) == 2), (case, ops[-3:])
    return dict(wal=wal_name, wal_bytes=len(wal), row_ops_tail=ops[-4:], tail=tail,
                witness=witness(case, spec, effects, db, wal_name))


def frame_at(wal, offset):
    """(length, LSN, op, scope) of the XWF1 frame header at `offset`."""
    assert wal[offset:offset + 4] == b'XWF1', offset
    return (int.from_bytes(wal[offset + 12:offset + 16], 'little'),
            int.from_bytes(wal[offset + 56:offset + 64], 'little'),
            int.from_bytes(wal[offset + 166:offset + 168], 'little'),
            int.from_bytes(wal[offset + 170:offset + 172], 'little'))


def preservation(spec):
    """The case's fault.preservation (consumers §9.4), from this file."""
    mode, op, *length = spec.get('keep', ('None', None))
    plan = dict(mode=mode)
    if op is not None:
        plan['F'] = dict(op=op, length=length[0] if length else FRAME_BYTES[op])
    if 'home' in spec:
        plan['partial_home'] = dict(page=spec['home'])
    return plan


def witness(case, spec, effects, db, wal_name):
    """D, F and partial_home of the produce cut (consumers §9.4)."""
    wal = (db / wal_name).read_bytes()
    mode, op, *expected = spec.get('keep', ('None', None))
    rows = [(i, row) for i, row in enumerate(effects) if row[2] == wal_name]
    writes = [(i, row) for i, row in rows if row[1] == 'write']
    synced = [i for i, row in rows if row[1] == 'sync' and row[5] == 'Ok(())']
    # Every successful WAL write's bytes are the file's (trace digest).
    for _, row in writes:
        if row[5].startswith('OK:'):
            offset, length = int(row[3]), int(row[4])
            _, size, sha = row[5].split(':')
            assert int(size) == length and offset + length <= len(wal), (case, row)
            assert digest(wal[offset:offset + length]) == sha, (case, row)
    attempts = [(i, row) for i, row in writes if row[5] == 'ATTEMPT']
    f = None
    if mode in ('RamOnly', 'FirstByte', 'WriteError', 'Durable', 'SyncError'):
        at, row = attempts[-1]
        offset, length = int(row[3]), int(row[4])
        assert length == (expected[0] if expected else FRAME_BYTES[op]), (case, row)
        f = dict(op=op, offset=offset, length=length, attempt_row=int(row[0]))
        after = [r for i, r in rows if i > at]
        if mode == 'RamOnly':
            # The blocked write is F's, and the file ends where it starts.
            assert effects[-1][1] == 'cut' and effects[at + 1:] == [effects[-1]], case
            assert len(wal) == offset, (case, len(wal), offset)
            f['bytes'] = 'absent'
        elif mode == 'FirstByte':
            assert after and after[0][5] == 'FIRST_BYTE:Ok(1)', (case, after[:1])
            assert len(wal) == offset + 1 and wal[offset:offset + 1] == b'X', case
            f['bytes'] = 'first byte'
        elif mode == 'WriteError':
            assert after and after[0][1] == 'fault' and int(after[0][3]) == offset \
                and int(after[0][4]) == length, (case, after[:1])
            assert len(wal) == offset, (case, len(wal), offset)
            f['bytes'] = 'absent'
        else:
            # F complete: its header is the expected frame and its bytes are
            # the trace's; Durable: an Ok sync of the WAL follows it.
            assert after and after[0][5].startswith('OK:'), (case, after[:1])
            frame_length, lsn, frame_op, scope = frame_at(wal, offset)
            assert (frame_length, frame_op, scope) == (length, op, 3), (case, frame_op)
            assert offset + length == len(wal), (case, offset, len(wal))
            f.update(lsn=lsn, sha256=digest(wal[offset:offset + length]))
            later = [i for i in synced if i > at]
            if mode == 'Durable':
                assert later, (case, 'no Ok sync after F')
                f['bytes'] = 'synced'
            else:
                assert not later, (case, 'F synced')
                faults = [r for i, r in rows if i > at and r[1] == 'fault']
                assert len(faults) == 1 and faults[0][5].startswith('SYNC_ERROR'), case
                f['bytes'] = 'present, sync failed'
        d_end = offset
        before = [i for i in synced if i < at]
    else:
        # Hook: the Forward's first RAM page, with no WAL write after the
        # last Ok sync. None: a worker cut (after a synced ACK, or the R
        # input whose last frames may be written but not yet synced).
        assert synced, case
        before = synced
        d_end = None
        if mode == 'Hook':
            assert not [r for i, r in writes if i > synced[-1]], (case, 'a WAL write after D')
            f = dict(op=op, bytes='never attempted')
    # D: the complete prefix the last Ok sync before F covered.
    assert before, (case, 'D unsynced')
    covered = [int(r[3]) + int(r[4]) for i, r in writes
               if i < before[-1] and r[5].startswith('OK:')]
    assert covered, case
    if d_end is None:
        d_end = max(covered)
        if mode == 'Hook':
            assert d_end == len(wal), (case, d_end, len(wal))
    assert max(covered) == d_end, (case, covered[-1:], d_end)
    frames, tail = wal_frames(wal[:d_end])
    assert tail == 0, (case, tail)
    d = dict(end=d_end, lsn=frames[-1][2], frames=len(frames), sha256=digest(wal[:d_end]))
    if mode == 'None':
        written, rest = wal_frames(wal)
        assert rest == 0, (case, rest)
        d['written_after'] = [op for at, _, _, op, scope in written if at >= d_end and scope == 3]
    out = dict(mode=mode, D=d, F=f)
    if 'home' in spec:
        assert f is not None and 'lsn' in f, case
        homes = []
        for data in sorted(db.glob('d3-data-*')):
            raw = data.read_bytes()
            for page in range(len(raw) // PAGE):
                image = raw[page * PAGE:(page + 1) * PAGE]
                if image[:4] != b'XDP1':
                    continue
                number = int.from_bytes(image[32:40], 'little')
                assert number == page, (case, data.name, page, number)
                homes.append((data.name, page, int.from_bytes(image[64:72], 'little')))
        newest = [h for h in homes if h[2] >= f['lsn']]
        assert len(newest) == 1 and newest[0][2] == f['lsn'], (case, newest)
        if spec['home'] is not None:
            assert newest[0][1] == spec['home'], (case, newest)
        out['partial_home'] = dict(file=newest[0][0], page=newest[0][1], lsn=newest[0][2],
                                   older=sorted({h[2] for h in homes} - {f['lsn']}))
    return out


def recovered_frames(case, spec, db, output):
    """Copy the newest WAL after recovery and count its Row CLR and Cancel
    frames (this epoch's), with this file's reader."""
    wal_name = sorted(p.name for p in db.iterdir() if p.name.startswith('d3-wal-'))[-1]
    raw = (db / wal_name).read_bytes()
    (output / f'recovered-{wal_name}').write_bytes(raw)
    frames, tail = wal_frames(raw)
    assert tail == 0, (case, tail)
    ops = [(op, scope) for _, _, _, op, scope in frames]
    clrs, cancels = ops.count((CLR, 3)), ops.count((CANCEL, 3))
    assert clrs == spec['clrs'], (case, 'CLR frames', clrs, spec['clrs'])
    assert cancels == spec.get('cancels', 0), (case, 'Cancel frames', cancels)
    return dict(wal=wal_name, sha256=digest(raw), clr_frames=clrs, cancel_frames=cancels)


def check_barrier(case, effects, db):
    label, at, before = BARRIERS[case]
    last = effects[-1]
    assert last[1] == 'cut' and last[2] == label and last[5].startswith(at), last
    done = [row for row in effects[:-1] if row[5] != 'ATTEMPT' and row[1] in MUTATIONS]
    if case in AFTER:
        # An After cut: its own successful event is the last one done.
        operation, prefix = at.split(':')
        assert done[-1][1] == operation and done[-1][2].startswith(prefix), done[-1]
        done = done[:-1]
    for back, (operation, prefix) in enumerate(reversed(before)):
        row = done[-1 - back]
        assert row[1] == operation and row[2].startswith(prefix), (case, row)
    published = sum(1 for row in done if row[1] == 'publish'
                    and row[2] == 'd3-current-stage')
    assert published == CURRENT_BEFORE[case], (case, published)
    if case == 'MCA03-R11':
        # The write the cut follows is the loser's AbortEnd, the last frame
        # of the new epoch: no cleanup frame (Prune, ReleaseRole, RootFree,
        # TerminalFinish) was written yet.
        write = done[-1]
        wal = (db / write[2]).read_bytes()
        offset, length = int(write[3]), int(write[4])
        frame = wal[offset:offset + length]
        assert frame[:4] == b'XWF1' and offset + length == len(wal), (case, offset)
        op = int.from_bytes(frame[166:168], 'little')
        scope = int.from_bytes(frame[170:172], 'little')
        assert (op, scope) == (ABORT_END, 3), (case, op, scope)
        ops = row_ops(wal_frames(wal)[0])
        assert ops[-1] == ABORT_END and not {PRUNE, RELEASE_ROLE, ROOT_FREE,
                                             TERMINAL_FINISH} & set(ops), ops
    return dict(barrier=label, current_published=published)


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
    names = [name for name in listed if name.endswith(f'::{WORKER}')]
    assert len(names) == 1, names
    plan = dict(selector=names[0], root=str(root), cases={c: CASES[c]['phases'] for c in selected},
                source=source_hash, elf=elf_hash, input=input_hash,
                per_process_seconds=SECONDS, log_bytes_cap=16777216, rust_stack=33554432,
                expected={c: dict(rows=CASES[c]['rows'], clrs=CASES[c]['clrs'],
                                  ack_row=CASES[c]['ack_row']) for c in selected})
    (out / 'execution-plan.json').write_text(json.dumps(plan, indent=2) + '\n')
    env = dict(os.environ, RUST_MIN_STACK='33554432', XADB_CATALOG_SOURCE_SHA256=source_hash,
               XADB_CATALOG_ELF_SHA256=elf_hash, XADB_CATALOG_INPUT_SHA256=input_hash)
    results, complete = [], {}
    for case in selected:
        spec = CASES[case]
        db = out / case / 'db'
        db.mkdir(parents=True)
        (out / case / 'input-plan.json').write_text(json.dumps(
            dict(case=case, fault=dict(preservation=preservation(spec)),
                 expected=dict(rows=spec['rows'], clrs=spec['clrs'],
                               cancels=spec.get('cancels', 0), ack_row=spec['ack_row'])),
            indent=2) + '\n')
        pids, acks, facts, previous, spent = [], [], {}, None, 0.0
        rows = list(spec['rows'])
        for phase in spec['phases']:
            output = out / case / phase
            output.mkdir()
            env.update(XADB_S10A_NATIVE_CASE=case, XADB_S10A_NATIVE_PHASE=phase,
                       XADB_S10A_NATIVE_DB=str(db), XADB_S10A_NATIVE_OUTPUT=str(output))
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
            spent += seconds
            results.append(dict(case=case, phase=phase, exit=code, seconds=round(seconds, 2)))
            (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
            assert (output / 'test.log').stat().st_size <= plan['log_bytes_cap']
            print(case, phase, code, round(seconds, 2), flush=True)
            cut_phase = phase == 'produce' or phase == 'crash'
            if code != (-signal.SIGABRT if cut_phase else 0):
                raise SystemExit(f'{case} {phase}: exit {code}')
            receipt = receipt_of(output)
            assert receipt['source'] == source_hash and receipt['elf'] == elf_hash
            assert receipt['case'] == case and receipt['phase'] == phase
            pids.append(receipt['pid'])
            effects = trace(output)
            current = snapshot(db)
            (output / 'files.json').write_text(json.dumps(current, indent=2) + '\n')
            if phase == 'produce':
                facts[phase] = check_produce(case, spec, receipt, effects, db)
                (out / case / 'witness.json').write_text(
                    json.dumps(facts[phase]['witness'], indent=2) + '\n')
                if 'ack_q' in receipt:
                    acks.append(int(receipt['ack_q']))
                if spec.get('unknown') and len(spec['rows']) == 2:
                    # The unacknowledged winner's Commit takes the next q.
                    acks.append(acks[-1] + 1)
            elif phase == 'crash' and case != 'MCA03-R12':
                facts[phase] = check_barrier(case, effects, db)
                assert current != previous, 'the crash wrote nothing'
            else:
                if phase == 'crash':
                    # R12: READY, the new ACK, then the cut with no mutation
                    # after the ACK's sync but the lease drop and closes.
                    assert receipt['cut'] == 'R12_AFTER_NEW_ACK'
                    syncs = [i for i, row in enumerate(effects)
                             if row[1] == 'sync' and row[5] == 'Ok(())']
                    assert {row[1] for row in effects[syncs[-1] + 1:]} <= \
                        {'drop_lease', 'close', 'read', 'metadata'}
                else:
                    assert receipt['result'] == 'ready'
                got = rows_of(receipt['rows'])
                assert got == rows, (case, phase, got, rows)
                assert int(receipt['q_ready']) == acks[-1], (case, phase, receipt['q_ready'], acks)
                assert receipt['row_cells'] == '0'
                first = phase == 'recover' or (case == 'MCA03-R12' and phase == 'crash')
                assert int(receipt['clrs']) == (spec['clrs'] if first else 0), \
                    (case, phase, receipt['clrs'])
                if first:
                    facts['recovered'] = recovered_frames(case, spec, db, output)
                if 'ack_row' in receipt:
                    row, q = int(receipt['ack_row']), int(receipt['ack_q'])
                    assert q == acks[-1] + 1, (case, phase, q, acks)
                    if phase == 'again':
                        # MCA04-N01: the issuer after a further reopen.
                        assert row == rows[-1][0] + 1 and row > spec['ack_row'], (case, row)
                    else:
                        want = spec['ack_row']
                        assert (row >= want[1]) if isinstance(want, tuple) else row == want, \
                            (case, row, want)
                        assert all(row > previous_row for previous_row, _ in rows)
                    acks.append(q)
                    rows.append((row, NEW))
                if case == 'MCA04-N01' and phase == 'recover':
                    # Commit -> Prune/free -> old WAL reclaim -> reopen: the
                    # recovery wrote the cleanup frames and unlinked the
                    # selected old WAL.
                    removed = [row for row in effects if row[1] == 'remove'
                               and row[5] == 'Ok(())']
                    assert [row[2] for row in removed] == ['d3-wal-00000001'], removed
                    wal_name = sorted(n for n in current if n.startswith('d3-wal-'))[-1]
                    ops = row_ops(wal_frames((db / wal_name).read_bytes())[0])
                    assert PRUNE in ops and ROOT_FREE in ops and TERMINAL_FINISH in ops, ops
                    facts['reclaimed'] = dict(removed='d3-wal-00000001', new_wal=wal_name,
                                              cleanup_ops=sorted(set(ops)))
            previous = current
        assert len(set(pids)) == len(spec['phases']), pids
        # The frozen bound is per native case (consumers §9.4): all phases.
        assert spent <= SECONDS, (case, spent)
        complete[case] = dict(pids=pids, acks=acks, rows=rows, facts=facts,
                              seconds=round(spent, 2), result='PASS')
        (out / 'complete.json').write_text(json.dumps(complete, indent=2) + '\n')


if __name__ == '__main__':
    main()
