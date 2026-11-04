use super::*;
use crate::{
    api::DatabaseId,
    storage::page::{FileId, PAGE_BYTES},
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

fn reference(number: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0002).unwrap(),
        page_no: number,
        incarnation: Generation::new(4).unwrap(),
    }
}
fn expectation(kind: PageKind, arity: u8, level: u8) -> TreeExpectation {
    TreeExpectation {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: reference(3),
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(6).unwrap(),
        },
        kind,
        arity,
        level,
    }
}
fn header(expected: TreeExpectation) -> PageHeader {
    PageHeader {
        identity: expected.identity,
        kind: expected.kind,
        level: expected.level,
        lsn: PageLsn(7),
    }
}
fn key(arity: u8, number: u64) -> Vec<u8> {
    let mut key = if arity == 0 { vec![] } else { vec![0] };
    key.extend(number.to_be_bytes());
    key
}
fn raw_reference(reference: PageRef) -> Vec<u8> {
    let mut bytes = 0x1_0000_0002_u64.to_le_bytes().to_vec();
    bytes.extend(reference.page_no.to_le_bytes());
    bytes.extend(reference.incarnation.get().to_le_bytes());
    bytes
}
fn internal_cell(arity: u8, number: u64, child: u64) -> Vec<u8> {
    let key = key(arity, number);
    let mut cell = (key.len() as u16).to_le_bytes().to_vec();
    cell.extend([0, 0]);
    cell.extend(key);
    cell.extend(raw_reference(reference(child)));
    cell
}
fn leaf_cell(arity: u8, number: u64, value_bytes: usize) -> Vec<u8> {
    let key = key(arity, number);
    let mut cell = (key.len() as u16).to_le_bytes().to_vec();
    cell.extend((value_bytes as u16).to_le_bytes());
    cell.extend(key);
    cell.extend(std::iter::repeat_n(number as u8, value_bytes));
    cell
}
fn metadata(arity: u8, left: Option<u64>) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[..6].copy_from_slice(b"XBT1\x01\0");
    bytes[6] = arity;
    if let Some(left) = left {
        bytes[8..].copy_from_slice(&raw_reference(reference(left)));
    }
    bytes
}
// Independent envelope assembly. Product encoders are neither source builders
// nor expected-byte generators. Keys/child order below are explicit fixtures.
fn raw_page(expected: TreeExpectation, cells: &[Vec<u8>], lsn: u64) -> Vec<u8> {
    let mut bytes = vec![0; PAGE_BYTES];
    bytes[..6].copy_from_slice(b"XDP1\x01\0");
    bytes[6] = expected.kind as u8;
    bytes[7] = expected.level;
    bytes[8..24].fill(0x11);
    for (offset, value) in [
        (24, 0x1_0000_0002_u64),
        (32, expected.identity.page.page_no),
        (40, expected.identity.page.incarnation.get()),
        (48, 5),
        (56, 6),
        (64, lsn),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes[72..74].copy_from_slice(&(cells.len() as u16).to_le_bytes());
    bytes[74..76].copy_from_slice(&((128 + 4 * cells.len()) as u16).to_le_bytes());
    let mut end = DIGEST_OFFSET;
    for (slot, cell) in cells.iter().enumerate() {
        let start = end - cell.len();
        bytes[128 + 4 * slot..130 + 4 * slot].copy_from_slice(&(start as u16).to_le_bytes());
        bytes[130 + 4 * slot..132 + 4 * slot].copy_from_slice(&(cell.len() as u16).to_le_bytes());
        bytes[start..end].copy_from_slice(cell);
        end = start;
    }
    assert!(end >= 128 + 4 * cells.len());
    bytes[76..78].copy_from_slice(&(end as u16).to_le_bytes());
    let digest = content_digest(&bytes[..DIGEST_OFFSET]);
    bytes[DIGEST_OFFSET..].copy_from_slice(&digest);
    bytes
}
const INTERNALS: [(PageKind, u8); 3] = [
    (PageKind::RowInternal, 0),
    (PageKind::CatalogInternal, 0),
    (PageKind::IndexInternal, 1),
];

#[test]
fn internal_insert_and_unlink_match_independent_pages() {
    for (kind, arity) in INTERNALS {
        let expected = expectation(kind, arity, 1);
        let source_cells = vec![
            metadata(arity, Some(10)),
            internal_cell(arity, 20, 11),
            internal_cell(arity, 40, 12),
            internal_cell(arity, 60, 13),
        ];
        let source = raw_page(expected, &source_cells, 7);
        let view = decode_tree_page(&source, expected).unwrap();
        for (number, left, at) in [(10, 10, 1), (30, 11, 2), (50, 12, 3), (70, 13, 4)] {
            let inserted = internal_cell(arity, number, 99);
            let mut meta = [0xa5; 32];
            let mut refs = [&b"sentinel"[..]; 6];
            let InternalEditPlan::Page(plan) = view
                .prepare_internal_edit(
                    InternalEdit::Insert {
                        left: reference(left),
                        cell: &inserted,
                    },
                    &mut meta,
                    &mut refs,
                )
                .unwrap()
            else {
                panic!("insert collapsed")
            };
            assert!(!plan.needs_split());
            let mut output = vec![0xa5; PAGE_BYTES];
            plan.encode(PageLsn(11), &mut output).unwrap();
            let mut expected_cells = source_cells.clone();
            expected_cells.insert(at, inserted.clone());
            assert_eq!(output, raw_page(expected, &expected_cells, 11));
        }
        for (child, remove, new_left) in [(10, 1, 11), (11, 1, 10), (12, 2, 10), (13, 3, 10)] {
            let mut meta = [0xa5; 32];
            let mut refs = [&b"sentinel"[..]; 5];
            let InternalEditPlan::Page(plan) = view
                .prepare_internal_edit(
                    InternalEdit::RemoveChild(reference(child)),
                    &mut meta,
                    &mut refs,
                )
                .unwrap()
            else {
                panic!("three children remain")
            };
            let mut output = vec![0xa5; PAGE_BYTES];
            plan.encode(PageLsn(12), &mut output).unwrap();
            let mut expected_cells = source_cells.clone();
            expected_cells.remove(remove);
            expected_cells[0] = metadata(arity, Some(new_left));
            assert_eq!(output, raw_page(expected, &expected_cells, 12));
        }
    }
}

#[test]
fn internal_collapse_preserves_scratch_and_selects_survivor() {
    for (kind, arity) in INTERNALS {
        for level in [1, 15] {
            let expected = expectation(kind, arity, level);
            let source = raw_page(
                expected,
                &[metadata(arity, Some(10)), internal_cell(arity, 20, 11)],
                7,
            );
            let view = decode_tree_page(&source, expected).unwrap();
            for (removed, survivor) in [(10, 11), (11, 10)] {
                let mut meta = [0xa5; 32];
                let mut refs = [];
                let InternalEditPlan::Collapse { survivor: actual } = view
                    .prepare_internal_edit(
                        InternalEdit::RemoveChild(reference(removed)),
                        &mut meta,
                        &mut refs,
                    )
                    .unwrap()
                else {
                    panic!("unary internal must be spliced")
                };
                assert_eq!(actual, reference(survivor));
                assert_eq!(meta, [0xa5; 32]);
            }
        }
    }
}

#[test]
fn internal_rejections_preserve_both_scratch_buffers() {
    let expected = expectation(PageKind::RowInternal, 0, 1);
    let source = raw_page(
        expected,
        &[
            metadata(0, Some(10)),
            internal_cell(0, 20, 11),
            internal_cell(0, 40, 12),
        ],
        7,
    );
    let view = decode_tree_page(&source, expected).unwrap();
    let duplicate = internal_cell(0, 20, 99);
    let alias = internal_cell(0, 30, 10);
    let self_alias = internal_cell(0, 30, 3);
    let good = internal_cell(0, 30, 99);
    let mut reincarnated = reference(11);
    reincarnated.incarnation = Generation::new(5).unwrap();
    let cases = [
        (
            InternalEdit::Insert {
                left: reference(11),
                cell: &duplicate,
            },
            StructureError::SeparatorExists,
        ),
        (
            InternalEdit::Insert {
                left: reference(10),
                cell: &good,
            },
            StructureError::ChildChanged,
        ),
        (
            InternalEdit::Insert {
                left: reincarnated,
                cell: &good,
            },
            StructureError::ChildChanged,
        ),
        (
            InternalEdit::Insert {
                left: reference(11),
                cell: &alias,
            },
            StructureError::Tree(TreeError::Corrupt),
        ),
        (
            InternalEdit::Insert {
                left: reference(11),
                cell: &self_alias,
            },
            StructureError::Tree(TreeError::Corrupt),
        ),
        (
            InternalEdit::RemoveChild(reference(99)),
            StructureError::ChildChanged,
        ),
        (
            InternalEdit::RemoveChild(reincarnated),
            StructureError::ChildChanged,
        ),
    ];
    for (edit, error) in cases {
        let mut meta = [0xa5; 32];
        let mut refs = [&b"sentinel"[..]; 5];
        assert!(
            matches!(view.prepare_internal_edit(edit,&mut meta,&mut refs),Err(actual) if actual==error)
        );
        assert_eq!(refs, [&b"sentinel"[..]; 5]);
        assert_eq!(meta, [0xa5; 32]);
    }
    let mut meta = [0xa5; 32];
    let mut refs = [&b"sentinel"[..]; 3];
    assert!(matches!(
        view.prepare_internal_edit(
            InternalEdit::Insert {
                left: reference(11),
                cell: &good
            },
            &mut meta,
            &mut refs
        ),
        Err(StructureError::ScratchTooSmall)
    ));
    assert_eq!(refs, [&b"sentinel"[..]; 3]);
    assert_eq!(meta, [0xa5; 32]);
}

fn assert_split(expected: TreeExpectation, cells: &[Vec<u8>], boundary: usize) {
    let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
    let split =
        SplitCandidate::prepare(header(expected), expected.arity, &refs, reference(90)).unwrap();
    let leaf = expected.level == 0;
    // Fixture specifies the boundary independently of the product splitter.
    assert_eq!(
        split.separator(),
        &cells[boundary + 1][4..4 + usize::from(u16::from_le_bytes([
            cells[boundary + 1][0],
            cells[boundary + 1][1]
        ]))]
    );
    for right in [false, true] {
        let mut meta = [0xa5; 32];
        let mut scratch = vec![&b"sentinel"[..]; cells.len()];
        let mut output = vec![0xa5; PAGE_BYTES];
        split
            .encode_side(right, PageLsn(19), &mut meta, &mut scratch, &mut output)
            .unwrap();
        let mut expected_side = expected;
        if right {
            expected_side.identity.page = reference(90);
        }
        let mut expected_cells = vec![if right && !leaf {
            let mut meta = metadata(expected.arity, None);
            meta[8..].copy_from_slice(&cells[boundary + 1][cells[boundary + 1].len() - 24..]);
            meta
        } else {
            cells[0].clone()
        }];
        expected_cells.extend_from_slice(if right {
            &cells[boundary + 1 + usize::from(!leaf)..]
        } else {
            &cells[1..boundary + 1]
        });
        assert_eq!(output, raw_page(expected_side, &expected_cells, 19));
        decode_tree_page(&output, expected_side).unwrap();
    }
    let mut meta = [0; 32];
    let mut cell = [0; 1064];
    let mut output = vec![0xa5; PAGE_BYTES];
    if expected.level == 15 {
        assert_eq!(
            split.encode_new_root(
                reference(91),
                PageLsn(19),
                &mut meta,
                &mut cell,
                &mut output
            ),
            Err(TreeError::InvalidInput)
        );
        assert_eq!(output, vec![0xa5; PAGE_BYTES]);
    } else {
        split
            .encode_new_root(
                reference(91),
                PageLsn(19),
                &mut meta,
                &mut cell,
                &mut output,
            )
            .unwrap();
        let root_kind = match expected.kind {
            PageKind::RowLeaf | PageKind::RowInternal => PageKind::RowInternal,
            PageKind::CatalogLeaf | PageKind::CatalogInternal => PageKind::CatalogInternal,
            _ => PageKind::IndexInternal,
        };
        let mut root = expectation(root_kind, expected.arity, expected.level + 1);
        root.identity.page = reference(91);
        let separator = split.separator();
        let mut root_cell = (separator.len() as u16).to_le_bytes().to_vec();
        root_cell.extend([0, 0]);
        root_cell.extend(separator);
        root_cell.extend(raw_reference(reference(90)));
        assert_eq!(
            output,
            raw_page(root, &[metadata(expected.arity, Some(3)), root_cell], 19)
        );
    }
}

#[test]
fn split_images_preserve_entries_and_promote_internal_pivot() {
    for (internal, arity) in INTERNALS {
        for level in [1, 14, 15] {
            let cells = vec![
                metadata(arity, Some(10)),
                internal_cell(arity, 20, 11),
                internal_cell(arity, 40, 12),
                internal_cell(arity, 60, 13),
            ];
            assert_split(expectation(internal, arity, level), &cells, 1);
        }
        let leaf = match internal {
            PageKind::RowInternal => PageKind::RowLeaf,
            PageKind::CatalogInternal => PageKind::CatalogLeaf,
            _ => PageKind::IndexLeaf,
        };
        // Costs are H,16(+one NULL tag),16(+one NULL tag): byte balance splits at 1.
        let cells = vec![
            metadata(arity, None),
            leaf_cell(arity, 10, 8078 - usize::from(arity)),
            leaf_cell(arity, 20, 0),
            leaf_cell(arity, 30, 0),
        ];
        assert_split(expectation(leaf, arity, 0), &cells, 1);
    }
}

#[test]
fn split_rejects_aliases_and_preserves_failed_output() {
    let expected = expectation(PageKind::RowInternal, 0, 1);
    let cells = [
        metadata(0, Some(10)),
        internal_cell(0, 20, 11),
        internal_cell(0, 40, 12),
        internal_cell(0, 60, 13),
    ];
    let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
    for number in [3, 10, 11, 12, 13] {
        for generation in [4, 5] {
            let mut right = reference(number);
            right.incarnation = Generation::new(generation).unwrap();
            assert!(matches!(
                SplitCandidate::prepare(header(expected), 0, &refs, right),
                Err(TreeError::InvalidInput)
            ));
        }
    }
    let split = SplitCandidate::prepare(header(expected), 0, &refs, reference(90)).unwrap();
    for number in [3, 90, 10, 11, 12, 13] {
        let mut meta = [0; 32];
        let mut cell = [0; 1064];
        let mut output = vec![0xa5; PAGE_BYTES];
        assert_eq!(
            split.encode_new_root(
                reference(number),
                PageLsn(19),
                &mut meta,
                &mut cell,
                &mut output
            ),
            Err(TreeError::InvalidInput)
        );
        assert_eq!(output, vec![0xa5; PAGE_BYTES]);
    }
    for right in [false, true] {
        let mut meta = [0; 32];
        let mut scratch = [];
        let mut output = vec![0xa5; PAGE_BYTES];
        assert_eq!(
            split.encode_side(right, PageLsn(19), &mut meta, &mut scratch, &mut output),
            Err(TreeError::NoSpace)
        );
        assert_eq!(output, vec![0xa5; PAGE_BYTES]);
        let mut scratch = [&[][..]; 4];
        let mut output = vec![0xa5; PAGE_BYTES - 1];
        assert!(
            split
                .encode_side(right, PageLsn(19), &mut meta, &mut scratch, &mut output)
                .is_err()
        );
        assert_eq!(output, vec![0xa5; PAGE_BYTES - 1]);
    }
}

#[test]
fn overflowing_parent_insert_splits_with_all_children_preserved() {
    for (kind, arity) in INTERNALS {
        let expected = expectation(kind, arity, 1);
        let cost = 40 + usize::from(arity);
        let count = TREE_ENTRY_CAPACITY / cost;
        let mut cells = vec![metadata(arity, Some(1000))];
        for i in 1..=count {
            cells.push(internal_cell(arity, (i * 2) as u64, 1000 + i as u64));
        }
        let source = raw_page(expected, &cells, 7);
        let view = decode_tree_page(&source, expected).unwrap();
        let added = internal_cell(arity, (count * 2 + 1) as u64, 2000);
        let mut meta = [0; 32];
        let mut scratch = vec![&[][..]; count + 2];
        let InternalEditPlan::Page(plan) = view
            .prepare_internal_edit(
                InternalEdit::Insert {
                    left: reference(1000 + count as u64),
                    cell: &added,
                },
                &mut meta,
                &mut scratch,
            )
            .unwrap()
        else {
            panic!("insert cannot collapse")
        };
        assert!(plan.needs_split());
        let mut output = vec![0xa5; PAGE_BYTES];
        assert_eq!(
            plan.encode(PageLsn(19), &mut output),
            Err(TreeError::NoSpace)
        );
        assert_eq!(output, vec![0xa5; PAGE_BYTES]);
        let split = plan.split(reference(90)).unwrap();
        let pivot = count.div_ceil(2);
        assert_eq!(split.separator(), key(arity, (pivot * 2 + 2) as u64));
        cells.push(added.clone());
        for right in [false, true] {
            let mut metadata_scratch = [0; 32];
            let mut references = vec![&[][..]; count + 2];
            let mut image = vec![0; PAGE_BYTES];
            split
                .encode_side(
                    right,
                    PageLsn(19),
                    &mut metadata_scratch,
                    &mut references,
                    &mut image,
                )
                .unwrap();
            let mut expected_side = expected;
            let mut expected_cells = vec![if right {
                metadata(arity, Some(1000 + pivot as u64 + 1))
            } else {
                cells[0].clone()
            }];
            expected_cells.extend_from_slice(if right {
                &cells[pivot + 2..]
            } else {
                &cells[1..pivot + 1]
            });
            if right {
                expected_side.identity.page = reference(90);
            }
            assert_eq!(image, raw_page(expected_side, &expected_cells, 19));
        }
    }
}

#[test]
fn leaf_edit_split_images_include_exact_pending_value() {
    use super::super::edit::LeafEdit;
    let expected = expectation(PageKind::RowLeaf, 0, 0);
    let cells = vec![
        metadata(0, None),
        leaf_cell(0, 10, 8078),
        leaf_cell(0, 30, 8078),
    ];
    let source = raw_page(expected, &cells, 7);
    let view = decode_tree_page(&source, expected).unwrap();
    let inserted = leaf_cell(0, 20, 1);
    let mut scratch = [&[][..]; 4];
    let plan = view
        .prepare_leaf_edit(LeafEdit::Insert(&inserted), &mut scratch)
        .unwrap();
    assert!(plan.needs_split());
    let split = plan.split_images(reference(90)).unwrap();
    assert_eq!(split.separator(), key(0, 20));
    let mut metadata = [0; 32];
    let mut refs = [&[][..]; 4];
    let mut output = vec![0; PAGE_BYTES];
    split
        .encode_side(true, PageLsn(19), &mut metadata, &mut refs, &mut output)
        .unwrap();
    let mut right = expected;
    right.identity.page = reference(90);
    assert_eq!(
        output,
        raw_page(right, &[cells[0].clone(), inserted, cells[2].clone()], 19)
    );
}

#[test]
fn internal_success_preserves_tail_and_truncated_cell_preserves_workspace() {
    for (kind, arity) in INTERNALS {
        let expected = expectation(kind, arity, 1);
        let source = raw_page(
            expected,
            &[
                metadata(arity, Some(10)),
                internal_cell(arity, 20, 11),
                internal_cell(arity, 40, 12),
            ],
            7,
        );
        let view = decode_tree_page(&source, expected).unwrap();
        let cell = internal_cell(arity, 30, 99);
        for edit in [
            InternalEdit::Insert {
                left: reference(11),
                cell: &cell,
            },
            InternalEdit::RemoveChild(reference(11)),
        ] {
            let count = if matches!(edit, InternalEdit::Insert { .. }) {
                4
            } else {
                2
            };
            let mut meta = [0xa5; 32];
            let mut scratch = [&b"tail"[..]; 8];
            assert!(matches!(
                view.prepare_internal_edit(edit, &mut meta, &mut scratch),
                Ok(InternalEditPlan::Page(_))
            ));
            assert!(scratch[count..].iter().all(|bytes| *bytes == b"tail"));
        }
        for length in 0..cell.len() {
            let mut meta = [0xa5; 32];
            let mut scratch = [&b"tail"[..]; 8];
            assert!(
                view.prepare_internal_edit(
                    InternalEdit::Insert {
                        left: reference(11),
                        cell: &cell[..length]
                    },
                    &mut meta,
                    &mut scratch
                )
                .is_err()
            );
            assert_eq!(scratch, [&b"tail"[..]; 8]);
            assert_eq!(meta, [0xa5; 32]);
        }
    }
}

#[test]
fn new_root_short_cell_and_output_preserve_output() {
    for (kind, arity) in INTERNALS {
        let expected = expectation(kind, arity, 1);
        let cells = [
            metadata(arity, Some(10)),
            internal_cell(arity, 20, 11),
            internal_cell(arity, 40, 12),
            internal_cell(arity, 60, 13),
        ];
        let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
        let split = SplitCandidate::prepare(header(expected), arity, &refs, reference(90)).unwrap();
        for length in 0..36 + usize::from(arity) {
            let mut meta = [0xa5; 32];
            let mut cell = vec![0xa5; length];
            let mut output = vec![0xa5; PAGE_BYTES];
            assert_eq!(
                split.encode_new_root(
                    reference(91),
                    PageLsn(19),
                    &mut meta,
                    &mut cell,
                    &mut output
                ),
                Err(TreeError::NoSpace)
            );
            assert_eq!(output, vec![0xa5; PAGE_BYTES]);
        }
        for length in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
            let mut meta = [0xa5; 32];
            let mut cell = [0xa5; 1064];
            let mut output = vec![0xa5; length];
            assert!(
                split
                    .encode_new_root(
                        reference(91),
                        PageLsn(19),
                        &mut meta,
                        &mut cell,
                        &mut output
                    )
                    .is_err()
            );
            assert_eq!(output, vec![0xa5; length]);
        }
    }
}

#[test]
fn weighted_split_preserves_rollback_space_without_installing_pending_edit() {
    for (kind, arity) in [
        (PageKind::RowLeaf, 0),
        (PageKind::CatalogLeaf, 0),
        (PageKind::IndexLeaf, 1),
    ] {
        let expected = expectation(kind, arity, 0);
        let cells = vec![
            metadata(arity, None),
            leaf_cell(arity, 10, 0),
            leaf_cell(arity, 20, 0),
            leaf_cell(arity, 30, 0),
        ];
        let source = raw_page(expected, &cells, 7);
        let view = decode_tree_page(&source, expected).unwrap();
        for (weights, request, boundary) in [
            ([8000, 1000, 1000], None, 1),
            ([1000, 1000, 1000], Some((30, 2000)), 2),
            ([1000, 1000, 1000], Some((10, 2000)), 1),
            ([8000, 1000, 1000], Some((10, 16 + usize::from(arity))), 1),
            ([8094, 4000, 4000], Some((5, 8094)), 1),
            ([8094, 4000, 4000], Some((40, 8094)), 2),
        ] {
            let pending_key = request.map(|(n, _)| key(arity, n));
            let pending = request.map(|(_, entry_cost)| LeafSpaceRequest {
                key: pending_key.as_deref().unwrap(),
                entry_cost,
            });
            let mut refs = [&b"tail"[..]; 6];
            let split = view
                .prepare_leaf_split(&weights, pending, reference(90), &mut refs)
                .unwrap();
            assert_eq!(split.separator(), key(arity, 10 * (boundary as u64 + 1)));
            for right in [false, true] {
                let mut meta = [0; 32];
                let mut scratch = [&[][..]; 4];
                let mut output = vec![0; PAGE_BYTES];
                split
                    .encode_side(right, PageLsn(19), &mut meta, &mut scratch, &mut output)
                    .unwrap();
                let mut side = expected;
                if right {
                    side.identity.page = reference(90);
                }
                let mut expected_cells = vec![cells[0].clone()];
                expected_cells.extend_from_slice(if right {
                    &cells[boundary + 1..]
                } else {
                    &cells[1..boundary + 1]
                });
                // The new key/grown value is NOT installed by the system split.
                assert_eq!(output, raw_page(side, &expected_cells, 19));
            }
            assert_eq!(&refs[4..], &[&b"tail"[..]; 2]);
        }
    }
}

#[test]
fn weighted_split_rejects_invalid_reservations_without_scratch_changes() {
    let expected = expectation(PageKind::RowLeaf, 0, 0);
    let cells = [
        metadata(0, None),
        leaf_cell(0, 10, 0),
        leaf_cell(0, 20, 0),
        leaf_cell(0, 30, 0),
    ];
    let source = raw_page(expected, &cells, 7);
    let view = decode_tree_page(&source, expected).unwrap();
    for weights in [
        &[16, 16][..],
        &[15, 16, 16],
        &[8095, 16, 16],
        &[8094, 8094, 16],
    ] {
        let mut refs = [&b"tail"[..]; 4];
        assert!(
            view.prepare_leaf_split(weights, None, reference(90), &mut refs)
                .is_err()
        );
        assert_eq!(refs, [&b"tail"[..]; 4]);
    }
    for (query, cost) in [
        (&b""[..], 16),
        (&10u64.to_be_bytes()[..], 0),
        (&10u64.to_be_bytes()[..], 8095),
    ] {
        let mut refs = [&b"tail"[..]; 4];
        assert!(
            view.prepare_leaf_split(
                &[16; 3],
                Some(LeafSpaceRequest {
                    key: query,
                    entry_cost: cost
                }),
                reference(90),
                &mut refs
            )
            .is_err()
        );
        assert_eq!(refs, [&b"tail"[..]; 4]);
    }
    let mut refs = [&b"tail"[..]; 3];
    assert!(matches!(
        view.prepare_leaf_split(&[16; 3], None, reference(90), &mut refs),
        Err(StructureError::ScratchTooSmall)
    ));
    assert_eq!(refs, [&b"tail"[..]; 3]);
    for incarnation in [4, 5] {
        let mut right = reference(3);
        right.incarnation = Generation::new(incarnation).unwrap();
        let mut refs = [&b"tail"[..]; 4];
        assert!(
            view.prepare_leaf_split(&[16; 3], None, right, &mut refs)
                .is_err()
        );
        assert_eq!(refs, [&b"tail"[..]; 4]);
    }
}

#[test]
fn weighted_split_matches_exhaustive_boundary_oracle() {
    let expected = expectation(PageKind::RowLeaf, 0, 0);
    let mut checked = 0;
    for seed in 1u64..=64 {
        let mut state = seed;
        for _ in 0..128 {
            let mut next = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                (state >> 32) as usize
            };
            let count = 2 + next() % 7;
            let mut weights = vec![16; count];
            let mut remaining = 16_188 - 16 * count;
            for weight in &mut weights {
                let additional = next() % (remaining.min(8_094 - 16) + 1);
                *weight += additional;
                remaining -= additional;
            }
            let query = (1 + next() % (2 * count + 1)) as u64;
            let requested = 16 + next() % (8_094 - 15);
            // Independent typed-key oracle: each existing row is (2,4,...).
            // Keep the full maximum history cost when an update shrinks.
            let mut candidates = Vec::new();
            for at in 1..count {
                let mut left = weights[..at].iter().sum::<usize>();
                let mut right = weights[at..].iter().sum::<usize>();
                let old = if query.is_multiple_of(2) && query / 2 <= count as u64 {
                    weights[(query / 2 - 1) as usize]
                } else {
                    0
                };
                let increase = requested.saturating_sub(old);
                if query < 2 * (at as u64 + 1) {
                    left += increase;
                } else {
                    right += increase;
                }
                if left <= 16_188 && right <= 16_188 {
                    candidates.push((left.max(right), at));
                }
            }
            candidates.sort_unstable();
            let best = candidates.first().map(|candidate| candidate.1);
            let mut cells = vec![metadata(0, None)];
            for i in 1..=count {
                cells.push(leaf_cell(0, 2 * i as u64, 0));
            }
            let source = raw_page(expected, &cells, 7);
            let view = decode_tree_page(&source, expected).unwrap();
            let query_bytes = query.to_be_bytes();
            let mut scratch = vec![&[][..]; count + 1];
            let result = view.prepare_leaf_split(
                &weights,
                Some(LeafSpaceRequest {
                    key: &query_bytes,
                    entry_cost: requested,
                }),
                reference(90),
                &mut scratch,
            );
            if let Some(best) = best {
                let split = result.unwrap();
                assert_eq!(
                    split.separator(),
                    (2 * (best as u64 + 1)).to_be_bytes(),
                    "seed {seed}, query {query}, weights {weights:?}"
                );
            } else {
                assert!(matches!(
                    result,
                    Err(StructureError::Tree(TreeError::NotSplittable))
                ));
                assert!(scratch.iter().all(|entry| entry.is_empty()));
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 8192);
}

#[test]
fn weighted_split_without_single_boundary_retries_structure_before_pending_edit() {
    let expected = expectation(PageKind::RowLeaf, 0, 0);
    let cells = [
        metadata(0, None),
        leaf_cell(0, 10, 0),
        leaf_cell(0, 20, 0),
        leaf_cell(0, 30, 0),
    ];
    let source = raw_page(expected, &cells, 7);
    let view = decode_tree_page(&source, expected).unwrap();
    let pending_key = 25u64.to_be_bytes();
    let request = LeafSpaceRequest {
        key: &pending_key,
        entry_cost: 8094,
    };
    let mut refs = [&b"sentinel"[..]; 4];
    assert!(matches!(
        view.prepare_leaf_split(&[5000, 8000, 3000], Some(request), reference(90), &mut refs),
        Err(StructureError::Tree(TreeError::NotSplittable))
    ));
    assert_eq!(refs, [&b"sentinel"[..]; 4]);
    // Structural-only split remains valid. Its right page is the next search
    // result; the owner must revalidate it after publishing the first SMO.
    let first = view
        .prepare_leaf_split(&[5000, 8000, 3000], None, reference(90), &mut refs)
        .unwrap();
    assert_eq!(first.separator(), 20u64.to_be_bytes());
    let mut meta = [0; 32];
    let mut scratch = [&[][..]; 4];
    let mut right = vec![0; PAGE_BYTES];
    first
        .encode_side(true, PageLsn(19), &mut meta, &mut scratch, &mut right)
        .unwrap();
    let mut expected_right = expected;
    expected_right.identity.page = reference(90);
    let right_view = decode_tree_page(&right, expected_right).unwrap();
    let mut refs = [&[][..]; 3];
    let second = right_view
        .prepare_leaf_split(&[8000, 3000], Some(request), reference(91), &mut refs)
        .unwrap();
    assert_eq!(second.separator(), 30u64.to_be_bytes());
    let mut meta = [0; 32];
    let mut scratch = [&[][..]; 3];
    let mut leaf = vec![0; PAGE_BYTES];
    second
        .encode_side(false, PageLsn(20), &mut meta, &mut scratch, &mut leaf)
        .unwrap();
    assert_eq!(
        leaf,
        raw_page(expected_right, &[cells[0].clone(), cells[2].clone()], 20)
    );
    // The reserved maximum for old row20 plus future row25 fits this page.
    assert!(8000 + request.entry_cost <= TREE_ENTRY_CAPACITY);
}
