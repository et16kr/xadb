use super::*;

const EMPTY: &[u8; PAGE_BYTES] = include_bytes!("../../tests/fixtures/dev3-page-v1-empty.bin");
const CELLS: &[u8; PAGE_BYTES] = include_bytes!("../../tests/fixtures/dev3-page-v1-cells.bin");

fn header() -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: PageRef {
                file: FileId::new(0x0000_0001_0000_0002).unwrap(),
                page_no: 3,
                incarnation: Generation::new(4).unwrap(),
            },
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(6).unwrap(),
        },
        kind: PageKind::RowLeaf,
        level: 0,
        lsn: PageLsn(7),
    }
}

fn decode(bytes: &[u8]) -> Result<PageView<'_>, PageError> {
    decode_page(bytes, header().identity, PageKind::RowLeaf)
}

fn resign(bytes: &mut [u8]) {
    let hash = content_digest(&bytes[..DIGEST_OFFSET]);
    bytes[DIGEST_OFFSET..].copy_from_slice(&hash);
}

#[test]
fn independent_page_vectors_match_every_byte_and_borrow_exact_cells() {
    // Expectations were constructed from the offset table with Python's
    // struct/hashlib, before consulting this production codec.
    for (fixture, cells) in [
        (EMPTY, vec![]),
        (CELLS, vec![b"alpha".as_slice(), &[0, 255], b"z"]),
    ] {
        let mut output = [0xa5; PAGE_BYTES];
        encode_page(header(), &cells, &mut output).unwrap();
        assert_eq!(&output, fixture);
        let view = decode(fixture).unwrap();
        assert_eq!(view.header(), header());
        assert_eq!(view.len(), cells.len());
        for (slot, expected) in cells.iter().enumerate() {
            assert_eq!(view.cell(slot), Some(*expected));
        }
        assert_eq!(view.cell(cells.len()), None);
        assert_eq!(view.cell(usize::MAX), None);
    }
}

#[test]
fn every_truncation_trailing_byte_and_single_byte_damage_is_rejected() {
    for length in 0..PAGE_BYTES {
        assert!(decode(&CELLS[..length]).is_err(), "length {length}");
    }
    let mut trailing = CELLS.to_vec();
    trailing.push(0);
    assert!(decode(&trailing).is_err());
    for offset in 0..PAGE_BYTES {
        let mut damaged = *CELLS;
        damaged[offset] ^= 1;
        assert!(decode(&damaged).is_err(), "offset {offset}");
    }
}

#[test]
fn valid_checksum_does_not_hide_invalid_structure_or_zero_identities() {
    let mutations: &[(usize, &[u8])] = &[
        (0, b"NOPE"),
        (6, &[0]),
        (6, &[12]),
        (7, &[1]),
        (8, &[0; 16]),
        (24, &[0; 4]),
        (28, &[0; 4]),
        (40, &[0; 8]),
        (48, &[0; 8]),
        (56, &[0; 8]),
        (72, &[255, 255]),
        (74, &[0, 0]),
        (76, &[255, 255]),
        (78, &[1]),
        (127, &[1]),
        (128, &[0, 0]),       // Directory/header overlap.
        (130, &[0, 0]),       // Empty cell.
        (130, &[255, 255]),   // Bounds/length overflow.
        (132, &[0xdb, 0x3f]), // Overlaps previous cell.
        (136, &[0xd6, 0x3f]), // Gap.
        (140, &[1]),          // Noncanonical free-space byte.
        (76, &[0xd7, 0x3f]),  // upper does not equal last cell offset.
    ];
    for (offset, replacement) in mutations {
        let mut damaged = *CELLS;
        damaged[*offset..*offset + replacement.len()].copy_from_slice(replacement);
        resign(&mut damaged);
        assert!(
            matches!(decode(&damaged), Err(PageError::Corrupt)),
            "offset {offset}"
        );
    }
    let mut unknown_version = *CELLS;
    unknown_version[4] = 2;
    resign(&mut unknown_version);
    assert!(matches!(
        decode(&unknown_version),
        Err(PageError::UnsupportedFormat)
    ));
}

#[test]
fn expected_database_file_page_incarnation_owner_and_kind_are_required() {
    let base = header().identity;
    let mut identities = [base; 6];
    identities[0].database = DatabaseId::new([0x22; 16]).unwrap();
    identities[1].page.file = FileId::new(0x0000_0002_0000_0002).unwrap();
    identities[2].page.page_no += 1;
    identities[3].page.incarnation = Generation::new(5).unwrap();
    identities[4].owner = ObjectId::new(6).unwrap();
    identities[5].owner_generation = Generation::new(7).unwrap();
    for expected in identities {
        assert!(matches!(
            decode_page(CELLS, expected, PageKind::RowLeaf),
            Err(PageError::IdentityMismatch)
        ));
    }
    assert!(matches!(
        decode_page(CELLS, base, PageKind::IndexLeaf),
        Err(PageError::KindMismatch)
    ));
    assert!(FileId::new(0).is_err());
    assert!(FileId::new(1).is_err());
    assert!(FileId::new(1 << 32).is_err());
    assert!(FileId::new(u64::MAX).is_ok());
}

#[test]
fn every_kind_and_internal_level_boundary_is_checked() {
    for byte in 1..=11 {
        let kind = PageKind::from_byte(byte).unwrap();
        let internal = matches!(byte, 2 | 4 | 6);
        for level in 0..=16 {
            let mut page_header = header();
            page_header.kind = kind;
            page_header.level = level;
            let mut output = [0xa5; PAGE_BYTES];
            let accepted = if internal {
                (1..16).contains(&level)
            } else {
                level == 0
            };
            assert_eq!(encode_page(page_header, &[], &mut output).is_ok(), accepted);
            if accepted {
                let page = decode_page(&output, page_header.identity, kind).unwrap();
                assert_eq!(page.header(), page_header);
            } else {
                assert_eq!(output, [0xa5; PAGE_BYTES]);
                let mut bad_header = *EMPTY;
                bad_header[6] = byte;
                bad_header[7] = level;
                resign(&mut bad_header);
                assert!(matches!(
                    decode_page(&bad_header, page_header.identity, kind),
                    Err(PageError::Corrupt)
                ));
            }
        }
    }
}

#[test]
fn exact_page_capacity_and_all_encode_errors_preserve_output() {
    let mut output = [0xa5; PAGE_BYTES];
    let payload = vec![0x56; DIGEST_OFFSET - HEADER_BYTES - 4];
    encode_page(header(), &[&payload], &mut output).unwrap();
    assert_eq!(decode(&output).unwrap().cell(0), Some(payload.as_slice()));

    for cells in [
        vec![vec![0; payload.len() + 1]],
        vec![vec![]],
        vec![vec![1]; (DIGEST_OFFSET - HEADER_BYTES) / 5 + 1],
    ] {
        let borrowed: Vec<_> = cells.iter().map(Vec::as_slice).collect();
        output.fill(0xa5);
        assert!(encode_page(header(), &borrowed, &mut output).is_err());
        assert_eq!(output, [0xa5; PAGE_BYTES]);
    }
    // Maximum number of nonempty slots and short destination boundaries.
    let cells = vec![&[1][..]; (DIGEST_OFFSET - HEADER_BYTES) / 5];
    encode_page(header(), &cells, &mut output).unwrap();
    assert_eq!(decode(&output).unwrap().len(), cells.len());
    for length in [0, 1, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let mut short = vec![0xa5; length];
        assert_eq!(
            encode_page(header(), &[], &mut short),
            Err(PageError::InvalidInput)
        );
        assert!(short.iter().all(|b| *b == 0xa5));
    }
}

#[test]
fn fixed_64_seed_128_operation_envelope_campaign_preserves_cells() {
    // Frozen V2 profile: seeds 1..=64, LCG parameters below, 128 operations each.
    // The input cells, not decoded/encoded production output, are the oracle.
    // Every fourth operation exercises reserved-byte structural rejection.
    for seed in 1_u64..=64 {
        let mut state = seed;
        for operation in 0..128 {
            let mut next = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 32) as usize
            };
            let count = next() % 12;
            let cells: Vec<Vec<u8>> = (0..count)
                .map(|_| {
                    let length = 1 + next() % 300;
                    (0..length).map(|_| next() as u8).collect()
                })
                .collect();
            let borrowed: Vec<_> = cells.iter().map(Vec::as_slice).collect();
            let mut output = [0; PAGE_BYTES];
            encode_page(header(), &borrowed, &mut output).unwrap();
            let view = decode(&output).unwrap();
            assert_eq!(view.len(), count);
            for (slot, expected) in cells.iter().enumerate() {
                assert_eq!(view.cell(slot), Some(expected.as_slice()));
            }
            if operation % 4 == 0 {
                output[78] = 1;
                resign(&mut output);
                assert!(matches!(decode(&output), Err(PageError::Corrupt)));
            }
        }
    }
}
