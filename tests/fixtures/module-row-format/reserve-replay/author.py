#!/usr/bin/env python3
"""S1-g RowReserve replay literals; run as `python3 -I author.py`.

Only stdlib (copy/hashlib/importlib/json/pathlib/struct/sys). The one imported
author is N1's (`../transition/author.py`), loaded by path and pinned by
SHA-256; no XADB encoder, decoder or test helper is imported or executed.
Every parent is read as bytes and pinned; N1's H history is re-run here and
each re-run frame must equal N1's published blob before any state is used.
Contract: dev3-module-boundaries `row-reserve-replay` (2026-10-29), tests
H08-H10. Outputs (blobs.bin, index.tsv, input-plan.json) are rewritten.

G-allocator   page 0 of N1's arena: genesis entries 0-8 (re-encoded from the
              pinned genesis P00 and compared), Status 9 (inc 2, object 2),
              Undo 10/11/14 (inc 2, object 3), anchor 12 and leaf 13 (inc 2,
              object 6), every other entry Free(1); LSN 60 (N1's INIT of 14).
G-allocator-free14 / -inc14   entry 14 Free(2) / Allocated generation 3.
G-leaf-crowded  A's full noh-300 leaf without row 10, row 11's record copied
              to row 18: key 10 absent and C=201 does not fit.
G-leaf-key10  N1's leaf before H-505 plus A's row-10 cell: key 10 present.
G-505-*       H-505 rewritten in its Job native after only (native digest
              and envelope digest recomputed): projection, C (budget kept
              consistent inside the cell), C 184/2229, the h=0 budget, the
              container page (Status 9, anchor 12, leaf 13, root U 10,
              unallocated 15) and incarnation 3.
G-505-ordinal18 / -jobgen / -undogen / -statement1   whole Reserve groups
              re-produced with one selector or issuance changed.
G-506-reserve2  a second Reserve after H-505 (a Reserved root exists).
G-507-reserve-z  Reserve of Z=canonical(13, None) in place of H-507, every
              carrier consistent; G-508-cancel-z its Cancel and
              G-508-forward-x-over-z the Forward of H-507's value over it.
G-508-forward-y  the Forward of Y=canonical(12, None) over H-507 (X);
G-508-cancel-x   the Cancel of H-507.
The two value-mismatched Forwards are N1's row_change with its commitment
assert satisfied by the Job's own projection: the inserted record is the
other value, everything else is N1's encoding.
"""
from hashlib import sha256
from pathlib import Path
from struct import pack, pack_into, unpack_from
import copy
import importlib.util
import json
import sys

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parent.parent
PARENTS = {
    'module-row-format/transition/author.py':
        '076ea2f2767851bd8046becbba6b1d05a00a6cd7838a9c9e052b6b2e033beec4',
    'module-row-format/transition/blobs.bin':
        '96f88af3b8973f97d4bedcb315c47aaa65fffb27027cafefbad37a876e6ded6c',
    'module-row-format/transition/index.tsv':
        '9d57952d6afecb04b2b03f1fa3eddd7348c2c8739fb841d9e22ac14a6f189f29',
    'module-row-format/genesis3-namespace/raw/P00.bin':
        '077d06af8b37bec32a9472ed498d3fa884fbe5e8fd84c457f6ebfa96e9946cfd',
    'module-row-insert/scope3/noh-300-leaf-full.bin':
        '63e019f509565c03d139c60239e964423683b7ad77e1b86feaa1d83bc9bf796c',
}


def parent(name):
    data = (FIXTURES / name).read_bytes()
    assert sha256(data).hexdigest() == PARENTS[name], name
    return data


def load_n1():
    sys.dont_write_bytecode = True  # leave N1's directory untouched
    parent('module-row-format/transition/author.py')
    spec = importlib.util.spec_from_file_location(
        'n1_transition_author', FIXTURES / 'module-row-format/transition/author.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.PROFILE = module.a('profile3.bin')
    return module


n1 = load_n1()


def n1_blobs():
    blobs = parent('module-row-format/transition/blobs.bin')
    out = {}
    for line in parent('module-row-format/transition/index.tsv').decode().splitlines():
        name, offset, length, digest = line.split('\t')
        data = blobs[int(offset):int(offset) + int(length)]
        assert sha256(data).hexdigest() == digest, name
        out[name] = data
    return out


N1 = n1_blobs()

# ---- allocator page 0 (dev3 allocation format: XAL1, 64 entries of 96B) -----

GENESIS_DB = bytes(range(1, 17))
ALLOCATION_KIND = 10


def slotted(db, kind, page_no, incarnation, owner, lsn, cells):
    """XDP1 slotted page, as N1's `page` but with an explicit kind byte."""
    out = bytearray(n1.PAGE)
    out[:4] = b'XDP1'
    pack_into('<HBB', out, 4, 1, kind, 0)
    out[8:24] = db
    pack_into('<6Q', out, 24, n1.FILE, page_no, incarnation, owner, 1, lsn)
    upper = n1.DIGEST
    for slot, cell in enumerate(cells):
        upper -= len(cell)
        pack_into('<HH', out, 128 + 4 * slot, upper, len(cell))
        out[upper:upper + len(cell)] = cell
    pack_into('<HHH', out, 72, len(cells), 128 + 4 * len(cells), upper)
    out[n1.DIGEST:] = n1.sha(out[:n1.DIGEST])
    return bytes(out)


def entry(state, kind=0, generation=1, obj=0, obj_generation=0):
    """State 0 Free(generation) or 2 Allocated{generation, object, og, kind}."""
    out = bytearray(96)
    out[0] = state
    out[1] = kind
    pack_into('<3Q', out, 8, generation, obj, obj_generation)
    return bytes(out)


def allocator(db, lsn, entries):
    assert len(entries) == 64
    head = bytearray(64)
    head[:4] = b'XAL1'
    pack_into('<HHQHH', head, 4, 1, 64, 0, 64, 96)
    head[32:56] = pack('<QQQ', n1.FILE, 0, 1)
    return slotted(db, ALLOCATION_KIND, 0, 1, 1, lsn, [bytes(head) + b''.join(entries)])


GENESIS = [entry(2, 10, 1, 1, 1), entry(2, 12, 1, 4, 1)] + [entry(2, 13, 1, 4, 1)] * 5 \
    + [entry(2, 11, 1, 5, 1), entry(2, 5, 1, 5, 1)] + [entry(0)] * 55
# Codec cross-check: the genesis allocator re-encodes to the pinned P00.
assert allocator(GENESIS_DB, 0, GENESIS) == parent('module-row-format/genesis3-namespace/raw/P00.bin')
# and N1's own pages re-encode through `slotted` (kind byte explicit).
assert slotted(n1.DB, 8, 14, 2, 3, 60, [n1.undo_free(2)]) == N1['H-undo14-0']

N1_ENTRIES = list(GENESIS)
N1_ENTRIES[9] = entry(2, 9, 2, 2, 1)       # Status 9
for undo_page in (10, 11, 14):
    N1_ENTRIES[undo_page] = entry(2, 8, 2, 3, 1)  # Undo
N1_ENTRIES[12] = entry(2, 11, 2, 6, 1)     # TreeAnchor of table 6
N1_ENTRIES[13] = entry(2, 1, 2, 6, 1)      # RowLeaf of table 6


def n1_allocator(changes=()):
    entries = list(N1_ENTRIES)
    for index, value in changes:
        entries[index] = value
    return allocator(n1.DB, 60, entries)


# ---- leaf variants ------------------------------------------------------------

def leaf_rows(page_bytes):
    rows = {}
    for slot in range(1, n1.get(page_bytes, 72, 2)):
        cell = n1.cell_at(page_bytes, slot)[1]
        rows[int.from_bytes(cell[4:12], 'big')] = cell[12:]
    return rows


def leaf_free(page_bytes):
    count, lower, upper = unpack_from('<HHH', page_bytes, 72)
    assert lower == 128 + 4 * count
    return upper - lower


# ---- frame rewrites ----------------------------------------------------------

def native_set(frame, kind, fields):
    """Set (offset, value) u64/u32 fields of the kind's after cell; the
    native after digest and the envelope digest are recomputed."""
    out = bytearray(frame)
    at = n1.native_at(frame, kind)
    before_len = n1.get(out, at + 12, 4)
    after_len = n1.get(out, at + 16, 4)
    cell = at + 128 + before_len
    for offset, value, width in fields:
        n1.put(out, cell + offset, value, width)
    out[at + 56:at + 88] = n1.sha(out[cell:cell + after_len])
    variant = n1.reseal(out)
    assert variant != frame and len(variant) == len(frame)
    return variant


def job_after(frame):
    at = n1.native_at(frame, 9)
    before_len = n1.get(frame, at + 12, 4)
    after_len = n1.get(frame, at + 16, 4)
    return frame[at + 128 + before_len:at + 128 + before_len + after_len]


def budget_fields(cell_bytes, h):
    wal = 311_608 + 2 * cell_bytes + 128 * h
    return [(280, cell_bytes, 4), (784, wal, 8), (880, wal, 8)]


def reserve(h, lsn, container, statement, canon, ordinal=None, job_step=1, undo_step=1):
    """N1's History.reserve with the Job ordinal and the Job/U generation
    steps as parameters; the defaults are N1's (asserted on H-505)."""
    actor = h.tx['actor']
    ordinal = h.first_free() if ordinal is None else ordinal
    assert h.bank[ordinal][6] == 0
    index = ordinal // 15
    roles = [('root', 0), ('jobs', index), ('undo', container)]
    before = h.snapshot(roles)
    tx_before, aging_before = h.tx_descriptor(), h.aging_descriptor()
    cell, _ = h.undo[container]
    assert cell == n1.undo_free(n1.get(cell, 8))
    undo = n1.slot_ref(container, n1.get(cell, 8) + undo_step)
    row = n1.get(h.root, 432) + 1
    sequence = n1.get(h.root, 392) + 1
    mutation = h.tx['mutation'] + 1
    head = n1.core_refs(h.status_core())[0]
    has_h = head is not None
    value = n1.record(row, canon, actor, mutation, undo)
    size = len(value) + 12
    job = n1.job_free(n1.get(h.bank[ordinal], 8) + job_step)
    job[6:8] = b'\x01\x03'
    job[16:64] = actor
    n1.put(job, 64, lsn)
    job[80:120] = n1.slot_ref(h.tx['status'], n1.get(actor, 40))
    job[128:168] = undo
    pack_into('<4Q', job, 168, sequence, *n1.TABLE)
    job[200:224] = n1.page_ref(12, 2)
    job[224:248] = n1.page_ref(13, 2)
    pack_into('<2Q', job, 248, statement, mutation)
    pack_into('<I', job, 280, size)
    job[288:320] = n1.projection(row, value)
    n1.put(job, 320, row)
    n1.put(job, 376, lsn)
    job[384:] = n1.budget(5, 311_608 + 2 * size + 128 * has_h)
    h.bank[ordinal] = job
    h.canonical[ordinal] = canon
    h.jobs_lsn[index] = lsn
    h.undo[container] = [n1.undo_free(n1.get(cell, 8) + undo_step), lsn]
    n1.put(h.root, 88, n1.get(h.root, 88) + 1)
    n1.put(h.root, 392, sequence)
    n1.put(h.root, 432, row)
    h.advance_root(lsn)
    item = n1.undo_item(job)
    h.tx['inventory'] = (lsn, 1, n1.sha(item))
    h.tx['mutation'] = mutation
    h.tx['credit'] = True
    h.advance_tx(lsn, n1.RESERVE)
    slot = ordinal % 15
    frame = h.emit(11, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                             ('undo', container, (0, 0))], before,
                   txn=(actor[:40], h.tx['chain'][1]),
                   owners=([tx_before, aging_before], [h.tx_descriptor(), h.aging_descriptor()]),
                   core=n1.core256(11, statement, mutation, sequence, current=undo),
                   aux=[n1.delta(1, item)], items=1)
    return frame, ordinal


def forward_other(h, lsn, ordinal, canon):
    """N1's Forward of `canon` over a Reserve that committed another value:
    the Job's own projection satisfies N1's assert, the leaf gets `canon`."""
    present = bytes(h.bank[ordinal][288:320])
    original = n1.projection
    n1.projection = lambda row, value: original(row, None) if value is None else present
    try:
        h.canonical[ordinal] = canon
        return h.row_change(2, lsn, ordinal)
    finally:
        n1.projection = original


def history():
    """N1's H history to H-508, each frame equal to N1's blob; the states
    before H-505 and after H-505/H-506/H-507 are kept."""
    h = n1.History.from_a('h-500', [0, 1], [10, 11])
    h.canonical[16] = n1.canonical(42, 'xadb')
    assert h.row_change(2, 501, 16) == n1.a('h-501-forward.wal')
    mark = n1.slot_ref(10, 2)
    assert h.status_patch(3, 502, statement=2, mark=mark) == N1['H-502']
    assert h.row_change(4, 503, 16) == n1.a('h-503-clr.wal')
    assert h.status_patch(5, 504, statement=2, mark=mark) == N1['H-504']
    h.undo[14] = [n1.undo_free(2), 60]
    assert h.undo_page(14) == N1['H-undo14-0']
    states = {'504': copy.deepcopy(h)}
    assert N1['H-505-canonical'] == n1.canonical(10, 'ten')
    frame, ordinal = h.reserve(505, 14, 3, N1['H-505-canonical'])
    assert frame == N1['H-505'] and ordinal == 17
    states['505'] = copy.deepcopy(h)
    assert h.cancel(506, 17) == N1['H-506']
    states['506'] = copy.deepcopy(h)
    assert N1['H-507-canonical'] == n1.canonical(11, None)
    frame, ordinal = h.reserve(507, 14, 4, N1['H-507-canonical'])
    assert frame == N1['H-507'] and ordinal == 17
    states['507'] = copy.deepcopy(h)
    assert h.row_change(2, 508, 17) == N1['H-508']
    # The parametric Reserve reproduces N1's at its defaults.
    for state, lsn, statement, canon, name in (
            ('504', 505, 3, N1['H-505-canonical'], 'H-505'),
            ('506', 507, 4, N1['H-507-canonical'], 'H-507')):
        assert reserve(copy.deepcopy(states[state]), lsn, 14, statement, canon) == (N1[name], 17)
    return states


def main():
    out = {}
    states = history()

    out['G-allocator'] = n1_allocator()
    out['G-allocator-free14'] = n1_allocator([(14, entry(0, generation=2))])
    out['G-allocator-inc14'] = n1_allocator([(14, entry(2, 8, 3, 3, 1))])

    full = parent('module-row-insert/scope3/noh-300-leaf-full.bin')
    rows = leaf_rows(full)
    assert n1.leaf_page(rows, n1.page_lsn(full)) == full and 10 in rows and 11 in rows
    crowded = {k: v for k, v in rows.items() if k != 10}
    eighteen = bytearray(rows[11])
    assert n1.get(eighteen, 8) == 11
    n1.put(eighteen, 8, 18)
    crowded[18] = bytes(eighteen)
    out['G-leaf-crowded'] = n1.leaf_page(crowded, n1.page_lsn(full))
    assert leaf_free(out['G-leaf-crowded']) < 201 + 4
    before = states['504']
    assert before.leaf() == n1.a('h-503-leaf.bin') and 10 not in before.rows
    out['G-leaf-key10'] = n1.leaf_page({**before.rows, 10: rows[10]}, before.leaf_lsn)
    assert leaf_free(out['G-leaf-key10']) >= 201 + 4

    h505 = N1['H-505']
    cell = job_after(h505)
    assert n1.get(cell, 280, 4) == 201 and n1.get(cell, 784) == 311_608 + 402 + 128
    assert n1.get(cell, 136) == 14 and n1.get(cell, 144) == 2
    flip = bytearray(cell[288:296])
    flip[0] ^= 1
    out['G-505-projection'] = native_set(h505, 9, [(288, int.from_bytes(flip, 'little'), 8)])
    out['G-505-c'] = native_set(h505, 9, budget_fields(200, 1))
    out['G-505-c184'] = native_set(h505, 9, budget_fields(184, 1))
    out['G-505-c2229'] = native_set(h505, 9, budget_fields(2229, 1))
    out['G-505-budget'] = native_set(h505, 9, budget_fields(201, 0))
    for page_no in (9, 12, 13, 10, 15):
        out[f'G-505-container{page_no}'] = native_set(h505, 9, [(136, page_no, 8)])
    out['G-505-container-inc3'] = native_set(h505, 9, [(144, 3, 8)])

    canon = N1['H-505-canonical']
    for name, kwargs in (('ordinal18', dict(ordinal=18)), ('jobgen', dict(job_step=2)),
                         ('undogen', dict(undo_step=2)), ('statement1', dict(statement=1))):
        arguments = dict(statement=3, **kwargs) if 'statement' not in kwargs else kwargs
        frame, _ = reserve(copy.deepcopy(states['504']), 505, 14, canon=canon, **arguments)
        assert frame != h505 and len(frame) == len(h505), name
        out[f'G-505-{name}'] = frame
    frame, ordinal = reserve(copy.deepcopy(states['505']), 506, 14, 3, canon)
    assert ordinal == 18
    out['G-506-reserve2'] = frame

    x, y, z = N1['H-507-canonical'], n1.canonical(12, None), n1.canonical(13, None)
    out['G-508-y-canonical'] = y
    out['G-507-z-canonical'] = z
    h = copy.deepcopy(states['507'])
    out['G-508-forward-y'] = forward_other(h, 508, 17, y)
    assert out['G-508-forward-y'] != N1['H-508'] and len(out['G-508-forward-y']) == len(N1['H-508'])
    out['G-508-cancel-x'] = copy.deepcopy(states['507']).cancel(508, 17)
    h = copy.deepcopy(states['506'])
    out['G-507-reserve-z'], ordinal = reserve(h, 507, 14, 4, z)
    assert ordinal == 17 and out['G-507-reserve-z'] != N1['H-507']
    assert job_after(out['G-507-reserve-z'])[288:320] != job_after(N1['H-507'])[288:320]
    assert job_after(out['G-507-reserve-z'])[280:284] == job_after(N1['H-507'])[280:284]
    after_z = copy.deepcopy(h)
    out['G-508-cancel-z'] = h.cancel(508, 17)
    out['G-508-forward-x-over-z'] = forward_other(after_z, 508, 17, x)

    blobs = bytearray()
    lines = []
    for name, data in out.items():
        lines.append(f'{name}\t{len(blobs)}\t{len(data)}\t{sha256(data).hexdigest()}')
        blobs += data
    (HERE / 'blobs.bin').write_bytes(bytes(blobs))
    (HERE / 'index.tsv').write_text('\n'.join(lines) + '\n')
    plan = dict(scope='S1-g RowReserve replay (contract row-reserve-replay H08-H10)',
                oracle='N1 author re-run (frames equal to N1 blobs) and independent byte packing',
                parents={name: PARENTS[name] for name in sorted(PARENTS)},
                outputs=[line.split('\t')[0] for line in lines])
    (HERE / 'input-plan.json').write_text(json.dumps(plan, indent=2) + '\n')


if __name__ == '__main__':
    main()
