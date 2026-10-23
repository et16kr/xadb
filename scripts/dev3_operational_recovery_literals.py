#!/usr/bin/env python3
"""Independent repeated INIT inputs; generation is not product verification.

The default first-extent boundary has one cancellation before each page after A.
Long histories and other profiles are prepared separately for the final S5 suite.
"""
import argparse
import hashlib
import json
from pathlib import Path

import dev3_operational_status_literals as operation

wire = operation.wire


def history(profile, genesis, pages, cancellations):
    wire.require(1 <= pages <= 63 and 0 <= cancellations <= 4096, 'history bounds')
    # Reject oversized combinations before allocating or constructing any frame.
    expected_bytes = pages * (21200 + 38000) + (pages - 1) * cancellations * (2 * 21200)
    wire.require(expected_bytes + 4096 <= 268435456, 'WAL capacity preflight')
    before, frames, statuses, work = genesis, bytearray(), {}, 0
    for page_no in range(1, pages + 1):
        for terminal in ['cancel'] * (cancellations if page_no > 1 else 0) + ['init']:
            work += 1
            birth = 2 * work - 1
            reserved, incarnation = operation.allocator_after(before, page_no, work, birth, 'reserve')
            item = operation.token(page_no, incarnation, birth)
            prepared = wire.descriptor(profile, work, birth, birth, 'reserve', item)
            frames += wire.wal(before, reserved, None, None, prepared, work, birth, birth, item, 'reserve')
            after, _ = operation.allocator_after(reserved, page_no, work, birth + 1, terminal)
            page = operation.status_page(page_no, incarnation, birth + 1) if terminal == 'init' else None
            descriptor = wire.descriptor(profile, work, birth + 1, birth, terminal, item)
            frames += wire.wal(reserved, after, page, prepared, descriptor, work, birth + 1, birth, item, terminal)
            if page is not None:
                statuses[page_no] = page
            before = after
    wire.require(len(frames) == expected_bytes, 'literal frame census')
    result = {'wal.frames-only': bytes(frames), 'allocator.after': bytes(before),
              'terminal.descriptor': bytes(descriptor)}
    result.update({f'status-{n}.after': bytes(data) for n, data in statuses.items()})
    return result, work


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--profile', type=int, choices=(1, 2, 3), required=True)
    parser.add_argument('--pages', type=int, default=63)
    parser.add_argument('--cancellations', type=int, default=1)
    args = parser.parse_args()
    root, output = args.root.resolve(), args.output.resolve()
    wire.require(root in output.parents and not output.exists(), 'new output inside root required')
    inputs, provenance = wire.load_inputs(root)
    values, work = history(inputs[f'profile{args.profile}-d3-profile.bin'],
                          inputs[f'profile{args.profile}-allocation.bin'], args.pages, args.cancellations)
    output.mkdir(parents=True)
    files = []
    for name, data in values.items():
        (output / name).write_bytes(data)
        files.append({'path': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    (output / 'manifest.json').write_text(json.dumps({
        'status': 'INPUTS_ONLY_NOT_PRODUCT_VERIFICATION', 'profile': args.profile,
        'pages': args.pages, 'cancellations_per_later_page': args.cancellations,
        'work_high_water': work, 'complete_lsn': 2 * work, 'provenance': provenance,
        'generators': {Path(p).name: wire.sha(Path(p).read_bytes()).hex()
                       for p in (__file__, operation.__file__, wire.__file__)},
        'files': files}, indent=2) + '\n')


if __name__ == '__main__':
    main()
