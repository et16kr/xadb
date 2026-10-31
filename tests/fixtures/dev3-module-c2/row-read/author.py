#!/usr/bin/env python3
"""S8 C+B independent literal ingress input; run as `python3 -I author.py`.

The C+B fixture writes a row without A (dev3-module-consumers §8.3): C's
actual Begin and Reserve issue the row, and this literal proposes the
canonical row and its Forward under that reservation. C then checks every
byte as for any producer. Nothing here is A's encoder, INSERT or CLR.

The live state is the S2 Row write fixture (table T object 6, Status 11,
the Free Undo 14, RowLeaf 13, RowReserve LSN 15, Forward LSN 16). The S2W-R09
author `dev3-module-c3/row-write/forward.py` (pinned by SHA-256 and
imported, not copied) rebuilds it on the N1 History model
(`module-row-format/transition/author.py`, pinned there) and re-encodes
every live page byte-equal before use. That model computes the RowReserve,
which must equal the pinned S2 literal, and then the Forward.

Writes literal/*.bin and expected.json beside this file and prints the json.
"""
import importlib.util
import json
import sys
from hashlib import sha256
from pathlib import Path
from struct import pack

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parent.parent
FORWARD = ('dev3-module-c3/row-write/forward.py',
           'cbab5fcf089099524ea31fa1da1b6673b617973623bc7c59b7334ef45064cd9f')
# The pages a Forward of the fixture's row changes: (file, model role, page).
AFTER = (('after-root.bin', 'root', 1), ('after-jobs.bin', 'jobs', 2),
         ('after-status.bin', 'status', 11), ('after-undo.bin', 'undo', 14),
         ('after-leaf.bin', 'leaf', 13))


def load(rel, digest, name):
    path = FIXTURES / rel
    assert sha256(path.read_bytes()).hexdigest() == digest, path
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    f = load(*FORWARD, 's2w_r09_forward_author')
    a = f.load_n1()
    live = {name: f.pinned(f.HERE / 'live' / name, digest) for name, digest in f.LIVE.items()}
    h = f.live_history(a, live)
    canonical = pack('<IBq', 1, 2, f.VALUE)
    reserve, ordinal = h.reserve(f.PREVIOUS_LSN + 1, f.FREE_UNDO, f.STATEMENT, canonical)
    assert (len(reserve), sha256(reserve).hexdigest()) == f.RESERVE
    frame = h.row_change(f.FORWARD, f.PREVIOUS_LSN + 2, ordinal)
    pages = {'root': h.root_page(), 'jobs': h.jobs_page(0),
             'status': h.status_page(f.STATUS_PAGE), 'undo': h.undo_page(f.FREE_UNDO),
             'leaf': h.leaf()}
    out = HERE / 'literal'
    out.mkdir(exist_ok=True)
    files = {'canonical.bin': canonical, 'forward-frame.bin': frame}
    for name, role, _ in AFTER:
        files[name] = pages[role]
    for name, data in files.items():
        (out / name).write_bytes(data)
    expected = {
        'case': 'MCB05-L01',
        'row': a.get(h.bank[ordinal], 320),
        'value': f.VALUE,
        'reserve_lsn': f.PREVIOUS_LSN + 1,
        'forward_lsn': f.PREVIOUS_LSN + 2,
        'canonical_sha256': sha256(canonical).hexdigest(),
        'files': {name: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
                  for name, data in sorted(files.items())},
        'pages': {name: page for name, _, page in AFTER},
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
