#!/usr/bin/env python3
"""Independent OVF-F01 literals (dev3-row-overflow.md#chunk-revision).

Built only from the contracts with struct/hashlib: the page envelope of
dev3-page-format.md (F01 bytes), the chunk cell of dev3-overflow-format.md,
and the revision that makes cell [24,48) reserved zero. It first rebuilds the
pre-revision literal recorded in dev3-overflow-format.md (root in the cell)
to check itself, then prints the revised literal. No product encoder is used.
"""
import hashlib
import struct

PAGE = 16384
DIGEST_AT = PAGE - 32
DB = bytes([11] * 16)
FILE = 0x0000000100000002
ROOT_PAGE, INCARNATION, OWNER, OWNER_GEN, LSN = 3, 4, 5, 6, 7


def page_ref(page_no):
    return struct.pack("<QQQ", FILE, page_no, INCARNATION)


def cell(payload, with_root):
    total = len(payload)
    count = -(-total // 16092)
    digest = hashlib.sha256(payload).digest()
    header = b"XOV1" + struct.pack("<HHHH", 1, 0, count, 0)
    header += struct.pack("<II", total, len(payload)) + bytes(4)
    header += page_ref(ROOT_PAGE) if with_root else bytes(24)
    header += bytes(24)  # last chunk: no next
    header += digest + bytes(24)
    assert len(header) == 128
    return header + payload


def page(cell_bytes):
    upper = DIGEST_AT - len(cell_bytes)
    header = b"XDP1" + struct.pack("<HBB", 1, 7, 0) + DB
    header += struct.pack("<QQQQQQ", FILE, ROOT_PAGE, INCARNATION, OWNER, OWNER_GEN, LSN)
    header += struct.pack("<HHH", 1, 128 + 4, upper) + bytes(50)
    assert len(header) == 128
    body = header + struct.pack("<HH", upper, len(cell_bytes))
    body += bytes(upper - len(body)) + cell_bytes
    assert len(body) == DIGEST_AT
    checksum = hashlib.sha256(body).digest()
    full = body + checksum
    return checksum.hex(), hashlib.sha256(full).hexdigest()


payload = bytes((i * 17 + 3) % 256 for i in range(2049))
assert hashlib.sha256(payload).hexdigest() == (
    "22f0a97231b1c2f1c8e4390f637461035e336295817ba70929f902e11b5e9809")
# Pre-revision literal of dev3-overflow-format.md.
assert page(cell(payload, True)) == (
    "1f97d1d77b1d98246096de2d96fc2675fe08524a5d377fae0534a26538c02a3b",
    "cd8499269433bc243136e100edb13f7328d0563d2a74f1098f23ec1cfc4634d5")
revised = cell(payload, False)
checksum, full = page(revised)
print("cell header", revised[:128].hex())
print("page checksum", checksum)
print("full-page sha256", full)
