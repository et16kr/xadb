#!/usr/bin/env python3
"""Independent scope2 leaf INPUT generation; never runs product code or tests.

Canonical offsets come from the pinned contracts, not from an encoder result.
Wire positives are deliberately SHAPE_ONLY: descriptor/native semantic bodies
contain obvious sentinels and are not accepted histories or semantic grants.
Generation is write-once: a pre-existing manifest or raw directory is an error.
"""
import hashlib
import json
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
DB = bytes(range(1, 17))
FILE = 0x100000001
MAX = (1 << 64) - 1
LIMIT = 8 * 1024 * 1024


def sha(raw):
    return hashlib.sha256(raw).digest()


def put(raw, at, value, width=8):
    struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[width], raw, at, value)


def page_ref(page, incarnation=2):
    return struct.pack('<QQQ', FILE, page, incarnation)


def undo_ref(generation=2):
    return page_ref(10) + struct.pack('<H6xQ', 0, generation)


def actor(local=1, generation=2):
    return DB + struct.pack('<QI4xQQ', 1, 1, local, generation)


def owner(system=False):
    reference = DB + struct.pack('<QB7xQQ', 1, 3, 1, 1) if system else actor()
    return bytes([2 if system else 1]) + bytes(7) + reference


def catalog(*, object_id=6, committed=False, maximum=False):
    if maximum:
        name = b'T' * 128
        columns = [f'c{i:02d}'.encode() + b'x' * 125 for i in range(32)]
    else:
        name, columns = (b'A' if object_id == 6 else b'B'), [b'i']
    body = struct.pack('<HBB', len(name), len(columns), 0) + name
    body += b''.join(struct.pack('<HBB', len(c), 2, 0) + c for c in columns)
    raw = bytearray(232 + len(body))
    raw[:4] = b'XCT1'
    put(raw, 4, 1, 2)
    raw[6:8] = bytes([1, 0 if committed else 2])
    for at, value in ((8, object_id), (16, 1), (24, 1)):
        put(raw, at, value)
    local = 1 if object_id == 6 else 2
    if committed:
        raw[32] = 1
        put(raw, 40, local)
    else:
        raw[32] = 2
        raw[40:88] = actor(local, local + 1)
        raw[136:176] = undo_ref(local + 1)
    raw[88:128] = actor(local, local + 1)[:40]
    put(raw, 128, 1)
    raw[176:200] = page_ref(11 if object_id == 6 else 13)
    put(raw, 224, len(body), 4)
    raw[232:] = body
    return bytes(raw)


def projection(value=None, object_id=6):
    raw = bytearray(48)
    raw[:4] = b'XCP1'
    put(raw, 4, 1, 2)
    put(raw, 6, 3, 2)
    raw[8] = 3
    for at, number in ((16, 5), (24, 1), (32, object_id)):
        put(raw, at, number)
    raw[40] = int(value is not None)
    return bytes(raw) + (value or b'')


def cell(value, object_id=6):
    return struct.pack('<HH', 8, len(value)) + struct.pack('>Q', object_id) + value


def core(op, *, before=None, after=None, sequence=1, q=1,
         cursor=(1, 2), live=True):
    raw = bytearray(256)
    put(raw, 0, op, 2)
    put(raw, 4, 256, 4)
    flags = 0
    if op in (2, 4, 11, 12):
        put(raw, 8, 1)
        put(raw, 16, 1)
    if op in (2, 4, 11, 12, 20, 21, 22) or op == 3 and live:
        put(raw, 24, sequence)
        raw[32:72] = undo_ref()
        flags |= 1
    if op == 3 and live:
        raw[72:112] = undo_ref()
        flags |= 2
    if op in (2, 4, 20):
        assert before is not None and after is not None
        raw[176:208], raw[208:240] = sha(before), sha(after)
        flags |= 24
    if op == 6:
        put(raw, 152, q)
    if op in (17, 20, 21, 22):
        put(raw, 160, cursor[0])
        put(raw, 168, cursor[1])
    put(raw, 2, flags, 2)
    return bytes(raw)


def page(number, kind, object_id, lsn, cells, incarnation=None):
    raw = bytearray(16384)
    raw[:4] = b'XDP1'
    put(raw, 4, 1, 2)
    raw[6] = kind
    raw[8:24] = DB
    raw[24:48] = page_ref(number, incarnation or (1 if number < 9 else 2))
    for at, value in ((48, object_id), (56, 1), (64, lsn)):
        put(raw, at, value)
    put(raw, 72, len(cells), 2)
    put(raw, 74, 128 + 4 * len(cells), 2)
    end = 16352
    for i, data in enumerate(cells):
        start = end - len(data)
        assert start >= 128 + 4 * len(cells)
        put(raw, 128 + 4 * i, start, 2)
        put(raw, 130 + 4 * i, len(data), 2)
        raw[start:end] = data
        end = start
    put(raw, 76, end, 2)
    raw[-32:] = sha(raw[:-32])
    return bytes(raw)


def native(kind, redo, before, after, *, slot=0, offset=None):
    raw = bytearray(128)
    raw[0:2] = bytes([kind, int(before is not None) + 2 * int(after is not None)])
    put(raw, 2, redo, 2)
    for data, slot_at, off_at, len_at, sha_at in (
        (before, 4, 8, 12, 24), (after, 6, 10, 16, 56)
    ):
        put(raw, slot_at, slot if data is not None else 65535, 2)
        if data is not None:
            put(raw, off_at, offset if offset is not None else 16352 - len(data), 2)
            put(raw, len_at, len(data), 4)
            raw[sha_at:sha_at + 32] = sha(data)
    raw += (before or b'') + (after or b'')
    put(raw, 20, len(raw), 4)
    return bytes(raw)


def redo(number, kind, object_id, after, *, init=False, rollback=False):
    data = after if init else after[16128:16352] if rollback else after[72:16352]
    raw = bytearray(64)
    raw[0:3] = bytes([2 if init else 1, kind, 0])
    put(raw, 4, 64 + len(data), 4)
    raw[8:32] = page_ref(number, 1 if number < 9 else 2)
    put(raw, 32, object_id)
    put(raw, 40, 1)
    put(raw, 48, 0 if init else 100)
    put(raw, 56, 0 if init else 16128 if rollback else 72, 2)
    put(raw, 58, len(data), 2)
    return bytes(raw) + data


def target(undo=True):
    return (b'\x01' + bytes(7) + undo_ref() + struct.pack('<Q', 1)) if undo else b'\x02' + bytes(7) + actor()


def graph(op):
    if op not in (2, 4, 20):
        return b'', b''
    indices = (3, 4, 5) if op in (2, 4) else (2, 3, 4)
    catalog_i, status_i, undo_i = indices
    edges = []
    for index, role, ref in ((catalog_i, 3, target()), (catalog_i, 9, target(False)), (status_i, 10, target())):
        e = bytearray(128)
        put(e, 0, index, 2)
        e[2] = role
        at = 64 if op == 2 else 8
        e[at:at + 56] = ref
        edges.append(bytes(e))
    counts = []
    for ref, index, count in ((target(), undo_i, 2), (target(False), status_i, 1)):
        c = bytearray(96)
        c[:56] = ref
        put(c, 56 if op != 2 else 64, count)
        put(c, 72, index, 2)
        counts.append(bytes(c))
    return b''.join(edges), b''.join(counts)


def inventory(op):
    if op not in (2, 11, 12):
        return b''
    out = []
    for kind, ref in ((1, page_ref(11)), (1, page_ref(12)), (2, undo_ref())):
        item = bytearray(96)
        item[0:2] = bytes([kind, 2])
        item[8:8 + len(ref)] = ref
        put(item, 64, 1)
        put(item, 72, 101 if op == 11 else 100)
        delta = bytearray(208)
        delta[0] = {11: 1, 2: 2, 12: 3}[op]
        delta[1] = 2 if op == 11 else 1
        at = 112 if op == 11 else 16
        delta[at:at + 96] = item
        out.append(bytes(delta))
    return b''.join(out)


def components(op, value):
    # Independent native inventory table. Non-Catalog bodies are explicit
    # semantic-invalid sentinel cells; page envelopes and native spans are real.
    record = cell(value)
    after_record = cell(catalog(committed=True, maximum=len(value) > 242))
    rows = {
        'allocator': (0, 10, 1, 6, 6208, 0),
        'root': (1, 12, 4, 8, 512, 0),
        'job': (2, 13, 4, 9, 1024, 1),
        'catalog': (8, 5, 5, 7, len(record), 1),
        'status': (9, 9, 2, 5, 224, 0),
        'undo': (10, 8, 3, 3, 432, 0),
        'anchor': (11, 11, 6, 10, 48, 0),
        'row': (12, 1, 6, 11, 32, 0),
    }
    names = {
        1: ('root', 'job', 'status'), 2: tuple(rows),
        3: ('status',), 4: ('allocator', 'root', 'job', 'catalog', 'status', 'undo'),
        6: ('root', 'job', 'status'), 7: ('root', 'job', 'status'),
        11: ('allocator', 'root', 'job', 'undo'), 12: ('allocator', 'root', 'job'),
        16: ('root',), 17: ('root', 'job', 'status'),
        20: ('root', 'job', 'catalog', 'status', 'undo'),
        21: ('root', 'job', 'undo'), 22: ('root', 'job', 'undo'),
    }[op]
    natives, redos = [], []
    for i, name in enumerate(names):
        number, kind, obj, native_kind, size, slot = rows[name]
        before, after = bytes([0x40 + i]) * size, bytes([0x60 + i]) * size
        if name == 'catalog':
            before = None if op == 2 else record
            after = None if op == 4 else after_record if op == 20 else record
        elif name == 'anchor':
            before = None
            body = bytearray(48)
            body[:8] = b'XTA1\x01\x00\x01\x00'
            body[8:32] = page_ref(12)
            put(body, 32, 1)
            after = bytes(body)
        elif name == 'row':
            before = None
            after = b'XBT1\x01\x00' + bytes(26)
        if name == 'job' and op in (1, 6, 7, 17):
            slot = 0
        offset = 16352 - size * (slot + 1)
        if name == 'catalog':
            offset = 16320 - size
        natives.append(native(native_kind, i, before, after, slot=slot, offset=offset))
        cells = ([bytes(1024)] if slot == 1 and name == 'job' else
                 [b'XBT1\x01\x00' + bytes(26)] if name == 'catalog' else [])
        if after is not None:
            cells.append(after)
        image = page(number, kind, obj, 101, cells)
        redos.append(redo(number, kind, obj, image, init=name in ('anchor', 'row'), rollback=op == 3))
    return b''.join(natives), b''.join(redos), len(natives), len(redos)


def frame(op, value, operation):
    handoff = op in (1, 2, 4, 6, 7, 11, 12)
    group = bytearray(256 if handoff else 192)
    group[:4] = b'XHG1' if handoff else b'XSG1'
    for at, v in ((4, 2), (6, op), (8, len(group)), (10, 2)):
        put(group, at, v, 2)
    put(group, 16, 101)
    prior_tx = 0 if op == 1 else 100
    prior_owner = 0 if op == 16 else 100
    n, r, nc, rc = components(op, value)
    edges, counts = graph(op)
    items = inventory(op)
    descriptors = bytes([0xD1]) * 1536
    if handoff:
        put(group, 24, prior_tx)
        put(group, 32, 100)
        group[40:96], group[96:152] = owner(), owner(True)
        if op != 1:
            group[152:184] = sha(b'independent prior Tx descriptor sentinel')
        group[184:216] = sha(b'independent prior Aging descriptor sentinel')
        for at, v in ((216, 1536), (220, 1536), (224, 256), (228, len(n)),
                      (232, len(edges) + len(counts) + len(items)), (240, len(r))):
            put(group, at, v, 4)
        for at, v in ((236, nc), (238, rc), (244, len(edges) // 128),
                      (246, len(counts) // 96), (248, len(items) // 208)):
            put(group, at, v, 2)
        descriptors += bytes([0xD2]) * 1536
    else:
        put(group, 24, prior_owner)
        group[32:88] = owner(op != 3)
        if op != 16:
            group[88:120] = sha(b'independent prior owner descriptor sentinel')
        for at, v in ((120, 1536), (136, len(n)), (140, len(edges)),
                      (144, len(counts)), (148, len(items)), (152, len(r))):
            put(group, at, v, 4)
        for at, v in ((124, nc), (126, len(edges) // 128), (128, len(counts) // 96),
                      (130, len(items) // 208), (132, rc)):
            put(group, at, v, 2)
    group += descriptors + operation + n + edges + counts + items + r
    put(group, 12, len(group), 4)
    raw = bytearray(160)
    raw[:4] = b'XWF1'
    put(raw, 4, 1, 2)
    kind = {1: 1, 2: 2, 3: 2, 4: 4, 6: 5, 7: 6, 11: 2, 12: 2}.get(op, 3)
    raw[6:8] = bytes([kind, 3 if handoff else 2])
    put(raw, 8, 160, 2)
    put(raw, 12, len(group) + 192, 4)
    put(raw, 16, len(group), 4)
    put(raw, 20, rc, 2)
    raw[24:40] = DB
    put(raw, 40, 1)
    put(raw, 48, 1, 4)
    put(raw, 56, 101)
    put(raw, 64, 100)
    if kind != 3:
        raw[72:112] = actor()[:40]
        put(raw, 112, prior_tx)
    raw[128:160] = sha(raw[:128])
    raw += group
    raw += sha(raw)
    return bytes(raw)


def changed(raw, at, value, width):
    out = bytearray(raw)
    put(out, at, value, width)
    return bytes(out)


def reseal_frame(raw):
    out = bytearray(raw)
    out[128:160] = sha(out[:128])
    out[-32:] = sha(out[:-32])
    return bytes(out)


def main():
    started = time.monotonic()
    if (HERE / 'raw').exists() or (HERE / 'manifest.json').exists():
        raise SystemExit('write-once generation: existing output is preserved')
    plan = json.loads((HERE / 'input-plan.json').read_text())
    for path, expected in plan['contracts'].items():
        assert sha((REPO / path).read_bytes()).hex() == expected, path
    records, raw_bytes = [], 0
    pending = []

    def add(identifier, category, data, expected, **context):
        nonlocal raw_bytes
        assert not any(row['id'] == identifier for row in records), identifier
        raw_bytes += len(data)
        assert raw_bytes <= LIMIT
        row = {'id': identifier, 'category': category, 'file': f'raw/{identifier}.bin',
               'bytes': len(data), 'sha256': sha(data).hex(), 'expected': expected, **context}
        records.append(row)
        pending.append((row['file'], data))

    vmin, vmax = catalog(), catalog(maximum=True)
    projections = {
        'P01': (6, None), 'P02': (6, vmin), 'P03': (6, catalog(committed=True)),
        'P04': (6, vmax), 'P05': (7, catalog(object_id=7)), 'P06': (7, None),
    }
    for identifier, (obj, value) in projections.items():
        add(identifier, 'projection', projection(value, obj), 'DIGEST_MATCH', object=obj,
            value_file=None if value is None else f'raw/{identifier}-value.bin',
            authority='NONE')
        if value is not None:
            add(identifier + '-value', 'catalog-value', value, 'CATALOG_VALID', object=obj)
            add(identifier + '-cell', 'catalog-cell', cell(value, obj), 'WRAPPER_MATCH', object=obj)
    negatives = [
        ('NP01', changed(vmin, 8, 7, 8), 'wrong object', 6),
        ('NP02', changed(vmin, 16, 2, 8), 'scope object generation', 6),
        ('NP03', changed(vmin, 24, 2, 8), 'scope schema epoch', 6),
        ('NP04', changed(vmin, 228, 1, 4), 'catalog reserved', 6),
        ('NP05', vmin[:-1], 'truncated definition', 6),
        ('NP06', vmin + b'\0', 'trailing definition', 6),
        ('NP07', changed(vmin, 7, 0, 2), 'hasUndo cleared while bytes remain; kind also damaged', 6),
        ('NP08', changed(vmin, 112, 2, 4), 'logical Tx epoch mismatches stamp', 6),
    ]
    # Single-field presence mutant (byte assignment avoids adjacent-field damage).
    n = bytearray(vmin); n[7] = 0
    negatives[6] = ('NP07', bytes(n), 'hasUndo cleared while bytes remain', 6)
    # Duplicate columns are separately built from the documented table body.
    duplicate = bytearray(vmin[:232])
    body = b'\x01\x00\x02\x00A\x01\x00\x02\x00i\x01\x00\x02\x00i'
    put(duplicate, 224, len(body), 4); duplicate += body
    negatives.append(('NP09', bytes(duplicate), 'duplicate typed column', 6))
    for identifier, data, reason, obj in negatives:
        add(identifier, 'projection-value-negative', data, 'REJECT', object=obj, reason=reason)
    for identifier, database, timeline, obj, value in (
        ('NP10', bytes([31]) * 16, 1, 6, vmin), ('NP11', DB, 2, 6, None),
        ('NP12', DB, 1, 5, None),
    ):
        add(identifier, 'projection-context-negative', value or b'', 'REJECT',
            database=database.hex(), timeline=timeline, object=obj, present=value is not None)

    missing, present, present_max = projection(), projection(vmin), projection(vmax)
    stable, stable_max = projection(catalog(committed=True)), projection(catalog(committed=True, maximum=True))
    specs = [
        ('C01', 1, {}), ('C02', 2, {'before': missing, 'after': present}),
        ('C03', 2, {'before': missing, 'after': present_max}),
        ('C04', 3, {'live': False}), ('C05', 3, {}),
        ('C06', 4, {'before': present, 'after': missing}),
        ('C07', 4, {'before': present_max, 'after': missing}),
        ('C08', 6, {}), ('C09', 6, {'q': MAX}), ('C10', 7, {}),
        ('C11', 11, {}), ('C12', 12, {}), ('C13', 16, {}),
        ('C14', 17, {'cursor': (2, 1)}), ('C15', 17, {'cursor': (63, 0)}),
        ('C16', 20, {'before': present, 'after': stable}),
        ('C17', 20, {'before': present_max, 'after': stable_max}),
        ('C18', 21, {'cursor': (2, 2)}), ('C19', 22, {'cursor': (63, 0)}),
        ('C20', 11, {'sequence': MAX}),
    ]
    cores = {}
    for identifier, op, args in specs:
        data = core(op, **args)
        cores[identifier] = data
        add(identifier, 'core', data, 'ACCEPT_CORE_ONLY', operation=op,
            parsed={'statement': 1 if op in (2, 4, 11, 12) else 0,
                    'mutation': 1 if op in (2, 4, 11, 12) else 0,
                    'sequence': args.get('sequence', 1) if op in (2, 4, 11, 12, 20, 21, 22) or op == 3 and args.get('live', True) else 0,
                    'q': args.get('q', 1) if op == 6 else 0,
                    'cursors': list(args.get('cursor', (1, 2))) if op in (17, 20, 21, 22) else [0, 0]})
    core_mutants = [
        ('C01', 8, 1, 8, 'unused Begin statement'), ('C01', 2, 32, 2, 'unknown flag'),
        ('C01', 4, 255, 4, 'wrong declared length'), ('C01', 240, 1, 8, 'reserved tail'),
        ('C02', 2, 1, 2, 'digest presence removed'), ('C02', 24, 0, 8, 'missing sequence'),
        ('C02', 8, 2, 8, 'statement outside single statement scope'),
        ('C02', 16, 2, 8, 'mutation outside single mutation scope'),
        ('C02', 56, 1, 2, 'nonzero Undo slot shape is valid but out-of-scope semantic context'),
        ('C04', 24, 1, 8, 'empty rollback sequence'), ('C05', 104, 3, 8, 'rollback next generation mismatch'),
        ('C05', 2, 1, 2, 'rollback next presence missing'),
        ('C08', 152, 0, 8, 'Commit q absent'), ('C11', 176, 1, 8, 'Reserve digest bytes without flag'),
        ('C12', 160, 1, 8, 'Cancel cursor unused'), ('C14', 160, 64, 8, 'before cursor above limit'),
        ('C14', 168, 64, 8, 'after cursor above limit'), ('C16', 8, 1, 8, 'Prune statement unused'),
        ('C18', 208, 1, 8, 'ReleaseRole projection unused'), ('C19', 112, 1, 8, 'RootFree target absent bytes'),
    ]
    # Nonzero typed slot is not a core-shape defect: defer to stable semantic
    # source comparison. This known-bad is explicitly labelled, never a false
    # expected core rejection.
    for i, (base, at, value, width, reason) in enumerate(core_mutants, 1):
        semantic = i == 9
        add(f'NC{i:02}', 'core-semantic-context' if semantic else 'core-negative',
            changed(cores[base], at, value, width),
            'ACCEPT_CORE_REJECT_STABLE_SOURCE' if semantic else 'REJECT',
            operation=int.from_bytes(cores[base][:2], 'little'), base=base, reason=reason)
    add('NC21', 'core-negative', cores['C01'][:-1], 'REJECT', operation=1, reason='truncated core')
    add('NC22', 'core-negative', cores['C01'] + b'\0', 'REJECT', operation=1, reason='trailing core')
    add('NC23', 'core-negative', cores['C01'], 'REJECT', operation=6, reason='expected operation differs')
    # Every operation's otherwise unused q or statement field is explicit.
    for i, identifier in enumerate(('C01', 'C02', 'C04', 'C06', 'C10', 'C11', 'C12', 'C13', 'C14', 'C16', 'C18', 'C19'), 24):
        add(f'NC{i:02}', 'core-negative', changed(cores[identifier], 152, 1, 8), 'REJECT',
            operation=int.from_bytes(cores[identifier][:2], 'little'), base=identifier, reason='q belongs only to Commit')

    wire_specs = [('W01', 1, 'C01', False, 56712), ('W02', 2, 'C02', False, 154094),
                  ('W03', 2, 'C03', True, 158440), ('W04', 3, 'C04', False, 3040),
                  ('W05', 3, 'C05', False, 3040), ('W06', 4, 'C06', False, 120238),
                  ('W07', 4, 'C07', True, 124584), ('W08', 6, 'C08', False, 56712),
                  ('W09', 7, 'C10', False, 56712), ('W10', 11, 'C11', False, 86640),
                  ('W11', 12, 'C12', False, 69304), ('W12', 16, 'C13', False, 19672),
                  ('W13', 17, 'C14', False, 55112), ('W14', 20, 'C16', False, 90004),
                  ('W15', 20, 'C17', True, 98696), ('W16', 21, 'C18', False, 55528),
                  ('W17', 22, 'C19', False, 55528)]
    wires = {}
    for identifier, op, cid, maximum, expected_size in wire_specs:
        data = frame(op, vmax if maximum else vmin, cores[cid])
        assert len(data) == expected_size, (identifier, len(data), expected_size)
        wires[identifier] = data
        h = 256 if op in (1, 2, 4, 6, 7, 11, 12) else 192
        n, r, nc, rc = components(op, vmax if maximum else vmin)
        e, c = graph(op); items = inventory(op)
        native_at = 160 + h + (3072 if h == 256 else 1536) + 256
        add(identifier, 'wire-shape-only', data, 'ACCEPT_LAYOUT_ONLY', operation=op,
            semantic_authority=False, descriptors='INVALID_SENTINELS_NOT_HISTORY',
            expected_stream={'database': DB.hex(), 'timeline': 1, 'epoch': 1, 'previous_lsn': 100},
            profile_format=2, core=cid,
            sections={'native': [native_at, len(n), nc], 'edges': [native_at + len(n), len(e), len(e)//128],
                      'counts': [native_at + len(n)+len(e), len(c), len(c)//96],
                      'items': [native_at + len(n)+len(e)+len(c), len(items), len(items)//208],
                      'redo': [len(data)-32-len(r), len(r), rc]})
    wire_mutants = [
        ('W02', 164, 1, 2, 'legacy version'), ('W02', 170, 1, 2, 'SystemInit scope'),
        ('W02', 170, 0, 2, 'legacy scope'), ('W02', 166, 5, 2, 'unsupported opcode'),
        ('W02', 168, 192, 2, 'XHG header width'), ('W02', 392, 1199, 4, 'XHG aux size off by one'),
        ('W02', 404, 2, 2, 'XHG E count mismatch'), ('W02', 406, 1, 2, 'XHG K count mismatch'),
        ('W02', 408, 2, 2, 'XHG D count mismatch'), ('W02', 410, 1, 2, 'XHG tail reserved'),
        ('W02', 20, 7, 2, 'outer redo count'), ('W02', 56, 102, 8, 'outer/group apply disagreement'),
        ('W02', 200, 2, 2, 'owner0 System tag with Tx body'),
        ('W02', 248, 1, 8, 'actor generation below first leased generation'),
        ('W01', 184, 1, 8, 'fresh Begin previous owner must be zero'),
        ('W12', 184, 1, 8, 'fresh Aging previous owner must be zero'),
        ('W14', 300, 383, 4, 'XSG E byte length'),
        ('W14', 294, 1, 2, 'XSG reserved'),
    ]
    # Write one compact patch recipe per negative, not another full 150KiB copy.
    # The consumer applies literal bytes and re-seals the specified outer SHA.
    # Original positives stay immutable and every recipe has its own fixed ID.
    for i, (base, at, value, width, reason) in enumerate(wire_mutants, 1):
        mutated = reseal_frame(changed(wires[base], at, value, width))
        add(f'NW{i:02}', 'wire-negative-patch', int(value).to_bytes(width, 'little'), 'REJECT',
            base=base, offset=at, reseal='XWF_HEADER_AND_FRAME_SHA256',
            materialized_sha256=sha(mutated).hex(), reason=reason)
    # Aux fields are intentionally outside LayoutView's present proof boundary.
    # They are stable-source negatives, not expected layout rejections.
    fw = next(row for row in records if row['id'] == 'W02')
    contexts = [
        ('NS01', fw['sections']['items'][0] + 16 + 2, 1, 2, 'group-local owner ordinal1 must fail semantic inventory'),
        ('NS02', fw['sections']['counts'][0] + 64, 1, 8, 'deduplicated Undo count1 instead of edge multiset2'),
        ('NS03', fw['sections']['edges'][0] + 128 + 2, 6, 2, 'CatalogStamp changed to UndoBeforeStamp role'),
    ]
    for identifier, at, value, width, reason in contexts:
        mutated = reseal_frame(changed(wires['W02'], at, value, width))
        add(identifier, 'wire-semantic-context-patch', int(value).to_bytes(width, 'little'),
            'ACCEPT_LAYOUT_REJECT_STABLE_SEMANTICS', base='W02', offset=at,
            reseal='XWF_HEADER_AND_FRAME_SHA256', materialized_sha256=sha(mutated).hex(), reason=reason)

    assert time.monotonic() - started < 10, 'generation deadline'
    raw_dir = HERE / 'raw'
    raw_dir.mkdir()
    for name, data in pending:
        with (HERE / name).open('xb') as out:
            out.write(data)
    manifest = {
        'schema': 1, 'input_id': 'D3-CREATE-LEAF-L1', 'state': 'INPUT_FROZEN_NOT_EXECUTED',
        'generator_sha256': sha(Path(__file__).read_bytes()).hex(),
        'plan_sha256': sha((HERE / 'input-plan.json').read_bytes()).hex(),
        'contracts': plan['contracts'], 'database': DB.hex(), 'timeline': 1,
        'raw_files': len(records), 'raw_bytes': raw_bytes, 'cases': records,
        'product_calls': 0, 'compiler_calls': 0, 'test_runner_calls': 0,
        'wire_positive_boundary': 'shape-only; sentinel descriptors/native bodies; never semantic authority',
    }
    (HERE / 'manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
    index = 'id\tpath\tbytes\tsha256\n' + ''.join(f"{r['id']}\t{r['file']}\t{r['bytes']}\t{r['sha256']}\n" for r in records)
    (HERE / 'raw-index.tsv').write_text(index)
    print(json.dumps({'state': manifest['state'], 'raw_files': len(records), 'raw_bytes': raw_bytes,
                      'manifest_sha256': sha((HERE / 'manifest.json').read_bytes()).hex(),
                      'index_sha256': sha(index.encode()).hex(), 'product_calls': 0}))


if __name__ == '__main__':
    main()
