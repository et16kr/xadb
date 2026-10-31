#!/usr/bin/env python3
"""Independent S3 Commit after the frozen one-row S2 fixture.

Uses only the pinned stdlib model and S2 input facts. It never imports product
code or uses actual Commit output to choose expected bytes.
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
    spec = importlib.util.spec_from_file_location('s2_forward_input', FORWARD)
    f = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(f)
    a = f.load_n1()
    live = {name: f.pinned(f.HERE / 'live' / name, digest) for name, digest in f.LIVE.items()}
    h = f.live_history(a, live)
    reserve, root = h.reserve(15, f.FREE_UNDO, 1, pack('<IBq', 1, 2, f.VALUE))
    assert (len(reserve), sha256(reserve).hexdigest()) == f.RESERVE
    forward = h.row_change(2, 16, root)
    assert sha256(forward).hexdigest() == '0ee305722bff97a71161e2111bb7a15059b3a35b8a941c603a45e2fc5de81043'
    frame = h.end(17, True)
    (HERE / 'commit.bin').write_bytes(frame)
    after = {'root1': h.root_page(), 'jobs2': h.jobs_page(0),
             'status11': h.status_page(11), 'bank': b''.join(map(bytes, h.bank))}
    expected = {'case': 'S3-R01', 'lsn': 17, 'q': 2,
                'frame_bytes': len(frame), 'frame_sha256': sha256(frame).hexdigest(),
                'after_sha256': {k: sha256(v).hexdigest() for k, v in after.items()},
                'input_author_sha256': FORWARD_SHA, 'model_sha256': f.N1[1]}
    (HERE / 'expected.json').write_text(json.dumps(expected, indent=2, sort_keys=True) + '\n')
    print(json.dumps(expected, sort_keys=True))


if __name__ == '__main__':
    main()
