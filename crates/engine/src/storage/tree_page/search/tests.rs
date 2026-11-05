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

/// The live page-at-a-time route must agree with an independently authored
/// partition map, including gaps and exact separators at both internal levels.
#[test]
fn row_route_steps_match_partition_model_and_inherited_bounds() {
    let images = tree(PageKind::RowInternal, PageKind::RowLeaf, 0);
    for query in 1u64..=120 {
        let (parent, target, lower, upper) = match query {
            1..=19 => (110, 111, None, Some(20u64)),
            20..=39 => (110, 112, Some(20u64), Some(40)),
            40..=59 => (120, 121, Some(40), Some(60)),
            60..=79 => (120, 122, Some(60), Some(80)),
            80..=99 => (130, 131, Some(80), Some(100)),
            _ => (130, 132, Some(100), None),
        };
        let mut bounds = RowRouteBounds::default();
        let mut request = expected(PageKind::RowInternal, 0, 2, 100);
        for (number, level) in [(100, 2), (parent, 1), (target, 0)] {
            assert_eq!(
                (request.identity.page.page_no, request.level),
                (number, level)
            );
            let next = route_row_step(&images[&number], request, &query.to_be_bytes(), &mut bounds)
                .unwrap();
            if level == 0 {
                assert_eq!(next, None);
            } else {
                request = next.unwrap();
            }
        }
        assert_eq!(
            bounds,
            RowRouteBounds {
                lower: lower.map(u64::to_be_bytes),
                upper: upper.map(u64::to_be_bytes),
            }
        );
        // The leaf at [40,60) cannot contain a key below its inherited lower
        // bound or equal to its upper bound, even if locally sorted and valid.
        if target == 121 {
            for entries in [&[(39, 0), (50, 0)][..], &[(40, 0), (60, 0)]] {
                let corrupt = page(request, None, entries);
                assert_eq!(
                    route_row_step(&corrupt, request, &query.to_be_bytes(), &mut bounds),
                    Err(TreeError::Corrupt)
                );
            }
        }
    }
}
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
        if internal == PageKind::RowInternal {
            // A live walk retains only copied bounds. Check every supported
            // Row height against the authored decreasing-level path.
            for height in 0..16u8 {
                let mut request = expected(
                    if height == 0 { leaf } else { internal },
                    0,
                    height,
                    115 - u64::from(height),
                );
                let mut bounds = RowRouteBounds::default();
                for remaining in (0..=height).rev() {
                    assert_eq!(request.level, remaining);
                    assert_eq!(request.identity.page, reference(115 - u64::from(remaining)));
                    let child = route_row_step(
                        &images[&request.identity.page.page_no],
                        request,
                        &1u64.to_be_bytes(),
                        &mut bounds,
                    )
                    .unwrap();
                    if remaining == 0 {
                        assert_eq!(child, None);
                    } else {
                        request = child.unwrap();
                    }
                }
                assert_eq!(bounds.lower, None);
                assert_eq!(bounds.upper, (height > 0).then(|| 986u64.to_be_bytes()));
            }
        }
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

/// SPL-A07 (dev3-tree-smo.md#automatic-split): the live split's separator
/// room is the codec's own bound. A RowInternal page of `n` separators takes
/// one more exactly when a page of `n + 1` encodes, at the boundary and one
/// past it; one Row separator costs its cell and slot.
#[test]
fn spl_a07_separator_room_is_the_codec_bound() {
    let expect = expected(PageKind::RowInternal, 0, 1, 9);
    let mut buffer = [0; 128];
    let cell = encode_internal_cell(
        PageKind::RowInternal,
        0,
        &key(0, 1),
        reference(10),
        &mut buffer,
    )
    .unwrap();
    assert_eq!(cell.len() + 4, ROW_SEPARATOR_COST);
    let fits = |count: u64| {
        let mut meta = [0; 32];
        encode_metadata(expect.kind, 0, Some(reference(10)), &mut meta).unwrap();
        let mut cells = vec![meta.to_vec()];
        for number in 1..=count {
            let mut buffer = [0; 128];
            let cell = encode_internal_cell(
                expect.kind,
                0,
                &key(0, number),
                reference(10 + number),
                &mut buffer,
            )
            .unwrap();
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
            0,
            &refs,
            &mut bytes,
        )
        .is_ok()
    };
    let last = TREE_ENTRY_CAPACITY / ROW_SEPARATOR_COST;
    for entries in [0, 1, last - 2, last - 1, last] {
        assert_eq!(
            separator_room(entries),
            fits(entries as u64 + 1),
            "{entries} separators"
        );
    }
    assert!(separator_room(last - 1) && !separator_room(last));
    assert!(!separator_room(usize::MAX));
}

/// SPL-A07: the split target over a route's room flags. From the leaf up,
/// the first page whose parent takes a separator; else the root.
#[test]
fn spl_a07_split_target_takes_the_lowest_page_whose_parent_has_room() {
    assert_eq!(split_target(&[]), None);
    // A root leaf: a RootSplit.
    assert_eq!(split_target(&[false]), Some(0));
    assert_eq!(split_target(&[true]), Some(0));
    // A leaf under a root with room: a ChildSplit of the leaf.
    assert_eq!(split_target(&[true, false]), Some(1));
    // Its root is full: the root first.
    assert_eq!(split_target(&[false, true]), Some(0));
    // Three levels: the leaf, its full parent, then a full root.
    assert_eq!(split_target(&[true, true, false]), Some(2));
    assert_eq!(split_target(&[true, false, false]), Some(1));
    assert_eq!(split_target(&[false, false, true]), Some(0));
    // The leaf's own flag is never read.
    assert_eq!(split_target(&[true, true, true]), Some(2));
}
