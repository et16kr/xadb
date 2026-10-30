//! C0_ONLY MCA00-V06: complete scope3 Row INSERT Forward/CLR candidates, H
//! absent and present, and a Forward whose physical page order differs from
//! the role order, against independent make_literals.py bytes. Literal
//! observations confer no actual source, reservation, pin or apply rights.

use super::{
    InsertError,
    group::{self, Change, FRAME_BYTES, GroupInput, Source},
};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        allocation_page::ReservationOwner,
        control::{Profile, row_profile},
        disk::RowPageRole,
        obligation,
        page::{FileId, PAGE_BYTES, PageIdentity, PageKind, PageRef},
        retention_obligation::{self, AgingObligationError, profile_reference},
        retention_page::{JOB_BYTES, JOB_COUNT, row::JobCells},
        row_record::RowSchemaInput,
        semantic_row::{self, jobs, ledger, status, undo},
        status_home::StatusHome,
        transaction_record::{StatusSlotRef, TxnRef},
        tree_page::{TreeExpectation, edit::LeafEditError},
        undo_record::UndoLink,
        undo_ref::UndoRef,
        wal_frame::{FrameExpectation, FrameKind, WalFrameHeader, WalStreamIdentity},
    },
    transaction::TxnId,
};
use foundation::identity::{Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch};

macro_rules! scope3 {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-insert/scope3/",
            $name
        ))
    };
}

type Page = &'static [u8; PAGE_BYTES];
type Descriptor = &'static [u8; 1536];
type Bank = &'static [u8; JOB_BYTES * JOB_COUNT];

const PROFILE: &[u8; 4096] = scope3!("profile3.bin");
const ANCHOR: Page = scope3!("anchor.bin");
/// Canonical (Int64 42, Utf8 "xadb"): L22, so the RowLeaf cell C is 202.
const CANONICAL: [u8; 22] = [
    2, 0, 0, 0, 2, 42, 0, 0, 0, 0, 0, 0, 0, 3, 4, 0, 0, 0, b'x', b'a', b'd', b'b',
];
const ROLES: [RowPageRole; 5] = [
    RowPageRole::Root,
    RowPageRole::RootJob,
    RowPageRole::Status,
    RowPageRole::Undo,
    RowPageRole::RowLeaf,
];
/// ForwardOrd: U on page14 follows the RowLeaf page13 in physical order.
const ORD_ROLES: [RowPageRole; 5] = [
    RowPageRole::Root,
    RowPageRole::RootJob,
    RowPageRole::Status,
    RowPageRole::RowLeaf,
    RowPageRole::Undo,
];
const ROOT_CELL: (u16, u16, u16) = (0, 15_840, 512);
const STATUS_CELL: (u16, u16, u16) = (0, 16_128, 224);
const UNDO_CELL: (u16, u16, u16) = (0, 15_920, 432);
const ANCHOR_CELL: (u16, u16, u16) = (0, 16_304, 48);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    ForwardNoH,
    ForwardH,
    ClrNoH,
    ClrH,
    ForwardOrd,
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
fn page_ref(page_no: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0001).unwrap(),
        page_no,
        incarnation: generation(incarnation),
    }
}
fn identity(page_no: u64, incarnation: u64, owner: u64) -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: page_ref(page_no, incarnation),
        owner: ObjectId::new(owner).unwrap(),
        owner_generation: generation(1),
    }
}
fn actor() -> TxnRef {
    let id = TxnId {
        database: namespace().database,
        timeline: 1,
        writer_epoch: 1,
        local: 5,
    };
    TxnRef::new(id, generation(2)).unwrap()
}
fn location() -> StatusSlotRef {
    StatusSlotRef::new(page_ref(9, 2), 0, generation(2)).unwrap()
}
fn undo_ref(page_no: u64, slot_generation: u64) -> UndoRef {
    UndoRef::new(page_ref(page_no, 2), 0, generation(slot_generation)).unwrap()
}
fn h_link() -> UndoLink {
    UndoLink {
        reference: undo_ref(10, 2),
        sequence: 1,
    }
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
fn profile() -> Profile {
    row_profile::decode_profile(PROFILE, None).unwrap()
}
fn leaf_expected() -> TreeExpectation {
    TreeExpectation {
        identity: identity(13, 2, 6),
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    }
}
fn job_cell(ordinal: usize) -> (u16, u16, u16) {
    let slot = ordinal % 15;
    (slot as u16, (16_352 - 1024 * (slot + 1)) as u16, 1024)
}
/// Literal directory scan of a RowLeaf: (slot, offset, length) of `row`.
fn row_cell(page: &[u8; PAGE_BYTES], row: u64) -> (u16, u16, u16) {
    let count = u16::from_le_bytes([page[72], page[73]]);
    for slot in 1..count {
        let at = 128 + 4 * usize::from(slot);
        let offset = u16::from_le_bytes([page[at], page[at + 1]]);
        let length = u16::from_le_bytes([page[at + 2], page[at + 3]]);
        let start = usize::from(offset);
        if page[start + 4..start + 12] == row.to_be_bytes() {
            return (slot, offset, length);
        }
    }
    panic!("literal row {row} is absent");
}

/// Input observations of one case plus its expected literal outputs.
struct Fixture {
    case: Case,
    before: [[u8; PAGE_BYTES]; 9],
    count: usize,
    sources: [Option<Source>; 9],
    bank: [[u8; JOB_BYTES]; JOB_COUNT],
    transaction: [u8; 1536],
    aging: [u8; 1536],
    columns: [Column; 2],
    header: WalFrameHeader,
    ordinal: usize,
    terminal: usize,
    undo: UndoLink,
    predecessor: Option<UndoLink>,
    statement: Option<u64>,
    mutation: u64,
    mark: Option<UndoRef>,
    canonical: Vec<u8>,
    cell_bytes: usize,
    projection: [u8; 32],
    target: Option<UndoRef>,
    next: Option<UndoLink>,
    after_order: [RowPageRole; 5],
    frame: &'static [u8],
    after: [Page; 5],
    transaction_after: Descriptor,
    aging_after: Descriptor,
}

impl Fixture {
    #[allow(clippy::type_complexity, reason = "one test observation tuple")]
    fn new(case: Case) -> Self {
        let h = matches!(case, Case::ForwardH | Case::ClrH);
        let forward = matches!(case, Case::ForwardNoH | Case::ForwardH | Case::ForwardOrd);
        let u_page = if case == Case::ForwardOrd { 14 } else { 11 };
        #[rustfmt::skip]
        let (pages, bank, transaction, aging, frame, after, transaction_after, aging_after): (
            [Page; 6], Bank, Descriptor, Descriptor, &'static [u8], [Page; 5], Descriptor, Descriptor,
        ) = match case {
            Case::ForwardNoH => (
                [scope3!("noh-300-root.bin"), scope3!("noh-300-jobs2.bin"), scope3!("noh-300-jobs2.bin"),
                 scope3!("noh-300-status.bin"), scope3!("noh-300-undo11.bin"), scope3!("noh-300-leaf.bin")],
                scope3!("noh-300-bank.bin"), scope3!("noh-300-tx.bin"), scope3!("noh-300-aging.bin"),
                scope3!("noh-301-forward.wal"),
                [scope3!("noh-301-root.bin"), scope3!("noh-301-jobs2.bin"), scope3!("noh-301-status.bin"),
                 scope3!("noh-301-undo11.bin"), scope3!("noh-301-leaf.bin")],
                scope3!("noh-301-tx.bin"), scope3!("noh-301-aging.bin"),
            ),
            Case::ClrNoH => (
                [scope3!("noh-301-root.bin"), scope3!("noh-301-jobs2.bin"), scope3!("noh-301-jobs2.bin"),
                 scope3!("noh-302-status.bin"), scope3!("noh-301-undo11.bin"), scope3!("noh-301-leaf.bin")],
                scope3!("noh-301-bank.bin"), scope3!("noh-302-tx.bin"), scope3!("noh-301-aging.bin"),
                scope3!("noh-303-clr.wal"),
                [scope3!("noh-303-root.bin"), scope3!("noh-303-jobs2.bin"), scope3!("noh-303-status.bin"),
                 scope3!("noh-303-undo11.bin"), scope3!("noh-303-leaf.bin")],
                scope3!("noh-303-tx.bin"), scope3!("noh-303-aging.bin"),
            ),
            Case::ForwardH => (
                [scope3!("h-500-root.bin"), scope3!("h-500-jobs3.bin"), scope3!("h-500-jobs2.bin"),
                 scope3!("h-500-status.bin"), scope3!("h-500-undo11.bin"), scope3!("h-500-leaf.bin")],
                scope3!("h-500-bank.bin"), scope3!("h-500-tx.bin"), scope3!("h-500-aging.bin"),
                scope3!("h-501-forward.wal"),
                [scope3!("h-501-root.bin"), scope3!("h-501-jobs3.bin"), scope3!("h-501-status.bin"),
                 scope3!("h-501-undo11.bin"), scope3!("h-501-leaf.bin")],
                scope3!("h-501-tx.bin"), scope3!("h-501-aging.bin"),
            ),
            Case::ClrH => (
                [scope3!("h-501-root.bin"), scope3!("h-501-jobs3.bin"), scope3!("h-501-jobs2.bin"),
                 scope3!("h-502-status.bin"), scope3!("h-501-undo11.bin"), scope3!("h-501-leaf.bin")],
                scope3!("h-501-bank.bin"), scope3!("h-502-tx.bin"), scope3!("h-501-aging.bin"),
                scope3!("h-503-clr.wal"),
                [scope3!("h-503-root.bin"), scope3!("h-503-jobs3.bin"), scope3!("h-503-status.bin"),
                 scope3!("h-503-undo11.bin"), scope3!("h-503-leaf.bin")],
                scope3!("h-503-tx.bin"), scope3!("h-503-aging.bin"),
            ),
            // The expected after images are in physical order: Leaf, then U.
            Case::ForwardOrd => (
                [scope3!("ord-300-root.bin"), scope3!("ord-300-jobs2.bin"), scope3!("ord-300-jobs2.bin"),
                 scope3!("ord-300-status.bin"), scope3!("ord-300-undo14.bin"), scope3!("ord-300-leaf.bin")],
                scope3!("ord-300-bank.bin"), scope3!("ord-300-tx.bin"), scope3!("ord-300-aging.bin"),
                scope3!("ord-301-forward.wal"),
                [scope3!("ord-301-root.bin"), scope3!("ord-301-jobs2.bin"), scope3!("ord-301-status.bin"),
                 scope3!("ord-301-leaf.bin"), scope3!("ord-301-undo14.bin")],
                scope3!("ord-301-tx.bin"), scope3!("ord-301-aging.bin"),
            ),
        };
        // H absent: terminal2 and U3 share Jobs page2. H present: terminal14
        // is on page2, U16 on page3, and H is the unchanged Undo page10.
        let (ordinal, terminal, sequence, lsn) = match case {
            Case::ForwardNoH | Case::ForwardOrd => (3, 2, 1, 301),
            Case::ClrNoH => (3, 2, 1, 303),
            Case::ForwardH => (16, 14, 2, 501),
            Case::ClrH => (16, 14, 2, 503),
        };
        let mut before = [[0; PAGE_BYTES]; 9];
        let images: Vec<Page> = match (h, pages[1] == pages[2]) {
            (false, true) => vec![pages[0], pages[1], pages[3], pages[4], pages[5], ANCHOR],
            (true, false) => vec![
                pages[0],
                pages[1],
                pages[2],
                pages[3],
                pages[4],
                pages[5],
                ANCHOR,
                scope3!("h-500-undo10.bin"),
            ],
            _ => unreachable!("closed literal layout"),
        };
        for (slot, image) in before.iter_mut().zip(&images) {
            *slot = **image;
        }
        let at = |index: usize| index - usize::from(!h && index > 1);
        let source = |index: usize, identity, kind, cell| {
            Some(Source {
                index: at(index),
                identity,
                kind,
                level: 0,
                cell,
            })
        };
        let leaf_cell = (!forward).then(|| row_cell(&before[at(5)], 7));
        let sources = [
            source(
                0,
                identity(1, 1, 4),
                PageKind::RetentionRoot,
                Some(ROOT_CELL),
            ),
            source(
                1,
                identity(2 + ordinal as u64 / 15, 1, 4),
                PageKind::RetentionJobs,
                Some(job_cell(ordinal)),
            ),
            source(
                if h { 2 } else { 1 },
                identity(2 + terminal as u64 / 15, 1, 4),
                PageKind::RetentionJobs,
                Some(job_cell(terminal)),
            ),
            source(
                3,
                identity(9, 2, 2),
                PageKind::TransactionStatus,
                Some(STATUS_CELL),
            ),
            source(4, identity(u_page, 2, 3), PageKind::Undo, Some(UNDO_CELL)),
            source(5, identity(13, 2, 6), PageKind::RowLeaf, leaf_cell),
            None,
            source(
                6,
                identity(12, 2, 6),
                PageKind::TreeAnchor,
                Some(ANCHOR_CELL),
            ),
            h.then(|| Source {
                index: 7,
                identity: identity(10, 2, 3),
                kind: PageKind::Undo,
                level: 0,
                cell: Some(UNDO_CELL),
            }),
        ];
        let mut cells = [[0; JOB_BYTES]; JOB_COUNT];
        for (cell, bytes) in cells.iter_mut().zip(bank.chunks_exact(JOB_BYTES)) {
            cell.copy_from_slice(bytes);
        }
        let projection = cells[ordinal][288..320].try_into().unwrap();
        let header = WalFrameHeader {
            stream: WalStreamIdentity {
                database: namespace().database,
                timeline: 1,
                writer_epoch: 1,
            },
            kind: if forward {
                FrameKind::Update
            } else {
                FrameKind::Clr
            },
            lsn,
            previous_lsn: lsn - 1,
            txn: Some(actor().id()),
            previous_txn_lsn: u64::from_le_bytes(transaction[72..80].try_into().unwrap()),
        };
        Self {
            case,
            before,
            count: images.len(),
            sources,
            bank: cells,
            transaction: *transaction,
            aging: *aging,
            columns: columns(),
            header,
            ordinal,
            terminal,
            undo: UndoLink {
                reference: undo_ref(u_page, 3),
                sequence,
            },
            predecessor: h.then(h_link),
            // Full transaction rollback (H absent) has no live statement.
            statement: (forward || h).then_some(sequence),
            mutation: sequence,
            mark: h.then(|| h_link().reference),
            canonical: CANONICAL.to_vec(),
            cell_bytes: 202,
            projection,
            target: (h && !forward).then(|| h_link().reference),
            next: (h && !forward).then(h_link),
            after_order: if case == Case::ForwardOrd {
                ORD_ROLES
            } else {
                ROLES
            },
            frame,
            after,
            transaction_after,
            aging_after,
        }
    }

    fn forward(&self) -> bool {
        matches!(
            self.case,
            Case::ForwardNoH | Case::ForwardH | Case::ForwardOrd
        )
    }

    /// Index of a role in the physical after order of this case.
    fn after_index(&self, role: RowPageRole) -> usize {
        self.after_order
            .iter()
            .position(|candidate| *candidate == role)
            .unwrap()
    }

    fn input(&self) -> GroupInput<'_> {
        GroupInput {
            profile: profile(),
            table: table(),
            schema: RowSchemaInput::Columns(&self.columns),
            leaf: leaf_expected(),
            expected: FrameExpectation {
                stream: self.header.stream,
                previous_lsn: self.header.previous_lsn,
            },
            header: self.header,
            actor: actor(),
            status: location(),
            row: RowId::new(7).unwrap(),
            undo: self.undo,
            statement: self.statement,
            mutation: self.mutation,
            mark: self.mark,
            predecessor: self.predecessor,
            root_job: self.ordinal,
            terminal_job: self.terminal,
            sources: self.sources,
            before: &self.before,
            before_count: self.count,
            after_order: &self.after_order,
            jobs: &self.bank,
            transaction: Some(&self.transaction),
            aging: Some(&self.aging),
            change: if self.forward() {
                Change::Forward {
                    canonical: &self.canonical,
                }
            } else {
                Change::Clr {
                    cell_bytes: self.cell_bytes,
                    projection: self.projection,
                    target: self.target,
                    next: self.next,
                }
            },
        }
    }

    fn produce(
        &self,
        frame: &mut [u8; FRAME_BYTES],
        after: &mut [[u8; PAGE_BYTES]; 5],
    ) -> Result<group::Produced, InsertError> {
        group::produce(&self.input(), frame, after, &mut scratch())
    }
}

/// The A scratch lane part, filled with a non-zero pattern: a production run
/// must not depend on what the lane held before.
fn scratch() -> Box<[u8; group::SCRATCH_BYTES]> {
    vec![0x5a; group::SCRATCH_BYTES]
        .into_boxed_slice()
        .try_into()
        .expect("scratch length")
}

/// Byte equality reporting only the first difference, so a failing log
/// stays small instead of printing two 90 KiB arrays.
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

fn literal_case(case: Case) {
    let fixture = Fixture::new(case);
    let mut frame = [0xa5; FRAME_BYTES];
    let mut after = [[0xa5; PAGE_BYTES]; 5];
    let produced = fixture.produce(&mut frame, &mut after).unwrap();
    assert_eq!(produced.frame_len, fixture.frame.len());
    assert_eq!(produced.after_count, 5);
    assert_eq!(produced.roles, fixture.after_order);
    let h = 128 * usize::from(fixture.predecessor.is_some());
    let fixed = if fixture.forward() { 91_304 } else { 91_096 };
    assert_eq!(produced.frame_len, fixed + 202 + h);
    for (index, expected) in fixture.after.iter().enumerate() {
        same(
            &format!("{case:?} after[{index}]"),
            &after[index],
            *expected,
        );
    }
    same(
        &format!("{case:?} Tx descriptor after"),
        &frame[416..1952],
        fixture.transaction_after,
    );
    same(
        &format!("{case:?} Aging descriptor after"),
        &frame[1952..3488],
        fixture.aging_after,
    );
    same(
        &format!("{case:?} frame"),
        &frame[..produced.frame_len],
        fixture.frame,
    );
}

#[test]
fn mca00_v06_forward_without_h_matches_literals() {
    literal_case(Case::ForwardNoH);
}

#[test]
fn mca00_v06_forward_with_h_matches_literals() {
    literal_case(Case::ForwardH);
}

#[test]
fn mca00_v06_clr_without_h_matches_literals() {
    literal_case(Case::ClrNoH);
}

#[test]
fn mca00_v06_clr_with_h_matches_literals() {
    literal_case(Case::ClrH);
}

/// Every refusal below is an input check, so no frame/after byte may change.
fn refuses(case: Case, error: InsertError, change: impl FnOnce(&mut Fixture)) {
    let mut fixture = Fixture::new(case);
    change(&mut fixture);
    let mut frame = [0xa5; FRAME_BYTES];
    let mut after = [[0xa5; PAGE_BYTES]; 5];
    assert_eq!(
        fixture.produce(&mut frame, &mut after),
        Err(error),
        "{case:?}"
    );
    assert!(frame.iter().all(|byte| *byte == 0xa5), "{case:?} frame");
    assert!(
        after.iter().flatten().all(|byte| *byte == 0xa5),
        "{case:?} after"
    );
}

#[test]
fn mca00_v06_forward_refusals_precede_output() {
    use InsertError::{IdentityMismatch, InvalidInput, OrderMismatch};
    // Stale Tx chain, wrong outer kind and a non-reserved canonical value.
    refuses(Case::ForwardNoH, IdentityMismatch, |f| {
        f.header.previous_txn_lsn -= 1
    });
    refuses(Case::ForwardNoH, IdentityMismatch, |f| {
        f.header.kind = FrameKind::Clr
    });
    refuses(Case::ForwardNoH, IdentityMismatch, |f| {
        f.canonical.truncate(21)
    });
    // The census-derived H must equal the lane's predecessor and h recipe.
    refuses(Case::ForwardH, IdentityMismatch, |f| f.predecessor = None);
    refuses(Case::ForwardNoH, IdentityMismatch, |f| {
        f.predecessor = Some(h_link())
    });
    refuses(Case::ForwardNoH, InvalidInput, |f| {
        f.sources[8] = f.sources[4]
    });
    // A mark must name an older retained root of this actor.
    refuses(Case::ForwardH, OrderMismatch, |f| {
        f.mark = Some(f.undo.reference)
    });
    // Forward needs the live statement; it must equal the reserve-sealed one.
    refuses(Case::ForwardNoH, InvalidInput, |f| f.statement = None);
    refuses(Case::ForwardNoH, IdentityMismatch, |f| {
        f.statement = Some(9)
    });
    // Exact sources, physical after order and the Tx descriptor are required.
    refuses(Case::ForwardH, IdentityMismatch, |f| {
        f.sources[0].as_mut().unwrap().cell = Some((0, 15_841, 512))
    });
    refuses(Case::ForwardH, IdentityMismatch, |f| {
        f.after_order.swap(2, 3)
    });
    refuses(Case::ForwardNoH, InvalidInput, |f| f.sources[1] = None);
    let mut fixture = Fixture::new(Case::ForwardNoH);
    let mut input = fixture.input();
    input.transaction = None;
    let mut frame = [0xa5; FRAME_BYTES];
    let mut after = [[0xa5; PAGE_BYTES]; 5];
    assert_eq!(
        group::produce(&input, &mut frame, &mut after, &mut scratch()),
        Err(InvalidInput)
    );
    assert!(frame.iter().all(|byte| *byte == 0xa5));
    assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
    // A U observed after this Forward (LSN301 > previous300) is refused.
    fixture.before[3] = *scope3!("noh-301-undo11.bin");
    assert_eq!(fixture.produce(&mut frame, &mut after), Err(OrderMismatch));
    assert!(frame.iter().all(|byte| *byte == 0xa5));
}

#[test]
fn mca00_v06_clr_refusals_precede_output() {
    use InsertError::{IdentityMismatch, InvalidInput, OrderMismatch};
    // Rollback target and next must be the actual Status/H relation.
    refuses(Case::ClrH, OrderMismatch, |f| f.target = None);
    refuses(Case::ClrH, OrderMismatch, |f| f.next = None);
    refuses(Case::ClrNoH, OrderMismatch, |f| f.next = Some(h_link()));
    // Statement rollback needs its statement and mark; full rollback has none.
    refuses(Case::ClrH, InvalidInput, |f| f.statement = None);
    refuses(Case::ClrH, InvalidInput, |f| f.mark = None);
    refuses(Case::ClrNoH, InvalidInput, |f| f.statement = Some(1));
    // The current Row/RootJob observation is exact.
    refuses(Case::ClrNoH, IdentityMismatch, |f| f.projection[0] ^= 1);
    refuses(Case::ClrNoH, IdentityMismatch, |f| f.cell_bytes = 201);
    refuses(Case::ClrH, IdentityMismatch, |f| {
        let cell = f.sources[5].as_mut().unwrap().cell.as_mut().unwrap();
        cell.1 += 1;
    });
    refuses(Case::ClrNoH, IdentityMismatch, |f| {
        f.header.kind = FrameKind::Update
    });
    // An Active Status (no RollbackStart) cannot accept a CLR.
    refuses(Case::ClrNoH, OrderMismatch, |f| {
        f.before[2] = *scope3!("noh-301-status.bin")
    });
}

/// U on page14 sorts after the RowLeaf page13: natives, edges, counts and
/// redo follow (Root, Jobs, Status, Leaf, U), unlike the role order.
#[test]
fn mca00_v06_forward_physical_order_differs_from_roles() {
    literal_case(Case::ForwardOrd);
    refuses(Case::ForwardOrd, InsertError::IdentityMismatch, |f| {
        f.after_order = ROLES
    });
}

/// The whole RowLeaf plan is checked before the first output byte.
#[test]
fn mca00_v06_leaf_refusals_precede_output() {
    // RowId7 already present (an older committed row) at the same LSN50.
    refuses(
        Case::ForwardNoH,
        InsertError::LeafEdit(LeafEditError::KeyExists),
        |f| f.before[4] = *scope3!("noh-300-leaf-dup7.bin"),
    );
    // 91 free entry bytes cannot hold the 206 that this row needs.
    refuses(Case::ForwardNoH, InsertError::Capacity, |f| {
        f.before[4] = *scope3!("noh-300-leaf-full.bin")
    });
    // Same length, other value: the record must match the Job projection.
    refuses(Case::ForwardNoH, InsertError::IdentityMismatch, |f| {
        f.canonical[21] = b'c'
    });
    refuses(Case::ForwardH, InsertError::IdentityMismatch, |f| {
        f.canonical[5] = 43
    });
}

/// Re-encode the Tx before with `extra` more CLR residual WAL/protected.
fn add_residual(fixture: &mut Fixture, extra: u64) {
    let mut spec = obligation::decode_descriptor(
        &fixture.transaction,
        ReservationOwner::Transaction(actor()),
        profile_reference(profile()),
    )
    .unwrap();
    spec.remaining[0].wal += extra;
    spec.remaining[0].protected += extra;
    obligation::encode_descriptor(&spec, &mut fixture.transaction).unwrap();
}

/// A stale or future Aging before and an inexact Tx residual are refused
/// before the first output byte.
#[test]
fn mca00_v06_descriptor_refusals_precede_output() {
    let aging = InsertError::Aging(AgingObligationError::IdentityMismatch);
    refuses(Case::ClrNoH, aging, |f| {
        f.aging = *scope3!("noh-300-aging.bin")
    });
    refuses(Case::ForwardNoH, aging, |f| {
        f.aging = *scope3!("noh-301-aging.bin")
    });
    refuses(Case::ClrH, aging, |f| f.aging = *scope3!("h-500-aging.bin"));
    // CLR with one step left but 128 residual bytes would end with WAL
    // and no step; Forward must carry the residual unchanged and exact.
    refuses(Case::ClrNoH, InsertError::BudgetMismatch, |f| {
        add_residual(f, 128)
    });
    refuses(Case::ClrH, InsertError::BudgetMismatch, |f| {
        add_residual(f, 128)
    });
    refuses(Case::ForwardNoH, InsertError::BudgetMismatch, |f| {
        add_residual(f, 128)
    });
}

/// writer_epoch0 would fail the envelope; LSN0 pages are genesis images.
#[test]
fn mca00_v06_header_and_zero_lsn_refusals_precede_output() {
    refuses(Case::ForwardNoH, InsertError::IdentityMismatch, |f| {
        f.header.stream.writer_epoch = 0
    });
    refuses(Case::ClrNoH, InsertError::IdentityMismatch, |f| {
        f.header.stream.writer_epoch = 0
    });
    refuses(Case::ForwardNoH, InsertError::OrderMismatch, |f| {
        f.before[5] = *scope3!("anchor-lsn0.bin")
    });
    refuses(Case::ClrH, InsertError::OrderMismatch, |f| {
        f.before[6] = *scope3!("anchor-lsn0.bin")
    });
}

/// Optional C cross-check: the shared scope3 observers accept every produced
/// group. Descriptor residuals, Aging and Status/Undo/Job transitions are
/// checked by their original observers; the Leaf observer needs a Catalog
/// record that these literals do not include.
#[test]
fn mca00_v06_produced_groups_pass_c_observers() {
    for case in [
        Case::ForwardNoH,
        Case::ForwardH,
        Case::ClrNoH,
        Case::ClrH,
        Case::ForwardOrd,
    ] {
        let fixture = Fixture::new(case);
        let mut frame = [0; FRAME_BYTES];
        let mut after = [[0; PAGE_BYTES]; 5];
        let produced = fixture.produce(&mut frame, &mut after).unwrap();
        let frame = &frame[..produced.frame_len];
        let expected = FrameExpectation {
            stream: fixture.header.stream,
            previous_lsn: fixture.header.previous_lsn,
        };
        let layout = semantic_row::decode_layout(frame, expected, profile()).unwrap();
        let h = fixture.predecessor.is_some();
        let (status_before, undo_before, jobs_before) = if h {
            (&fixture.before[3], &fixture.before[4], &fixture.before[1])
        } else {
            (&fixture.before[2], &fixture.before[3], &fixture.before[1])
        };
        let context = undo::UndoContext {
            profile: profile(),
            actor: actor(),
            location: location(),
        };
        let statement = fixture.statement.map(|sequence| status::StatementBoundary {
            sequence,
            mark: if fixture.forward() {
                None
            } else {
                fixture.mark
            },
        });
        let images = status::StatusImages {
            before: status_before,
            after: &after[fixture.after_index(RowPageRole::Status)],
        };
        status::observe(
            layout,
            status::StatusContext {
                profile: profile(),
                actor: actor(),
                location: location(),
                statement,
                commit: None,
            },
            images,
        )
        .unwrap_or_else(|error| panic!("{case:?} status: {error:?}"));
        undo::observe(
            layout,
            context,
            JobCells::original(&fixture.bank),
            undo::UndoSource::Existing {
                ordinal: fixture.ordinal,
            },
            undo::UndoImages {
                before: undo_before,
                after: &after[fixture.after_index(RowPageRole::Undo)],
                predecessor: h.then_some(&fixture.before[7]),
            },
            Some(undo::StatusProof {
                statement,
                commit: None,
                images,
            }),
        )
        .unwrap_or_else(|error| panic!("{case:?} undo: {error:?}"));
        jobs::observe(
            layout,
            jobs::Inputs {
                context,
                jobs: JobCells::original(&fixture.bank),
                ordinal: fixture.ordinal,
                images: jobs::JobImages {
                    before: jobs_before,
                    after: &after[fixture.after_index(RowPageRole::RootJob)],
                },
                reserve: None,
                sender: fixture.forward().then_some(&fixture.transaction),
                commit: None,
            },
        )
        .unwrap_or_else(|error| panic!("{case:?} jobs: {error:?}"));
        let slot = fixture.ordinal % 15;
        let jobs_after = &after[fixture.after_index(RowPageRole::RootJob)];
        let job: &[u8; JOB_BYTES] = jobs_after[usize::from(job_cell(slot).1)..][..JOB_BYTES]
            .try_into()
            .unwrap();
        let status_after = &after[fixture.after_index(RowPageRole::Status)];
        let home = StatusHome::decode(&status_after[16_128..16_352]).unwrap();
        let transaction: &[u8; 1536] = frame[416..1952].try_into().unwrap();
        ledger::validate_transaction(
            transaction,
            profile(),
            home,
            location(),
            JobCells::original(&fixture.bank)
                .replace(fixture.ordinal, job)
                .unwrap(),
        )
        .unwrap_or_else(|error| panic!("{case:?} ledger: {error:?}"));
        let root_after = &after[fixture.after_index(RowPageRole::Root)];
        retention_obligation::row::decode(&frame[1952..3488], profile(), root_after)
            .unwrap_or_else(|error| panic!("{case:?} aging: {error:?}"));
    }
}
