//! Only the new Free/page paths and the extracted common header are selected.
use super::*;
use crate::storage::page::{DIGEST_OFFSET, PageView};

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-undo-free-page"
);
fn raw(name: &str) -> Vec<u8> {
    std::fs::read(format!("{INPUT}/{name}")).unwrap()
}
fn mark(name: &str, cases: usize) {
    println!("XADB_UNDO_FREE_CASE {name} {cases}");
}
fn initial_identity() -> PageIdentity {
    PageIdentity {
        database: DatabaseId::new([0x31; 16]).unwrap(),
        page: PageRef {
            file: FileId::new(0x1_0000_0001).unwrap(),
            page_no: 2,
            incarnation: g(1),
        },
        owner: oid(3),
        owner_generation: g(1),
    }
}
fn rehash(bytes: &mut [u8]) {
    let hash = content_digest(&bytes[..DIGEST_OFFSET]);
    bytes[DIGEST_OFFSET..].copy_from_slice(&hash);
}

#[test]
fn undo_free_uf01_independent_literals() {
    for (value, name) in [
        (1, "free-g1.bin"),
        (6, "free-g6.bin"),
        (u64::MAX, "free-gmax.bin"),
    ] {
        let expected = raw(name);
        assert_eq!(expected.len(), 432);
        let mut output = [0xa5; 449];
        assert_eq!(encode_free_undo_record(g(value), &mut output), Ok(432));
        assert_eq!(&output[..432], expected);
        assert_eq!(&output[432..], &[0xa5; 17]);
        assert_eq!(decode_free_undo_record(&expected, g(value)), Ok(g(value)));
    }
    mark("UF01", 3);
}

#[test]
fn undo_free_uf02_record_bounds() {
    let input = raw("free-g1.bin");
    for len in 0..432 {
        let mut output = vec![0xa5; len];
        assert_eq!(
            encode_free_undo_record(g(1), &mut output),
            Err(UndoError::NoSpace)
        );
        assert!(output.iter().all(|b| *b == 0xa5));
        assert_eq!(
            decode_free_undo_record(&input[..len], g(1)),
            Err(UndoError::Corrupt)
        );
    }
    for len in [433, 864] {
        let mut input = input.clone();
        input.resize(len, 0);
        assert_eq!(
            decode_free_undo_record(&input, g(1)),
            Err(UndoError::Corrupt)
        );
    }
    mark("UF02", 866);
}

// One mutation in each semantic or reserved field, not product-produced bytes.
const RECORD_FLIPS: &[usize] = &[
    0, 4, 7, 16, 24, 64, 72, 80, 88, 120, 168, 216, 272, 320, 324, 325, 326, 327, 328, 329, 332,
    334, 336, 400, 424, 431,
];
#[test]
fn undo_free_uf03_record_rejections() {
    let original = raw("free-g1.bin");
    for &at in RECORD_FLIPS {
        let mut b = original.clone();
        b[at] ^= 1;
        let error = if at == 4 {
            UndoError::UnsupportedFormat
        } else {
            UndoError::Corrupt
        };
        assert_eq!(decode_free_undo_record(&b, g(1)), Err(error), "offset {at}");
    }
    for (state, error) in [
        (1, UndoError::StateMismatch),
        (2, UndoError::StateMismatch),
        (3, UndoError::Corrupt),
        (255, UndoError::Corrupt),
    ] {
        let mut b = original.clone();
        b[6] = state;
        assert_eq!(decode_free_undo_record(&b, g(1)), Err(error));
    }
    let mut zero = original.clone();
    zero[8..16].fill(0);
    assert_eq!(
        decode_free_undo_record(&zero, g(1)),
        Err(UndoError::Corrupt)
    );
    assert_eq!(
        decode_free_undo_record(&original, g(2)),
        Err(UndoError::IdentityMismatch)
    );
    // Header precedence is preserved even when a state/body is also invalid.
    let mut b = original.clone();
    b[4] = 2;
    b[7] = 1;
    b[8] = 0;
    b[6] = 1;
    assert_eq!(
        decode_free_undo_record(&b, g(2)),
        Err(UndoError::UnsupportedFormat)
    );
    b[4] = 1;
    assert_eq!(decode_free_undo_record(&b, g(2)), Err(UndoError::Corrupt));
    b[7] = 0;
    b[8] = 1;
    assert_eq!(
        decode_free_undo_record(&b, g(2)),
        Err(UndoError::IdentityMismatch)
    );
    mark("UF03", RECORD_FLIPS.len() + 9);
}

#[test]
fn undo_free_uf04_existing_header_compatibility() {
    let f = Fixtures::new();
    // Existing independent U01/U02/U13 only, since the common header moved.
    for i in [0, 1, 12] {
        let input = match i {
            0 => raw("free-g6.bin"),
            1 => raw("existing-U02-live.bin"),
            12 => raw("existing-U13-releasing.bin"),
            _ => unreachable!(),
        };
        let expected = f.expect(i);
        let schema = f.schema(i);
        let view = decode_undo_record(&input, expected, &schema).unwrap();
        assert_eq!(view.generation(), g(6));
        assert_eq!(
            view.state(),
            [
                UndoRecordState::Free,
                UndoRecordState::Live,
                UndoRecordState::Releasing
            ][usize::from(i != 0) + usize::from(i == 12)]
        );
        for (at, value, error) in [
            (0, 0, UndoError::Corrupt),
            (4, 2, UndoError::UnsupportedFormat),
            (7, 1, UndoError::Corrupt),
            (329, 1, UndoError::Corrupt),
            (334, 1, UndoError::Corrupt),
            (424, 1, UndoError::Corrupt),
            (8, 0, UndoError::Corrupt),
            (8, 7, UndoError::IdentityMismatch),
        ] {
            let mut damaged = input.clone();
            damaged[at] = value;
            assert_eq!(
                decode_undo_record(&damaged, expected, &schema).unwrap_err(),
                error
            );
        }
        let mut damaged = input.clone();
        damaged[6] = 255;
        damaged[8] = 7;
        assert_eq!(
            decode_undo_record(&damaged, expected, &schema).unwrap_err(),
            UndoError::IdentityMismatch
        );
        damaged[8] = 6;
        assert_eq!(
            decode_undo_record(&damaged, expected, &schema).unwrap_err(),
            UndoError::Corrupt
        );
        let mut invalid = expected;
        invalid.namespace.timeline = 0;
        assert_eq!(
            decode_undo_record(&[], invalid, &schema).unwrap_err(),
            UndoError::InvalidInput
        );
    }
    let expected = f.expect(0);
    let mut output = [0xa5; 449];
    assert_eq!(
        encode_undo_record(UndoCell::Free(g(6)), expected, &[], &mut output),
        Ok(432)
    );
    assert_eq!(&output[..432], literal(0));
    assert_eq!(&output[432..], &[0xa5; 17]);
    let mut invalid = expected;
    invalid.target.entity = 0;
    let mut output = [0xa5; 431];
    assert_eq!(
        encode_undo_record(UndoCell::Free(g(6)), invalid, &[], &mut output),
        Err(UndoError::InvalidInput)
    );
    assert_eq!(output, [0xa5; 431]);
    assert_eq!(
        decode_undo_record(&[], invalid, &[]).unwrap_err(),
        UndoError::InvalidInput
    );
    assert_eq!(
        encode_undo_record(UndoCell::Free(g(7)), expected, &[], &mut output),
        Err(UndoError::IdentityMismatch)
    );
    assert_eq!(output, [0xa5; 431]);
    assert_eq!(
        encode_undo_record(UndoCell::Free(g(6)), expected, &[], &mut output),
        Err(UndoError::NoSpace)
    );
    assert_eq!(output, [0xa5; 431]);
    let mut damaged = raw("free-g6.bin");
    damaged[16] = 1;
    assert_eq!(
        decode_undo_record(&damaged, expected, &[]).unwrap_err(),
        UndoError::Corrupt
    );
    damaged[16] = 0;
    damaged.push(0);
    assert_eq!(
        decode_undo_record(&damaged, expected, &[]).unwrap_err(),
        UndoError::Corrupt
    );
    mark("UF04", 43);
}

#[test]
fn undo_free_uf05_initial_page() {
    let original = raw("initial-page.bin");
    let mut output = vec![0xa5; PAGE_BYTES];
    assert_eq!(
        encode_initial_undo_page(initial_identity(), PageLsn(1234), &mut output),
        Ok(())
    );
    assert_eq!(output, original);
    assert_eq!(
        initial_undo_page_lsn(&original, initial_identity()),
        Ok(PageLsn(1234))
    );
    for lsn in [0, u64::MAX] {
        // Independent expected page field substitution; hash is envelope-only.
        let mut expected = original.clone();
        expected[64..72].copy_from_slice(&lsn.to_le_bytes());
        rehash(&mut expected);
        encode_initial_undo_page(initial_identity(), PageLsn(lsn), &mut output).unwrap();
        assert_eq!(output, expected);
        assert_eq!(
            initial_undo_page_lsn(&expected, initial_identity()),
            Ok(PageLsn(lsn))
        );
    }
    for len in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let mut output = vec![0xa5; len];
        assert_eq!(
            encode_initial_undo_page(initial_identity(), PageLsn(1234), &mut output),
            Err(UndoError::InvalidInput)
        );
        assert!(output.iter().all(|b| *b == 0xa5));
        let mut input = original.clone();
        input.resize(len, 0);
        assert_eq!(
            initial_undo_page_lsn(&input, initial_identity()),
            Err(UndoError::Corrupt)
        );
    }
    mark("UF05", 9);
}

const PAGE_FLIPS: &[usize] = &[
    0, 4, 7, 72, 74, 76, 78, 128, 130, 132, 15920, 15924, 15927, 15936, 16249, 16344,
];
#[test]
fn undo_free_uf06_page_rejections() {
    let original = raw("initial-page.bin");
    for &at in PAGE_FLIPS {
        let mut bytes = original.clone();
        bytes[at] ^= 1;
        rehash(&mut bytes);
        let error = if at == 4 || at == 15924 {
            UndoError::UnsupportedFormat
        } else {
            UndoError::Corrupt
        };
        assert_eq!(
            initial_undo_page_lsn(&bytes, initial_identity()),
            Err(error),
            "offset {at}"
        );
    }
    for at in [8, 24, 32, 40, 48, 56] {
        let mut expected = initial_identity();
        match at {
            8 => expected.database = DatabaseId::new([0x32; 16]).unwrap(),
            24 => expected.page.file = FileId::new(0x1_0000_0002).unwrap(),
            32 => expected.page.page_no = 3,
            40 => expected.page.incarnation = g(2),
            48 => expected.owner = oid(4),
            56 => expected.owner_generation = g(2),
            _ => unreachable!(),
        }
        assert_eq!(
            initial_undo_page_lsn(&original, expected),
            Err(UndoError::IdentityMismatch)
        );
    }
    let mut bytes = original.clone();
    bytes[6] = 9;
    rehash(&mut bytes);
    assert_eq!(
        initial_undo_page_lsn(&bytes, initial_identity()),
        Err(UndoError::IdentityMismatch)
    );
    bytes = original.clone();
    bytes[15928] = 2;
    rehash(&mut bytes);
    assert_eq!(
        initial_undo_page_lsn(&bytes, initial_identity()),
        Err(UndoError::IdentityMismatch)
    );
    for state in [1, 2] {
        let mut bytes = original.clone();
        bytes[15926] = state;
        rehash(&mut bytes);
        assert_eq!(
            initial_undo_page_lsn(&bytes, initial_identity()),
            Err(UndoError::StateMismatch)
        );
    }
    let mut bytes = original.clone();
    bytes[15920..15924].copy_from_slice(b"XUC1");
    rehash(&mut bytes);
    assert_eq!(
        initial_undo_page_lsn(&bytes, initial_identity()),
        Err(UndoError::Corrupt)
    );
    let mut bytes = original.clone();
    bytes[DIGEST_OFFSET] ^= 1;
    assert_eq!(
        initial_undo_page_lsn(&bytes, initial_identity()),
        Err(UndoError::Corrupt)
    );
    // Valid envelopes with zero/two slots still do not match the initial shape.
    let free = raw("free-g1.bin");
    for cells in [&[][..], &[&free[..], &free[..]][..]] {
        let mut bytes = vec![0; PAGE_BYTES];
        encode_page(
            PageHeader {
                identity: initial_identity(),
                kind: PageKind::Undo,
                level: 0,
                lsn: PageLsn(1234),
            },
            cells,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(
            initial_undo_page_lsn(&bytes, initial_identity()),
            Err(UndoError::Corrupt)
        );
    }
    mark("UF06", PAGE_FLIPS.len() + 14);
}

#[test]
fn undo_free_uf07_source_value_census() {
    // Conservative simultaneous source values of the new pure helpers and
    // nested unchanged page/hash codec, not measured machine stack or RSS.
    let bytes = 432
        + 4 * size_of::<PageHeader>()
        + 4 * size_of::<PageIdentity>()
        + 2 * size_of::<PageView<'static>>()
        + 4 * size_of::<Generation>()
        + 4 * size_of::<ContentHasher>()
        + 64
        + 1536;
    assert!(bytes <= 4096, "{bytes}");
    println!("XADB_UNDO_FREE_CENSUS {bytes} bound4096 caller_page16384 retained0 heap0");
    mark("UF07", 1);
}

#[test]
#[ignore = "final DEV3 S5: expanded per-byte mutations and all short page lengths"]
fn undo_free_s5_expanded_corruption() {
    let free = raw("free-g1.bin");
    let mut count = 0;
    for at in 0..432 {
        if RECORD_FLIPS.contains(&at) || [6, 8].contains(&at) {
            continue;
        }
        let mut bytes = free.clone();
        bytes[at] ^= 1;
        assert!(
            decode_free_undo_record(&bytes, g(1)).is_err(),
            "record {at}"
        );
        count += 1;
    }
    let page = raw("initial-page.bin");
    for at in 0..DIGEST_OFFSET {
        if (64..72).contains(&at) || PAGE_FLIPS.contains(&at) || [6, 15926].contains(&at) {
            continue;
        }
        let mut bytes = page.clone();
        bytes[at] ^= 1;
        rehash(&mut bytes);
        assert!(
            initial_undo_page_lsn(&bytes, initial_identity()).is_err(),
            "page {at}"
        );
        count += 1;
    }
    for len in 1..PAGE_BYTES - 1 {
        assert_eq!(
            initial_undo_page_lsn(&page[..len], initial_identity()),
            Err(UndoError::Corrupt)
        );
        count += 1;
        let mut output = vec![0xa5; len];
        assert_eq!(
            encode_initial_undo_page(initial_identity(), PageLsn(1234), &mut output),
            Err(UndoError::InvalidInput)
        );
        assert!(output.iter().all(|b| *b == 0xa5));
        count += 1;
    }
    assert_eq!(count, 49_494);
    mark("S5", count);
}
