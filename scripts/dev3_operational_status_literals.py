#!/usr/bin/env python3
"""Independent P2 literals: A INIT, B reserve/cancel, B reserve/INIT.

Uses the existing independent wire field primitives, never product execution or
codecs. Explicitly addresses page2 and preserves every other allocator entry.
Generation of fixtures is not product verification.
"""
import argparse
import hashlib
import json
from pathlib import Path

import dev3_status_init_literals as wire


def allocator_after(before, page_no, work, lsn, operation):
    wire.require(1 <= page_no < 64 and before[-32:] == wire.sha(before[:-32]), 'allocator input')
    offset = wire.CELL_AT + 64 + page_no * 96
    generation = wire.at(before, offset + 8)
    if operation == 'reserve':
        wire.require(before[offset] == 0 and generation < 2**64 - 1, 'free entry')
        generation += 1
        fields = [(0, bytes([1, 0, 2])), (8, wire.u(generation)), (32, wire.work_owner(work))]
    else:
        wire.require(before[offset] == 1 and before[offset+32:offset+80] == wire.work_owner(work), 'reserved entry')
        fields = [(8, wire.u(generation))]
        if operation == 'init':
            fields += [(0, bytes([2, 9, 0])), (16, wire.u(2)), (24, wire.u(1))]
        else:
            wire.require(operation == 'cancel', 'terminal')
    result = bytearray(before)
    result[offset:offset+96] = wire.record(96, fields)
    result[64:72] = wire.u(lsn)
    return wire.sealed_page(result), generation


def token(page_no, incarnation, birth):
    return bytes(wire.record(96, [(0, bytes([1, 3])),
        (8, wire.u(wire.FILE) + wire.u(page_no) + wire.u(incarnation)),
        (64, wire.u(1)), (72, wire.u(birth))]))


def status_page(page_no, incarnation, lsn):
    return wire.sealed_page(wire.record(wire.PAGE, [
        (0, b'XDP1' + wire.u(1, 2) + bytes([9, 0])), (8, wire.DB),
        (24, wire.u(wire.FILE) + wire.u(page_no) + wire.u(incarnation) + wire.u(2) + wire.u(1) + wire.u(lsn)),
        (72, wire.u(1, 2) + wire.u(132, 2) + wire.u(wire.HOME_AT, 2)),
        (128, wire.u(wire.HOME_AT, 2) + wire.u(wire.HOME_LEN, 2)),
        (wire.HOME_AT, wire.free_status_home()),
    ]))


def history(profile, genesis):
    before, output, frames = genesis, {}, []
    for work, (page_no, terminal) in enumerate(((1, 'init'), (2, 'cancel'), (2, 'init')), 1):
        birth = 2 * work - 1
        reserved, incarnation = allocator_after(before, page_no, work, birth, 'reserve')
        item = token(page_no, incarnation, birth)
        reserve_descriptor = wire.descriptor(profile, work, birth, birth, 'reserve', item)
        reserve = wire.wal(before, reserved, None, None, reserve_descriptor, work, birth, birth, item, 'reserve')
        after, actual = allocator_after(reserved, page_no, work, birth + 1, terminal)
        wire.require(actual == incarnation, 'terminal incarnation')
        page = status_page(page_no, incarnation, birth + 1) if terminal == 'init' else None
        descriptor = wire.descriptor(profile, work, birth + 1, birth, terminal, item)
        ending = wire.wal(reserved, after, page, reserve_descriptor, descriptor, work, birth + 1, birth, item, terminal)
        for part, data in {'allocator.before': before, 'allocator.reserved': reserved,
            'allocator.after': after, 'reserve.descriptor': reserve_descriptor,
            'terminal.descriptor': descriptor, 'reserve.wal': reserve, 'terminal.wal': ending}.items():
            output[f'work{work}-{part}'] = bytes(data)
        if page is not None:
            output[f'work{work}-status.after'] = page
        frames += [reserve, ending]
        before = after
    output['wal.frames-only'] = b''.join(frames)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root, output = args.root.resolve(), args.output.resolve()
    wire.require(root in output.parents and not output.exists(), 'new fixture directory required')
    inputs, provenance = wire.load_inputs(root)
    files = []
    for profile in (1, 2, 3):
        for name, data in history(inputs[f'profile{profile}-d3-profile.bin'], inputs[f'profile{profile}-allocation.bin']).items():
            path = output / f'p{profile}' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            files.append({'path':str(path.relative_to(output)), 'bytes':len(data), 'sha256':hashlib.sha256(data).hexdigest()})
    manifest = {'status':'INPUTS_ONLY_NOT_PRODUCT_VERIFICATION', 'generator_sha256':wire.sha(Path(__file__).read_bytes()).hex(),
        'wire_generator_sha256':wire.sha(Path(wire.__file__).read_bytes()).hex(), 'provenance':provenance,
        'history':[{'work':1,'page':1,'incarnation':2,'terminal':'init','lsn':2},
                   {'work':2,'page':2,'incarnation':2,'terminal':'cancel','lsn':4},
                   {'work':3,'page':2,'incarnation':3,'terminal':'init','lsn':6}], 'files':files}
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')


if __name__ == '__main__':
    main()
