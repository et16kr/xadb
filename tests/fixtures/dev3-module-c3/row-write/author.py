#!/usr/bin/env python3
"""S2W-R06 independent RowReserve literal; run as `python3 -I author.py`.

The live fixture of the S2-b2 Row write cases (catalog_memory/row_write.rs):
table T (object 6) committed by the Catalog actor, the Row actor begun on
Status 11, its first statement, the Free Undo 14, and one row of T with the
single Int64 value 7. Its RowReserve is the next group, LSN 15.

The frame is computed by the N1 author's History model
(module-row-format/transition/author.py, pinned by SHA-256 and imported, not
copied), which no XADB encoder, decoder or test helper reaches. The live
state it starts from is read from live/*.bin, each pinned by SHA-256 here and
again by the Rust case against the live pool and control, and every one of
them is re-encoded by the model and required byte-equal before use (codec
cross-check). Only the facts that differ from the N1 history are set here:
the live DatabaseId (1..16), the live Profile, the actor's Status page 11,
the ATT slot 0 of the Row actor and the canonical row of T's one Int64
column, encoded by N1's canonical layout (column count, type tag 2, i64).

Writes expected.json beside this file and prints it.
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
# The live fixture's scalars: the WAL complete LSN before the reserve, the
# statement sequence, the Status and Free Undo pages and T's value.
PREVIOUS_LSN = 14
STATEMENT = 1
STATUS_PAGE = 11
FREE_UNDO = 14
VALUE = 7


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


def main():
    a = load_n1()
    live = {name: pinned(HERE / 'live' / name, digest) for name, digest in LIVE.items()}
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
    # Codec cross-check: the model re-encodes every live input byte-equal.
    assert h.root_page() == root
    assert b''.join(map(bytes, h.bank)) == bank
    assert h.jobs_page(0) == live['jobs2.bin']
    assert h.status_page(STATUS_PAGE) == status
    assert h.undo_page(FREE_UNDO) == undo
    assert h.leaf() == leaf
    assert h.tx_descriptor() == tx
    assert h.aging_descriptor() == aging
    assert h.status_core()[8:56] == h.tx['actor']
    canonical = pack('<IBq', 1, 2, VALUE)
    root_cursor = a.get(h.root, 72)
    frame, ordinal = h.reserve(PREVIOUS_LSN + 1, FREE_UNDO, STATEMENT, canonical)
    job = h.bank[ordinal]
    expected = {
        'case': 'S2W-R06',
        'lsn': PREVIOUS_LSN + 1,
        'frame_bytes': len(frame),
        'frame_sha256': sha256(frame).hexdigest(),
        'canonical_sha256': sha256(canonical).hexdigest(),
        'root_cursor_before': root_cursor,
        'root_job': {'ordinal': ordinal, 'jobs_page': 2 + ordinal // 15,
                     'slot': ordinal % 15, 'generation': a.get(job, 8)},
        'row': a.get(job, 320),
        'undo': {'page': FREE_UNDO, 'slot': 0, 'generation': a.get(job, 160)},
        'sequence': a.get(job, 168),
        'mutation': a.get(job, 256),
        'after_sha256': {'root1': sha256(h.root_page()).hexdigest(),
                         'jobs2': sha256(h.jobs_page(0)).hexdigest(),
                         'undo14': sha256(h.undo_page(FREE_UNDO)).hexdigest(),
                         'bank': sha256(b''.join(map(bytes, h.bank))).hexdigest(),
                         'tx': sha256(h.tx_descriptor()).hexdigest(),
                         'aging': sha256(h.aging_descriptor()).hexdigest()},
    }
    text = json.dumps(expected, indent=2, sort_keys=True) + '\n'
    (HERE / 'expected.json').write_text(text)
    print(text, end='')


if __name__ == '__main__':
    main()
