#!/usr/bin/env python3
"""Independent Status home flush input authoring; never executes product code.

Wire fields are explicit struct/hashlib tables. The copied INIT field table is
neutral provenance from the completed .a input author, not a product encoder.
Only generation into a new directory is supported; no input overwrite or test
PASS is possible. Product, compiler, native and checker execution are separate.
"""
import argparse
import copy
import csv
import hashlib
import io
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent.parent
UNIT = "D3-STATUS-FLUSH-001"
STATUS = "GENERATED_INPUTS_NOT_EXECUTION"
BASE = Path("crates/engine/tests/fixtures/dev3-storage-session/literal")
PAGE = 16_384
CELL_AT = 10_144
CELL_LEN = 6_208
HOME_AT = 16_128
HOME_LEN = 224
DESCRIPTOR = 1_536
RESERVE_LEN = 21_200
INIT_LEN = 38_000
MEMORY = 114_688
PROTECTED = 70_768
DB = bytes(range(1, 17))
FILE = 0x100000001
INPUTS = {
    "profile1-d3-profile.bin": "c440c3143d335aad95eeaff540f2918dd7dd16bb0e8cfaf43ca8a8fd8da62bf4",
    "profile2-d3-profile.bin": "e45db9f3b6d87837bc2d626f641034c7b647b711fe8c761990731eb5db16052f",
    "profile3-d3-profile.bin": "e86fdb05e8a470f64fa351b9626cd667b45824d40ef0bbe376f9c1bfcc4765d0",
    "profile1-allocation.bin": "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
    "profile2-allocation.bin": "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
    "profile3-allocation.bin": "1e7974d79a66f61c7baccce3064a70e765dfcd3a1992331c9631ffd9fbe892d7",
}
INIT_SECTIONS = (
    ("physical_header", 0, 160), ("group_header", 160, 352),
    ("descriptor", 352, 1888), ("core", 1888, 2144),
    ("allocator_native", 2144, 14688), ("status_native", 14688, 15040),
    ("consume", 15040, 15248), ("allocator_patch", 15248, 21520),
    ("status_init", 21520, 37968), ("frame_sha256", 37968, 38000),
)


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def sha(value):
    return hashlib.sha256(value).digest()


def u(value, width=8):
    return struct.pack({2: "<H", 4: "<I", 8: "<Q"}[width], value)


def at(data, offset, width=8):
    return struct.unpack_from({2: "<H", 4: "<I", 8: "<Q"}[width], data, offset)[0]


def record(size, fields):
    """Explicit nonoverlapping fields; all unmentioned reserved bytes are zero."""
    out, occupied = bytearray(size), bytearray(size)
    for offset, value in fields:
        end = offset + len(value)
        require(0 <= offset <= end <= size, "field outside record")
        require(not any(occupied[offset:end]), "overlapping field")
        out[offset:end] = value
        occupied[offset:end] = bytes([1]) * len(value)
    return out


def sealed_page(data):
    require(len(data) == PAGE, "page size")
    data[-32:] = sha(data[:-32])
    return bytes(data)


def work_owner(work):
    require(1 <= work < 2**64, "work ID")
    return record(48, [(0, DB), (16, u(1)), (24, bytes([1])),
                       (32, u(work)), (40, u(1))])


def token(incarnation, birth):
    return bytes(record(96, [(0, bytes([1, 3])),
                             (8, u(FILE) + u(1) + u(incarnation)),
                             (64, u(1)), (72, u(birth))]))


def allocation_after(before, work, lsn, operation):
    require(len(before) == PAGE and before[-32:] == sha(before[:-32]), "allocator before")
    entry_at = CELL_AT + 64 + 96  # page1; page0 is the allocator itself.
    generation = at(before, entry_at + 8)
    if operation == "reserve":
        require(before[entry_at] == 0 and generation < 2**64 - 1, "Free(g) required")
        generation += 1
        entry = record(96, [(0, bytes([1, 0, 2])), (8, u(generation)),
                            (32, work_owner(work))])
    else:
        require(before[entry_at] == 1, "Reserved(g) required")
        require(before[entry_at + 32:entry_at + 80] == work_owner(work), "reserved claimant")
        if operation == "cancel":
            entry = record(96, [(8, u(generation))])
        elif operation == "init":
            entry = record(96, [(0, bytes([2, 9, 0])), (8, u(generation)),
                                (16, u(2)), (24, u(1))])
        else:
            raise ValueError("operation")
    out = bytearray(before)
    out[entry_at:entry_at + 96] = entry
    out[64:72] = u(lsn)
    return sealed_page(out), generation


def free_status_home():
    core = record(192, [(0, b"XTS1" + u(1, 2)), (48, u(1))])
    return bytes(record(HOME_LEN, [(0, b"XSH1" + u(1, 2) + u(32, 2)), (32, core)]))


def status_page(incarnation, lsn):
    # The closed caller payload is NOT an input: after-only INIT creates all bytes.
    return sealed_page(record(PAGE, [
        (0, b"XDP1" + u(1, 2) + bytes([9, 0])), (8, DB),
        (24, u(FILE) + u(1) + u(incarnation) + u(2) + u(1) + u(lsn)),
        (72, u(1, 2) + u(132, 2) + u(HOME_AT, 2)),
        (128, u(HOME_AT, 2) + u(HOME_LEN, 2)), (HOME_AT, free_status_home()),
    ]))


def descriptor(profile, work, lsn, birth, operation, item):
    finished = operation != "reserve"
    actual = INIT_LEN if operation == "init" else RESERVE_LEN
    reference = record(64, [(0, bytes([1])), (8, u(1)),
                            (16, u(4096, 4)), (32, sha(profile))])
    fields = [
        (0, b"XOD1" + u(1, 2) + bytes([6 if finished else 5])),
        (8, u(DESCRIPTOR, 4)), (16, bytes([2])), (24, work_owner(work)),
        (72, u(lsn) + u(birth if finished else 0) + u(birth)),
        (96, reference), (352, bytes([1, 4 if finished else 1])),
        (360, u(1) + u(1 if finished else 0) + u(1)), (448, sha(b"")),
        (544, u(lsn) + u(0 if finished else 1)),
        (560, sha(b"" if finished else item)),
        (608, u(RESERVE_LEN + actual if finished else RESERVE_LEN)),
        (624, u(2 if finished else 1)),
    ]
    fields.extend((640 + 128 * category, bytes([category + 1])) for category in range(5))
    if not finished:
        fields.extend([
            (904, u(1) + u(INIT_LEN)), (936, u(1)),
            (960, u(2) + u(MEMORY)), (992, u(1)), (1008, u(PROTECTED)),
            (1280, u(2) + u(MEMORY)), (1312, u(1)),
            (1328, u(PROTECTED) + u(birth)),
        ])
    out = record(DESCRIPTOR, fields)
    out[1504:] = sha(out[:1504])
    return bytes(out)


def allocation_native(before, after):
    old, new = before[CELL_AT:-32], after[CELL_AT:-32]
    return record(128, [
        (0, bytes([6, 3])), (8, u(CELL_AT, 2) * 2),
        (12, u(CELL_LEN, 4) * 2 + u(12_544, 4)), (24, sha(old) + sha(new)),
    ]) + old + new


def status_native(after):
    home = after[HOME_AT:-32]
    return record(128, [
        (0, bytes([5, 2]) + u(1, 2) + u(65535, 2) + u(0, 2)),
        (10, u(HOME_AT, 2)), (16, u(HOME_LEN, 4) + u(352, 4)),
        (56, sha(home)),
    ]) + home


def delta(item, operation):
    action = {"reserve": 1, "init": 2, "cancel": 3}[operation]
    return bytes(record(208, [(0, bytes([action, 2 if action == 1 else 1])),
                              (112 if action == 1 else 16, item)]))


def patch(before, after):
    return record(64, [
        (0, bytes([1, 10])), (4, u(6272, 4)), (8, before[24:64]),
        (48, before[64:72]), (56, u(CELL_AT, 2) + u(CELL_LEN, 2)),
    ]) + after[CELL_AT:-32]


def init_redo(after):
    return record(64, [
        (0, bytes([2, 9])), (4, u(16_448, 4)), (8, after[24:64]),
        (58, u(PAGE, 2)),
    ]) + after


def wal(before, after, page, old_descriptor, new_descriptor, work, lsn, birth, item, operation):
    is_init, terminal = operation == "init", operation != "reserve"
    length, opcode = (INIT_LEN if is_init else RESERVE_LEN), (10 if terminal else 8)
    natives = allocation_native(before, after)
    redos = patch(before, after)
    if is_init:
        natives += status_native(page)
        redos += init_redo(page)
    prefix = record(160, [
        (0, b"XWF1" + u(1, 2) + bytes([3, 2])), (8, u(160, 2)),
        (12, u(length, 4) + u(length - 192, 4) + u(2 if is_init else 1, 2)),
        (24, DB + u(1) + u(1, 4)), (56, u(lsn) + u(lsn - 1)),
    ])
    prefix[128:] = sha(prefix[:128])
    group = record(192, [
        (0, b"XSG1" + u(1, 2) + u(opcode, 2) + u(192, 2)),
        (12, u(length - 192, 4)), (16, u(lsn) + u(birth if terminal else 0)),
        (32, bytes([2])), (40, work_owner(work)),
        (88, sha(old_descriptor) if terminal else bytes(32)),
        (120, u(DESCRIPTOR, 4) + u(2 if is_init else 1, 2)),
        (130, u(1, 2) + u(2 if is_init else 1, 2)),
        (136, u(len(natives), 4)), (148, u(208, 4) + u(len(redos), 4)),
    ])
    core = record(256, [(0, u(opcode, 2)), (4, u(256, 4)),
                        (168, u(1 if terminal else 0))])
    material = prefix + group + new_descriptor + core + natives + delta(item, operation) + redos
    require(len(material) == length - 32, "candidate section sum")
    return bytes(material + sha(material))


def candidate_history(profile, genesis, cancellations, final_terminal):
    """Bounded cancellation histories; each new work gets one PageToken."""
    require(cancellations in (0, 1, 2, 8, 31), "frozen .b history bound")
    require(final_terminal in ("init", "cancel"), "final terminal")
    out, rows, before = {}, [], genesis
    for cycle in range(cancellations + 1):
        work, birth = cycle + 1, 2 * cycle + 1
        reserved, incarnation = allocation_after(before, work, birth, "reserve")
        item = token(incarnation, birth)
        reserve_descriptor = descriptor(profile, work, birth, birth, "reserve", item)
        reserve = wal(before, reserved, None, None, reserve_descriptor,
                      work, birth, birth, item, "reserve")
        terminal = "cancel" if cycle < cancellations else final_terminal
        lsn = birth + 1
        after, actual_incarnation = allocation_after(reserved, work, lsn, terminal)
        require(actual_incarnation == incarnation, "terminal preserves incarnation")
        page = status_page(incarnation, lsn) if terminal == "init" else None
        final_descriptor = descriptor(profile, work, lsn, birth, terminal, item)
        ending = wal(reserved, after, page, reserve_descriptor, final_descriptor,
                     work, lsn, birth, item, terminal)
        stem = f"work{work}"
        for suffix, value in {
            "allocator.before": before, "allocator.reserved": reserved,
            "allocator.after": after, "token.bin": item,
            "reserve.descriptor": reserve_descriptor, f"{terminal}.descriptor": final_descriptor,
            "reserve.wal": reserve, f"{terminal}.wal": ending,
            f"{terminal}.delta": delta(item, terminal),
        }.items():
            out[f"{stem}-{suffix}"] = value
        if page is not None:
            out[f"{stem}-status.after"] = page
            out[f"{stem}-status.home"] = page[HOME_AT:-32]
        rows.append({"work": work, "birth_lsn": birth, "terminal_lsn": lsn,
                     "page_incarnation": incarnation, "slot_generation": 1,
                     "terminal": terminal, "terminal_bytes": len(ending),
                     "consumed_wal": RESERVE_LEN + len(ending)})
        before = after
    # This excludes the existing physical epoch header and is named accordingly.
    out["wal.frames-only"] = b"".join(
        out[f"work{row['work']}-{op}.wal"] for row in rows
        for op in ("reserve", row["terminal"])
    )
    return out, rows



# Explicit XDW1/XDS1/XCS1 tables preserved from independent oracle provenance.
SLOT, BANK, SELECTOR = 16544, 16384, 4096
DATABASE = DB

def digest(data):
    return hashlib.sha256(data).hexdigest()

def seal(data):
    return bytes(data) + hashlib.sha256(data).digest()

def uint(data, offset):
    return struct.unpack_from("<Q", data, offset)[0]

def control(generation, batch, page=None):
    """XDW1 table: header128, optional one entry88, trailer32."""
    require(0 < generation < 2**64 and 0 <= batch < 2**64, 'control counters')
    require(page is None or batch > 0, 'pending batch')
    out = bytearray(128 + (88 if page is not None else 0))
    struct.pack_into('<4sHBB16sQQQH', out, 0, b'XDW1', 1,
                     int(page is not None), 0, DATABASE, 1, generation, batch,
                     int(page is not None))
    if page is not None:
        inspect_page(page)
        struct.pack_into('<Q', out, 56, uint(page, 64))
        struct.pack_into('<HBB', out, 128, 0, page[6], page[7])
        out[136:160] = page[24:48]
        out[160:176] = page[48:64]
        out[176:184] = page[64:72]
        out[184:216] = hashlib.sha256(page).digest()
    return seal(out)


def slot(batch, page):
    """XDS1 slot0, sequence=batch, full image checksum plus envelope checksum."""
    inspect_page(page)
    require(0 < batch < 2**64, 'slot sequence')
    out = bytearray(128)
    struct.pack_into('<4sHH16sQQ', out, 0, b'XDS1', 1, 0, DATABASE, 1, batch)
    out[40:64], out[64:80], out[80:88] = page[24:48], page[48:64], page[64:72]
    out[88:90] = page[6:8]
    out[96:128] = hashlib.sha256(page).digest()
    return seal(out + page)


def selector(profile, generation, bank, selected):
    """XCS1 role2, ControlRef kind4; digest covers exact logical XDW1."""
    require(len(profile) == SELECTOR and bank in (0, 1), 'selector input')
    require(profile[16:32] == DATABASE and uint(profile, 32) == 1, 'profile namespace')
    out = bytearray(SELECTOR - 32)
    struct.pack_into('<4sHBB16sQQQ', out, 0, b'XCS1', 1, 2, 0,
                     DATABASE, 1, generation, uint(profile, 40))
    out[48:80] = hashlib.sha256(profile).digest()
    struct.pack_into('<BBHIQI', out, 80, 4, bank, 0, 0, uint(selected, 32), len(selected))
    out[112:144] = hashlib.sha256(selected).digest()
    return seal(out)


def inspect_page(page):
    require(len(page) == PAGE, 'page length')
    require(page[8:24] == DATABASE and page[6] in (9, 10) and page[7] == 0, 'allocator identity')
    require(struct.unpack_from('<5Q', page, 24) == (0x100000001, 0, 1, 1, 1) if page[6] == 10 else (uint(page, 24) == 0x100000001 and uint(page, 32) == 1 and uint(page, 40) >= 2 and uint(page, 48) == 2 and uint(page, 56) == 1), 'target identity')
    require(uint(page, 64) > 0, 'logged image')
    require(page[-32:] == hashlib.sha256(page[:-32]).digest(), 'input page checksum')


def effects():
    result = []
    steps = ('stage_write', 'stage_sync', 'bank_write', 'bank_sync', 'selector_create',
             'selector_write', 'selector_sync', 'selector_publish', 'directory_sync')
    for phase in ('issuance', 'pending', 'retirement'):
        if phase == 'pending':
            result.extend([('images.slot_write', 'write', SLOT), ('images.file_sync', 'sync', 0)])
        if phase == 'retirement':
            result.extend([('home.page_write', 'write', PAGE), ('home.file_sync', 'sync', 0)])
        for step in steps:
            kind = ('write' if step.endswith('_write') else 'create' if step.endswith('_create')
                    else 'publish' if step.endswith('_publish') else 'directory_sync'
                    if step == 'directory_sync' else 'sync')
            length = (SELECTOR if step == 'selector_write' else BANK) if kind == 'write' else 0
            result.append((phase + '.' + step, kind, length))
    return [{'ordinal': i, 'name': name, 'kind': kind, 'bytes': length}
            for i, (name, kind, length) in enumerate(result, 1)]


EFFECTS = effects()
WRITES = tuple(row['ordinal'] for row in EFFECTS if row['kind'] == 'write')
UNKNOWNS = (8, 9, 19, 20, 30, 31)


def phase_at(cut):
    return 0 if cut <= 9 else 1 if cut <= 20 else 2


def material_state(initial, images, phase_files, cut=32, fault='none'):
    """Declarative writes/publish-prefix model from the 31-effect contract.

    The returned files are affected regions only: image slot0 and home page0;
    all remaining file bytes are independently fixed as unchanged. A failed
    lower call is attempted but not counted completed even after mutation.
    """
    state = dict(initial)
    for row in EFFECTS:
        n, kind = row['ordinal'], row['kind']
        if (n > cut and fault not in ('none', 'short', 'interrupted')) or (n == cut and fault == 'before'):
            break
        phase = phase_at(n)
        group = phase_files[phase]
        target = payload = None
        if kind == 'write':
            if n == 10:
                target, payload = 'd3-dwb-images:slot0', images['slot']
            elif n == 21:
                target, payload = 'home:page' + str(uint(images['home'], 32)), images['home']
            elif row['name'].endswith('selector_write'):
                target, payload = 'd3-dwb-select-stage', group['selector']
            elif row['name'].endswith('stage_write'):
                target, payload = 'd3-dwb-stage', group['bank']
            else:
                target, payload = 'd3-dwb-' + str(group['bank_index']), group['bank']
            if n == cut and fault == 'half':
                prefix = len(payload) // 2
                old = state[target]
                state[target] = payload[:prefix] + old[prefix:]
            else:
                state[target] = payload
        elif kind == 'create':
            require('d3-dwb-select-stage' not in state, 'create-new stage exists')
            state['d3-dwb-select-stage'] = b''
        elif kind == 'publish':
            state['DWB'] = state.pop('d3-dwb-select-stage')
        if n == cut and fault in ('before', 'half', 'after'):
            break
    completed = 31 if fault in ('none', 'short', 'interrupted') else cut - 1
    known_phase = sum(completed >= n for n in (9, 20, 31)) - 1
    possible = [known_phase]
    if fault in ('before', 'after') and cut in UNKNOWNS:
        # Protection union begins before publish is attempted, even if actual
        # BeforeError leaves old namespace bytes. It is not a claim of mutation.
        possible.append(phase_at(cut))
    return state, {'attempted': 31 if fault in ('none', 'short', 'interrupted') else cut,
                   'completed': completed, 'known_selection_index': known_phase,
                   'possible_selection_indices': possible,
                   'home_sync_confirmed': completed >= 22,
                   'physical_effect_cut': min(cut, 31),
                   'physical_write_subcalls_at_cut': 2 if fault in ('half', 'short', 'interrupted') else 1,
                   'original_lower_error_required': fault in ('before', 'half', 'after'),
                   'top_effect': 'CallerMustResolve' if fault in ('before', 'half', 'after') else None,
                   'fault_call_mutated': fault in ('half', 'after')}


GENESIS = BASE
INIT = Path('tests/fixtures/dev3-status-init')
INSTALL = Path('tests/fixtures/dev3-doublewrite-install')
DRAFT = Path('artifacts/dev3/resume-20261022/status-home-flush-input-draft-r0.tsv')
DECISIONS = Path('artifacts/dev3/resume-20261022/status-home-flush-owner-decisions-r1.md')
ALIASES = {'PF01': 'SF-P1-SA-PREFIX-10', 'PF02': 'SF-P1-SA-AFTER-19',
           'PF03': 'SF-P1-SA-PREFIX-21', 'PF04': 'SF-P1-SA-AFTER-22',
           'PF05': 'SF-P1-SA-AFTER-31', 'PF08': 'SF-P1-AS-PREFIX-21'}
CONTRACTS = ('devdoc/contracts/dev3-doublewrite-format.md',
             'devdoc/contracts/dev3-control-format.md', 'devdoc/contracts/dev3-page-format.md')


def json_bytes(value):
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()


def tsv_bytes(fields, rows):
    out = io.StringIO(newline='')
    writer = csv.DictWriter(out, fields, delimiter='\t', lineterminator='\n')
    writer.writeheader()
    writer.writerows(rows)
    return out.getvalue().encode()


def valid_witness():
    return dict(events=['claim', 'capture', 'wal_durable', 'issuance_durable', 'image_write',
                        'image_sync', 'pending_durable', 'home_write', 'home_sync',
                        'retirement_durable', 'finalize', 'claim_return', 'slot_reuse'],
                sealed_target=True, published=True, role_exact=True, extent_exact=True,
                exclusive_claim=True, zero_home_authorized=True, home_lsn_allowed=True,
                equal_lsn_bytes_equal=True, retained_target_unchanged=True, continuation_io=0)


def inspect_witness(w):
    """Independent origin/order predicates, not a product state machine."""
    def before(a, b, why):
        require(a in w['events'] and b in w['events'] and
                w['events'].index(a) < w['events'].index(b), why)
    for key, reason in (('sealed_target', 'N01'), ('published', 'N02'), ('role_exact', 'N03'),
                        ('extent_exact', 'N04'), ('exclusive_claim', 'N05'),
                        ('zero_home_authorized', 'N06'), ('home_lsn_allowed', 'N07'),
                        ('equal_lsn_bytes_equal', 'N08')):
        require(w[key] is True, reason)
    before('home_sync', 'retirement_durable', 'N09')
    before('retirement_durable', 'finalize', 'N10')
    require(w['retained_target_unchanged'] is True, 'N11')
    require(w['continuation_io'] == 0, 'N12')
    for a, b in (('claim', 'capture'), ('capture', 'image_write'), ('wal_durable', 'image_write'),
                 ('issuance_durable', 'image_write'), ('image_write', 'image_sync'),
                 ('image_sync', 'pending_durable'), ('pending_durable', 'home_write'),
                 ('home_write', 'home_sync'), ('retirement_durable', 'slot_reuse'),
                 ('finalize', 'claim_return')):
        before(a, b, 'ORDER:' + a + ':' + b)


def negative_witnesses():
    keys = ('sealed_target', 'published', 'role_exact', 'extent_exact', 'exclusive_claim',
            'zero_home_authorized', 'home_lsn_allowed', 'equal_lsn_bytes_equal',
            None, None, 'retained_target_unchanged', 'continuation_io')
    rows = []
    for number, key in enumerate(keys, 1):
        w = valid_witness()
        if key:
            w[key] = 1 if key == 'continuation_io' else False
        else:
            event, predecessor = (('retirement_durable', 'home_sync') if number == 9
                                  else ('finalize', 'retirement_durable'))
            w['events'].remove(event)
            w['events'].insert(w['events'].index(predecessor), event)
        rows.append(dict(id=f'N{number:02}', expected_rejection=f'N{number:02}', witness=w))
    return rows


def mandatory(root):
    rows = list(csv.DictReader(io.StringIO((root / DRAFT).read_text()), delimiter='\t'))
    changes = {
        'BA03': dict(named_variants='normal-headroom,exhausted-headroom,actual-C5-unpublished',
                     action='At actual INIT C5 ACK with DPT/content held, other-thread Status call returns by bounded ACK',
                     expected='NotInitialized/Unchanged before flush_preflight; no claim/attempt/batch/I/O or DPT/content wait'),
        'BA05': dict(named_variants='missing-workspace,poison-before-take',
                     expected='Invariant or Poisoned/fatal; outer and nested effect CallerMustResolve; damage preserved; namespace0; no fallback'),
        'BA07': dict(expected='NoDirty leaves Ready and no retained seal/capture/claim; NoPending continuation; no batch/I/O; registration/LSN/counters retained'),
        'BA09': dict(named_variants='allocator-policy/status-capture,status-policy/allocator-capture,prepared-class,prepared-kind,prepared-level'),
        'BA10': dict(named_variants='reserve,initialize-private-stale-authority,reused-wrappers',
                     action='Public reserve again; private stale INIT authority; actual reused wrappers in new binder',
                     expected='AlreadyInitialized public reserve; private stale authority rejected; registered wrappers fatal; consumed initialize is not a public retry'),
        'BH01': dict(expected='ContentMismatch/fatal; lower same-identity nonzero LSN rejected before issuance; original bytes preserved'),
        'BH03': dict(setup='Envelope-valid equal-LSN Status with Free generation2 and unequal bytes',
                     expected='ContentMismatch/fatal before issuance; synthetic unequal home is not a second canonical Free'),
        'BH07': dict(named_variants='half,nonzero,bad-checksum'),
        'BH08': dict(named_variants='not-live,wrong-extent,short-mapped',
                     setup='TEST_ONLY coherent trusted plan.filemap plus selected FILEMAP bytes; Retained state reaches IdentityMismatch, bootstrap page64 and mapped16384 reach Control::Corrupt before range/home; not a changed-plan operating path'),
        'PF07': dict(setup='TEST_ONLY coherent trusted plan.filemap plus selected FILEMAP bytes omit matching payload file entry; not a changed-plan operating path'),
        'RF02': dict(named_variants='file-below,file-at,file-above,file-aggregate,owner-aggregate,busy-return-retry',
                     setup='Source-derived representable aggregate and individual File cap typed cases; separate healthy workspace contention',
                     expected='Capacity returns Bound for supported .a action; Busy returns Bound for adapter retry after lock release; File<=8192 does not imply aggregate acceptance'),
    }
    for row in rows:
        row.update(changes.get(row['id'], {}))
        row['status'] = 'INPUT_NOT_RUN'
        row['s5_alias'] = ALIASES.get(row['id'], '-')
        row['executor'] = 'python-witness' if row['id'] == 'RF03' else 'rust'
    require(len(rows) == 43, 'mandatory43')
    return rows


def build(root):
    blobs, literals, inputs, states, batches, faults, histories = {}, {}, [], [], [], [], []
    def read(path, expected=None):
        value = (root / path).read_bytes()
        if expected is not None:
            require(digest(value) == expected, 'pinned input ' + str(path))
        inputs.append(dict(path=str(path), bytes=len(value), sha256=digest(value)))
        return value
    def put(name, value):
        require(name not in literals, 'duplicate logical name ' + name)
        key = digest(value)
        blobs.setdefault('blobs/' + key + '.bin', value)
        literals[name] = dict(blob='blobs/' + key + '.bin', bytes=len(value), sha256=key)
        return name
    def state_put(key, state):
        for name, value in sorted(state.items()):
            logical = put(key + '/' + name, value)
            states.append(dict(state=key, region=name, logical=logical))
    def materials(profile, page, batch_id, key=None):
        phases = []
        for offset, name in enumerate(('issuance', 'pending', 'retirement')):
            generation = 3 * batch_id - 1 + offset
            logical = control(generation, batch_id, page if name == 'pending' else None)
            full = logical + bytes(BANK - len(logical))
            bank_index = (generation - 1) % 2
            selected = selector(profile, generation, bank_index, logical)
            phases.append(dict(bank=full, selector=selected, bank_index=bank_index))
            if key:
                put(key + '/' + name + '.control', logical)
                put(key + '/' + name + '.bank', full)
                put(key + '/' + name + '.selector', selected)
        return phases
    def batch(profile, state, page, batch_id, key):
        phases = materials(profile, page, batch_id, key)
        image = slot(batch_id, page)
        put(key + '/capture', page); put(key + '/slot', image)
        state_put(key + '/before', state)
        after, facts = material_state(state, {'slot': image, 'home': page}, phases)
        state_put(key + '/after', after)
        batches.append(dict(id=key, batch=batch_id, target='S' if page[6] == 9 else 'A',
                            lsn=at(page, 64), page_no=at(page, 32), incarnation=at(page, 40),
                            before=key + '/before', after=key + '/after'))
        return after, image, phases, facts

    previous_manifest = json.loads(read(INIT / 'manifest.json'))
    prior = {row['path']: row for row in previous_manifest['literals']}
    installed_manifest = json.loads(read(INSTALL / 'manifest.json'))
    provenance_source = read(Path('scripts/dev3_status_init_literals.py'))
    table_source = read(Path('scripts/dev3_allocator_flush_oracle.py'))
    read(DRAFT)
    read(DECISIONS, 'a7b72dcba7a6f58c0266431be038013d9ffc633925a3dc985774ad9461f51aac')
    for path in CONTRACTS:
        read(Path(path))
    candidate_cache = {}
    for profile_id in (1, 2, 3):
        profile = read(GENESIS / f'profile{profile_id}-d3-profile.bin', INPUTS[f'profile{profile_id}-d3-profile.bin'])
        initial_a = read(GENESIS / f'profile{profile_id}-allocation.bin', INPUTS[f'profile{profile_id}-allocation.bin'])
        stage_name = INSTALL / 'literal' / f'profile{profile_id}-d3-dwb-stage.bin'
        installed = installed_manifest['installed_files'][str(profile_id)]
        stage = read(stage_name, installed['d3-dwb-stage']['sha256'])
        initial_control = control(1, 0)
        fresh = {'d3-dwb-stage': stage, 'd3-dwb-0': initial_control + bytes(BANK - 160),
                 'd3-dwb-1': bytes(BANK), 'DWB': selector(profile, 1, 0, initial_control),
                 'd3-dwb-images:slot0': bytes(SLOT), 'home:page0': initial_a, 'home:page1': bytes(PAGE)}
        require(digest(fresh['DWB']) == installed['DWB']['sha256'], 'initial selector provenance')
        state_put(f'p{profile_id}/fresh', fresh)
        for cancellations in (0, 1, 2, 8, 31):
            values, history = candidate_history(profile, initial_a, cancellations, 'init')
            for name, value in values.items():
                if cancellations != 31:
                    old = f'p{profile_id}-cancel{cancellations}-init/{name}'
                    require(digest(value) == prior[old]['sha256'] and len(value) == prior[old]['bytes'], 'completed .a provenance ' + old)
                put(f'p{profile_id}-c{cancellations}/' + name, value)
            work = cancellations + 1
            pages = {'A': values[f'work{work}-allocator.after'], 'S': values[f'work{work}-status.after']}
            candidate_cache[(profile_id, cancellations)] = (profile, fresh, values, pages)
            for order in ('AS', 'SA'):
                state = fresh
                for batch_id, target in enumerate(order, 1):
                    key = f'p{profile_id}-c{cancellations}-{order}-b{batch_id}'
                    old = state
                    state, image, phases, facts = batch(profile, state, pages[target], batch_id, key)
                    if cancellations == 0 and target == 'S':
                        for label, mode, cuts in (('BEFORE', 'before', range(1, 32)), ('AFTER', 'after', range(1, 32)),
                                                  ('PREFIX', 'half', WRITES), ('SHORT', 'short', WRITES),
                                                  ('INTERRUPTED', 'interrupted', WRITES)):
                            for cut in cuts:
                                fid = f'SF-P{profile_id}-{order}-{label}-{cut:02}'
                                partial, diag = material_state(old, {'slot': image, 'home': pages[target]}, phases, cut, mode)
                                # Successful Status-first short/interrupted campaigns also finish A.
                                if label in ('SHORT', 'INTERRUPTED') and order == 'SA':
                                    partial, _ = material_state(partial, {'slot': slot(2, pages['A']), 'home': pages['A']}, materials(profile, pages['A'], 2))
                                state_put(fid, partial)
                                alias = next((case for case, sid in ALIASES.items() if sid == fid), '-')
                                success = label in ('SHORT', 'INTERRUPTED')
                                known = diag['known_selection_index']
                                known_generation = (3 * (batch_id - 1) + 1 if known == -1
                                                    else 3 * batch_id - 1 + known)
                                possible_generations = [3 * (batch_id - 1) + 1 if i == -1
                                                        else 3 * batch_id - 1 + i
                                                        for i in diag['possible_selection_indices']]
                                campaign = dict(state='Ready' if success else 'Failed',
                                    completed_batches=2 if success else batch_id-1,
                                    final_batch=2 if success else batch_id,
                                    final_target=order[-1] if success else 'S',
                                    known_generation=7 if success else known_generation,
                                    possible_generations=[7] if success else possible_generations,
                                    last_issued=2 if success else batch_id-1+int(diag['completed'] >= 9),
                                    attempted=31 if success else diag['attempted'],
                                    completed=31 if success else diag['completed'],
                                    homes_synced=success or diag['home_sync_confirmed'],
                                    retirement_selected=success)
                                faults.append(dict(id=fid, profile=profile_id, order=order, cancellations=0,
                                                   mode=label, cut=cut, batch_input=key, state=fid,
                                                   mandatory_alias=alias, selection='S5_ALIAS' if alias != '-' else 'S5_EXTRA',
                                                   status='INPUT_NOT_RUN', injected_batch=batch_id,
                                                   injected_batch_facts=diag, final_campaign_facts=campaign))
                if cancellations:
                    histories.append(dict(id=f'SH-P{profile_id}-C{cancellations}-{order}', profile=profile_id,
                                          cancellations=cancellations, order=order,
                                          final_state=f'p{profile_id}-c{cancellations}-{order}-b2/after', status='INPUT_NOT_RUN'))
    # BR03/BR04 include the actual reserved allocator image, never a DWB reset.
    for profile_id, cancellations, label, final_order in ((1, 0, 'BR03', 'AS'), (1, 0, 'BR04', 'SA'), (3, 1, 'native-P3', 'SA')):
        profile, fresh, values, pages = candidate_cache[(profile_id, cancellations)]
        work = cancellations + 1
        state, _, _, _ = batch(profile, fresh, values[f'work{work}-allocator.reserved'], 1, label + '-b1')
        for number, target in enumerate(final_order, 2):
            state, _, _, _ = batch(profile, state, pages[target], number, label + f'-b{number}')
    current = candidate_cache[(1, 0)][3]['S']
    mutations = {'BH01-lower-lsn': (64, u(1)), 'BH03-free-generation2': (HOME_AT + 32 + 48, u(2)),
                 'BH04-higher-lsn': (64, u(3)), 'BH05-lower-incarnation': (40, u(1)),
                 'BH05-higher-incarnation': (40, u(3)), 'BH06-object': (48, u(3)),
                 'BH06-owner-generation': (56, u(2)), 'BH06-kind': (6, bytes([10])),
                 'BH06-level': (7, bytes([1]))}
    for name, (offset, value) in mutations.items():
        changed = bytearray(current); changed[offset:offset + len(value)] = value
        put('homes/' + name, sealed_page(changed))
    put('homes/BH02-exact', current)
    put('homes/BH07-half', current[:PAGE//2] + bytes(PAGE//2))
    put('homes/BH07-nonzero', bytes([0x5a]) * PAGE)
    changed = bytearray(current); changed[-1] ^= 1
    put('homes/BH07-bad-checksum', bytes(changed))

    # Mechanical trusted-plan inputs. Exact refs/checksums are coherent, but
    # wrong-extent/short-mapped deliberately violate canonical FILEMAP meaning;
    # their Control::Corrupt is earlier than payload range/home inspection.
    original_root = read(GENESIS / 'profile1-root.bin')
    original_chunk = read(GENESIS / 'profile1-chunk.bin')
    original_selector = read(GENESIS / 'profile1-FILEMAP.bin')
    filemaps = []
    for variant, cause in (('not-live', 'IdentityMismatch'), ('wrong-extent', 'Control::Corrupt'),
                           ('short-mapped', 'Control::Corrupt'), ('missing', 'MissingMapping')):
        root_bytes, chunk = bytearray(original_root), bytearray(original_chunk)
        if variant == 'not-live':
            chunk[128 + 88] = 2
        elif variant == 'wrong-extent':
            chunk[128 + 40:128 + 48] = u(64)
        elif variant == 'short-mapped':
            chunk[128 + 8:128 + 16] = u(PAGE)
            root_bytes[136:144] = u(PAGE)
        else:
            # Empty FILEMAP root keeps the envelope/profile/namespace identity.
            root_bytes = root_bytes[:192] + bytes(32)
            root_bytes[12:16], root_bytes[52:56], root_bytes[56:60] = u(224, 4), u(0, 4), u(64, 4)
            root_bytes[128:160] = bytes(32)
            chunk = bytearray()
        if chunk:
            chunk[-32:] = sha(chunk[:-32])
            root_bytes[224:256] = sha(chunk)
        root_bytes[-32:] = sha(root_bytes[:-32])
        select = bytearray(original_selector)
        select[96:100] = u(len(root_bytes), 4)
        select[112:144] = sha(root_bytes)
        select[-32:] = sha(select[:-32])
        container = bytearray(2113536)
        container[:len(root_bytes)] = root_bytes
        container[16384:16384 + len(chunk)] = chunk
        stem = 'filemap/' + variant
        for name, value in (('root', root_bytes), ('chunk', chunk), ('selector', select),
                            ('reference', select[80:144]), ('container', container)):
            put(stem + '/' + name, bytes(value))
        filemaps.append(dict(variant=variant, expected_cause=cause,
                             origin='TEST_ONLY trusted-plan plus selected FILEMAP bytes',
                             prefix=stem))

    require(len(faults) == 570 and len(histories) == 24, 'expanded tuple census')
    files = dict(blobs)
    rows = mandatory(root)
    files['mandatory.tsv'] = tsv_bytes(list(rows[0]), rows)
    files['literals.tsv'] = tsv_bytes(['logical', 'blob', 'bytes', 'sha256'],
                                     [dict(logical=k, **v) for k, v in sorted(literals.items())])
    files['states.tsv'] = tsv_bytes(['state', 'region', 'logical'], states)
    files['batches.tsv'] = tsv_bytes(list(batches[0]), batches)
    fields = ('id', 'profile', 'order', 'cancellations', 'mode', 'cut', 'batch_input', 'state', 'mandatory_alias', 'selection', 'status')
    files['faults.tsv'] = tsv_bytes(fields, [{k: r[k] for k in fields} for r in faults])
    files['fault-facts.json'] = json_bytes(faults)
    files['histories.tsv'] = tsv_bytes(list(histories[0]), histories)
    files['effects.tsv'] = tsv_bytes(list(EFFECTS[0]), EFFECTS)
    files['known-bad.json'] = json_bytes(dict(valid=valid_witness(), negatives=negative_witnesses()))
    files['filemap-inputs.tsv'] = tsv_bytes(list(filemaps[0]), filemaps)
    files['aliases.json'] = json_bytes(dict(status='ADOPTED_SETUP_ACTION_ORACLE_ALIAS', aliases=ALIASES,
        setup='profile1/cancel0/fresh selected DWB, actual INIT, target order AS or SA',
        action='same Status batch, cut and failure mode; same first complete allocator batch for AS',
        oracle='same full affected regions, protection union, lower error, retained claim/fence and no RAM clean'))
    manifest = dict(schema='xadb-status-home-flush-literals-v1', unit=UNIT, status=STATUS,
        product_tests='NOT_RUN', compiler_tests='NOT_RUN', checker_tests='NOT_RUN', native_tests='NOT_RUN',
        generator=dict(path='scripts/dev3_status_home_flush_literals.py', sha256=digest(Path(__file__).read_bytes())),
        provenance_inputs=inputs, init_field_table_source_sha256=digest(provenance_source), dwb_field_table_source_sha256=digest(table_source),
        mandatory_cases=43, rust_mandatory=42, python_witness_cases=1, fault_tuples=570,
        mandatory_fault_aliases=6, s5_fault_extra=564, s5_history_extra=24,
        pre_alias_union=637, unique_scenarios=631, known_bad=12, counts=[0, 1, 2, 8, 31],
        write_effects=list(WRITES), logical_effects=31,
        budget_seconds=dict(mandatory=120, s5=1200, compiler=30, native_per_profile=30),
        native_bytes=dict(per_database=24*1024*1024, three_databases=72*1024*1024),
        file_assembly={'home': dict(bytes=1048576, regions=['home:page0', 'home:page1'], zero_suffix=1048576-2*PAGE),
                       'd3-dwb-images': dict(bytes=16711680, region='d3-dwb-images:slot0', zero_suffix=16711680-SLOT),
                       'd3-dwb-reserved': dict(bytes=8192, all_zero=True),
                       'other_installed_files': 'exact original bytes/identity; WAL appends frozen frames-only literal'},
        outputs=[dict(path=p, bytes=len(v), sha256=digest(v)) for p, v in sorted(files.items())])
    return files, manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root, output = args.root.resolve(), args.output.resolve()
    require(root in output.parents and not output.exists(), 'new repository output only; preserve old inputs')
    files, manifest = build(root)
    output.mkdir(parents=True, exist_ok=False)
    for name, data in files.items():
        path = output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as stream:
            stream.write(data)
    with (output / 'manifest.json').open('xb') as stream:
        stream.write(json_bytes(manifest))
    print(json.dumps(dict(status=STATUS, output=str(output), outputs=len(files), mandatory=43,
                          s5_fault_tuples=570, aliases=6, s5_extra=588,
                          manifest_sha256=digest((output / 'manifest.json').read_bytes()), product_pass=False)))


if __name__ == '__main__':
    main()
