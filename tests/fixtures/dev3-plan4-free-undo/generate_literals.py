#!/usr/bin/env python3
"""Explicit-field plan4 Free Undo inputs; never runs or imports product code.

All outputs are create-new. Original pinned references remain read-only and are
indexed by repository path instead of being copied. A generated base is a pure
byte/claim input, not a selected-source or native-recovery authority.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
import time

sys.dont_write_bytecode = True
PAGE, BASE, CELL = 16384, 1048576, 10144
DB, FILE = bytes(range(1, 17)), 0x100000001
SECTIONS = ((4096, 55296, 36, 1536), (65536, PAGE, 1, PAGE),
            (81920, 6 * PAGE, 6, PAGE), (180224, 64 * 352, 64, 352),
            (202752, 64 * 96, 64, 96), (208896, PAGE, 1, PAGE))


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).digest()


def sha(raw):
    return digest(raw).hex()


def put(raw, at, value, width=8):
    struct.pack_into({2: '<H', 4: '<I', 8: '<Q'}[width], raw, at, value)


def get(raw, at, width=8):
    return struct.unpack_from({2: '<H', 4: '<I', 8: '<Q'}[width], raw, at)[0]


def seal(raw):
    raw[-32:] = digest(raw[:-32])
    return bytes(raw)


def work(number):
    return DB + struct.pack('<QB7xQQ', 1, 1, number, 1)


def reference(number, incarnation):
    return struct.pack('<QQQ', FILE, number, incarnation)


def free_cell(kind):
    raw = bytearray(432 if kind == 'undo' else 224)
    if kind == 'undo':
        raw[:4] = b'XUR1'
        put(raw, 4, 1, 2)
        put(raw, 8, 1)
    else:
        raw[:4] = b'XSH1'
        put(raw, 4, 1, 2)
        put(raw, 6, 32, 2)
        raw[32:36] = b'XTS1'
        put(raw, 36, 1, 2)
        put(raw, 80, 1)
    return bytes(raw)


def payload(number, incarnation, lsn, kind):
    body = free_cell(kind)
    raw = bytearray(PAGE)
    raw[:4] = b'XDP1'
    put(raw, 4, 1, 2)
    raw[6] = 8 if kind == 'undo' else 9
    raw[8:24] = DB
    for at, value in ((24, FILE), (32, number), (40, incarnation),
                      (48, 3 if kind == 'undo' else 2), (56, 1), (64, lsn)):
        put(raw, at, value)
    start = PAGE - 32 - len(body)
    for at, value in ((72, 1), (74, 132), (76, start), (128, start), (130, len(body))):
        put(raw, at, value, 2)
    raw[start:PAGE - 32] = body
    return seal(raw)


def token(number, incarnation, birth):
    raw = bytearray(96)
    raw[:2] = bytes([1, 3])
    raw[8:32] = reference(number, incarnation)
    put(raw, 64, 1)
    put(raw, 72, birth)
    return bytes(raw)


def allocator(before, number, incarnation, work_id, lsn, terminal):
    raw = bytearray(before)
    at = CELL + 64 + number * 96
    raw[at:at + 96] = bytes(96)
    put(raw, at + 8, incarnation)
    if terminal == 'reserve':
        require(before[at] == 0 and get(before, at + 8) + 1 == incarnation, 'reserve generation')
        raw[at:at + 3] = bytes([1, 0, 2])
        raw[at + 32:at + 80] = work(work_id)
    else:
        require(before[at] == 1 and get(before, at + 8) == incarnation, 'terminal reservation')
        require(before[at + 32:at + 80] == work(work_id), 'terminal claimant')
        if terminal != 'cancel':
            raw[at:at + 2] = bytes([2, 8 if terminal == 'undo' else 9])
            put(raw, at + 16, 3 if terminal == 'undo' else 2)
            put(raw, at + 24, 1)
    put(raw, 64, lsn)
    return seal(raw)


def descriptor(profile, work_id, birth, terminal, item):
    finished = terminal != 'reserve'
    lsn = birth + int(finished)
    raw = bytearray(1536)
    raw[:4] = b'XOD1'
    put(raw, 4, 1, 2)
    raw[6] = 6 if finished else 5
    put(raw, 8, 1536, 4)
    raw[16] = 2
    raw[24:72] = work(work_id)
    for at, value in ((72, lsn), (80, birth if finished else 0), (88, birth),
                      (104, 1), (544, lsn), (552, int(not finished)), (624, 1 + int(finished))):
        put(raw, at, value)
    raw[96] = 1
    put(raw, 112, 4096, 4)
    raw[128:160] = digest(profile)
    raw[352:354] = bytes([1, 4 if finished else 1])
    for at, value in ((360, 1), (368, int(finished)), (376, 1)):
        put(raw, at, value)
    raw[448:480] = digest(b'')
    raw[560:592] = digest(b'' if finished else item)
    consumed = {'reserve': 21200, 'cancel': 42400, 'status': 59200, 'undo': 59408}[terminal]
    put(raw, 608, consumed)
    for category in range(5):
        raw[640 + category * 128] = category + 1
    if not finished:
        for at, value in ((904, 1), (912, 38208), (936, 1), (960, 2), (968, 114688),
                          (992, 1), (1008, 70976), (1280, 2), (1288, 114688),
                          (1312, 1), (1328, 70976), (1336, birth)):
            put(raw, at, value)
    return seal(raw)


def operation(profile, before, number, incarnation, work_id, birth, terminal, epoch, old=b''):
    finished, init = terminal != 'reserve', terminal in ('status', 'undo')
    lsn = birth + int(finished)
    length = {'reserve': 21200, 'cancel': 21200, 'status': 38000, 'undo': 38208}[terminal]
    after = allocator(before, number, incarnation, work_id, lsn, terminal)
    item = token(number, incarnation, birth)
    desc = descriptor(profile, work_id, birth, terminal, item)
    image = payload(number, incarnation, lsn, terminal) if init else None
    body = free_cell(terminal) if init else b''
    opcode = 10 if finished else 8
    raw = bytearray(length)
    raw[:8] = b'XWF1\x01\x00\x03\x02'
    put(raw, 8, 160, 2)
    put(raw, 12, length, 4)
    put(raw, 16, length - 192, 4)
    put(raw, 20, 2 if init else 1, 2)
    raw[24:40] = DB
    for at, value in ((40, 1), (56, lsn), (64, lsn - 1)):
        put(raw, at, value)
    put(raw, 48, epoch, 4)
    raw[128:160] = digest(raw[:128])
    raw[160:164] = b'XSG1'
    for at, value, width in ((164, 2, 2), (166, opcode, 2), (168, 192, 2), (170, 1, 2),
                             (172, length - 192, 4), (176, lsn, 8),
                             (184, birth if finished else 0, 8), (280, 1536, 4),
                             (284, 2 if init else 1, 2), (290, 1, 2), (292, 2 if init else 1, 2),
                             (296, 12544 + (128 + len(body) if init else 0), 4),
                             (308, 208, 4), (312, 22720 if init else 6272, 4)):
        put(raw, at, value, width)
    raw[192] = 2
    raw[200:248] = work(work_id)
    if finished:
        require(len(old) == 1536, 'before descriptor')
        raw[248:280] = digest(old)
    raw[352:1888] = desc
    put(raw, 1888, opcode, 2)
    put(raw, 1892, 256, 4)
    put(raw, 2056, int(finished))
    native = 2144
    raw[native:native + 2] = bytes([6, 3])
    for at, value, width in ((8, CELL, 2), (10, CELL, 2), (12, 6208, 4), (16, 6208, 4), (20, 12544, 4)):
        put(raw, native + at, value, width)
    raw[native + 24:native + 56] = digest(before[CELL:16352])
    raw[native + 56:native + 88] = digest(after[CELL:16352])
    raw[2272:8480] = before[CELL:16352]
    raw[8480:14688] = after[CELL:16352]
    delta = 14688
    if init:
        size = 128 + len(body)
        raw[delta:delta + 8] = bytes([3 if terminal == 'undo' else 5, 2, 1, 0, 255, 255, 0, 0])
        for at, value, width in ((10, 16352 - len(body), 2), (16, len(body), 4), (20, size, 4)):
            put(raw, delta + at, value, width)
        raw[delta + 56:delta + 88] = digest(body)
        raw[delta + 128:delta + size] = body
        delta += size
    raw[delta] = 1 if not finished else 2 if init else 3
    raw[delta + 1] = 1 if finished else 2
    item_at = delta + (16 if finished else 112)
    raw[item_at:item_at + 96] = item
    patch = delta + 208
    raw[patch:patch + 2] = bytes([1, 10])
    put(raw, patch + 4, 6272, 4)
    raw[patch + 8:patch + 56] = before[24:72]
    put(raw, patch + 56, CELL, 2)
    put(raw, patch + 58, 6208, 2)
    raw[patch + 64:patch + 6272] = after[CELL:16352]
    if init:
        redo = patch + 6272
        raw[redo:redo + 2] = bytes([2, 8 if terminal == 'undo' else 9])
        put(raw, redo + 4, 16448, 4)
        raw[redo + 8:redo + 48] = image[24:64]
        put(raw, redo + 58, 16384, 2)
        raw[redo + 64:redo + 16448] = image
    require((patch + 6272 + (16448 if init else 0)) == length - 32, 'wire arithmetic')
    return seal(raw), after, desc, image


def base_literal(refs, allocator_image, last_descriptor, payloads, prefix):
    """One pure base6 input; physical/DWB fields are explicit claims, not traces."""
    raw = bytearray(BASE)
    raw[:4] = b'XRB1'
    for at, value, width in ((4, 2, 2), (6, 1, 2), (8, BASE, 4), (12, 4096, 4),
                             (144, 1, 4), (148, 1, 4), (336, 4096, 4), (1184, 1, 4)):
        put(raw, at, value, width)
    raw[16:32] = DB
    for at, value in ((32, 1), (40, 1), (80, 4), (88, 1), (96, 6), (152, len(prefix)),
                      (208, 3), (240, 5), (248, 6), (264, 1 << 9), (288, 1),
                      (328, 1), (448, get(refs['filemap.bin'], 32)), (504, 1), (1152, 32 << 20)):
        put(raw, at, value)
    raw[48:80] = digest(refs['profile.bin'])
    raw[160:192] = digest(prefix)
    raw[304] = 3
    raw[320] = 1
    raw[352:384] = digest(refs['profile.bin'])
    raw[384:448] = refs['filemap.bin'][80:144]
    raw[464:496] = digest(refs['bootstrap.bin'])
    raw[1192:1224] = digest(refs['wal-header.bin'])
    for i in range(5):
        raw[512 + i * 128] = i + 1
    raw[4096 + 32 * 1536:4096 + 33 * 1536] = last_descriptor
    raw[65536:81920] = allocator_image
    for n in range(1, 7):
        raw[81920 + (n - 1) * PAGE:81920 + n * PAGE] = refs[f'P{n:02}.bin']
    images = {0: allocator_image, **{n: refs[f'P{n:02}.bin'] for n in range(1, 9)}, **payloads}
    for number, image in images.items():
        at = 202752 + number * 96
        raw[at:at + 3] = bytes([1, image[6], 0])
        raw[at + 8:at + 56] = image[24:72]
        raw[at + 56:at + 88] = digest(image)
        if number >= 9:
            at = 180224 + number * 352
            raw[at:at + 4] = bytes([2 if image[6] == 8 else 1, image[6], 0, 1])
            raw[at + 8:at + 56] = image[24:72]
            put(raw, at + 56, get(image, 64))
            raw[at + 64:at + 96] = digest(image)
            if image[6] == 9:
                raw[at + 128:at + 352] = image[16128:16352]
    inventory = bytearray(160)
    inventory[:4] = b'XOI1'
    inventory[6:8] = bytes([1, 2])
    for at, value, width in ((4, 1, 2), (8, 128, 2), (10, 96, 2), (12, 160, 4)):
        put(inventory, at, value, width)
    inventory[16:32] = DB
    for at, value in ((32, 1), (40, 1), (48, 6)):
        put(inventory, at, value)
    inventory[88:120] = digest(b'')
    raw[208896:209056] = seal(inventory)
    for i, (offset, length, count, entry) in enumerate(SECTIONS):
        at = 1280 + i * 64
        put(raw, at, i + 1, 2)
        for field, value in ((4, offset), (8, length), (12, count), (16, entry)):
            put(raw, at + field, value, 4)
        raw[at + 32:at + 64] = digest(raw[offset:offset + length])
    return seal(raw)


def reserved_recovery(profile, initial):
    """Independent unfinished native work3, then original recovery in epoch2."""
    before = initial
    for work_id, kind in ((1, 'status'), (2, 'undo')):
        birth, number = work_id * 2 - 1, work_id + 8
        _, reserved, desc, _ = operation(profile, before, number, 2, work_id, birth, 'reserve', 1)
        _, before, _, _ = operation(profile, reserved, number, 2, work_id, birth, kind, 1, desc)
    _, reserved, desc, _ = operation(profile, before, 11, 2, 3, 5, 'reserve', 1)
    frame, after, _, _ = operation(profile, reserved, 11, 2, 3, 5, 'cancel', 2, desc)
    return frame, after


def append_reserved(repo, here, plan_bytes, refs, start):
    """Create only the newly approved delta; old raw files/receipts are immutable."""
    previous = (here / 'raw-index.tsv').read_bytes()
    require(sha(previous) == 'e48c132b3473e9f160d77903774fcfedb66cbae9e410481307ef15a42de23239', 'prior index')
    entries = {}
    for line in previous.decode().splitlines()[1:]:
        key, name, length, checksum = line.split('\t')
        raw = (repo / name).read_bytes()
        require(len(raw) == int(length) and sha(raw) == checksum, 'prior raw drift')
        entries[key] = (name, raw)
    additions = reserved_recovery(refs['profile.bin'], refs['P00.bin'])
    created = []
    for name, raw in zip(('reserved-cancel-l6.wal', 'reserved-cancel-l6.page'), additions):
        path = here / 'literals' / name
        with path.open('xb') as stream:
            stream.write(raw)
        entries[name] = (str(path.relative_to(repo)), raw)
        created.append({'path': name, 'bytes': len(raw), 'sha256': sha(raw)})
    index = 'key\tpath\tbytes\tsha256\n' + ''.join(
        f'{key}\t{name}\t{len(raw)}\t{sha(raw)}\n' for key, (name, raw) in sorted(entries.items()))
    # The prior bytes and recipes were preserved before the new input plan.
    history = here / 'literals/history/r0/raw-index.tsv'
    require(history.read_bytes() == previous, 'missing prior index evidence')
    require((here / 'input-plan.json').read_bytes() == plan_bytes, 'input plan drift')
    for name, raw in entries.values():
        require((repo / name).read_bytes() == raw, 'raw changed during extension')
    require(sum(len(raw) for name, raw in entries.values()
                if '/dev3-plan4-free-undo/literals/' in name) < 2 << 20, 'new raw disk cap')
    (here / 'raw-index.tsv').write_text(index)
    receipt = dict(schema='XADB_PLAN4_FREE_UNDO_EXTENSION_V1',
                   status='INPUT_GENERATED_NOT_PRODUCT_VERIFICATION',
                   generator_sha256=sha(Path(__file__).read_bytes()), input_plan_sha256=sha(plan_bytes),
                   previous_index_sha256=sha(previous), raw_index_sha256=sha(index.encode()),
                   generated=created, indexed_files=len(entries), elapsed_seconds=time.monotonic() - start)
    require(receipt['elapsed_seconds'] < 30, 'generation time cap')
    with (here / 'literals/reserved-recovery-generation-receipt.json').open('x') as stream:
        json.dump(receipt, stream, indent=2, sort_keys=True)
        stream.write('\n')
    print(json.dumps(receipt, sort_keys=True))


def main():
    start = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo', type=Path, required=True)
    parser.add_argument('--append-reserved-recovery', action='store_true')
    args = parser.parse_args()
    repo = args.repo.resolve(strict=True)
    here = Path(__file__).resolve().parent
    plan_path = here / 'input-plan.json'
    plan_bytes = plan_path.read_bytes()
    plan = json.loads(plan_bytes)
    require(plan['schema'] == 'XADB_PLAN4_FREE_UNDO_INPUT_V1', 'input schema')
    files, refs, original = {}, {}, {}
    for name, row in plan['references'].items():
        source = repo / row['path']
        value = source.read_bytes()
        require(len(value) == row['bytes'] and sha(value) == row['sha256'], f'pinned reference {name}')
        refs[name] = value
        original[source] = value
        files['reference/' + name] = (row['path'], value)
    if args.append_reserved_recovery:
        append_reserved(repo, here, plan_bytes, refs, start)
        return
    out = here / 'literals'
    out.mkdir(exist_ok=False)

    def save(key, raw):
        require(key not in files, 'duplicate literal key')
        path = out / key
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as stream:
            stream.write(raw)
        files[key] = (str(path.relative_to(repo)), raw)

    profile = refs['profile.bin']
    native_frames, native_payloads = {}, {}
    before = refs['P00.bin']
    base_inputs = None
    for work_id, kind in enumerate(('status', 'undo', 'undo', 'status', 'undo', 'status', 'undo'), 1):
        number, birth = 8 + work_id, 2 * work_id - 1
        epoch = 1 if work_id <= 3 else 2 if work_id <= 5 else 3
        first, reserved, desc, _ = operation(profile, before, number, 2, work_id, birth, 'reserve', epoch)
        second, before, desc, image = operation(profile, reserved, number, 2, work_id, birth, kind, epoch, desc)
        native_frames[birth], native_frames[birth + 1] = first, second
        native_payloads[number] = image
        save(f'native-page{number}-init.page', image)
        save(f'native-allocator-l{birth + 1}.page', before)
        if work_id == 3:
            base_inputs = (before, desc, dict(native_payloads))
    prefix = refs['wal-header.bin'] + b''.join(native_frames[n] for n in range(1, 7))
    for epoch, lsns in ((1, range(1, 7)), (2, range(7, 11)), (3, range(11, 15))):
        save(f'native-wal-e{epoch}-suffix.bin', b''.join(native_frames[n] for n in lsns))
    save('base/mixed-l6.xrb', base_literal(refs, *base_inputs, prefix))
    # Pure Status shares native 1/2; pure Undo/Cancel share reserve1 but have
    # distinct adjacent terminal2. Identity is fixed by the input, not output.
    _, reserved, reserve_desc, _ = operation(profile, refs['P00.bin'], 9, 2, 1, 1, 'reserve', 1)
    for terminal in ('undo', 'cancel'):
        frame, _, _, _ = operation(profile, reserved, 9, 2, 1, 1, terminal, 1, reserve_desc)
        save(f'pure/{terminal}-02.wal', frame)
    # The memory chain has independently preserved N=0 L3..7 groups, after the
    # newly generated v2 Status reserve/INIT. Its later allocator LSN is 2, not 7.
    before = allocator(reserved, 9, 2, 1, 2, 'status')
    for work_id, number, incarnation, birth, terminal in ((2, 10, 2, 8, 'undo'),
                                                        (3, 11, 2, 10, 'cancel'),
                                                        (4, 11, 3, 12, 'undo')):
        first, reserved, desc, _ = operation(profile, before, number, incarnation, work_id, birth, 'reserve', 1)
        second, before, _, _ = operation(profile, reserved, number, incarnation, work_id, birth, terminal, 1, desc)
        save(f'memory/{birth:02}.wal', first)
        save(f'memory/{birth + 1:02}.wal', second)
    recovery_frame, recovery_allocator = reserved_recovery(profile, refs['P00.bin'])
    save('reserved-cancel-l6.wal', recovery_frame)
    save('reserved-cancel-l6.page', recovery_allocator)
    index = 'key\tpath\tbytes\tsha256\n' + ''.join(
        f'{key}\t{path}\t{len(raw)}\t{sha(raw)}\n' for key, (path, raw) in sorted(files.items()))
    with (here / 'raw-index.tsv').open('x') as stream:
        stream.write(index)
    receipt = dict(schema='XADB_PLAN4_FREE_UNDO_GENERATION_V1',
                   status='INPUT_GENERATED_NOT_PRODUCT_VERIFICATION',
                   generator_sha256=sha(Path(__file__).read_bytes()), input_plan_sha256=sha(plan_bytes),
                   raw_index_sha256=sha(index.encode()),
                   generated_files=sum('/dev3-plan4-free-undo/literals/' in path for path, _ in files.values()),
                   generated_bytes=sum(len(raw) for path, raw in files.values() if '/dev3-plan4-free-undo/literals/' in path),
                   indexed_files=len(files), elapsed_seconds=time.monotonic() - start,
                   base_claim='pure capture6 physical32MiB/DWB-generation1; no selected-source authority')
    require(receipt['generated_bytes'] < 2 << 20, 'new raw disk cap')
    require(time.monotonic() - start < 30, 'generation time cap')
    require(plan_path.read_bytes() == plan_bytes, 'input drift')
    for path, value in original.items():
        require(path.read_bytes() == value, 'original reference changed')
    with (out / 'generation-receipt.json').open('x') as stream:
        json.dump(receipt, stream, indent=2, sort_keys=True)
        stream.write('\n')
    print(json.dumps(receipt, sort_keys=True))


if __name__ == '__main__':
    main()
