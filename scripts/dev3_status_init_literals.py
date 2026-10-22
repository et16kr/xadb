#!/usr/bin/env python3
"""Independent literals for the adopted closed Status INIT contract.

Only pinned genesis inputs and explicit field placement are used. Generation
is not product verification. Refuses to overwrite an existing destination.
The original unexecuted draft remains preserved under artifacts.
"""

import argparse
import hashlib
import json
from pathlib import Path
import struct


STATUS = "ADOPTED_INPUTS_PRODUCT_NOT_RUN"
BASE = Path("crates/engine/tests/fixtures/dev3-storage-session/literal")
PAGE = 16_384
CELL_AT = 10_144
CELL_LEN = 6_208
HOME_AT = 16_128
HOME_LEN = 224
DESCRIPTOR = 1_536
RESERVE_LEN = 21_200
INIT_LEN = 38_000
MEMORY = 114_688
PROTECTED = 70_768
DB = bytes(range(1, 17))
FILE = 0x100000001
INPUTS = {
    "profile1-d3-profile.bin": "c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4",
    "profile2-d3-profile.bin": "e45db9f3b6d87837bc2d626f641034c7b647b711fe8c761990731eb5db16052f",
    "profile3-d3-profile.bin": "e86fdb05e8a470f64fa351b9626cd667b45824d40ef0bbe376f9c1bfcc4765d0",
    "profile1-allocation.bin": "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
    "profile2-allocation.bin": "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
    "profile3-allocation.bin": "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
}
INIT_SECTIONS = (
    ("physical_header", 0, 160), ("group_header", 160, 352),
    ("descriptor", 352, 1888), ("core", 1888, 2144),
    ("allocator_native", 2144, 14688), ("status_native", 14688, 15040),
    ("consume", 15040, 15248), ("allocator_patch", 15248, 21520),
    ("status_init", 21520, 37968), ("frame_sha256", 37968, 38000),
)


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def sha(value):
    return hashlib.sha256(value).digest()


def u(value, width=8):
    return struct.pack({2: "<H", 4: "<I", 8: "<Q"}[width], value)


def at(data, offset, width=8):
    return struct.unpack_from({2: "<H", 4: "<I", 8: "<Q"}[width], data, offset)[0]


def record(size, fields):
    """Explicit nonoverlapping fields; all unmentioned reserved bytes are zero."""
    out, occupied = bytearray(size), bytearray(size)
    for offset, value in fields:
        end = offset + len(value)
        require(0 <= offset <= end <= size, "field outside record")
        require(not any(occupied[offset:end]), "overlapping field")
        out[offset:end] = value
        occupied[offset:end] = bytes([1]) * len(value)
    return out


def sealed_page(data):
    require(len(data) == PAGE, "page size")
    data[-32:] = sha(data[:-32])
    return bytes(data)


def work_owner(work):
    require(1 <= work < 2**64, "work ID")
    return record(48, [(0, DB), (16, u(1)), (24, bytes([1])),
                       (32, u(work)), (40, u(1))])


def token(incarnation, birth):
    return bytes(record(96, [(0, bytes([1, 3])),
                             (8, u(FILE) + u(1) + u(incarnation)),
                             (64, u(1)), (72, u(birth))]))


def allocation_after(before, work, lsn, operation):
    require(len(before) == PAGE and before[-32:] == sha(before[:-32]), "allocator before")
    entry_at = CELL_AT + 64 + 96  # page1; page0 is the allocator itself.
    generation = at(before, entry_at + 8)
    if operation == "reserve":
        require(before[entry_at] == 0 and generation < 2**64 - 1, "Free(g) required")
        generation += 1
        entry = record(96, [(0, bytes([1, 0, 2])), (8, u(generation)),
                            (32, work_owner(work))])
    else:
        require(before[entry_at] == 1, "Reserved(g) required")
        require(before[entry_at + 32:entry_at + 80] == work_owner(work), "reserved claimant")
        if operation == "cancel":
            entry = record(96, [(8, u(generation))])
        elif operation == "init":
            entry = record(96, [(0, bytes([2, 9, 0])), (8, u(generation)),
                                (16, u(2)), (24, u(1))])
        else:
            raise ValueError("operation")
    out = bytearray(before)
    out[entry_at:entry_at + 96] = entry
    out[64:72] = u(lsn)
    return sealed_page(out), generation


def free_status_home():
    core = record(192, [(0, b"XTS1" + u(1, 2)), (48, u(1))])
    return bytes(record(HOME_LEN, [(0, b"XSH1" + u(1, 2) + u(32, 2)), (32, core)]))


def status_page(incarnation, lsn):
    # The closed caller payload is NOT an input: after-only INIT creates all bytes.
    return sealed_page(record(PAGE, [
        (0, b"XDP1" + u(1, 2) + bytes([9, 0])), (8, DB),
        (24, u(FILE) + u(1) + u(incarnation) + u(2) + u(1) + u(lsn)),
        (72, u(1, 2) + u(132, 2) + u(HOME_AT, 2)),
        (128, u(HOME_AT, 2) + u(HOME_LEN, 2)), (HOME_AT, free_status_home()),
    ]))


def descriptor(profile, work, lsn, birth, operation, item):
    finished = operation != "reserve"
    actual = INIT_LEN if operation == "init" else RESERVE_LEN
    reference = record(64, [(0, bytes([1])), (8, u(1)),
                            (16, u(4096, 4)), (32, sha(profile))])
    fields = [
        (0, b"XOD1" + u(1, 2) + bytes([6 if finished else 5])),
        (8, u(DESCRIPTOR, 4)), (16, bytes([2])), (24, work_owner(work)),
        (72, u(lsn) + u(birth if finished else 0) + u(birth)),
        (96, reference), (352, bytes([1, 4 if finished else 1])),
        (360, u(1) + u(1 if finished else 0) + u(1)), (448, sha(b"")),
        (544, u(lsn) + u(0 if finished else 1)),
        (560, sha(b"" if finished else item)),
        (608, u(RESERVE_LEN + actual if finished else RESERVE_LEN)),
        (624, u(2 if finished else 1)),
    ]
    fields.extend((640 + 128 * category, bytes([category + 1])) for category in range(5))
    if not finished:
        fields.extend([
            (904, u(1) + u(INIT_LEN)), (936, u(1)),
            (960, u(2) + u(MEMORY)), (992, u(1)), (1008, u(PROTECTED)),
            (1280, u(2) + u(MEMORY)), (1312, u(1)),
            (1328, u(PROTECTED) + u(birth)),
        ])
    out = record(DESCRIPTOR, fields)
    out[1504:] = sha(out[:1504])
    return bytes(out)


def allocation_native(before, after):
    old, new = before[CELL_AT:-32], after[CELL_AT:-32]
    return record(128, [
        (0, bytes([6, 3])), (8, u(CELL_AT, 2) * 2),
        (12, u(CELL_LEN, 4) * 2 + u(12_544, 4)), (24, sha(old) + sha(new)),
    ]) + old + new


def status_native(after):
    home = after[HOME_AT:-32]
    return record(128, [
        (0, bytes([5, 2]) + u(1, 2) + u(65535, 2) + u(0, 2)),
        (10, u(HOME_AT, 2)), (16, u(HOME_LEN, 4) + u(352, 4)),
        (56, sha(home)),
    ]) + home


def delta(item, operation):
    action = {"reserve": 1, "init": 2, "cancel": 3}[operation]
    return bytes(record(208, [(0, bytes([action, 2 if action == 1 else 1])),
                              (112 if action == 1 else 16, item)]))


def patch(before, after):
    return record(64, [
        (0, bytes([1, 10])), (4, u(6272, 4)), (8, before[24:64]),
        (48, before[64:72]), (56, u(CELL_AT, 2) + u(CELL_LEN, 2)),
    ]) + after[CELL_AT:-32]


def init_redo(after):
    return record(64, [
        (0, bytes([2, 9])), (4, u(16_448, 4)), (8, after[24:64]),
        (58, u(PAGE, 2)),
    ]) + after


def wal(before, after, page, old_descriptor, new_descriptor, work, lsn, birth, item, operation):
    is_init, terminal = operation == "init", operation != "reserve"
    length, opcode = (INIT_LEN if is_init else RESERVE_LEN), (10 if terminal else 8)
    natives = allocation_native(before, after)
    redos = patch(before, after)
    if is_init:
        natives += status_native(page)
        redos += init_redo(page)
    prefix = record(160, [
        (0, b"XWF1" + u(1, 2) + bytes([3, 2])), (8, u(160, 2)),
        (12, u(length, 4) + u(length - 192, 4) + u(2 if is_init else 1, 2)),
        (24, DB + u(1) + u(1, 4)), (56, u(lsn) + u(lsn - 1)),
    ])
    prefix[128:] = sha(prefix[:128])
    group = record(192, [
        (0, b"XSG1" + u(1, 2) + u(opcode, 2) + u(192, 2)),
        (12, u(length - 192, 4)), (16, u(lsn) + u(birth if terminal else 0)),
        (32, bytes([2])), (40, work_owner(work)),
        (88, sha(old_descriptor) if terminal else bytes(32)),
        (120, u(DESCRIPTOR, 4) + u(2 if is_init else 1, 2)),
        (130, u(1, 2) + u(2 if is_init else 1, 2)),
        (136, u(len(natives), 4)), (148, u(208, 4) + u(len(redos), 4)),
    ])
    core = record(256, [(0, u(opcode, 2)), (4, u(256, 4)),
                        (168, u(1 if terminal else 0))])
    material = prefix + group + new_descriptor + core + natives + delta(item, operation) + redos
    require(len(material) == length - 32, "candidate section sum")
    return bytes(material + sha(material))


def candidate_history(profile, genesis, cancellations, final_terminal):
    """Bounded cancellation histories; each new work gets one PageToken."""
    require(cancellations in (0, 1, 2, 8), "frozen history bound")
    require(final_terminal in ("init", "cancel"), "final terminal")
    out, rows, before = {}, [], genesis
    for cycle in range(cancellations + 1):
        work, birth = cycle + 1, 2 * cycle + 1
        reserved, incarnation = allocation_after(before, work, birth, "reserve")
        item = token(incarnation, birth)
        reserve_descriptor = descriptor(profile, work, birth, birth, "reserve", item)
        reserve = wal(before, reserved, None, None, reserve_descriptor,
                      work, birth, birth, item, "reserve")
        terminal = "cancel" if cycle < cancellations else final_terminal
        lsn = birth + 1
        after, actual_incarnation = allocation_after(reserved, work, lsn, terminal)
        require(actual_incarnation == incarnation, "terminal preserves incarnation")
        page = status_page(incarnation, lsn) if terminal == "init" else None
        final_descriptor = descriptor(profile, work, lsn, birth, terminal, item)
        ending = wal(reserved, after, page, reserve_descriptor, final_descriptor,
                     work, lsn, birth, item, terminal)
        stem = f"work{work}"
        for suffix, value in {
            "allocator.before": before, "allocator.reserved": reserved,
            "allocator.after": after, "token.bin": item,
            "reserve.descriptor": reserve_descriptor, f"{terminal}.descriptor": final_descriptor,
            "reserve.wal": reserve, f"{terminal}.wal": ending,
            f"{terminal}.delta": delta(item, terminal),
        }.items():
            out[f"{stem}-{suffix}"] = value
        if page is not None:
            out[f"{stem}-status.after"] = page
            out[f"{stem}-status.home"] = page[HOME_AT:-32]
        rows.append({"work": work, "birth_lsn": birth, "terminal_lsn": lsn,
                     "page_incarnation": incarnation, "slot_generation": 1,
                     "terminal": terminal, "terminal_bytes": len(ending),
                     "consumed_wal": RESERVE_LEN + len(ending)})
        before = after
    # This excludes the existing physical epoch header and is named accordingly.
    out["wal.frames-only"] = b"".join(
        out[f"work{row['work']}-{op}.wal"] for row in rows
        for op in ("reserve", row["terminal"])
    )
    return out, rows


def load_inputs(root):
    inputs, receipts = {}, []
    for name, expected in INPUTS.items():
        relative = BASE / name
        data = (root / relative).read_bytes()
        require(sha(data).hex() == expected, f"pinned genesis identity: {relative}")
        inputs[name] = data
        receipts.append({"path": str(relative), "bytes": len(data), "sha256": expected})
    return inputs, receipts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root, output = args.root.resolve(), args.output.resolve()
    require(root in output.parents and not output.exists(), "new repository fixture directory only")
    inputs, provenance = load_inputs(root)
    outputs, scenarios = {}, []
    for profile_id in (1, 2, 3):
        for cancellations in (0, 1, 2, 8):
            for terminal in ("init", "cancel"):
                values, history = candidate_history(
                    inputs[f"profile{profile_id}-d3-profile.bin"],
                    inputs[f"profile{profile_id}-allocation.bin"], cancellations, terminal)
                prefix = f"p{profile_id}-cancel{cancellations}-{terminal}/"
                outputs.update((prefix + name, data) for name, data in values.items())
                scenarios.append({"profile": profile_id, "cancellations": cancellations,
                                  "terminal": terminal, "history": history,
                                  "selection_status": "INPUT_ONLY_NOT_RUN"})
    manifest = {
        "status": STATUS, "canonical_fixture": True, "product_execution": False,
        "checker_or_selftest_execution": False,
        "generation_is_not_validation": True, "generator_sha256": sha(Path(__file__).read_bytes()).hex(),
        "input_provenance": provenance, "init_sections": INIT_SECTIONS,
        "scenarios": scenarios,
        "literals": [{"path": name, "bytes": len(data), "sha256": sha(data).hex()}
                     for name, data in sorted(outputs.items())],
    }
    output.mkdir()
    for name, data in outputs.items():
        destination = output / name
        destination.parent.mkdir(exist_ok=True)
        destination.write_bytes(data)
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"status": STATUS, "output": str(output),
                      "literal_count": len(outputs), "product_pass": False}))


if __name__ == "__main__":
    main()
