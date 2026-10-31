"""Profile3 Catalog scope2 CREATE serializer for stdlib-only generators.

`load()` executes the hash-pinned version2 serializer
`dev3-catalog-create/transition/generate_literals.py` as a fresh module object
and replaces only its three version-bearing primitives:

* profile()   -> the Profile3 ID1 envelope (version3 header, resealed);
* free_job()  -> Job3 cells, so every job() cell is also Job3;
* root_page() -> Root3 cells with row_high@432 taken from state key
                 'row_high' (default 0; a genesis Root must keep 0), and the
                 Job3 census aggregate (additive debt summed, peak maximized)
                 as the budget digest and returned aggregate. A Catalog-only
                 bank keeps the version2 aggregate exactly (asserted); a bank
                 with Row-scope cells gets their registry16 peak.

It adds row_job() for Row-scope Job3 cells (kind1 leased Terminal or kind3
Reserved/Retained Root with registry16 cleanup) that a caller may place in a
state's 'jobs' before refresh_root()/successor(). The caller keeps 'issued',
'uhigh' and 'row_high' consistent with the cells it places.

Everything derived from them is recomputed by the unchanged version2 phase
code: Root Job digest over the Job3 bank, page trailers, descriptor Profile
digest and seal, Aging work bytes and remaining rows, Job sender digests (SHA
of the Profile3 Transaction descriptor), frame native cells, redo patches and
trailers. Callers use the version2 API names (fresh, setup, plan, successor,
frame, canonical_cases, refresh_root, page, cell_at, replace_cell, ...).
emit() and main() write into the version2 directory and are disabled. No XADB
code is imported. Only pathlib/hashlib/importlib/struct are used.
"""
from pathlib import Path
import hashlib
import importlib.util
import struct

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parent.parent
V2 = FIXTURES / 'dev3-catalog-create' / 'transition' / 'generate_literals.py'
V2_SHA = '3716f9d63f099051363bebeeaf0ebe4586ccd0984aa91735a7de82eba7d23b7e'
PROFILE3_SHA = 'f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e'
ROW_HIGH_AT = 432
BUDGET_AT = 384                 # Job cell budget rows, 5 x 128 bytes
REGISTRY_AT = 3 * 128 + 88      # cleanup row registry within a budget/aggregate
ROW_REGISTRY = 16


def load():
    source = V2.read_bytes()
    if hashlib.sha256(source).hexdigest() != V2_SHA:
        raise ValueError('version2 serializer pin')
    spec = importlib.util.spec_from_file_location('xadb_catalog_profile3_serializer', V2)
    t = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(t)
    profile2, free_job2, root_page2 = t.profile, t.free_job, t.root_page

    def profile():
        raw = bytearray(profile2())
        struct.pack_into('<H', raw, 4, 3)
        raw = t.seal(raw)
        t.need(hashlib.sha256(raw).hexdigest() == PROFILE3_SHA, 'Profile3 literal')
        return raw

    def free_job(generation=1):
        raw = bytearray(free_job2(generation))
        t.need(raw[4:6] == b'\x02\x00', 'version2 free Job recipe')
        struct.pack_into('<H', raw, 4, 3)
        return bytes(raw)

    def census_aggregate(jobs):
        # One accumulator over every occupied cell: columns 64..104 are the
        # one-executor peak (max), every other column is additive debt (sum).
        out = bytearray(640)
        for row in range(5):
            out[row * 128] = row + 1
        for cell in jobs:
            if cell[6] == 0:
                continue
            for row in range(5):
                for at in range(8, 120, 8):
                    offset = row * 128 + at
                    value = t.get(cell, BUDGET_AT + offset)
                    prior = t.get(out, offset)
                    t.put(out, offset, max(prior, value) if 64 <= at < 112 else prior + value)
        return bytes(out)

    def root_page(s):
        page2, aggregate2 = root_page2(s)
        cell, _ = t.cell_at(page2, 0)
        t.need(cell[4:6] == b'\x02\x00' and cell[ROW_HIGH_AT:] == bytes(80), 'version2 Root recipe')
        aggregate = census_aggregate(s['jobs'])
        row_scope = any(c[6] and t.get(c, BUDGET_AT + REGISTRY_AT) == ROW_REGISTRY for c in s['jobs'])
        t.need(row_scope or aggregate == aggregate2, 'Catalog-only bank keeps the version2 aggregate')
        root = bytearray(cell)
        struct.pack_into('<H', root, 4, 3)
        row_high = s.get('row_high', 0)
        t.need(row_high == 0 or s['revision'] > 1, 'genesis Root3 keeps row_high 0')
        struct.pack_into('<Q', root, ROW_HIGH_AT, row_high)
        root[160:192] = t.sha(aggregate)
        return t.replace_cell(page2, 0, bytes(root)), aggregate

    def row_job(*, kind, state, generation, local, actor_generation, status, lease,
                adoption=0, undo=0, ug=0, sequence=0, row=0, pair=(0, 0), cell=185, h=0,
                statement=1, mutation=1, object_id=6, sender=bytes(32),
                projection=bytes(32), incarnation=2):
        """Row-scope Job3 cell: leased Terminal (kind1) or Reserved/Retained Root (kind3)."""
        t.need((kind, state) in ((1, 1), (3, 1), (3, 2)), 'Row Terminal or Reserved/Retained Root')
        raw = bytearray(1024)
        raw[:4] = b'XQJ1'
        struct.pack_into('<H', raw, 4, 3)
        raw[6], raw[7] = state, kind
        t.put(raw, 8, generation)
        raw[16:64] = t.actor(local, actor_generation)
        t.put(raw, 64, lease)
        raw[80:120] = t.slot(status, actor_generation, incarnation)
        if state == 2:
            t.need(adoption > lease and any(sender), 'Retained adoption and sender')
            t.put(raw, 72, adoption)
            raw[328:360] = sender
        t.put(raw, 376, adoption if state == 2 else lease)
        if kind == 1:
            steps, wal = 1, 55112
        else:
            raw[128:168] = t.slot(undo, ug, incarnation)
            for at, value in ((168, sequence), (176, object_id), (184, 1), (192, 1),
                              (248, statement), (256, mutation), (320, row)):
                t.put(raw, at, value)
            raw[200:224] = t.ref(pair[0], incarnation)
            raw[224:248] = t.ref(pair[1], incarnation)
            t.put(raw, 280, cell, 4)
            raw[288:320] = projection
            steps, wal = 5, 311608 + 2 * cell + 128 * h
        budget = bytearray(t.budgets({3: (steps, wal, 0, 0)}))
        t.put(budget, REGISTRY_AT, ROW_REGISTRY)
        raw[BUDGET_AT:] = budget
        return bytes(raw)

    def disabled(*_args, **_kwargs):
        raise RuntimeError('the version2 writer is not part of the Profile3 serializer')

    t.profile, t.free_job, t.root_page = profile, free_job, root_page
    t.census_aggregate, t.row_job = census_aggregate, row_job
    t.emit = t.main = disabled
    return t
