//! Raw birth records are written independently of the production grammar.
//! These layout fixtures do not claim full current-tree/history closure.
use super::*;
use crate::storage::{allocation_page::*, anchor, page, tree_page};
use base::{Certificate, PageIdentity, PageLsn, PageRef, get, image_records};
use foundation::identity::{Generation, ObjectId};
use image_records::StructuralOperation;

fn reference(number: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: base::first_file(),
        page_no: number,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}
fn header(number: u64, incarnation: u64, level: u8, lsn: u64) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: expected().profile.spec().namespace.database,
            page: reference(number, incarnation),
            owner: ObjectId::new(6).unwrap(),
            owner_generation: one(),
        },
        kind: if level == 0 {
            PageKind::RowLeaf
        } else {
            PageKind::RowInternal
        },
        level,
        lsn: PageLsn(lsn),
    }
}
fn certificate(header: PageHeader) -> Certificate {
    Certificate {
        header,
        full_digest: [0x42; 32],
    }
}
fn raw_certificate(h: PageHeader, digest: [u8; 32]) -> [u8; 96] {
    let mut raw = [0; 96];
    raw[0] = 1;
    raw[1] = h.kind as u8;
    raw[2] = h.level;
    h.identity.page.encode(&mut raw[8..32]).unwrap();
    put(&mut raw, 32, h.identity.owner.get());
    put(&mut raw, 40, 1);
    put(&mut raw, 48, h.lsn.0);
    raw[56..88].copy_from_slice(&digest);
    raw
}
fn claim(
    h: PageHeader,
    role: u8,
    operation: u64,
    birth: u64,
    peer: Option<PageRef>,
) -> [u8; INIT_BYTES] {
    let mut raw = [0; INIT_BYTES];
    raw[0] = 1;
    raw[1] = role;
    raw[2] = h.kind as u8;
    raw[3] = h.level;
    raw[4] = 1;
    h.identity.page.encode(&mut raw[8..32]).unwrap();
    put(&mut raw, 32, 6);
    put(&mut raw, 40, 1);
    put(&mut raw, 48, birth);
    put32(&mut raw, 56, 1);
    raw[64..96].fill(0x51);
    raw[96..128].fill(0x62);
    // Independent SystemWorkRef bytes: namespace, Structural kind, birth id,
    // writer epoch as generation. No product claim/descriptor encoder is used.
    raw[128..144].copy_from_slice(&expected().profile.spec().namespace.database.bytes());
    put(&mut raw, 144, expected().profile.spec().namespace.timeline);
    raw[152] = 2;
    put(&mut raw, 160, birth);
    put(&mut raw, 168, 1);
    put(&mut raw, 176, birth);
    put(&mut raw, 184, 6);
    put(&mut raw, 192, 1);
    put(&mut raw, 200, 1);
    reference(11, 2).encode(&mut raw[208..232]).unwrap();
    reference(12, 2).encode(&mut raw[232..256]).unwrap();
    if let Some(peer) = peer {
        peer.encode(&mut raw[256..280]).unwrap();
    }
    put(&mut raw, 280, operation);
    put(&mut raw, 288, 1);
    put(&mut raw, 296, 2);
    raw[304..336].fill(0x73);
    raw
}
fn profile2() -> Expected {
    let mut bytes = [0; 4096];
    control::encode_catalog_profile(expected().profile.spec(), &mut bytes).unwrap();
    Expected {
        profile: control::decode_bootstrap_profile(&bytes, None).unwrap(),
        ..expected()
    }
}
fn view(raw: &[u8; INIT_BYTES], h: PageHeader) -> base::InitProvenance<'_> {
    image_records::validate_provenance(raw, certificate(h), expected()).unwrap();
    base::InitProvenance { bytes: raw }
}
fn image(h: PageHeader) -> Box<[u8; PAGE_BYTES]> {
    let mut bytes = Box::new([0; PAGE_BYTES]);
    let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
    let cell = tree_page::encode_metadata(
        h.kind,
        0,
        (h.level > 0).then_some(reference(12, 2)),
        &mut metadata,
    )
    .unwrap();
    if h.level == 0 {
        tree_page::encode_tree_page(h, 0, &[cell], bytes.as_mut()).unwrap();
    } else {
        // Independent canonical separator: RowId100, right child17. The
        // complete reachable tree is deliberately outside this layout fixture.
        let mut separator = [0; 36];
        separator[..2].copy_from_slice(&8u16.to_le_bytes());
        separator[4..12].copy_from_slice(&100u64.to_be_bytes());
        reference(17, 2).encode(&mut separator[12..]).unwrap();
        tree_page::encode_tree_page(h, 0, &[cell, &separator], bytes.as_mut()).unwrap();
    }
    bytes
}
fn entry(bytes: &mut [u8; BASE_BYTES], ordinal: usize, new: AllocationEntry) {
    let allocator = allocation_page::decode_allocation_page(
        bytes[ARENA..ARENA + PAGE_BYTES].try_into().unwrap(),
        base::identity(expected(), 0, one(), 1),
        expected().profile.spec().namespace,
    )
    .unwrap();
    let h = allocator.header();
    let mut entries: [AllocationEntry; 64] = std::array::from_fn(|i| allocator.entry(i).unwrap());
    entries[ordinal] = new;
    let mut cell = [0; ALLOCATION_CELL_BYTES];
    allocation_page::encode_allocation_cell(
        h,
        expected().profile.spec().namespace,
        &entries,
        &mut cell,
    )
    .unwrap();
    allocation_page::encode_allocation_page(
        h,
        expected().profile.spec().namespace,
        &cell,
        &mut bytes[ARENA..ARENA + PAGE_BYTES],
    )
    .unwrap();
    let digest = content_digest(&bytes[ARENA..ARENA + PAGE_BYTES]);
    bytes[CERTIFICATES + 56..CERTIFICATES + 88].copy_from_slice(&digest);
}
fn add(bytes: &mut [u8; BASE_BYTES], h: PageHeader, raw: [u8; INIT_BYTES]) {
    let ordinal = h.identity.page.page_no as usize;
    let page = image(h);
    bytes[ARENA + ordinal * PAGE_BYTES..ARENA + (ordinal + 1) * PAGE_BYTES]
        .copy_from_slice(page.as_ref());
    bytes[CERTIFICATES + ordinal * 96..CERTIFICATES + (ordinal + 1) * 96]
        .copy_from_slice(&raw_certificate(h, content_digest(page.as_ref())));
    bytes[PROVENANCE + ordinal * INIT_BYTES..PROVENANCE + (ordinal + 1) * INIT_BYTES]
        .copy_from_slice(&raw);
    entry(
        bytes,
        ordinal,
        AllocationEntry::Allocated {
            generation: h.identity.page.incarnation,
            object: h.identity.owner,
            object_generation: one(),
            kind: h.kind,
        },
    );
    reseal(bytes);
}
fn root_fixture() -> Box<[u8; BASE_BYTES]> {
    let mut bytes = backing(ACTIVE);
    for (number, role, level, peer) in [(13, 5, 0, 14), (14, 6, 1, 13)] {
        let h = header(number, 2, level, 70);
        add(
            &mut bytes,
            h,
            claim(h, role, 1, 70, Some(reference(peer, 2))),
        );
    }
    bytes
}
fn raw_mut(bytes: &mut [u8; BASE_BYTES], ordinal: usize) -> &mut [u8; INIT_BYTES] {
    (&mut bytes[PROVENANCE + ordinal * INIT_BYTES..PROVENANCE + (ordinal + 1) * INIT_BYTES])
        .try_into()
        .unwrap()
}

#[test]
fn structural_base_certificate_levels_and_profile_isolation() {
    for level in 0..=15 {
        let h = header(13, 2, level, 70);
        let page = image(h);
        let expected_raw = raw_certificate(h, content_digest(page.as_ref()));
        let mut raw = [0xa5; 96];
        encode_certificate(expected(), h, page.as_ref(), &mut raw).unwrap();
        assert_eq!(raw, expected_raw);
        assert_eq!(
            image_records::decode_certificate(&raw, expected(), 13)
                .unwrap()
                .unwrap()
                .header,
            h
        );
        assert_eq!(
            image_records::decode_certificate(&raw, profile2(), 13).is_ok(),
            level == 0
        );
    }
    for (kind, level, number) in [
        (PageKind::RowInternal, 0, 13),
        (PageKind::RowInternal, 16, 13),
        (PageKind::RowLeaf, 1, 13),
        (PageKind::RowInternal, 1, 8),
    ] {
        let h = PageHeader {
            kind,
            ..header(number, if number < 9 { 1 } else { 2 }, level, 70)
        };
        assert!(
            image_records::decode_certificate(
                &raw_certificate(h, [0; 32]),
                expected(),
                number as usize
            )
            .is_err()
        );
    }
}

#[test]
fn structural_base_birth_roles_and_borrowed_observations() {
    for role in [5, 6] {
        for level in if role == 5 { 0..=14 } else { 1..=15 } {
            for operation in if role == 5 { 1..=2 } else { 1..=1 } {
                let h = header(13, 2, level, 90);
                let peer = (operation == 1).then_some(reference(14, 2));
                let raw = claim(h, role, operation, 70, peer);
                let p = view(&raw, h);
                assert!(std::ptr::eq(p.bytes(), &raw));
                assert_eq!(p.object().unwrap().object.get(), 6);
                assert_eq!(p.pair(), None);
                assert_eq!(p.init_lsn(), p.reserve_lsn());
                let birth = p.structural().unwrap();
                assert_eq!(birth.level(), level);
                assert_eq!(birth.anchor(), reference(11, 2));
                assert_eq!(birth.left(), reference(12, 2));
                assert_eq!(birth.peer(), peer);
                assert_eq!(birth.revisions(), [1, 2]);
                assert_eq!(birth.core_digest(), &[0x73; 32]);
                assert_eq!(
                    birth.operation(),
                    if operation == 1 {
                        StructuralOperation::RootSplit
                    } else {
                        StructuralOperation::ChildSplit
                    }
                );
                assert_eq!(
                    image_records::validate_provenance(&raw, certificate(h), profile2()),
                    Err(Error::Unsupported)
                );
            }
        }
    }
}

#[test]
fn structural_base_rejects_scalar_role_and_physical_alias_mutations() {
    for (role, level) in [(5, 0), (6, 1)] {
        let h = header(13, 2, level, 90);
        let original = claim(h, role, 1, 70, Some(reference(14, 2)));
        for offset in [
            0, 2, 3, 4, 5, 8, 32, 40, 48, 56, 60, 128, 144, 152, 153, 160, 168, 176, 184, 192, 200,
            280, 288, 296, 336,
        ] {
            let mut raw = original;
            raw[offset] ^= 0x40;
            assert!(
                image_records::validate_provenance(&raw, certificate(h), expected()).is_err(),
                "role={role} offset={offset}"
            );
        }
        for (at, value) in [
            (48, 0),
            (48, 91),
            (176, 69),
            (288, 0),
            (288, u64::MAX),
            (296, 0),
        ] {
            let mut raw = original;
            put(&mut raw, at, value);
            assert!(image_records::validate_provenance(&raw, certificate(h), expected()).is_err());
        }
        for at in [208, 232, 256] {
            for alias in [11, 12, 13, 14] {
                let own = match at {
                    208 => 11,
                    232 => 12,
                    _ => 14,
                };
                if alias == own {
                    continue;
                }
                let mut raw = original;
                reference(alias, 9).encode(&mut raw[at..at + 24]).unwrap();
                assert!(
                    image_records::validate_provenance(&raw, certificate(h), expected()).is_err(),
                    "at={at} alias={alias}"
                );
            }
        }
    }
    for (role, level, operation) in [(5, 15, 1), (6, 0, 1), (6, 1, 2)] {
        let h = header(13, 2, level, 90);
        let raw = claim(
            h,
            role,
            operation,
            70,
            (operation == 1).then_some(reference(14, 2)),
        );
        assert!(image_records::validate_provenance(&raw, certificate(h), expected()).is_err());
    }
}

#[test]
fn structural_base_preserves_legacy_birth_rules() {
    for ordinal in 9..13 {
        let raw: &[u8; INIT_BYTES] = ACTIVE
            [PROVENANCE + ordinal * INIT_BYTES..PROVENANCE + (ordinal + 1) * INIT_BYTES]
            .try_into()
            .unwrap();
        let cert = image_records::decode_certificate(
            ACTIVE[CERTIFICATES + ordinal * 96..CERTIFICATES + (ordinal + 1) * 96]
                .try_into()
                .unwrap(),
            expected(),
            ordinal,
        )
        .unwrap()
        .unwrap();
        for e in [expected(), profile2()] {
            image_records::validate_provenance(raw, cert, e).unwrap();
            for mutation in 0..3 {
                let mut bad = *raw;
                match mutation {
                    0 => put(&mut bad, 176, get(raw, 48)),
                    1 => bad[3] = 1,
                    _ => bad[256] = 1,
                }
                assert!(image_records::validate_provenance(&bad, cert, e).is_err());
            }
        }
        assert!(view(raw, cert.header).structural().is_none());
    }
}

#[test]
fn structural_base_layout_accepts_historical_nonroot_and_retired_peers() {
    let mut bytes = root_fixture();
    // ACTIVE is a raw-layout literal, not a fully decoded Anchor fixture.
    // Give this observation an actual canonical current root/revision.
    let anchor_header = decode_layout(bytes.as_ref(), expected())
        .unwrap()
        .certificate(11)
        .unwrap()
        .unwrap()
        .header;
    anchor::encode_anchor(
        anchor_header,
        anchor::AnchorRoot {
            tree: anchor::TreeKind::Row,
            page: reference(12, 2),
            level: 0,
            revision: 1,
        },
        &mut bytes[ARENA + 11 * PAGE_BYTES..ARENA + 12 * PAGE_BYTES],
    )
    .unwrap();
    image_changed(&mut bytes, 11);
    let first = decode_layout(bytes.as_ref(), expected()).unwrap();
    assert_eq!(first.provenance(14).unwrap().unwrap().role(), 6);
    // Birth role6 does not assert today's root/revision. The existing Anchor
    // still names the CREATE leaf; raw layout makes no current-tree claim.
    let anchor_page = first.image(11).unwrap().unwrap();
    let anchor_header = first.certificate(11).unwrap().unwrap().header;
    let root = anchor::decode_anchor(anchor_page, anchor_header.identity)
        .unwrap()
        .root();
    assert_ne!(root.page, reference(14, 2));
    assert_ne!(root.revision, 2);
    // Retire the peer. Historical left/peer presence is not a layout rule.
    for (at, width) in [
        (CERTIFICATES, 96),
        (PROVENANCE, INIT_BYTES),
        (ARENA, PAGE_BYTES),
    ] {
        bytes[at + 14 * width..at + 15 * width].fill(0);
    }
    entry(
        &mut bytes,
        14,
        AllocationEntry::Free(Generation::new(2).unwrap()),
    );
    reseal(&mut bytes);
    decode_layout(bytes.as_ref(), expected()).unwrap();
    // Same physical address, next incarnation and another birth is allowed.
    let h = header(14, 3, 0, 80);
    add(&mut bytes, h, claim(h, 5, 2, 80, None));
    decode_layout(bytes.as_ref(), expected()).unwrap();
    // Exact old incarnation with a different birth is not a retired peer.
    let h = header(14, 2, 0, 80);
    add(&mut bytes, h, claim(h, 5, 2, 80, None));
    assert!(decode_layout(bytes.as_ref(), expected()).is_err());
}

#[test]
fn structural_base_rejects_cross_class_birth_collision() {
    let mut bytes = root_fixture();
    // A locally valid historical Allocation INIT at the same complete LSN
    // cannot evade the Structural grouping through its different owner class.
    let mut cert = decode_layout(bytes.as_ref(), expected())
        .unwrap()
        .certificate(9)
        .unwrap()
        .unwrap();
    cert.header.lsn = PageLsn(70);
    put(raw_mut(&mut bytes, 9), 48, 70);
    image_records::validate_provenance(raw_mut(&mut bytes, 9), cert, expected()).unwrap();
    assert!(
        validate_structural_groups(
            LayoutView {
                bytes: &bytes,
                expected: expected()
            },
            1
        )
        .is_err()
    );
}

#[test]
fn structural_base_groups_bind_same_lsn_even_with_changed_owner() {
    for mutation in 0..9 {
        let mut bytes = root_fixture();
        let raw = raw_mut(&mut bytes, 14);
        match mutation {
            0 => {
                put32(raw, 56, 2);
                put(raw, 168, 2);
            }
            1 => raw[64] ^= 1,
            2 => raw[304] ^= 1,
            3 => {
                put(raw, 288, 2);
                put(raw, 296, 3);
            }
            4 => reference(15, 2).encode(&mut raw[208..232]).unwrap(),
            5 => reference(15, 2).encode(&mut raw[232..256]).unwrap(),
            6 => reference(15, 2).encode(&mut raw[256..280]).unwrap(),
            7 => {
                raw[3] = 2;
            }
            _ => {
                put(raw, 32, 7);
                put(raw, 184, 7);
            }
        }
        // Every mutation is locally well-formed. The group check must reject
        // it, even when changed epoch means a different full SystemWorkRef.
        let mut h = header(14, 2, if mutation == 7 { 2 } else { 1 }, 70);
        if mutation == 8 {
            h.identity.owner = ObjectId::new(7).unwrap();
        }
        image_records::validate_provenance(raw_mut(&mut bytes, 14), certificate(h), expected())
            .unwrap();
        assert!(
            validate_structural_groups(
                LayoutView {
                    bytes: &bytes,
                    expected: expected()
                },
                2
            )
            .is_err(),
            "mutation={mutation}"
        );
    }
    let mut bytes = root_fixture();
    for ordinal in [13, 14] {
        let raw = raw_mut(&mut bytes, ordinal);
        put32(raw, 56, 2);
        put(raw, 168, 2);
    }
    reseal(&mut bytes);
    assert!(decode_layout(bytes.as_ref(), expected()).is_err()); // future epoch
    let mut bytes = root_fixture();
    let h = header(15, 2, 0, 70);
    add(&mut bytes, h, claim(h, 5, 1, 70, Some(reference(14, 2))));
    assert!(decode_layout(bytes.as_ref(), expected()).is_err()); // third claim/duplicate role
    let mut bytes = backing(ACTIVE);
    for ordinal in [13, 14] {
        let h = header(ordinal, 2, 0, 70);
        add(&mut bytes, h, claim(h, 5, 2, 70, None));
    }
    assert!(decode_layout(bytes.as_ref(), expected()).is_err()); // Child has one INIT
}

#[test]
fn structural_base_raw_digest_is_observation_only() {
    let mut bytes = root_fixture();
    raw_mut(&mut bytes, 13)[96] ^= 1;
    reseal(&mut bytes);
    decode_layout(bytes.as_ref(), expected()).unwrap();
    // No selected CURRENT or original accepted WAL is supplied by this test.
}

#[test]
fn structural_base_source_bound_covers_new_typed_carriers() {
    use std::mem::size_of;
    // Separate inventory from the production sum: common and Structural
    // grammar frames, then nested identity/accessor codecs and group loop.
    let grammar = size_of::<(&[u8; INIT_BYTES], Certificate, Expected, PageRef)>()
        + size_of::<(
            &[u8; INIT_BYTES],
            Certificate,
            Expected,
            SystemWorkRef,
            PageRef,
            PageRef,
            u64,
            Option<PageRef>,
            [Option<PageRef>; 4],
        )>()
        + size_of::<(usize, Option<PageRef>, PageRef)>()
        + 2 * size_of::<std::result::Result<SystemWorkRef, AllocationError>>()
        + 5 * size_of::<std::result::Result<PageRef, page::PageError>>()
        + 2 * size_of::<Result<()>>();
    let accessor = 4 * size_of::<base::InitProvenance<'_>>()
        + 4 * size_of::<base::StructuralInit<'_>>()
        + 2 * size_of::<ReservationOwner>()
        + 4 * size_of::<Option<foundation::identity::ObjectHandle>>()
        + 4 * size_of::<Option<PageRef>>()
        + 8 * size_of::<&[u8]>()
        + 16 * size_of::<u64>();
    let measured = grammar + accessor;
    let bound = image_records::provenance_source_bytes();
    println!(
        "STRUCTURAL_BASE_SOURCE measured={measured} bound={bound} row_layout={} catalog_layout={} row_closure={}",
        layout_source_bytes(),
        base::catalog::layout_source_bytes(),
        source_bytes()
    );
    assert!(measured <= bound);
    assert!(
        layout_source_bytes()
            >= base::catalog::layout_source_bytes() + 2 * size_of::<base::StructuralInit<'_>>()
    );
    assert!(source_bytes() >= layout_source_bytes());
}
