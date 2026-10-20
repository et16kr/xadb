#!/usr/bin/env python3
"""Build/check independent DEV3 page-envelope v1 vectors using only stdlib.

Source contract: devdoc/contracts/dev3-page-format.md, F01/F02, 2026-10-20.
This file was written from that contract without reading the production codec.
It checks envelope bytes only: cells have no key, row, undo or tree meaning.

Frozen input: database=16*11 hex, file=0x0000000100000002, page=3,
incarnation=4, owner=5/generation6, LSN=7, row-leaf kind1/level0.
empty cells=(); nonempty cells=(616c706861, 00ff, 7a).
The latter's slots are db3f0500 d93f0200 d83f0100 and physical payload is
7a00ff616c706861: slots follow logical order; payload grows backward.

All integers use explicit little-endian struct fields. A zero-filled 16384-byte
image contains the 128-byte header, 4-byte slot entries, and backward-packed
cells. SHA-256 of bytes [0,16352) occupies [16352,16384).
Frozen SHA-256 of the COMPLETE fixture files:
  empty: de44e1eda7207bbd7aac6310bb9e307cfe2b37d9bca788056a69569d55735867
  cells: 908281ca2193411014868af608a28cc6aa06a129e8f6eb4d554bd12de3f2659c
Frozen stored digests (of the prefix alone):
  empty: 2ec6d1703db2f3a7fb35c2fa5e2196403555783087ad8e8f292f79b5fe408d98
  cells: d605092358921f9446a2cbb6f2de40bce9cbc289d548f20cdaba3671970896bf

Default execution checks existing fixtures and independent known-bad vectors.
--write creates both fixtures only when neither exists; it never replaces them.
No production module, generated production bytes, compiler or DB is invoked.
This is a bounded representation check, not durability or DEV3 qualification.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parent.parent
FIXTURE_DIRECTORY = ROOT / 'crates/engine/tests/fixtures'
PAGE_BYTES = 16_384
HEADER_BYTES = 128
DIGEST_OFFSET = 16_352
IDENTITY = (b'\x11' * 16, 0x0000000100000002, 3, 4, 5, 6)
FIXTURES = {
    'dev3-page-v1-empty.bin': {
        'cells': (),
        'sha256': 'de44e1eda7207bbd7aac6310bb9e307cfe2b37d9bca788056a69569d55735867',
        'stored_digest': '2ec6d1703db2f3a7fb35c2fa5e2196403555783087ad8e8f292f79b5fe408d98',
    },
    'dev3-page-v1-cells.bin': {
        'cells': (b'alpha', b'\x00\xff', b'z'),
        'sha256': '908281ca2193411014868af608a28cc6aa06a129e8f6eb4d554bd12de3f2659c',
        'stored_digest': 'd605092358921f9446a2cbb6f2de40bce9cbc289d548f20cdaba3671970896bf',
    },
}


class InvalidEnvelope(ValueError):
    """Independent structural rejection with the violated contract boundary."""


def require(condition, reason):
    if not condition:
        raise InvalidEnvelope(reason)


def build_vector(cells):
    """Construct this fixed header from the F01 table, without production code."""
    count = len(cells)
    require(count <= 0xffff, 'slot count overflow')
    require(all(isinstance(cell, bytes) and cell for cell in cells), 'empty/non-byte cell')
    lower = HEADER_BYTES + 4 * count
    upper = DIGEST_OFFSET - sum(map(len, cells))
    require(lower <= upper, 'insufficient page space')
    page = bytearray(PAGE_BYTES)
    struct.pack_into(
        '<4sHBB16sQQQQQQHHH', page, 0,
        b'XDP1', 1, 1, 0, *IDENTITY, 7, count, lower, upper,
    )
    end = DIGEST_OFFSET
    for slot, cell in enumerate(cells):
        start = end - len(cell)
        struct.pack_into('<HH', page, HEADER_BYTES + slot * 4, start, len(cell))
        page[start:end] = cell
        end = start
    page[DIGEST_OFFSET:] = hashlib.sha256(page[:DIGEST_OFFSET]).digest()
    return bytes(page)


def validate(page, expected_identity=IDENTITY, expected_kind=1):
    """Read each F01 field independently and return cells in logical slot order."""
    require(len(page) == PAGE_BYTES, 'page length')
    require(page[0:4] == b'XDP1', 'magic')
    require(int.from_bytes(page[4:6], 'little') == 1, 'unsupported format')
    require(hashlib.sha256(page[:DIGEST_OFFSET]).digest() == page[DIGEST_OFFSET:], 'digest')
    kind, level = page[6], page[7]
    require(kind in range(1, 12), 'kind')
    require(1 <= level <= 15 if kind in (2, 4, 6) else level == 0, 'level')
    identity = (bytes(page[8:24]),) + tuple(
        int.from_bytes(page[offset:offset + 8], 'little')
        for offset in (24, 32, 40, 48, 56)
    )
    database, file_id, _, incarnation, owner, generation = identity
    require(database != bytes(16), 'zero database')
    require(file_id >> 32 != 0 and file_id & 0xffffffff != 0, 'file epoch/local')
    require(incarnation != 0 and owner != 0 and generation != 0, 'zero identity')
    require(identity == expected_identity, 'identity mismatch')
    require(kind == expected_kind, 'kind mismatch')
    count, lower, upper = struct.unpack_from('<HHH', page, 72)
    require(lower == HEADER_BYTES + count * 4, 'directory lower')
    require(lower <= upper <= DIGEST_OFFSET, 'directory/cell bounds')
    require(not any(page[78:HEADER_BYTES]), 'reserved bytes')
    require(not any(page[lower:upper]), 'unused bytes')
    cells = []
    next_end = DIGEST_OFFSET
    for slot in range(count):
        offset, length = struct.unpack_from('<HH', page, HEADER_BYTES + 4 * slot)
        require(length != 0, 'empty cell')
        require(upper <= offset and offset + length == next_end, 'noncanonical cell')
        cells.append(bytes(page[offset:next_end]))
        next_end = offset
    require(next_end == upper, 'upper/trailing cell space')
    return tuple(cells)


def with_change(page, offset, replacement):
    """Recompute the checksum so semantic mutants cannot fail by checksum alone."""
    changed = bytearray(page)
    changed[offset:offset + len(replacement)] = replacement
    changed[DIGEST_OFFSET:] = hashlib.sha256(changed[:DIGEST_OFFSET]).digest()
    return bytes(changed)


def must_reject(page, expected_identity=IDENTITY, expected_kind=1, reason=None):
    try:
        validate(page, expected_identity, expected_kind)
    except InvalidEnvelope as error:
        if reason is not None and str(error) != reason:
            raise AssertionError(f'expected {reason!r}, got {str(error)!r}') from error
        return
    raise AssertionError('known-bad envelope was accepted')


def verify_known_bad(pages):
    """Exercise every short length, every byte, and checksummed structural damage."""
    page = pages['dev3-page-v1-cells.bin']
    for length in range(PAGE_BYTES):
        must_reject(page[:length], reason='page length')
    must_reject(page + b'\x00', reason='page length')
    for offset in range(PAGE_BYTES):
        changed = bytearray(page)
        changed[offset] ^= 1
        must_reject(changed)
    semantic = (
        (0, b'BAD!', 'magic'),
        (4, b'\x02\x00', 'unsupported format'),
        (6, b'\x00', 'kind'),
        (6, b'\x0c', 'kind'),
        (7, b'\x01', 'level'),
        (6, b'\x02\x00', 'level'),
        (6, b'\x02\x10', 'level'),
        (8, bytes(16), 'zero database'),
        (24, struct.pack('<Q', 2), 'file epoch/local'),
        (24, struct.pack('<Q', 1 << 32), 'file epoch/local'),
        (40, bytes(8), 'zero identity'),
        (48, bytes(8), 'zero identity'),
        (56, bytes(8), 'zero identity'),
        (72, b'\xff\xff', 'directory lower'),
        (74, struct.pack('<H', 128), 'directory lower'),
        (76, struct.pack('<H', 139), 'directory/cell bounds'),
        (76, b'\xff\xff', 'directory/cell bounds'),
        (78, b'\x01', 'reserved bytes'),
        (127, b'\x01', 'reserved bytes'),
        (140, b'\x01', 'unused bytes'),
        (130, bytes(2), 'empty cell'),
        (128, struct.pack('<H', 16_346), 'noncanonical cell'),
        (132, struct.pack('<H', 16_346), 'noncanonical cell'),
        (132, struct.pack('<H', 16_344), 'noncanonical cell'),
        (134, struct.pack('<H', 0xffff), 'noncanonical cell'),
        (136, struct.pack('<H', 127), 'noncanonical cell'),
    )
    for offset, replacement, reason in semantic:
        must_reject(with_change(page, offset, replacement), reason=reason)
    empty = pages['dev3-page-v1-empty.bin']
    must_reject(with_change(empty, 76, struct.pack('<H', DIGEST_OFFSET - 1)),
                reason='upper/trailing cell space')
    for component in range(len(IDENTITY)):
        expected = list(IDENTITY)
        expected[component] = b'\x22' * 16 if component == 0 else expected[component] + 1
        must_reject(page, tuple(expected), reason='identity mismatch')
    must_reject(page, expected_kind=3, reason='kind mismatch')
    return {
        'short_lengths': PAGE_BYTES,
        'oversized_lengths': 1,
        'single_byte_damage': PAGE_BYTES,
        'checksummed_semantic_damage': len(semantic) + 1,
        'expected_identity_or_kind_mismatch': len(IDENTITY) + 1,
    }


def verify_boundaries(pages):
    page = pages['dev3-page-v1-cells.bin']
    accepted = 0
    for kind in range(1, 12):
        levels = (1, 15) if kind in (2, 4, 6) else (0,)
        for level in levels:
            require(validate(with_change(page, 6, bytes((kind, level))),
                             expected_kind=kind) == FIXTURES['dev3-page-v1-cells.bin']['cells'],
                    'kind/level boundary cells')
            accepted += 1
    for offset, component in ((32, 2), (64, None)):
        expected = list(IDENTITY)
        if component is not None:
            expected[component] = 0
        validate(with_change(page, offset, bytes(8)), tuple(expected))
        accepted += 1
    capacity = DIGEST_OFFSET - HEADER_BYTES - 4
    require(validate(build_vector((b'\xa5' * capacity,))) == (b'\xa5' * capacity,),
            'exact cell capacity')
    accepted += 1
    for cells in ((b'',), (b'\xa5' * (capacity + 1),)):
        try:
            build_vector(cells)
        except InvalidEnvelope:
            continue
        raise AssertionError('invalid fixture construction was accepted')
    return {'accepted_boundaries': accepted, 'rejected_construction_boundaries': 2}


def write_new(pages):
    """Create the two new owned fixtures, cleaning only this call's partial files."""
    paths = [FIXTURE_DIRECTORY / name for name in pages]
    require(not any(path.exists() or path.is_symlink() for path in paths),
            '--write refuses existing fixture paths')
    FIXTURE_DIRECTORY.mkdir(parents=True, exist_ok=True)
    created = []
    try:
        for path in paths:
            with path.open('xb') as stream:
                created.append(path)
                stream.write(pages[path.name])
    except BaseException:
        for path in created:
            path.unlink()
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true',
                        help='create both fixtures only if neither exists')
    args = parser.parse_args()
    pages = {name: build_vector(spec['cells']) for name, spec in FIXTURES.items()}
    for name, page in pages.items():
        spec = FIXTURES[name]
        require(hashlib.sha256(page).hexdigest() == spec['sha256'], f'{name}: frozen file hash')
        require(page[DIGEST_OFFSET:].hex() == spec['stored_digest'], f'{name}: frozen stored digest')
        require(validate(page) == spec['cells'], f'{name}: logical cells')
    require(pages['dev3-page-v1-cells.bin'][128:140].hex() == 'db3f0500d93f0200d83f0100',
            'frozen literal slot directory')
    require(pages['dev3-page-v1-cells.bin'][16_344:16_352].hex() == '7a00ff616c706861',
            'frozen literal physical cells')
    negative = verify_known_bad(pages)
    boundaries = verify_boundaries(pages)
    if args.write:
        write_new(pages)
    for name, page in pages.items():
        require((FIXTURE_DIRECTORY / name).read_bytes() == page, f'{name}: fixture differs')
        print(f'{name} {len(page)} bytes SHA-256 {FIXTURES[name]["sha256"]}')
    print(f'PASS independent envelope vectors: {negative}; {boundaries}')
    print('XADB_DEV3_PAGE_VECTORS ' + json.dumps({
        'status': 'PASS',
        'fixture_sha256': {name: spec['sha256'] for name, spec in FIXTURES.items()},
        'negative': negative,
        'boundaries': boundaries,
    }, sort_keys=True))


if __name__ == '__main__':
    main()
