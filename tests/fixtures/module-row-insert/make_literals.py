"""Write fixed C0 byte observations; never import or run the engine.

These are not authenticated pages, an installed database, or a recovery input.
The byte positions follow dev3-page-format/dev3-row-format/dev3-tree-format.
"""

from hashlib import sha256
from pathlib import Path
from struct import pack, pack_into

HERE = Path(__file__).parent
PAGE_BYTES = 16384
DIGEST = PAGE_BYTES - 32
DATABASE = bytes([1]) * 16


def row_record(row, canonical=bytes.fromhex("010000000101"), committed=False):
    result = bytearray(168 + len(canonical))
    result[:4] = b"XRW1"
    pack_into("<HBBQ", result, 4, 1, 0 if committed else 2, 1, row)
    result[16] = 1 if committed else 2
    for start in ((72,) if committed else (24, 72)):
        result[start : start + 16] = DATABASE
        pack_into("<QI", result, start + 16, 2, 3)
        pack_into("<Q", result, start + 32, 4)
    if committed:
        pack_into("<Q", result, 24, 7)
    else:
        pack_into("<Q", result, 64, 5)
    pack_into("<Q", result, 112, row - 1)
    if not committed:
        pack_into("<QQQH", result, 120, 0x100000001, min(row + 20, 63), 1, 0)
        pack_into("<Q", result, 152, 2)
    pack_into("<I", result, 160, len(canonical))
    result[168:] = canonical
    return bytes(result)


def row_leaf(rows, lsn, canonical=bytes.fromhex("010000000101"), committed_through=0, corrupt_row=0):
    metadata = b"XBT1\x01\x00" + bytes(26)
    cells = [metadata]
    for row in rows:
        record = row_record(row, canonical, row <= committed_through)
        if row == corrupt_row:
            record = b"BAD!" + record[4:]
        cells.append(pack("<HH", 8, len(record)) + row.to_bytes(8, "big") + record)
    result = bytearray(PAGE_BYTES)
    result[:4] = b"XDP1"
    pack_into("<HBB", result, 4, 1, 1, 0)
    result[8:24] = DATABASE
    pack_into("<QQQQQQ", result, 24, 0x100000001, 10, 1, 20, 1, lsn)
    upper = DIGEST
    for slot, cell in enumerate(cells):
        upper -= len(cell)
        pack_into("<HH", result, 128 + slot * 4, upper, len(cell))
        result[upper : upper + len(cell)] = cell
    pack_into("<HHH", result, 72, len(cells), 128 + 4 * len(cells), upper)
    result[DIGEST:] = sha256(result[:DIGEST]).digest()
    return bytes(result)


if __name__ == "__main__":
    literals = {
        "row7.bin": row_record(7),
        "empty-before.bin": row_leaf([], 11),
        "row7-after.bin": row_leaf([7], 12),
        "rows3-9-before.bin": row_leaf([3, 9], 11),
        "rows3-7-9-after.bin": row_leaf([3, 7, 9], 12),
        "bad-row-before.bin": row_leaf([7], 11, corrupt_row=7),
        "rows84-before.bin": row_leaf(range(2, 86), 11, bytes.fromhex("0100000000"), 85),
        "rows85-after.bin": row_leaf(range(2, 87), 12, bytes.fromhex("0100000000"), 85),
    }
    for name, data in literals.items():
        (HERE / name).write_bytes(data)
