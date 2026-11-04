//! Independent WIRE_ONLY corpus. Body sentinels must never become authorities.
use super::*;
use crate::storage::{
    control,
    wal_frame::{FrameExpectation, FrameKind, WalStreamIdentity},
};
use foundation::codec::content_digest;

const INPUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/wire/frames.bin"
));
const INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/wire/index.tsv"
));
const PROFILES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));

fn profile(id: usize) -> control::Profile {
    control::row_profile::decode_profile(&PROFILES[(id - 1) * 4096..id * 4096], None).unwrap()
}
fn expected(p: control::Profile) -> FrameExpectation {
    FrameExpectation {
        stream: WalStreamIdentity {
            database: p.spec().namespace.database,
            timeline: 1,
            writer_epoch: 1,
        },
        previous_lsn: 100,
    }
}
fn check(b: &[u8]) -> Result<LayoutView<'_>> {
    decode_layout(b, expected(profile(1)), profile(1))
}
fn input(name: &str) -> &'static [u8] {
    let line = INDEX
        .lines()
        .find(|l| l.split('\t').next() == Some(name))
        .unwrap();
    let f: Vec<_> = line.split('\t').collect();
    let at: usize = f[1].parse().unwrap();
    let len: usize = f[2].parse().unwrap();
    &INPUT[at..at + len]
}
fn put(b: &mut [u8], at: usize, v: u64, width: usize) {
    b[at..at + width].copy_from_slice(&v.to_le_bytes()[..width]);
}
fn reseal(b: &mut [u8]) {
    let hash = content_digest(&b[..128]);
    b[128..160].copy_from_slice(&hash);
    let end = b.len() - 32;
    let hash = content_digest(&b[..end]);
    b[end..].copy_from_slice(&hash);
}
fn reject(name: &str, change: impl FnOnce(&mut Vec<u8>)) {
    let original = input(name);
    let mut b = original.to_vec();
    change(&mut b);
    assert_ne!(&b[..], original, "mutation must change {name}");
    reseal(&mut b);
    assert!(check(&b).is_err(), "accepted changed {name}");
}
fn core_at(b: &[u8]) -> usize {
    if b[7] == 3 {
        160 + 256 + 3072
    } else {
        160 + 192 + 1536
    }
}
fn natives(b: &[u8]) -> Vec<usize> {
    let mut at = core_at(b) + 256;
    let count = u16_at(b, 160 + if b[7] == 3 { 236 } else { 124 });
    (0..count)
        .map(|_| {
            let old = at;
            at += u32_at(b, at + 20) as usize;
            old
        })
        .collect()
}
fn redo_at(b: &[u8]) -> usize {
    b.len() - 32 - u32_at(b, 160 + if b[7] == 3 { 240 } else { 152 }) as usize
}
fn digest_hex(b: &[u8]) -> String {
    content_digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn wire_independent_thirty_frames_and_profile_ids() {
    assert_eq!(INDEX.lines().count(), 30);
    for line in INDEX.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let b = input(f[0]);
        assert_eq!(digest_hex(b), f[6]);
        let op = Operation::decode(f[3].parse().unwrap()).unwrap();
        let c = f[4].parse().unwrap();
        let h = f[5] == "1";
        for id in 1..=3 {
            let p = profile(id);
            let v = decode_layout(b, expected(p), p).unwrap();
            assert_eq!(v.bytes().as_ptr(), b.as_ptr());
            assert_eq!(v.operation(), op);
            assert_eq!(v.shape(), op.shape(c, h).unwrap());
            assert_eq!(op.frame_bytes(c, h).unwrap(), b.len());
            assert_eq!(v.header().kind, op.outer());
            assert_eq!(v.core().operation(), op);
            assert_eq!(v.native_records().count(), v.shape().native_count as usize);
            assert_eq!(v.edges().len(), 128 * v.shape().edges as usize);
            assert_eq!(v.counts().len(), 96 * v.shape().counts as usize);
            assert_eq!(v.items().len(), 208 * v.shape().items as usize);
            assert_eq!(v.descriptor(0).unwrap()[0], 209);
            assert!(v.descriptor(if op.handoff() { 2 } else { 1 }).is_none());
        }
    }
}

#[test]
fn wire_rejects_profile_scope_opcode_and_outer_aliases() {
    for legacy in [1, 2] {
        let mut bytes = [0; 4096];
        if legacy == 1 {
            control::encode_profile(profile(1).spec(), &mut bytes).unwrap();
        } else {
            control::encode_catalog_profile(profile(1).spec(), &mut bytes).unwrap();
        }
        let p = control::decode_bootstrap_profile(&bytes, None).unwrap();
        assert!(decode_layout(input("op-2"), expected(p), p).is_err());
    }
    for v in [0, 1, 3, 65535] {
        reject("op-2", |b| put(b, 164, v, 2));
    }
    for v in [0, 1, 2, 4, 65535] {
        reject("op-2", |b| put(b, 170, v, 2));
    }
    for v in [0, 8, 13, 65535] {
        reject("op-2", |b| put(b, 166, v, 2));
    }
    reject("op-2", |b| b[7] = 2);
    reject("op-5", |b| b[7] = 3);
    reject("op-5", |b| b[6] = 3);
    reject("op-2", |b| b[6] = 4);
    reject("op-2", |b| b[160..164].copy_from_slice(b"XSG1"));
    reject("op-5", |b| b[160..164].copy_from_slice(b"XHG1"));
}

#[test]
fn wire_rejects_owner_and_prior_chain_mismatches() {
    let aging = input("op-2")[256..312].to_vec();
    reject("op-5", |b| b[192..248].copy_from_slice(&aging));
    reject("op-2", |b| {
        let tx = b[200..256].to_vec();
        let age = b[256..312].to_vec();
        b[200..256].copy_from_slice(&age);
        b[256..312].copy_from_slice(&tx);
    });
    for at in [201, 208, 224, 232, 240] {
        reject("op-2", |b| b[at] ^= 1);
    }
    reject("op-2", |b| put(b, 248, 1, 8));
    reject("op-2", |b| put(b, 184, 0, 8));
    reject("op-2", |b| put(b, 184, 99, 8));
    reject("op-2", |b| put(b, 192, 101, 8));
    reject("op-2", |b| put(b, 192, 0, 8));
    reject("op-1", |b| b[312] = 1);
    reject("op-1", |b| put(b, 184, 100, 8));
    reject("op-16", |b| b[248] = 1);
    reject("op-16", |b| put(b, 184, 100, 8));
    reject("op-5", |b| put(b, 184, 0, 8));
}

#[test]
fn wire_core_multiple_statement_and_mark_relations() {
    let full = check(input("rollback-full-head")).unwrap().core();
    assert_eq!(full.statement(), 0);
    assert_eq!(full.current(), full.next());
    assert!(full.target().is_none());
    let finish = check(input("finish-mark")).unwrap().core();
    assert_eq!(finish.statement(), 3);
    assert_eq!(finish.current(), finish.next());
    assert_eq!(finish.next(), finish.target());
    let last = check(input("clr-last")).unwrap().core();
    assert_eq!(last.statement(), 3);
    assert_eq!(last.mutation(), 7);
    assert_eq!(last.next(), last.target());
    for (name, field, value, width) in [
        ("op-2", 2, 57, 2),
        ("op-2", 2, 9, 2),
        ("op-2", 4, 255, 4),
        ("op-2", 8, 0, 8),
        ("op-2", 16, 0, 8),
        ("op-2", 24, 0, 8),
        ("op-2", 152, 1, 8),
        ("op-2", 160, 1, 8),
        ("op-2", 240, 1, 1),
        ("op-6", 152, 0, 8),
        ("op-6", 8, 1, 8),
        ("op-6", 32, 1, 1),
        ("op-3", 24, 1, 8),
        ("op-5", 8, 0, 8),
        ("op-5", 16, 1, 8),
        ("finish-mark", 112 + 32, 3, 8),
        ("rollback-statement-mark", 8, 0, 8),
        ("op-20", 160, 64, 8),
        ("op-21", 168, 64, 8),
    ] {
        reject(name, |b| {
            let at = core_at(b) + field;
            put(b, at, value, width);
        });
    }
    reject("row-2-185-1", |b| {
        let at = core_at(b);
        let current = b[at + 32..at + 72].to_vec();
        b[at + 72..at + 112].copy_from_slice(&current);
    });
    reject("clr-target", |b| {
        let at = core_at(b);
        let current = b[at + 32..at + 72].to_vec();
        b[at + 112..at + 152].copy_from_slice(&current);
    });
    reject("clr-target", |b| {
        let at = core_at(b);
        put(b, at + 2, 29, 2);
        b[at + 72..at + 112].fill(0);
    });
    reject("rollback-full-head", |b| {
        let at = core_at(b);
        b[at + 72 + 8] ^= 1;
    });
    reject("finish-mark", |b| {
        let at = core_at(b);
        put(b, at + 2, 3, 2);
        b[at + 112..at + 152].fill(0);
    });
    // Shape and core both encode predecessor presence and must agree.
    reject("row-2-185-1", |b| {
        let at = core_at(b);
        put(b, at + 2, 25, 2);
        b[at + 72..at + 112].fill(0);
    });
}

#[test]
fn wire_rejects_section_lengths_and_frame_corruption() {
    for (name, at, width) in [
        ("op-2", 168, 2),
        ("op-2", 172, 4),
        ("op-2", 176, 8),
        ("op-2", 376, 4),
        ("op-2", 380, 4),
        ("op-2", 384, 4),
        ("op-2", 388, 4),
        ("op-2", 392, 4),
        ("op-2", 396, 2),
        ("op-2", 398, 2),
        ("op-2", 400, 4),
        ("op-2", 404, 2),
        ("op-2", 406, 2),
        ("op-2", 408, 2),
        ("op-2", 410, 1),
        ("op-5", 280, 4),
        ("op-5", 284, 2),
        ("op-5", 286, 2),
        ("op-5", 288, 2),
        ("op-5", 290, 2),
        ("op-5", 292, 2),
        ("op-5", 294, 1),
        ("op-5", 296, 4),
        ("op-5", 300, 4),
        ("op-5", 304, 4),
        ("op-5", 308, 4),
        ("op-5", 312, 4),
        ("op-5", 316, 1),
    ] {
        reject(name, |b| {
            let value = b[at] as u64 + 1;
            put(b, at, value, width);
        });
    }
    let b = input("op-2");
    for n in [0, 159, 351, b.len() - 1] {
        assert!(check(&b[..n]).is_err());
    }
    let mut bad = b.to_vec();
    bad.push(0);
    assert!(check(&bad).is_err());
    let mut bad = b.to_vec();
    bad[1000] ^= 1;
    assert!(check(&bad).is_err());
    let mut bad = b.to_vec();
    bad[128] ^= 1;
    assert!(check(&bad).is_err());
}

#[test]
fn wire_rejects_native_kind_length_presence_digest_and_order() {
    for kind in [0, 2, 4, 6, 7, 10, 11, 255] {
        reject("op-2", |b| {
            let at = natives(b)[4];
            b[at] = kind;
        });
    }
    for (i, field, value, width) in [
        (0, 0, 9, 1),
        (0, 2, 1, 2),
        (1, 2, 0, 2),
        (4, 1, 3, 1),
        (4, 12, 1, 4),
        (4, 16, 184, 4),
        (4, 16, 2229, 4),
        (0, 20, 127, 4),
        (0, 4, 1, 2),
        (0, 8, 71, 2),
        (0, 10, 16351, 2),
        (1, 4, 15, 2),
        (4, 6, 0, 2),
        (4, 4, 0, 2),
        (4, 8, 1, 2),
        (4, 24, 1, 1),
        (0, 88, 1, 1),
    ] {
        reject("op-2", |b| {
            let at = natives(b)[i] + field;
            put(b, at, value, width);
        });
    }
    reject("op-2", |b| {
        let at = natives(b)[0];
        b[at + 128] ^= 1;
    });
    reject("op-20", |b| {
        let at = natives(b)[4];
        put(b, at + 16, 186, 4);
    });
    reject("op-4", |b| {
        let at = natives(b)[4];
        b[at + 1] = 2;
    });
}

#[test]
fn wire_rejects_redo_init_kind_level_span_order_and_aliases() {
    for (field, value, width) in [
        (0, 2, 1),
        (1, 8, 1),
        (2, 1, 1),
        (4, 64, 4),
        (56, 73, 2),
        (58, 16279, 2),
    ] {
        reject("op-2", |b| {
            let at = redo_at(b) + field;
            put(b, at, value, width);
        });
    }
    reject("op-2", |b| {
        let at = redo_at(b);
        let old = b[at + 8..at + 32].to_vec();
        b[at + 16344 + 8..at + 16344 + 32].copy_from_slice(&old);
    });
    reject("op-2", |b| {
        let at = redo_at(b);
        let first = b[at..at + 16344].to_vec();
        let second = b[at + 16344..at + 32688].to_vec();
        b[at..at + 16344].copy_from_slice(&second);
        b[at + 16344..at + 32688].copy_from_slice(&first);
    });
    for name in ["op-3", "op-5"] {
        reject(name, |b| {
            let at = redo_at(b);
            put(b, at + 56, 72, 2);
        });
    }
}

#[test]
fn wire_closed_shape_equations_and_boundaries() {
    for (op, bytes) in [
        (Operation::Reserve, 57336),
        (Operation::Cancel, 40000),
        (Operation::Begin, 56712),
        (Operation::Commit, 56712),
        (Operation::AbortEnd, 56712),
        (Operation::RollbackStart, 3040),
        (Operation::StatementFinish, 3040),
        (Operation::AgingStart, 19672),
        (Operation::ReleaseRole, 55528),
        (Operation::RootFree, 55528),
        (Operation::TerminalFinish, 55112),
    ] {
        assert_eq!(op.frame_bytes(0, false).unwrap(), bytes);
        assert!(op.shape(1, false).is_err());
        assert!(op.shape(0, true).is_err());
    }
    for c in [185, 2228] {
        for h in [false, true] {
            assert_eq!(
                Operation::Forward.frame_bytes(c, h).unwrap(),
                91304 + c + 128 * usize::from(h)
            );
            assert_eq!(
                Operation::Clr.frame_bytes(c, h).unwrap(),
                91096 + c + 128 * usize::from(h)
            );
            assert_eq!(
                Operation::Prune.frame_bytes(c, h).unwrap(),
                89496 + 2 * c + 128 * usize::from(h)
            );
            assert!(Operation::Prune.frame_bytes(c, h).unwrap() <= FRAME_BYTES);
        }
    }
    for op in [Operation::Forward, Operation::Clr, Operation::Prune] {
        for c in [0, 184, 2229, usize::MAX] {
            assert!(op.shape(c, false).is_err());
        }
    }
}

#[test]
fn wire_present_native_slot_and_directory_bounds() {
    for slot in [3244, 60000] {
        reject("op-2", |b| {
            let at = natives(b)[4];
            put(b, at + 6, slot, 2);
        });
    }
    // A kind1 slot1 must be beyond its minimum two-entry directory. The
    // remaining actual page cells and native/redo equality are still opaque.
    let mut b = input("op-2").to_vec();
    let at = natives(&b)[4];
    put(&mut b, at + 10, 136, 2);
    reseal(&mut b);
    assert!(check(&b).is_ok());
    put(&mut b, at + 10, 135, 2);
    reseal(&mut b);
    assert!(check(&b).is_err());
    // Both present sides of Prune must satisfy the same directory bound.
    for side in [8, 10] {
        reject("op-20", |b| {
            let at = natives(b)[4];
            put(b, at + side, 135, 2);
        });
    }
}

#[test]
fn wire_dialect_reports_unsupported_before_semantic_use() {
    for legacy in [1, 2] {
        let mut bytes = [0; 4096];
        if legacy == 1 {
            control::encode_profile(profile(1).spec(), &mut bytes).unwrap();
        } else {
            control::encode_catalog_profile(profile(1).spec(), &mut bytes).unwrap();
        }
        let p = control::decode_bootstrap_profile(&bytes, None).unwrap();
        assert_eq!(
            decode_layout(input("op-2"), expected(p), p).err(),
            Some(Error::Unsupported)
        );
    }
    for (name, at, value, width) in [
        ("op-2", 164, 1, 2),
        ("op-2", 170, 2, 2),
        // 8 is DeleteForward since dev3-row-delete.md and 10 UpdateForward
        // since dev3-row-update.md; 14 is still unknown.
        ("op-2", 166, 14, 2),
        ("op-2", 7, 2, 1),
        ("op-5", 7, 3, 1),
        ("op-2", 6, 4, 1),
        ("op-5", 6, 4, 1),
    ] {
        let mut b = input(name).to_vec();
        put(&mut b, at, value, width);
        reseal(&mut b);
        assert_eq!(check(&b).err(), Some(Error::Unsupported), "{name}@{at}");
    }
    for (name, magic) in [("op-2", b"XSG1"), ("op-5", b"XHG1")] {
        let mut b = input(name).to_vec();
        b[160..164].copy_from_slice(magic);
        reseal(&mut b);
        assert_eq!(check(&b).err(), Some(Error::Unsupported));
    }
    assert!(check(input("op-2")).is_ok());
    assert!(check(input("op-5")).is_ok());
}

// ---- RDL-F: Row DELETE first-path wire literals (dev3-row-delete.md
// #delete-shape). Independent packing after the module-row-format author
// (author.py): integers and SHA only, no product encoder; the descriptor,
// native and auxiliary bodies are opaque sentinels as there. ----

fn pack(parts: &[(u64, usize)]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(value, width) in parts {
        out.extend_from_slice(&value.to_le_bytes()[..width]);
    }
    out
}
fn wire_db() -> Vec<u8> {
    (1..=16).collect()
}
fn wire_ref(n: u64) -> Vec<u8> {
    pack(&[(0x1_0000_0001, 8), (n, 8), (2, 8)])
}
fn wire_undo(n: u64) -> Vec<u8> {
    let mut b = wire_ref(n);
    b.extend(pack(&[(0, 2), (0, 6), (2, 8)]));
    b
}
fn wire_owner(system: bool) -> Vec<u8> {
    let mut b = vec![if system { 2 } else { 1 }];
    b.extend([0; 7]);
    b.extend(wire_db());
    if system {
        b.extend(pack(&[(1, 8), (3, 1), (0, 7), (1, 8), (1, 8)]));
    } else {
        b.extend(pack(&[(1, 8), (1, 4), (0, 4), (1, 8), (2, 8)]));
    }
    b
}
/// DELETE's (8/9/23) and UPDATE's (10/13/24) Forward/Clr/Prune core: the
/// INSERT role's refs, plan, projection and cursors (author.py `core` for ops
/// 2/4/20), its own opcode.
fn delete_core(op: u16, h: bool) -> Vec<u8> {
    let prune = matches!(op, 23 | 24);
    let mut b = vec![0; 256];
    put(&mut b, 0, u64::from(op), 2);
    put(&mut b, 4, 256, 4);
    let mut flags = 1 | 24;
    if !prune {
        put(&mut b, 8, 3, 8);
        put(&mut b, 16, 7, 8);
    }
    b[32..72].copy_from_slice(&wire_undo(14));
    put(&mut b, 24, 8, 8);
    if h {
        flags |= 2;
        b[72..112].copy_from_slice(&wire_undo(13));
    }
    b[176..208].copy_from_slice(&content_digest(b"WIRE_ONLY before"));
    b[208..240].copy_from_slice(&content_digest(b"WIRE_ONLY after"));
    if prune {
        put(&mut b, 160, 62, 8);
        put(&mut b, 168, 63, 8);
    }
    put(&mut b, 2, flags, 2);
    b
}
/// The contract's literal sides per role (dev3-row-delete.md#delete-shape),
/// written out here and never taken from `operation::native_sides`.
fn delete_sides(op: u16, role: &str, c: usize) -> (Option<usize>, Option<usize>) {
    let live = 432 + c - 180;
    match (role, op) {
        ("root", _) => (Some(512), Some(512)),
        ("job", _) => (Some(1024), Some(1024)),
        ("status", _) => (Some(224), Some(224)),
        ("undo", 8) => (Some(432), Some(live)),
        ("undo", _) => (Some(live), Some(432)),
        ("row", 8) => (Some(c), Some(180)),
        ("row", 9) => (Some(180), Some(c)),
        ("row", _) => (Some(180), None),
        _ => unreachable!(),
    }
}
fn delete_bodies(op: u16, c: usize) -> (Vec<u8>, Vec<u8>, usize) {
    row_bodies(|name| delete_sides(op, name, c))
}
/// The five Row natives and redo records with each role's (before, after)
/// sizes from `sides`.
fn row_bodies(sides: impl Fn(&str) -> (Option<usize>, Option<usize>)) -> (Vec<u8>, Vec<u8>, usize) {
    // name, page number, page kind, object, native kind, slot
    let roles = [
        ("root", 1, 12, 4, 8, 0),
        ("job", 2, 13, 4, 9, 1),
        ("status", 9, 9, 2, 5, 0),
        ("undo", 14, 8, 3, 3, 0),
        ("row", 20, 1, 6, 1, 1),
    ];
    let (mut natives, mut redos) = (Vec::new(), Vec::new());
    for (i, &(name, number, page_kind, object, kind, slot)) in roles.iter().enumerate() {
        let (before, after) = sides(name);
        let before = before.map(|n| vec![64 + i as u8; n]);
        let after = after.map(|n| vec![96 + i as u8; n]);
        let mut b = vec![0; 128];
        b[0] = kind;
        b[1] = u8::from(before.is_some()) + 2 * u8::from(after.is_some());
        put(&mut b, 2, i as u64, 2);
        for (value, s, o, length, digest) in [(&before, 4, 8, 12, 24), (&after, 6, 10, 16, 56)] {
            put(&mut b, s, if value.is_some() { slot } else { 65535 }, 2);
            if let Some(value) = value {
                put(&mut b, o, 16352 - value.len() as u64 * (slot + 1), 2);
                put(&mut b, length, value.len() as u64, 4);
                b[digest..digest + 32].copy_from_slice(&content_digest(value));
            }
        }
        b.extend(before.unwrap_or_default());
        b.extend(after.unwrap_or_default());
        let total = b.len() as u64;
        put(&mut b, 20, total, 4);
        natives.extend(b);
        let length = 16280;
        let mut r = vec![0; 64];
        r[0] = 1;
        r[1] = page_kind;
        put(&mut r, 4, 64 + length, 4);
        r[8..32].copy_from_slice(&wire_ref(number));
        if number < 9 {
            put(&mut r, 24, 1, 8);
        }
        put(&mut r, 32, object, 8);
        put(&mut r, 40, 1, 8);
        put(&mut r, 48, 100, 8);
        put(&mut r, 56, 72, 2);
        put(&mut r, 58, length, 2);
        r.extend(vec![128 + i as u8; length as usize]);
        redos.extend(r);
    }
    (natives, redos, roles.len())
}
/// One sealed scope3 DELETE frame (author.py `frame`, ops 8/9/23).
fn delete_frame(op: u16, c: usize, h: bool) -> Vec<u8> {
    row_frame(op, h, delete_bodies(op, c))
}
/// One sealed scope3 Row value frame (DELETE 8/9/23, UPDATE 10/13/24) over
/// its native and redo bodies.
fn row_frame(op: u16, h: bool, bodies: (Vec<u8>, Vec<u8>, usize)) -> Vec<u8> {
    let handoff = !matches!(op, 23 | 24);
    let mut g = vec![0; if handoff { 256 } else { 192 }];
    g[..4].copy_from_slice(if handoff { b"XHG1" } else { b"XSG1" });
    let header = g.len() as u64;
    for (at, value) in [(4, 2), (6, u64::from(op)), (8, header), (10, 3)] {
        put(&mut g, at, value, 2);
    }
    put(&mut g, 16, 101, 8);
    let (native, redo, count) = bodies;
    let (e, k, i) = (3 + u64::from(h), 2, u64::from(matches!(op, 8 | 10)));
    let mut descriptors = vec![209; 1536];
    if handoff {
        put(&mut g, 24, 100, 8);
        put(&mut g, 32, 100, 8);
        g[40..96].copy_from_slice(&wire_owner(false));
        g[96..152].copy_from_slice(&wire_owner(true));
        g[152..184].copy_from_slice(&content_digest(b"WIRE_ONLY prior Tx"));
        g[184..216].copy_from_slice(&content_digest(b"WIRE_ONLY prior Aging"));
        for (at, value) in [
            (216, 1536),
            (220, 1536),
            (224, 256),
            (228, native.len() as u64),
            (232, 128 * e + 96 * k + 208 * i),
            (240, redo.len() as u64),
        ] {
            put(&mut g, at, value, 4);
        }
        for (at, value) in [
            (236, count as u64),
            (238, count as u64),
            (244, e),
            (246, k),
            (248, i),
        ] {
            put(&mut g, at, value, 2);
        }
        descriptors.extend(vec![210; 1536]);
    } else {
        put(&mut g, 24, 100, 8);
        g[32..88].copy_from_slice(&wire_owner(true));
        g[88..120].copy_from_slice(&content_digest(b"WIRE_ONLY prior owner"));
        for (at, value) in [
            (120, 1536),
            (136, native.len() as u64),
            (140, 128 * e),
            (144, 96 * k),
            (148, 208 * i),
            (152, redo.len() as u64),
        ] {
            put(&mut g, at, value, 4);
        }
        for (at, value) in [
            (124, count as u64),
            (126, e),
            (128, k),
            (130, i),
            (132, count as u64),
        ] {
            put(&mut g, at, value, 2);
        }
    }
    g.extend(descriptors);
    g.extend(delete_core(op, h));
    g.extend(native);
    g.extend(vec![225; 128 * e as usize]);
    g.extend(vec![226; 96 * k as usize]);
    g.extend(vec![227; 208 * i as usize]);
    g.extend(redo);
    let length = g.len() as u64;
    put(&mut g, 12, length, 4);
    let mut b = vec![0; 160];
    b[..4].copy_from_slice(b"XWF1");
    put(&mut b, 4, 1, 2);
    b[6] = match op {
        8 | 10 => 2,
        9 | 13 => 4,
        _ => 3,
    };
    b[7] = if handoff { 3 } else { 2 };
    put(&mut b, 8, 160, 2);
    put(&mut b, 12, length + 192, 4);
    put(&mut b, 16, length, 4);
    put(&mut b, 20, count as u64, 2);
    b[24..40].copy_from_slice(&wire_db());
    put(&mut b, 40, 1, 8);
    put(&mut b, 56, 101, 8);
    put(&mut b, 64, 100, 8);
    put(&mut b, 48, 1, 4);
    if b[6] != 3 {
        let mut actor = wire_db();
        actor.extend(pack(&[(1, 8), (1, 4), (0, 4), (1, 8), (2, 8)]));
        b[72..112].copy_from_slice(&actor[..40]);
        put(&mut b, 112, 100, 8);
    }
    let seal = content_digest(&b[..128]);
    b[128..160].copy_from_slice(&seal);
    b.extend(g);
    let seal = content_digest(&b);
    b.extend(seal);
    b
}
const DELETE_OPS: [(Operation, u16); 3] = [
    (Operation::DeleteForward, 8),
    (Operation::DeleteClr, 9),
    (Operation::DeletePrune, 23),
];
const DELETE_CASES: [(usize, bool); 4] = [(185, false), (185, true), (2228, false), (2228, true)];

/// RDL-F01: 8/9/23 decode, keep their INSERT role, and take the INSERT
/// role's outer/handoff (Update/Clr handoff, System single owner). Every
/// still-unknown opcode stays Unsupported.
#[test]
fn rdl_f01_delete_opcodes_decode_role_outer_handoff() {
    for (op, code, role, outer, handoff) in [
        (
            Operation::DeleteForward,
            8,
            Operation::Forward,
            FrameKind::Update,
            true,
        ),
        (
            Operation::DeleteClr,
            9,
            Operation::Clr,
            FrameKind::Clr,
            true,
        ),
        (
            Operation::DeletePrune,
            23,
            Operation::Prune,
            FrameKind::System,
            false,
        ),
    ] {
        assert_eq!(Operation::decode(code), Ok(op));
        assert_eq!(op as u16, code);
        assert_eq!(op.role(), role);
        assert_eq!(op.outer(), outer);
        assert_eq!(op.handoff(), handoff);
        assert!(op.row() && !op.status_patch());
        assert_eq!(op.native_mask(), role.native_mask());
    }
    for op in [
        Operation::Forward,
        Operation::Clr,
        Operation::Prune,
        Operation::Reserve,
    ] {
        assert_eq!(op.role(), op);
    }
    // 10, 13 and 24 are UPDATE's since dev3-row-update.md (RUP-F01).
    for code in [0, 14, 15, 18, 19, 25, 65535] {
        assert_eq!(
            Operation::decode(code).err(),
            Some(Error::Unsupported),
            "{code}"
        );
    }
}

/// RDL-F02: the closed DELETE shapes and frames at C 185/2228 and h 0/1, the
/// largest Row frame 95888 B inside FRAME_BYTES, and C bounds.
#[test]
fn rdl_f02_delete_shape_and_frame_equations() {
    for (c, h) in DELETE_CASES {
        let h128 = 128 * usize::from(h);
        for (op, natives, frame, items) in [
            (
                Operation::DeleteForward,
                5024 + 2 * c,
                91304 + 2 * c + h128,
                1,
            ),
            (Operation::DeleteClr, 5024 + 2 * c, 91096 + 2 * c + h128, 0),
            (Operation::DeletePrune, 5024 + c, 89496 + c + h128, 0),
        ] {
            let shape = op.shape(c, h).unwrap();
            assert_eq!(shape.native_count, 5);
            assert_eq!(shape.native_bytes, natives);
            assert_eq!(shape.edges, 3 + u16::from(h));
            assert_eq!(shape.counts, 2);
            assert_eq!(shape.items, items);
            assert_eq!(shape.redo_count, 5);
            assert_eq!(op.frame_bytes(c, h).unwrap(), frame);
            assert!(frame <= FRAME_BYTES);
        }
    }
    assert_eq!(
        Operation::DeleteForward.frame_bytes(2228, true).unwrap(),
        95888
    );
    assert_eq!(Operation::DeleteClr.frame_bytes(2228, true).unwrap(), 95680);
    assert_eq!(
        Operation::DeletePrune.frame_bytes(2228, true).unwrap(),
        91852
    );
    assert_eq!(FRAME_BYTES - 95888, 2416);
    for (op, _) in DELETE_OPS {
        for c in [0, 180, 184, 2229, usize::MAX] {
            assert!(op.shape(c, false).is_err(), "{op:?} {c}");
        }
    }
}

/// RDL-F03: independently packed DELETE frames decode under every scope3
/// profile, with the contract's per-role native sides (leaf C/180, U 432 and
/// 432+L) and the INSERT role's core rules.
#[test]
fn rdl_f03_delete_frames_decode_with_contract_native_sides() {
    for (op, code) in DELETE_OPS {
        for (c, h) in DELETE_CASES {
            let b = delete_frame(code, c, h);
            assert_eq!(b.len(), op.frame_bytes(c, h).unwrap());
            for id in 1..=3 {
                let p = profile(id);
                let v = decode_layout(&b, expected(p), p).unwrap();
                assert_eq!(v.operation(), op);
                assert_eq!(v.shape(), op.shape(c, h).unwrap());
                assert_eq!(v.header().kind, op.outer());
                assert_eq!(v.core().operation(), op);
                assert_eq!(v.core().next().is_some(), h);
                let names = ["root", "job", "status", "undo", "row"];
                assert_eq!(v.native_records().count(), names.len());
                for (native, name) in v.native_records().zip(names) {
                    let (before, after) = delete_sides(code, name, c);
                    assert_eq!(native.before().map(<[u8]>::len), before, "{op:?} {name}");
                    assert_eq!(native.after().map(<[u8]>::len), after, "{op:?} {name}");
                }
            }
        }
    }
}

/// RDL-F04: odd and out-of-range native totals, a DELETE native with the
/// INSERT sides, wrong leaf presence and an INSERT frame relabelled as a
/// DELETE opcode are all refused.
#[test]
fn rdl_f04_delete_native_totals_and_sides_refused() {
    fn refuse(op: u16, c: usize, h: bool, change: impl FnOnce(&mut Vec<u8>), want: Error) {
        let mut b = delete_frame(op, c, h);
        change(&mut b);
        reseal(&mut b);
        assert_eq!(check(&b).err(), Some(want), "op {op} C {c}");
    }
    // Native total at group 160+228 (handoff) / 160+136 (XSG).
    for (op, at) in [(8, 160 + 228), (9, 160 + 228)] {
        for total in [5024 + 2 * 185 + 1, 5024 + 2 * 184, 5024 + 2 * 2229, 5023] {
            refuse(op, 185, false, |b| put(b, at, total, 4), Error::Corrupt);
        }
    }
    for total in [5024 + 184, 5024 + 2229, 5023] {
        refuse(
            23,
            185,
            false,
            |b| put(b, 160 + 136, total, 4),
            Error::Corrupt,
        );
    }
    // Native records start after the group header, descriptors and core.
    let native_at = |b: &[u8], op: u16| {
        let h = if op == 23 { 192 } else { 256 };
        let descriptors = if op == 23 { 1536 } else { 3072 };
        let mut at = 160 + h + descriptors + 256;
        let mut out = Vec::new();
        for _ in 0..5 {
            out.push(at);
            at += u32::from_le_bytes(b[at + 20..at + 24].try_into().unwrap()) as usize;
        }
        out
    };
    // The INSERT U sizes (432/432) on a DELETE U: after (DeleteForward) or
    // before (DeleteClr/DeletePrune) length is wrong.
    refuse(
        8,
        185,
        false,
        |b| {
            let at = native_at(b, 8)[3];
            put(b, at + 16, 432, 4);
        },
        Error::Corrupt,
    );
    for op in [9, 23] {
        refuse(
            op,
            185,
            false,
            |b| {
                let at = native_at(b, op)[3];
                put(b, at + 12, 432, 4);
            },
            Error::Corrupt,
        );
    }
    // The leaf presence: DeletePrune removes (1), the other two replace (3).
    refuse(
        23,
        185,
        false,
        |b| {
            let at = native_at(b, 23)[4];
            b[at + 1] = 3;
        },
        Error::Corrupt,
    );
    refuse(
        8,
        185,
        false,
        |b| {
            let at = native_at(b, 8)[4];
            b[at + 1] = 2;
        },
        Error::Corrupt,
    );
    // DeleteForward with the INSERT tombstone side: leaf after C, not 180.
    refuse(
        8,
        2228,
        true,
        |b| {
            let at = native_at(b, 8)[4];
            put(b, at + 16, 2228, 4);
        },
        Error::Corrupt,
    );
    // An INSERT Forward/Clr frame relabelled as 8/9: same outer, odd C.
    reject("op-2", |b| put(b, 166, 8, 2));
    reject("op-4", |b| put(b, 166, 9, 2));
    reject("op-20", |b| put(b, 166, 23, 2));
}

/// RDL-F04 (core): each DELETE opcode keeps exactly its INSERT role's core
/// rules (dev3-row-delete.md#delete-states): DeleteForward/DeleteClr are
/// planned (statement and mutation, no cursors), DeletePrune is not (no
/// statement or mutation, cursors below 64), Forward/Prune refs are 1 or 3
/// and Clr's may also be 7. Offsets are the contract core's literal fields.
#[test]
fn rdl_f04_delete_core_role_rules() {
    fn run(op: u16, h: bool, change: impl FnOnce(&mut Vec<u8>, usize)) -> Option<Error> {
        let mut b = delete_frame(op, 185, h);
        let core = if op == 23 {
            160 + 192 + 1536
        } else {
            160 + 256 + 3072
        };
        assert_eq!(u16_at(&b, core), op);
        change(&mut b, core);
        reseal(&mut b);
        check(&b).map(|_| ()).err()
    }
    // A third reference (target, flag bit 2) beside current and next.
    let target = |b: &mut Vec<u8>, core: usize| {
        let flags = u16_at(b, core + 2) | 4;
        put(b, core + 2, u64::from(flags), 2);
        b[core + 112..core + 152].copy_from_slice(&wire_undo(12));
    };
    for op in [8, 9] {
        for (field, value) in [(8, 0), (16, 0), (160, 1), (168, 1)] {
            assert_eq!(
                run(op, false, |b, at| put(b, at + field, value, 8)),
                Some(Error::Corrupt),
                "op {op} core+{field}={value}"
            );
        }
    }
    for (field, value) in [(8, 3), (16, 7), (160, 64), (168, 64)] {
        assert_eq!(
            run(23, false, |b, at| put(b, at + field, value, 8)),
            Some(Error::Corrupt),
            "op 23 core+{field}={value}"
        );
    }
    assert_eq!(run(23, false, |b, at| put(b, at + 160, 63, 8)), None);
    for op in [8, 23] {
        assert_eq!(run(op, true, target), Some(Error::OrderMismatch), "op {op}");
    }
    assert_eq!(run(9, true, target), None, "DeleteClr refs 7");
}

// ---- RUP-F: Row UPDATE first-path wire literals (dev3-row-update.md
// #update-shape). The same independent packing as RDL-F; sizes are the
// contract's integers, never `operation::native_sides` or `shape`. ----

/// The contract's literal sides per role for UPDATE (#update-shape): leaf
/// C→C' (Forward), C'→C (Clr), C'→C' (Prune); U 432↔432+L.
fn update_sides(op: u16, role: &str, c: usize, n: usize) -> (Option<usize>, Option<usize>) {
    let live = 432 + c - 180;
    match (role, op) {
        ("root", _) => (Some(512), Some(512)),
        ("job", _) => (Some(1024), Some(1024)),
        ("status", _) => (Some(224), Some(224)),
        ("undo", 10) => (Some(432), Some(live)),
        ("undo", _) => (Some(live), Some(432)),
        ("row", 10) => (Some(c), Some(n)),
        ("row", 13) => (Some(n), Some(c)),
        ("row", _) => (Some(n), Some(n)),
        _ => unreachable!(),
    }
}
/// One sealed scope3 UPDATE frame of opcode `op`, old cell C and new cell C'.
fn update_frame(op: u16, c: usize, n: usize, h: bool) -> Vec<u8> {
    row_frame(op, h, row_bodies(|name| update_sides(op, name, c, n)))
}
const UPDATE_OPS: [(Operation, u16); 3] = [
    (Operation::UpdateForward, 10),
    (Operation::UpdateClr, 13),
    (Operation::UpdatePrune, 24),
];
/// (C, C'): the four bound pairs, a growing, a shrinking and an equal one.
const UPDATE_CELLS: [(usize, usize); 7] = [
    (185, 185),
    (185, 2228),
    (2228, 185),
    (2228, 2228),
    (199, 230),
    (230, 199),
    (400, 400),
];
/// Each native record's start in a Row frame of opcode `op`.
fn native_starts(b: &[u8], op: u16) -> Vec<usize> {
    let prune = matches!(op, 23 | 24);
    let mut at = 160 + if prune { 192 + 1536 } else { 256 + 3072 } + 256;
    let mut out = Vec::new();
    for _ in 0..5 {
        out.push(at);
        at += u32::from_le_bytes(b[at + 20..at + 24].try_into().unwrap()) as usize;
    }
    out
}

/// RUP-F01: 10/13/24 decode as UPDATE's three operations with the INSERT
/// role's outer, handoff and native mask; only they name a new cell.
#[test]
fn rup_f01_update_opcodes_decode_role_outer_handoff() {
    for (op, code, role, outer, handoff) in [
        (
            Operation::UpdateForward,
            10,
            Operation::Forward,
            FrameKind::Update,
            true,
        ),
        (
            Operation::UpdateClr,
            13,
            Operation::Clr,
            FrameKind::Clr,
            true,
        ),
        (
            Operation::UpdatePrune,
            24,
            Operation::Prune,
            FrameKind::System,
            false,
        ),
    ] {
        assert_eq!(Operation::decode(code), Ok(op));
        assert_eq!(op as u16, code);
        assert_eq!(op.role(), role);
        assert_eq!(op.outer(), outer);
        assert_eq!(op.handoff(), handoff);
        assert!(op.row() && op.update() && !op.status_patch());
        assert_eq!(op.native_mask(), role.native_mask());
    }
    for code in [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 16, 17, 20, 21, 22, 23] {
        assert!(!Operation::decode(code).unwrap().update(), "{code}");
    }
}

/// RUP-F02: the closed UPDATE shapes and frames over (C, C') bounds, growing,
/// shrinking and equal pairs and h 0/1; the largest Row frame 97936 B leaves
/// 368 B of FRAME_BYTES; C' is required exactly for the UPDATE opcodes.
#[test]
fn rup_f02_update_shape_and_frame_equations() {
    for (c, n) in UPDATE_CELLS {
        for h in [false, true] {
            let h128 = 128 * usize::from(h);
            for (op, natives, frame, items) in [
                (
                    Operation::UpdateForward,
                    4844 + 2 * c + n,
                    91124 + 2 * c + n + h128,
                    1,
                ),
                (
                    Operation::UpdateClr,
                    4844 + 2 * c + n,
                    90916 + 2 * c + n + h128,
                    0,
                ),
                (
                    Operation::UpdatePrune,
                    4844 + c + 2 * n,
                    89316 + c + 2 * n + h128,
                    0,
                ),
            ] {
                let shape = op.shape_cells(c, n, h).unwrap();
                assert_eq!(shape.native_count, 5);
                assert_eq!(shape.native_bytes, natives, "{op:?} {c} {n}");
                assert_eq!(shape.edges, 3 + u16::from(h));
                assert_eq!(shape.counts, 2);
                assert_eq!(shape.items, items);
                assert_eq!(shape.redo_count, 5);
                assert_eq!(shape.redo_bytes, 81720);
                assert_eq!((shape.cell, usize::from(shape.new_cell)), (c, n));
                assert_eq!(shape.predecessor, h);
                assert_eq!(op.frame_bytes_cells(c, n, h).unwrap(), frame);
                assert!(frame <= FRAME_BYTES);
            }
        }
    }
    let max = |op: Operation| op.frame_bytes_cells(2228, 2228, true).unwrap();
    assert_eq!(max(Operation::UpdateForward), 97936);
    assert_eq!(max(Operation::UpdateClr), 97728);
    assert_eq!(max(Operation::UpdatePrune), 96128);
    assert_eq!(FRAME_BYTES - 97936, 368);
    for (op, _) in UPDATE_OPS {
        for bad in [0, 180, 184, 2229, usize::MAX] {
            assert!(op.shape_cells(185, bad, false).is_err(), "{op:?} C' {bad}");
            assert!(op.shape_cells(bad, 185, false).is_err(), "{op:?} C {bad}");
        }
        // `shape` names no C', so it cannot shape an UPDATE.
        assert!(op.shape(185, false).is_err());
        assert!(op.frame_bytes(185, false).is_err());
    }
    for op in [
        Operation::Forward,
        Operation::DeleteForward,
        Operation::DeletePrune,
    ] {
        assert!(op.shape_cells(185, 185, false).is_err(), "{op:?} with C'");
        assert_eq!(
            op.shape_cells(185, 0, false).unwrap(),
            op.shape(185, false).unwrap()
        );
    }
    assert!(Operation::Reserve.shape_cells(0, 185, false).is_err());
}

/// RUP-F03: independently packed UPDATE frames decode under every scope3
/// profile with C' from the leaf record and C from the total, and each
/// native carries the contract's sides.
#[test]
fn rup_f03_update_frames_decode_with_contract_native_sides() {
    for (op, code) in UPDATE_OPS {
        for (c, n) in UPDATE_CELLS {
            for h in [false, true] {
                let b = update_frame(code, c, n, h);
                assert_eq!(b.len(), op.frame_bytes_cells(c, n, h).unwrap());
                for id in 1..=3 {
                    let p = profile(id);
                    let v = decode_layout(&b, expected(p), p).unwrap();
                    assert_eq!(v.operation(), op);
                    assert_eq!(v.shape(), op.shape_cells(c, n, h).unwrap());
                    assert_eq!(v.header().kind, op.outer());
                    assert_eq!(v.core().operation(), op);
                    assert_eq!(v.core().next().is_some(), h);
                    let names = ["root", "job", "status", "undo", "row"];
                    assert_eq!(v.native_records().count(), names.len());
                    for (native, name) in v.native_records().zip(names) {
                        let (before, after) = update_sides(code, name, c, n);
                        assert_eq!(native.before().map(<[u8]>::len), before, "{op:?} {name}");
                        assert_eq!(native.after().map(<[u8]>::len), after, "{op:?} {name}");
                    }
                }
            }
        }
    }
}

/// RUP-F04: the kind-1 pre-scan and C' derivation refuse, all as Corrupt
/// (#update-shape): no or two leaf records, C' outside 185..=2228 with an
/// in-range C, an odd Forward/Clr remainder, record lengths 0, 127 and past
/// the section, bytes left after five records, a section past the frame.
/// A native that disagrees with (C, C'), and INSERT/DELETE frames relabelled
/// as UPDATE opcodes, are refused too.
#[test]
fn rup_f04_update_cell_derivation_refused() {
    fn refuse(op: u16, c: usize, n: usize, change: impl FnOnce(&mut Vec<u8>)) {
        let mut b = update_frame(op, c, n, false);
        change(&mut b);
        reseal(&mut b);
        assert_eq!(
            check(&b).err(),
            Some(Error::Corrupt),
            "op {op} C {c} C' {n}"
        );
    }
    let total_at = |op: u16| 160 + if op == 24 { 136 } else { 228 };
    let side = |op: u16| if op == 10 { 16 } else { 12 };
    for (_, op) in UPDATE_OPS {
        // No kind-1 record: the leaf record relabelled as kind 2.
        refuse(op, 185, 230, |b| {
            let at = native_starts(b, op)[4];
            b[at] = 2;
        });
        // Two kind-1 records: the U record relabelled as kind 1.
        refuse(op, 185, 230, |b| {
            let at = native_starts(b, op)[3];
            b[at] = 1;
        });
        // Record lengths 0 and 127 (the U record) and one past the section.
        for length in [0, 127] {
            refuse(op, 185, 230, |b| {
                let at = native_starts(b, op)[3];
                put(b, at + 20, length, 4);
            });
        }
        refuse(op, 185, 230, |b| {
            let at = native_starts(b, op)[4];
            let length = u32::from_le_bytes(b[at + 20..at + 24].try_into().unwrap());
            put(b, at + 20, u64::from(length) + 1, 4);
        });
        // 128 bytes left after the five records, and a section past the frame.
        refuse(op, 185, 230, |b| {
            let at = total_at(op);
            let total = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
            put(b, at, u64::from(total) + 128, 4);
        });
        refuse(op, 185, 230, |b| put(b, total_at(op), 0xffff_fff0, 4));
        // The U's live size disagrees with the derived C.
        refuse(op, 185, 230, |b| {
            let at = native_starts(b, op)[3];
            let field = if op == 10 { 16 } else { 12 };
            put(b, at + field, 432 + 6, 4);
        });
    }
    // C' out of range while the total still derives an in-range C: Forward
    // and Clr with C=C'=185 read C' 183 (C 186) and with C=C'=2228 read
    // C' 2230 (C 2227); Prune reads C' 183 (C 189) and 2230 (C 2224).
    for (_, op) in UPDATE_OPS {
        for (cells, header) in [(185, 183), (2228, 2230)] {
            refuse(op, cells, cells, |b| {
                let at = native_starts(b, op)[4];
                put(b, at + side(op), header, 4);
            });
        }
    }
    // An odd Forward/Clr remainder: C 185, C' 186 read as C' 187.
    for op in [10, 13] {
        refuse(op, 185, 186, |b| {
            let at = native_starts(b, op)[4];
            put(b, at + side(op), 187, 4);
        });
    }
    // The leaf's other side and presence: Forward's before is C, Prune's
    // leaf is replaced (3), not removed (1).
    refuse(10, 2228, 185, |b| {
        let at = native_starts(b, 10)[4];
        put(b, at + 12, 2227, 4);
    });
    refuse(24, 185, 230, |b| {
        let at = native_starts(b, 24)[4];
        b[at + 1] = 1;
    });
    // DELETE frames relabelled 8→10, 9→13, 23→24: their leaf names the
    // 180-byte tombstone, which is no C'.
    for (from, to) in [(8, 10), (9, 13), (23, 24)] {
        for (c, h) in DELETE_CASES {
            let mut b = delete_frame(from, c, h);
            let core = if from == 23 {
                160 + 192 + 1536
            } else {
                160 + 256 + 3072
            };
            put(&mut b, 166, to, 2);
            put(&mut b, core, to, 2);
            reseal(&mut b);
            assert_eq!(check(&b).err(), Some(Error::Corrupt), "{from}→{to} C {c}");
        }
    }
    // INSERT Forward/Clr/Prune frames relabelled as 10/13/24.
    reject("op-2", |b| put(b, 166, 10, 2));
    reject("op-4", |b| put(b, 166, 13, 2));
    reject("op-20", |b| put(b, 166, 24, 2));
}

/// RUP-F08 (core): each UPDATE opcode keeps exactly its INSERT role's core
/// rules (#update-shape): UpdateForward/UpdateClr are planned (statement and
/// mutation, no cursors), UpdatePrune is not (no statement or mutation,
/// cursors below 64), Forward/Prune refs are 1 or 3 and Clr's may also be 7.
#[test]
fn rup_f08_update_core_role_rules() {
    fn run(op: u16, h: bool, change: impl FnOnce(&mut Vec<u8>, usize)) -> Option<Error> {
        let mut b = update_frame(op, 185, 230, h);
        let core = if op == 24 {
            160 + 192 + 1536
        } else {
            160 + 256 + 3072
        };
        assert_eq!(u16_at(&b, core), op);
        change(&mut b, core);
        reseal(&mut b);
        check(&b).map(|_| ()).err()
    }
    let target = |b: &mut Vec<u8>, core: usize| {
        let flags = u16_at(b, core + 2) | 4;
        put(b, core + 2, u64::from(flags), 2);
        b[core + 112..core + 152].copy_from_slice(&wire_undo(12));
    };
    for op in [10, 13] {
        for (field, value) in [(8, 0), (16, 0), (160, 1), (168, 1)] {
            assert_eq!(
                run(op, false, |b, at| put(b, at + field, value, 8)),
                Some(Error::Corrupt),
                "op {op} core+{field}={value}"
            );
        }
    }
    for (field, value) in [(8, 3), (16, 7), (160, 64), (168, 64)] {
        assert_eq!(
            run(24, false, |b, at| put(b, at + field, value, 8)),
            Some(Error::Corrupt),
            "op 24 core+{field}={value}"
        );
    }
    assert_eq!(run(24, false, |b, at| put(b, at + 160, 63, 8)), None);
    for op in [10, 24] {
        assert_eq!(run(op, true, target), Some(Error::OrderMismatch), "op {op}");
    }
    assert_eq!(run(13, true, target), None, "UpdateClr refs 7");
}
