"""Raw full-state before snapshots for internal replay, not selected histories.

Use the pinned, independent byte author only for lower page/Base envelopes.
The replay implementation and split candidate are never called by this writer.
"""
from hashlib import sha256
from pathlib import Path
import importlib.util

ROOT = Path(__file__).resolve().parent
PARENT = ROOT.parent / 'current-tree'
AUTHOR_SHA = 'c2dbfa3cbd87c3beab84b52a9d9fe17732e6d473ee508a8da04d84173df59358'
assert sha256((PARENT / 'author.py').read_bytes()).hexdigest() == AUTHOR_SHA
spec = importlib.util.spec_from_file_location('raw_tree_inputs', PARENT / 'author.py')
raw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(raw)


def internal_before():
    b = bytearray((PARENT / 'root-split.xrb').read_bytes())
    old = raw.image(b, 11)
    values = raw.cells(old)
    leaf18 = raw.page(old, 18, 1, 0, 75, [values[0], values[2]])
    leaf20 = raw.page(old, 20, 1, 0, 76, [values[0]])
    leaf11 = raw.page(old, 11, 1, 0, 75, values[:2])
    root = raw.internal(raw.image(b, 17), [2, 3, 4], [11, 18, 16, 20], 76)
    for n, page in ((11, leaf11), (18, leaf18), (20, leaf20), (17, root)):
        raw.install(b, n, page)
    for n, page, lsn, revision, left in ((18, leaf18, 75, 2, 11), (20, leaf20, 76, 3, 16)):
        claim = raw.claim(page, 5, lsn, 2, None, revision)
        claim[232:256] = raw.pref(left)
        b[raw.CLAIMS+n*352:raw.CLAIMS+(n+1)*352] = claim
    raw.allocator(b, [(18, 1), (20, 1)], 76)
    raw.anchor(b, 4, 76)
    return raw.finish(b, 76, 2)


def child_before(parent):
    b = bytearray(parent)
    template = raw.image(b, 17)
    meta = raw.cells(raw.image(b, 20))[0]
    leaf24 = raw.page(raw.image(b, 20), 24, 1, 0, 85, [meta])
    leaf25 = raw.page(raw.image(b, 20), 25, 1, 0, 86, [meta])
    sibling = raw.internal(template, [6], [24, 25], 90, 22)
    root = raw.internal(template, [5], [17, 22], 90, 21)
    root[7] = 2
    raw.seal(root)
    for n, page in ((24, leaf24), (25, leaf25), (22, sibling), (21, root)):
        raw.install(b, n, page)
    entries = ((21, root, 6, 80, 1, 19, 4, 17),
               (24, leaf24, 5, 85, 2, None, 5, 20),
               (25, leaf25, 5, 86, 2, None, 6, 24),
               (22, sibling, 5, 90, 2, None, 7, 17))
    for n, page, role, lsn, op, peer, rev, left in entries:
        claim = raw.claim(page, role, lsn, op, peer, rev)
        claim[232:256] = raw.pref(left)
        b[raw.CLAIMS+n*352:raw.CLAIMS+(n+1)*352] = claim
    raw.allocator(b, [(21, 2), (22, 2), (24, 1), (25, 1)], 90)
    # The absent birth peer19 has actually retired at incarnation2; a later
    # split may reuse its address only with incarnation3.
    allocation = raw.image(b, 0)
    raw.put(allocation, raw.get(allocation, 128, 2)+64+19*96+8, 2)
    raw.install(b, 0, raw.seal(allocation))
    raw.anchor(b, 8, 90, 21)
    anchor = raw.image(b, 10)
    anchor[raw.get(anchor, 128, 2)+7] = 2
    raw.install(b, 10, raw.seal(anchor))
    return raw.finish(b, 90, 2)


if __name__ == '__main__':
    before = internal_before()
    (ROOT / 'internal-root-before.xrb').write_bytes(before)
    (ROOT / 'internal-child-before.xrb').write_bytes(child_before(before))
    parents = [PARENT / 'author.py', PARENT / 'root-split.xrb',
               ROOT.parent / 'structural-bank/root.xod',
               ROOT.parent / 'structural-bank/child.xod']
    (ROOT / 'parents.sha256').write_text(''.join(sha256(p.read_bytes()).hexdigest()+'  '+str(p.relative_to(ROOT.parent))+'\n' for p in parents))
    (ROOT / 'inputs.sha256').write_text(''.join(sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n' for p in sorted(ROOT.glob('*.xrb'))))
