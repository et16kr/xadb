#!/usr/bin/env python3
"""Independent Profile3 full-namespace references for profile IDs 1-3 (U2-C4b).

Only pathlib/struct/hashlib/json; no XADB encoder/decoder, test helper or other
fixture author is imported or executed. Parents are read as bytes and pinned
by SHA-256. The output mirrors the dev3-plan4-genesis record IDs (REF-Pn-*,
XBS-Pn, P00..P08, JOB-FREE, QUEUE-ROOT, BUDGET-ZERO) for database 01..10,
timeline 1, generation 1. Outputs are create-new; an existing output refuses.

Derivation (published offsets; every other byte is the original record):
  d3-profile       module-row-format/envelopes/profiles.bin[n] (Profile3)
  d3-wal-00000001  [56:88] = SHA(profile)
  WRITER           [64:96] = SHA(profile), [96:128] = SHA(epoch)
  FILEMAP, DWB     [48:80] = SHA(profile)
  d3-dwb-stage     [48:80] = SHA(profile)
  XBS-Pn           [48:80] = SHA(profile)   (plan kind4/high-water5 kept)
  root, chunk, empty  unchanged (no profile binding)
  P01..P06         genesis3/pages.bin namespace A (Root3, Job3 x 64)
  P00, P07, P08    unchanged allocator, anchor and leaf pages
and the trailing 32 bytes of each changed record are SHA-256 of the rest.
Nothing here is product output; it is not executed against the product
before the single current-format switch.
"""
from pathlib import Path
import hashlib
import json
import struct

ROOT = Path(__file__).resolve().parent
FIXTURES = ROOT.parent.parent
PAGE = 16384
PROFILE = 4096
MIB = 1 << 20
PARENTS = {
    'module-row-format/envelopes/profiles.bin':
        'e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a',
    'module-row-format/genesis3/pages.bin':
        'c527c443c0c4e661c0eefff48b352e5f23c7526fa303f336697a2d04e1c471fe',
    'module-row-format/root3/initial-root.bin':
        'd6079c1ea15ea2668be3150c6128d9be4a72e28855336ea7c1d971f73d0b3fc4',
    'module-row-format/root3/free-job.bin':
        'aee93979e2d5bf1901a0daf894a8f8c705fcb3335581f215f405b6326a8dbe69',
    'dev3-plan4-genesis/raw-index.tsv':
        '0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13',
}
# serializer3.PROFILE3_SHA and the U2-C3 catalog3 blob: the same ID1 literal.
PROFILE3_ID1 = 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e'
CONTROLS = ('d3-profile', 'd3-wal-00000001', 'WRITER', 'FILEMAP', 'root', 'chunk',
            'DWB', 'empty', 'd3-dwb-stage')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def parent(path):
    raw = (FIXTURES / path).read_bytes()
    need(sha(raw) == PARENTS[path], ('parent pin', path))
    return raw


def genesis_records():
    """dev3-plan4-genesis records, each checked against its pinned index row."""
    index = parent('dev3-plan4-genesis/raw-index.tsv').decode()
    records = {}
    for line in index.splitlines()[1:]:
        key, path, size, digest = line.split('\t')
        raw = (FIXTURES / 'dev3-plan4-genesis' / path).read_bytes()
        need(len(raw) == int(size) and sha(raw) == digest, ('genesis record', key))
        records[key] = raw
    need(len(records) == 42, 'dev3-plan4-genesis inventory')
    return records


def reseal(raw):
    out = bytearray(raw)
    out[-32:] = hashlib.sha256(out[:-32]).digest()
    return bytes(out)


def bind(raw, fields):
    out = bytearray(raw)
    for at, value in fields:
        out[at:at + len(value)] = value
    return reseal(out)


def changed(old, new):
    return [i for i in range(len(old)) if old[i] != new[i]]


def only(old, new, ranges, name):
    allowed = set()
    for start, end in ranges + [(len(old) - 32, len(old))]:
        allowed.update(range(start, end))
    need(len(old) == len(new) and set(changed(old, new)) <= allowed, ('bound fields only', name))


def derive(records, profiles, pages):
    out = {}
    for n in (1, 2, 3):
        ref = lambda name: records[f'REF-P{n}-{name}']
        v1 = ref('d3-profile')
        profile = profiles[(n - 1) * PROFILE:n * PROFILE]
        # Profile3 is the Profile1 literal with version3 and the 2MiB base.
        relabel = bytearray(v1)
        struct.pack_into('<H', relabel, 4, 3)
        struct.pack_into('<Q', relabel, 296, 2 * MIB)
        need(reseal(relabel) == profile, ('Profile3 equation', n))
        need(n != 1 or sha(profile) == PROFILE3_ID1, 'serializer3 Profile3 pin')
        h, h1 = hashlib.sha256(profile).digest(), hashlib.sha256(v1).digest()
        # The original records bind the Profile1 hash at exactly these offsets.
        old_epoch = ref('d3-wal-00000001')
        for name, ats in (('d3-wal-00000001', (56,)), ('WRITER', (64,)), ('FILEMAP', (48,)),
                          ('DWB', (48,)), ('d3-dwb-stage', (48,))):
            need(all(ref(name)[at:at + 32] == h1 for at in ats), ('Profile1 binding', n, name))
        need(ref('WRITER')[96:128] == hashlib.sha256(old_epoch).digest(), ('epoch binding', n))
        need(records[f'XBS-P{n}'][48:80] == h1, ('XBS1 binding', n))
        epoch = bind(old_epoch, [(56, h)])
        values = {
            'd3-profile': profile,
            'd3-wal-00000001': epoch,
            'WRITER': bind(ref('WRITER'), [(64, h), (96, hashlib.sha256(epoch).digest())]),
            'FILEMAP': bind(ref('FILEMAP'), [(48, h)]),
            'root': ref('root'),
            'chunk': ref('chunk'),
            'DWB': bind(ref('DWB'), [(48, h)]),
            'empty': ref('empty'),
            'd3-dwb-stage': bind(ref('d3-dwb-stage'), [(48, h)]),
        }
        for name, ranges in (('d3-wal-00000001', [(56, 88)]), ('WRITER', [(64, 128)]),
                             ('FILEMAP', [(48, 80)]), ('DWB', [(48, 80)]),
                             ('d3-dwb-stage', [(48, 80)]), ('root', []), ('chunk', []),
                             ('empty', [])):
            only(ref(name), values[name], ranges, (n, name))
        for name in CONTROLS:
            out[f'REF-P{n}-{name}'] = values[name]
        xbs = bind(records[f'XBS-P{n}'], [(48, h)])
        only(records[f'XBS-P{n}'], xbs, [(48, 80)], ('XBS', n))
        need(xbs[176:208] == hashlib.sha256(values['root']).digest(), ('XBS root', n))
        need(xbs[208:240] == hashlib.sha256(records['P00']).digest(), ('XBS allocator', n))
        need(struct.unpack_from('<QQ', xbs, 120)[0] == 5 and struct.unpack_from('<Q', xbs, 280)[0] == 4,
             ('XBS plan kind4/high-water5', n))
        out[f'XBS-P{n}'] = xbs
    for i in range(9):
        key = f'P{i:02}'
        if 1 <= i <= 6:
            page = pages[(i - 1) * PAGE:i * PAGE]
            # Same XDP1 header, owner and slot directory; only cells/trailer differ.
            need(page[:128] == records[key][:128], ('page header', key))
            out[key] = page
        else:
            out[key] = records[key]
    out['QUEUE-ROOT'] = parent('module-row-format/root3/initial-root.bin')
    out['JOB-FREE'] = parent('module-row-format/root3/free-job.bin')
    out['BUDGET-ZERO'] = records['BUDGET-ZERO']
    root_cell = out['QUEUE-ROOT']
    need(root_cell[4:6] == b'\x03\x00' and out['JOB-FREE'][4:6] == b'\x03\x00', 'Root3/Job3 cells')
    need(out['P01'].find(root_cell) > 0, 'Root3 genesis cell on P01')
    need(len(out) == 42, 'record inventory')
    return out


def compose(records, n):
    """The 15-file namespace of profile n (same layout as the Profile1 one)."""
    ref = lambda name: records[f'REF-P{n}-{name}']
    bank0 = bytearray(2_113_536)
    bank0[:len(ref('root'))] = ref('root')
    bank0[PAGE:PAGE + len(ref('chunk'))] = ref('chunk')
    data = bytearray(MIB)
    for i in range(9):
        data[i * PAGE:(i + 1) * PAGE] = records[f'P{i:02}']
    dwb0 = bytearray(PAGE)
    dwb0[:len(ref('empty'))] = ref('empty')
    files = {
        'LOCK': b'', 'd3-profile': ref('d3-profile'), 'd3-wal-00000001': ref('d3-wal-00000001'),
        'WRITER': ref('WRITER'), 'd3-bootstrap': records[f'XBS-P{n}'], 'FILEMAP': ref('FILEMAP'),
        'd3-filemap-0': bytes(bank0), 'd3-filemap-1': bytes(2_113_536),
        'd3-data-0000000100000001': bytes(data), 'DWB': ref('DWB'),
        'd3-dwb-stage': ref('d3-dwb-stage'), 'd3-dwb-0': bytes(dwb0), 'd3-dwb-1': bytes(PAGE),
        'd3-dwb-images': bytes(16_711_680), 'd3-dwb-reserved': bytes(8192),
    }
    need(len(files) == 15 and sum(map(len, files.values())) == 22_069_248, 'namespace size')
    return {name: dict(bytes=len(raw), sha256=sha(raw)) for name, raw in sorted(files.items())}


def create(path, raw):
    with path.open('xb') as stream:
        stream.write(raw)


def write():
    for name in ('raw', 'index.tsv', 'manifest.json'):
        need(not (ROOT / name).exists(), ('create-new output exists', name))
    records = genesis_records()
    profiles = parent('module-row-format/envelopes/profiles.bin')
    pages = parent('module-row-format/genesis3/pages.bin')
    out = derive(records, profiles, pages[:6 * PAGE])
    (ROOT / 'raw').mkdir()
    rows = {}
    for key, raw in sorted(out.items()):
        create(ROOT / 'raw' / f'{key}.bin', raw)
        rows[key] = dict(path=f'raw/{key}.bin', bytes=len(raw), sha256=sha(raw))
    index = 'id\tpath\tbytes\tsha256\n' + ''.join(
        f'{k}\t{r["path"]}\t{r["bytes"]}\t{r["sha256"]}\n' for k, r in sorted(rows.items()))
    create(ROOT / 'index.tsv', index.encode())
    manifest = dict(
        schema='XADB_ROW_GENESIS3_NAMESPACE_V1',
        status='FROZEN_INPUTS_NOT_EXECUTED_AGAINST_PRODUCT',
        baseline='c83771b',
        scope=('Profile3 full-namespace references for profile IDs 1-3 (database 01..10, '
               'timeline1, generation1): the post-switch counterparts of the '
               'dev3-plan4-genesis records read by retention_genesis_tests::files_for. '
               'Stdlib offset substitution only; no product run. Consumers are '
               're-pointed in the single current-format switch (U2-C4c).'),
        author_sha256=sha(Path(__file__).read_bytes()),
        parents={p: dict(bytes=len((FIXTURES / p).read_bytes()), sha256=h) for p, h in PARENTS.items()},
        records=rows, index_sha256=sha(index.encode()),
        composed_namespaces={str(n): compose(out, n) for n in (1, 2, 3)},
        runtime_admission='NOT_ADMITTED: executable only after the single current-format switch',
    )
    create(ROOT / 'manifest.json', (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode())
    return dict(status='GENERATED_INPUT_NOT_PRODUCT_EXECUTION', records=len(rows),
                record_bytes=sum(r['bytes'] for r in rows.values()),
                manifest_sha256=sha((ROOT / 'manifest.json').read_bytes()))


if __name__ == '__main__':
    print(json.dumps(write(), sort_keys=True))
