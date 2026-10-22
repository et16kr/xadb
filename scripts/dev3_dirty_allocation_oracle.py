#!/usr/bin/env python3
"""Generate independent Dirty98304 allocation fixtures; never run product code.

The format tables are dev3-allocation-reservation.md, dev3-allocation-format.md,
dev3-obligation-format.md and dev3-wal-format.md. The adopted private resource
context changes the Terminal memory peak to 98304, selected by the trusted
consumer, never inferred from bytes. History remains reserve/cancel 1, 8, 63.

This stdlib-only implementation adapts the independently reviewed field-layout
algorithms in the two recorded provenance scripts. Neither is imported/executed.
Legacy output literals are read only for provenance hashes; all new descriptor,
frame and page bytes are constructed from the six pinned genesis input literals.
--write creates a NEW directory; it never overwrites any existing output.
Fixture generation, internal arithmetic assertions and manifest census are not
product/compiler/checker test PASS evidence. Selftests remain separate selectors.
"""

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = Path('tests/fixtures/dev3-dirty-allocation')
BASE = Path('crates/engine/tests/fixtures/dev3-storage-session/literal')
LEGACY = Path('tests/fixtures/dev3-allocation-reservation')
DIRTY_MEMORY = 98_304
PAGE_BYTES = 16_384
CELL_OFFSET = 10_144
CELL_BYTES = 6_208
DESCRIPTOR_BYTES = 1_536
DATABASE = bytes(range(1, 17))
FILE_ID = 0x100000001
HISTORY = (1, 8, 63)
PROFILES = (1, 2, 3)
INPUT_SHA256 = {
    'profile1-d3-profile.bin': 'c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4',
    'profile2-d3-profile.bin': 'e45db9f3b6d87837bc2d626f641034c7b647b711fe8c761990731eb5db16052f',
    'profile3-d3-profile.bin': 'e86fdb05e8a470f64fa351b9626cd667b45824d40ef0bbe376f9c1bfcc4765d0',
    'profile1-allocation.bin': '1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7',
    'profile2-allocation.bin': '1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7',
    'profile3-allocation.bin': '1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7',
}
ALGORITHM_PROVENANCE = {
    'artifacts/dev3/allocation-reservation-preparation-20261021/r4-literals.py':
        '18d7a69412776f1a50e3f658dd66d117b98953b6e91c8e3619b2b19ba1436ffc',
    'artifacts/dev3/allocation-reservation-independent-review-20261021/r2-raw-audit.py':
        '7fb21276d06f2f116f44840fefbdeba9a3c01b20fc02942158c5f031e55609d4',
}
LEGACY_MANIFEST_SHA256 = '872c11b8645035849bf87859d418dd484394528ca21746768a2a30b2e092202d'
CONTRACTS = (
    'devdoc/contracts/dev3-allocation-reservation.md',
    'devdoc/contracts/dev3-allocation-format.md',
    'devdoc/contracts/dev3-obligation-format.md',
    'devdoc/contracts/dev3-wal-format.md',
)


def sha(data):
    return hashlib.sha256(data).digest()


def integer(value, width=8):
    return value.to_bytes(width, 'little')


def read_integer(data, offset, width=8):
    return int.from_bytes(data[offset:offset + width], 'little')


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def form(size, fields):
    """Place nonoverlapping explicit contract fields in a zero-filled record."""
    result = bytearray(size)
    occupied = bytearray(size)
    for position, value in fields:
        end = position + len(value)
        require(position >= 0 and end <= size, 'field bounds')
        require(not any(occupied[position:end]), 'overlapping fields')
        result[position:end] = value
        occupied[position:end] = b'\1' * len(value)
    return result


def frame_length(count):
    require(count in range(1, 64), 'allocation count')
    return 20_992 + 208 * count


def work_owner(work_id):
    require(work_id in (1, 2, 3), 'history work id')
    return form(48, [(0, DATABASE), (16, integer(1)), (24, b'\1'),
                     (32, integer(work_id)), (40, integer(1))])


def ownership_item(page_no, generation, birth_lsn):
    return form(96, [(0, b'\1\3'),
                     (8, integer(FILE_ID) + integer(page_no) + integer(generation)),
                     (64, integer(1)), (72, integer(birth_lsn))])


def descriptor(profile, work_id, count, lsn, birth_lsn, cancel, items):
    """XOD1 is rebuilt from semantic fields, including the new Terminal peak."""
    length = frame_length(count)
    reference = form(64, [(0, b'\1'), (8, integer(1)),
                          (16, integer(4096, 4)), (32, sha(profile))])
    fields = [
        (0, b'XOD1' + integer(1, 2) + bytes([6 if cancel else 5])),
        (8, integer(DESCRIPTOR_BYTES, 4)), (16, b'\2'), (24, work_owner(work_id)),
        (72, integer(lsn) + integer(birth_lsn if cancel else 0) + integer(birth_lsn)),
        (96, reference), (352, bytes([1, 4 if cancel else 1])),
        (360, integer(1) + integer(count if cancel else 0) + integer(count)),
        (448, sha(b'')), (544, integer(lsn) + integer(0 if cancel else count)),
        (560, sha(b'' if cancel else items)),
        (608, integer(length * (2 if cancel else 1))),
        (624, integer(2 if cancel else 1)),
    ]
    fields.extend((640 + 128 * category, bytes([category + 1])) for category in range(5))
    if not cancel:
        # Terminal row starts896. Memory is row+72=968; aggregate memory1288.
        fields.extend([
            (904, integer(1) + integer(length)), (936, integer(count)),
            (960, integer(1) + integer(DIRTY_MEMORY)), (992, integer(1)),
            (1008, integer(length + PAGE_BYTES * count)),
            (1280, integer(1) + integer(DIRTY_MEMORY)), (1312, integer(1)),
            (1328, integer(length + PAGE_BYTES * count) + integer(birth_lsn)),
        ])
    result = form(DESCRIPTOR_BYTES, fields)
    result[1504:] = sha(result[:1504])
    return bytes(result)


def changed_page(before, work_id, count, lsn, cancel):
    """Independent Free(g)->Reserved(g+1)->Free(g), preserving other entries."""
    require(len(before) == PAGE_BYTES, 'before page length')
    result = bytearray(before)
    items = bytearray()
    birth_lsn = lsn - 1 if cancel else lsn
    for page_no in range(1, count + 1):
        offset = CELL_OFFSET + 64 + 96 * page_no
        generation = read_integer(before, offset + 8)
        if cancel:
            require(before[offset] == 1, 'cancel before state')
            entry = form(96, [(8, integer(generation))])
        else:
            require(before[offset] == 0 and generation < 2**64 - 1, 'reserve before state')
            generation += 1
            entry = form(96, [(0, b'\1\0\2'), (8, integer(generation)),
                              (32, work_owner(work_id))])
        result[offset:offset + 96] = entry
        items.extend(ownership_item(page_no, generation, birth_lsn))
    result[64:72] = integer(lsn)
    result[-32:] = sha(result[:-32])
    return bytes(result), bytes(items)


def wal_frame(before, after, after_descriptor, before_descriptor, work_id, count, lsn, items, cancel):
    """Assemble full XWF1/XSG1 sections; no patching of a legacy WAL image."""
    operation = 10 if cancel else 8
    length = frame_length(count)
    prefix = form(160, [
        (0, b'XWF1' + integer(1, 2) + b'\3\2'), (8, integer(160, 2)),
        (12, integer(length, 4) + integer(length - 192, 4) + integer(1, 2)),
        (24, DATABASE + integer(1) + integer(1, 4)),
        (56, integer(lsn) + integer(lsn - 1)),
    ])
    prefix[128:] = sha(prefix[:128])
    group = form(192, [
        (0, b'XSG1' + integer(1, 2) + integer(operation, 2) + integer(192, 2)),
        (12, integer(length - 192, 4)),
        (16, integer(lsn) + integer(lsn - 1 if cancel else 0)),
        (32, b'\2'), (40, work_owner(work_id)),
        (88, sha(before_descriptor) if cancel else bytes(32)),
        (120, integer(DESCRIPTOR_BYTES, 4) + integer(1, 2)),
        (130, integer(count, 2) + integer(1, 2)), (136, integer(12544, 4)),
        (148, integer(208 * count, 4) + integer(6272, 4)),
    ])
    core = form(256, [(0, integer(operation, 2)), (4, integer(256, 4)),
                      (168, integer(count if cancel else 0))])
    old_cell = before[CELL_OFFSET:-32]
    new_cell = after[CELL_OFFSET:-32]
    native = form(128, [
        (0, b'\6\3'), (8, integer(CELL_OFFSET, 2) * 2),
        (12, integer(CELL_BYTES, 4) * 2 + integer(12544, 4)),
        (24, sha(old_cell) + sha(new_cell)),
    ]) + old_cell + new_cell
    deltas = bytearray()
    for ordinal in range(count):
        item = items[96 * ordinal:96 * (ordinal + 1)]
        deltas.extend(form(208, [
            (0, bytes([3, 1]) if cancel else bytes([1, 2])),
            (16 if cancel else 112, item),
        ]))
    redo = form(64, [
        (0, b'\1\12'), (4, integer(6272, 4)), (8, before[24:64]),
        (48, before[64:72]), (56, integer(CELL_OFFSET, 2) + integer(CELL_BYTES, 2)),
    ]) + new_cell
    material = prefix + group + after_descriptor + core + native + deltas + redo
    require(len(material) == length - 32, 'assembled frame length')
    return bytes(material + sha(material))


def inspect_operation(frame, before_page, page, current_descriptor, previous_descriptor, count, lsn, cancel):
    """Read back independent fixed fields/checksums, never invoke product code."""
    require(len(frame) == frame_length(count), 'frame length')
    require(len(page) == PAGE_BYTES and len(current_descriptor) == DESCRIPTOR_BYTES, 'output sizes')
    require(frame[128:160] == sha(frame[:128]) and frame[-32:] == sha(frame[:-32]), 'frame checksums')
    require(page[-32:] == sha(page[:-32]), 'page checksum')
    require(current_descriptor[-32:] == sha(current_descriptor[:-32]), 'descriptor checksum')
    expected_memory = 0 if cancel else DIRTY_MEMORY
    require(read_integer(current_descriptor, 968) == expected_memory, 'trusted Terminal memory')
    require(read_integer(current_descriptor, 1288) == expected_memory, 'trusted aggregate memory')
    require(frame[352:1888] == current_descriptor, 'embedded descriptor')
    require(frame[248:280] == (sha(previous_descriptor) if cancel else bytes(32)), 'before descriptor hash')
    require(read_integer(frame, 56) == lsn and read_integer(page, 64) == lsn, 'LSN')
    require(frame[2272:8480] == before_page[CELL_OFFSET:-32], 'native before cell')
    if cancel:
        require(read_integer(previous_descriptor, 968) == DIRTY_MEMORY and
                read_integer(previous_descriptor, 1288) == DIRTY_MEMORY, 'trusted before Terminal memory')
    require(frame[8480:14688] == page[CELL_OFFSET:-32], 'native after cell')
    redo = 14688 + 208 * count
    require(frame[redo + 64:redo + 6272] == page[CELL_OFFSET:-32], 'redo after cell')


def frozen_inputs(root):
    result = {}
    receipts = []
    for name, expected in INPUT_SHA256.items():
        path = BASE / name
        data = (root / path).read_bytes()
        require(sha(data).hex() == expected, f'input identity: {path}')
        result[name] = data
        receipts.append({'path': str(path), 'bytes': len(data), 'sha256': expected})
    return result, receipts


def legacy_provenance(root):
    path = root / LEGACY / 'manifest.json'
    require(sha(path.read_bytes()).hex() == LEGACY_MANIFEST_SHA256, 'legacy manifest identity')
    manifest = json.loads(path.read_text())
    result = {}
    for row in manifest['literals']:
        data = (root / LEGACY / row['path']).read_bytes()
        require(len(data) == row['bytes'] and sha(data).hex() == row['sha256'], 'legacy literal identity')
        result[row['path']] = row
    require(len(result) == 63, 'legacy provenance census')
    return result


def build(root):
    inputs, input_rows = frozen_inputs(root)
    provenance = legacy_provenance(root)
    outputs, operations, cycles = {}, [], []
    for profile_id in PROFILES:
        profile = inputs[f'profile{profile_id}-d3-profile.bin']
        page = inputs[f'profile{profile_id}-allocation.bin']
        offset = 4096
        for work_id, count in enumerate(HISTORY, 1):
            previous_descriptor = None
            for cancel in (False, True):
                operation = 'cancel' if cancel else 'reserve'
                lsn = 2 * work_id - (0 if cancel else 1)
                birth = 2 * work_id - 1
                before = page
                page, items = changed_page(before, work_id, count, lsn, cancel)
                current = descriptor(profile, work_id, count, lsn, birth, cancel, items)
                frame = wal_frame(before, page, current, previous_descriptor, work_id, count, lsn, items, cancel)
                inspect_operation(frame, before, page, current, previous_descriptor, count, lsn, cancel)
                stem = f'p{profile_id}-n{count}-'
                for suffix, data in [('frame', frame), ('page', page), ('descriptor', current)]:
                    outputs[stem + operation + '.' + suffix] = data
                if not cancel:
                    outputs[stem + 'items.bin'] = items
                operations.append({
                    'id': f'DA-ORACLE-P{profile_id}-N{count}-{operation.upper()}',
                    'status': 'NOT_RUN', 'resource_context': 'Dirty98304',
                    'selection': 'INTERMEDIATE_AND_S5' if profile_id == 1 and count == 1 else 'S5_ONLY',
                    'profile': profile_id, 'count': count, 'work_id': work_id,
                    'operation': operation, 'opcode': 10 if cancel else 8,
                    'lsn': lsn, 'previous_lsn': lsn - 1, 'birth_lsn': birth,
                    'wal_start': offset, 'frame_bytes': len(frame), 'wal_end': offset + len(frame),
                    'page_lsn': lsn, 'modification_counter': lsn, 'first_rec_lsn': 1,
                    'descriptor_terminal_memory': 0 if cancel else DIRTY_MEMORY,
                    'before_descriptor_sha256': sha(previous_descriptor).hex() if cancel else None,
                    'outputs': {suffix: stem + operation + '.' + suffix for suffix in ('frame', 'page', 'descriptor')},
                })
                offset += len(frame)
                previous_descriptor = current
            cycles.append({'profile': profile_id, 'count': count, 'work_id': work_id,
                           'reserve_lsn': 2 * work_id - 1, 'cancel_lsn': 2 * work_id,
                           'wal_end': offset, 'frame_bytes': frame_length(count), 'first_rec_lsn': 1})
    require(len(outputs) == 63 and len(operations) == 18 and len(cycles) == 9, 'new fixture census')
    literal_rows = []
    for name, data in sorted(outputs.items()):
        old = provenance[name]
        literal_rows.append({
            'path': name, 'bytes': len(data), 'sha256': sha(data).hex(),
            'legacy_provenance': {'path': str(LEGACY / name), 'sha256': old['sha256'],
                                  'same_bytes_by_sha256': sha(data).hex() == old['sha256'],
                                  'usage': 'PROVENANCE_ONLY_NOT_PRODUCT_EXECUTION'},
        })
    generator = Path(__file__).resolve()
    manifest = {
        'schema_version': 1, 'status': 'GENERATED_ORACLE_INPUTS_NOT_PRODUCT_EXECUTION',
        'product_tests': 'NOT_RUN', 'compiler_tests': 'NOT_RUN', 'oracle_selftests': 'NOT_RUN',
        'resource_context': {'name': 'Dirty98304', 'terminal_memory': DIRTY_MEMORY,
                             'selection': 'trusted owner context; not inferred from wire',
                             'descriptor_offsets': {'terminal_row_memory': 968, 'aggregate_memory': 1288}},
        'generator': {'path': 'scripts/dev3_dirty_allocation_oracle.py', 'sha256': sha(generator.read_bytes()).hex(),
                      'runtime_dependencies': ['Python standard library only']},
        'algorithm_provenance_not_runtime_dependencies': ALGORITHM_PROVENANCE,
        'contracts_read': {path: sha((root / path).read_bytes()).hex() for path in CONTRACTS},
        'inputs': input_rows, 'legacy_manifest_sha256': LEGACY_MANIFEST_SHA256,
        'history_per_profile': ['reserve1', 'cancel1', 'reserve8', 'cancel8', 'reserve63', 'cancel63'],
        'literal_count': len(outputs), 'literal_bytes': sum(map(len, outputs.values())),
        'operation_count': len(operations), 'intermediate_operation_count': 2,
        's5_operation_count': 18, 'operations': operations, 'cycles': cycles, 'literals': literal_rows,
    }
    return outputs, manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--output', type=Path, default=OUTPUT)
    parser.add_argument('--write', action='store_true', required=True, help='create new fixtures only, never overwrite')
    args = parser.parse_args()
    root = args.root.resolve()
    output = args.output if args.output.is_absolute() else root / args.output
    require(not output.exists(), 'output already exists; preserve original attempt')
    outputs, manifest = build(root)
    output.mkdir(parents=True, exist_ok=False)
    for name, data in outputs.items():
        with (output / name).open('xb') as destination:
            destination.write(data)
    with (output / 'manifest.json').open('x') as destination:
        destination.write(json.dumps(manifest, indent=2) + '\n')
    # Read-only identity retention check; never execute or modify legacy files.
    legacy_provenance(root)
    print(json.dumps({'status': manifest['status'], 'product_tests': 'NOT_RUN',
                      'output': str(output), 'literal_count': len(outputs),
                      'literal_bytes': manifest['literal_bytes'], 'operation_count': len(manifest['operations']),
                      'manifest_sha256': sha((output / 'manifest.json').read_bytes()).hex()}))


if __name__ == '__main__':
    main()
