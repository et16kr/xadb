//! Independent WIRE_ONLY corpus. Body sentinels must never become authorities.
use super::*;
use crate::storage::{
    control,
    wal_frame::{FrameExpectation, WalStreamIdentity},
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
        ("op-2", 166, 8, 2),
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
