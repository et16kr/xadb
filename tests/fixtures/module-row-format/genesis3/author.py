#!/usr/bin/env python3
"""Independent Profile3 genesis retention pages over hash-pinned Root3/Job3 cells.
Only pathlib/struct/hashlib; no XADB encoder/decoder, test helper or other
fixture author is imported or executed. Parents are read as bytes only.
"""
from pathlib import Path
import hashlib
import struct

ROOT = Path(__file__).resolve().parent
PAGE = 16384
DIGEST = PAGE - 32
DB_A = bytes(range(1, 17))
DB_B = bytes(range(0xA1, 0xB1))


def parent(path, expected):
    data = (ROOT.parent / path).read_bytes()
    assert hashlib.sha256(data).hexdigest() == expected, path
    return data


def put(out, at, value, width=8):
    struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[width], out, at, value)


def page(db, number, kind, lsn, cells):
    # XDP1 v1: kind, level0, database, PageRef(first file, number,
    # incarnation1), queue owner object4/generation1, LSN, slot directory;
    # cells packed downward from the digest, zero gap, SHA-256 trailer.
    out = bytearray(PAGE)
    out[:4] = b'XDP1'
    put(out, 4, 1, 2)
    out[6] = kind
    out[8:24] = db
    for at, value in [(24, 0x0000000100000001), (32, number), (40, 1),
                      (48, 4), (56, 1), (64, lsn)]:
        put(out, at, value)
    put(out, 72, len(cells), 2)
    put(out, 74, 128 + 4 * len(cells), 2)
    end = DIGEST
    for slot, cell in enumerate(cells):
        start = end - len(cell)
        put(out, 128 + 4 * slot, start, 2)
        put(out, 130 + 4 * slot, len(cell), 2)
        out[start:end] = cell
        end = start
    put(out, 76, end, 2)
    out[DIGEST:] = hashlib.sha256(out[:DIGEST]).digest()
    return bytes(out)


def genesis(db, root_cell, free_cell):
    # Root page1 (kind12) holds one Root cell; Jobs pages2..6 (kind13) hold
    # 15,15,15,15,4 identical Free cells. Every genesis page is unlogged LSN0.
    pages = [page(db, 1, 12, 0, [root_cell])]
    for index in range(5):
        pages.append(page(db, 2 + index, 13, 0,
                          [free_cell] * (4 if index == 4 else 15)))
    return pages


def legacy2(initial3):
    # The Profile2-current Root2/Job2 genesis recipe: same layout, version2
    # cells, and a Root2 job digest over64 Free Job2 cells. Rejected input.
    free = bytearray(1024)
    free[:4] = b'XQJ1'
    put(free, 4, 2, 2)
    put(free, 8, 1)
    root = bytearray(initial3)
    put(root, 4, 2, 2)
    root[128:160] = hashlib.sha256(bytes(free) * 64).digest()
    return bytes(root), bytes(free)


def write():
    initial = parent('root3/initial-root.bin',
                     'd6079c1ea15ea2668be3150c6128d9be4a72e28855336ea7c1d971f73d0b3fc4')
    free = parent('root3/free-job.bin',
                  'aee93979e2d5bf1901a0daf894a8f8c705fcb3335581f215f405b6326a8dbe69')
    active = parent('root3/active-root.bin',
                    '8162de64c7d733908c5f617f89fac36e4cf65186bed83a0b79282178d39be5f9')
    envelopes = parent('envelopes/pages.bin',
                       'a6877a6d30c795f532803ce348a56962735652d80d1d06c28239d873f9fac6d7')
    # Namespace B changes only the Root cell database (192..208); the Job and
    # budget commitments and fixed PageRefs do not depend on the database.
    initial_b = bytearray(initial)
    initial_b[192:208] = DB_B
    pages = genesis(DB_A, initial, free) + genesis(DB_B, bytes(initial_b), free)
    # The earlier independent envelope author wrote the same six LSN0 pages.
    assert b''.join(pages[:6]) == envelopes[:6 * PAGE]
    root2, free2 = legacy2(initial)
    rejects = genesis(DB_A, root2, free2)
    # A valid active Root3 (Aging, apply99) at its own LSN is not genesis.
    rejects.append(page(DB_A, 1, 12, 99, [active]))
    (ROOT / 'pages.bin').write_bytes(b''.join(pages))
    (ROOT / 'rejects.bin').write_bytes(b''.join(rejects))


if __name__ == '__main__':
    write()
