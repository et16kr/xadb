"""Write fixed C0 byte observations; never import or run the engine.

These are not authenticated pages, an installed database, or a recovery input.
The byte positions follow dev3-page-format/dev3-row-format/dev3-tree-format.
The scope3 section below follows dev3-obligation-format/dev3-module-consumers
(XWF1/XHG1 Forward/CLR, XOD1, Root3/Job3, XSH1/XTS1, XUR1) and simulates the
history before each case only with byte packing and SHA-256.
"""

from hashlib import sha256
from pathlib import Path
from struct import pack, pack_into, unpack_from

HERE = Path(__file__).parent
PAGE_BYTES = 16384
DIGEST = PAGE_BYTES - 32
DATABASE = bytes([1]) * 16


def row_record(row, canonical=bytes.fromhex("010000000101"), committed=False):
    result = bytearray(168 + len(canonical))
    result[:4] = b"XRW1"
    pack_into("<HBBQ", result, 4, 1, 0 if committed else 2, 1, row)
    result[16] = 1 if committed else 2
    for start in ((72,) if committed else (24, 72)):
        result[start : start + 16] = DATABASE
        pack_into("<QI", result, start + 16, 2, 3)
        pack_into("<Q", result, start + 32, 4)
    if committed:
        pack_into("<Q", result, 24, 7)
    else:
        pack_into("<Q", result, 64, 5)
    pack_into("<Q", result, 112, row - 1)
    if not committed:
        pack_into("<QQQH", result, 120, 0x100000001, min(row + 20, 63), 1, 0)
        pack_into("<Q", result, 152, 2)
    pack_into("<I", result, 160, len(canonical))
    result[168:] = canonical
    return bytes(result)


def row_leaf(rows, lsn, canonical=bytes.fromhex("010000000101"), committed_through=0, corrupt_row=0):
    metadata = b"XBT1\x01\x00" + bytes(26)
    cells = [metadata]
    for row in rows:
        record = row_record(row, canonical, row <= committed_through)
        if row == corrupt_row:
            record = b"BAD!" + record[4:]
        cells.append(pack("<HH", 8, len(record)) + row.to_bytes(8, "big") + record)
    result = bytearray(PAGE_BYTES)
    result[:4] = b"XDP1"
    pack_into("<HBB", result, 4, 1, 1, 0)
    result[8:24] = DATABASE
    pack_into("<QQQQQQ", result, 24, 0x100000001, 10, 1, 20, 1, lsn)
    upper = DIGEST
    for slot, cell in enumerate(cells):
        upper -= len(cell)
        pack_into("<HH", result, 128 + slot * 4, upper, len(cell))
        result[upper : upper + len(cell)] = cell
    pack_into("<HHH", result, 72, len(cells), 128 + 4 * len(cells), upper)
    result[DIGEST:] = sha256(result[:DIGEST]).digest()
    return bytes(result)


# MCA00-V06: scope3 Row INSERT Forward/CLR literals, H absent and present.
# The history before each case (AgingStart, empty terminal cleanups, Begin,
# Reserve, statement/transaction RollbackStart) is simulated here only from the
# contract byte tables. Nothing below reads engine output.

SCOPE3 = HERE / "scope3"
FILE = 0x1_0000_0001
ROW_PEAK = (9, 425_984, 0, 16, 5, 1)
RELEASE_BYTES = 55_528
BEGIN_BYTES = 56_712
RESERVE_BYTES = 57_336
STATUS_BYTES = 3_040
TERMINAL_FINISH_BYTES = 55_112
AGING_START_BYTES = 19_672
TABLE = (6, 1, 1)  # object, generation, schema epoch
DYNAMIC = 2  # incarnation of every dynamic page below
STATUS_PAGE, H_PAGE, U_PAGE, ANCHOR_PAGE, LEAF_PAGE = 9, 10, 11, 12, 13
KIND = {"leaf": 1, "undo": 8, "status": 9, "anchor": 11, "root": 12, "jobs": 13}
NATIVE = {"leaf": 1, "undo": 3, "status": 5, "root": 8, "jobs": 9}
ROLES = ("root", "jobs", "status", "undo", "leaf")  # physical order 1,2..6,9,11,13


def sha(data):
    return sha256(data).digest()


def u64(value):
    return pack("<Q", value)


def page_ref(page_no, incarnation):
    return pack("<QQQ", FILE, page_no, incarnation)


def txn_id(local):
    return DATABASE + pack("<QI4xQ", 1, 1, local)


ACTOR_ID = txn_id(5)
ACTOR = ACTOR_ID + u64(2)
OLD_ID = txn_id(3)


def slot_ref(page_no, generation):
    """UndoRef40 and StatusSlotRef40 share PageRef24, slot u16, zero6, gen u64."""
    return page_ref(page_no, DYNAMIC) + pack("<H6xQ", 0, generation)


STATUS_REF = slot_ref(STATUS_PAGE, 2)
AGING_REF = DATABASE + pack("<QB7xQQ", 1, 3, 1, 1)


def page(kind, page_no, incarnation, owner, lsn, cells):
    result = bytearray(PAGE_BYTES)
    result[:4] = b"XDP1"
    pack_into("<HBB", result, 4, 1, KIND[kind], 0)
    result[8:24] = DATABASE
    pack_into("<QQQQQQ", result, 24, FILE, page_no, incarnation, owner, 1, lsn)
    upper = DIGEST
    for slot, cell in enumerate(cells):
        upper -= len(cell)
        pack_into("<HH", result, 128 + 4 * slot, upper, len(cell))
        result[upper : upper + len(cell)] = cell
    pack_into("<HHH", result, 72, len(cells), 128 + 4 * len(cells), upper)
    result[DIGEST:] = sha(result[:DIGEST])
    return bytes(result)


def cell_at(page_bytes, slot):
    offset, length = unpack_from("<HH", page_bytes, 128 + 4 * slot)
    return offset, page_bytes[offset : offset + length]


def profile3():
    result = bytearray(4096)
    result[:4] = b"XDC1"
    pack_into("<HBBHHI", result, 4, 3, 1, 0, 128, 0, 4096)
    result[16:32] = DATABASE
    pack_into("<QQIII", result, 32, 1, 1, 0, 0, 4096 - 160)
    prefix = bytearray(188)
    for at, value in (
        (8, 8 << 30),
        (16, 1 << 30),
        (24, 256 << 20),
        (32, 16 << 20),
        (40, 4 << 20),
        (48, 8 << 20),
        (56, 1 << 20),
        (64, 8 << 20),
        (72, 64 << 20),
        (80, 16 << 20),
        (88, 4 << 20),
        (96, 12 << 20),
        (104, 4 << 20),
        (160, 16384 * 129),
        (168, 2 << 20),
    ):
        pack_into("<Q", prefix, at, value)
    for at, value in (
        (0, 1),
        (112, 32),
        (116, 256),
        (120, 1 << 20),
        (124, 32),
        (128, 128),
        (136, 16384),
        (148, 8192),
        (156, 16384),
        (176, 2),
        (180, 4096),
        (184, 1 << 20),
    ):
        pack_into("<I", prefix, at, value)
    for at, value in ((132, 16), (134, 8), (140, 64), (142, 64), (144, 1010), (152, 64)):
        pack_into("<H", prefix, at, value)
    result[128:316] = prefix
    result[4064:] = sha(result[:4064])
    return bytes(result)


PROFILE = profile3()
PROFILE_REF = pack("<BB2xIQI12x", 1, 0, 0, 1, 4096) + sha(PROFILE)


def canonical(number, text):
    result = pack("<IBq", 2, 2, number)
    if text is None:
        return result + b"\0"
    data = text.encode()
    return result + pack("<BI", 3, len(data)) + data


def record(row, canon, undo=None, mutation=1):
    """XRW1; undo None is an older committed, already pruned row."""
    result = bytearray(168 + len(canon))
    result[:4] = b"XRW1"
    pack_into("<HBBQ", result, 4, 1, 0 if undo is None else 2, 1, row)
    if undo is None:
        result[16] = 1
        result[24:32] = u64(1)
        result[72:120] = OLD_ID + u64(mutation)
    else:
        result[16] = 2
        result[24:72] = ACTOR
        result[72:120] = ACTOR_ID + u64(mutation)
        result[120:160] = undo
    pack_into("<I", result, 160, len(canon))
    result[168:] = canon
    return bytes(result)


def projection(row, value):
    prefix = bytearray(48)
    prefix[:8] = b"XRP1\x01\x00\x01\x00"
    prefix[8] = 1
    pack_into("<QQQ", prefix, 16, TABLE[0], TABLE[1], row)
    prefix[40] = 0 if value is None else 1
    return sha(bytes(prefix) + (value or b""))


def leaf_page(rows, lsn):
    cells = [b"XBT1\x01\x00" + bytes(26)]
    for row in sorted(rows):
        cells.append(pack("<HH", 8, len(rows[row])) + row.to_bytes(8, "big") + rows[row])
    return page("leaf", LEAF_PAGE, DYNAMIC, TABLE[0], lsn, cells)


def leaf_slot(page_bytes, row):
    for slot in range(1, unpack_from("<H", page_bytes, 72)[0]):
        if cell_at(page_bytes, slot)[1][4:12] == row.to_bytes(8, "big"):
            return slot
    raise ValueError(row)


def anchor_page():
    cell = bytearray(48)
    cell[:4] = b"XTA1"
    pack_into("<HBB", cell, 4, 1, KIND["leaf"], 0)
    cell[8:32] = page_ref(LEAF_PAGE, DYNAMIC)
    pack_into("<Q", cell, 32, 1)
    return page("anchor", ANCHOR_PAGE, DYNAMIC, TABLE[0], 5, [bytes(cell)])


def undo_cell(generation, state=0, plan=None, incoming=0, cut=False):
    """XUR1 Free, or a Missing Row root (Live 1 / Releasing 2 at cursor0)."""
    result = bytearray(432)
    result[:4] = b"XUR1"
    pack_into("<HBxQ", result, 4, 1, state, generation)
    if state:
        pack_into("<Q", result, 16, plan["seq"])
        result[24:64] = ACTOR_ID
        pack_into("<QQQ", result, 64, 2, plan["mutation"], incoming)
        result[88] = 1
        pack_into("<QQQ", result, 96, TABLE[0], TABLE[1], plan["row"])
        if state == 1 and plan["previous"] is not None:
            result[120:168] = plan["previous"]
            result[326] = 1
        elif cut:
            result[326] = 2
    return bytes(result)


def status_cell(status):
    core = bytearray(192)
    core[:4] = b"XTS1"
    pack_into("<HBB", core, 4, 1, status["state"], status["rollback"])
    core[8:56] = ACTOR
    refs = (status["head"],)
    if status["state"] == 2:
        refs += (status["target"], status["next"])
    for index, reference in enumerate(refs):
        if reference is not None:
            core[64] |= 1 << index
            core[72 + 40 * index : 112 + 40 * index] = reference
    return b"XSH1" + pack("<HHQ", 1, 32, status["incoming"]) + bytes(16) + bytes(core)


def budget(steps, wal):
    result = bytearray(640)
    for row in range(5):
        result[row * 128] = row + 1
    if steps:
        pack_into("<QQ", result, 3 * 128 + 8, steps, wal)
        pack_into("<6Q", result, 3 * 128 + 64, *ROW_PEAK)
        pack_into("<Q", result, 3 * 128 + 112, wal)
    return result


def job_free(generation):
    result = bytearray(1024)
    result[:4] = b"XQJ1"
    pack_into("<HxxQ", result, 4, 3, generation)
    return result


def job_terminal(lease):
    result = job_free(2)
    result[6:8] = b"\x01\x01"
    result[16:64] = ACTOR
    pack_into("<Q", result, 64, lease)
    result[80:120] = STATUS_REF
    pack_into("<Q", result, 376, lease)
    result[384:] = budget(1, TERMINAL_FINISH_BYTES)
    return bytes(result)


def job_root(plan, state, adoption=0, sender=bytes(32), last=0, release=0):
    """Job3 Row root: Reserved 1, Retained 2 or Compensated 4 at cursor0."""
    result = job_free(2)
    result[6:8] = bytes([state, 3])
    result[16:64] = ACTOR
    pack_into("<QQ", result, 64, plan["lease"], adoption)
    result[80:120] = STATUS_REF
    result[128:168] = plan["undo"]
    pack_into("<4Q", result, 168, plan["seq"], *TABLE)
    if state != 4:
        result[200:224] = page_ref(ANCHOR_PAGE, DYNAMIC)
        result[224:248] = page_ref(LEAF_PAGE, DYNAMIC)
        pack_into("<I", result, 280, plan["cell"])
        result[288:320] = plan["projection"]
    pack_into("<4Q", result, 248, plan["statement"], plan["mutation"], 0, release)
    pack_into("<Q", result, 320, plan["row"])
    result[328:360] = sender
    pack_into("<Q", result, 376, last)
    h = 128 if plan["previous"] is not None else 0
    if state == 4:
        result[384:] = budget(4, 4 * RELEASE_BYTES)
    else:
        result[384:] = budget(5, 311_608 + 2 * plan["cell"] + h)
    return bytes(result)


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
                value = unpack_from("<Q", cell, 384 + offset)[0]
                current = unpack_from("<Q", aggregate, offset)[0]
                merged = max(current, value) if 64 <= at < 112 else current + value
                pack_into("<Q", aggregate, offset, merged)
    return masks, sha(b"".join(bank)), bytes(aggregate)


def row_values(steps, wal, undo_slots=0):
    if not steps:
        return [0] * 14
    return [steps, wal, 0, 0, 0, undo_slots, 0, *ROW_PEAK, wal]


def descriptor(owner, chain, inventory, consumed, rows, minimum, mutation, work=None):
    """XOD1. owner is ("tx", status core192) or ("aging", (sequence, cursor))."""
    result = bytearray(1536)
    result[:4] = b"XOD1"
    pack_into("<H", result, 4, 1)
    pack_into("<I", result, 8, 1536)
    if owner[0] == "tx":
        result[6] = owner[1][6]
        result[16] = 1
        result[24:72] = ACTOR
        result[160:352] = owner[1]
    else:
        result[6] = 5
        result[16] = 2
        result[24:72] = AGING_REF
        result[352:354] = b"\x03\x02"
        pack_into("<3Q", result, 360, owner[1][0], owner[1][1], 64)
        result[384:480] = work
    pack_into("<3Q", result, 72, chain[0], chain[1], chain[2])
    result[96:160] = PROFILE_REF
    pack_into("<2Q", result, 544, inventory[0], inventory[1])
    result[560:592] = inventory[2]
    pack_into("<5Q", result, 592, 0, 0, consumed, 0, chain[3])
    values = [row_values(*row) for row in rows]
    for index, row in enumerate(values):
        base = 640 + 128 * index
        result[base] = index + 1
        pack_into("<14Q", result, base + 8, *row)
    for index in range(6):
        pack_into("<Q", result, 1280 + 8 * index, max(row[7 + index] for row in values))
    pack_into("<3Q", result, 1328, sum(row[13] for row in values), minimum, mutation)
    result[1504:] = sha(result[:1504])
    return bytes(result)


def undo_target(link):
    return b"\x01" + bytes(7) + link


TXN_TARGET = b"\x02" + bytes(7) + ACTOR


def edge(index, role, old, new):
    result = bytearray(128)
    pack_into("<HB", result, 0, index, role)
    if old is not None:
        result[8:64] = old
    if new is not None:
        result[64:120] = new
    return bytes(result)


def count(target, before, after, index):
    return target + pack("<QQH22x", before, after, index)


def undo_item(plan):
    item = bytearray(96)
    item[:2] = b"\x02\x02"
    item[8:48] = plan["undo"]
    pack_into("<QQ", item, 64, 1, plan["lease"])
    return bytes(item)


def native(kind, index, before, after):
    header = bytearray(128)
    header[0] = kind
    pack_into("<H", header, 2, index)
    bodies = []
    sides = ((1, 4, 8, 12, 24, before), (2, 6, 10, 16, 56, after))
    for bit, slot_at, offset_at, length_at, digest_at, side in sides:
        if side is None:
            pack_into("<H", header, slot_at, 0xFFFF)
            continue
        offset, cell = cell_at(*side)
        header[1] |= bit
        pack_into("<H", header, slot_at, side[1])
        pack_into("<H", header, offset_at, offset)
        pack_into("<I", header, length_at, len(cell))
        header[digest_at : digest_at + 32] = sha(cell)
        bodies.append(cell)
    pack_into("<I", header, 20, 128 + sum(map(len, bodies)))
    return bytes(header) + b"".join(bodies)


def redo(kind, page_no, incarnation, owner, before, after):
    header = bytearray(64)
    pack_into("<BBBxI", header, 0, 1, KIND[kind], 0, 64 + 16280)
    header[8:32] = page_ref(page_no, incarnation)
    pack_into("<3QHH", header, 32, owner, 1, unpack_from("<Q", before, 64)[0], 72, 16280)
    return bytes(header) + after[72:DIGEST]


def plan(row, number, text, page_no, generation, seq, statement, previous):
    undo = slot_ref(page_no, generation)
    value = record(row, canonical(number, text), undo, statement)
    return dict(
        row=row,
        page=page_no,
        generation=generation,
        undo=undo,
        seq=seq,
        statement=statement,
        mutation=statement,
        previous=previous,
        record=value,
        cell=len(value) + 12,
        projection=projection(row, value),
    )


class History:
    """One actor T on Status page9; prior empty transactions left `prior` Jobs
    Free at generation2, completed and committed, with the queue cursor there."""

    def __init__(self, prior):
        self.bank = [job_free(2 if ordinal < prior else 1) for ordinal in range(64)]
        last = 100 + 3 * prior  # AgingStart100, then Begin/Commit/TerminalFinish
        self.root = dict(
            revision=2 + 3 * prior,
            cursor=prior,
            apply=last,
            issued=prior,
            completed=prior,
            undo_high=0,
            commit=prior,
            consumed=AGING_START_BYTES + prior * TERMINAL_FINISH_BYTES,
            row_high=9,
        )
        self.aging_chain = [last, last - 1, 100, 1 + 3 * prior]
        self.jobs_lsn = [last if prior else 0, 0, 0, 0, 0]
        self.status = None
        self.status_lsn = 0
        self.undo = {H_PAGE: (undo_cell(1), 60), U_PAGE: (undo_cell(2), 60)}
        self.rows = {
            2: record(2, canonical(1, "a"), mutation=1),
            9: record(9, canonical(9, None), mutation=2),
        }
        self.leaf_lsn = 50
        self.tx = None

    def root_cell(self):
        masks, digest, aggregate = summarize(self.bank)
        result = bytearray(512)
        result[:4] = b"XQR1"
        pack_into("<HBxII", result, 4, 3, 1, 512, 64)
        result[16:64] = AGING_REF
        root = self.root
        pack_into(
            "<7Q", result, 64, root["revision"], root["cursor"], root["apply"], root["issued"], *masks
        )
        result[128:160] = digest
        result[160:192] = sha(aggregate)
        result[192:208] = DATABASE
        pack_into("<3Q", result, 208, 1, 4, 1)
        result[232:256] = page_ref(1, 1)
        for index in range(5):
            result[256 + 24 * index : 280 + 24 * index] = page_ref(2 + index, 1)
        pack_into(
            "<8Q",
            result,
            376,
            root["completed"],
            0,
            root["undo_high"],
            TABLE[0],
            root["commit"],
            root["consumed"],
            0,
            root["row_high"],
        )
        return bytes(result)

    def root_page(self):
        return page("root", 1, 1, 4, self.root["apply"], [self.root_cell()])

    def jobs_page(self, index):
        cells = self.bank[15 * index : 15 * index + (4 if index == 4 else 15)]
        return page("jobs", 2 + index, 1, 4, self.jobs_lsn[index], cells)

    def status_page(self):
        return page("status", STATUS_PAGE, DYNAMIC, 2, self.status_lsn, [status_cell(self.status)])

    def undo_page(self, page_no):
        cell, lsn = self.undo[page_no]
        return page("undo", page_no, DYNAMIC, 3, lsn, [cell])

    def aging(self):
        cell = self.root_cell()
        steps, wal = unpack_from("<QQ", summarize(self.bank)[2], 3 * 128 + 8)
        work = bytearray(96)
        work[0] = 7
        pack_into("<QI", work, 8, self.root["revision"], 16384)
        work[32:64] = sha(self.root_page())
        work[64:96] = sha(cell)
        rows = [(0, 0), (0, 0), (0, 0), (steps, wal), (0, 0)]
        owner = ("aging", (self.root["revision"], self.root["cursor"]))
        inventory = (100, 0, sha(b""))
        return descriptor(owner, self.aging_chain, inventory, self.root["consumed"], rows, 100, 0, bytes(work))

    def transaction(self):
        tx = self.tx
        owner = ("tx", status_cell(self.status)[32:])
        return descriptor(owner, tx["chain"], tx["inventory"], tx["consumed"], tx["rows"], tx["chain"][2], tx["mutation"])

    def pages(self, ordinal, plan):
        return dict(
            root=self.root_page(),
            jobs=self.jobs_page(ordinal // 15),
            status=self.status_page(),
            undo=self.undo_page(plan["page"]),
            leaf=leaf_page(self.rows, self.leaf_lsn),
        )

    def advance_root(self, lsn):
        self.root["revision"] += 1
        self.root["apply"] = lsn
        self.aging_chain = [lsn, self.aging_chain[0], 100, self.aging_chain[3] + 1]

    def advance_transaction(self, lsn, cost):
        chain = self.tx["chain"]
        self.tx["chain"] = [lsn, chain[0], chain[2], chain[3] + 1]
        self.tx["consumed"] += cost

    def begin(self, lsn, ordinal):
        self.bank[ordinal] = job_terminal(lsn)
        self.jobs_lsn[ordinal // 15] = lsn
        self.root["issued"] += 1
        self.advance_root(lsn)
        self.status = dict(incoming=0, state=1, rollback=0, head=None, target=None, next=None)
        self.status_lsn = lsn
        rows = [(0, 0), (0, 0), (1, BEGIN_BYTES), (3, 3 * STATUS_BYTES), (0, 0)]
        self.tx = dict(chain=[lsn, 0, lsn, 1], inventory=(lsn, 0, sha(b"")), consumed=BEGIN_BYTES, rows=rows, mutation=0)

    def reserve(self, lsn, ordinal, plan):
        plan["lease"] = lsn
        h = 128 if plan["previous"] is not None else 0
        self.bank[ordinal] = job_root(plan, 1, last=lsn)
        self.jobs_lsn[ordinal // 15] = lsn
        self.root["issued"] += 1
        self.root["undo_high"] = plan["seq"]
        self.root["row_high"] = max(self.root["row_high"], plan["row"])
        self.advance_root(lsn)
        self.undo[plan["page"]] = (undo_cell(plan["generation"]), lsn)
        rows = self.tx["rows"]
        rows[0] = (rows[0][0] + 1, rows[0][1] + 91_096 + plan["cell"] + h)
        rows[1] = (1, 91_304 + plan["cell"] + h, 1)
        self.advance_transaction(lsn, RESERVE_BYTES)
        self.tx["inventory"] = (lsn, 1, sha(undo_item(plan)))
        self.tx["mutation"] = plan["mutation"]

    def rollback_start(self, lsn, mark=None, statement=False):
        head = self.status["head"]
        self.status.update(state=2, rollback=1 if statement else 2, target=mark, next=head)
        self.status_lsn = lsn
        self.advance_transaction(lsn, STATUS_BYTES)
        self.tx["rows"][3] = (2, 2 * STATUS_BYTES) if statement else (0, 0)

    def change(self, forward, lsn, previous_lsn, ordinal, plan):
        """Apply Forward or CLR to this history and return its whole frame."""
        h = plan["previous"] is not None
        cell = plan["cell"]
        before = self.pages(ordinal, plan)
        tx_before, aging_before = self.transaction(), self.aging()
        incoming = self.status["incoming"]
        u_target = undo_target(plan["undo"] + u64(plan["seq"]))
        h_target = undo_target(plan["previous"]) if h else None
        next_ref = plan["previous"][:40] if h else None
        self.status_lsn = self.leaf_lsn = self.jobs_lsn[ordinal // 15] = lsn
        if forward:
            self.status.update(head=plan["undo"], incoming=incoming + 1)
            self.undo[plan["page"]] = (undo_cell(plan["generation"], 1, plan, 2), lsn)
            self.rows[plan["row"]] = plan["record"]
            plan["adoption"], plan["sender"] = lsn, sha(tx_before)
            self.bank[ordinal] = job_root(plan, 2, lsn, plan["sender"], lsn)
            self.advance_transaction(lsn, 91_304 + cell + 128 * h)
            self.tx["inventory"] = (lsn, 0, sha(b""))
            self.tx["rows"][1] = (0, 0)
            target = None
        else:
            target = self.status["target"]
            self.status.update(head=next_ref, next=next_ref, incoming=incoming - 1)
            self.undo[plan["page"]] = (undo_cell(plan["generation"], 2, plan, 0, cut=h), lsn)
            del self.rows[plan["row"]]
            self.bank[ordinal] = job_root(plan, 4, plan["adoption"], plan["sender"], lsn, lsn)
            clr = 91_096 + cell + 128 * h
            self.advance_transaction(lsn, clr)
            steps, wal = self.tx["rows"][0]
            self.tx["rows"][0] = (steps - 1, wal - clr)
        self.advance_root(lsn)
        after = self.pages(ordinal, plan)
        tx_after, aging_after = self.transaction(), self.aging()

        op, items = (2, 1) if forward else (4, 0)
        core = bytearray(256)
        flags = 1 | (2 if h else 0) | (4 if target is not None else 0) | 24
        pack_into("<HHI3Q", core, 0, op, flags, 256, plan["statement"], plan["mutation"], plan["seq"])
        core[32:72] = plan["undo"]
        if h:
            core[72:112] = next_ref
        if target is not None:
            core[112:152] = target
        present = projection(plan["row"], plan["record"])
        missing = projection(plan["row"], None)
        core[176:208], core[208:240] = (missing, present) if forward else (present, missing)

        slots = dict(root=0, jobs=ordinal % 15, status=0, undo=0)
        natives = b""
        for index, role in enumerate(ROLES):
            if role == "leaf" and forward:
                sides = (None, (after[role], leaf_slot(after[role], plan["row"])))
            elif role == "leaf":
                sides = ((before[role], leaf_slot(before[role], plan["row"])), None)
            else:
                sides = ((before[role], slots[role]), (after[role], slots[role]))
            natives += native(NATIVE[role], index, *sides)
        old, new = (None, u_target) if forward else (u_target, None)
        edges = [edge(2, 10, h_target, u_target) if forward else edge(2, 10, u_target, h_target)]
        if h:
            edges.append(edge(3, 4, None, h_target) if forward else edge(3, 4, h_target, None))
        edges += [edge(4, 1, old, new), edge(4, 7, *((None, TXN_TARGET) if forward else (TXN_TARGET, None)))]
        counts = [
            count(u_target, *((0, 2) if forward else (2, 0)), 3),
            count(TXN_TARGET, incoming, incoming + 1 if forward else incoming - 1, 2),
        ]
        deltas = b""
        if forward:
            delta = bytearray(208)
            delta[:2] = b"\x02\x01"
            delta[16:112] = undo_item(plan)
            deltas = bytes(delta)
        identities = dict(
            root=(1, 1, 4),
            jobs=(2 + ordinal // 15, 1, 4),
            status=(STATUS_PAGE, DYNAMIC, 2),
            undo=(plan["page"], DYNAMIC, 3),
            leaf=(LEAF_PAGE, DYNAMIC, TABLE[0]),
        )
        redos = b"".join(redo(role, *identities[role], before[role], after[role]) for role in ROLES)
        auxiliary = b"".join(edges) + b"".join(counts) + deltas
        length = 160 + 256 + 3072 + 256 + len(natives) + len(auxiliary) + len(redos) + 32
        assert length == (91_304 if forward else 91_096) + cell + 128 * h
        assert len(natives) == 5024 + cell and len(redos) == 81_720

        group = bytearray(256)
        group[:4] = b"XHG1"
        pack_into("<4HIQ", group, 4, 2, op, 256, 3, length - 192, lsn)
        pack_into("<2Q", group, 24, unpack_from("<Q", tx_before, 72)[0], unpack_from("<Q", aging_before, 72)[0])
        group[40:96], group[96:152] = tx_after[16:72], aging_after[16:72]
        group[152:184], group[184:216] = sha(tx_before), sha(aging_before)
        shape = (1536, 1536, 256, len(natives), len(auxiliary), 5, 5, len(redos), len(edges), 2, items)
        pack_into("<5I2HI3H", group, 216, *shape)

        header = bytearray(160)
        header[:4] = b"XWF1"
        pack_into("<HBBH2xIIH2x", header, 4, 1, 2 if forward else 4, 3, 160, length, length - 192, 5)
        header[24:40] = DATABASE
        pack_into("<QI4xQQ", header, 40, 1, 1, lsn, previous_lsn)
        header[72:112] = ACTOR_ID
        pack_into("<Q", header, 112, unpack_from("<Q", tx_before, 72)[0])
        header[128:160] = sha(header[:128])
        frame = bytes(header + group) + tx_after + aging_after + bytes(core) + natives + auxiliary + redos
        return frame + sha(frame)

    def snapshot(self, ordinals, undo_pages):
        result = dict(
            root=self.root_page(),
            status=self.status_page(),
            leaf=leaf_page(self.rows, self.leaf_lsn),
            bank=b"".join(self.bank),
            tx=self.transaction(),
            aging=self.aging(),
        )
        for index in sorted({ordinal // 15 for ordinal in ordinals}):
            result[f"jobs{2 + index}"] = self.jobs_page(index)
        for page_no in undo_pages:
            result[f"undo{page_no}"] = self.undo_page(page_no)
        return result


def write_scope3():
    literals = {"profile3.bin": PROFILE, "anchor.bin": anchor_page()}

    def keep(prefix, values, names=None):
        for name, data in values.items():
            if names is None or name in names:
                literals[f"{prefix}-{name}.bin"] = data

    # H absent: terminal ordinal2 and U ordinal3 alias Jobs page2.
    noh = History(2)
    noh.begin(200, 2)
    u = plan(7, 42, "xadb", U_PAGE, 3, 1, 1, None)
    noh.reserve(300, 3, u)
    keep("noh-300", noh.snapshot([2, 3], [U_PAGE]))
    literals["noh-301-forward.wal"] = noh.change(True, 301, 300, 3, u)
    keep("noh-301", noh.snapshot([2, 3], [U_PAGE]))
    noh.rollback_start(302)
    keep("noh-302", noh.snapshot([3], []), {"status", "tx"})
    literals["noh-303-clr.wal"] = noh.change(False, 303, 302, 3, u)
    keep("noh-303", noh.snapshot([3], [U_PAGE]))

    # H present: terminal14 on page2, H15/U16 on page3, statement mark H.
    hist = History(14)
    hist.begin(200, 14)
    hp = plan(4, 7, None, H_PAGE, 2, 1, 1, None)
    hist.reserve(300, 15, hp)
    hist.change(True, 400, 300, 15, hp)
    up = plan(7, 42, "xadb", U_PAGE, 3, 2, 2, hp["undo"] + u64(hp["seq"]))
    hist.reserve(500, 16, up)
    keep("h-500", hist.snapshot([14, 16], [H_PAGE, U_PAGE]))
    literals["h-501-forward.wal"] = hist.change(True, 501, 500, 16, up)
    keep("h-501", hist.snapshot([14, 16], [U_PAGE]))
    hist.rollback_start(502, hp["undo"], statement=True)
    keep("h-502", hist.snapshot([16], []), {"status", "tx"})
    literals["h-503-clr.wal"] = hist.change(False, 503, 502, 16, up)
    keep("h-503", hist.snapshot([16], [U_PAGE]))

    SCOPE3.mkdir(exist_ok=True)
    for name, data in literals.items():
        (SCOPE3 / name).write_bytes(data)


if __name__ == "__main__":
    literals = {
        "row7.bin": row_record(7),
        "empty-before.bin": row_leaf([], 11),
        "row7-after.bin": row_leaf([7], 12),
        "rows3-9-before.bin": row_leaf([3, 9], 11),
        "rows3-7-9-after.bin": row_leaf([3, 7, 9], 12),
        "bad-row-before.bin": row_leaf([7], 11, corrupt_row=7),
        "rows84-before.bin": row_leaf(range(2, 86), 11, bytes.fromhex("0100000000"), 85),
        "rows85-after.bin": row_leaf(range(2, 87), 12, bytes.fromhex("0100000000"), 85),
    }
    for name, data in literals.items():
        (HERE / name).write_bytes(data)
    write_scope3()
