//! S2-d: the Cancel of a first Row INSERT's reservation (S2-0 Row write
//! amendment §1 "order", the refusal table and §6, S2-0b
//! `#row-write-validate-once`) over the actual committed CREATE of table T
//! that the preflight and reserve cases use. Here are S2W-R08 (with a
//! non-Stale Before and its retry, the refusal table's "cancel의 stage 전
//! 거절"), the cancel ends of R10, R11 and R21(1) after a refused Forward step
//! (S2-c's prepare and apply), the cancel halves of R12, R15, R22 and R23(2)
//! (a cancel alone; the prepare-then-cancel order of R23(2) is in
//! `row_forward.rs`), R18 over a live reservation, and the Reserve and Cancel
//! parts of S2W-H01..H05. The expected values are fixture facts, the
//! independent literal of `tests/fixtures/dev3-module-c3/row-write/`
//! (expected.json and expected-cancel.json) and the refusal table's (phase,
//! cause, effect), never values copied from the body.
use super::super::super::super::plan4_pages::{
    RetentionFlushPhase, observe_recovery_frame, retention_replay_bank, retention_replay_cells,
    retention_replay_image, seed_recovery_replay,
};
use super::super::faults::{rejected, unchanged_error};
use super::super::support::namespace;
use super::row_forward::{FORWARD_FRAME, ORDER};
use super::row_preflight::{FREE_UNDO, create_table, resident, t_frames, with_pins};
use super::*;
use crate::storage::{
    control::row_profile,
    dirty_frame::{self, SharedPool},
    disk::{
        IsolationLevel, RetentionCause, RetentionError, RetentionPhase, RowOperation,
        RowOwnedError, RowPreflightStep, RowProposal, StatementFinishError, TableStep,
    },
    page::{self, PageKind},
    retention_page::row::{self as job3, JobState},
    row::insert::{produce_insert, validate_insert},
    row_record::ValueRef,
    wal_frame::{FrameExpectation, WalStreamIdentity},
};
use crate::transaction::concurrency::{
    self,
    catalog_test_support::{self as hook, Point, Resume},
};
use std::sync::mpsc::sync_channel;

/// The one value of T's row (`i`, Int64, not null).
const VALUE: i64 = 7;
/// The S2W-R06 and S2W-R08 author (stdlib Python that imports the pinned N1
/// author and no product code) and its outputs, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c3/row-write";
/// The fixture's WAL complete LSN before the reserve; the RowReserve is the
/// next LSN and its Cancel the one after.
const PREVIOUS_LSN: u64 = 14;
/// The literal RowReserve frame (expected.json): its bytes and SHA-256.
const RESERVE_FRAME: (usize, &str) = (
    57_336,
    "6e3badc82e1864c405f846a05372e2367f6a266a10e0a4e0c8c3d28267870fbe",
);
/// SHA-256 of the 64 Job cells the RowReserve leaves (expected.json `bank`).
const RESERVED_JOBS: &str = "e74d3a7218763061365b3261373e41506f2537f82fb07951000f8049e41438cf";
/// The literal Cancel frame (expected-cancel.json): its bytes and SHA-256.
pub(super) const CANCEL_FRAME: (usize, &str) = (
    40_000,
    "c2b02eaa12afa2c12eb0427ad8a6deb9746ccb884571ab3157928fa6ba6aee4b",
);
/// SHA-256 of the Cancel literal's after images (Root 1, Jobs 2) and control
/// (the 64 Job cells, the ATT row, the Aging row), in the author's key order.
const CANCELLED: [(&str, &str); 5] = [
    (
        "aging",
        "7fa7e3882f2d08834a95d9547301ab83c67aa75f1f1fe12142c709229a1be004",
    ),
    (
        "bank",
        "f5b2ae1315315650e8f32cd04ed30ef7c49345fc03a14a22a226b8b561e26825",
    ),
    (
        "jobs2",
        "81098863253704f7300ba72f877abc28a556d37899e210f7cbf52e09bfeea891",
    ),
    (
        "root1",
        "cb21bb0987e2e099b5b3880deebf2cdec5a6664af2ce96c59e1806d5d490c3c7",
    ),
    (
        "tx",
        "5aceb6234982016a82df90f2f74032ae2c68edb1806fa14fb8fa6f4646469a5f",
    ),
];
/// SHA-256 of the Free Undo 14 after the Reserve; the Cancel keeps it.
const UNDO14: &str = "b975e67e40072ca7edf708059869743acea9bd078e513cbd07d4e3d8e7d8de4e";
/// The Root cell's (reserved count, cancelled count, Undo sequence
/// high-water, RowId high-water) after the Reserve and after the Cancel:
/// nothing the Reserve issued is returned.
const ROOT_RESERVED: [u64; 4] = [4, 0, 2, 1];
const ROOT_CANCELLED: [u64; 4] = [4, 1, 2, 1];
/// The Root cell offsets the author reads those four at.
const ROOT_COUNTS: [usize; 4] = [88, 384, 392, 432];
/// The RootJob of the literal: ordinal 3, generation 2, Free (state 0) after
/// the Cancel.
const ROOT_JOB: (usize, u8, u64) = (3, 0, 2);
/// The Aging descriptor's control row (bank row 32 of 1664 bytes).
const AGING: usize = 32 * 1664;
/// The first byte of the control's XOI inventory region [131072, 147456).
/// The draft commitment folds that region's digest (`ControlCut`); the
/// whole-ledger entry check reads the bank and the Job cells, not it.
const XOI_BYTE: usize = 131_072;

/// A refused cancel's error, with its ReservedRow back.
macro_rules! before {
    ($result:expr) => {
        match $result {
            Err(RowOwnedError::Before { error, owned }) => (error, owned),
            Err(RowOwnedError::Fatal(error)) => panic!("Fatal {error:?}"),
            Ok(_) => panic!("a refused cancel"),
        }
    };
}

/// A successful reserve's ReservedRow.
macro_rules! reserved {
    ($result:expr) => {
        match $result {
            Ok(reserved) => reserved,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("reserve {error:?}")
            }
        }
    };
}

/// A successful cancel's loan.
macro_rules! cancelled {
    ($result:expr) => {
        match $result {
            Ok(cancelled) => cancelled.into_private(),
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("cancel {error:?}")
            }
        }
    };
}

/// A successful prepare's PreparedRow.
macro_rules! prepared {
    ($result:expr) => {
        match $result {
            Ok(prepared) => prepared,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("prepare {error:?}")
            }
        }
    };
}

/// A's Forward proposal through the actual producer lane (S2-c).
macro_rules! produce {
    ($reserved:ident) => {
        produce_insert($reserved.producer_lane().unwrap()).unwrap()
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

/// A's plan for the one value, through the actual validation lane.
macro_rules! plan {
    ($preflight:ident) => {
        validate_insert(
            &[ValueRef::Int64(VALUE)],
            $preflight.validation_lane().unwrap(),
        )
        .unwrap()
    };
}

/// Live == replay through the original `history::validate_live`, plus the
/// fixture's own byte comparison of the replayed ATT rows and Job cells
/// (row_transactions' check, over a writer or a ReservedRow).
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

/// The pool Root's cell 0 at `ROOT_COUNTS`, read as the author reads it.
fn root_counts(pool: &SharedPool<'_, '_, '_, '_>) -> [u64; 4] {
    let (_, request) = resident(pool, 1);
    let bytes = page_of(pool, 1);
    let root = page::decode_page(&bytes[..], request.identity, PageKind::RetentionRoot).unwrap();
    let cell = root.cell(0).unwrap();
    ROOT_COUNTS.map(|at| u64_at(cell, at))
}

/// The commitment refusal: Before Unchanged (Prepare, Frame Stale).
fn stale(error: &RetentionError) -> bool {
    (error.phase, error.effect) == (RetentionPhase::Prepare, Effect::Unchanged)
        && matches!(error.cause, RetentionCause::Frame(frame)
            if frame.cause == dirty_frame::Cause::Stale)
}

/// The author's files agree with the literals above: expected.json states
/// the RowReserve frame and Job cells, and expected-cancel.json (indent 2,
/// sorted keys) states each Cancel fact in its own object.
fn author_literal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR);
    let reserve = String::from_utf8(read(&root.join("expected.json"), 1 << 16)).unwrap();
    for fact in [
        format!("\"frame_bytes\": {}", RESERVE_FRAME.0),
        format!("\"frame_sha256\": \"{}\"", RESERVE_FRAME.1),
        format!("\"bank\": \"{RESERVED_JOBS}\""),
        format!("\"undo14\": \"{UNDO14}\""),
        format!("\"lsn\": {}", PREVIOUS_LSN + 1),
    ] {
        assert!(reserve.contains(&fact), "expected.json lacks {fact}");
    }
    let cancel = String::from_utf8(read(&root.join("expected-cancel.json"), 1 << 16)).unwrap();
    let counts = |name: &str, [reserved, cancelled, sequence, row]: [u64; 4]| {
        format!(
            "  \"{name}\": {{\n    \"cancelled_count\": {cancelled},\n    \
             \"reserved_count\": {reserved},\n    \"row_high_water\": {row},\n    \
             \"sequence_high_water\": {sequence},\n    \"undo14\": \"{UNDO14}\"\n  }}"
        )
    };
    let after = CANCELLED
        .iter()
        .map(|(name, digest)| format!("    \"{name}\": \"{digest}\""))
        .collect::<Vec<_>>()
        .join(",\n");
    for fact in [
        "\"case\": \"S2W-R08\"".to_string(),
        format!("\"frame_bytes\": {}", CANCEL_FRAME.0),
        format!("\"frame_sha256\": \"{}\"", CANCEL_FRAME.1),
        format!("\"lsn\": {}", PREVIOUS_LSN + 2),
        counts("after_reserve", ROOT_RESERVED),
        counts("after_cancel", ROOT_CANCELLED),
        format!("  \"after_sha256\": {{\n{after}\n  }}"),
        format!(
            "  \"root_job\": {{\n    \"generation\": {},\n    \"ordinal\": {},\n    \
             \"state\": {}\n  }}",
            ROOT_JOB.2, ROOT_JOB.0, ROOT_JOB.1
        ),
    ] {
        assert!(cancel.contains(&fact), "expected-cancel.json lacks {fact}");
    }
}

/// The per-frame body of `replay_durable` (the genesis seed, then
/// `observe_recovery_frame` for every frame of the WAL file) over the whole
/// durable WAL. Returns the replay and the last frame's LSN.
fn durable_replay(state: &State) -> (Box<[u8; RETENTION_REPLAY_BYTES]>, u64) {
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
    let mut at = 4096;
    let mut previous = 0;
    while at < wal.len() {
        let length = u32::from_le_bytes(wal[at + 12..at + 16].try_into().unwrap()) as usize;
        let expected = FrameExpectation {
            stream,
            previous_lsn: previous,
        };
        let observed = observe_recovery_frame(
            profile,
            expected,
            &wal[at..at + length],
            &mut durable,
            &mut cursor,
        )
        .unwrap_or_else(|error| panic!("frame at {at}: {error:?}"));
        previous = observed.header.lsn;
        at += length;
    }
    assert_eq!(at, wal.len());
    (durable, previous)
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

/// S2W-R08: Reserve→Cancel. The Cancel frame is the WAL's one new frame
/// after the RowReserve and equals the independent literal (bytes and
/// SHA-256, LSN 16). The pool's Root 1 and Jobs 2 and the control's Job
/// cells, ATT row and Aging row equal the literal's; the RootJob is Free at
/// its generation. Nothing the Reserve issued is returned: the Root keeps its
/// RowId and Undo sequence high-water and its reserved count, and the Free
/// Undo 14 keeps the image the Reserve left. The table's write bit stays,
/// the admission, the gate and the lease are returned with no fence, and the
/// loan comes back once, the same loan. The actor is no longer Reserved: its
/// statement ends and the transaction aborts empty with no fence.
#[test]
fn s2w_r08_cancel_returns_nothing_issued_and_keeps_the_write_bit() {
    case("S2W-R08", || {
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
                let reserved = reserved!(preflight.reserve(input));
                let reserve = wal_since(state, wal);
                assert_eq!((reserve.len(), hex(&reserve).as_str()), RESERVE_FRAME);
                let (actor, root_job, row_id, undo_ref, _) = reserved.test_issued();
                assert_eq!(root_job.ordinal().unwrap(), ROOT_JOB.0);
                let reserved_root = root_counts(pool);
                let reserved_undo = hex(&page_of(pool, FREE_UNDO)[..]);
                assert_eq!(reserved_root, ROOT_RESERVED);
                assert_eq!(reserved_undo, UNDO14);
                let (wal, _) = files(state);
                let returned = cancelled!(reserved.cancel());
                assert_eq!(returned.as_ptr(), loan);
                let frame = wal_since(state, wal);
                let cancelled_root = root_counts(pool);
                println!(
                    "S2W_R08 issued=(row={} undo=({},{},{}) root_job=({},{},{})) \
                     frame_bytes={} frame_sha={} root_counts reserve={reserved_root:?} \
                     cancel={cancelled_root:?} masks={:?} gate_closed={} source_used={}",
                    row_id.get(),
                    undo_ref.page().page_no,
                    undo_ref.slot(),
                    undo_ref.generation().get(),
                    root_job.page().page_no,
                    root_job.slot(),
                    root_job.generation().get(),
                    frame.len(),
                    hex(&frame),
                    view.test_table_masks(),
                    hook::observe(core).read_gate_closed,
                    reader.catalog_test_source_used() - used,
                );
                assert_eq!((frame.len(), hex(&frame).as_str()), CANCEL_FRAME);
                assert_eq!(
                    writer.free_undo_test_state().unwrap().0.complete_lsn,
                    PREVIOUS_LSN + 2
                );
                assert_eq!(cancelled_root, ROOT_CANCELLED);
                assert_eq!(hex(&page_of(pool, FREE_UNDO)[..]), UNDO14);
                let jobs = writer.catalog_test_jobs();
                let job = &jobs[ROOT_JOB.0];
                assert_eq!((job[6], u64_at(job, 8)), (ROOT_JOB.1, ROOT_JOB.2));
                let decoded = job3::decode_job(job, namespace()).unwrap();
                assert_eq!(decoded.state(), JobState::Free);
                assert_eq!(decoded.source(), None);
                let bank = writer.recovery_test_bank();
                let after = [
                    hex(&bank[AGING..AGING + 1536]),
                    hex(jobs.as_flattened()),
                    hex(&page_of(pool, 2)[..]),
                    hex(&page_of(pool, 1)[..]),
                    hex(&bank[..1536]),
                ];
                for ((name, pin), live) in CANCELLED.iter().zip(&after) {
                    assert_eq!(live, pin, "after {name}");
                }
                assert_eq!(Some(actor), row.test_write_identity().0);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(!fence.is_fenced());
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R08 with a refusal before the stage other than Stale (the refusal
/// table's "cancel의 stage 전 거절 → Before(같은 wrapper)"): a second
/// Foreground owner holds a target-set read of the Root's frame, so the
/// install's write acquisition of the Cancel's targets is refused Unchanged
/// (Install, Frame Busy) before the stage. The cancel is Before on the same
/// ReservedRow (same
/// issued identity) with no WAL, file, page (every page of T with its stamp)
/// or fence change, the gate still closed and the admission still charged.
/// Once the read is returned, the retried cancel on that ReservedRow lands
/// the literal's Cancel frame and Root, returns the same loan once and leaves
/// no fence.
#[test]
fn s2w_r08_busy_target_cancel_is_before_then_retry_lands_the_literal() {
    case("S2W-R08", || {
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
                let loan = private.as_ptr();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let reserved = reserved!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let effects = files(state);
                let frames = t_frames(pool);
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                let (root, _) = resident(pool, 1);
                let (error, owned) =
                    with_pins(pool, &mut holder, &[root], || before!(reserved.cancel()));
                println!("S2W_R08 busy_root_read cancel={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Install, Effect::Unchanged)
                );
                assert!(!stale(&error));
                assert!(matches!(error.cause, RetentionCause::Frame(frame)
                    if frame.cause == dirty_frame::Cause::Busy));
                assert_eq!(owned.test_issued(), issued);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert_eq!(root_counts(pool), ROOT_RESERVED);
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert!(!fence.is_fenced());
                holder.check_idle().unwrap();
                drop(holder);
                let returned = cancelled!(owned.cancel());
                assert_eq!(returned.as_ptr(), loan);
                let frame = wal_since(state, effects.0);
                println!(
                    "S2W_R08 retry frame_bytes={} frame_sha={}",
                    frame.len(),
                    hex(&frame)
                );
                assert_eq!((frame.len(), hex(&frame).as_str()), CANCEL_FRAME);
                assert_eq!(root_counts(pool), ROOT_CANCELLED);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(!fence.is_fenced());
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// The Cancel that ends a reservation after a refused Forward step lands
/// the literal's Cancel frame (LSN 16) and Root counts, returns the same loan
/// once, keeps the table's write bit, returns the admission charge, reopens
/// the gate and leaves no fence (S2W-R10, R11 and R21(1)'s cancel ends).
macro_rules! cancel_lands {
    ($reserved:expr, $case:literal, $state:ident, $from:expr, $pool:ident, $loan:ident,
     $view:ident, $slot:ident, $core:ident, $reader:ident, $used:ident, $fence:ident) => {{
        let returned = cancelled!($reserved.cancel());
        assert_eq!(returned.as_ptr(), $loan);
        let frame = wal_since($state, $from);
        println!(
            "{} cancel frame_bytes={} frame_sha={}",
            $case,
            frame.len(),
            hex(&frame)
        );
        assert_eq!((frame.len(), hex(&frame).as_str()), CANCEL_FRAME);
        assert_eq!(root_counts($pool), ROOT_CANCELLED);
        assert_eq!($view.test_table_masks(), (1 << $slot, 1 << $slot));
        assert!(!hook::observe($core).read_gate_closed);
        assert_eq!($reader.catalog_test_source_used(), $used);
        assert!(!$fence.is_fenced());
    }};
}

/// S2W-R10 (cancel end): A's proposal tampered in the three forms the
/// contract row names (one byte of A's frame and of A's first after page,
/// written in the loan after A's produce, and the physical roles swapped) is
/// Before Unchanged (Prepare, RowSemantic), not Stale, on the same
/// ReservedRow (issued identity and draft cut), with no WAL, file, page
/// (every page of T with its stamp) or fence change, the gate still closed
/// and the admission still charged. The cancel that follows on that
/// ReservedRow lands the literal's Cancel. The declared-metadata forms and
/// the Forward end are S2-c's
/// (`row_forward::s2w_r10_tampered_proposal_is_before_on_the_same_reservation`).
#[test]
fn s2w_r10_tampered_proposal_is_before_then_cancel_lands_the_literal() {
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
                let (_, slot, _) = table.test_identity();
                let pool = reader.catalog_test_pool();
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let loan = private.as_ptr();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = reserved!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let draft = reserved.test_draft();
                let effects = files(state);
                let frames = t_frames(pool);
                let mut swapped = ORDER;
                swapped.swap(0, 1);
                let mut refusals = Vec::new();
                for name in ["frame_byte", "after_byte", "roles"] {
                    let proposal = produce!(reserved);
                    let proposal = if name == "roles" {
                        RowProposal::new(RowOperation::Forward, FORWARD_FRAME.0, 5, swapped)
                    } else {
                        let (_, _, _, _, frame, after, ..) =
                            reserved.producer_lane().unwrap().parts();
                        if name == "frame_byte" {
                            frame[FORWARD_FRAME.0 / 2] ^= 1;
                        } else {
                            after[0][PAGE_BYTES / 2] ^= 1;
                        }
                        proposal
                    };
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
                    assert!(
                        matches!(error.cause, RetentionCause::RowSemantic(_)),
                        "{name}"
                    );
                }
                assert_eq!(reserved.test_issued(), issued);
                assert_eq!(reserved.test_draft(), draft);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert!(!fence.is_fenced());
                cancel_lands!(
                    reserved, "S2W_R10", state, effects.0, pool, loan, view, slot, core, reader,
                    used, fence
                );
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R11 (cancel end): a refusal before apply's stage (S2-c's one-shot
/// cfg(test) refusal after the table write bit and before the install) is
/// Before Unchanged (Stage, Busy) on the same PreparedRow; `reopen` returns
/// the same ReservedRow (issued identity and draft cut), with no WAL, file,
/// page or fence change, the write bit kept, the statement's revision
/// unmoved and the gate still closed. The cancel on that ReservedRow lands
/// the literal's Cancel and leaves the revision where it was. The Forward
/// end is S2-c's
/// (`row_forward::s2w_r11_apply_stage_refusal_reopens_the_same_reservation`).
#[test]
fn s2w_r11_apply_stage_refusal_reopens_then_cancel_lands_the_literal() {
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
                let loan = private.as_ptr();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = reserved!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let draft = reserved.test_draft();
                let proposal = produce!(reserved);
                let prepared = prepared!(reserved.prepare(proposal));
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
                let reserved = prepared.reopen();
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
                cancel_lands!(
                    reserved, "S2W_R11", state, effects.0, pool, loan, view, slot, core, reader,
                    used, fence
                );
                assert_eq!(view.revision(), revision);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R21(1) (cancel end): one source stamp shown changed once to apply's
/// recompute before its stage (S2-c's cfg(test) one-shot of the shared
/// kind-2 recompute, armed right before apply) makes apply Before Unchanged
/// (Prepare, Frame Stale) on the same PreparedRow, with no WAL, file, page or
/// issuer change. A reopens and cancels, and the cancel succeeds on the same
/// reservation: it lands the literal's Cancel and issues no transaction. The
/// Forward end is S2-c's
/// (`row_forward::s2w_r21_one_stale_stamp_at_apply_is_before_then_reopen`).
#[test]
fn s2w_r21_one_stale_stamp_at_apply_is_before_then_cancel_lands_the_literal() {
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
                let pool = reader.catalog_test_pool();
                let used = reader.catalog_test_source_used();
                let locals = writer.row_test_locals();
                let loan = private.as_ptr();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let mut reserved = reserved!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let proposal = produce!(reserved);
                let prepared = prepared!(reserved.prepare(proposal));
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
                let reserved = prepared.reopen();
                assert_eq!(reserved.test_issued(), issued);
                cancel_lands!(
                    reserved, "S2W_R21", state, effects.0, pool, loan, view, slot, core, reader,
                    used, fence
                );
                // Neither the refused apply nor the Cancel issues a
                // transaction.
                assert_eq!(writer.row_test_locals(), locals);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R12 (cancel half, `write`): a WAL write fault after the Cancel's
/// stage is Fatal (Durable, Wal), CallerMustResolve, with the fence and no
/// frame written. The gate is reopened and the lease ends; the admission's
/// checked release is refused by the fenced ledger, which keeps its charge
/// visible (no manufactured capacity). The reservation, still active, fences
/// again in its Drop, and the write bit stays.
#[test]
fn s2w_r12_cancel_write_fault_after_stage_is_fatal() {
    case("S2W-R12", || r12(false));
}

/// S2W-R12 (cancel half, `sync`): the same at the durable wait, after the
/// Cancel frame is written.
#[test]
fn s2w_r12_cancel_sync_fault_after_stage_is_fatal() {
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
            let reserved = reserved!(preflight.reserve(input));
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
            let error = match reserved.cancel() {
                Err(RowOwnedError::Fatal(error)) => error,
                Err(RowOwnedError::Before { error, .. }) => panic!("Before {error:?}"),
                Ok(_) => panic!("a failed cancel"),
            };
            let (end, _) = files(state);
            println!(
                "S2W_R12 cancel sync={sync} refusal={error:?} wal_gained={} gate_closed={} \
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
            // The admission's checked release is refused by the fenced
            // ledger, which keeps the Fatal bookkeeping visible.
            assert_eq!(reader.catalog_test_source_used(), used + charge);
            assert_eq!(view.test_table_masks().1, 1 << slot);
            assert_eq!(end - start, if sync { CANCEL_FRAME.0 } else { 0 });
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

/// S2W-R15 (the cancel end): a reader admitted before the preflight holds
/// the preflight at its read gate until it returns; while the ReservedRow is
/// held a new read operation is Unchanged InstallationPending. The Cancel
/// ends the reservation without the fence: the gate is open again, a new
/// read operation on the same core is admitted and returns, and the
/// transaction then aborts empty.
#[test]
fn s2w_r15_cancel_ends_the_reservation_without_the_fence() {
    case("S2W-R15", || {
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
                let (wal, _) = files(state);
                let (admitted, admission) = sync_channel(1);
                let (entered, waiting) = sync_channel(1);
                let (advance, release) = sync_channel(1);
                hook::install(core, Point::ReadGateWaiting, entered, release);
                std::thread::scope(|scope| {
                    let reading = scope.spawn(move || {
                        // The !Send Owner stays on this thread.
                        let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                        let operation = late.try_read_operation(core).unwrap();
                        admitted.send(()).unwrap();
                        waiting.recv_timeout(WAIT).unwrap();
                        let observed = hook::observe(core);
                        operation.finish_checked().unwrap();
                        advance.send(Resume::Continue).unwrap();
                        late.check_idle().unwrap();
                        observed
                    });
                    admission.recv_timeout(WAIT).unwrap();
                    let mut preflight = ready!(writer, owner, view, table, private, flush);
                    let waited = reading.join().unwrap();
                    assert!(waited.read_gate_closed);
                    assert_eq!(waited.active_read_operations, 1);
                    let input = plan!(preflight);
                    let reserved = reserved!(preflight.reserve(input));
                    let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                    let refused = late.try_read_operation(core).err().unwrap();
                    assert_eq!(refused.cause, concurrency::Cause::InstallationPending);
                    assert!(matches!(refused.effect, Effect::Unchanged));
                    let _ = cancelled!(reserved.cancel());
                    let observed = hook::observe(core);
                    let operation = late.try_read_operation(core).unwrap();
                    let reading = hook::observe(core);
                    println!(
                        "S2W_R15 at_gate_wait={waited:?} reserved_reader={refused:?} \
                         cancelled={observed:?} reader_after_cancel={reading:?}"
                    );
                    assert!(!observed.read_gate_closed);
                    assert!(!observed.fenced);
                    assert_eq!(reading.active_read_operations, 1);
                    operation.finish_checked().unwrap();
                    late.check_idle().unwrap();
                    drop(late);
                    assert_eq!(
                        wal_since(state, wal).len(),
                        RESERVE_FRAME.0 + CANCEL_FRAME.0
                    );
                    assert!(!fence.is_fenced());
                });
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R18 over a live reservation: the actor's RootJob is RootReserved in
/// the pool and the control because a ReservedRow was given up with its
/// reservation kept (`test_release_keeping_reservation`, a fixture: no
/// healthy session reaches it, since the reservation is a durable obligation
/// only a Cancel or an apply discharges). The next preflight is Unchanged
/// (Prepare, WrongState) after the gate and returns the source, then the
/// gate, then the lease, with no WAL, file, mask or fence change.
#[test]
fn s2w_r18_live_root_reserved_actor_is_wrong_state() {
    case("S2W-R18", || {
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
                let used = reader.catalog_test_source_used();
                let loan = private.as_ptr();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let reserved = reserved!(preflight.reserve(input));
                let (actor, root_job, ..) = reserved.test_issued();
                let returned = reserved.test_release_keeping_reservation().unwrap();
                assert_eq!(returned.as_ptr(), loan);
                assert!(!fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                let job = {
                    let job = job3::decode_job(
                        &writer.catalog_test_jobs()[root_job.ordinal().unwrap()],
                        namespace(),
                    )
                    .unwrap();
                    (job.state(), job.source())
                };
                assert_eq!(job, (JobState::RootReserved, Some(actor)));
                let masks = view.test_table_masks();
                let effects = files(state);
                let error = writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .err()
                    .unwrap();
                println!(
                    "S2W_R18 live_reserved job=({:?},{}) refusal={error:?} masks={masks:?}",
                    job.0,
                    root_job.ordinal().unwrap()
                );
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::WrongState));
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(view.test_table_masks(), masks);
                assert_eq!(files(state), effects);
                assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                assert!(!fence.is_fenced());
                // The fixture's state has no healthy end: the statement end
                // refuses the Reserved actor the same way (Before WrongState,
                // `row_statement_head`), and the dropped view fences.
                let view = match writer.finish_statement(owner, view) {
                    Err(StatementFinishError::Before { error, view }) => {
                        assert_eq!(
                            (error.phase, error.effect),
                            (RetentionPhase::Finalize, Effect::Unchanged)
                        );
                        assert!(matches!(error.cause, RetentionCause::WrongState));
                        view
                    }
                    Err(StatementFinishError::Fatal(error)) => panic!("Fatal {error:?}"),
                    Ok(()) => panic!("a Reserved actor's statement end"),
                };
                assert!(!fence.is_fenced());
                drop(view);
                assert!(fence.is_fenced());
                drop(row);
            }
        );
    });
}

/// S2W-R22 (cancel half): with every target resident, a preflight alone
/// checks the replay out once; a preflight, `validation_lane`, reserve and
/// cancel together check it out the same once, so the cancel opens no replay
/// (S2-0b decision 1). Scopes are counted the same way.
#[test]
fn s2w_r22_cancel_checks_out_no_replay() {
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
                let reserved = reserved!(preflight.reserve(input));
                let _ = cancelled!(reserved.cancel());
                let through_cancel = writer.row_test_replay_counts();
                println!(
                    "S2W_R22 preflight scopes={} checkouts={} \
                     preflight_lane_reserve_cancel scopes={} checkouts={}",
                    preflight_only.0 - start.0,
                    preflight_only.1 - start.1,
                    through_cancel.0 - preflight_only.0,
                    through_cancel.1 - preflight_only.1
                );
                assert_eq!(preflight_only.1 - start.1, 1);
                assert_eq!(through_cancel.1 - preflight_only.1, 1);
                assert_eq!(
                    through_cancel.0 - preflight_only.0,
                    preflight_only.0 - start.0
                );
                assert!(!fence.is_fenced());
                let _ = reader;
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R23(2) (cancel half): after the Reserve's install, one control byte
/// (`XOI_BYTE`) changed with the draft commitment not folded again. The
/// cancel is Before Unchanged (Prepare, Frame Stale) on the same
/// ReservedRow, with no WAL, file, page or issuer change (every page of T in
/// the pool with its stamp, the transaction issuer's locals, the Root's
/// high-waters, the Job cells the Reserve left); the wrapper keeps its
/// admission charge. That ReservedRow's Drop leaves the fence, and the
/// fenced ledger keeps the charge visible (§1 gate: an original Fatal keeps
/// its charge under the fence).
#[test]
fn s2w_r23_control_byte_after_reserve_is_stale_cancel_then_drop_fences() {
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
                let mut reserved = reserved!(preflight.reserve(input));
                let issued = reserved.test_issued();
                let effects = files(state);
                let frames = t_frames(pool);
                reserved.test_flip_control(XOI_BYTE);
                let (error, owned) = before!(reserved.cancel());
                println!(
                    "S2W_R23 control_byte={XOI_BYTE} cancel={error:?} t_pages={:?}",
                    frames.iter().map(|frame| frame.0).collect::<Vec<_>>()
                );
                assert!(stale(&error));
                assert_eq!(owned.test_issued(), issued);
                assert_eq!(files(state), effects);
                for (before, after) in frames.iter().zip(t_frames(pool)) {
                    assert_eq!(*before, after, "page {}", before.0);
                }
                assert_eq!(root_counts(pool), ROOT_RESERVED);
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

/// S2W-H01..H05, the Reserve and Cancel parts: the original
/// `history::validate_live` after the Reserve (while the ReservedRow holds
/// it) and after the Cancel is live == replay, with the replayed ATT rows
/// and Job cells byte-equal to the live ones. The durable replay of the
/// whole WAL after the Cancel (`replay_durable`'s per-frame body from the
/// genesis seed) accepts every frame through the Cancel's LSN and equals the
/// live Job cells, ATT rows and the Root 1, Jobs 2 and Free Undo 14 pool
/// pages. A crash image in the Reserved state and one after the Cancel each
/// stop at the interim analysis gate with every file unchanged.
#[test]
fn s2w_h01_h05_reserve_and_cancel_replay_equal_live_and_stop_at_the_gate() {
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
                let mut reserved = reserved!(preflight.reserve(input));
                let at_reserve = live_replay!(reserved.test_live_replay());
                let reserved_image = state.lock().unwrap().images();
                let _ = cancelled!(reserved.cancel());
                let at_cancel = live_replay!(writer.row_test_live_replay(owner));
                let (durable, last) = durable_replay(state);
                println!(
                    "S2W_H01_H05 validate_live reserve=ok cancel=ok jobs_changed={} \
                     durable_frames_to={last}",
                    at_reserve[3] != at_cancel[3]
                );
                assert_eq!(last, PREVIOUS_LSN + 2);
                assert_ne!(at_reserve[3], at_cancel[3]);
                assert_eq!(
                    retention_replay_cells(&durable).as_flattened(),
                    &at_cancel[3][..]
                );
                assert_eq!(
                    &retention_replay_bank(&durable)[..at_cancel[2].len()],
                    &at_cancel[2][..]
                );
                for number in [1, 2, FREE_UNDO] {
                    assert_eq!(
                        retention_replay_image(&durable, number as usize),
                        &*page_of(pool, number),
                        "page {number}"
                    );
                }
                let cancelled_image = state.lock().unwrap().images();
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
                (reserved_image, cancelled_image)
            }
        );
        gated(&crashes.0);
        gated(&crashes.1);
    });
}
