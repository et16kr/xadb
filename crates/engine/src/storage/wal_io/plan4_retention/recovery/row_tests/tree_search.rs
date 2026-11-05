//! Selector-only observations over immutable images; these fixtures do not
//! grant allocation, full-base, live pin, or scope3 multi-level authority.
use super::*;
use crate::storage::{
    anchor::{self, AnchorRoot, TreeKind},
    tree_page::{EntryPayload, TreeExpectation},
};

fn identity(number: u64) -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: page_ref(number, 2),
        owner: table().object,
        owner_generation: table().generation,
    }
}
fn source() -> (Vec<u8>, [u8; JOB_BYTES]) {
    let mut arena = vec![0; history::ARENA_END];
    arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&catalog_leaf(6, false, page_ref(12, 2)));
    arena[12 * PAGE_BYTES..13 * PAGE_BYTES].copy_from_slice(scope3!("anchor.bin"));
    arena[13 * PAGE_BYTES..14 * PAGE_BYTES].copy_from_slice(scope3!("h-501-leaf.bin"));
    let bank = scope3!("h-501-bank.bin");
    let raw: [u8; JOB_BYTES] = bank[16 * JOB_BYTES..17 * JOB_BYTES].try_into().unwrap();
    let job = job3::decode_job(&raw, namespace()).unwrap();
    assert_eq!(job.row_id().unwrap().get(), 7);
    assert_eq!(job.pair().unwrap(), [page_ref(12, 2), page_ref(13, 2)]);
    (arena, raw)
}
fn tree_page(number: u64, level: u8, cells: &[&[u8]]) -> Page {
    let mut output = [0; PAGE_BYTES];
    tree_page::encode_tree_page(
        PageHeader {
            identity: identity(number),
            kind: if level == 0 {
                PageKind::RowLeaf
            } else {
                PageKind::RowInternal
            },
            level,
            lsn: PageLsn(502),
        },
        0,
        cells,
        &mut output,
    )
    .unwrap();
    output
}
fn split(arena: &mut [u8]) {
    let before = arena[13 * PAGE_BYTES..14 * PAGE_BYTES].to_vec();
    let view = tree_page::decode_tree_page(
        &before,
        TreeExpectation {
            identity: identity(13),
            kind: PageKind::RowLeaf,
            level: 0,
            arity: 0,
        },
    )
    .unwrap();
    assert_eq!(view.len(), 4);
    let mut metadata = [0; 32];
    tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut metadata).unwrap();
    for (indices, number) in [(&[0usize, 1][..], 13), (&[2, 3][..], 22)] {
        let mut cells = vec![metadata.to_vec()];
        for &index in indices {
            let entry = view.entry(index).unwrap();
            assert_eq!(entry.key, [2u64, 4, 7, 9][index].to_be_bytes());
            let EntryPayload::Value(value) = entry.payload else {
                panic!("leaf");
            };
            let mut cell = [0; 4096];
            let cell =
                tree_page::encode_leaf_cell(PageKind::RowLeaf, 0, entry.key, value, &mut cell)
                    .unwrap();
            cells.push(cell.to_vec());
        }
        let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
        let bytes = tree_page(number, 0, &refs);
        arena[number as usize * PAGE_BYTES..(number as usize + 1) * PAGE_BYTES]
            .copy_from_slice(&bytes);
    }
    let mut metadata = [0; 32];
    tree_page::encode_metadata(
        PageKind::RowInternal,
        0,
        Some(page_ref(13, 2)),
        &mut metadata,
    )
    .unwrap();
    let mut cell = [0; 64];
    let cell = tree_page::encode_internal_cell(
        PageKind::RowInternal,
        0,
        &7u64.to_be_bytes(),
        page_ref(22, 2),
        &mut cell,
    )
    .unwrap();
    let root = tree_page(21, 1, &[&metadata, cell]);
    arena[21 * PAGE_BYTES..22 * PAGE_BYTES].copy_from_slice(&root);
    anchor::encode_anchor(
        PageHeader {
            identity: identity(12),
            kind: PageKind::TreeAnchor,
            level: 0,
            lsn: PageLsn(502),
        },
        AnchorRoot {
            tree: TreeKind::Row,
            page: page_ref(21, 2),
            level: 1,
            revision: 2,
        },
        &mut arena[12 * PAGE_BYTES..13 * PAGE_BYTES],
    )
    .unwrap();
}

#[test]
fn row_replay_tree_selector_uses_current_anchor_instead_of_job_hint() {
    let (mut arena, raw) = source();
    let job = job3::decode_job(&raw, namespace()).unwrap();
    let before = history::row::table_source(&arena, profile(), job).unwrap();
    assert_eq!(before.leaf_image.unwrap(), scope3!("h-501-leaf.bin"));
    split(&mut arena);
    let selected = history::row::table_source(&arena, profile(), job).unwrap();
    assert_eq!(
        selected.leaf_image.unwrap().as_slice(),
        &arena[22 * PAGE_BYTES..23 * PAGE_BYTES]
    );
    assert_ne!(
        selected.leaf_image.unwrap().as_slice(),
        &arena[13 * PAGE_BYTES..14 * PAGE_BYTES]
    );
    assert_eq!(selected.handle, table());
    assert_eq!(selected.anchor, page_ref(12, 2));
    // The selector does not modify the historical Job hint or tree bytes.
    assert_eq!(job.pair().unwrap()[1], page_ref(13, 2));
    let prior = arena.clone();
    history::row::table_source(&arena, profile(), job).unwrap();
    assert_eq!(arena, prior);
}

#[test]
fn row_replay_tree_selector_rejects_wrong_identity_range_and_catalog_binding() {
    let (mut arena, raw) = source();
    split(&mut arena);
    let job = job3::decode_job(&raw, namespace()).unwrap();
    let original = arena.clone();
    for defect in 0..4 {
        arena.copy_from_slice(&original);
        match defect {
            // A locally valid but different leaf cannot satisfy root's child.
            0 => {
                let wrong = arena[13 * PAGE_BYTES..14 * PAGE_BYTES].to_vec();
                arena[22 * PAGE_BYTES..23 * PAGE_BYTES].copy_from_slice(&wrong);
            }
            // Correct identity but lower key outside subtree [7, infinity).
            1 => {
                let mut metadata = [0; 32];
                tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut metadata).unwrap();
                let mut cell = [0; 32];
                let cell = tree_page::encode_leaf_cell(
                    PageKind::RowLeaf,
                    0,
                    &1u64.to_be_bytes(),
                    b"bad",
                    &mut cell,
                )
                .unwrap();
                let wrong = tree_page(22, 0, &[&metadata, cell]);
                arena[22 * PAGE_BYTES..23 * PAGE_BYTES].copy_from_slice(&wrong);
            }
            2 => arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&catalog_leaf(
                6,
                false,
                page_ref(11, 2),
            )),
            3 => arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&catalog_leaf(
                6,
                true,
                page_ref(12, 2),
            )),
            _ => unreachable!(),
        }
        let prior = arena.clone();
        assert!(
            history::row::table_source(&arena, profile(), job).is_err(),
            "defect {defect}"
        );
        assert_eq!(arena, prior);
    }
}

#[test]
fn row_replay_tree_selector_fits_existing_source_admission() {
    use crate::storage::wal_io::plan4_test_support::Writer;
    let row = history::row::source_bytes();
    let owner = super::super::super::resources::source_census::<Writer>();
    println!(
        "tree selector source: row semantic={}, owner peak={}, metadata={}, whole={}",
        row.semantic, owner.peak, owner.metadata, owner.whole
    );
    assert!(super::super::super::resources::fits::<Writer>());
    assert!(owner.peak <= 65_536 && owner.metadata <= 196_608 && owner.whole <= 262_144);
}
