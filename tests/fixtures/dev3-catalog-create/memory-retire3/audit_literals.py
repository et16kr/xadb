#!/usr/bin/env python3
"""Profile3 MF14 read-only raw auditor; no serializer or product imports.

A copy of the v2 memory-retire auditor. The native raw-parser skeleton is
preserved as text, with the explicit MF14 actor, durable epoch1 cut and
recovery epoch2 schedule. Changes: version3 Root/Job wire, the Root3
row_high/reserved tail and the Profile3 profile blob; the N01/N02 and
checkpoint comparisons use native3 and memory-gap3; the cross-version
transition R comparison is replaced by schedule equivalence with the frozen v2
memory-retire corpus (Profile-bound bytes differ).
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
PLAN_SHA = '0a6efde6c8e0c6e39217765af970802dd7a0a184d93e2aa9f98d97dcbf3a8017'
PROFILE3_ID1 = 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e'
HANDOFF = {1, 2, 4, 6, 7, 11, 12}
SIZES = {1: 56712, 2: 154094, 3: 3040, 4: 120238, 6: 56712, 7: 56712,
         11: 86640, 12: 69304, 16: 19672, 17: 55112, 20: 90004, 21: 55528, 22: 55528}
OPERATIONS = [(1, 'begin'), (11, 'reserve'), (2, 'forward'), (3, 'rollback'), (4, 'clr'),
              (7, 'abort'), (21, 'release1'), (21, 'release2'), (21, 'release3'),
              (22, 'rootfree'), (17, 'finish')]
# (lsn, operation, outer epoch, WAL file start, WAL file end); epoch2 restarts at4096.
SCHEDULE = [(1, 16, 1, 4096, 23768), (2, 8, 1, 23768, 44968), (3, 10, 1, 44968, 82968),
            (4, 8, 1, 82968, 104168), (5, 10, 1, 104168, 142376), (6, 1, 1, 142376, 199088),
            (7, 11, 1, 199088, 285728), (8, 2, 1, 285728, 439822), (9, 3, 1, 439822, 442862),
            (10, 4, 2, 4096, 124334), (11, 7, 2, 124334, 181046), (12, 21, 2, 181046, 236574),
            (13, 21, 2, 236574, 292102), (14, 21, 2, 292102, 347630), (15, 22, 2, 347630, 403158),
            (16, 17, 2, 403158, 458270)]
CHECKPOINT_SCALARS = {
    5: {'lsn': 5, 'revision': 2, 'issued': 0, 'completed': 0, 'canceled': 0, 'cursor': 0,
        'undo_high': 0, 'object_high': 5, 'C': 0, 'aging_consumed': 19672, 'Q': 0,
        'aging_groups': 1, 'aging_remaining': 0},
    9: {'lsn': 9, 'revision': 5, 'issued': 2, 'completed': 0, 'canceled': 0, 'cursor': 0,
        'undo_high': 1, 'object_high': 6, 'C': 0, 'aging_consumed': 19672, 'Q': 0,
        'aging_groups': 4, 'aging_remaining': 367228}}
STATE_KEYS = ('before', 'after', 'jobs_before', 'jobs_after', 'transaction_before',
              'transaction_after', 'aging_before', 'aging_after', 'system_before', 'system_after')
CHECKPOINT_KEYS = ('lsn', 'scalar', 'pages', 'jobs', 'aging', 'system', 'active_transaction',
                   'root_masks', 'live_tables')


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


def actor(epoch, generation, local=1):
    return bytes(range(1, 17)) + struct.pack('<QI4xQQ', 1, epoch, local, generation)


def root_check(page, jobs, aging, expected_actor):
    root = cell(page)[0]
    check(root[:6] == b'XQR1\x03\x00' and len(root) == 512, 'Root3 wire')
    check(number(root, 432) == 0 and not any(root[440:512]), 'Catalog-only Root3 row_high0 and zero tail')
    check(len(jobs) == 65536, 'whole Jobs')
    masks = [0, 0, 0]
    steps = remaining = 0
    for index in range(64):
        job = jobs[index * 1024:(index + 1) * 1024]
        check(job[:6] == b'XQJ1\x03\x00' and job[7] != 3, ('Job3 wire, no Row kind', index))
        state = job[6]
        masks[0 if state == 0 else 1 if state == 1 else 2] |= 1 << index
        if state:
            check(job[16:64] == expected_actor, ('Job actor', index))
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


def semantic_frame(row, frame, get, expected_actor):
    op = row['operation']
    handoff = op in HANDOFF
    at = 160
    h = frame[at:at + (256 if handoff else 192)]
    check(h[:4] == (b'XHG1' if handoff else b'XSG1'), 'semantic owner header')
    check(number(h, 4, 2) == 2 and number(h, 6, 2) == op and number(h, 10, 2) == 2, 'scope2/op')
    descriptors = 2 if handoff else 1
    descriptor_at = at + len(h)
    if handoff or op == 3:
        expected = expected_actor
        check(frame[72:112] == expected[:40], 'outer original transaction identity')
        check(frame[descriptor_at + 24:descriptor_at + 72] == expected, 'descriptor original actor')
    else:
        check(frame[72:112] == bytes(40), 'nontransaction outer owner canonical zero')
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


def current_identities(row, get, expected):
    for key in ('transaction_before', 'transaction_after'):
        if row[key]:
            descriptor = get(row[key])
            check(descriptor[24:72] == expected, ('original transaction actor', row['id'], key))
            check(descriptor[-32:] == digest(descriptor[:-32]), 'transaction descriptor seal')
    for role, h in row['after'].items():
        page = get(h)
        if role == '4':
            status = cell(page)[0]
            if status[38]:
                check(status[40:88] == expected, 'Status original actor')
            check(number(status, 80) == (2 if row['lsn'] >= 6 else 1), 'Status generation independent of local')
        elif role == '5':
            undo = cell(page)[0]
            check(number(undo, 8) == (2 if row['lsn'] >= 7 else 1), 'Undo generation independent of local')
            if undo[6]:
                check(undo[24:72] == expected and number(undo, 16) == 1, 'Undo creator/sequence')
        elif role == '3':
            for index in range(1, number(page, 72, 2)):
                leaf = cell(page, index)[0]
                check(number(leaf, 0, 2) == 8 and leaf[4:12] == (6).to_bytes(8, 'big'), 'Catalog first object key')
                value = leaf[12:]
                check(value[:4] == b'XCT1' and number(value, 8) == 6, 'Catalog first object')
                check(value[88:128] == expected[:40], 'Catalog original logical creator')
                check(value[32] == 2 and value[40:88] == expected, 'Catalog uncommitted transaction actor')


def continuity(rows):
    for prior, row in zip(rows, rows[1:]):
        for key in ('jobs', 'transaction', 'aging'):
            check(row[key + '_before'] == prior[key + '_after'], ('row continuity', row['id'], key))
        check(set(row['before']) == set(prior['before']) | set(prior['after']), ('page role continuity', row['id']))
        for role, h in row['before'].items():
            check(h == prior['after'].get(role, prior['before'].get(role)), ('page continuity', row['id'], role))


def retire_specific(plan, rows, get, expected_actor):
    spec = plan['scenarios']['MF14']
    failure = spec['failure']
    by_lsn = {r['lsn']: r for r in rows}
    durable = [r for r in rows if r['epoch'] == 1]
    recovery = [r for r in rows if r['epoch'] == 2]
    check([r['lsn'] for r in durable] == list(range(1, 10)) and durable[-1]['operation'] == 3,
          'durable epoch1 history ends at RollbackStart9')
    check(failure['durable_last_lsn'] == 9 and failure['wal_file_delta'] == 0
          and failure['staged_lsn'] == 10 and failure['staged_operation'] == 4
          and failure['staged_durable'] is False and failure['effect'] == 'Fenced'
          and failure['retirement_marker'] == 'retained', 'declared staged-unwind failure')
    check(durable[-1]['wal_end'] == failure['durable_wal_end'] == 442862, 'no staged CLR bytes in epoch1 WAL file')
    check([r['lsn'] for r in recovery] == list(range(10, 17)) and recovery[0]['wal_start'] == 4096,
          'recovery epoch2 restarts at file offset4096')
    check([r['lsn'] for r in rows if r['operation'] == 4] == [10] and by_lsn[10]['epoch'] == 2,
          'only the recovery CLR10 exists')
    rollback, clr = by_lsn[9], by_lsn[10]
    frame9, frame10 = get(rollback['frame']), get(clr['frame'])
    check(number(frame9, 112) == 8 and number(frame9, 160 + 24) == 8, 'RollbackStart9 chains to Forward8 Tx')
    check(clr['transaction_before'] == rollback['transaction_after'], 'CLR10 before Tx is RollbackStart9 after Tx')
    check(number(get(clr['transaction_before']), 72) == 9 and number(frame10, 112) == 9
          and number(frame10, 160 + 24) == 9, 'recovery CLR10 chains to durable RollbackStart9')
    tx9 = get(rollback['transaction_after'])
    check([number(tx9, at) for at in (72, 80, 88, 624)] == [9, 8, 6, 4], 'RollbackStart9 Tx chain')
    check(cell(get(rollback['after']['4']))[0][38] == 2, 'RollbackStart9 Status aborting')
    check(rollback['after'].keys() == {'4'} and rollback['aging_after'] == rollback['aging_before']
          and rollback['jobs_after'] == rollback['jobs_before'], 'RollbackStart9 changes Status only')
    catalog = get(clr['after']['3'])
    check(number(catalog, 72, 2) == 1, 'Catalog T absent after CLR10')
    allocator = cell(get(clr['after']['0']))[0]
    for n in spec['logical']['pair']:
        entry = allocator[64 + 96 * n:64 + 96 * (n + 1)]
        check(entry[0] == 0 and number(entry, 8) == 2, ('pair Free after CLR10', n))
    undo = cell(get(clr['after']['5']))[0]
    check(undo[6] == 2 and undo[328] == 0 and number(undo, 8) == 2, 'Undo Releasing0 after CLR10')
    root_job = get(clr['jobs_after'])[1024:2048]
    check(root_job[6] == 4 and root_job[7] == 2 and number(root_job, 272) == 10, 'RootJob compensated at recovery CLR10')
    for lsn, cursor in ((12, 1), (13, 2), (14, 3)):
        job = get(by_lsn[lsn]['jobs_after'])[1024:2048]
        check(job[6] == 4 and job[121] == cursor and number(job, 272) == 10, ('ReleaseRole cursor', lsn))
    terminal = get(by_lsn[11]['jobs_after'])[:1024]
    check(terminal[6] == 2 and terminal[7] == 1 and terminal[120] == 2 and number(terminal, 264) == 0
          and number(terminal, 72) == 11, 'AbortEnd11 aborted terminal outcome q0')
    check(cell(get(by_lsn[11]['after']['4']))[0][38] == 4, 'AbortEnd11 Status aborted')
    final_jobs = get(by_lsn[16]['jobs_after'])
    check(all(final_jobs[i * 1024 + 6] == final_jobs[i * 1024 + 7] == 0 for i in range(64)), 'final Jobs all Free')
    check(cell(get(by_lsn[16]['after']['4']))[0][38] == 0, 'TerminalFinish16 Status free')
    check(rollback['plan']['local'] == 1, 'MF14 actor local1')
    for row in rows:
        if row['lsn'] >= 6:
            p = row['plan']
            check((p['local'], p['generation'], p['ug'], p['sequence'], p['object'], p['q'], p['outcome'],
                   p['pair'], p['inc'], p['terminal'], p['root'], p['terminal_gen'], p['root_gen'])
                  == (1, 2, 2, 1, 6, 0, 'aborted', [11, 12], 2, 0, 1, 2, 2), 'MF14 first rollback cycle')
    check(expected_actor == actor(1, 2, 1), 'actor epoch1/local1/generation2')


def schedule_equivalence(manifest):
    """Same history shape as the frozen v2 memory-retire corpus; only Profile-bound bytes differ."""
    old = json.loads((HERE.parent / 'memory-retire' / 'manifest.json').read_bytes())
    keys = ('id', 'scenario', 'lsn', 'epoch', 'kind', 'operation', 'bytes', 'wal_start', 'wal_end')
    check(len(old['rows']) == len(manifest['rows']), 'v2 row count')
    for new, prior in zip(manifest['rows'], old['rows']):
        check(all(new[k] == prior[k] for k in keys), ('v2 schedule', new['id']))
        check(set(new['before']) == set(prior['before']) and set(new['after']) == set(prior['after']),
              ('v2 page roles', new['id']))
        check(new['frame'] != prior['frame'], ('Profile-bound frame differs', new['id']))
    check([(c['scenario'], c['lsn'], c['scalar']) for c in manifest['checkpoints']]
          == [(c['scenario'], c['lsn'], c['scalar']) for c in old['checkpoints']], 'v2 checkpoint scalars')


def original_comparisons(plan, manifest, get):
    for path, expected in {**plan['source_inputs'], **plan['serializer_parent'], **plan['immutable_existing']}.items():
        check(digest((REPO / path).read_bytes()).hex() == expected, ('original source preserved', path))
    rows = manifest['rows']
    native_dir = HERE.parent / 'native3'
    native = json.loads((native_dir / 'manifest.json').read_bytes())
    n01 = {r['lsn']: r for r in native['rows'] if r['scenario'] == 'N01'}
    n02 = {r['lsn']: r for r in native['rows'] if r['scenario'] == 'N02'}
    check(sorted(n02) == list(range(1, 17)), 'native N02 inventory')
    for row in rows:
        lsn = row['lsn']
        if lsn <= 5:
            for key in ('frame',) + STATE_KEYS:
                check(row[key] == n01[lsn][key], ('original common N01 prefix', row['id'], key))
        original = n02[lsn]
        for key in STATE_KEYS:
            check(row[key] == original[key], ('native N02 state', row['id'], key))
        check((row['kind'], row['operation'], row['bytes']) == (original['kind'], original['operation'], original['bytes']),
              ('native N02 operation', row['id']))
        same_epoch = lsn <= 8 or lsn == 10
        check((row['epoch'] == original['epoch']) == same_epoch, ('declared N02 epoch relation', row['id']))
        if same_epoch:
            check(row['frame'] == original['frame'], ('N02 identical frame', row['id']))
        else:
            old = (native_dir / 'raw' / (original['frame'] + '.bin')).read_bytes()
            check(digest(old).hex() == original['frame'], 'original N02 frame receipt')
            current = get(row['frame'])
            check(len(current) == len(old) and current[:48] == old[:48] and current[52:128] == old[52:128]
                  and current[160:-32] == old[160:-32], ('MF14 only outer epoch and seals differ from N02', row['id']))
            check(number(current, 48, 4) == row['epoch'] and number(old, 48, 4) == original['epoch'], 'N02 epoch bytes')
    schedule_equivalence(manifest)
    native_points = {c['lsn']: c for c in native['checkpoints'] if c['scenario'] == 'N02'}
    gap = json.loads((HERE.parent / 'memory-gap3' / 'manifest.json').read_bytes())
    gap_points = {c['lsn']: c for c in gap['checkpoints'] if c['scenario'] == 'MC04'}
    for point in manifest['checkpoints']:
        original = gap_points[5] if point['lsn'] == 5 else native_points[point['lsn']]
        for key in CHECKPOINT_KEYS:
            check(point[key] == original[key], ('original checkpoint equality', point['lsn'], key))
    check(len(manifest['checkpoints']) == 3, 'exact three checkpoints')
    for scenario, spec in plan['scenarios'].items():
        check([p['lsn'] for p in manifest['checkpoints'] if p['scenario'] == scenario]
              == spec['checkpoints'], 'exact checkpoint LSN inventory')


def run():
    start = time.monotonic()
    plan_raw = (HERE / 'input-plan.json').read_bytes()
    check(digest(plan_raw).hex() == PLAN_SHA, 'adopted plan')
    plan = json.loads(plan_raw)
    check(list(plan['scenarios']) == ['MF14'], 'single MF14 scenario')
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
    check(len(rows) == 16 and len({r['id'] for r in rows}) == 16, 'exact 16 row inventory')
    tsv = (HERE / 'steps.tsv').read_text().splitlines()
    fields = tsv[0].split('\t')
    check(len(tsv) == 17, 'steps exact row count')
    for line, row in zip(tsv[1:], rows):
        values = []
        for field in fields:
            value = row[field]
            if isinstance(value, dict):
                value = ','.join(f'{k}:{v}' for k, v in sorted(value.items(), key=lambda item: int(item[0])))
            values.append('-' if value is None else str(value))
        check(line.split('\t') == values, 'steps/manifest exact agreement')
    spec = plan['scenarios']['MF14']
    check([tuple(item) for item in spec['operations']] == OPERATIONS, 'declared rollback operation order')
    check([(p['epoch'], p['lsns']) for p in spec['phases']] == [(1, [1, 9]), (2, [10, 16])]
          and spec['phases'][1]['selected_capture'] == 9, 'declared epoch phases')
    expected_actor = actor(1, 2, 1)
    check(all(r['scenario'] == 'MF14' and r['id'] == f'MF14-L{r["lsn"]:02}' for r in rows), 'MF14 row identity')
    check([r['lsn'] for r in rows] == list(range(1, 17)), 'closed LSN inventory')
    check([r['operation'] for r in rows] == [16, 8, 10, 8, 10] + [op for op, _ in OPERATIONS],
          'exact operation inventory')
    check([(r['lsn'], r['operation'], r['epoch'], r['wal_start'], r['wal_end']) for r in rows] == SCHEDULE,
          'exact LSN/op/epoch/WAL offset schedule')
    offset = {}
    for row in rows:
        frame = get(row['frame']); lsn = row['lsn']
        epoch = next(p['epoch'] for p in spec['phases'] if p['lsns'][0] <= lsn <= p['lsns'][1])
        check(row['kind'] == ('system' if 2 <= lsn <= 5 else 'semantic'), 'row kind')
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
            semantic_frame(row, frame, get, expected_actor)
        root_hash = row['after'].get('1', row['before'].get('1'))
        root_check(get(root_hash), get(row['jobs_after']), get(row['aging_after']), expected_actor)
        current_identities(row, get, expected_actor)
    continuity(rows)
    retire_specific(plan, rows, get, expected_actor)
    final = next(c for c in manifest['checkpoints'] if c['scenario'] == 'MF14' and c['lsn'] == 16)
    check(final['scalar'] == spec['final'], 'independent final scalar')
    by_lsn = {r['lsn']: r for r in rows}
    for point in manifest['checkpoints']:
        check(point['scenario'] == 'MF14', 'checkpoint scenario')
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
        root = root_check(get(point['pages']['1']), get(point['jobs']), get(point['aging']), expected_actor)
        for key, at in [('revision', 64), ('cursor', 72), ('issued', 88), ('completed', 376),
                        ('canceled', 384), ('undo_high', 392), ('object_high', 400), ('C', 408), ('aging_consumed', 416)]:
            check(point['scalar'][key] == number(root, at), ('checkpoint raw scalar', key))
        check(point['scalar']['Q'] == point['scalar']['C'] == 0, 'rollback Q/C zero')
        if point['lsn'] in CHECKPOINT_SCALARS:
            check(point['scalar'] == CHECKPOINT_SCALARS[point['lsn']], ('independent checkpoint scalar', point['lsn']))
        row = by_lsn[point['lsn']]
        check(point['jobs'] == row['jobs_after'] and point['aging'] == row['aging_after'], 'checkpoint row agreement')
        for role, h in row['after'].items():
            page_number = number(get(h), 32)
            check(point['pages'][str(page_number)] == h, ('checkpoint current changed page', point['lsn'], role))
    points = {p['lsn']: p for p in manifest['checkpoints']}
    check(points[5]['active_transaction'] is None and points[5]['live_tables'] == []
          and points[5]['root_masks'] == [2 ** 64 - 1, 0, 0], 'capture5 setup')
    check(points[9]['active_transaction'] == by_lsn[9]['transaction_after'] and points[9]['live_tables'] == [6]
          and points[9]['root_masks'] == [2 ** 64 - 4, 1, 2], 'selected capture9 aborting T')
    check(points[16]['active_transaction'] is None and points[16]['live_tables'] == []
          and points[16]['root_masks'] == [2 ** 64 - 1, 0, 0]
          and points[16]['pages']['11'] == points[16]['pages']['12'] == digest(bytes(16384)).hex(),
          'final READY/Missing T and Free pair')
    profile = get(manifest['profile'])
    check(profile[:6] == b'XDC1\x03\x00' and digest(profile).hex() == PROFILE3_ID1, 'Profile3 ID1 envelope')
    original_comparisons(plan, manifest, get)
    check(time.monotonic() - start < plan['limits']['auditor_seconds'], 'audit deadline')
    return dict(status='INPUT_AUDIT_PASS_NOT_PRODUCT_EXECUTION', rows=16, raw_count=len(raw),
                raw_bytes=sum(map(len, raw.values())), checkpoints=len(manifest['checkpoints']),
                manifest_sha256=digest((HERE / 'manifest.json').read_bytes()).hex(),
                wall_seconds=time.monotonic() - start)


if __name__ == '__main__':
    result = run()
    print(json.dumps(result, sort_keys=True))
