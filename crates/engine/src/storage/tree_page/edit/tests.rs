use super::*;
use crate::{
    api::DatabaseId,
    storage::page::{FileId, PAGE_BYTES},
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};
use std::collections::BTreeMap;

const FAMILIES: [(PageKind, u8, u8); 3] = [
    (PageKind::RowLeaf, 1, 0),
    (PageKind::IndexLeaf, 3, 1),
    (PageKind::CatalogLeaf, 5, 0),
];

fn expected(kind: PageKind, arity: u8) -> TreeExpectation {
    TreeExpectation {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: PageRef {
                file: FileId::new(0x1_0000_0002).unwrap(),
                page_no: 3,
                incarnation: Generation::new(4).unwrap(),
            },
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(6).unwrap(),
        },
        kind,
        level: 0,
        arity,
    }
}

// Independent literal scalar tags and BE RowId suffix. Ordinals 1..15 are
// NULL; 16..31 are false. Typed (scalar, RowId) order is exactly ordinal order.
fn key(arity: u8, ordinal: u64) -> Vec<u8> {
    if arity == 0 {
        return ordinal.to_be_bytes().to_vec();
    }
    let (mut bytes, row) = if ordinal < 16 {
        (vec![0], ordinal)
    } else {
        (vec![1, 0], ordinal - 15)
    };
    bytes.extend(row.to_be_bytes());
    bytes
}

fn cell(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut bytes = (key.len() as u16).to_le_bytes().to_vec();
    bytes.extend((value.len() as u16).to_le_bytes());
    bytes.extend(key);
    bytes.extend(value);
    bytes
}

// Assemble the frozen envelope directly. Neither source nor expected bytes
// use the product tree/page encoders, candidate mapping or lower_bound.
fn wire_page(tag: u8, arity: u8, entries: &[Vec<u8>], lsn: u64) -> Vec<u8> {
    let mut output = vec![0; 16_384];
    output[..4].copy_from_slice(b"XDP1");
    output[4..6].copy_from_slice(&1u16.to_le_bytes());
    output[6] = tag;
    output[8..24].fill(0x11);
    for (offset, value) in [
        (24, 0x1_0000_0002u64),
        (32, 3),
        (40, 4),
        (48, 5),
        (56, 6),
        (64, lsn),
    ] {
        output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    let count = entries.len() + 1;
    output[72..74].copy_from_slice(&(count as u16).to_le_bytes());
    output[74..76].copy_from_slice(&((128 + 4 * count) as u16).to_le_bytes());
    let mut metadata = vec![0; 32];
    metadata[..6].copy_from_slice(b"XBT1\x01\0");
    metadata[6] = arity;
    let mut end = 16_352;
    for (slot, bytes) in std::iter::once(&metadata).chain(entries).enumerate() {
        let start = end - bytes.len();
        output[128 + 4 * slot..130 + 4 * slot].copy_from_slice(&(start as u16).to_le_bytes());
        output[130 + 4 * slot..132 + 4 * slot].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
        output[start..end].copy_from_slice(bytes);
        end = start;
    }
    assert!(end >= 128 + 4 * count);
    output[76..78].copy_from_slice(&(end as u16).to_le_bytes());
    let hash = content_digest(&output[..16_352]);
    output[16_352..].copy_from_slice(&hash);
    output
}

fn entries(arity: u8, model: &BTreeMap<u64, Vec<u8>>) -> Vec<Vec<u8>> {
    model
        .iter()
        .map(|(ordinal, value)| cell(&key(arity, *ordinal), value))
        .collect()
}

#[test]
fn leaf_edit_insert_replace_remove_match_independent_pages() {
    for (kind, tag, arity) in FAMILIES {
        let mut model = BTreeMap::new();
        let mut source = wire_page(tag, arity, &[], 7);
        for (step, (operation, ordinal, value)) in [
            (0, 16, b"middle".as_slice()),
            (0, 1, b"first"),
            (0, 31, b"last"),
            (1, 16, b"new version"),
            (1, 1, b""),
            (1, 31, b"tail"),
            (2, 16, b""),
            (2, 1, b""),
            (2, 31, b""),
        ]
        .into_iter()
        .enumerate()
        {
            let before = source.clone();
            let query = key(arity, ordinal);
            let new_cell = cell(&query, value);
            let view = decode_tree_page(&source, expected(kind, arity)).unwrap();
            let mut scratch = [b"sentinel".as_slice(); 6];
            let edit = match operation {
                0 => LeafEdit::Insert(&new_cell),
                1 => LeafEdit::Replace(&new_cell),
                _ => LeafEdit::Remove(&query),
            };
            let plan = view.prepare_leaf_edit(edit, &mut scratch).unwrap();
            if operation == 2 {
                model.remove(&ordinal);
            } else {
                model.insert(ordinal, value.to_vec());
            }
            let expected_cells = entries(arity, &model);
            assert_eq!(
                plan.entry_bytes(),
                expected_cells.iter().map(|entry| entry.len() + 4).sum()
            );
            assert!(!plan.needs_split());
            let mut output = vec![0xA5; PAGE_BYTES];
            plan.encode(PageLsn(8 + step as u64), &mut output).unwrap();
            assert_eq!(
                output,
                wire_page(tag, arity, &expected_cells, 8 + step as u64)
            );
            assert_eq!(
                scratch[model.len() + 1..],
                [b"sentinel".as_slice(); 6][model.len() + 1..]
            );
            assert_eq!(source, before);
            assert_eq!(new_cell, cell(&query, value));
            source = output;
        }
    }
}

#[test]
fn leaf_edit_rejections_preserve_inputs_and_scratch() {
    for (kind, tag, arity) in FAMILIES {
        let present = cell(&key(arity, 16), b"A");
        let missing = cell(&key(arity, 31), b"B");
        let source = wire_page(tag, arity, std::slice::from_ref(&present), 7);
        let view = decode_tree_page(&source, expected(kind, arity)).unwrap();
        let absent_key = key(arity, 31);
        let mut trailing = missing.clone();
        trailing.push(0);
        for (edit, error) in [
            (LeafEdit::Insert(&present), LeafEditError::KeyExists),
            (LeafEdit::Replace(&missing), LeafEditError::KeyMissing),
            (LeafEdit::Remove(&absent_key), LeafEditError::KeyMissing),
            (
                LeafEdit::Insert(&[0, 0, 0]),
                LeafEditError::Tree(TreeError::Corrupt),
            ),
            (
                LeafEdit::Replace(&trailing),
                LeafEditError::Tree(TreeError::Corrupt),
            ),
            (
                LeafEdit::Remove(&[]),
                LeafEditError::Tree(TreeError::InvalidInput),
            ),
        ] {
            let mut scratch = [b"untouched".as_slice(); 4];
            assert_eq!(
                view.prepare_leaf_edit(edit, &mut scratch).unwrap_err(),
                error
            );
            assert_eq!(scratch, [b"untouched".as_slice(); 4]);
        }
        for length in 0..3 {
            let mut scratch = vec![b"untouched".as_slice(); length];
            assert_eq!(
                view.prepare_leaf_edit(LeafEdit::Insert(&missing), &mut scratch)
                    .unwrap_err(),
                LeafEditError::ScratchTooSmall
            );
            assert!(scratch.iter().all(|value| *value == b"untouched"));
        }
        for length in [3, 4] {
            let mut scratch = vec![b"untouched".as_slice(); length];
            let plan = view
                .prepare_leaf_edit(LeafEdit::Insert(&missing), &mut scratch)
                .unwrap();
            for output_len in [PAGE_BYTES - 1, PAGE_BYTES + 1] {
                let mut output = vec![0xA5; output_len];
                assert_eq!(
                    plan.encode(PageLsn(8), &mut output),
                    Err(TreeError::Page(PageError::InvalidInput))
                );
                assert!(output.iter().all(|byte| *byte == 0xA5));
            }
        }
        assert_eq!(source, wire_page(tag, arity, &[present], 7));
    }
    let metadata = encode_metadata(
        PageKind::RowInternal,
        0,
        Some(PageRef {
            page_no: 8,
            ..expected(PageKind::RowLeaf, 0).identity.page
        }),
        &mut [0; 32],
    )
    .unwrap()
    .to_vec();
    let mut internal = vec![0; PAGE_BYTES];
    let mut child = vec![0; 36];
    let right = PageRef {
        page_no: 9,
        ..expected(PageKind::RowLeaf, 0).identity.page
    };
    let child = encode_internal_cell(
        PageKind::RowInternal,
        0,
        &1u64.to_be_bytes(),
        right,
        &mut child,
    )
    .unwrap();
    let header = PageHeader {
        identity: expected(PageKind::RowLeaf, 0).identity,
        kind: PageKind::RowInternal,
        level: 1,
        lsn: PageLsn(7),
    };
    encode_tree_page(header, 0, &[&metadata, child], &mut internal).unwrap();
    let view = decode_tree_page(
        &internal,
        TreeExpectation {
            kind: PageKind::RowInternal,
            level: 1,
            ..expected(PageKind::RowLeaf, 0)
        },
    )
    .unwrap();
    let mut scratch = [b"keep".as_slice(); 3];
    let remove_key = 1u64.to_be_bytes();
    assert_eq!(
        view.prepare_leaf_edit(LeafEdit::Remove(&remove_key), &mut scratch)
            .unwrap_err(),
        LeafEditError::Tree(TreeError::InvalidInput)
    );
    assert_eq!(scratch, [b"keep".as_slice(); 3]);
}

#[test]
fn leaf_edit_byte_limits_and_overflow_handoff() {
    for (kind, tag, arity) in FAMILIES {
        let make = |ordinal, cost| {
            let query = key(arity, ordinal);
            cell(&query, &vec![ordinal as u8; cost - query.len() - 8])
        };
        let first = make(1, 8094);
        for cost in [8093, 8094, 8095] {
            let second = make(16, 8094);
            let source = wire_page(tag, arity, &[first.clone(), second], 7);
            let view = decode_tree_page(&source, expected(kind, arity)).unwrap();
            let replacement = make(16, cost);
            let mut scratch = [b"keep".as_slice(); 4];
            let result = view.prepare_leaf_edit(LeafEdit::Replace(&replacement), &mut scratch);
            if cost == 8095 {
                assert_eq!(
                    result.unwrap_err(),
                    LeafEditError::Tree(TreeError::EntryTooLarge)
                );
                assert_eq!(scratch, [b"keep".as_slice(); 4]);
            } else {
                let plan = result.unwrap();
                let mut output = vec![0; PAGE_BYTES];
                plan.encode(PageLsn(8), &mut output).unwrap();
                assert_eq!(
                    output,
                    wire_page(tag, arity, &[first.clone(), replacement], 8)
                );
            }
        }
        // C+H-1 and C+H retain complete split candidates.
        for second_cost in [8093, 8094] {
            let source = wire_page(tag, arity, &[make(1, 8094), make(16, second_cost)], 7);
            let view = decode_tree_page(&source, expected(kind, arity)).unwrap();
            let added = make(31, 8094);
            let mut scratch = [b"keep".as_slice(); 4];
            let plan = view
                .prepare_leaf_edit(LeafEdit::Insert(&added), &mut scratch)
                .unwrap();
            assert_eq!(plan.entry_bytes(), 8094 + second_cost + 8094);
            assert!(plan.needs_split());
            let split = plan.split().unwrap();
            assert_eq!(split.index, 1);
            assert_eq!(split.separator, key(arity, 16));
            assert_eq!(split.left_bytes, 8094);
            assert_eq!(split.right_bytes, second_cost + 8094);
            let mut output = vec![0xA5; PAGE_BYTES];
            assert_eq!(
                plan.encode(PageLsn(8), &mut output),
                Err(TreeError::NoSpace)
            );
            assert!(output.iter().all(|byte| *byte == 0xA5));
        }
        // A growing replacement crosses C without exceeding H per cell.
        let source = wire_page(
            tag,
            arity,
            &[make(1, 6000), make(16, 6000), make(31, 4187)],
            7,
        );
        let view = decode_tree_page(&source, expected(kind, arity)).unwrap();
        for cost in [4187, 4188, 4189] {
            let replacement = make(31, cost);
            let mut scratch = [b"keep".as_slice(); 4];
            let plan = view
                .prepare_leaf_edit(LeafEdit::Replace(&replacement), &mut scratch)
                .unwrap();
            assert_eq!(plan.entry_bytes(), 12000 + cost);
            assert_eq!(plan.needs_split(), cost == 4189);
            let mut output = vec![0xA5; PAGE_BYTES];
            if cost == 4189 {
                assert_eq!(
                    plan.encode(PageLsn(8), &mut output),
                    Err(TreeError::NoSpace)
                );
                assert!(output.iter().all(|byte| *byte == 0xA5));
                assert_eq!(plan.split().unwrap().index, 1);
            } else {
                plan.encode(PageLsn(8), &mut output).unwrap();
                assert_eq!(
                    output,
                    wire_page(tag, arity, &[make(1, 6000), make(16, 6000), replacement], 8)
                );
            }
        }
        let mut scratch = [b"keep".as_slice(); 4];
        let remove_key = key(arity, 1);
        let plan = view
            .prepare_leaf_edit(LeafEdit::Remove(&remove_key), &mut scratch)
            .unwrap();
        let mut output = vec![0; PAGE_BYTES];
        plan.encode(PageLsn(8), &mut output).unwrap();
        assert_eq!(
            output,
            wire_page(tag, arity, &[make(16, 6000), make(31, 4187)], 8)
        );
    }
}

#[test]
fn leaf_edit_maximum_reference_scratch_is_bounded() {
    assert_eq!(MAX_LEAF_EDIT_CELLS, 1013);
    let cells: Vec<_> = (1..=1011).map(|n| cell(&key(0, n), &[])).collect();
    let source = wire_page(1, 0, &cells, 7);
    let view = decode_tree_page(&source, expected(PageKind::RowLeaf, 0)).unwrap();
    let added = cell(&key(0, 1012), &[]);
    for count in [1012, 1013, 1014] {
        let mut scratch = vec![b"keep".as_slice(); count];
        let result = view.prepare_leaf_edit(LeafEdit::Insert(&added), &mut scratch);
        if count == 1012 {
            assert_eq!(result.unwrap_err(), LeafEditError::ScratchTooSmall);
            assert!(scratch.iter().all(|value| *value == b"keep"));
        } else {
            let plan = result.unwrap();
            assert_eq!(plan.entry_bytes(), 16192);
            assert!(plan.needs_split());
            assert_eq!(plan.split().unwrap().index, 506);
            if count == 1014 {
                assert_eq!(scratch[1013], b"keep");
            }
        }
    }
    assert_eq!(MAX_LEAF_EDIT_CELLS * size_of::<&[u8]>(), 16208);
    println!(
        "XADB_LEAF_EDIT_LAYOUT view={} edit={} plan={} result={} scratch={}",
        size_of::<TreePageView<'_>>(),
        size_of::<LeafEdit<'_>>(),
        size_of::<LeafEditPlan<'_, '_>>(),
        size_of::<Result<LeafEditPlan<'_, '_>, LeafEditError>>(),
        MAX_LEAF_EDIT_CELLS * size_of::<&[u8]>()
    );
}

// D3-TREE-EDIT-G1: seeds 1..=64 (S5 1..=256), wrapping LCG below. Each op
// draws action, ordinal and value length in that order. Every history starts
// empty; all three families run the same independent typed operation stream.
pub(super) fn history(seeds: u64, operations: usize) {
    for (kind, tag, arity) in FAMILIES {
        for seed in 1..=seeds {
            let mut state = seed;
            let mut draw = || {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                state >> 32
            };
            let mut model = BTreeMap::new();
            let mut source = wire_page(tag, arity, &[], 7);
            for step in 0..operations {
                let action = draw() % 3;
                let ordinal = draw() % 31 + 1;
                let value = vec![(step % 251) as u8; (draw() % 97) as usize];
                let query = key(arity, ordinal);
                let new_cell = cell(&query, &value);
                let edit = match action {
                    0 => LeafEdit::Insert(&new_cell),
                    1 => LeafEdit::Replace(&new_cell),
                    _ => LeafEdit::Remove(&query),
                };
                let view = decode_tree_page(&source, expected(kind, arity)).unwrap();
                let mut scratch = [b"keep".as_slice(); 33];
                let result = view.prepare_leaf_edit(edit, &mut scratch);
                let present = model.contains_key(&ordinal);
                if action == 0 && present || action != 0 && !present {
                    assert_eq!(
                        result.unwrap_err(),
                        if present {
                            LeafEditError::KeyExists
                        } else {
                            LeafEditError::KeyMissing
                        }
                    );
                    assert_eq!(scratch, [b"keep".as_slice(); 33]);
                    continue;
                }
                if action == 2 {
                    model.remove(&ordinal);
                } else {
                    model.insert(ordinal, value);
                }
                let mut output = vec![0xA5; PAGE_BYTES];
                result
                    .unwrap()
                    .encode(PageLsn(step as u64 + 8), &mut output)
                    .unwrap();
                assert_eq!(
                    output,
                    wire_page(tag, arity, &entries(arity, &model), step as u64 + 8),
                    "seed={seed} step={step} family={tag}"
                );
                source = output;
            }
        }
    }
}

#[test]
fn leaf_edit_generated_histories_match_ordered_map() {
    history(64, 128);
}
