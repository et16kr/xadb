//! N1: C-owned scope3 Row group producers and the whole-group observer against
//! independent `module-row-format/transition/author.py` literals and A's
//! MCA00-V06 Forward/CLR literals. Literal observations confer no actual
//! source, reservation, pin, staging, installation or recovery authority.
//! S1-a repeats every literal and variant observation through
//! `observe_borrowed` over a lent replay tail.

use super::{
    Change, Inputs, Issued, JOB, LEAF, Observation, ROOT, ROW_REPLAY_SCRATCH_BYTES, ReplayLoan,
    ReservePlan, STATUS, TableSource, UNDO, frame, observe, observe_borrowed, observe_with,
    prepare, produce,
};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        control::{Profile, row_profile},
        obligation,
        page::{DIGEST_OFFSET, FileId, PAGE_BYTES, PageIdentity, PageKind, PageLsn, PageRef},
        retention_genesis::{self, RetentionGenesisPage},
        retention_obligation,
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self, JobCells, JobScope, source_tests as frames},
        },
        row_record::{self, RecordExpectation, RecordVersion, RowSchemaInput},
        semantic_row::{
            Error, FRAME_BYTES, Operation, decode_layout, jobs, status, status::StatementBoundary,
            undo,
        },
        status_home::StatusHome,
        transaction_record::{StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef},
        tree_page::{
            self, EntryPayload, TreeExpectation,
            edit::{LeafEdit, MAX_LEAF_EDIT_CELLS},
        },
        undo_ref::UndoRef,
        wal_frame::{self, FrameExpectation, WalStreamIdentity},
    },
    transaction::TxnId,
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch},
};
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
const PROFILE: &[u8; 4096] = scope3!("profile3.bin");
/// Every framing-valid variant and the exact refusal its base's observer must
/// give: the recomputed canonical group differs from the candidate.
const REJECTS: [(&str, Error); 19] = [
    ("R-A-noh-301-count", Error::Corrupt),
    ("R-A-noh-301-status", Error::Corrupt),
    ("R-A-noh-303-redo", Error::Corrupt),
    ("R-A-h-501-edge", Error::Corrupt),
    ("R-A-h-501-item", Error::Corrupt),
    ("R-A-h-503-core", Error::Corrupt),
    ("R-A-h-503-descriptor", Error::Corrupt),
    ("R-E-100-descriptor", Error::Corrupt),
    ("R-E-101-job", Error::Corrupt),
    ("R-E-102-q", Error::Corrupt),
    ("R-E-107-cursor", Error::Corrupt),
    ("R-N-302-patch", Error::Corrupt),
    ("R-N-305-undo", Error::Corrupt),
    ("R-H-505-item", Error::Corrupt),
    ("R-H-506-sequence", Error::Corrupt),
    ("R-H-509-owner", Error::Corrupt),
    ("R-H-510-digest", Error::Corrupt),
    ("R-H-511-count", Error::Corrupt),
    ("R-H-515-root", Error::Corrupt),
];

type Page = [u8; PAGE_BYTES];
type Descriptor = [u8; 1536];

thread_local! {
    /// S1-a: while set, every World observation of a literal or R- variant is
    /// repeated through `observe_borrowed`; counts (accepted, refused).
    static BORROWED: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

/// A lent replay tail between two sentinel runs, as the WORK_END tail sits
/// between provenance and the home page.
fn new_lane() -> Vec<u8> {
    let mut lane = vec![0; ROW_REPLAY_SCRATCH_BYTES + 64];
    lane[..32].fill(0x97);
    lane[ROW_REPLAY_SCRATCH_BYTES + 32..].fill(0x53);
    lane
}
fn lent(lane: &mut [u8]) -> &mut [u8; ROW_REPLAY_SCRATCH_BYTES] {
    (&mut lane[32..ROW_REPLAY_SCRATCH_BYTES + 32])
        .try_into()
        .unwrap()
}
/// The lent tail is zero again and neither neighbour changed.
fn restored(label: &str, lane: &[u8]) {
    assert!(
        lane[32..ROW_REPLAY_SCRATCH_BYTES + 32]
            .iter()
            .all(|byte| *byte == 0),
        "{label}: lent tail"
    );
    assert!(
        lane[..32].iter().all(|byte| *byte == 0x97),
        "{label}: before"
    );
    assert!(
        lane[ROW_REPLAY_SCRATCH_BYTES + 32..]
            .iter()
            .all(|byte| *byte == 0x53),
        "{label}: after"
    );
}

/// Repeat one owned observation through the borrowed observer from the same
/// `fill` loans: the same result, all five after loans byte for byte and the
/// lent tail restored.
fn borrowed_twin<'a>(
    label: &str,
    inputs: &Inputs<'a>,
    bytes: &'a [u8],
    fill: u8,
    owned: std::result::Result<Observation, Error>,
    owned_after: &[Page; 5],
) {
    let Some((accepted, refused)) = BORROWED.get() else {
        return;
    };
    let mut lane = new_lane();
    let mut after = loans(fill);
    let result = observe_borrowed(inputs, bytes, &mut after, lent(&mut lane));
    assert_eq!(result, owned, "{label}: borrowed result");
    assert!(*after == *owned_after, "{label}: borrowed after loans");
    restored(label, &lane);
    BORROWED.set(Some(match result {
        Ok(_) => (accepted + 1, refused),
        Err(_) => (accepted, refused + 1),
    }));
}

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
fn page_of(bytes: &[u8]) -> Page {
    bytes.try_into().unwrap()
}

/// Byte equality reporting only the first difference.
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
    row_profile::decode_profile(PROFILE, None).unwrap()
}
fn page_ref(page_no: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0001).unwrap(),
        page_no,
        incarnation: generation(incarnation),
    }
}
fn actor(local: u64, status_generation: u64) -> TxnRef {
    let id = TxnId {
        database: namespace().database,
        timeline: 1,
        writer_epoch: 1,
        local,
    };
    TxnRef::new(id, generation(status_generation)).unwrap()
}
fn slot(page_no: u64, slot_generation: u64) -> StatusSlotRef {
    StatusSlotRef::new(page_ref(page_no, 2), 0, generation(slot_generation)).unwrap()
}
fn undo_ref(page_no: u64, slot_generation: u64) -> UndoRef {
    UndoRef::new(page_ref(page_no, 2), 0, generation(slot_generation)).unwrap()
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
fn expectation(previous_lsn: u64) -> FrameExpectation {
    FrameExpectation {
        stream: WalStreamIdentity {
            database: namespace().database,
            timeline: 1,
            writer_epoch: 1,
        },
        previous_lsn,
    }
}
fn boundary(sequence: u64, mark: Option<UndoRef>) -> StatementBoundary {
    StatementBoundary { sequence, mark }
}
fn loans(fill: u8) -> Box<[Page; 5]> {
    vec![[fill; PAGE_BYTES]; 5]
        .into_boxed_slice()
        .try_into()
        .unwrap()
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

/// The RowLeaf's own page reference from its header (file, page, incarnation).
fn leaf_ref(leaf: &Page) -> PageRef {
    PageRef::decode(&leaf[24..48]).unwrap()
}

/// A's full noh-300 RowLeaf with row 10 removed and row 11's record copied to
/// row 18: row 10 is absent and its Reserve cell (201B) no longer fits.
fn crowded_leaf() -> Box<Page> {
    let full = scope3!("noh-300-leaf-full.bin");
    let expected = TreeExpectation {
        identity: PageIdentity {
            database: namespace().database,
            page: leaf_ref(full),
            owner: table().object,
            owner_generation: table().generation,
        },
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    };
    let columns = columns();
    let schema = RowSchemaInput::Columns(&columns);
    let lsn = PageLsn(u64_at(full, 64));
    let view = tree_page::decode_tree_page(full, expected).unwrap();
    let eleven = 11u64.to_be_bytes();
    let at = view.lower_bound(&eleven).unwrap();
    assert!(at.equal);
    let EntryPayload::Value(value) = view.entry(at.index).unwrap().payload else {
        panic!("row 11 value");
    };
    let record = row_record::decode_row_record_with_schema(
        value,
        RecordExpectation {
            namespace: namespace(),
            row: RowId::new(11).unwrap(),
        },
        schema,
    )
    .unwrap();
    let row = RowId::new(18).unwrap();
    let mut bytes = vec![0; 4096];
    let length = row_record::encode_row_record_with_schema(
        RecordExpectation {
            namespace: namespace(),
            row,
        },
        schema,
        RecordVersion {
            row,
            ..record.version()
        },
        record.payload(),
        &mut bytes,
    )
    .unwrap();
    let mut cell_bytes = vec![0; 4096];
    let eighteen = 18u64.to_be_bytes();
    let inserted = tree_page::encode_leaf_cell(
        PageKind::RowLeaf,
        0,
        &eighteen,
        &bytes[..length],
        &mut cell_bytes,
    )
    .unwrap();
    let ten = 10u64.to_be_bytes();
    let mut refs: Vec<&[u8]> = vec![&[]; MAX_LEAF_EDIT_CELLS];
    let mut removed: Box<Page> = vec![0; PAGE_BYTES].into_boxed_slice().try_into().unwrap();
    view.prepare_leaf_edit(LeafEdit::Remove(&ten), &mut refs[..])
        .unwrap()
        .encode(lsn, &mut removed[..])
        .unwrap();
    let view = tree_page::decode_tree_page(&removed[..], expected).unwrap();
    let mut refs: Vec<&[u8]> = vec![&[]; MAX_LEAF_EDIT_CELLS];
    let plan = view
        .prepare_leaf_edit(LeafEdit::Insert(inserted), &mut refs[..])
        .unwrap();
    assert!(!plan.needs_split());
    let mut crowded: Box<Page> = vec![0; PAGE_BYTES].into_boxed_slice().try_into().unwrap();
    plan.encode(lsn, &mut crowded[..]).unwrap();
    crowded
}

/// Which stable before observations one step borrows.
#[derive(Clone, Copy, Default)]
struct Pages {
    jobs: Option<usize>,
    status: Option<u64>,
    undo: Option<u64>,
    previous: Option<u64>,
    table: bool,
}

/// The simulated stabilized state between groups (test-only bookkeeping).
struct World {
    lsn: u64,
    root: Page,
    jobs: [Option<Page>; 5],
    bank: [[u8; JOB_BYTES]; JOB_COUNT],
    status: Vec<(u64, Page)>,
    undo: Vec<(u64, Page)>,
    leaf: Option<Page>,
    transaction: Option<Descriptor>,
    aging: Option<Descriptor>,
    columns: [Column; 2],
    /// Counts R- variants checked against the before state of their base.
    rejects: Option<Cell<usize>>,
}

impl World {
    fn empty(lsn: u64, root: &[u8]) -> Box<Self> {
        Box::new(Self {
            lsn,
            root: page_of(root),
            jobs: [None; 5],
            bank: [[0; JOB_BYTES]; JOB_COUNT],
            status: Vec::new(),
            undo: Vec::new(),
            leaf: None,
            transaction: None,
            aging: None,
            columns: columns(),
            rejects: None,
        })
    }

    /// A's snapshot pages and descriptors at `lsn`.
    #[allow(clippy::too_many_arguments, reason = "one literal snapshot")]
    fn from_a(
        lsn: u64,
        root: &Page,
        jobs: &[(usize, &Page)],
        bank: &[u8; JOB_BYTES * JOB_COUNT],
        status: &Page,
        undo: &[(u64, &Page)],
        leaf: &Page,
        transaction: &Descriptor,
        aging: &Descriptor,
    ) -> Box<Self> {
        let mut world = Self::empty(lsn, root);
        for (index, page) in jobs {
            world.jobs[*index] = Some(**page);
        }
        for (ordinal, cell) in world.bank.iter_mut().enumerate() {
            cell.copy_from_slice(&bank[ordinal * JOB_BYTES..(ordinal + 1) * JOB_BYTES]);
        }
        world.status.push((9, *status));
        for (page_no, page) in undo {
            world.undo.push((*page_no, **page));
        }
        world.leaf = Some(*leaf);
        world.transaction = Some(*transaction);
        world.aging = Some(*aging);
        world
    }

    /// Genesis Root/Jobs (checked against the production genesis encoder)
    /// and a Free(1) Status home on page20 at LSN10.
    fn genesis() -> Box<Self> {
        let mut root = [0xa5; PAGE_BYTES];
        retention_genesis::encode_row_current(namespace(), RetentionGenesisPage::Root, &mut root)
            .unwrap();
        same("E genesis Root", &root, blob("E-root-0"));
        let mut jobs = [0xa5; PAGE_BYTES];
        retention_genesis::encode_row_current(
            namespace(),
            RetentionGenesisPage::Jobs { index: 0 },
            &mut jobs,
        )
        .unwrap();
        same("E genesis Jobs0", &jobs, blob("E-jobs0-0"));
        let mut world = Self::empty(99, &root);
        world.jobs[0] = Some(jobs);
        for job in &mut world.bank {
            job.copy_from_slice(cell(&jobs, 0));
        }
        world.status.push((20, page_of(blob("E-status20-0"))));
        world
    }

    fn find(pages: &[(u64, Page)], page_no: u64) -> &Page {
        &pages
            .iter()
            .find(|(number, _)| *number == page_no)
            .unwrap_or_else(|| panic!("page {page_no}"))
            .1
    }
    fn replace(pages: &mut Vec<(u64, Page)>, page_no: u64, page: &Page) {
        match pages.iter_mut().find(|(number, _)| *number == page_no) {
            Some(entry) => entry.1 = *page,
            None => pages.push((page_no, *page)),
        }
    }

    fn inputs<'a>(&'a self, change: Change<'a>, pages: Pages) -> Inputs<'a> {
        Inputs {
            profile: profile(),
            frame: expectation(self.lsn),
            change,
            jobs: &self.bank,
            root: &self.root,
            jobs_page: pages
                .jobs
                .map(|index| self.jobs[index].as_ref().expect("jobs page")),
            status: pages
                .status
                .map(|page_no| Self::find(&self.status, page_no)),
            undo: pages.undo.map(|page_no| Self::find(&self.undo, page_no)),
            previous: pages
                .previous
                .map(|page_no| Self::find(&self.undo, page_no)),
            table: pages.table.then(|| TableSource {
                handle: table(),
                anchor: page_ref(12, 2),
                anchor_image: scope3!("anchor.bin"),
                leaf_image: self.leaf.as_ref(),
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

    /// Every `R-<name>-*` variant keeps framing valid and is refused by the
    /// observer against the same before observations.
    fn check_rejects(&self, name: &str, inputs: &Inputs<'_>) {
        let Some(counter) = &self.rejects else {
            return;
        };
        let prefix = format!("R-{name}-");
        for line in INDEX.lines() {
            let variant = line.split('\t').next().unwrap();
            if !variant.starts_with(&prefix) {
                continue;
            }
            let bytes = blob(variant);
            decode_layout(bytes, inputs.frame, inputs.profile)
                .unwrap_or_else(|error| panic!("{variant} framing {error:?}"));
            let expected = REJECTS
                .iter()
                .find(|(name, _)| *name == variant)
                .unwrap_or_else(|| panic!("{variant} has no expected refusal"))
                .1;
            let mut after = loans(0);
            let owned = observe(inputs, bytes, &mut after);
            borrowed_twin(variant, inputs, bytes, 0, owned, &after);
            let result = owned.map(|_| ());
            assert_eq!(result, Err(expected), "{variant}");
            println!("N1_REJECT\t{variant}\t{expected:?}");
            counter.set(counter.get() + 1);
        }
    }

    /// Produce, compare with the literal, observe the literal, install.
    fn produce(&mut self, name: &str, change: Change<'_>, pages: Pages) -> Observation {
        let expected = blob(name);
        let mut after = loans(0xa5);
        let mut frame = vec![0xa5; FRAME_BYTES];
        let observation = {
            let inputs = self.inputs(change, pages);
            let observation = produce(&inputs, &mut after, &mut frame)
                .unwrap_or_else(|error| panic!("{name}: produce {error:?}"));
            assert_eq!(observation.frame_bytes, expected.len(), "{name}");
            assert!(frame[expected.len()..].iter().all(|byte| *byte == 0xa5));
            same(name, &frame[..expected.len()], expected);
            let mut again = loans(0);
            let observed = observe(&inputs, expected, &mut again)
                .unwrap_or_else(|error| panic!("{name}: observe {error:?}"));
            assert_eq!(observed, observation, "{name}");
            borrowed_twin(name, &inputs, expected, 0, Ok(observed), &again);
            for &role in &observation.order[..usize::from(observation.count)] {
                same(name, &again[usize::from(role)], &after[usize::from(role)]);
            }
            self.cross_check(name, change, pages, expected, &after, &observation);
            self.check_rejects(name, &inputs);
            observation
        };
        self.install(name, &observation, &after, expected);
        println!(
            "N1_GROUP\t{name}\t{:?}\tbytes={}\torder={:?}",
            observation.operation,
            observation.frame_bytes,
            &observation.order[..usize::from(observation.count)]
        );
        observation
    }

    /// Observe a candidate this module does not produce (A's Forward/CLR).
    fn observe(
        &mut self,
        name: &str,
        bytes: &'static [u8],
        change: Change<'_>,
        pages: Pages,
    ) -> Observation {
        let mut after = loans(0xa5);
        let observation = {
            let inputs = self.inputs(change, pages);
            let observation = observe(&inputs, bytes, &mut after)
                .unwrap_or_else(|error| panic!("{name}: observe {error:?}"));
            assert_eq!(observation.frame_bytes, bytes.len());
            borrowed_twin(name, &inputs, bytes, 0xa5, Ok(observation), &after);
            // A's operations stay unsupported on the producer side.
            let mut frame = vec![0xa5; FRAME_BYTES];
            let mut untouched = loans(0xa5);
            assert_eq!(
                produce(&inputs, &mut untouched, &mut frame).map(|_| ()),
                Err(Error::Unsupported),
                "{name}"
            );
            self.cross_check(name, change, pages, bytes, &after, &observation);
            self.check_rejects(name, &inputs);
            observation
        };
        self.install(name, &observation, &after, bytes);
        println!(
            "N1_OBSERVED\t{name}\t{:?}\tbytes={}\torder={:?}",
            observation.operation,
            observation.frame_bytes,
            &observation.order[..usize::from(observation.count)]
        );
        observation
    }

    /// The existing scope3 local observers (status.rs, undo.rs, jobs.rs) over
    /// the same frame, before pages and computed afters, so the transition's
    /// rules and theirs cannot drift. leaf.rs needs a Catalog record, which
    /// the transition inputs do not carry; its Row rules are compared through
    /// A's literals instead.
    fn cross_check(
        &self,
        name: &str,
        change: Change<'_>,
        pages: Pages,
        frame: &[u8],
        after: &[Page; 5],
        observed: &Observation,
    ) {
        let (Some(actor), Some(location)) = (observed.actor, observed.status) else {
            return;
        };
        let layout = decode_layout(frame, expectation(self.lsn), profile()).unwrap();
        let op = observed.operation;
        let present =
            |role: usize| observed.order[..usize::from(observed.count)].contains(&(role as u8));
        let context = undo::UndoContext {
            profile: profile(),
            actor,
            location,
        };
        let jobs = JobCells::original(&self.bank);
        let statement = match change {
            Change::Forward { statement, .. } | Change::StatementFinish { statement, .. } => {
                Some(statement)
            }
            Change::RollbackStart { statement, .. } | Change::Clr { statement, .. } => statement,
            _ => None,
        };
        let status_before = self
            .status
            .iter()
            .find(|(page_no, _)| *page_no == location.page().page_no)
            .map(|(_, page)| page);
        let head_before = status_before.and_then(|page| {
            match StatusHome::decode(cell(page, 0)).unwrap().core() {
                TransactionStatusCell::Occupied(status) => status.undo_head(),
                TransactionStatusCell::Free(_) => None,
            }
        });
        let commit = match op {
            Operation::Commit => Some(CommitSeq::new(layout.core().q()).unwrap()),
            Operation::Prune => match StatusHome::decode(cell(status_before.unwrap(), 0))
                .unwrap()
                .core()
            {
                TransactionStatusCell::Occupied(status) => match status.state() {
                    TransactionState::Committed(q) => Some(q),
                    _ => panic!("{name}: Prune before Commit"),
                },
                TransactionStatusCell::Free(_) => panic!("{name}: free Status"),
            },
            _ => None,
        };
        let status_images = present(STATUS).then(|| status::StatusImages {
            before: status_before.unwrap(),
            after: &after[STATUS],
        });
        if let Some(images) = status_images {
            status::observe(
                layout,
                status::StatusContext {
                    profile: profile(),
                    actor,
                    location,
                    statement,
                    commit,
                },
                images,
            )
            .unwrap_or_else(|error| panic!("{name}: status.rs {error:?}"));
        }
        let ordinal = observed.job_ordinal;
        if present(UNDO) {
            let row = matches!(op, Operation::Forward | Operation::Clr | Operation::Prune);
            let source = match (change, observed.issued) {
                (Change::Reserve(plan), Some(issued)) => undo::UndoSource::Reserve {
                    ordinal: ordinal.unwrap(),
                    planned: undo::UndoReservation {
                        reference: issued.undo,
                        sequence: issued.sequence,
                        statement: plan.statement,
                        mutation: issued.mutation,
                    },
                },
                _ => undo::UndoSource::Existing {
                    ordinal: ordinal.unwrap(),
                },
            };
            let images = undo::UndoImages {
                before: World::find(&self.undo, u64_at(&after[UNDO], 32)),
                after: &after[UNDO],
                predecessor: if row {
                    pages
                        .previous
                        .map(|page_no| World::find(&self.undo, page_no))
                } else {
                    None
                },
            };
            let proof = row.then(|| undo::StatusProof {
                statement,
                commit,
                images: status_images.unwrap(),
            });
            undo::observe(layout, context, jobs, source, images, proof)
                .unwrap_or_else(|error| panic!("{name}: undo.rs {error:?}"));
        }
        if present(JOB) {
            let ordinal = ordinal.unwrap();
            let reserve = match (change, observed.issued) {
                (Change::Reserve(plan), Some(issued)) => {
                    let job = cell(&after[JOB], ordinal % 15);
                    Some(jobs::ReservePlan {
                        undo: issued.undo,
                        sequence: issued.sequence,
                        table: plan.table,
                        pair: [page_ref(12, 2), leaf_ref(self.leaf.as_ref().unwrap())],
                        statement: plan.statement,
                        mutation: issued.mutation,
                        row: issued.row,
                        cell_bytes: (180 + plan.canonical.len()) as u32,
                        // The producer's own record digest; the observer checks
                        // the after Job against it, not its derivation.
                        projection: job[288..320].try_into().unwrap(),
                        predecessor: head_before.is_some(),
                    })
                }
                _ => None,
            };
            let sender = matches!(
                op,
                Operation::Forward | Operation::Commit | Operation::AbortEnd
            )
            .then(|| self.transaction.as_ref().unwrap());
            jobs::observe(
                layout,
                jobs::Inputs {
                    context,
                    jobs,
                    ordinal,
                    images: jobs::JobImages {
                        before: self.jobs[ordinal / 15].as_ref().unwrap(),
                        after: &after[JOB],
                    },
                    reserve,
                    sender,
                    commit,
                },
            )
            .unwrap_or_else(|error| panic!("{name}: jobs.rs {error:?}"));
        }
        println!(
            "N1_CROSS\t{name}\tstatus={}\tundo={}\tjobs={}",
            present(STATUS),
            present(UNDO),
            present(JOB)
        );
    }

    /// Replay each redo record over the current before page, require the
    /// computed after page, then install pages, the Job and descriptors.
    fn install(&mut self, name: &str, observation: &Observation, after: &[Page; 5], frame: &[u8]) {
        let layout = decode_layout(frame, expectation(self.lsn), profile()).unwrap();
        let lsn = layout.header().lsn;
        let mut redo = layout.redo();
        for index in 0..usize::from(observation.count) {
            let role = usize::from(observation.order[index]);
            let length = u32::from_le_bytes(redo[4..8].try_into().unwrap()) as usize;
            let record = wal_frame::decode_redo_record(layout.header(), &redo[..length]).unwrap();
            redo = &redo[length..];
            let image = &after[role];
            let page_no = u64_at(image, 32);
            let before = match role {
                ROOT => self.root,
                JOB => self.jobs[observation.job_ordinal.unwrap() / 15].unwrap(),
                STATUS => *Self::find(&self.status, page_no),
                UNDO => *Self::find(&self.undo, page_no),
                LEAF => self.leaf.unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(record.prior_lsn, u64_at(&before, 64), "{name} prior");
            let mut replay = before;
            replay[64..72].copy_from_slice(&lsn.to_le_bytes());
            let offset = usize::from(record.offset);
            replay[offset..offset + record.data.len()].copy_from_slice(record.data);
            let digest = content_digest(&replay[..DIGEST_OFFSET]);
            replay[DIGEST_OFFSET..].copy_from_slice(&digest);
            same(&format!("{name} redo replay role{role}"), &replay, image);
            match role {
                ROOT => self.root = *image,
                JOB => {
                    let ordinal = observation.job_ordinal.unwrap();
                    self.jobs[ordinal / 15] = Some(*image);
                    self.bank[ordinal].copy_from_slice(cell(image, ordinal % 15));
                }
                STATUS => Self::replace(&mut self.status, page_no, image),
                UNDO => Self::replace(&mut self.undo, page_no, image),
                _ => self.leaf = Some(*image),
            }
        }
        assert!(redo.is_empty());
        let operation = layout.operation();
        if operation.handoff() {
            self.transaction = Some(*layout.descriptor(0).unwrap());
            self.aging = Some(*layout.descriptor(1).unwrap());
        } else if operation.status_patch() {
            self.transaction = Some(*layout.descriptor(0).unwrap());
        } else {
            self.aging = Some(*layout.descriptor(0).unwrap());
        }
        self.lsn = lsn;
    }

    fn check_page(&self, label: &str, actual: &Page, expected: &Page) {
        same(label, actual, expected);
    }
    fn check_bank(&self, label: &str, expected: &[u8; JOB_BYTES * JOB_COUNT]) {
        for (ordinal, job) in self.bank.iter().enumerate() {
            same(
                &format!("{label} Job{ordinal}"),
                job,
                &expected[ordinal * JOB_BYTES..(ordinal + 1) * JOB_BYTES],
            );
        }
    }
}

fn pages() -> Pages {
    Pages::default()
}

// ---- history E: genesis, two empty Row transactions ------------------------

fn history_e(rejects: bool) -> usize {
    let mut world = World::genesis();
    world.rejects = rejects.then(|| Cell::new(0));
    let status20 = Pages {
        jobs: Some(0),
        status: Some(20),
        ..pages()
    };
    world.produce("E-100", Change::AgingStart, pages());
    let begin = world.produce(
        "E-101",
        Change::Begin {
            actor: actor(7, 2),
            status: slot(20, 2),
            scope: JobScope::Row,
        },
        status20,
    );
    assert_eq!(begin.job_ordinal, Some(0));
    assert_eq!(begin.actor, Some(actor(7, 2)));
    world.produce("E-102", Change::Commit { terminal: 0 }, status20);
    world.produce("E-103", Change::TerminalFinish { ordinal: 0 }, status20);
    let begin = world.produce(
        "E-104",
        Change::Begin {
            actor: actor(8, 3),
            status: slot(20, 3),
            scope: JobScope::Row,
        },
        status20,
    );
    assert_eq!(begin.job_ordinal, Some(1));
    world.produce(
        "E-105",
        Change::RollbackStart {
            terminal: 1,
            statement: None,
        },
        Pages {
            status: Some(20),
            ..pages()
        },
    );
    world.produce("E-106", Change::AbortEnd { terminal: 1 }, status20);
    world.produce("E-107", Change::TerminalFinish { ordinal: 1 }, status20);
    assert_eq!(u64_at(cell(&world.root, 0), 72), 2, "queue cursor");
    world.rejects.map_or(0, Cell::into_inner)
}

#[test]
fn n1_empty_transactions_match_literals() {
    assert_eq!(history_e(false), 0);
}

// ---- history N: A's noh history to a complete cleanup ----------------------

fn noh_301() -> Box<World> {
    World::from_a(
        301,
        scope3!("noh-301-root.bin"),
        &[(0, scope3!("noh-301-jobs2.bin"))],
        scope3!("noh-301-bank.bin"),
        scope3!("noh-301-status.bin"),
        &[(11, scope3!("noh-301-undo11.bin"))],
        scope3!("noh-301-leaf.bin"),
        scope3!("noh-301-tx.bin"),
        scope3!("noh-301-aging.bin"),
    )
}

fn history_n(rejects: bool) -> usize {
    let mut world = noh_301();
    world.rejects = rejects.then(|| Cell::new(0));
    let status9 = Pages {
        jobs: Some(0),
        status: Some(9),
        ..pages()
    };
    world.produce(
        "N-302",
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
    world.check_page(
        "N-302 Status",
        World::find(&world.status, 9),
        scope3!("noh-302-status.bin"),
    );
    same(
        "N-302 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("noh-302-tx.bin"),
    );
    world.observe(
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
    check_snapshot_noh303(&world);
    world.produce("N-304", Change::AbortEnd { terminal: 2 }, status9);
    let undo11 = Pages {
        jobs: Some(0),
        undo: Some(11),
        ..pages()
    };
    for name in ["N-305", "N-306", "N-307"] {
        world.produce(name, Change::ReleaseRole { ordinal: 3 }, undo11);
    }
    world.produce("N-308", Change::RootFree { ordinal: 3 }, undo11);
    world.produce("N-309", Change::TerminalFinish { ordinal: 2 }, status9);
    world.rejects.map_or(0, Cell::into_inner)
}

fn check_snapshot_noh303(world: &World) {
    world.check_page("noh-303 Root", &world.root, scope3!("noh-303-root.bin"));
    world.check_page(
        "noh-303 Jobs2",
        world.jobs[0].as_ref().unwrap(),
        scope3!("noh-303-jobs2.bin"),
    );
    world.check_page(
        "noh-303 Status",
        World::find(&world.status, 9),
        scope3!("noh-303-status.bin"),
    );
    world.check_page(
        "noh-303 Undo11",
        World::find(&world.undo, 11),
        scope3!("noh-303-undo11.bin"),
    );
    world.check_page(
        "noh-303 Leaf",
        world.leaf.as_ref().unwrap(),
        scope3!("noh-303-leaf.bin"),
    );
    same(
        "noh-303 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("noh-303-tx.bin"),
    );
    same(
        "noh-303 Aging",
        world.aging.as_ref().unwrap(),
        scope3!("noh-303-aging.bin"),
    );
    world.check_bank("noh-303", scope3!("noh-303-bank.bin"));
}

#[test]
fn n1_full_rollback_cleanup_match_literals() {
    assert_eq!(history_n(false), 0);
}

// ---- history H: statement rollback, Reserve/Cancel, Commit, Prune ---------

fn h_501() -> Box<World> {
    World::from_a(
        501,
        scope3!("h-501-root.bin"),
        &[
            (0, scope3!("h-501-jobs2.bin")),
            (1, scope3!("h-501-jobs3.bin")),
        ],
        scope3!("h-501-bank.bin"),
        scope3!("h-501-status.bin"),
        &[
            (10, scope3!("h-500-undo10.bin")),
            (11, scope3!("h-501-undo11.bin")),
        ],
        scope3!("h-501-leaf.bin"),
        scope3!("h-501-tx.bin"),
        scope3!("h-501-aging.bin"),
    )
}

/// H-502..H-504: statement 2 rolls back to its mark through A's CLR and
/// finishes, leaving the h-504 state that H-505 reserves from.
fn h_statement_rollback(world: &mut World) {
    let mark = boundary(2, Some(undo_ref(10, 2)));
    world.produce(
        "H-502",
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
    world.check_page(
        "H-502 Status",
        World::find(&world.status, 9),
        scope3!("h-502-status.bin"),
    );
    same(
        "H-502 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("h-502-tx.bin"),
    );
    world.observe(
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
    world.check_page("h-503 Root", &world.root, scope3!("h-503-root.bin"));
    world.check_page(
        "h-503 Jobs3",
        world.jobs[1].as_ref().unwrap(),
        scope3!("h-503-jobs3.bin"),
    );
    world.check_page(
        "h-503 Status",
        World::find(&world.status, 9),
        scope3!("h-503-status.bin"),
    );
    world.check_page(
        "h-503 Undo11",
        World::find(&world.undo, 11),
        scope3!("h-503-undo11.bin"),
    );
    world.check_page(
        "h-503 Leaf",
        world.leaf.as_ref().unwrap(),
        scope3!("h-503-leaf.bin"),
    );
    same(
        "h-503 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("h-503-tx.bin"),
    );
    same(
        "h-503 Aging",
        world.aging.as_ref().unwrap(),
        scope3!("h-503-aging.bin"),
    );
    world.check_bank("h-503", scope3!("h-503-bank.bin"));
    world.produce(
        "H-504",
        Change::StatementFinish {
            terminal: 14,
            statement: mark,
        },
        Pages {
            status: Some(9),
            ..pages()
        },
    );
}

fn history_h(rejects: bool) -> usize {
    let mut world = h_501();
    world.rejects = rejects.then(|| Cell::new(0));
    h_statement_rollback(&mut world);
    World::replace(&mut world.undo, 14, &page_of(blob("H-undo14-0")));
    let reserve = Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(14),
        table: true,
        ..pages()
    };
    let plan = ReservePlan {
        terminal: 14,
        container: page_ref(14, 2),
        table: table(),
        statement: 3,
        canonical: blob("H-505-canonical"),
    };
    let reserved = world.produce("H-505", Change::Reserve(&plan), reserve);
    assert_eq!(reserved.job_ordinal, Some(17));
    assert_eq!(
        reserved.issued,
        Some(Issued {
            row: RowId::new(10).unwrap(),
            undo: undo_ref(14, 3),
            sequence: 3,
            mutation: 3,
        })
    );
    world.produce(
        "H-506",
        Change::Cancel { ordinal: 17 },
        Pages {
            jobs: Some(1),
            status: Some(9),
            undo: Some(14),
            ..pages()
        },
    );
    let plan = ReservePlan {
        statement: 4,
        canonical: blob("H-507-canonical"),
        ..plan
    };
    let reserved = world.produce("H-507", Change::Reserve(&plan), reserve);
    assert_eq!(
        reserved.issued,
        Some(Issued {
            row: RowId::new(11).unwrap(),
            undo: undo_ref(14, 4),
            sequence: 4,
            mutation: 4,
        })
    );
    let row = Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(14),
        previous: Some(10),
        table: true,
    };
    world.observe(
        "H-508",
        blob("H-508"),
        Change::Forward {
            ordinal: 17,
            statement: boundary(4, None),
        },
        row,
    );
    world.produce(
        "H-509",
        Change::Commit { terminal: 14 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    world.produce("H-510", Change::Prune { ordinal: 17 }, row);
    world.produce(
        "H-511",
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
        world.produce(name, Change::ReleaseRole { ordinal: 16 }, undo11);
    }
    world.produce("H-515", Change::RootFree { ordinal: 16 }, undo11);
    world.rejects.map_or(0, Cell::into_inner)
}

#[test]
fn n1_statement_reserve_commit_prune_match_literals() {
    assert_eq!(history_h(false), 0);
}

// ---- A's Forward/CLR literals through the whole-group observer ------------

/// A's noh-300 snapshot and its Forward301 inputs.
fn noh_300() -> Box<World> {
    World::from_a(
        300,
        scope3!("noh-300-root.bin"),
        &[(0, scope3!("noh-300-jobs2.bin"))],
        scope3!("noh-300-bank.bin"),
        scope3!("noh-300-status.bin"),
        &[(11, scope3!("noh-300-undo11.bin"))],
        scope3!("noh-300-leaf.bin"),
        scope3!("noh-300-tx.bin"),
        scope3!("noh-300-aging.bin"),
    )
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

fn a_literals(rejects: bool) -> usize {
    let mut checked = 0;
    // noh Forward301 from noh-300.
    let mut world = noh_300();
    world.rejects = rejects.then(|| Cell::new(0));
    let (change, inputs) = forward_301();
    let observed = world.observe("A-noh-301", scope3!("noh-301-forward.wal"), change, inputs);
    assert_eq!(observed.operation, Operation::Forward);
    world.check_page("noh-301 Root", &world.root, scope3!("noh-301-root.bin"));
    world.check_page(
        "noh-301 Jobs2",
        world.jobs[0].as_ref().unwrap(),
        scope3!("noh-301-jobs2.bin"),
    );
    world.check_page(
        "noh-301 Status",
        World::find(&world.status, 9),
        scope3!("noh-301-status.bin"),
    );
    world.check_page(
        "noh-301 Undo11",
        World::find(&world.undo, 11),
        scope3!("noh-301-undo11.bin"),
    );
    world.check_page(
        "noh-301 Leaf",
        world.leaf.as_ref().unwrap(),
        scope3!("noh-301-leaf.bin"),
    );
    same(
        "noh-301 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("noh-301-tx.bin"),
    );
    same(
        "noh-301 Aging",
        world.aging.as_ref().unwrap(),
        scope3!("noh-301-aging.bin"),
    );
    world.check_bank("noh-301", scope3!("noh-301-bank.bin"));
    checked += world.rejects.take().map_or(0, Cell::into_inner);

    // noh CLR303 from noh-301 with A's RollbackStart302 Status/Tx.
    let mut world = noh_301();
    world.lsn = 302;
    World::replace(&mut world.status, 9, scope3!("noh-302-status.bin"));
    world.transaction = Some(*scope3!("noh-302-tx.bin"));
    world.rejects = rejects.then(|| Cell::new(0));
    world.observe(
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
    check_snapshot_noh303(&world);
    checked += world.rejects.take().map_or(0, Cell::into_inner);

    // h Forward501 from h-500 with the H proof page10.
    let mut world = World::from_a(
        500,
        scope3!("h-500-root.bin"),
        &[
            (0, scope3!("h-500-jobs2.bin")),
            (1, scope3!("h-500-jobs3.bin")),
        ],
        scope3!("h-500-bank.bin"),
        scope3!("h-500-status.bin"),
        &[
            (10, scope3!("h-500-undo10.bin")),
            (11, scope3!("h-500-undo11.bin")),
        ],
        scope3!("h-500-leaf.bin"),
        scope3!("h-500-tx.bin"),
        scope3!("h-500-aging.bin"),
    );
    world.rejects = rejects.then(|| Cell::new(0));
    let h_row = Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(11),
        previous: Some(10),
        table: true,
    };
    world.observe(
        "A-h-501",
        scope3!("h-501-forward.wal"),
        Change::Forward {
            ordinal: 16,
            statement: boundary(2, None),
        },
        h_row,
    );
    world.check_page("h-501 Root", &world.root, scope3!("h-501-root.bin"));
    world.check_page(
        "h-501 Jobs3",
        world.jobs[1].as_ref().unwrap(),
        scope3!("h-501-jobs3.bin"),
    );
    world.check_page(
        "h-501 Status",
        World::find(&world.status, 9),
        scope3!("h-501-status.bin"),
    );
    world.check_page(
        "h-501 Undo11",
        World::find(&world.undo, 11),
        scope3!("h-501-undo11.bin"),
    );
    world.check_page(
        "h-501 Leaf",
        world.leaf.as_ref().unwrap(),
        scope3!("h-501-leaf.bin"),
    );
    same(
        "h-501 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("h-501-tx.bin"),
    );
    same(
        "h-501 Aging",
        world.aging.as_ref().unwrap(),
        scope3!("h-501-aging.bin"),
    );
    world.check_bank("h-501", scope3!("h-501-bank.bin"));
    checked += world.rejects.take().map_or(0, Cell::into_inner);

    // h CLR503 (statement mark H) from h-501 with A's RollbackStart502.
    let mut world = h_501();
    world.lsn = 502;
    World::replace(&mut world.status, 9, scope3!("h-502-status.bin"));
    world.transaction = Some(*scope3!("h-502-tx.bin"));
    world.rejects = rejects.then(|| Cell::new(0));
    world.observe(
        "A-h-503",
        scope3!("h-503-clr.wal"),
        Change::Clr {
            ordinal: 16,
            statement: Some(boundary(2, Some(undo_ref(10, 2)))),
        },
        h_row,
    );
    world.check_page("h-503 Root", &world.root, scope3!("h-503-root.bin"));
    world.check_page(
        "h-503 Undo11",
        World::find(&world.undo, 11),
        scope3!("h-503-undo11.bin"),
    );
    world.check_page(
        "h-503 Leaf",
        world.leaf.as_ref().unwrap(),
        scope3!("h-503-leaf.bin"),
    );
    same(
        "h-503 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("h-503-tx.bin"),
    );
    same(
        "h-503 Aging",
        world.aging.as_ref().unwrap(),
        scope3!("h-503-aging.bin"),
    );
    checked += world.rejects.take().map_or(0, Cell::into_inner);

    // ord Forward301: U on page14 follows the RowLeaf page13 physically.
    let mut world = World::from_a(
        300,
        scope3!("ord-300-root.bin"),
        &[(0, scope3!("ord-300-jobs2.bin"))],
        scope3!("ord-300-bank.bin"),
        scope3!("ord-300-status.bin"),
        &[(14, scope3!("ord-300-undo14.bin"))],
        scope3!("ord-300-leaf.bin"),
        scope3!("ord-300-tx.bin"),
        scope3!("ord-300-aging.bin"),
    );
    let observed = world.observe(
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
    assert_eq!(
        &observed.order[..5],
        &[ROOT as u8, JOB as u8, STATUS as u8, LEAF as u8, UNDO as u8]
    );
    world.check_page("ord-301 Root", &world.root, scope3!("ord-301-root.bin"));
    world.check_page(
        "ord-301 Status",
        World::find(&world.status, 9),
        scope3!("ord-301-status.bin"),
    );
    world.check_page(
        "ord-301 Undo14",
        World::find(&world.undo, 14),
        scope3!("ord-301-undo14.bin"),
    );
    world.check_page(
        "ord-301 Leaf",
        world.leaf.as_ref().unwrap(),
        scope3!("ord-301-leaf.bin"),
    );
    same(
        "ord-301 Tx",
        world.transaction.as_ref().unwrap(),
        scope3!("ord-301-tx.bin"),
    );
    same(
        "ord-301 Aging",
        world.aging.as_ref().unwrap(),
        scope3!("ord-301-aging.bin"),
    );
    world.check_bank("ord-301", scope3!("ord-301-bank.bin"));
    checked
}

#[test]
fn n1_observer_accepts_a_forward_clr_literals() {
    assert_eq!(a_literals(false), 0);
}

#[test]
fn n1_observer_rejects_mutated_literals() {
    let checked = a_literals(true) + history_e(true) + history_n(true) + history_h(true);
    println!("N1_REJECTS checked={checked}");
    assert_eq!(checked, REJECTS.len());
}

// ---- refusals return before the first output byte --------------------------

fn refuses(label: &str, inputs: &Inputs<'_>, frame_bytes: usize, expected: Error) {
    let mut after = loans(0xa5);
    let mut frame = vec![0xa5; frame_bytes];
    assert_eq!(
        produce(inputs, &mut after, &mut frame).map(|_| ()),
        Err(expected),
        "{label}"
    );
    assert!(
        after.iter().flatten().all(|byte| *byte == 0xa5),
        "{label} after"
    );
    assert!(frame.iter().all(|byte| *byte == 0xa5), "{label} frame");
    println!("N1_REFUSAL\t{label}\t{expected:?}");
}

#[test]
fn n1_producer_refusals_precede_output() {
    let mut world = World::genesis();
    let status20 = Pages {
        jobs: Some(0),
        status: Some(20),
        ..pages()
    };
    for (label, change) in [
        (
            "forward is A's",
            Change::Forward {
                ordinal: 0,
                statement: boundary(1, None),
            },
        ),
        (
            "clr is A's",
            Change::Clr {
                ordinal: 0,
                statement: None,
            },
        ),
    ] {
        refuses(
            label,
            &world.inputs(change, status20),
            FRAME_BYTES,
            Error::Unsupported,
        );
    }
    world.produce("E-100", Change::AgingStart, pages());
    let begin = |actor, status, scope| Change::Begin {
        actor,
        status,
        scope,
    };
    refuses(
        "Catalog scope is the Catalog owner's",
        &world.inputs(begin(actor(7, 2), slot(20, 2), JobScope::Catalog), status20),
        FRAME_BYTES,
        Error::Unsupported,
    );
    refuses(
        "Status generation does not follow Free(1)",
        &world.inputs(begin(actor(7, 3), slot(20, 3), JobScope::Row), status20),
        FRAME_BYTES,
        Error::IdentityMismatch,
    );
    refuses(
        "short frame loan",
        &world.inputs(begin(actor(7, 2), slot(20, 2), JobScope::Row), status20),
        56_711,
        Error::InvalidInput,
    );
    refuses(
        "Commit of a Free terminal",
        &world.inputs(Change::Commit { terminal: 0 }, status20),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    // An observer refusal: the candidate's opcode is not the requested change.
    {
        let inputs = world.inputs(Change::Commit { terminal: 0 }, status20);
        let mut after = loans(0xa5);
        assert_eq!(
            observe(&inputs, blob("E-101"), &mut after).map(|_| ()),
            Err(Error::IdentityMismatch)
        );
    }
    let stale: Descriptor = world.aging.expect("E-100 Aging");
    world.produce(
        "E-101",
        begin(actor(7, 2), slot(20, 2), JobScope::Row),
        status20,
    );
    refuses(
        "statement rollback without statement credit",
        &world.inputs(
            Change::RollbackStart {
                terminal: 0,
                statement: Some(boundary(1, None)),
            },
            Pages {
                status: Some(20),
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    {
        let mut inputs = world.inputs(Change::Commit { terminal: 0 }, status20);
        inputs.aging = Some(&stale);
        // Its Running sequence names the Root revision before E-101.
        refuses(
            "stale Aging descriptor",
            &inputs,
            FRAME_BYTES,
            Error::IdentityMismatch,
        );
        let mut inputs = world.inputs(Change::Commit { terminal: 0 }, status20);
        inputs.frame = expectation(100);
        refuses(
            "before page newer than the frame",
            &inputs,
            FRAME_BYTES,
            Error::OrderMismatch,
        );
    }

    // Cleanup before the terminal is Ready, from A's noh-303 snapshot.
    let mut world = noh_301();
    world.lsn = 303;
    world.root = *scope3!("noh-303-root.bin");
    world.jobs[0] = Some(*scope3!("noh-303-jobs2.bin"));
    for (ordinal, job) in world.bank.iter_mut().enumerate() {
        job.copy_from_slice(
            &scope3!("noh-303-bank.bin")[ordinal * JOB_BYTES..(ordinal + 1) * JOB_BYTES],
        );
    }
    World::replace(&mut world.status, 9, scope3!("noh-303-status.bin"));
    World::replace(&mut world.undo, 11, scope3!("noh-303-undo11.bin"));
    world.leaf = Some(*scope3!("noh-303-leaf.bin"));
    world.transaction = Some(*scope3!("noh-303-tx.bin"));
    world.aging = Some(*scope3!("noh-303-aging.bin"));
    let undo11 = Pages {
        jobs: Some(0),
        undo: Some(11),
        ..pages()
    };
    refuses(
        "ReleaseRole before TerminalReady",
        &world.inputs(Change::ReleaseRole { ordinal: 3 }, undo11),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    world.produce(
        "N-304",
        Change::AbortEnd { terminal: 2 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    refuses(
        "RootFree before cursor3",
        &world.inputs(Change::RootFree { ordinal: 3 }, undo11),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    refuses(
        "TerminalFinish while a root remains",
        &world.inputs(
            Change::TerminalFinish { ordinal: 2 },
            Pages {
                jobs: Some(0),
                status: Some(9),
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );

    // h-503 snapshot: statement rollback in progress.
    let mut world = h_501();
    world.lsn = 503;
    world.root = *scope3!("h-503-root.bin");
    world.jobs[1] = Some(*scope3!("h-503-jobs3.bin"));
    for (ordinal, job) in world.bank.iter_mut().enumerate() {
        job.copy_from_slice(
            &scope3!("h-503-bank.bin")[ordinal * JOB_BYTES..(ordinal + 1) * JOB_BYTES],
        );
    }
    World::replace(&mut world.status, 9, scope3!("h-503-status.bin"));
    World::replace(&mut world.undo, 11, scope3!("h-503-undo11.bin"));
    World::replace(&mut world.undo, 14, &page_of(blob("H-undo14-0")));
    world.leaf = Some(*scope3!("h-503-leaf.bin"));
    world.transaction = Some(*scope3!("h-503-tx.bin"));
    world.aging = Some(*scope3!("h-503-aging.bin"));
    let plan = ReservePlan {
        terminal: 14,
        container: page_ref(14, 2),
        table: table(),
        statement: 3,
        canonical: blob("H-505-canonical"),
    };
    refuses(
        "Reserve while a statement rollback is open",
        &world.inputs(
            Change::Reserve(&plan),
            Pages {
                jobs: Some(1),
                status: Some(9),
                undo: Some(14),
                table: true,
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    refuses(
        "Cancel of a compensated root",
        &world.inputs(
            Change::Cancel { ordinal: 16 },
            Pages {
                jobs: Some(1),
                status: Some(9),
                undo: Some(11),
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    refuses(
        "Prune before Commit",
        &world.inputs(
            Change::Prune { ordinal: 15 },
            Pages {
                jobs: Some(1),
                status: Some(9),
                undo: Some(10),
                table: true,
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );
    refuses(
        "TerminalFinish of a leased terminal",
        &world.inputs(
            Change::TerminalFinish { ordinal: 14 },
            Pages {
                jobs: Some(0),
                status: Some(9),
                ..pages()
            },
        ),
        FRAME_BYTES,
        Error::OrderMismatch,
    );

    // h-504 state: H-505's Reserve of row 10 against other RowLeaf proofs
    // (consumers.md:73 table/anchor/leaf proof; :282 space before effects).
    let mut world = h_501();
    h_statement_rollback(&mut world);
    World::replace(&mut world.undo, 14, &page_of(blob("H-undo14-0")));
    let reserve = Pages {
        jobs: Some(1),
        status: Some(9),
        undo: Some(14),
        table: true,
        ..pages()
    };
    let full: &Page = scope3!("noh-300-leaf-full.bin");
    let crowded = crowded_leaf();
    for (label, leaf, expected) in [
        (
            "Reserve without the RowLeaf proof",
            None,
            Error::InvalidInput,
        ),
        (
            "Reserve of a RowId already in the RowLeaf",
            Some(full),
            Error::IdentityMismatch,
        ),
        (
            "Reserve whose cell would split the RowLeaf",
            Some(&*crowded),
            Error::OrderMismatch,
        ),
    ] {
        let mut inputs = world.inputs(Change::Reserve(&plan), reserve);
        inputs.table.as_mut().expect("table").leaf_image = leaf;
        refuses(label, &inputs, FRAME_BYTES, expected);
    }
}

// ---- source bound -----------------------------------------------------------

/// Function frames read from row_record, tree_page, undo_record, the identity
/// codecs they call and the cells.rs material recipe. Each frame sums
/// `size_of` over its arguments, locals, temporaries and both sides of each
/// return boundary while its deepest nested call runs (the retention_page::row
/// `frames` convention); a chain adds the callee chain it enters. Index-key
/// decoding is omitted: RowLeaf pages and Row Undo targets never reach it.
/// These sums check the counted `source` terms; they are not stack samples.
mod measured {
    use super::super::source::returned;
    use super::frames;
    use crate::{
        api::{Column, DataType, DatabaseId, Namespace},
        storage::{
            catalog_record::{CatalogColumn, CatalogError, TableDefinitionView},
            control::Profile,
            overflow::{OVERFLOW_HEAD_BYTES, OverflowError, OverflowHead},
            page::{FileId, PageError, PageHeader, PageKind, PageLsn, PageRef, PageView},
            retention_page::row::JobView,
            row_record::{
                CanonicalRowView, MAX_ROW_COLUMNS, ROW_HEADER_BYTES, RecordExpectation,
                RecordVersion, RowError, RowPayload, RowRecordView, RowSchemaInput, ValueRef,
            },
            semantic_row::{Error, LayoutView, Operation, Shape},
            transaction_record::{LogicalRowVersion, TxnRef, VersionStamp},
            tree_page::{
                self, EntryPayload, SearchPosition, TreeEntry, TreeError, TreeExpectation,
                TreePageView,
                edit::{LeafEdit, LeafEditError, LeafEditPlan},
            },
            undo_record::{
                UNDO_RECORD_BYTES, UndoBeforeKind, UndoCell, UndoEdge, UndoError, UndoLink,
                UndoPayload, UndoPayloadHead, UndoRecord, UndoRecordExpectation, UndoRecordView,
                UndoRelease, UndoTargetView,
            },
            undo_ref::UndoRef,
            wal_frame::{
                FrameEnvelope, FrameExpectation, RedoRecordView, WalError, WalFrameHeader,
            },
        },
        transaction::TxnId,
    };
    use foundation::identity::{CommitSeq, Generation, ObjectHandle, ObjectId, RowId};
    use std::{array, iter::Enumerate, mem::size_of, ops::Range, slice::IterMut, str};

    type R<T, E> = std::result::Result<T, E>;
    type Fe = foundation::Error;

    // ---- identity codecs (page, transaction_record, undo_ref) ----

    fn page_ref_decode() -> usize {
        size_of::<&[u8]>()
            + returned::<PageRef, PageError>()
            + size_of::<R<FileId, PageError>>()
            + returned::<Generation, Fe>()
    }
    fn page_ref_encode() -> usize {
        size_of::<(PageRef, &mut [u8])>() + size_of::<R<(), PageError>>()
    }
    fn txn_id_decode() -> usize {
        size_of::<&[u8]>()
            + size_of::<R<TxnId, PageError>>()
            + size_of::<([u8; 16], [u8; 4], TxnId)>()
            + size_of::<R<DatabaseId, crate::Error>>()
    }
    fn txn_id_encode() -> usize {
        size_of::<(TxnId, &mut [u8])>() + size_of::<R<(), PageError>>() + size_of::<(TxnId, bool)>()
    }
    /// TxnRef::decode: its return and TxnRef::new's result, then decode_txn_id.
    fn txn_ref_decode() -> usize {
        size_of::<&[u8]>()
            + 2 * size_of::<R<TxnRef, PageError>>()
            + returned::<TxnId, PageError>()
            + returned::<Generation, Fe>()
            + txn_id_decode()
    }
    fn txn_ref_encode() -> usize {
        size_of::<(TxnRef, &mut [u8])>()
            + 2 * size_of::<R<(), PageError>>()
            + size_of::<[u8; 48]>()
            + txn_id_encode()
    }
    fn stamp_decode() -> usize {
        size_of::<&[u8]>()
            + size_of::<R<VersionStamp, PageError>>()
            + returned::<CommitSeq, Fe>()
            + returned::<TxnRef, PageError>()
            + txn_ref_decode()
    }
    fn stamp_encode() -> usize {
        size_of::<(VersionStamp, &mut [u8])>()
            + 2 * size_of::<R<(), PageError>>()
            + size_of::<([u8; 56], TxnRef)>()
            + txn_ref_encode()
    }
    fn logical_decode() -> usize {
        size_of::<&[u8]>()
            + 2 * size_of::<R<LogicalRowVersion, PageError>>()
            + returned::<TxnId, PageError>()
            + txn_id_decode()
    }
    fn logical_encode() -> usize {
        size_of::<(LogicalRowVersion, &mut [u8])>()
            + 2 * size_of::<R<(), PageError>>()
            + size_of::<[u8; 48]>()
            + txn_id_encode()
    }
    fn undo_ref_decode() -> usize {
        size_of::<&[u8]>()
            + 2 * size_of::<R<UndoRef, PageError>>()
            + returned::<PageRef, PageError>()
            + size_of::<(u16, [u8; 8])>()
            + returned::<Generation, Fe>()
            + page_ref_decode()
    }
    fn undo_ref_encode() -> usize {
        size_of::<(UndoRef, &mut [u8])>() + 2 * size_of::<R<(), PageError>>() + page_ref_encode()
    }

    // ---- row_record ----

    /// RecordVersion::validate: self, expected, the logical id, the stamp's TxnRef.
    fn rr_validate() -> usize {
        size_of::<(RecordVersion, RecordExpectation)>()
            + size_of::<R<(), RowError>>()
            + size_of::<(TxnId, TxnRef)>()
    }
    /// decode_version while an identity decoder or validate runs.
    fn rr_decode_version() -> usize {
        size_of::<(&[u8], bool, RecordExpectation)>()
            + size_of::<R<RecordVersion, RowError>>()
            + returned::<RowId, Fe>()
            + returned::<VersionStamp, PageError>()
            + returned::<LogicalRowVersion, PageError>()
            + returned::<UndoRef, PageError>()
            + size_of::<(Option<UndoRef>, RecordVersion)>()
            + size_of::<R<(), RowError>>()
            + stamp_decode()
                .max(logical_decode())
                .max(undo_ref_decode())
                .max(rr_validate())
    }
    /// parse_value: input, return, tag, Int64 bytes, length/end, payload, UTF-8.
    fn rr_parse_value() -> usize {
        size_of::<&[u8]>()
            + size_of::<R<(ValueRef<'_>, usize), RowError>>()
            + size_of::<(u8, [u8; 8], usize, usize, &[u8])>()
            + size_of::<ValueRef<'_>>()
            + size_of::<R<&str, str::Utf8Error>>()
    }
    /// RowSchemaInput::column over a Catalog table's column and read_name.
    fn rr_column() -> usize {
        size_of::<(&RowSchemaInput<'_>, usize)>()
            + size_of::<R<(DataType, bool), RowError>>()
            + returned::<CatalogColumn<'_>, CatalogError>()
            + size_of::<(TableDefinitionView<'_>, usize, usize, usize)>()
            + returned::<&str, CatalogError>()
            + returned::<DataType, CatalogError>()
            + size_of::<(
                &[u8],
                R<&str, str::Utf8Error>,
                &str,
                str::Chars<'_>,
                char,
                bool,
            )>()
    }
    /// decode_canonical_row_with_schema while parse_value or a column runs.
    pub(super) fn rr_canonical() -> usize {
        size_of::<(&[u8], RowSchemaInput<'_>)>()
            + size_of::<R<CanonicalRowView<'_>, RowError>>()
            + size_of::<R<(), RowError>>()
            + size_of::<[u32; MAX_ROW_COLUMNS + 1]>()
            + size_of::<[usize; 3]>()
            + returned::<(ValueRef<'_>, usize), RowError>()
            + returned::<(DataType, bool), RowError>()
            + size_of::<CanonicalRowView<'_>>()
            + rr_parse_value().max(rr_column())
    }
    fn overflow_decode() -> usize {
        size_of::<&[u8]>()
            + 2 * size_of::<R<OverflowHead, OverflowError>>()
            + returned::<PageRef, PageError>()
            + size_of::<([u8; 32], OverflowHead)>()
            + page_ref_decode().max(size_of::<(PageRef, usize, [u8; 32])>())
    }
    fn overflow_encode() -> usize {
        size_of::<(OverflowHead, &mut [u8])>()
            + 2 * size_of::<R<(), OverflowError>>()
            + page_ref_encode()
    }
    /// decode_row_record_with_schema while decode_version, the canonical
    /// parse or the overflow head decode runs.
    pub(super) fn rr_decode_record() -> usize {
        size_of::<(&[u8], RecordExpectation, RowSchemaInput<'_>)>()
            + size_of::<R<RowRecordView<'_>, RowError>>()
            + size_of::<R<(), RowError>>()
            + returned::<RecordVersion, RowError>()
            + size_of::<(&[u8], (bool, u8), RowPayload<'_>)>()
            + returned::<CanonicalRowView<'_>, RowError>()
            + returned::<OverflowHead, OverflowError>()
            + size_of::<RowRecordView<'_>>()
            + rr_decode_version()
                .max(rr_canonical())
                .max(overflow_decode())
    }
    fn rr_version_bytes() -> usize {
        size_of::<(RecordVersion, &mut [u8])>()
            + size_of::<R<(), RowError>>()
            + size_of::<R<(), PageError>>()
            + size_of::<UndoRef>()
            + stamp_encode().max(logical_encode()).max(undo_ref_encode())
    }
    /// encode_row_record_with_schema while validate, version_bytes, the
    /// canonical check or the overflow head encoder runs.
    pub(super) fn rr_encode_record() -> usize {
        size_of::<(
            RecordExpectation,
            RowSchemaInput<'_>,
            RecordVersion,
            RowPayload<'_>,
            &mut [u8],
        )>() + size_of::<R<usize, RowError>>()
            + 3 * size_of::<R<(), RowError>>()
            + size_of::<[u8; ROW_HEADER_BYTES]>()
            + size_of::<[u8; OVERFLOW_HEAD_BYTES]>()
            + size_of::<(&[u8], usize, CanonicalRowView<'_>, OverflowHead)>()
            + size_of::<R<CanonicalRowView<'_>, RowError>>()
            + size_of::<R<(), OverflowError>>()
            + rr_validate()
                .max(rr_version_bytes())
                .max(rr_canonical())
                .max(overflow_encode())
    }

    // ---- tree_page ----

    /// (CheckedKey, its Result, Layout, its Result): tree_page's own carriers.
    fn carriers() -> (usize, usize, usize, usize) {
        tree_page::catalog_source_carrier_bytes()
    }
    fn tp_checked_key() -> usize {
        let (_, key_result, _, _) = carriers();
        size_of::<(PageKind, u8, &[u8])>()
            + key_result
            + size_of::<R<[u8; 8], array::TryFromSliceError>>()
            + size_of::<([u8; 8], u64)>()
    }
    fn tp_parse_entry() -> usize {
        let (_, key_result, _, _) = carriers();
        size_of::<(PageKind, u8, &[u8])>()
            + size_of::<R<TreeEntry<'_>, TreeError>>()
            + size_of::<(usize, bool, usize, &[u8])>()
            + size_of::<R<bool, TreeError>>()
            + key_result
            + size_of::<EntryPayload<'_>>()
            + returned::<PageRef, PageError>()
            + tp_checked_key().max(page_ref_decode())
    }
    /// validate_cells with the widest `cell_at` capture (prepare_leaf_edit's
    /// three references) while parse_entry, checked_key or a prior child's
    /// PageRef decode runs; `previous` is charged as a full key Result.
    fn tp_validate_cells() -> usize {
        let (key, key_result, layout, layout_result) = carriers();
        size_of::<(PageHeader, u8, usize, [&(); 3], usize)>()
            + layout_result
            + 3 * size_of::<R<bool, TreeError>>()
            + size_of::<(bool, &[u8], Option<PageRef>)>()
            + returned::<PageRef, PageError>()
            + size_of::<(usize, Option<&[u8]>, &[u8], usize)>()
            + 2 * key_result
            + key
            + returned::<TreeEntry<'_>, TreeError>()
            + 2 * size_of::<Range<usize>>()
            + size_of::<(&[u8], usize, PageRef)>()
            + layout
            + tp_parse_entry()
                .max(tp_checked_key())
                .max(page_ref_decode())
    }
    /// page::decode_page while its digest or the header PageRef decode runs.
    fn page_decode() -> usize {
        frames::decode_page_frame() + frames::content_digest_frame().max(page_ref_decode())
    }
    fn page_encode() -> usize {
        frames::encode_page_frame() + frames::content_digest_frame()
    }
    /// decode_tree_page while decode_page or validate_cells runs.
    pub(super) fn tp_decode() -> usize {
        let (_, _, layout, layout_result) = carriers();
        size_of::<(&[u8], TreeExpectation)>()
            + size_of::<R<TreePageView<'_>, TreeError>>()
            + size_of::<R<bool, TreeError>>()
            + returned::<PageView<'_>, PageError>()
            + layout
            + layout_result
            + size_of::<&PageView<'_>>()
            + size_of::<TreePageView<'_>>()
            + page_decode().max(tp_validate_cells())
    }
    fn tp_entry() -> usize {
        size_of::<(&TreePageView<'_>, usize)>()
            + size_of::<R<TreeEntry<'_>, TreeError>>()
            + size_of::<(usize, Option<&[u8]>, &[u8], PageHeader)>()
            + tp_parse_entry()
    }
    /// lower_bound while entry or checked_key runs.
    pub(super) fn tp_lower_bound() -> usize {
        let (key, key_result, _, _) = carriers();
        size_of::<(&TreePageView<'_>, &[u8])>()
            + size_of::<R<SearchPosition, TreeError>>()
            + 2 * key
            + key_result
            + size_of::<[usize; 3]>()
            + returned::<TreeEntry<'_>, TreeError>()
            + size_of::<(PageHeader, bool)>()
            + tp_entry().max(tp_checked_key())
    }
    pub(super) fn tp_leaf_cell() -> usize {
        let (_, key_result, _, _) = carriers();
        size_of::<(PageKind, u8, &[u8], &[u8], &mut [u8])>()
            + size_of::<R<&[u8], TreeError>>()
            + size_of::<R<bool, TreeError>>()
            + key_result
            + size_of::<(usize, Option<usize>)>()
            + tp_checked_key()
    }
    /// prepare_leaf_edit while parse_entry, lower_bound or validate_cells runs.
    pub(super) fn tp_prepare_leaf_edit() -> usize {
        let (_, _, layout, layout_result) = carriers();
        size_of::<(&TreePageView<'_>, LeafEdit<'_>, &mut [&[u8]])>()
            + size_of::<R<LeafEditPlan<'_, '_>, LeafEditError>>()
            + size_of::<(PageHeader, &[u8], usize, usize)>()
            + size_of::<R<bool, TreeError>>()
            + returned::<TreeEntry<'_>, TreeError>()
            + returned::<SearchPosition, TreeError>()
            + size_of::<[&(); 3]>()
            + layout
            + layout_result
            + size_of::<Enumerate<IterMut<'_, &[u8]>>>()
            + size_of::<(usize, &mut &[u8], Option<&[u8]>)>()
            + size_of::<LeafEditPlan<'_, '_>>()
            + tp_parse_entry()
                .max(tp_lower_bound())
                .max(tp_validate_cells())
    }
    /// LeafEditPlan::encode → encode_tree_page while validate_cells or
    /// encode_page runs.
    pub(super) fn tp_encode() -> usize {
        let (_, _, _, layout_result) = carriers();
        size_of::<(&LeafEditPlan<'_, '_>, PageLsn, &mut [u8])>()
            + 2 * size_of::<R<(), TreeError>>()
            + size_of::<PageHeader>()
            + size_of::<(PageHeader, u8, &[&[u8]], &mut [u8])>()
            + layout_result
            + size_of::<&&[&[u8]]>()
            + size_of::<R<(), PageError>>()
            + page_encode().max(tp_validate_cells())
    }

    // ---- undo_record and the cells.rs material recipe ----

    fn ur_target_validate() -> usize {
        size_of::<UndoTargetView<'_>>() + size_of::<R<(), UndoError>>()
    }
    fn ur_target_parse() -> usize {
        size_of::<(&[u8], &[u8])>()
            + size_of::<R<UndoTargetView<'_>, UndoError>>()
            + size_of::<UndoTargetView<'_>>()
            + size_of::<(R<ObjectId, Fe>, R<Generation, Fe>)>()
            + size_of::<R<(), UndoError>>()
            + ur_target_validate()
    }
    fn ur_expectation_valid() -> usize {
        size_of::<UndoRecordExpectation<'_>>()
            + size_of::<R<(), UndoError>>()
            + ur_target_validate()
    }
    /// validate_record over a Row target while the target check or the Row
    /// before parse runs. Its CatalogValue arm (catalog_expectation and
    /// decode_catalog_before) is unreachable: a Row target refuses that before
    /// kind first (undo_record.rs:534-550).
    fn ur_validate_record() -> usize {
        size_of::<(UndoRecord<'_>, bool, UndoRecordExpectation<'_>, &[Column])>()
            + size_of::<R<usize, UndoError>>()
            + size_of::<R<(), UndoError>>()
            + returned::<TxnRef, PageError>()
            + size_of::<(array::IntoIter<UndoEdge, 2>, UndoEdge, UndoLink)>()
            + size_of::<(UndoRelease, [UndoEdge; 2], UndoEdge, [bool; 4])>()
            + returned::<LogicalRowVersion, UndoError>()
            + returned::<VersionStamp, UndoError>()
            + size_of::<(usize, UndoPayloadHead, usize, Option<PageRef>)>()
            + returned::<CanonicalRowView<'_>, RowError>()
            + ur_target_validate().max(rr_canonical_columns())
    }
    /// decode_canonical_row: the column-schema wrapper over rr_canonical.
    fn rr_canonical_columns() -> usize {
        size_of::<(&[u8], &[Column])>()
            + size_of::<R<CanonicalRowView<'_>, RowError>>()
            + rr_canonical()
    }
    fn ur_encoded_len() -> usize {
        size_of::<(UndoCell<'_>, UndoRecordExpectation<'_>, &[Column])>()
            + size_of::<R<usize, UndoError>>()
            + size_of::<R<(), UndoError>>()
            + size_of::<UndoRecord<'_>>()
            + ur_expectation_valid().max(ur_validate_record())
    }
    fn ur_free_encode() -> usize {
        size_of::<(Generation, &mut [u8])>()
            + size_of::<R<usize, UndoError>>()
            + size_of::<Option<&mut [u8]>>()
    }
    fn ur_head_encode() -> usize {
        size_of::<(UndoPayloadHead, &mut [u8])>()
            + size_of::<R<usize, UndoError>>()
            + size_of::<R<(), PageError>>()
            + page_ref_encode()
    }
    /// encode_undo_record while the length check or a field encoder runs.
    fn ur_encode() -> usize {
        size_of::<(
            UndoCell<'_>,
            UndoRecordExpectation<'_>,
            &[Column],
            &mut [u8],
        )>() + size_of::<R<usize, UndoError>>()
            + 2 * returned::<usize, UndoError>()
            + size_of::<[u8; UNDO_RECORD_BYTES]>()
            + size_of::<(&[u8], &[u8], UndoRecord<'_>)>()
            + size_of::<(
                Enumerate<array::IntoIter<UndoEdge, 2>>,
                usize,
                UndoEdge,
                UndoLink,
            )>()
            + size_of::<(
                usize,
                VersionStamp,
                LogicalRowVersion,
                UndoPayloadHead,
                PageRef,
            )>()
            + 2 * size_of::<R<(), PageError>>()
            + ur_encoded_len()
                .max(txn_id_encode())
                .max(undo_ref_encode())
                .max(stamp_encode())
                .max(logical_encode())
                .max(ur_head_encode())
                .max(page_ref_encode())
                .max(ur_free_encode())
    }
    fn ur_record_generation() -> usize {
        size_of::<(&[u8], Generation)>()
            + returned::<Generation, UndoError>()
            + returned::<Generation, Fe>()
    }
    fn ur_head_decode() -> usize {
        size_of::<&[u8]>()
            + 2 * size_of::<R<UndoPayloadHead, UndoError>>()
            + returned::<PageRef, PageError>()
            + size_of::<([u8; 32], UndoPayloadHead)>()
            + page_ref_decode().max(size_of::<(PageRef, usize, [u8; 32])>())
    }
    fn ur_edge_decode() -> usize {
        size_of::<(u8, &[u8])>()
            + size_of::<R<UndoEdge, UndoError>>()
            + returned::<UndoRef, PageError>()
            + size_of::<UndoLink>()
            + undo_ref_decode()
    }
    /// decode_undo_record: every field result of the record literal, then
    /// validate_record or one field decoder.
    fn ur_decode() -> usize {
        size_of::<(&[u8], UndoRecordExpectation<'_>, &[Column])>()
            + size_of::<R<UndoRecordView<'_>, UndoError>>()
            + 2 * size_of::<R<(), UndoError>>()
            + returned::<Generation, UndoError>()
            + size_of::<UndoCell<'_>>()
            + size_of::<[usize; 2]>()
            + returned::<UndoTargetView<'_>, UndoError>()
            + size_of::<(UndoBeforeKind, UndoPayload<'_>)>()
            + returned::<UndoPayloadHead, UndoError>()
            + size_of::<UndoRecord<'_>>()
            + returned::<TxnId, PageError>()
            + returned::<Generation, Fe>()
            + 2 * returned::<UndoEdge, UndoError>()
            + returned::<VersionStamp, PageError>()
            + returned::<LogicalRowVersion, PageError>()
            + returned::<PageRef, PageError>()
            + size_of::<R<usize, UndoError>>()
            + size_of::<UndoRecordView<'_>>()
            + ur_expectation_valid()
                .max(ur_record_generation())
                .max(ur_target_parse())
                .max(ur_head_decode())
                .max(txn_id_decode())
                .max(ur_edge_decode())
                .max(stamp_decode())
                .max(logical_decode())
                .max(page_ref_decode())
                .max(ur_validate_record())
    }
    /// cells::check_material over encode_material: the canonical cell stays
    /// live while encode_undo_record or decode_undo_record runs.
    pub(super) fn ur_material() -> usize {
        let check = size_of::<(Namespace, JobView<'_>, UndoCell<'_>, &[u8])>()
            + 2 * size_of::<R<(), Error>>()
            + size_of::<[u8; UNDO_RECORD_BYTES]>();
        let encode = size_of::<(
            Namespace,
            JobView<'_>,
            UndoCell<'_>,
            &mut [u8; UNDO_RECORD_BYTES],
        )>() + size_of::<R<(), Error>>()
            + returned::<UndoRef, Error>()
            + returned::<ObjectHandle, Error>()
            + returned::<RowId, Error>()
            + size_of::<UndoRecordExpectation<'_>>()
            + returned::<usize, UndoError>()
            + returned::<UndoRecordView<'_>, UndoError>();
        check + encode + ur_encode().max(ur_decode())
    }

    // ---- framing ----

    /// decode_layout's own frame over a page decode, the envelope finish, a
    /// native page decode or a page encode, without the SHA closure.
    pub(super) fn frame_callee() -> usize {
        let layout_decode = size_of::<(&[u8], FrameExpectation, Profile)>()
            + size_of::<R<LayoutView<'_>, Error>>()
            + size_of::<LayoutView<'_>>()
            + size_of::<R<FrameEnvelope<'_>, WalError>>()
            + size_of::<(Operation, Shape, &[u8], [usize; 5])>()
            + size_of::<R<RedoRecordView<'_>, WalError>>()
            + page_decode();
        let envelope = size_of::<WalFrameHeader>()
            + size_of::<[u8; 160]>()
            + size_of::<R<(), WalError>>()
            + frames::content_digest_frame();
        layout_decode
            .max(envelope)
            .max(page_decode())
            .max(page_encode())
    }
}

/// Whole measured chains above `outer` (every frame live over the phases):
/// a phase's own locals, one SHA closure and one measured callee chain.
fn measured_chains(outer: usize) -> [(&'static str, usize); 15] {
    use super::source;
    use std::mem::size_of;
    let sha = frames::SHA;
    let row_job = frames::job_chains()
        .iter()
        .map(|(_, bytes)| *bytes)
        .max()
        .unwrap();
    let row_root = frames::root_chains()
        .iter()
        .map(|(_, bytes)| *bytes)
        .max()
        .unwrap();
    let prepared = outer + prepare::locals_bytes() + sha;
    let framing = outer + frame::locals_bytes() + source::slack() + sha;
    // finish_aging's aggregate is the backing's; its Job cells, slot and
    // descriptor halves are locals.
    let writing = outer
        + size_of::<(JobCells<'_>, usize, &mut [u8; 1536], &mut [u8; 1536])>()
        + size_of::<[&[u8]; source::ROW_EDIT_CELLS]>()
        + size_of::<retention_obligation::row::AgingTransition<'_>>()
        + size_of::<tree_page::TreePageView<'_>>()
        + sha;
    [
        ("prepare+row_job_census", prepared + row_job),
        ("prepare+row_root_decode", prepared + row_root),
        (
            "prepare+descriptor_codec",
            prepared + obligation::descriptor_source_bytes(),
        ),
        // The ATT spec lives only in these two helper frames, not in prepare's.
        ("prepare+tx_before", prepared + prepare::tx_before_bytes()),
        (
            "prepare+descriptor_after",
            prepared + prepare::descriptor_after_bytes(),
        ),
        (
            "prepare+aging_decode",
            prepared + retention_obligation::row::source_bytes(),
        ),
        ("prepare+row_codec_bound", prepared + row::source_bytes()),
        (
            "prepare+row_record_decode",
            prepared + measured::rr_decode_record(),
        ),
        (
            "prepare+row_record_encode",
            prepared + measured::rr_encode_record(),
        ),
        ("prepare+tree_page_decode", prepared + measured::tp_decode()),
        (
            "prepare+leaf_edit",
            prepared + measured::tp_prepare_leaf_edit(),
        ),
        ("prepare+undo_material", prepared + measured::ur_material()),
        ("frame+callee", framing + measured::frame_callee()),
        (
            "write+aging_next",
            writing + retention_obligation::row::source_bytes().max(row_job),
        ),
        (
            "write+leaf_encode",
            writing + measured::tp_prepare_leaf_edit().max(measured::tp_encode()),
        ),
    ]
}

/// The owned wrapper over the body: local backing, arguments, result.
fn owned_outer() -> usize {
    use std::mem::size_of;
    size_of::<[u8; ROW_REPLAY_SCRATCH_BYTES]>()
        + size_of::<(&Inputs<'_>, &[u8], &mut [Page; 5])>()
        + size_of::<std::result::Result<Observation, Error>>()
}

#[test]
fn n1_transition_source_bounds_cover_measured_frames() {
    use super::source;
    let bound = super::source_bytes();
    let entry = super::entry_bytes();
    let sha = frames::SHA;
    assert_eq!(source::SHA, sha, "pinned SHA closure");
    // Each codec chain, measured function by function, within its counted term.
    let codecs = [
        (
            "row_record_decode",
            measured::rr_decode_record(),
            source::row_codec(),
        ),
        (
            "row_record_encode",
            measured::rr_encode_record(),
            source::row_codec(),
        ),
        (
            "row_record_canonical",
            measured::rr_canonical(),
            source::row_codec(),
        ),
        (
            "tree_page_decode",
            measured::tp_decode(),
            source::tree_codec(),
        ),
        (
            "tree_page_lower_bound",
            measured::tp_lower_bound(),
            source::tree_codec(),
        ),
        (
            "tree_page_leaf_cell",
            measured::tp_leaf_cell(),
            source::tree_codec(),
        ),
        (
            "tree_page_prepare_leaf_edit",
            measured::tp_prepare_leaf_edit(),
            source::leaf_edit(),
        ),
        (
            "tree_page_plan_encode",
            measured::tp_encode(),
            source::leaf_edit(),
        ),
        (
            "undo_record_material",
            measured::ur_material(),
            source::material(),
        ),
        (
            "frame_callee",
            measured::frame_callee(),
            source::frame_callee(),
        ),
    ];
    for (name, chain, counted) in codecs {
        println!("N1_SOURCE_CODEC\t{name}\tmeasured={chain}\tcounted={counted}");
        assert!(
            chain <= counted,
            "{name}: measured {chain} > counted {counted}"
        );
    }
    let chains = measured_chains(owned_outer() + entry);
    for (name, bytes) in chains {
        println!("N1_SOURCE_CHAIN\t{name}\t{bytes}");
        assert!(bound >= bytes, "{name}");
    }
    let measured = chains.iter().map(|(_, bytes)| *bytes).max().unwrap();
    println!(
        "N1_SOURCE bound={bound} measured={measured} outer={} entry={entry} split={} prepare={} locals={} callee={} frame={} write={} slack={} cap=65536 recovered_peak=65512",
        owned_outer(),
        prepare::split_bytes(),
        prepare::source_bytes(),
        prepare::locals_bytes(),
        prepare::callee_bytes(),
        frame::source_bytes(),
        prepare::write_source_bytes(),
        source::slack(),
    );
    println!(
        "N1_SOURCE_TERMS ledger={} tx_before={} descriptor_after={} status={} material={} leaf={} job_edit={} graph={} row_codec={} tree_codec={} leaf_edit={} frame_callee={}",
        source::ledger(),
        prepare::tx_before_bytes(),
        prepare::descriptor_after_bytes(),
        source::status(),
        source::material(),
        source::leaf(),
        source::job_edit(),
        source::graph(),
        source::row_codec(),
        source::tree_codec(),
        source::leaf_edit(),
        source::frame_callee(),
    );
    assert_eq!(
        bound,
        owned_outer()
            + entry
            + prepare::split_bytes()
                .max(prepare::source_bytes())
                .max(frame::source_bytes())
                .max(prepare::write_source_bytes())
    );
    assert!(bound <= 65_536, "native short-source cap");
}

// ---- S1-a: the observer over a lent replay tail ----------------------------

/// Owned observations of N1's produced literals and A's Forward/CLR literals.
const LITERAL_OBSERVATIONS: usize = 35;

/// Run literal histories with every observation repeated through
/// `observe_borrowed`: (the histories' variant count, accepted, refused).
fn with_borrowed(run: impl FnOnce() -> usize) -> (usize, usize, usize) {
    BORROWED.set(Some((0, 0)));
    let checked = run();
    let (accepted, refused) = BORROWED.take().expect("borrowed counts");
    (checked, accepted, refused)
}

#[test]
fn s1a_borrowed_observer_accepts_n1_and_a_literals() {
    let (checked, accepted, refused) = with_borrowed(|| {
        history_e(false) + history_n(false) + history_h(false) + a_literals(false)
    });
    println!("S1A_BORROWED accepted={accepted} refused={refused}");
    assert_eq!((checked, accepted, refused), (0, LITERAL_OBSERVATIONS, 0));
}

#[test]
fn s1a_borrowed_observer_rejects_mutated_literals() {
    let (checked, accepted, refused) =
        with_borrowed(|| a_literals(true) + history_e(true) + history_n(true) + history_h(true));
    println!("S1A_BORROWED_REJECTS checked={checked} accepted={accepted} refused={refused}");
    assert_eq!(
        (checked, accepted, refused),
        (REJECTS.len(), LITERAL_OBSERVATIONS, REJECTS.len())
    );
}

/// The backing parts in `ROW_REPLAY_SCRATCH_BYTES` order.
fn backing_parts() -> [(&'static str, usize); 8] {
    [
        ("descriptors", 2 * 1536),
        ("core", 256),
        ("cell", super::MAX_CELL),
        ("job", JOB_BYTES),
        ("root", crate::storage::retention_page::ROOT_BYTES),
        ("undo", crate::storage::undo_record::UNDO_RECORD_BYTES),
        ("aux", super::graph::AUX_BYTES),
        ("aggregate", super::AGGREGATE_BYTES),
    ]
}

#[test]
fn s1a_borrowed_tail_refused_dirty_and_restored_on_every_exit() {
    let world = noh_300();
    let (change, pages) = forward_301();
    let inputs = world.inputs(change, pages);
    let bytes = scope3!("noh-301-forward.wal");
    let mut owned_after = loans(0);
    let observed = observe(&inputs, bytes, &mut owned_after).unwrap();

    // A nonzero byte at either end of any part: refused before any work,
    // the lent tail and the after loans kept as they were.
    let mut start = 0;
    for (part, length) in backing_parts() {
        for at in [start, start + length - 1] {
            let mut lane = new_lane();
            lane[32 + at] = 0x5a;
            let mut after = loans(0xa5);
            assert_eq!(
                observe_borrowed(&inputs, bytes, &mut after, lent(&mut lane)).map(|_| ()),
                Err(Error::InvalidInput),
                "{part}@{at}"
            );
            assert_eq!(lane[32 + at], 0x5a, "{part}@{at} kept");
            lane[32 + at] = 0;
            restored(part, &lane);
            assert!(after.iter().flatten().all(|byte| *byte == 0xa5), "{part}");
            println!("S1A_DIRTY_REFUSED\t{part}\t{at}");
        }
        start += length;
    }
    assert_eq!(start, ROW_REPLAY_SCRATCH_BYTES);

    // Success: the same observation and after loans, the tail zero again.
    let mut lane = new_lane();
    let mut after = loans(0);
    let borrowed = observe_borrowed(&inputs, bytes, &mut after, lent(&mut lane)).unwrap();
    assert_eq!(borrowed, observed);
    assert!(*after == *owned_after);
    restored("success", &lane);

    // An error inside prepare, after the census and Job copy were written.
    let late = Change::Forward {
        ordinal: 3,
        statement: boundary(2, None),
    };
    let inputs = world.inputs(late, pages);
    let mut owned_after = loans(0);
    let refused = observe(&inputs, bytes, &mut owned_after);
    assert_eq!(refused.map(|_| ()), Err(Error::OrderMismatch));
    let mut lane = new_lane();
    let mut after = loans(0);
    assert_eq!(
        observe_borrowed(&inputs, bytes, &mut after, lent(&mut lane)),
        refused
    );
    assert!(*after == *owned_after);
    restored("prepare error", &lane);

    // A forced unwind while the loan holds the real observation's values.
    let inputs = world.inputs(change, pages);
    let mut lane = new_lane();
    let mut dirty = Vec::new();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let loan = ReplayLoan::new(lent(&mut lane)).unwrap();
        let mut after = loans(0);
        assert_eq!(
            observe_with(&inputs, bytes, &mut after, &mut *loan.0).unwrap(),
            observed
        );
        let mut at = 0;
        for (part, length) in backing_parts() {
            if loan.0[at..at + length].iter().any(|byte| *byte != 0) {
                dirty.push(part);
            }
            at += length;
        }
        panic!("forced unwind with the Row replay loan");
    }));
    assert!(unwound.is_err());
    restored("unwind", &lane);
    println!("S1A_UNWIND dirty={dirty:?}");
    // Forward writes every part but the Prune/Reserve cell.
    assert_eq!(
        dirty,
        [
            "descriptors",
            "core",
            "job",
            "root",
            "undo",
            "aux",
            "aggregate"
        ]
    );
}

/// S1-a target, a dated reference rather than a live value. These history
/// phases were recorded by C2 on base b304bea (original /root history, absent
/// from this repository); 8c0f252, this repository's parentless import of
/// that line, already includes later C2 rounds (survey:1715-1850, e.g.
/// c2-compact-dpt's Plan4 population change), so they predate every commit
/// here, including this base 5135e16
/// (module-independence-survey-20261026.md#c2-borrowed-replay-workspace:
/// retention 24715B, population 25743B, initialize 26120B); semantic_catalog
/// resources/transition and plan4_retention recovery.rs/population.rs changed
/// since. `initialize` sits beside `frame` in history `file_scope`
/// (history.rs:129-130), not inside it; of the `frame` arms (:124-128) C2
/// recorded, population is the largest. The assert takes that stricter
/// frame-phase reading. S1-b derives the live target from
/// `HistorySourceCensus` instead.
const FRAME_PHASE_MAX: usize = 25_743;
/// The looser reading: history `initialize` at the same measurement.
const INITIALIZE_MAX: usize = 26_120;

#[test]
fn s1a_borrowed_observer_source_bound_covers_measured_frames() {
    use super::source;
    use std::mem::size_of;
    let bound = super::observe_borrowed_source_bytes();
    let entry = super::entry_bytes();
    let wrapper = size_of::<(
        &Inputs<'_>,
        &[u8],
        &mut [Page; 5],
        &mut [u8; ROW_REPLAY_SCRATCH_BYTES],
    )>() + source::returned::<ReplayLoan<'_>, Error>()
        + size_of::<std::result::Result<Observation, Error>>()
        + size_of::<(&[u8], std::slice::Iter<'_, u8>, bool)>();
    let phases = [
        ("split", prepare::split_bytes()),
        ("prepare", prepare::source_bytes()),
        ("frame", frame::source_bytes()),
        ("write", prepare::write_source_bytes()),
    ];
    assert_eq!(
        bound,
        wrapper + entry + phases.iter().map(|(_, bytes)| *bytes).max().unwrap()
    );
    // The Row-target Undo material chain stays within its counted term.
    let material = measured::ur_material();
    println!(
        "S1A_SOURCE_CODEC\tundo_record_material_row\tmeasured={material}\tcounted={}",
        source::material()
    );
    assert!(material <= source::material());
    let chains = measured_chains(wrapper + entry);
    for (name, bytes) in chains {
        println!("S1A_SOURCE_CHAIN\t{name}\t{bytes}");
        assert!(bound >= bytes, "{name}");
    }
    let measured = chains.iter().map(|(_, bytes)| *bytes).max().unwrap();
    let callees = [
        ("ledger", source::ledger()),
        ("tx_before", prepare::tx_before_bytes()),
        ("descriptor_after", prepare::descriptor_after_bytes()),
        (
            "aging",
            crate::storage::retention_obligation::row::source_bytes(),
        ),
        ("material", source::material()),
        ("leaf", source::leaf()),
        ("row_census", row::source_bytes()),
        ("status", source::status()),
        ("job_edit", source::job_edit()),
        ("graph", source::graph()),
    ];
    let dominant = callees.iter().max_by_key(|(_, bytes)| *bytes).unwrap();
    assert_eq!(dominant.1, prepare::callee_bytes());
    println!(
        "S1A_SOURCE borrowed={bound} measured={measured} owned={} wrapper={wrapper} entry={entry} prepared={} locals={} callee={} dominant={} sha={} slack={} scratch={ROW_REPLAY_SCRATCH_BYTES} tail={} target={FRAME_PHASE_MAX} room={} initialize={INITIALIZE_MAX} initialize_room={}",
        super::source_bytes(),
        size_of::<super::Prepared<'_, '_>>(),
        prepare::locals_bytes(),
        prepare::callee_bytes(),
        dominant.0,
        source::SHA,
        source::slack(),
        2 * PAGE_BYTES - 64 * 352,
        FRAME_PHASE_MAX as i64 - bound as i64,
        INITIALIZE_MAX as i64 - bound as i64,
    );
    for (name, bytes) in phases {
        println!("S1A_SOURCE_PHASE\t{name}\t{bytes}");
    }
    for (name, bytes) in callees {
        println!("S1A_SOURCE_CALLEE\t{name}\t{bytes}");
    }
    assert!(ROW_REPLAY_SCRATCH_BYTES <= 2 * PAGE_BYTES - 64 * 352);
    assert!(
        bound <= FRAME_PHASE_MAX,
        "borrowed Row observer above the frame-phase max"
    );
}
