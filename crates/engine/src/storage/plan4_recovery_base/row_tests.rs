use super::*;
use crate::storage::{control, plan4_recovery_base as base};
use base::{AllocationTerminal, ControlRef, PageHeader, SelectedControl, put};

const INITIAL: &[u8; BASE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/base4/initial.xrb"
));
const ACTIVE: &[u8; BASE_BYTES] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/base4/active.xrb"
));
const PROFILES: &[u8; 12288] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));

fn expected() -> Expected {
    Expected {
        profile: control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap(),
        filemap: SelectedControl {
            generation: 1,
            reference: ControlRef::decode(&INITIAL[384..448]).unwrap(),
        },
        bootstrap_digest: INITIAL[464..496].try_into().unwrap(),
    }
}
fn backing(bytes: &[u8]) -> Box<[u8; BASE_BYTES]> {
    bytes.to_vec().into_boxed_slice().try_into().unwrap()
}
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn reseal(bytes: &mut [u8; BASE_BYTES]) {
    for (i, (offset, length, _, _)) in SECTIONS.iter().copied().enumerate() {
        let digest = content_digest(&bytes[offset..offset + length]);
        bytes[1280 + i * 64 + 32..1280 + i * 64 + 64].copy_from_slice(&digest);
    }
    seal(bytes);
}
fn image_changed(bytes: &mut [u8; BASE_BYTES], n: usize) {
    let at = ARENA + n * PAGE_BYTES;
    seal(&mut bytes[at..at + PAGE_BYTES]);
    let digest = content_digest(&bytes[at..at + PAGE_BYTES]);
    bytes[CERTIFICATES + n * 96 + 56..CERTIFICATES + n * 96 + 88].copy_from_slice(&digest);
    reseal(bytes);
}
fn sections(bytes: &[u8; BASE_BYTES]) -> Sections<'_> {
    Sections {
        descriptors: bytes[BANK..BANK + DESCRIPTOR_BYTES].try_into().unwrap(),
        certificates: bytes[CERTIFICATES..CERTIFICATES + 64 * 96]
            .as_chunks::<96>()
            .0
            .try_into()
            .unwrap(),
        provenance: bytes[PROVENANCE..PROVENANCE + 64 * 352]
            .as_chunks::<352>()
            .0
            .try_into()
            .unwrap(),
        inventory: bytes[INVENTORY..INVENTORY + PAGE_BYTES].try_into().unwrap(),
        images: bytes[ARENA..ARENA + 64 * PAGE_BYTES]
            .as_chunks::<PAGE_BYTES>()
            .0
            .try_into()
            .unwrap(),
    }
}
fn reject(bytes: &mut [u8; BASE_BYTES]) {
    reseal(bytes);
    assert!(decode_layout(bytes, expected()).is_err());
}

#[test]
fn base4_literal_layout_roundtrip_and_borrows() {
    for bytes in [INITIAL, ACTIVE] {
        let view = decode_layout(bytes, expected()).unwrap();
        assert!(std::ptr::eq(view.bytes(), bytes));
        assert!(std::ptr::eq(
            view.descriptor_bank().as_ptr(),
            bytes[BANK..].as_ptr()
        ));
        assert!(std::ptr::eq(
            view.inventory().as_ptr(),
            bytes[INVENTORY..].as_ptr()
        ));
        assert_eq!(view.digest(), content_digest(bytes));
        let h = view.header();
        assert_eq!(h.format, BaseFormat::V4RowMissing);
        let dynamic = std::ptr::eq(bytes, ACTIVE);
        assert_eq!(h.active_transactions, if dynamic { 2 } else { 0 });
        assert_eq!(h.leased.count_ones(), if dynamic { 3 } else { 0 });
        assert_eq!(
            h.allocation_terminal,
            if dynamic {
                AllocationTerminal::InitializedUndo
            } else {
                AllocationTerminal::None
            }
        );
        for ordinal in 0..64 {
            let cert = view.certificate(ordinal).unwrap();
            assert_eq!(cert.is_some(), ordinal < if dynamic { 13 } else { 9 });
            if let Some(cert) = cert {
                let image = view.image(ordinal).unwrap().unwrap();
                assert!(std::ptr::eq(
                    image.as_ptr(),
                    bytes[ARENA + ordinal * PAGE_BYTES..].as_ptr()
                ));
                let mut out = [0xa5; 96];
                encode_certificate(expected(), cert.header, image, &mut out).unwrap();
                assert_eq!(
                    out,
                    bytes[CERTIFICATES + ordinal * 96..CERTIFICATES + (ordinal + 1) * 96]
                );
            } else {
                assert!(view.image(ordinal).unwrap().is_none());
            }
            let provenance = view.provenance(ordinal).unwrap();
            assert_eq!(provenance.is_some(), dynamic && (9..13).contains(&ordinal));
            if let Some(p) = provenance {
                assert_eq!(p.role(), ordinal as u8 - 8);
                assert_eq!(p.page().page_no, ordinal as u64);
                assert_eq!(p.writer_epoch(), 1);
                assert_eq!(p.reserve_lsn() + 1, p.init_lsn());
                assert_eq!(
                    p.initial_digest(),
                    &content_digest(view.image(ordinal).unwrap().unwrap())
                );
                assert!(std::ptr::eq(
                    p.bytes().as_ptr(),
                    bytes[PROVENANCE + ordinal * 352..].as_ptr()
                ));
                let _ = p.source();
                assert_eq!(p.object().is_some(), ordinal >= 11);
                assert_eq!(p.pair().is_some(), ordinal >= 11);
            }
        }
        let mut out = backing(&vec![0xa5; BASE_BYTES]);
        assert_eq!(
            encode_layout(h, expected(), sections(bytes), &mut out)
                .unwrap()
                .bytes(),
            bytes
        );
    }
}

#[test]
fn base4_exact_dispatch_and_shared_header_modes() {
    assert_eq!(
        std::mem::size_of::<HeaderMode>(),
        std::mem::size_of::<bool>()
    );
    for version in [0u16, 1, 2, 3, 5, u16::MAX] {
        let mut bytes = backing(INITIAL);
        put16(&mut *bytes, 4, version);
        reseal(&mut bytes);
        assert_eq!(
            decode_layout(&*bytes, expected()).err(),
            Some(Error::Unsupported)
        );
    }
    let old1 = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dev3-plan4-recovery-base/vectors/p1/references/profile.bin"
    ));
    let mut old2 = PROFILES[..4096].to_vec();
    put16(&mut old2, 4, 2);
    seal(&mut old2);
    let profiles = [
        control::decode_bootstrap_profile(old1, None).unwrap(),
        control::decode_bootstrap_profile(&old2, None).unwrap(),
        expected().profile,
    ];
    let original = decode_layout(INITIAL, expected()).unwrap().header();
    for (format, id) in [
        (BaseFormat::V1Status, 0),
        (BaseFormat::V2System, 0),
        (BaseFormat::V3Catalog, 1),
        (BaseFormat::V4RowMissing, 2),
    ] {
        for (index, profile) in profiles.iter().copied().enumerate() {
            let e = Expected {
                profile,
                ..expected()
            };
            let h = Header { format, ..original };
            assert_eq!(base::validate_header(h, e).is_ok(), index == id);
        }
        let e = Expected {
            profile: profiles[id],
            ..expected()
        };
        let h = Header { format, ..original };
        let mut raw = vec![0; e.profile.base_bytes()];
        base::write_header(h, e, &mut raw[..4096]).unwrap();
        for mode in [HeaderMode::Legacy, HeaderMode::Catalog, HeaderMode::Row] {
            let allowed = match mode {
                HeaderMode::Legacy => matches!(format, BaseFormat::V1Status | BaseFormat::V2System),
                HeaderMode::Catalog => format != BaseFormat::V4RowMissing,
                HeaderMode::Row => format == BaseFormat::V4RowMissing,
            };
            assert_eq!(read_header_format(&raw, e, mode).is_ok(), allowed);
        }
    }
    assert_eq!(
        base::catalog::decode_layout(INITIAL, expected()).err(),
        Some(Error::Unsupported)
    );
    let old: &[u8; base::BASE_BYTES] = INITIAL[..base::BASE_BYTES].try_into().unwrap();
    assert_eq!(
        base::read_header(old, expected()).err(),
        Some(Error::Unsupported)
    );
    let zero_page = [0; PAGE_BYTES];
    let queue = [[0; PAGE_BYTES]; 6];
    let statuses = [[0; 352]; 64];
    let mut legacy_out = vec![0; base::BASE_BYTES];
    let mut scratch = [[0; PAGE_BYTES]; 4];
    assert_eq!(
        base::encode(
            Header {
                format: BaseFormat::V4RowMissing,
                ..original
            },
            expected(),
            base::Sections {
                descriptors: sections(INITIAL).descriptors,
                allocator: &zero_page,
                queue: &queue,
                statuses: &statuses,
                certificates: sections(INITIAL).certificates,
                inventory: &zero_page,
            },
            legacy_out.as_mut_slice().try_into().unwrap(),
            &mut scratch
        )
        .err(),
        Some(Error::Unsupported)
    );
}

#[test]
fn base4_section_geometry_gaps_and_checksums() {
    for len in [0, 4096, BASE_BYTES - 1, BASE_BYTES + 1] {
        assert_eq!(
            decode_layout(&vec![0; len], expected()).err(),
            Some(Error::Corrupt)
        );
    }
    for at in [0, BASE_BYTES - 1, ARENA + PAGE_BYTES - 1] {
        let mut b = backing(INITIAL);
        b[at] ^= 1;
        assert_eq!(decode_layout(&*b, expected()).err(), Some(Error::Corrupt));
    }
    for i in 0..5 {
        let mut b = backing(INITIAL);
        b[1280 + i * 64 + 4] ^= 1;
        seal(&mut *b);
        assert_eq!(decode_layout(&*b, expected()).err(), Some(Error::Corrupt));
        let mut b = backing(INITIAL);
        b[1280 + i * 64 + 32] ^= 1;
        seal(&mut *b);
        assert_eq!(decode_layout(&*b, expected()).err(), Some(Error::Corrupt));
    }
    for at in [
        1600,
        4096 + DESCRIPTOR_BYTES,
        65536 + 6144,
        73728 + 22528,
        98304 + PAGE_BYTES,
        ARENA + 64 * PAGE_BYTES,
    ] {
        let mut b = backing(INITIAL);
        b[at] = 1;
        reject(&mut b);
    }
}

#[test]
fn base4_allocator_certificate_and_image_relations() {
    // Free and Reserved slots must have no certificate, INIT or page bytes.
    for n in [13, 14] {
        for section in [CERTIFICATES, PROVENANCE, ARENA] {
            let width = if section == CERTIFICATES {
                96
            } else if section == PROVENANCE {
                352
            } else {
                PAGE_BYTES
            };
            let mut b = backing(ACTIVE);
            b[section + n * width] = 1;
            reject(&mut b);
        }
    }
    for n in [0, 9] {
        let mut b = backing(ACTIVE);
        b[CERTIFICATES + n * 96..CERTIFICATES + (n + 1) * 96].fill(0);
        reject(&mut b);
        for field in [8, 16, 24, 32, 40, 48, 88] {
            let mut b = backing(ACTIVE);
            b[CERTIFICATES + n * 96 + field] ^= 1;
            reject(&mut b);
        }
    }
    let mut b = backing(ACTIVE);
    b[ARENA + 9 * PAGE_BYTES + 100] = 1;
    reseal(&mut b);
    assert!(decode_layout(&*b, expected()).is_err());
    // Current bytes and their digest can agree while certificate identity differs.
    let mut b = backing(ACTIVE);
    put(&mut *b, ARENA + 9 * PAGE_BYTES + 64, 1);
    image_changed(&mut b, 9);
    assert!(decode_layout(&*b, expected()).is_err());
    let mut b = backing(ACTIVE);
    put(&mut *b, 96, 6);
    put(&mut *b, 248, 6);
    reject(&mut b);
}

#[test]
fn base4_init_records_and_reciprocal_pair() {
    for (n, field) in [
        (1, 0),
        (9, 0),
        (9, 1),
        (9, 2),
        (9, 4),
        (9, 8),
        (9, 32),
        (9, 40),
        (9, 48),
        (9, 56),
        (9, 128),
        (9, 152),
        (9, 168),
        (9, 176),
        (9, 184),
        (9, 256),
        (10, 1),
        (11, 128),
        (11, 152),
        (11, 168),
        (11, 184),
        (11, 208),
        (11, 240),
    ] {
        let mut b = backing(ACTIVE);
        b[PROVENANCE + n * 352 + field] ^= 1;
        reject(&mut b);
    }
    // Each variant is locally well formed; the other record must still agree.
    for field in [48, 64, 160, 176] {
        let mut b = backing(ACTIVE);
        let at = PROVENANCE + 12 * 352 + field;
        if field == 48 {
            put(&mut *b, at, 5);
            put(&mut *b, PROVENANCE + 12 * 352 + 176, 4);
        } else if field == 176 {
            put(&mut *b, at, 4);
        } else {
            b[at] ^= 2;
        }
        reseal(&mut b);
        assert_eq!(
            decode_layout(&*b, expected()).err(),
            Some(Error::ProvenanceMismatch)
        );
    }
}

#[test]
fn base4_queue_lsn_and_header_summary() {
    for at in [256, 264, 272, 280, 288, 296] {
        let mut b = backing(ACTIVE);
        b[at] ^= 1;
        reject(&mut b);
    }
    // Both generic image and certificate agree; Row wrapper or queue order rejects.
    for n in [1, 2] {
        let mut b = backing(ACTIVE);
        put(&mut *b, ARENA + n * PAGE_BYTES + 64, 100);
        put(&mut *b, CERTIFICATES + n * 96 + 48, 100);
        put(&mut *b, 96, 100);
        put(&mut *b, 248, 100);
        image_changed(&mut b, n);
        assert!(decode_layout(&*b, expected()).is_err());
    }
    for n in [1, 2] {
        let mut b = backing(ACTIVE);
        let at = ARENA + n * PAGE_BYTES;
        let cell = get16(&*b, at + 128) as usize;
        b[at + cell + 4] = 2;
        image_changed(&mut b, n);
        assert!(decode_layout(&*b, expected()).is_err());
    }
}

#[test]
fn base4_layout_does_not_claim_graph_or_history() {
    // A raw layout deliberately cannot grant semantic descriptors/inventory,
    // Job census commitments, original INIT/frame history or selected ownership.
    let mut b = backing(ACTIVE);
    b[BANK] = 0xa5;
    b[INVENTORY] = 0x5a;
    let at = ARENA + PAGE_BYTES;
    let cell = get16(&*b, at + 128) as usize;
    b[at + cell + 128] ^= 1; // Root's opaque job digest, full census checks separately.
    b[PROVENANCE + 9 * 352 + 96] ^= 1; // Initial image claim is not the current image hash.
    b[160] ^= 1;
    image_changed(&mut b, 1);
    let view = decode_layout(&*b, expected()).unwrap();
    assert_eq!(view.descriptor_bank()[0], 0xa5);
    assert_eq!(view.inventory()[0], 0x5a);
}

#[test]
fn base4_encode_rejections_and_accessor_bounds() {
    let e = expected();
    let h = decode_layout(INITIAL, e).unwrap().header();
    let view = decode_layout(ACTIVE, e).unwrap();
    for n in [64, usize::MAX] {
        assert_eq!(view.certificate(n).err(), Some(Error::InvalidInput));
        assert_eq!(view.provenance(n).err(), Some(Error::InvalidInput));
        assert_eq!(view.image(n).err(), Some(Error::InvalidInput));
    }
    let mut output = backing(&vec![0xa5; BASE_BYTES]);
    assert_eq!(
        encode_layout(
            Header {
                format: BaseFormat::V3Catalog,
                ..h
            },
            e,
            sections(INITIAL),
            &mut output
        )
        .err(),
        Some(Error::Unsupported)
    );
    assert!(output.iter().all(|b| *b == 0xa5));
    assert_eq!(
        encode_layout(
            Header {
                capture_lsn: 0,
                ..h
            },
            e,
            sections(INITIAL),
            &mut output
        )
        .err(),
        Some(Error::InvalidInput)
    );
    let mut bad = backing(INITIAL);
    bad[ARENA + 9 * PAGE_BYTES] = 1;
    assert!(encode_layout(h, e, sections(&bad), &mut output).is_err());
    // Later error leaves unpublished staging changed, as documented.
    assert_eq!(output[ARENA + 9 * PAGE_BYTES], 1);
    let cert = view.certificate(9).unwrap().unwrap();
    let image = view.image(9).unwrap().unwrap();
    let mut record = [0xa5; 96];
    let wrong = PageHeader {
        lsn: base::PageLsn(3),
        ..cert.header
    };
    assert_eq!(
        encode_certificate(e, wrong, image, &mut record),
        Err(Error::IdentityMismatch)
    );
    assert_eq!(record, [0xa5; 96]);
}
