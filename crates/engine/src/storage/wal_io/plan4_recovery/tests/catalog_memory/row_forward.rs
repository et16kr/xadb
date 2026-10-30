//! S2-c: the Forward path of a first Row INSERT (S2-0 Row write amendment §1
//! "order", the refusal table and §6, S2-0b `#row-write-validate-once`):
//! `ReservedRow::producer_lane`, A's actual producer (`produce_insert`),
//! `ReservedRow::prepare` and `PreparedRow::apply`, over the actual committed
//! CREATE of table T that the preflight and reserve cases use. Here are
//! S2W-R09, R10, R11, R12, R13, R21, R24, the producer_lane, prepare and
//! apply halves of R22, R23(2) in its prepare-then-cancel order, and the
//! Forward parts of S2W-H01..H05. R10, R11 and R21(1) here end their
//! reservation with a successful Forward on the same reservation; their
//! cancel ends are in `row_cancel.rs` (S2-d). A Row transaction with a
//! durable Forward has no healthy end before S3/S4 (commit, rollback), so
//! these cases stop with the statement ended and the transaction dropped. The expected values are fixture facts, the
//! independent literal of `tests/fixtures/dev3-module-c3/row-write/`
//! (expected.json and expected-forward.json) and the refusal table's (phase,
//! cause, effect), never values copied from the body.
use super::super::super::super::plan4_pages::{
    RetentionCause as ReplayCause, RetentionFlushPhase, observe_recovery_frame,
    retention_replay_bank, retention_replay_cells, retention_replay_image, seed_recovery_replay,
};
use super::super::faults::{rejected, unchanged_error};
use super::super::support::namespace;
use super::row_preflight::{FIXTURE_T, FREE_UNDO, create_table, resident, system_init};
use super::*;
use crate::storage::{
    control::row_profile,
    dirty_frame::{self, SharedPool},
    disk::{
        IsolationLevel, RetentionCause, RetentionError, RetentionPhase, RowOperation,
        RowOwnedError, RowPageRole, RowPreflightStep, RowProposal, TableStep,
    },
    retention_page::row::{self as job3, JobState},
    row::insert::{produce_insert, validate_insert},
    row_record::ValueRef,
    wal_frame::{FrameExpectation, WalStreamIdentity},
};
use crate::transaction::concurrency::catalog_test_support as hook;

/// The one value of T's row (`i`, Int64, not null).
const VALUE: i64 = 7;
/// The S2W-R06 and S2W-R09 authors (stdlib Python that imports the pinned N1
/// author and no product code) and their outputs, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c3/row-write";
/// The fixture's WAL complete LSN before the reserve; the RowReserve is the
/// next LSN and its Forward the one after.
const PREVIOUS_LSN: u64 = 14;
/// The literal RowReserve frame (expected.json): its bytes and SHA-256.
const RESERVE_FRAME: (usize, &str) = (
    57_336,
    "6e3badc82e1864c405f846a05372e2367f6a266a10e0a4e0c8c3d28267870fbe",
);
/// SHA-256 of the 64 Job cells the RowReserve leaves (expected.json `bank`).
const RESERVED_JOBS: &str = "e74d3a7218763061365b3261373e41506f2537f82fb07951000f8049e41438cf";
/// The literal Forward frame (expected-forward.json): its bytes and SHA-256.
pub(super) const FORWARD_FRAME: (usize, &str) = (
    91_497,
    "0ee305722bff97a71161e2111bb7a15059b3a35b8a941c603a45e2fc5de81043",
);
/// SHA-256 of the Forward literal's after images (Root 1, Jobs 2, Status 11,
/// RowLeaf 13, the Undo 14) and control (the 64 Job cells, the ATT row, the
/// Aging row), in the author's key order.
const FORWARDED: [(&str, &str); 8] = [
    (
        "aging",
        "229676a8c452119a07c6517808b235dc9c5d2426d548d36882d055349b8e123a",
    ),
    (
        "bank",
        "71ed8b07fb2594d20befccc3d2ad6c30c5674546e374cb5608e66e0ed5fe84d7",
    ),
    (
        "jobs2",
        "f5627aa6bf03b8b05c068f99d3405624fad788d64fbda3557307d319128356df",
    ),
    (
        "leaf13",
        "0c3689d98f178c24eb75ff3276bbed2ee7bb410ca38103a00d6708d1ee4e8ff4",
    ),
    (
        "root1",
        "32e2fe6450e9b2fe0cbf07707e6a568cf6d193b6f5383365240e979f3fe49a40",
    ),
    (
        "status11",
        "8371f123e181f5bc9d4c437a7b51282f84a24f2c9dd350ef07b577c7d14f5162",
    ),
    (
        "tx",
        "5ce42256ee656bb794c65387dc8dc548be29f7d0677271af2b81911993ae9ce2",
    ),
    (
        "undo14",
        "801bbcfb6457e6e86916422054ef035475038c644786723d7cd600ee2b954cb8",
    ),
];
/// The literal's RootJob after the Forward: ordinal 3, RootRetained (state
/// 2), generation 2, statement 1, the Reserve's lease LSN 15 and the
/// Forward's adoption LSN 16; its RowId is 1.
const ROOT_JOB: (usize, u8, u64, u64, (u64, u64)) =
    (3, 2, 2, 1, (PREVIOUS_LSN + 1, PREVIOUS_LSN + 2));
/// A's physical order of the five changed pages: Root 1, Jobs 2, Status 11,
/// RowLeaf 13, the Undo 14 (rank order of T).
pub(super) const ORDER: [RowPageRole; 5] = [
    RowPageRole::Root,
    RowPageRole::RootJob,
    RowPageRole::Status,
    RowPageRole::RowLeaf,
    RowPageRole::Undo,
];
/// The Aging descriptor's control row (bank row 32 of 1664 bytes).
const AGING: usize = 32 * 1664;
/// The first byte of the control's XOI inventory region [131072, 147456),
/// the byte the Cancel's R23(2) half changes too: the draft commitment folds
/// that region's digest (`ControlCut`); the whole-ledger entry check reads
/// the bank and the Job cells, not it.
const XOI_BYTE: usize = 131_072;
/// The actor's ATT row (control slot 0): its status-category ledger row
/// (remaining[3], base 1024) holds the steps at 1032 and the WAL bytes at
/// 1040. An Active actor has one Status patch (3040 B) without the
/// statement credit and three with it (6080 B more).
const CREDIT: (usize, usize) = (1032, 1040);
const STATUS_PATCH: u64 = 3_040;

/// A refused step's error, with its wrapper back.
macro_rules! before {
    ($result:expr) => {
        match $result {
            Err(RowOwnedError::Before { error, owned }) => (error, owned),
            Err(RowOwnedError::Fatal(error)) => panic!("Fatal {error:?}"),
            Ok(_) => panic!("a refused step"),
        }
    };
}

/// A failed step's Fatal error.
macro_rules! fatal {
    ($result:expr) => {
        match $result {
            Err(RowOwnedError::Fatal(error)) => error,
            Err(RowOwnedError::Before { error, .. }) => panic!("Before {error:?}"),
            Ok(_) => panic!("a failed step"),
        }
    };
}

/// A successful step's wrapper.
macro_rules! done {
    ($result:expr) => {
        match $result {
            Ok(done) => done,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("step {error:?}")
            }
        }
    };
}

/// A Ready preflight of the fixture's statement, every source resident.
macro_rules! ready {
    ($writer:ident, $owner:ident, $view:ident, $table:ident, $private:ident, $flush:ident) => {
        match $writer
            .preflight_row($owner, &$view, &$table, $private, &mut $flush)
            .unwrap()
        {
            RowPreflightStep::Ready(preflight) => preflight,
            RowPreflightStep::FlushRequired { .. } => panic!("every source is resident"),
        }
    };
}

/// A's plan for one value, through the actual validation lane.
macro_rules! plan {
    ($preflight:ident) => {
        plan!($preflight, VALUE)
    };
    ($preflight:ident, $value:expr) => {
        validate_insert(
            &[ValueRef::Int64($value)],
            $preflight.validation_lane().unwrap(),
        )
        .unwrap()
    };
}

/// A's Forward proposal through the actual producer lane.
macro_rules! produce {
    ($reserved:ident) => {
        produce_insert($reserved.producer_lane().unwrap()).unwrap()
    };
}

/// Live == replay through the original `history::validate_live`, plus the
/// fixture's own byte comparison of the replayed ATT rows and Job cells.
macro_rules! live_replay {
    ($checked:expr) => {{
        let (checked, copies) = $checked;
        checked.unwrap();
        assert_eq!(copies[0], copies[2], "replayed ATT rows");
        assert_eq!(copies[1], copies[3], "replayed Job cells");
        copies
    }};
}

/// (WAL file length, every file image) of the directory: an effect is any
/// change of either.
fn files(state: &State) -> (usize, Images) {
    let state = state.lock().unwrap();
    (state.files["d3-wal-00000001"].bytes.len(), state.images())
}

/// The bytes the WAL file gained from `from` on.
fn wal_since(state: &State, from: usize) -> Vec<u8> {
    state.lock().unwrap().files["d3-wal-00000001"].bytes[from..].to_vec()
}

/// A copy of one resident pool page.
fn page_of(pool: &SharedPool<'_, '_, '_, '_>, number: u64) -> Box<[u8; PAGE_BYTES]> {
    let (index, _) = resident(pool, number);
    let mut bytes = boxed::<PAGE_BYTES>();
    probe::diagnose(pool, index, &mut bytes);
    bytes
}

/// Every page of T in the pool (`FIXTURE_T`, rank order): its bytes, frame
/// stamp and dirty flag, through the diagnostic probe alone, so it also reads
/// a fenced pool.
fn t_frames(
    pool: &SharedPool<'_, '_, '_, '_>,
) -> Vec<(u64, Box<[u8; PAGE_BYTES]>, dirty_frame::FrameStamp, bool)> {
    FIXTURE_T
        .iter()
        .map(|number| {
            let index = probe::registered_index(pool, *number).unwrap();
            let mut bytes = boxed::<PAGE_BYTES>();
            let snapshot = probe::diagnose(pool, index, &mut bytes);
            (*number, bytes, snapshot.frame, snapshot.dirty)
        })
        .collect()
}

/// The commitment refusal: Before Unchanged (Prepare, Frame Stale).
fn stale(error: &RetentionError) -> bool {
    (error.phase, error.effect) == (RetentionPhase::Prepare, Effect::Unchanged)
        && matches!(error.cause, RetentionCause::Frame(frame)
            if frame.cause == dirty_frame::Cause::Stale)
}

/// The actor's (statement-credit steps, WAL bytes) in its ATT row.
fn credit(bank: &[u8]) -> (u64, u64) {
    (u64_at(bank, CREDIT.0), u64_at(bank, CREDIT.1))
}

/// The author's files agree with the literals above: expected.json states
/// the RowReserve frame, and expected-forward.json (indent 2, sorted keys)
/// states each Forward fact in its own object.
fn author_literal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR);
    let reserve = String::from_utf8(read(&root.join("expected.json"), 1 << 16)).unwrap();
    for fact in [
        format!("\"frame_bytes\": {}", RESERVE_FRAME.0),
        format!("\"frame_sha256\": \"{}\"", RESERVE_FRAME.1),
        format!("\"lsn\": {}", PREVIOUS_LSN + 1),
        format!("\"bank\": \"{RESERVED_JOBS}\""),
    ] {
        assert!(reserve.contains(&fact), "expected.json lacks {fact}");
    }
    let forward = String::from_utf8(read(&root.join("expected-forward.json"), 1 << 16)).unwrap();
    let after = FORWARDED
        .iter()
        .map(|(name, digest)| format!("    \"{name}\": \"{digest}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    for fact in [
        "\"case\": \"S2W-R09\"".to_string(),
        format!("\"frame_bytes\": {}", FORWARD_FRAME.0),
        format!("\"frame_sha256\": \"{}\"", FORWARD_FRAME.1),
        format!("\"lsn\": {}", PREVIOUS_LSN + 2),
        "\"row\": 1".to_string(),
        format!("  \"after_sha256\": {{\n{after}\n  }}"),
        format!(
            "  \"root_job\": {{\n    \"adoption_lsn\": {},\n    \"generation\": {},\n    \
             \"lease_lsn\": {},\n    \"ordinal\": {},\n    \"state\": {},\n    \
             \"statement\": {}\n  }}",
            ROOT_JOB.4.1, ROOT_JOB.2, ROOT_JOB.4.0, ROOT_JOB.0, ROOT_JOB.1, ROOT_JOB.3
        ),
    ] {
        assert!(
            forward.contains(&fact),
            "expected-forward.json lacks {fact}"
        );
    }
}

/// The literal's after images and control, live, in `FORWARDED`'s order.
macro_rules! forwarded {
    ($writer:ident, $pool:ident) => {{
        let bank = $writer.recovery_test_bank();
        [
            hex(&bank[AGING..AGING + 1536]),
            hex($writer.catalog_test_jobs().as_flattened()),
            hex(&page_of($pool, 2)[..]),
            hex(&page_of($pool, 13)[..]),
            hex(&page_of($pool, 1)[..]),
            hex(&page_of($pool, 11)[..]),
            hex(&bank[..1536]),
            hex(&page_of($pool, FREE_UNDO)[..]),
        ]
    }};
}

/// The per-frame body of `replay_durable` (the genesis seed, then
/// `observe_recovery_frame` for every frame of the WAL file) over the whole
/// durable WAL. Returns the replay, each frame's (LSN, result) and the last
/// accepted LSN; it stops at the first refused frame.
#[allow(clippy::type_complexity)]
fn durable_replay(
    state: &State,
) -> (
    Box<[u8; RETENTION_REPLAY_BYTES]>,
    Vec<(u64, std::result::Result<(), ReplayCause>)>,
) {
    let (wal, profile) = {
        let state = state.lock().unwrap();
        let profile =
            row_profile::decode_profile(&state.files["d3-profile"].bytes, Some(namespace()))
                .unwrap();
        (state.files["d3-wal-00000001"].bytes.clone(), profile)
    };
    let stream = WalStreamIdentity {
        database: profile.spec().namespace.database,
        timeline: 1,
        writer_epoch: 1,
    };
    let mut durable = boxed::<RETENTION_REPLAY_BYTES>();
    let mut cursor = seed_recovery_replay(profile, None, &mut durable).unwrap();
    let mut frames = Vec::new();
    let mut at = 4096;
    let mut previous = 0;
    while at < wal.len() {
        let length = u32::from_le_bytes(wal[at + 12..at + 16].try_into().unwrap()) as usize;
        let lsn = u64::from_le_bytes(wal[at + 56..at + 64].try_into().unwrap());
        let expected = FrameExpectation {
            stream,
            previous_lsn: previous,
        };
        match observe_recovery_frame(
            profile,
            expected,
            &wal[at..at + length],
            &mut durable,
            &mut cursor,
        ) {
            Ok(observed) => {
                previous = observed.header.lsn;
                frames.push((previous, Ok(())));
            }
            Err(cause) => {
                frames.push((lsn, Err(cause)));
                return (durable, frames);
            }
        }
        at += length;
    }
    assert_eq!(at, wal.len());
    (durable, frames)
}

/// Restart over an image holding a Row group stops at the interim analysis
/// gate: Unsupported/CallerMustResolve, database Unchanged, every file kept.
fn gated(images: &Images) {
    let directory = clone_directory(images);
    let state = directory.state();
    rejected(&directory, &state, |error, observation| {
        unchanged_error(error, observation, Plan4RecoveryPhase::Analysis);
        assert!(matches!(error.cause, Plan4RecoveryCause::Unsupported));
    });
    assert_eq!(
        &state.lock().unwrap().images(),
        images,
        "every file unchanged"
    );
}

/// S2W-R09: Reserve→producer_lane→A's produce→prepare→apply. The Forward
/// frame is the WAL's one new frame after the RowReserve and equals the
/// independent literal (bytes and SHA-256, LSN 16); the prepared cut names
/// it (LSN, length, A's physical roles, operation Forward). The pool's Root
/// 1, Jobs 2, Status 11, RowLeaf 13 and Undo 14 and the control's Job cells,
/// ATT row and Aging row equal the literal's. The RootJob is RootRetained
/// with the statement's sequence, and the view's sequence and mark are the
/// statement's (1, no mark) with its revision moved by one. The scalar
/// observation is the Forward of the issued RowId and UndoRef; the table's
/// write bit stays; the admission, the gate and the lease are returned with
/// no fence, and the loan comes back once, the same loan. The statement then
/// ends with no WAL.
#[test]
fn s2w_r09_forward_lands_the_literal_and_retains_the_root() {
    case("S2W-R09", || {
        author_literal();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let pool = reader.catalog_test_pool();
                let used = reader.catalog_test_source_used();
                let loan = private.as_ptr();
                let (wal, _) = files(state);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let reserve = wal_since(state, wal);
                assert_eq!((reserve.len(), hex(&reserve).as_str()), RESERVE_FRAME);
                let (actor, root_job, row_id, undo_ref, _) = reserved.test_issued();
                let revision = view.revision();
                let proposal = produce!(reserved);
                println!("S2W_R09 proposal={proposal:?}");
                let (wal, _) = files(state);
                let prepared = done!(reserved.prepare(proposal));
                let (_, _, lsn, frame_len, roles, count, operation) = prepared.test_forward_cut();
                assert_eq!(wal_since(state, wal).len(), 0, "prepare writes no WAL");
                let applied = done!(prepared.apply());
                let (observation, returned) = applied.into_parts();
                let frame = wal_since(state, wal);
                let jobs = writer.catalog_test_jobs();
                let job = job3::decode_job(&jobs[ROOT_JOB.0], namespace()).unwrap();
                println!(
                    "S2W_R09 cut=(lsn={lsn} frame_len={frame_len} roles={roles:#x} \
                     count={count} operation={operation}) frame_bytes={} frame_sha={} \
                     observation={observation:?} root_job=({:?},{},{},{},{}) view=(sequence={} \
                     mark={:?} revision={}->{}) masks={:?}",
                    frame.len(),
                    hex(&frame),
                    job.state(),
                    job.generation().get(),
                    job.statement(),
                    job.lease_lsn(),
                    job.adoption_lsn(),
                    view.test_sequence(),
                    view.test_mark(),
                    revision,
                    view.revision(),
                    view.test_table_masks(),
                );
                assert_eq!((frame.len(), hex(&frame).as_str()), FORWARD_FRAME);
                assert_eq!(
                    (lsn, frame_len as usize, count, operation),
                    (PREVIOUS_LSN + 2, FORWARD_FRAME.0, 5, 0)
                );
                assert_eq!(returned.as_ptr(), loan);
                assert_eq!(observation.operation(), RowOperation::Forward);
                assert_eq!((observation.row(), observation.undo()), (row_id, undo_ref));
                assert_eq!(root_job.ordinal().unwrap(), ROOT_JOB.0);
                assert_eq!(job.state(), JobState::RootRetained);
                assert_eq!(jobs[ROOT_JOB.0][6], ROOT_JOB.1);
                assert_eq!(job.source(), Some(actor));
                assert_eq!(
                    (
                        job.generation().get(),
                        job.statement(),
                        (job.lease_lsn(), job.adoption_lsn())
                    ),
                    (ROOT_JOB.2, ROOT_JOB.3, ROOT_JOB.4)
                );
                assert_eq!(job.row_id(), Some(row_id));
                assert_eq!(job.undo(), Some(undo_ref));
                assert_eq!(view.test_sequence(), job.statement());
                assert_eq!(view.test_mark(), None);
                assert_eq!(view.revision(), revision + 1);
                for ((name, pin), live) in FORWARDED.iter().zip(forwarded!(writer, pool)) {
                    assert_eq!(&live, pin, "after {name}");
                }
                assert_eq!(
                    writer.free_undo_test_state().unwrap().0.complete_lsn,
                    PREVIOUS_LSN + 2
                );
                assert_eq!(Some(actor), row.test_write_identity().0);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(!fence.is_fenced());
                let (wal, _) = files(state);
                assert!(writer.finish_statement(owner, view).is_ok());
                assert_eq!(files(state).0, wal);
                assert!(!fence.is_fenced());
                drop(row);
            }
        );
    });
}

/// S2W-R10: a prepare of a tampered A proposal is Before Unchanged (Prepare)
/// on the same ReservedRow (same issued identity and draft cut), with no
/// WAL, file, page or fence change and the gate still closed. The tampered
/// forms are A's declared metadata (operation, frame length, after count,
/// the physical roles) and A's bytes in the loan (one frame byte, one after
/// page byte, written through a second producer lane after A's produce). A
/// fresh produce on that ReservedRow then prepares and applies the literal's
/// Forward. The Cancel that also ends such a reservation is
/// `row_cancel::s2w_r10_tampered_proposal_is_before_then_cancel_lands_the_literal`.
#[test]
fn s2w_r10_tampered_proposal_is_before_on_the_same_reservation() {
    case("S2W-R10", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let draft = reserved.test_draft();
                let effects = files(state);
                let frames = t_frames(pool);
                let length = FORWARD_FRAME.0;
                let mut swapped = ORDER;
                swapped.swap(0, 1);
                let declared = [
                    (
                        "operation",
                        RowProposal::new(RowOperation::Clr, length, 5, ORDER),
                    ),
                    (
                        "frame_len",
                        RowProposal::new(RowOperation::Forward, length - 1, 5, ORDER),
                    ),
                    (
                        "frame_len_past_range",
                        RowProposal::new(RowOperation::Forward, 98_305, 5, ORDER),
                    ),
                    (
                        "after_count",
                        RowProposal::new(RowOperation::Forward, length, 4, ORDER),
                    ),
                    (
                        "roles",
                        RowProposal::new(RowOperation::Forward, length, 5, swapped),
                    ),
                ];
                // A's produce fills the loan's frame and after range once;
                // each metadata form is checked over those bytes.
                let _ = produce!(reserved);
                let mut refusals = Vec::new();
                for (name, proposal) in declared {
                    let (error, owned) = before!(reserved.prepare(proposal));
                    reserved = owned;
                    refusals.push((name, error));
                }
                // One byte of A's frame, then of A's first after page,
                // changed in the loan after A's produce.
                for (name, at) in [("frame_byte", 0usize), ("after_byte", 1)] {
                    let proposal = produce!(reserved);
                    {
                        let (_, _, _, _, frame, after, ..) =
                            reserved.producer_lane().unwrap().parts();
                        if at == 0 {
                            frame[length / 2] ^= 1;
                        } else {
                            after[0][PAGE_BYTES / 2] ^= 1;
                        }
                    }
                    let (error, owned) = before!(reserved.prepare(proposal));
                    reserved = owned;
                    refusals.push((name, error));
                }
                for (name, error) in &refusals {
                    println!("S2W_R10 tampered={name} prepare={error:?}");
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged),
                        "{name}"
                    );
                    assert!(!stale(error), "{name}");
                }
                let semantic = |at: usize| match &refusals[at].1.cause {
                    RetentionCause::RowSemantic(error) => *error,
                    cause => panic!("{} {cause:?}", refusals[at].0),
                };
                // C's own check of A's declared metadata, then C's observer
                // over A's bytes: a short declared length is the observer's
                // truncated frame and a changed frame byte its frame digest.
                use crate::storage::{semantic_row::Error as Semantic, wal_frame::WalError};
                let expected = [
                    Semantic::InvalidInput,
                    Semantic::Wal(WalError::Truncated),
                    Semantic::InvalidInput,
                    Semantic::InvalidInput,
                    Semantic::IdentityMismatch,
                    Semantic::Wal(WalError::Corrupt),
                    Semantic::IdentityMismatch,
                ];
                for (at, cause) in expected.into_iter().enumerate() {
                    assert_eq!(semantic(at), cause, "{}", refusals[at].0);
                }
                assert_eq!(reserved.test_issued(), issued);
                assert_eq!(reserved.test_draft(), draft);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert!(hook::observe(core).read_gate_closed);
                assert!(!fence.is_fenced());
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let _ = done!(prepared.apply());
                let frame = wal_since(state, effects.0);
                println!(
                    "S2W_R10 after_refusals frame_bytes={} frame_sha={}",
                    frame.len(),
                    hex(&frame)
                );
                assert_eq!((frame.len(), hex(&frame).as_str()), FORWARD_FRAME);
                assert!(!hook::observe(core).read_gate_closed);
                assert!(!fence.is_fenced());
                assert!(writer.finish_statement(owner, view).is_ok());
                drop(row);
            }
        );
    });
}

/// S2W-R11: a refusal before apply's stage (a one-shot cfg(test) refusal
/// after the table write bit and before the existing-group install) is
/// Before Unchanged (Stage, Busy) on the same PreparedRow; `reopen` returns
/// the same ReservedRow (issued identity and draft cut), with no WAL, file,
/// page or fence change, the write bit kept and the gate still closed. On
/// that ReservedRow a fresh produce, prepare and apply land the literal's
/// Forward. The Cancel after `reopen` is
/// `row_cancel::s2w_r11_apply_stage_refusal_reopens_then_cancel_lands_the_literal`.
#[test]
fn s2w_r11_apply_stage_refusal_reopens_the_same_reservation() {
    case("S2W-R11", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let pool = reader.catalog_test_pool();
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let draft = reserved.test_draft();
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let cut = prepared.test_forward_cut();
                let effects = files(state);
                let frames = t_frames(pool);
                let revision = view.revision();
                let once = prepared.test_forward_stage_refusal_once();
                let (error, prepared) = before!(prepared.apply());
                drop(once);
                println!("S2W_R11 apply={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Stage, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Busy));
                assert_eq!(prepared.test_forward_cut(), cut);
                let mut reserved = prepared.reopen();
                assert_eq!(reserved.test_issued(), issued);
                assert_eq!(reserved.test_draft(), draft);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert_eq!(view.revision(), revision);
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert!(!fence.is_fenced());
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let _ = done!(prepared.apply());
                let frame = wal_since(state, effects.0);
                assert_eq!((frame.len(), hex(&frame).as_str()), FORWARD_FRAME);
                assert_eq!(view.revision(), revision + 1);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                assert!(writer.finish_statement(owner, view).is_ok());
                drop(row);
            }
        );
    });
}

/// S2W-R12 (Forward, `write`): a WAL write fault after apply's stage is
/// Fatal (Durable, Wal), CallerMustResolve, with the fence and no frame
/// written. The gate is reopened and the lease ends; the admission's checked
/// release is refused by the fenced ledger, which keeps its charge visible.
/// The reservation, still active, fences again in its Drop, and the write
/// bit stays.
#[test]
fn s2w_r12_apply_write_fault_after_stage_is_fatal() {
    case("S2W-R12", || r12(false));
}

/// S2W-R12 (Forward, `sync`): the same at the durable wait, after the
/// Forward frame is written.
#[test]
fn s2w_r12_apply_sync_fault_after_stage_is_fatal() {
    case("S2W-R12", || r12(true));
}

fn r12(sync: bool) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 12,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            row_statement!(live, owner, flusher, private, reader_private, row_status
                => writer, reader, flush, row, view, table);
            let _ = settle!(writer, owner, view, table, private, flush, flusher);
            let (_, slot, _) = table.test_identity();
            let used = reader.catalog_test_source_used();
            let charge = writer.catalog_test_retirement_charge() as u64;
            let mut preflight = ready!(writer, owner, view, table, private, flush);
            let input = plan!(preflight);
            let mut reserved = done!(preflight.reserve(input));
            let proposal = produce!(reserved);
            let prepared = done!(reserved.prepare(proposal));
            let revision = view.revision();
            let (start, _) = files(state);
            {
                let mut adapter = state.lock().unwrap();
                adapter.events.clear();
                adapter.fault_hits = 0;
                adapter.fault = Some(port::Fault {
                    key: port::Key::new(
                        if sync { "sync" } else { "write" },
                        "d3-wal-00000001",
                        if sync { 0 } else { start as u64 },
                        1,
                    ),
                    action: if sync {
                        port::Action::AfterError
                    } else {
                        port::Action::BeforeError
                    },
                });
            }
            let error = fatal!(prepared.apply());
            let (end, _) = files(state);
            println!(
                "S2W_R12 apply sync={sync} refusal={error:?} wal_gained={} gate_closed={} \
                 fenced={}",
                end - start,
                hook::observe(core).read_gate_closed,
                fence.is_fenced()
            );
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Durable, Effect::CallerMustResolve)
            );
            assert!(matches!(error.cause, RetentionCause::Wal(_)));
            assert!(fence.is_fenced());
            assert!(!hook::observe(core).read_gate_closed);
            assert_eq!(hook::observe(core).active_read_operations, 0);
            assert_eq!(reader.catalog_test_source_used(), used + charge);
            assert_eq!(view.test_table_masks().1, 1 << slot);
            assert_eq!(view.revision(), revision);
            assert_eq!(end - start, if sync { FORWARD_FRAME.0 } else { 0 });
            {
                let adapter = state.lock().unwrap();
                assert_eq!(adapter.fault_hits, 1);
                assert!(adapter.fault.is_none());
            }
            drop(view);
            drop(row);
        }
    );
}

/// S2W-R21(1): a one-shot cfg(test) stamp change seen only by apply's draft
/// recompute before its stage makes apply Before Unchanged (Prepare, Frame
/// Stale) on the same PreparedRow, with no WAL, file, page or issuer change
/// (every page of T with its stamp, the transaction issuer's locals). A then
/// reopens: the same ReservedRow's next produce, prepare and apply land the
/// literal's Forward. The Cancel after `reopen` is
/// `row_cancel::s2w_r21_one_stale_stamp_at_apply_is_before_then_cancel_lands_the_literal`.
#[test]
fn s2w_r21_one_stale_stamp_at_apply_is_before_then_reopen() {
    case("S2W-R21", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let locals = writer.row_test_locals();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let effects = files(state);
                let frames = t_frames(pool);
                let once = prepared.test_forward_stale_stamp_once();
                let (error, prepared) = before!(prepared.apply());
                drop(once);
                println!("S2W_R21 stale_stamp apply={error:?}");
                assert!(stale(&error));
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert!(hook::observe(core).read_gate_closed);
                assert!(!fence.is_fenced());
                let mut reserved = prepared.reopen();
                assert_eq!(reserved.test_issued(), issued);
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let _ = done!(prepared.apply());
                let frame = wal_since(state, effects.0);
                assert_eq!((frame.len(), hex(&frame).as_str()), FORWARD_FRAME);
                // Neither the refused apply nor the Forward issues a
                // transaction.
                assert_eq!(writer.row_test_locals(), locals);
                assert!(!fence.is_fenced());
                assert!(writer.finish_statement(owner, view).is_ok());
                drop(row);
            }
        );
    });
}

/// S2W-R21(2): a write target of T reported not resident to apply's typed
/// read (a one-shot cfg(test) refusal of that read, the frame the contract
/// empties) is Fatal (Prepare, Invariant), CallerMustResolve, with the
/// fence and no WAL or file change. The gate is reopened; the admission's
/// checked release is refused by the fenced ledger, which keeps its charge
/// visible, and the table's write bit stays (as R12 and R23(2) assert).
#[test]
fn s2w_r21_missing_target_at_apply_is_fatal() {
    case("S2W-R21", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let effects = files(state);
                let once = prepared.test_forward_missing_target_once();
                let error = fatal!(prepared.apply());
                drop(once);
                println!("S2W_R21 missing_target apply={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::CallerMustResolve)
                );
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert!(fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert_eq!(view.test_table_masks().1, 1 << slot);
                assert_eq!(files(state), effects);
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R22 (producer_lane, prepare and apply halves): with every target
/// resident, a preflight alone checks the replay out once; a preflight,
/// `validation_lane`, reserve, `producer_lane`, prepare and apply together
/// check it out the same once, and each of the three Forward steps alone
/// opens no scope and checks nothing out (S2-0b decision 1).
#[test]
fn s2w_r22_forward_checks_out_no_replay() {
    case("S2W-R22", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let start = writer.row_test_replay_counts();
                let (_, flushed) = settle!(writer, owner, view, table, private, flush, flusher);
                assert!(flushed.is_empty());
                let preflight_only = writer.row_test_replay_counts();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let reserve = reserved.test_replay_counts();
                let proposal = produce!(reserved);
                let lane_counts = reserved.test_replay_counts();
                let prepared = done!(reserved.prepare(proposal));
                let prepare = prepared.test_replay_counts();
                let _ = done!(prepared.apply());
                let apply = writer.row_test_replay_counts();
                let step = |from: (u64, u64), to: (u64, u64)| (to.0 - from.0, to.1 - from.1);
                println!(
                    "S2W_R22 preflight scopes={} checkouts={} through_reserve={:?} \
                     producer_lane={:?} prepare={:?} apply={:?}",
                    preflight_only.0 - start.0,
                    preflight_only.1 - start.1,
                    step(preflight_only, reserve),
                    step(reserve, lane_counts),
                    step(lane_counts, prepare),
                    step(prepare, apply),
                );
                assert_eq!(preflight_only.1 - start.1, 1);
                assert_eq!(
                    step(preflight_only, reserve),
                    (preflight_only.0 - start.0, 1)
                );
                assert_eq!(step(reserve, lane_counts), (0, 0));
                assert_eq!(step(lane_counts, prepare), (0, 0));
                assert_eq!(step(prepare, apply), (0, 0));
                assert!(!fence.is_fenced());
                let _ = reader;
                assert!(writer.finish_statement(owner, view).is_ok());
                drop(row);
            }
        );
    });
}

/// S2W-R23(2): after the Reserve's install, one control byte (`XOI_BYTE`)
/// changed with the draft commitment not folded again. The producer lane
/// checks no chain and lends it; A's produce proposes; the prepare is Before
/// Unchanged (Prepare, Frame Stale) on the same ReservedRow, and the cancel
/// that follows on it is Before (Prepare, Frame Stale) too, on the same
/// ReservedRow. Between them and after them there is no WAL, file, page or
/// issuer change (every page of T in the pool with its stamp, the
/// transaction issuer's locals, the Job cells the Reserve left), and the
/// wrapper keeps its admission charge. That ReservedRow's Drop leaves the
/// fence, and the fenced ledger keeps the charge visible. The cancel alone
/// is `row_cancel::s2w_r23_control_byte_after_reserve_is_stale_cancel_then_drop_fences`.
#[test]
fn s2w_r23_control_byte_after_reserve_is_stale_prepare_then_cancel_then_drop_fences() {
    case("S2W-R23", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let pool = reader.catalog_test_pool();
                let locals = writer.row_test_locals();
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let effects = files(state);
                let frames = t_frames(pool);
                reserved.test_flip_control(XOI_BYTE);
                let proposal = produce!(reserved);
                let (error, owned) = before!(reserved.prepare(proposal));
                println!("S2W_R23 control_byte={XOI_BYTE} prepare={error:?}");
                assert!(stale(&error));
                assert_eq!(owned.test_issued(), issued);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert!(!fence.is_fenced());
                // The cancel on the same ReservedRow sees the same change.
                let (error, owned) = before!(owned.cancel());
                println!("S2W_R23 control_byte={XOI_BYTE} cancel={error:?}");
                assert!(stale(&error));
                assert_eq!(owned.test_issued(), issued);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert!(!fence.is_fenced());
                drop(owned);
                assert!(fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert_eq!(writer.row_test_locals(), locals);
                assert_eq!(
                    hex(writer.catalog_test_jobs().as_flattened()),
                    RESERVED_JOBS
                );
                assert_eq!(view.test_table_masks().1, 1 << slot);
                assert_eq!(files(state), effects);
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-H01..H05, the Forward parts: the original `history::validate_live`
/// after the Reserve (while the ReservedRow holds it) and after the Forward
/// is live == replay, with the replayed ATT rows and Job cells byte-equal to
/// the live ones. The durable replay of the whole WAL after the Forward
/// (`replay_durable`'s per-frame body from the genesis seed) accepts every
/// frame through the Forward's LSN and equals the live Job cells, ATT rows
/// and the Root 1, Jobs 2, Status 11, RowLeaf 13 and Undo 14 pool pages. A
/// crash image in the Reserved state and one after the Forward each stop at
/// the interim analysis gate with every file unchanged. The Cancel parts are
/// `row_cancel::s2w_h01_h05_reserve_and_cancel_replay_equal_live_and_stop_at_the_gate`.
#[test]
fn s2w_h01_h05_forward_replay_equals_live_and_stops_at_the_gate() {
    case("S2W-H01..H05", || {
        let oracle = Oracle::new();
        let crashes = with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let at_reserve = live_replay!(reserved.test_live_replay());
                let reserved_image = state.lock().unwrap().images();
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let _ = done!(prepared.apply());
                let at_forward = live_replay!(writer.row_test_live_replay(owner));
                let (durable, frames) = durable_replay(state);
                println!(
                    "S2W_H01_H05 validate_live reserve=ok forward=ok jobs_changed={} \
                     durable_frames={:?}",
                    at_reserve[3] != at_forward[3],
                    frames.iter().map(|frame| frame.0).collect::<Vec<_>>()
                );
                assert!(frames.iter().all(|frame| frame.1.is_ok()));
                assert_eq!(frames.last().unwrap().0, PREVIOUS_LSN + 2);
                assert_ne!(at_reserve[3], at_forward[3]);
                assert_eq!(
                    retention_replay_cells(&durable).as_flattened(),
                    &at_forward[3][..]
                );
                assert_eq!(
                    &retention_replay_bank(&durable)[..at_forward[2].len()],
                    &at_forward[2][..]
                );
                for number in [1, 2, 11, 13, FREE_UNDO] {
                    assert_eq!(
                        retention_replay_image(&durable, number as usize),
                        &*page_of(pool, number),
                        "page {number}"
                    );
                }
                let forward_image = state.lock().unwrap().images();
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(!fence.is_fenced());
                drop(row);
                (reserved_image, forward_image)
            }
        );
        gated(&crashes.0);
        gated(&crashes.1);
    });
}

/// The pool Root's free cursor (Root cell offset 72), read as the author
/// reads it.
fn root_cursor(pool: &SharedPool<'_, '_, '_, '_>) -> u64 {
    let (_, request) = resident(pool, 1);
    let bytes = page_of(pool, 1);
    let root = crate::storage::page::decode_page(
        &bytes[..],
        request.identity,
        crate::storage::page::PageKind::RetentionRoot,
    )
    .unwrap();
    u64_at(root.cell(0).unwrap(), 72)
}

/// The pool Root with its free cursor (Root cell offset 72) moved by one,
/// modulo 64, re-encoded with the same header (its LSN) in place; the frame's
/// stamp is not touched.
fn move_root_cursor(pool: &SharedPool<'_, '_, '_, '_>) {
    let (index, request) = resident(pool, 1);
    dirty_frame::status_load_test_support::edit_content(pool, index, |bytes| {
        let (header, mut cell) = {
            let root = crate::storage::page::decode_page(
                &bytes[..],
                request.identity,
                crate::storage::page::PageKind::RetentionRoot,
            )
            .unwrap();
            (root.header(), root.cell(0).unwrap().to_vec())
        };
        let cursor = (u64_at(&cell, 72) + 1) % 64;
        cell[72..80].copy_from_slice(&cursor.to_le_bytes());
        crate::storage::page::encode_page(header, &[&cell[..]], &mut bytes[..]).unwrap();
    });
}

/// S2W-R24: a self-consistent divergence after the Reserve that the live
/// Forward cannot see and the replay of F2 can. **The byte:** the Root free
/// cursor, Root cell offset 72 (S2-0b design §7 "Notes for S2-c", 4), in the
/// contract's post-install form: after the durable Reserve, the fixture moves
/// it by one, modulo 64, in the pool Root (re-encoded, stamp unchanged), and
/// re-encodes its one control mirror, the Aging descriptor (control row 32:
/// the progress cursor, and the work digests of the Root page and cell at
/// 384..480; note 4's "no control mirror" does not hold for this Root3
/// format), then folds the draft commitment again over that pool and
/// control. No live Forward check pins the cursor to another source, so the
/// producer lane, A's produce, prepare and apply succeed, and F2, whose Root
/// before and after carry the moved cursor, is durable; the replay of F1
/// leaves the old one. The next row's preflight (a second Free Undo is in
/// place) replays F2 in its `validate_live` (or in a flush's capture proof
/// first, note 5) and is Fatal (Source, c), CallerMustResolve, with the
/// fence and no effect; c is the cause `observe_recovery_frame` reports for
/// F2 when the same WAL is walked frame by frame from the genesis seed,
/// which accepts F1. Restart stops at F2: Analysis, Retention(c), database
/// Unchanged, no mutation and no READY.
#[test]
fn s2w_r24_root_cursor_is_found_late_and_stops_restart() {
    case("S2W-R24", || {
        let oracle = Oracle::new();
        let (image, live_cause, previous) = with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let second = system_init(live, owner, flusher, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                // The second Free Undo's frames are before the Reserve.
                let previous = writer.free_undo_test_state().unwrap().0.complete_lsn;
                let (wal, _) = files(state);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = done!(preflight.reserve(input));
                let reserve = wal_since(state, wal).len();
                let cursor = root_cursor(pool);
                move_root_cursor(pool);
                reserved.test_mirror_aging(&page_of(pool, 1));
                reserved.test_refold_draft().unwrap();
                let moved = root_cursor(pool);
                let proposal = produce!(reserved);
                let prepared = done!(reserved.prepare(proposal));
                let _ = done!(prepared.apply());
                let installed = root_cursor(pool);
                let forward = wal_since(state, wal).len() - reserve;
                assert_eq!(
                    writer.free_undo_test_state().unwrap().0.complete_lsn,
                    previous + 2
                );
                assert!(!fence.is_fenced());
                let effects = files(state);
                // The next row: settle its residency, then the preflight
                // that replays F2.
                let mut detected = None;
                for _ in 0..16 {
                    match writer.preflight_row(owner, &view, &table, private, &mut flush) {
                        Err(error) => {
                            detected = Some(("preflight", error));
                            break;
                        }
                        Ok(RowPreflightStep::Ready(_)) => panic!("F2 replayed"),
                        Ok(RowPreflightStep::FlushRequired { target }) => {
                            let mut step = match flush.try_flush(flusher, &target) {
                                Ok(step) => step,
                                Err(error) => {
                                    detected = Some(("flush", error));
                                    break;
                                }
                            };
                            while !matches!(
                                step,
                                RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
                            ) {
                                step = match flush.continue_flush(flusher) {
                                    Ok(step) => step,
                                    Err(error) => {
                                        detected = Some(("flush", error));
                                        break;
                                    }
                                };
                            }
                            if detected.is_some() {
                                break;
                            }
                        }
                    }
                }
                let (point, error) = detected.expect("F2 is found");
                println!(
                    "S2W_R24 byte=root_cell_offset_72 cursor reserve={cursor} moved={moved} \
                     forward={installed} second_undo={:?} frames=(reserve={reserve} \
                     forward={forward}) detected_at={point} refusal={error:?}",
                    second.identity().page.page_no
                );
                assert_eq!(moved, (cursor + 1) % 64);
                // F2's Root after keeps the moved cursor, which the replay of
                // F1 does not have.
                assert_eq!(installed, moved);
                assert_eq!(forward, FORWARD_FRAME.0);
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Source, Effect::CallerMustResolve)
                );
                assert!(fence.is_fenced());
                assert_eq!(files(state), effects, "the next row has no effect");
                drop(view);
                drop(row);
                (effects.1, format!("{:?}", error.cause), previous)
            }
        );
        // The same WAL frame by frame from the genesis seed: F1 accepted, F2
        // refused with the live cause.
        let directory = clone_directory(&image);
        let state = directory.state();
        let (_, frames) = durable_replay(&state);
        let (lsn, refused) = frames.last().unwrap();
        println!(
            "S2W_R24 durable frames={:?} f2_lsn={lsn} f2_cause={refused:?} live_cause={live_cause}",
            frames
                .iter()
                .map(|(lsn, result)| (*lsn, result.is_ok()))
                .collect::<Vec<_>>()
        );
        assert_eq!(*lsn, previous + 2);
        let accepted = &frames[frames.len() - 2];
        assert_eq!(accepted.0, previous + 1);
        assert!(accepted.1.is_ok());
        let durable_cause = match refused {
            Err(cause) => format!("{cause:?}"),
            Ok(()) => panic!("F2 accepted"),
        };
        assert_eq!(durable_cause, live_cause);
        rejected(&directory, &state, |error, observation| {
            println!("S2W_R24 restart={error:?} observation={observation:?}");
            unchanged_error(error, observation, Plan4RecoveryPhase::Analysis);
            match &error.cause {
                Plan4RecoveryCause::Retention(cause) => {
                    assert_eq!(format!("{cause:?}"), live_cause)
                }
                cause => panic!("restart cause {cause:?}"),
            }
        });
        assert_eq!(
            &state.lock().unwrap().images(),
            &image,
            "every file unchanged"
        );
    });
}

/// S2W-R13: two statements of three rows each, every row Reserve, A's
/// produce, prepare and apply in the same Row transaction (six Free Undo
/// containers in place). The 6080 B statement credit (two Status patches
/// beside the one an Active actor holds) is taken by the first statement's
/// first Reserve and held from then on: the second statement inherits it,
/// so no later Reserve takes it again. The second statement's mark is the
/// actor's ATT undo head when it begins: the first statement's last row's
/// Undo, which the view and the slot both hold, and every Forward of a
/// statement carries that statement's sequence. The credit is read from
/// the live control while each Reserve is held, at the statement end and
/// after every Forward. Live == replay is S2W-H01's (a replay per row would
/// exceed the case's time bound).
#[test]
fn s2w_r13_two_statements_of_three_rows_inherit_the_statement_credit() {
    case("S2W-R13", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = (core, state);
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                for _ in 0..5 {
                    let _ = system_init(live, owner, flusher, true);
                }
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let mut credits = vec![("begin", credit(writer.recovery_test_bank()))];
                let mut rows: Vec<(
                    foundation::identity::RowId,
                    crate::storage::undo_ref::UndoRef,
                    u64,
                )> = Vec::new();
                let mut view = view;
                let mut table = table;
                for statement in 1..=2u64 {
                    if statement == 2 {
                        assert!(writer.finish_statement(owner, view).is_ok());
                        credits.push(("statement_end", credit(writer.recovery_test_bank())));
                        view = writer.begin_statement(owner, &row).unwrap();
                        let (_, last_undo, _) = rows[rows.len() - 1];
                        let mut encoded = [0; 40];
                        last_undo.encode(&mut encoded).unwrap();
                        let slot = row.test_statement_slot();
                        println!(
                            "S2W_R13 statement=2 mark={:?} slot=(sequence={} mark={})",
                            view.test_mark(),
                            slot.0,
                            hex(&slot.1)
                        );
                        assert_eq!(view.test_mark(), Some(last_undo));
                        assert_eq!(slot, (2, encoded));
                        table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    }
                    assert_eq!(view.test_sequence(), statement);
                    for at in 0..3 {
                        // Every source stays resident here (no settle).
                        let mut preflight = ready!(writer, owner, view, table, private, flush);
                        let input = plan!(preflight, VALUE + rows.len() as i64);
                        let mut reserved = done!(preflight.reserve(input));
                        credits.push(("reserve", credit(reserved.test_control_bank())));
                        let (_, root_job, row_id, undo_ref, _) = reserved.test_issued();
                        let proposal = produce!(reserved);
                        let prepared = done!(reserved.prepare(proposal));
                        let (observation, _) = done!(prepared.apply()).into_parts();
                        assert_eq!(observation.operation(), RowOperation::Forward);
                        credits.push(("forward", credit(writer.recovery_test_bank())));
                        let job = job3::decode_job(
                            &writer.catalog_test_jobs()[root_job.ordinal().unwrap()],
                            namespace(),
                        )
                        .unwrap();
                        println!(
                            "S2W_R13 statement={statement} row={at} row_id={} undo=({},{}) \
                             job=({:?},{}) credit={:?}",
                            row_id.get(),
                            undo_ref.page().page_no,
                            undo_ref.slot(),
                            job.state(),
                            job.statement(),
                            credits.last().unwrap().1,
                        );
                        assert_eq!(job.state(), JobState::RootRetained);
                        assert_eq!(job.statement(), statement);
                        rows.push((row_id, undo_ref, statement));
                    }
                }
                let begin = credits[0].1;
                println!("S2W_R13 credits={credits:?}");
                // An Active actor: one Status patch without the credit,
                // three with it.
                assert_eq!(begin, (1, STATUS_PATCH));
                for (step, held) in &credits[1..] {
                    assert_eq!(*held, (3, 3 * STATUS_PATCH), "{step}");
                }
                assert_eq!(
                    rows.iter().map(|(row, ..)| row.get()).collect::<Vec<_>>(),
                    [1, 2, 3, 4, 5, 6]
                );
                assert!(!fence.is_fenced());
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(!fence.is_fenced());
                drop(row);
            }
        );
    });
}
