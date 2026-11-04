use super::*;
use crate::{
    api::DatabaseId,
    storage::{
        key::encode_physical_key,
        page::{self, FileId, PAGE_BYTES, PageLsn},
    },
};
use foundation::{
    identity::{Generation, ObjectId, RowId},
    value::Value,
};
use std::collections::BTreeMap;

const FAMILIES: [(PageKind, PageKind, u8); 3] = [
    (PageKind::RowInternal, PageKind::RowLeaf, 0),
    (PageKind::CatalogInternal, PageKind::CatalogLeaf, 0),
    (PageKind::IndexInternal, PageKind::IndexLeaf, 2),
];
const PREORDER: [u64; 10] = [100, 110, 111, 112, 120, 121, 122, 130, 131, 132];
fn reference(number: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0002).unwrap(),
        page_no: number,
        incarnation: Generation::new(1).unwrap(),
    }
}
fn expected(kind: PageKind, arity: u8, level: u8, number: u64) -> TreeExpectation {
    TreeExpectation {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: reference(number),
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(1).unwrap(),
        },
        kind,
        arity,
        level,
    }
}
fn key(arity: u8, number: u64) -> Vec<u8> {
    if arity == 0 {
        return number.to_be_bytes().to_vec();
    }
    // Typed index order is (number/40, NULL, RowId). Distinct RowIds within a
    // composite tie and composite boundaries both occur in the authored tree.
    let mut buffer = [0; 64];
    let physical = encode_physical_key(
        &[Value::Int64((number / 40) as i64), Value::Null],
        RowId::new(number).unwrap(),
        &mut buffer,
    )
    .unwrap();
    let len = physical.composite().as_bytes().len() + 8;
    buffer[..len].to_vec()
}
// Product codecs prepare only source inputs. Expected topology, traversal,
// range failures and callback counts are authored independently below.
fn page(expect: TreeExpectation, left: Option<PageRef>, entries: &[(u64, PageRef)]) -> Vec<u8> {
    let mut metadata = [0; 32];
    encode_metadata(expect.kind, expect.arity, left, &mut metadata).unwrap();
    let mut cells = vec![metadata.to_vec()];
    for &(number, child) in entries {
        let mut buffer = [0; 128];
        let query = key(expect.arity, number);
        let cell = if expect.level == 0 {
            encode_leaf_cell(
                expect.kind,
                expect.arity,
                &query,
                &number.to_le_bytes(),
                &mut buffer,
            )
            .unwrap()
        } else {
            encode_internal_cell(expect.kind, expect.arity, &query, child, &mut buffer).unwrap()
        };
        cells.push(cell.to_vec());
    }
    let refs: Vec<_> = cells.iter().map(Vec::as_slice).collect();
    let mut bytes = vec![0; PAGE_BYTES];
    encode_tree_page(
        PageHeader {
            identity: expect.identity,
            kind: expect.kind,
            level: expect.level,
            lsn: PageLsn(7),
        },
        expect.arity,
        &refs,
        &mut bytes,
    )
    .unwrap();
    bytes
}
fn branch(expect: TreeExpectation, left: u64, entries: &[(u64, u64)]) -> Vec<u8> {
    page(
        expect,
        Some(reference(left)),
        &entries
            .iter()
            .map(|(key, child)| (*key, reference(*child)))
            .collect::<Vec<_>>(),
    )
}
fn leaf(expect: TreeExpectation, keys: &[u64]) -> Vec<u8> {
    page(
        expect,
        None,
        &keys
            .iter()
            .map(|key| (*key, reference(0)))
            .collect::<Vec<_>>(),
    )
}
fn tree(internal: PageKind, leaf_kind: PageKind, arity: u8) -> BTreeMap<u64, Vec<u8>> {
    let mut images = BTreeMap::new();
    for (number, level, left, entries) in [
        (100, 2, 110, vec![(40, 120), (80, 130)]),
        (110, 1, 111, vec![(20, 112)]),
        (120, 1, 121, vec![(60, 122)]),
        (130, 1, 131, vec![(100, 132)]),
    ] {
        images.insert(
            number,
            branch(expected(internal, arity, level, number), left, &entries),
        );
    }
    for (number, keys) in [
        (111, [10, 15]),
        (112, [20, 30]),
        (121, [40, 50]),
        (122, [60, 70]),
        (131, [80, 90]),
        (132, [100, 110]),
    ] {
        images.insert(number, leaf(expected(leaf_kind, arity, 0, number), &keys));
    }
    images
}
fn audit(
    root: TreeExpectation,
    images: &BTreeMap<u64, Vec<u8>>,
    workspace: &mut [Option<TreeExpectation>],
    calls: &mut Vec<u64>,
) -> Result<usize, TreeError> {
    audit_tree_in_images(root, workspace, |request| {
        calls.push(request.identity.page.page_no);
        images
            .get(&request.identity.page.page_no)
            .map(Vec::as_slice)
            .ok_or(TreeError::Corrupt)
    })
}

#[test]
fn immutable_tree_audit_visits_all_families_in_exact_preorder() {
    for (internal, leaf, arity) in FAMILIES {
        let images = tree(internal, leaf, arity);
        let root = expected(internal, arity, 2, 100);
        let mut workspace = [Some(root); 13];
        let mut calls = vec![];
        assert_eq!(audit(root, &images, &mut workspace, &mut calls), Ok(10));
        assert_eq!(calls, PREORDER);
        let visited: Vec<_> = workspace[..10]
            .iter()
            .map(|entry| entry.unwrap().identity.page.page_no)
            .collect();
        assert_eq!(visited, PREORDER);
        assert_eq!(workspace[10..], [None; 3]);
        for node in workspace[..10].iter().flatten() {
            assert_eq!(node.arity, arity);
            assert_eq!(node.identity.owner, root.identity.owner);
            let (kind, level) = if node.identity.page.page_no == 100 {
                (internal, 2)
            } else if [110, 120, 130].contains(&node.identity.page.page_no) {
                (internal, 1)
            } else {
                (leaf, 0)
            };
            assert_eq!((node.kind, node.level), (kind, level));
        }
    }
}

#[test]
fn immutable_tree_audit_rejects_off_path_bounds_and_identity() {
    for (internal, leaf_kind, arity) in FAMILIES {
        // Lower equality is valid in the normal tree; upper equality is not.
        for (number, keys) in [
            (121, [39, 50]),
            (121, [40, 60]),
            (122, [60, 80]),
            (131, [79, 90]),
        ] {
            let mut images = tree(internal, leaf_kind, arity);
            images.insert(number, leaf(expected(leaf_kind, arity, 0, number), &keys));
            assert_eq!(
                audit(
                    expected(internal, arity, 2, 100),
                    &images,
                    &mut [None; 10],
                    &mut vec![]
                ),
                Err(TreeError::Corrupt)
            );
        }
        for wrong in 0..3 {
            let mut images = tree(internal, leaf_kind, arity);
            let mut e = expected(leaf_kind, arity, 0, 132);
            match wrong {
                0 => e.identity.owner = ObjectId::new(6).unwrap(),
                1 => e.identity.page.incarnation = Generation::new(2).unwrap(),
                _ => e.identity.owner_generation = Generation::new(2).unwrap(),
            }
            images.insert(132, leaf(e, &[100, 110]));
            assert!(
                audit(
                    expected(internal, arity, 2, 100),
                    &images,
                    &mut [None; 10],
                    &mut vec![]
                )
                .is_err()
            );
        }
        let mut images = tree(internal, leaf_kind, arity);
        images.insert(
            120,
            branch(expected(internal, arity, 1, 120), 121, &[(80, 122)]),
        );
        assert_eq!(
            audit(
                expected(internal, arity, 2, 100),
                &images,
                &mut [None; 10],
                &mut vec![]
            ),
            Err(TreeError::Corrupt)
        );
        let mut images = tree(internal, leaf_kind, arity);
        images.insert(
            132,
            branch(expected(internal, arity, 1, 132), 201, &[(105, 202)]),
        );
        assert!(
            audit(
                expected(internal, arity, 2, 100),
                &images,
                &mut [None; 10],
                &mut vec![]
            )
            .is_err()
        );
        // A conservative separator need not equal the current subtree minimum.
        let mut images = tree(internal, leaf_kind, arity);
        images.insert(121, leaf(expected(leaf_kind, arity, 0, 121), &[41, 50]));
        assert_eq!(
            audit(
                expected(internal, arity, 2, 100),
                &images,
                &mut [None; 10],
                &mut vec![]
            ),
            Ok(10)
        );
    }
}

#[test]
fn immutable_tree_audit_rejects_shared_children_and_incarnation_aliases_before_load() {
    let (internal, leaf, arity) = FAMILIES[0];
    let root = expected(internal, arity, 2, 100);
    for incarnation in [1, 2] {
        let mut images = tree(internal, leaf, arity);
        let mut shared = reference(112);
        shared.incarnation = Generation::new(incarnation).unwrap();
        images.insert(
            120,
            page(
                expected(internal, arity, 1, 120),
                Some(shared),
                &[(60, reference(122))],
            ),
        );
        let mut calls = vec![];
        assert_eq!(
            audit(root, &images, &mut [None; 10], &mut calls),
            Err(TreeError::Corrupt)
        );
        assert_eq!(calls, [100, 110, 111, 112, 120]);
        let mut images = tree(internal, leaf, arity);
        let mut ancestor = reference(100);
        ancestor.incarnation = Generation::new(incarnation).unwrap();
        images.insert(
            110,
            page(
                expected(internal, arity, 1, 110),
                Some(ancestor),
                &[(20, reference(112))],
            ),
        );
        let mut calls = vec![];
        assert_eq!(
            audit(root, &images, &mut [None; 10], &mut calls),
            Err(TreeError::Corrupt)
        );
        assert_eq!(calls, [100, 110]);
    }
}

#[test]
fn immutable_tree_audit_bounds_maximum_depth_and_accepts_empty_leaf() {
    for (internal, leaf_kind, arity) in FAMILIES {
        let mut images = BTreeMap::new();
        // Only the leftmost path exists in this fixture. Full height16 needs
        // at least65,535 pages; this is the capacity/error boundary, not that test.
        for depth in 0..15 {
            images.insert(
                100 + depth,
                branch(
                    expected(internal, arity, (15 - depth) as u8, 100 + depth),
                    101 + depth,
                    &[(1000 - depth, 200 + depth)],
                ),
            );
        }
        images.insert(115, leaf(expected(leaf_kind, arity, 0, 115), &[1]));
        let mut calls = vec![];
        assert_eq!(
            audit(
                expected(internal, arity, 15, 100),
                &images,
                &mut [None; 16],
                &mut calls
            ),
            Err(TreeError::NoSpace)
        );
        assert_eq!(calls, (100..116).collect::<Vec<_>>());
        let root = expected(leaf_kind, arity, 0, 1);
        let empty = leaf(root, &[]);
        let mut workspace = [Some(root); 2];
        assert_eq!(
            audit_tree_in_images(root, &mut workspace, |_| Ok(&empty)),
            Ok(1)
        );
        assert_eq!(workspace, [Some(root), None]);
        // A metadata-only internal page is a valid page envelope, invalid XBT1.
        let root = expected(internal, arity, 1, 1);
        let mut meta = [0; 32];
        encode_metadata(internal, arity, Some(reference(2)), &mut meta).unwrap();
        let mut unary = vec![0; PAGE_BYTES];
        page::encode_page(
            PageHeader {
                identity: root.identity,
                kind: internal,
                level: 1,
                lsn: PageLsn(7),
            },
            &[&meta],
            &mut unary,
        )
        .unwrap();
        assert_eq!(
            audit_tree_in_images(root, &mut [None; 2], |_| Ok(&unary)),
            Err(TreeError::Corrupt)
        );
    }
}

#[test]
fn immutable_tree_audit_checks_capacity_before_load_and_preserves_callback_error() {
    let (internal, leaf, arity) = FAMILIES[0];
    let root = expected(internal, arity, 2, 100);
    let images = tree(internal, leaf, arity);
    for capacity in 0..=10 {
        let mut calls = vec![];
        let mut workspace = vec![Some(root); capacity];
        let result = audit(root, &images, &mut workspace, &mut calls);
        assert_eq!(
            result,
            if capacity == 10 {
                Ok(10)
            } else {
                Err(TreeError::NoSpace)
            }
        );
        assert_eq!(calls, PREORDER[..capacity]);
    }
    let mut calls = 0;
    assert_eq!(
        audit_tree_in_images(
            TreeExpectation { level: 16, ..root },
            &mut [None; 10],
            |_| {
                calls += 1;
                Err(TreeError::Corrupt)
            }
        ),
        Err(TreeError::InvalidInput)
    );
    assert_eq!(calls, 0);
    let mut calls = vec![];
    assert_eq!(
        audit_tree_in_images(root, &mut [None; 10], |e| {
            calls.push(e.identity.page.page_no);
            if calls.len() == 5 {
                Err(TreeError::NoSpace)
            } else {
                Ok(images.get(&e.identity.page.page_no).unwrap().as_slice())
            }
        }),
        Err(TreeError::NoSpace)
    );
    assert_eq!(calls, PREORDER[..5]);
}

#[test]
fn immutable_tree_audit_distinguishes_file_identity_and_unreachable_owned_pages() {
    let (internal, leaf_kind, arity) = FAMILIES[0];
    let root = expected(internal, arity, 1, 1);
    let left = expected(leaf_kind, arity, 0, 2);
    let mut right = left;
    right.identity.page.file = FileId::new(0x1_0000_0003).unwrap();
    let parent = page(root, Some(left.identity.page), &[(20, right.identity.page)]);
    let left_bytes = leaf(left, &[10]);
    let right_bytes = leaf(right, &[20]);
    let mut calls = vec![];
    let mut workspace = [None; 3];
    assert_eq!(
        audit_tree_in_images(root, &mut workspace, |e| {
            calls.push(e.identity.page);
            if e == root {
                Ok(&parent)
            } else if e == left {
                Ok(&left_bytes)
            } else if e == right {
                Ok(&right_bytes)
            } else {
                Err(TreeError::Corrupt)
            }
        }),
        Ok(3)
    );
    assert_eq!(
        calls,
        [root.identity.page, left.identity.page, right.identity.page]
    );
    let root = expected(internal, arity, 2, 100);
    let mut images = tree(internal, leaf_kind, arity);
    images.insert(999, leaf(expected(leaf_kind, arity, 0, 999), &[1]));
    let mut workspace = [None; 11];
    let mut calls = vec![];
    assert_eq!(audit(root, &images, &mut workspace, &mut calls), Ok(10));
    assert!(!calls.contains(&999));
    assert_eq!(workspace[10], None);
    // The canonical caller, not this reachable-tree function, compares with ownership.
    let missing: Vec<_> = images
        .keys()
        .filter(|page| !calls.contains(page))
        .copied()
        .collect();
    assert_eq!(missing, [999]);
}
