#!/usr/bin/env python3
"""Independent D2 check for 08-3 O2a (review-r1): contract integers and,
with a dump directory, the ChunkWrite frames byte by byte.

Only struct/hashlib and the contract's numbers are used; no product
encoder or decoder. Usage:

    python3 -I review_r1_check.py            # integers only
    python3 -I review_r1_check.py DUMP_DIR   # integers and frames

DUMP_DIR holds what a scratch-only probe test wrote (not part of the
branch): chunk_frame_{n}.bin, alloc_before_{n}.bin, alloc_after_{n}.bin,
job_before_{n}.bin, job_after_{n}.bin, lsn_{n}.txt, lease.txt, value.bin
and db.bin for the three ChunkWrites of a 40,018-byte row (k = 3).
"""
import hashlib
import struct
import sys

PAGE = 16384
DIGEST_OFFSET = PAGE - 32
PAGE_HEADER = 128
CHUNK_HEADER = 128
PAYLOAD = DIGEST_OFFSET - PAGE_HEADER - 4 - CHUNK_HEADER  # 16,092
ALLOC_HEADER = 64
ALLOC_ENTRY = 96
ALLOC_CELL = ALLOC_HEADER + 64 * ALLOC_ENTRY
KIND_OVERFLOW, KIND_ALLOCATION, KIND_ROOT, KIND_JOBS = 7, 10, 12, 13
TABLE_OBJECT, TABLE_GENERATION = 6, 1

failures = []
FILE = None


def check(label, ok):
    print(("PASS " if ok else "FAIL ") + label)
    if not ok:
        failures.append(label)


def sha(data):
    return hashlib.sha256(data).digest()


def u16(b, at):
    return struct.unpack_from("<H", b, at)[0]


def u32(b, at):
    return struct.unpack_from("<I", b, at)[0]


def u64(b, at):
    return struct.unpack_from("<Q", b, at)[0]


# ---- contract integers (dev3-row-overflow.md #opcodes, #ledger) ----------
P = 16_344                       # whole-body Patch: 64 + (16352 - 72)
assert P == 64 + (DIGEST_OFFSET - 72)
XHG = 3_776                      # XWF1 160 + XHG 256 + 2 x 1536 + core 256 + digest 32
NATIVES_AT = 160 + 256 + 2 * 1536 + 256
assert XHG == NATIVES_AT + 32
natives = (128 + 512 + 512) + (128 + 1024 + 1024) + (128 + 96 + 96) + (128 + 128)
redo = 2 * P + (64 + ALLOC_ENTRY) + (64 + PAGE)
W = XHG + natives + redo
check(f"W = {W} (natives {natives}, redo {redo}) == 56,976", W == 56_976)
check("allocator entry patch 160", 64 + ALLOC_ENTRY == 160)
check("chunk INIT 16,448", 64 + PAGE == 16_448)
check("chunk payload 16,092", PAYLOAD == 16_092)

C = 12 + 168 + 64
check("overflow cell C = 244", C == 244)
for h in (0, 1):
    F = 91_304 + C + 128 * h
    clr = 91_096 + C + 128 * h
    prune = 89_984 + 128 * h
    R = 55_528
    X = 59_056
    cleanup = 311_608 + 2 * C + 128 * h
    check(f"h={h}: F = {F} == 91,548+128h", F == 91_548 + 128 * h)
    check(f"h={h}: CLR = {clr} == 91,340+128h", clr == 91_340 + 128 * h)
    check(f"h={h}: cleanup = {cleanup} == 312,096+128h", cleanup == 312_096 + 128 * h)
    check(f"h={h}: Prune path {prune + 4 * R} == cleanup", prune + 4 * R == cleanup)
    check(f"h={h}: CLR returns {cleanup - (4 * R + X)} == 30,928+128h",
          cleanup - (4 * R + X) == 30_928 + 128 * h)
check("4R + X = 281,168", 4 * 55_528 + 59_056 == 281_168)
# ledger row 1 over the written count for k = 3 (the tests' 40,018 B row), h = 1
total = 40_018
k = -(-total // PAYLOAD)
check("k(40,018) = 3", k == 3)
F1 = 91_548 + 128
rows = [(1 + k - w, (k - w) * W + F1) for w in range(k + 1)]
check(f"row1 by written {rows}",
      rows == [(4, 262_604), (3, 205_628), (2, 148_652), (1, 91_676)])
check("k=9 row1 9W+F (h=1) = 604,460", 9 * W + F1 == 604_460)
check("k=9 root subtotal 974,020",
      57_336 + 9 * W + F1 + (89_984 + 128) + 4 * 55_528 == 974_020)
# measured group sizes printed by the O2a tests
check("Forward(overflow) frame 91,676 (h=1)", F1 == 91_676)
check("CLR(overflow) frame 91,468 (h=1)", 91_340 + 128 == 91_468)
check("Prune(overflow) frame 90,112 (h=1)", 89_984 + 128 == 90_112)

if len(sys.argv) < 2:
    sys.exit(1 if failures else 0)

# ---- frames -------------------------------------------------------------
dump = sys.argv[1]


def read(name):
    with open(f"{dump}/{name}", "rb") as f:
        return f.read()


def page_ok(page, kind, page_no, incarnation, owner, owner_gen, lsn, db):
    return (page[:4] == b"XDP1" and u16(page, 4) == 1 and page[6] == kind
            and page[7] == 0 and page[8:24] == db and u64(page, 24) == FILE
            and u64(page, 32) == page_no and u64(page, 40) == incarnation
            and u64(page, 48) == owner and u64(page, 56) == owner_gen
            and u64(page, 64) == lsn and page[DIGEST_OFFSET:] == sha(page[:DIGEST_OFFSET]))


value = read("value.bin")
db = read("db.bin")
lease = int(read("lease.txt"))
check("value length 40,018", len(value) == total)
digest = sha(value)
front = None  # (page_no, incarnation) of the pending front
for n in range(3):
    frame = read(f"chunk_frame_{n}.bin")
    before = read(f"alloc_before_{n}.bin")
    after = read(f"alloc_after_{n}.bin")
    jb, ja = read(f"job_before_{n}.bin"), read(f"job_after_{n}.bin")
    lsn = int(read(f"lsn_{n}.txt"))
    index = k - 1 - n
    tag = f"chunk{index}"
    FILE = u64(before, 24)  # the first file's FileId, from the allocator header
    check(f"{tag}: frame {len(frame)} == W", len(frame) == W)
    check(f"{tag}: frame digest", frame[-32:] == sha(frame[:-32]))
    # independent allocator pick: lowest ordinal >= 9 that is Free with a
    # next generation, no Reserved entry anywhere
    cell_start = DIGEST_OFFSET - ALLOC_CELL
    check(f"{tag}: allocator single cell at {cell_start}",
          u16(before, 72) == 1 and u16(before, PAGE_HEADER) == cell_start
          and u16(before, PAGE_HEADER + 2) == ALLOC_CELL)
    entries = [before[cell_start + ALLOC_HEADER + i * ALLOC_ENTRY:
                      cell_start + ALLOC_HEADER + (i + 1) * ALLOC_ENTRY] for i in range(64)]
    check(f"{tag}: no Reserved entry", all(e[0] != 1 for e in entries))
    pick = next(i for i in range(9, 64)
                if entries[i][0] == 0 and u64(entries[i], 8) != 2**64 - 1)
    gen = u64(entries[pick], 8) + 1
    check(f"{tag}: pick page {pick} generation {gen} == 15+{n}, 2", (pick, gen) == (15 + n, 2))
    at = cell_start + ALLOC_HEADER + pick * ALLOC_ENTRY
    entry = bytearray(ALLOC_ENTRY)
    entry[0], entry[1] = 2, KIND_OVERFLOW
    struct.pack_into("<QQQ", entry, 8, gen, TABLE_OBJECT, TABLE_GENERATION)
    check(f"{tag}: allocator after entry Allocated(2, table, Overflow)",
          after[at:at + ALLOC_ENTRY] == bytes(entry))
    # literal Patch replay: entry bytes + header LSN + page digest
    replay = bytearray(before)
    replay[at:at + ALLOC_ENTRY] = entry
    struct.pack_into("<Q", replay, 64, lsn)
    replay[DIGEST_OFFSET:] = sha(bytes(replay[:DIGEST_OFFSET]))
    check(f"{tag}: allocator after == before + 96 B patch + LSN + digest", bytes(replay) == after)
    # natives at 3,744: allocator(6), Root(8), Jobs(9), chunk(12)
    check(f"{tag}: XWF1 160 then XHG1", frame[:4] == b"XWF1" and frame[160:164] == b"XHG1"
          and u32(frame, 160 + 228) == natives and u32(frame, 160 + 240) == redo)
    at_native = NATIVES_AT
    kinds, headers = [], []
    for _ in range(4):
        hdr = frame[at_native:at_native + 128]
        kinds.append(hdr[0])
        headers.append((at_native, hdr))
        at_native += u32(hdr, 20)
    check(f"{tag}: native kinds {kinds} == [6, 8, 9, 12]", kinds == [6, 8, 9, 12])
    pos, hdr = headers[0]
    body = frame[pos + 128:pos + 128 + 192]
    check(f"{tag}: kind 6 header presence 3 slot 0/0 offset {at}/{at} 96/96 total 320",
          hdr[1] == 3 and u16(hdr, 4) == 0 and u16(hdr, 6) == 0 and u16(hdr, 8) == at
          and u16(hdr, 10) == at and u32(hdr, 12) == 96 and u32(hdr, 16) == 96
          and u32(hdr, 20) == 320 and hdr[88:128] == bytes(40))
    check(f"{tag}: kind 6 bodies are the entry before/after with digests",
          body[:96] == before[at:at + 96] and body[96:] == bytes(entry)
          and hdr[24:56] == sha(body[:96]) and hdr[56:88] == sha(body[96:]))
    length = min(PAYLOAD, total - index * PAYLOAD)
    chunk_cell = DIGEST_OFFSET - (CHUNK_HEADER + length)
    pos, hdr = headers[3]
    nbody = frame[pos + 128:pos + 256]
    check(f"{tag}: kind 12 header presence 2, before slot MAX/offset 0, after slot 0 offset {chunk_cell}, 0/128, total 256",
          hdr[1] == 2 and u16(hdr, 4) == 0xFFFF and u16(hdr, 8) == 0 and u16(hdr, 6) == 0
          and u16(hdr, 10) == chunk_cell and u32(hdr, 12) == 0 and u32(hdr, 16) == 128
          and u32(hdr, 20) == 256 and hdr[24:56] == bytes(32) and hdr[56:88] == sha(nbody))
    # redo: after natives (aux 0) four records in page order
    at_redo = at_native
    recs = []
    while at_redo < len(frame) - 32:
        recs.append(frame[at_redo:at_redo + u32(frame, at_redo + 4)])
        at_redo += u32(frame, at_redo + 4)
    check(f"{tag}: redo ends at the frame digest", at_redo == len(frame) - 32 and len(recs) == 4)
    r0 = recs[0]
    check(f"{tag}: redo 0 Patch Allocation page 0 owner 1 prior {u64(before, 64)} offset {at} 96",
          r0[0] == 1 and r0[1] == KIND_ALLOCATION and r0[2] == 0 and u64(r0, 8) == FILE
          and u64(r0, 16) == 0 and u64(r0, 24) == 1 and u64(r0, 32) == 1 and u64(r0, 40) == 1
          and u64(r0, 48) == u64(before, 64) and u16(r0, 56) == at and u16(r0, 58) == 96
          and r0[64:] == bytes(entry))
    check(f"{tag}: redo 1/2 whole-body Patch of Root/Jobs",
          recs[1][1] == KIND_ROOT and recs[2][1] == KIND_JOBS
          and all(r[0] == 1 and u16(r, 56) == 72 and len(r) == P for r in recs[1:3]))
    r3 = recs[3]
    image = r3[64:]
    check(f"{tag}: redo 3 INIT Overflow page {pick} inc 2 owner 6/1 prior 0 offset 0 16,384",
          r3[0] == 2 and r3[1] == KIND_OVERFLOW and r3[2] == 0 and u64(r3, 8) == FILE and u64(r3, 16) == pick
          and u64(r3, 24) == 2 and u64(r3, 32) == TABLE_OBJECT and u64(r3, 40) == TABLE_GENERATION
          and u64(r3, 48) == 0 and u16(r3, 56) == 0 and u16(r3, 58) == PAGE and len(r3) == 16_448)
    check(f"{tag}: INIT image header (kind 7, page {pick}, inc 2, owner 6/1, LSN {lsn}, digest)",
          page_ok(image, KIND_OVERFLOW, pick, 2, TABLE_OBJECT, TABLE_GENERATION, lsn, db))
    cell = image[chunk_cell:DIGEST_OFFSET]
    check(f"{tag}: one slot at {chunk_cell} of {CHUNK_HEADER + length}",
          u16(image, 72) == 1 and u16(image, 74) == PAGE_HEADER + 4 and u16(image, 76) == chunk_cell
          and u16(image, PAGE_HEADER) == chunk_cell and u16(image, PAGE_HEADER + 2) == CHUNK_HEADER + length
          and image[PAGE_HEADER + 4:chunk_cell] == bytes(chunk_cell - PAGE_HEADER - 4))
    nxt = bytes(24) if front is None else struct.pack("<QQQ", FILE, front[0], front[1])
    check(f"{tag}: cell XOV1 v1 index {index} count {k} total {total} len {length}, next = old front, root zero, digest",
          cell[:4] == b"XOV1" and u16(cell, 4) == 1 and u16(cell, 6) == index
          and u16(cell, 8) == k and cell[10:12] == bytes(2) and u32(cell, 12) == total
          and u32(cell, 16) == length and cell[20:48] == bytes(28) and cell[48:72] == nxt
          and cell[72:104] == digest and cell[104:128] == bytes(24))
    check(f"{tag}: payload is window {index}",
          cell[128:] == value[index * PAYLOAD:index * PAYLOAD + length])
    check(f"{tag}: kind 12 body is the cell header", nbody == cell[:128])
    # Job: pending front/total/written, last_step; nothing else but [328,360) and [376,384)
    exp = bytearray(jb)
    struct.pack_into("<QQQ", exp, 328, FILE, pick, 2)
    struct.pack_into("<IH", exp, 352, total, n + 1)
    struct.pack_into("<Q", exp, 376, lsn)
    check(f"{tag}: Job after = before with pending ({pick}, {total}, {n + 1}) and last_step {lsn}",
          bytes(exp) == ja and ja[285] == 1 and u32(ja, 280) == 244)
    check(f"{tag}: lease {lease} < LSN {lsn}", lease < lsn)
    front = (pick, 2)

print(f"{len(failures)} failure(s)")
sys.exit(1 if failures else 0)
