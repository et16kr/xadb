#!/usr/bin/env python3
"""Independent Job3/Root3 literals: offsets, integer arithmetic, SHA-256.
No imports from product code, product encoders, fixture authors or test helpers.
The census proves only cells; it does not manufacture actual persistent pages.
"""
from pathlib import Path
import hashlib
import json
import struct

ROOT = Path(__file__).resolve().parent
DB = bytes(range(1, 17))

def u64(out, at, value):
    struct.pack_into('<Q', out, at, value)

def page(out, at, number):
    struct.pack_into('<QQQ', out, at, 0x100000001, number, 1)

def budget(steps, wal, registry):
    out = bytearray(640)
    for row in range(5):
        out[row * 128] = row + 1
    for at, value in [(8, steps), (16, wal), (64, 9), (72, 425984),
                      (88, registry), (96, 5), (104, 1), (112, wal)]:
        u64(out, 384 + at, value)
    return out

def job(kind=0, state=0, *, actor=1, status=9, undo=12, sequence=1,
        row=1, statement=1, mutation=1, cell=185, h=0, cursor=0, outcome=0,
        lease=None, adoption=None, q=1, row_terminal=False):
    out = bytearray(1024)
    out[:4] = b'XQJ1'
    struct.pack_into('<H', out, 4, 3)
    out[6:8] = bytes([state, kind])
    u64(out, 8, 1 if kind == 0 else 2)
    if kind == 0:
        return out
    out[16:32] = DB
    u64(out, 32, 1)
    struct.pack_into('<I', out, 40, 1)
    u64(out, 48, actor)
    u64(out, 56, 2)
    lease = (1 if kind == 1 else 2) if lease is None else lease
    adoption = (50 if kind == 1 else 3) if adoption is None else adoption
    u64(out, 64, lease)
    page(out, 80, status)
    u64(out, 112, 2)
    if state > 1:
        u64(out, 72, adoption)
        out[328:360] = bytes([0x55]) * 32
    u64(out, 376, lease if state == 1 else adoption)
    if kind == 1:
        out[120] = outcome
        u64(out, 264, q if outcome == 1 else 0)
        steps, wal = 1, 55112
    else:
        page(out, 128, undo)
        u64(out, 160, 2)
        u64(out, 168, sequence)
        for at, value in [(176, 6), (184, 1), (192, 1),
                          (248, statement), (256, mutation)]:
            u64(out, at, value)
        if kind == 3:
            u64(out, 320, row)
        if state < 3:
            page(out, 200, 10)
            page(out, 224, 11)
            struct.pack_into('<I', out, 280, cell)
            out[288:320] = bytes([0x44]) * 32
            steps, wal = 5, 311608 + 2 * cell + 128 * h
        else:
            out[121] = cursor
            u64(out, 264, q if state == 3 else 0)
            u64(out, 272, 60 if state == 3 else 40)
            u64(out, 376, (60 if state == 3 else 40) if cursor == 0 else 60 + cursor)
            steps, wal = 4 - cursor, (4 - cursor) * 55528
    out[384:] = budget(steps, wal, 16 if kind == 3 or row_terminal else 9)
    return out

def census(occupied):
    jobs = occupied + [job() for _ in range(64 - len(occupied))]
    aggregate = bytearray(640)
    for r in range(5):
        aggregate[r * 128] = r + 1
    masks = [0, 0, 0]
    for ordinal, j in enumerate(jobs):
        masks[0 if j[6] == 0 else 1 if j[6] == 1 else 2] |= 1 << ordinal
        for r in range(5):
            for at in range(8, 120, 8):
                off = r * 128 + at
                prior = struct.unpack_from('<Q', aggregate, off)[0]
                value = struct.unpack_from('<Q', j, 384 + off)[0]
                u64(aggregate, off, max(prior, value) if 64 <= at < 112 else prior + value)
    out = bytearray(512)
    out[:4] = b'XQR1'
    struct.pack_into('<H', out, 4, 3)
    out[6] = 1
    struct.pack_into('<II', out, 8, 512, 64)
    out[16:32] = DB
    u64(out, 32, 1)
    out[40] = 3
    for at, value in [(48, 1), (56, 1), (64, 2), (80, 99), (88, len(occupied)),
                      (96, masks[0]), (104, masks[1]), (112, masks[2]),
                      (208, 1), (216, 4), (224, 1), (392, 20), (400, 6),
                      (408, 2), (432, 20)]:
        u64(out, at, value)
    out[128:160] = hashlib.sha256(b''.join(jobs)).digest()
    out[160:192] = hashlib.sha256(aggregate).digest()
    out[192:208] = DB
    for i in range(6):
        page(out, 232 + i * 24, i + 1)
    return b''.join(jobs), out, aggregate

def write():
    cells = [job()]
    labels = ['free']
    for row_terminal in [False, True]:
        for state, outcome in [(1, 0), (2, 1), (2, 2)]:
            cells.append(job(1, state, outcome=outcome, row_terminal=row_terminal))
            labels.append(('row' if row_terminal else 'catalog') + f'-terminal-{outcome}')
    for kind in [2, 3]:
        for state in [1, 2]:
            for cell, h in ([(254, 0), (4600, 0)] if kind == 2 else [(185, 0), (2228, 1)]):
                cells.append(job(kind, state, cell=cell, h=h))
                labels.append(f'kind{kind}-state{state}-cell{cell}-h{h}')
        for state in [3, 4]:
            for cursor in range(4):
                cells.append(job(kind, state, cursor=cursor))
                labels.append(f'kind{kind}-state{state}-cursor{cursor}')
    (ROOT / 'cells.bin').write_bytes(b''.join(cells))
    (ROOT / 'index.json').write_text(json.dumps(labels, indent=2) + '\n')
    groups = [
        # Multiple statements/roots/actors sharing one existing table.
        [job(1, 1, row_terminal=True),
         job(3, 2),
         job(3, 2, undo=13, sequence=2, row=2, statement=2, mutation=2, cell=2228, h=1),
         job(3, 4, undo=14, sequence=3, row=3, statement=3, mutation=3),
         job(3, 1, undo=15, sequence=4, row=4, statement=4, mutation=4, h=1),
         job(1, 1, actor=2, status=16, row_terminal=True),
         job(3, 2, actor=2, status=16, undo=17, sequence=5, row=5)],
        # Successful earlier rows and a compensated suffix can commit together.
        [job(1, 2, outcome=1, row_terminal=True), job(3, 2),
         job(3, 3, cursor=1, undo=13, sequence=2, row=2, mutation=2),
         job(3, 4, cursor=2, undo=14, sequence=3, row=3, mutation=3)],
        # Full abort has no retained roots; four independent cleanup cursors.
        [job(1, 2, outcome=2, row_terminal=True)] +
        [job(3, 4, cursor=i, undo=12+i, sequence=1+i, row=1+i, mutation=1+i) for i in range(4)],
        # Catalog committed table followed by a different actor's Row Reserve.
        [job(1, 2, actor=2, status=16, outcome=1),
         job(2, 2, actor=2, status=16, undo=17, sequence=8, cell=254),
         job(1, 1, row_terminal=True), job(3, 2, sequence=9, lease=60, adoption=61)],
    ]
    result = [census(g) for g in groups]
    for i, name in enumerate(['jobs.bin', 'roots.bin', 'aggregates.bin']):
        (ROOT / name).write_bytes(b''.join(r[i] for r in result))

if __name__ == '__main__':
    write()
