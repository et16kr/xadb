#!/usr/bin/env python3
"""Read-only independent scalar/envelope auditor; no serializer/product import."""
import hashlib
import json
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
PLAN_SHA = 'd6306d7396ad70165a291a14208c35b7e6e8091f0a4c67e870ea9e96d34e0a34'
HANDOFF = {1, 2, 4, 6, 7, 11, 12}
SIZES = {1: 56712, 2: 154094, 3: 3040, 4: 120238, 6: 56712, 7: 56712,
         11: 86640, 12: 69304, 16: 19672, 17: 55112, 20: 90004, 21: 55528, 22: 55528}


def check(value, explanation):
    if not value:
        raise ValueError(explanation)


def digest(raw):
    return hashlib.sha256(raw).digest()


def number(raw, at, width=8):
    return int.from_bytes(raw[at:at + width], 'little')


def cell(page, ordinal=0):
    count = number(page, 72, 2)
    check(0 <= ordinal < count, ('slot bound', ordinal, count))
    at = 128 + ordinal * 4
    offset, length = number(page, at, 2), number(page, at + 2, 2)
    check(128 + 4 * count <= offset <= offset + length <= 16352, 'cell range')
    return page[offset:offset + length], offset


def actor(epoch, generation):
    return bytes(range(1, 17)) + struct.pack('<QI4xQQ', 1, epoch, 1, generation)


def root_check(page, jobs, aging):
    root = cell(page)[0]
    check(root[:6] == b'XQR1\x02\x00' and len(root) == 512, 'Root wire')
    check(len(jobs) == 65536, 'whole Jobs')
    masks = [0, 0, 0]
    steps = remaining = 0
    for index in range(64):
        job = jobs[index * 1024:(index + 1) * 1024]
        check(job[:6] == b'XQJ1\x02\x00', ('Job wire', index))
        state = job[6]
        masks[0 if state == 0 else 1 if state == 1 else 2] |= 1 << index
        if state:
            check(job[16:64] in (actor(1, 2), actor(2, 3)), ('Job actor', index))
            steps += number(job, 384 + 3 * 128 + 8)
            remaining += number(job, 384 + 3 * 128 + 16)
    check([number(root, i) for i in (96, 104, 112)] == masks, 'Root masks')
    check(root[128:160] == digest(jobs), 'Root Jobs digest')
    check(number(root, 88) == number(root, 376) + number(root, 384) + (masks[1] | masks[2]).bit_count(), 'Root conservation')
    check(number(root, 400) == number(root, 392) + 5, 'CREATE object/undo high')
    check(number(root, 408) <= number(root, 88) - number(root, 392), 'C/Begin bound')
    if aging:
        check(aging[:6] == b'XOD1\x01\x00' and aging[-32:] == digest(aging[:-32]), 'Aging descriptor seal')
        check(aging[416:448] == digest(page) and aging[448:480] == digest(root), 'Aging exact Root witness')
        check(number(aging, 360) == number(root, 64) == number(aging, 624) + 1, 'Aging revision/groups')
        check(number(aging, 368) == number(root, 72), 'Aging cursor')
        check(number(aging, 608) == number(root, 416), 'actual Aging consume once')
        check(number(aging, 1032) == steps and number(aging, 1040) == remaining, 'Aging remaining sum')
        check(root[160:192] == digest(aging[640:1280]), 'Root budget digest')
    return root


def semantic_frame(row, frame, get):
    op = row['operation']
    handoff = op in HANDOFF
    at = 160
    h = frame[at:at + (256 if handoff else 192)]
    check(h[:4] == (b'XHG1' if handoff else b'XSG1'), 'semantic owner header')
    check(number(h, 4, 2) == 2 and number(h, 6, 2) == op and number(h, 10, 2) == 2, 'scope2/op')
    descriptors = 2 if handoff else 1
    descriptor_at = at + len(h)
    if handoff or op == 3:
        p = row['plan']; expected = actor(1 if p['local'] == 1 else 2, p['generation'])
        check(frame[72:112] == expected[:40], 'outer original transaction identity')
        check(frame[descriptor_at + 24:descriptor_at + 72] == expected, 'descriptor original actor')
    for i in range(descriptors):
        d = frame[descriptor_at + i * 1536:descriptor_at + (i + 1) * 1536]
        check(len(d) == 1536 and d[-32:] == digest(d[:-32]), 'descriptor digest')
        prior = row['transaction_before'] if i == 0 and (handoff or op == 3) else row['aging_before']
        offset = 152 + 32 * i if handoff else 88
        check(h[offset:offset + 32] == (digest(get(prior)) if prior else bytes(32)), 'sender-before SHA')
    native_at = descriptor_at + descriptors * 1536 + 256
    native_bytes = number(h, 228 if handoff else 136, 4)
    native_count = number(h, 236 if handoff else 124, 2)
    redo_count = number(h, 238 if handoff else 132, 2)
    aux_bytes = number(h, 232, 4) if handoff else sum(number(h, i, 4) for i in (140, 144, 148))
    redo_bytes = number(h, 240 if handoff else 152, 4)
    check(native_count == redo_count == len(row['after']), 'native/redo/changed bijection')
    position = native_at
    natives = []
    for _ in range(native_count):
        n = frame[position:position + 128]
        length = number(n, 20, 4)
        check(length >= 128 and position + length <= native_at + native_bytes, 'native length')
        cursor = position + 128
        for bit, slot_at, off_at, len_at, hash_at in ((1, 4, 8, 12, 24), (2, 6, 10, 16, 56)):
            selected, offset, size = number(n, slot_at, 2), number(n, off_at, 2), number(n, len_at, 4)
            if n[1] & bit:
                body = frame[cursor:cursor + size]; cursor += size
                check(selected != 65535 and digest(body) == n[hash_at:hash_at + 32], 'native selected bytes digest')
            else:
                check(selected == 65535 and offset == size == 0 and n[hash_at:hash_at + 32] == bytes(32), 'native absent canonical')
        check(cursor == position + length, 'native body consumption')
        natives.append(n)
        position += length
    check(position == native_at + native_bytes, 'native section consumption')
    position += aux_bytes
    end = position + redo_bytes
    after = {number(get(h), 32): get(h) for h in row['after'].values()}
    prior_number = -1
    for index in range(redo_count):
        r = frame[position:position + 64]; length = number(r, 4, 4)
        page_number = number(r, 16)
        check(page_number > prior_number and page_number in after, 'canonical actual page order')
        prior_number = page_number; image = after[page_number]
        check(r[8:48] == image[24:64] and r[1] == image[6], 'redo identity/kind')
        data = frame[position + 64:position + length]
        offset, size = number(r, 56, 2), number(r, 58, 2)
        check(length == 64 + size and data == image[offset:offset + size], 'exact canonical redo bytes')
        n = natives[index]
        if n[1] & 2:
            selected = number(n, 6, 2); body, actual_offset = cell(image, selected)
            check(actual_offset == number(n, 10, 2) and len(body) == number(n, 16, 4)
                  and digest(body) == n[56:88], 'native after vs whole page')
        position += length
    check(position == end == len(frame) - 32, 'redo/frame exact end')


def run():
    start = time.monotonic()
    plan_raw = (HERE / 'input-plan.json').read_bytes()
    check(digest(plan_raw).hex() == PLAN_SHA, 'adopted plan')
    plan = json.loads(plan_raw)
    manifest = json.loads((HERE / 'manifest.json').read_bytes())
    check(manifest['plan_sha256'] == PLAN_SHA and manifest['authority'] == 'NONE', 'manifest plan/authority')
    index = (HERE / 'raw-index.tsv').read_bytes()
    check(digest(index).hex() == manifest['raw_index_sha256'], 'index binding')
    check(digest((HERE / 'steps.tsv').read_bytes()).hex() == manifest['steps_sha256'], 'steps binding')
    raw = {}
    for line in index.decode().splitlines()[1:]:
        h, length, relative = line.split('\t')
        check(relative == f'raw/{h}.bin' and h not in raw, 'unique fixed raw path')
        data = (HERE / relative).read_bytes()
        check(len(data) == int(length) <= plan['limits']['raw_file_bytes'] and digest(data).hex() == h, 'full raw receipt')
        if len(data) == 16384 and data[:4] == b'XDP1':
            check(data[-32:] == digest(data[:-32]), 'page digest')
        raw[h] = data
    check({p.name for p in (HERE / 'raw').iterdir()} == {h + '.bin' for h in raw}, 'complete raw inventory')
    check(len(raw) == manifest['raw_count'] and sum(map(len, raw.values())) == manifest['raw_bytes']
          <= plan['limits']['raw_corpus_bytes'], 'corpus exact/cap')
    get = lambda h: raw[h] if h else None
    rows = manifest['rows']
    check(len(rows) == 56 and len({r['id'] for r in rows}) == 56, 'exact 56 row inventory')
    for scenario, last in (('N01', 25), ('N02', 16), ('N03', 15)):
        selected = [r for r in rows if r['scenario'] == scenario]
        check([r['lsn'] for r in selected] == list(range(1, last + 1)), ('closed LSN inventory', scenario))
        offset = {}
        for row in selected:
            frame = get(row['frame']); lsn = row['lsn']
            epoch = next(p['epoch'] for p in plan['scenarios'][scenario]['phases'] if p['lsns'][0] <= lsn <= p['lsns'][1])
            check(frame[:6] == b'XWF1\x01\x00' and frame[128:160] == digest(frame[:128])
                  and frame[-32:] == digest(frame[:-32]), 'outer envelope')
            check(number(frame, 48, 4) == row['epoch'] == epoch and number(frame, 56) == lsn
                  and number(frame, 64) == lsn - 1, 'epoch/absolute LSN')
            expected_size = SIZES[row['operation']] if row['kind'] == 'semantic' else {2: 21200, 3: 38000, 4: 21200, 5: 38208}[lsn]
            check(row['bytes'] == len(frame) == number(frame, 12, 4) == expected_size, 'independent frame length')
            check(row['wal_start'] == offset.get(epoch, 4096), 'independent file start')
            offset[epoch] = row['wal_start'] + expected_size
            check(row['wal_end'] == offset[epoch], 'independent file end')
            if row['kind'] == 'semantic':
                semantic_frame(row, frame, get)
            root_hash = row['after'].get('1', row['before'].get('1'))
            root_check(get(root_hash), get(row['jobs_after']), get(row['aging_after']))
        final = next(c for c in manifest['checkpoints'] if c['scenario'] == scenario and c['lsn'] == last)
        check(final['scalar'] == plan['scenarios'][scenario]['final'], ('independent final scalar', scenario))
    for point in manifest['checkpoints']:
        check(set(point['pages']) == {str(n) for n in range(64)}, 'whole first extent map')
        allocator = cell(get(point['pages']['0']))[0]
        for n in range(64):
            entry = allocator[64 + 96 * n:64 + 96 * (n + 1)]
            image = get(point['pages'][str(n)])
            if entry[0] == 2:
                check(image[:4] == b'XDP1' and number(image, 32) == n
                      and number(image, 40) == number(entry, 8)
                      and image[6] == entry[1] and image[48:64] == entry[16:32], 'allocated exact page identity')
            else:
                check(image == bytes(16384), 'Free/Reserved logical arena zero')
        root = root_check(get(point['pages']['1']), get(point['jobs']), get(point['aging']))
        for key, at in [('revision', 64), ('cursor', 72), ('issued', 88), ('completed', 376),
                        ('canceled', 384), ('undo_high', 392), ('object_high', 400), ('C', 408), ('aging_consumed', 416)]:
            check(point['scalar'][key] == number(root, at), ('checkpoint raw scalar', key))
    for row in [r for r in rows if r['scenario'] == 'N03']:
        original = next(r for r in rows if r['id'] == row['source_row'])
        check(all(row[k] == v for k, v in original.items() if k not in ('id', 'scenario')), 'N03 exact prefix reuse')
    check(time.monotonic() - start < plan['limits']['auditor_seconds'], 'audit deadline')
    return dict(status='INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION', rows=56, raw_count=len(raw),
                raw_bytes=sum(map(len, raw.values())), checkpoints=len(manifest['checkpoints']),
                manifest_sha256=digest((HERE / 'manifest.json').read_bytes()).hex(),
                wall_seconds=time.monotonic() - start)


if __name__ == '__main__':
    result = run()
    print(json.dumps(result, sort_keys=True))
