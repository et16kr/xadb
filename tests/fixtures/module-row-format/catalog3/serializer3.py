"""Profile3 Catalog scope2 CREATE serializer for stdlib-only generators.

`load()` executes the hash-pinned version2 serializer
`dev3-catalog-create/transition/generate_literals.py` as a fresh module object
and replaces only its three version-bearing primitives:

* profile()   -> the Profile3 ID1 envelope (version3 header, resealed);
* free_job()  -> Job3 cells, so every job() cell is also Job3;
* root_page() -> Root3 cells with row_high@432 taken from state key
                 'row_high' (default 0; a genesis Root must keep 0).

Everything derived from them is recomputed by the unchanged version2 phase
code: Root Job digest over the Job3 bank, page trailers, descriptor Profile
digest and seal, Aging work bytes, Job sender digests (SHA of the Profile3
Transaction descriptor), frame native cells, redo patches and trailers.
Callers use the version2 API names (fresh, setup, plan, successor, frame,
canonical_cases, page, cell_at, replace_cell, ...). emit() and main() write
into the version2 directory and are disabled. No XADB code is imported.
Only pathlib/hashlib/importlib/struct are used.
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

    def root_page(s):
        page2, aggregate = root_page2(s)
        cell, _ = t.cell_at(page2, 0)
        t.need(cell[4:6] == b'\x02\x00' and cell[ROW_HIGH_AT:] == bytes(80), 'version2 Root recipe')
        root = bytearray(cell)
        struct.pack_into('<H', root, 4, 3)
        row_high = s.get('row_high', 0)
        t.need(row_high == 0 or s['revision'] > 1, 'genesis Root3 keeps row_high 0')
        struct.pack_into('<Q', root, ROW_HIGH_AT, row_high)
        return t.replace_cell(page2, 0, bytes(root)), aggregate

    def disabled(*_args, **_kwargs):
        raise RuntimeError('the version2 writer is not part of the Profile3 serializer')

    t.profile, t.free_job, t.root_page = profile, free_job, root_page
    t.emit = t.main = disabled
    return t
