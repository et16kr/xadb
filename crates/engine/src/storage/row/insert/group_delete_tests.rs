//! RDL-A (dev3-row-delete.md#tests): A's DELETE groups, DeleteForward (h0,
//! effect-1 H, effect-0 H) and DeleteClr, and its INSERT Forward over an
//! effect-1 H, produced from the before states of history D and compared
//! byte for byte with the independent `transition-delete/author.py`
//! literals that C's observer accepts (`transition_delete_tests.rs`).
//! Literal observations confer no actual source, reservation, pin, staging,
//! installation or recovery authority.

use super::{
    InsertError,
    group::{self, Change, FRAME_BYTES, GroupInput, Source},
};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        control::{Profile, row_profile},
        disk::RowPageRole,
        page::{self, FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageRef},
        retention_page::row::{self, JobState},
        row_record::RowSchemaInput,
        semantic_row::{
            Error as RowError,
            transition::tests::{
                RowAfters,
                delete::{ABefore, a_actor, a_befores, a_group_over, padded_leaf},
            },
        },
        status_home::StatusHome,
        transaction_record::{
            LogicalRowVersion, RollbackState, TransactionState, TransactionStatusCell, VersionStamp,
        },
        tree_page::{TREE_ENTRY_CAPACITY, TreeExpectation},
        undo_record::{self, UndoCell, UndoLink, UndoRecordExpectation},
        wal_frame::{FrameExpectation, FrameKind, WalFrameHeader, WalStreamIdentity},
    },
};
use foundation::identity::{Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch};

type Page = [u8; PAGE_BYTES];
/// One before image of A's group: the page, its identity, its kind and the
/// cell (slot, offset, length) it carries.
type Image<'p> = (&'p Page, PageIdentity, PageKind, Option<(u16, u16, u16)>);

const PROFILE: &[u8; 4096] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/scope3/profile3.bin"
));
const ANCHOR: &Page = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/scope3/anchor.bin"
));
const D_BLOBS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition-delete/blobs.bin"
));
const D_INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/transition-delete/index.tsv"
));
const ROOT_CELL: (u16, u16, u16) = (0, 15_840, 512);
const STATUS_CELL: (u16, u16, u16) = (0, 16_128, 224);
const ANCHOR_CELL: (u16, u16, u16) = (0, 16_304, 48);
/// The RootJob of each A group and T2's TerminalLeased Job.
const ORDINALS: [(&str, usize); 5] = [
    ("D-524", 16),
    ("D-526", 19),
    ("D-528", 19),
    ("D-531", 20),
    ("D-533", 21),
];
const TERMINAL: usize = 18;

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
fn identity(page_no: u64, incarnation: u64, owner: u64) -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: page_ref(page_no, incarnation),
        owner: ObjectId::new(owner).unwrap(),
        owner_generation: generation(1),
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
/// (slot, offset, length) of slot `slot` by the literal page directory.
fn directory(page: &Page, slot: u16) -> (u16, u16, u16) {
    let at = 128 + 4 * usize::from(slot);
    (
        slot,
        u16::from_le_bytes([page[at], page[at + 1]]),
        u16::from_le_bytes([page[at + 2], page[at + 3]]),
    )
}
/// The RowLeaf directory entry of `row`.
fn row_cell(page: &Page, row: u64) -> (u16, u16, u16) {
    let count = u16::from_le_bytes([page[72], page[73]]);
    (1..count)
        .map(|slot| directory(page, slot))
        .find(|(_, offset, _)| {
            let start = usize::from(*offset);
            page[start + 4..start + 12] == row.to_be_bytes()
        })
        .unwrap_or_else(|| panic!("row {row} is absent"))
}
fn bytes_of(page: &Page, cell: (u16, u16, u16)) -> &[u8] {
    &page[usize::from(cell.1)..usize::from(cell.1) + usize::from(cell.2)]
}
/// One history D literal by its `index.tsv` row (name, offset, length).
fn blob(name: &str) -> &'static [u8] {
    let line = D_INDEX
        .lines()
        .find(|line| line.split('\t').next() == Some(name))
        .unwrap_or_else(|| panic!("literal {name}"));
    let mut fields = line.split('\t').skip(1);
    let offset: usize = fields.next().unwrap().parse().unwrap();
    let length: usize = fields.next().unwrap().parse().unwrap();
    &D_BLOBS[offset..offset + length]
}
/// History D's INSERT value (statement 4, D-530/D-531).
fn insert_canonical() -> &'static [u8] {
    blob("D-530-canonical")
}

/// One A group's input over its history D before state.
struct Case {
    before: Box<[Page; 9]>,
    count: usize,
    sources: [Option<Source>; 9],
    header: WalFrameHeader,
    row: RowId,
    undo: UndoLink,
    statement: Option<u64>,
    mutation: u64,
    mark: Option<crate::storage::undo_ref::UndoRef>,
    predecessor: Option<UndoLink>,
    ordinal: usize,
    after_order: [RowPageRole; 5],
    columns: [Column; 2],
    clr: Option<(Option<crate::storage::undo_ref::UndoRef>, Option<UndoLink>)>,
    sealed: (usize, [u8; 32]),
}

impl Case {
    fn new(state: &ABefore) -> Self {
        let ordinal = ORDINALS
            .iter()
            .find(|(name, _)| *name == state.name)
            .unwrap()
            .1;
        let (actor, location) = a_actor();
        let job = row::decode_job(&state.bank[ordinal], namespace()).unwrap();
        let undo = UndoLink {
            reference: job.undo().unwrap(),
            sequence: job.undo_sequence(),
        };
        // The census predecessor H: T2's RootRetained root with the largest
        // smaller Undo sequence.
        let predecessor = state
            .bank
            .iter()
            .filter_map(|cell| row::decode_job(cell, namespace()).ok())
            .filter(|other| {
                other.source() == Some(actor)
                    && other.state() == JobState::RootRetained
                    && other.undo_sequence() < undo.sequence
            })
            .max_by_key(|other| other.undo_sequence())
            .map(|other| UndoLink {
                reference: other.undo().unwrap(),
                sequence: other.undo_sequence(),
            });
        let status = StatusHome::decode(bytes_of(&state.status, STATUS_CELL)).unwrap();
        let TransactionStatusCell::Occupied(core) = status.core() else {
            panic!("T2's Status is occupied");
        };
        let clr = match core.state() {
            TransactionState::Aborting(RollbackState::Statement { target, .. }) => {
                Some((target, predecessor))
            }
            TransactionState::Aborting(RollbackState::Transaction { .. }) => {
                Some((None, predecessor))
            }
            _ => None,
        };
        let forward = clr.is_none();
        let u_page = undo.reference.page().page_no;
        let find = |page_no: u64| -> &Page {
            &state
                .undo
                .iter()
                .find(|(number, _)| *number == page_no)
                .unwrap()
                .1
        };
        let mut images: Vec<Image<'_>> = vec![
            (
                &state.root,
                identity(1, 1, 4),
                PageKind::RetentionRoot,
                Some(ROOT_CELL),
            ),
            (
                &state.jobs,
                identity(3, 1, 4),
                PageKind::RetentionJobs,
                Some(job_cell(ordinal)),
            ),
            (
                &state.status,
                identity(20, 2, 2),
                PageKind::TransactionStatus,
                Some(STATUS_CELL),
            ),
            (
                find(u_page),
                identity(u_page, 2, 3),
                PageKind::Undo,
                Some(directory(find(u_page), 0)),
            ),
            (
                &state.leaf,
                identity(13, 2, 6),
                PageKind::RowLeaf,
                (job.effect() == row::RowEffect::Delete || !forward)
                    .then(|| row_cell(&state.leaf, job.row_id().unwrap().get())),
            ),
            (
                ANCHOR,
                identity(12, 2, 6),
                PageKind::TreeAnchor,
                Some(ANCHOR_CELL),
            ),
        ];
        if let Some(link) = predecessor {
            let page_no = link.reference.page().page_no;
            images.push((
                find(page_no),
                identity(page_no, 2, 3),
                PageKind::Undo,
                Some(directory(find(page_no), 0)),
            ));
        }
        let mut before: Box<[Page; 9]> = vec![[0; PAGE_BYTES]; 9]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        for (slot, image) in before.iter_mut().zip(&images) {
            *slot = *image.0;
        }
        let source = |index: usize| {
            let (_, identity, kind, cell) = images[index];
            Some(Source {
                index,
                identity,
                kind,
                level: 0,
                cell,
            })
        };
        let sources = [
            source(0),
            source(1),
            Some(Source {
                cell: Some(job_cell(TERMINAL)),
                ..source(1).unwrap()
            }),
            source(2),
            source(3),
            source(4),
            None,
            source(5),
            predecessor.and_then(|_| source(6)),
        ];
        // The five changed pages in physical (page number) order.
        let mut roles = [
            (1, RowPageRole::Root),
            (3, RowPageRole::RootJob),
            (20, RowPageRole::Status),
            (u_page, RowPageRole::Undo),
            (13, RowPageRole::RowLeaf),
        ];
        roles.sort_unstable_by_key(|(page_no, _)| *page_no);
        let lsn = state.previous_lsn + 1;
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
            previous_lsn: state.previous_lsn,
            txn: Some(actor.id()),
            previous_txn_lsn: u64::from_le_bytes(state.transaction[72..80].try_into().unwrap()),
        };
        let row = job.row_id().unwrap();
        let old = bytes_of(&state.leaf, row_cell_or_zero(&state.leaf, row.get()));
        let sealed = (
            job.cell_bytes() as usize,
            if old.is_empty() {
                [0; 32]
            } else {
                group::projection(table(), row, Some(&old[12..])).unwrap()
            },
        );
        let _ = location;
        Self {
            before,
            count: images.len(),
            sources,
            header,
            row,
            undo,
            statement: Some(job.statement()),
            mutation: job.mutation(),
            mark: match clr {
                Some((target, _)) => target,
                None => predecessor.map(|link| link.reference),
            },
            predecessor,
            ordinal,
            after_order: roles.map(|(_, role)| role),
            columns: columns(),
            clr,
            sealed,
        }
    }

    fn input<'a>(&'a self, state: &'a ABefore, change: Change<'a>) -> GroupInput<'a> {
        let (actor, location) = a_actor();
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
            actor,
            status: location,
            row: self.row,
            undo: self.undo,
            statement: self.statement,
            mutation: self.mutation,
            mark: self.mark,
            predecessor: self.predecessor,
            root_job: self.ordinal,
            terminal_job: TERMINAL,
            sources: self.sources,
            before: &self.before,
            before_count: self.count,
            after_order: &self.after_order,
            jobs: &state.bank,
            transaction: Some(&state.transaction),
            aging: Some(&state.aging),
            change,
        }
    }

    /// The change A's lane would carry for this group.
    fn change(&self, state: &ABefore) -> Change<'static> {
        let job = row::decode_job(&state.bank[self.ordinal], namespace()).unwrap();
        match self.clr {
            Some((target, next)) => Change::Clr {
                cell_bytes: job.cell_bytes() as usize,
                projection: *job.projection_digest(),
                target,
                next,
            },
            None if job.effect() == row::RowEffect::Delete => Change::DeleteForward {
                cell_bytes: self.sealed.0,
                projection: self.sealed.1,
            },
            None => Change::Forward {
                canonical: insert_canonical(),
            },
        }
    }
}

/// The RowLeaf entry of `row`, or an empty one when it is absent.
fn row_cell_or_zero(page: &Page, row: u64) -> (u16, u16, u16) {
    let count = u16::from_le_bytes([page[72], page[73]]);
    (1..count)
        .map(|slot| directory(page, slot))
        .find(|(_, offset, _)| {
            let start = usize::from(*offset);
            page[start + 4..start + 12] == row.to_be_bytes()
        })
        .unwrap_or((0, 0, 0))
}

fn scratch() -> Box<[u8; group::SCRATCH_BYTES]> {
    vec![0x5a; group::SCRATCH_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap()
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

/// A produced group: A's answer, its frame and its five after images.
type ProducedGroup = (
    Result<group::Produced, InsertError>,
    Box<[u8; FRAME_BYTES]>,
    Box<[Page; 5]>,
);

fn produce(case: &Case, state: &ABefore, change: Change<'_>) -> ProducedGroup {
    let mut frame: Box<[u8; FRAME_BYTES]> = vec![0xa5; FRAME_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let mut after: Box<[Page; 5]> = vec![[0xa5; PAGE_BYTES]; 5]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let result = group::produce(
        &case.input(state, change),
        &mut frame,
        &mut after,
        &mut scratch(),
    );
    (result, frame, after)
}

/// RDL-A: A produces each of its history D groups byte for byte: three
/// DeleteForwards (h0, effect-1 H, effect-0 H), the DeleteClr of a statement
/// rollback over an effect-1 H, and the INSERT Forward over an effect-1 H.
#[test]
fn rdl_a_delete_groups_match_literals() {
    let states = a_befores();
    for state in &states {
        let case = Case::new(state);
        let change = case.change(state);
        let (result, frame, after) = produce(&case, state, change);
        let produced = result.unwrap_or_else(|error| panic!("{}: {error:?}", state.name));
        assert_eq!(produced.after_count, 5, "{}", state.name);
        assert_eq!(produced.roles, case.after_order, "{}", state.name);
        assert_eq!(produced.frame_len, state.frame.len(), "{}", state.name);
        same(state.name, &frame[..produced.frame_len], state.frame);
        assert!(
            frame[produced.frame_len..].iter().all(|byte| *byte == 0xa5),
            "{}: frame tail",
            state.name
        );
        assert!(
            after
                .iter()
                .all(|page| page.iter().any(|byte| *byte != 0xa5))
        );
        println!(
            "RDL_A_GROUP\t{}\tbytes={}\troles={:?}",
            state.name, produced.frame_len, produced.roles
        );
    }
}

/// RDL-A: A refuses a DELETE group before its first output byte when the
/// lane's input disagrees with the RootJob or the pages: the effect binds
/// the change (an INSERT value on a delete root, a sealed old row on an
/// INSERT root: Unsupported), the sealed old row binds the RowLeaf (another
/// projection or cell C: IdentityMismatch), the change binds the RootJob's
/// RowId (another committed row: IdentityMismatch), and DeleteClr restores
/// only the committed RowValue its Live U holds (a U whose before value is
/// not the Job's: IdentityMismatch or a codec refusal).
#[test]
fn rdl_a_delete_producer_refuses_before_output() {
    let states = a_befores();
    let by_name = |name: &str| states.iter().find(|state| state.name == name).unwrap();
    let refused = |label: &str, case: &Case, state: &ABefore, change: Change<'_>| {
        let (result, frame, after) = produce(case, state, change);
        let error = result.map(|_| ()).expect_err(label);
        assert!(frame.iter().all(|byte| *byte == 0xa5), "{label}: frame");
        assert!(
            after.iter().flatten().all(|byte| *byte == 0xa5),
            "{label}: after"
        );
        println!("RDL_A_REFUSED\t{label}\t{error:?}");
        error
    };

    let state = by_name("D-524");
    let case = Case::new(state);
    assert_eq!(
        refused(
            "INSERT value on a delete root",
            &case,
            state,
            Change::Forward {
                canonical: insert_canonical(),
            },
        ),
        InsertError::Unsupported
    );
    let mut projection = case.sealed.1;
    projection[0] ^= 1;
    assert_eq!(
        refused(
            "another sealed projection",
            &case,
            state,
            Change::DeleteForward {
                cell_bytes: case.sealed.0,
                projection,
            },
        ),
        InsertError::IdentityMismatch
    );
    assert_eq!(
        refused(
            "another sealed cell",
            &case,
            state,
            Change::DeleteForward {
                cell_bytes: case.sealed.0 + 1,
                projection: case.sealed.1,
            },
        ),
        InsertError::IdentityMismatch
    );

    // Another RowId than the RootJob's: row 9, committed and pruned in the
    // same leaf, with its own sealed cell and projection.
    let mut other = Case::new(state);
    other.row = RowId::new(9).unwrap();
    let old = bytes_of(&state.leaf, row_cell(&state.leaf, 9));
    assert_eq!(
        refused(
            "another RowId",
            &other,
            state,
            Change::DeleteForward {
                cell_bytes: old.len(),
                projection: group::projection(table(), other.row, Some(&old[12..])).unwrap(),
            },
        ),
        InsertError::IdentityMismatch
    );

    let state = by_name("D-531");
    let case = Case::new(state);
    let job = row::decode_job(&state.bank[case.ordinal], namespace()).unwrap();
    assert_eq!(
        refused(
            "sealed old row on an INSERT root",
            &case,
            state,
            Change::DeleteForward {
                cell_bytes: job.cell_bytes() as usize,
                projection: *job.projection_digest(),
            },
        ),
        InsertError::Unsupported
    );

    // DeleteClr over a U whose Live before is not a committed row: the
    // same record re-encoded (a valid page and codec record) with the
    // actor's own stamp, the before an own-row DELETE would leave
    // (dev3-row-delete.md#scope). A restores only `Committed` rows.
    let state = by_name("D-528");
    let mut case = Case::new(state);
    let source = case.sources[4].unwrap();
    let (actor, _) = a_actor();
    let expected = UndoRecordExpectation {
        namespace: namespace(),
        reference: case.undo.reference,
        target: group::row_target(table(), case.row),
    };
    let page =
        page::decode_page(&case.before[source.index], source.identity, PageKind::Undo).unwrap();
    let schema = RowSchemaInput::Columns(&case.columns);
    let mut record = undo_record::decode_undo_record(page.cell(0).unwrap(), expected, &schema)
        .unwrap()
        .live_before()
        .unwrap();
    // The codec takes an own stamp only with an own earlier logical.
    record.before_stamp = Some(VersionStamp::Transaction(actor));
    record.before_logical = Some(LogicalRowVersion::new(actor.id(), 1).unwrap());
    let mut cell = vec![0; 4096];
    let length =
        undo_record::encode_undo_record(UndoCell::Live(record), expected, &schema, &mut cell)
            .unwrap();
    let header = PageHeader {
        lsn: page.header().lsn,
        ..page.header()
    };
    let mut mutant = [0; PAGE_BYTES];
    page::encode_page(header, &[&cell[..length]], &mut mutant).unwrap();
    case.before[source.index] = mutant;
    assert_eq!(
        refused(
            "DeleteClr over an uncommitted RowValue before",
            &case,
            state,
            case.change(state),
        ),
        InsertError::IdentityMismatch
    );
}

/// RDL-C/RDL-A (dev3-row-delete.md#leaf-reservation): three of A's history
/// D groups produced over their before RowLeaf padded with committed rows,
/// each at the reservation's edge, where C's whole-group observer and
/// leaf.rs (with status.rs, undo.rs and jobs.rs over the same leaf when C
/// accepts) give the same answer.
/// - The INSERT Forward over the effect-1 H (D-531, D = 19 from T2's
///   Retained delete root of row 2). At exactly entries + cell + D =
///   capacity, both accept A's frame. One byte fuller, A still writes it (the
///   cell fits without a split; Reserve proved the fit under the same gate)
///   and both refuse it as OrderMismatch, the split check the reservation
///   extends.
/// - The DeleteForward of row 9 (D-526, D = 19) and its DeleteClr (D-528,
///   D = 19 + 14 with row 9's own root). At exactly entries + D = capacity
///   before the group, both accept A's frame: the group moves entries and D
///   by the same C - 180, so the after leaf is at capacity too. One byte
///   fuller, A still writes the group (A checks only that its cell fits) and
///   both refuse the before leaf as Corrupt.
#[test]
fn rdl_a_groups_at_the_leaf_reservation_boundary() {
    let states = a_befores();
    // The new row's entry: its record (168 + L), cell head 12 and slot 4.
    let entry = 168 + insert_canonical().len() + 16;
    for (group, d, entry, refusal) in [
        ("D-531", 199 - 180, entry, RowError::OrderMismatch),
        ("D-526", 199 - 180, 0, RowError::Corrupt),
        ("D-528", (199 - 180) + (194 - 180), 0, RowError::Corrupt),
    ] {
        let state = states.iter().find(|state| state.name == group).unwrap();
        let reserved: usize = state
            .bank
            .iter()
            .filter_map(|cell| row::decode_job(cell, namespace()).ok())
            .filter(|job| {
                job.state() == JobState::RootRetained && job.effect() == row::RowEffect::Delete
            })
            .map(|job| job.cell_bytes() as usize - 180)
            .sum();
        assert_eq!(reserved, d, "{group} D");
        for (free, expected) in [
            (entry + reserved, Ok(())),
            (entry + reserved - 1, Err(refusal)),
        ] {
            let padded = padded_leaf(&state.leaf, free);
            let mut case = Case::new(state);
            case.before[case.sources[5].unwrap().index] = *padded;
            let (result, frame, after) = produce(&case, state, case.change(state));
            let produced = result.unwrap_or_else(|error| panic!("{group} free {free}: {error:?}"));
            let role = |role: RowPageRole| {
                let at = produced.roles.iter().position(|r| *r == role).unwrap();
                &after[at]
            };
            let afters = RowAfters {
                undo: role(RowPageRole::Undo),
                status: role(RowPageRole::Status),
                leaf: role(RowPageRole::RowLeaf),
            };
            assert_eq!(
                a_group_over(group, &padded, &frame[..produced.frame_len], afters),
                (expected, expected),
                "{group} free {free}"
            );
            println!(
                "RDL_A_LEAF_BOUNDARY\t{group}\tentries+cell+D={}\t{expected:?}",
                TREE_ENTRY_CAPACITY - free + entry + reserved
            );
        }
    }
}
