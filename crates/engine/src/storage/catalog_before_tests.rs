//! Independent XCB1/XUR1 inputs frozen by catalog-before-input-freeze-d2.md.
//! No database owner, lower test wrapper, or native process is invoked here.
use super::{before::*, *};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace, TableSpec},
    storage::{
        page::{FileId, PageIdentity, PageRef},
        transaction_record::{LogicalRowVersion, TxnRef, VersionStamp},
        undo_record::*,
        undo_ref::UndoRef,
    },
    transaction::TxnId,
};
use std::{
    collections::BTreeSet,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
};
use xadb_foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation, ObjectHandle, ObjectId, SchemaEpoch},
};

pub(in crate::storage) fn ids(prefix: &str, first: usize, last: usize) -> Vec<String> {
    (first..=last)
        .map(|n| format!("CB-{prefix}{n:02}"))
        .collect()
}
fn all_ids() -> BTreeSet<String> {
    let mut all = BTreeSet::new();
    for (p, n) in [
        ("P", 15),
        ("C", 3),
        ("N", 50),
        ("A", 9),
        ("L", 6),
        ("E", 8),
        ("I", 10),
        ("H", 14),
        ("K", 5),
        ("B", 2),
        ("W", 24),
    ] {
        all.extend(ids(p, 1, n));
    }
    all.insert("CB-ST01".to_owned());
    all.extend((0..512).map(|n| format!("CB-G{n:03}")));
    for (k, len) in [106, 2048, 2049, 4452, 105, 232].into_iter().enumerate() {
        all.extend((0..len).map(|d| format!("CB-T{:02}-{d:04}", k + 1)));
        all.insert(format!("CB-T{:02}-TAIL", k + 1));
    }
    assert_eq!(all.len(), 9657);
    all
}
pub(in crate::storage) struct Harness {
    selected: BTreeSet<String>,
    owned: BTreeSet<String>,
    seen: BTreeSet<String>,
}
impl Harness {
    pub(in crate::storage) fn new(owned: Vec<String>) -> Self {
        let all = all_ids();
        let parse = |name: &str| -> Option<BTreeSet<String>> {
            std::env::var(name).ok().map(|value| {
                assert!(!value.is_empty(), "empty {name}");
                let mut result = BTreeSet::new();
                for id in value.split(',') {
                    assert!(all.contains(id), "unknown exact ID {id} in {name}");
                    assert!(result.insert(id.to_owned()), "duplicate ID {id} in {name}");
                }
                result
            })
        };
        let include = parse("XADB_CATALOG_BEFORE_INCLUDE").unwrap_or_else(|| all.clone());
        let exclude = parse("XADB_CATALOG_BEFORE_EXCLUDE").unwrap_or_default();
        let count = owned.len();
        let owned: BTreeSet<_> = owned.into_iter().collect();
        assert_eq!(owned.len(), count, "duplicate harness ownership");
        assert!(owned.is_subset(&all));
        let selected: BTreeSet<_> = owned
            .intersection(&include)
            .filter(|id| !exclude.contains(*id))
            .cloned()
            .collect();
        assert!(!selected.is_empty(), "empty harness selection");
        Self {
            selected,
            owned,
            seen: BTreeSet::new(),
        }
    }
    pub(in crate::storage) fn case(&mut self, id: &str, operation: impl FnOnce()) {
        assert!(self.owned.contains(id), "foreign harness ID {id}");
        assert!(self.seen.insert(id.to_owned()), "duplicate execution {id}");
        if !self.selected.contains(id) {
            println!("CATALOG_BEFORE_SKIP\t{id}\tselector");
            return;
        }
        println!("CATALOG_BEFORE_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(()) => println!("CATALOG_BEFORE_PASS\t{id}"),
            Err(error) => {
                println!("CATALOG_BEFORE_FAIL\t{id}");
                resume_unwind(error);
            }
        }
    }
    pub(in crate::storage) fn finish(self) {
        assert_eq!(self.seen, self.owned, "unvisited harness ID");
    }
}
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn digest_hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn raw(id: &str) -> Vec<u8> {
    let base = repo().join("tests/fixtures/dev3-catalog-before");
    let bytes = std::fs::read(base.join(format!("raw/{id}.bin"))).unwrap();
    let manifest = std::fs::read_to_string(base.join("manifest.json")).unwrap();
    let key = format!("\"{id}\": {{");
    let entry = manifest.split_once(&key).expect("raw manifest ID").1;
    let hash = entry
        .split_once("\"sha256\": \"")
        .expect("raw manifest SHA")
        .1;
    assert_eq!(digest_hex(&bytes), &hash[..64]);
    bytes
}
fn g(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn oid(n: u64) -> ObjectId {
    ObjectId::new(n).unwrap()
}
fn identity(object: u64, generation: u64, schema: u64) -> ObjectHandle {
    ObjectHandle {
        object: oid(object),
        generation: g(generation),
        schema_epoch: SchemaEpoch::new(schema).unwrap(),
    }
}
fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
    }
}
fn tx(epoch: u32, local: u64) -> TxnId {
    TxnId {
        database: ns().database,
        timeline: 17,
        writer_epoch: epoch,
        local,
    }
}
fn page(n: u64, counter: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new((3_u64 << 32) | counter).unwrap(),
        page_no: n,
        incarnation: g(incarnation),
    }
}
fn reference(n: u64, generation: u64) -> UndoRef {
    UndoRef::new(page(n, 5, 7), 0, g(generation)).unwrap()
}
fn expected(j: usize) -> UndoRecordExpectation<'static> {
    UndoRecordExpectation {
        namespace: ns(),
        reference: reference(10000 + j as u64, 9),
        target: UndoTargetView {
            kind: UndoTargetKind::Catalog,
            owner: oid(9001),
            generation: g(4),
            entity: 100000 + j as u64,
            arity: 0,
            key: &[],
        },
    }
}
fn catalog_expected(j: usize) -> CatalogExpectation {
    CatalogExpectation {
        namespace: ns(),
        object: oid(100000 + j as u64),
    }
}
fn dtype(tag: usize) -> DataType {
    [
        DataType::Bool,
        DataType::Int64,
        DataType::Utf8,
        DataType::Binary,
    ][tag - 1]
}
fn type_tag(t: DataType) -> u8 {
    match t {
        DataType::Bool => 1,
        DataType::Int64 => 2,
        DataType::Utf8 => 3,
        DataType::Binary => 4,
    }
}
fn named(prefix: &str, len: usize) -> String {
    assert!(prefix.len() <= len);
    format!("{prefix}{}", "a".repeat(len - prefix.len()))
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Definition {
    Table(TableSpec),
    Index {
        parent: ObjectHandle,
        name: String,
        mode: IndexMode,
        columns: Vec<u8>,
    },
}
impl Definition {
    fn borrowed(&self) -> CatalogDefinition<'_> {
        match self {
            Self::Table(t) => CatalogDefinition::Table(t),
            Self::Index {
                parent,
                name,
                mode,
                columns,
            } => CatalogDefinition::Index(IndexDefinition {
                parent: *parent,
                name,
                mode: *mode,
                columns,
            }),
        }
    }
    fn body(&self) -> Vec<u8> {
        let mut body = Vec::new();
        match self {
            Self::Table(t) => {
                body.extend_from_slice(&(t.name.len() as u16).to_le_bytes());
                body.extend_from_slice(&[t.columns.len() as u8, 0]);
                body.extend_from_slice(t.name.as_bytes());
                for c in &t.columns {
                    body.extend_from_slice(&(c.name.len() as u16).to_le_bytes());
                    body.extend_from_slice(&[type_tag(c.data_type), u8::from(c.nullable)]);
                    body.extend_from_slice(c.name.as_bytes());
                }
            }
            Self::Index {
                name,
                mode,
                columns,
                ..
            } => {
                body.extend_from_slice(&(name.len() as u16).to_le_bytes());
                body.extend_from_slice(&[*mode as u8, columns.len() as u8]);
                body.extend_from_slice(columns);
                body.resize(8, 0);
                body.extend_from_slice(name.as_bytes());
            }
        }
        body
    }
    fn check(&self, view: CatalogDefinitionView<'_>) {
        match (self, view) {
            (Self::Table(t), CatalogDefinitionView::Table(v)) => {
                assert_eq!(v.name(), t.name);
                assert_eq!(v.len(), t.columns.len());
                for (i, c) in t.columns.iter().enumerate() {
                    assert_eq!(
                        v.column(i).unwrap(),
                        CatalogColumn {
                            name: &c.name,
                            data_type: c.data_type,
                            nullable: c.nullable
                        }
                    );
                }
            }
            (
                Self::Index {
                    parent,
                    name,
                    mode,
                    columns,
                },
                CatalogDefinitionView::Index(v),
            ) => {
                assert_eq!(v.parent, *parent);
                assert_eq!(v.name, name.as_str());
                assert_eq!(v.mode, *mode);
                assert_eq!(v.columns, columns.as_slice());
            }
            _ => panic!("wrong typed definition"),
        }
    }
}
fn definition(j: usize) -> Definition {
    if (10..=12).contains(&j) {
        return Definition::Index {
            parent: identity(50, 7, 8),
            name: named("I", if j == 11 { 128 } else { 1 }),
            mode: [
                IndexMode::PrimaryKey,
                IndexMode::Unique,
                IndexMode::Nonunique,
            ][j - 10],
            columns: if j == 11 { vec![0, 1, 2, 3] } else { vec![0] },
        };
    }
    let (name, columns) = match j {
        6 => (
            "T".to_owned(),
            ['b', 'i', 'u', 'x']
                .into_iter()
                .enumerate()
                .map(|(i, n)| Column {
                    name: n.to_string(),
                    data_type: dtype(i + 1),
                    nullable: i % 2 == 0,
                })
                .collect(),
        ),
        7 | 8 | 9 | 14 | 15 => {
            let maximum = matches!(j, 9 | 14 | 15);
            (
                named(
                    "T",
                    if maximum {
                        128
                    } else {
                        100 + usize::from(j == 8)
                    },
                ),
                (0..if maximum { 32 } else { 14 })
                    .map(|i| Column {
                        name: named(&format!("c{i:02}"), 128),
                        data_type: dtype(1 + i % 4),
                        nullable: i % 2 == 1,
                    })
                    .collect(),
            )
        }
        _ => (
            "T".to_owned(),
            vec![Column {
                name: "i".to_owned(),
                data_type: DataType::Int64,
                nullable: false,
            }],
        ),
    };
    Definition::Table(TableSpec { name, columns })
}
fn before_header(j: usize) -> CatalogBeforeHeader {
    CatalogBeforeHeader {
        identity: identity(100000 + j as u64, 7, 8),
        retired: matches!(j, 5 | 12),
        anchor: page(20000 + j as u64, 6, 5),
    }
}
fn put(b: &mut [u8], offset: usize, value: u64, width: usize) {
    b[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
}
fn wire_page(b: &mut [u8], offset: usize, p: PageRef) {
    let counter = if p.file == FileId::new(0x3_0000_0005).unwrap() {
        5
    } else {
        assert_eq!(p.file, FileId::new(0x3_0000_0006).unwrap());
        6
    };
    put(b, offset, 0x3_0000_0000 | counter, 8);
    put(b, offset + 8, p.page_no, 8);
    put(b, offset + 16, p.incarnation.get(), 8);
}
fn wire_ref(b: &mut [u8], offset: usize, r: UndoRef) {
    wire_page(b, offset, r.page());
    put(b, offset + 24, u64::from(r.slot()), 2);
    put(b, offset + 32, r.generation().get(), 8);
}
fn wire_tx(b: &mut [u8], offset: usize, t: TxnId) {
    b[offset..offset + 16].copy_from_slice(&t.database.bytes());
    put(b, offset + 16, t.timeline, 8);
    put(b, offset + 24, u64::from(t.writer_epoch), 4);
    put(b, offset + 32, t.local, 8);
}
fn independent_payload(header: CatalogBeforeHeader, d: &Definition) -> Vec<u8> {
    let body = d.body();
    let mut b = vec![0; 96];
    b[..4].copy_from_slice(b"XCB1");
    put(&mut b, 4, 1, 2);
    b[7] = u8::from(header.retired);
    put(&mut b, 8, header.identity.generation.get(), 8);
    put(&mut b, 16, header.identity.schema_epoch.get(), 8);
    wire_page(&mut b, 24, header.anchor);
    match d {
        Definition::Table(_) => b[6] = 1,
        Definition::Index { parent, .. } => {
            b[6] = 2;
            put(&mut b, 48, parent.object.get(), 8);
            put(&mut b, 56, parent.generation.get(), 8);
            put(&mut b, 64, parent.schema_epoch.get(), 8);
        }
    }
    put(&mut b, 72, body.len() as u64, 4);
    b.extend_from_slice(&body);
    b
}
fn value(j: usize) -> Vec<u8> {
    independent_payload(before_header(j), &definition(j))
}
fn record<'a>(j: usize, payload: &'a [u8]) -> UndoRecord<'a> {
    let releasing = matches!(j, 2 | 13 | 14 | 15);
    let missing = j <= 2;
    let creator = UndoCreator {
        id: tx(18, 1000 + j as u64),
        status_generation: g(20),
    };
    let before_stamp = if missing || j >= 13 {
        None
    } else if j == 4 {
        Some(VersionStamp::Transaction(
            TxnRef::new(creator.id, g(20)).unwrap(),
        ))
    } else {
        Some(VersionStamp::Committed(CommitSeq::new(77).unwrap()))
    };
    let before_logical = if missing {
        None
    } else {
        Some(
            LogicalRowVersion::new(
                if j == 4 {
                    creator.id
                } else {
                    tx(16, 3000 + j as u64)
                },
                if j == 4 { 99 } else { 50 },
            )
            .unwrap(),
        )
    };
    UndoRecord {
        creator,
        sequence: 10000 + j as u64,
        mutation_sequence: 100,
        incoming: if releasing { 0 } else { 2 },
        target: expected(j).target,
        edges: if j >= 13 {
            [UndoEdge::Released; 2]
        } else {
            [UndoEdge::Never; 2]
        },
        before_kind: if missing {
            UndoBeforeKind::Missing
        } else {
            UndoBeforeKind::CatalogValue
        },
        before_stamp,
        before_logical,
        payload: if missing {
            UndoPayload::None
        } else if payload.len() <= 2048 {
            UndoPayload::Inline(payload)
        } else {
            UndoPayload::Chunks(
                UndoPayloadHead::new(
                    page(30000 + j as u64, 5, 7),
                    payload.len(),
                    content_digest(payload),
                )
                .unwrap(),
            )
        },
        release: UndoRelease {
            cursor: if j >= 13 { 3 } else { 0 },
            chunks_released: u16::from(j == 15),
            next: if j == 14 {
                Some(page(30014, 5, 7))
            } else {
                None
            },
        },
    }
}
fn cell<'a>(j: usize, payload: &'a [u8]) -> UndoCell<'a> {
    if matches!(j, 2 | 13 | 14 | 15) {
        UndoCell::Releasing(record(j, payload))
    } else {
        UndoCell::Live(record(j, payload))
    }
}
fn check_before(v: CatalogBeforeView<'_>, header: CatalogBeforeHeader, d: &Definition) {
    assert_eq!(v.header(), header);
    v.validate_handle(header.identity).unwrap();
    d.check(v.definition());
}
fn context(view: UndoRecordView<'_>) -> UndoChunkContext {
    UndoChunkContext::from_record(
        view,
        PageIdentity {
            database: ns().database,
            page: view.reference().page(),
            owner: oid(9000),
            owner_generation: g(3),
        },
    )
    .unwrap()
}

#[test]
fn catalog_before_literals_match_independent_raw() {
    let mut h = Harness::new(ids("P", 1, 15));
    for j in 1..=15 {
        h.case(&format!("CB-P{j:02}"), || {
            let input = raw(&format!("P{j:02}"));
            let saved = input.clone();
            let payload = value(j);
            let logical = record(j, &payload);
            let e = expected(j);
            let mut out = vec![0xa5; input.len() + 16];
            assert_eq!(
                undo_record_encoded_len(cell(j, &payload), e, &[]).unwrap(),
                input.len()
            );
            assert_eq!(
                encode_undo_record(cell(j, &payload), e, &[], &mut out).unwrap(),
                input.len()
            );
            assert_eq!(&out[..input.len()], input);
            assert!(out[input.len()..].iter().all(|b| *b == 0xa5));
            let view = decode_undo_record(&input, e, &[]).unwrap();
            assert_eq!(view.reference(), e.reference);
            assert_eq!(view.generation(), g(9));
            assert_eq!(
                view.metadata().unwrap(),
                UndoRecordMetadata {
                    creator: logical.creator,
                    sequence: logical.sequence,
                    mutation_sequence: logical.mutation_sequence,
                    incoming: logical.incoming,
                    target: logical.target
                }
            );
            if matches!(j, 2 | 13 | 14 | 15) {
                assert_eq!(view.state(), UndoRecordState::Releasing);
                assert_eq!(view.release_progress().unwrap().progress, logical.release);
            } else {
                assert_eq!(view.state(), UndoRecordState::Live);
                assert_eq!(view.live_before().unwrap(), logical);
            }
            if j == 1 {
                assert!(decode_inline_catalog_before(view).unwrap().is_none());
            }
            if matches!(j, 3 | 4 | 5 | 6 | 7 | 10 | 11 | 12) {
                let actual = decode_inline_catalog_before(view).unwrap().unwrap();
                check_before(actual, before_header(j), &definition(j));
            }
            assert_eq!(input, saved);
        });
    }
    h.finish();
}
fn negative_base(n: usize) -> usize {
    match n {
        7..=10 => 4,
        14..=16 => 1,
        24 | 25 => 10,
        30 => 6,
        33 | 34 | 36 | 49 | 50 => 8,
        35 => 9,
        41 | 47 => 14,
        45 => 13,
        46 | 48 => 15,
        _ => 3,
    }
}
fn reject_root(n: usize) {
    let input = raw(&format!("N{n:02}"));
    let saved = input.clone();
    let j = negative_base(n);
    let mut e = expected(j);
    let mut key = vec![1, 1];
    key.extend_from_slice(&e.target.entity.to_be_bytes());
    match n {
        1 => e.target.kind = UndoTargetKind::Row,
        2 => {
            e.target.kind = UndoTargetKind::Index;
            e.target.arity = 1;
            e.target.key = &key;
        }
        5 => e.target.entity += 1,
        6 => e.target.generation = g(5),
        _ => {}
    }
    assert!(decode_undo_record(&input, e, &[]).is_err(), "N{n:02}");
    assert_eq!(input, saved);
}
#[test]
fn catalog_before_target_and_stamp_rejections() {
    let mut h = Harness::new(ids("N", 1, 12));
    for n in 1..=12 {
        h.case(&format!("CB-N{n:02}"), || reject_root(n));
    }
    h.finish();
}
#[test]
fn catalog_before_shape_and_storage_rejections() {
    let mut owned = ids("N", 13, 36);
    owned.extend(ids("H", 1, 14));
    let mut h = Harness::new(owned);
    for n in 13..=36 {
        h.case(&format!("CB-N{n:02}"), || reject_root(n));
    }
    for n in 1..=14 {
        h.case(&format!("CB-H{n:02}"), || {
            let j = if (7..=11).contains(&n) || n == 13 {
                10
            } else if n == 12 {
                11
            } else {
                3
            };
            let mut b = value(j);
            let mut e = catalog_expected(j);
            match n {
                1..=4 => b.truncate([0, 4, 6, 95][n - 1]),
                5 => b[95] = 1,
                6 => b[6] = 3,
                7 => b[98] = 0,
                8 => b[98] = 4,
                9 => b[99] = 0,
                10 => b[99] = 5,
                11 => b[100] = 32,
                12 => b[101] = 0,
                13 => b[101] = 1,
                14 => e.namespace.timeline = 0,
                _ => unreachable!(),
            }
            let saved = b.clone();
            let result = decode_catalog_before(&b, e);
            if n == 14 {
                assert_eq!(result.unwrap_err(), CatalogError::InvalidInput);
            } else {
                assert!(result.is_err());
            }
            assert_eq!(b, saved);
        });
    }
    h.finish();
}
#[test]
fn catalog_before_chunk_materialization_is_typed() {
    let mut owned = ids("C", 1, 3);
    owned.extend(ids("N", 37, 40));
    owned.extend(ids("N", 49, 50));
    let mut h = Harness::new(owned);
    for (n, j) in [(1, 8), (2, 9), (3, 14)] {
        h.case(&format!("CB-C{n:02}"), || {
            let root = raw(&format!("P{j:02}"));
            let bytes = raw(&format!("C{n:02}"));
            let root_saved = root.clone();
            let saved = bytes.clone();
            let view = decode_undo_record(&root, expected(j), &[]).unwrap();
            let ctx = context(view);
            let current = page(30000 + j as u64, 5, 7);
            let chunk = decode_undo_chunk_cell(&bytes, ctx, current, 0).unwrap();
            assert_eq!(chunk.payload(), value(j));
            assert_eq!(chunk.next(), None);
            let mut output = vec![0xa5; bytes.len() + 16];
            assert_eq!(
                encode_undo_chunk_cell(ctx, current, 0, None, &value(j), &mut output).unwrap(),
                bytes.len()
            );
            assert_eq!(&output[..bytes.len()], bytes);
            assert!(output[bytes.len()..].iter().all(|v| *v == 0xa5));
            let mut verifier = UndoChunkVerifier::new(ctx);
            verifier.push(chunk).unwrap();
            verifier.finish().unwrap();
            if n == 3 {
                validate_release_chunk(view, chunk).unwrap();
                assert_eq!(
                    decode_materialized_catalog_before(view, chunk.payload()).unwrap_err(),
                    UndoError::StateMismatch
                );
            } else {
                check_before(
                    decode_materialized_catalog_before(view, chunk.payload()).unwrap(),
                    before_header(j),
                    &definition(j),
                );
            }
            assert_eq!(root, root_saved);
            assert_eq!(bytes, saved);
        });
    }
    for n in 37..=40 {
        h.case(&format!("CB-N{n:02}"), || {
            let mut root = raw("P08");
            let bytes = raw(&format!("N{n:02}"));
            if n == 40 {
                root[368..400].copy_from_slice(&content_digest(&bytes[160..]));
            }
            let saved = bytes.clone();
            let root_saved = root.clone();
            let view = decode_undo_record(&root, expected(8), &[]).unwrap();
            let ctx = context(view);
            let result = decode_undo_chunk_cell(&bytes, ctx, page(30008, 5, 7), 0);
            if n == 37 || n == 39 {
                assert!(result.is_err());
            } else {
                let chunk = result.unwrap();
                let mut verifier = UndoChunkVerifier::new(ctx);
                if n == 38 {
                    assert_eq!(&bytes[120..152], &root[368..400]);
                    assert_ne!(content_digest(chunk.payload()).as_slice(), &root[368..400]);
                    decode_catalog_before(chunk.payload(), catalog_expected(8)).unwrap();
                    assert_eq!(verifier.push(chunk), Err(UndoError::Corrupt));
                    assert_eq!(
                        decode_materialized_catalog_before(view, chunk.payload()).unwrap_err(),
                        UndoError::Corrupt
                    );
                } else {
                    verifier.push(chunk).unwrap();
                    verifier.finish().unwrap();
                    assert!(decode_materialized_catalog_before(view, chunk.payload()).is_err());
                }
            }
            assert_eq!(bytes, saved);
            assert_eq!(root, root_saved);
        });
    }
    for n in 49..=50 {
        h.case(&format!("CB-N{n:02}"), || {
            let bytes = raw(&format!("N{n:02}"));
            let saved = bytes.clone();
            let mut e = expected(8);
            if n == 49 {
                e.target.kind = UndoTargetKind::Row;
            }
            let view = decode_undo_record(&bytes, e, &[]).unwrap();
            let input = if n == 49 { value(8) } else { canonical_row() };
            let source = input.clone();
            let UndoPayload::Chunks(head) = view.live_before().unwrap().payload else {
                panic!("normal chunk root")
            };
            assert_eq!(head.total(), input.len());
            assert_eq!(head.digest(), content_digest(&input));
            if n == 49 {
                decode_catalog_before(&input, catalog_expected(8)).unwrap();
                assert_eq!(
                    decode_materialized_catalog_before(view, &input).unwrap_err(),
                    UndoError::StateMismatch
                );
            } else {
                let schema = [Column {
                    name: "v".to_owned(),
                    data_type: DataType::Binary,
                    nullable: false,
                }];
                crate::storage::row_record::decode_canonical_row(&input, &schema).unwrap();
                assert_eq!(
                    decode_materialized_before(view, &input, &schema).unwrap_err(),
                    UndoError::StateMismatch
                );
            }
            assert_eq!(input, source);
            assert_eq!(bytes, saved);
        });
    }
    h.finish();
}
fn canonical_row() -> Vec<u8> {
    let mut b = vec![1, 0, 0, 0, 4];
    b.extend_from_slice(&2040_u32.to_le_bytes());
    b.extend((0..2040).map(|i| ((17 * i + 3) % 256) as u8));
    b
}
fn legacy_accessor_input(n: usize) -> (Vec<u8>, UndoRecordExpectation<'static>, Vec<Column>) {
    let name = if n == 6 { "U04" } else { "U08" };
    let bytes = std::fs::read(repo().join(format!(
        "tests/fixtures/dev3-catalog-before/references/{name}.bin"
    )))
    .unwrap();
    assert_eq!(
        digest_hex(&bytes),
        if n == 6 {
            "c267a18f8ac192c9e4e9908ba54695c18854abe1a11e2fc9a7f88e589269f8be"
        } else {
            "84bea8378fa74d5830e29c3177c778da2acbc4ba54eaf34ab67a57e0a6e04c3d"
        }
    );
    let e = UndoRecordExpectation {
        namespace: ns(),
        reference: UndoRef::new(
            PageRef {
                file: FileId::new(0x1_0000_0002).unwrap(),
                page_no: 30,
                incarnation: g(4),
            },
            0,
            g(6),
        )
        .unwrap(),
        target: UndoTargetView {
            kind: if n == 6 {
                UndoTargetKind::Row
            } else {
                UndoTargetKind::Index
            },
            owner: oid(5),
            generation: g(6),
            entity: 258,
            arity: u8::from(n == 7),
            key: if n == 6 {
                &[]
            } else {
                &[1, 1, 0, 0, 0, 0, 0, 0, 1, 2]
            },
        },
    };
    let columns = if n == 6 {
        [
            DataType::Bool,
            DataType::Bool,
            DataType::Int64,
            DataType::Utf8,
            DataType::Binary,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, t)| Column {
            name: format!("c{i}"),
            data_type: t,
            nullable: i == 0,
        })
        .collect()
    } else {
        vec![]
    };
    (bytes, e, columns)
}
#[test]
fn catalog_before_release_has_no_live_accessor() {
    let mut owned = ids("N", 41, 48);
    owned.extend(ids("A", 1, 9));
    let mut h = Harness::new(owned);
    for n in 41..=48 {
        h.case(&format!("CB-N{n:02}"), || reject_root(n));
    }
    for n in 1..=9 {
        h.case(&format!("CB-A{n:02}"), || {
            let (bytes, e, columns) = match n {
                1..=4 => {
                    let j = [2, 13, 14, 15][n - 1];
                    (raw(&format!("P{j:02}")), expected(j), vec![])
                }
                5 => (raw("P13"), expected(13), vec![]),
                6 | 7 => legacy_accessor_input(n),
                8 => (raw("P08"), expected(8), vec![]),
                9 => {
                    let mut b = vec![0; 432];
                    b[..4].copy_from_slice(b"XUR1");
                    put(&mut b, 4, 1, 2);
                    put(&mut b, 8, 9, 8);
                    (b, expected(1), vec![])
                }
                _ => unreachable!(),
            };
            let saved = bytes.clone();
            let view = decode_undo_record(&bytes, e, &columns).unwrap();
            if n <= 4 {
                assert_eq!(view.live_before().unwrap_err(), UndoError::StateMismatch);
            } else {
                assert_eq!(
                    decode_inline_catalog_before(view).unwrap_err(),
                    UndoError::StateMismatch
                );
            }
            assert_eq!(bytes, saved);
        });
    }
    h.finish();
}
#[test]
fn catalog_before_loaded_version_identity() {
    let mut h = Harness::new(ids("L", 1, 6));
    for n in 1..=6 {
        h.case(&format!("CB-L{n:02}"), || {
            let mut source = raw("P03");
            let mut target = source.clone();
            let mut se = expected(3);
            let mut te = se;
            se.reference = reference(70001, 9);
            te.reference = reference(70002, 9);
            se.target.entity = 700001;
            te.target.entity = 700001;
            put(&mut source, 16, 200, 8);
            wire_tx(&mut source, 24, tx(18, 7001));
            put(&mut source, 72, 200, 8);
            put(&mut source, 112, 700001, 8);
            wire_tx(&mut source, 272, tx(16, 7002));
            put(&mut source, 312, 100, 8);
            wire_ref(&mut source, 168, te.reference);
            put(&mut source, 208, 100, 8);
            source[327] = 1;
            put(&mut target, 16, 100, 8);
            wire_tx(&mut target, 24, tx(16, 7002));
            put(&mut target, 64, 21, 8);
            put(&mut target, 112, 700001, 8);
            put(&mut target, 224, 76, 8);
            wire_tx(&mut target, 272, tx(15, 7003));
            match n {
                2 => {
                    te.target.entity = 700002;
                    put(&mut target, 112, 700002, 8);
                }
                3 => {
                    te.namespace.timeline = 18;
                    put(&mut target, 40, 18, 8);
                    put(&mut target, 288, 18, 8);
                }
                4 => put(&mut target, 16, 101, 8),
                5 => put(&mut target, 72, 101, 8),
                6 => {
                    te.reference = reference(70002, 10);
                    put(&mut target, 8, 10, 8);
                }
                _ => {}
            }
            let source_saved = source.clone();
            let target_saved = target.clone();
            let s = decode_undo_record(&source, se, &[]).unwrap();
            let t = decode_undo_record(&target, te, &[]).unwrap();
            let result = validate_loaded_edge(s, UndoEdgeRole::Version, t);
            assert_eq!(
                result,
                match n {
                    1 => Ok(()),
                    4 => Err(UndoError::SequenceMismatch),
                    _ => Err(UndoError::IdentityMismatch),
                }
            );
            assert_eq!(source, source_saved);
            assert_eq!(target, target_saved);
        });
    }
    h.finish();
}
#[test]
fn catalog_before_encoder_failure_preserves_output() {
    let mut owned = ids("E", 1, 8);
    owned.extend(ids("I", 1, 10));
    let mut h = Harness::new(owned);
    for n in 1..=8 {
        h.case(&format!("CB-E{n:02}"), || {
            let (j, required) = match n {
                1 | 2 => (3, 106),
                3 | 4 => (7, 2048),
                5 | 6 => (9, 432),
                7 | 8 => (10, 537),
                _ => unreachable!(),
            };
            let payload = value(j);
            let saved = payload.clone();
            let d = definition(j);
            let d_saved = d.clone();
            let mut out = vec![0xa5; required + 16];
            let length = if n % 2 == 1 { required - 1 } else { 0 };
            if n <= 4 {
                assert_eq!(
                    encode_catalog_before(
                        catalog_expected(j),
                        before_header(j),
                        d.borrowed(),
                        &mut out[..length]
                    ),
                    Err(CatalogError::NoSpace)
                );
            } else {
                assert_eq!(
                    encode_undo_record(cell(j, &payload), expected(j), &[], &mut out[..length]),
                    Err(UndoError::NoSpace)
                );
            }
            assert_eq!(out, vec![0xa5; required + 16]);
            assert_eq!(payload, saved);
            assert_eq!(d, d_saved);
        });
    }
    for n in 1..=10 {
        h.case(&format!("CB-I{n:02}"), || {
            let j = match n {
                4 => 6,
                5 => 10,
                6 => 11,
                9 => 1,
                _ => 3,
            };
            let payload = value(if n == 9 { 3 } else { j });
            let payload_saved = payload.clone();
            let mut d = definition(j);
            let mut header = before_header(j);
            let mut e = catalog_expected(j);
            match n {
                1..=3 => {
                    let Definition::Table(t) = &mut d else {
                        unreachable!()
                    };
                    t.name = match n {
                        1 => String::new(),
                        2 => "a".repeat(129),
                        _ => "\0".to_owned(),
                    };
                }
                4 => {
                    let Definition::Table(t) = &mut d else {
                        unreachable!()
                    };
                    t.columns[1].name = t.columns[0].name.clone();
                }
                5 => {
                    let Definition::Index { parent, .. } = &mut d else {
                        unreachable!()
                    };
                    parent.object = e.object;
                }
                6 => {
                    let Definition::Index { columns, .. } = &mut d else {
                        unreachable!()
                    };
                    columns[1] = 0;
                }
                7 => header.identity.object = oid(e.object.get() + 1),
                10 => e.namespace.timeline = 0,
                _ => {}
            }
            let d_saved = d.clone();
            let header_saved = header;
            let e_saved = e;
            if n == 8 || n == 9 {
                let mut r = record(j, &payload);
                r.payload = if n == 8 {
                    UndoPayload::None
                } else {
                    UndoPayload::Inline(&payload)
                };
                let saved = r;
                let mut out = [0xa5; 2496];
                assert_eq!(
                    encode_undo_record(UndoCell::Live(r), expected(j), &[], &mut out),
                    Err(UndoError::InvalidInput)
                );
                assert_eq!(out, [0xa5; 2496]);
                assert_eq!(r, saved);
            } else {
                let mut out = [0xa5; 4468];
                assert_eq!(
                    encode_catalog_before(e, header, d.borrowed(), &mut out),
                    Err(if n == 7 {
                        CatalogError::IdentityMismatch
                    } else {
                        CatalogError::InvalidInput
                    })
                );
                assert_eq!(out, [0xa5; 4468]);
            }
            assert_eq!(payload, payload_saved);
            assert_eq!(d, d_saved);
            assert_eq!(header, header_saved);
            assert_eq!(e, e_saved);
        });
    }
    h.finish();
}
#[test]
fn catalog_before_xct1_definition_bridge() {
    let mut h = Harness::new(ids("B", 1, 2));
    for (n, j) in [(1, 6), (2, 12)] {
        h.case(&format!("CB-B{n:02}"), || {
            let payload = value(j);
            let d = definition(j);
            let bh = before_header(j);
            let r = record(j, &payload);
            let header = CatalogHeader {
                identity: bh.identity,
                retired: bh.retired,
                stamp: r.before_stamp.unwrap(),
                logical: r.before_logical.unwrap(),
                undo: Some(expected(j).reference),
                anchor: bh.anchor,
            };
            let body = d.body();
            let mut literal = vec![0; 232];
            literal[..4].copy_from_slice(b"XCT1");
            put(&mut literal, 4, 1, 2);
            literal[6] = if j == 6 { 1 } else { 2 };
            literal[7] = 2 | u8::from(bh.retired);
            put(&mut literal, 8, bh.identity.object.get(), 8);
            put(&mut literal, 16, 7, 8);
            put(&mut literal, 24, 8, 8);
            literal[32] = 1;
            put(&mut literal, 40, 77, 8);
            wire_tx(&mut literal, 88, tx(16, 3000 + j as u64));
            put(&mut literal, 128, 50, 8);
            wire_ref(&mut literal, 136, expected(j).reference);
            wire_page(&mut literal, 176, bh.anchor);
            if j == 12 {
                put(&mut literal, 200, 50, 8);
                put(&mut literal, 208, 7, 8);
                put(&mut literal, 216, 8, 8);
            }
            put(&mut literal, 224, body.len() as u64, 4);
            literal.extend_from_slice(&body);
            assert_eq!(literal.len(), if n == 1 { 257 } else { 241 });
            let mut xcb = vec![0xa5; payload.len() + 16];
            assert_eq!(
                encode_catalog_before(catalog_expected(j), bh, d.borrowed(), &mut xcb).unwrap(),
                payload.len()
            );
            assert_eq!(&xcb[..payload.len()], payload);
            assert!(xcb[payload.len()..].iter().all(|b| *b == 0xa5));
            check_before(
                decode_catalog_before(&xcb[..payload.len()], catalog_expected(j)).unwrap(),
                bh,
                &d,
            );
            let mut xct = vec![0xa5; literal.len() + 16];
            assert_eq!(
                encode_catalog_record(catalog_expected(j), header, d.borrowed(), &mut xct).unwrap(),
                literal.len()
            );
            assert_eq!(&xct[..literal.len()], literal);
            assert!(xct[literal.len()..].iter().all(|b| *b == 0xa5));
            let view = decode_catalog_record(&xct[..literal.len()], catalog_expected(j)).unwrap();
            assert_eq!(view.header(), header);
            d.check(view.definition());
            assert_eq!(&xcb[96..payload.len()], &xct[232..literal.len()]);
        });
    }
    h.finish();
}
#[test]
fn catalog_before_static_source_census() {
    let mut h = Harness::new(vec!["CB-ST01".to_owned()]);
    h.case("CB-ST01", || {
        assert_eq!(CATALOG_BEFORE_HEADER_BYTES, 96);
        assert_eq!(MAX_CATALOG_BEFORE_BYTES, 4452);
        assert_eq!(UNDO_RECORD_BYTES + 2048, 2480);
        assert_eq!(MAX_CATALOG_BEFORE_BYTES.div_ceil(UNDO_CHUNK_PAYLOAD), 1);
        assert_eq!(UNDO_CHUNK_HEADER_BYTES + MAX_CATALOG_BEFORE_BYTES, 4612);
        let census = super::before::source_census();
        assert_eq!(
            census.map(|(name, _)| name),
            [
                "nominal_caller",
                "retained_inputs_returns",
                "xcb1_codecs_accessors",
                "shared_definition_helpers",
                "xct1_bridge",
                "undo_root_codecs",
                "undo_accessors_edges",
                "chunk_cell_codecs",
                "chunk_verifier_hash",
                "wire_identity_helpers"
            ]
        );
        for (name, bytes) in census {
            println!("CATALOG_BEFORE_CENSUS\t{name}\t{bytes}");
            assert!(bytes > 0);
        }
        assert!(census[0].1 <= 24576, "nominal caller exceeds 24576");
        let total: usize = census.into_iter().map(|(_, bytes)| bytes).sum();
        println!("CATALOG_BEFORE_CENSUS\tconservative_total\t{total}");
        assert!(total <= 65536, "source value total exceeds 65536");
    });
    h.finish();
}
// S5 expected bytes use explicit packing, never a production encoder result.
fn independent_root(e: UndoRecordExpectation<'_>, r: UndoRecord<'_>) -> Vec<u8> {
    let mut b = vec![0; 432];
    b[..4].copy_from_slice(b"XUR1");
    put(&mut b, 4, 1, 2);
    b[6] = 1;
    put(&mut b, 8, e.reference.generation().get(), 8);
    put(&mut b, 16, r.sequence, 8);
    wire_tx(&mut b, 24, r.creator.id);
    put(&mut b, 64, r.creator.status_generation.get(), 8);
    put(&mut b, 72, r.mutation_sequence, 8);
    put(&mut b, 80, r.incoming, 8);
    b[88] = 3;
    put(&mut b, 96, r.target.owner.get(), 8);
    put(&mut b, 104, r.target.generation.get(), 8);
    put(&mut b, 112, r.target.entity, 8);
    match r.before_stamp.unwrap() {
        VersionStamp::Committed(q) => {
            b[216] = 1;
            put(&mut b, 224, q.get(), 8);
        }
        VersionStamp::Transaction(t) => {
            b[216] = 2;
            wire_tx(&mut b, 224, t.id());
            put(&mut b, 264, t.generation().get(), 8);
        }
    }
    let logical = r.before_logical.unwrap();
    wire_tx(&mut b, 272, logical.transaction());
    put(&mut b, 312, logical.mutation_sequence(), 8);
    b[324] = 5;
    match r.payload {
        UndoPayload::Inline(bytes) => {
            put(&mut b, 320, bytes.len() as u64, 4);
            b[325] = 1;
            b.extend_from_slice(bytes);
        }
        UndoPayload::Chunks(head) => {
            put(&mut b, 320, head.total() as u64, 4);
            b[325] = 2;
            wire_page(&mut b, 336, head.root());
            put(&mut b, 360, head.total() as u64, 4);
            put(&mut b, 364, 1, 2);
            b[368..400].copy_from_slice(&head.digest());
        }
        UndoPayload::None => panic!("generated profile requires value"),
    }
    b
}
fn independent_chunk(
    e: UndoRecordExpectation<'_>,
    sequence: u64,
    current: PageRef,
    payload: &[u8],
) -> Vec<u8> {
    let mut b = vec![0; 160];
    b[..4].copy_from_slice(b"XUC1");
    put(&mut b, 4, 1, 2);
    put(&mut b, 8, 1, 2);
    put(&mut b, 12, payload.len() as u64, 4);
    put(&mut b, 16, payload.len() as u64, 4);
    wire_ref(&mut b, 24, e.reference);
    put(&mut b, 64, sequence, 8);
    wire_page(&mut b, 72, current);
    b[120..152].copy_from_slice(&content_digest(payload));
    put(&mut b, 152, e.namespace.timeline, 8);
    b.extend_from_slice(payload);
    b
}
fn generated_definition(s: usize, p: usize) -> Definition {
    if (4..=6).contains(&p) {
        return Definition::Index {
            parent: identity(50, 5 + (s % 7) as u64, 100 + s as u64),
            name: format!("I{p}{s:02}{}", "a".repeat(s)),
            mode: [
                IndexMode::PrimaryKey,
                IndexMode::Unique,
                IndexMode::Nonunique,
            ][p - 4],
            columns: (0..1 + s % 4).map(|j| ((s + 3 * j) % 32) as u8).collect(),
        };
    }
    if p == 7 {
        return Definition::Table(TableSpec {
            name: format!("테이블{s:02}"),
            columns: (0..4)
                .map(|i| Column {
                    name: format!("열{i:02}"),
                    data_type: dtype(i + 1),
                    nullable: (s >> i) & 1 != 0,
                })
                .collect(),
        });
    }
    let long = p >= 2;
    Definition::Table(TableSpec {
        name: named("T", if long { 101 + s % 28 } else { 1 + s }),
        columns: (0..if long { 14 } else { 1 + s % 8 })
            .map(|i| Column {
                name: if long {
                    named(&format!("c{i:02}{s:02}"), 128)
                } else {
                    format!("c{i:02}{s:02}{}", "a".repeat(s % 8))
                },
                data_type: dtype(1 + (s + i) % 4),
                nullable: !(s + i).is_multiple_of(2),
            })
            .collect(),
    })
}
#[test]
#[ignore = "DEV3 final S5 expanded inventory; select exact cases separately"]
fn catalog_before_s5_generated_definitions() {
    let owned = (0..512).map(|n| format!("CB-G{n:03}")).collect();
    let mut h = Harness::new(owned);
    for n in 0..512 {
        h.case(&format!("CB-G{n:03}"), || {
            let (s, p) = (n / 8, n % 8);
            let d = generated_definition(s, p);
            let header = CatalogBeforeHeader {
                identity: identity(200000 + n as u64, 10 + (s % 7) as u64, 300 + s as u64),
                retired: if p < 4 { p % 2 == 1 } else { (s >> 1) & 1 != 0 },
                anchor: page(60000 + n as u64, 6, 5),
            };
            let ce = CatalogExpectation {
                namespace: ns(),
                object: header.identity.object,
            };
            let payload = independent_payload(header, &d);
            let saved_payload = payload.clone();
            let mut e = expected(3);
            e.target.entity = 200000 + n as u64;
            e.reference = reference(50000 + n as u64, 100 + s as u64);
            let creator = UndoCreator {
                id: tx(18 + (s % 3) as u32, 20000 + n as u64),
                status_generation: g(20 + s as u64),
            };
            let own = s % 2 == 0;
            let r = UndoRecord {
                creator,
                sequence: 100000 + n as u64,
                mutation_sequence: 100 + s as u64,
                incoming: (s % 3) as u64,
                target: e.target,
                edges: [UndoEdge::Never; 2],
                before_kind: UndoBeforeKind::CatalogValue,
                before_stamp: Some(if own {
                    VersionStamp::Transaction(
                        TxnRef::new(creator.id, creator.status_generation).unwrap(),
                    )
                } else {
                    VersionStamp::Committed(CommitSeq::new(1000 + n as u64).unwrap())
                }),
                before_logical: Some(
                    LogicalRowVersion::new(
                        if own {
                            creator.id
                        } else {
                            tx(9 + (s % 3) as u32, 40000 + n as u64)
                        },
                        if own { 99 + s as u64 } else { 50 + s as u64 },
                    )
                    .unwrap(),
                ),
                payload: if p == 2 || p == 3 {
                    UndoPayload::Chunks(
                        UndoPayloadHead::new(
                            page(70000 + n as u64, 5, 7),
                            payload.len(),
                            content_digest(&payload),
                        )
                        .unwrap(),
                    )
                } else {
                    UndoPayload::Inline(&payload)
                },
                release: UndoRelease {
                    cursor: 0,
                    chunks_released: 0,
                    next: None,
                },
            };
            assert_eq!(
                payload.len(),
                match p {
                    2 | 3 => 2049 + s % 28,
                    4..=6 => 108 + s,
                    7 => 147,
                    _ => payload.len(),
                }
            );
            if p < 2 {
                assert!(payload.len() <= 292);
            }
            let mut xcb = vec![0xa5; payload.len() + 16];
            assert_eq!(
                catalog_before_encoded_len(ce, header, d.borrowed()).unwrap(),
                payload.len()
            );
            assert_eq!(
                encode_catalog_before(ce, header, d.borrowed(), &mut xcb).unwrap(),
                payload.len()
            );
            assert_eq!(&xcb[..payload.len()], payload);
            assert!(xcb[payload.len()..].iter().all(|b| *b == 0xa5));
            check_before(decode_catalog_before(&payload, ce).unwrap(), header, &d);
            let root = independent_root(e, r);
            let saved = root.clone();
            let mut out = vec![0xa5; root.len() + 16];
            assert_eq!(
                encode_undo_record(UndoCell::Live(r), e, &[], &mut out).unwrap(),
                root.len()
            );
            assert_eq!(&out[..root.len()], root);
            assert!(out[root.len()..].iter().all(|b| *b == 0xa5));
            let view = decode_undo_record(&root, e, &[]).unwrap();
            assert_eq!(view.reference(), e.reference);
            assert_eq!(view.live_before().unwrap(), r);
            if p == 2 || p == 3 {
                let current = page(70000 + n as u64, 5, 7);
                let chunk = independent_chunk(e, r.sequence, current, &payload);
                let ctx = context(view);
                let cv = decode_undo_chunk_cell(&chunk, ctx, current, 0).unwrap();
                let mut encoded = vec![0xa5; chunk.len() + 16];
                assert_eq!(
                    encode_undo_chunk_cell(ctx, current, 0, None, &payload, &mut encoded).unwrap(),
                    chunk.len()
                );
                assert_eq!(&encoded[..chunk.len()], chunk);
                assert!(encoded[chunk.len()..].iter().all(|b| *b == 0xa5));
                let mut verifier = UndoChunkVerifier::new(ctx);
                verifier.push(cv).unwrap();
                verifier.finish().unwrap();
                check_before(
                    decode_materialized_catalog_before(view, cv.payload()).unwrap(),
                    header,
                    &d,
                );
            } else {
                check_before(
                    decode_inline_catalog_before(view).unwrap().unwrap(),
                    header,
                    &d,
                );
            }
            assert_eq!(payload, saved_payload);
            assert_eq!(root, saved);
        });
    }
    h.finish();
}
fn truncation_base(k: usize) -> (Vec<u8>, CatalogExpectation) {
    let j = [3, 7, 8, 9, 10, 11][k - 1];
    let mut d = definition(j);
    if let Definition::Index { parent, .. } = &mut d {
        *parent = identity(60, 17, 18);
    }
    let header = CatalogBeforeHeader {
        identity: identity(900000 + k as u64, 17, 18),
        retired: false,
        anchor: page(80000 + k as u64, 6, 9),
    };
    (
        independent_payload(header, &d),
        CatalogExpectation {
            namespace: ns(),
            object: header.identity.object,
        },
    )
}
#[test]
#[ignore = "DEV3 final S5 expanded inventory; select exact cases separately"]
fn catalog_before_s5_all_prefixes_and_trailing() {
    let mut owned = vec![];
    for (k, len) in [106, 2048, 2049, 4452, 105, 232].into_iter().enumerate() {
        owned.extend((0..len).map(|d| format!("CB-T{:02}-{d:04}", k + 1)));
        owned.push(format!("CB-T{:02}-TAIL", k + 1));
    }
    let mut h = Harness::new(owned);
    for (k, length) in [106, 2048, 2049, 4452, 105, 232].into_iter().enumerate() {
        let (k, b) = (k + 1, truncation_base(k + 1));
        let (bytes, e) = b;
        assert_eq!(bytes.len(), length);
        for d in 0..length {
            h.case(&format!("CB-T{k:02}-{d:04}"), || {
                let input = bytes[..d].to_vec();
                let saved = input.clone();
                assert!(decode_catalog_before(&input, e).is_err());
                assert_eq!(input, saved);
            });
        }
        h.case(&format!("CB-T{k:02}-TAIL"), || {
            let mut input = bytes.clone();
            input.push(0);
            let saved = input.clone();
            assert!(decode_catalog_before(&input, e).is_err());
            assert_eq!(input, saved);
        });
    }
    h.finish();
}
#[test]
#[ignore = "DEV3 final S5 expanded inventory; select exact cases separately"]
fn catalog_before_s5_reserved_and_flags() {
    let mut h = Harness::new(ids("W", 1, 24));
    for n in 1..=24 {
        h.case(&format!("CB-W{n:02}"), || {
            let (mut input, e) = truncation_base(1);
            if n <= 18 {
                input[76 + n] = 1;
            } else {
                input[7] = 1 << (n - 17);
            }
            let saved = input.clone();
            assert!(decode_catalog_before(&input, e).is_err());
            assert_eq!(input, saved);
        });
    }
    h.finish();
}
