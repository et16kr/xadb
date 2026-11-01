#!/usr/bin/env python3
"""S9 C+B two-root literal ingress input; run as `python3 -I author.py`.

The S8 literal (`../row-read/author.py`) has one row, so the known-bad
facts that need an actual predecessor H could not be stated (K07, and the
original forms of K04 and K05). This history gives the same Row actor two
roots in one statement: C's actual Begin and two RowReserves issue them,
and this literal proposes both canonical rows and both Forwards. The second
Forward has the first root's U as its actual predecessor H. Nothing here is
A's encoder, INSERT or CLR, and no XADB encoder or decoder runs.

The live state is C's actual state before the first RowReserve, captured
from the memory writer of `catalog_memory/row_literal_two.rs` and pinned
here by SHA-256 (the S2 live fixture precedent): table T (object 6), the
Row actor on Status 11, its first statement, RowLeaf 13 empty, and two Free
Undo containers, 14 (the S2 one) and 15 (one more actual Undo INIT before
the Begin, LSN 15–16). The Rust case checks the pool and control against
the same pins before it reserves. The N1 History model
(`module-row-format/transition/author.py`, pinned by the S2W-R09 author
`dev3-module-c3/row-write/forward.py`, itself pinned here) re-encodes every
live input byte-equal before use. The model computes both RowReserves,
which C's WAL must equal, and both Forwards:

  LSN 17 RowReserve (root 1, Undo 14, RowId 1, mutation 1)  — C
  LSN 18 Forward    (root 1, value 7, no H)                 — this literal
  LSN 19 RowReserve (root 2, Undo 15, RowId 2, mutation 2)  — C
  LSN 20 Forward    (root 2, value 8, H = root 1's U)       — this literal

Known-bad (consumers §8.3, each one semantic change to the second Forward,
made in the model as S8 did, the before images and both descriptors handed
to `row_change` as its own first reads):

  K07  a CountChange of H with before 0 and after 0 (the actual H keeps its
       incoming count across the move, so the actual frame has no H count)
  K04  the Forward consumes the first root's mutation 1 again
  K05  a different H of the same size (the actual H's generation + 1)
  K01  the U target's table generation 2, the Job, record and projection
       keep T generation 1 (target only)

Each K is stored as a byte patch of the files it changes (known-bad/Kxx.patch,
the S8 format), with the patched file's length and SHA-256. A K that changes
a frame's length stores the frame whole beside it. A refused K leaves the
second reservation to its Cancel, which this model also computes (LSN 20).

Writes live pins check, literal/*.bin, known-bad/*.patch and expected.json
beside this file and prints the json.
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
# The S8 author, for its known-bad helpers (patched, runs, patch_text).
S8 = ('dev3-module-c2/row-read/author.py',
      'eda4a8a043a6393efe734e978a5aa14f28d0ce5c3a4624b8e5bec068e9733fe7')
LIVE = {
    'root1.bin': '1a036fc2744150d2219461abd250728fc9111cc5f758ee8025888743ab1533ae',
    'jobs2.bin': '20c699c3ecf493a253f8719c4d9db3392323d25d1e29bb6a7f612d691b08a3ea',
    'status11.bin': 'dbdffac2932ae3f46ae046217f36ff7656d17a88f710652017092f21f9076860',
    'leaf13.bin': '30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396',
    'undo14.bin': '7fe328b11b1d505fa2f35c0297e5cfcc369143ea7e8272298d564fe751774eab',
    'undo15.bin': 'd826cb72764ce486fcc4f9346b6ce6e93a7d967bc1e24d285dd6b4f80acac31d',
    'bank.bin': '90a616fc3dace26ea94ff705ec46e28d846292b54d7bbb3cb6a6d85b9ec07efe',
    'tx.bin': '3d1838767a9da54b61a3c86b46dfb8153c645c5504e2d44cb438237e9403fd51',
    'aging.bin': 'a35e819253d8a82d64cfc087a24c68bae014cec2874fd7b6dfefb1f11cecd8af',
    'profile.bin': 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e',
}
# The live fixture's scalars: the WAL complete LSN before the first
# RowReserve, the statement, the two Free Undo containers (C reserves the
# lower first) and T's two values.
PREVIOUS_LSN = 16
STATEMENT = 1
UNDO = (14, 15)
VALUES = (7, 8)
# The pages a Forward of root r changes: (file suffix, model role, page).
ROLES = (('root', 'root'), ('jobs', 'jobs'), ('status', 'status'), ('undo', 'undo'),
         ('leaf', 'leaf'))


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
    undo = live['undo15.bin']
    h.undo[UNDO[1]] = [a.cell_at(undo, 0)[1], a.page_lsn(undo)]
    assert h.undo_page(UNDO[1]) == undo
    return h


def canonical(value):
    return pack('<IBq', 1, 2, value)


def pages(f, h, undo):
    return {'root': h.root_page(), 'jobs': h.jobs_page(0),
            'status': h.status_page(f.STATUS_PAGE), 'undo': h.undo_page(undo),
            'leaf': h.leaf()}


def first_root(f, a, live):
    """Root 1: its RowReserve (C's) and Forward (this literal)."""
    h = live_history(f, a, live)
    reserve1, ordinal1 = h.reserve(PREVIOUS_LSN + 1, UNDO[0], STATEMENT, canonical(VALUES[0]))
    forward1 = h.row_change(f.FORWARD, PREVIOUS_LSN + 2, ordinal1)
    files = {'forward1-frame.bin': forward1,
             **{f'after1-{name}.bin': data for name, data in pages(f, h, UNDO[0]).items()}}
    reserve2, ordinal2 = h.reserve(PREVIOUS_LSN + 3, UNDO[1], STATEMENT, canonical(VALUES[1]))
    return h, (reserve1, ordinal1), (reserve2, ordinal2), files


def second_forward(f, a, live, change=None):
    """Root 2's Forward with `change` (None: the literal). The before images
    and both descriptors are taken first and handed to `row_change` as its
    own first reads, so a change reaches only the after state and the frame;
    `change(h, ordinal, canonical, stash)` returns its restores."""
    h, _, (_, ordinal), _ = first_root(f, a, live)
    roles = [('root', 0), ('jobs', ordinal // 15), ('status', f.STATUS_PAGE),
             ('undo', UNDO[1]), ('leaf', 0)]
    first = {'snapshot': h.snapshot(roles), 'tx_descriptor': h.tx_descriptor(),
             'aging_descriptor': h.aging_descriptor()}
    for name, value in first.items():
        original = getattr(h, name)
        setattr(h, name, lambda *args, _o=original, _v=[value]: _v.pop() if _v else _o(*args))
    stash = {}
    restores = (change(h, ordinal, canonical(VALUES[1]), stash) if change else None) or []
    try:
        frame = h.row_change(f.FORWARD, PREVIOUS_LSN + 4, ordinal)
    finally:
        for restore in restores:
            restore()
    files = {'forward2-frame.bin': stash.get('frame', frame),
             **{f'after2-{name}.bin': data for name, data in pages(f, h, UNDO[1]).items()}}
    return h, ordinal, files


# ---- known-bad ---------------------------------------------------------------

def k07(s8, a):  # a CountChange of H, before 0 and after 0 (net-0 move)
    """The frame gains one 96 B count; the transaction descriptor after
    charges the longer frame, so the frame is consistent except for the
    stated count."""
    def change(h, ordinal, canon, stash):
        emit = h.emit

        def with_h(op, lsn, roles, before, txn=None, owners=None, core=None, aux=(),
                   items=0, counts=0, edges=0):
            frame = emit(op, lsn, roles, before, txn, owners, core, aux, items, counts, edges)
            kept, consumed = h.tx['inventory'], h.tx['consumed']
            h.tx['consumed'] = consumed + 96
            descriptor = h.tx_descriptor()
            h.tx['consumed'] = consumed
            assert h.tx['inventory'] == kept
            befores, afters = owners
            previous = h.predecessor(h.tx['actor'], a.get(h.bank[ordinal], 168))
            target = a.target_undo(previous[128:168] + previous[168:176])

            def aux_h(at):
                out = aux(at)
                edges_, rest = out[:edges], out[edges:]
                return edges_ + rest[:counts] + [a.count(target, 0, 0, at['undo'])] + rest[counts:]
            stash['frame'] = emit(op, lsn, roles, before, txn, (befores, [descriptor, afters[1]]),
                                  core, aux_h, items, counts + 1, edges)
            assert len(stash['frame']) == len(frame) + 96
            return frame
        h.emit = with_h
        return [lambda: setattr(h, 'emit', emit)]
    return change


def k04(s8, a):  # the Forward consumes the first root's mutation 1 again
    def change(h, ordinal, canon, stash):
        job = h.bank[ordinal]
        assert a.get(job, 256) == 2
        a.put(job, 256, 1)
        s8.relink(a, h, job, canon)
    return change


def k05(s8, a):  # a different H of the same size (actual H generation + 1)
    def change(h, ordinal, canon, stash):
        original = h.predecessor

        def other(actor, sequence):
            actual = original(actor, sequence)
            assert actual is not None
            fake = bytearray(actual)
            fake[128:168] = a.slot_ref(a.get(actual, 136), a.get(actual, 160) + 1)
            return fake
        h.predecessor = other
        return [lambda: setattr(h, 'predecessor', original)]
    return change


def k01(s8, a):  # the U target's table generation 2; Job, record, projection keep 1
    def change(h, ordinal, canon, stash):
        def material(original):
            def foreign(job, live, incoming, edge, link=None, cursor=0):
                out = bytearray(original(job, live, incoming, edge, link, cursor))
                if live:
                    assert a.get(out, 104) == a.TABLE[1]
                    a.put(out, 104, a.TABLE[1] + 1)
                return bytes(out)
            return foreign
        return [s8.patched(a, 'undo_material', material)]
    return change


KNOWN_BAD = [
    ('K07', 'a CountChange of the actual H with before 0 and after 0 (net-0 move)', k07),
    ('K04', "the second Forward consumes the first root's mutation 1 again", k04),
    ('K05', 'a different H of the same size (the actual H generation + 1)', k05),
    ('K01', "the U target's table generation 2; Job, record and projection keep T 1", k01),
]


def main():
    f = load(*FORWARD, 's2w_r09_forward_author')
    s8 = load(*S8, 's8_row_read_author')
    a = f.load_n1()
    live = {name: f.pinned(HERE / "live" / name, digest)
            for name, digest in LIVE.items()}
    h, (reserve1, _), (reserve2, ordinal2), first = first_root(f, a, live)
    h2, ordinal, second = second_forward(f, a, live)
    assert ordinal == ordinal2
    # The actual predecessor: the second Forward's H is root 1's U.
    job1 = [job for job in h2.bank if a.get(job, 320) == 1][0]
    assert h2.predecessor(h2.tx['actor'], a.get(h2.bank[ordinal], 168))[128:168] == job1[128:168]
    files = {'canonical1.bin': canonical(VALUES[0]), 'canonical2.bin': canonical(VALUES[1]),
             **first, **second}
    out = HERE / 'literal'
    out.mkdir(exist_ok=True)
    for name, data in files.items():
        (out / name).write_bytes(data)
    valid = dict(files)
    # Control: the first-read hand-over with no change is the literal.
    _, _, control = second_forward(f, a, live, lambda h, o, c, s: None)
    assert {**first, **control} == {**first, **second}
    # The Cancel of the second reservation (a refused K leaves it to this).
    hc, _, (_, ordinal_c), _ = first_root(f, a, live)
    cancel = hc.cancel(PREVIOUS_LSN + 4, ordinal_c)
    bad = HERE / 'known-bad'
    bad.mkdir(exist_ok=True)
    known = {}
    for case, what, build in KNOWN_BAD:
        _, _, variant = second_forward(f, a, live, build(s8, a))
        variant = {**valid, **variant}
        assert variant != valid, case
        text = s8.patch_text(case, what, valid, variant, bad)
        (bad / f'{case}.patch').write_text(text)
        known[case] = {
            'what': what,
            'patch': {'bytes': len(text.encode()), 'sha256': sha256(text.encode()).hexdigest()},
            'files': {name: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
                      for name, data in sorted(variant.items()) if data != valid[name]},
        }
    pins = {FORWARD[0]: FORWARD[1], S8[0]: S8[1], f.N1[0]: f.N1[1],
            **{'dev3-module-c2/row-read-two/live/' + name: digest
               for name, digest in LIVE.items()}}
    frame = lambda data: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
    expected = {
        'schema': 'xadb-module-row-v1',
        'source_kind': 'C+B',
        'provenance': {'author': 'dev3-module-c2/row-read-two/author.py '
                                 '(run as python3 -I author.py)',
                       'pinned_inputs': dict(sorted(pins.items())),
                       'no_xadb_code': 'no XADB encoder, decoder, A producer or B reader is run'},
        'case': 'MCB05-L02',
        'rows': [[1, VALUES[0]], [2, VALUES[1]]],
        'previous_lsn': PREVIOUS_LSN,
        'reserve': [dict(frame(reserve1), lsn=PREVIOUS_LSN + 1, undo=UNDO[0]),
                    dict(frame(reserve2), lsn=PREVIOUS_LSN + 3, undo=UNDO[1])],
        'forward_lsn': [PREVIOUS_LSN + 2, PREVIOUS_LSN + 4],
        'files': {name: frame(data) for name, data in sorted(files.items())},
        'pages': {'root': 1, 'jobs': 2, 'status': f.STATUS_PAGE, 'undo': list(UNDO),
                  'leaf': f.LEAF_PAGE},
        'known_bad': known,
        'cancel2': dict(frame(cancel), lsn=PREVIOUS_LSN + 4),
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
