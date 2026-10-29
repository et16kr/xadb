#!/usr/bin/env python3
"""Independent Profile3 Catalog scope2 CREATE literals over hash-pinned parents.

Only pathlib/struct/hashlib/importlib. No XADB encoder/decoder, test helper or
other fixture author is imported or executed; parents are read as bytes only.
A second derivation cross-checks every accepted literal: the hash-pinned
reusable serializer3.py replays the same closed phases of the pinned version2
serializer with Profile3 primitives, and its bytes must be equal.

Profile3 keeps the Catalog CREATE meaning. The only byte changes against the
pinned Profile2 corpus are: Profile envelope version3; Root/Job cell version3;
Root3 row_high@432 (carried unchanged by every Catalog group); Job sender
digest@328 (SHA of the before Transaction descriptor, which carries the Profile
digest); Root Job digest over the version3 bank; page trailers; descriptor
Profile digest, Aging work bytes (page/Root cell SHA) and seals; frame native
Root/Job cells, digests, redo patches, group prior-descriptor digests and the
frame trailer.
"""
from pathlib import Path
import hashlib
import importlib.util
import struct
import sys

sys.dont_write_bytecode = True  # loading serializer3.py leaves no cache file

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT.parent.parent
PARENT = FIXTURES / 'dev3-catalog-create' / 'transition'
PAGE = 16384
TRAILER = PAGE - 32
ROW_HIGH = 7
PROFILE2 = 'b8aab8141cbc1807844468c353d62b759e5001d5c3f8f82e54c1f4efeadd3b24'
SERIALIZER3 = '1e77315f0d397df090b9745e8f21f342f8948507757ee25b183601a23724788f'
CASES = ['G01', 'C01', 'C02', 'C03', 'C04']
ROOT_KIND, JOBS_KIND = 12, 13          # page kinds RetentionRoot / RetentionJobs
NATIVE_ROOT, NATIVE_JOB = 8, 9         # native record kinds of those pages


def sha(raw):
    return hashlib.sha256(raw).digest()


def hexsha(raw):
    return hashlib.sha256(raw).hexdigest()


def get(raw, at, width=8):
    return struct.unpack_from({2: '<H', 4: '<I', 8: '<Q'}[width], raw, at)[0]


def put(out, at, value, width=8):
    struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[width], out, at, value)


def pinned(path, expected):
    data = path.read_bytes()
    assert hexsha(data) == expected, path
    return data


def parent_raw(key):
    return pinned(PARENT / 'raw' / f'{key}.bin', key)


# --- XDP1 page: slot directory at 128 (start u16, length u16), SHA trailer.
def cells(page):
    return [(get(page, 128 + 4 * i, 2), get(page, 130 + 4 * i, 2))
            for i in range(get(page, 72, 2))]


def cell(page, slot):
    start, length = cells(page)[slot]
    return bytes(page[start:start + length])


def with_cells(page, replacement):
    out = bytearray(page)
    for slot, value in replacement.items():
        start, length = cells(page)[slot]
        assert len(value) == length
        out[start:start + length] = value
    out[TRAILER:] = sha(bytes(out[:TRAILER]))
    return bytes(out)


def reseal(page):
    assert page[TRAILER:] == sha(page[:TRAILER])


# --- XQJ1/XQR1 cells: version u16@4. Root Job digest@128, row_high@432.
# Forward/Commit/AbortEnd store SHA(before Transaction descriptor) at Job 328.
SENDERS = {}  # SHA(Profile2 descriptor) -> SHA(the same descriptor under Profile3)


def job3(value):
    assert value[:4] == b'XQJ1' and get(value, 4, 2) == 2
    out = bytearray(value)
    put(out, 4, 3, 2)
    if any(value[328:360]):
        out[328:360] = SENDERS[bytes(value[328:360])]
    return bytes(out)


def root3(value, bank2, row_high):
    assert value[:4] == b'XQR1' and get(value, 4, 2) == 2
    assert value[128:160] == sha(b''.join(bank2)), 'parent Root digest binds bank'
    assert value[432:512] == bytes(80)
    out = bytearray(value)
    put(out, 4, 3, 2)
    out[128:160] = sha(b''.join(job3(c) for c in bank2))
    put(out, 432, row_high)
    return bytes(out)


def root_page3(page, bank2, row_high):
    reseal(page)
    assert page[6] == ROOT_KIND
    return with_cells(page, {0: root3(cell(page, 0), bank2, row_high)})


def jobs_page3(page):
    reseal(page)
    assert page[6] == JOBS_KIND
    return with_cells(page, {i: job3(cell(page, i)) for i in range(len(cells(page)))})


def after_bank(bank2, jobs_page):
    # A group replaces at most one Job cell; the after Jobs page carries it.
    bank = list(bank2)
    index = get(jobs_page, 32) - 2
    for slot in range(len(cells(jobs_page))):
        bank[index * 15 + slot] = cell(jobs_page, slot)
    return bank


# --- XOD1 descriptor: ControlRef@96 (digest@128), Aging work@384 (page SHA
# @416, Root cell SHA@448), seal over [0,1504).
class Profiles:
    def __init__(self):
        self.p2 = parent_raw(PROFILE2)
        p3 = bytearray(self.p2)
        put(p3, 4, 3, 2)
        p3[4064:] = sha(bytes(p3[:4064]))
        self.p3 = bytes(p3)
        envelopes = pinned(FIXTURES / 'module-row-format' / 'envelopes' / 'profiles.bin',
                           'e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a')
        assert self.p3 == envelopes[:4096], 'cross-check: earlier Profile3 ID1 author'

    def descriptor(self, raw, root_page2=None, root_page3=None):
        assert raw[:4] == b'XOD1' and raw[1504:] == sha(raw[:1504])
        assert raw[128:160] == sha(self.p2)
        out = bytearray(raw)
        out[128:160] = sha(self.p3)
        if root_page2 is not None:
            assert raw[16] == 2, 'System Aging owner'
            assert raw[416:448] == sha(root_page2) and raw[448:480] == sha(cell(root_page2, 0))
            out[416:448] = sha(root_page3)
            out[448:480] = sha(cell(root_page3, 0))
        else:
            assert raw[16] == 1, 'Transaction owner'
        out[1504:] = sha(bytes(out[:1504]))
        return bytes(out)


# --- XWF1 frame: 160B header, XHG(256)/XSG(192) group, descriptors, core,
# native records, auxiliaries, redo records, SHA trailer.
def frame3(raw, profiles, before2, before3, after_pages2, after_pages3):
    out = bytearray(raw)
    handoff = raw[7] == 3
    group = 256 if handoff else 192
    owners = 2 if handoff else 1
    payload = raw[160:]
    native_at = 160 + group + 1536 * owners + 256
    if handoff:
        native_bytes, aux_bytes = get(payload, 228, 4), get(payload, 232, 4)
    else:
        native_bytes = get(payload, 136, 4)
        aux_bytes = sum(get(payload, at, 4) for at in (140, 144, 148))
    # Group prior digests are SHA of the before descriptors.
    for ordinal in range(owners):
        at = 160 + (152 + 32 * ordinal if handoff else 88)
        if before2[ordinal] is None:
            assert raw[at:at + 32] == bytes(32)
        else:
            assert raw[at:at + 32] == sha(before2[ordinal])
            out[at:at + 32] = sha(before3[ordinal])
    # After descriptors: Aging is ordinal1 of XHG and the only XSG owner here.
    for ordinal in range(owners):
        at = 160 + group + 1536 * ordinal
        aging = ordinal == 1 or not handoff
        out[at:at + 1536] = profiles.descriptor(
            raw[at:at + 1536],
            after_pages2[1] if aging else None,
            after_pages3[1] if aging else None)
    at = native_at
    while at < native_at + native_bytes:
        kind, flags = raw[at], raw[at + 1]
        before_len, after_len, total = get(raw, at + 12, 4), get(raw, at + 16, 4), get(raw, at + 20, 4)
        if kind in (NATIVE_ROOT, NATIVE_JOB):
            role = 1 if kind == NATIVE_ROOT else 2
            body = at + 128
            if flags & 1:
                old = raw[body:body + before_len]
                new = (cell(before3['root'], 0) if kind == NATIVE_ROOT
                       else job3(old))
                assert kind == NATIVE_JOB or old == cell(before2['root'], 0)
                out[body:body + before_len] = new
                out[at + 24:at + 56] = sha(new)
                body += before_len
            if flags & 2:
                old = raw[body:body + after_len]
                slot = get(raw, at + 6, 2)
                assert old == cell(after_pages2[role], slot)
                new = cell(after_pages3[role], slot)
                out[body:body + after_len] = new
                out[at + 56:at + 88] = sha(new)
        at += total
    assert at == native_at + native_bytes
    at += aux_bytes
    while at < len(raw) - 32:
        length, page_kind = get(raw, at + 4, 4), raw[at + 1]
        if page_kind in (ROOT_KIND, JOBS_KIND):
            role = 1 if page_kind == ROOT_KIND else 2
            assert raw[at] == 1 and get(raw, at + 56, 2) == 72
            assert raw[at + 64:at + length] == after_pages2[role][72:TRAILER]
            out[at + 64:at + length] = after_pages3[role][72:TRAILER]
        at += length
    assert at == len(raw) - 32
    out[-32:] = sha(bytes(out[:-32]))
    return bytes(out)


def serializer_cross_check(p3):
    """Replay G01 and C01..C04 with the reusable Profile3 serializer."""
    pinned(ROOT / 'serializer3.py', SERIALIZER3)
    spec = importlib.util.spec_from_file_location('serializer3', ROOT / 'serializer3.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    t = module.load()
    genesis = t.fresh(t.profile())
    started, ordinal = t.successor(genesis, {}, 16, 'aging', 1)
    states = {'G01': (genesis, started, {}, 16, ordinal)}
    before = t.setup(started)
    before['row_high'] = ROW_HIGH  # the synthetic Root3 high-water of C01..C04
    t.refresh_root(before)
    before['tx'] = None
    p = t.plan(1)
    for number, (op, phase) in enumerate(t.COMMIT[:4], 1):
        after, ordinal = t.successor(before, p, op, phase, before['last_lsn'] + 1)
        states[f'C{number:02}'] = (before, after, p, op, ordinal)
        before = after
    for cid, (b, a, plan, op, ordinal) in states.items():
        wire, roles = t.frame(b, a, plan, op, ordinal)
        expect = p3[cid]
        observed = {'jobs': b''.join(b['jobs']), 'root': b['images'][1],
                    'jobs_page': b['images'].get(2), 'allocator': b['images'][0],
                    'anchor': b['anchor'], 'catalog': b['images'].get(3),
                    'status': b['images'].get(4), 'undo': b['images'].get(5),
                    'transaction': b['tx'], 'aging': b['aging'], 'frame': wire,
                    **{f'after{r}': a['images'][r] if r in roles else None for r in range(8)}}
        for name, value in observed.items():
            assert value == expect[name], ('serializer3 cross-check', cid, name)


class Blobs:
    def __init__(self):
        self.data = bytearray()
        self.index = {}

    def add(self, raw):
        key = hexsha(raw)
        if (PARENT / 'raw' / f'{key}.bin').exists():
            return key  # identical parent blob: the harness reads the parent
        if key not in self.index:
            self.index[key] = (len(self.data), len(raw))
            self.data += raw
        return key


def write():
    pinned(PARENT / 'cases.tsv', 'f5091dfb128b1c49bfe5b00d8e96628b81b9c7ce215c47ae23c44736940fdf26')
    rows = {}
    for line in (PARENT / 'cases.tsv').read_text().splitlines():
        fields = line.split('\t')
        rows[fields[0]] = fields
    profiles = Profiles()
    blobs = Blobs()
    blob = lambda f, at: None if f[at] == '-' else parent_raw(f[at])
    out_rows = []
    columns = ['jobs', 'root', 'jobs_page', 'allocator', 'anchor', 'catalog', 'status',
               'undo', 'transaction', 'aging', 'frame'] + [f'after{r}' for r in range(8)]
    parent_at = {'jobs': 17, 'anchor': 18, 'transaction': 19, 'aging': 20, 'frame': 21,
                  'allocator': 23, 'root': 24, 'jobs_page': 25, 'catalog': 26, 'status': 27,
                  'undo': 28, **{f'after{r}': 29 + r for r in range(8)}}
    profile3 = blobs.add(profiles.p3)
    for cid in CASES:
        tx2 = blob(rows[cid], 19)
        if tx2 is not None:
            SENDERS[sha(tx2)] = sha(profiles.descriptor(tx2))
    p3 = {}
    banks_after = {}
    for cid in CASES:
        f = rows[cid]
        bank_raw = blob(f, 17)
        bank2 = [bank_raw[i * 1024:(i + 1) * 1024] for i in range(64)]
        root2 = blob(f, 24)
        row_high = 0 if cell(root2, 0)[6] == 0 else ROW_HIGH
        root_3 = root_page3(root2, bank2, row_high)
        bank3 = b''.join(job3(c) for c in bank2)
        after2 = {r: blob(f, 29 + r) for r in range(8)}
        after3 = dict(after2)
        if after2[2] is not None:
            after3[2] = jobs_page3(after2[2])
            bank_after = after_bank(bank2, after2[2])
        else:
            bank_after = bank2
        banks_after[cid] = (b''.join(bank_after), b''.join(job3(c) for c in bank_after))
        after3[1] = root_page3(after2[1], bank_after, row_high)
        assert get(cell(after3[1], 0), 432) == row_high, 'Catalog group carries row_high'
        tx2, aging2 = blob(f, 19), blob(f, 20)
        tx3 = None if tx2 is None else profiles.descriptor(tx2)
        aging3 = None if aging2 is None else profiles.descriptor(aging2, root2, root_3)
        handoff = blob(f, 21)[7] == 3
        before2 = [tx2, aging2] if handoff else [aging2]
        before3 = [tx3, aging3] if handoff else [aging3]
        frame = frame3(blob(f, 21), profiles,
                       {**dict(enumerate(before2)), 'root': root2},
                       {**dict(enumerate(before3)), 'root': root_3},
                       after2, after3)
        values = {
            'jobs': bank3, 'root': root_3,
            'jobs_page': None if blob(f, 25) is None else jobs_page3(blob(f, 25)),
            'transaction': tx3, 'aging': aging3, 'frame': frame,
            **{f'after{r}': after3[r] for r in range(8)},
        }
        for name in ('allocator', 'anchor', 'catalog', 'status', 'undo'):
            values[name] = blob(f, parent_at[name])
        p3[cid] = values
        keys = {name: '-' if values[name] is None else blobs.add(values[name]) for name in columns}
        out_rows.append([cid + '-P3', 'ACCEPT', '3', cid] + [keys[c] for c in columns] + ['OK'])
        parent_keys = {name: f[parent_at[name]] for name in columns}
        out_rows.append([cid + '-P2', 'ACCEPT', '2', cid] + [parent_keys[c] for c in columns] + ['OK'])
    # Independent chain cross-checks: consecutive parent Roots are equal, so
    # are the Profile3 counterparts and the Aging predecessor descriptors.
    for a, b in [('C01', 'C02'), ('C02', 'C03'), ('C03', 'C04')]:
        assert rows[a][30] == rows[b][24]
        aging = slice(160 + 256 + 1536, 160 + 256 + 3072)
        assert parent_raw(rows[a][21])[aging] == parent_raw(rows[b][20])
        assert p3[a]['after1'] == p3[b]['root']
        assert p3[a]['frame'][aging] == p3[b]['aging']
        assert banks_after[a][0] == parent_raw(rows[b][17])
        assert banks_after[a][1] == p3[b]['jobs'], 'Profile3 bank chain incl. sender'
    root3_initial = pinned(FIXTURES / 'module-row-format' / 'root3' / 'initial-root.bin',
                           'd6079c1ea15ea2668be3150c6128d9be4a72e28855336ea7c1d971f73d0b3fc4')
    assert cell(p3['G01']['root'], 0) == root3_initial, 'cross-check: earlier Root3 author'
    serializer_cross_check(p3)

    def variant(cid, suffix, cls, profile, changes, expect):
        values = dict(p3[cid])
        values.update(changes)
        keys = {name: '-' if values[name] is None else blobs.add(values[name]) for name in columns}
        out_rows.append([f'{cid}-{suffix}', cls, profile, cid] + [keys[c] for c in columns] + [expect])

    def bank_with(cid, ordinal, value):
        bank = bytearray(p3[cid]['jobs'])
        bank[ordinal * 1024:(ordinal + 1) * 1024] = value
        return bytes(bank)

    def cell3(cid, ordinal):
        return p3[cid]['jobs'][ordinal * 1024:(ordinal + 1) * 1024]

    # Profile2 pages/descriptors under the Profile3 binding and the converse.
    f = rows['C04']
    out_rows.append(['C04-P2-UNDER-P3', 'REJECT_INPUT', '3', 'C04']
                    + [f[parent_at[c]] for c in columns] + ['Queue(UnsupportedFormat)'])
    variant('C04', 'P3-UNDER-P2', 'REJECT_INPUT', '2', {}, 'Queue(UnsupportedFormat)')
    # Root version4 and Job version2/4 in an otherwise Profile3 input.
    root = bytearray(cell(p3['C04']['root'], 0))
    put(root, 4, 4, 2)
    variant('C04', 'ROOT-V4', 'REJECT_INPUT', '3',
            {'root': with_cells(p3['C04']['root'], {0: bytes(root)})}, 'Queue(UnsupportedFormat)')
    for ordinal, version, name in [(40, 2, 'JOB-V2-FREE'), (0, 4, 'JOB-V4-SELECTED')]:
        value = bytearray(cell3('C04', ordinal))
        put(value, 4, version, 2)
        variant('C04', name, 'REJECT_INPUT', '3',
                {'jobs': bank_with('C04', ordinal, bytes(value))}, 'Queue(UnsupportedFormat)')
    # Row kind3/registry16 cells where the Catalog group expects its own Job.
    registry = 384 + 3 * 128 + 88
    reserved = bytearray(cell3('C03', 1))
    assert reserved[6:8] == b'\x01\x02' and get(reserved, registry) == 9
    reserved[7] = 3
    put(reserved, 320, 1)
    put(reserved, registry, 16)
    variant('C03', 'ROW-RESERVED', 'REJECT_INPUT', '3',
            {'jobs': bank_with('C03', 1, bytes(reserved))}, 'Unsupported')
    terminal = bytearray(cell3('C04', 0))
    assert terminal[6:8] == b'\x01\x01' and get(terminal, registry) == 9
    put(terminal, registry, 16)
    variant('C04', 'ROW-TERMINAL', 'REJECT_INPUT', '3',
            {'jobs': bank_with('C04', 0, bytes(terminal))}, 'Unsupported')
    retained = bytearray(cell3('C04', 1))
    assert retained[6:8] == b'\x02\x02' and cell3('C04', 5)[6:8] == b'\x00\x00'
    retained[7] = 3
    put(retained, 320, 2)
    put(retained, registry, 16)
    variant('C04', 'ROW-SAME-ACTOR', 'REJECT_INPUT', '3',
            {'jobs': bank_with('C04', 5, bytes(retained))}, 'Unsupported')
    # A version3 kind2 cell is never read as kind3: RowId bytes are corrupt.
    catalog = bytearray(cell3('C03', 1))
    put(catalog, 320, 1)
    variant('C03', 'KIND2-ROWID', 'REJECT_INPUT', '3',
            {'jobs': bank_with('C03', 1, bytes(catalog))}, 'Queue(Corrupt)')
    # A fully resealed Commit frame whose after Root changes row_high.
    c04 = p3['C04']
    moved = bytearray(cell(c04['after1'], 0))
    put(moved, 432, ROW_HIGH + 1)
    moved_page = with_cells(c04['after1'], {0: bytes(moved)})
    tx2, aging2 = parent_raw(rows['C04'][19]), parent_raw(rows['C04'][20])
    after2 = {r: blob(rows['C04'], 29 + r) for r in range(8)}
    after3 = {r: c04[f'after{r}'] for r in range(8)}
    after3[1] = moved_page
    changed = frame3(parent_raw(rows['C04'][21]), profiles,
                     {0: tx2, 1: aging2, 'root': parent_raw(rows['C04'][24])},
                     {0: c04['transaction'], 1: c04['aging'], 'root': c04['root']},
                     after2, after3)
    variant('C04', 'ROW-HIGH-FRAME', 'REJECT_FRAME', '3',
            {'frame': changed, 'after1': moved_page}, 'Corrupt')

    (ROOT / 'blobs.bin').write_bytes(bytes(blobs.data))
    index = ['sha256\toffset\tbytes'] + [f'{k}\t{o}\t{n}' for k, (o, n) in blobs.index.items()]
    (ROOT / 'blobs.tsv').write_text('\n'.join(index) + '\n')
    header = ['id', 'class', 'profile', 'base'] + columns + ['expect']
    (ROOT / 'cases.tsv').write_text(
        '\n'.join('\t'.join(row) for row in [header] + out_rows) + '\n')
    assert profile3 in blobs.index


if __name__ == '__main__':
    write()
