#!/usr/bin/env python3
"""Independent S4 full rollback after the frozen one-row S2 input.

Only pinned stdlib fixture models are imported. Expected bytes are computed
before examining product output; the product is never used as an oracle.
"""
import importlib.util
import json
import sys
from hashlib import sha256
from pathlib import Path
from struct import pack

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
FORWARD = HERE.parent / 'row-write' / 'forward.py'
FORWARD_SHA = 'cbab5fcf089099524ea31fa1da1b6673b617973623bc7c59b7334ef45064cd9f'


def main():
    assert sha256(FORWARD.read_bytes()).hexdigest() == FORWARD_SHA
    spec = importlib.util.spec_from_file_location('s4_input', FORWARD)
    f = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(f)
    a = f.load_n1()
    live = {name: f.pinned(f.HERE / 'live' / name, digest) for name, digest in f.LIVE.items()}
    h = f.live_history(a, live)
    reserve, root = h.reserve(15, f.FREE_UNDO, 1, pack('<IBq', 1, 2, f.VALUE))
    assert (len(reserve), sha256(reserve).hexdigest()) == f.RESERVE
    forward = h.row_change(2, 16, root)
    assert sha256(forward).hexdigest() == '0ee305722bff97a71161e2111bb7a15059b3a35b8a941c603a45e2fc5de81043'
    frames = [h.status_patch(3, 17), h.row_change(4, 18, root), h.end(19, False)]
    names = ['rollback-start', 'clr', 'abort-end']
    for name, frame in zip(names, frames):
        (HERE / (name + '.bin')).write_bytes(frame)
    (HERE / 'rollback.bin').write_bytes(b''.join(frames))
    after = {'root1': h.root_page(), 'jobs2': h.jobs_page(0), 'status11': h.status_page(11),
             'undo14': h.undo_page(14), 'leaf13': h.leaf(), 'bank': b''.join(map(bytes, h.bank))}
    expected = {'case': 'S4-R02', 'lsn': 19, 'q': 0,
                'frames': {name: {'bytes': len(frame), 'sha256': sha256(frame).hexdigest()} for name, frame in zip(names, frames)},
                'after_sha256': {name: sha256(data).hexdigest() for name, data in after.items()},
                'input_author_sha256': FORWARD_SHA, 'model_sha256': f.N1[1]}
    (HERE / 'expected.json').write_text(json.dumps(expected, indent=2, sort_keys=True) + '\n')
    print(json.dumps(expected, sort_keys=True))


if __name__ == '__main__':
    main()
