#!/usr/bin/env python3
"""Independent scope3 Row DELETE transition literals; run as `python3 -I author.py`.

Only hashlib/pathlib/struct/json. No XADB encoder, decoder, test helper or
other fixture author is imported or executed. The scope3 History model below
started as a copy of `module-row-format/transition/author.py` (the N1 author)
and is extended here from dev3-row-delete.md alone: opcodes 8/9/23 and their
shapes (#delete-shape), the Job3 effect byte @284 (#job-effect), the
tombstone (#tombstone), the delete U (#delete-undo), the delete costs
(#delete-costs), the edge/count multiset (#delete-edges) and the leaf
reservation (#leaf-reservation). A's MCA00-V06 snapshots, the root3 cells and
the N1 literals are read as bytes only, pinned by SHA-256; history H is re-run
here and each of its frames must equal the N1 literal of the same name before
history D continues from its end state (codec cross-check).

D  from the end of H (actor T, terminal 14, ReleasingCommitted roots 15/17
   holding rows 4/11): Begin516 of T2 on Status page 20; ReleaseRole517-519
   and RootFree520 of T's root 15 (row 4 becomes deletable, Undo page 10
   Free); statement 1 delete Reserve521 of row 2 and Cancel522; statement 2
   delete Reserve523 and DeleteForward524 (h0, A's frame); statement 3 delete
   Reserve525 of row 9, DeleteForward526 (effect-1 H), RollbackStart527,
   DeleteClr528 (effect-1 H, A's frame), StatementFinish529; statement 4
   INSERT Reserve530 and Forward531 (effect-1 H, A's frame); statement 5
   delete Reserve532 of row 4 and DeleteForward533 (effect-0 H, A's frame);
   Commit534; DeletePrune535 (h1), Prune536 (effect-1 H), DeletePrune537 (h0);
   ReleaseRole538-540 and RootFree541 of the committed delete root,
   ReleaseRole542-544 and RootFree545 of the compensated one.

Readings chosen where the contract leaves a field open (each is also enforced
by the Rust observer): the delete Reserve keeps every INSERT Reserve field
but row_high, with C = the old row's cell, the effect byte 1, the target RowId
and the planned tombstone's projection; a delete root's Releasing Job keeps
@284 and zeroes [280,284) and [285,320); the restored row of DeleteClr is the
old record byte for byte; the Releasing delete U keeps before stamp (until
cursor 3) and logical and drops its body.
"""
from hashlib import sha256
from pathlib import Path
from struct import pack, pack_into, unpack_from
import json

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parent.parent
PARENTS = {
    'module-row-format/transition/blobs.bin': '96f88af3b8973f97d4bedcb315c47aaa65fffb27027cafefbad37a876e6ded6c',
    'module-row-format/transition/index.tsv': '9d57952d6afecb04b2b03f1fa3eddd7348c2c8739fb841d9e22ac14a6f189f29',
    'module-row-format/scope3/profile3.bin': '8335423a65c2870d4c3d115ef5011e3c82d2fc2b4341e7e10d5b7321ec5a3934',
    'module-row-format/scope3/h-500-aging.bin': 'cdae28875cc0f13764e22ba76f2707e0c3c3f71593c8b8b7a30b0e0d702787c6',
    'module-row-format/scope3/h-500-bank.bin': '52b8ba5f93ef087ff60606bd0656840152074923550cc86e860b952235556c81',
    'module-row-format/scope3/h-500-jobs2.bin': 'dbd19662a93dff63885fa09016faedfc406b01a1ed01217506c67ed0f03fc628',
    'module-row-format/scope3/h-500-jobs3.bin': '37a290fb7e7fc6b4e18ed4e1a5ec74af344f3b8721e6c910fde480265c03c109',
    'module-row-format/scope3/h-500-leaf.bin': '53aba09198d8a858f3a963c248db4aaa68777b01e4cb9e9d3a8d086b534bf0ac',
    'module-row-format/scope3/h-500-root.bin': '3e30315fdcbc65be40699fc3fcc99392e9bcb29a5971710c7029191888415495',
    'module-row-format/scope3/h-500-status.bin': '37faf4cf4b6b016d3ea5e636a5b4a27cb4d8fc2b74e05128338fa2c350d58a8e',
    'module-row-format/scope3/h-500-tx.bin': 'c276fe136719aa7773a342ff4c56582401b176607b696ce2af439676f1385ee5',
    'module-row-format/scope3/h-500-undo10.bin': '6ab2d3a2dc9dbae486d5a0895fb9a1f92ba0716f579bc9513c3b20ce12a1ee7e',
    'module-row-format/scope3/h-500-undo11.bin': '07f6d0cbd44e58526b472a9a3388c293e8897a8eaceeb3f8a89596644c8083db',
    'module-row-format/scope3/h-501-aging.bin': '9f79cc68e1023f8f13bfec376f0699beeff5ea51b15ed5aef59e2b97a31c6d6c',
    'module-row-format/scope3/h-501-bank.bin': 'b2840f4683037fc2eba5403bd2a46c2b51ba8428ec254d324fdfa55079b2bede',
    'module-row-format/scope3/h-501-forward.wal': '43848ceb656e03e6cc0cc01cdf1c69bdac7f88d6de932942795b815aa64a650b',
    'module-row-format/scope3/h-501-jobs2.bin': 'dbd19662a93dff63885fa09016faedfc406b01a1ed01217506c67ed0f03fc628',
    'module-row-format/scope3/h-501-jobs3.bin': '828e458a0fcd3887e866e6d5b21613d38a538597cca9156399d354c96878aec4',
    'module-row-format/scope3/h-501-leaf.bin': 'b451ed8bcaa6ce84fa62876beac248dba93aedbedc690a70e5fc6bed5ffd116b',
    'module-row-format/scope3/h-501-root.bin': 'd9256e603318df318d3d5f7073bb82ce866d9f1627b88ba3bb73206207b41ef2',
    'module-row-format/scope3/h-501-status.bin': 'ee58aedd6182d6f2f9d4e2b6846305899997be3742c4a8a48111729721afc1c6',
    'module-row-format/scope3/h-501-tx.bin': 'b57724b0f0e51bb515391aa08065a1f322a4f370a1000307a72a8091081055c6',
    'module-row-format/scope3/h-501-undo11.bin': 'fbdae72a06d0e76155f4181e4be763360fe935edb187bc2ab74f3a71fd2a2bd1',
    'module-row-format/scope3/h-502-status.bin': 'a60413c2ea6cf04175db3bc536018111e3982ec8f05b801770f3cf75a9fa308e',
    'module-row-format/scope3/h-502-tx.bin': '157e6dd1c041536a4b8ba2521f1fbf774824b9873c1725674cc40273d46f1473',
    'module-row-format/scope3/h-503-aging.bin': 'ab3e33882bf994a75897cbe61aeb0087905cbfa98b703ffbf358bf866bc8246b',
    'module-row-format/scope3/h-503-bank.bin': '7deffdb41582c1e7d1f2d5513c4d8c8b0402d461c74d706b5b40c6c97fb64cc6',
    'module-row-format/scope3/h-503-clr.wal': '96238da38c9b0ba4a236808c13a278513fd7eb9cf901418c4c21cc099bf66367',
    'module-row-format/scope3/h-503-jobs3.bin': '0c289bd95d11cdb0e73483e8090410475b15e7285dc09d5bcb4029ce3bc893ce',
    'module-row-format/scope3/h-503-leaf.bin': '439a518e203b674475caf65bf2de1b42b1243d5235930edb82dc3b8dd177c568',
    'module-row-format/scope3/h-503-root.bin': '07d501fa7f4c0c30b842d67ff2f9d478154fd29a1fb18eed1d287f336aeef1be',
    'module-row-format/scope3/h-503-status.bin': '875c8ff8e78e5e915d8344b3139bca47e9b95af5604687cd32a3c4dfdce4423b',
    'module-row-format/scope3/h-503-tx.bin': 'eac0effed85df8b28acecd181b839aad2d75f0bcda071c261122a69544e2e229',
    'module-row-format/scope3/h-503-undo11.bin': '1e2706f2120d9fbe4fcbb75ce99fd315db3475d145db0a1965b8c7fc77f19d6d',
}


READ = set()


def parent(name):
    data = (FIXTURES / name).read_bytes()
    assert sha256(data).hexdigest() == PARENTS[name], name
    READ.add(name)
    return data


def a(name):
    return parent('module-row-format/scope3/' + name)


def n1(name):
    """One N1 literal (module-row-format/transition) by its index name."""
    for line in parent('module-row-format/transition/index.tsv').decode().splitlines():
        label, offset, length, digest = line.split('\t')
        if label == name:
            data = parent('module-row-format/transition/blobs.bin')[int(offset):int(offset) + int(length)]
            assert sha256(data).hexdigest() == digest, name
            return data
    raise KeyError(name)


PAGE = 16384
DIGEST = PAGE - 32
DB = bytes([1]) * 16
FILE = 0x1_0000_0001
ROW_PEAK = (9, 425_984, 0, 16, 5, 1)
RELEASE, BEGIN, RESERVE, CANCEL = 55_528, 56_712, 57_336, 40_000
STATUS_PATCH, TERMINAL_FINISH, AGING_START = 3_040, 55_112, 19_672
TABLE = (6, 1, 1)
KIND = dict(leaf=1, undo=8, status=9, anchor=11, root=12, jobs=13)
NATIVE = dict(leaf=1, undo=3, status=5, root=8, jobs=9)
# opcode -> XWF1 FrameKind (Begin1 Update2 System3 Clr4 Commit5 AbortEnd6)
OUTER = {1: 1, 2: 2, 3: 2, 4: 4, 5: 2, 6: 5, 7: 6, 8: 2, 9: 4, 11: 2, 12: 2,
         16: 3, 17: 3, 20: 3, 21: 3, 22: 3, 23: 3}
HANDOFF = {1, 2, 4, 6, 7, 8, 9, 11, 12}
DELETE_FORWARD, DELETE_CLR, DELETE_PRUNE = 8, 9, 23
TOMBSTONE_CELL = 180
TREE_ENTRY_CAPACITY = 16188
STATUS_OPS = {3, 5}


def sha(data):
    return sha256(data).digest()


def u64(value):
    return pack('<Q', value)


def get(data, at, width=8):
    return int.from_bytes(data[at:at + width], 'little')


def put(data, at, value, width=8):
    data[at:at + width] = value.to_bytes(width, 'little')


def page_ref(page_no, incarnation):
    return pack('<QQQ', FILE, page_no, incarnation)


def txn_id(local):
    return DB + pack('<QI4xQ', 1, 1, local)


def slot_ref(page_no, generation):
    """UndoRef40/StatusSlotRef40: PageRef24(inc2), slot0, zero6, generation."""
    return page_ref(page_no, 2) + pack('<H6xQ', 0, generation)


AGING_REF = DB + pack('<QB7xQQ', 1, 3, 1, 1)
PROFILE = None  # set in main after the parent is pinned


def page(kind, page_no, incarnation, owner, lsn, cells):
    out = bytearray(PAGE)
    out[:4] = b'XDP1'
    pack_into('<HBB', out, 4, 1, KIND[kind], 0)
    out[8:24] = DB
    pack_into('<6Q', out, 24, FILE, page_no, incarnation, owner, 1, lsn)
    upper = DIGEST
    for slot, cell in enumerate(cells):
        upper -= len(cell)
        pack_into('<HH', out, 128 + 4 * slot, upper, len(cell))
        out[upper:upper + len(cell)] = cell
    pack_into('<HHH', out, 72, len(cells), 128 + 4 * len(cells), upper)
    out[DIGEST:] = sha(out[:DIGEST])
    return bytes(out)


def cell_at(page_bytes, slot):
    offset, length = unpack_from('<HH', page_bytes, 128 + 4 * slot)
    return offset, page_bytes[offset:offset + length]


def page_lsn(page_bytes):
    return get(page_bytes, 64)


# ---- Job3 / Root3 ---------------------------------------------------------

def budget(steps, wal):
    out = bytearray(640)
    for row in range(5):
        out[row * 128] = row + 1
    if steps:
        pack_into('<QQ', out, 3 * 128 + 8, steps, wal)
        pack_into('<6Q', out, 3 * 128 + 64, *ROW_PEAK)
        pack_into('<Q', out, 3 * 128 + 112, wal)
    return out


def job_free(generation):
    out = bytearray(1024)
    out[:4] = b'XQJ1'
    pack_into('<HxxQ', out, 4, 3, generation)
    return out


def summarize(bank):
    aggregate = budget(0, 0)
    masks = [0, 0, 0]
    for ordinal, cell in enumerate(bank):
        state = cell[6]
        masks[0 if state == 0 else 1 if state == 1 else 2] |= 1 << ordinal
        if not state:
            continue
        for row in range(5):
            for at in range(8, 120, 8):
                offset = row * 128 + at
                value, current = get(cell, 384 + offset), get(aggregate, offset)
                put(aggregate, offset, max(current, value) if 64 <= at < 112 else current + value)
    return masks, sha(b''.join(bank)), bytes(aggregate)


# ---- Status / Undo / Row ---------------------------------------------------

def status_core(actor, state, rollback=0, q=0, head=None, target=None, nxt=None):
    core = bytearray(192)
    core[:4] = b'XTS1'
    pack_into('<HBB', core, 4, 1, state, rollback)
    core[8:56] = actor
    if state == 3:
        put(core, 56, q)
    for index, reference in enumerate((head, target, nxt)):
        if reference is not None:
            core[64] |= 1 << index
            core[72 + 40 * index:112 + 40 * index] = reference
    return bytes(core)


def status_free(generation):
    core = bytearray(192)
    core[:4] = b'XTS1'
    pack_into('<H', core, 4, 1)
    put(core, 48, generation)
    return bytes(core)


def home(core, incoming):
    return b'XSH1' + pack('<HHQ', 1, 32, incoming) + bytes(16) + core


def core_refs(core):
    flags = core[64]
    return [core[72 + 40 * i:112 + 40 * i] if flags >> i & 1 else None for i in range(3)]


def undo_free(generation):
    out = bytearray(432)
    out[:4] = b'XUR1'
    pack_into('<HBxQ', out, 4, 1, 0, generation)
    return bytes(out)


def undo_material(job, live, incoming, edge, link=None, cursor=0):
    """Missing Row root material of a root Job (edge tags: 0 Never, 1 Linked,
    2 Cut, 3 Released). The second edge is Released once cursor >= 1."""
    out = bytearray(432)
    out[:4] = b'XUR1'
    pack_into('<HBxQ', out, 4, 1, 1 if live else 2, get(job, 160))
    put(out, 16, get(job, 168))
    out[24:64] = job[16:56]
    put(out, 64, get(job, 56))
    put(out, 72, get(job, 256))
    put(out, 80, incoming)
    out[88] = 1
    pack_into('<QQQ', out, 96, get(job, 176), get(job, 184), get(job, 320))
    if edge == 1:
        out[120:168] = link
    out[326] = edge
    out[327] = 3 if cursor >= 1 else 0
    out[328] = cursor
    return bytes(out)


def canonical(number, text):
    out = pack('<IBq', 2, 2, number)
    if text is None:
        return out + b'\0'
    data = text.encode()
    return out + pack('<BI', 3, len(data)) + data


def record(row, canon, actor, mutation, undo):
    out = bytearray(168 + len(canon))
    out[:4] = b'XRW1'
    pack_into('<HBBQ', out, 4, 1, 2, 1, row)
    out[16] = 2
    out[24:72] = actor
    out[72:120] = actor[:40] + u64(mutation)
    out[120:160] = undo
    pack_into('<I', out, 160, len(canon))
    out[168:] = canon
    return bytes(out)


def tombstone(row, actor, mutation, undo):
    """dev3-row-delete.md#tombstone: flags deleted|undo, payload tag None, no
    body; Transaction(actor), logical (actor, mutation), undo U. 168 B."""
    out = bytearray(record(row, b'', actor, mutation, undo))
    out[6], out[7] = 3, 0
    assert len(out) == TOMBSTONE_CELL - 12
    return bytes(out)


def committed_record(row, q, logical, canon):
    """An inline row `Committed(q)`, `logical`, undo None (a pruned row and
    DeleteClr's restored row)."""
    out = bytearray(record(row, canon, bytes(48), 0, bytes(40)))
    out[6] = 0
    out[16] = 1
    out[24:72] = bytes(48)
    put(out, 24, q)
    out[72:120] = logical
    return bytes(out)


def delete_undo(job, live, incoming, edge, link, cursor, before):
    """dev3-row-delete.md#delete-undo over the Row root material: before kind
    RowValue (2), before stamp `Committed(q_old)` (None once cursor 3), the
    old logical, and the old canonical inline after the 432-byte header while
    Live (value storage Inline1, total L); Releasing has no body."""
    q_old, logical, value = before
    out = bytearray(undo_material(job, live, incoming, edge, link, cursor))
    if q_old is not None:
        out[216] = 1
        put(out, 224, q_old)
    out[272:320] = logical
    out[324] = 2
    if value is None:
        assert not live
        return bytes(out)
    assert live and 5 <= len(value) <= 2048
    put(out, 320, len(value), 4)
    out[325] = 1
    return bytes(out) + value


def restamp(value, q):
    """Committed(q), undo None, same logical version and canonical."""
    out = bytearray(value)
    out[6] = 0
    out[16] = 1
    out[24:72] = bytes(48)
    put(out, 24, q)
    out[120:160] = bytes(40)
    return bytes(out)


def projection(row, value):
    prefix = bytearray(48)
    prefix[:8] = b'XRP1\x01\x00\x01\x00'
    prefix[8] = 1
    pack_into('<QQQ', prefix, 16, TABLE[0], TABLE[1], row)
    prefix[40] = 0 if value is None else 1
    return sha(bytes(prefix) + (value or b''))


def leaf_page(rows, lsn):
    cells = [b'XBT1\x01\x00' + bytes(26)]
    for row in sorted(rows):
        cells.append(pack('<HH', 8, len(rows[row])) + row.to_bytes(8, 'big') + rows[row])
    return page('leaf', 13, 2, TABLE[0], lsn, cells)


def leaf_slot(page_bytes, row):
    for slot in range(1, get(page_bytes, 72, 2)):
        if cell_at(page_bytes, slot)[1][4:12] == row.to_bytes(8, 'big'):
            return slot
    raise ValueError(row)


# ---- XOD1 descriptors and the Row ledger -----------------------------------

def debt(steps, wal, slots=0):
    return [0] * 14 if not steps else [steps, wal, 0, 0, 0, slots, 0, *ROW_PEAK, wal]


def descriptor(owner, chain, inventory, consumed, rows, minimum, mutation):
    out = bytearray(1536)
    out[:4] = b'XOD1'
    pack_into('<H', out, 4, 1)
    pack_into('<I', out, 8, 1536)
    if owner[0] == 'tx':
        core, actor = owner[1], owner[2]
        out[6] = core[6]
        out[16] = 1
        out[24:72] = actor
        out[160:352] = core
    else:
        revision, cursor, work = owner[1:]
        out[6] = 5
        out[16] = 2
        out[24:72] = AGING_REF
        out[352:354] = b'\x03\x02'
        pack_into('<3Q', out, 360, revision, cursor, 64)
        out[384:480] = work
    pack_into('<3Q', out, 72, *chain[:3])
    out[96:160] = pack('<BB2xIQI12x', 1, 0, 0, 1, 4096) + sha(PROFILE)
    pack_into('<2Q', out, 544, inventory[0], inventory[1])
    out[560:592] = inventory[2]
    pack_into('<5Q', out, 592, 0, 0, consumed, 0, chain[3])
    for index, row in enumerate(rows):
        base = 640 + 128 * index
        out[base] = index + 1
        pack_into('<14Q', out, base + 8, *row)
    for index in range(6):
        put(out, 1280 + 8 * index, max(row[7 + index] for row in rows))
    pack_into('<3Q', out, 1328, sum(row[13] for row in rows), minimum, mutation)
    out[1504:] = sha(out[:1504])
    return bytes(out)


def effect(job):
    """Job3 @284: 0 INSERT, 1 DELETE (dev3-row-delete.md#job-effect)."""
    assert job[284] in (0, 1) and not any(job[285:288])
    return job[284]


def cells(job):
    """C-multiples of a root's frames: INSERT C, DELETE 2C (#delete-costs)."""
    return (2 if effect(job) else 1) * get(job, 280, 4)


def has_h(job):
    # cleanup 311608+2C+128h (INSERT) or 311608+C+128h (DELETE).
    return get(job, 384 + 400) == 311_736 + (1 if effect(job) else 2) * get(job, 280, 4)


def tx_rows(bank, actor, core, credit):
    """consumers.md Row ledger: CLR debt per Reserved/Retained root while not
    terminal, the Reserved Forward row, the terminal row and control."""
    state, rollback = core[6], core[7]
    terminal = state in (3, 4)
    steps = wal = 0
    reserved = None
    for job in bank:
        if job[6] in (1, 2) and job[7] == 3 and job[16:64] == actor:
            cell, h = cells(job), has_h(job)
            if not terminal:
                steps, wal = steps + 1, wal + 91_096 + cell + 128 * h
            if job[6] == 1:
                reserved = 91_304 + cell + 128 * h
    control = (3 if credit else 1) if state == 1 else 2 if (state, rollback) == (2, 1) else 0
    return [debt(steps, wal), debt(1, reserved, 1) if reserved else debt(0, 0),
            debt(0, 0) if terminal else debt(1, BEGIN), debt(control, control * STATUS_PATCH),
            debt(0, 0)]


# ---- frame pieces -----------------------------------------------------------

def native(kind, index, before, after):
    header = bytearray(128)
    header[0] = kind
    pack_into('<H', header, 2, index)
    bodies = []
    for bit, slot_at, offset_at, length_at, digest_at, side in (
            (1, 4, 8, 12, 24, before), (2, 6, 10, 16, 56, after)):
        if side is None:
            pack_into('<H', header, slot_at, 0xFFFF)
            continue
        offset, cell = cell_at(*side)
        header[1] |= bit
        pack_into('<H', header, slot_at, side[1])
        pack_into('<H', header, offset_at, offset)
        pack_into('<I', header, length_at, len(cell))
        header[digest_at:digest_at + 32] = sha(cell)
        bodies.append(cell)
    pack_into('<I', header, 20, 128 + sum(map(len, bodies)))
    return bytes(header) + b''.join(bodies)


def redo(kind, identity, before, after, offset, length):
    header = bytearray(64)
    pack_into('<BBBxI', header, 0, 1, KIND[kind], 0, 64 + length)
    header[8:32] = page_ref(identity[0], identity[1])
    pack_into('<3QHH', header, 32, identity[2], 1, page_lsn(before), offset, length)
    return bytes(header) + after[offset:offset + length]


def core256(op, statement=0, mutation=0, sequence=0, current=None, nxt=None, target=None,
            q=0, cursors=(0, 0), digests=None):
    out = bytearray(256)
    flags = 0
    for bit, at, reference in ((0, 32, current), (1, 72, nxt), (2, 112, target)):
        if reference is not None:
            flags |= 1 << bit
            out[at:at + 40] = reference
    if digests is not None:
        flags |= 24
        out[176:208], out[208:240] = digests
    pack_into('<HHI3Q', out, 0, op, flags, 256, statement, mutation, sequence)
    pack_into('<3Q', out, 152, q, *cursors)
    return bytes(out)


def target_undo(link):
    return b'\x01' + bytes(7) + link


def target_txn(actor):
    return b'\x02' + bytes(7) + actor


def edge(index, role, old, new):
    out = bytearray(128)
    pack_into('<HB', out, 0, index, role)
    if old is not None:
        out[8:64] = old
    if new is not None:
        out[64:120] = new
    return bytes(out)


def count(target, before, after, index):
    return target + pack('<QQH22x', before, after, index)


def delta(action, item):
    out = bytearray(208)
    out[0] = action
    out[1] = 2 if action == 1 else 1
    at = 112 if action == 1 else 16
    out[at:at + 96] = item
    return bytes(out)


def undo_item(job):
    item = bytearray(96)
    item[:2] = b'\x02\x02'
    item[8:48] = job[128:168]
    pack_into('<QQ', item, 64, 1, get(job, 64))
    return bytes(item)


def graph(adding, actor, u, h, incoming, at):
    """Edges sorted by (native index, role) and U-then-T counts."""
    t = target_txn(actor)
    u, h = target_undo(u), (target_undo(h) if h else None)
    pending = [(at['status'], 10, *((h, u) if adding else (u, h))),
               (at['leaf'], 1, *((None, u) if adding else (u, None))),
               (at['leaf'], 7, *((None, t) if adding else (t, None)))]
    if h:
        pending.append((at['undo'], 4, *((None, h) if adding else (h, None))))
    edges = [edge(*item) for item in sorted(pending, key=lambda item: item[:2])]
    counts = [count(u, *((0, 2) if adding else (2, 0)), at['undo']),
              count(t, incoming, incoming + 1 if adding else incoming - 1, at['status'])]
    return edges, counts


# ---- one simulated history --------------------------------------------------

IDENTITY = dict(root=lambda s, n: (1, 1, 4), jobs=lambda s, n: (2 + n, 1, 4),
                status=lambda s, n: (n, 2, 2), undo=lambda s, n: (n, 2, 3),
                leaf=lambda s, n: (13, 2, TABLE[0]))


class History:
    def __init__(self):
        self.root = bytearray(512)
        self.root_lsn = 0
        self.bank = []
        self.jobs_lsn = {}
        self.status = {}
        self.undo = {}
        self.rows = {}
        self.leaf_lsn = 0
        self.tx = None
        self.aging = None
        self.canonical = {}
        # Delete roots: ordinal -> (q_old, old logical, old canonical).
        self.deleted = {}

    # pages and descriptors
    def refresh(self):
        masks, digest, aggregate = summarize(self.bank)
        pack_into('<3Q', self.root, 96, *masks)
        self.root[128:160] = digest
        self.root[160:192] = sha(aggregate)
        return aggregate

    def root_page(self):
        return page('root', 1, 1, 4, self.root_lsn, [bytes(self.root)])

    def jobs_page(self, index):
        cells = [bytes(c) for c in self.bank[15 * index:15 * index + (4 if index == 4 else 15)]]
        return page('jobs', 2 + index, 1, 4, self.jobs_lsn[index], cells)

    def status_page(self, page_no):
        cell, lsn = self.status[page_no]
        return page('status', page_no, 2, 2, lsn, [cell])

    def undo_page(self, page_no):
        cell, lsn = self.undo[page_no]
        return page('undo', page_no, 2, 3, lsn, [cell])

    def leaf(self):
        return leaf_page(self.rows, self.leaf_lsn)

    def page_of(self, role, number):
        return dict(root=lambda: self.root_page(), jobs=lambda: self.jobs_page(number),
                    status=lambda: self.status_page(number), undo=lambda: self.undo_page(number),
                    leaf=lambda: self.leaf())[role]()

    def aging_descriptor(self):
        aggregate = self.refresh()
        work = bytearray(96)
        work[0] = 7
        pack_into('<QI', work, 8, get(self.root, 64), 16384)
        work[32:64] = sha(self.root_page())
        work[64:96] = sha(bytes(self.root))
        rows = [list(unpack_from('<14Q', aggregate, row * 128 + 8)) for row in range(5)]
        chain = self.aging
        return descriptor(('aging', get(self.root, 64), get(self.root, 72), bytes(work)), chain,
                          (chain[2], 0, sha(b'')), get(self.root, 416), rows, chain[2], 0)

    def status_core(self):
        return self.status[self.tx['status']][0][32:]

    def tx_descriptor(self):
        tx = self.tx
        core = self.status_core()
        rows = tx_rows(self.bank, tx['actor'], core, tx['credit'])
        return descriptor(('tx', core, tx['actor']), tx['chain'], tx['inventory'],
                          tx['consumed'], rows, tx['chain'][2], tx['mutation'])

    # loading and checking A's snapshots
    @classmethod
    def from_a(cls, prefix, jobs, undo):
        h = cls()
        root_page = a(f'{prefix}-root.bin')
        h.root = bytearray(cell_at(root_page, 0)[1])
        h.root_lsn = page_lsn(root_page)
        bank = a(f'{prefix}-bank.bin')
        h.bank = [bytearray(bank[i * 1024:(i + 1) * 1024]) for i in range(64)]
        for index in jobs:
            h.jobs_lsn[index] = page_lsn(a(f'{prefix}-jobs{2 + index}.bin'))
        status = a(f'{prefix}-status.bin')
        h.status[9] = [cell_at(status, 0)[1], page_lsn(status)]
        for page_no in undo:
            undo_bytes = a(f'{prefix}-undo{page_no}.bin')
            h.undo[page_no] = [cell_at(undo_bytes, 0)[1], page_lsn(undo_bytes)]
        leaf = a(f'{prefix}-leaf.bin')
        h.leaf_lsn = page_lsn(leaf)
        for slot in range(1, get(leaf, 72, 2)):
            cell = cell_at(leaf, slot)[1]
            h.rows[int.from_bytes(cell[4:12], 'big')] = cell[12:]
        tx = a(f'{prefix}-tx.bin')
        h.tx = dict(actor=tx[24:72], status=9,
                    chain=[get(tx, 72), get(tx, 80), get(tx, 88), get(tx, 624)],
                    inventory=(get(tx, 544), get(tx, 552), tx[560:592]),
                    consumed=get(tx, 608), mutation=get(tx, 1344),
                    credit=get(tx, 640 + 3 * 128 + 8) == 3)
        aging = a(f'{prefix}-aging.bin')
        h.aging = [get(aging, 72), get(aging, 80), get(aging, 88), get(aging, 624)]
        h.check(prefix, jobs, undo, ('root', 'bank', 'status', 'leaf', 'tx', 'aging'))
        return h

    def check(self, prefix, jobs=(), undo=(), names=('root', 'bank', 'status', 'leaf', 'tx', 'aging')):
        """Re-encode this model and require A's exact snapshot bytes."""
        actual = dict(root=self.root_page, bank=lambda: b''.join(map(bytes, self.bank)),
                      status=lambda: self.status_page(9), leaf=self.leaf,
                      tx=self.tx_descriptor, aging=self.aging_descriptor)
        for name in names:
            assert actual[name]() == a(f'{prefix}-{name}.bin'), (prefix, name)
        for index in jobs:
            assert self.jobs_page(index) == a(f'{prefix}-jobs{2 + index}.bin'), (prefix, index)
        for page_no in undo:
            assert self.undo_page(page_no) == a(f'{prefix}-undo{page_no}.bin'), (prefix, page_no)

    # state helpers
    def first_free(self):
        for offset in range(64):
            ordinal = (get(self.root, 72) + offset) % 64
            if self.bank[ordinal][6] == 0:
                return ordinal
        raise ValueError('no free Job')

    def advance_root(self, lsn, cost=0, ordinal=None):
        put(self.root, 64, get(self.root, 64) + 1)
        put(self.root, 80, lsn)
        if cost:
            put(self.root, 416, get(self.root, 416) + cost)
        if ordinal is not None:
            put(self.root, 72, (ordinal + 1) % 64)
        self.root_lsn = lsn
        self.refresh()
        chain = self.aging
        self.aging = [lsn, chain[0], chain[2], chain[3] + 1]

    def advance_tx(self, lsn, cost):
        chain = self.tx['chain']
        self.tx['chain'] = [lsn, chain[0], chain[2], chain[3] + 1]
        self.tx['consumed'] += cost

    def predecessor(self, actor, sequence):
        best = None
        for job in self.bank:
            if (job[16:64] == actor and job[6] == 2 and job[7] == 3
                    and get(job, 168) < sequence
                    and (best is None or get(best, 168) < get(job, 168))):
                best = job
        return best

    def head_sequence(self, head):
        if head is None:
            return 0
        for job in self.bank:
            if job[6] and job[7] == 3 and job[128:168] == head:
                return get(job, 168)
        raise ValueError('head without root')

    # frame assembly
    def emit(self, op, lsn, roles, before, txn=None, owners=None, core=None, aux=(), items=0,
             counts=0, edges=0):
        """roles: [(role, number, slots(before, after))]; before: pages before
        this group by role. owners: (descriptor befores, descriptor afters)."""
        changed = []
        for role, number, (before_slot, after_slot) in roles:
            identity = IDENTITY[role](self, number)
            changed.append((identity[0], role, identity, before[role],
                            self.page_of(role, number), before_slot, after_slot))
        changed.sort(key=lambda item: item[0])
        at = {item[1]: index for index, item in enumerate(changed)}
        natives = b''
        redos = b''
        offset, length = (16128, 224) if op in STATUS_OPS else (72, 16280)
        for index, (_, role, identity, old, new, bs, as_) in enumerate(changed):
            natives += native(NATIVE[role], index,
                              None if bs is None else (old, bs), None if as_ is None else (new, as_))
            redos += redo(role, identity, old, new, offset, length)
        if callable(aux):
            aux = aux(at)
        auxiliary = b''.join(aux)
        befores, afters = owners
        handoff = op in HANDOFF
        size = 256 if handoff else 192
        length_total = 160 + size + 1536 * len(afters) + 256 + len(natives) + len(auxiliary) + len(redos) + 32
        group = bytearray(size)
        group[:4] = b'XHG1' if handoff else b'XSG1'
        pack_into('<4HIQ', group, 4, 2, op, size, 3, length_total - 192, lsn)
        shape = (len(natives), len(auxiliary), len(changed), len(redos), edges, counts, items)
        if handoff:
            for ordinal in range(2):
                old = befores[ordinal]
                put(group, 24 + 8 * ordinal, get(old, 72) if old else 0)
                group[40 + 56 * ordinal:96 + 56 * ordinal] = afters[ordinal][16:72]
                group[152 + 32 * ordinal:184 + 32 * ordinal] = sha(old) if old else bytes(32)
            pack_into('<5I2HI3H', group, 216, 1536, 1536, 256, shape[0], shape[1], shape[2],
                      shape[2], shape[3], edges, counts, items)
        else:
            old = befores[0]
            put(group, 24, get(old, 72) if old else 0)
            group[32:88] = afters[0][16:72]
            group[88:120] = sha(old) if old else bytes(32)
            pack_into('<IHHHHH', group, 120, 1536, len(changed), edges, counts, items, len(changed))
            pack_into('<5I', group, 136, len(natives), edges * 128, counts * 96, items * 208, len(redos))
        header = bytearray(160)
        header[:4] = b'XWF1'
        pack_into('<HBBH2xIIH2x', header, 4, 1, OUTER[op], 3 if handoff else 2, 160,
                  length_total, length_total - 192, len(changed))
        header[24:40] = DB
        pack_into('<QI4xQQ', header, 40, 1, 1, lsn, lsn - 1)
        if txn is not None:
            header[72:112] = txn[0]
            put(header, 112, txn[1])
        header[128:160] = sha(header[:128])
        body = bytes(header) + bytes(group) + b''.join(afters) + core + natives + auxiliary + redos
        assert len(body) + 32 == length_total
        return body + sha(body)

    def snapshot(self, roles):
        return {role: self.page_of(role, number) for role, number in roles}

    # ---- operations ------------------------------------------------------
    def aging_start(self, lsn):
        roles = [('root', 0)]
        before = self.snapshot(roles)
        self.root[6] = 1
        self.root[16:64] = AGING_REF
        self.aging = [0, 0, 0, 0]
        self.advance_root(lsn, AGING_START)
        self.aging = [lsn, 0, lsn, 1]
        return self.emit(16, lsn, [('root', 0, (0, 0))], before,
                         owners=([None], [self.aging_descriptor()]), core=core256(16))

    def begin(self, lsn, local, generation, status_page):
        ordinal = self.first_free()
        index = ordinal // 15
        roles = [('root', 0), ('jobs', index), ('status', status_page)]
        before = self.snapshot(roles)
        aging_before = self.aging_descriptor()
        actor = txn_id(local) + u64(generation)
        old = self.bank[ordinal]
        assert self.status[status_page][0][32:] == status_free(generation - 1)
        job = job_free(get(old, 8) + 1)
        job[6:8] = b'\x01\x01'
        job[16:64] = actor
        put(job, 64, lsn)
        job[80:120] = slot_ref(status_page, generation)
        put(job, 376, lsn)
        job[384:] = budget(1, TERMINAL_FINISH)
        self.bank[ordinal] = job
        self.jobs_lsn[index] = lsn
        self.status[status_page] = [home(status_core(actor, 1), 0), lsn]
        put(self.root, 88, get(self.root, 88) + 1)
        self.advance_root(lsn)
        self.tx = dict(actor=actor, status=status_page, chain=[lsn, 0, lsn, 1],
                       inventory=(lsn, 0, sha(b'')), consumed=BEGIN, mutation=0, credit=False)
        slot = ordinal % 15
        return self.emit(1, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                                  ('status', status_page, (0, 0))], before,
                         txn=(actor[:40], 0), owners=([None, aging_before],
                                                      [self.tx_descriptor(), self.aging_descriptor()]),
                         core=core256(1)), ordinal

    def terminal_of(self, actor):
        for ordinal, job in enumerate(self.bank):
            if job[6] and job[7] == 1 and job[16:64] == actor:
                return ordinal
        raise ValueError('no terminal')

    def end(self, lsn, commit):
        actor = self.tx['actor']
        ordinal = self.terminal_of(actor)
        index = ordinal // 15
        page_no = self.tx['status']
        roles = [('root', 0), ('jobs', index), ('status', page_no)]
        before = self.snapshot(roles)
        tx_before, aging_before = self.tx_descriptor(), self.aging_descriptor()
        cell, _ = self.status[page_no]
        incoming = get(cell, 8)
        head = core_refs(cell[32:])[0]
        job = self.bank[ordinal]
        job[6] = 2
        job[120] = 1 if commit else 2
        put(job, 72, lsn)
        q = 0
        if commit:
            q = get(self.root, 408) + 1
            put(job, 264, q)
            put(self.root, 408, q)
            core = status_core(actor, 3, q=q, head=head)
        else:
            assert head is None and incoming == 0
            core = status_core(actor, 4)
        job[328:360] = sha(tx_before)
        put(job, 376, lsn)
        self.jobs_lsn[index] = lsn
        self.status[page_no] = [home(core, incoming), lsn]
        self.advance_root(lsn)
        self.advance_tx(lsn, BEGIN)
        slot = ordinal % 15
        op = 6 if commit else 7
        return self.emit(op, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                                   ('status', page_no, (0, 0))], before,
                         txn=(actor[:40], self.tx['chain'][1]),
                         owners=([tx_before, aging_before], [self.tx_descriptor(), self.aging_descriptor()]),
                         core=core256(op, q=q))

    def terminal_finish(self, lsn, ordinal):
        index = ordinal // 15
        job = self.bank[ordinal]
        page_no = get(job, 88)  # Status PageRef.page_no
        roles = [('root', 0), ('jobs', index), ('status', page_no)]
        before = self.snapshot(roles)
        aging_before = self.aging_descriptor()
        cursor = get(self.root, 72)
        generation = get(job, 56)
        self.bank[ordinal] = job_free(get(job, 8))
        self.jobs_lsn[index] = lsn
        self.status[page_no] = [home(status_free(generation), 0), lsn]
        put(self.root, 376, get(self.root, 376) + 1)
        self.advance_root(lsn, TERMINAL_FINISH, ordinal)
        slot = ordinal % 15
        return self.emit(17, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                                   ('status', page_no, (0, 0))], before,
                         owners=([aging_before], [self.aging_descriptor()]),
                         core=core256(17, cursors=(cursor, (ordinal + 1) % 64)))

    def status_patch(self, op, lsn, statement=None, mark=None):
        """RollbackStart (statement None: transaction) or StatementFinish."""
        page_no = self.tx['status']
        actor = self.tx['actor']
        before = self.snapshot([('status', page_no)])
        tx_before = self.tx_descriptor()
        cell, _ = self.status[page_no]
        incoming = get(cell, 8)
        head = core_refs(cell[32:])[0]
        sequence = self.head_sequence(head)
        if op == 3 and statement is None:
            core = status_core(actor, 2, 2, head=head, nxt=head)
            out_core = core256(3, sequence=sequence, current=head, nxt=head)
        elif op == 3:
            core = status_core(actor, 2, 1, head=head, target=mark, nxt=head)
            out_core = core256(3, statement=statement, sequence=sequence, current=head, nxt=head,
                               target=mark)
        else:
            assert mark == head
            core = status_core(actor, 1, head=head)
            out_core = core256(5, statement=statement, sequence=sequence, current=head, nxt=head,
                               target=head)
            self.tx['credit'] = False
        self.status[page_no] = [home(core, incoming), lsn]
        self.advance_tx(lsn, STATUS_PATCH)
        return self.emit(op, lsn, [('status', page_no, (0, 0))], before,
                         txn=(actor[:40], self.tx['chain'][1]),
                         owners=([tx_before], [self.tx_descriptor()]), core=out_core)

    def reserve(self, lsn, container, statement, canon):
        actor = self.tx['actor']
        ordinal = self.first_free()
        index = ordinal // 15
        roles = [('root', 0), ('jobs', index), ('undo', container)]
        before = self.snapshot(roles)
        tx_before, aging_before = self.tx_descriptor(), self.aging_descriptor()
        cell, _ = self.undo[container]
        assert cell == undo_free(get(cell, 8))
        undo = slot_ref(container, get(cell, 8) + 1)
        row = get(self.root, 432) + 1
        sequence = get(self.root, 392) + 1
        mutation = self.tx['mutation'] + 1
        head = core_refs(self.status_core())[0]
        h = head is not None
        value = record(row, canon, actor, mutation, undo)
        size = len(value) + 12
        job = job_free(get(self.bank[ordinal], 8) + 1)
        job[6:8] = b'\x01\x03'
        job[16:64] = actor
        put(job, 64, lsn)
        job[80:120] = slot_ref(self.tx['status'], get(actor, 40))
        job[128:168] = undo
        pack_into('<4Q', job, 168, sequence, *TABLE)
        job[200:224] = page_ref(12, 2)
        job[224:248] = page_ref(13, 2)
        pack_into('<2Q', job, 248, statement, mutation)
        pack_into('<I', job, 280, size)
        job[288:320] = projection(row, value)
        put(job, 320, row)
        put(job, 376, lsn)
        job[384:] = budget(5, 311_608 + 2 * size + 128 * h)
        self.bank[ordinal] = job
        self.canonical[ordinal] = canon
        self.jobs_lsn[index] = lsn
        self.undo[container] = [undo_free(get(cell, 8) + 1), lsn]
        put(self.root, 88, get(self.root, 88) + 1)
        put(self.root, 392, sequence)
        put(self.root, 432, row)
        self.advance_root(lsn)
        item = undo_item(job)
        self.tx['inventory'] = (lsn, 1, sha(item))
        self.tx['mutation'] = mutation
        self.tx['credit'] = True
        self.advance_tx(lsn, RESERVE)
        slot = ordinal % 15
        frame = self.emit(11, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                                    ('undo', container, (0, 0))], before,
                          txn=(actor[:40], self.tx['chain'][1]),
                          owners=([tx_before, aging_before], [self.tx_descriptor(), self.aging_descriptor()]),
                          core=core256(11, statement, mutation, sequence, current=undo),
                          aux=[delta(1, item)], items=1)
        return frame, ordinal

    def cancel(self, lsn, ordinal):
        actor = self.tx['actor']
        index = ordinal // 15
        roles = [('root', 0), ('jobs', index)]
        before = self.snapshot(roles)
        tx_before, aging_before = self.tx_descriptor(), self.aging_descriptor()
        job = self.bank[ordinal]
        item = undo_item(job)
        out_core = core256(12, get(job, 248), get(job, 256), get(job, 168), current=job[128:168])
        self.bank[ordinal] = job_free(get(job, 8))
        self.jobs_lsn[index] = lsn
        put(self.root, 384, get(self.root, 384) + 1)
        self.advance_root(lsn)
        self.tx['inventory'] = (lsn, 0, sha(b''))
        self.advance_tx(lsn, CANCEL)
        slot = ordinal % 15
        return self.emit(12, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot))], before,
                         txn=(actor[:40], self.tx['chain'][1]),
                         owners=([tx_before, aging_before], [self.tx_descriptor(), self.aging_descriptor()]),
                         core=out_core, aux=[delta(3, item)], items=1)

    def row_change(self, op, lsn, ordinal):
        """Forward 2, CLR 4 (both A's frames, rebuilt here) or Prune 20."""
        actor = self.tx['actor'] if self.tx else self.bank[ordinal][16:64]
        job = self.bank[ordinal]
        index = ordinal // 15
        page_no = get(job, 88)  # Status PageRef.page_no
        undo_page = get(job, 136)
        undo = job[128:168]
        sequence = get(job, 168)
        row = get(job, 320)
        size = get(job, 280, 4)
        previous = self.predecessor(actor, sequence)
        h = previous is not None
        h_ref = previous[128:168] if h else None
        h_link = previous[128:168] + previous[168:176] if h else None
        u_link = undo + u64(sequence)
        roles = [('root', 0), ('jobs', index), ('status', page_no), ('undo', undo_page), ('leaf', 0)]
        before = self.snapshot(roles)
        tx_before = self.tx_descriptor() if op != 20 else None
        aging_before = self.aging_descriptor()
        cell, _ = self.status[page_no]
        incoming = get(cell, 8)
        core = cell[32:]
        head, target, nxt = core_refs(core)
        present = job[288:320]
        missing = projection(row, None)
        statement, mutation = get(job, 248), get(job, 256)
        if op == 2:
            value = record(row, self.canonical[ordinal], actor, mutation, undo)
            assert projection(row, value) == present and len(value) + 12 == size
            self.status[page_no] = [home(status_core(actor, 1, head=undo), incoming + 1), lsn]
            self.undo[undo_page] = [undo_material(job, True, 2, 1 if h else 0, h_link), lsn]
            self.rows[row] = value
            job[6] = 2
            put(job, 72, lsn)
            job[328:360] = sha(tx_before)
            put(job, 376, lsn)
            out_core = core256(2, statement, mutation, sequence, current=undo, nxt=h_ref,
                               digests=(missing, present))
            item = undo_item(job)
            slots = (None, None)
        else:
            old_slot = leaf_slot(before['leaf'], row)
            assert head == undo
            if op == 4:
                self.status[page_no] = [home(status_core(actor, 2, core[7], head=h_ref, target=target,
                                                         nxt=h_ref), incoming - 1), lsn]
                del self.rows[row]
                after_digest = missing
                q = 0
            else:
                q = get(core, 56)
                assert core[6] == 3
                self.rows[row] = restamp(self.rows[row], q)
                after_digest = projection(row, self.rows[row])
                self.status[page_no] = [home(status_core(actor, 3, q=q, head=h_ref), incoming - 1), lsn]
            self.undo[undo_page] = [undo_material(job, False, 0, 2 if h else 0), lsn]
            job[6] = 4 if op == 4 else 3
            job[200:248] = bytes(48)
            job[280:320] = bytes(40)
            put(job, 264, q)
            put(job, 272, lsn)
            put(job, 376, lsn)
            job[384:] = budget(4, 4 * RELEASE)
            if op == 4:
                out_core = core256(4, statement, mutation, sequence, current=undo, nxt=h_ref,
                                   target=target, digests=(present, missing))
            else:
                out_core = core256(20, 0, 0, sequence, current=undo, nxt=h_ref,
                                   cursors=(get(self.root, 72), (ordinal + 1) % 64),
                                   digests=(present, after_digest))
            slots = (old_slot, old_slot if op == 20 else None)
        self.jobs_lsn[index] = lsn
        self.leaf_lsn = lsn
        frame_bytes = {2: 91_304 + size, 4: 91_096 + size, 20: 89_496 + 2 * size}[op] + 128 * h
        if op == 20:
            self.advance_root(lsn, frame_bytes, ordinal)
        else:
            self.advance_root(lsn)
            self.advance_tx(lsn, frame_bytes)
            if op == 2:
                self.tx['inventory'] = (lsn, 0, sha(b''))
        if op == 2:
            slots = (None, leaf_slot(self.leaf(), row))
        slot = ordinal % 15

        def aux(at):
            edges, counts = graph(op == 2, actor, u_link, h_link, incoming, at)
            return edges + counts + ([delta(2, item)] if op == 2 else [])

        if op == 20:
            owners = ([aging_before], [self.aging_descriptor()])
            txn = None
        else:
            owners = ([tx_before, aging_before], [self.tx_descriptor(), self.aging_descriptor()])
            txn = (actor[:40], self.tx['chain'][1])
        frame = self.emit(op, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                                    ('status', page_no, (0, 0)), ('undo', undo_page, (0, 0)),
                                    ('leaf', 0, slots)], before, txn=txn, owners=owners,
                          core=out_core, aux=aux, items=int(op == 2), counts=2, edges=3 + h)
        assert len(frame) == frame_bytes, (op, len(frame), frame_bytes)
        return frame

    # ---- DELETE (dev3-row-delete.md) -----------------------------------
    def holder(self, row):
        """The non-Free root Job holding `row`, if any (#classification)."""
        for ordinal, job in enumerate(self.bank):
            if job[6] and job[7] == 3 and get(job, 320) == row:
                return ordinal
        return None

    def leaf_room(self):
        """#leaf-reservation: entry bytes plus D, the sum of C-180 over the
        table's Retained effect-1 roots, stay within the entry capacity."""
        used = sum(len(value) + 16 for value in self.rows.values())
        reserved = sum(get(job, 280, 4) - TOMBSTONE_CELL for job in self.bank
                       if job[6] == 2 and job[7] == 3 and effect(job)
                       and get(job, 176) == TABLE[0])
        assert used + reserved <= TREE_ENTRY_CAPACITY, (used, reserved)
        return TREE_ENTRY_CAPACITY - used - reserved

    def reserve_delete(self, lsn, container, statement, row):
        """Delete RowReserve (#delete-states): INSERT's Reserve without the
        RowId issue; C is the committed, pruned old row's cell."""
        actor = self.tx['actor']
        assert self.holder(row) is None
        old = self.rows[row]
        q_old, logical = get(old, 24), old[72:120]
        assert old == committed_record(row, q_old, logical, old[168:])
        ordinal = self.first_free()
        index = ordinal // 15
        roles = [('root', 0), ('jobs', index), ('undo', container)]
        before = self.snapshot(roles)
        tx_before, aging_before = self.tx_descriptor(), self.aging_descriptor()
        cell, _ = self.undo[container]
        assert cell == undo_free(get(cell, 8))
        undo = slot_ref(container, get(cell, 8) + 1)
        sequence = get(self.root, 392) + 1
        mutation = self.tx['mutation'] + 1
        h = core_refs(self.status_core())[0] is not None
        size = len(old) + 12
        job = job_free(get(self.bank[ordinal], 8) + 1)
        job[6:8] = b'\x01\x03'
        job[16:64] = actor
        put(job, 64, lsn)
        job[80:120] = slot_ref(self.tx['status'], get(actor, 40))
        job[128:168] = undo
        pack_into('<4Q', job, 168, sequence, *TABLE)
        job[200:224] = page_ref(12, 2)
        job[224:248] = page_ref(13, 2)
        pack_into('<2Q', job, 248, statement, mutation)
        pack_into('<I', job, 280, size)
        job[284] = 1
        job[288:320] = projection(row, tombstone(row, actor, mutation, undo))
        put(job, 320, row)
        put(job, 376, lsn)
        job[384:] = budget(5, 311_608 + size + 128 * h)
        self.bank[ordinal] = job
        self.jobs_lsn[index] = lsn
        self.undo[container] = [undo_free(get(cell, 8) + 1), lsn]
        put(self.root, 88, get(self.root, 88) + 1)
        put(self.root, 392, sequence)
        self.advance_root(lsn)
        item = undo_item(job)
        self.tx['inventory'] = (lsn, 1, sha(item))
        self.tx['mutation'] = mutation
        self.tx['credit'] = True
        self.advance_tx(lsn, RESERVE)
        slot = ordinal % 15
        frame = self.emit(11, lsn, [('root', 0, (0, 0)), ('jobs', index, (slot, slot)),
                                    ('undo', container, (0, 0))], before,
                          txn=(actor[:40], self.tx['chain'][1]),
                          owners=([tx_before, aging_before], [self.tx_descriptor(), self.aging_descriptor()]),
                          core=core256(11, statement, mutation, sequence, current=undo),
                          aux=[delta(1, item)], items=1)
        assert self.holder(row) == ordinal
        return frame, ordinal

    def delete_change(self, op, lsn, ordinal):
        """DeleteForward 8, DeleteClr 9 (A's, modelled here) or DeletePrune 23
        (#delete-states): INSERT's Forward/CLR/Prune graph over a tombstone."""
        actor = self.tx['actor']
        job = self.bank[ordinal]
        assert effect(job) == 1
        index = ordinal // 15
        page_no = get(job, 88)
        undo_page = get(job, 136)
        undo = job[128:168]
        sequence = get(job, 168)
        row = get(job, 320)
        size = get(job, 280, 4)
        previous = self.predecessor(actor, sequence)
        h = previous is not None
        h_ref = previous[128:168] if h else None
        h_link = previous[128:168] + previous[168:176] if h else None
        u_link = undo + u64(sequence)
        roles = [('root', 0), ('jobs', index), ('status', page_no), ('undo', undo_page), ('leaf', 0)]
        before = self.snapshot(roles)
        tx_before = self.tx_descriptor() if op != DELETE_PRUNE else None
        aging_before = self.aging_descriptor()
        cell, _ = self.status[page_no]
        incoming = get(cell, 8)
        core = cell[32:]
        head, target, nxt = core_refs(core)
        statement, mutation = get(job, 248), get(job, 256)
        slot = leaf_slot(before['leaf'], row)
        self.leaf_room()
        if op == DELETE_FORWARD:
            old = self.rows[row]
            q_old, logical, canon = get(old, 24), old[72:120], old[168:]
            assert old == committed_record(row, q_old, logical, canon) and len(old) + 12 == size
            tomb = tombstone(row, actor, mutation, undo)
            assert projection(row, tomb) == job[288:320]
            self.deleted[ordinal] = (q_old, logical, canon)
            self.status[page_no] = [home(status_core(actor, 1, head=undo), incoming + 1), lsn]
            self.undo[undo_page] = [delete_undo(job, True, 2, 1 if h else 0, h_link, 0,
                                                (q_old, logical, canon)), lsn]
            self.rows[row] = tomb
            job[6] = 2
            put(job, 72, lsn)
            job[328:360] = sha(tx_before)
            put(job, 376, lsn)
            out_core = core256(op, statement, mutation, sequence, current=undo, nxt=h_ref,
                               digests=(projection(row, old), projection(row, tomb)))
            item = undo_item(job)
            slots = (slot, slot)
        else:
            q_old, logical, canon = self.deleted[ordinal]
            tomb = self.rows[row]
            assert head == undo and tomb == tombstone(row, actor, mutation, undo)
            present = projection(row, tomb)
            assert present == job[288:320]
            if op == DELETE_CLR:
                restored = committed_record(row, q_old, logical, canon)
                self.status[page_no] = [home(status_core(actor, 2, core[7], head=h_ref, target=target,
                                                         nxt=h_ref), incoming - 1), lsn]
                self.rows[row] = restored
                after_digest = projection(row, restored)
                q = 0
                slots = (slot, slot)
            else:
                q = get(core, 56)
                assert core[6] == 3
                del self.rows[row]
                after_digest = projection(row, None)
                self.status[page_no] = [home(status_core(actor, 3, q=q, head=h_ref), incoming - 1), lsn]
                slots = (slot, None)
            self.undo[undo_page] = [delete_undo(job, False, 0, 2 if h else 0, None, 0,
                                                (q_old, logical, None)), lsn]
            job[6] = 4 if op == DELETE_CLR else 3
            job[200:248] = bytes(48)
            job[280:284] = bytes(4)
            job[285:320] = bytes(35)
            put(job, 264, q)
            put(job, 272, lsn)
            put(job, 376, lsn)
            job[384:] = budget(4, 4 * RELEASE)
            if op == DELETE_CLR:
                out_core = core256(op, statement, mutation, sequence, current=undo, nxt=h_ref,
                                   target=target, digests=(present, after_digest))
            else:
                out_core = core256(op, 0, 0, sequence, current=undo, nxt=h_ref,
                                   cursors=(get(self.root, 72), (ordinal + 1) % 64),
                                   digests=(present, after_digest))
        self.jobs_lsn[index] = lsn
        self.leaf_lsn = lsn
        frame_bytes = {DELETE_FORWARD: 91_304 + 2 * size, DELETE_CLR: 91_096 + 2 * size,
                       DELETE_PRUNE: 89_496 + size}[op] + 128 * h
        if op == DELETE_PRUNE:
            self.advance_root(lsn, frame_bytes, ordinal)
        else:
            self.advance_root(lsn)
            self.advance_tx(lsn, frame_bytes)
            if op == DELETE_FORWARD:
                self.tx['inventory'] = (lsn, 0, sha(b''))
        self.leaf_room()
        job_slot = ordinal % 15

        def aux(at):
            edges, counts = graph(op == DELETE_FORWARD, actor, u_link, h_link, incoming, at)
            return edges + counts + ([delta(2, item)] if op == DELETE_FORWARD else [])

        if op == DELETE_PRUNE:
            owners = ([aging_before], [self.aging_descriptor()])
            txn = None
        else:
            owners = ([tx_before, aging_before], [self.tx_descriptor(), self.aging_descriptor()])
            txn = (actor[:40], self.tx['chain'][1])
        frame = self.emit(op, lsn, [('root', 0, (0, 0)), ('jobs', index, (job_slot, job_slot)),
                                    ('status', page_no, (0, 0)), ('undo', undo_page, (0, 0)),
                                    ('leaf', 0, slots)], before, txn=txn, owners=owners,
                          core=out_core, aux=aux, items=int(op == DELETE_FORWARD), counts=2,
                          edges=3 + h)
        assert len(frame) == frame_bytes, (op, len(frame), frame_bytes)
        return frame

    def release(self, lsn, ordinal, free):
        job = self.bank[ordinal]
        index = ordinal // 15
        undo_page = get(job, 136)
        roles = [('root', 0), ('jobs', index), ('undo', undo_page)]
        before = self.snapshot(roles)
        aging_before = self.aging_descriptor()
        cursor = job[121]
        root_cursor = get(self.root, 72)
        out_core = core256(22 if free else 21, sequence=get(job, 168), current=job[128:168],
                           cursors=(root_cursor, (ordinal + 1) % 64))
        old_edge = self.undo[undo_page][0][326]
        if free:
            assert cursor == 3
            self.undo[undo_page] = [undo_free(get(job, 160)), lsn]
            self.bank[ordinal] = job_free(get(job, 8))
            put(self.root, 376, get(self.root, 376) + 1)
        else:
            assert cursor < 3
            edge_tag = 3 if cursor + 1 >= 2 else old_edge
            if effect(job):
                q_old, logical, _ = self.deleted[ordinal]
                prior = (q_old if cursor + 1 < 3 else None, logical, None)
                material = delete_undo(job, False, 0, edge_tag, None, cursor + 1, prior)
            else:
                material = undo_material(job, False, 0, edge_tag, cursor=cursor + 1)
            self.undo[undo_page] = [material, lsn]
            job[121] = cursor + 1
            job[384:] = budget(3 - cursor, (3 - cursor) * RELEASE)
            put(job, 376, lsn)
        self.jobs_lsn[index] = lsn
        self.advance_root(lsn, RELEASE, ordinal)
        slot = ordinal % 15
        return self.emit(22 if free else 21, lsn,
                         [('root', 0, (0, 0)), ('jobs', index, (slot, slot)), ('undo', undo_page, (0, 0))],
                         before, owners=([aging_before], [self.aging_descriptor()]), core=out_core)


# ---- variants rejected by the whole-group observer ---------------------------

def reseal(frame):
    out = bytearray(frame)
    out[-32:] = sha(out[:-32])
    return bytes(out)


def sections(frame):
    handoff = frame[7] == 3
    size = 256 if handoff else 192
    group = frame[160:160 + size]
    descriptors = 2 if handoff else 1
    core_at = 160 + size + 1536 * descriptors
    native_at = core_at + 256
    native_bytes = get(group, 228 if handoff else 136, 4)
    aux_at = native_at + native_bytes
    edges = get(group, 244 if handoff else 126, 2)
    counts = get(group, 246 if handoff else 128, 2)
    items = get(group, 248 if handoff else 130, 2)
    redo_at = aux_at + edges * 128 + counts * 96 + items * 208
    return dict(group=160, core=core_at, native=native_at, edges=aux_at,
                counts=aux_at + edges * 128, items=aux_at + edges * 128 + counts * 96,
                redo=redo_at, descriptor=160 + size)


def native_at(frame, kind):
    at = sections(frame)['native']
    while frame[at] != kind:
        at += get(frame, at + 20, 4)
    return at


def mutate(frame, how):
    out = bytearray(frame)
    s = sections(frame)
    if how == 'count-after':  # T CountChange after +1 (counts are opaque to framing)
        at = s['counts'] + 96 + 64
        put(out, at, get(out, at) + 1)
    elif how == 'edge-sequence':  # first edge's new/old target sequence +1
        at = s['edges'] + (64 if any(out[s['edges'] + 64:s['edges'] + 120]) else 8) + 48
        put(out, at, get(out, at) + 1)
    elif how == 'item-lsn':  # InventoryDelta Item96 change LSN +1
        at = s['items'] + (112 if out[s['items']] == 1 else 16) + 72
        put(out, at, get(out, at) + 1)
    elif how.startswith('native-'):  # one after-cell byte, cell digest kept consistent
        kind, offset = (int(part) for part in how[7:].split('@'))
        at = native_at(frame, kind)
        before_len = get(out, at + 12, 4)
        after_len = get(out, at + 16, 4)
        cell_at_ = at + 128 + before_len
        out[cell_at_ + offset] ^= 1
        out[at + 56:at + 88] = sha(out[cell_at_:cell_at_ + after_len])
    elif how.startswith('redo-'):  # one byte inside the last redo record data
        offset = int(how[5:])
        at = s['redo']
        while at + get(out, at + 4, 4) < len(out) - 32:
            at += get(out, at + 4, 4)
        out[at + 64 + offset] ^= 1
    elif how == 'core-digest':  # after projection digest
        out[s['core'] + 208] ^= 1
    elif how.startswith('core-'):  # one core u64 field +1
        at = s['core'] + int(how[5:])
        put(out, at, get(out, at) + 1)
    elif how.startswith('descriptor-'):  # descriptor u64 +1, its own SHA resealed
        ordinal, offset = (int(part) for part in how[11:].split('@'))
        at = s['descriptor'] + 1536 * ordinal
        put(out, at + offset, get(out, at + offset) + 1)
        out[at + 1504:at + 1536] = sha(out[at:at + 1504])
    elif how == 'group-digest':  # first prior-owner digest flipped
        handoff = frame[7] == 3
        out[160 + (184 if handoff else 88)] ^= 1
    else:
        raise ValueError(how)
    return reseal(out)


# ---- histories H (re-run) and D ----------------------------------------------

def history_h(out):
    h = History.from_a('h-500', [0, 1], [10, 11])
    h.canonical[16] = canonical(42, 'xadb')
    assert h.row_change(2, 501, 16) == a('h-501-forward.wal')
    h.check('h-501', [0, 1], [11])
    mark = slot_ref(10, 2)
    out['H-502'] = h.status_patch(3, 502, statement=2, mark=mark)
    h.check('h-502', names=('status', 'tx'))
    assert h.row_change(4, 503, 16) == a('h-503-clr.wal')
    h.check('h-503', [1], [11])
    out['H-504'] = h.status_patch(5, 504, statement=2, mark=mark)
    h.undo[14] = [undo_free(2), 60]
    out['H-undo14-0'] = h.undo_page(14)
    out['H-505-canonical'] = canonical(10, 'ten')
    out['H-505'], ordinal = h.reserve(505, 14, 3, out['H-505-canonical'])
    assert ordinal == 17
    out['H-506'] = h.cancel(506, 17)
    out['H-507-canonical'] = canonical(11, None)
    out['H-507'], ordinal = h.reserve(507, 14, 4, out['H-507-canonical'])
    assert ordinal == 17
    out['H-508'] = h.row_change(2, 508, 17)
    out['H-509'] = h.end(509, True)
    out['H-510'] = h.row_change(20, 510, 17)
    out['H-511'] = h.row_change(20, 511, 15)
    for lsn in (512, 513, 514):
        out[f'H-{lsn}'] = h.release(lsn, 16, False)
    out['H-515'] = h.release(515, 16, True)
    return h


def history_d(out, h):
    """dev3-row-delete.md#delete-states from the end of H; see the docstring."""
    h.status[20] = [home(status_free(1), 0), 60]
    out['D-status20-0'] = h.status_page(20)
    for page_no in (15, 16):
        h.undo[page_no] = [undo_free(2), 60]
        out[f'D-undo{page_no}-0'] = h.undo_page(page_no)
    out['D-516'], terminal = h.begin(516, 6, 2, 20)
    assert terminal == 18
    # T's committed, pruned INSERT roots still hold rows 4 and 11 (Busy).
    assert (h.holder(4), h.holder(11)) == (15, 17)
    for lsn in (517, 518, 519):
        out[f'D-{lsn}'] = h.release(lsn, 15, False)
    out['D-520'] = h.release(520, 15, True)
    assert h.holder(4) is None and h.undo[10][0] == undo_free(2)
    out['D-521'], p = h.reserve_delete(521, 11, 1, 2)
    out['D-522'] = h.cancel(522, p)
    out['D-523'], p = h.reserve_delete(523, 11, 2, 2)
    out['D-524'] = h.delete_change(DELETE_FORWARD, 524, p)
    out['D-525'], q = h.reserve_delete(525, 10, 3, 9)
    out['D-526'] = h.delete_change(DELETE_FORWARD, 526, q)
    mark = h.bank[p][128:168]
    out['D-527'] = h.status_patch(3, 527, statement=3, mark=mark)
    out['D-528'] = h.delete_change(DELETE_CLR, 528, q)
    out['D-529'] = h.status_patch(5, 529, statement=3, mark=mark)
    # The compensated root keeps row 9 until T2 ends (Busy).
    assert h.holder(9) == q
    out['D-530-canonical'] = canonical(12, 'twelve')
    out['D-530'], r = h.reserve(530, 15, 4, out['D-530-canonical'])
    out['D-531'] = h.row_change(2, 531, r)
    out['D-532'], s = h.reserve_delete(532, 16, 5, 4)
    out['D-533'] = h.delete_change(DELETE_FORWARD, 533, s)
    out['D-534'] = h.end(534, True)
    out['D-535'] = h.delete_change(DELETE_PRUNE, 535, s)
    out['D-536'] = h.row_change(20, 536, r)
    out['D-537'] = h.delete_change(DELETE_PRUNE, 537, p)
    for lsn in (538, 539, 540):
        out[f'D-{lsn}'] = h.release(lsn, p, False)
    out['D-541'] = h.release(541, p, True)
    for lsn in (542, 543, 544):
        out[f'D-{lsn}'] = h.release(lsn, q, False)
    out['D-545'] = h.release(545, q, True)
    print('ordinals', dict(terminal=terminal, p=p, q=q, r=r, s=s))
    return h


# Rejected variants: (name, base frame, mutation). Every variant keeps framing,
# owners, record shapes and digests valid; only the semantic graph differs.
REJECTS = [
    ('R-D-521-effect', 'D-521', 'native-9@284'),
    ('R-D-524-undo', 'D-524', 'native-3@440'),
    ('R-D-524-leaf', 'D-524', 'native-1@124'),
    ('R-D-526-item', 'D-526', 'item-lsn'),
    ('R-D-528-restore', 'D-528', 'native-1@182'),
    ('R-D-528-count', 'D-528', 'count-after'),
    ('R-D-531-edge', 'D-531', 'edge-sequence'),
    ('R-D-533-descriptor', 'D-533', 'descriptor-0@608'),
    ('R-D-535-digest', 'D-535', 'core-digest'),
    ('R-D-537-undo', 'D-537', 'native-3@224'),
    ('R-D-538-stamp', 'D-538', 'native-3@224'),
]


def main():
    global PROFILE
    PROFILE = a('profile3.bin')
    replay = {}
    h = history_h(replay)
    for name, data in replay.items():
        assert data == n1(name), name
    out = {}
    history_d(out, h)
    for name, base, how in REJECTS:
        source = out[base]
        variant = mutate(source, how)
        assert variant != source and len(variant) == len(source), name
        out[name] = variant
    blobs = bytearray()
    rows = []
    for name, data in out.items():
        rows.append(f'{name}\t{len(blobs)}\t{len(data)}\t{sha256(data).hexdigest()}')
        blobs += data
    assert READ == set(PARENTS), sorted(set(PARENTS) - READ)
    (HERE / 'blobs.bin').write_bytes(bytes(blobs))
    (HERE / 'index.tsv').write_text('\n'.join(rows) + '\n')
    plan = dict(scope='scope3 Row DELETE groups (dev3-row-delete.md) after the N1 history H',
                oracle='independent byte packing and SHA-256 only; A and N1 literals re-encoded before use',
                parents={name: PARENTS[name] for name in sorted(PARENTS)},
                rejects=[dict(name=n, base=b, mutation=m) for n, b, m in REJECTS])
    (HERE / 'input-plan.json').write_text(json.dumps(plan, indent=2) + '\n')


if __name__ == '__main__':
    main()
