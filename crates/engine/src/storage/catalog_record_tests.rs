use super::*;
use crate::{
    api::{Column, DatabaseId},
    storage::{
        page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn},
        transaction_record::TxnRef,
        tree_page::{
            EntryPayload, TreeExpectation, decode_tree_page, encode_leaf_cell, encode_metadata,
            encode_tree_page,
        },
    },
    transaction::TxnId,
};
use foundation::{codec::content_digest, identity::CommitSeq};

fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn literal(index: usize) -> Vec<u8> {
    hex(LITERALS[index])
}

fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 17,
    }
}

fn transaction() -> TxnId {
    TxnId {
        database: namespace().database,
        timeline: 17,
        writer_epoch: 19,
        local: 23,
    }
}

fn object(n: u64) -> ObjectId {
    ObjectId::new(n).unwrap()
}
fn generation(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn schema_epoch(n: u64) -> SchemaEpoch {
    SchemaEpoch::new(n).unwrap()
}

fn table_identity() -> ObjectHandle {
    ObjectHandle {
        object: object(0x1300000001),
        generation: generation(5),
        schema_epoch: schema_epoch(7),
    }
}

fn header(index: usize) -> CatalogHeader {
    let mut identity = table_identity();
    if index >= 3 {
        identity.object = object(0x1300000001 + index as u64 - 2);
        identity.schema_epoch = schema_epoch(9);
    }
    CatalogHeader {
        identity,
        retired: index == 2,
        stamp: if matches!(index, 2 | 4) {
            VersionStamp::Transaction(TxnRef::new(transaction(), generation(37)).unwrap())
        } else {
            VersionStamp::Committed(CommitSeq::new(31).unwrap())
        },
        logical: LogicalRowVersion::new(transaction(), if index == 2 { 30 } else { 29 }).unwrap(),
        undo: matches!(index, 2 | 4).then(|| {
            UndoRef::new(
                PageRef {
                    file: FileId::new(0x1300000002).unwrap(),
                    page_no: 10,
                    incarnation: generation(2),
                },
                1,
                generation(3),
            )
            .unwrap()
        }),
        anchor: PageRef {
            file: FileId::new(0x1300000001).unwrap(),
            page_no: if index < 3 { 3 } else { 98 + index as u64 },
            incarnation: generation(4),
        },
    }
}

fn expected(index: usize) -> CatalogExpectation {
    CatalogExpectation {
        namespace: namespace(),
        object: header(index).identity.object,
    }
}

fn column(name: &str, data_type: DataType, nullable: bool) -> Column {
    Column {
        name: name.into(),
        data_type,
        nullable,
    }
}

fn table(index: usize) -> TableSpec {
    if index == 0 {
        TableSpec {
            name: "T".into(),
            columns: vec![column("i", DataType::Int64, false)],
        }
    } else {
        TableSpec {
            name: "테이블".into(),
            columns: vec![
                column("flag", DataType::Bool, true),
                column("balance", DataType::Int64, false),
                column("word", DataType::Utf8, true),
                column("data", DataType::Binary, false),
            ],
        }
    }
}

fn index_definition(index: usize) -> IndexDefinition<'static> {
    match index {
        3 => IndexDefinition {
            parent: table_identity(),
            name: "pk_i",
            mode: IndexMode::PrimaryKey,
            columns: &[1],
        },
        4 => IndexDefinition {
            parent: table_identity(),
            name: "uq_flag_word",
            mode: IndexMode::Unique,
            columns: &[0, 2],
        },
        5 => IndexDefinition {
            parent: table_identity(),
            name: "by_data",
            mode: IndexMode::Nonunique,
            columns: &[3],
        },
        _ => panic!("index fixture"),
    }
}

fn encode_fixture(index: usize, out: &mut [u8]) -> Result<usize, CatalogError> {
    let spec = table(index);
    let definition = if index < 3 {
        CatalogDefinition::Table(&spec)
    } else {
        CatalogDefinition::Index(index_definition(index))
    };
    encode_catalog_record(expected(index), header(index), definition, out)
}

fn table_roundtrip(spec: &TableSpec) -> Result<Vec<u8>, CatalogError> {
    let mut out = vec![0x91; MAX_CATALOG_CELL_BYTES + 16];
    let result = encode_catalog_record(
        expected(0),
        header(0),
        CatalogDefinition::Table(spec),
        &mut out,
    );
    match result {
        Err(error) => {
            assert!(out.iter().all(|b| *b == 0x91));
            Err(error)
        }
        Ok(n) => {
            assert!(out[n..].iter().all(|b| *b == 0x91));
            assert_table(decode_catalog_record(&out[..n], expected(0)).unwrap(), spec);
            out.truncate(n);
            Ok(out)
        }
    }
}

fn assert_table(view: CatalogRecordView<'_>, spec: &TableSpec) {
    let CatalogDefinitionView::Table(definition) = view.definition() else {
        panic!("table");
    };
    assert_eq!(definition.name(), spec.name);
    assert_eq!(definition.len(), spec.columns.len());
    for (i, column) in spec.columns.iter().enumerate() {
        assert_eq!(
            definition.column(i).unwrap(),
            CatalogColumn {
                name: &column.name,
                data_type: column.data_type,
                nullable: column.nullable,
            }
        );
    }
    assert_eq!(
        definition.column(spec.columns.len()),
        Err(CatalogError::InvalidInput)
    );
}

#[test]
fn catalog_literals_and_borrowed_definitions() {
    let sizes = [242, 280, 280, 244, 252, 247];
    for (i, size) in sizes.into_iter().enumerate() {
        let bytes = literal(i);
        assert_eq!(bytes.len(), size);
        assert_eq!(content_digest(&bytes).as_slice(), hex(HASHES[i]));
        let mut out = vec![0x8d; size + 7];
        assert_eq!(encode_fixture(i, &mut out), Ok(size));
        assert_eq!(&out[..size], bytes);
        assert_eq!(&out[size..], &[0x8d; 7]);
        let view = decode_catalog_record(&bytes, expected(i)).unwrap();
        assert_eq!(view.header(), header(i));
        if i < 3 {
            assert_table(view, &table(i));
            let CatalogDefinitionView::Table(table) = view.definition() else {
                unreachable!()
            };
            assert_eq!(table.name().as_ptr(), bytes[236..].as_ptr());
        } else {
            assert_eq!(
                view.definition(),
                CatalogDefinitionView::Index(index_definition(i))
            );
            let CatalogDefinitionView::Index(index) = view.definition() else {
                unreachable!()
            };
            assert_eq!(index.name.as_ptr(), bytes[240..].as_ptr());
            let table_bytes = literal(1);
            validate_index_against_table(
                view,
                decode_catalog_record(&table_bytes, expected(1)).unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn catalog_table_name_column_and_size_bounds() {
    for length in [0, 1, 128, 129] {
        for which in 0..2 {
            let mut spec = table(0);
            if which == 0 {
                spec.name = "x".repeat(length);
            } else {
                spec.columns[0].name = "x".repeat(length);
            }
            assert_eq!(
                crate::catalog::validate_spec(&spec).is_ok(),
                (1..=128).contains(&length)
            );
            assert_eq!(table_roundtrip(&spec).is_ok(), (1..=128).contains(&length));
        }
    }
    for count in [0, 1, 32, 33] {
        let spec = TableSpec {
            name: "T".into(),
            columns: (0..count)
                .map(|i| column(&format!("c{i}"), DataType::Bool, false))
                .collect(),
        };
        assert_eq!(table_roundtrip(&spec).is_ok(), (1..=32).contains(&count));
    }
    let mut max = TableSpec {
        name: "x".repeat(128),
        columns: (0..32)
            .map(|i| {
                column(
                    &format!("{i:02}{}", "c".repeat(126)),
                    DataType::Binary,
                    true,
                )
            })
            .collect(),
    };
    assert_eq!(table_roundtrip(&max).unwrap().len(), 4588);
    max.columns[31].name = max.columns[0].name.clone();
    assert_eq!(table_roundtrip(&max), Err(CatalogError::InvalidInput));
    for name in ["한", "é", "\0", "a\0b", &"한".repeat(43)] {
        for which in 0..2 {
            let mut spec = table(0);
            if which == 0 {
                spec.name = name.into();
            } else {
                spec.columns[0].name = name.into();
            }
            assert_eq!(
                table_roundtrip(&spec).is_ok(),
                !name.contains('\0') && name.len() <= 128
            );
        }
    }
    // The index name follows the same byte-exact limits.
    for length in [0, 1, 128, 129] {
        let name = "i".repeat(length);
        let index = IndexDefinition {
            name: &name,
            ..index_definition(3)
        };
        let mut out = [0x37; 400];
        let result = encode_catalog_record(
            expected(3),
            header(3),
            CatalogDefinition::Index(index),
            &mut out,
        );
        if (1..=128).contains(&length) {
            let n = result.unwrap();
            assert_eq!(n, 240 + length);
            assert!(decode_catalog_record(&out[..n], expected(3)).is_ok());
        } else {
            assert_eq!(result, Err(CatalogError::InvalidInput));
            assert_eq!(out, [0x37; 400]);
        }
    }
}

#[test]
fn catalog_index_columns_and_parent_matrix() {
    let mut counts = [0, 0];
    for mode in 0..=4 {
        for arity in 0..=5 {
            let mut bytes = literal(3);
            bytes[234] = mode;
            bytes[235] = arity;
            bytes[236..240].fill(0);
            for i in 0..arity.min(4) {
                bytes[236 + i as usize] = i;
            }
            let valid = (1..=3).contains(&mode) && (1..=4).contains(&arity);
            let result = decode_catalog_record(&bytes, expected(3));
            assert_eq!(result.is_ok(), valid, "mode={mode} arity={arity}");
            counts[usize::from(!valid)] += 1;
            if valid {
                let CatalogDefinitionView::Index(index) = result.unwrap().definition() else {
                    unreachable!()
                };
                assert_eq!(index.columns, &(0..arity).collect::<Vec<_>>());
            }
        }
    }
    assert_eq!(counts, [12, 18]);
    let mut negatives = 0;
    for damage in 0..3 {
        let i = if damage == 0 { 4 } else { 3 };
        let mut bytes = literal(i);
        match damage {
            0 => bytes[237] = bytes[236],
            1 => bytes[237] = 1,
            _ => bytes[236] = 32,
        }
        assert_eq!(
            decode_catalog_record(&bytes, expected(i)),
            Err(CatalogError::Corrupt)
        );
        negatives += 1;
    }
    for damage in 0..9 {
        let mut definition = index_definition(3);
        let mut table_header = header(1);
        let mut table_expected = expected(1);
        match damage {
            0 => definition.columns = &[4],
            1 => definition.columns = &[0],
            2 => definition.columns = &[1, 2],
            3 => definition.parent.object = object(0x1300000099),
            4 => definition.parent.generation = generation(6),
            5 => definition.parent.schema_epoch = schema_epoch(8),
            6 | 7 => {
                let mut txn = transaction();
                if damage == 6 {
                    txn.database = DatabaseId::new([0x22; 16]).unwrap();
                } else {
                    txn.timeline = 18;
                }
                table_expected.namespace = Namespace {
                    database: txn.database,
                    timeline: txn.timeline,
                };
                table_header.logical = LogicalRowVersion::new(txn, 29).unwrap();
            }
            _ => {}
        }
        let mut index_bytes = [0; 400];
        let n = encode_catalog_record(
            expected(3),
            header(3),
            CatalogDefinition::Index(definition),
            &mut index_bytes,
        )
        .unwrap();
        let index = decode_catalog_record(&index_bytes[..n], expected(3)).unwrap();
        let spec = table(1);
        let mut table_bytes = [0; 400];
        let m = encode_catalog_record(
            table_expected,
            table_header,
            CatalogDefinition::Table(&spec),
            &mut table_bytes,
        )
        .unwrap();
        let table = if damage == 8 {
            index
        } else {
            decode_catalog_record(&table_bytes[..m], table_expected).unwrap()
        };
        assert_eq!(
            validate_index_against_table(index, table),
            Err(CatalogError::DefinitionMismatch),
            "extra {}",
            damage + 4
        );
        negatives += 1;
    }
    assert_eq!(negatives, 12);
}

#[test]
fn catalog_lengths_damage_and_outputs() {
    let mut counts = [0; 3];
    for i in 0..6 {
        let bytes = literal(i);
        for n in 0..bytes.len() {
            assert!(decode_catalog_record(&bytes[..n], expected(i)).is_err());
            counts[0] += 1;
            let mut out = vec![0xa5; n];
            assert_eq!(encode_fixture(i, &mut out), Err(CatalogError::NoSpace));
            assert!(out.iter().all(|b| *b == 0xa5));
            counts[1] += 1;
        }
        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            decode_catalog_record(&trailing, expected(i)),
            Err(CatalogError::Corrupt)
        );
        counts[2] += 1;
    }
    assert_eq!(counts, [1545, 1545, 6]);
    let cases: [(usize, usize, usize, u64); 48] = [
        (0, 0, 1, 0),
        (0, 4, 2, 2),
        (0, 6, 1, 0),
        (0, 6, 1, 3),
        (0, 7, 1, 4),
        (0, 8, 8, 0),
        (0, 16, 8, 0),
        (0, 24, 8, 0),
        (0, 32, 1, 0),
        (0, 33, 1, 1),
        (0, 40, 8, 0),
        (0, 88, 16, 0),
        (0, 104, 8, 0),
        (0, 112, 4, 0),
        (0, 116, 4, 1),
        (0, 120, 8, 0),
        (0, 128, 8, 0),
        (0, 136, 1, 1),
        (0, 176, 4, 0),
        (0, 180, 4, 0),
        (0, 192, 8, 0),
        (0, 200, 8, 1),
        (0, 208, 8, 1),
        (0, 216, 8, 1),
        (0, 224, 4, 11),
        (0, 228, 1, 1),
        (0, 232, 2, 0),
        (0, 232, 2, 129),
        (0, 234, 1, 0),
        (0, 234, 1, 33),
        (0, 235, 1, 1),
        (0, 236, 1, 255),
        (0, 236, 1, 0),
        (0, 237, 2, 0),
        (0, 239, 1, 0),
        (0, 239, 1, 5),
        (0, 240, 1, 2),
        (0, 241, 1, 255),
        (0, 241, 1, 0),
        (0, 224, 4, 11),
        (3, 200, 8, 0),
        (3, 208, 8, 0),
        (3, 216, 8, 0),
        (3, 200, 8, 0x1300000002),
        (3, 234, 1, 0),
        (3, 235, 1, 0),
        (3, 236, 1, 32),
        (3, 237, 1, 1),
    ];
    for (ordinal, (i, offset, length, value)) in cases.into_iter().enumerate() {
        let mut bytes = literal(i);
        bytes[offset..offset + length].fill(0);
        let count = length.min(8);
        bytes[offset..offset + count].copy_from_slice(&value.to_le_bytes()[..count]);
        if ordinal == 39 {
            bytes.push(0);
        }
        let original = bytes.clone();
        assert_eq!(
            decode_catalog_record(&bytes, expected(i)),
            Err(if ordinal == 1 {
                CatalogError::UnsupportedFormat
            } else {
                CatalogError::Corrupt
            }),
            "N{:02}",
            ordinal + 1
        );
        assert_eq!(bytes, original);
    }
    // Semantic caller failures, including invalid final items, preserve output.
    let mut invalid_table = table(1);
    invalid_table.columns[3].name.clear();
    let mut out = [0x17; 5000];
    assert_eq!(
        encode_catalog_record(
            expected(1),
            header(1),
            CatalogDefinition::Table(&invalid_table),
            &mut out
        ),
        Err(CatalogError::InvalidInput)
    );
    assert_eq!(out, [0x17; 5000]);
    for columns in [&[][..], &[0, 1, 2, 3, 4], &[0, 0], &[0, 32]] {
        let index = IndexDefinition {
            columns,
            ..index_definition(3)
        };
        assert_eq!(
            encode_catalog_record(
                expected(3),
                header(3),
                CatalogDefinition::Index(index),
                &mut out
            ),
            Err(CatalogError::InvalidInput)
        );
        assert_eq!(out, [0x17; 5000]);
    }
    let index = IndexDefinition {
        parent: header(3).identity,
        ..index_definition(3)
    };
    assert_eq!(
        encode_catalog_record(
            expected(3),
            header(3),
            CatalogDefinition::Index(index),
            &mut out
        ),
        Err(CatalogError::InvalidInput)
    );
    assert_eq!(out, [0x17; 5000]);
}

#[test]
fn catalog_namespace_handle_and_versions() {
    let bytes = literal(4);
    let view = decode_catalog_record(&bytes, expected(4)).unwrap();
    let mut negatives = 0;
    for damage in 0..3 {
        let mut context = expected(4);
        match damage {
            0 => context.namespace.database = DatabaseId::new([0x22; 16]).unwrap(),
            1 => context.namespace.timeline += 1,
            _ => context.object = object(context.object.get() + 1),
        }
        assert_eq!(
            decode_catalog_record(&bytes, context),
            Err(CatalogError::IdentityMismatch)
        );
        let mut out = [0x29; 512];
        assert_eq!(
            encode_catalog_record(
                context,
                header(4),
                CatalogDefinition::Index(index_definition(4)),
                &mut out
            ),
            Err(CatalogError::IdentityMismatch)
        );
        assert_eq!(out, [0x29; 512]);
        negatives += 1;
    }
    for damage in 0..3 {
        let mut handle = header(4).identity;
        match damage {
            0 => handle.object = object(handle.object.get() + 1),
            1 => handle.generation = generation(6),
            _ => handle.schema_epoch = schema_epoch(10),
        }
        assert_eq!(
            view.validate_handle(handle),
            Err(CatalogError::IdentityMismatch)
        );
        negatives += 1;
    }
    view.validate_handle(header(4).identity).unwrap();
    for damage in 0..4 {
        let mut id = transaction();
        match damage {
            0 => id.database = DatabaseId::new([0x22; 16]).unwrap(),
            1 => id.timeline += 1,
            2 => id.writer_epoch += 1,
            _ => id.local += 1,
        }
        let mut changed = bytes.clone();
        VersionStamp::Transaction(TxnRef::new(id, generation(37)).unwrap())
            .encode(&mut changed[32..88])
            .unwrap();
        assert_eq!(
            decode_catalog_record(&changed, expected(4)),
            Err(CatalogError::IdentityMismatch)
        );
        let mut h = header(4);
        h.stamp = VersionStamp::Transaction(TxnRef::new(id, generation(37)).unwrap());
        let mut out = [0x29; 512];
        assert_eq!(
            encode_catalog_record(
                expected(4),
                h,
                CatalogDefinition::Index(index_definition(4)),
                &mut out
            ),
            Err(CatalogError::IdentityMismatch)
        );
        assert_eq!(out, [0x29; 512]);
        negatives += 1;
    }
    assert_eq!(negatives, 10);
    let mut historical = 0;
    for epoch in [1, 18] {
        for own in [false, true] {
            let id = TxnId {
                writer_epoch: epoch,
                ..transaction()
            };
            let mut h = header(4);
            h.logical = LogicalRowVersion::new(id, 29).unwrap();
            h.stamp = if own {
                VersionStamp::Transaction(TxnRef::new(id, generation(37)).unwrap())
            } else {
                VersionStamp::Committed(CommitSeq::new(31).unwrap())
            };
            let mut out = [0; 512];
            let n = encode_catalog_record(
                expected(4),
                h,
                CatalogDefinition::Index(index_definition(4)),
                &mut out,
            )
            .unwrap();
            assert_eq!(
                decode_catalog_record(&out[..n], expected(4))
                    .unwrap()
                    .header(),
                h
            );
            historical += 1;
        }
    }
    assert_eq!(historical, 4);
    assert_eq!(&literal(1)[232..], &literal(2)[232..]);
    assert_eq!(header(1).anchor, header(2).anchor);
    let context = CatalogExpectation {
        namespace: Namespace {
            timeline: 0,
            ..namespace()
        },
        ..expected(4)
    };
    assert_eq!(
        decode_catalog_record(&bytes, context),
        Err(CatalogError::InvalidInput)
    );
}

#[test]
fn catalog_tree_values_and_fixed_profiles() {
    for i in 0..6 {
        let bytes = literal(i);
        let object = header(i).identity.object;
        let key = object.get().to_be_bytes();
        let page_header = PageHeader {
            identity: PageIdentity {
                database: namespace().database,
                page: header(0).anchor,
                owner: self::object(55),
                owner_generation: generation(2),
            },
            kind: PageKind::CatalogLeaf,
            level: 0,
            lsn: PageLsn(11),
        };
        let mut meta = [0; 32];
        let meta = encode_metadata(PageKind::CatalogLeaf, 0, None, &mut meta).unwrap();
        let mut cell = vec![0; bytes.len() + 12];
        let cell = encode_leaf_cell(PageKind::CatalogLeaf, 0, &key, &bytes, &mut cell).unwrap();
        let mut page = [0; PAGE_BYTES];
        encode_tree_page(page_header, 0, &[meta, cell], &mut page).unwrap();
        let tree = decode_tree_page(
            &page,
            TreeExpectation {
                identity: page_header.identity,
                kind: PageKind::CatalogLeaf,
                level: 0,
                arity: 0,
            },
        )
        .unwrap();
        let entry = tree.entry(0).unwrap();
        let key_object = self::object(u64::from_be_bytes(entry.key.try_into().unwrap()));
        let EntryPayload::Value(value) = entry.payload else {
            panic!("catalog leaf")
        };
        assert_eq!(
            decode_catalog_record(
                value,
                CatalogExpectation {
                    namespace: namespace(),
                    object: key_object
                }
            )
            .unwrap()
            .header(),
            header(i)
        );
        assert_eq!(
            decode_catalog_record(
                value,
                CatalogExpectation {
                    namespace: namespace(),
                    object: self::object(key_object.get() + 1)
                }
            ),
            Err(CatalogError::IdentityMismatch)
        );
    }
    let order = [1_u64, 255, 256, 0x1300000001, u64::MAX];
    assert!(
        order
            .windows(2)
            .all(|p| p[0].to_be_bytes() < p[1].to_be_bytes())
    );
    let mut profiles = 0;
    for seed in 0..64 {
        for sample in 0..128 {
            let n = 128 * seed + sample;
            let mut h = header(0);
            h.identity = ObjectHandle {
                object: object(0x1300000100 + n),
                generation: generation(1 + n),
                schema_epoch: schema_epoch(1 + n),
            };
            h.logical = LogicalRowVersion::new(transaction(), 1 + n).unwrap();
            h.stamp = VersionStamp::Committed(CommitSeq::new(1 + n).unwrap());
            h.anchor.page_no = 1000 + n;
            h.retired = n & 16 != 0;
            let context = CatalogExpectation {
                namespace: namespace(),
                object: h.identity.object,
            };
            let spec = TableSpec {
                name: format!("t{n}"),
                columns: (0..1 + n % 32)
                    .map(|i| {
                        column(
                            &format!("c{i}"),
                            match i % 4 {
                                0 => DataType::Bool,
                                1 => DataType::Int64,
                                2 => DataType::Utf8,
                                _ => DataType::Binary,
                            },
                            i % 2 == 1,
                        )
                    })
                    .collect(),
            };
            let name = format!("i{n}");
            let positions: Vec<u8> = (0..(1 + n % 4) as u8).collect();
            let index = IndexDefinition {
                parent: table_identity(),
                name: &name,
                mode: match n % 3 {
                    0 => IndexMode::PrimaryKey,
                    1 => IndexMode::Unique,
                    _ => IndexMode::Nonunique,
                },
                columns: &positions,
            };
            let definition = if n % 2 == 0 {
                CatalogDefinition::Table(&spec)
            } else {
                CatalogDefinition::Index(index)
            };
            let expected_body_len = if n % 2 == 0 {
                4 + spec.name.len() + spec.columns.iter().map(|c| 4 + c.name.len()).sum::<usize>()
            } else {
                8 + name.len()
            };
            let mut out = [0x48; MAX_CATALOG_CELL_BYTES + 1];
            let length = encode_catalog_record(context, h, definition, &mut out).unwrap();
            assert_eq!(length, 232 + expected_body_len);
            assert_eq!(&out[8..16], &(0x1300000100 + n).to_le_bytes());
            assert_eq!(&out[16..24], &(1 + n).to_le_bytes());
            assert_eq!(&out[24..32], &(1 + n).to_le_bytes());
            assert_eq!(&out[128..136], &(1 + n).to_le_bytes());
            assert_eq!(&out[184..192], &(1000 + n).to_le_bytes());
            assert_eq!(&out[224..228], &(expected_body_len as u32).to_le_bytes());
            assert!(out[length..].iter().all(|b| *b == 0x48));
            let view = decode_catalog_record(&out[..length], context).unwrap();
            assert_eq!(view.header(), h);
            if n % 2 == 0 {
                assert_table(view, &spec);
                let mut offset = 236 + spec.name.len();
                for (i, column) in spec.columns.iter().enumerate() {
                    assert_eq!(out[offset + 2], 1 + (i % 4) as u8);
                    assert_eq!(out[offset + 3], (i % 2) as u8);
                    assert_eq!(
                        &out[offset + 4..offset + 4 + column.name.len()],
                        column.name.as_bytes()
                    );
                    offset += 4 + column.name.len();
                }
                assert_eq!(offset, length);
            } else {
                assert_eq!(out[234], (1 + n % 3) as u8);
                assert_eq!(out[235], (1 + n % 4) as u8);
                assert_eq!(&out[236..236 + positions.len()], positions);
                assert_eq!(view.definition(), CatalogDefinitionView::Index(index));
            }
            profiles += 1;
        }
    }
    assert_eq!(profiles, 8192);
}

// Frozen independent vectors generated before this product source.
const LITERALS: [&str; 6] = [
    "584354310100010001000000130000000500000000000000070000000000000001000000000000001f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000013000000030000000000000004000000000000000000000000000000000000000000000000000000000000000a0000000000000001000100540100020069",
    "584354310100010001000000130000000500000000000000070000000000000001000000000000001f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001d0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000001300000003000000000000000400000000000000000000000000000000000000000000000000000000000000300000000000000009000400ed858cec9db4ebb89404000101666c61670700020062616c616e636504000301776f72640400040064617461",
    "58435431010001030100000013000000050000000000000007000000000000000200000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000002500000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001e0000000000000002000000130000000a00000000000000020000000000000001000000000000000300000000000000010000001300000003000000000000000400000000000000000000000000000000000000000000000000000000000000300000000000000009000400ed858cec9db4ebb89404000101666c61670700020062616c616e636504000301776f72640400040064617461",
    "584354310100020002000000130000000500000000000000090000000000000001000000000000001f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000013000000650000000000000004000000000000000100000013000000050000000000000007000000000000000c000000000000000400010101000000706b5f69",
    "58435431010002020300000013000000050000000000000009000000000000000200000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000002500000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001d0000000000000002000000130000000a0000000000000002000000000000000100000000000000030000000000000001000000130000006600000000000000040000000000000001000000130000000500000000000000070000000000000014000000000000000c0002020002000075715f666c61675f776f7264",
    "584354310100020004000000130000000500000000000000090000000000000001000000000000001f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000111111111111111111111111111111111100000000000000130000000000000017000000000000001d00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000013000000670000000000000004000000000000000100000013000000050000000000000007000000000000000f00000000000000070003010300000062795f64617461",
];
const HASHES: [&str; 6] = [
    "433eae5477405a1cc8b561c13da2975eb58912a97f5da4e74d6928814b9616af",
    "15f8948388798e05bdc36a98945bb9e12b17fa13ef84fef04f6c10d4f5a7ba6d",
    "98cae1d3fd0811d73f968c750e28d5375c9fc5f887241ba0f15375c23a79eb65",
    "8c6b12dca7e2776418f98d18cdadd734212e22e3a755b50660d12c02dab83f32",
    "f5d48120011e3dc6ba41076853e83ff06015527ea5cac54e595a2eebeb4b8f30",
    "da199ff61844a63730dfdc49a40ff72881719ac10a568f325e3f2c44370e77ff",
];
