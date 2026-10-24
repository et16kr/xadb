#!/usr/bin/env python3
"""Independent G6 N=0 literals, SOURCE PREPARATION ONLY until approved.

No product module, executable, encoder or decoder is imported. This generator
uses documented byte offsets, struct and hashlib. Its one-vector output is a
semantic input after a separately proved reserve/INIT, not a recovery database.
Run only after independent source/recipe review and an explicit generation gate.
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
PAGE_BYTES = 16384
DATABASE = bytes(range(1, 17))
FILE_ID = 0x100000001
MAX_U64 = (1 << 64) - 1
WIRE_SHA = "0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2"
PLAN4_MANIFEST_SHA = "82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22"
PLAN4_INDEX_SHA = "0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13"
PRICES = {"aging-start": 19672, "begin": 56712, "rollback": 3040,
          "abort": 56712, "finish": 55112}
RAW_BYTES = 398608
RAW_FILES = 24
REFERENCE_BYTES = 151552
OUTPUT_LIMIT = 2 * 1024 * 1024
SECONDS_LIMIT = 10


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
    return struct.pack("<QQQ", FILE_ID, number, incarnation)


def slot_ref(number, slot, generation, incarnation=1):
    return page_ref(number, incarnation) + struct.pack("<H6xQ", slot, generation)


def actor():
    # TxnId(database,timeline1,epoch1,local1), Status generation2.
    return DATABASE + struct.pack("<QI4xQQ", 1, 1, 1, 2)


def aging_ref():
    return DATABASE + struct.pack("<QB7xQQ", 1, 3, 1, 1)


def owner(reference, system=False):
    return bytes([2 if system else 1]) + bytes(7) + reference


def rows(entries, peak):
    """entries maps zero-based category to (steps, wal/protected)."""
    raw = bytearray(640)
    for index in range(5):
        at = index * 128
        raw[at] = index + 1
        if index in entries:
            steps, wal = entries[index]
            require(steps > 0 and wal > 0, "nonzero row has positive debt")
            put(raw, at + 8, steps)
            put(raw, at + 16, wal)
            for field, value in enumerate(peak):
                put(raw, at + 64 + 8 * field, value)
            put(raw, at + 112, wal)
    return bytes(raw)


def home(state, generation):
    raw = bytearray(224)
    raw[:4] = b"XSH1"
    put(raw, 4, 1, 2)
    put(raw, 6, 32, 2)
    raw[32:36] = b"XTS1"
    put(raw, 36, 1, 2)
    raw[38] = state
    if state:
        raw[40:88] = actor()
    else:
        put(raw, 80, generation)
    if state == 2:
        raw[39] = 2  # Full rollback, no Undo/head/next/stop.
    return bytes(raw)


def page(number, kind, object_id, lsn, cells, incarnation=1):
    raw = bytearray(PAGE_BYTES)
    raw[:4] = b"XDP1"
    put(raw, 4, 1, 2)
    raw[6] = kind
    raw[8:24] = DATABASE
    for offset, value in ((24, FILE_ID), (32, number), (40, incarnation),
                          (48, object_id), (56, 1), (64, lsn)):
        put(raw, offset, value)
    put(raw, 72, len(cells), 2)
    put(raw, 74, 128 + 4 * len(cells), 2)
    end = PAGE_BYTES - 32
    for index, cell in enumerate(cells):
        start = end - len(cell)
        require(start >= 128 + 4 * len(cells), "page cell overlap")
        put(raw, 128 + 4 * index, start, 2)
        put(raw, 130 + 4 * index, len(cell), 2)
        raw[start:end] = cell
        end = start
    put(raw, 76, end, 2)
    return seal(raw)


def job(state, status_ref, peak, sender_digest=bytes(32)):
    raw = bytearray(1024)
    raw[:4] = b"XQJ1"
    put(raw, 4, 1, 2)
    put(raw, 8, 1 if state == "initial" else 2)
    if state in ("initial", "free"):
        return bytes(raw)
    require(state in ("leased", "ready"), "closed Job state")
    raw[6:8] = bytes([1 if state == "leased" else 2, 1])
    raw[16:64] = actor()
    put(raw, 64, 4)
    raw[80:120] = status_ref
    raw[122] = 16
    put(raw, 376, 4)
    if state == "ready":
        put(raw, 72, 6)
        raw[120:122] = bytes([2, 1])
        raw[328:360] = sender_digest
        put(raw, 376, 6)
    raw[384:] = rows({3: (1, PRICES["finish"])}, peak)
    return bytes(raw)


def root(phase, ordinal, cells, peak):
    raw = bytearray(512)
    raw[:4] = b"XQR1"
    put(raw, 4, 1, 2)
    put(raw, 8, 512, 4)
    put(raw, 12, 64, 4)
    phases = {"initial": (1, 0), "start": (2, 3), "begin": (3, 4),
              "abort": (4, 6), "finish": (5, 7)}
    revision, apply = phases[phase]
    if phase != "initial":
        raw[6] = 1
        raw[16:64] = aging_ref()
    put(raw, 64, revision)
    put(raw, 72, (ordinal + 1) % 64 if phase == "finish" else 0)
    put(raw, 80, apply)
    put(raw, 88, int(phase in ("begin", "abort", "finish")))
    occupied = phase in ("begin", "abort")
    put(raw, 96, MAX_U64 ^ ((1 << ordinal) if occupied else 0))
    put(raw, 104, (1 << ordinal) if phase == "begin" else 0)
    put(raw, 112, (1 << ordinal) if phase == "abort" else 0)
    raw[128:160] = digest(b"".join(cells))
    raw[160:192] = digest(rows({3: (1, PRICES["finish"])} if occupied else {}, peak))
    raw[192:208] = DATABASE
    for offset, value in ((208, 1), (216, 4), (224, 1),
                          (376, int(phase == "finish"))):
        put(raw, offset, value)
    raw[232:256] = page_ref(1)
    for index in range(5):
        raw[256 + index * 24:280 + index * 24] = page_ref(2 + index)
    return bytes(raw)


def descriptor(profile, peak, *, system, phase, root_page=None, status_home=None):
    raw = bytearray(1536)
    raw[:4] = b"XOD1"
    put(raw, 4, 1, 2)
    put(raw, 8, 1536, 4)
    if system:
        raw[6] = 5
        raw[16:72] = owner(aging_ref(), system=True)
        apply, previous, groups = {"start": (3, 0, 1), "begin": (4, 3, 2),
                                   "abort": (6, 4, 3), "finish": (7, 6, 4)}[phase]
        birth = 3
        consumed = PRICES["aging-start"] + (PRICES["finish"] if phase == "finish" else 0)
        remaining = {3: (1, PRICES["finish"])} if phase in ("begin", "abort") else {}
        raw[352:354] = bytes([3, 2])
        put(raw, 360, groups + 1)
        put(raw, 368, get(root_page, 15840 + 72))
        put(raw, 376, 64)
        raw[384] = 7
        put(raw, 392, groups + 1)
        put(raw, 400, PAGE_BYTES, 4)
        raw[416:448] = digest(root_page)
        raw[448:480] = digest(root_page[15840:16352])
    else:
        raw[6] = {"begin": 1, "rollback": 2, "abort": 4}[phase]
        raw[16:72] = owner(actor())
        apply, previous, groups = {"begin": (4, 0, 1), "rollback": (5, 4, 2),
                                   "abort": (6, 5, 3)}[phase]
        birth = 4
        consumed = {"begin": 56712, "rollback": 59752, "abort": 116464}[phase]
        remaining = {"begin": {2: (1, 56712), 3: (1, 3040)},
                     "rollback": {2: (1, 56712)}, "abort": {}}[phase]
        raw[160:352] = status_home[32:]
    for offset, value in ((72, apply), (80, previous), (88, birth),
                          (544, birth), (608, consumed), (624, groups), (1336, birth)):
        put(raw, offset, value)
    raw[96] = 1
    put(raw, 104, get(profile, 40))
    put(raw, 112, 4096, 4)
    raw[128:160] = digest(profile)
    raw[560:592] = digest(b"")
    raw[640:1280] = rows(remaining, peak)
    if remaining:
        for field, value in enumerate(peak):
            put(raw, 1280 + 8 * field, value)
    put(raw, 1328, sum(value[1] for value in remaining.values()))
    return seal(raw)


def native(kind, redo_index, slot, before, after):
    cell_at = 16352 - len(before) * (slot + 1)
    require(len(before) == len(after), "closed native lengths")
    raw = bytearray(128 + len(before) + len(after))
    raw[:2] = bytes([kind, 3])
    for offset, value in ((2, redo_index), (4, slot), (6, slot),
                          (8, cell_at), (10, cell_at)):
        put(raw, offset, value, 2)
    for offset, value in ((12, len(before)), (16, len(after)), (20, len(raw))):
        put(raw, offset, value, 4)
    raw[24:56] = digest(before)
    raw[56:88] = digest(after)
    raw[128:128 + len(before)] = before
    raw[128 + len(before):] = after
    return bytes(raw)


def patch(before, after, narrow=False):
    offset, size = (16128, 224) if narrow else (72, 16280)
    raw = bytearray(64 + size)
    raw[:2] = bytes([1, before[6]])
    put(raw, 4, len(raw), 4)
    raw[8:32] = before[24:48]
    raw[32:56] = before[48:72]
    put(raw, 56, offset, 2)
    put(raw, 58, size, 2)
    raw[64:] = after[offset:offset + size]
    return bytes(raw)


def envelope(payload, *, kind, tag, lsn, previous_owner, count, transaction):
    raw = bytearray(192 + len(payload))
    raw[:4] = b"XWF1"
    put(raw, 4, 1, 2)
    raw[6:8] = bytes([kind, tag])
    put(raw, 8, 160, 2)
    put(raw, 12, len(raw), 4)
    put(raw, 16, len(payload), 4)
    put(raw, 20, count, 2)
    raw[24:40] = DATABASE
    put(raw, 40, 1)
    put(raw, 48, 1, 4)
    put(raw, 56, lsn)
    put(raw, 64, lsn - 1)
    if transaction:
        raw[72:112] = actor()[:40]
        put(raw, 112, previous_owner)
    raw[128:160] = digest(raw[:128])
    raw[160:-32] = payload
    return seal(raw)


def operation(op, job_reference, status_reference, before_root, before_cells, peak):
    raw = bytearray(256)
    if op in (16, 17):
        raw[:4] = b"XRO1"
        put(raw, 4, 1, 2)
        put(raw, 6, op, 2)
        put(raw, 8, 256, 4)
        put(raw, 16, get(before_root, 64))
        put(raw, 24, get(before_root, 64) + 1)
        if op == 17:
            raw[32:72] = job_reference
            raw[72:112] = status_reference
            raw[112:160] = actor()
        raw[160:192] = digest(before_root)
        raw[192:224] = digest(b"".join(before_cells))
        put(raw, 224, 1 if op == 16 else 3)
    else:
        raw[:4] = b"XHO1"
        put(raw, 4, 1, 2)
        put(raw, 6, op, 2)
        put(raw, 8, 256, 4)
        raw[24:64] = job_reference
        raw[64:104] = status_reference
        if op == 3:
            raw[208:240] = digest(rows({}, peak))
    return bytes(raw)


def group_single(op, apply, previous, after, before, operation_bytes, natives, redos, system):
    raw = bytearray(192)
    raw[:4] = b"XSG1"
    for offset, value in ((4, 1), (6, op), (8, 192), (124, len(natives)), (132, len(redos))):
        put(raw, offset, value, 2)
    put(raw, 16, apply)
    put(raw, 24, previous)
    raw[32:88] = owner(aging_ref() if system else actor(), system)
    if before is not None:
        raw[88:120] = digest(before)
    put(raw, 120, 1536, 4)
    put(raw, 136, sum(map(len, natives)), 4)
    put(raw, 152, sum(map(len, redos)), 4)
    raw += after + operation_bytes + b"".join(natives) + b"".join(redos)
    put(raw, 12, len(raw), 4)
    return envelope(raw, kind=3 if system else 2, tag=2, lsn=apply,
                    previous_owner=previous, count=len(redos), transaction=not system)


def group_handoff(op, sender_after, receiver_after, sender_before, receiver_before,
                  operation_bytes, natives, redos):
    begin = op == 4
    raw = bytearray(256)
    raw[:4] = b"XHG1"
    for offset, value in ((4, 1), (6, op), (8, 256), (236, 3), (238, 3)):
        put(raw, offset, value, 2)
    put(raw, 16, 4 if begin else 6)
    put(raw, 24, 0 if begin else 5)
    put(raw, 32, 3 if begin else 4)
    raw[40:96] = owner(actor())
    raw[96:152] = owner(aging_ref(), True)
    if sender_before is not None:
        raw[152:184] = digest(sender_before)
    raw[184:216] = digest(receiver_before)
    for offset, value in ((216, 1536), (220, 1536), (224, 256),
                          (228, sum(map(len, natives))), (240, sum(map(len, redos)))):
        put(raw, offset, value, 4)
    raw += sender_after + receiver_after + operation_bytes + b"".join(natives) + b"".join(redos)
    put(raw, 12, len(raw), 4)
    return envelope(raw, kind=1 if begin else 6, tag=3, lsn=4 if begin else 6,
                    previous_owner=0 if begin else 5, count=3, transaction=True)


def pinned_references(repo, profile):
    base = repo / "tests/fixtures/dev3-plan4-genesis"
    require(sha((base / "manifest.json").read_bytes()) == PLAN4_MANIFEST_SHA, "original manifest drift")
    index = (base / "raw-index.tsv").read_bytes()
    require(sha(index) == PLAN4_INDEX_SHA, "original raw index drift")
    selected = {f"P{i:02}" for i in range(9)} | {f"REF-P{profile}-d3-profile"}
    result, provenance = {}, []
    for line in index.decode().splitlines()[1:]:
        name, relative, size, expected = line.split("\t")
        if name in selected:
            path = base / relative
            value = path.read_bytes()
            require(len(value) == int(size) and sha(value) == expected, f"original input drift: {name}")
            result[name] = value
            provenance.append(dict(id=name, path=str(path.relative_to(repo)), bytes=len(value), sha256=expected))
    require(set(result) == selected, "missing reference")
    return result, provenance


def vector(references, profile, ordinal, peak):
    require(1 <= profile <= 3 and 0 <= ordinal < 64, "closed profile/ordinal")
    selected_page, selected_slot = 2 + ordinal // 15, ordinal % 15
    require(selected_page <= 6 and (selected_page != 6 or selected_slot < 4), "job geometry")
    status_reference = slot_ref(9, 0, 2, 2)
    job_reference = slot_ref(selected_page, selected_slot, 2)
    profile_bytes = references[f"REF-P{profile}-d3-profile"]
    homes = {name: home(state, generation) for name, state, generation in (
        ("free", 0, 1), ("begin", 1, 2), ("rollback", 2, 2), ("abort", 4, 2), ("finish", 0, 2))}
    status_pages = {name: page(9, 9, 2, lsn, [homes[name]], 2) for name, lsn in (
        ("free", 2), ("begin", 4), ("rollback", 5), ("abort", 6), ("finish", 7))}
    tx = {name: descriptor(profile_bytes, peak, system=False, phase=name, status_home=homes[name])
          for name in ("begin", "rollback", "abort")}
    job_values = {name: job(name, status_reference, peak, digest(tx["rollback"]))
                  for name in ("initial", "leased", "ready", "free")}
    cells = {}
    root_cells, root_pages, jobs_pages = {}, {}, {}
    for phase, state in (("initial", "initial"), ("start", "initial"),
                         ("begin", "leased"), ("abort", "ready"), ("finish", "free")):
        cells[phase] = [job_values["initial"]] * 64
        cells[phase][ordinal] = job_values[state]
        root_cells[phase] = root(phase, ordinal, cells[phase], peak)
        lsn = {"initial": 0, "start": 3, "begin": 4, "abort": 6, "finish": 7}[phase]
        root_pages[phase] = page(1, 12, 4, lsn, [root_cells[phase]])
        first = (selected_page - 2) * 15
        count = 4 if selected_page == 6 else 15
        jobs_pages[phase] = page(selected_page, 13, 4, 0 if phase == "start" else lsn,
                                 cells[phase][first:first + count])
    require(root_pages["initial"] == references["P01"], "independent genesis root anchor")
    require(jobs_pages["initial"] == references[f"P{selected_page:02}"], "independent genesis jobs anchor")
    aging = {name: descriptor(profile_bytes, peak, system=True, phase=name, root_page=root_pages[name])
             for name in ("start", "begin", "abort", "finish")}
    def triplet(before_phase, after_phase, before_status):
        natives = [native(8, 0, 0, root_cells[before_phase], root_cells[after_phase]),
                   native(9, 1, selected_slot, cells[before_phase][ordinal], cells[after_phase][ordinal]),
                   native(5, 2, 0, homes[before_status], homes[after_phase])]
        redos = [patch(root_pages[before_phase], root_pages[after_phase]),
                 patch(jobs_pages[before_phase], jobs_pages[after_phase]),
                 patch(status_pages[before_status], status_pages[after_phase])]
        return natives, redos
    start_operation = operation(16, job_reference, status_reference, root_cells["initial"], cells["initial"], peak)
    frames = {"aging-start": group_single(16, 3, 0, aging["start"], None, start_operation,
        [native(8, 0, 0, root_cells["initial"], root_cells["start"])],
        [patch(root_pages["initial"], root_pages["start"])], True)}
    for name, op, before_phase, before_status in (("begin", 4, "start", "free"), ("abort", 3, "begin", "rollback")):
        natives, redos = triplet(before_phase, name, before_status)
        frames[name] = group_handoff(op, tx[name], aging[name], None if name == "begin" else tx["rollback"],
            aging[before_phase], operation(op, job_reference, status_reference, root_cells[before_phase], cells[before_phase], peak),
            natives, redos)
    rollback_op = bytearray(256)
    put(rollback_op, 0, 3, 2)
    put(rollback_op, 4, 256, 4)
    frames["rollback"] = group_single(3, 5, 4, tx["rollback"], tx["begin"], bytes(rollback_op),
        [native(5, 0, 0, homes["begin"], homes["rollback"])],
        [patch(status_pages["begin"], status_pages["rollback"], True)], False)
    natives, redos = triplet("abort", "finish", "abort")
    frames["finish"] = group_single(17, 7, 6, aging["finish"], aging["abort"],
        operation(17, job_reference, status_reference, root_cells["abort"], cells["abort"], peak), natives, redos, True)
    for name, value in frames.items():
        require(len(value) == PRICES[name], f"independent frame price: {name}")
    result = {f"{name}.wal": value for name, value in frames.items()}
    result.update({f"status-{name}.page": value for name, value in status_pages.items()})
    result.update({f"root-{name}.page": root_pages[name] for name in ("start", "begin", "abort", "finish")})
    result.update({f"jobs-{name}.page": jobs_pages[name] for name in ("begin", "abort", "finish")})
    result.update({f"tx-{name}.descriptor": value for name, value in tx.items()})
    result.update({f"aging-{name}.descriptor": value for name, value in aging.items()})
    require(len(result) == RAW_FILES and sum(map(len, result.values())) == RAW_BYTES, "raw inventory arithmetic")
    return result


def create_new(path, raw):
    with path.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def main():
    started = time.monotonic()
    source_before = sha(Path(__file__).read_bytes())
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--approval", type=Path, required=True)
    parser.add_argument("--approval-sha256", required=True)
    parser.add_argument("--profile", type=int, default=1)
    parser.add_argument("--ordinal", type=int, default=0)
    args = parser.parse_args()
    repo = args.repo.resolve(strict=True)
    approval_raw = args.approval.read_bytes()
    require(sha(approval_raw) == args.approval_sha256, "approval input SHA mismatch")
    approval = json.loads(approval_raw)
    require(approval.get("schema") == "XADB_G6_LITERAL_GENERATION_GATE_V1", "approval schema")
    require(approval.get("status") == "APPROVED_FOR_GENERATION", "no generation approval")
    require(approval.get("wire_sha256") == WIRE_SHA, "wire approval drift")
    require(approval.get("generator_sha256") == source_before, "generator approval drift")
    require(approval.get("profile") == args.profile and approval.get("ordinal") == args.ordinal, "vector approval drift")
    require(approval.get("raw_files") == RAW_FILES and approval.get("raw_bytes") == RAW_BYTES, "output inventory approval drift")
    peak = tuple(approval["resource_peak"])
    require(len(peak) == 6 and all(type(value) is int and 0 <= value <= MAX_U64 for value in peak), "explicit ResourcePeak6 required")
    require(approval.get("resource_peak_status") in ("APPROVED_SEMANTIC_INPUT", "F05_ACCEPTED"), "peak input approval missing")
    require(isinstance(approval.get("resource_peak_evidence_sha256"), str)
            and len(approval["resource_peak_evidence_sha256"]) == 64, "peak provenance missing")
    references, provenance = pinned_references(repo, args.profile)
    originals = {row["path"]: row["sha256"] for row in provenance}
    raw = vector(references, args.profile, args.ordinal, peak)
    require(sum(map(len, references.values())) == REFERENCE_BYTES, "reference inventory arithmetic")
    require(time.monotonic() - started <= SECONDS_LIMIT, "generator preparation budget")
    args.output.mkdir(parents=False, exist_ok=False)
    (args.output / "raw").mkdir()
    (args.output / "references").mkdir()
    files = {}
    for directory, values in (("raw", raw), ("references", references)):
        for name, value in sorted(values.items()):
            relative = f"{directory}/{name}" + (".bin" if directory == "references" else "")
            create_new(args.output / relative, value)
            files[relative] = dict(bytes=len(value), sha256=sha(value))
    index = "path\tbytes\tsha256\n" + "".join(
        f"{path}\t{entry['bytes']}\t{entry['sha256']}\n" for path, entry in sorted(files.items()))
    create_new(args.output / "raw-index.tsv", index.encode())
    manifest = dict(schema="XADB_G6_LITERAL_INPUT_V1", status="GENERATED_INPUT_NOT_PRODUCT_EVIDENCE",
        wire_sha256=WIRE_SHA, generator_sha256=source_before,
        approval_sha256=sha(approval_raw), resource_peak=list(peak),
        resource_peak_status=approval["resource_peak_status"], resource_peak_evidence_sha256=approval["resource_peak_evidence_sha256"],
        profile=args.profile, ordinal=args.ordinal, namespace=dict(database=DATABASE.hex(), timeline=1),
        actor=dict(epoch=1, local=1, generation=2), status_location=dict(page=9, incarnation=2, slot=0, free_generation=1),
        frame_prices=PRICES, lifecycle_bytes=191248, separate_allocation_bytes=59200,
        begin_admission=171576, after_begin_future=114864, same_debt_aging_job=55112,
        raw_files=RAW_FILES, raw_bytes=RAW_BYTES, reference_files=len(references), reference_bytes=REFERENCE_BYTES,
        reference_provenance=provenance, files=files, raw_index_sha256=sha(index.encode()),
        exclusions=["No reserve/INIT WAL constructed", "No current/base/recovery authority", "No product execution", "No F05 claim from semantic input approval"])
    create_new(args.output / "manifest.json", (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode())
    for relative, expected in originals.items():
        require(sha((repo / relative).read_bytes()) == expected, "original reference changed during generation")
    base = repo / "tests/fixtures/dev3-plan4-genesis"
    require(sha((base / "manifest.json").read_bytes()) == PLAN4_MANIFEST_SHA, "original manifest changed during generation")
    require(sha((base / "raw-index.tsv").read_bytes()) == PLAN4_INDEX_SHA, "original index changed during generation")
    require(sha(Path(__file__).read_bytes()) == source_before, "generator source changed during generation")
    require(sha(args.approval.read_bytes()) == args.approval_sha256, "approval changed during generation")
    total = sum(path.stat().st_size for path in args.output.rglob("*") if path.is_file())
    require(total <= OUTPUT_LIMIT, "generator output budget")
    manifest_sha = sha((args.output / "manifest.json").read_bytes())
    elapsed = time.monotonic() - started
    require(elapsed <= SECONDS_LIMIT, "generator total budget")
    print(json.dumps(dict(status="INPUT_GENERATED", raw_files=RAW_FILES, raw_bytes=RAW_BYTES,
        reference_files=len(references), reference_bytes=REFERENCE_BYTES, output_bytes=total,
        elapsed_seconds=elapsed, manifest_sha256=manifest_sha), sort_keys=True))


if __name__ == "__main__":
    main()
