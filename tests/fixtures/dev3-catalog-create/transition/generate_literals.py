#!/usr/bin/env python3
"""D3-CREATE-TRANSITION-G1: independent, write-once semantic INPUT literals.

No Rust, product encoder, decoder, test binary, or observed product output is
imported or executed. The closed phase tables below are the oracle; serializers
implement the published fixed offsets. These bytes convey no runtime authority.
"""
import copy
import hashlib
import json
from pathlib import Path
import struct
import sys
import time
import traceback

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
DB = bytes(range(1, 17))
FILE = 0x100000001
PAGE = 16384
PEAK = (9, 425984, 0, 9, 5, 1)
HANDOFF = (1, 2, 4, 6, 7, 11, 12)
ROLES = {1: (1, 2, 4), 2: tuple(range(8)), 3: (4,), 4: (0, 1, 2, 3, 4, 5),
         6: (1, 2, 4), 7: (1, 2, 4), 11: (0, 1, 2, 5), 12: (0, 1, 2),
         16: (1,), 17: (1, 2, 4), 20: (1, 2, 3, 4, 5), 21: (1, 2, 5), 22: (1, 2, 5)}
NATIVE_KIND = (6, 8, 9, 7, 5, 3, 10, 11)
# Each row declares the complete logical phase; repeated release phases are
# separate original groups, never a synthesized cursor jump.
COMMIT = ((1, 'begin'), (11, 'reserve'), (2, 'forward'), (6, 'commit'),
          (20, 'prune'), (21, 'release1'), (21, 'release2'), (21, 'release3'),
          (22, 'rootfree'), (17, 'finish'))
ROLLBACK = ((1, 'begin'), (11, 'reserve'), (2, 'forward'), (3, 'rollback'),
            (4, 'clr'), (7, 'abort'), (21, 'release1'), (21, 'release2'),
            (21, 'release3'), (22, 'rootfree'), (17, 'finish'))
CANCEL = ((1, 'begin'), (11, 'reserve'), (12, 'cancel'), (3, 'emptyrollback'),
          (7, 'abort'), (17, 'finish'))


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def sha(raw):
    return hashlib.sha256(raw).digest()


def put(raw, at, value, width=8):
    need(0 <= value < 1 << (8 * width), ('integer range', at, value, width))
    struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[width], raw, at, value)


def get(raw, at, width=8):
    return int.from_bytes(raw[at:at + width], 'little')


def ref(number, incarnation=2):
    return struct.pack('<QQQ', FILE, number, incarnation)


def slot(number, generation, incarnation=2):
    return ref(number, incarnation) + struct.pack('<H6xQ', 0, generation)


def actor(local=1, generation=2):
    return DB + struct.pack('<QI4xQQ', 1, 1, local, generation)


def work():
    return DB + struct.pack('<QB7xQQ', 1, 3, 1, 1)


def seal(raw):
    raw[-32:] = sha(raw[:-32])
    return bytes(raw)


def profile():
    raw = bytearray(4096)
    raw[:4] = b'XDC1'; put(raw, 4, 2, 2); raw[6] = 1
    put(raw, 8, 128, 2); put(raw, 12, 4096, 4); raw[16:32] = DB
    put(raw, 32, 1); put(raw, 40, 1); put(raw, 56, 3936, 4)
    for at, value in ((8, 8 << 30), (16, 1 << 30), (24, 256 << 20),
                      (32, 16 << 20), (40, 4 << 20), (48, 8 << 20),
                      (56, 1 << 20), (64, 8 << 20), (72, 64 << 20),
                      (80, 16 << 20), (88, 4 << 20), (96, 12 << 20),
                      (104, 4 << 20), (160, 2113536), (168, 2097152)):
        put(raw, 128 + at, value)
    for at, value in ((0, 1), (112, 32), (116, 256), (120, 1 << 20),
                      (124, 32), (128, 128), (136, 16384), (148, 8192),
                      (156, 16384), (176, 2), (180, 4096), (184, 1 << 20)):
        put(raw, 128 + at, value, 4)
    for at, value in ((132, 16), (134, 8), (140, 64), (142, 64), (144, 1010), (152, 64)):
        put(raw, 128 + at, value, 2)
    raw = seal(raw)
    need(sha(raw).hex() == 'b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24', 'Profile2 literal')
    return raw


def page(number, kind, owner, lsn, cells, incarnation=1):
    raw = bytearray(PAGE)
    raw[:4] = b'XDP1'; put(raw, 4, 1, 2); raw[6] = kind; raw[8:24] = DB
    raw[24:48] = ref(number, incarnation)
    for at, value in ((48, owner), (56, 1), (64, lsn)):
        put(raw, at, value)
    put(raw, 72, len(cells), 2); put(raw, 74, 128 + 4 * len(cells), 2)
    end = 16352
    for index, cell in enumerate(cells):
        start = end - len(cell)
        need(start >= 128 + 4 * len(cells), 'page body bound')
        put(raw, 128 + 4 * index, start, 2); put(raw, 130 + 4 * index, len(cell), 2)
        raw[start:end] = cell; end = start
    put(raw, 76, end, 2)
    return seal(raw)


def cell_at(raw, index):
    at, size = get(raw, 128 + 4 * index, 2), get(raw, 130 + 4 * index, 2)
    return raw[at:at + size], at


def replace_cell(raw, index, value):
    old, at = cell_at(raw, index)
    need(len(old) == len(value), 'same-length mutant')
    result = bytearray(raw); result[at:at + len(old)] = value
    return seal(result)


def budgets(active=None):
    raw = bytearray(640)
    for row in range(5):
        raw[row * 128] = row + 1
    for row, (steps, wal, pages, undos) in (active or {}).items():
        values = (steps, wal, 0, 0, pages, undos, 0, *PEAK, wal)
        for index, value in enumerate(values):
            put(raw, row * 128 + 8 + index * 8, value)
    return bytes(raw)


def schema(maximum=False, name='T'):
    name = b'T' * 128 if maximum else name.encode()
    columns = [f'c{i:02}'.encode() + b'x' * 125 for i in range(32)] if maximum else [b'i']
    need(len(set(columns)) == len(columns), 'unique column literals')
    return struct.pack('<HBB', len(name), len(columns), 0) + name + b''.join(
        struct.pack('<HBB', len(col), 2, 0) + col for col in columns)


def catalog_record(p, committed=False):
    body = schema(p['maximum'], p['name'])
    raw = bytearray(232 + len(body))
    raw[:4] = b'XCT1'; put(raw, 4, 1, 2); raw[6:8] = bytes((1, 0 if committed else 2))
    for at, value in ((8, p['object']), (16, 1), (24, 1), (128, 1)):
        put(raw, at, value)
    raw[32] = 1 if committed else 2
    if committed:
        put(raw, 40, p['q'])
    else:
        raw[40:88] = actor(p['local'], p['generation'])
        raw[136:176] = slot(p['undo'], p['ug'])
    raw[88:128] = actor(p['local'], p['generation'])[:40]
    raw[176:200] = ref(p['pair'][0], p['inc'])
    put(raw, 224, len(body), 4); raw[232:] = body
    return bytes(raw)


def tree_cell(value, object_id):
    return struct.pack('<HH', 8, len(value)) + struct.pack('>Q', object_id) + value


def projection(object_id, value=None):
    raw = bytearray(48); raw[:4] = b'XCP1'; put(raw, 4, 1, 2); put(raw, 6, 3, 2)
    raw[8] = 3
    for at, value_ in ((16, 5), (24, 1), (32, object_id)):
        put(raw, at, value_)
    raw[40] = int(value is not None)
    return bytes(raw) + (value or b'')


def status(p, phase, head=False):
    raw = bytearray(224); raw[:8] = b'XSH1\x01\x00\x20\x00'
    core = bytearray(192); core[:6] = b'XTS1\x01\x00'
    if phase == 'free':
        put(core, 48, p['generation'])
    else:
        core[6] = {'active': 1, 'aborting': 2, 'committed': 3, 'aborted': 4}[phase]
        core[8:56] = actor(p['local'], p['generation'])
        if phase == 'aborting':
            core[7] = 2
        if phase == 'committed':
            put(core, 56, p['q'])
        if head:
            core[64] = 5 if phase == 'aborting' else 1
            core[72:112] = slot(p['undo'], p['ug'])
            if phase == 'aborting':
                core[152:192] = slot(p['undo'], p['ug'])
            put(raw, 8, 1)
    raw[32:] = core
    return bytes(raw)


def undo(p, phase, cursor=0):
    raw = bytearray(432); raw[:6] = b'XUR1\x01\x00'; put(raw, 8, p['ug'])
    if phase != 'free':
        raw[6] = 1 if phase == 'live' else 2
        put(raw, 16, p['sequence']); raw[24:72] = actor(p['local'], p['generation'])
        put(raw, 72, 1); put(raw, 80, 2 if phase == 'live' else 0)
        raw[88] = 3
        for at, value in ((96, 5), (104, 1), (112, p['object'])):
            put(raw, at, value)
        raw[326] = 3 if cursor >= 2 else 0
        raw[327] = 3 if cursor >= 1 else 0
        raw[328] = cursor
    return bytes(raw)


def anchor(root, tree=1, incarnation=2):
    raw = bytearray(48); raw[:6] = b'XTA1\x01\x00'; raw[6] = tree
    raw[8:32] = ref(root, incarnation); put(raw, 32, 1)
    return bytes(raw)


def allocation(entries, lsn):
    raw = bytearray(6208); raw[:6] = b'XAL1\x01\x00'
    put(raw, 6, 64, 2); put(raw, 16, 64, 2); put(raw, 18, 96, 2)
    raw[32:56] = ref(0, 1)
    for index in range(64):
        state, inc, kind, object_id, claimant = entries.get(index, (0, 1, 0, 0, b''))
        at = 64 + 96 * index; raw[at] = state; put(raw, at + 8, inc)
        if state == 2:
            raw[at + 1] = kind; put(raw, at + 16, object_id); put(raw, at + 24, 1)
        elif state == 1:
            raw[at + 2] = 1; raw[at + 32:at + 80] = claimant
    return page(0, 10, 1, lsn, [raw])


def free_job(generation=1):
    raw = bytearray(1024); raw[:6] = b'XQJ1\x02\x00'; put(raw, 8, generation)
    return bytes(raw)


def job(p, kind, phase, generation, lease, lsn, adoption=0, sender=bytes(32), release=0, cursor=0):
    raw = bytearray(free_job(generation)); raw[6] = phase; raw[7] = kind
    raw[16:64] = actor(p['local'], p['generation']); raw[80:120] = slot(p['status'], p['generation'])
    for at, value in ((64, lease), (72, adoption), (376, lsn)):
        put(raw, at, value)
    raw[328:360] = sender
    if kind == 1:
        if phase == 2:
            raw[120] = 1 if p['outcome'] == 'committed' else 2
            put(raw, 264, p['q'] if p['outcome'] == 'committed' else 0)
        steps, cost = 1, 55112
    else:
        raw[128:168] = slot(p['undo'], p['ug'])
        for at, value in ((168, p['sequence']), (176, p['object']), (184, 1), (192, 1), (248, 1), (256, 1)):
            put(raw, at, value)
        if phase < 3:
            raw[200:224] = ref(p['pair'][0], p['inc']); raw[224:248] = ref(p['pair'][1], p['inc'])
            put(raw, 280, p['c'], 4); raw[288:320] = p['projection']
            steps, cost = 5, 311608 + 2 * p['c']
        else:
            raw[121] = cursor; put(raw, 272, release)
            put(raw, 264, p['q'] if phase == 3 else 0)
            steps, cost = 4 - cursor, (4 - cursor) * 55528
    raw[384:] = budgets({3: (steps, cost, 0, 0)})
    return bytes(raw)


def root_page(s):
    masks = [0, 0, 0]; steps = cost = 0
    for index, cell in enumerate(s['jobs']):
        state, kind = cell[6:8]
        masks[0 if state == 0 else 1 if state == 1 else 2] |= 1 << index
        if state:
            steps += get(cell, 384 + 3 * 128 + 8); cost += get(cell, 384 + 3 * 128 + 16)
    aggregate = budgets({3: (steps, cost, 0, 0)} if steps else {})
    raw = bytearray(512); raw[:6] = b'XQR1\x02\x00'; raw[6] = int(s['revision'] > 1)
    put(raw, 8, 512, 4); put(raw, 12, 64, 4)
    if raw[6]:
        raw[16:64] = work()
    for at, value in ((64, s['revision']), (72, s['cursor']), (80, s['root_lsn']),
                      (88, s['issued']), (96, masks[0]), (104, masks[1]), (112, masks[2]),
                      (208, 1), (216, 4), (224, 1), (376, s['completed']),
                      (384, s['canceled']), (392, s['uhigh']), (400, s['ohigh']),
                      (408, s['commit']), (416, s['aging_cost'])):
        put(raw, at, value)
    raw[128:160] = sha(b''.join(s['jobs'])); raw[160:192] = sha(aggregate)
    raw[192:208] = DB; raw[232:256] = ref(1, 1)
    for i in range(5):
        raw[256 + i * 24:280 + i * 24] = ref(2 + i, 1)
    need(s['issued'] == s['completed'] + s['canceled'] + (masks[1] | masks[2]).bit_count(), 'Root conservation')
    return page(1, 12, 4, s['root_lsn'], [raw]), aggregate


def descriptor(owner_bytes, owner_tag, chain, remaining, inventory, consumed, birth, mutation, profile_raw,
               status_cell=None, root=None, revision=0, cursor=0):
    raw = bytearray(1536); raw[:6] = b'XOD1\x01\x00'; raw[6] = owner_tag; put(raw, 8, 1536, 4)
    raw[16:72] = owner_bytes
    for at, value in zip((72, 80, 88, 624), chain):
        put(raw, at, value)
    raw[96] = 1; put(raw, 104, 1); put(raw, 112, 4096, 4); raw[128:160] = sha(profile_raw)
    if status_cell is not None:
        raw[160:352] = status_cell[32:]
    else:
        raw[352:354] = b'\x03\x02'
        for at, value in ((360, revision), (368, cursor), (376, 64), (392, revision)):
            put(raw, at, value)
        raw[384] = 7; put(raw, 400, 16384, 4)
        raw[416:448] = sha(root); raw[448:480] = sha(cell_at(root, 0)[0])
    change, items = inventory
    put(raw, 544, change); put(raw, 552, len(items) // 96); raw[560:592] = sha(items)
    put(raw, 608, consumed); raw[640:1280] = remaining
    for col in range(6):
        put(raw, 1280 + col * 8, max(get(remaining, row * 128 + 64 + col * 8) for row in range(5)))
    put(raw, 1328, sum(get(remaining, row * 128 + 112) for row in range(5)))
    put(raw, 1336, birth); put(raw, 1344, mutation)
    return seal(raw)


def items(p, lease):
    out = bytearray(288)
    for index, value in enumerate((ref(p['pair'][0], p['inc']), ref(p['pair'][1], p['inc']), slot(p['undo'], p['ug']))):
        at = index * 96; out[at:at + 2] = bytes((1 if index < 2 else 2, 2))
        out[at + 8:at + 8 + len(value)] = value
        put(out, at + 64, 1); put(out, at + 72, lease)
    return bytes(out)


def cost(op, c=254):
    return {1: 56712, 2: 153840 + c, 3: 3040, 4: 119984 + c, 6: 56712, 7: 56712,
            11: 86640, 12: 69304, 16: 19672, 17: 55112, 20: 89496 + 2 * c,
            21: 55528, 22: 55528}[op]


def fresh(profile_raw, alternative=False):
    entries = {0: (2, 1, 10, 1, b''), 1: (2, 1, 12, 4, b''),
               **{n: (2, 1, 13, 4, b'') for n in range(2, 7)},
               7: (2, 1, 11, 5, b''), 8: (2, 1, 5, 5, b'')}
    s = dict(jobs=[free_job() for _ in range(64)], revision=1, cursor=0, root_lsn=0,
             issued=0, completed=0, canceled=0, uhigh=0, ohigh=5, commit=0,
             aging_cost=0, entries=entries, tables={}, images={}, tx=None, aging=None,
             aging_chain=(0, 0, 0, 0), profile=profile_raw, last_lsn=0)
    s['images'][0] = allocation(entries, 0)
    s['images'][1] = root_page(s)[0]
    s['images'][2] = page(2, 13, 4, 0, s['jobs'][:15])
    s['images'][3] = page(8, 5, 5, 0, [b'XBT1\x01\x00' + bytes(26)])
    s['anchor'] = page(7, 11, 5, 0, [anchor(8, 5, 1)])
    return s


def setup(s, alternative=False):
    s = copy.deepcopy(s)
    status_n, undo_n, status_lsn, undo_lsn, end = (20, 9, 7, 3, 7) if alternative else (9, 10, 3, 5, 5)
    for n, kind, obj in ((status_n, 9, 2), (undo_n, 8, 3)):
        s['entries'][n] = (2, 2, kind, obj, b'')
    if alternative:
        s['entries'][11] = (2, 2, 9, 2, b'')
        s['external_gap'] = page(11, 9, 2, 5, [status(dict(generation=1), 'free')], 2)
    s['images'][0] = allocation(s['entries'], end)
    s['images'][4] = page(status_n, 9, 2, status_lsn, [status(dict(generation=1), 'free')], 2)
    s['images'][5] = page(undo_n, 8, 3, undo_lsn, [undo(dict(ug=1), 'free')], 2)
    s['last_lsn'] = end
    return s


def plan(local=1, *, canceled=False, maximum=False, alternative=False):
    p = dict(local=local, generation=local + 1, ug=local + 1, sequence=local,
             object=local + 5, q=local, name='T' if local == 1 else 'U', maximum=maximum,
             status=20 if alternative else 9, undo=9 if alternative else 10,
             pair=(10, 12) if alternative else (11, 12) if local == 1 or canceled else (13, 14),
             inc=local + 1 if canceled else 2,
             terminal=0 if local == 1 else 1, root=1 if local == 1 else 2,
             terminal_gen=2 if local == 1 else 3, root_gen=2,
             outcome='aborted' if canceled else 'committed')
    p['c'] = len(catalog_record(p)) + 12
    p['projection'] = sha(projection(p['object'], catalog_record(p)))
    return p


def successor(before, p, op, phase, lsn):
    """Serialize the declared closed phase; no candidate/product bytes read."""
    s = copy.deepcopy(before); s['last_lsn'] = lsn
    if op == 16:
        s['revision'], s['root_lsn'], s['aging_cost'] = 2, lsn, 19672
    elif op != 3:
        s['revision'] += 1; s['root_lsn'] = lsn
        if op not in HANDOFF:
            s['aging_cost'] += cost(op, p['c'])
    terminal, root = p.get('terminal', 0), p.get('root', 1)
    if phase == 'begin':
        s['issued'] += 1; s['begin'] = lsn; s['tx_cost'] = 0; s['tx_groups'] = 0
        s['inv'] = (lsn, b''); s['mutation'] = 0
        s['jobs'][terminal] = job(p, 1, 1, p['terminal_gen'], lsn, lsn)
    if phase == 'reserve':
        s['issued'] += 1; s['ohigh'] = p['object']; s['uhigh'] = p['sequence']; s['reserve'] = lsn
        s['jobs'][root] = job(p, 2, 1, p['root_gen'], lsn, lsn)
        s['inv'] = (lsn, items(p, lsn))
    if phase == 'forward':
        s['forward'] = lsn; s['forward_sender'] = sha(before['tx']); s['mutation'] = 1
        s['jobs'][root] = job(p, 2, 2, p['root_gen'], s['reserve'], lsn, lsn, s['forward_sender'])
        s['inv'] = (lsn, b''); s['tables'][p['object']] = catalog_record(p)
    if phase in ('commit', 'abort'):
        s['jobs'][terminal] = job(p, 1, 2, p['terminal_gen'], s['begin'], lsn, lsn, sha(before['tx']))
        if phase == 'commit':
            s['commit'] = p['q']
    if phase in ('prune', 'clr'):
        s['release'] = lsn; s['disposition'] = 3 if phase == 'prune' else 4
        s['jobs'][root] = job(p, 2, s['disposition'], p['root_gen'], s['reserve'], lsn,
                               s['forward'], s['forward_sender'], lsn)
        if phase == 'prune':
            s['tables'][p['object']] = catalog_record(p, True)
        else:
            del s['tables'][p['object']]
    if phase.startswith('release'):
        cursor = int(phase[-1])
        s['jobs'][root] = job(p, 2, s['disposition'], p['root_gen'], s['reserve'], lsn,
                               s['forward'], s['forward_sender'], s['release'], cursor)
    if phase in ('cancel', 'rootfree'):
        s['jobs'][root] = free_job(p['root_gen'])
        if phase == 'cancel':
            s['canceled'] += 1; s['inv'] = (lsn, b'')
        else:
            s['completed'] += 1
    if phase == 'finish':
        s['jobs'][terminal] = free_job(p['terminal_gen']); s['completed'] += 1
    ordinal = None if op in (16, 3) else terminal if op in (1, 6, 7, 17) else root
    if op not in HANDOFF and op not in (16, 3):
        s['cursor'] = (ordinal + 1) % 64
    if op in (11, 2, 12, 4):
        for index, n in enumerate(p['pair']):
            s['entries'][n] = ((1, p['inc'], 0, 0, actor(p['local'], p['generation'])) if op == 11 else
                               (2, p['inc'], 11 if index == 0 else 1, p['object'], b'') if op == 2 else
                               (0, p['inc'], 0, 0, b''))
        s['images'][0] = allocation(s['entries'], lsn)
    if op in (11, 2, 4, 20, 21, 22):
        uphase = 'free' if op in (11, 22) else 'live' if op == 2 else 'releasing'
        cursor = int(phase[-1]) if op == 21 else 0
        s['images'][5] = page(p['undo'], 8, 3, lsn, [undo(p, uphase, cursor)], 2)
    status_phases = {'begin': ('active', False), 'forward': ('active', True),
                     'commit': ('committed', True), 'prune': ('committed', False),
                     'rollback': ('aborting', True), 'emptyrollback': ('aborting', False),
                     'clr': ('aborting', False), 'abort': ('aborted', False), 'finish': ('free', False)}
    if phase in status_phases:
        s['status_phase'], s['status_head'] = status_phases[phase]
        s['images'][4] = page(p['status'], 9, 2, lsn, [status(p, s['status_phase'], s['status_head'])], 2)
    if op in (2, 4, 20):
        cells = [b'XBT1\x01\x00' + bytes(26)] + [tree_cell(v, obj) for obj, v in sorted(s['tables'].items())]
        s['images'][3] = page(8, 5, 5, lsn, cells)
    if op == 2:
        s['images'][6] = page(p['pair'][0], 11, p['object'], lsn, [anchor(p['pair'][1], incarnation=p['inc'])], p['inc'])
        s['images'][7] = page(p['pair'][1], 1, p['object'], lsn, [b'XBT1\x01\x00' + bytes(26)], p['inc'])
    if op != 3:
        s['images'][1], aggregate = root_page(s)
        if ordinal is not None:
            start = ordinal // 15 * 15
            s['images'][2] = page(2 + ordinal // 15, 13, 4, lsn, s['jobs'][start:start + 15])
        old = before['aging_chain']; s['aging_chain'] = (lsn, old[0], old[2] or lsn, old[3] + 1)
        s['aging'] = descriptor(b'\x02' + bytes(7) + work(), 5, s['aging_chain'], aggregate,
                                (s['aging_chain'][2], b''), s['aging_cost'], s['aging_chain'][2], 0,
                                s['profile'], root=s['images'][1], revision=s['revision'], cursor=s['cursor'])
    if op in HANDOFF or op == 3:
        s['tx_cost'] += cost(op, p['c']); s['tx_groups'] += 1
        prior = 0 if op == 1 else get(before['tx'], 72)
        remaining = {}
        if s['status_phase'] not in ('committed', 'aborted'):
            remaining[2] = (1, 56712, 0, 0)
        if s['status_phase'] == 'active':
            remaining[3] = (1, 3040, 0, 0)
        root_state = s['jobs'][root][6]
        if root_state == 1:
            remaining[0] = (1, 119984 + p['c'], 0, 0)
            remaining[1] = (1, 153840 + p['c'], 2, 1)
        elif root_state == 2 and s['status_phase'] not in ('committed', 'aborted'):
            remaining[0] = (1, 119984 + p['c'], 0, 0)
        scell = cell_at(s['images'][4], 0)[0]
        s['tx'] = descriptor(b'\x01' + bytes(7) + actor(p['local'], p['generation']), scell[38],
                             (lsn, prior, s['begin'], s['tx_groups']), budgets(remaining), s['inv'],
                             s['tx_cost'], s['begin'], s['mutation'], s['profile'], status_cell=scell)
    return s, ordinal


def core(p, op, before, after, ordinal):
    raw = bytearray(256); put(raw, 0, op, 2); put(raw, 4, 256, 4); flags = 0
    live = op != 3 or before.get('status_head', False)
    if op in (11, 2, 12, 4):
        put(raw, 8, 1); put(raw, 16, 1)
    if op in (11, 2, 12, 4, 20, 21, 22) or op == 3 and live:
        put(raw, 24, p['sequence']); raw[32:72] = slot(p['undo'], p['ug']); flags |= 1
    if op == 3 and live:
        raw[72:112] = slot(p['undo'], p['ug']); flags |= 2
    if op == 6:
        put(raw, 152, p['q'])
    if op in (17, 20, 21, 22):
        put(raw, 160, before['cursor']); put(raw, 168, after['cursor'])
    if op in (2, 4, 20):
        raw[176:208] = sha(projection(p['object'], before['tables'].get(p['object'])))
        raw[208:240] = sha(projection(p['object'], after['tables'].get(p['object'])))
        flags |= 24
    put(raw, 2, flags, 2)
    return bytes(raw)


def frame(before, after, p, op, ordinal, overrides=None):
    overrides = overrides or {}
    lsn = after['last_lsn']; roles = sorted(ROLES[op], key=lambda r: get(after['images'][r], 32))
    natives = []; redos = []
    for index, role in enumerate(roles):
        a = after['images'][role]; b = None if role >= 6 else before['images'][role]
        bs = None if role >= 6 or role == 3 and op == 2 else ordinal % 15 if role == 2 else 0
        ass = None if role == 3 and op == 4 else ordinal % 15 if role == 2 else 0
        if role == 3:
            if bs is not None:
                bs = 1 + sorted(before['tables']).index(p['object'])
            if ass is not None:
                ass = 1 + sorted(after['tables']).index(p['object'])
        n = bytearray(128); n[0] = NATIVE_KIND[role]; put(n, 2, index, 2)
        bodies = []
        for image, selected, flag, slot_at, off_at, len_at, digest_at in (
            (b, bs, 1, 4, 8, 12, 24), (a, ass, 2, 6, 10, 16, 56)):
            put(n, slot_at, 65535 if selected is None else selected, 2)
            if selected is not None:
                data, offset = cell_at(image, selected); n[1] |= flag
                put(n, off_at, offset, 2); put(n, len_at, len(data), 4); n[digest_at:digest_at + 32] = sha(data)
                bodies.append(data)
        put(n, 20, 128 + sum(map(len, bodies)), 4); natives.append(bytes(n) + b''.join(bodies))
        data = a if b is None else a[16128:16352] if op == 3 else a[72:16352]
        r = bytearray(64); r[:3] = bytes((2 if b is None else 1, a[6], 0))
        put(r, 4, len(data) + 64, 4); r[8:32] = a[24:48]; r[32:48] = a[48:64]
        put(r, 48, get(b, 64) if b is not None else 0)
        put(r, 56, 0 if b is None else 16128 if op == 3 else 72, 2); put(r, 58, len(data), 2)
        redos.append(bytes(r) + data)
    edges = []; counts = []
    if op in (2, 4, 20):
        u = b'\x01' + bytes(7) + slot(p['undo'], p['ug']) + struct.pack('<Q', p['sequence'])
        t = b'\x02' + bytes(7) + actor(p['local'], p['generation'])
        for index, role, target in sorted(((roles.index(3), 3, u), (roles.index(3), 9, t), (roles.index(4), 10, u))):
            e = bytearray(128); put(e, 0, index, 2); e[2] = role
            at = 64 if op == 2 else 8; e[at:at + 56] = target; edges.append(bytes(e))
        for target, amount, index in ((u, 2, roles.index(5)), (t, 1, roles.index(4))):
            c = bytearray(96); c[:56] = target; put(c, 64 if op == 2 else 56, amount)
            put(c, 72, index, 2); counts.append(bytes(c))
    deltas = []
    if op in (11, 2, 12):
        for start in range(0, 288, 96):
            d = bytearray(208); d[:2] = bytes(({11: 1, 2: 2, 12: 3}[op], 2 if op == 11 else 1))
            at = 112 if op == 11 else 16; d[at:at + 96] = items(p, after['reserve'])[start:start + 96]; deltas.append(bytes(d))
    descs = [after['tx'], after['aging']] if op in HANDOFF else [after['tx'] if op == 3 else after['aging']]
    prior = [before['tx'], before['aging']] if op in HANDOFF else [before['tx'] if op == 3 else before['aging']]
    h = bytearray(256 if op in HANDOFF else 192); h[:4] = b'XHG1' if op in HANDOFF else b'XSG1'
    for at, value in ((4, 2), (6, op), (8, len(h)), (10, 2)):
        put(h, at, value, 2)
    put(h, 16, lsn)
    for i, d in enumerate(descs):
        oa, pa, ha = (40 + 56 * i, 24 + 8 * i, 152 + 32 * i) if op in HANDOFF else (32, 24, 88)
        h[oa:oa + 56] = d[16:72]
        if prior[i] is not None:
            put(h, pa, get(prior[i], 72)); h[ha:ha + 32] = sha(prior[i])
    n, e, c, it, r = map(b''.join, (natives, edges, counts, deltas, redos))
    if op in HANDOFF:
        for at, value in ((216, 1536), (220, 1536), (224, 256), (228, len(n)), (232, len(e) + len(c) + len(it)), (240, len(r))):
            put(h, at, value, 4)
        for at, value in ((236, len(roles)), (238, len(roles)), (244, len(edges)), (246, len(counts)), (248, len(deltas))):
            put(h, at, value, 2)
    else:
        for at, value in ((120, 1536), (136, len(n)), (140, len(e)), (144, len(c)), (148, len(it)), (152, len(r))):
            put(h, at, value, 4)
        for at, value in ((124, len(roles)), (126, len(edges)), (128, len(counts)), (130, len(deltas)), (132, len(roles))):
            put(h, at, value, 2)
    body = b''.join(descs) + core(p, op, before, after, ordinal) + n + e + c + it + r
    put(h, 12, len(h) + len(body), 4)
    outer = bytearray(160); outer[:6] = b'XWF1\x01\x00'
    outer[6:8] = bytes(({1: 1, 2: 2, 3: 2, 4: 4, 6: 5, 7: 6, 11: 2, 12: 2}.get(op, 3), 3 if op in HANDOFF else 2))
    put(outer, 8, 160, 2); put(outer, 12, 192 + len(h) + len(body), 4); put(outer, 16, len(h) + len(body), 4)
    put(outer, 20, len(roles), 2); outer[24:40] = DB; put(outer, 40, 1); put(outer, 48, 1, 4)
    put(outer, 56, lsn); put(outer, 64, lsn - 1)
    if op in HANDOFF or op == 3:
        outer[72:112] = actor(p['local'], p['generation'])[:40]
        put(outer, 112, 0 if op == 1 else get(before['tx'], 72))
    outer[128:160] = sha(outer[:128]); raw = outer + h + body + bytes(32)
    need(len(raw) == cost(op, p.get('c', 254)), ('frame cost', op, len(raw)))
    return seal(raw), roles


def canonical_cases(profile_raw):
    cases = {}
    def add(identifier, before, after, p, op, ordinal):
        wire, roles = frame(before, after, p, op, ordinal)
        cases[identifier] = dict(id=identifier, before=before, after=after, plan=copy.deepcopy(p),
                                 op=op, ordinal=ordinal, frame=wire, roles=roles, expected='ACCEPT')
    genesis = fresh(profile_raw)
    started, ordinal = successor(genesis, {}, 16, 'aging', 1)
    add('G01', genesis, started, {}, 16, ordinal)
    s5 = setup(started)
    for prefix, phases, cycles in (('C', COMMIT, 2), ('R', ROLLBACK, 1), ('K', CANCEL, 2)):
        before = copy.deepcopy(s5); number = 0
        for local in range(1, cycles + 1):
            # A distinct actor has no existing Tx descriptor. The previous
            # terminal descriptor remains in that case's immutable after/frame.
            before = copy.deepcopy(before); before['tx'] = None
            p = plan(local, canceled=prefix == 'K')
            if prefix == 'R':
                p['outcome'] = 'aborted'
            for op, phase in phases:
                after, ordinal = successor(before, p, op, phase, before['last_lsn'] + 1)
                number += 1; add(f'{prefix}{number:02}', before, after, p, op, ordinal)
                before = after
        totals = {'C': (25, 754128, 4, 4, 0, 7, 2, 2),
                  'R': (16, 296896, 2, 2, 0, 6, 1, 0),
                  'K': (17, 129896, 4, 2, 2, 7, 2, 0)}[prefix]
        actual = tuple(before[k] for k in ('last_lsn', 'aging_cost', 'issued', 'completed', 'canceled', 'ohigh', 'uhigh', 'commit'))
        need(actual == totals, ('independent sequence totals', prefix, actual, totals))
        need(before['tx_cost'] == {'C': 354158, 'R': 477436, 'K': 272408}[prefix], 'Tx consume total')
    for identifier, maximum, alternative in (('P01', False, True), ('P02', True, False)):
        before = setup(started, alternative); p = plan(maximum=maximum, alternative=alternative)
        for op, phase in COMMIT[:3]:
            after, ordinal = successor(before, p, op, phase, before['last_lsn'] + 1)
            if op == 2:
                add(identifier, before, after, p, op, ordinal)
            before = after
    need(len(cases) == 46, 'positive inventory')
    return cases


def refresh_root(s):
    s['images'][1], aggregate = root_page(s)
    old = s['aging']
    if old is not None:
        s['aging'] = descriptor(b'\x02' + bytes(7) + work(), 5, s['aging_chain'], aggregate,
                                (s['aging_chain'][2], b''), s['aging_cost'], s['aging_chain'][2], 0,
                                s['profile'], root=s['images'][1], revision=s['revision'], cursor=s['cursor'])
    s['images'][2] = page(2, 13, 4, get(s['images'][2], 64), s['jobs'][:15])


def mutants(cases):
    bases = ('C10', 'C10', 'R11', 'C10', *('R04',) * 7, 'C03', 'C03', 'C03', 'C03',
             'C03', 'C04', 'C03', 'C13', 'R06', 'C09', 'C10', 'C02', 'C13', 'C03', 'C02')
    for index, base in enumerate(bases, 1):
        case = copy.deepcopy(cases[base]); case['id'] = f'N{index:02}'; case['base'] = base
        case['expected'] = 'REJECT_FRAME' if index in (16, 17, 18, 19, 24) else 'REJECT_INPUT'
        b, a, p = case['before'], case['after'], case['plan']
        if index in (1, 2, 3, 4):
            phase = 'aborted' if index == 1 else 'committed'
            pp = dict(p, q=2 if index == 2 else 1, generation=3 if index == 4 else p['generation'])
            b['images'][4] = replace_cell(b['images'][4], 0, status(pp, phase))
        elif index == 5:
            b['images'].pop(5)
        elif 6 <= index <= 11:
            u = bytearray(cell_at(b['images'][5], 0)[0])
            if index == 6:
                u = undo(p, 'free')
            else:
                at, value = {7: (56, 77), 8: (16, p['sequence'] + 1), 9: (112, p['object'] + 1), 10: (80, 1), 11: (16, p['sequence'])}[index]
                put(u, at, value)
            b['images'][5] = replace_cell(b['images'][5], 0, u)
            if index == 11:
                image = bytearray(b['images'][5]); put(image, 64, a['last_lsn']); b['images'][5] = seal(image)
        elif index == 12:
            entry = list(b['entries'][p['pair'][0]]); entry[4] = actor(77, p['generation'])
            b['entries'][p['pair'][0]] = tuple(entry)
            b['images'][0] = allocation(b['entries'], get(b['images'][0], 64))
        elif index == 13:
            d = bytearray(b['tx']); d[560:592] = sha(b'independent wrong inventory'); b['tx'] = seal(d)
        elif index in (14, 25):
            j = bytearray(b['jobs'][p['root']])
            if index == 14:
                j[288:320] = sha(b'independent wrong plan')
            else:
                put(j, 280, 255, 4); j[384:] = budgets({3: (5, 312118, 0, 0)})
                d = bytearray(b['tx'])
                for row, amount in ((0, 120239), (1, 154095)):
                    put(d, 640 + row * 128 + 16, amount); put(d, 640 + row * 128 + 112, amount)
                put(d, 1328, get(d, 1328) + 2); b['tx'] = seal(d)
            b['jobs'][p['root']] = bytes(j); refresh_root(b)
        elif index == 15:
            j = bytearray(cell_at(b['images'][2], 0)[0]); put(j, 8, get(j, 8) + 1)
            b['images'][2] = replace_cell(b['images'][2], 0, j)
        elif index == 16:
            f = bytearray(case['frame']); f[312:344] = sha(b'wrong previous Tx'); case['frame'] = seal(f)
        elif index == 17:
            j = bytearray(a['jobs'][p['terminal']]); j[328:360] = sha(a['tx'])
            a['jobs'][p['terminal']] = bytes(j); refresh_root(a)
            case['frame'] = frame(b, a, p, case['op'], case['ordinal'])[0]
        elif index == 18:
            a['aging_cost'] += cost(2, p['c']); refresh_root(a)
            case['frame'] = frame(b, a, p, case['op'], case['ordinal'])[0]
        elif index == 19:
            v = bytearray(a['tables'][6]); v[236] = ord('V'); a['tables'][6] = bytes(v)
            cells = [b'XBT1\x01\x00' + bytes(26)] + [tree_cell(vv, obj) for obj, vv in sorted(a['tables'].items())]
            a['images'][3] = page(8, 5, 5, a['last_lsn'], cells)
            case['frame'] = frame(b, a, p, case['op'], case['ordinal'])[0]
        elif index == 20:
            case['op'] = 21; case['ordinal'] = p['root']
        elif index == 21:
            j = bytearray(b['jobs'][p['root']]); j[121] = 2
            j[384:] = budgets({3: (2, 111056, 0, 0)})
            b['jobs'][p['root']] = bytes(j)
            b['images'][5] = replace_cell(b['images'][5], 0, undo(p, 'releasing', 2))
            refresh_root(b)
        elif index == 22:
            old = cases['C09']['before']
            b['jobs'][p['root']] = old['jobs'][p['root']]
            b['completed'] -= 1
            b['images'][5] = old['images'][5]
            refresh_root(b)
        elif index == 23:
            p['pair'] = (13, 14)
        elif index == 24:
            f = bytearray(case['frame']); at = 160 + 256 + 3072 + 256
            for role in case['roles']:
                length = get(f, at + 20, 4)
                if role == 3:
                    put(f, at + 6, 1, 2); put(f, at + 10, cell_at(a['images'][3], 1)[1], 2)
                    break
                at += length
            case['frame'] = seal(f)
        elif index == 26:
            p['pair'] = (11, 11)
        cases[case['id']] = case
    need(len(cases) == 72, 'final inventory')


def emit(cases, profile_raw):
    blobs = {}; records = []
    def blob(raw):
        if raw is None:
            return '-'
        need(len(raw) <= 163840, 'raw file bound')
        h = sha(raw).hex(); blobs[h] = raw; return h
    profile_id = blob(profile_raw)
    for identifier, case in sorted(cases.items()):
        b, a, p, op = case['before'], case['after'], case['plan'], case['op']
        row = dict(id=identifier, base=case.get('base', '-'), expected=case['expected'], operation=op,
                   lsn=a['last_lsn'], ordinal=case['ordinal'], plan={k: v.hex() if isinstance(v, bytes) else v for k, v in p.items()},
                   before={str(r): blob(image) for r, image in b['images'].items() if r < 6},
                   jobs=blob(b''.join(b['jobs'])), anchor=blob(b['anchor']), transaction=blob(b['tx']), aging=blob(b['aging']),
                   frame=blob(case['frame']), order=case['roles'], after={str(r): blob(a['images'][r]) for r in case['roles']},
                   external_gap=blob(b.get('external_gap')))
        # Runtime harness uses this bounded, explicitly ordered scalar record;
        # JSON is the human/auditor view, not an implicit test parser dependency.
        fields = [identifier, row['expected'], str(op), str(row['lsn']), str(case['ordinal'] if case['ordinal'] is not None else -1),
                  str(p.get('local', 0)), str(p.get('generation', 0)), str(p.get('status', 0)), str(p.get('undo', 0)),
                  str(p.get('ug', 0)), str(p.get('pair', (0, 0))[0]), str(p.get('pair', (0, 0))[1]), str(p.get('inc', 0)),
                  str(p.get('c', 0)), p.get('projection', bytes(32)).hex(), str(int(p.get('maximum', False))), p.get('name', '-'),
                  row['jobs'], row['anchor'], row['transaction'], row['aging'], row['frame'],
                  ','.join(map(str, row['order'])), *[row['before'].get(str(r), '-') for r in range(6)],
                  *[row['after'].get(str(r), '-') for r in range(8)]]
        row['tsv'] = '\t'.join(fields); records.append(row)
    total = sum(map(len, blobs.values())); need(total <= 20 * 1024 * 1024, ('corpus cap', total))
    need(len(records) == 72 and sum(r['expected'] == 'ACCEPT' for r in records) == 46, 'manifest count')
    index = 'sha256\tbytes\tfile\n' + ''.join(f'{h}\t{len(raw)}\traw/{h}.bin\n' for h, raw in sorted(blobs.items()))
    selectors = '\n'.join(r.pop('tsv') for r in records) + '\n'
    manifest = dict(generator='D3-CREATE-TRANSITION-G1', authority='NONE', status='GENERATED_INPUT_NOT_RUN',
                    count=72, positive=46, negative=26, raw_count=len(blobs), raw_bytes=total, profile=profile_id,
                    generator_sha256=sha(Path(__file__).read_bytes()).hex(),
                    input_plan_sha256=sha((HERE / 'input-plan.json').read_bytes()).hex(),
                    raw_index_sha256=sha(index.encode()).hex(), cases_tsv_sha256=sha(selectors.encode()).hex(), cases=records)
    # All arithmetic and corpus bounds close before the first raw write.
    (HERE / 'raw').mkdir()
    for h, raw in sorted(blobs.items()):
        with (HERE / 'raw' / (h + '.bin')).open('xb') as f:
            f.write(raw)
    for name, data in (('raw-index.tsv', index.encode()), ('cases.tsv', selectors.encode()),
                       ('manifest.json', (json.dumps(manifest, indent=2, ensure_ascii=False) + '\n').encode())):
        with (HERE / name).open('xb') as f:
            f.write(data)
    return dict(status='GENERATED_INPUT_NOT_RUN', cases=72, raw_files=len(blobs), raw_bytes=total,
                manifest_sha256=sha((HERE / 'manifest.json').read_bytes()).hex())


def main():
    need(not any((HERE / p).exists() for p in ('raw', 'manifest.json', 'generation-result.json')), 'write-once output already exists')
    started = time.monotonic()
    try:
        plan_raw = json.loads((HERE / 'input-plan.json').read_text())
        for path, h in plan_raw['contracts'].items():
            need(sha((REPO / path).read_bytes()).hex() == h, ('contract pin', path))
        prof = profile(); cases = canonical_cases(prof); mutants(cases)
        result = emit(cases, prof)
    except Exception as error:
        result = dict(status='GENERATION_FAILED_NOT_PRODUCT_RUN', error=repr(error), traceback=traceback.format_exc())
        raise
    finally:
        result['wall_seconds'] = time.monotonic() - started
        result['generator_sha256'] = sha(Path(__file__).read_bytes()).hex()
        with (HERE / 'generation-result.json').open('x') as f:
            json.dump(result, f, indent=2); f.write('\n')
        print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    main()
