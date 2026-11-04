use super::*;
use crate::{
    api::DatabaseId,
    storage::page::{FileId, PAGE_BYTES, PageLsn},
};
use foundation::identity::{Generation, ObjectId};
use std::collections::BTreeMap;

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
    let mut key = if arity == 0 { vec![] } else { vec![0] };
    key.extend(number.to_be_bytes());
    key
}
// Existing independently verified codecs build source pages only. Expected
// search results are the authored typed key/page map, never codec output.
fn page(expect: TreeExpectation, left: Option<PageRef>, entries: &[(u64, u64)]) -> Vec<u8> {
    let mut meta = [0; 32];
    encode_metadata(expect.kind, expect.arity, left, &mut meta).unwrap();
    let mut cells = vec![meta.to_vec()];
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
            encode_internal_cell(
                expect.kind,
                expect.arity,
                &query,
                reference(child),
                &mut buffer,
            )
            .unwrap()
        };
        cells.push(cell.to_vec());
    }
    let refs: Vec<&[u8]> = cells.iter().map(Vec::as_slice).collect();
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
const FAMILIES: [(PageKind, PageKind, u8); 3] = [
    (PageKind::RowInternal, PageKind::RowLeaf, 0),
    (PageKind::CatalogInternal, PageKind::CatalogLeaf, 0),
    (PageKind::IndexInternal, PageKind::IndexLeaf, 1),
];
fn tree(internal: PageKind, leaf: PageKind, arity: u8) -> BTreeMap<u64, Vec<u8>> {
    let mut images = BTreeMap::new();
    for (number, level, left, entries) in [
        (100, 2, 110, vec![(40, 120), (80, 130)]),
        (110, 1, 111, vec![(20, 112)]),
        (120, 1, 121, vec![(60, 122)]),
        (130, 1, 131, vec![(100, 132)]),
    ] {
        images.insert(
            number,
            page(
                expected(internal, arity, level, number),
                Some(reference(left)),
                &entries,
            ),
        );
    }
    for (number, entries) in [
        (111, vec![(10, 0), (15, 0)]),
        (112, vec![(20, 0), (30, 0)]),
        (121, vec![(40, 0), (50, 0)]),
        (122, vec![(60, 0), (70, 0)]),
        (131, vec![(80, 0), (90, 0)]),
        (132, vec![(100, 0), (110, 0)]),
    ] {
        images.insert(
            number,
            page(expected(leaf, arity, 0, number), None, &entries),
        );
    }
    images
}
#[test]
fn immutable_tree_search_matches_typed_points_and_exact_path() {
    for (internal, leaf, arity) in FAMILIES {
        let images = tree(internal, leaf, arity);
        let present = [10u64, 15, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110];
        for query in 1..=120 {
            let (parent, target) = match query {
                1..=19 => (110, 111),
                20..=39 => (110, 112),
                40..=59 => (120, 121),
                60..=79 => (120, 122),
                80..=99 => (130, 131),
                _ => (130, 132),
            };
            let mut path = Vec::new();
            let bytes = key(arity, query);
            let found = find_leaf_in_images(expected(internal, arity, 2, 100), &bytes, |request| {
                path.push((
                    request.identity.page.page_no,
                    request.kind,
                    request.level,
                    request.arity,
                ));
                images
                    .get(&request.identity.page.page_no)
                    .map(Vec::as_slice)
                    .ok_or(TreeError::Corrupt)
            })
            .unwrap();
            assert_eq!(
                path,
                vec![
                    (100, internal, 2, arity),
                    (parent, internal, 1, arity),
                    (target, leaf, 0, arity)
                ]
            );
            let position = found.lower_bound(&bytes).unwrap();
            assert_eq!(position.equal, present.contains(&query));
            if position.equal {
                assert_eq!(
                    found.entry(position.index).unwrap().payload,
                    EntryPayload::Value(&query.to_le_bytes())
                );
            }
        }
    }
}
#[test]
fn immutable_tree_search_rejects_inherited_range_violations() {
    for (internal, leaf, arity) in FAMILIES {
        for (number, level, left, entries, query) in [
            (121, 0, None, vec![(39, 0), (50, 0)], 50), // lower bound 40
            (121, 0, None, vec![(40, 0), (60, 0)], 50), // upper bound 60
            (122, 0, None, vec![(60, 0), (80, 0)], 70), // inherited upper 80
            (120, 1, Some(reference(121)), vec![(39, 122)], 50), // ancestor lower 40
            (120, 1, Some(reference(121)), vec![(80, 122)], 50), // ancestor upper 80
            (111, 0, None, vec![(1, 0), (20, 0)], 1),   // propagated upper 20
        ] {
            let mut images = tree(internal, leaf, arity);
            images.insert(
                number,
                page(
                    expected(
                        if level == 0 { leaf } else { internal },
                        arity,
                        level,
                        number,
                    ),
                    left,
                    &entries,
                ),
            );
            assert!(matches!(
                find_leaf_in_images(
                    expected(internal, arity, 2, 100),
                    &key(arity, query),
                    |request| Ok(images
                        .get(&request.identity.page.page_no)
                        .unwrap()
                        .as_slice())
                ),
                Err(TreeError::Corrupt)
            ));
        }
    }
}
#[test]
fn immutable_tree_search_rejects_wrong_child_and_ancestor_alias() {
    let (internal, leaf, arity) = FAMILIES[0];
    let mut images = tree(internal, leaf, arity);
    let root = expected(internal, arity, 2, 100);
    let mut loads = 0;
    assert!(matches!(
        find_leaf_in_images(root, &10u64.to_be_bytes(), |request| {
            loads += 1;
            if loads == 2 {
                Err(TreeError::NoSpace)
            } else {
                Ok(images
                    .get(&request.identity.page.page_no)
                    .unwrap()
                    .as_slice())
            }
        }),
        Err(TreeError::NoSpace)
    ));
    assert_eq!(loads, 2);
    // A valid page from another child must not supply its own expectation.
    assert!(
        find_leaf_in_images(root, &10u64.to_be_bytes(), |request| {
            let number = if request.identity.page.page_no == 111 {
                112
            } else {
                request.identity.page.page_no
            };
            Ok(images.get(&number).unwrap().as_slice())
        })
        .is_err()
    );
    for incarnation in [1, 2] {
        let mut ancestor = reference(100);
        ancestor.incarnation = Generation::new(incarnation).unwrap();
        images.insert(
            110,
            page(expected(internal, 0, 1, 110), Some(ancestor), &[(20, 112)]),
        );
        let mut loads = 0;
        assert!(matches!(
            find_leaf_in_images(root, &10u64.to_be_bytes(), |request| {
                loads += 1;
                Ok(images
                    .get(&request.identity.page.page_no)
                    .unwrap()
                    .as_slice())
            }),
            Err(TreeError::Corrupt)
        ));
        assert_eq!(loads, 2); // do not ask the source to read the ancestor again
    }
}
#[test]
fn immutable_tree_search_bounds_height_and_rejects_bad_input_before_loading() {
    for (internal, leaf, arity) in FAMILIES {
        let mut images = BTreeMap::new();
        // A 16-page selected path. Off-path children are deliberately absent;
        // this unit checks path search, not whole-tree reachability/audit.
        for depth in 0..15 {
            images.insert(
                100 + depth,
                page(
                    expected(internal, arity, (15 - depth) as u8, 100 + depth),
                    Some(reference(101 + depth)),
                    &[(1000 - depth, 200 + depth)],
                ),
            );
        }
        images.insert(115, page(expected(leaf, arity, 0, 115), None, &[(1, 0)]));
        let mut loads = 0;
        let found = find_leaf_in_images(
            expected(internal, arity, 15, 100),
            &key(arity, 1),
            |request| {
                loads += 1;
                Ok(images
                    .get(&request.identity.page.page_no)
                    .unwrap()
                    .as_slice())
            },
        )
        .unwrap();
        assert_eq!(loads, 16);
        assert_eq!(found.header().identity.page, reference(115));
        for (root, query) in [
            (expected(internal, arity, 16, 100), key(arity, 1)),
            (expected(internal, arity, 15, 100), key(arity, 0)),
        ] {
            let mut loads = 0;
            assert!(matches!(
                find_leaf_in_images(root, &query, |_| {
                    loads += 1;
                    Err(TreeError::Corrupt)
                }),
                Err(TreeError::InvalidInput)
            ));
            assert_eq!(loads, 0);
        }
        let root = expected(leaf, arity, 0, 10);
        let empty = page(root, None, &[]);
        assert_eq!(
            find_leaf_in_images(root, &key(arity, 1), |_| Ok(&empty))
                .unwrap()
                .len(),
            0
        );
    }
}
