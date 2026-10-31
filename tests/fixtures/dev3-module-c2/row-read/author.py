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

Known-bad (MCB05-K, consumers §8.3): each K is one semantic change to that
Forward, made in the model before or around `row_change` so that every byte
the change implies (cells, natives, redo, descriptors, digests, the frame
SHA) is re-encoded consistently and only the stated fact is wrong. The
before images and descriptors stay the actual pre-Forward ones. A K is
stored as a byte patch of each literal file it changes, with the patched
file's length and SHA-256 (known-bad/Kxx.patch); K07 needs an actual
predecessor H, which this one-row history does not have, and is recorded
not run.

Writes literal/*.bin, known-bad/*.patch and expected.json beside this file
and prints the json.
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
# The S2 Cancel literal of the same reservation: a K's refusal must leave
# the reservation to this Cancel (S2W-R08).
CANCEL = ('dev3-module-c3/row-write/expected-cancel.json',
          'a66479c326b77dcccf4b9ae63e5e83a1e3eaa6b079adcea0bb4570a56aa9dc36')
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


def reserved(f, a, live):
    """The live model after the pinned RowReserve: (history, ordinal, row)."""
    h = f.live_history(a, live)
    canonical = pack('<IBq', 1, 2, f.VALUE)
    reserve, ordinal = h.reserve(f.PREVIOUS_LSN + 1, f.FREE_UNDO, f.STATEMENT, canonical)
    assert (len(reserve), sha256(reserve).hexdigest()) == f.RESERVE
    return h, ordinal, canonical


def outputs(f, h, frame):
    pages = {'root': h.root_page(), 'jobs': h.jobs_page(0),
             'status': h.status_page(f.STATUS_PAGE), 'undo': h.undo_page(f.FREE_UNDO),
             'leaf': h.leaf()}
    files = {'forward-frame.bin': frame}
    for name, role, _ in AFTER:
        files[name] = pages[role]
    return files


def forward(f, a, live):
    h, ordinal, canonical = reserved(f, a, live)
    frame = h.row_change(f.FORWARD, f.PREVIOUS_LSN + 2, ordinal)
    return h, ordinal, canonical, outputs(f, h, frame)


# ---- known-bad ---------------------------------------------------------------

def patched(a, name, replacement):
    """Set module global `name` of the model; returns the restore."""
    original = getattr(a, name)
    setattr(a, name, replacement(original) if callable(original) else replacement)
    return lambda: setattr(a, name, original)


def changed(f, a, live, change):
    """The Forward with one change. The pre-Forward before images and both
    descriptors are taken first and handed to `row_change` as its own
    first reads, so the change reaches only the after state and the frame;
    `change(h, ordinal, canonical)` returns the restores it needs."""
    h, ordinal, canonical = reserved(f, a, live)
    roles = [('root', 0), ('jobs', ordinal // 15), ('status', f.STATUS_PAGE),
             ('undo', f.FREE_UNDO), ('leaf', 0)]
    first = {'snapshot': h.snapshot(roles), 'tx_descriptor': h.tx_descriptor(),
             'aging_descriptor': h.aging_descriptor()}
    for name, value in first.items():
        original = getattr(h, name)
        setattr(h, name, lambda *args, _o=original, _v=[value]: _v.pop() if _v else _o(*args))
    stash = {}
    restores = change(h, ordinal, canonical, stash) or []
    try:
        frame = h.row_change(f.FORWARD, f.PREVIOUS_LSN + 2, ordinal)
    finally:
        for restore in restores:
            restore()
    return outputs(f, h, stash.get('frame', frame)), h, ordinal


def relink(a, h, job, canonical, row=None):
    """The Job's planned projection for the record its fields now imply."""
    row = a.get(job, 320) if row is None else row
    value = a.record(row, canonical, h.tx['actor'], a.get(job, 256), bytes(job[128:168]))
    job[288:320] = a.projection(row, value)


def k01(a):  # foreign table generation 1 -> 2 in the Job, record target and digests
    def change(h, ordinal, canonical, stash):
        job = h.bank[ordinal]
        a_table = a.TABLE
        restore = patched(a, 'TABLE', (a_table[0], a_table[1] + 1, a_table[2]))
        a.put(job, 184, a_table[1] + 1)
        relink(a, h, job, canonical)
        return [restore]
    return change


def k02(a):  # forged RowId 2 (the Reserve issued 1) in Job, record, leaf key, U
    def change(h, ordinal, canonical, stash):
        job = h.bank[ordinal]
        a.put(job, 320, 2)
        relink(a, h, job, canonical)
    return change


def k03(a):  # U generation +1 in the Job (record undo, Status head, U cell)
    def change(h, ordinal, canonical, stash):
        job = h.bank[ordinal]
        a.put(job, 160, a.get(job, 160) + 1)
        relink(a, h, job, canonical)
    return change


def k04(a):  # the Forward plans its mutation again: descriptor last 1 -> 2
    def change(h, ordinal, canonical, stash):
        h.tx['mutation'] += 1
    return change


def k05(a):  # predecessor H claimed (Linked edge, next) where the actual is none
    def change(h, ordinal, canonical, stash):
        job = h.bank[ordinal]
        fake = bytearray(1024)
        fake[128:168] = a.slot_ref(15, 1)
        a.put(fake, 168, a.get(job, 168) - 1)
        original = h.predecessor
        h.predecessor = lambda actor, sequence: fake
        return [lambda: setattr(h, 'predecessor', original)]
    return change


def k06(a):  # the Live U's incoming count 1 (the graph adds 2)
    def change(h, ordinal, canonical, stash):
        return [patched(a, 'undo_material', lambda undo_material: (
            lambda job, live, incoming, edge, link=None, cursor=0:
            undo_material(job, live, 1 if live else incoming, edge, link, cursor)))]
    return change


def k08(a):  # the row's Transaction stamp is another transaction (TxnId local +1)
    def change(h, ordinal, canonical, stash):
        actor = h.tx['actor']
        other = actor[:32] + a.u64(a.get(actor, 32) + 1) + actor[40:]

        def record(original):
            def stamped(row, canon, who, mutation, undo):
                out = bytearray(original(row, canon, who, mutation, undo))
                out[24:72] = other
                return bytes(out)
            return stamped

        def projection(original):
            def planned(row, value):
                if value is not None and value[24:72] == other:
                    value = value[:24] + actor + value[72:]
                return original(row, value)
            return planned
        return [patched(a, 'record', record), patched(a, 'projection', projection)]
    return change


def k10(a):  # the Forward leaves the Reserved inventory item unconsumed
    """No InventoryDelta, and the transaction descriptor after keeps the
    Reserved inventory (change LSN, one item) and charges the 208 B shorter
    frame, so the frame is consistent except that the Forward does not
    consume its Reserved item."""
    def change(h, ordinal, canonical, stash):
        emit = h.emit
        reserved = h.tx['inventory']

        def without(op, lsn, roles, before, txn=None, owners=None, core=None, aux=(),
                    items=0, counts=0, edges=0):
            frame = emit(op, lsn, roles, before, txn, owners, core, aux, items, counts, edges)
            kept, consumed = h.tx['inventory'], h.tx['consumed']
            h.tx['inventory'], h.tx['consumed'] = reserved, consumed - 208
            descriptor = h.tx_descriptor()
            h.tx['inventory'], h.tx['consumed'] = kept, consumed
            befores, afters = owners
            stash['frame'] = emit(op, lsn, roles, before, txn, (befores, [descriptor, afters[1]]),
                                  core, lambda at: aux(at)[:-1], 0, counts, edges)
            assert len(stash['frame']) == len(frame) - 208
            return frame
        h.emit = without
        return [lambda: setattr(h, 'emit', emit)]
    return change


def k11(a):  # the actor's full-abort (CLR) WAL debt one byte short
    def change(h, ordinal, canonical, stash):
        def rows(original):
            def short(*args):
                out = [list(row) for row in original(*args)]
                assert out[0][0] == 1
                out[0][1] -= 1
                return out
            return short
        return [patched(a, 'tx_rows', rows)]
    return change


def redo_of(a, frame, page_no):
    at = a.sections(frame)['redo']
    while at < len(frame) - 32:
        if a.get(frame, at + 16) == page_no:
            return at
        at += a.get(frame, at + 4, 4)
    raise ValueError(page_no)


def k09(f, a, files):  # one canonical byte of the leaf redo differs from native
    frame = bytearray(files['forward-frame.bin'])
    leaf = files['after-leaf.bin']
    offset, cell = a.cell_at(leaf, 1)
    redo = redo_of(a, frame, f.LEAF_PAGE)
    assert a.get(frame, redo + 56, 2) == 72
    frame[redo + 64 + offset + 12 + 168 - 72] ^= 1
    return dict(files, **{'forward-frame.bin': a.reseal(frame)})


def k12(f, a, files):  # the group declares scope 2 (old format), frame resealed
    frame = bytearray(files['forward-frame.bin'])
    assert a.get(frame, 160 + 10, 2) == 3
    a.put(frame, 160 + 10, 2, 2)
    return dict(files, **{'forward-frame.bin': a.reseal(frame)})


KNOWN_BAD = [
    ('K01', 'foreign table generation (Job, U target, projection)', k01),
    ('K02', 'forged RowId 2, the Reserve issued 1', k02),
    ('K03', 'wrong U generation', k03),
    ('K04', 'planned mutation consumed again (descriptor last mutation 2)', k04),
    ('K05', 'predecessor H claimed, the actual predecessor is none', k05),
    ('K06', 'U incoming 1 (the graph gives 2)', k06),
    ('K08', 'row Transaction stamp is another transaction (TxnId local +1)', k08),
    ('K09', 'leaf redo row bytes differ from the native after cell', 'k09'),
    ('K10', 'Reserved inventory item left unconsumed (no InventoryDelta, descriptor keeps it)', k10),
    ('K11', 'full-abort CLR WAL debt decremented by one', k11),
    ('K12', 'the valid Row frame declared as a scope2 group (scope field only)', 'k12'),
]
NOT_RUN = {'K07': 'H before count 0 with a net-0 move needs an actual predecessor H '
                  '(a second root of the same actor); this one-row history has none'}


def runs(base, new):
    """Patch ops (start, end, replacement) of an equal-length change: the
    differing byte runs, joined across gaps under 16 bytes."""
    ops = []
    for at in (i for i in range(len(base)) if base[i] != new[i]):
        if ops and at - ops[-1][1] < 16:
            ops[-1][1] = at + 1
        else:
            ops.append([at, at + 1])
    return [(start, end, new[start:end]) for start, end in ops]


def apply(base, ops):
    out, at = bytearray(), 0
    for start, end, replacement in ops:
        out += base[at:start] + replacement
        at = end
    return bytes(out + base[at:])


def patch_text(case, what, valid, files, bad):
    """A file of the same length is stored as byte runs over the literal; a
    file whose length changes (K05, K10 frames) is stored whole beside it."""
    lines = [f'XADB-S8-KNOWN-BAD-V1 {case}', f'# {what}']
    for name in sorted(files):
        data = files[name]
        if data == valid[name]:
            continue
        head = f'file {name} {len(data)} {sha256(data).hexdigest()}'
        if len(data) != len(valid[name]):
            whole = f'{case}-{name}'
            (bad / whole).write_bytes(data)
            lines.append(f'{head} whole {whole}')
            continue
        ops = runs(valid[name], data)
        assert apply(valid[name], ops) == data
        lines.append(head)
        lines += [f'{start} {end} {replacement.hex()}' for start, end, replacement in ops]
    lines.append('end')
    return '\n'.join(lines) + '\n'


def load_json(rel, digest):
    data = (FIXTURES / rel).read_bytes()
    assert sha256(data).hexdigest() == digest, rel
    return data.decode()


def main():
    f = load(*FORWARD, 's2w_r09_forward_author')
    a = f.load_n1()
    live = {name: f.pinned(f.HERE / 'live' / name, digest) for name, digest in f.LIVE.items()}
    h, ordinal, canonical, files = forward(f, a, live)
    out = HERE / 'literal'
    out.mkdir(exist_ok=True)
    files = {'canonical.bin': canonical, **files}
    for name, data in files.items():
        (out / name).write_bytes(data)
    valid = dict(files)
    # Control: the capture and hand-over in `changed` with no change gives
    # the literal byte-equal, so a K differs only by its own change.
    control, _, _ = changed(f, a, live, lambda h, ordinal, canonical, stash: None)
    assert {'canonical.bin': canonical, **control} == valid
    bad = HERE / 'known-bad'
    bad.mkdir(exist_ok=True)
    known = {}
    for case, what, build in KNOWN_BAD:
        if isinstance(build, str):
            variant = globals()[build](f, a, valid)
        else:
            variant, _, _ = changed(f, a, live, build(a))
            variant = {'canonical.bin': canonical, **variant}
        assert variant != valid, case
        text = patch_text(case, what, valid, variant, bad)
        (bad / f'{case}.patch').write_text(text)
        known[case] = {
            'what': what,
            'patch': {'bytes': len(text.encode()), 'sha256': sha256(text.encode()).hexdigest()},
            'files': {name: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
                      for name, data in sorted(variant.items()) if data != valid[name]},
        }
    cancel = json.loads(load_json(*CANCEL))
    pins = {FORWARD[0]: FORWARD[1], f.N1[0]: f.N1[1], CANCEL[0]: CANCEL[1],
            **{'dev3-module-c3/row-write/live/' + name: digest for name, digest in f.LIVE.items()}}
    expected = {
        'schema': 'xadb-module-row-v1',
        'source_kind': 'C+B',
        'provenance': {'author': 'dev3-module-c2/row-read/author.py (run as python3 -I author.py)',
                       'pinned_inputs': dict(sorted(pins.items())),
                       'no_xadb_code': 'no XADB encoder, decoder, A producer or B reader is run'},
        'case': 'MCB05-L01',
        'row': a.get(h.bank[ordinal], 320),
        'value': f.VALUE,
        'reserve_lsn': f.PREVIOUS_LSN + 1,
        'forward_lsn': f.PREVIOUS_LSN + 2,
        'canonical_sha256': sha256(canonical).hexdigest(),
        'files': {name: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
                  for name, data in sorted(files.items())},
        'pages': {name: page for name, _, page in AFTER},
        'known_bad': known,
        'known_bad_not_run': NOT_RUN,
        'cancel': {'source': CANCEL[0], 'lsn': cancel['lsn'],
                   'frame_bytes': cancel['frame_bytes'],
                   'frame_sha256': cancel['frame_sha256']},
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
