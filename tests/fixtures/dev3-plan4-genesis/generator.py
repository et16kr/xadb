#!/usr/bin/env python3
"""Independent compact plan4 wire corpus; no product code or process imports.

Source preparation does not authorize running this generator. The frozen input
and independent recipe review must precede its one create-new invocation.
"""
import argparse
import hashlib
import json
import struct
import sys
import time
from pathlib import Path

sys.dont_write_bytecode = True
PAGE = 16384
DATABASE = bytes(range(1, 17))
FILE_ID = 0x100000001
STORAGE_MANIFEST_SHA = '2e35b50c42b21f12448e98c8e2825d1bfaa73dc4bc1f72dca0c5cafb79aa3d5f'
DWB_MANIFEST_SHA = 'b794e062669d4e7442d1398a484dadfaafd193ab5ff9de70788a6a8177139811'
INPUT_SHA = 'a6f0150ce501fa954839440f0cc7db98cf462b72734077f624fca33a608a988c'
G = 'storage::wal_io::bootstrap::plan4_genesis::tests::'
Q = 'storage::retention_genesis::tests::'
S = 'storage::recovery_select::plan4::tests::'
NATIVE = 'storage::recovery_select::plan4::native::worker_plan4_genesis'


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def u16(raw, offset, value):
    struct.pack_into('<H', raw, offset, value)


def u32(raw, offset, value):
    struct.pack_into('<I', raw, offset, value)


def u64(raw, offset, value):
    struct.pack_into('<Q', raw, offset, value)


def seal(raw):
    raw[-32:] = hashlib.sha256(raw[:-32]).digest()
    return bytes(raw)


def page(number, owner, kind, cells):
    raw = bytearray(PAGE)
    raw[:4] = b'XDP1'
    u16(raw, 4, 1)
    raw[6] = kind
    raw[8:24] = DATABASE
    for at, value in ((24, FILE_ID), (32, number), (40, 1), (48, owner), (56, 1)):
        u64(raw, at, value)
    u16(raw, 72, len(cells))
    u16(raw, 74, 128 + 4 * len(cells))
    end = PAGE - 32
    for index, cell in enumerate(cells):
        start = end - len(cell)
        u16(raw, 128 + index * 4, start)
        u16(raw, 130 + index * 4, len(cell))
        raw[start:end] = cell
        end = start
    u16(raw, 76, end)
    return seal(raw)


def ref(raw, offset, number):
    for at, value in ((offset, FILE_ID), (offset + 8, number), (offset + 16, 1)):
        u64(raw, at, value)


def new_raw():
    job = bytearray(1024)
    job[:4] = b'XQJ1'
    u16(job, 4, 1)
    u64(job, 8, 1)
    aggregate = bytearray(640)
    for i in range(5):
        aggregate[i * 128] = i + 1
    root = bytearray(512)
    root[:4] = b'XQR1'
    u16(root, 4, 1)
    u32(root, 8, 512)
    u32(root, 12, 64)
    u64(root, 64, 1)
    u64(root, 96, (1 << 64) - 1)
    root[128:160] = hashlib.sha256(job * 64).digest()
    root[160:192] = hashlib.sha256(aggregate).digest()
    root[192:208] = DATABASE
    for at, value in ((208, 1), (216, 4), (224, 1)):
        u64(root, at, value)
    ref(root, 232, 1)
    for i in range(5):
        ref(root, 256 + 24 * i, i + 2)
    allocation = bytearray(6208)
    allocation[:4] = b'XAL1'
    u16(allocation, 4, 1)
    u16(allocation, 6, 64)
    u16(allocation, 16, 64)
    u16(allocation, 18, 96)
    ref(allocation, 32, 0)
    allocated = [(10, 1), (12, 4), *[(13, 4)] * 5, (11, 5), (5, 5)]
    for i in range(64):
        at = 64 + 96 * i
        u64(allocation, at + 8, 1)
        if i < 9:
            kind, owner = allocated[i]
            allocation[at] = 2
            allocation[at + 1] = kind
            u64(allocation, at + 16, owner)
            u64(allocation, at + 24, 1)
    anchor = bytearray(48)
    anchor[:4] = b'XTA1'
    u16(anchor, 4, 1)
    anchor[6] = 5
    ref(anchor, 8, 8)
    u64(anchor, 32, 1)
    leaf = bytearray(32)
    leaf[:4] = b'XBT1'
    u16(leaf, 4, 1)
    result = {'P00': page(0, 1, 10, [allocation]), 'P01': page(1, 4, 12, [root]),
              'P07': page(7, 5, 11, [anchor]), 'P08': page(8, 5, 5, [leaf]),
              'JOB-FREE': bytes(job), 'QUEUE-ROOT': bytes(root), 'BUDGET-ZERO': bytes(aggregate)}
    result.update({f'P{i:02}': page(i, 4, 13, [job] * (4 if i == 6 else 15)) for i in range(2, 7)})
    return result


def references(repo):
    source = repo / 'tests/fixtures/dev3-storage-session/manifest.json'
    dwb_source = repo / 'tests/fixtures/dev3-doublewrite-install/manifest.json'
    assert sha(source.read_bytes()) == STORAGE_MANIFEST_SHA
    assert sha(dwb_source.read_bytes()) == DWB_MANIFEST_SHA
    storage = json.loads(source.read_text())['literal_files']
    dwb = json.loads(dwb_source.read_text())['literal_files']
    refs, bootstrap, provenance = {}, {}, []
    for profile in (1, 2, 3):
        for name in ('d3-profile', 'd3-wal-00000001', 'WRITER', 'FILEMAP', 'root', 'chunk', 'd3-bootstrap'):
            filename = f'profile{profile}-{name}.bin'
            relative = Path('crates/engine/tests/fixtures/dev3-storage-session/literal') / filename
            raw = (repo / relative).read_bytes()
            expected = storage[filename]
            assert len(raw) == expected['bytes'] and sha(raw) == expected['sha256']
            provenance.append(dict(path=str(relative), bytes=len(raw), sha256=sha(raw)))
            if name == 'd3-bootstrap':
                bootstrap[profile] = raw
            else:
                refs[f'REF-P{profile}-{name}'] = raw
        for name in ('DWB', 'empty', 'd3-dwb-stage'):
            filename = f'literal/profile{profile}-{name}.bin'
            relative = Path('tests/fixtures/dev3-doublewrite-install') / filename
            raw = (repo / relative).read_bytes()
            expected = dwb[filename]
            assert len(raw) == expected['bytes'] and sha(raw) == expected['sha256']
            provenance.append(dict(path=str(relative), bytes=len(raw), sha256=sha(raw)))
            refs[f'REF-P{profile}-{name}'] = raw
    return refs, bootstrap, provenance


def inventory():
    cases = {}
    def add(case, selector, phase='mandatory', category='memory', operation=None, expected='frozen test-input operation and exact postconditions'):
        assert case not in cases
        cases[case] = dict(selector=selector, phase=phase, category=category,
                           operation=operation or selector.rsplit('::', 1)[-1], expected=expected,
                           expected_region='full operation inputs/outputs; test-input frozen SHA')
    for suffix, numbers, selector in (
        ('C', range(1, 4), G + 'plan4_canonical_profiles'),
        ('N', range(1, 49), G + 'plan4_genesis_page_mutants'),
        ('B', range(1, 37), S + 'plan4_control_and_closure_inputs'),
        ('I', range(1, 21), G + 'plan4_install_and_final_identity_boundaries'),
        ('O', range(1, 22), S + 'plan4_owner_and_old_dispatch_boundaries'),
        ('E', range(1, 13), Q + 'plan4_codec_invalid_inputs'),
        ('F', range(1, 3), G + 'plan4_prepared_control_seals')):
        for number in numbers:
            add(f'P4-{suffix}{number:02}', selector)
    for prefix, actions, selector in (
        ('W', 'SIZCEP', G + 'plan4_prepared_page_write_boundaries'),
        ('R-P', 'SZCE', G + 'plan4_readback_page_boundaries'),
        ('R-S', 'SZCE', S + 'plan4_selection_page_boundaries')):
        for page_number in range(9):
            for action in actions:
                add(f'P4-{prefix}{page_number:02}-{action}', selector, operation=f'{prefix}/page{page_number}/{action}')
    groups = (([1, 2, *range(14, 22)], 'storage::page::tests::plan4_changed_kind_inputs'),
              (range(3, 11), 'storage::allocation_page::tests::plan4_changed_kind_inputs'),
              (range(11, 14), 'storage::doublewrite_control::tests::plan4_changed_kind_inputs'),
              (range(22, 28), 'storage::wal_frame::tests::plan4_changed_kind_inputs'),
              ([28, 29], G + 'plan4_selected_page_kind_inputs'),
              ([30, 31], G + 'plan4_buffer_trace_kind_inputs'))
    for numbers, selector in groups:
        for number in numbers:
            add(f'P4-K{number:02}', selector)
    for number, kind in ((1, 'memory'), (2, 'native')):
        add(f'P4-ST{number:02}', G + f'plan4_source_census_{kind}', category='static')
    for profile in (1, 2, 3):
        for phase in ('PRODUCE', 'SELECT', 'REENTRY'):
            add(f'P4-NATIVE-P{profile}-{phase}', NATIVE, 'mandatory' if profile == 1 else 's5', 'native', phase)
    for profile in (2, 3):
        for prefix, count in (('N', 48), ('B', 32)):
            for number in range(1, count + 1):
                add(f'P4-SP{profile}-{prefix}{number:02}', S + 'plan4_s5_profile_closures', 's5', operation='full G3 acquire')
    omitted = {(0, 'G0'), (63, 'G2'), (31, 'STATE1'), (17, 'B1023')}
    for ordinal in range(64):
        for field in ('G0', 'G2', 'STATE1', 'R7', 'SRC16', 'B384', 'B511', 'B1023', 'R320'):
            if (ordinal, field) not in omitted:
                add(f'P4-SJ{ordinal:02}-{field}', Q + 'plan4_s5_job_fields', 's5')
    for offset in range(384, 511):
        add(f'P4-SR{offset:03}', Q + 'plan4_s5_root_reserved', 's5')
    for kind in (12, 13):
        for level in range(1, 256):
            if level not in (1, 15, 16):
                add(f'P4-SK{kind}-{level:03}', 'storage::page::tests::plan4_s5_kind_levels', 's5')
    for event in [*(f'PAGE{i}' for i in range(9)), 'DATA_SYNC', 'FILEMAP_PUBLISH', 'DWB_PUBLISH']:
        for side in ('BEFORE', 'AFTER'):
            add(f'P4-SC{event}-{side}', G + 'plan4_s5_crash_prefixes', 's5')
    for profile in (1, 2, 3):
        for seed in range(1, 9):
            add(f'P4-SE{profile}-{seed}', G + 'plan4_s5_entropy_profiles', 's5')
    assert len(cases) == 1721
    assert sum(row['phase'] == 'mandatory' for row in cases.values()) == 304
    assert sum(row['category'] == 'static' for row in cases.values()) == 2
    assert len({row['selector'] for row in cases.values() if row['phase'] == 'mandatory'}) == 19
    return cases


def create(path, raw):
    with path.open('xb') as stream:
        stream.write(raw)
        stream.flush()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--input-document', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    started = time.monotonic()
    assert args.repo.is_absolute() and args.output.is_absolute() and args.input_document.is_absolute()
    assert sha(args.input_document.read_bytes()) == INPUT_SHA
    for name in ('raw', 'references', 'manifest.json', 'raw-index.tsv', 'provenance.md'):
        assert not (args.output / name).exists(), f'create-new destination exists: {name}'
    raws = new_raw()
    refs, original_bootstrap, dependencies = references(args.repo)
    for profile, old in original_bootstrap.items():
        bootstrap = bytearray(old)
        u64(bootstrap, 120, 5)
        u64(bootstrap, 280, 4)
        bootstrap[208:240] = hashlib.sha256(raws['P00']).digest()
        raws[f'XBS-P{profile}'] = seal(bootstrap)
    assert len(raws) == 15 and sum(map(len, raws.values())) == 161920
    assert len(refs) == 27 and sum(map(len, refs.values())) == 112800
    cases = inventory()
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'raw').mkdir()
    (args.output / 'references').mkdir()
    rows = {}
    for directory, source in (('raw', raws), ('references', refs)):
        for key, value in sorted(source.items()):
            path = f'{directory}/{key}.bin'
            create(args.output / path, value)
            rows[key] = dict(path=path, bytes=len(value), sha256=sha(value))
    index = 'id\tpath\tbytes\tsha256\n' + ''.join(f'{key}\t{row["path"]}\t{row["bytes"]}\t{row["sha256"]}\n' for key, row in sorted(rows.items()))
    create(args.output / 'raw-index.tsv', index.encode())
    manifest = dict(schema='XADB_PLAN4_GENESIS_INPUT_V1', input_sha256=INPUT_SHA,
                    generator_sha256=sha(Path(__file__).read_bytes()), database_hex=DATABASE.hex(),
                    source_dependencies=dependencies, raw=rows, raw_index_sha256=sha(index.encode()),
                    mandatory=304, semantic=302, static=2, s5=1417, total=1721,
                    stable_files=15, stable_bytes=22069248, lifecycle_names=18,
                    cases=cases)
    create(args.output / 'manifest.json', (json.dumps(manifest, indent=2, sort_keys=True) + '\n').encode())
    provenance = f'''# Plan4 independent compact input provenance

Input specification SHA: `{INPUT_SHA}`. Generated from explicit LE offsets and
Python hashlib only; no product encoder, library, binary or test helper imported.
New raw15/161920B plus exact existing independent reference27/112800B =42/274720B.
The dependency path/length/SHA table is in manifest.json. Existing XBS1 literals
provide unchanged control fields; only plan/high-water/allocator digest/trailer
are changed by the independent documented equation. No old product tests ran.

The generator requires the original frozen input document and these retained
dependencies to regenerate an input revision. Runtime tests use only this tracked
directory; ignored artifacts are not runtime dependencies. Generation is
create-new and existing raw/references/manifest refuse overwrite. Failed partial
generation must be preserved and reviewed, never silently replaced.

Full native files are composed from these exact records plus stated zero extents.
Stable15/22069248B includes LOCK; whole lifecycle18 also names writer, FILEMAP and
DWB temporary selector files. Raw generation is not a product PASS.
'''
    create(args.output / 'provenance.md', provenance.encode())
    print(json.dumps(dict(status='INPUT_CREATED', raw_files=len(rows), raw_bytes=274720,
                          mandatory=304, s5=1417, elapsed_seconds=time.monotonic()-started,
                          manifest_sha256=sha((args.output / 'manifest.json').read_bytes()))))


if __name__ == '__main__':
    main()
