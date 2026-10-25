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
