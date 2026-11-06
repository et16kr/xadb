use super::*;
const ROOT_SPLIT: &[u8; BASE_BYTES] = fixture!("current-tree/root-split.xrb");
const CHILD_SPLIT: &[u8; BASE_BYTES] = fixture!("current-tree/child-split.xrb");

fn put(raw: &mut [u8], at: usize, value: u64) {
    raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn seal(raw: &mut [u8]) {
    let n = raw.len() - 32;
    let digest = content_digest(&raw[..n]);
    raw[n..].copy_from_slice(&digest);
}
fn reseal(raw: &mut [u8; BASE_BYTES]) {
    for (i, (at, len)) in [
        (BANK, DESCRIPTOR_BYTES),
        (CERTIFICATES, 64 * 96),
        (PROVENANCE, 64 * 352),
        (INVENTORY, PAGE_BYTES),
        (ARENA, 64 * PAGE_BYTES),
    ]
    .into_iter()
    .enumerate()
    {
        let digest = content_digest(&raw[at..at + len]);
        raw[1280 + i * 64 + 32..1280 + (i + 1) * 64].copy_from_slice(&digest);
    }
    seal(raw);
}
fn change_page(raw: &mut [u8; BASE_BYTES], ordinal: usize, edit: impl FnOnce(&mut [u8])) {
    let p = &mut raw[ARENA + ordinal * PAGE_BYTES..ARENA + (ordinal + 1) * PAGE_BYTES];
    edit(p);
    seal(p);
    let digest = content_digest(p);
    let header: [u8; 48] = p[24..72].try_into().unwrap();
    let at = CERTIFICATES + ordinal * 96;
    raw[at + 8..at + 56].copy_from_slice(&header);
    raw[at + 56..at + 88].copy_from_slice(&digest);
    reseal(raw);
}
fn offset(page: &[u8], slot: usize) -> usize {
    usize::from(u16::from_le_bytes(
        page[128 + slot * 4..130 + slot * 4].try_into().unwrap(),
    ))
}

#[test]
fn base4_current_tree_accepts_root_child_split_with_moved_retained_rows() {
    assert_eq!(
        decode(LIVE, expected(), &mut scratch()).unwrap().census(),
        LIVE_CENSUS
    );
    for (bytes, row2_page, leaves) in [
        (ROOT_SPLIT, 11, vec![11, 16]),
        (CHILD_SPLIT, 18, vec![11, 16, 18]),
    ] {
        let view = decode(bytes, expected(), &mut scratch()).unwrap();
        assert_eq!(view.census(), LIVE_CENSUS);
        let tree = current_tree::observe(view.layout(), 0).unwrap();
        assert_eq!(
            (9..64).filter(|&i| tree.has_leaf(i)).collect::<Vec<_>>(),
            leaves
        );
        assert_eq!(
            tree.find(RowId::new(2).unwrap())
                .unwrap()
                .header()
                .identity
                .page
                .page_no,
            row2_page
        );
        assert_eq!(
            tree.find(RowId::new(3).unwrap())
                .unwrap()
                .header()
                .identity
                .page
                .page_no,
            16
        );
        let graph = Graph::new(view.layout()).unwrap();
        for ordinal in [1, 2] {
            assert_eq!(graph.job(ordinal).unwrap().pair().unwrap()[1].page_no, 11);
        }
        let (output, result) = encoded(bytes);
        assert_eq!(result.unwrap(), LIVE_CENSUS);
        assert_eq!(&*output, bytes);
    }
}

#[test]
fn base4_current_tree_rejects_alias_range_identity_and_anchor_history() {
    for mutation in 0..7 {
        let mut bytes = backing(ROOT_SPLIT);
        match mutation {
            0 => change_page(&mut bytes, 17, |p| {
                let c = offset(p, 1);
                p[c + 4..c + 12].copy_from_slice(&2u64.to_be_bytes());
            }),
            1 => change_page(&mut bytes, 17, |p| {
                let c = offset(p, 1);
                put(p, c + 20, 11);
            }),
            2 => change_page(&mut bytes, 17, |p| {
                let c = offset(p, 1);
                put(p, c + 28, 3);
            }),
            3 => change_page(&mut bytes, 17, |p| {
                let c = offset(p, 1);
                put(p, c + 20, 14);
            }),
            4 => change_page(&mut bytes, 10, |p| {
                let c = offset(p, 0);
                p[c + 7] = 2;
            }),
            5 => change_page(&mut bytes, 10, |p| {
                let c = offset(p, 0);
                put(p, c + 32, 1);
            }),
            _ => change_page(&mut bytes, 10, |p| put(p, 64, 69)),
        }
        // Page/checksum/provenance grammar still closes before tree rejection.
        assert!(
            decode_layout(&*bytes, expected()).is_ok(),
            "layout {mutation}"
        );
        assert!(
            decode(&*bytes, expected(), &mut scratch()).is_err(),
            "closure {mutation}"
        );
    }
}

#[test]
fn base4_current_tree_retained_current_leaf_cannot_precede_job_step() {
    let mut bytes = backing(ROOT_SPLIT);
    // An older birth of the two current pages is a raw history observation.
    // First prove its whole current closure, then change only the leaf LSN.
    for ordinal in [16, 17] {
        let p = &mut bytes[PROVENANCE + ordinal * 352..PROVENANCE + (ordinal + 1) * 352];
        for at in [48, 160, 176] {
            put(p, at, 25);
        }
    }
    reseal(&mut bytes);
    assert_eq!(decode(&*bytes, expected(), &mut scratch()).err(), None);
    change_page(&mut bytes, 16, |p| put(p, 64, 31));
    assert!(decode_layout(&*bytes, expected()).is_ok());
    assert_eq!(
        decode(&*bytes, expected(), &mut scratch()).err(),
        Some(RowBaseError::Actor(actor::Error::Order))
    );
}

const MIXED: &[u8; BASE_BYTES] = fixture!("current-tree/delete-update.xrb");
const MIXED_LEFT_EXACT: &[u8; BASE_BYTES] = fixture!("current-tree/delete-update-11-0.xrb");
const MIXED_RIGHT_EXACT: &[u8; BASE_BYTES] = fixture!("current-tree/delete-update-16-0.xrb");

#[test]
fn base4_current_tree_delete_update_distribute_typed_undo_reservations_by_leaf() {
    for (bytes, ordinal, debt) in [(MIXED_LEFT_EXACT, 11, 1009), (MIXED_RIGHT_EXACT, 16, 1300)] {
        let view = decode(bytes, expected(), &mut scratch()).unwrap();
        let tree = current_tree::observe(view.layout(), 0).unwrap();
        let leaf = tree.leaf(ordinal).unwrap();
        let used = (0..leaf.len())
            .map(|at| match leaf.entry(at).unwrap().payload {
                EntryPayload::Value(value) => value.len() + 16,
                _ => panic!("row"),
            })
            .sum::<usize>();
        assert_eq!(crate::storage::tree_page::TREE_ENTRY_CAPACITY - used, debt);
        let graph = Graph::new(view.layout()).unwrap();
        assert_eq!(graph.leaf_reservation(&tree, &leaf), Ok(()));
        assert_eq!(graph.job(2).unwrap().pair().unwrap()[1].page_no, 11);
        assert_eq!(
            tree.find(RowId::new(40).unwrap())
                .unwrap()
                .header()
                .identity
                .page
                .page_no,
            16
        );
    }
    for bytes in [
        fixture!("current-tree/delete-update-11-1.xrb"),
        fixture!("current-tree/delete-update-16-1.xrb"),
    ] {
        assert!(decode_layout(bytes, expected()).is_ok());
        assert_eq!(
            decode(bytes, expected(), &mut scratch()).err(),
            Some(RowBaseError::Corrupt)
        );
    }
}

#[test]
fn base4_current_tree_typed_undo_and_same_catalog_schema_are_required() {
    assert_eq!(
        decode(MIXED, expected(), &mut scratch()).unwrap().census(),
        LIVE_CENSUS
    );
    for mutation in 0..3 {
        let mut bytes = backing(MIXED);
        match mutation {
            // The before body is well formed UTF8 but the table declares Binary.
            0 => change_page(&mut bytes, 13, |p| {
                let c = offset(p, 0);
                p[c + 432 + 4] = 3;
            }),
            // The same-cut Catalog switches Binary to UTF8; rows and U stay Binary.
            1 => change_page(&mut bytes, 8, |p| {
                let c = offset(p, 1);
                let len = usize::from(u16::from_le_bytes(p[134..136].try_into().unwrap()));
                p[c + len - 3] = 3;
            }),
            // Job C stays 1689 but its typed Undo's claimed body length changes.
            _ => change_page(&mut bytes, 13, |p| {
                let c = offset(p, 0);
                p[c + 320..c + 324].copy_from_slice(&1508u32.to_le_bytes());
            }),
        }
        assert!(decode_layout(&*bytes, expected()).is_ok());
        assert!(matches!(
            decode(&*bytes, expected(), &mut scratch()).err(),
            Some(RowBaseError::Actor(_))
        ));
    }
}

#[test]
fn base4_current_tree_preserves_reserved_absence_and_ready_releasing_states() {
    for (name, bytes) in [
        (
            "state-row-reserved",
            fixture!("current-tree/state-row-reserved.xrb"),
        ),
        (
            "state-ready-writer",
            fixture!("current-tree/state-ready-writer.xrb"),
        ),
        (
            "state-compensated-writer",
            fixture!("current-tree/state-compensated-writer.xrb"),
        ),
    ] {
        let before = state(name);
        let expected_census = decode(&*before, expected(), &mut scratch())
            .unwrap()
            .census();
        let view = decode(bytes, expected(), &mut scratch()).unwrap();
        assert_eq!(view.census(), expected_census, "{name}");
        let tree = current_tree::observe(view.layout(), 0).unwrap();
        assert_ne!(
            tree.find(RowId::new(3).unwrap())
                .unwrap()
                .header()
                .identity
                .page
                .page_no,
            11
        );
        if name == "state-row-reserved" {
            let leaf = tree.find(RowId::new(4).unwrap()).unwrap();
            assert!(!leaf.lower_bound(&4u64.to_be_bytes()).unwrap().equal);
        }
    }
}

#[test]
fn base4_current_tree_rejects_orphan_and_global_logical_duplicate() {
    let mut orphan = backing(CHILD_SPLIT);
    // Omit middle child18 while preserving a locally valid internal root.
    change_page(&mut orphan, 17, |p| {
        let meta = p[offset(p, 0)..offset(p, 0) + 32].to_vec();
        let last = p[offset(p, 2)..offset(p, 2) + 36].to_vec();
        p[72..78].fill(0);
        p[128..PAGE_BYTES - 32].fill(0);
        p[72..74].copy_from_slice(&2u16.to_le_bytes());
        p[74..76].copy_from_slice(&136u16.to_le_bytes());
        let mut end = PAGE_BYTES - 32;
        for (i, cell) in [meta, last].into_iter().enumerate() {
            end -= cell.len();
            p[end..end + cell.len()].copy_from_slice(&cell);
            p[128 + 4 * i..130 + 4 * i].copy_from_slice(&(end as u16).to_le_bytes());
            p[130 + 4 * i..132 + 4 * i].copy_from_slice(&(cell.len() as u16).to_le_bytes());
        }
        p[76..78].copy_from_slice(&(end as u16).to_le_bytes());
    });
    let layout = decode_layout(&*orphan, expected()).unwrap();
    assert_eq!(
        current_tree::observe(layout, 0).err(),
        Some(BaseError::IdentityMismatch)
    );
    assert!(decode(&*orphan, expected(), &mut scratch()).is_err());
    let mut duplicate = backing(MIXED_RIGHT_EXACT);
    let first = &MIXED_RIGHT_EXACT[ARENA + 11 * PAGE_BYTES..ARENA + 12 * PAGE_BYTES];
    let logical = first[offset(first, 1) + 12 + 72..offset(first, 1) + 12 + 120].to_vec();
    change_page(&mut duplicate, 16, |p| {
        let c = offset(p, 2);
        p[c + 12 + 72..c + 12 + 120].copy_from_slice(&logical);
    });
    let layout = decode_layout(&*duplicate, expected()).unwrap();
    assert!(current_tree::observe(layout, 0).is_ok());
    assert_eq!(
        decode(&*duplicate, expected(), &mut scratch()).err(),
        Some(RowBaseError::Identity)
    );
}

#[test]
fn base4_current_tree_revision_bound_accepts_maximum_high_water_without_wrap() {
    let mut bytes = backing(ROOT_SPLIT);
    for at in [96, 216, 248] {
        put(&mut *bytes, at, u64::MAX);
    }
    let d = &mut bytes[BANK + 33 * 1536..BANK + 34 * 1536];
    for at in [56, 72, 88, 544] {
        put(d, at, u64::MAX);
    }
    seal(d);
    let len =
        u32::from_le_bytes(bytes[INVENTORY + 12..INVENTORY + 16].try_into().unwrap()) as usize;
    put(&mut *bytes, INVENTORY + 48, u64::MAX);
    seal(&mut bytes[INVENTORY..INVENTORY + len]);
    change_page(&mut bytes, 10, |p| {
        let c = offset(p, 0);
        put(p, c + 32, u64::MAX);
    });
    assert_eq!(decode(&*bytes, expected(), &mut scratch()).err(), None);
    let mut invalid = backing(ROOT_SPLIT);
    change_page(&mut invalid, 10, |p| {
        let c = offset(p, 0);
        put(p, c + 32, 72);
    });
    assert!(decode_layout(&*invalid, expected()).is_ok());
    assert!(current_tree::observe(decode_layout(&*invalid, expected()).unwrap(), 0).is_err());
}

/// The state matrix is intentionally actor-only: raw layout, Root census,
/// Status and typed Undo are real observations; no ATT or full-base acceptance
/// is claimed by these vectors.
#[test]
fn base4_current_tree_moved_delete_update_reserved_and_release_presence() {
    let fixtures: [(u8, &str, &[u8; BASE_BYTES]); 6] = [
        (1, "reserved", fixture!("current-tree/actor-1-reserved.xrb")),
        (
            1,
            "committed",
            fixture!("current-tree/actor-1-committed.xrb"),
        ),
        (
            1,
            "compensated",
            fixture!("current-tree/actor-1-compensated.xrb"),
        ),
        (2, "reserved", fixture!("current-tree/actor-2-reserved.xrb")),
        (
            2,
            "committed",
            fixture!("current-tree/actor-2-committed.xrb"),
        ),
        (
            2,
            "compensated",
            fixture!("current-tree/actor-2-compensated.xrb"),
        ),
    ];
    let parent = Graph::new(decode_layout(MIXED, expected()).unwrap()).unwrap();
    assert_eq!(parent.job(2).unwrap().pair().unwrap()[1].page_no, 11);
    for (effect, state, bytes) in fixtures {
        let layout = decode_layout(bytes, expected())
            .unwrap_or_else(|e| panic!("layout {effect}/{state}: {e:?}"));
        let graph = Graph::new(layout).unwrap();
        let actor = graph.job(0).unwrap().source().unwrap();
        let tree = current_tree::observe(layout, 0).unwrap();
        let leaf = tree.find(RowId::new(40).unwrap()).unwrap();
        assert_eq!(leaf.header().identity.page.page_no, 16);
        assert!(leaf.header().lsn.0 < graph.job(2).unwrap().last_step());
        if state == "reserved" {
            assert_eq!(graph.job(2).unwrap().pair().unwrap()[1].page_no, 11);
        } else {
            assert!(graph.job(2).unwrap().pair().is_none());
        }
        let observation = actor::observe(layout, actor, &mut scratch())
            .unwrap_or_else(|e| panic!("{effect}/{state}: {e:?}"));
        assert_eq!(observation.retained, 1);
        assert_eq!(observation.transaction_rows, 1);
        assert_eq!(observation.reserved, usize::from(state == "reserved"));
        assert_eq!(observation.releasing, usize::from(state != "reserved"));
        let present = effect != 1 || state != "committed";
        assert_eq!(
            leaf.lower_bound(&40u64.to_be_bytes()).unwrap().equal,
            present
        );
        let mut bad = backing(bytes);
        change_page(&mut bad, 16, |p| {
            let metadata = p[offset(p, 0)..offset(p, 0) + 32].to_vec();
            let source = &MIXED[ARENA + 16 * PAGE_BYTES..ARENA + 17 * PAGE_BYTES];
            let start = offset(source, 1);
            let size = usize::from(u16::from_le_bytes(source[134..136].try_into().unwrap()));
            let added = source[start..start + size].to_vec();
            let cells = if present {
                vec![metadata]
            } else {
                vec![metadata, added]
            };
            p[128..PAGE_BYTES - 32].fill(0);
            p[72..74].copy_from_slice(&(cells.len() as u16).to_le_bytes());
            p[74..76].copy_from_slice(&((128 + 4 * cells.len()) as u16).to_le_bytes());
            let mut end = PAGE_BYTES - 32;
            for (i, cell) in cells.iter().enumerate() {
                end -= cell.len();
                p[end..end + cell.len()].copy_from_slice(cell);
                p[128 + 4 * i..130 + 4 * i].copy_from_slice(&(end as u16).to_le_bytes());
                p[130 + 4 * i..132 + 4 * i].copy_from_slice(&(cell.len() as u16).to_le_bytes());
            }
            p[76..78].copy_from_slice(&(end as u16).to_le_bytes());
        });
        let layout = decode_layout(&*bad, expected()).unwrap();
        assert_eq!(
            actor::observe(layout, actor, &mut scratch()).err(),
            Some(actor::Error::Identity),
            "{effect}/{state}"
        );
    }
}

#[test]
fn base4_current_tree_accepts_internal_root_split_and_checks_off_path_internal() {
    let bytes = fixture!("current-tree/internal-root-split.xrb");
    let view = decode(bytes, expected(), &mut scratch()).unwrap();
    assert_eq!(view.census(), LIVE_CENSUS);
    let tree = current_tree::observe(view.layout(), 0).unwrap();
    assert_eq!(
        tree.find(RowId::new(2).unwrap())
            .unwrap()
            .header()
            .identity
            .page
            .page_no,
        18
    );
    assert_eq!(
        tree.find(RowId::new(3).unwrap())
            .unwrap()
            .header()
            .identity
            .page
            .page_no,
        16
    );
    let mut bad = backing(bytes);
    // The right internal is off the row2 path; whole observation still visits it.
    change_page(&mut bad, 19, |p| {
        let c = offset(p, 0);
        put(p, c + 16, 11);
    });
    assert!(decode_layout(&*bad, expected()).is_ok());
    assert!(current_tree::observe(decode_layout(&*bad, expected()).unwrap(), 0).is_err());
}

// UNL-B01..B09 (dev3-tree-smo.md#empty-leaf-unlink). The two snapshots are
// authored independently (author.py leaf_unlink/root_collapse): a LeafUnlink90
// that listed empty leaf19, and a RootCollapse90 that listed the CREATE leaf11
// and root17. They prove current-state closure only, not selected history.
const UNLINK_LEAF: &[u8; BASE_BYTES] = fixture!("current-tree/unlink-leaf.xrb");
const UNLINK_COLLAPSE: &[u8; BASE_BYTES] = fixture!("current-tree/unlink-collapse.xrb");

/// Independent XTR1 cell: (page, incarnation, kind, level, retire LSN).
fn xtr1(entries: &[(u64, u64, u8, u8, u64)]) -> Vec<u8> {
    let mut c = b"XTR1\x01\x00".to_vec();
    c.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    c.extend_from_slice(&[0; 8]);
    for (n, incarnation, kind, level, lsn) in entries {
        c.extend_from_slice(&0x1_0000_0001u64.to_le_bytes());
        c.extend_from_slice(&n.to_le_bytes());
        c.extend_from_slice(&incarnation.to_le_bytes());
        c.extend_from_slice(&[*kind, *level, 0, 0, 0, 0, 0, 0]);
        c.extend_from_slice(&lsn.to_le_bytes());
    }
    c
}
/// Replace every cell of a page image in place (the caller reseals).
fn set_cells(p: &mut [u8], cells: &[Vec<u8>]) {
    p[128..PAGE_BYTES - 32].fill(0);
    p[72..74].copy_from_slice(&(cells.len() as u16).to_le_bytes());
    p[74..76].copy_from_slice(&((128 + 4 * cells.len()) as u16).to_le_bytes());
    let mut end = PAGE_BYTES - 32;
    for (i, cell) in cells.iter().enumerate() {
        end -= cell.len();
        p[end..end + cell.len()].copy_from_slice(cell);
        p[128 + 4 * i..130 + 4 * i].copy_from_slice(&(end as u16).to_le_bytes());
        p[130 + 4 * i..132 + 4 * i].copy_from_slice(&(cell.len() as u16).to_le_bytes());
    }
    p[76..78].copy_from_slice(&(end as u16).to_le_bytes());
}
fn cell(p: &[u8], slot: usize) -> Vec<u8> {
    let size = usize::from(u16::from_le_bytes(
        p[130 + slot * 4..132 + slot * 4].try_into().unwrap(),
    ));
    p[offset(p, slot)..offset(p, slot) + size].to_vec()
}
fn leaves(tree: &current_tree::Table<'_>) -> Vec<usize> {
    (9..64).filter(|&i| tree.has_leaf(i)).collect()
}
fn row_page(tree: &current_tree::Table<'_>, row: u64) -> u64 {
    tree.find(RowId::new(row).unwrap())
        .unwrap()
        .header()
        .identity
        .page
        .page_no
}

/// UNL-B01/B07/B08/B09: Base4 accepts a listed empty leaf and a listed
/// internal page, the retired CREATE leaf keeps its pair claim and Job
/// hints, a bank unlink owner beside older split claims, and a Base
/// rewrite keeps the list and claims byte for byte. Restart over these
/// states is UNL-R (U2).
#[test]
fn unl_b01_base4_accepts_listed_retired_leaf_and_internal_pages() {
    use crate::storage::semantic_row::structure::{Operation, bank};
    for (bytes, live, rows, operation) in [
        (
            UNLINK_LEAF,
            vec![11, 16, 18],
            [11, 18, 16],
            Operation::LeafUnlink,
        ),
        (
            UNLINK_COLLAPSE,
            vec![16],
            [16, 16, 16],
            Operation::RootCollapse,
        ),
    ] {
        // B08: bank33 holds the unlink owner90; the split claims (70, 80, 85)
        // are older, so none of them names that owner.
        let raw: &[u8; 1536] = bytes[BANK + 33 * 1536..BANK + 34 * 1536]
            .try_into()
            .unwrap();
        assert_eq!(
            bank::observe(expected().profile, 90, 1, 90, Some(raw))
                .unwrap()
                .unwrap()
                .operation,
            operation
        );
        let view = decode(bytes, expected(), &mut scratch()).unwrap();
        assert_eq!(view.census(), LIVE_CENSUS);
        let tree = current_tree::observe(view.layout(), 0).unwrap();
        assert_eq!(leaves(&tree), live);
        assert_eq!([1, 2, 3].map(|row| row_page(&tree, row)), rows);
        // B07: Job hints keep naming the CREATE leaf11, retired or not.
        let graph = Graph::new(view.layout()).unwrap();
        for ordinal in [1, 2] {
            assert_eq!(graph.job(ordinal).unwrap().pair().unwrap()[1].page_no, 11);
        }
        // B09: a Base rewrite keeps the list, retired images and claims.
        let (output, result) = encoded(bytes);
        assert_eq!(result.unwrap(), LIVE_CENSUS);
        assert_eq!(&*output, bytes);
    }
}

/// UNL-B02..B06: an orphan outside the list, a reached listed page, an
/// unallocated item, a non-empty retired leaf, and a wrong incarnation or
/// kind are refused by the closure after the layout grammar accepts them.
#[test]
fn unl_b02_b06_base4_rejects_retired_list_ownership_mismatch() {
    for mutation in 0..6 {
        let mut bytes = backing(UNLINK_LEAF);
        let list: Option<Vec<u8>> = match mutation {
            0 => None,
            1 => Some(xtr1(&[(18, 2, 1, 0, 90), (19, 2, 1, 0, 90)])),
            2 => Some(xtr1(&[(19, 2, 1, 0, 90), (20, 2, 1, 0, 90)])),
            3 => {
                let row = cell(
                    &UNLINK_LEAF[ARENA + 16 * PAGE_BYTES..ARENA + 17 * PAGE_BYTES],
                    1,
                );
                change_page(&mut bytes, 19, |p| {
                    let meta = cell(p, 0);
                    set_cells(p, &[meta, row]);
                });
                continue_with(&bytes, mutation);
                continue;
            }
            4 => Some(xtr1(&[(19, 3, 1, 0, 90)])),
            _ => Some(xtr1(&[(19, 2, 2, 1, 90)])),
        };
        change_page(&mut bytes, 10, |p| {
            let root = cell(p, 0);
            match &list {
                None => set_cells(p, &[root]),
                Some(list) => set_cells(p, &[root, list.clone()]),
            }
        });
        continue_with(&bytes, mutation);
    }
    fn continue_with(bytes: &[u8; BASE_BYTES], mutation: usize) {
        let layout = decode_layout(bytes, expected()).unwrap();
        assert_eq!(
            current_tree::observe(layout, 0).err(),
            Some(BaseError::IdentityMismatch),
            "mutation {mutation}"
        );
        assert!(decode(bytes, expected(), &mut scratch()).is_err());
    }
}

/// A valid page/checksum is insufficient: retirement must belong to the
/// Structural history, even when a later capture admits a newer anchor LSN.
#[test]
fn unl_review_retirement_cannot_exceed_structural_high_water() {
    for source in [UNLINK_LEAF, UNLINK_COLLAPSE] {
        let mut bytes = backing(source);
        // Advance only the capture (and its inventory binding), not the
        // Structural bank/high-water at 90. No fixture author is invoked.
        put(&mut *bytes, 96, 91);
        put(&mut *bytes, 248, 91);
        let inventory_len =
            u32::from_le_bytes(bytes[INVENTORY + 12..INVENTORY + 16].try_into().unwrap()) as usize;
        put(&mut *bytes, INVENTORY + 48, 91);
        // The inventory includes the nested Row inventory at this fixed offset.
        if bytes[INVENTORY + 64..INVENTORY + 72]
            .iter()
            .any(|b| *b != 0)
        {
            let nested = INVENTORY + 2176;
            let nested_len =
                u32::from_le_bytes(bytes[nested + 12..nested + 16].try_into().unwrap()) as usize;
            put(&mut *bytes, nested + 48, 91);
            seal(&mut bytes[nested..nested + nested_len]);
            let digest = content_digest(&bytes[nested..nested + nested_len]);
            bytes[INVENTORY + 88..INVENTORY + 120].copy_from_slice(&digest);
        }
        seal(&mut bytes[INVENTORY..INVENTORY + inventory_len]);
        change_page(&mut bytes, 10, |p| {
            put(p, 64, 91);
        });
        // A later anchor with the original retirement is still valid.
        assert_eq!(decode(&*bytes, expected(), &mut scratch()).err(), None);
        change_page(&mut bytes, 10, |p| {
            let at = offset(p, 1);
            put(p, at + 16 + 32, 91);
        });
        let layout = decode_layout(&*bytes, expected()).unwrap();
        assert_eq!(layout.header().system_high_water[1], 90);
        assert_eq!(
            current_tree::observe(layout, 0).err(),
            Some(BaseError::IdentityMismatch),
        );
        assert!(decode(&*bytes, expected(), &mut scratch()).is_err());
    }
}
