//! Inputs are native Row/Job/U cells from independent D/U literal histories.
//! Only lower codecs assemble input pages; no scope3 producer/observer or
//! prior transition suite runs. Expected restore costs use the literals'
//! before/after Row cells, independently of Job counts and the U decoder.
use super::*;
use crate::{
    api::{Column, DataType},
    storage::{
        allocation_page::{self, ALLOCATION_CELL_BYTES, AllocationEntry, EXTENT_PAGES},
        control::{self, Profile},
        page::{self, FileId, PageHeader, PageLsn},
        retention_page::{JOB_BYTES, JOB_COUNT},
        semantic_row,
        wal_frame::{self, WalStreamIdentity},
    },
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, SchemaEpoch},
};
use std::collections::BTreeMap;

const PROFILE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/scope3/profile3.bin"
));
const D: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition-delete/blobs.bin"
));
const DI: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition-delete/index.tsv"
));
const U: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition-update/blobs.bin"
));
const UI: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition-update/index.tsv"
));
type Page = [u8; PAGE_BYTES];

fn literal(name: &str) -> &'static [u8] {
    let (index, bytes) = if name.starts_with("D-") {
        (DI, D)
    } else {
        (UI, U)
    };
    let fields: Vec<_> = index
        .lines()
        .find(|line| line.split('\t').next() == Some(name))
        .unwrap()
        .split('\t')
        .collect();
    let at: usize = fields[1].parse().unwrap();
    let length: usize = fields[2].parse().unwrap();
    let bytes = &bytes[at..at + length];
    let hash: String = content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(hash, fields[3]);
    bytes
}
fn one() -> Generation {
    Generation::new(1).unwrap()
}
fn profile() -> Profile {
    control::row_profile::decode_profile(PROFILE, None).unwrap()
}
fn reference(page_no: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0001).unwrap(),
        page_no,
        incarnation: Generation::new(if page_no == 0 { 1 } else { 2 }).unwrap(),
    }
}
fn identity(page_no: u64, owner: u64) -> PageIdentity {
    PageIdentity {
        database: profile().spec().namespace.database,
        page: reference(page_no),
        owner: ObjectId::new(owner).unwrap(),
        owner_generation: one(),
    }
}
fn table() -> ObjectHandle {
    ObjectHandle {
        object: ObjectId::new(6).unwrap(),
        generation: one(),
        schema_epoch: SchemaEpoch::new(1).unwrap(),
    }
}
fn frame(lsn: u64) -> FrameExpectation {
    FrameExpectation {
        stream: WalStreamIdentity {
            database: profile().spec().namespace.database,
            timeline: 1,
            writer_epoch: 1,
        },
        previous_lsn: lsn,
    }
}
fn columns() -> [Column; 2] {
    [
        Column {
            name: "n".into(),
            data_type: DataType::Int64,
            nullable: false,
        },
        Column {
            name: "t".into(),
            data_type: DataType::Utf8,
            nullable: true,
        },
    ]
}

struct Fixture {
    lsn: u64,
    rows: BTreeMap<u64, Vec<u8>>,
    undo: BTreeMap<u64, Vec<u8>>,
    jobs: Box<[[u8; JOB_BYTES]; JOB_COUNT]>,
    oracle: BTreeMap<u64, usize>,
}
impl Fixture {
    fn new(names: &[&str]) -> Self {
        let mut value = Self {
            lsn: 1,
            rows: BTreeMap::new(),
            undo: BTreeMap::new(),
            jobs: vec![[0; JOB_BYTES]; JOB_COUNT]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            oracle: BTreeMap::new(),
        };
        for job in value.jobs.iter_mut() {
            row::free_job(one(), job);
        }
        for name in names {
            value.add(name);
        }
        value
    }
    fn add(&mut self, name: &str) {
        let bytes = literal(name);
        let previous = u64::from_le_bytes(bytes[64..72].try_into().unwrap());
        let layout = semantic_row::decode_layout(bytes, frame(previous), profile()).unwrap();
        self.lsn = layout.header().lsn;
        let mut raw = layout.redo();
        let mut targets = Vec::new();
        while !raw.is_empty() {
            let n = u32::from_le_bytes(raw[4..8].try_into().unwrap()) as usize;
            targets.push(
                wal_frame::decode_redo_record(layout.header(), &raw[..n])
                    .unwrap()
                    .target,
            );
            raw = &raw[n..];
        }
        for native in layout.native_records() {
            let Some(after) = native.after() else {
                continue;
            };
            match native.kind() {
                1 => {
                    let id = u64::from_be_bytes(after[4..12].try_into().unwrap());
                    let restore = if matches!(
                        layout.operation(),
                        semantic_row::Operation::Forward
                            | semantic_row::Operation::DeleteForward
                            | semantic_row::Operation::UpdateForward
                    ) {
                        native.before().map_or(0, |before| before.len() + 4)
                    } else {
                        0
                    };
                    self.oracle.insert(id, (after.len() + 4).max(restore));
                    self.rows.insert(id, after.to_vec());
                }
                3 | 4 => {
                    self.undo.insert(
                        targets[usize::from(native.redo_index())].page.page_no,
                        after.to_vec(),
                    );
                }
                9 => {
                    let key = &after[128..168];
                    let at = self
                        .jobs
                        .iter()
                        .position(|job| job[6] != 0 && &job[128..168] == key)
                        .or_else(|| self.jobs.iter().position(|job| job[6] == 0))
                        .unwrap();
                    self.jobs[at].copy_from_slice(after);
                }
                _ => (),
            }
        }
    }
    fn leaf(&self) -> Box<Page> {
        let metadata = b"XBT1\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let cells: Vec<&[u8]> = std::iter::once(metadata.as_slice())
            .chain(self.rows.values().map(Vec::as_slice))
            .collect();
        let mut out = Box::new([0; PAGE_BYTES]);
        page::encode_page(
            PageHeader {
                identity: identity(13, 6),
                kind: PageKind::RowLeaf,
                level: 0,
                lsn: PageLsn(self.lsn),
            },
            &cells,
            out.as_mut(),
        )
        .unwrap();
        out
    }
    fn undo_pages(&self) -> BTreeMap<u64, Box<Page>> {
        self.undo
            .iter()
            .map(|(&number, cell)| {
                let mut page = Box::new([0; PAGE_BYTES]);
                page::encode_page(
                    PageHeader {
                        identity: identity(number, 3),
                        kind: PageKind::Undo,
                        level: 0,
                        lsn: PageLsn(self.lsn),
                    },
                    &[cell],
                    page.as_mut(),
                )
                .unwrap();
                (number, page)
            })
            .collect()
    }
    fn allocation(&self) -> Box<Page> {
        let mut entries = [AllocationEntry::Free(one()); EXTENT_PAGES];
        let owned = std::iter::once((0, 1, PageKind::Allocation))
            .chain([(12, 6, PageKind::TreeAnchor), (13, 6, PageKind::RowLeaf)])
            .chain(self.undo.keys().map(|n| (*n, 3, PageKind::Undo)));
        for (n, owner, kind) in owned {
            entries[n as usize] = AllocationEntry::Allocated {
                generation: reference(n).incarnation,
                object: ObjectId::new(owner).unwrap(),
                object_generation: one(),
                kind,
            };
        }
        let header = PageHeader {
            identity: identity(0, 1),
            kind: PageKind::Allocation,
            level: 0,
            lsn: PageLsn(self.lsn),
        };
        let mut cell = [0; ALLOCATION_CELL_BYTES];
        allocation_page::encode_allocation_cell(
            header,
            profile().spec().namespace,
            &entries,
            &mut cell,
        )
        .unwrap();
        let mut out = Box::new([0; PAGE_BYTES]);
        allocation_page::encode_allocation_page(
            header,
            profile().spec().namespace,
            &cell,
            out.as_mut(),
        )
        .unwrap();
        out
    }
    fn observe(&self) -> Result<(Weights, Vec<usize>)> {
        let leaf = self.leaf();
        let allocation = self.allocation();
        let allocation = allocation_page::decode_allocation_page(
            allocation.as_ref(),
            identity(0, 1),
            profile().spec().namespace,
        )
        .unwrap();
        let undo = self.undo_pages();
        let cols = columns();
        let input = Inputs {
            frame: frame(self.lsn),
            table: table(),
            anchor: reference(12),
            schema: RowSchemaInput::Columns(&cols),
            leaf: TreeExpectation {
                identity: identity(13, 6),
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
            image: &leaf,
            jobs: JobCells::original(&self.jobs),
            allocation: &allocation,
        };
        let mut out = vec![usize::MAX; self.rows.len() + 2];
        let result = derive(&input, &mut out, |r| {
            undo.get(&r.page_no)
                .map(Box::as_ref)
                .ok_or(Error::Exhausted)
        })?;
        assert_eq!(&out[self.rows.len()..], &[usize::MAX; 2]);
        out.truncate(self.rows.len());
        Ok((result, out))
    }
}

#[test]
fn structural_weights_use_real_delete_update_and_missing_before_cells() {
    for names in [
        &["D-524"][..],
        &["D-524", "D-526"],
        &["U-558"],
        &["U-558", "U-560"],
        &["U-558", "U-565"],
        &["U-562"],
    ] {
        let fixture = Fixture::new(names);
        let (summary, weights) = fixture
            .observe()
            .unwrap_or_else(|e| panic!("{names:?}: {e:?}"));
        let expected: Vec<_> = fixture.oracle.values().copied().collect();
        assert_eq!(weights, expected, "{names:?}");
        assert_eq!(summary.rows, fixture.rows.len());
        assert_eq!(summary.restorable, expected.iter().sum());
        assert_eq!(
            summary.actual,
            fixture.rows.values().map(|cell| cell.len() + 4).sum()
        );
        println!(
            "STRUCTURAL_WEIGHTS {names:?} weights={weights:?} actual={} restorable={}",
            summary.actual, summary.restorable
        );
    }
}

#[test]
fn structural_weights_allow_historical_leaf_hint_but_reject_reserved_and_aliases() {
    let mut fixture = Fixture::new(&["U-558", "U-560"]);
    fixture.jobs[0][232..240].copy_from_slice(&40u64.to_le_bytes());
    fixture.jobs[1][232..240].copy_from_slice(&41u64.to_le_bytes());
    assert_eq!(fixture.observe().unwrap().1, vec![228, 208]);
    for variant in 0..4 {
        let mut bad = Fixture::new(&["U-558", "U-560"]);
        match variant {
            0 => {
                bad.add("U-564");
            } // Global Reserved, before any structural work.
            1 => bad.jobs[2] = bad.jobs[0],
            2 => {
                let sequence: [u8; 8] = bad.jobs[0][168..176].try_into().unwrap();
                bad.jobs[1][168..176].copy_from_slice(&sequence);
            }
            3 => {
                let address: [u8; 24] = bad.jobs[0][128..152].try_into().unwrap();
                bad.jobs[1][128..152].copy_from_slice(&address);
            }
            _ => unreachable!(),
        }
        assert!(bad.observe().is_err(), "mutation {variant}");
    }
}

#[test]
fn structural_weights_reject_real_undo_identity_edge_and_typed_before_mutations() {
    for variant in 0..7 {
        let mut bad = Fixture::new(&["U-558", "U-560"]);
        let cell = bad.undo.get_mut(&11).unwrap();
        match variant {
            0 => cell[96..104].copy_from_slice(&7u64.to_le_bytes()), // target owner
            1 => cell[48..56].copy_from_slice(&99u64.to_le_bytes()), // creator identity
            2 => cell[168..176].copy_from_slice(&999u64.to_le_bytes()), // previous-version Never
            3 => cell[432] = 0xff, // full schema decode must inspect actual RowValue
            4 => cell[6] = 2,      // Releasing cannot supply a live before
            5 => cell[8..16].copy_from_slice(&99u64.to_le_bytes()), // slot generation
            6 => cell[128..136].copy_from_slice(&15u64.to_le_bytes()), // valid but wrong transaction predecessor page
            _ => unreachable!(),
        }
        if variant == 3 {
            let namespace = profile().spec().namespace;
            let job = row::decode_job(&bad.jobs[1], namespace).unwrap();
            let previous = row::decode_job(&bad.jobs[0], namespace).unwrap();
            // The old outline observer accepts these metadata-identical
            // bytes. Only the new typed before check can reject their value.
            cells::check_live(
                namespace,
                job,
                UndoEdge::Linked(cells::link(previous).unwrap()),
                cell,
            )
            .unwrap();
        }
        assert!(bad.observe().is_err(), "Undo mutation {variant}");
    }
}

#[test]
fn structural_weights_reject_current_version_projection_and_committed_tombstone() {
    for variant in 0..4 {
        let mut bad = Fixture::new(&["U-558"]);
        match variant {
            0 => bad.jobs[0][288] ^= 1,
            1 => bad.jobs[0][224..232].copy_from_slice(&2u64.to_le_bytes()), // wrong file in hint remains structurally invalid
            2 => row::free_job(one(), &mut bad.jobs[0]),
            3 => {
                let cols = columns();
                let bytes = bad.rows.get(&9).unwrap();
                let expected = RecordExpectation {
                    namespace: profile().spec().namespace,
                    row: RowId::new(9).unwrap(),
                };
                let decoded = row_record::decode_row_record(&bytes[12..], expected, &cols).unwrap();
                let mut record = [0; row_record::ROW_HEADER_BYTES];
                row_record::encode_row_record_with_schema(
                    expected,
                    RowSchemaInput::Columns(&cols),
                    RecordVersion {
                        stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
                        undo: None,
                        ..decoded.version()
                    },
                    RowPayload::Deleted,
                    &mut record,
                )
                .unwrap();
                let mut cell = vec![0; record.len() + 12];
                tree_page::encode_leaf_cell(
                    PageKind::RowLeaf,
                    0,
                    &9u64.to_be_bytes(),
                    &record,
                    &mut cell,
                )
                .unwrap();
                bad.rows.insert(9, cell);
                row::free_job(one(), &mut bad.jobs[0]);
            }
            _ => unreachable!(),
        }
        assert!(bad.observe().is_err(), "row mutation {variant}");
    }
}

#[test]
fn structural_weights_check_capacity_cut_allocation_and_callback_error() {
    let fixture = Fixture::new(&["U-558"]);
    let leaf = fixture.leaf();
    let allocation_bytes = fixture.allocation();
    let allocation = allocation_page::decode_allocation_page(
        allocation_bytes.as_ref(),
        identity(0, 1),
        profile().spec().namespace,
    )
    .unwrap();
    let undo = fixture.undo_pages();
    let cols = columns();
    let mut input = Inputs {
        frame: frame(fixture.lsn),
        table: table(),
        anchor: reference(12),
        schema: RowSchemaInput::Columns(&cols),
        leaf: TreeExpectation {
            identity: identity(13, 6),
            kind: PageKind::RowLeaf,
            level: 0,
            arity: 0,
        },
        image: &leaf,
        jobs: JobCells::original(&fixture.jobs),
        allocation: &allocation,
    };
    assert_eq!(
        derive(&input, &mut [], |_| panic!(
            "capacity must precede callback"
        )),
        Err(Error::Exhausted)
    );
    assert_eq!(
        derive(&input, &mut [0], |_| Err(Error::Unsupported)),
        Err(Error::Unsupported)
    );
    input.frame.previous_lsn -= 1;
    assert_eq!(
        derive(&input, &mut [0], |_| panic!("cut must precede callback")),
        Err(Error::IdentityMismatch)
    );
    input.frame.previous_lsn += 1;
    input.leaf.identity.page.incarnation = Generation::new(3).unwrap();
    assert!(
        derive(&input, &mut [0], |_| panic!(
            "identity must precede callback"
        ))
        .is_err()
    );
    input.leaf.identity.page.incarnation = Generation::new(2).unwrap();
    let mut absent = Fixture::new(&["U-558"]);
    absent.undo.clear();
    let allocation_bytes = absent.allocation();
    let unowned = allocation_page::decode_allocation_page(
        allocation_bytes.as_ref(),
        identity(0, 1),
        profile().spec().namespace,
    )
    .unwrap();
    input.allocation = &unowned;
    assert_eq!(
        derive(&input, &mut [0], |_| panic!(
            "allocation must precede callback"
        )),
        Err(Error::IdentityMismatch)
    );
    input.allocation = &allocation;
    assert_eq!(
        derive(&input, &mut [0], |r| Ok(undo[&r.page_no].as_ref()))
            .unwrap()
            .restorable,
        228
    );
}

#[test]
fn structural_weights_enforce_actual_plus_undo_capacity_at_exact_boundary() {
    for extra in [0, 1] {
        let mut fixture = Fixture::new(&["U-558", "U-560"]);
        let cols = columns();
        let schema = RowSchemaInput::Columns(&cols);
        let source = row_record::decode_row_record(
            &fixture.rows[&9][12..],
            RecordExpectation {
                namespace: profile().spec().namespace,
                row: RowId::new(9).unwrap(),
            },
            &cols,
        )
        .unwrap()
        .version()
        .logical
        .transaction();
        // Existing U9/U12 contribute228+208 restorable bytes. Eight added
        // committed rows contribute6*2232+2157+203=15752. Their actual
        // bytes fit the page even at +1 because U12 still needs10 to undo.
        for (at, cost) in [2232, 2232, 2232, 2232, 2232, 2232, 2157 + extra, 203]
            .into_iter()
            .enumerate()
        {
            let id = 100 + at as u64;
            let text = "x".repeat(cost - 202); // two-column canonical overhead18 + record/tree184
            let mut canonical = vec![0; cost - 184];
            let length = row_record::encode_canonical_row_borrowed(
                schema,
                &[
                    row_record::ValueRef::Int64(id as i64),
                    row_record::ValueRef::Utf8(&text),
                ],
                &mut canonical,
            )
            .unwrap();
            assert_eq!(length, cost - 184);
            let payload = row_record::decode_canonical_row(&canonical, &cols).unwrap();
            let version = RecordVersion {
                row: RowId::new(id).unwrap(),
                stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
                logical: LogicalRowVersion::new(source, id).unwrap(),
                undo: None,
            };
            let mut bytes = vec![0; cost - 16];
            row_record::encode_row_record_with_schema(
                RecordExpectation {
                    namespace: profile().spec().namespace,
                    row: version.row,
                },
                schema,
                version,
                RowPayload::Inline(payload),
                &mut bytes,
            )
            .unwrap();
            let mut cell = vec![0; cost - 4];
            tree_page::encode_leaf_cell(PageKind::RowLeaf, 0, &id.to_be_bytes(), &bytes, &mut cell)
                .unwrap();
            fixture.rows.insert(id, cell);
        }
        if extra == 0 {
            let (summary, _) = fixture.observe().unwrap();
            assert_eq!(summary.actual, 16178);
            assert_eq!(summary.restorable, 16188);
        } else {
            assert_eq!(fixture.observe(), Err(Error::BudgetMismatch));
        }
    }
}

#[test]
fn structural_weights_reject_missing_predecessor_and_stale_or_future_images() {
    for variant in 0..4 {
        let mut fixture = Fixture::new(&["U-558", "U-560"]);
        if variant == 0 {
            row::free_job(one(), &mut fixture.jobs[0]);
            fixture.rows.remove(&9); // select only row12, so its missing H is reached
        }
        let mut leaf = fixture.leaf();
        let mut undo = fixture.undo_pages();
        if variant != 0 {
            let page = if variant == 3 {
                leaf.as_mut()
            } else {
                undo.get_mut(&11).unwrap().as_mut()
            };
            let lsn = if variant == 2 { 561u64 } else { 558u64 };
            page[64..72].copy_from_slice(&lsn.to_le_bytes());
            let hash = content_digest(&page[..page::DIGEST_OFFSET]);
            page[page::DIGEST_OFFSET..].copy_from_slice(&hash);
        }
        let allocation_bytes = fixture.allocation();
        let allocation = allocation_page::decode_allocation_page(
            allocation_bytes.as_ref(),
            identity(0, 1),
            profile().spec().namespace,
        )
        .unwrap();
        let cols = columns();
        let input = Inputs {
            frame: frame(560),
            table: table(),
            anchor: reference(12),
            schema: RowSchemaInput::Columns(&cols),
            leaf: TreeExpectation {
                identity: identity(13, 6),
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
            image: &leaf,
            jobs: JobCells::original(&fixture.jobs),
            allocation: &allocation,
        };
        let mut visited = Vec::new();
        assert!(
            derive(&input, &mut [0; 2], |r| {
                visited.push(r.page_no);
                Ok(undo[&r.page_no].as_ref())
            })
            .is_err(),
            "mutation {variant}"
        );
        if variant == 0 {
            assert_eq!(visited, [11]);
        }
    }
}
