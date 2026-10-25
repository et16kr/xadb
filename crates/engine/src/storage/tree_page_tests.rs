use super::*;
use crate::{
    api::DatabaseId,
    storage::{
        key::{MAX_PHYSICAL_KEY_BYTES, encode_physical_key},
        page::{FileId, PAGE_BYTES, PageLsn},
    },
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId, RowId},
    value::Value,
};

const FAMILIES: [(PageKind, PageKind, u8); 3] = [
    (PageKind::RowLeaf, PageKind::RowInternal, 0),
    (PageKind::IndexLeaf, PageKind::IndexInternal, 1),
    (PageKind::CatalogLeaf, PageKind::CatalogInternal, 0),
];

fn reference(page_no: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x0000_0001_0000_0002).unwrap(),
        page_no,
        incarnation: Generation::new(3).unwrap(),
    }
}

fn header(kind: PageKind, level: u8) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: DatabaseId::new([0x11; 16]).unwrap(),
            page: PageRef {
                incarnation: Generation::new(4).unwrap(),
                ..reference(3)
            },
            owner: ObjectId::new(5).unwrap(),
            owner_generation: Generation::new(6).unwrap(),
        },
        kind,
        level,
        lsn: PageLsn(7),
    }
}

fn expected(header: PageHeader, arity: u8) -> TreeExpectation {
    TreeExpectation {
        identity: header.identity,
        kind: header.kind,
        level: header.level,
        arity,
    }
}

fn hex(input: &str) -> Vec<u8> {
    assert_eq!(input.len() % 2, 0);
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn views(cells: &[Vec<u8>]) -> Vec<&[u8]> {
    cells.iter().map(Vec::as_slice).collect()
}

// Test inputs are assembled from the frozen wire offsets, independently of the
// tree encoder. PageRef itself has its own independent reference-format tests.
fn metadata(arity: u8, leftmost: Option<PageRef>) -> Vec<u8> {
    let mut bytes = hex("5842543101000000000000000000000000000000000000000000000000000000");
    bytes[6] = arity;
    if let Some(child) = leftmost {
        child.encode(&mut bytes[8..]).unwrap();
    }
    bytes
}

fn leaf_cell(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut bytes = (key.len() as u16).to_le_bytes().to_vec();
    bytes.extend((value.len() as u16).to_le_bytes());
    bytes.extend(key);
    bytes.extend(value);
    bytes
}

fn internal_cell(key: &[u8], right: PageRef) -> Vec<u8> {
    let mut bytes = (key.len() as u16).to_le_bytes().to_vec();
    bytes.extend([0, 0]);
    bytes.extend(key);
    let mut child = [0; 24];
    right.encode(&mut child).unwrap();
    bytes.extend(child);
    bytes
}

fn physical(values: &[Value], row: u64) -> Vec<u8> {
    let mut bytes = [0; MAX_PHYSICAL_KEY_BYTES];
    let key = encode_physical_key(values, RowId::new(row).unwrap(), &mut bytes).unwrap();
    let len = key.composite().as_bytes().len() + 8;
    bytes[..len].to_vec()
}

fn number_key(arity: u8, number: u64) -> Vec<u8> {
    if arity == 0 {
        number.to_be_bytes().to_vec()
    } else {
        // This helper's ordinal domain is small and positive.
        physical(&[Value::Int64(number as i64)], 1)
    }
}

fn page(header: PageHeader, arity: u8, cells: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0; PAGE_BYTES];
    encode_tree_page(header, arity, &views(cells), &mut bytes).unwrap();
    bytes
}

fn golden_cells(index: bool, internal: bool) -> Vec<Vec<u8>> {
    match (index, internal) {
        (false, false) => vec![
            hex("5842543101000000000000000000000000000000000000000000000000000000"),
            hex("08000100000000000000000141"),
            hex("08000200000000000000010200ff"),
        ],
        (true, false) => vec![
            hex("5842543101000100000000000000000000000000000000000000000000000000"),
            hex("0900010000000000000000000141"),
            hex("0a0002000100000000000000010200ff"),
        ],
        (false, true) => vec![
            hex("5842543101000000020000000100000008000000000000000300000000000000"),
            hex("080000000000000000000102020000000100000009000000000000000300000000000000"),
        ],
        (true, true) => vec![
            hex("5842543101000100020000000100000008000000000000000300000000000000"),
            hex("0a00000001000000000000000102020000000100000009000000000000000300000000000000"),
        ],
    }
}

// Independent envelope construction using literal offsets and frozen digest,
// without the production page/tree encoders or their checksum routine.
fn golden_page(kind: u8, level: u8, cells: &[Vec<u8>], checksum: &str) -> Vec<u8> {
    let mut bytes = vec![0; 16_384];
    bytes[..8].copy_from_slice(&[b'X', b'D', b'P', b'1', 1, 0, kind, level]);
    bytes[8..24].fill(0x11);
    for (offset, value) in [
        (24, 0x0000_0001_0000_0002u64),
        (32, 3),
        (40, 4),
        (48, 5),
        (56, 6),
        (64, 7),
    ] {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes[72..74].copy_from_slice(&(cells.len() as u16).to_le_bytes());
    bytes[74..76].copy_from_slice(&(128u16 + 4 * cells.len() as u16).to_le_bytes());
    let mut end = 16_352;
    for (slot, cell) in cells.iter().enumerate() {
        let start = end - cell.len();
        bytes[128 + 4 * slot..130 + 4 * slot].copy_from_slice(&(start as u16).to_le_bytes());
        bytes[130 + 4 * slot..132 + 4 * slot].copy_from_slice(&(cell.len() as u16).to_le_bytes());
        bytes[start..end].copy_from_slice(cell);
        end = start;
    }
    bytes[76..78].copy_from_slice(&(end as u16).to_le_bytes());
    bytes[16_352..].copy_from_slice(&hex(checksum));
    bytes
}

#[test]
fn tree_golden_pages_and_cells() {
    let hashes = [
        (
            "eca84cc85e166ede52841deb22d27397ff50cf6e52d35cee19d3f7137b0f77ae",
            "c4de9a633e8021cb256f4a0254718c41f2854fdc39fe0c883e0dcb8e3cfa91ef",
        ),
        (
            "bd30079dd665fd4bd585c36a8b97138838befba5e2c960da3da331c8a1d38c4b",
            "295d4f7e4275b9024d3ece4cee9465d23f6b6259104c0aaa860b859974f95c2c",
        ),
        (
            "1fd2e7bca17bb41e5f858bb661ab8eee07bce9abb098ba7811e38aa8245315c1",
            "d2dd2a32ffa8b0b8eb3fa8c145cdc540467166af5ff729898bece589eb1d2f9d",
        ),
        (
            "565b5b5b8061fbd6b08b0d427efc28c968f2000639dc66a25a1391f2edc3d93f",
            "5d46cf75a9c53671e37ac99baec885e3c4e9c7def4d9ee5c155910667584bd1a",
        ),
        (
            "0b04e62e52ea32a6846057edd977dc936b3c8d45aac35dd76b8407a264ddca6c",
            "2c301c28ebc5ea5c191843cfdb29593e479a998352086bead3c2091afd11555c",
        ),
        (
            "977aefe78ea8f9ffeec46a69e513fbd4cfff13de573711e3a174c7da305c1147",
            "54a1664f6ae5e019cbbea67e8a1bcafc01cf06d2ccac1e7e25bca18ed786c636",
        ),
    ];
    let mut count = 0;
    for (leaf, internal, arity) in FAMILIES {
        for (kind, level) in [(leaf, 0), (internal, 1)] {
            let cells = golden_cells(arity == 1, level == 1);
            let expected_bytes = golden_page(kind as u8, level, &cells, hashes[count].0);
            assert_eq!(
                content_digest(&expected_bytes).as_slice(),
                hex(hashes[count].1)
            );
            let encoded = page(header(kind, level), arity, &cells);
            assert_eq!(encoded, expected_bytes);
            let decoded = decode_tree_page(&encoded, expected(header(kind, level), arity)).unwrap();
            assert_eq!(decoded.header(), header(kind, level));
            assert_eq!(decoded.len(), cells.len() - 1);
            let mut scratch = [0xa5; 128];
            assert_eq!(
                encode_metadata(
                    kind,
                    arity,
                    (level != 0).then(|| reference(8)),
                    &mut scratch
                )
                .unwrap(),
                cells[0]
            );
            for (index, cell) in cells.iter().enumerate().skip(1) {
                let entry = decoded.entry(index - 1).unwrap();
                let encoded = match entry.payload {
                    EntryPayload::Value(value) => {
                        encode_leaf_cell(kind, arity, entry.key, value, &mut scratch)
                    }
                    EntryPayload::RightChild(right) => {
                        encode_internal_cell(kind, arity, entry.key, right, &mut scratch)
                    }
                }
                .unwrap();
                assert_eq!(encoded, cell);
            }
            count += 1;
        }
        let empty = page(header(leaf, 0), arity, &[metadata(arity, None)]);
        assert_eq!(
            decode_tree_page(&empty, expected(header(leaf, 0), arity))
                .unwrap()
                .len(),
            0
        );
        let high = page(header(internal, 15), arity, &golden_cells(arity == 1, true));
        assert_eq!(
            decode_tree_page(&high, expected(header(internal, 15), arity))
                .unwrap()
                .header()
                .level,
            15
        );
    }
    assert_eq!(count, 6);
}

fn ordered_scalars() -> Vec<Value> {
    vec![
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int64(i64::MIN),
        Value::Int64(-2),
        Value::Int64(-1),
        Value::Int64(0),
        Value::Int64(1),
        Value::Int64(i64::MAX),
        Value::Utf8("".into()),
        Value::Utf8("\0".into()),
        Value::Utf8("a".into()),
        Value::Utf8("a\0".into()),
        Value::Utf8("é".into()),
        Value::Bytes(vec![]),
        Value::Bytes(vec![0]),
        Value::Bytes(vec![0, 0]),
        Value::Bytes(vec![0, 255]),
        Value::Bytes(vec![1]),
        Value::Bytes(vec![255]),
    ]
}

#[test]
fn tree_leaf_lower_bound_matches_typed_oracle() {
    let ids = [1u64, 3, 9, 258, u64::MAX];
    for kind in [PageKind::RowLeaf, PageKind::CatalogLeaf] {
        let mut cells = vec![metadata(0, None)];
        cells.extend(ids.map(|id| leaf_cell(&id.to_be_bytes(), &[])));
        let bytes = page(header(kind, 0), 0, &cells);
        let view = decode_tree_page(&bytes, expected(header(kind, 0), 0)).unwrap();
        for query in [1u64, 2, 3, 4, 9, 10, 257, 258, 259, u64::MAX] {
            let oracle = ids
                .iter()
                .position(|key| *key >= query)
                .unwrap_or(ids.len());
            assert_eq!(
                view.lower_bound(&query.to_be_bytes()).unwrap(),
                SearchPosition {
                    index: oracle,
                    equal: ids.get(oracle) == Some(&query),
                }
            );
        }
    }
    let scalars = ordered_scalars();
    assert_eq!(scalars.len(), 20);
    let mut cells = vec![metadata(1, None)];
    let mut ordinals = Vec::new();
    for (ordinal, scalar) in scalars.iter().enumerate() {
        for row in [1, 258, u64::MAX] {
            cells.push(leaf_cell(&physical(std::slice::from_ref(scalar), row), &[]));
            ordinals.push((ordinal, row));
        }
    }
    assert_eq!(ordinals.len(), 60);
    let bytes = page(header(PageKind::IndexLeaf, 0), 1, &cells);
    let view = decode_tree_page(&bytes, expected(header(PageKind::IndexLeaf, 0), 1)).unwrap();
    let mut query_count = 0;
    for (ordinal, scalar) in scalars.iter().enumerate() {
        for row in [1, 2, 258, u64::MAX - 1, u64::MAX] {
            let tuple = (ordinal, row);
            let oracle = ordinals
                .iter()
                .position(|key| *key >= tuple)
                .unwrap_or(ordinals.len());
            let query = physical(std::slice::from_ref(scalar), row);
            assert_eq!(
                view.lower_bound(&query).unwrap(),
                SearchPosition {
                    index: oracle,
                    equal: ordinals.get(oracle) == Some(&tuple),
                }
            );
            query_count += 1;
        }
    }
    assert_eq!(query_count, 100); // Sixty exact and forty gap queries.
}

#[test]
fn tree_internal_routing_and_child_expectations() {
    for (leaf, internal, arity) in FAMILIES {
        for level in [1, 15] {
            let mut cells = vec![metadata(arity, Some(reference(8)))];
            cells.push(internal_cell(&number_key(arity, 10), reference(9)));
            cells.push(internal_cell(&number_key(arity, 20), reference(10)));
            let parent = header(internal, level);
            let bytes = page(parent, arity, &cells);
            let view = decode_tree_page(&bytes, expected(parent, arity)).unwrap();
            for (query, child) in [
                (1, 8),
                (9, 8),
                (10, 9),
                (11, 9),
                (19, 9),
                (20, 10),
                (21, 10),
            ] {
                assert_eq!(
                    view.route_child(&number_key(arity, query)).unwrap(),
                    reference(child)
                );
            }
            for index in 0..3 {
                let child = view.child_expectation(index).unwrap();
                assert_eq!(
                    child,
                    TreeExpectation {
                        identity: PageIdentity {
                            page: reference(8 + index as u64),
                            ..parent.identity
                        },
                        kind: if level == 1 { leaf } else { internal },
                        level: level - 1,
                        arity,
                    }
                );
            }
            assert!(view.child_expectation(3).is_err());
            assert!(view.child_expectation(usize::MAX).is_err());
            assert!(view.entry(usize::MAX).is_err());
        }
    }
}

fn reject_damage(header: PageHeader, arity: u8, cells: &[Vec<u8>]) {
    let mut bytes = vec![0; PAGE_BYTES];
    // The envelope transport recomputes checksum; it does not inspect tree cells.
    encode_page(header, &views(cells), &mut bytes).unwrap();
    assert!(decode_page(&bytes, header.identity, header.kind).is_ok());
    assert!(decode_tree_page(&bytes, expected(header, arity)).is_err());
}

#[test]
fn tree_rejects_checksummed_structural_damage() {
    let mut count = 0;
    for (leaf, internal, arity) in FAMILIES {
        for (kind, level) in [(leaf, 0), (internal, 1)] {
            let original = golden_cells(arity == 1, level == 1);
            for mutation in 0..8 {
                let mut cells = original.clone();
                match mutation {
                    0 => cells[0][0] ^= 1,
                    1 => cells[0][4] = 2,
                    2 => cells[0][7] = 1,
                    3 => cells[0][6] = if arity == 0 { 1 } else { 0 },
                    4 => cells[1][0] = cells[1][0].wrapping_add(1),
                    5 => {
                        cells[1].pop();
                    }
                    6 => cells[1].push(0),
                    7 => {
                        if arity == 0 {
                            cells[1][4..12].fill(0);
                        } else {
                            cells[1][4] = 0xff;
                        }
                    }
                    _ => unreachable!(),
                }
                reject_damage(header(kind, level), arity, &cells);
                count += 1;
            }
            for duplicate in [false, true] {
                let mut cells = if level == 0 {
                    vec![
                        metadata(arity, None),
                        leaf_cell(&number_key(arity, 1), &[]),
                        leaf_cell(&number_key(arity, 2), &[]),
                    ]
                } else {
                    vec![
                        metadata(arity, Some(reference(8))),
                        internal_cell(&number_key(arity, 1), reference(9)),
                        internal_cell(&number_key(arity, 2), reference(10)),
                    ]
                };
                if duplicate {
                    let len = usize::from(u16::from_le_bytes([cells[1][0], cells[1][1]]));
                    let key = cells[1][4..4 + len].to_vec();
                    cells[2][4..4 + len].copy_from_slice(&key);
                } else {
                    cells.swap(1, 2);
                }
                reject_damage(header(kind, level), arity, &cells);
                count += 1;
            }
        }
        let mut cells = golden_cells(arity == 1, false);
        cells[0][8] = 1;
        reject_damage(header(leaf, 0), arity, &cells);
        count += 1;
        for mutation in 0..5 {
            let mut cells = golden_cells(arity == 1, true);
            let end = cells[1].len();
            match mutation {
                0 => cells[1][2] = 1,
                1 => cells[1][end - 24..end - 16].fill(0),
                2 => cells[1][end - 8..end].fill(0),
                3 => header(internal, 1)
                    .identity
                    .page
                    .encode(&mut cells[1][end - 24..])
                    .unwrap(),
                4 => reference(8).encode(&mut cells[1][end - 24..]).unwrap(),
                _ => unreachable!(),
            }
            reject_damage(header(internal, 1), arity, &cells);
            count += 1;
        }
    }
    assert_eq!(count, 78);
}

#[test]
fn tree_enforces_entry_page_and_output_limits() {
    assert_eq!(
        (
            TREE_ENTRY_CAPACITY,
            MAX_TREE_ENTRY_COST,
            MAX_TREE_SPLIT_COST
        ),
        (16_188, 8_094, 24_282)
    );
    for kind in [PageKind::RowLeaf, PageKind::CatalogLeaf] {
        let mut output = vec![0xa5; 8_091];
        assert_eq!(
            encode_leaf_cell(kind, 0, &1u64.to_be_bytes(), &vec![7; 8_078], &mut output)
                .unwrap()
                .len(),
            8_090
        );
        let before = output.clone();
        assert_eq!(
            encode_leaf_cell(kind, 0, &1u64.to_be_bytes(), &vec![7; 8_079], &mut output),
            Err(TreeError::EntryTooLarge)
        );
        assert_eq!(output, before);
        let full = vec![
            metadata(0, None),
            leaf_cell(&1u64.to_be_bytes(), &vec![7; 8_078]),
            leaf_cell(&2u64.to_be_bytes(), &vec![7; 8_078]),
        ];
        let bytes = page(header(kind, 0), 0, &full);
        assert_eq!(
            u16::from_le_bytes(bytes[74..76].try_into().unwrap()),
            u16::from_le_bytes(bytes[76..78].try_into().unwrap())
        );
        let mut many = vec![metadata(0, None)];
        many.extend((1u64..=1_011).map(|id| leaf_cell(&id.to_be_bytes(), &[])));
        let _ = page(header(kind, 0), 0, &many);
        many.push(leaf_cell(&1_012u64.to_be_bytes(), &[]));
        assert_eq!(
            encode_tree_page(header(kind, 0), 0, &views(&many), &mut vec![0; PAGE_BYTES]),
            Err(TreeError::NoSpace)
        );
        many.pop();
        many[1] = leaf_cell(&1u64.to_be_bytes(), &[7; 12]);
        let _ = page(header(kind, 0), 0, &many); // Exact C.
        many[1] = leaf_cell(&1u64.to_be_bytes(), &[7; 13]);
        assert_eq!(
            encode_tree_page(header(kind, 0), 0, &views(&many), &mut vec![0; PAGE_BYTES]),
            Err(TreeError::NoSpace)
        );
    }
    let maximal = physical(&[Value::Bytes(vec![1; 1_021])], 1);
    assert_eq!(maximal.len(), 1_032);
    let mut output = [0; 2_000];
    assert!(encode_leaf_cell(PageKind::IndexLeaf, 1, &maximal, &[], &mut output).is_ok());
    let mut oversized = maximal;
    oversized.insert(1, 1);
    assert_eq!(oversized.len(), 1_033);
    assert_eq!(
        encode_leaf_cell(PageKind::IndexLeaf, 1, &oversized, &[], &mut output),
        Err(TreeError::InvalidInput)
    );
    reject_damage(
        header(PageKind::IndexLeaf, 0),
        1,
        &[metadata(1, None), leaf_cell(&oversized, &[])],
    );
}

// Oracle consumes only lengths. It knows no production split/view implementation.
fn oracle_split(costs: &[usize], internal: bool) -> (usize, usize, usize) {
    let mut candidates = Vec::new();
    for index in 1..costs.len() - usize::from(internal) {
        let left: usize = costs[..index].iter().sum();
        let right: usize = costs[index + usize::from(internal)..].iter().sum();
        if left <= 16_188 && right <= 16_188 {
            candidates.push((left.max(right), index, left, right));
        }
    }
    candidates.sort_unstable();
    let (_, index, left, right) = candidates[0];
    (index, left, right)
}

fn leaf_cost_cells(costs: &[usize]) -> Vec<Vec<u8>> {
    let mut cells = vec![metadata(0, None)];
    for (index, cost) in costs.iter().enumerate() {
        cells.push(leaf_cell(
            &(index as u64 + 1).to_be_bytes(),
            &vec![0x41; cost - 16],
        ));
    }
    cells
}

fn check_leaf_split(costs: &[usize]) {
    let cells = leaf_cost_cells(costs);
    let result = choose_split(header(PageKind::RowLeaf, 0), 0, &views(&cells));
    if costs.iter().sum::<usize>() > 24_282 {
        assert_eq!(result, Err(TreeError::NoSpace));
        return;
    }
    let plan = result.unwrap();
    assert_eq!(
        (plan.index, plan.left_bytes, plan.right_bytes),
        oracle_split(costs, false)
    );
    assert_eq!(plan.separator, (plan.index as u64 + 1).to_be_bytes());
    assert_eq!(plan.right_leftmost, None);
    let mut left = vec![cells[0].clone()];
    left.extend_from_slice(&cells[1..=plan.index]);
    let mut right = vec![cells[0].clone()];
    right.extend_from_slice(&cells[plan.index + 1..]);
    for half in [&left, &right] {
        let bytes = page(header(PageKind::RowLeaf, 0), 0, half);
        assert_eq!(
            decode_tree_page(&bytes, expected(header(PageKind::RowLeaf, 0), 0))
                .unwrap()
                .len(),
            half.len() - 1
        );
    }
}

#[test]
fn tree_leaf_split_uses_bytes() {
    let mut count = 0;
    for length in 2u32..=8 {
        for mut encoded in 0..3usize.pow(length) {
            let mut costs = Vec::new();
            for _ in 0..length {
                costs.push([16, 4_096, 8_094][encoded % 3]);
                encoded /= 3;
            }
            check_leaf_split(&costs);
            count += 1;
        }
    }
    assert_eq!(count, 9_837);
    for costs in [&[8_094, 16, 16, 16, 16][..], &[8_094, 8_094, 8_094][..]] {
        check_leaf_split(costs);
        let cells = leaf_cost_cells(costs);
        assert_eq!(
            choose_split(header(PageKind::RowLeaf, 0), 0, &views(&cells))
                .unwrap()
                .index,
            1
        );
    }
}

fn check_internal_split(kind: PageKind, arity: u8, cells: &[Vec<u8>]) {
    let costs: Vec<usize> = cells[1..].iter().map(|cell| cell.len() + 4).collect();
    let result = choose_split(header(kind, 1), arity, &views(cells));
    if costs.iter().sum::<usize>() > 24_282 {
        assert_eq!(result, Err(TreeError::NoSpace));
        return;
    }
    let plan = result.unwrap();
    assert_eq!(
        (plan.index, plan.left_bytes, plan.right_bytes),
        oracle_split(&costs, true)
    );
    let pivot = &cells[plan.index + 1];
    let key_len = usize::from(u16::from_le_bytes([pivot[0], pivot[1]]));
    assert_eq!(plan.separator, &pivot[4..4 + key_len]);
    let promoted_child = PageRef::decode(&pivot[4 + key_len..]).unwrap();
    assert_eq!(plan.right_leftmost, Some(promoted_child));
    let mut left = vec![cells[0].clone()];
    left.extend_from_slice(&cells[1..=plan.index]);
    let mut right = vec![metadata(arity, Some(promoted_child))];
    right.extend_from_slice(&cells[plan.index + 2..]);
    let mut original_children = vec![PageRef::decode(&cells[0][8..]).unwrap()];
    for cell in &cells[1..] {
        original_children.push(PageRef::decode(&cell[cell.len() - 24..]).unwrap());
    }
    let mut resulting_children = Vec::new();
    for half in [&left, &right] {
        let bytes = page(header(kind, 1), arity, half);
        let view = decode_tree_page(&bytes, expected(header(kind, 1), arity)).unwrap();
        assert!(view.len() >= 1);
        resulting_children.extend(
            (0..=view.len()).map(|index| view.child_expectation(index).unwrap().identity.page),
        );
        for index in 0..view.len() {
            assert_ne!(view.entry(index).unwrap().key, plan.separator);
        }
    }
    assert_eq!(resulting_children, original_children);
}

#[test]
fn tree_internal_split_promotes_separator() {
    let mut minimum = vec![metadata(0, Some(reference(8)))];
    for id in 1u64..=3 {
        minimum.push(internal_cell(&id.to_be_bytes(), reference(id + 20)));
    }
    check_internal_split(PageKind::RowInternal, 0, &minimum);
    assert_eq!(
        choose_split(header(PageKind::RowInternal, 1), 0, &views(&minimum))
            .unwrap()
            .index,
        1
    );
    let mut count = 0;
    for seed in 1u64..=64 {
        for length in 3..=40 {
            let mut state = seed;
            let mut cells = vec![metadata(2, Some(reference(8)))];
            for index in 0..length {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let payload_len = [0, 500, 1_012][((state >> 32) % 3) as usize];
                let key = physical(
                    &[
                        Value::Int64(index as i64),
                        Value::Bytes(vec![1; payload_len]),
                    ],
                    index as u64 + 1,
                );
                let cell = internal_cell(&key, reference(index as u64 + 20));
                assert_eq!(cell.len() + 4, 52 + payload_len);
                cells.push(cell);
            }
            check_internal_split(PageKind::IndexInternal, 2, &cells);
            count += 1;
        }
    }
    assert_eq!(count, 2_432);
}

#[test]
fn tree_child_alias_and_kind_level_guards() {
    for (leaf, internal, arity) in FAMILIES {
        let parent = header(internal, 1);
        let mut cells = vec![
            metadata(arity, Some(reference(8))),
            internal_cell(&number_key(arity, 10), reference(9)),
            internal_cell(&number_key(arity, 20), reference(10)),
        ];
        for address in [parent.identity.page, reference(8), reference(9)] {
            let aliased = PageRef {
                incarnation: Generation::new(99).unwrap(),
                ..address
            };
            cells[2] = internal_cell(&number_key(arity, 20), aliased);
            reject_damage(parent, arity, &cells);
        }
        let foreign_file = PageRef {
            file: FileId::new(0x0000_0002_0000_0002).unwrap(),
            ..reference(8)
        };
        cells[2] = internal_cell(&number_key(arity, 20), foreign_file);
        let bytes = page(parent, arity, &cells);
        let view = decode_tree_page(&bytes, expected(parent, arity)).unwrap();
        let child_expected = view.child_expectation(2).unwrap();
        assert_eq!(child_expected.identity.page, foreign_file);
        let child_header = PageHeader {
            identity: child_expected.identity,
            kind: leaf,
            level: 0,
            lsn: PageLsn(0),
        };
        let child_cells = vec![metadata(arity, None)];
        let child = page(child_header, arity, &child_cells);
        assert!(decode_tree_page(&child, child_expected).is_ok());
        for mutation in 0..7 {
            let mut incorrect = child_expected;
            match mutation {
                0 => incorrect.identity.database = DatabaseId::new([0x22; 16]).unwrap(),
                1 => incorrect.identity.owner = ObjectId::new(55).unwrap(),
                2 => incorrect.identity.owner_generation = Generation::new(66).unwrap(),
                3 => incorrect.identity.page.incarnation = Generation::new(99).unwrap(),
                4 => incorrect.identity.page.page_no += 1,
                5 => incorrect.identity.page.file = FileId::new(0x0000_0003_0000_0002).unwrap(),
                6 => {
                    incorrect.kind = if leaf == PageKind::RowLeaf {
                        PageKind::CatalogLeaf
                    } else {
                        PageKind::RowLeaf
                    }
                }
                _ => unreachable!(),
            }
            if mutation == 6 {
                incorrect.arity = 0;
            }
            assert!(decode_tree_page(&child, incorrect).is_err());
        }
        let mut wrong_level = expected(parent, arity);
        wrong_level.level = 2;
        assert!(matches!(
            decode_tree_page(&bytes, wrong_level),
            Err(TreeError::LevelMismatch)
        ));
        let wrong_kind = TreeExpectation {
            kind: leaf,
            level: 0,
            ..expected(parent, arity)
        };
        assert!(matches!(
            decode_tree_page(&bytes, wrong_kind),
            Err(TreeError::Page(PageError::KindMismatch))
        ));
        if arity == 1 {
            let wrong_schema = TreeExpectation {
                arity: 2,
                ..child_expected
            };
            assert!(matches!(
                decode_tree_page(&child, wrong_schema),
                Err(TreeError::SchemaMismatch)
            ));
        }
    }
}

fn preserves_output(run: impl FnOnce(&mut [u8]) -> Result<(), TreeError>, length: usize) {
    let mut output = vec![0xa5; length];
    assert!(run(&mut output).is_err());
    assert_eq!(output, vec![0xa5; length]);
}

#[test]
fn tree_encoders_preserve_outputs() {
    for (leaf, internal, arity) in FAMILIES {
        for length in 0..32 {
            preserves_output(
                |out| encode_metadata(leaf, arity, None, out).map(|_| ()),
                length,
            );
        }
        preserves_output(
            |out| encode_metadata(leaf, arity, Some(reference(8)), out).map(|_| ()),
            64,
        );
        preserves_output(
            |out| encode_metadata(internal, arity, None, out).map(|_| ()),
            64,
        );
        preserves_output(|out| encode_metadata(leaf, 5, None, out).map(|_| ()), 64);
        let key = number_key(arity, 1);
        for length in 0..4 + key.len() + 1 {
            preserves_output(
                |out| encode_leaf_cell(leaf, arity, &key, &[1], out).map(|_| ()),
                length,
            );
        }
        for length in 0..4 + key.len() + 24 {
            preserves_output(
                |out| encode_internal_cell(internal, arity, &key, reference(9), out).map(|_| ()),
                length,
            );
        }
        preserves_output(
            |out| encode_leaf_cell(internal, arity, &key, &[], out).map(|_| ()),
            128,
        );
        preserves_output(
            |out| encode_internal_cell(leaf, arity, &key, reference(9), out).map(|_| ()),
            128,
        );
        preserves_output(
            |out| encode_leaf_cell(leaf, arity, &[], &[], out).map(|_| ()),
            128,
        );
        preserves_output(
            |out| encode_internal_cell(internal, arity, &[], reference(9), out).map(|_| ()),
            128,
        );
        preserves_output(
            |out| encode_leaf_cell(leaf, arity, &key, &vec![0; 8_095], out).map(|_| ()),
            128,
        );
        let mut scratch = [0xa5; 128];
        let size = encode_metadata(leaf, arity, None, &mut scratch)
            .unwrap()
            .len();
        assert!(scratch[size..].iter().all(|byte| *byte == 0xa5));
        scratch.fill(0xa5);
        let size = encode_leaf_cell(leaf, arity, &key, &[7], &mut scratch)
            .unwrap()
            .len();
        assert!(scratch[size..].iter().all(|byte| *byte == 0xa5));
        scratch.fill(0xa5);
        let size = encode_internal_cell(internal, arity, &key, reference(9), &mut scratch)
            .unwrap()
            .len();
        assert!(scratch[size..].iter().all(|byte| *byte == 0xa5));
        let mut cells = golden_cells(arity == 1, false);
        for length in [0, PAGE_BYTES - 1, PAGE_BYTES + 1] {
            preserves_output(
                |out| encode_tree_page(header(leaf, 0), arity, &views(&cells), out),
                length,
            );
        }
        preserves_output(
            |out| encode_tree_page(header(leaf, 1), arity, &views(&cells), out),
            PAGE_BYTES,
        );
        preserves_output(
            |out| encode_tree_page(header(leaf, 0), 5, &views(&cells), out),
            PAGE_BYTES,
        );
        cells[2].push(7);
        preserves_output(
            |out| encode_tree_page(header(leaf, 0), arity, &views(&cells), out),
            PAGE_BYTES,
        );
        cells[2] = cells[1].clone();
        preserves_output(
            |out| encode_tree_page(header(leaf, 0), arity, &views(&cells), out),
            PAGE_BYTES,
        );
        let empty_cells = vec![metadata(arity, None)];
        let empty = page(header(leaf, 0), arity, &empty_cells);
        let view = decode_tree_page(&empty, expected(header(leaf, 0), arity)).unwrap();
        assert_eq!(
            view.lower_bound(&key).unwrap(),
            SearchPosition {
                index: 0,
                equal: false
            }
        );
        assert_eq!(view.lower_bound(&[]), Err(TreeError::InvalidInput));
        assert_eq!(view.route_child(&key), Err(TreeError::InvalidInput));
        assert!(view.child_expectation(0).is_err());
        assert_eq!(
            choose_split(header(leaf, 0), arity, &views(&empty_cells)),
            Err(TreeError::NotSplittable)
        );
        let one = vec![
            metadata(arity, Some(reference(8))),
            internal_cell(&key, reference(9)),
        ];
        assert_eq!(
            choose_split(header(internal, 1), arity, &views(&one)),
            Err(TreeError::NotSplittable)
        );
        reject_damage(
            header(internal, 1),
            arity,
            &[metadata(arity, Some(reference(8)))],
        );
    }
    preserves_output(
        |out| encode_metadata(PageKind::Overflow, 0, None, out).map(|_| ()),
        128,
    );
    preserves_output(
        |out| encode_tree_page(header(PageKind::RowLeaf, 0), 0, &[], out),
        PAGE_BYTES,
    );
    let too_large = vec![
        metadata(0, None),
        leaf_cell(&1u64.to_be_bytes(), &vec![0; 8_079]),
    ];
    preserves_output(
        |out| encode_tree_page(header(PageKind::RowLeaf, 0), 0, &views(&too_large), out),
        PAGE_BYTES,
    );
    let too_full = leaf_cost_cells(&[8_094, 8_094, 16]);
    preserves_output(
        |out| encode_tree_page(header(PageKind::RowLeaf, 0), 0, &views(&too_full), out),
        PAGE_BYTES,
    );
}
