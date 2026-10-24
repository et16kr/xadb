#!/usr/bin/env python3
"""Independent XRB1 literals; source preparation is not generation approval.

Only hashlib/struct and immutable pinned genesis bytes are used. No product or
previous generator is imported. These snapshots are byte/census inputs, not an
original WAL, a selected base, an INIT proof, or a native recovery database.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
PAGE, BASE = 16384, 1048576
DB, FILE = bytes(range(1, 17)), 0x100000001
MAX = (1 << 64) - 1
WIRE = "0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2"
OLD_INDEX = "0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13"
OLD_MANIFEST = "82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22"
NAMES = ("reserved", "cancelled", "init", "start", "active", "rollback", "ready", "finish",
         "active2", "active32", "ready55", "mixed55", "ordinal63", "epoch2", "reserved32")
SECTIONS = ((4096, 55296, 36, 1536), (65536, 16384, 1, 16384),
            (81920, 98304, 6, 16384), (180224, 22528, 64, 352),
            (202752, 6144, 64, 96), (208896, 16384, 1, 16384))
CAP, SECONDS = 16 << 20, 30


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).digest()


def sha(raw):
    return digest(raw).hex()


def put(raw, offset, value, width=8):
    struct.pack_into({2: "<H", 4: "<I", 8: "<Q"}[width], raw, offset, value)


def get(raw, offset, width=8):
    return struct.unpack_from({2: "<H", 4: "<I", 8: "<Q"}[width], raw, offset)[0]


def seal(raw):
    raw[-32:] = digest(raw[:-32])
    return bytes(raw)


def page_ref(number, incarnation=1):
    return struct.pack("<QQQ", FILE, number, incarnation)


def actor(local, generation=2):
    return DB + struct.pack("<QI4xQQ", 1, 1, local, generation)


def system(kind, number=1):
    return DB + struct.pack("<QB7xQQ", 1, kind, number, 1)


def owner(reference, system_owner=False):
    return bytes([2 if system_owner else 1]) + bytes(7) + reference


def page(number, kind, object_id, lsn, cells, incarnation=1):
    raw = bytearray(PAGE)
    raw[:4] = b"XDP1"
    put(raw, 4, 1, 2)
    raw[6] = kind
    raw[8:24] = DB
    for at, value in ((24, FILE), (32, number), (40, incarnation), (48, object_id), (56, 1), (64, lsn)):
        put(raw, at, value)
    put(raw, 72, len(cells), 2)
    put(raw, 74, 128 + 4 * len(cells), 2)
    end = PAGE - 32
    for i, cell in enumerate(cells):
        end -= len(cell)
        require(end >= 128 + 4 * len(cells), "page geometry")
        put(raw, 128 + 4 * i, end, 2)
        put(raw, 130 + 4 * i, len(cell), 2)
        raw[end:end + len(cell)] = cell
    put(raw, 76, end, 2)
    return seal(raw)


def cell(raw, ordinal=0):
    start, length = get(raw, 128 + 4 * ordinal, 2), get(raw, 130 + 4 * ordinal, 2)
    return raw[start:start + length]


def rows(values, peak):
    raw = bytearray(640)
    for i in range(5):
        at = 128 * i
        raw[at] = i + 1
        if i in values:
            steps, wal, pages, protected = values[i]
            for o, n in ((8, steps), (16, wal), (40, pages), (112, protected)):
                put(raw, at + o, n)
            for field, n in enumerate(peak):
                put(raw, at + 64 + field * 8, n)
    return bytes(raw)


def home(state=0, local=1, generation=1):
    raw = bytearray(224)
    raw[:4] = b"XSH1"
    put(raw, 4, 1, 2)
    put(raw, 6, 32, 2)
    raw[32:36] = b"XTS1"
    put(raw, 36, 1, 2)
    raw[38] = state
    if state:
        raw[40:88] = actor(local, generation)
    else:
        put(raw, 80, generation)
    if state == 2:
        raw[39] = 2
    return bytes(raw)


def descriptor(profile, reference, state, apply, previous, birth, groups, remaining,
               consumed, core=None, progress=None, root_page=None, inventory=None, minimum=None):
    raw = bytearray(1536)
    raw[:4] = b"XOD1"
    put(raw, 4, 1, 2)
    raw[6] = state
    put(raw, 8, 1536, 4)
    raw[16:72] = owner(reference, progress is not None)
    for at, n in ((72, apply), (80, previous), (88, birth), (544, apply if inventory else birth),
                  (608, consumed), (624, groups), (1336, birth if minimum is None else minimum)):
        put(raw, at, n)
    raw[96] = 1
    put(raw, 104, get(profile, 40))
    put(raw, 112, 4096, 4)
    raw[128:160] = digest(profile)
    if core is not None:
        raw[160:352] = core[32:]
    if progress is not None:
        kind, phase, sequence, cursor, limit = progress
        raw[352:354] = bytes([kind, phase])
        for at, n in ((360, sequence), (368, cursor), (376, limit)):
            put(raw, at, n)
        raw[448:480] = digest(b"")
    if root_page is not None:
        raw[384] = 7
        put(raw, 392, get(cell(root_page), 64))
        put(raw, 400, PAGE, 4)
        raw[416:448], raw[448:480] = digest(root_page), digest(cell(root_page))
    put(raw, 552, int(inventory is not None))
    raw[560:592] = digest(inventory or b"")
    raw[640:1280] = remaining
    for field in range(6):
        put(raw, 1280 + 8 * field, max(get(remaining, i * 128 + 64 + field * 8) for i in range(5)))
    put(raw, 1328, sum(get(remaining, i * 128 + 112) for i in range(5)))
    return seal(raw)


def inventory(active, generation, capture, item=None):
    table = b"".join(d[16:72] for d in active)
    chunk = b""
    if item is not None:
        chunk = bytearray(256)
        chunk[:4] = b"XOI1"
        put(chunk, 4, 1, 2)
        chunk[6:8] = bytes([2, 2])
        put(chunk, 8, 128, 2)
        put(chunk, 10, 96, 2)
        put(chunk, 12, 256, 4)
        chunk[16:32] = DB
        for at, n in ((32, 1), (40, generation), (48, capture), (64, 1)):
            put(chunk, at, n)
        put(chunk, 80, 1, 4)
        chunk[88:120] = digest(table)
        chunk[128:224] = item
        chunk = seal(chunk)
    root = bytearray(160 + len(table))
    root[:4] = b"XOI1"
    put(root, 4, 1, 2)
    root[6:8] = bytes([1, 2])
    put(root, 8, 128, 2)
    put(root, 10, 96, 2)
    put(root, 12, len(root), 4)
    root[16:32] = DB
    for at, n in ((32, 1), (40, generation), (48, capture), (64, int(item is not None)),
                  (72, int(item is not None)), (80, len(chunk))):
        put(root, at, n)
    put(root, 56, len(active), 2)
    root[88:120] = digest(chunk)
    root[128:-32] = table
    result = bytearray(PAGE)
    result[:len(root)] = seal(root)
    result[2176:2176 + len(chunk)] = chunk
    return bytes(result)


def snapshot(name, references, peak):
    require(name in NAMES, "closed snapshot")
    profile = references["profile"]
    count = {"reserved": 0, "cancelled": 0, "active2": 2, "active32": 32,
             "ready55": 55, "mixed55": 55, "reserved32": 32}.get(name, 1)
    active_limit = 32 if name == "mixed55" else count
    ready_all = name in ("ready", "ready55", "epoch2", "finish")
    root_cell = bytearray(cell(references["P01"]))
    free = bytes(cell(references["P02"]))
    jobs = [free] * 64
    status_pages, initial_pages, txs = {}, {}, []
    alloc_cell = bytearray(cell(references["P00"]))
    for i in range(count):
        at = 64 + (9 + i) * 96
        alloc_cell[at:at + 96] = bytes(96)
        alloc_cell[at:at + 2] = bytes([2, 9])
        for o, n in ((8, 2), (16, 2), (24, 1)):
            put(alloc_cell, at + o, n)
        initial_pages[9 + i] = page(9 + i, 9, 2, 2 * i + 2, [home()], 2)
        status_pages[9 + i] = initial_pages[9 + i]
    reserved = name == "reserved"
    cancelled = name == "cancelled"
    if reserved or cancelled:
        at = 64 + 9 * 96
        alloc_cell[at:at + 96] = bytes(96)
        put(alloc_cell, at + 8, 2)
        if reserved:
            alloc_cell[at], alloc_cell[at + 2] = 1, 2
            alloc_cell[at + 32:at + 80] = system(1)
    allocation_lsn = 1 if reserved else 2 if cancelled else 2 * count
    allocator = page(0, 10, 1, allocation_lsn, [alloc_cell])
    age = name not in ("reserved", "cancelled", "init")
    lsn = 2 * count + int(age)
    age_birth, age_previous, age_apply = lsn, 0, lsn
    issued = completed = leased_mask = ready_mask = 0
    job_lsn = [0] * 5
    if age:
        root_cell[6] = 1
        root_cell[16:64] = system(3)
        put(root_cell, 64, 2)
        put(root_cell, 80, lsn)
    if age and name != "start":
        for i in range(count):
            ordinal = 63 if name == "ordinal63" else i
            lsn += 1
            birth = lsn
            issued += 1
            body = home(1, i + 1, 2)
            terminal = rows({2: (1, 56712, 0, 56712), 3: (1, 3040, 0, 3040)}, peak)
            tx = descriptor(profile, actor(i + 1), 1, lsn, 0, birth, 1, terminal, 56712, core=body)
            j = bytearray(1024)
            j[:4] = b"XQJ1"
            put(j, 4, 1, 2)
            j[6:8] = bytes([1, 1])
            put(j, 8, 2)
            j[16:64] = actor(i + 1)
            put(j, 64, birth)
            j[80:120] = page_ref(9 + i, 2) + struct.pack("<H6xQ", 0, 2)
            j[122] = 16
            put(j, 376, lsn)
            j[384:] = rows({3: (1, 55112, 0, 55112)}, peak)
            age_previous, age_apply = age_apply, lsn
            leased_mask |= 1 << ordinal
            if ready_all or i >= active_limit or name == "rollback":
                lsn += 1
                body = home(2, i + 1, 2)
                tx = descriptor(profile, actor(i + 1), 2, lsn, birth, birth, 2,
                                rows({2: (1, 56712, 0, 56712)}, peak), 59752, core=body)
                if name != "rollback":
                    lsn += 1
                    body = home(4, i + 1, 2)
                    j[6], j[120], j[121] = 2, 2, 1
                    put(j, 72, lsn)
                    j[328:360] = digest(tx)
                    put(j, 376, lsn)
                    age_previous, age_apply = age_apply, lsn
                    leased_mask &= ~(1 << ordinal)
                    ready_mask |= 1 << ordinal
                    tx = None
            if name == "finish":
                lsn += 1
                body = home(0, generation=2)
                j = bytearray(free)
                put(j, 8, 2)
                ready_mask = 0
                completed = 1
                age_previous, age_apply = age_apply, lsn
                put(root_cell, 72, 1)
            if tx is not None:
                txs.append(tx)
            jobs[ordinal] = bytes(j)
            # Rollback has no queue redo: its Job page keeps the Begin LSN.
            job_lsn[ordinal // 15] = birth if name == "rollback" else lsn
            status_pages[9 + i] = page(9 + i, 9, 2, lsn, [body], 2)
    if age:
        revision = 2 + 2 * issued - leased_mask.bit_count() + completed
        for at, n in ((64, revision), (80, age_apply), (88, issued),
                      (96, MAX ^ (leased_mask | ready_mask)), (104, leased_mask), (112, ready_mask), (376, completed)):
            put(root_cell, at, n)
        root_cell[128:160] = digest(b"".join(jobs))
        debt = (leased_mask | ready_mask).bit_count()
        root_cell[160:192] = digest(rows({3: (debt, debt * 55112, 0, debt * 55112)} if debt else {}, peak))
    root_page = page(1, 12, 4, age_apply if age else 0, [root_cell])
    queue = [root_page] + [page(2 + i, 13, 4, job_lsn[i], jobs[i * 15:min(i * 15 + 15, 64)]) for i in range(5)]
    late_reserve = name == "reserved32"
    if late_reserve:
        lsn += 1
        allocation_lsn = lsn
        at = 64 + 41 * 96
        alloc_cell[at:at + 96] = bytes(96)
        alloc_cell[at], alloc_cell[at + 2] = 1, 2
        put(alloc_cell, at + 8, 2)
        alloc_cell[at + 32:at + 80] = system(1, 33)
        allocator = page(0, 10, 1, lsn, [alloc_cell])
        reserved = True
    high_water = count + 1 if late_reserve else max(count, 1)
    item = None
    if reserved:
        item = bytearray(96)
        item[:2] = bytes([1, 3])
        item[8:32] = page_ref(41 if late_reserve else 9, 2)
        put(item, 64, 1)
        put(item, 72, allocation_lsn)
        item = bytes(item)
    alloc_remaining = rows({2: (1, 38000, 1, 70768)} if reserved else {}, (2, 114688, 0, 0, 1, 0))
    allocation_descriptor = descriptor(profile, system(1, high_water), 5 if reserved else 6,
        allocation_lsn, 0 if reserved else allocation_lsn - 1, allocation_lsn if reserved else allocation_lsn - 1,
        1 if reserved else 2, alloc_remaining, 21200 if reserved else 42400 if cancelled else 59200,
        progress=(1, 1 if reserved else 4, 1, 0 if reserved else 1, 1), inventory=item, minimum=allocation_lsn if reserved else 0)
    # Finished allocation inventory change follows terminal apply, not birth.
    allocation_descriptor = bytearray(allocation_descriptor)
    put(allocation_descriptor, 544, allocation_lsn)
    allocation_descriptor = seal(allocation_descriptor)
    descriptors = bytearray(55296)
    for i, d in enumerate(txs):
        descriptors[i * 1536:(i + 1) * 1536] = d
    descriptors[32 * 1536:33 * 1536] = allocation_descriptor
    aging = None
    if age:
        debt = (leased_mask | ready_mask).bit_count()
        aging = descriptor(profile, system(3), 5, age_apply, age_previous, age_birth,
            get(root_cell, 64) - 1, rows({3: (debt, debt * 55112, 0, debt * 55112)} if debt else {}, peak),
            19672 + completed * 55112, progress=(3, 2, get(root_cell, 64), get(root_cell, 72), 64), root_page=root_page)
        descriptors[34 * 1536:35 * 1536] = aging
    active = txs + ([allocation_descriptor] if reserved else []) + ([aging] if aging else [])
    generation = 2 if name == "epoch2" else 1
    enumerated_item = None if item is None else bytearray(item)
    if enumerated_item is not None:
        put(enumerated_item, 2, len(txs), 2)
    inventory_bytes = inventory(active, generation, allocation_lsn if not age else lsn, enumerated_item)
    status = bytearray(64 * 352)
    for number, image in status_pages.items():
        at = number * 352
        status[at:at + 4] = bytes([1, 9, 0, 1])
        status[at + 8:at + 56] = image[24:72]
        put(status, at + 56, 2 * (number - 9) + 2)
        status[at + 64:at + 96] = digest(image)
        status[at + 128:at + 352] = cell(image)
    images = {0: allocator, **{i + 1: q for i, q in enumerate(queue)},
              7: references["P07"], 8: references["P08"], **status_pages}
    certs = bytearray(64 * 96)
    for number, image in images.items():
        at = number * 96
        certs[at:at + 3] = bytes([1, image[6], image[7]])
        certs[at + 8:at + 56] = image[24:72]
        certs[at + 56:at + 88] = digest(image)
    raw = bytearray(BASE)
    raw[:4] = b"XRB1"
    for at, value, width in ((4, 1, 2), (6, 1, 2), (8, BASE, 4), (12, 4096, 4)):
        put(raw, at, value, width)
    raw[16:32] = DB
    capture = allocation_lsn if not age else lsn
    suffix_bytes = (int(age) * 19672 + issued * 56712
                    + (ready_mask.bit_count() + completed + int(name == "rollback")) * 3040
                    + (ready_mask.bit_count() + completed) * 56712 + completed * 55112)
    source_bytes = count * 59200 + suffix_bytes + 21200 if late_reserve else 21200 if reserved else 42400 if cancelled else count * 59200 + suffix_bytes
    for at, value in ((32, 1), (40, get(profile, 40)), (80, 4), (88, generation), (96, capture),
                      (152, 4096 + source_bytes),
                      (192, issued), (208, high_water), (224, int(age)), (240, 5), (248, capture),
                      (264, sum(1 << number for number in status_pages)), (272, leased_mask), (280, ready_mask),
                      (288, get(root_cell, 64)), (296, get(root_cell, 64) - 1 if age else 0),
                      (448, get(references["filemap"], 32)), (504, 1), (1152, 32 << 20)):
        put(raw, at, value)
    raw[48:80] = digest(profile)
    put(raw, 144, 1, 4)
    put(raw, 148, 1, 4)
    raw[160:192] = digest(b"XRB1 independent snapshot prefix claim:" + name.encode())
    put(raw, 256, len(txs), 4)
    put(raw, 260, int(age) + int(reserved), 4)
    raw[304] = 0 if reserved else 1 if cancelled else 2
    raw[320] = 1
    put(raw, 328, get(profile, 40))
    put(raw, 336, 4096, 4)
    raw[352:384] = digest(profile)
    raw[384:448] = references["filemap"][80:144]
    raw[464:496] = digest(references["bootstrap"])
    raw[1192:1224] = digest(references["wal-header"])
    put(raw, 1184, 1, 4)
    for category in range(5):
        at = 512 + category * 128
        raw[at] = category + 1
        for offset in (8, 16, 24, 32, 40, 48, 56, 112):
            put(raw, at + offset, sum(get(d, 640 + category * 128 + offset) for d in active))
        for offset in range(64, 112, 8):
            put(raw, at + offset, max((get(d, 640 + category * 128 + offset) for d in active), default=0))
    for (offset, length, _, _), contents in zip(SECTIONS, (descriptors, allocator, b"".join(queue), status, certs, inventory_bytes)):
        require(len(contents) == length, "section exact length")
        raw[offset:offset + length] = contents
    if name == "epoch2":
        previous = snapshot("ready", references, peak)
        put(raw, 104, 1)
        raw[112:144] = digest(previous)
        put(raw, 144, 2, 4)
        put(raw, 1184, 2, 4)
        put(raw, 152, 4096)
        put(raw, 192, 0)
        raw[1192:1224] = digest(b"XWE2 pure source header claim")
    for i, (offset, length, count_items, entry) in enumerate(SECTIONS):
        at = 1280 + i * 64
        put(raw, at, i + 1, 2)
        for field, value in ((4, offset), (8, length), (12, count_items), (16, entry)):
            put(raw, at + field, value, 4)
        raw[at + 32:at + 64] = digest(raw[offset:offset + length])
    return seal(raw)


def create(path, raw):
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def main():
    start = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--approval", type=Path, required=True)
    parser.add_argument("--approval-sha256", required=True)
    args = parser.parse_args()
    source = Path(__file__).read_bytes()
    input_plan_path = Path(__file__).with_name("input-plan.json")
    input_plan_raw = input_plan_path.read_bytes()
    approval_raw = args.approval.read_bytes()
    require(sha(approval_raw) == args.approval_sha256, "approval hash")
    approval = json.loads(approval_raw)
    require(approval.get("schema") == "XADB_XRB1_GENERATION_GATE_V1" and approval.get("status") == "APPROVED_FOR_GENERATION", "generation gate")
    require(approval.get("generator_sha256") == sha(source) and approval.get("wire_sha256") == WIRE, "frozen source")
    require(approval.get("input_plan_sha256") == sha(input_plan_raw), "input plan binding")
    require(approval.get("names") == list(NAMES) and approval.get("raw_bytes") == len(NAMES) * BASE, "exact scope")
    peak = tuple(approval["resource_peak"])
    require(len(peak) == 6 and all(type(n) is int and 0 <= n <= MAX for n in peak), "explicit peak")
    require(approval.get("resource_peak_status") in ("APPROVED_SEMANTIC_INPUT", "F05_ACCEPTED"), "peak scope")
    require(len(approval.get("resource_peak_evidence_sha256", "")) == 64, "peak evidence")
    old = args.repo.resolve(strict=True) / "tests/fixtures/dev3-plan4-genesis"
    old_manifest, old_index = (old / "manifest.json").read_bytes(), (old / "raw-index.tsv").read_bytes()
    require(sha(old_manifest) == OLD_MANIFEST and sha(old_index) == OLD_INDEX, "original identities")
    wanted = {f"P{i:02}": f"P{i:02}" for i in range(9)}
    wanted.update({"REF-P1-d3-profile": "profile", "REF-P2-d3-profile": "other-profile",
                   "REF-P1-FILEMAP": "filemap", "XBS-P1": "bootstrap", "REF-P1-d3-wal-00000001": "wal-header"})
    refs, provenance = {}, []
    for line in old_index.decode().splitlines()[1:]:
        key, path, length, expected = line.split("\t")
        if key in wanted:
            raw = (old / path).read_bytes()
            require(len(raw) == int(length) and sha(raw) == expected, "pinned original raw")
            refs[wanted[key]] = raw
            provenance.append(dict(path=str(old / path), bytes=len(raw), sha256=expected))
    require(len(refs) == 14, "exact original references")
    args.output.mkdir(parents=False, exist_ok=False)
    (args.output / "raw").mkdir()
    (args.output / "references").mkdir()
    files = {}
    for key, raw in refs.items():
        path = f"references/{key}.bin"
        create(args.output / path, raw)
        files[path] = dict(bytes=len(raw), sha256=sha(raw))
    for name in NAMES:
        raw = snapshot(name, refs, peak)
        path = f"raw/{name}.xrb"
        create(args.output / path, raw)
        files[path] = dict(bytes=len(raw), sha256=sha(raw))
        require(time.monotonic() - start <= SECONDS, "generation elapsed")
    index = "path\tbytes\tsha256\n" + "".join(f"{p}\t{v['bytes']}\t{v['sha256']}\n" for p, v in sorted(files.items()))
    create(args.output / "raw-index.tsv", index.encode())
    manifest = dict(schema="XADB_XRB1_LITERAL_INPUT_V1", status="GENERATED_INPUT_NOT_PRODUCT_EVIDENCE",
                    wire_sha256=WIRE, generator_sha256=sha(source), input_plan_sha256=sha(input_plan_raw), approval_sha256=sha(approval_raw),
                    raw_files=15, raw_bytes=15 * BASE, reference_files=14, reference_bytes=sum(map(len, refs.values())),
                    resource_peak=list(peak), resource_peak_status=approval["resource_peak_status"],
                    source_provenance=provenance, files=files, index_sha256=sha(index.encode()),
                    limitations=["prefix SHA and epoch2 header are explicit pure claims", "No selected closure/INIT/WAL/native authority"])
    create(args.output / "manifest.json", (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode())
    for row in provenance:
        require(sha(Path(row["path"]).read_bytes()) == row["sha256"], "original changed")
    require((old / "manifest.json").read_bytes() == old_manifest and (old / "raw-index.tsv").read_bytes() == old_index, "original metadata changed")
    require(Path(__file__).read_bytes() == source and args.approval.read_bytes() == approval_raw, "source/approval changed")
    require(input_plan_path.read_bytes() == input_plan_raw, "input plan changed")
    require(sum(p.stat().st_size for p in args.output.rglob("*") if p.is_file()) <= CAP, "output cap")
    require(time.monotonic() - start <= SECONDS, "finalization elapsed")
    print(json.dumps(dict(status="INPUT_GENERATED", files=len(files), raw_bytes=15 * BASE,
                          elapsed=time.monotonic() - start, index_sha256=sha(index.encode())), sort_keys=True))


if __name__ == "__main__":
    main()
