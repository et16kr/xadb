#!/usr/bin/env python3
"""S9 C+B literal ingress after a reopen; run as `python3 -I author.py`.

Consumers §8.3 native C+B: P1 is the actual install, CREATE and Commit;
P2's actual recovery (RejectInitialLosers) authenticates the table and its
anchor, and only then the independent literal is applied and committed; P3
recovers again and B reads. The S8 literal is bound to the first open (WAL
stream 1, LSN 15/16). This literal is late-bound to P2's actual identity:
the state after one recovery to READY.

The live state is C's actual state after that recovery, the Row actor's
Begin, its first statement, B's table issue and a settled preflight,
captured from the memory writer of `catalog_memory/row_reopen_literal.rs`
and pinned here by SHA-256 (the S2 live fixture precedent): WAL stream 2
(`d3-wal-00000002`), complete LSN 20, the Free Undo C reserves is page 10
(the Catalog actor's released Undo, generation 3), the Root's next Job is
ordinal 2. The Rust cases (memory and native P2) check the pool and the
control against the same pins before the reserve. The N1 History model
(`module-row-format/transition/author.py`, pinned by the S2W-R09 author
`dev3-module-c3/row-write/forward.py`, pinned here) re-encodes every live
input byte-equal, with the reading of `forward.py`'s `live_history` and the
Free Undo page as a parameter. It computes the RowReserve (LSN 21), which
C's WAL must equal, and the Forward (LSN 22) of RowId 1, Int64 7.

The model writes WAL stream 1 into every frame header (offset 48). The one
late binding is that field: each frame's header gets stream 2, its header
SHA-256 and its frame seal are recomputed, and nothing else changes. That
the bound RowReserve equals C's actual frame byte for byte is the check
that no other identity moved.

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
LIVE = {
    'root1.bin': '808882c278e81121c51d285dd690f6e4d35475b43cf6a6ff71f5b81d8c8c409e',
    'jobs2.bin': 'fa17df47a7e82af55001274aa717481c0e0a3b04a9e010ea7eb51ab95d251a0e',
    'status11.bin': '6d2ef6534c8229c097c47c79f9c3333a50593382a7d58933f9f512763dc19b82',
    'leaf13.bin': '30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396',
    'undo10.bin': '8a4a1464e63518e84b23042d936990dca3d29e3da59e0842e0bd5b6cf748eb6c',
    'bank.bin': '63984ecee4d90f49a7838bba2c5f5d748c6a2829059bb79dd17d5376df0f23d5',
    'tx.bin': '5007191f2a5b4ee8cf932a532e286cc4bfb782d89eaf4b6ebebdfd251b2356a3',
    'aging.bin': 'df632162d6196501cc70ecc310ebbf48d43ec6b65b8d39545fd06e611abf9d9a',
    'profile.bin': 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e',
}
# The live fixture's scalars after one recovery.
STREAM = 2
PREVIOUS_LSN = 20
STATEMENT = 1
FREE_UNDO = 10
VALUE = 7
AFTER = (('after-root.bin', 'root', 1), ('after-jobs.bin', 'jobs', 2),
         ('after-status.bin', 'status', 11), ('after-undo.bin', 'undo', FREE_UNDO),
         ('after-leaf.bin', 'leaf', 13))


def load(rel, digest, name):
    path = FIXTURES / rel
    assert sha256(path.read_bytes()).hexdigest() == digest, path
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def live_history(f, a, live):
    """`forward.py`'s `live_history` with the Free Undo page FREE_UNDO."""
    a.DB = bytes(range(1, 17))
    a.AGING_REF = a.DB + pack('<QB7xQQ', 1, 3, 1, 1)
    a.PROFILE = live['profile.bin']
    h = a.History()
    root = live['root1.bin']
    h.root = bytearray(a.cell_at(root, 0)[1])
    h.root_lsn = a.page_lsn(root)
    bank = live['bank.bin']
    h.bank = [bytearray(bank[i * 1024:(i + 1) * 1024]) for i in range(64)]
    h.jobs_lsn[0] = a.page_lsn(live['jobs2.bin'])
    status = live['status11.bin']
    h.status[f.STATUS_PAGE] = [a.cell_at(status, 0)[1], a.page_lsn(status)]
    undo = live[f'undo{FREE_UNDO}.bin']
    h.undo[FREE_UNDO] = [a.cell_at(undo, 0)[1], a.page_lsn(undo)]
    leaf = live['leaf13.bin']
    h.leaf_lsn = a.page_lsn(leaf)
    for slot in range(1, a.get(leaf, 72, 2)):
        cell = a.cell_at(leaf, slot)[1]
        h.rows[int.from_bytes(cell[4:12], 'big')] = cell[12:]
    tx, aging = live['tx.bin'], live['aging.bin']
    h.tx = dict(actor=tx[24:72], status=f.STATUS_PAGE,
                chain=[a.get(tx, 72), a.get(tx, 80), a.get(tx, 88), a.get(tx, 624)],
                inventory=(a.get(tx, 544), a.get(tx, 552), tx[560:592]),
                consumed=a.get(tx, 608), mutation=a.get(tx, 1344),
                credit=a.get(tx, 640 + 3 * 128 + 8) == 3)
    h.aging = [a.get(aging, 72), a.get(aging, 80), a.get(aging, 88), a.get(aging, 624)]
    assert h.root_page() == root
    assert b''.join(map(bytes, h.bank)) == bank
    assert h.jobs_page(0) == live['jobs2.bin']
    assert h.status_page(f.STATUS_PAGE) == status
    assert h.undo_page(FREE_UNDO) == undo
    assert h.leaf() == leaf
    assert h.tx_descriptor() == tx
    assert h.aging_descriptor() == aging
    assert h.status_core()[8:56] == h.tx['actor']
    return h


def restream(a, frame):
    """The frame with header stream STREAM (offset 48), its header SHA-256
    (128..160) and its seal recomputed; the model wrote stream 1."""
    assert a.get(frame, 48, 4) == 1
    out = bytearray(frame)
    a.put(out, 48, STREAM, 4)
    out[128:160] = a.sha(bytes(out[:128]))
    return a.reseal(bytes(out))


def main():
    f = load(*FORWARD, 's2w_r09_forward_author')
    a = f.load_n1()
    live = {name: f.pinned(HERE / 'live' / name, digest) for name, digest in LIVE.items()}
    h = live_history(f, a, live)
    canonical = pack('<IBq', 1, 2, VALUE)
    reserve, ordinal = h.reserve(PREVIOUS_LSN + 1, FREE_UNDO, STATEMENT, canonical)
    reserve = restream(a, reserve)
    forward = restream(a, h.row_change(f.FORWARD, PREVIOUS_LSN + 2, ordinal))
    pages = {'root': h.root_page(), 'jobs': h.jobs_page(0),
             'status': h.status_page(f.STATUS_PAGE), 'undo': h.undo_page(FREE_UNDO),
             'leaf': h.leaf()}
    files = {'canonical.bin': canonical, 'forward-frame.bin': forward,
             **{name: pages[role] for name, role, _ in AFTER}}
    out = HERE / 'literal'
    out.mkdir(exist_ok=True)
    for name, data in files.items():
        (out / name).write_bytes(data)
    job = h.bank[ordinal]
    pins = {FORWARD[0]: FORWARD[1], f.N1[0]: f.N1[1],
            **{'dev3-module-c2/row-read-reopen/live/' + name: digest
               for name, digest in LIVE.items()}}
    frame = lambda data: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
    expected = {
        'schema': 'xadb-module-row-v1',
        'source_kind': 'C+B',
        'provenance': {'author': 'dev3-module-c2/row-read-reopen/author.py '
                                 '(run as python3 -I author.py)',
                       'pinned_inputs': dict(sorted(pins.items())),
                       'no_xadb_code': 'no XADB encoder, decoder, A producer or B reader is run'},
        'case': 'MCB05-L03',
        'stream': STREAM,
        'previous_lsn': PREVIOUS_LSN,
        'row': a.get(job, 320),
        'value': VALUE,
        'ordinal': ordinal,
        'free_undo': FREE_UNDO,
        'reserve': dict(frame(reserve), lsn=PREVIOUS_LSN + 1),
        'forward_lsn': PREVIOUS_LSN + 2,
        'files': {name: frame(data) for name, data in sorted(files.items())},
        'pages': {name: page for name, _, page in AFTER},
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
