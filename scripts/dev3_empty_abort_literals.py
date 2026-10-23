#!/usr/bin/env python3
"""Independent raw Begin -> empty RollbackStart -> AbortEnd literal inputs.

Uses the documented XDP1/XTS1/XSH1/XOD1/XSG1/XWF1 offsets and hashlib.
No product encoder, decoder, test binary or product output is imported. The
default Begin portion must equal the four pinned, previously reviewed literals.
Generation is fixture preparation, not a product test or recovery result.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct


ROOT = Path(__file__).resolve().parents[1]
PAGE_BYTES = 16384
FRAME_BYTES = 3040
HOME_BYTES = 224
MAX_U64 = (1 << 64) - 1
DB = bytes(range(1, 17))
FILE = (2 << 32) | 1
PAGE_NO = 65
INCARNATION = 4
OBJECT = 55
OBJECT_GENERATION = 56
TIMELINE = 17
WRITER_EPOCH = 20
LOCAL = 19
PEAK = (2, 114688, 0, 2, 5, 1)
PROFILE_HASHES = {
    1: "fff74c31eeef815940cbc0e0d6fb42810d6222a657efe95982137b410cddaf31",
    2: "cf628a9cefbd704738e58b29abcec9d9171001f769e4b897142e04ba2882eeff",
    3: "b361d47bcaf7fc771fd8e727af40992d589de7b0c9e0958fe031b64ae46cb920",
}
BEGIN_HASHES = {
    "before.page": "d29af3ff84a47cd5b71523c3bbb67ae55c59033ba26d1ad2aef9ea86bda97a8d",
    "after.page": "e3b8c437fc3987045fcab0366f25dc0fbc4ff3bab4cac5c92b10b61ff92169ad",
    "begin.wal": "0e04edc056e8099290a6207a1dba9c68d4591f109ca0c01091db3d773d824ebd",
    "after.descriptor": "55e5c9cff1bbcce526ec98f306bb4d023186185e6bea98dcfe53e0e6882b544a",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).digest()


def put(data, offset, value, width=8):
    struct.pack_into({2: "<H", 4: "<I", 8: "<Q"}[width], data, offset, value)


def at(data, offset, width=8):
    return struct.unpack_from({2: "<H", 4: "<I", 8: "<Q"}[width], data, offset)[0]


def seal(data):
    data[-32:] = sha(data[:-32])
    return bytes(data)


def checked_input(path, digest):
    data = (ROOT / path).read_bytes()
    require(sha(data).hex() == digest, f"pinned input changed: {path}")
    return data


def inputs():
    profiles = {
        profile: checked_input(
            f"crates/engine/tests/fixtures/dev3-ownership-profile{profile}.bin", digest
        )
        for profile, digest in PROFILE_HASHES.items()
    }
    begin = {
        name: checked_input(f"crates/engine/tests/fixtures/dev3-status-begin/{name}", digest)
        for name, digest in BEGIN_HASHES.items()
    }
    return profiles, begin


def actor_bytes(generation, local=LOCAL, epoch=WRITER_EPOCH):
    out = bytearray(48)
    out[:16] = DB
    put(out, 16, TIMELINE)
    put(out, 24, epoch, 4)
    put(out, 32, local)
    put(out, 40, generation)
    return bytes(out)


def home(state, generation, local=LOCAL, epoch=WRITER_EPOCH):
    out = bytearray(HOME_BYTES)
    out[:4] = b"XSH1"
    put(out, 4, 1, 2)
    put(out, 6, 32, 2)
    out[32:36] = b"XTS1"
    put(out, 36, 1, 2)
    out[38] = state
    if state:
        out[40:88] = actor_bytes(generation, local, epoch)
    else:
        put(out, 80, generation)
    if state == 2:
        out[39] = 2  # full Transaction rollback, no head/target/next
    return bytes(out)


def page(selected, slot, lsn):
    count = max(2, slot + 1)
    out = bytearray(PAGE_BYTES)
    out[:4] = b"XDP1"
    put(out, 4, 1, 2)
    out[6] = 9
    out[8:24] = DB
    for offset, value in [(24, FILE), (32, PAGE_NO), (40, INCARNATION),
                          (48, OBJECT), (56, OBJECT_GENERATION), (64, lsn)]:
        put(out, offset, value)
    put(out, 72, count, 2)
    put(out, 74, 128 + 4 * count, 2)
    put(out, 76, 16352 - HOME_BYTES * count, 2)
    sentinel = 0 if slot != 0 else 1
    for index in range(count):
        start = 16352 - HOME_BYTES * (index + 1)
        put(out, 128 + 4 * index, start, 2)
        put(out, 130 + 4 * index, HOME_BYTES, 2)
        value = (selected if index == slot else
                 home(1, 8, local=18) if index == sentinel else home(0, 7))
        out[start:start + HOME_BYTES] = value
    return seal(out)


def descriptor(profile, actor, cell, phase):
    # phase0 Begin, phase1 RollbackStart, phase2 empty AbortEnd.
    lsns = (41, 43, 53)
    out = bytearray(1536)
    out[:4] = b"XOD1"
    put(out, 4, 1, 2)
    out[6] = (1, 2, 4)[phase]
    put(out, 8, 1536, 4)
    out[16] = 1
    out[24:72] = actor
    put(out, 72, lsns[phase])
    put(out, 80, 0 if phase == 0 else lsns[phase - 1])
    put(out, 88, 41)
    out[96] = 1  # Profile ControlRef: slot0/index0, generation from pinned profile
    put(out, 104, at(profile, 40))
    put(out, 112, 4096, 4)
    out[128:160] = sha(profile)
    out[160:352] = cell[32:]
    put(out, 544, 41)
    out[560:592] = sha(b"")
    put(out, 608, FRAME_BYTES * (phase + 1))
    put(out, 624, phase + 1)
    active_rows = ({2, 3}, {2}, set())[phase]
    for row in range(5):
        base = 640 + 128 * row
        out[base] = row + 1
        if row in active_rows:
            values = (1, FRAME_BYTES, 0, 0, 0, 0, 0, *PEAK, FRAME_BYTES)
            for field, value in enumerate(values):
                put(out, base + 8 + 8 * field, value)
    if active_rows:
        for field, value in enumerate(PEAK):
            put(out, 1280 + 8 * field, value)
    put(out, 1328, len(active_rows) * FRAME_BYTES)
    put(out, 1336, 41)
    return seal(out)


def frame(opcode, outer_kind, lsn, global_previous, owner_previous,
          actor, slot, old_page, new_home, after_descriptor, before_descriptor,
          writer_epoch=WRITER_EPOCH):
    cell_at = 16352 - HOME_BYTES * (slot + 1)
    old_home = old_page[cell_at:cell_at + HOME_BYTES]
    group = bytearray(2848)
    group[:4] = b"XSG1"
    for offset, value in [(4, 1), (6, opcode), (8, 192), (124, 1), (132, 1)]:
        put(group, offset, value, 2)
    for offset, value in [(12, 2848), (120, 1536), (136, 576), (152, 288)]:
        put(group, offset, value, 4)
    put(group, 16, lsn)
    put(group, 24, owner_previous)
    group[32] = 1
    group[40:88] = actor
    if before_descriptor is not None:
        group[88:120] = sha(before_descriptor)
    group[192:1728] = after_descriptor
    put(group, 1728, opcode, 2)
    put(group, 1732, 256, 4)
    native = bytearray(576)
    native[:2] = bytes([5, 3])
    for offset, value in [(4, slot), (6, slot), (8, cell_at), (10, cell_at)]:
        put(native, offset, value, 2)
    for offset, value in [(12, HOME_BYTES), (16, HOME_BYTES), (20, 576)]:
        put(native, offset, value, 4)
    native[24:56] = sha(old_home)
    native[56:88] = sha(new_home)
    native[128:352] = old_home
    native[352:576] = new_home
    group[1984:2560] = native
    redo = bytearray(288)
    redo[:2] = bytes([1, 9])
    put(redo, 4, 288, 4)
    for offset, value in [(8, FILE), (16, PAGE_NO), (24, INCARNATION),
                          (32, OBJECT), (40, OBJECT_GENERATION), (48, at(old_page, 64))]:
        put(redo, offset, value)
    put(redo, 56, cell_at, 2)
    put(redo, 58, HOME_BYTES, 2)
    redo[64:] = new_home
    group[2560:] = redo
    out = bytearray(FRAME_BYTES)
    out[:4] = b"XWF1"
    put(out, 4, 1, 2)
    out[6:8] = bytes([outer_kind, 2])
    put(out, 8, 160, 2)
    put(out, 12, FRAME_BYTES, 4)
    put(out, 16, 2848, 4)
    put(out, 20, 1, 2)
    out[24:40] = DB
    put(out, 40, TIMELINE)
    put(out, 48, writer_epoch, 4)
    put(out, 56, lsn)
    put(out, 64, global_previous)
    out[72:112] = actor[:40]
    put(out, 112, owner_previous)
    out[128:160] = sha(out[:128])
    out[160:3008] = group
    return seal(out)


def make_vector(profile=1, slot=1, free_generation=19):
    """Return one bounded raw vector, suitable for sequential639-case S5 use.

    The generation argument is the *pre-Begin Free* generation. Occupied
    generation is checked +1, including MAX-1 -> MAX. No all-vector cache.
    """
    require(profile in PROFILE_HASHES, "profile must be1..3")
    require(0 <= slot <= 70, "slot must be0..70")
    require(1 <= free_generation < MAX_U64, "Free generation must permit +1")
    profiles, pinned_begin = inputs()
    generation = free_generation + 1
    actor = actor_bytes(generation)
    cells = [home(0, free_generation), home(1, generation),
             home(2, generation), home(4, generation)]
    pages = [page(cell, slot, lsn) for cell, lsn in zip(cells, (35, 41, 43, 53))]
    descriptors = [descriptor(profiles[profile], actor, cells[phase + 1], phase)
                   for phase in range(3)]
    frames = [
        frame(1, 1, 41, 37, 0, actor, slot, pages[0], cells[1], descriptors[0], None),
        frame(3, 2, 43, 42, 41, actor, slot, pages[1], cells[2], descriptors[1], descriptors[0]),
        frame(7, 6, 53, 47, 43, actor, slot, pages[2], cells[3], descriptors[2], descriptors[1]),
    ]
    files = {
        "begin.before.page": pages[0], "begin.after.page": pages[1],
        "begin.wal": frames[0], "begin.descriptor": descriptors[0],
        "rollback.wal": frames[1], "before.page": pages[2],
        "before.descriptor": descriptors[1], "abort.wal": frames[2],
        "after.page": pages[3], "after.descriptor": descriptors[2],
    }
    if (profile, slot, free_generation) == (1, 1, 19):
        for generated, old in [("begin.before.page", "before.page"),
                               ("begin.after.page", "after.page"),
                               ("begin.wal", "begin.wal"),
                               ("begin.descriptor", "after.descriptor")]:
            require(files[generated] == pinned_begin[old], f"Begin anchor mismatch: {old}")
    require(sum(map(len, files.values())) <= 1 << 20, "active fixture exceeds1MiB")
    metadata = {
        "profile": profile, "slot": slot, "free_generation": free_generation,
        "actor_generation": generation, "actor_local": LOCAL, "actor_epoch": WRITER_EPOCH,
        "namespace_timeline": TIMELINE, "writer_epoch": WRITER_EPOCH,
        "page_count": max(2, slot + 1), "cell_offset": 16352 - HOME_BYTES * (slot + 1),
        "page_ref": [FILE, PAGE_NO, INCARNATION], "object": [OBJECT, OBJECT_GENERATION],
        "page_lsn": [35, 41, 43, 53], "global_previous_lsn": [37, 42, 47],
        "owner_previous_lsn": [0, 41, 43], "consumed_wal": [3040, 6080, 9120],
        "remaining_wal": [6080, 3040, 0], "profile_ref_hex": descriptors[0][96:160].hex(),
        "raw_bytes": sum(map(len, files.values())),
    }
    return files, metadata


def write_selected(destination):
    require(not destination.exists(), "refuse to overwrite an existing destination")
    destination.mkdir(parents=True)
    variants = []
    for subdir, profile, slot, generation in [
        ("", 1, 1, 19), ("p2-slot0-free1", 2, 0, 1),
        ("p3-slot70-free18446744073709551614", 3, 70, MAX_U64 - 1),
    ]:
        files, metadata = make_vector(profile, slot, generation)
        directory = destination / subdir
        directory.mkdir(exist_ok=True)
        records = {}
        for name, data in files.items():
            with (directory / name).open("xb") as output:
                output.write(data)
            records[name] = {"bytes": len(data), "sha256": sha(data).hex()}
        variants.append({"directory": subdir or ".", **metadata, "files": records})
    manifest = {
        "status": "RAW_FIXTURES_AUTHORED_PRODUCT_NOT_RUN",
        "contract": "devdoc/contracts/dev3-obligation-format.md#empty-abort-preparation",
        "generator": "scripts/dev3_empty_abort_literals.py",
        "generator_sha256": sha(Path(__file__).read_bytes()).hex(),
        "method": "Documented raw offsets + hashlib/struct; no product imports/encoder/decoder.",
        "input_profiles_sha256": PROFILE_HASHES, "input_begin_sha256": BEGIN_HASHES,
        "default_begin_anchor": "four generated Begin outputs equal pinned prior literals",
        "variants": variants,
        "s5": {"status": "AUTHORED_NOT_RUN", "profiles": [1, 2, 3],
               "slots": [0, 70], "slot_range_inclusive": True,
               "free_generations": [1, 19, MAX_U64 - 1],
               "actor_generations": [2, 20, MAX_U64], "combinations": 639,
               "api": "make_vector(profile, slot, free_generation); generate/release one at a time"},
        "limits": {"active_fixture_bytes": 1 << 20, "retained_case_bytes": 8 << 20},
        "warning": "The three pure groups have deliberate global-LSN gaps; this is no native WAL/recovery prefix or live owner proof.",
    }
    encoded = json.dumps(manifest, indent=2) + "\n"
    require(sum(v["raw_bytes"] for v in variants) + len(encoded.encode()) <= 8 << 20,
            "retained fixture cap")
    with (destination / "vectors.json").open("x") as output:
        output.write(encoded)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    result = write_selected(args.out)
    print(json.dumps({"status": result["status"], "output": str(args.out),
                      "raw_bytes": sum(v["raw_bytes"] for v in result["variants"]),
                      "variants": len(result["variants"])}))
