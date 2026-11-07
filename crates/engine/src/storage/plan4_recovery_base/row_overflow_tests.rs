//! OVF-F06 (dev3-row-overflow.md#base4): the Overflow certificate role and
//! the role 7 INIT claim of a ChunkWrite's chunk. Raw records are written
//! here from the contract's offsets, never by a product claim encoder.
use super::*;
use crate::storage::page;
use base::{Certificate, PageIdentity, PageLsn, PageRef, image_records};
use foundation::identity::{Generation, ObjectId};

fn reference(number: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: base::first_file(),
        page_no: number,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}
/// Chunk page 20 of table 6 at LSN 70.
fn chunk_header(kind: PageKind, level: u8) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: expected().profile.spec().namespace.database,
            page: reference(20, 2),
            owner: ObjectId::new(6).unwrap(),
            owner_generation: one(),
        },
        kind,
        level,
        lsn: PageLsn(70),
    }
}
fn raw_certificate(h: PageHeader, object: u64, digest: [u8; 32]) -> [u8; 96] {
    let mut raw = [0; 96];
    raw[0] = 1;
    raw[1] = h.kind as u8;
    raw[2] = h.level;
    h.identity.page.encode(&mut raw[8..32]).unwrap();
    put(&mut raw, 32, object);
    put(&mut raw, 40, 1);
    put(&mut raw, 48, h.lsn.0);
    raw[56..88].copy_from_slice(&digest);
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
/// A role 7 claim: the inserting transaction (database, timeline 1, writer
/// epoch 1, local 3, generation 2), root lease 60 < INIT 70, table 6, anchor
/// page 10, chunk `index` of a `total`-byte chain of `count` chunks.
fn claim(index: u16, count: u16, total: u32) -> [u8; INIT_BYTES] {
    let mut raw = [0; INIT_BYTES];
    raw[0] = 1;
    raw[1] = 7;
    raw[2] = PageKind::Overflow as u8;
    raw[4] = 2;
    reference(20, 2).encode(&mut raw[8..32]).unwrap();
    put(&mut raw, 32, 6);
    put(&mut raw, 40, 1);
    put(&mut raw, 48, 70);
    raw[56..60].copy_from_slice(&1u32.to_le_bytes());
    raw[64..96].fill(0x51);
    raw[96..128].fill(0x62);
    raw[128..144].copy_from_slice(&expected().profile.spec().namespace.database.bytes());
    put(&mut raw, 144, 1);
    raw[152..156].copy_from_slice(&1u32.to_le_bytes());
    put(&mut raw, 160, 3);
    put(&mut raw, 168, 2);
    put(&mut raw, 176, 60);
    put(&mut raw, 184, 6);
    put(&mut raw, 192, 1);
    put(&mut raw, 200, 1);
    reference(10, 2).encode(&mut raw[208..232]).unwrap();
    raw[232..234].copy_from_slice(&index.to_le_bytes());
    raw[234..236].copy_from_slice(&count.to_le_bytes());
    raw[236..240].copy_from_slice(&total.to_le_bytes());
    raw
}
fn certificate() -> Certificate {
    Certificate {
        header: chunk_header(PageKind::Overflow, 0),
        full_digest: [0x42; 32],
    }
}
fn validate(raw: &[u8; INIT_BYTES]) -> Result<()> {
    image_records::validate_provenance(raw, certificate(), expected())
}

/// OVF-F06: a table's Overflow page is a dynamic certificate role (level 0,
/// object ≥ 6) of the Row profile only; its encoder round-trips.
#[test]
fn ovf_f06_overflow_certificate_role() {
    let h = chunk_header(PageKind::Overflow, 0);
    let mut image = Box::new([0; PAGE_BYTES]);
    page::encode_page(h, &[&[0x5a; 300]], image.as_mut()).unwrap();
    let expected_raw = raw_certificate(h, 6, content_digest(image.as_ref()));
    let mut raw = [0xa5; 96];
    image_records::encode_certificate(expected(), h, image.as_ref(), &mut raw).unwrap();
    assert_eq!(raw, expected_raw);
    let decoded = image_records::decode_certificate(&raw, expected(), 20)
        .unwrap()
        .unwrap();
    assert_eq!(decoded.header, h);
    // The catalog profile has no overflow chunks.
    assert_eq!(
        image_records::decode_certificate(&raw, profile2(), 20).err(),
        Some(Error::Unsupported)
    );
    // A system object, a level, a fixed ordinal.
    for (object, level, ordinal) in [(5, 0, 20), (2, 0, 20), (6, 1, 20)] {
        let h = chunk_header(PageKind::Overflow, level);
        let raw = raw_certificate(h, object, [0; 32]);
        assert!(
            image_records::decode_certificate(&raw, expected(), ordinal).is_err(),
            "{object} {level}"
        );
    }
    let mut fixed = raw_certificate(h, 6, [0; 32]);
    reference(8, 1).encode(&mut fixed[8..32]).unwrap();
    assert!(image_records::decode_certificate(&fixed, expected(), 8).is_err());
}

/// OVF-F06: the role 7 claim's every field accepted and refused; `pair()`
/// is None and `chunk()` reports the chain position; roles 3 and 4 keep
/// their `pair()`.
#[test]
fn ovf_f06_role7_provenance_fields() {
    for (index, count, total) in [
        (0, 1, 2_049),
        (0, 1, 16_092),
        (1, 2, 16_093),
        (8, 9, 131_072),
        (3, 9, 128_737),
    ] {
        let raw = claim(index, count, total);
        validate(&raw).unwrap();
        let view = base::InitProvenance { bytes: &raw };
        assert_eq!(view.role(), 7);
        assert_eq!(view.pair(), None);
        assert!(view.structural().is_none());
        let chunk = view.chunk().unwrap();
        assert_eq!(chunk.anchor, reference(10, 2));
        assert_eq!(
            (chunk.index, chunk.count, chunk.total),
            (index, count, total)
        );
        assert_eq!(view.object().unwrap().object.get(), 6);
        assert_eq!(view.reserve_lsn(), 60);
        assert_eq!(view.init_lsn(), 70);
    }
    let refused = |raw: [u8; INIT_BYTES]| {
        assert_eq!(validate(&raw), Err(Error::ProvenanceMismatch));
    };
    // Chain position: total range, count = ceil(total/16092), index < count.
    for (index, count, total) in [
        (0, 1, 2_048),
        (0, 9, 131_073),
        (0, 2, 16_092),
        (0, 1, 16_093),
        (2, 2, 16_093),
        (0, 0, 2_049),
    ] {
        refused(claim(index, count, total));
    }
    let ok = || claim(1, 2, 16_093);
    // Reserved tail, level, lease ≥ INIT, the table, the anchor.
    for at in [240, 255, 256, 351] {
        let mut raw = ok();
        raw[at] = 1;
        refused(raw);
    }
    let mut raw = ok();
    raw[3] = 1;
    refused(raw);
    let mut raw = ok();
    put(&mut raw, 176, 70);
    refused(raw);
    for (at, value) in [(184, 5), (184, 7), (192, 2), (200, 2)] {
        let mut raw = ok();
        put(&mut raw, at, value);
        assert!(validate(&raw).is_err(), "{at}");
    }
    for anchor in [
        reference(20, 2),
        reference(20, 3),
        reference(8, 2),
        reference(64, 2),
    ] {
        let mut raw = ok();
        anchor.encode(&mut raw[208..232]).unwrap();
        refused(raw);
    }
    let mut raw = ok();
    raw[208..232].fill(0);
    assert!(validate(&raw).is_err());
    // The source: another timeline or writer epoch, generation 1.
    for (at, value) in [(144, 2), (168, 1)] {
        let mut raw = ok();
        put(&mut raw, at, value);
        assert!(validate(&raw).is_err(), "{at}");
    }
    let mut raw = ok();
    raw[56..60].copy_from_slice(&2u32.to_le_bytes());
    refused(raw);
    // Role 7 names a chunk page; another role or source kind is not one.
    let mut raw = ok();
    raw[4] = 1;
    assert!(validate(&raw).is_err());
    for role in [3, 4] {
        let mut raw = ok();
        raw[1] = role;
        assert!(validate(&raw).is_err());
    }
    for role in [8, 9] {
        let mut raw = ok();
        raw[1] = role;
        assert_eq!(validate(&raw), Err(Error::Unsupported));
    }
    // A role 7 claim of a RowLeaf certificate, and under the catalog profile.
    let mut raw = ok();
    raw[2] = PageKind::RowLeaf as u8;
    let leaf = Certificate {
        header: chunk_header(PageKind::RowLeaf, 0),
        full_digest: [0x42; 32],
    };
    assert_eq!(
        image_records::validate_provenance(&raw, leaf, expected()),
        Err(Error::ProvenanceMismatch)
    );
    assert!(image_records::validate_provenance(&ok(), certificate(), profile2()).is_err());
}

/// OVF-F06: a CREATE claim (roles 3/4) keeps its `pair()` and has no
/// `chunk()`.
#[test]
fn ovf_f06_create_claims_keep_their_pair() {
    for role in [3u8, 4] {
        let mut raw = claim(0, 1, 2_049);
        raw[1] = role;
        raw[2] = if role == 3 {
            PageKind::TreeAnchor
        } else {
            PageKind::RowLeaf
        } as u8;
        let (anchor, root) = if role == 3 {
            (reference(20, 2), reference(11, 2))
        } else {
            (reference(10, 2), reference(20, 2))
        };
        anchor.encode(&mut raw[208..232]).unwrap();
        root.encode(&mut raw[232..256]).unwrap();
        let cert = Certificate {
            header: chunk_header(
                if role == 3 {
                    PageKind::TreeAnchor
                } else {
                    PageKind::RowLeaf
                },
                0,
            ),
            full_digest: [0x42; 32],
        };
        image_records::validate_provenance(&raw, cert, expected()).unwrap();
        let view = base::InitProvenance { bytes: &raw };
        assert_eq!(view.pair(), Some([anchor, root]));
        assert_eq!(view.chunk(), None);
    }
}
