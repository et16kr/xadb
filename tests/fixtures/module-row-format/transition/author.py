#!/usr/bin/env python3
"""N1 independent scope3 Row transition literals; run as `python3 -I author.py`.

Only hashlib/pathlib/struct/json. No XADB encoder, decoder, test helper or
other fixture author is imported or executed. A's MCA00-V06 snapshots/frames
and the root3 genesis cells are read as bytes only, pinned by SHA-256, and
every one of them is re-encoded here before it is used (codec cross-check).
Byte positions follow dev3-obligation-format, dev3-module-consumers
(row-states, row-shape, Row ledger), dev3-create-group-format (scope3 body
order) and the scope3 wire layout. Three histories are simulated:

E  genesis: AgingStart100, two empty Row transactions (commit, full abort).
N  A's noh-300: A Forward301, RollbackStart302, A CLR303, AbortEnd304,
   ReleaseRole305-307, RootFree308, TerminalFinish309.
H  A's h-500: A Forward501, statement RollbackStart502, A CLR503,
   StatementFinish504, Reserve505, Cancel506, Reserve507, Forward508 (this
   model, observe-only in Rust), Commit509, Prune510 (h1), Prune511 (h0),
   ReleaseRole512-514, RootFree515.

Contract readings chosen where the text leaves a field open (each is also
enforced by the Rust observer): Begin control 1x3040; the first Reserve of a
statement restores 3x3040; StatementFinish returns to 1x3040; Reserve issues
RowId=row_high+1, sequence=undo_high+1, mutation=last+1; ReleaseRole/RootFree
require the actor's TerminalReady; TerminalFinish/RootFree keep generations.
"""
from hashlib import sha256
from pathlib import Path
from struct import pack, pack_into, unpack_from
import json

HERE = Path(__file__).resolve().parent
FIXTURES = HERE.parent.parent
PARENTS = {
    'module-row-insert/scope3/profile3.bin': '8335423a65c2870d4c3d115ef5011e3c82d2fc2b4341e7e10d5b7321ec5a3934',
    'module-row-insert/scope3/anchor.bin': '47809208242862c6d8fa75cf2beb496f0f04cc55a0871edfe333fe47248d30d8',
    'module-row-insert/scope3/h-500-aging.bin': 'cdae28875cc0f13764e22ba76f2707e0c3c3f71593c8b8b7a30b0e0d702787c6',
    'module-row-insert/scope3/h-500-bank.bin': '52b8ba5f93ef087ff60606bd0656840152074923550cc86e860b952235556c81',
    'module-row-insert/scope3/h-500-jobs2.bin': 'dbd19662a93dff63885fa09016faedfc406b01a1ed01217506c67ed0f03fc628',
    'module-row-insert/scope3/h-500-jobs3.bin': '37a290fb7e7fc6b4e18ed4e1a5ec74af344f3b8721e6c910fde480265c03c109',
    'module-row-insert/scope3/h-500-leaf.bin': '53aba09198d8a858f3a963c248db4aaa68777b01e4cb9e9d3a8d086b534bf0ac',
    'module-row-insert/scope3/h-500-root.bin': '3e30315fdcbc65be40699fc3fcc99392e9bcb29a5971710c7029191888415495',
    'module-row-insert/scope3/h-500-status.bin': '37faf4cf4b6b016d3ea5e636a5b4a27cb4d8fc2b74e05128338fa2c350d58a8e',
    'module-row-insert/scope3/h-500-tx.bin': 'c276fe136719aa7773a342ff4c56582401b176607b696ce2af439676f1385ee5',
    'module-row-insert/scope3/h-500-undo10.bin': '6ab2d3a2dc9dbae486d5a0895fb9a1f92ba0716f579bc9513c3b20ce12a1ee7e',
    'module-row-insert/scope3/h-500-undo11.bin': '07f6d0cbd44e58526b472a9a3388c293e8897a8eaceeb3f8a89596644c8083db',
    'module-row-insert/scope3/h-501-aging.bin': '9f79cc68e1023f8f13bfec376f0699beeff5ea51b15ed5aef59e2b97a31c6d6c',
    'module-row-insert/scope3/h-501-bank.bin': 'b2840f4683037fc2eba5403bd2a46c2b51ba8428ec254d324fdfa55079b2bede',
    'module-row-insert/scope3/h-501-forward.wal': '43848ceb656e03e6cc0cc01cdf1c69bdac7f88d6de932942795b815aa64a650b',
    'module-row-insert/scope3/h-501-jobs2.bin': 'dbd19662a93dff63885fa09016faedfc406b01a1ed01217506c67ed0f03fc628',
    'module-row-insert/scope3/h-501-jobs3.bin': '828e458a0fcd3887e866e6d5b21613d38a538597cca9156399d354c96878aec4',
    'module-row-insert/scope3/h-501-leaf.bin': 'b451ed8bcaa6ce84fa62876beac248dba93aedbedc690a70e5fc6bed5ffd116b',
    'module-row-insert/scope3/h-501-root.bin': 'd9256e603318df318d3d5f7073bb82ce866d9f1627b88ba3bb73206207b41ef2',
    'module-row-insert/scope3/h-501-status.bin': 'ee58aedd6182d6f2f9d4e2b6846305899997be3742c4a8a48111729721afc1c6',
    'module-row-insert/scope3/h-501-tx.bin': 'b57724b0f0e51bb515391aa08065a1f322a4f370a1000307a72a8091081055c6',
    'module-row-insert/scope3/h-501-undo11.bin': 'fbdae72a06d0e76155f4181e4be763360fe935edb187bc2ab74f3a71fd2a2bd1',
    'module-row-insert/scope3/h-502-status.bin': 'a60413c2ea6cf04175db3bc536018111e3982ec8f05b801770f3cf75a9fa308e',
    'module-row-insert/scope3/h-502-tx.bin': '157e6dd1c041536a4b8ba2521f1fbf774824b9873c1725674cc40273d46f1473',
    'module-row-insert/scope3/h-503-aging.bin': 'ab3e33882bf994a75897cbe61aeb0087905cbfa98b703ffbf358bf866bc8246b',
    'module-row-insert/scope3/h-503-bank.bin': '7deffdb41582c1e7d1f2d5513c4d8c8b0402d461c74d706b5b40c6c97fb64cc6',
    'module-row-insert/scope3/h-503-clr.wal': '96238da38c9b0ba4a236808c13a278513fd7eb9cf901418c4c21cc099bf66367',
    'module-row-insert/scope3/h-503-jobs3.bin': '0c289bd95d11cdb0e73483e8090410475b15e7285dc09d5bcb4029ce3bc893ce',
    'module-row-insert/scope3/h-503-leaf.bin': '439a518e203b674475caf65bf2de1b42b1243d5235930edb82dc3b8dd177c568',
    'module-row-insert/scope3/h-503-root.bin': '07d501fa7f4c0c30b842d67ff2f9d478154fd29a1fb18eed1d287f336aeef1be',
    'module-row-insert/scope3/h-503-status.bin': '875c8ff8e78e5e915d8344b3139bca47e9b95af5604687cd32a3c4dfdce4423b',
    'module-row-insert/scope3/h-503-tx.bin': 'eac0effed85df8b28acecd181b839aad2d75f0bcda071c261122a69544e2e229',
    'module-row-insert/scope3/h-503-undo11.bin': '1e2706f2120d9fbe4fcbb75ce99fd315db3475d145db0a1965b8c7fc77f19d6d',
    'module-row-insert/scope3/noh-300-aging.bin': 'f6e70affe927b26871301e38cb3988ae827b3b23e15938e4c179c099c53d9c2c',
    'module-row-insert/scope3/noh-300-bank.bin': '749bca03e94d92952cd148264ff831032705e3a3c471b4c3cb67f615f09f9e91',
    'module-row-insert/scope3/noh-300-jobs2.bin': '674836beb88b8c7f7a1e8680694d40c21e96fd0f9f9a4dc753c49977f2ef22ab',
    'module-row-insert/scope3/noh-300-leaf.bin': '9368dd2fc059c059b1deed137c40191119fd9820d293e48f2743dc318adb8b93',
    'module-row-insert/scope3/noh-300-root.bin': '6daa975bc66f7bd5ca89e04fbc204e71bafc802c9c61188df25f4a1c38577c79',
    'module-row-insert/scope3/noh-300-status.bin': 'cb5405c0004e3df2b6e814ab8e071bdc0eb21599003e5e8b7802ba4476618842',
    'module-row-insert/scope3/noh-300-tx.bin': 'e19753bf1c09f8da7da96642555a028e8f769d27ffd33ca030397df983516ea5',
    'module-row-insert/scope3/noh-300-undo11.bin': 'febd55ded07c01aeba2cbdc9efb2e2da60f81a35a7e002574b19573a5b6573a6',
    'module-row-insert/scope3/noh-301-aging.bin': '9674b2e263a739eea600f840916e929c14725719067095e75b90814a371577f2',
    'module-row-insert/scope3/noh-301-bank.bin': 'b02b6ad366b8cc5692a5a3711c9ee5dd06e569f9ff26c945fb64ba06bb192baa',
    'module-row-insert/scope3/noh-301-forward.wal': 'd8948457a8b7ff532ca8ebeb6f4f0e6dc26fe07e81b439c952340b364f88c640',
    'module-row-insert/scope3/noh-301-jobs2.bin': '6ffd22da9d3084d6eee9bab779d77020a71e032592d0da237c89ad3db50e0ed6',
    'module-row-insert/scope3/noh-301-leaf.bin': '88a380a97d06fec1d483cdc1b90e011c01d1578bc512632b8357446b8d96943d',
    'module-row-insert/scope3/noh-301-root.bin': 'ed2d657771fcd03870033a12fb15e8e2840e7c4cd63fed3a2703d2d4c1424732',
    'module-row-insert/scope3/noh-301-status.bin': '95e82c292768304b1b680618a4cb126a4571d805d4a363ab9528c3cbab77909d',
    'module-row-insert/scope3/noh-301-tx.bin': '5e2007230657517f2ea6ed571da425407e64c49c3f3dd17655057a5820357ca9',
    'module-row-insert/scope3/noh-301-undo11.bin': 'dfc78fa02f4080b8a8c70f90217adba94d5a2138374933cafb4fb564d3b1cc80',
    'module-row-insert/scope3/noh-302-status.bin': '9d57b91758bc99b08d940a1f2e3110b860324c816ddd7cd6af83e5b2c52e2e39',
    'module-row-insert/scope3/noh-302-tx.bin': '74f47d60e6eb19ee99833367a634c59ea124990a1e416d20a8d6f9edbc493bb0',
    'module-row-insert/scope3/noh-303-aging.bin': 'c3411d6adb6779dab55c41c4bac2d7750befb3feeff925318e9417019b986e69',
    'module-row-insert/scope3/noh-303-bank.bin': '303b6ed77b22a83f1d2598a4916bf9b8d16b931f8253e2dfdfdde50e50d8eff9',
    'module-row-insert/scope3/noh-303-clr.wal': '6c56e0f60baf42992acc0ba1c374e727ebbc2af020713a09be04cd11896bb9ed',
    'module-row-insert/scope3/noh-303-jobs2.bin': '89cb8b3d8077020e14accc486d229dbb1005b7e6bdaf95abdcd29527c5784751',
    'module-row-insert/scope3/noh-303-leaf.bin': '9c7bf464eb365693107f93c17208de43d3038ec481138169ec730d47419ff5f4',
    'module-row-insert/scope3/noh-303-root.bin': 'bf3ab737a018ecdc18ab930e77420e1fc6e74322b9fe7aa88a3153e150771cc0',
    'module-row-insert/scope3/noh-303-status.bin': '6052538ec87d742807f75a1860070157a0b9ef51a3df03a0e64cecd94bf572f5',
    'module-row-insert/scope3/noh-303-tx.bin': '25c3d8387c226509d375f59c98c42da4720a2237238055c02230a13154705b7d',
    'module-row-insert/scope3/noh-303-undo11.bin': '0218bb5fa33da73c81f952ef5adbbb8298187b412d8655ac6084eb10bae1d117',
    'module-row-format/root3/initial-root.bin': 'd6079c1ea15ea2668be3150c6128d9be4a72e28855336ea7c1d971f73d0b3fc4',
    'module-row-format/root3/free-job.bin': 'aee93979e2d5bf1901a0daf894a8f8c705fcb3335581f215f405b6326a8dbe69',
}


def parent(name):
    data = (FIXTURES / name).read_bytes()
    assert sha256(data).hexdigest() == PARENTS[name], name
    return data


def a(name):
    return parent('module-row-insert/scope3/' + name)


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
OUTER = {1: 1, 2: 2, 3: 2, 4: 4, 5: 2, 6: 5, 7: 6, 11: 2, 12: 2,
         16: 3, 17: 3, 20: 3, 21: 3, 22: 3}
HANDOFF = {1, 2, 4, 6, 7, 11, 12}
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


def has_h(job):
    return get(job, 384 + 400) == 311_736 + 2 * get(job, 280, 4)


def tx_rows(bank, actor, core, credit):
    """consumers.md Row ledger: CLR debt per Reserved/Retained root while not
    terminal, the Reserved Forward row, the terminal row and control."""
    state, rollback = core[6], core[7]
    terminal = state in (3, 4)
    steps = wal = 0
    reserved = None
    for job in bank:
        if job[6] in (1, 2) and job[7] == 3 and job[16:64] == actor:
            cell, h = get(job, 280, 4), has_h(job)
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
            self.undo[undo_page] = [undo_material(job, False, 0, edge_tag, cursor=cursor + 1), lsn]
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


# ---- the three histories ------------------------------------------------------

def genesis_root():
    cell = bytearray(parent('module-row-format/root3/initial-root.bin'))
    cell[192:208] = DB  # namespace substitution, as genesis3/author.py
    return cell


def history_e(out):
    h = History()
    h.root = genesis_root()
    free = parent('module-row-format/root3/free-job.bin')
    h.bank = [bytearray(free) for _ in range(64)]
    h.refresh()
    assert bytes(h.root) == bytes(genesis_root())
    h.jobs_lsn = {index: 0 for index in range(5)}
    h.status[20] = [home(status_free(1), 0), 10]
    out['E-root-0'] = h.root_page()
    out['E-jobs0-0'] = h.jobs_page(0)
    out['E-status20-0'] = h.status_page(20)
    out['E-100'] = h.aging_start(100)
    out['E-101'], first = h.begin(101, 7, 2, 20)
    out['E-102'] = h.end(102, True)
    out['E-103'] = h.terminal_finish(103, first)
    out['E-104'], second = h.begin(104, 8, 3, 20)
    out['E-105'] = h.status_patch(3, 105)
    out['E-106'] = h.end(106, False)
    out['E-107'] = h.terminal_finish(107, second)
    assert (first, second) == (0, 1)
    return h


def history_n(out):
    h = History.from_a('noh-300', [0], [11])
    h.canonical[3] = canonical(42, 'xadb')
    assert h.row_change(2, 301, 3) == a('noh-301-forward.wal')
    h.check('noh-301', [0], [11])
    out['N-302'] = h.status_patch(3, 302)
    h.check('noh-302', names=('status', 'tx'))
    assert h.row_change(4, 303, 3) == a('noh-303-clr.wal')
    h.check('noh-303', [0], [11])
    out['N-304'] = h.end(304, False)
    for lsn in (305, 306, 307):
        out[f'N-{lsn}'] = h.release(lsn, 3, False)
    out['N-308'] = h.release(308, 3, True)
    out['N-309'] = h.terminal_finish(309, 2)
    return h


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


# Rejected variants: (name, base frame, mutation). Every variant keeps framing,
# owners, record shapes and digests valid; only the semantic graph differs.
REJECTS = [
    ('R-A-noh-301-count', 'A-noh-301', 'count-after'),
    ('R-A-noh-301-status', 'A-noh-301', 'native-5@8'),
    ('R-A-h-501-edge', 'A-h-501', 'edge-sequence'),
    ('R-A-h-501-item', 'A-h-501', 'item-lsn'),
    ('R-A-noh-303-redo', 'A-noh-303', 'redo-4000'),
    ('R-A-h-503-core', 'A-h-503', 'core-24'),
    ('R-A-h-503-descriptor', 'A-h-503', 'descriptor-0@608'),
    ('R-E-100-descriptor', 'E-100', 'descriptor-0@608'),
    ('R-E-101-job', 'E-101', 'native-9@392'),
    ('R-E-102-q', 'E-102', 'core-152'),
    ('R-E-107-cursor', 'E-107', 'core-168'),
    ('R-N-302-patch', 'N-302', 'redo-7'),
    ('R-N-305-undo', 'N-305', 'native-3@328'),
    ('R-H-505-item', 'H-505', 'item-lsn'),
    ('R-H-506-sequence', 'H-506', 'core-24'),
    ('R-H-509-owner', 'H-509', 'group-digest'),
    ('R-H-510-digest', 'H-510', 'core-digest'),
    ('R-H-511-count', 'H-511', 'count-after'),
    ('R-H-515-root', 'H-515', 'native-8@376'),
]


def main():
    global PROFILE
    PROFILE = a('profile3.bin')
    out = {}
    history_e(out)
    history_n(out)
    history_h(out)
    bases = {'A-noh-301': a('noh-301-forward.wal'), 'A-noh-303': a('noh-303-clr.wal'),
             'A-h-501': a('h-501-forward.wal'), 'A-h-503': a('h-503-clr.wal')}
    for name, base, how in REJECTS:
        source = bases.get(base) or out[base]
        variant = mutate(source, how)
        assert variant != source and len(variant) == len(source), name
        out[name] = variant
    blobs = bytearray()
    rows = []
    for name, data in out.items():
        rows.append(f'{name}\t{len(blobs)}\t{len(data)}\t{sha256(data).hexdigest()}')
        blobs += data
    (HERE / 'blobs.bin').write_bytes(bytes(blobs))
    (HERE / 'index.tsv').write_text('\n'.join(rows) + '\n')
    plan = dict(scope='N1 C-owned scope3 Row groups (pure, uncalled) and the whole-group observer',
                oracle='independent byte packing and SHA-256 only; A literals re-encoded before use',
                parents={name: PARENTS[name] for name in sorted(PARENTS)},
                rejects=[dict(name=n, base=b, mutation=m) for n, b, m in REJECTS])
    (HERE / 'input-plan.json').write_text(json.dumps(plan, indent=2) + '\n')


if __name__ == '__main__':
    main()
