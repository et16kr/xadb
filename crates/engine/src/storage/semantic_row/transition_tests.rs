//! N1: C-owned scope3 Row group producers and the whole-group observer against
//! independent `module-row-format/transition/author.py` literals and A's
//! MCA00-V06 Forward/CLR literals. Literal observations confer no actual
//! source, reservation, pin, staging, installation or recovery authority.

use super::{
    Change, Inputs, Issued, JOB, LEAF, Observation, ROOT, ReservePlan, STATUS, TableSource, UNDO,
    frame, observe, prepare, produce,
};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        control::{Profile, row_profile},
        obligation,
        page::{DIGEST_OFFSET, FileId, PAGE_BYTES, PageRef},
        retention_genesis::{self, RetentionGenesisPage},
        retention_obligation,
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self, JobScope, source_tests as frames},
        },
        row_record::RowSchemaInput,
        semantic_row::{Error, FRAME_BYTES, Operation, decode_layout, status::StatementBoundary},
        transaction_record::{StatusSlotRef, TxnRef},
        undo_ref::UndoRef,
        wal_frame::{self, FrameExpectation, WalStreamIdentity},
    },
    transaction::TxnId,
};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch},
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
const REJECTED_VARIANTS: usize = 19;

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
            let mut after = loans(0);
            let error = observe(inputs, bytes, &mut after)
                .map(|_| ())
                .expect_err(variant);
            println!("N1_REJECT\t{variant}\t{error:?}");
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
            for &role in &observation.order[..usize::from(observation.count)] {
                same(name, &again[usize::from(role)], &after[usize::from(role)]);
            }
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
            // A's operations stay unsupported on the producer side.
            let mut frame = vec![0xa5; FRAME_BYTES];
            let mut untouched = loans(0xa5);
            assert_eq!(
                produce(&inputs, &mut untouched, &mut frame).map(|_| ()),
                Err(Error::Unsupported),
                "{name}"
            );
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

fn history_h(rejects: bool) -> usize {
    let mut world = h_501();
    world.rejects = rejects.then(|| Cell::new(0));
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

fn a_literals(rejects: bool) -> usize {
    let mut checked = 0;
    // noh Forward301 from noh-300.
    let mut world = World::from_a(
        300,
        scope3!("noh-300-root.bin"),
        &[(0, scope3!("noh-300-jobs2.bin"))],
        scope3!("noh-300-bank.bin"),
        scope3!("noh-300-status.bin"),
        &[(11, scope3!("noh-300-undo11.bin"))],
        scope3!("noh-300-leaf.bin"),
        scope3!("noh-300-tx.bin"),
        scope3!("noh-300-aging.bin"),
    );
    world.rejects = rejects.then(|| Cell::new(0));
    let observed = world.observe(
        "A-noh-301",
        scope3!("noh-301-forward.wal"),
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
    );
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
    assert_eq!(checked, REJECTED_VARIANTS);
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
}

// ---- source bound -----------------------------------------------------------

#[test]
fn n1_transition_source_bounds_cover_measured_frames() {
    use super::super::{LayoutView, Shape};
    use std::mem::size_of;
    let bound = super::source_bytes();
    let entry = super::entry_bytes();
    let sha = frames::SHA;
    let digest = frames::content_digest_frame() + sha;
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
    // decode_layout: arguments, returned views, envelope and redo walk with
    // Init validation's nested page view, and the frame digest.
    let layout_decode = size_of::<(&[u8], FrameExpectation, Profile)>()
        + size_of::<Result<LayoutView<'_>, Error>>()
        + size_of::<LayoutView<'_>>()
        + size_of::<Result<wal_frame::FrameEnvelope<'_>, wal_frame::WalError>>()
        + size_of::<(Operation, Shape, &[u8], [usize; 5])>()
        + size_of::<Result<wal_frame::RedoRecordView<'_>, wal_frame::WalError>>()
        + frames::decode_page_frame()
        + digest;
    let envelope = size_of::<wal_frame::WalFrameHeader>()
        + 160
        + size_of::<Result<(), wal_frame::WalError>>()
        + digest;
    let page_decode = frames::decode_page_frame() + digest;
    let page_encode = frames::encode_page_frame() + digest;
    let callee = [layout_decode, envelope, page_decode, page_encode]
        .into_iter()
        .max()
        .unwrap();
    let prepared = entry + prepare::locals_bytes() + sha;
    let framing = entry + frame::locals_bytes();
    let chains = [
        ("prepare+row_job_census", prepared + row_job),
        ("prepare+row_root_decode", prepared + row_root),
        (
            "prepare+descriptor_codec",
            prepared + obligation::descriptor_source_bytes(),
        ),
        (
            "prepare+aging_decode",
            prepared + retention_obligation::row::source_bytes(),
        ),
        ("prepare+row_codec_bound", prepared + row::source_bytes()),
        ("frame+layout_decode", framing + layout_decode),
        ("frame+envelope_finish", framing + envelope),
        ("frame+page_decode", framing + page_decode),
        ("frame+page_encode", framing + page_encode),
        (
            "write+aging_next",
            entry
                + 640
                + size_of::<retention_obligation::row::AgingTransition<'_>>()
                + retention_obligation::row::source_bytes().max(row_job),
        ),
    ];
    for (name, bytes) in chains {
        println!("N1_SOURCE_CHAIN\t{name}\t{bytes}");
        assert!(bound >= bytes, "{name}");
    }
    assert!(callee <= frame::CALLEE_BYTES, "framing callee {callee}");
    let measured = chains.iter().map(|(_, bytes)| *bytes).max().unwrap();
    println!(
        "N1_SOURCE bound={bound} measured={measured} entry={entry} prepare={} locals={} callee={} frame={} frame_callee_measured={callee} write={} cap=65536 recovered_peak=65512",
        prepare::source_bytes(),
        prepare::locals_bytes(),
        prepare::callee_bytes(),
        frame::source_bytes(),
        prepare::write_source_bytes()
    );
    assert_eq!(
        bound,
        entry
            + prepare::source_bytes()
                .max(frame::source_bytes())
                .max(prepare::write_source_bytes())
    );
    assert!(bound <= 65_536, "native short-source cap");
}
