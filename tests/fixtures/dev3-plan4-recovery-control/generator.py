#!/usr/bin/env python3
"""Independent XRC1/XWE2/XWS2 literals. SOURCE ONLY until an exact external gate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import time

WIRE_SHA = "0561d2c1fc40daeacf65d530b245932a8daf4b213e9476ca1df72a48023e3ae2"
MANIFEST_SHA = "82880ea05480b9779bb2d5764c88f6423d2c74da1d4e68e8bf2bb0092e8eba22"
INDEX_SHA = "0c057920532b389a678c4fc1ea74e82d86a4d457dbd90b8d88ff16cfedd09e13"
BYTES = 4096
RAW_COUNT = 13
REFERENCE_COUNT = 4
PAYLOAD_BYTES = 69632
OUTPUT_LIMIT = 1 << 20
SECONDS = 10


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).digest()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def u32(out, at, value):
    struct.pack_into("<I", out, at, value)


def u64(out, at, value):
    struct.pack_into("<Q", out, at, value)


def finish(out):
    out[4064:] = digest(out[:4064])
    return bytes(out)


def tagged(label):
    # Opaque local values, never a claim that an XRB1 or prefix exists on disk.
    return digest(("XADB-G6-control-" + label).encode())


def prefix(magic, profile):
    out = bytearray(BYTES)
    out[:8] = magic + bytes([1, 0, 1, 0])
    u32(out, 8, BYTES)
    u32(out, 12, 256)
    out[16:32] = profile[16:32]
    out[32:40] = profile[32:40]
    return out


def bind_profile(out, at, profile):
    out[at:at + 8] = profile[40:48]
    out[at + 8:at + 40] = digest(profile)


def epoch(profile, number):
    out = prefix(b"XWE2", profile)
    u32(out, 40, number)
    bind_profile(out, 48, profile)
    for at, value in ((88, 7), (96, 4096), (112, 268435456), (120, 2), (168, 7)):
        u64(out, at, value)
    u32(out, 104, 1048576)
    out[128:160] = tagged("base-generation-2-lsn-7")
    u32(out, 160, 1)
    out[176:208] = tagged("previous-source-header")
    u32(out, 208, 1)
    return finish(out)


def writer(profile, number, epoch_raw):
    out = prefix(b"XWS2", profile)
    u64(out, 40, number)
    u32(out, 48, number)
    u32(out, 52, 1)
    bind_profile(out, 56, profile)
    out[96:128] = digest(epoch_raw)
    u64(out, 128, 2)
    u64(out, 136, 7)
    out[144:176] = tagged("base-generation-2-lsn-7")
    u64(out, 176, 4096)
    u64(out, 184, 6 if number == 2 else 9)
    return finish(out)


def closure(*, slot=0, generation=2, lsn=7, anchor=2, mode=0, epoch_no=0,
            anchor_digest=None, epoch_digest=bytes(32), sealed_offset=0,
            sealed_lsn=0, sealed_digest=bytes(32), writer_digest=bytes(32), writer_generation=0):
    out = bytearray(256)
    out[:4] = bytes([anchor, mode, slot, 1])
    u32(out, 4, epoch_no)
    u64(out, 8, generation)
    u64(out, 16, lsn)
    u32(out, 24, 4096 if anchor == 1 else 1048576)
    out[32:64] = tagged(f"base-generation-{generation}-lsn-{lsn}") if anchor_digest is None else anchor_digest
    out[64:96] = epoch_digest
    u64(out, 96, 0 if mode == 0 else 4096)
    u64(out, 104, sealed_offset)
    u64(out, 112, sealed_lsn)
    out[120:152] = sealed_digest
    out[152:184] = writer_digest
    u64(out, 184, writer_generation)
    return bytes(out)


def current(profile, generation, issued, retired, writer_high, base_high, a, b, intent=None):
    out = prefix(b"XRC1", profile)
    u64(out, 40, generation)
    bind_profile(out, 48, profile)
    u32(out, 88, issued)
    u32(out, 100, retired)
    u64(out, 224, writer_high)
    u64(out, 232, base_high)
    if intent is not None:
        number, active, epoch_raw, writer_raw = intent
        out[92] = 1
        u32(out, 96, number)
        u64(out, 104, 2)
        u64(out, 112, 7)
        u64(out, 120, active)
        out[128:160] = tagged("base-generation-2-lsn-7")
        out[160:192] = digest(epoch_raw)
        out[192:224] = digest(writer_raw)
    out[256:512], out[512:768] = a, b
    return finish(out)


def literals(refs):
    profile = refs["PROFILE.bin"]
    e2, e3 = epoch(profile, 2), epoch(profile, 3)
    w2, w3 = writer(profile, 2, e2), writer(profile, 3, e3)
    a, b = closure(slot=0), closure(slot=1)
    open_args = dict(mode=1, epoch_no=2, epoch_digest=digest(e2), writer_digest=digest(w2), writer_generation=2)
    sealed_args = dict(open_args, mode=2, sealed_offset=65536, sealed_lsn=12, sealed_digest=tagged("sealed-prefix-12"))
    sealed = closure(slot=1, **sealed_args)
    empty = closure(slot=1, **dict(sealed_args, sealed_offset=4096, sealed_lsn=7, sealed_digest=digest(b"")))
    genesis = closure(anchor=1, generation=1, lsn=0, mode=2, epoch_no=1,
        anchor_digest=digest(refs["BOOTSTRAP.bin"]), epoch_digest=digest(refs["WAL1.bin"]),
        sealed_offset=254544, sealed_lsn=7, sealed_digest=tagged("genesis-complete-prefix-7"),
        writer_digest=digest(refs["WRITER1.bin"]), writer_generation=1)
    return {
        "current-base-pair.bin": current(profile, 4, 1, 1, 1, 2, b, a),
        "current-base-same.bin": current(profile, 4, 1, 1, 1, 2, a, a),
        "current-genesis-fallback.bin": current(profile, 1, 1, 0, 1, 1, closure(slot=1, generation=1), genesis),
        "current-base-fallback.bin": current(profile, 8, 2, 1, 2, 3, closure(slot=0, generation=3, lsn=12), sealed),
        "current-intent.bin": current(profile, 5, 2, 1, 2, 2, b, a, (2, 6, e2, w2)),
        "current-burned.bin": current(profile, 7, 2, 1, 2, 2, b, a),
        "current-open.bin": current(profile, 6, 2, 1, 2, 2, closure(slot=1, **open_args), closure(slot=0, **open_args)),
        "current-sealed.bin": current(profile, 7, 2, 1, 2, 2, sealed, sealed),
        "current-sealed-empty.bin": current(profile, 7, 2, 1, 2, 2, empty, empty),
        "epoch2.bin": e2, "epoch3.bin": e3, "writer2.bin": w2, "writer3.bin": w3,
    }


def references(repo, profile):
    base = repo / "tests/fixtures/dev3-plan4-genesis"
    manifest = (base / "manifest.json").read_bytes()
    index = (base / "raw-index.tsv").read_bytes()
    require(sha(manifest) == MANIFEST_SHA and sha(index) == INDEX_SHA, "original input identity changed")
    wanted = {f"REF-P{profile}-d3-profile": "PROFILE.bin", f"XBS-P{profile}": "BOOTSTRAP.bin",
              f"REF-P{profile}-d3-wal-00000001": "WAL1.bin", f"REF-P{profile}-WRITER": "WRITER1.bin"}
    refs, provenance = {}, []
    for line in index.decode().splitlines()[1:]:
        name, relative, length, expected = line.split("\t")
        if name in wanted:
            path = base / relative
            raw = path.read_bytes()
            require(len(raw) == int(length) == BYTES and sha(raw) == expected, "original reference mismatch")
            refs[wanted[name]] = raw
            provenance.append(dict(path=str(path.relative_to(repo)), bytes=len(raw), sha256=expected))
    require(len(refs) == REFERENCE_COUNT, "missing exact original reference")
    return refs, provenance


def create_new(path, raw):
    with path.open("xb") as file:
        file.write(raw)
        file.flush()
        os.fsync(file.fileno())


def fsync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def main():
    started = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--profile", type=int, choices=(1, 2, 3), default=1)
    parser.add_argument("--approval", type=Path, required=True)
    parser.add_argument("--approval-sha256", required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve()
    source_sha = sha(source.read_bytes())
    input_path = source.parent / "input-plan.json"
    input_sha = sha(input_path.read_bytes())
    approval_raw = args.approval.read_bytes()
    require(sha(approval_raw) == args.approval_sha256, "approval identity mismatch")
    gate = json.loads(approval_raw)
    require(gate.get("schema") == "XADB_G6_CONTROL_LITERAL_GATE_V1" and gate.get("status") == "APPROVED_FOR_GENERATION", "no exact generation gate")
    require(gate.get("generator_sha256") == source_sha and gate.get("wire_sha256") == WIRE_SHA, "source/wire gate mismatch")
    require(gate.get("input_plan_sha256") == input_sha, "exact input plan gate mismatch")
    require(gate.get("profile") == args.profile and gate.get("raw_files") == RAW_COUNT and gate.get("payload_bytes") == PAYLOAD_BYTES, "inventory gate mismatch")
    repo = args.repo.resolve(strict=True)
    refs, provenance = references(repo, args.profile)
    raw = literals(refs)
    require(len(raw) == RAW_COUNT and all(len(value) == BYTES for value in raw.values()), "literal arithmetic")
    require(time.monotonic() - started <= SECONDS, "preparation time limit")
    args.output.mkdir(parents=False, exist_ok=False)
    for name in ("raw", "references"):
        (args.output / name).mkdir()
    files = {}
    for directory, items in (("raw", raw), ("references", refs)):
        for name, value in sorted(items.items()):
            relative = f"{directory}/{name}"
            create_new(args.output / relative, value)
            files[relative] = dict(bytes=len(value), sha256=sha(value))
        fsync_directory(args.output / directory)
    index = "path\tbytes\tsha256\n" + "".join(f"{path}\t{value['bytes']}\t{value['sha256']}\n" for path, value in sorted(files.items()))
    create_new(args.output / "raw-index.tsv", index.encode())
    manifest = dict(schema="XADB_G6_CONTROL_LITERAL_V1", status="GENERATED_INPUT_NOT_PRODUCT_EVIDENCE", profile=args.profile,
        generator_sha256=source_sha, input_plan_sha256=input_sha, wire_sha256=WIRE_SHA, approval_sha256=args.approval_sha256,
        raw_files=RAW_COUNT, reference_files=REFERENCE_COUNT, payload_bytes=PAYLOAD_BYTES,
        raw_index_sha256=sha(index.encode()), files=files, provenance=provenance,
        limitation="Opaque base/prefix digest tags are local byte inputs, not an actual base, WAL prefix, selected epoch, burn or deletion proof")
    create_new(args.output / "manifest.json", (json.dumps(manifest, indent=2, sort_keys=True)+"\n").encode())
    fsync_directory(args.output)
    for item in provenance:
        require(sha((repo / item['path']).read_bytes()) == item['sha256'], "original changed")
    require(sha(source.read_bytes()) == source_sha and sha(args.approval.read_bytes()) == args.approval_sha256, "gate/source changed")
    require(sha(input_path.read_bytes()) == input_sha, "input plan changed")
    require(sha((repo / "tests/fixtures/dev3-plan4-genesis/manifest.json").read_bytes()) == MANIFEST_SHA, "original manifest changed")
    require(sha((repo / "tests/fixtures/dev3-plan4-genesis/raw-index.tsv").read_bytes()) == INDEX_SHA, "original index changed")
    output_bytes = sum(p.stat().st_size for p in args.output.rglob("*") if p.is_file())
    require(output_bytes <= OUTPUT_LIMIT and time.monotonic()-started <= SECONDS, "total generation budget")
    print(json.dumps(dict(status="INPUT_GENERATED", manifest_sha256=sha((args.output/"manifest.json").read_bytes()), output_bytes=output_bytes, elapsed_seconds=time.monotonic()-started)))


if __name__ == "__main__":
    main()
