#!/usr/bin/env python3
"""Independent full Base4 closure inputs, stdlib only. No product/generator import.

genesis/aging are complete current states with no System allocation. live adds
a committed Catalog CREATE (C), a finished Row INSERT (F) and an Active Row
actor (A) with two Retained roots, after a finished System Undo INIT. r2 states
are live-parented run lists: a Prepared System page reservation (one XOI item),
and a Catalog actor D whose CREATE is Reserved (three XOI items) or Retained and
uncommitted. Integers and SHA claims here are not WAL history; D's residual rows
copy the Profile2 Catalog recipe only as a stand-in for the Profile3 owner.
"""
from pathlib import Path
import hashlib
import json
import struct

ROOT = Path(__file__).resolve().parent
PARENTS = {
    '../base4/initial.xrb': '9473833616a56fbf74fcfc51d14c5eeb2ff96a4ce7cc3889d3f3eb3e809f4df0',
    '../job3/cells.bin': 'd2ea7dae34cd200c8cf6d75500e669f8ddb54ddbb32f7ab26575648342de8dc7',
    '../envelopes/profiles.bin': 'e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a',
    '../../dev3-plan4-recovery-base/vectors/p1/references/profile.bin':
        'c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4',
}
P = 16384
BASE = 2097152
DB = bytes(range(1, 17))
FILE = 0x100000001
BANK, CERTS, INITS, INVENTORY, ARENA = 4096, 65536, 73728, 98304, 262144
SECTIONS = [(BANK, 36 * 1536), (CERTS, 64 * 96), (INITS, 64 * 352), (INVENTORY, P), (ARENA, 64 * P)]
STATUS, UNDO, ANCHOR, ROWLEAF = 9, 8, 11, 1
AGING_START_BYTES = 19672
FRAME1 = 20992 + 208  # one-page System allocation frame
SYS_PEAK = [2, 114688, 0, 0, 1, 0]
CREATE_PEAK = [9, 425984, 0, 9, 5, 1]
# Profile2 Catalog frame bytes for a 254-byte Catalog cell (stand-in recipe).
BEGIN, RESERVE, FORWARD254, CLR254 = 56712, 86640, 154094, 120238


def sha(b): return hashlib.sha256(b).digest()
def put(b, at, v, w=8): struct.pack_into({1: '<B', 2: '<H', 4: '<I', 8: '<Q'}[w], b, at, v)
def get(b, at, w=8): return int.from_bytes(b[at:at + w], 'little')
def seal(b):
    b[-32:] = sha(b[:-32])
    return b
def parent(name):
    b = (ROOT / name).read_bytes()
    assert sha(b).hex() == PARENTS[name], name
    return b


INITIAL = parent('../base4/initial.xrb')
CELLS = parent('../job3/cells.bin')
PROFILE = parent('../envelopes/profiles.bin')[:4096]
OLD_PROFILE = parent('../../dev3-plan4-recovery-base/vectors/p1/references/profile.bin')


def cell(n): return bytearray(CELLS[n * 1024:(n + 1) * 1024])
def image(b, n): return bytearray(b[ARENA + n * P:ARENA + (n + 1) * P])
def pref(n, inc=2): return struct.pack('<QQQ', FILE, n, inc)
def slot_ref(n, gen=2): return pref(n) + bytes(8) + struct.pack('<Q', gen)  # UndoRef/StatusSlotRef
def txn_id(local): return DB + struct.pack('<QIIQ', 1, 1, 0, local)
def txn_ref(local, gen): return txn_id(local) + struct.pack('<Q', gen)
def work_ref(kind, ident): return DB + struct.pack('<QIIQQ', 1, kind, 0, ident, 1)
def free_job(gen):
    b = bytearray(1024); b[:6] = b'XQJ1\x03\x00'; put(b, 8, gen); return b


def page(n, kind, owner, lsn, cells, inc=2):
    b = bytearray(P); b[:6] = b'XDP1\x01\x00'; b[6] = kind; b[8:24] = DB; b[24:48] = pref(n, inc)
    for at, v in [(48, owner), (56, 1), (64, lsn)]: put(b, at, v)
    put(b, 72, len(cells), 2); put(b, 74, 128 + 4 * len(cells), 2); end = P - 32
    for i, c in enumerate(cells):
        start = end - len(c); put(b, 128 + 4 * i, start, 2); put(b, 130 + 4 * i, len(c), 2)
        b[start:end] = c; end = start
    put(b, 76, end, 2)
    return seal(b)
def cells_of(img):
    return [bytes(img[get(img, 128 + 4 * i, 2):get(img, 128 + 4 * i, 2) + get(img, 130 + 4 * i, 2)])
            for i in range(get(img, 72, 2))]
def relsn(img, lsn, cells=None):
    b = bytearray(img); put(b, 64, lsn)
    if cells is not None:
        n = get(b, 32); kind = b[6]; owner = get(b, 48); inc = get(b, 40)
        return page(n, kind, owner, lsn, cells, inc)
    return seal(b)


# ---- record encoders -------------------------------------------------------
def status_home(core, incoming=0):
    b = bytearray(224); b[:6] = b'XSH1\x01\x00'; put(b, 6, 32, 2); put(b, 8, incoming); b[32:] = core; return b
def free_core(gen):
    b = bytearray(192); b[:6] = b'XTS1\x01\x00'; put(b, 48, gen); return b
def active_core(ref, head=None):
    b = bytearray(192); b[:6] = b'XTS1\x01\x00'; b[6] = 1; b[8:56] = ref
    if head: b[64] = 1; b[72:112] = head
    return b
def free_undo(gen):
    b = bytearray(432); b[:6] = b'XUR1\x01\x00'; put(b, 8, gen); return b
def live_undo(gen, seq, creator, mutation, row, previous=None):
    b = bytearray(432); b[:6] = b'XUR1\x01\x00'; b[6] = 1; put(b, 8, gen); put(b, 16, seq)
    b[24:72] = creator; put(b, 72, mutation); put(b, 80, 2)
    b[88] = 1; put(b, 96, 6); put(b, 104, 1); put(b, 112, row)
    if previous:
        b[326] = 1; b[120:160] = previous[0]; put(b, 160, previous[1])
    return b
def schema(name): return struct.pack('<HBB', 1, 1, 0) + name + struct.pack('<HBB', 1, 2, 0) + b'i'
def catalog_record(creator, q, obj=6, anchor=10, stamp=None, undo=None, name=b'T'):
    s = schema(name)
    b = bytearray(232 + len(s)); b[:8] = b'XCT1\x01\x00\x01\x00'
    for at, v in [(8, obj), (16, 1), (24, 1), (128, 1)]: put(b, at, v)
    if stamp:
        b[7] = 2; b[32] = 2; b[40:88] = stamp; b[136:176] = undo
    else:
        b[32] = 1; put(b, 40, q)
    b[88:128] = creator; b[176:200] = pref(anchor); put(b, 224, len(s), 4); b[232:] = s
    return b
def catalog_undo(gen, seq, creator, obj):
    b = bytearray(432); b[:6] = b'XUR1\x01\x00'; b[6] = 1; put(b, 8, gen); put(b, 16, seq)
    b[24:72] = creator; put(b, 72, 1); put(b, 80, 2)
    b[88] = 3; put(b, 96, 5); put(b, 104, 1); put(b, 112, obj)
    return b
def catalog_projection(obj, value):
    b = bytearray(48); b[:8] = b'XCP1\x01\x00\x03\x00'; b[8] = 3
    for at, v in [(16, 5), (24, 1), (32, obj)]: put(b, at, v)
    b[40] = 1
    return sha(bytes(b) + value)
def row_record(row, writer, mutation, value, stamp_ref=None, undo=None, q=0):
    b = bytearray(181); b[:6] = b'XRW1\x01\x00'; b[7] = 1; put(b, 8, row)
    if stamp_ref:
        b[6] = 2; b[16] = 2; b[24:72] = stamp_ref; b[120:160] = undo
    else:
        b[16] = 1; put(b, 24, q)
    b[72:112] = writer; put(b, 112, mutation); put(b, 160, 13, 4)
    b[168:] = struct.pack('<IBq', 1, 2, value)
    return b
def entry(key, value): return struct.pack('<HH', 8, len(value)) + struct.pack('>Q', key) + value
def projection(row, value):
    b = bytearray(48); b[:8] = b'XRP1\x01\x00\x01\x00'; b[8] = 1
    for at, v in [(16, 6), (24, 1), (32, row)]: put(b, at, v)
    b[40] = 1
    return sha(bytes(b) + value)
META = bytes(cells_of(image(INITIAL, 8))[0])  # leaf metadata, arity0 and no leftmost
def anchor_cell(leaf):
    c = bytearray(48); c[:8] = b'XTA1\x01\x00\x01\x00'; c[8:32] = pref(leaf); put(c, 32, 1); return c
ANCHOR_CELL = anchor_cell(11)


def control_ref(kind, generation, length, digest):
    b = bytearray(64); b[0] = kind; put(b, 8, generation); put(b, 16, length, 4); b[32:64] = digest; return b
PROFILE_REF = control_ref(1, 1, 4096, sha(PROFILE))
ROW_PEAK = [9, 425984, 0, 16, 5, 1]


def debt(steps, wal, undo=0):
    return dict(steps=steps, wal=wal, undo=undo) if steps else None
def budget_rows(rows):
    b = bytearray(640)
    for i in range(5):
        b[i * 128] = i + 1
        r = rows.get(i)
        if not r: continue
        put(b, i * 128 + 8, r['steps']); put(b, i * 128 + 16, r['wal'])
        put(b, i * 128 + 40, r.get('pages', 0)); put(b, i * 128 + 48, r.get('undo', 0))
        for k, v in enumerate(r.get('peak', ROW_PEAK)): put(b, i * 128 + 64 + 8 * k, v)
        put(b, i * 128 + 112, r.get('protected', r['wal']))
    return b
def add_rows(a, b):
    out = bytearray(a)
    for i in range(5):
        for at in range(8, 120, 8):
            o = i * 128 + at
            put(out, o, max(get(a, o), get(b, o)) if 64 <= at < 112 else get(a, o) + get(b, o))
    return out
def descriptor(tag, owner_kind, owner, chain, rows, *, core=None, progress=None, work=None,
               change=0, last_mutation=0, consumed=0, minimum=None, items=0, digest=None):
    apply, previous, birth, groups = chain
    b = bytearray(1536); b[:6] = b'XOD1\x01\x00'; b[6] = tag; put(b, 8, 1536, 4); b[16] = owner_kind
    b[24:72] = owner
    for at, v in [(72, apply), (80, previous), (88, birth), (544, change), (608, consumed),
                  (624, groups), (1336, birth if minimum is None else minimum), (1344, last_mutation)]:
        put(b, at, v)
    b[96:160] = PROFILE_REF; b[560:592] = digest or sha(b''); put(b, 552, items)
    if core: b[160:352] = core
    if progress:
        kind, phase, sequence, cursor, limit = progress
        b[352] = kind; b[353] = phase
        for at, v in [(360, sequence), (368, cursor), (376, limit)]: put(b, at, v)
        b[448:480] = sha(b'')
    if work: b[384:480] = work
    b[640:1280] = rows
    for k in range(6): put(b, 1280 + 8 * k, max(get(rows, i * 128 + 64 + 8 * k) for i in range(5)))
    put(b, 1328, sum(get(rows, i * 128 + 112) for i in range(5)))
    return seal(b)
def aging_descriptor(root_page, root, apply, previous, birth):
    work = bytearray(96); work[0] = 7; put(work, 8, get(root, 64)); put(work, 16, P, 4)
    work[32:64] = sha(root_page); work[64:96] = sha(root)
    groups = get(root, 64) - 1
    rows = bytearray(640)
    for i in range(5): rows[i * 128] = i + 1
    rows[3 * 128:4 * 128] = AGGREGATE[0][3 * 128:4 * 128]
    return descriptor(5, 2, root[16:64], (apply, previous, birth, groups), rows,
                      progress=(3, 2, get(root, 64), get(root, 72), 64), work=work,
                      change=birth, consumed=get(root, 416))
AGGREGATE = [None]


def xoi_envelope(kind, length, generation, capture):
    b = bytearray(length); b[:6] = b'XOI1\x01\x00'; b[6] = kind; b[7] = 2
    put(b, 8, 128, 2); put(b, 10, 96, 2); put(b, 12, length, 4); b[16:32] = DB; put(b, 32, 1)
    put(b, 40, generation); put(b, 48, capture)
    return b
def inventory_root(generation, capture, owners, items=()):
    """Whole XOI1 bank page: root at 0, the single chunk (if any) at 2176."""
    rows = b''.join(bytes([kind]) + bytes(7) + ref for kind, ref in owners)
    chunk = b''
    if items:
        c = xoi_envelope(2, 160 + 96 * len(items), generation, capture)
        put(c, 64, 1); put(c, 80, len(items), 4); c[88:120] = sha(rows)
        for i, item in enumerate(items): c[128 + 96 * i:224 + 96 * i] = item
        chunk = bytes(seal(c))
    b = xoi_envelope(1, 160 + len(rows), generation, capture)
    put(b, 56, len(owners), 2); put(b, 64, int(bool(items))); put(b, 72, len(items))
    put(b, 80, len(chunk)); b[88:120] = sha(chunk); b[128:128 + len(rows)] = rows
    page = bytearray(P); page[:len(b)] = seal(b); page[2176:2176 + len(chunk)] = chunk
    return page
def xoi_item(kind, category, owner, ref, lsn):
    b = bytearray(96); b[0] = kind; b[1] = category; put(b, 2, owner, 2); b[8:8 + len(ref)] = ref
    put(b, 64, 1); put(b, 72, lsn)
    return b
def normalized(items): return sha(b''.join(bytes(i[:2]) + bytes(2) + bytes(i[4:]) for i in items))


def init_record(img, role, source_kind, init, reserve, source, frame, *, pair=None, obj=6):
    p = bytearray(352); p[:5] = bytes([1, role, img[6], 0, source_kind]); p[8:48] = img[24:64]
    put(p, 48, init); put(p, 56, 1, 4); p[64:96] = frame; p[96:128] = sha(img)
    p[128:176] = source; put(p, 176, reserve)
    if pair:
        for at, v in [(184, obj), (192, 1), (200, 1)]: put(p, at, v)
        p[208:232] = pref(pair[0]); p[232:256] = pref(pair[1])
    return p


def assemble(header, images, inits, descriptors, inventory, allocated, reserved=None):
    b = bytearray(INITIAL)
    for at, v, w in header: put(b, at, v, w)
    allocator = images.setdefault(0, image(INITIAL, 0))
    start = get(allocator, 128, 2) + 64
    for n, img in allocated.items():
        a = start + n * 96; allocator[a:a + 96] = bytes(96); allocator[a] = 2; allocator[a + 1] = img[6]
        put(allocator, a + 8, 2); put(allocator, a + 16, get(img, 48)); put(allocator, a + 24, 1)
    for n, (tag, claimant) in (reserved or {}).items():
        a = start + n * 96; allocator[a:a + 96] = bytes(96); allocator[a] = 1; allocator[a + 2] = tag
        put(allocator, a + 8, 2); allocator[a + 32:a + 80] = claimant
    seal(allocator)
    for n, img in images.items():
        b[ARENA + n * P:ARENA + (n + 1) * P] = img
        a = CERTS + n * 96; b[a:a + 96] = bytes(96); b[a:a + 3] = bytes([1, img[6], 0])
        b[a + 8:a + 56] = img[24:72]; b[a + 56:a + 88] = sha(img)
    for n, p in inits.items(): b[INITS + n * 352:INITS + (n + 1) * 352] = p
    for slot, d in descriptors.items(): b[BANK + slot * 1536:BANK + (slot + 1) * 1536] = d
    b[INVENTORY:INVENTORY + P] = inventory
    for i, (at, size) in enumerate(SECTIONS): b[1280 + 64 * i + 32:1280 + 64 * i + 64] = sha(b[at:at + size])
    return seal(b)


def root_page(root, lsn):
    return relsn(image(INITIAL, 1), lsn, [bytes(root)])
def initial_root():
    img = image(INITIAL, 1); return bytearray(cells_of(img)[0])


def genesis():
    return assemble([], {}, {}, {}, inventory_root(1, 1, []), {})


def aging(genesis_digest):
    root = initial_root(); root[6] = 1; root[16:64] = work_ref(3, 1)
    for at, v in [(64, 2), (80, 2), (416, AGING_START_BYTES)]: put(root, at, v)
    rp = root_page(root, 2)
    AGGREGATE[0] = budget_rows({})
    aging_d = aging_descriptor(rp, root, 2, 0, 2)
    header = [(88, 2, 8), (96, 2, 8), (248, 2, 8), (104, 1, 8), (224, 1, 8), (260, 1, 4),
              (288, 2, 8), (296, 1, 8)]
    b = assemble(header, {1: rp}, {}, {34: aging_d}, inventory_root(2, 2, [(2, work_ref(3, 1))]), {})
    b[112:144] = genesis_digest
    return reheader(b)


def reheader(b):
    for i, (at, size) in enumerate(SECTIONS): b[1280 + 64 * i + 32:1280 + 64 * i + 64] = sha(b[at:at + size])
    return seal(b)


LIVE = dict(undo_high=4, f_row=1, f_q=2, f_local=2, free_undo=3, status14_digest=None,
            last_mutation=2, att=True, aging=True, inventory_gen=2, catalog_terminal=False,
            reserved=False, slot32='finished', system=False, system_terminal=0, system_owner=1,
            create=None, undo15_lsn=36, item_order=(0, 1, 2), projection=None)


def live(genesis_digest, **changes):
    v = dict(LIVE, **changes)
    a_ref = txn_ref(3, 2); c_ref = txn_ref(1, 2); f_id = txn_id(v['f_local'])
    u12, u13 = slot_ref(12), slot_ref(13)
    # D Begins on Status14 (Free3 -> generation 4) at 35, Reserves Undo15
    # (generation 4) plus pages 16/17 at 36 and, if Retained, Forwards at 37.
    create = v['create']
    d_ref = txn_ref(4, 4); u15 = slot_ref(15, 4)
    apply, revision = {None: (32, 20), 'reserved': (36, 22), 'retained': (37, 23)}[create]
    capture = 35 if v['system'] else {None: 34, 'reserved': 36, 'retained': 37}[create]
    rec7 = catalog_record(d_ref[:40], 0, obj=7, anchor=16, stamp=d_ref, undo=u15, name=b'U')
    # Pages: C's Status/Undo reused by F; A's own Status/Undo; C's CREATE pair.
    rows = [
        (v['f_row'], row_record(v['f_row'], f_id, 1, 101, q=v['f_q'])),
        (2, row_record(2, txn_id(3), 1, 102, a_ref, u12)),
        (3, row_record(3, txn_id(3), 2, 103, a_ref, u13)),
    ]
    rows.sort()
    catalog = [META, entry(6, catalog_record(c_ref[:40], 1))]
    images = {
        8: relsn(image(INITIAL, 8), 11, catalog),
        9: page(9, STATUS, 2, 32, [status_home(active_core(a_ref, u13), 2)]),
        10: page(10, ANCHOR, 6, 8, [ANCHOR_CELL]),
        11: page(11, ROWLEAF, 6, 32, [META] + [entry(k, r) for k, r in rows]),
        12: page(12, UNDO, 3, 28, [live_undo(2, 3, a_ref, 1, 2)]),
        13: page(13, UNDO, 3, 32, [live_undo(2, 4, a_ref, 2, 3, (u12, 3))]),
        14: page(14, STATUS, 2, 20, [status_home(free_core(3))]),
        15: page(15, UNDO, 3, 19, [free_undo(v['free_undo'])]),
    }
    if create == 'reserved':
        images[14] = page(14, STATUS, 2, 35, [status_home(active_core(d_ref))])
        images[15] = page(15, UNDO, 3, v['undo15_lsn'], [free_undo(4)])
    elif create == 'retained':
        images[8] = relsn(image(INITIAL, 8), 37, catalog + [entry(7, rec7)])
        images[14] = page(14, STATUS, 2, 37, [status_home(active_core(d_ref, u15), 1)])
        images[15] = page(15, UNDO, 3, 37, [catalog_undo(4, 5, d_ref, 7)])
        images[16] = page(16, ANCHOR, 7, 37, [anchor_cell(17)])
        images[17] = page(17, ROWLEAF, 7, 37, [META])
    initial = {
        9: page(9, STATUS, 2, 23, [status_home(free_core(1))]),
        10: page(10, ANCHOR, 6, 8, [ANCHOR_CELL]),
        11: page(11, ROWLEAF, 6, 8, [META]),
        12: page(12, UNDO, 3, 26, [free_undo(1)]),
        13: page(13, UNDO, 3, 30, [free_undo(1)]),
        14: page(14, STATUS, 2, 2, [status_home(free_core(1))]),
        15: page(15, UNDO, 3, 5, [free_undo(1)]),
    }
    create_frame = sha(b'opaque CREATE INIT frame')
    inits = {}
    for n, ident, init in [(14, 1, 2), (15, 2, 5), (9, 3, 23), (12, 4, 26), (13, 5, 30)]:
        inits[n] = init_record(initial[n], 1 if initial[n][6] == STATUS else 2, 1, init, init - 1,
                               work_ref(1, ident), sha(b'opaque System INIT frame' + bytes([n])))
    for n, role in [(10, 3), (11, 4)]:
        inits[n] = init_record(initial[n], role, 2, 8, 7, c_ref, create_frame, pair=(10, 11))
    if create == 'retained':
        d_frame = sha(b'opaque D CREATE INIT frame')
        for n, role in [(16, 3), (17, 4)]:
            inits[n] = init_record(images[n], role, 2, 37, 36, d_ref, d_frame, pair=(16, 17), obj=7)
    if v['status14_digest']:
        inits[14][96:128] = v['status14_digest']
    # Jobs: A's Terminal and two Retained roots; C/F used ordinals 3..6.
    bank = [free_job(1) for _ in range(64)]
    for o in range(3, 7): bank[o] = free_job(2)
    t = cell(4); t[16:64] = a_ref; t[80:120] = slot_ref(9); put(t, 64, 24); put(t, 376, 24); bank[0] = t
    for o, (lease, undo, seq, mutation, row, h) in enumerate([(27, u12, 3, 1, 2, 0), (31, u13, 4, 2, 3, 1)], 1):
        reserved = v['reserved'] and o == 2
        j = cell(19 if reserved else 21); j[16:64] = a_ref; j[80:120] = slot_ref(9); j[128:168] = undo
        j[200:224] = pref(10); j[224:248] = pref(11)
        value = dict(rows)[row]
        for at, x in [(64, lease), (168, seq), (248, mutation), (256, mutation), (320, row)]: put(j, at, x)
        put(j, 280, len(value) + 12, 4)
        wal = 311608 + 2 * (len(value) + 12) + 128 * h
        put(j, 384 + 3 * 128 + 16, wal); put(j, 384 + 3 * 128 + 112, wal)
        if reserved:
            put(j, 376, lease)
        else:
            put(j, 72, lease + 1); put(j, 376, lease + 1); j[288:320] = projection(row, value)
        bank[o] = j
    leased = (1 << 0) | ((1 << 2) if v['reserved'] else 0)
    if v['catalog_terminal']:
        d = cell(1); d[16:64] = txn_ref(4, 2); d[80:120] = slot_ref(16); put(d, 8, 3)
        put(d, 64, 30); put(d, 376, 30); bank[3] = d; leased |= 1 << 3
    ready = ((1 << 1) | (1 << 2)) & ~leased
    if create:
        # Catalog-scope D: Terminal at ordinal 7, CREATE root at ordinal 8.
        t = cell(1); t[16:64] = d_ref; t[80:120] = slot_ref(14, 4); put(t, 64, 35); put(t, 376, 35)
        bank[7] = t
        r = cell(7 if create == 'reserved' else 9); r[16:64] = d_ref; r[80:120] = slot_ref(14, 4)
        r[128:168] = u15; r[200:224] = pref(16); r[224:248] = pref(17)
        for at, x in [(64, 36), (168, 5), (176, 7)]: put(r, at, x)
        r[288:320] = v['projection'] or catalog_projection(7, rec7)
        if create == 'reserved':
            put(r, 376, 36); leased |= (1 << 7) | (1 << 8)
        else:
            put(r, 72, 37); put(r, 376, 37); leased |= 1 << 7; ready |= 1 << 8
        bank[8] = r
    occupied = 3 + int(v['catalog_terminal']) + 2 * int(bool(create))
    free = ((1 << 64) - 1) & ~(leased | ready)
    aggregate = budget_rows({})
    for j in bank:
        for i in range(5):
            for at in range(8, 120, 8):
                o = i * 128 + at; x = get(j, 384 + o)
                put(aggregate, o, max(get(aggregate, o), x) if 64 <= at < 112 else get(aggregate, o) + x)
    AGGREGATE[0] = aggregate
    jobs_page = relsn(image(INITIAL, 2), apply, [bytes(c) for c in bank[:15]])
    root = initial_root(); root[6] = 1; root[16:64] = work_ref(3, 1)
    undo_high = v['undo_high'] + int(bool(create))
    for at, x in [(64, revision), (72, 7), (80, apply), (88, 4 + occupied), (96, free), (104, leased),
                  (112, ready), (376, 4), (384, 0), (392, undo_high), (400, 6 + int(bool(create))),
                  (408, 2), (416, 120000), (424, 0), (432, 3)]:
        put(root, at, x)
    root[128:160] = sha(b''.join(bank)); root[160:192] = sha(aggregate)
    rp = root_page(root, apply)
    images.update({1: rp, 2: jobs_page})
    # Descriptors: A in ATT slot0, D in slot1, the last System and Aging.
    a_rows = budget_rows({0: debt(2, (91096 + 193) + (91096 + 193 + 128)), 2: debt(1, 56712),
                          3: debt(3, 9120)})
    remaining = add_rows(a_rows, aggregate)
    descriptors = {}
    if v['slot32'] == 'finished':
        # Exact original Finished UndoRoot INIT terminal: no remainder retained.
        descriptors[32] = descriptor(6, 2, work_ref(1, 5), (30, 29, 29, 2), budget_rows({}),
                                     progress=(1, 4, 1, 1, 1), change=30,
                                     consumed=FRAME1 + 38208, minimum=0)
    else:
        descriptors[32] = descriptor(6, 2, work_ref(1, 5), (30, 29, 29, 2), budget_rows({}),
                                     progress=(1, 4, 1, 1, 1), change=30)
    owners = [(1, a_ref)]
    items = []
    reserved_entries = {}
    if create:
        owners.append((1, d_ref))
        d_debt = lambda wal, **x: dict(steps=1, wal=wal, peak=CREATE_PEAK, **x)
        if create == 'reserved':
            items = [xoi_item(1, 2, 1, pref(16), 36), xoi_item(1, 2, 1, pref(17), 36),
                     xoi_item(2, 2, 1, u15, 36)]
            d_rows = budget_rows({0: d_debt(CLR254), 1: d_debt(FORWARD254, pages=2, undo=1),
                                  2: d_debt(56712), 3: d_debt(3040)})
            descriptors[1] = descriptor(1, 1, d_ref, (36, 35, 35, 2), d_rows, core=active_core(d_ref),
                                        change=36, consumed=BEGIN + RESERVE, items=3,
                                        digest=normalized(items))
            items = [items[i] for i in v['item_order']]
            reserved_entries = {16: (1, d_ref), 17: (1, d_ref)}
        else:
            d_rows = budget_rows({0: d_debt(CLR254), 2: d_debt(56712), 3: d_debt(3040)})
            descriptors[1] = descriptor(1, 1, d_ref, (37, 36, 35, 3), d_rows,
                                        core=active_core(d_ref, u15), change=37,
                                        consumed=BEGIN + RESERVE + FORWARD254, last_mutation=1)
        remaining = add_rows(remaining, d_rows)
    if v['system']:
        # Prepared one-page System reservation of page16, born at capture.
        work = work_ref(1, 6); items = [xoi_item(1, 3, v['system_owner'], pref(16), 35)]
        s_rows = budget_rows({2: dict(steps=1, wal=38208, pages=1, peak=SYS_PEAK,
                                      protected=38208 + 2 * P)})
        descriptors[32] = descriptor(5, 2, work, (35, 0, 35, 1), s_rows, progress=(1, 1, 1, 0, 1),
                                     change=35, consumed=FRAME1, items=1,
                                     digest=normalized(items))
        owners.append((2, work)); reserved_entries = {16: (2, work)}
        remaining = add_rows(remaining, s_rows)
    owners.append((2, work_ref(3, 1)))
    if v['att']:
        descriptors[0] = descriptor(1, 1, a_ref, (32, 31, 24, 5), a_rows,
                                    core=active_core(a_ref, u13), change=32,
                                    last_mutation=v['last_mutation'])
    if v['aging']:
        descriptors[34] = aging_descriptor(rp, root, apply, 31 if apply == 32 else 32, 3)
    allocated = {n: images[n] for n in range(9, 18) if n in images}
    images[0] = relsn(image(INITIAL, 0), capture if v['system'] or create else 30)
    active = 1 + int(v['catalog_terminal']) + int(bool(create))
    header = [(88, 2, 8), (96, capture, 8), (248, capture, 8), (104, 1, 8), (152, 65536, 8),
              (192, 4 if v['catalog_terminal'] or create else 3, 8),
              (208, 6 if v['system'] else 5, 8), (224, 1, 8),
              (256, active, 4), (260, 1 + int(v['system']), 4), (264, (1 << 9) | (1 << 14), 8),
              (272, leased, 8), (280, ready, 8), (288, revision, 8), (296, revision - 1, 8),
              (304, v['system_terminal'] if v['system'] else 3, 1)]
    b = assemble(header, images, inits, descriptors,
                 inventory_root(v['inventory_gen'], capture, owners, items), allocated,
                 reserved_entries)
    b[512:1152] = remaining
    # Allocation must be sealed after the arena images it describes.
    b[112:144] = genesis_digest
    return reheader(b)


def patch_header_page(b, n, mutate):
    img = image(b, n); mutate(img); seal(img)
    b[ARENA + n * P:ARENA + (n + 1) * P] = img
    b[CERTS + n * 96 + 56:CERTS + n * 96 + 88] = sha(img)
    return reheader(b)


def runs(parent_bytes, child):
    """Byte runs where child differs, merging gaps shorter than 16 bytes."""
    out = []
    for block in range(0, BASE, 4096):
        if parent_bytes[block:block + 4096] == child[block:block + 4096]:
            continue
        for i in range(block, block + 4096):
            if parent_bytes[i] == child[i]:
                continue
            if out and i - (out[-1][0] + out[-1][1]) < 16:
                out[-1][1] = i + 1 - out[-1][0]
            else:
                out.append([i, 1])
    return [(at, bytes(child[at:at + length])) for at, length in out]


def main():
    g = genesis(); gd = sha(g)
    a = aging(gd)
    l = live(gd)
    bases = {'genesis': g, 'aging': a, 'live': l}
    variants = []
    for version in [0, 1, 2, 3, 5, 65535]:
        b = bytearray(g); put(b, 4, version, 2); variants.append((f'base-version-{version}', 'genesis', seal(b)))
    for version in [2, 4]:
        def root_version(img, version=version):
            at = get(img, 128, 2); put(img, at + 4, version, 2)
        variants.append((f'root-version-{version}', 'genesis', patch_header_page(bytearray(g), 1, root_version)))
        def job_version(img, version=version):
            at = get(img, 128, 2); put(img, at + 4, version, 2)
        variants.append((f'job-version-{version}', 'genesis', patch_header_page(bytearray(g), 2, job_version)))
    b = bytearray(g); b[304] = 1; variants.append(('terminal-without-system', 'genesis', reheader(b)))
    b = bytearray(a); r = budget_rows({3: debt(1, 1)}); b[512:1152] = r
    variants.append(('aging-header-budget', 'aging', reheader(b)))
    for name, change in [
        ('issuer-undo-high', dict(undo_high=5)),
        ('row-high-global', dict(f_row=4)),
        ('committed-q-collision', dict(f_q=1)),
        ('catalog-row-mixing', dict(f_local=1)),
        ('free-undo-generation', dict(free_undo=6)),
        ('init-initial-digest', dict(status14_digest=sha(b'not the canonical initial Status page'))),
        ('descriptor-last-mutation', dict(last_mutation=1)),
        ('att-missing', dict(att=False)),
        ('inventory-generation', dict(inventory_gen=1)),
        ('aging-descriptor-missing', dict(aging=False)),
        ('catalog-scope-terminal', dict(catalog_terminal=True)),
        ('reserved-row-root', dict(reserved=True)),
        ('allocation-finished-noncanonical', dict(slot32='noncanonical')),
        ('state-system-reserved', dict(system=True)),
        ('system-reserved-terminal', dict(system=True, system_terminal=3)),
        ('system-item-owner', dict(system=True, system_owner=0)),
        ('state-create-reserved', dict(create='reserved')),
        ('create-undo-lsn', dict(create='reserved', undo15_lsn=35)),
        ('create-item-order', dict(create='reserved', item_order=(1, 0, 2))),
        ('state-create-retained', dict(create='retained')),
        ('retained-projection', dict(create='retained', projection=sha(b'not the CREATE projection'))),
    ]:
        variants.append((name, 'live', live(gd, **change)))
    for name, data in bases.items():
        (ROOT / f'{name}.xrb').write_bytes(data)
    names = list(bases)
    blob = bytearray()
    index = []
    for name, base, data in variants:
        diff = runs(bases[base], data)
        encoded = name.encode()
        blob += struct.pack('<BB', names.index(base), len(encoded)) + encoded + struct.pack('<I', len(diff))
        for at, chunk in diff:
            blob += struct.pack('<II', at, len(chunk)) + chunk
        index.append(dict(name=name, parent=base, runs=len(diff), sha256=sha(data).hex()))
    (ROOT / 'variants.bin').write_bytes(blob)
    envelopes = bytearray()
    for version in [0, 1, 2, 4, 65535]:
        if version == 1:
            envelopes += OLD_PROFILE
        else:
            e = bytearray(PROFILE); put(e, 4, version, 2); envelopes += seal(e)
    (ROOT / 'envelopes.bin').write_bytes(envelopes)
    (ROOT / 'index.json').write_text(json.dumps(dict(bases=names, variants=index,
                                                     envelopes=[0, 1, 2, 4, 65535]), indent=1) + '\n')


if __name__ == '__main__':
    main()
