//! Densest supported cells, using raw record bytes and lower input codecs.
//! This is a local candidate observation, not an accepted actor/history cut.
use super::*;

fn catalog_schema(world: &mut World, columns: &[Column]) {
    let old = world.pages.get(&8).unwrap();
    let mut h = world.header(8, 5, PageKind::CatalogLeaf, 0);
    h.identity.page.incarnation = one();
    let view = tree_page::decode_tree_page(old.as_ref(), expected(h)).unwrap();
    let EntryPayload::Value(value) = view.entry(0).unwrap().payload else {
        panic!("Catalog");
    };
    let expect = CatalogExpectation {
        namespace: profile().spec().namespace,
        object: table().object,
    };
    let header = catalog_record::decode_catalog_record(value, expect)
        .unwrap()
        .header();
    let spec = TableSpec {
        name: "t".into(),
        columns: columns.to_vec(),
    };
    let mut record = [0; 1024];
    let count = catalog_record::encode_catalog_table_record(
        expect,
        header,
        CatalogTableInput::Spec(&spec),
        &mut record,
    )
    .unwrap();
    let mut cell = [0; 1200];
    let cell = tree_page::encode_leaf_cell(
        PageKind::CatalogLeaf,
        0,
        &6u64.to_be_bytes(),
        &record[..count],
        &mut cell,
    )
    .unwrap();
    let mut meta = [0; 32];
    tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut meta).unwrap();
    let h = view.header();
    let mut out = Box::new([0; PAGE_BYTES]);
    tree_page::encode_tree_page(h, 0, &[&meta, cell], out.as_mut()).unwrap();
    world.pages.insert(8, out);
}

fn raw_cell(template: &[u8], row: u64, deleted: bool) -> Vec<u8> {
    let mut record = template[..168].to_vec();
    record[8..16].copy_from_slice(&row.to_le_bytes());
    record[112..120].copy_from_slice(&row.to_le_bytes());
    if deleted {
        record[6] = 3;
        record[7] = 0;
        record[160..164].fill(0);
    } else {
        record[6] = 0;
        record[7] = 1;
        record[16] = 1;
        record[24..72].fill(0);
        record[24..32].copy_from_slice(&2u64.to_le_bytes());
        record[120..160].fill(0);
        record[160..164].copy_from_slice(&5u32.to_le_bytes());
        record.extend_from_slice(&[1, 0, 0, 0, 0]); // one NULL canonical value
    }
    let mut cell = Vec::from([8, 0]);
    cell.extend_from_slice(&(record.len() as u16).to_le_bytes());
    cell.extend_from_slice(&row.to_be_bytes());
    cell.extend_from_slice(&record);
    cell
}

#[test]
fn structural_candidate_dense_nullable_rows_fit_reduced_scratch_and_preserve_all_bytes() {
    let mut world = World::new(&["U-558", "U-560"]);
    let template = world.fixture.rows.values().next().unwrap()[12..].to_vec();
    let columns = [Column {
        name: "v".into(),
        data_type: DataType::Bool,
        nullable: true,
    }];
    catalog_schema(&mut world, &columns);
    world.fixture.rows = (1..=85)
        .map(|row| (row, raw_cell(&template, row, false)))
        .collect();
    for job in world.fixture.jobs.iter_mut() {
        row::free_job(one(), job);
    }
    let mut meta = [0; 32];
    tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut meta).unwrap();
    let refs: Vec<&[u8]> = std::iter::once(meta.as_slice())
        .chain(world.fixture.rows.values().map(Vec::as_slice))
        .collect();
    let mut leaf = Box::new([0; PAGE_BYTES]);
    tree_page::encode_tree_page(
        world.header(13, 6, PageKind::RowLeaf, 0),
        0,
        &refs,
        leaf.as_mut(),
    )
    .unwrap();
    world.pages.insert(13, leaf);
    let mut after = World::outputs();
    let candidate = product::prepare(&world.inputs(13), &mut after, |p| world.page(p)).unwrap();
    let mut actual = leaf_cells(&after[2], expected(candidate.targets()[2]));
    actual.extend(leaf_cells(&after[3], expected(candidate.targets()[3])));
    assert_eq!(actual.len(), 85);
    for (id, bytes) in actual {
        assert_eq!(bytes, world.fixture.rows[&id][12..]);
    }
}

#[test]
fn structural_candidate_tombstone_ceiling_includes_shortest_supported_record() {
    let world = World::new(&["U-558", "U-560"]);
    let template = &world.fixture.rows.values().next().unwrap()[12..];
    let cells: Vec<_> = (1..=88).map(|row| raw_cell(template, row, true)).collect();
    for (i, cell) in cells.iter().enumerate() {
        let row = row_record::decode_row_record(
            &cell[12..],
            RecordExpectation {
                namespace: profile().spec().namespace,
                row: RowId::new(i as u64 + 1).unwrap(),
            },
            &columns(),
        )
        .unwrap();
        assert_eq!(row.payload(), RowPayload::Deleted);
        assert_eq!(cell.len() + 4, 184);
    }
    let mut meta = [0; 32];
    tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut meta).unwrap();
    let refs: Vec<&[u8]> = std::iter::once(meta.as_slice())
        .chain(cells.iter().map(Vec::as_slice))
        .collect();
    let mut page = Box::new([0; PAGE_BYTES]);
    let h = world.header(13, 6, PageKind::RowLeaf, 0);
    tree_page::encode_tree_page(h, 0, &refs[..88], page.as_mut()).unwrap();
    assert_eq!(
        tree_page::decode_tree_page(page.as_ref(), expected(h))
            .unwrap()
            .len(),
        87
    );
    assert!(tree_page::encode_tree_page(h, 0, &refs, page.as_mut()).is_err());
    assert_eq!(product::MAX_LEAF_ROWS, 87);
    assert_eq!(product::ROW_REFS, 406);
}

#[test]
fn structural_candidate_overdense_untyped_leaf_rejects_without_output() {
    let mut world = World::new(&["U-558", "U-560"]);
    let cells: Vec<_> = (1..=88u64)
        .map(|row| {
            let mut cell = vec![8, 0, 0, 0];
            cell.extend_from_slice(&row.to_be_bytes());
            cell
        })
        .collect();
    let mut meta = [0; 32];
    tree_page::encode_metadata(PageKind::RowLeaf, 0, None, &mut meta).unwrap();
    let refs: Vec<&[u8]> = std::iter::once(meta.as_slice())
        .chain(cells.iter().map(Vec::as_slice))
        .collect();
    let mut leaf = Box::new([0; PAGE_BYTES]);
    let h = world.header(13, 6, PageKind::RowLeaf, 0);
    tree_page::encode_tree_page(h, 0, &refs, leaf.as_mut()).unwrap();
    assert_eq!(
        tree_page::decode_tree_page(leaf.as_ref(), expected(h))
            .unwrap()
            .len(),
        88
    );
    world.pages.insert(13, leaf);
    let mut out = World::outputs();
    assert_eq!(
        product::prepare(&world.inputs(13), &mut out, |p| world.page(p)).err(),
        Some(Error::Exhausted)
    );
    assert!(out.as_flattened().iter().all(|b| *b == 0xa5));
}

#[test]
fn structural_candidate_local_parent_last_separator_fits_reduced_scratch() {
    let world = World::new(&["U-558", "U-560"]);
    let mut meta = [0; 32];
    tree_page::encode_metadata(PageKind::RowInternal, 0, Some(reference(1000)), &mut meta).unwrap();
    let cells: Vec<[u8; 36]> = (1..=403u64)
        .map(|i| {
            let mut cell = [0; 36];
            tree_page::encode_internal_cell(
                PageKind::RowInternal,
                0,
                &(i * 1000).to_be_bytes(),
                reference(1000 + i),
                &mut cell,
            )
            .unwrap();
            cell
        })
        .collect();
    let refs: Vec<&[u8]> = std::iter::once(meta.as_slice())
        .chain(cells.iter().map(|c| c.as_slice()))
        .collect();
    let mut before = Box::new([0; PAGE_BYTES]);
    let h = world.header(23, 6, PageKind::RowInternal, 1);
    tree_page::encode_tree_page(h, 0, &refs, before.as_mut()).unwrap();
    let view = tree_page::decode_tree_page(before.as_ref(), expected(h)).unwrap();
    let mut out = Box::new([0xa5; PAGE_BYTES]);
    product::encode_parent(
        &view,
        reference(1403),
        &404000u64.to_be_bytes(),
        reference(2000),
        PageLsn(world.fixture.lsn + 1),
        out.as_mut(),
    )
    .unwrap();
    let after = tree_page::decode_tree_page(out.as_ref(), expected(h)).unwrap();
    assert_eq!(after.len(), 404);
    assert_eq!(
        after.child_expectation(404).unwrap().identity.page,
        reference(2000)
    );
    for i in 0..403 {
        assert_eq!(after.entry(i).unwrap().key, view.entry(i).unwrap().key);
    }
}
