#!/usr/bin/env python3
"""S9 C+B literal rollback transitions; run as `python3 -I author.py`.

The C+B live fixture's failed statement and full rollback (consumers §8.3:
"실패 statement/CLR/Prune 관측은 C의 동일 checked ingress에 독립 literal
전이를 넣는다"). C+B links no A, so the CLR that compensates the literal
row is this literal too: C selects the rollback task, and the literal
proposes the CLR frame and its five after images through the task's
producer lane; C checks every byte and applies it. RollbackStart,
StatementFinish, AbortEnd and Commit are C's own frames; the model computes
each, and C's WAL must equal it.

The state before the rollback is the S8 literal row (`dev3-module-c2/
row-read/`): the S2 live fixture, RowReserve LSN 15 and the literal Forward
LSN 16 of RowId 1, Int64 7. The S2W-R09 author `dev3-module-c3/row-write/
forward.py` (pinned by SHA-256, imported, not copied) rebuilds it on the N1
History model (`module-row-format/transition/author.py`, pinned there) with
every live input re-encoded byte-equal; its RowReserve and Forward must
equal the pinned S2/S8 frames before anything is taken.

- `statement` (MCB05-L04): the first statement's RollbackStart (LSN 17,
  statement 1, mark None), the literal CLR (18), StatementFinish (19), then
  the transaction's Commit (20, q 2) with no row.
- `full` (MCB05-L05): the transaction RollbackStart (17) after the statement
  finished, the literal CLR (18), AbortEnd (19). These frames are the ones
  the S4 author (`dev3-module-c3/row-rollback/author.py`) pins for A's CLR
  of the same row; they are checked equal here.

Writes literal/<case>/*.bin and expected.json beside this file and prints
the json.
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
# The S8 literal Forward of the row (dev3-module-c2/row-read/expected.json).
S8_FORWARD = '0ee305722bff97a71161e2111bb7a15059b3a35b8a941c603a45e2fc5de81043'
# The S4 author's full-rollback frames (dev3-module-c3/row-rollback/expected.json).
S4_FRAMES = {
    'rollback-start': 'b4af98e28211d0ef990d2e54767b9e046fed100197b91f01f29df9b9ba156d92',
    'clr': 'caa18deac3fecd9b9c51d861c12b25aa4ca547f087a766cf2947ec3a19f9f4e0',
    'abort-end': '6d83741e6afd832e93fa0ca76eba65054f0cce56b380f7721c8e57d8fef4eaf5',
}
STATEMENT = 1
# The pages a CLR of the row changes: (file, model role, page).
AFTER = (('after-root.bin', 'root', 1), ('after-jobs.bin', 'jobs', 2),
         ('after-status.bin', 'status', 11), ('after-undo.bin', 'undo', 14),
         ('after-leaf.bin', 'leaf', 13))
CASES = {'statement': 'MCB05-L04', 'full': 'MCB05-L05'}


def load(rel, digest, name):
    path = FIXTURES / rel
    assert sha256(path.read_bytes()).hexdigest() == digest, path
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def forwarded(f, a, live):
    """The model after the S8 literal Forward: (history, root ordinal)."""
    h = f.live_history(a, live)
    reserve, ordinal = h.reserve(f.PREVIOUS_LSN + 1, f.FREE_UNDO, f.STATEMENT,
                                 pack('<IBq', 1, 2, f.VALUE))
    assert (len(reserve), sha256(reserve).hexdigest()) == f.RESERVE
    forward = h.row_change(f.FORWARD, f.PREVIOUS_LSN + 2, ordinal)
    assert sha256(forward).hexdigest() == S8_FORWARD
    return h, ordinal


def pages(f, h):
    return {'root': h.root_page(), 'jobs': h.jobs_page(0),
            'status': h.status_page(f.STATUS_PAGE), 'undo': h.undo_page(f.FREE_UNDO),
            'leaf': h.leaf()}


def history(f, a, live, case):
    """The case's frames by name (in LSN order) and the CLR's after images."""
    h, ordinal = forwarded(f, a, live)
    lsn = f.PREVIOUS_LSN + 2
    frames = {}
    if case == 'statement':
        frames['rollback-start'] = h.status_patch(3, lsn + 1, statement=STATEMENT, mark=None)
    else:
        frames['rollback-start'] = h.status_patch(3, lsn + 1)
    frames['clr'] = h.row_change(4, lsn + 2, ordinal)
    after = pages(f, h)
    if case == 'statement':
        frames['statement-finish'] = h.status_patch(5, lsn + 3, statement=STATEMENT, mark=None)
        frames['commit'] = h.end(lsn + 4, True)
    else:
        frames['abort-end'] = h.end(lsn + 3, False)
        for name, digest in S4_FRAMES.items():
            assert sha256(frames[name]).hexdigest() == digest, name
    return frames, after, a.get(h.root, 408)


def main():
    f = load(*FORWARD, 's2w_r09_forward_author')
    a = f.load_n1()
    live = {name: f.pinned(f.HERE / 'live' / name, digest) for name, digest in f.LIVE.items()}
    frame = lambda data: {'bytes': len(data), 'sha256': sha256(data).hexdigest()}
    cases = {}
    for case, case_id in CASES.items():
        frames, after, q = history(f, a, live, case)
        files = {f'{name}.bin': data for name, data in frames.items()}
        files.update({name: after[role] for name, role, _ in AFTER})
        out = HERE / 'literal' / case
        out.mkdir(parents=True, exist_ok=True)
        for name, data in files.items():
            (out / name).write_bytes(data)
        lsn = f.PREVIOUS_LSN + 3
        cases[case] = {
            'case': case_id,
            'frames': [dict(frame(data), name=name, lsn=lsn + i)
                       for i, (name, data) in enumerate(frames.items())],
            'files': {name: frame(data) for name, data in sorted(files.items())},
            'q': q,
        }
    expected = {
        'schema': 'xadb-module-row-v1',
        'source_kind': 'C+B',
        'provenance': {'author': 'dev3-module-c2/row-read-rollback/author.py '
                                 '(run as python3 -I author.py)',
                       'pinned_inputs': {FORWARD[0]: FORWARD[1], f.N1[0]: f.N1[1]},
                       'no_xadb_code': 'no XADB encoder, decoder, A producer or B reader is run'},
        'row': 1,
        'value': f.VALUE,
        'previous_lsn': f.PREVIOUS_LSN + 2,
        'pages': {name: page for name, _, page in AFTER},
        'cases': cases,
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
