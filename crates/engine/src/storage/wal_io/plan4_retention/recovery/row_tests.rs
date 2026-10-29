//! S1-b: the scope3 Row arm of `apply_frame` (XADB-MODULE-ROW-BEGIN-001,
//! "replay와 분석 gate") over the independent N1 transition literals
//! (`module-row-format/transition`) and A's Forward/CLR literals
//! (`module-row-insert/scope3`). Before arenas are the literal snapshots at
//! their own page numbers plus a Catalog leaf (page 8) encoded here for A's
//! table. Expected afters are each frame's redo records applied to the before
//! pages here, A's snapshot literals and the bank rule of the consumer
//! contract; no product producer or observer output is a golden. RowReserve
//! has no replay successor (its frame lacks the canonical row value) and is
//! installed test-side from its redo after the arm refuses it.
use super::super::{control as ledger, history, population};
use super::*;
use crate::{
    api::{Column, DataType, DatabaseId, Namespace, TableSpec},
    storage::{
        catalog_record::{
            CatalogDefinition, CatalogExpectation, CatalogHeader, encode_catalog_record,
        },
        control::row_profile,
        page::{DIGEST_OFFSET, FileId, PAGE_BYTES, PageHeader, PageIdentity, PageLsn},
        retention_genesis::{self, RetentionGenesisPage},
        retention_page::{JOB_BYTES, JOB_COUNT, row as job3},
        row_record::RowSchemaInput,
        semantic_row::{
            Operation, decode_layout,
            status::StatementBoundary,
            transition::{
                self, Change, Inputs, JOB, Observation, ROW_REPLAY_SCRATCH_BYTES, TableSource,
            },
        },
        transaction_record::{LogicalRowVersion, StatusSlotRef, TxnRef, VersionStamp},
        tree_page,
        undo_ref::UndoRef,
        wal_frame::{self, RedoKind, WalStreamIdentity},
    },
    transaction::TxnId,
};
use foundation::identity::{CommitSeq, ObjectHandle, SchemaEpoch};
use std::cell::Cell;

macro_rules! scope3 {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-insert/scope3/",
            $name
        ))
    };
}
const BLOBS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition/blobs.bin"
));
const INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition/index.tsv"
));

type Page = [u8; PAGE_BYTES];
type Descriptor = [u8; 1536];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
/// One literal from blobs.bin, located and SHA-checked through index.tsv.
fn blob(name: &str) -> &'static [u8] {
    for line in INDEX.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields[0] == name {
            let offset: usize = fields[1].parse().unwrap();
            let length: usize = fields[2].parse().unwrap();
            let bytes = &BLOBS[offset..offset + length];
            assert_eq!(hex(&content_digest(bytes)), fields[3], "{name}");
            return bytes;
        }
    }
    panic!("missing literal {name}");
}
fn page_of(bytes: &[u8]) -> &Page {
    bytes.try_into().unwrap()
}
fn same(label: &str, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len(), "{label}: length");
    if let Some(at) = actual.iter().zip(expected).position(|(a, b)| a != b) {
        let end = (at + 16).min(actual.len());
        panic!(
            "{label}: first difference at byte {at}: actual {:02x?} expected {:02x?}",
            &actual[at..end],
            &expected[at..end]
        );
    }
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
/// Cell `slot` of a slotted page, by its literal directory entry.
fn cell(page: &[u8], slot: usize) -> &[u8] {
    let at = 128 + 4 * slot;
    let offset = usize::from(u16::from_le_bytes([page[at], page[at + 1]]));
    let length = usize::from(u16::from_le_bytes([page[at + 2], page[at + 3]]));
    &page[offset..offset + length]
}
fn generation(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new([1; 16]).unwrap(),
        timeline: 1,
    }
}
fn profile() -> Profile {
    row_profile::decode_profile(scope3!("profile3.bin"), None).unwrap()
}
fn page_ref(page_no: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0001).unwrap(),
        page_no,
        incarnation: generation(incarnation),
    }
}
fn actor(local: u64, status_generation: u64) -> TxnRef {
    TxnRef::new(
        TxnId {
            database: namespace().database,
            timeline: 1,
            writer_epoch: 1,
            local,
        },
        generation(status_generation),
    )
    .unwrap()
}
fn slot(page_no: u64, slot_generation: u64) -> StatusSlotRef {
    StatusSlotRef::new(page_ref(page_no, 2), 0, generation(slot_generation)).unwrap()
}
fn undo_ref(page_no: u64, slot_generation: u64) -> UndoRef {
    UndoRef::new(page_ref(page_no, 2), 0, generation(slot_generation)).unwrap()
}
fn boundary(sequence: u64, mark: Option<UndoRef>) -> StatementBoundary {
    StatementBoundary { sequence, mark }
}
fn table() -> ObjectHandle {
    ObjectHandle {
        object: ObjectId::new(6).unwrap(),
        generation: generation(1),
        schema_epoch: SchemaEpoch::new(1).unwrap(),
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

/// A fixed CatalogLeaf (page 8) holding one XCT1 record for `object` with
/// A's two columns: the replay reads the Row schema from the canonical
/// Catalog, never from the candidate.
fn catalog_leaf(object: u64, retired: bool, anchor: PageRef) -> Page {
    let spec = TableSpec {
        name: "row_table".into(),
        columns: columns().to_vec(),
    };
    let identity = ObjectHandle {
        object: ObjectId::new(object).unwrap(),
        ..table()
    };
    let header = CatalogHeader {
        identity,
        retired,
        stamp: VersionStamp::Committed(CommitSeq::new(1).unwrap()),
        logical: LogicalRowVersion::new(
            TxnId {
                database: namespace().database,
                timeline: 1,
                writer_epoch: 1,
                local: 1,
            },
            1,
        )
        .unwrap(),
        undo: None,
        anchor,
    };
    let mut record = vec![0; 4096];
    let length = encode_catalog_record(
        CatalogExpectation {
            namespace: namespace(),
            object: identity.object,
        },
        header,
        CatalogDefinition::Table(&spec),
        &mut record,
    )
    .unwrap();
    let key = object.to_be_bytes();
    let mut metadata = [0; 32];
    let metadata =
        tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut metadata).unwrap();
    let mut entry = vec![0; 4 + key.len() + length];
    let entry = tree_page::encode_leaf_cell(
        PageKind::CatalogLeaf,
        0,
        &key,
        &record[..length],
        &mut entry,
    )
    .unwrap();
    let mut page = [0; PAGE_BYTES];
    tree_page::encode_tree_page(
        PageHeader {
            identity: PageIdentity {
                database: namespace().database,
                page: page_ref(8, 1),
                owner: ObjectId::new(5).unwrap(),
                owner_generation: generation(1),
            },
            kind: PageKind::CatalogLeaf,
            level: 0,
            lsn: PageLsn(1),
        },
        0,
        &[metadata, entry],
        &mut page,
    )
    .unwrap();
    page
}

/// Which stable before observations one step borrows (N1's `Pages`): the
/// test's own logical arguments for the live `record` comparison.
#[derive(Clone, Copy, Default)]
struct Pages {
    jobs: Option<usize>,
    status: Option<u64>,
    undo: Option<u64>,
    previous: Option<u64>,
    table: bool,
}
fn pages() -> Pages {
    Pages::default()
}

#[derive(Clone, Copy, Default)]
struct Mode {
    /// H02: every `R-<step>-*` variant is refused with no effect first.
    rejects: bool,
    /// H04: `record` on a live-layout control loan must equal the replay.
    live: bool,
}

/// Everything `apply_frame` may change, for no-effect comparisons.
#[derive(PartialEq, Eq)]
struct Saved {
    arena: Vec<u8>,
    cells: Vec<u8>,
    bank: Vec<u8>,
    claims: Vec<u8>,
    tail: Vec<u8>,
    cursor: (u64, Option<u32>, u64),
}
impl std::fmt::Debug for Saved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Saved(cursor {:?})", self.cursor)
    }
}

/// The disjoint replay loans of `apply_frame` and its cursor, from a literal
/// snapshot. Test-side tracking of the last Tx/Aging descriptors mirrors N1's
/// stabilized world for the live `record` comparison's observer inputs.
struct Replay {
    arena: Vec<u8>,
    cells: Box<[[u8; JOB_BYTES]; JOB_COUNT]>,
    bank: Vec<u8>,
    after: Box<[Page; 8]>,
    claims: Box<[[u8; 352]; 64]>,
    tail: Box<[u8; history::REPLAY_TAIL_BYTES]>,
    cursor: RetentionReplayCursor,
    transaction: Option<Descriptor>,
    aging: Option<Descriptor>,
    columns: [Column; 2],
    mode: Mode,
    rejected: Cell<usize>,
}

impl Replay {
    /// `pages` at their own header page numbers; unnamed Jobs pages are the
    /// genesis encoding; the bank's single Row TerminalLeased actor (if any)
    /// gets `transaction` at ATT row `att`.
    fn new(
        lsn: u64,
        pages: &[&Page],
        bank: Option<&[u8; JOB_BYTES * JOB_COUNT]>,
        transaction: Option<(&Descriptor, usize)>,
        aging: Option<&Descriptor>,
        adopted: u64,
    ) -> Self {
        let ns = namespace();
        let mut arena = vec![0; history::ARENA_END];
        for index in 0..5u8 {
            let at = (2 + usize::from(index)) * PAGE_BYTES;
            retention_genesis::encode_row_current(
                ns,
                RetentionGenesisPage::Jobs { index },
                &mut arena[at..at + PAGE_BYTES],
            )
            .unwrap();
        }
        arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&catalog_leaf(
            6,
            false,
            page_ref(12, 2),
        ));
        arena[12 * PAGE_BYTES..13 * PAGE_BYTES].copy_from_slice(scope3!("anchor.bin"));
        for page in pages {
            let page_no = u64_at(&page[..], 32) as usize;
            assert!(page_no < 64 && page_no != 8);
            arena[page_no * PAGE_BYTES..(page_no + 1) * PAGE_BYTES].copy_from_slice(&page[..]);
        }
        let mut cells = Box::new([[0; JOB_BYTES]; JOB_COUNT]);
        for (ordinal, job) in cells.iter_mut().enumerate() {
            match bank {
                Some(bank) => {
                    job.copy_from_slice(&bank[ordinal * JOB_BYTES..(ordinal + 1) * JOB_BYTES])
                }
                None => {
                    let page = 2 + ordinal / 15;
                    job.copy_from_slice(cell(
                        &arena[page * PAGE_BYTES..(page + 1) * PAGE_BYTES],
                        ordinal % 15,
                    ))
                }
            }
        }
        let mut replay = Self {
            arena,
            cells,
            bank: vec![0; history::BANK_END - history::JOB_END],
            after: Box::new([[0; PAGE_BYTES]; 8]),
            claims: Box::new([[0; 352]; 64]),
            tail: vec![0; history::REPLAY_TAIL_BYTES]
                .into_boxed_slice()
                .try_into()
                .unwrap(),
            cursor: RetentionReplayCursor {
                allocation: population::ReplayAllocation::new(),
                adopted_local: adopted,
                previous_lsn: lsn,
                epoch: Some(1),
                predecessor_epoch: 0,
                base_seeded: false,
            },
            transaction: transaction.map(|(raw, _)| *raw),
            aging: aging.copied(),
            columns: columns(),
            mode: Mode::default(),
            rejected: Cell::new(0),
        };
        if let Some((raw, att)) = transaction {
            let (actor, location) = replay.leased();
            replay.put_att(att, raw, actor, location);
        }
        if let Some(raw) = aging {
            let row = ledger::row_mut(&mut replay.bank, ledger::AGING_SLOT);
            row[..1536].copy_from_slice(raw);
            row[ledger::PHASE] = ledger::STABLE;
        }
        replay
    }
    fn with(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }
    /// The single Row TerminalLeased actor of the bank and its Status.
    fn leased(&self) -> (TxnRef, StatusSlotRef) {
        let mut found = None;
        for job in self.cells.iter() {
            let view = job3::decode_job(job, namespace()).unwrap();
            if view.state() == job3::JobState::TerminalLeased {
                assert_eq!(view.scope(), Some(job3::JobScope::Row));
                assert!(found.is_none());
                found = Some((view.source().unwrap(), view.status().unwrap()));
            }
        }
        found.expect("leased Row actor")
    }
    fn put_att(&mut self, att: usize, raw: &Descriptor, actor: TxnRef, location: StatusSlotRef) {
        let row = ledger::row_mut(&mut self.bank, att);
        row.fill(0);
        row[..1536].copy_from_slice(raw);
        actor.encode(&mut row[ledger::ACTOR]).unwrap();
        location.encode(&mut row[ledger::LOCATION]).unwrap();
        row[ledger::PHASE] = ledger::STABLE;
    }
    fn page(&self, page_no: u64) -> &Page {
        let at = page_no as usize * PAGE_BYTES;
        self.arena[at..at + PAGE_BYTES].try_into().unwrap()
    }
    fn set_page(&mut self, page: &Page) {
        let at = u64_at(page, 32) as usize * PAGE_BYTES;
        self.arena[at..at + PAGE_BYTES].copy_from_slice(page);
    }
    fn expectation(&self) -> FrameExpectation {
        FrameExpectation {
            stream: WalStreamIdentity {
                database: namespace().database,
                timeline: 1,
                writer_epoch: 1,
            },
            previous_lsn: self.cursor.previous_lsn,
        }
    }
    fn saved(&self) -> Saved {
        Saved {
            arena: self.arena.clone(),
            cells: self.cells.as_flattened().to_vec(),
            bank: self.bank.clone(),
            claims: self.claims.as_flattened().to_vec(),
            tail: self.tail.to_vec(),
            cursor: (
                self.cursor.previous_lsn,
                self.cursor.epoch,
                self.cursor.adopted_local,
            ),
        }
    }
    fn apply_with(
        &mut self,
        expected: FrameExpectation,
        frame: &[u8],
    ) -> ObservationResult<RetentionFrameObservation> {
        self.after.fill([0xa5; PAGE_BYTES]);
        apply_frame(
            profile(),
            expected,
            frame,
            &mut self.arena,
            &mut self.cells,
            &mut self.bank,
            &mut self.after,
            &mut self.claims,
            &mut self.tail,
            &mut self.cursor,
        )
    }
    fn apply(&mut self, frame: &[u8]) -> ObservationResult<RetentionFrameObservation> {
        self.apply_with(self.expectation(), frame)
    }
    /// H02: the frame is refused and nothing the replay owns changes.
    fn refused(&mut self, label: &str, frame: &[u8]) -> String {
        let before = self.saved();
        let result = self.apply(frame);
        let cause = format!("{:?}", result.as_ref().err());
        assert!(result.is_err(), "{label} accepted");
        assert_eq!(self.saved(), before, "{label}: effect on refusal");
        println!("S1B_REFUSED\t{label}\t{cause}");
        cause
    }
    /// Each redo record of `frame` applied to the current before page: the
    /// only expected page afters (independent of the observer).
    fn redo_afters(&self, frame: &[u8]) -> Vec<(u64, Page)> {
        let layout = decode_layout(frame, self.expectation(), profile()).unwrap();
        let header = layout.header();
        let mut redo = layout.redo();
        let mut out: Vec<(u64, Page)> = Vec::new();
        while !redo.is_empty() {
            let length = u32::from_le_bytes(redo[4..8].try_into().unwrap()) as usize;
            let record = wal_frame::decode_redo_record(header, &redo[..length]).unwrap();
            redo = &redo[length..];
            assert!(matches!(record.kind, RedoKind::Patch));
            let page_no = record.target.page.page_no;
            assert!(out.iter().all(|(n, _)| *n != page_no));
            let mut image = *self.page(page_no);
            assert_eq!(record.prior_lsn, u64_at(&image, 64));
            image[64..72].copy_from_slice(&header.lsn.to_le_bytes());
            let offset = usize::from(record.offset);
            image[offset..offset + record.data.len()].copy_from_slice(record.data);
            let digest = content_digest(&image[..DIGEST_OFFSET]);
            image[DIGEST_OFFSET..].copy_from_slice(&digest);
            out.push((page_no, image));
        }
        out
    }
    /// The consumer contract's control rule for one group (N1 install): the
    /// Tx row (XHG/Tx-owned XSG; cleared by Commit/AbortEnd), the Aging row
    /// (every op with a Root), the changed Jobs page's cells.
    fn expected_control(
        &self,
        frame: &[u8],
        begin: Option<StatusSlotRef>,
        afters: &[(u64, Page)],
    ) -> (Vec<u8>, Vec<u8>) {
        let layout = decode_layout(frame, self.expectation(), profile()).unwrap();
        let op = layout.operation();
        let mut bank = self.bank.clone();
        if op.handoff() || op.status_patch() {
            let Some(crate::storage::allocation_page::ReservationOwner::Transaction(actor)) =
                layout.owner(0)
            else {
                panic!("transaction owner");
            };
            let att = if op == Operation::Begin {
                (0..ledger::ATT_SLOTS)
                    .find(|i| ledger::row(&bank, *i).iter().all(|b| *b == 0))
                    .unwrap()
            } else {
                (0..ledger::ATT_SLOTS)
                    .find(|i| {
                        let row = ledger::row(&bank, *i);
                        row[ledger::PHASE] == ledger::STABLE
                            && TxnRef::decode(&row[ledger::ACTOR]).unwrap() == actor
                    })
                    .unwrap()
            };
            let location = match begin {
                Some(location) => location,
                None => StatusSlotRef::decode(&ledger::row(&bank, att)[ledger::LOCATION]).unwrap(),
            };
            let row = ledger::row_mut(&mut bank, att);
            row.fill(0);
            if !matches!(op, Operation::Commit | Operation::AbortEnd) {
                row[..1536].copy_from_slice(layout.descriptor(0).unwrap());
                actor.encode(&mut row[ledger::ACTOR]).unwrap();
                location.encode(&mut row[ledger::LOCATION]).unwrap();
                row[ledger::PHASE] = ledger::STABLE;
            }
        }
        if !op.status_patch() {
            let row = ledger::row_mut(&mut bank, ledger::AGING_SLOT);
            row[..1536].copy_from_slice(layout.descriptor(usize::from(op.handoff())).unwrap());
            row[ledger::PHASE] = ledger::STABLE;
        }
        let mut cells = self.cells.as_flattened().to_vec();
        for (page_no, image) in afters {
            if (2..=6).contains(page_no) {
                let index = *page_no as usize - 2;
                for slot in 0..(JOB_COUNT - index * 15).min(15) {
                    let ordinal = index * 15 + slot;
                    cells[ordinal * JOB_BYTES..(ordinal + 1) * JOB_BYTES]
                        .copy_from_slice(cell(image, slot));
                }
            }
        }
        (bank, cells)
    }
    /// Observer inputs from the step's own logical arguments (N1's), over the
    /// current replay pages.
    fn inputs<'a>(&'a self, change: Change<'a>, pages: Pages) -> Inputs<'a> {
        Inputs {
            profile: profile(),
            frame: self.expectation(),
            change,
            jobs: &self.cells,
            root: self.page(1),
            jobs_page: pages.jobs.map(|index| self.page(2 + index as u64)),
            status: pages.status.map(|n| self.page(n)),
            undo: pages.undo.map(|n| self.page(n)),
            previous: pages.previous.map(|n| self.page(n)),
            table: pages.table.then(|| TableSource {
                handle: table(),
                anchor: page_ref(12, 2),
                anchor_image: self.page(12),
                leaf_image: Some(self.page(13)),
                schema: RowSchemaInput::Columns(&self.columns),
            }),
            transaction: if matches!(change, Change::Begin { .. }) {
                None
            } else {
                self.transaction.as_ref()
            },
            aging: self.aging.as_ref(),
        }
    }
    /// H04: the same `record` body on a live-layout control loan (rows at
    /// 0..65536, Jobs at 65536..131072) given the step's own observation.
    fn live_record(&self, frame: &[u8], change: Change<'_>, pages: Pages) -> Vec<u8> {
        let inputs = self.inputs(change, pages);
        let mut afters = Box::new([[0; PAGE_BYTES]; 5]);
        let observation = transition::observe(&inputs, frame, &mut afters)
            .unwrap_or_else(|e| panic!("live observe {e:?}"));
        let layout = decode_layout(frame, inputs.frame, profile()).unwrap();
        let op = layout.operation();
        let mut control = vec![0; 2 * 65_536];
        control[..65_536].copy_from_slice(&self.bank);
        control[65_536..].copy_from_slice(self.cells.as_flattened());
        let slot = if op == Operation::Begin {
            (0..ledger::ATT_SLOTS).find(|i| ledger::row(&self.bank, *i).iter().all(|b| *b == 0))
        } else if op.handoff() || op.status_patch() {
            let actor = observation.actor.unwrap();
            (0..ledger::ATT_SLOTS).find(|i| {
                let row = ledger::row(&self.bank, *i);
                row[ledger::PHASE] == ledger::STABLE
                    && TxnRef::decode(&row[ledger::ACTOR]).unwrap() == actor
            })
        } else {
            None
        };
        let (rows, jobs) = control.split_at_mut(65_536);
        let jobs: &mut [[u8; JOB_BYTES]; JOB_COUNT] =
            jobs.as_chunks_mut::<JOB_BYTES>().0.try_into().unwrap();
        history::row::record(
            rows,
            jobs,
            layout,
            &observation,
            observation.job_ordinal.map(|_| &afters[JOB]),
            slot,
            profile(),
        )
        .unwrap();
        control
    }
    /// One accepted group: R- refusals (H02), then `apply_frame`, compared
    /// with the redo afters, the control rule and (H04) the live `record`.
    fn step(&mut self, name: &str, frame: &[u8], change: Change<'_>, pages: Pages) {
        if self.mode.rejects {
            self.variants(name);
        }
        let afters = self.redo_afters(frame);
        let begin = match change {
            Change::Begin { status, .. } => Some(status),
            _ => None,
        };
        let (bank, cells) = self.expected_control(frame, begin, &afters);
        let live = self
            .mode
            .live
            .then(|| self.live_record(frame, change, pages));
        let layout_header = decode_layout(frame, self.expectation(), profile())
            .unwrap()
            .header();
        let before = self.saved();
        let adopted = self.cursor.adopted_local;
        let observed = self
            .apply(frame)
            .unwrap_or_else(|e| panic!("{name}: apply {e:?}"));
        assert_eq!(observed.header, layout_header, "{name}");
        let mut changed: Vec<u64> = observed
            .changed
            .iter()
            .flatten()
            .map(|r| r.identity.page.page_no)
            .collect();
        changed.sort_unstable();
        let mut expected: Vec<u64> = afters.iter().map(|(n, _)| *n).collect();
        expected.sort_unstable();
        assert_eq!(changed, expected, "{name}: changed pages");
        assert!(
            observed
                .initialized
                .iter()
                .chain(&observed.retired)
                .all(Option::is_none)
        );
        for page_no in 0..64u64 {
            let at = page_no as usize * PAGE_BYTES;
            let actual = &self.arena[at..at + PAGE_BYTES];
            match afters.iter().find(|(n, _)| *n == page_no) {
                Some((_, image)) => same(&format!("{name} page{page_no}"), actual, image),
                None => assert!(
                    actual == &before.arena[at..at + PAGE_BYTES],
                    "{name} page{page_no}"
                ),
            }
        }
        same(&format!("{name} cells"), self.cells.as_flattened(), &cells);
        same(&format!("{name} bank"), &self.bank, &bank);
        assert!(self.tail.iter().all(|b| *b == 0), "{name}: tail");
        assert!(self.claims.as_flattened() == before.claims.as_slice());
        assert_eq!(self.cursor.previous_lsn, layout_header.lsn);
        assert_eq!(self.cursor.epoch, Some(1));
        let expected_adopted = match change {
            Change::Begin { actor, .. } => actor.id().local,
            _ => adopted,
        };
        assert_eq!(
            self.cursor.adopted_local, expected_adopted,
            "{name}: adopted"
        );
        if let Some(control) = live {
            same(&format!("{name} live rows"), &control[..65_536], &self.bank);
            same(
                &format!("{name} live Jobs"),
                &control[65_536..],
                self.cells.as_flattened(),
            );
        }
        self.track(frame, &before);
        println!(
            "S1B_REPLAYED\t{name}\t{:?}\tpages={changed:?}",
            change.operation()
        );
    }
    /// Test-side Tx/Aging descriptor tracking (N1 install rule).
    fn track(&mut self, frame: &[u8], before: &Saved) {
        let expected = FrameExpectation {
            previous_lsn: before.cursor.0,
            ..self.expectation()
        };
        let layout = decode_layout(frame, expected, profile()).unwrap();
        let op = layout.operation();
        if op.handoff() {
            self.transaction = Some(*layout.descriptor(0).unwrap());
            self.aging = Some(*layout.descriptor(1).unwrap());
        } else if op.status_patch() {
            self.transaction = Some(*layout.descriptor(0).unwrap());
        } else {
            self.aging = Some(*layout.descriptor(0).unwrap());
        }
    }
    /// RowReserve: refused before any effect, then installed test-side from
    /// its own redo and the control rule so the history can continue.
    fn reserve(&mut self, name: &str, frame: &[u8]) {
        if self.mode.rejects {
            self.variants(name);
        }
        let cause = self.refused(name, frame);
        assert_eq!(cause, "Some(Unsupported)", "{name}");
        let afters = self.redo_afters(frame);
        let (bank, cells) = self.expected_control(frame, None, &afters);
        let before = self.saved();
        for (_, image) in &afters {
            self.set_page(image);
        }
        self.bank = bank;
        self.cells.as_flattened_mut().copy_from_slice(&cells);
        self.cursor.previous_lsn += 1;
        self.track(frame, &before);
        println!("S1B_RESERVE_INSTALLED\t{name}");
    }
    /// Every `R-<name>-*` variant of the transition corpus is refused here.
    fn variants(&mut self, name: &str) {
        let prefix = format!("R-{name}-");
        for line in INDEX.lines() {
            let variant = line.split('\t').next().unwrap();
            if variant.starts_with(&prefix) {
                self.refused(variant, blob(variant));
                self.rejected.set(self.rejected.get() + 1);
            }
        }
    }
    fn check_page(&self, label: &str, page: &Page) {
        same(label, self.page(u64_at(page, 32)), page);
    }
    fn check_tx(&self, label: &str, raw: &[u8]) {
        let (actor, _) = self.leased();
        let att = (0..ledger::ATT_SLOTS)
            .find(|i| {
                let row = ledger::row(&self.bank, *i);
                row[ledger::PHASE] == ledger::STABLE
                    && TxnRef::decode(&row[ledger::ACTOR]).unwrap() == actor
            })
            .unwrap();
        same(label, &ledger::row(&self.bank, att)[..1536], raw);
    }
    fn check_aging(&self, label: &str, raw: &[u8]) {
        same(
            label,
            &ledger::row(&self.bank, ledger::AGING_SLOT)[..1536],
            raw,
        );
    }
    fn check_bank(&self, label: &str, bank: &[u8]) {
        same(label, self.cells.as_flattened(), bank);
    }
}

// ---- N1 histories through `apply_frame` --------------------------------------

fn genesis(mode: Mode) -> Replay {
    let mut root = [0; PAGE_BYTES];
    retention_genesis::encode_row_current(namespace(), RetentionGenesisPage::Root, &mut root)
        .unwrap();
    same("E genesis Root", &root, blob("E-root-0"));
    Replay::new(
        99,
        &[page_of(blob("E-root-0")), page_of(blob("E-status20-0"))],
        None,
        None,
        None,
        1,
    )
    .with(mode)
}

fn history_e(mode: Mode) -> usize {
    let mut r = genesis(mode);
    let status20 = Pages {
        jobs: Some(0),
        status: Some(20),
        ..pages()
    };
    r.step("E-100", blob("E-100"), Change::AgingStart, pages());
    let begin = |local, generation| Change::Begin {
        actor: actor(local, generation),
        status: slot(20, generation),
        scope: job3::JobScope::Row,
    };
    r.step("E-101", blob("E-101"), begin(7, 2), status20);
    r.step(
        "E-102",
        blob("E-102"),
        Change::Commit { terminal: 0 },
        status20,
    );
    r.step(
        "E-103",
        blob("E-103"),
        Change::TerminalFinish { ordinal: 0 },
        status20,
    );
    r.step("E-104", blob("E-104"), begin(8, 3), status20);
    r.step(
        "E-105",
        blob("E-105"),
        Change::RollbackStart {
            terminal: 1,
            statement: None,
        },
        Pages {
            status: Some(20),
            ..pages()
        },
    );
    r.step(
        "E-106",
        blob("E-106"),
        Change::AbortEnd { terminal: 1 },
        status20,
    );
    r.step(
        "E-107",
        blob("E-107"),
        Change::TerminalFinish { ordinal: 1 },
        status20,
    );
    assert_eq!(u64_at(cell(r.page(1), 0), 72), 2, "queue cursor");
    r.rejected.get()
}

fn noh_301(mode: Mode) -> Replay {
    Replay::new(
        301,
        &[
            scope3!("noh-301-root.bin"),
            scope3!("noh-301-jobs2.bin"),
            scope3!("noh-301-status.bin"),
            scope3!("noh-301-undo11.bin"),
            scope3!("noh-301-leaf.bin"),
        ],
        Some(scope3!("noh-301-bank.bin")),
        Some((scope3!("noh-301-tx.bin"), 3)),
        Some(scope3!("noh-301-aging.bin")),
        1,
    )
    .with(mode)
}

fn check_noh303(r: &Replay) {
    r.check_page("noh-303 Root", scope3!("noh-303-root.bin"));
    r.check_page("noh-303 Jobs2", scope3!("noh-303-jobs2.bin"));
    r.check_page("noh-303 Status", scope3!("noh-303-status.bin"));
    r.check_page("noh-303 Undo11", scope3!("noh-303-undo11.bin"));
    r.check_page("noh-303 Leaf", scope3!("noh-303-leaf.bin"));
    r.check_tx("noh-303 Tx", scope3!("noh-303-tx.bin"));
    r.check_aging("noh-303 Aging", scope3!("noh-303-aging.bin"));
    r.check_bank("noh-303 bank", scope3!("noh-303-bank.bin"));
}

fn history_n(mode: Mode) -> usize {
    let mut r = noh_301(mode);
    let status9 = Pages {
        jobs: Some(0),
        status: Some(9),
        ..pages()
    };
    r.step(
        "N-302",
        blob("N-302"),
        Change::RollbackStart {
            terminal: 2,
            statement: None,
        },
        Pages {
            status: Some(9),
            undo: Some(11),
            ..pages()
        },
    );
    r.check_page("N-302 Status", scope3!("noh-302-status.bin"));
    r.check_tx("N-302 Tx", scope3!("noh-302-tx.bin"));
    r.step(
        "N-303",
        scope3!("noh-303-clr.wal"),
        Change::Clr {
            ordinal: 3,
            statement: None,
        },
        Pages {
            jobs: Some(0),
            status: Some(9),
            undo: Some(11),
            table: true,
            ..pages()
        },
    );
    check_noh303(&r);
    r.step(
        "N-304",
        blob("N-304"),
        Change::AbortEnd { terminal: 2 },
        status9,
    );
    let undo11 = Pages {
        jobs: Some(0),
        undo: Some(11),
        ..pages()
    };
    for name in ["N-305", "N-306", "N-307"] {
        r.step(name, blob(name), Change::ReleaseRole { ordinal: 3 }, undo11);
    }
    r.step(
        "N-308",
        blob("N-308"),
        Change::RootFree { ordinal: 3 },
        undo11,
    );
    r.step(
        "N-309",
        blob("N-309"),
        Change::TerminalFinish { ordinal: 2 },
        status9,
    );
    r.rejected.get()
}

fn h_501(mode: Mode) -> Replay {
    Replay::new(
        501,
        &[
            scope3!("h-501-root.bin"),
            scope3!("h-501-jobs2.bin"),
            scope3!("h-501-jobs3.bin"),
            scope3!("h-501-status.bin"),
            scope3!("h-500-undo10.bin"),
            scope3!("h-501-undo11.bin"),
            scope3!("h-501-leaf.bin"),
        ],
        Some(scope3!("h-501-bank.bin")),
        Some((scope3!("h-501-tx.bin"), 0)),
        Some(scope3!("h-501-aging.bin")),
        1,
    )
    .with(mode)
}

fn history_h(mode: Mode) -> usize {
    let mut r = h_501(mode);
    let mark = boundary(2, Some(undo_ref(10, 2)));
    r.step(
        "H-502",
        blob("H-502"),
        Change::RollbackStart {
            terminal: 14,
            statement: Some(mark),
        },
        Pages {
            status: Some(9),
            undo: Some(11),
            ..pages()
        },
    );
    r.check_page("H-502 Status", scope3!("h-502-status.bin"));
    r.check_tx("H-502 Tx", scope3!("h-502-tx.bin"));
    r.step(
        "H-503",
        scope3!("h-503-clr.wal"),
        Change::Clr {
            ordinal: 16,
            statement: Some(mark),
        },
        Pages {
            jobs: Some(1),
            status: Some(9),
            undo: Some(11),
            previous: Some(10),
            table: true,
        },
    );
    r.check_page("h-503 Root", scope3!("h-503-root.bin"));
    r.check_page("h-503 Jobs3", scope3!("h-503-jobs3.bin"));
    r.check_page("h-503 Status", scope3!("h-503-status.bin"));
    r.check_page("h-503 Undo11", scope3!("h-503-undo11.bin"));
    r.check_page("h-503 Leaf", scope3!("h-503-leaf.bin"));
    r.check_tx("h-503 Tx", scope3!("h-503-tx.bin"));
    r.check_aging("h-503 Aging", scope3!("h-503-aging.bin"));
    r.check_bank("h-503 bank", scope3!("h-503-bank.bin"));
    r.step(
        "H-504",
        blob("H-504"),
        Change::StatementFinish {
            terminal: 14,
            statement: mark,
        },
        Pages {
            status: Some(9),
            ..pages()
        },
    );
    // N1's pre-initialized Undo page 14 (System INIT is not a Row group).
    r.set_page(page_of(blob("H-undo14-0")));
    r.reserve("H-505", blob("H-505"));
    r.step(
        "H-506",
        blob("H-506"),
        Change::Cancel { ordinal: 17 },
        Pages {
            jobs: Some(1),
            status: Some(9),
            undo: Some(14),
            ..pages()
        },
    );
    r.reserve("H-507", blob("H-507"));
    let row = Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(14),
        previous: Some(10),
        table: true,
    };
    r.step(
        "H-508",
        blob("H-508"),
        Change::Forward {
            ordinal: 17,
            statement: boundary(4, None),
        },
        row,
    );
    r.step(
        "H-509",
        blob("H-509"),
        Change::Commit { terminal: 14 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    r.step("H-510", blob("H-510"), Change::Prune { ordinal: 17 }, row);
    r.step(
        "H-511",
        blob("H-511"),
        Change::Prune { ordinal: 15 },
        Pages {
            undo: Some(10),
            previous: None,
            ..row
        },
    );
    let undo11 = Pages {
        jobs: Some(1),
        undo: Some(11),
        ..pages()
    };
    for name in ["H-512", "H-513", "H-514"] {
        r.step(
            name,
            blob(name),
            Change::ReleaseRole { ordinal: 16 },
            undo11,
        );
    }
    r.step(
        "H-515",
        blob("H-515"),
        Change::RootFree { ordinal: 16 },
        undo11,
    );
    r.rejected.get()
}

/// A's Forward/CLR candidates (not produced by C) from their own snapshots.
fn a_literals(mode: Mode) -> usize {
    let mut checked = 0;
    let mut r = noh_300(mode);
    let (change, inputs) = forward_301();
    r.step("A-noh-301", scope3!("noh-301-forward.wal"), change, inputs);
    r.check_page("noh-301 Root", scope3!("noh-301-root.bin"));
    r.check_page("noh-301 Jobs2", scope3!("noh-301-jobs2.bin"));
    r.check_page("noh-301 Status", scope3!("noh-301-status.bin"));
    r.check_page("noh-301 Undo11", scope3!("noh-301-undo11.bin"));
    r.check_page("noh-301 Leaf", scope3!("noh-301-leaf.bin"));
    r.check_tx("noh-301 Tx", scope3!("noh-301-tx.bin"));
    r.check_aging("noh-301 Aging", scope3!("noh-301-aging.bin"));
    r.check_bank("noh-301 bank", scope3!("noh-301-bank.bin"));
    checked += r.rejected.get();

    let mut r = noh_301(mode);
    r.cursor.previous_lsn = 302;
    r.set_page(scope3!("noh-302-status.bin"));
    let (actor, location) = r.leased();
    r.put_att(3, scope3!("noh-302-tx.bin"), actor, location);
    r.transaction = Some(*scope3!("noh-302-tx.bin"));
    r.step(
        "A-noh-303",
        scope3!("noh-303-clr.wal"),
        Change::Clr {
            ordinal: 3,
            statement: None,
        },
        Pages {
            jobs: Some(0),
            status: Some(9),
            undo: Some(11),
            table: true,
            ..pages()
        },
    );
    check_noh303(&r);
    checked += r.rejected.get();

    let mut r = Replay::new(
        500,
        &[
            scope3!("h-500-root.bin"),
            scope3!("h-500-jobs2.bin"),
            scope3!("h-500-jobs3.bin"),
            scope3!("h-500-status.bin"),
            scope3!("h-500-undo10.bin"),
            scope3!("h-500-undo11.bin"),
            scope3!("h-500-leaf.bin"),
        ],
        Some(scope3!("h-500-bank.bin")),
        Some((scope3!("h-500-tx.bin"), 0)),
        Some(scope3!("h-500-aging.bin")),
        1,
    )
    .with(mode);
    let h_row = Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(11),
        previous: Some(10),
        table: true,
    };
    r.step(
        "A-h-501",
        scope3!("h-501-forward.wal"),
        Change::Forward {
            ordinal: 16,
            statement: boundary(2, None),
        },
        h_row,
    );
    r.check_page("h-501 Root", scope3!("h-501-root.bin"));
    r.check_page("h-501 Jobs3", scope3!("h-501-jobs3.bin"));
    r.check_page("h-501 Status", scope3!("h-501-status.bin"));
    r.check_page("h-501 Undo11", scope3!("h-501-undo11.bin"));
    r.check_page("h-501 Leaf", scope3!("h-501-leaf.bin"));
    r.check_tx("h-501 Tx", scope3!("h-501-tx.bin"));
    r.check_aging("h-501 Aging", scope3!("h-501-aging.bin"));
    r.check_bank("h-501 bank", scope3!("h-501-bank.bin"));
    checked += r.rejected.get();

    let mut r = h_501(mode);
    r.cursor.previous_lsn = 502;
    r.set_page(scope3!("h-502-status.bin"));
    let (actor, location) = r.leased();
    r.put_att(0, scope3!("h-502-tx.bin"), actor, location);
    r.transaction = Some(*scope3!("h-502-tx.bin"));
    r.step(
        "A-h-503",
        scope3!("h-503-clr.wal"),
        Change::Clr {
            ordinal: 16,
            statement: Some(boundary(2, Some(undo_ref(10, 2)))),
        },
        h_row,
    );
    r.check_page("h-503 Root", scope3!("h-503-root.bin"));
    r.check_page("h-503 Undo11", scope3!("h-503-undo11.bin"));
    r.check_page("h-503 Leaf", scope3!("h-503-leaf.bin"));
    r.check_tx("h-503 Tx", scope3!("h-503-tx.bin"));
    r.check_aging("h-503 Aging", scope3!("h-503-aging.bin"));
    checked += r.rejected.get();

    let mut r = Replay::new(
        300,
        &[
            scope3!("ord-300-root.bin"),
            scope3!("ord-300-jobs2.bin"),
            scope3!("ord-300-status.bin"),
            scope3!("ord-300-undo14.bin"),
            scope3!("ord-300-leaf.bin"),
        ],
        Some(scope3!("ord-300-bank.bin")),
        Some((scope3!("ord-300-tx.bin"), 5)),
        Some(scope3!("ord-300-aging.bin")),
        1,
    )
    .with(mode);
    r.step(
        "A-ord-301",
        scope3!("ord-301-forward.wal"),
        Change::Forward {
            ordinal: 3,
            statement: boundary(1, None),
        },
        Pages {
            jobs: Some(0),
            status: Some(9),
            undo: Some(14),
            table: true,
            ..pages()
        },
    );
    r.check_page("ord-301 Root", scope3!("ord-301-root.bin"));
    r.check_page("ord-301 Status", scope3!("ord-301-status.bin"));
    r.check_page("ord-301 Undo14", scope3!("ord-301-undo14.bin"));
    r.check_page("ord-301 Leaf", scope3!("ord-301-leaf.bin"));
    r.check_tx("ord-301 Tx", scope3!("ord-301-tx.bin"));
    r.check_aging("ord-301 Aging", scope3!("ord-301-aging.bin"));
    r.check_bank("ord-301 bank", scope3!("ord-301-bank.bin"));
    checked + r.rejected.get()
}
fn noh_300(mode: Mode) -> Replay {
    Replay::new(
        300,
        &[
            scope3!("noh-300-root.bin"),
            scope3!("noh-300-jobs2.bin"),
            scope3!("noh-300-status.bin"),
            scope3!("noh-300-undo11.bin"),
            scope3!("noh-300-leaf.bin"),
        ],
        Some(scope3!("noh-300-bank.bin")),
        Some((scope3!("noh-300-tx.bin"), 3)),
        Some(scope3!("noh-300-aging.bin")),
        1,
    )
    .with(mode)
}
fn forward_301() -> (Change<'static>, Pages) {
    (
        Change::Forward {
            ordinal: 3,
            statement: boundary(1, None),
        },
        Pages {
            jobs: Some(0),
            status: Some(9),
            undo: Some(11),
            table: true,
            ..pages()
        },
    )
}

/// H01: every N1 group (E, N, H) and A's five Forward/CLR candidates replay
/// through `apply_frame` to exactly their redo afters, the bank rule and A's
/// snapshot literals; the tail stays zero, the cursor advances by one LSN
/// and records only adopted Begins. RowReserve (H-505/H-507) is refused with
/// no effect.
#[test]
fn s1b_row_arm_replays_n1_and_a_literals() {
    let mode = Mode::default();
    assert_eq!(
        history_e(mode) + history_n(mode) + history_h(mode) + a_literals(mode),
        0
    );
}

/// H02: every R- variant of the corpus and every replay precondition is
/// refused before any effect: arena, Jobs, bank, provenance, tail and cursor
/// are byte-identical afterwards (a dirty tail is refused, never cleared).
#[test]
fn s1b_row_arm_refuses_mutations_without_effect() {
    let mode = Mode {
        rejects: true,
        ..Mode::default()
    };
    let refused = history_e(mode) + history_n(mode) + history_h(mode) + a_literals(mode);
    assert_eq!(refused, 19, "every R- variant reached");

    let frame = scope3!("noh-301-forward.wal");
    let mut r = noh_300(Mode::default());
    // Dirty tail bytes inside and beyond the Row prefix.
    for at in [
        0,
        ROW_REPLAY_SCRATCH_BYTES - 1,
        ROW_REPLAY_SCRATCH_BYTES,
        history::REPLAY_TAIL_BYTES - 1,
    ] {
        r.tail[at] = 1;
        assert_eq!(r.refused(&format!("tail{at}"), frame), "Some(Invariant)");
        r.tail[at] = 0;
    }
    // Cursor/stream preconditions.
    let expected = r.expectation();
    for (label, bad) in [
        (
            "previous-lsn",
            FrameExpectation {
                previous_lsn: 299,
                ..expected
            },
        ),
        (
            "writer-epoch",
            FrameExpectation {
                stream: WalStreamIdentity {
                    writer_epoch: 2,
                    ..expected.stream
                },
                ..expected
            },
        ),
    ] {
        let before = r.saved();
        let result = r.apply_with(bad, frame);
        assert!(matches!(result, Err(RetentionCause::Invariant)), "{label}");
        assert_eq!(r.saved(), before, "{label}");
    }
    // Canonical-state selections the arm derives logical arguments from.
    let cases: [(&str, &str, Box<dyn Fn(&mut Replay)>); 6] = [
        (
            "no-att-row",
            "Some(Invariant)",
            Box::new(|r: &mut Replay| ledger::row_mut(&mut r.bank, 3).fill(0)),
        ),
        (
            "duplicate-reserved-root",
            "Some(Invariant)",
            Box::new(|r: &mut Replay| {
                let root = r.cells[3];
                r.cells[40] = root;
            }),
        ),
        (
            "catalog-retired",
            "Some(Unsupported)",
            Box::new(|r: &mut Replay| {
                let page = catalog_leaf(6, true, page_ref(12, 2));
                r.arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&page);
            }),
        ),
        (
            "catalog-missing",
            "Some(Invariant)",
            Box::new(|r: &mut Replay| {
                let page = catalog_leaf(7, false, page_ref(12, 2));
                r.arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&page);
            }),
        ),
        (
            "catalog-anchor",
            "Some(Invariant)",
            Box::new(|r: &mut Replay| {
                let page = catalog_leaf(6, false, page_ref(12, 3));
                r.arena[8 * PAGE_BYTES..9 * PAGE_BYTES].copy_from_slice(&page);
            }),
        ),
        (
            "catalog-absent",
            "",
            Box::new(|r: &mut Replay| r.arena[8 * PAGE_BYTES..9 * PAGE_BYTES].fill(0)),
        ),
    ];
    for (label, cause, mutate) in cases {
        let mut r = noh_300(Mode::default());
        mutate(&mut r);
        let refused = r.refused(label, frame);
        if !cause.is_empty() {
            assert_eq!(refused, cause, "{label}");
        }
    }
    // The unmutated snapshot is accepted (the refusals above are causal).
    noh_300(Mode::default()).step("A-noh-301", frame, forward_301().0, forward_301().1);
}

/// H03: the frame's scope selects exactly one arm. A Row group relabelled
/// scope2 reaches only the Catalog observer, scope1 only the System arm
/// (which refuses any Reserved Job3 root first), and any other scope or
/// layout version is Unsupported; none falls back to another arm. The
/// Catalog-group-as-scope3 direction is MBC40-V01's scope loop
/// (`format_tests`), refused by the Row decoder.
#[test]
fn s1b_scope_routes_only_to_its_own_arm() {
    let frame = scope3!("noh-301-forward.wal");
    assert_eq!(u16::from_le_bytes([frame[170], frame[171]]), 3);
    for scope in [2u16, 1, 0, 4, u16::MAX] {
        let mut relabelled = frame.to_vec();
        relabelled[170..172].copy_from_slice(&scope.to_le_bytes());
        let mut r = noh_300(Mode::default());
        let cause = r.refused(&format!("scope{scope}"), &relabelled);
        match scope {
            2 => assert!(cause.starts_with("Some(CatalogSemantic("), "{cause}"),
            1 => assert_eq!(cause, "Some(Invariant)"),
            _ => assert_eq!(cause, "Some(Unsupported)"),
        }
    }
    for version in [1u16, 3] {
        let mut relabelled = frame.to_vec();
        relabelled[164..166].copy_from_slice(&version.to_le_bytes());
        let mut r = noh_300(Mode::default());
        assert_eq!(
            r.refused(&format!("version{version}"), &relabelled),
            "Some(Unsupported)"
        );
    }
    // A Row group whose own decoding fails stays a Row observer refusal.
    let mut damaged = frame.to_vec();
    damaged[200] ^= 1;
    let mut r = noh_300(Mode::default());
    assert!(
        r.refused("damaged", &damaged)
            .starts_with("Some(RowSemantic(")
    );
}

/// H04: `record` is one body for live recording and replay: on a live-layout
/// control loan (rows, then Jobs) with the step's own observation it writes
/// exactly the bytes replay wrote into the replay bank and cells, for every
/// recorded op (all but RowReserve).
#[test]
fn s1b_record_is_byte_equal_live_and_replay() {
    let mode = Mode {
        live: true,
        ..Mode::default()
    };
    assert_eq!(
        history_e(mode) + history_n(mode) + history_h(mode) + a_literals(mode),
        0
    );
    // `record` checks everything before its first write.
    let mut r = genesis(Mode::default());
    r.step("E-100", blob("E-100"), Change::AgingStart, pages());
    let frame = blob("E-101");
    let inputs = r.inputs(
        Change::Begin {
            actor: actor(7, 2),
            status: slot(20, 2),
            scope: job3::JobScope::Row,
        },
        Pages {
            jobs: Some(0),
            status: Some(20),
            ..pages()
        },
    );
    let mut afters = Box::new([[0; PAGE_BYTES]; 5]);
    let observation = transition::observe(&inputs, frame, &mut afters).unwrap();
    let layout = decode_layout(frame, inputs.frame, profile()).unwrap();
    let mut rows = r.bank.clone();
    let mut jobs = r.cells.clone();
    let wrong_after = Box::new([0u8; PAGE_BYTES]);
    for (label, slot, after, observation) in [
        ("no-slot", None, Some(&afters[JOB]), observation),
        ("slot-range", Some(32), Some(&afters[JOB]), observation),
        ("no-jobs-after", Some(0), None, observation),
        ("bad-jobs-after", Some(0), Some(&*wrong_after), observation),
        (
            "other-op",
            Some(0),
            Some(&afters[JOB]),
            Observation {
                operation: Operation::Commit,
                ..observation
            },
        ),
    ] {
        let result = history::row::record(
            &mut rows,
            &mut jobs,
            layout,
            &observation,
            after,
            slot,
            profile(),
        );
        assert!(result.is_err(), "{label}");
        assert!(rows == r.bank && jobs == r.cells, "{label}: written");
    }
}

/// H07: Catalog and Row Begins share the one `adopted_local` cursor field
/// (`apply_frame` lends `cursor.adopted_local` to either arm, and
/// `SourceFacts` reads it through `adopted_local()`): a Row Begin whose
/// local is not above it, whichever arm adopted it, is refused with no
/// effect, and an accepted Row Begin advances it for the next Begin of
/// either scope.
#[test]
fn s1b_row_begin_shares_adopted_local() {
    let begin = |local, generation| Change::Begin {
        actor: actor(local, generation),
        status: slot(20, generation),
        scope: job3::JobScope::Row,
    };
    let status20 = Pages {
        jobs: Some(0),
        status: Some(20),
        ..pages()
    };
    let mut r = genesis(Mode::default());
    r.step("E-100", blob("E-100"), Change::AgingStart, pages());
    for adopted in [7, 8, u64::MAX] {
        r.cursor.adopted_local = adopted;
        assert_eq!(
            r.refused(&format!("adopted{adopted}"), blob("E-101")),
            "Some(Invariant)"
        );
    }
    // An issued-only gap below the Begin is valid.
    r.cursor.adopted_local = 3;
    r.step("E-101", blob("E-101"), begin(7, 2), status20);
    assert_eq!(r.cursor.adopted_local(), 7);
    r.step(
        "E-102",
        blob("E-102"),
        Change::Commit { terminal: 0 },
        status20,
    );
    r.step(
        "E-103",
        blob("E-103"),
        Change::TerminalFinish { ordinal: 0 },
        status20,
    );
    // Non-Begin groups never move it; the next Begin (local 8) must exceed 7.
    assert_eq!(r.cursor.adopted_local(), 7);
    r.cursor.adopted_local = 8;
    assert_eq!(r.refused("adopted8", blob("E-104")), "Some(Invariant)");
    r.cursor.adopted_local = 7;
    r.step("E-104", blob("E-104"), begin(8, 3), status20);
    assert_eq!(r.cursor.adopted_local(), 8);
}

/// `record` owns its slot precondition (review A2): a Begin slot must be an
/// all-zero row, and any other Tx op must find this actor's own STABLE row
/// at this Status with a clear tail. Each refusal leaves the bank and cells
/// untouched; the actor's own row is accepted.
#[test]
fn s1d_record_guards_its_own_slot() {
    let status20 = Pages {
        jobs: Some(0),
        status: Some(20),
        ..pages()
    };
    let record =
        |r: &Replay, frame: &[u8], change: Change<'static>, slot: usize, rows: &mut Vec<u8>| {
            let inputs = r.inputs(change, status20);
            let mut afters = Box::new([[0; PAGE_BYTES]; 5]);
            let observation = transition::observe(&inputs, frame, &mut afters).unwrap();
            let layout = decode_layout(frame, inputs.frame, profile()).unwrap();
            let mut jobs = r.cells.clone();
            let result = history::row::record(
                rows,
                &mut jobs,
                layout,
                &observation,
                Some(&afters[JOB]),
                Some(slot),
                profile(),
            );
            (result, jobs == r.cells)
        };
    let mut r = genesis(Mode::default());
    r.step("E-100", blob("E-100"), Change::AgingStart, pages());
    let begin = Change::Begin {
        actor: actor(7, 2),
        status: slot(20, 2),
        scope: job3::JobScope::Row,
    };
    for at in [0, ledger::PHASE, ledger::SLOT_BYTES - 1] {
        let mut rows = r.bank.clone();
        rows[at] = 1;
        let before = rows.clone();
        let (result, cells) = record(&r, blob("E-101"), begin, 0, &mut rows);
        assert!(
            result.is_err() && cells && rows == before,
            "Begin row byte {at}"
        );
    }
    let mut rows = r.bank.clone();
    assert!(record(&r, blob("E-101"), begin, 0, &mut rows).0.is_ok());
    r.step("E-101", blob("E-101"), begin, status20);
    let commit = Change::Commit { terminal: 0 };
    let own = ledger::row(&r.bank, 0).to_vec();
    let mut wrong: Vec<(&str, usize, Vec<u8>)> = vec![("empty slot", 1, r.bank.clone())];
    for (label, at) in [
        ("phase", ledger::PHASE),
        ("tail", ledger::PHASE + 1),
        ("location", ledger::LOCATION.start + 8),
        ("actor", ledger::ACTOR.start + 8),
    ] {
        let mut rows = r.bank.clone();
        rows[at] ^= 1;
        wrong.push((label, 0, rows));
    }
    // Another actor's row copied into slot 1 is still not this actor's.
    let mut rows = r.bank.clone();
    rows[ledger::SLOT_BYTES..2 * ledger::SLOT_BYTES].copy_from_slice(&own);
    rows[ledger::SLOT_BYTES + ledger::ACTOR.start + 8] ^= 1;
    wrong.push(("foreign actor", 1, rows));
    for (label, slot, mut rows) in wrong {
        let before = rows.clone();
        let (result, cells) = record(&r, blob("E-102"), commit, slot, &mut rows);
        assert!(result.is_err() && cells && rows == before, "{label}");
    }
    let mut rows = r.bank.clone();
    let (result, _) = record(&r, blob("E-102"), commit, 0, &mut rows);
    assert!(result.is_ok());
    assert!(ledger::row(&rows, 0).iter().all(|byte| *byte == 0));
}
