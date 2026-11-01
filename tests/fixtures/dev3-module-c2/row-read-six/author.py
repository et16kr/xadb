#!/usr/bin/env python3
"""S10 C+B six-row corpus input (MCB01, MCB02); run as `python3 -I author.py`.

Consumers §9.2 MCB01 reads a committed corpus of six rows a..f joined to an
actual receipt, with a RowId gap between them. This history writes that
corpus with the same Row actor in one statement, without A: C's actual
Begin, RowReserves and one Cancel issue every identity, and this literal
proposes each canonical row and Forward. The gap is RowId 3, which C
issues to a reservation of an Int64 0 row (`canonical-gap.bin`, the plan C
issues it under) and then cancels (the Cancel is C's; the model computes it
too). Nothing here is A's encoder, INSERT or CLR, and no XADB
encoder or decoder runs.

The live state is C's actual state before the first RowReserve, captured
from the memory writer of `catalog_memory/row_corpus.rs` and pinned here by
SHA-256 (the S2 and S9 live fixture precedent): table T (object 6, one
Int64 column), the Row actor on Status 11, its first statement, RowLeaf 13
empty and seven Free Undo containers 14..20 (the S2 one and six more actual
Undo INITs before the Begin, LSN 15..26). The N1 History model
(`module-row-format/transition/author.py`, pinned by the S2W-R09 author
`dev3-module-c3/row-write/forward.py`, itself pinned here) re-encodes every
live input byte-equal before use. The model computes every RowReserve and
the Cancel, which C's WAL must equal, and every Forward:

  LSN 27/28  a  RowId 1  Int64 10   Reserve, Forward (no H)
  LSN 29/30  b  RowId 2  Int64 20   Reserve, Forward (H = a's U)
  LSN 31/32     RowId 3             Reserve, Cancel  (the gap)
  LSN 33/34  c  RowId 4  Int64 30   Reserve, Forward (H = b's U)
  LSN 35/36  d  RowId 5  Int64 40
  LSN 37/38  e  RowId 6  Int64 50
  LSN 39/40  f  RowId 7  Int64 60

Each Reserve takes the lowest Free Undo container; the cancelled one (16)
stays Free at its new generation and c reserves it again, so f takes 19 and
20 stays Free. C's Commit (q 2) is C's own and not part of this literal.

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
    'root1.bin': '24e9f7363a0e7b5be6efe203cb0a5f3edd4dfa123b534bb7ab6214d7c52ffdc1',
    'jobs2.bin': '52a6fc41e8f5d1866fc043c456b894e2add2535247334f38425346109a9630f0',
    'status11.bin': '075233ef08107d672ed1fbf050c71588f7e21d5dc6ef7ccfb29b11aa41cb2d2b',
    'leaf13.bin': '30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396',
    'undo14.bin': '7fe328b11b1d505fa2f35c0297e5cfcc369143ea7e8272298d564fe751774eab',
    'undo15.bin': 'd826cb72764ce486fcc4f9346b6ce6e93a7d967bc1e24d285dd6b4f80acac31d',
    'undo16.bin': '9124b063b342045a1cf74bbf04a8fcc9e243559e4834ff827a793ceaecfce586',
    'undo17.bin': 'efe4e8407e685879c102eeca0ff8112b320d8ecb2987940e32c15aaf6befd781',
    'undo18.bin': '6c7f060c0b6eba6dd038991cf0a7deb5f29655677352d1bd532088a31ab85174',
    'undo19.bin': '0ed54969ac3645d94461ca7acf8128f0e89b789ef82f2396ea1dea7725db5d62',
    'undo20.bin': '63da4e8d9ad0c738e2b580ecebf96746b5af565d36f09c4c5efa82d48209f351',
    'bank.bin': '14e495647b5bf0ff4d28a666172830acd1911a5bb922d0d61da1cebb5f281ec6',
    'tx.bin': '65d28b0988462a426fdf6299af18679edd7e5d93aba4b87df813d3be9fd1ffdc',
    'aging.bin': '6c7316883e3fa6330829a0469b70946cedea4e901fd36db4af1d569d9d25e8f6',
    'profile.bin': 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e',
}
# The WAL complete LSN before the first RowReserve and the statement.
PREVIOUS_LSN = 26
STATEMENT = 1
FREE_UNDO = (14, 15, 16, 17, 18, 19, 20)
# The corpus in issue order: (symbol, Int64 value), None for the cancelled
# reservation, with the Free Undo container each reservation takes.
STEPS = (('a', 10, 14), ('b', 20, 15), (None, None, 16), ('c', 30, 16), ('d', 40, 17),
         ('e', 50, 18), ('f', 60, 19))
# The pages a Forward changes: (file stem, model role).
ROLES = ('root', 'jobs', 'status', 'undo', 'leaf')


def load(rel, digest, name):
    path = FIXTURES / rel
    assert sha256(path.read_bytes()).hexdigest() == digest, path
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def live_history(f, a, live):
    """The model at the captured live state, every input re-encoded."""
    h = f.live_history(a, live)
    for page_no in FREE_UNDO[1:]:
        undo = live[f'undo{page_no}.bin']
        h.undo[page_no] = [a.cell_at(undo, 0)[1], a.page_lsn(undo)]
        assert h.undo_page(page_no) == undo
    return h


def canonical(value):
    return pack('<IBq', 1, 2, value)


def pages(f, h, undo):
    return {'root': h.root_page(), 'jobs': h.jobs_page(0),
            'status': h.status_page(f.STATUS_PAGE), 'undo': h.undo_page(undo),
            'leaf': h.leaf()}


def frame(data):
    return {'bytes': len(data), 'sha256': sha256(data).hexdigest()}


def main():
    f = load(*FORWARD, 's2w_r09_forward_author')
    a = f.load_n1()
    live = {name: f.pinned(HERE / 'live' / name, digest) for name, digest in LIVE.items()}
    h = live_history(f, a, live)
    lsn = PREVIOUS_LSN
    files, steps, rows, forward = {}, [], [], 0
    for symbol, value, undo in STEPS:
        lsn += 1
        canon = canonical(value if value is not None else 0)
        reserve, ordinal = h.reserve(lsn, undo, STATEMENT, canon)
        row = a.get(h.bank[ordinal], 320)
        steps.append(dict(frame(reserve), op='reserve', lsn=lsn, undo=undo, row=row))
        lsn += 1
        if symbol is None:
            files['canonical-gap.bin'] = canon
            cancel = h.cancel(lsn, ordinal)
            steps.append(dict(frame(cancel), op='cancel', lsn=lsn, row=row))
            continue
        forward += 1
        previous = h.predecessor(h.tx['actor'], a.get(h.bank[ordinal], 168))
        change = h.row_change(f.FORWARD, lsn, ordinal)
        files[f'canonical{forward}.bin'] = canon
        files[f'forward{forward}-frame.bin'] = change
        for name, data in pages(f, h, undo).items():
            files[f'after{forward}-{name}.bin'] = data
        steps.append(dict(frame(change), op='forward', lsn=lsn, row=row, undo=undo,
                          h=None if previous is None else a.get(previous, 320)))
        rows.append([symbol, row, value])
    out = HERE / 'literal'
    out.mkdir(exist_ok=True)
    for name, data in files.items():
        (out / name).write_bytes(data)
    pins = {FORWARD[0]: FORWARD[1], f.N1[0]: f.N1[1],
            **{'dev3-module-c2/row-read-six/live/' + name: digest
               for name, digest in LIVE.items()}}
    expected = {
        'schema': 'xadb-module-row-v1',
        'source_kind': 'C+B',
        'provenance': {'author': 'dev3-module-c2/row-read-six/author.py '
                                 '(run as python3 -I author.py)',
                       'pinned_inputs': dict(sorted(pins.items())),
                       'no_xadb_code': 'no XADB encoder, decoder, A producer or B reader is run'},
        'case': 'MCB01',
        'rows': rows,
        'gap': [step['row'] for step in steps if step['op'] == 'cancel'],
        'previous_lsn': PREVIOUS_LSN,
        'statement': STATEMENT,
        'steps': steps,
        'files': {name: frame(data) for name, data in sorted(files.items())},
        'pages': {'root': 1, 'jobs': 2, 'status': f.STATUS_PAGE, 'leaf': f.LEAF_PAGE,
                  'undo': list(FREE_UNDO)},
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
