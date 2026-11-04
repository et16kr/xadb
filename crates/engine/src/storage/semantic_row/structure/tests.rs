//! WIRE_ONLY: raw XSG/XSM/XOD/native input independent of the scope4 observer.
//! Page/redo encoders prepare envelope-valid opaque images, not canonical tree
//! transitions. No fixture is an allocation, split or replay acceptance proof.
use super::*;
use crate::storage::{
    control,
    page::{self, FileId, PAGE_BYTES, PageHeader, PageIdentity, PageLsn},
    wal_frame::WalStreamIdentity,
};

const PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/scope3/profile3.bin"
));
const C: usize = 1888;
const N: usize = 2144;
const R: usize = 2784;
const ORDER: [u8; 5] = [0, 1, 4, 3, 2];

fn profile() -> Profile {
    control::row_profile::decode_profile(PROFILE, None).unwrap()
}
fn expected() -> FrameExpectation {
    FrameExpectation {
        stream: WalStreamIdentity {
            database: profile().spec().namespace.database,
            timeline: profile().spec().namespace.timeline,
            writer_epoch: 1,
        },
        previous_lsn: 100,
    }
}
fn check(b: &[u8]) -> Result<LayoutView<'_>> {
    decode_layout(b, expected(), profile())
}
fn put(b: &mut [u8], at: usize, value: u64, width: usize) {
    b[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}
fn reference(b: &mut [u8], at: usize, page: u64) {
    put(b, at, (1 << 32) | 1, 8);
    put(b, at + 8, page, 8);
    put(b, at + 16, 1, 8);
}
fn page_ref(number: u64) -> PageRef {
    PageRef {
        file: FileId::new((1 << 32) | 1).unwrap(),
        page_no: number,
        incarnation: Generation::new(1).unwrap(),
    }
}
fn hash_tail(b: &mut [u8]) {
    let end = b.len() - 32;
    let digest = content_digest(&b[..end]);
    b[end..].copy_from_slice(&digest);
}
fn seal(b: &mut [u8]) {
    hash_tail(&mut b[352..C]);
    let header = content_digest(&b[..128]);
    b[128..160].copy_from_slice(&header);
    hash_tail(b);
}

fn fixture(opcode: u16, level: u8, parent_is_root: bool) -> Vec<u8> {
    assert!(matches!(opcode, 1 | 2));
    let root = opcode == 1;
    let len = if root { 84744 } else { 84640 };
    let kind = if level == 0 {
        PageKind::RowLeaf
    } else {
        PageKind::RowInternal
    };
    let p = profile();
    let mut b = vec![0; len];
    // Independently authored physical and semantic envelopes.
    b[..4].copy_from_slice(b"XWF1");
    put(&mut b, 4, 1, 2);
    b[6] = 3;
    b[7] = 2;
    put(&mut b, 8, 160, 2);
    put(&mut b, 12, len as u64, 4);
    put(&mut b, 16, (len - 192) as u64, 4);
    put(&mut b, 20, 5, 2);
    b[24..40].copy_from_slice(&p.spec().namespace.database.bytes());
    put(&mut b, 40, p.spec().namespace.timeline, 8);
    put(&mut b, 48, 1, 4);
    put(&mut b, 56, 101, 8);
    put(&mut b, 64, 100, 8);
    {
        let g = &mut b[160..352];
        g[..4].copy_from_slice(b"XSG1");
        put(g, 4, 2, 2);
        put(g, 6, u64::from(opcode), 2);
        put(g, 8, 192, 2);
        put(g, 10, 4, 2);
        put(g, 12, (len - 192) as u64, 4);
        put(g, 16, 101, 8);
        g[32] = 2;
        g[40..56].copy_from_slice(&p.spec().namespace.database.bytes());
        put(g, 56, p.spec().namespace.timeline, 8);
        g[64] = 2;
        put(g, 72, 101, 8);
        put(g, 80, 1, 8);
        put(g, 120, 1536, 4);
        put(g, 124, 5, 2);
        put(g, 132, 5, 2);
        put(g, 136, 640, 4);
        put(g, 152, (len - R - 32) as u64, 4);
    }
    let owner: [u8; 56] = b[192..248].try_into().unwrap();
    {
        let d = &mut b[352..C];
        d[..4].copy_from_slice(b"XOD1");
        put(d, 4, 1, 2);
        d[6] = 6;
        put(d, 8, 1536, 4);
        d[16..72].copy_from_slice(&owner);
        put(d, 72, 101, 8);
        put(d, 88, 101, 8);
        d[96] = 1;
        put(d, 104, p.spec().generation, 8);
        put(d, 112, 4096, 4);
        d[128..160].copy_from_slice(&p.digest());
        d[352] = 2;
        d[353] = 4;
        put(d, 360, 1, 8);
        put(d, 368, 1, 8);
        put(d, 376, 1, 8);
        d[448..480].copy_from_slice(&content_digest(&[]));
        put(d, 544, 101, 8);
        d[560..592].copy_from_slice(&content_digest(&[]));
        put(d, 608, len as u64, 8);
        put(d, 624, 1, 8);
        for i in 0..5 {
            d[640 + i * 128] = i as u8 + 1;
        }
    }
    {
        let c = &mut b[C..N];
        c[..4].copy_from_slice(b"XSM1");
        put(c, 4, 1, 2);
        put(c, 6, u64::from(opcode), 2);
        put(c, 8, 77, 8);
        put(c, 16, 2, 8);
        put(c, 24, 3, 8);
        for (at, number) in [(32, 12), (56, 24), (80, 22), (104, 21)] {
            reference(c, at, number);
        }
        reference(
            c,
            128,
            if root {
                24
            } else if parent_is_root {
                21
            } else {
                60
            },
        );
        put(c, 152, 7, 8);
        c[160] = kind as u8;
        c[161] = level;
        c[163] = level
            + if root {
                0
            } else if parent_is_root {
                1
            } else {
                2
            };
        put(c, 168, 1, 8);
        for (at, cost) in [
            (184, if level == 0 { 80 } else { 64 }),
            (192, if level == 0 { 96 } else { 64 }),
            (200, 64),
            (208, 64),
        ] {
            put(c, at, cost, 8);
        }
    }
    let mut redo_at = R;
    for (index, role) in ORDER.into_iter().enumerate() {
        let (number, kind, page_level) = match role {
            0 => (1, PageKind::Allocation, 0),
            1 => (12, PageKind::TreeAnchor, 0),
            2 => (24, kind, level),
            3 => (22, kind, level),
            4 => (21, PageKind::RowInternal, level + 1),
            _ => unreachable!(),
        };
        let init = role == 3 || root && role == 4;
        let identity = PageIdentity {
            database: p.spec().namespace.database,
            page: page_ref(number),
            owner: ObjectId::new(if role == 0 { 1 } else { 77 }).unwrap(),
            owner_generation: Generation::new(if role == 0 { 1 } else { 2 }).unwrap(),
        };
        let header = PageHeader {
            identity,
            kind,
            level: page_level,
            lsn: PageLsn(101),
        };
        let mut after = vec![0; PAGE_BYTES];
        page::encode_page(header, &[&[role + 1]], &mut after).unwrap();
        let mut before = vec![0; PAGE_BYTES];
        page::encode_page(
            PageHeader {
                lsn: PageLsn(99),
                ..header
            },
            &[&[role + 1]],
            &mut before,
        )
        .unwrap();
        let native = &mut b[N + index * 128..N + (index + 1) * 128];
        native[0] = role;
        native[1] = u8::from(init);
        put(native, 2, index as u64, 2);
        native[4] = kind as u8;
        native[5] = page_level;
        reference(native, 8, number);
        put(native, 32, identity.owner.get(), 8);
        put(native, 40, identity.owner_generation.get(), 8);
        if !init {
            put(native, 48, 99, 8);
            native[56..88].copy_from_slice(&content_digest(&before));
        }
        native[88..120].copy_from_slice(&content_digest(&after));
        let target = RedoTarget {
            page: identity.page,
            owner: identity.owner,
            owner_generation: identity.owner_generation,
            kind,
            level: page_level,
        };
        let record_len = if init {
            wal_frame::encode_init_record(
                identity.database,
                101,
                target,
                &after,
                &mut b[redo_at..len - 32],
            )
            .unwrap()
            .len()
        } else {
            wal_frame::encode_patch_record(
                target,
                99,
                72,
                &after[72..16352],
                &mut b[redo_at..len - 32],
            )
            .unwrap()
            .len()
        };
        redo_at += record_len;
    }
    assert_eq!(redo_at, len - 32);
    seal(&mut b);
    b
}
fn reject(original: &[u8], change: impl FnOnce(&mut [u8])) -> Error {
    let mut b = original.to_vec();
    change(&mut b);
    assert_ne!(b, original);
    seal(&mut b);
    let preserved = b.clone();
    let error = check(&b).err().expect("mutant accepted");
    assert_eq!(b, preserved);
    error
}
fn redo_offsets(b: &[u8]) -> Vec<usize> {
    let mut at = R;
    let mut out = Vec::new();
    for _ in 0..5 {
        out.push(at);
        at += u32_at(b, at + 4) as usize;
    }
    out
}

#[test]
fn structural_wire_independent_root_child_leaf_internal_frames() {
    for (op, level, parent_root) in [(1, 0, true), (1, 2, true), (2, 0, true), (2, 2, false)] {
        let b = fixture(op, level, parent_root);
        let v = check(&b).unwrap();
        assert_eq!(b.len(), if op == 1 { 84744 } else { 84640 });
        assert_eq!(v.operation() as u16, op);
        assert_eq!(v.header().lsn, 101);
        assert_eq!(v.core().bytes(), &b[C..N]);
        assert_eq!(v.core().operation() as u16, op);
        assert_eq!(v.core().object().schema_epoch.get(), 3);
        assert_eq!(v.core().revision(), 7);
        assert_eq!(
            v.core().before_root().page_no,
            if op == 1 {
                24
            } else if parent_root {
                21
            } else {
                60
            }
        );
        assert!(v.native(5).is_none());
        assert!(v.native(usize::MAX).is_none());
        let redo: Vec<_> = v.redo().collect();
        assert_eq!(redo.len(), 5);
        for (i, role) in ORDER.into_iter().enumerate() {
            let n = v.native(i).unwrap();
            assert_eq!(n.role(), role);
            assert_eq!(n.target(), redo[i].target);
            assert_eq!(n.is_init(), role == 3 || op == 1 && role == 4);
            assert_eq!(n.prior_lsn(), if n.is_init() { 0 } else { 99 });
        }
    }
}

#[test]
fn structural_wire_rejects_core_identity_alias_and_scalar_mutations() {
    let b = fixture(1, 0, true);
    for (at, value, width) in [
        (8, 0, 8),
        (16, 0, 8),
        (24, 0, 8),
        (152, 0, 8),
        (152, u64::MAX, 8),
        (161, 15, 1),
        (162, 1, 1),
        (164, 1, 1),
        (168, 0, 8),
        (168, u64::MAX, 8),
        (184, u64::MAX, 8),
        (200, 0, 8),
        (200, 81, 8),
        (216, 1, 1),
        (224, 1, 1),
    ] {
        reject(&b, |b| put(b, C + at, value, width));
    }
    for dst in [32, 80, 104, 128] {
        reject(&b, |b| {
            reference(b, C + dst, 24);
            if dst == 128 {
                put(b, C + dst + 16, 2, 8);
            }
        });
    }
    // Same physical right address with a new incarnation still aliases left.
    reject(&b, |b| {
        reference(b, C + 80, 24);
        put(b, C + 96, 2, 8);
    });
    let child = fixture(2, 0, true);
    reject(&child, |b| reference(b, C + 128, 60));
    reject(&child, |b| put(b, C + 144, 2, 8));
    let deep = fixture(2, 0, false);
    for page in [1, 12, 21, 22, 24] {
        reject(&deep, |b| reference(b, C + 128, page));
    }
    let internal = fixture(1, 1, true);
    assert_eq!(
        reject(&internal, |b| put(b, C + 184, 65, 8)),
        Error::BudgetMismatch
    );
}

#[test]
fn structural_wire_rejects_owner_chain_and_self_consistent_finished_debt() {
    let b = fixture(1, 0, true);
    for (at, value, width) in [
        (112, 1, 8),
        (160 + 24, 99, 8),
        (160 + 88, 1, 1),
        (160 + 64, 1, 1),
        (160 + 72, 100, 8),
        (160 + 80, 2, 8),
        (352 + 56, 100, 8),
        (352 + 64, 2, 8),
        (352 + 360, 2, 8),
        (352 + 368, 0, 8),
        (352 + 544, 100, 8),
        (352 + 608, 1, 8),
        (352 + 616, 1, 8),
        (352 + 1336, 1, 8),
    ] {
        reject(&b, |b| put(b, at, value, width));
    }
    reject(&b, |b| {
        b[72..88].copy_from_slice(&profile().spec().namespace.database.bytes())
    });
    reject(&b, |b| {
        b[352 + 6] = 5;
        b[352 + 353] = 3;
    });
    reject(&b, |b| {
        put(b, 352 + 80, 99, 8);
        put(b, 352 + 88, 99, 8);
        put(b, 352 + 624, 2, 8);
    });
    // All five zero-debt rows are explicit obligations even for Finished.
    for row in 0..5 {
        let mut bad = b.clone();
        let at = 352 + 640 + row * 128;
        put(&mut bad, at + 8, 1, 8);
        put(&mut bad, at + 16, 100, 8);
        seal(&mut bad);
        let owner = SystemWorkRef::decode(&bad[200..248]).unwrap();
        assert!(
            obligation::decode_descriptor(
                &bad[352..C],
                ReservationOwner::System(owner),
                profile_reference(profile())
            )
            .is_ok()
        );
        assert_eq!(check(&bad).err(), Some(Error::BudgetMismatch));
    }
    // A valid foreign profile must not bind to this descriptor.
    let profiles = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
    ));
    let other = control::row_profile::decode_profile(&profiles[4096..8192], None).unwrap();
    assert!(decode_layout(&b, expected(), other).is_err());
}

#[test]
fn structural_wire_rejects_native_redo_bijection_and_init_corruption() {
    for op in [1, 2] {
        let b = fixture(op, 0, true);
        let offsets = redo_offsets(&b);
        for (index, &r) in offsets.iter().enumerate() {
            let n = N + index * 128;
            for (at, value, width) in [
                (n, 9, 1),
                (n + 1, 1 - u64::from(b[n + 1]), 1),
                (n + 2, 9, 2),
                (n + 4, 9, 1),
                (n + 5, 15, 1),
                (n + 6, 1, 1),
                (n + 32, 0, 8),
                (n + 40, 0, 8),
                (n + 120, 1, 1),
                (r + 32, 88, 8),
                (r + 40, 3, 8),
                (r + 48, 101, 8),
                (r + 56, 71, 2),
            ] {
                reject(&b, |b| put(b, at, value, width));
            }
            if b[n + 1] == 1 {
                reject(&b, |b| b[n + 56] = 1);
                reject(&b, |b| b[n + 88] ^= 1);
                reject(&b, |b| {
                    put(b, r + 64 + 64, 100, 8);
                    hash_tail(&mut b[r + 64..r + 64 + 16384]);
                });
                reject(&b, |b| {
                    b[r + 64 + 16351] ^= 1;
                    hash_tail(&mut b[r + 64..r + 64 + 16384]);
                });
            } else {
                reject(&b, |b| put(b, n + 48, 100, 8)); // Native/redo prior mismatch.
                reject(&b, |b| {
                    put(b, n + 48, 101, 8);
                    put(b, r + 48, 101, 8);
                });
            }
        }
        reject(&b, |b| b[N + 128] = 0);
        // Swap complete first two equally-sized PATCH records, retaining their targets.
        reject(&b, |b| {
            let (first, tail) = b[offsets[0]..offsets[2]].split_at_mut(16344);
            first.swap_with_slice(tail);
        });
        // Repeated physical target, changed incarnation, coherent native+redo.
        reject(&b, |b| {
            reference(b, N + 8, 12);
            put(b, N + 24, 2, 8);
            reference(b, R + 8, 12);
            put(b, R + 24, 2, 8);
        });
    }
}

#[test]
fn structural_wire_rejects_truncation_lengths_reserved_and_unsupported() {
    let b = fixture(1, 0, true);
    for len in [
        0,
        1,
        159,
        160,
        351,
        352,
        C - 1,
        C,
        N - 1,
        N,
        R - 1,
        R,
        84640,
        84711,
        84743,
    ] {
        assert!(check(&b[..len]).is_err(), "prefix {len}");
    }
    let mut trailing = b.clone();
    trailing.push(0);
    assert!(check(&trailing).is_err());
    for (at, value, width) in [
        (7, 3, 1),
        (20, 4, 2),
        (160 + 4, 3, 2),
        (160 + 6, 3, 2),
        (160 + 8, 191, 2),
        (160 + 10, 3, 2),
        (160 + 12, u32::MAX as u64, 4),
        (160 + 120, u32::MAX as u64, 4),
        (160 + 124, 4, 2),
        (160 + 126, 1, 2),
        (160 + 128, 1, 2),
        (160 + 130, 1, 2),
        (160 + 132, 4, 2),
        (160 + 134, 1, 1),
        (160 + 136, u32::MAX as u64, 4),
        (160 + 140, 1, 4),
        (160 + 144, 1, 4),
        (160 + 148, 1, 4),
        (160 + 152, u32::MAX as u64, 4),
        (160 + 156, 1, 1),
        (352 + 1352, 1, 1),
        (C + 160, 3, 1),
    ] {
        reject(&b, |b| put(b, at, value, width));
    }
    for r in redo_offsets(&b) {
        for size in [0, 63, 64, u32::MAX as u64] {
            reject(&b, |b| put(b, r + 4, size, 4));
        }
    }
    let core = &b[C..N];
    for len in 0..256 {
        assert!(decode_core(&core[..len], Operation::RootSplit).is_err());
    }
    let mut extra = core.to_vec();
    extra.push(0);
    assert!(decode_core(&extra, Operation::RootSplit).is_err());
}

#[test]
fn structural_wire_accepts_exact_scalar_limits_and_predecessor_commitment() {
    let b = fixture(1, 14, true);
    assert!(check(&b).is_ok());
    let b = fixture(2, 14, true);
    assert!(check(&b).is_ok());
    let b = fixture(1, 0, true);
    let mut next = b.clone();
    put(&mut next, C + 176, 100, 8);
    next[C + 224..C + 256].fill(0x42);
    seal(&mut next);
    assert_eq!(check(&next).unwrap().core().previous_high_water(), 100);
    assert_eq!(
        reject(&next, |b| put(b, C + 176, 101, 8)),
        Error::OrderMismatch
    );
    reject(&next, |b| b[C + 224..C + 256].fill(0));
    let mut edge = b.clone();
    put(&mut edge, C + 152, u64::MAX - 1, 8);
    for at in [184, 192, 200, 208] {
        put(&mut edge, C + at, 8094, 8);
    }
    seal(&mut edge);
    let v = check(&edge).unwrap();
    assert_eq!(v.core().actual_costs(), [8094, 8094]);
    assert_eq!(v.core().weighted_costs(), [8094, 8094]);
    assert_eq!(
        reject(&edge, |b| put(b, C + 184, 8095, 8)),
        Error::BudgetMismatch
    );
    // PATCH commitments cannot be checked without canonical before/after pages.
    let mut commitment = b.clone();
    commitment[N + 56..N + 120].fill(0x55);
    seal(&mut commitment);
    assert!(check(&commitment).is_ok());
}
