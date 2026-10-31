#!/usr/bin/env python3
"""S2W-R09 independent Forward literal; run as `python3 -I forward.py`.

The live fixture is the one author.py states (catalog_memory/row_write.rs):
table T (object 6), the Row actor on Status 11, its first statement, the
Free Undo 14 and one row of T with the single Int64 value 7. Its RowReserve
is LSN 15 and the Forward of that reservation (S2-c,
catalog_memory/row_forward.rs) is the next group, LSN 16.

Both frames are computed by the N1 author's History model
(module-row-format/transition/author.py, pinned by SHA-256 and imported, not
copied), which no XADB encoder, decoder or test helper reaches. The live
state is read from live/*.bin under the pins author.py states, and every
input is re-encoded by the model and required byte-equal before use (codec
cross-check). The model's RowReserve must equal expected.json's frame
before its Forward is taken. Only the facts author.py sets are set here.

Writes expected-forward.json beside this file and prints it.
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
N1 = ('module-row-format/transition/author.py',
      '076ea2f2767851bd8046becbba6b1d05a00a6cd7838a9c9e052b6b2e033beec4')
LIVE = {
    'root1.bin': '700ee5b8e8ceb843789fad495a6bade5917c6d92ae21f3d2e4d2abdcd6b156bc',
    'jobs2.bin': 'a2aa825f6e3af848a07559ea6d15bc590825c28127e4862c80c13835d7b32312',
    'status11.bin': '9d52d6d97261aec98b2050edba42173bbb385a56e90abb9ade352bb8a3e6c4b5',
    'leaf13.bin': '30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396',
    'undo14.bin': '7fe328b11b1d505fa2f35c0297e5cfcc369143ea7e8272298d564fe751774eab',
    'bank.bin': '1f89dcdb2e244f6d2cbf33cf72a030df2daf18b72e8f7cd9b3a463de18382bdb',
    'tx.bin': '20abdcb33ae7095e2c6a335b5f3a5020bd13f53668c2a8ab1ba8b4eefcf3c3a6',
    'aging.bin': '7670867ac62398be66282adf9f8b670c73d56c571c700919a232f5d489f4b266',
    'profile.bin': 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e',
}
# expected.json's RowReserve frame, which the model must reproduce first.
RESERVE = (57_336, '6e3badc82e1864c405f846a05372e2367f6a266a10e0a4e0c8c3d28267870fbe')
PREVIOUS_LSN = 14
STATEMENT = 1
STATUS_PAGE = 11
FREE_UNDO = 14
LEAF_PAGE = 13
VALUE = 7
FORWARD = 2


def pinned(path, digest):
    data = path.read_bytes()
    assert sha256(data).hexdigest() == digest, path
    return data


def load_n1():
    path = FIXTURES / N1[0]
    pinned(path, N1[1])
    spec = importlib.util.spec_from_file_location('n1_transition_author', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def live_history(a, live):
    """The model at the live fixture, every input re-encoded byte-equal."""
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
    h.status[STATUS_PAGE] = [a.cell_at(status, 0)[1], a.page_lsn(status)]
    undo = live['undo14.bin']
    h.undo[FREE_UNDO] = [a.cell_at(undo, 0)[1], a.page_lsn(undo)]
    leaf = live['leaf13.bin']
    h.leaf_lsn = a.page_lsn(leaf)
    for slot in range(1, a.get(leaf, 72, 2)):
        cell = a.cell_at(leaf, slot)[1]
        h.rows[int.from_bytes(cell[4:12], 'big')] = cell[12:]
    tx, aging = live['tx.bin'], live['aging.bin']
    h.tx = dict(actor=tx[24:72], status=STATUS_PAGE,
                chain=[a.get(tx, 72), a.get(tx, 80), a.get(tx, 88), a.get(tx, 624)],
                inventory=(a.get(tx, 544), a.get(tx, 552), tx[560:592]),
                consumed=a.get(tx, 608), mutation=a.get(tx, 1344),
                credit=a.get(tx, 640 + 3 * 128 + 8) == 3)
    h.aging = [a.get(aging, 72), a.get(aging, 80), a.get(aging, 88), a.get(aging, 624)]
    assert h.root_page() == root
    assert b''.join(map(bytes, h.bank)) == bank
    assert h.jobs_page(0) == live['jobs2.bin']
    assert h.status_page(STATUS_PAGE) == status
    assert h.undo_page(FREE_UNDO) == undo
    assert h.leaf() == leaf
    assert h.tx_descriptor() == tx
    assert h.aging_descriptor() == aging
    assert h.status_core()[8:56] == h.tx['actor']
    return h


def main():
    a = load_n1()
    live = {name: pinned(HERE / 'live' / name, digest) for name, digest in LIVE.items()}
    h = live_history(a, live)
    canonical = pack('<IBq', 1, 2, VALUE)
    reserve, ordinal = h.reserve(PREVIOUS_LSN + 1, FREE_UNDO, STATEMENT, canonical)
    assert (len(reserve), sha256(reserve).hexdigest()) == RESERVE
    frame = h.row_change(FORWARD, PREVIOUS_LSN + 2, ordinal)
    job = h.bank[ordinal]
    expected = {
        'case': 'S2W-R09',
        'lsn': PREVIOUS_LSN + 2,
        'frame_bytes': len(frame),
        'frame_sha256': sha256(frame).hexdigest(),
        'root_job': {'ordinal': ordinal, 'state': job[6], 'generation': a.get(job, 8),
                     'statement': a.get(job, 248), 'lease_lsn': a.get(job, 64),
                     'adoption_lsn': a.get(job, 72)},
        'row': a.get(job, 320),
        'after_sha256': {'root1': sha256(h.root_page()).hexdigest(),
                         'jobs2': sha256(h.jobs_page(0)).hexdigest(),
                         'status11': sha256(h.status_page(STATUS_PAGE)).hexdigest(),
                         'undo14': sha256(h.undo_page(FREE_UNDO)).hexdigest(),
                         'leaf13': sha256(h.leaf()).hexdigest(),
                         'bank': sha256(b''.join(map(bytes, h.bank))).hexdigest(),
                         'tx': sha256(h.tx_descriptor()).hexdigest(),
                         'aging': sha256(h.aging_descriptor()).hexdigest()},
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected-forward.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
