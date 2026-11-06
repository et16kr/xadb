use super::*;
use crate::{
    api::DatabaseId,
    storage::page::{DIGEST_OFFSET, FileId, PAGE_BYTES, PageLsn},
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

fn header(page_no: u64, kind: PageKind, level: u8) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: PageRef {
                file: FileId::new(0x0000_0001_0000_0002).unwrap(),
                page_no,
                incarnation: Generation::new(4).unwrap(),
            },
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(6).unwrap(),
        },
        kind,
        level,
        lsn: PageLsn(7),
    }
}

fn root() -> AnchorRoot {
    AnchorRoot {
        tree: TreeKind::Row,
        page: header(3, PageKind::RowLeaf, 0).identity.page,
        level: 0,
        revision: 7,
    }
}

fn hex(input: &str) -> Vec<u8> {
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn anchor_cell_matches_independent_literal_and_validates_every_tree_level() {
    let anchor_header = header(8, PageKind::TreeAnchor, 0);
    let mut output = [0; PAGE_BYTES];
    encode_anchor(anchor_header, root(), &mut output).unwrap();
    let expected = hex(
        "585441310100010002000000010000000300000000000000040000000000000007000000000000000000000000000000",
    );
    let page = decode_page(&output, anchor_header.identity, PageKind::TreeAnchor).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page.cell(0).unwrap(), expected);
    for (tree, leaf, internal) in [
        (TreeKind::Row, PageKind::RowLeaf, PageKind::RowInternal),
        (
            TreeKind::Index,
            PageKind::IndexLeaf,
            PageKind::IndexInternal,
        ),
        (
            TreeKind::Catalog,
            PageKind::CatalogLeaf,
            PageKind::CatalogInternal,
        ),
    ] {
        for level in [0, 1, 15] {
            let root = AnchorRoot {
                tree,
                level,
                ..root()
            };
            encode_anchor(anchor_header, root, &mut output).unwrap();
            let anchor = decode_anchor(&output, anchor_header.identity).unwrap();
            assert_eq!(anchor.root(), root);
            let kind = if level == 0 { leaf } else { internal };
            let mut root_bytes = [0; PAGE_BYTES];
            encode_page(header(3, kind, level), &[], &mut root_bytes).unwrap();
            assert!(anchor.decode_root(&root_bytes).is_ok());
        }
    }
}

#[test]
fn anchor_rejects_checksummed_semantic_damage_and_wrong_cell_count() {
    let anchor_header = header(8, PageKind::TreeAnchor, 0);
    let mut bytes = [0; PAGE_BYTES];
    encode_anchor(anchor_header, root(), &mut bytes).unwrap();
    for size in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let mut input = bytes.to_vec();
        input.resize(size, 0);
        assert!(decode_anchor(&input, anchor_header.identity).is_err());
    }
    let cell_start = DIGEST_OFFSET - ANCHOR_CELL_BYTES;
    for (offset, value) in [(0, b'Z'), (4, 2), (6, 2), (7, 16), (40, 1), (47, 255)] {
        let mut damaged = bytes;
        damaged[cell_start + offset] = value;
        let checksum = content_digest(&damaged[..DIGEST_OFFSET]);
        damaged[DIGEST_OFFSET..].copy_from_slice(&checksum);
        assert!(decode_anchor(&damaged, anchor_header.identity).is_err());
    }
    let cell = bytes[cell_start..DIGEST_OFFSET].to_vec();
    for cells in [vec![], vec![&cell[..], &cell[..]], vec![&cell[..47]]] {
        let mut damaged = [0; PAGE_BYTES];
        encode_page(anchor_header, &cells, &mut damaged).unwrap();
        assert!(decode_anchor(&damaged, anchor_header.identity).is_err());
    }
    for range in [8..12, 12..16, 24..32, 32..40] {
        let mut damaged = cell.clone();
        damaged[range].fill(0);
        let mut page = [0; PAGE_BYTES];
        encode_page(anchor_header, &[&damaged], &mut page).unwrap();
        assert!(decode_anchor(&page, anchor_header.identity).is_err());
    }
    let mut self_root = cell;
    self_root[16..24].copy_from_slice(&8_u64.to_le_bytes());
    encode_page(anchor_header, &[&self_root], &mut bytes).unwrap();
    assert!(decode_anchor(&bytes, anchor_header.identity).is_err());
}

#[test]
fn every_encode_validation_error_preserves_anchor_output() {
    let anchor_header = header(8, PageKind::TreeAnchor, 0);
    let mut bytes = [0xa5; PAGE_BYTES];
    for root in [
        AnchorRoot {
            level: 16,
            ..root()
        },
        AnchorRoot {
            revision: 0,
            ..root()
        },
        AnchorRoot {
            page: anchor_header.identity.page,
            ..root()
        },
        AnchorRoot {
            page: PageRef {
                incarnation: Generation::new(9).unwrap(),
                ..anchor_header.identity.page
            },
            ..root()
        },
    ] {
        assert_eq!(
            encode_anchor(anchor_header, root, &mut bytes),
            Err(PageError::InvalidInput)
        );
        assert_eq!(bytes, [0xa5; PAGE_BYTES]);
    }
    for bad_header in [
        PageHeader {
            kind: PageKind::RowLeaf,
            ..anchor_header
        },
        PageHeader {
            level: 1,
            ..anchor_header
        },
    ] {
        assert!(encode_anchor(bad_header, root(), &mut bytes).is_err());
        assert_eq!(bytes, [0xa5; PAGE_BYTES]);
    }
    for length in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
        let mut output = vec![0xa5; length];
        assert!(encode_anchor(anchor_header, root(), &mut output).is_err());
        assert_eq!(output, vec![0xa5; length]);
    }
}

#[test]
fn root_load_requires_database_owner_reference_kind_and_level() {
    let anchor_header = header(8, PageKind::TreeAnchor, 0);
    let mut bytes = [0; PAGE_BYTES];
    encode_anchor(anchor_header, AnchorRoot { level: 1, ..root() }, &mut bytes).unwrap();
    let anchor = decode_anchor(&bytes, anchor_header.identity).unwrap();
    let base = header(3, PageKind::RowInternal, 1);
    let identity = base.identity;
    let bad_identities = [
        PageIdentity {
            database: DatabaseId::new([0x22; 16]).unwrap(),
            ..identity
        },
        PageIdentity {
            owner: ObjectId::new(9).unwrap(),
            ..identity
        },
        PageIdentity {
            owner_generation: Generation::new(9).unwrap(),
            ..identity
        },
        PageIdentity {
            page: PageRef {
                file: FileId::new(0x100000003).unwrap(),
                ..identity.page
            },
            ..identity
        },
        PageIdentity {
            page: PageRef {
                page_no: 9,
                ..identity.page
            },
            ..identity
        },
        PageIdentity {
            page: PageRef {
                incarnation: Generation::new(9).unwrap(),
                ..identity.page
            },
            ..identity
        },
    ];
    for identity in bad_identities {
        encode_page(PageHeader { identity, ..base }, &[], &mut bytes).unwrap();
        assert_eq!(
            anchor.decode_root(&bytes).err(),
            Some(PageError::IdentityMismatch)
        );
    }
    encode_page(
        PageHeader {
            kind: PageKind::IndexInternal,
            ..base
        },
        &[],
        &mut bytes,
    )
    .unwrap();
    assert_eq!(
        anchor.decode_root(&bytes).err(),
        Some(PageError::KindMismatch)
    );
    encode_page(PageHeader { level: 2, ..base }, &[], &mut bytes).unwrap();
    assert_eq!(anchor.decode_root(&bytes).err(), Some(PageError::Corrupt));
}

// UNL-P01..P04 (dev3-tree-smo.md#empty-leaf-unlink): XTR1 retired list.
// Expected cells are independent byte literals; the page envelope encoder
// only places them as cell 1.
fn first(page_no: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0001).unwrap(),
        page_no,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}
fn first_header(page_no: u64, lsn: u64) -> PageHeader {
    let mut h = header(page_no, PageKind::TreeAnchor, 0);
    h.identity.page = first(page_no, 4);
    h.lsn = PageLsn(lsn);
    h
}
fn row_root(page_no: u64, level: u8, revision: u64) -> AnchorRoot {
    AnchorRoot {
        tree: TreeKind::Row,
        page: first(page_no, 4),
        level,
        revision,
    }
}
/// (page, incarnation, kind byte, level, retire LSN)
fn literal(entries: &[(u64, u64, u8, u8, u64)]) -> Vec<u8> {
    let mut c = b"XTR1\x01\x00".to_vec();
    c.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    c.extend_from_slice(&[0; 8]);
    for (page_no, incarnation, kind, level, lsn) in entries {
        c.extend_from_slice(&0x0000_0001_0000_0001u64.to_le_bytes());
        c.extend_from_slice(&page_no.to_le_bytes());
        c.extend_from_slice(&incarnation.to_le_bytes());
        c.push(*kind);
        c.push(*level);
        c.extend_from_slice(&[0; 6]);
        c.extend_from_slice(&lsn.to_le_bytes());
    }
    c
}
fn listed_page(h: PageHeader, root: AnchorRoot, list: &[u8]) -> [u8; PAGE_BYTES] {
    let mut plain = [0; PAGE_BYTES];
    encode_anchor(h, root, &mut plain).unwrap();
    let cell0 = decode_page(&plain, h.identity, PageKind::TreeAnchor)
        .unwrap()
        .cell(0)
        .unwrap()
        .to_vec();
    let mut bytes = [0; PAGE_BYTES];
    encode_page(h, &[&cell0, list], &mut bytes).unwrap();
    bytes
}
fn items(list: RetiredList<'_>) -> Vec<RetiredPage> {
    (0..list.len()).map(|i| list.get(i).unwrap()).collect()
}

/// UNL-P01: n = 1, 2 and 55 match independent literals in both directions.
#[test]
fn unl_p01_retired_list_matches_independent_literal_for_1_2_and_55() {
    let h = first_header(9, 90);
    let root = row_root(10, 1, 7);
    let one: Vec<_> = vec![(20, 2, 1, 0, 90)];
    let two: Vec<_> = vec![(11, 3, 2, 1, 80), (20, 2, 1, 0, 90)];
    // 55 entries need every first-extent ordinal 9..63, so this vector
    // places the anchor and root outside them (first file, page 3/4).
    let full: Vec<_> = (9..64).map(|n| (n, 1, 1, 0, 90 - (n % 2))).collect();
    let full_h = first_header(3, 90);
    let full_root = row_root(4, 1, 7);
    for (h, root, entries) in [(h, root, one), (h, root, two), (full_h, full_root, full)] {
        let list = literal(&entries);
        let typed: Vec<_> = entries
            .iter()
            .map(|(n, inc, kind, level, lsn)| RetiredPage {
                page: first(*n, *inc),
                kind: PageKind::from_byte(*kind).unwrap(),
                level: *level,
                retire_lsn: *lsn,
            })
            .collect();
        let mut out = [0; PAGE_BYTES];
        encode_listed_anchor(h, root, &list, &mut out).unwrap();
        assert_eq!(out, listed_page(h, root, &list));
        let decoded = retired(&out, h.identity).unwrap();
        assert_eq!(decoded.cell(), list.as_slice());
        assert_eq!(items(decoded), typed);
        assert_eq!(decode_anchor(&out, h.identity).unwrap().root(), root);
        // `extend` from empty merges an ordered typed list into the same cell.
        let mut cell = [0; RETIRED_CELL_BYTES];
        let used = RetiredList::EMPTY.extend(&typed, &mut cell).unwrap();
        assert_eq!(&cell[..used], list.as_slice());
    }
    // Merging into an existing list keeps physical order.
    let base = listed_page(h, root, &literal(&[(11, 3, 2, 1, 80), (30, 2, 1, 0, 85)]));
    let mut cell = [0; RETIRED_CELL_BYTES];
    let used = retired(&base, h.identity)
        .unwrap()
        .extend(
            &[RetiredPage {
                page: first(20, 2),
                kind: PageKind::RowLeaf,
                level: 0,
                retire_lsn: 90,
            }],
            &mut cell,
        )
        .unwrap();
    assert_eq!(
        &cell[..used],
        literal(&[(11, 3, 2, 1, 80), (20, 2, 1, 0, 90), (30, 2, 1, 0, 85)]).as_slice()
    );
}

/// UNL-P02: n = 0 and n = 56 are not lists; length must match n.
#[test]
fn unl_p02_retired_list_count_and_length_bounds() {
    let h = first_header(3, 90);
    let root = row_root(4, 1, 7);
    let mut zero = literal(&[]);
    assert_eq!(zero.len(), 16);
    let mut over = literal(&(9..64).map(|n| (n, 1, 1, 0, 90)).collect::<Vec<_>>());
    over.extend_from_slice(&literal(&[(5, 1, 1, 0, 90)])[16..]);
    over[6..8].copy_from_slice(&56u16.to_le_bytes());
    let mut short = literal(&[(20, 2, 1, 0, 90), (21, 2, 1, 0, 90)]);
    short[6..8].copy_from_slice(&3u16.to_le_bytes());
    for list in [&mut zero, &mut over, &mut short] {
        let page = listed_page(h, root, list);
        assert_eq!(
            decode_anchor(&page, h.identity).err(),
            Some(PageError::Corrupt)
        );
        let mut out = [0xa5; PAGE_BYTES];
        assert_eq!(
            encode_listed_anchor(h, root, list, &mut out),
            Err(PageError::InvalidInput)
        );
        assert_eq!(out, [0xa5; PAGE_BYTES]);
    }
    let mut cell = [0; RETIRED_CELL_BYTES];
    assert!(RetiredList::EMPTY.extend(&[], &mut cell).is_err());
    let full = listed_page(
        h,
        root,
        &literal(&(9..64).map(|n| (n, 1, 1, 0, 90)).collect::<Vec<_>>()),
    );
    let one = RetiredPage {
        page: first(5, 1),
        kind: PageKind::RowLeaf,
        level: 0,
        retire_lsn: 90,
    };
    assert!(
        retired(&full, h.identity)
            .unwrap()
            .extend(&[one], &mut cell)
            .is_err()
    );
}

/// UNL-P03: order, alias, file/page range, kind/level, LSN, reserved bytes
/// and version damage are refused by the decoder and the encoder.
#[test]
fn unl_p03_retired_list_rejects_order_alias_range_kind_and_lsn() {
    let h = first_header(9, 90);
    let root = row_root(10, 1, 7);
    let ok = (20, 2, 1, 0, 90);
    let lists: Vec<Vec<u8>> = vec![
        literal(&[(21, 2, 1, 0, 90), (20, 2, 1, 0, 90)]),
        literal(&[(20, 2, 1, 0, 90), (20, 3, 1, 0, 90)]),
        literal(&[(9, 2, 1, 0, 90)]),
        literal(&[(10, 3, 1, 0, 90)]),
        literal(&[(8, 2, 1, 0, 90)]),
        literal(&[(64, 2, 1, 0, 90)]),
        literal(&[(20, 2, 5, 0, 90)]),
        literal(&[(20, 2, 1, 1, 90)]),
        literal(&[(20, 2, 2, 0, 90)]),
        literal(&[(20, 2, 2, 16, 90)]),
        literal(&[(20, 2, 1, 0, 0)]),
        literal(&[(20, 2, 1, 0, 91)]),
        {
            let mut c = literal(&[ok]);
            c[8] = 1;
            c
        },
        {
            let mut c = literal(&[ok]);
            c[16 + 26] = 1;
            c
        },
        {
            let mut c = literal(&[ok]);
            c[16..24].copy_from_slice(&0x0000_0001_0000_0002u64.to_le_bytes());
            c
        },
        {
            let mut c = literal(&[ok]);
            c[0] = b'Z';
            c
        },
    ];
    for (i, list) in lists.iter().enumerate() {
        let page = listed_page(h, root, list);
        assert_eq!(
            decode_anchor(&page, h.identity).err(),
            Some(PageError::Corrupt),
            "list {i}"
        );
        assert!(retired(&page, h.identity).is_err());
        let mut out = [0xa5; PAGE_BYTES];
        assert_eq!(
            encode_listed_anchor(h, root, list, &mut out),
            Err(PageError::InvalidInput),
            "encode {i}"
        );
        assert_eq!(out, [0xa5; PAGE_BYTES]);
    }
    let mut version = literal(&[ok]);
    version[4] = 2;
    assert_eq!(
        decode_anchor(&listed_page(h, root, &version), h.identity).err(),
        Some(PageError::UnsupportedFormat)
    );
}

/// UNL-P04: only a Row anchor of revision 2 or later carries a list, a
/// third cell is refused, and the one-cell anchor bytes are unchanged.
#[test]
fn unl_p04_list_is_row_only_and_one_cell_anchor_is_unchanged() {
    let h = first_header(9, 90);
    let list = literal(&[(20, 2, 1, 0, 90)]);
    for root in [
        AnchorRoot {
            tree: TreeKind::Index,
            ..row_root(10, 1, 7)
        },
        AnchorRoot {
            tree: TreeKind::Catalog,
            ..row_root(10, 0, 7)
        },
        row_root(10, 1, 1),
    ] {
        let page = listed_page(h, root, &list);
        assert_eq!(
            decode_anchor(&page, h.identity).err(),
            Some(PageError::Corrupt)
        );
        let mut out = [0xa5; PAGE_BYTES];
        assert_eq!(
            encode_listed_anchor(h, root, &list, &mut out),
            Err(PageError::InvalidInput)
        );
    }
    let root = row_root(10, 1, 7);
    let page = listed_page(h, root, &list);
    let cell0 = decode_page(&page, h.identity, PageKind::TreeAnchor)
        .unwrap()
        .cell(0)
        .unwrap()
        .to_vec();
    let mut three = [0; PAGE_BYTES];
    encode_page(h, &[&cell0, &list, &list], &mut three).unwrap();
    assert_eq!(
        decode_anchor(&three, h.identity).err(),
        Some(PageError::Corrupt)
    );
    let mut plain = [0; PAGE_BYTES];
    encode_anchor(h, root, &mut plain).unwrap();
    let mut empty = [0; PAGE_BYTES];
    encode_listed_anchor(h, root, &[], &mut empty).unwrap();
    assert_eq!(plain, empty);
    assert_eq!(
        decode_page(&plain, h.identity, PageKind::TreeAnchor)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(retired(&plain, h.identity).unwrap(), RetiredList::EMPTY);
}

/// UNL-C01 (dev3-tree-smo.md#empty-leaf-unlink): the XTR1 list's source
/// cost per decoder path. A root-only `decode_anchor` adds the
/// `check_list` Result to its frame; `check_list` and its walk run after
/// `decode_page` has returned and stay below every page term the existing
/// censuses pass (the page codec and the tree codec), so the increment
/// over those terms is that Result alone. `retired` and
/// `encode_listed_anchor` are whole new calls, larger than the page term.
#[test]
fn unl_c01_xtr1_codec_cost_per_decoder_path() {
    use crate::storage::{page::PageError, semantic_catalog, semantic_row::transition};
    use std::mem::size_of;
    let result = 2 * size_of::<Result<(), PageError>>();
    let check = super::list_check_bytes();
    let walk = super::walk_source_bytes();
    assert!(check > walk);
    for (name, page) in [
        ("page_read_codec", semantic_catalog::page_read_codec_bytes()),
        ("tree_codec", transition::tree_source_bytes()),
    ] {
        assert!(
            check <= page,
            "{name}: check_list {check} > page term {page}"
        );
        assert_eq!(super::decode_source_bytes(page), page + result);
        assert_eq!(super::decode_increment_bytes(page), result);
        let retired = super::retired_source_bytes(page);
        assert!(retired > super::decode_source_bytes(page) + super::decoder_frame_bytes());
        let listed = super::listed_encode_source_bytes(page);
        assert!(listed > page);
        println!(
            "UNL_C01 page_term={name} page={page} decode={} increment={} check_list={check} \
             walk={walk} retired={retired} listed_encode={listed} extend={}",
            super::decode_source_bytes(page),
            super::decode_increment_bytes(page),
            super::extend_source_bytes()
        );
    }
    // A page term below the walk pays the walk instead of the page.
    assert_eq!(super::decode_source_bytes(0), result + check);
}
