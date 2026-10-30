//! S2-b2: `validation_lane` and RowReserve (S2-0 Row write amendment §1
//! "order" and §6, S2-0b `#row-write-validate-once`) over the actual
//! committed CREATE of table T that the preflight cases use. Only the cases
//! reserve can exercise are here: S2W-R04, R06, R07, R14, the reserve halves
//! of R05, R11, R15 and R22, and R23(1) with its preflight arena half (the
//! S2-b1 review's A1). The cases that need cancel, prepare or apply are later
//! units'. A ReservedRow has no supplied cancel yet, so every successful
//! reserve here ends with its Drop, which fences the session (S2W-R23(2)'s
//! Drop rule). The expected values are fixture facts, the independent
//! literal of `tests/fixtures/dev3-module-c3/row-write/` and the refusal
//! table's (phase, cause, effect), never values copied from the body.
use super::row_preflight::{
    FIXTURE_T, FREE_UNDO, create_table, fold_identity, fold_stamp, resident,
};
use super::*;
use crate::storage::{
    dirty_frame::{self, SharedPool, status_load_test_support},
    disk::{
        IsolationLevel, RetentionCause, RetentionError, RetentionPhase, RowInputPlan,
        RowOwnedError, RowPreflightStep, TableStep,
    },
    page::{self, PageKind},
    retention_page::JobSlotRef,
    row::insert::validate_insert,
    row_record::ValueRef,
    transaction_record::TxnRef,
    undo_ref::UndoRef,
};
use crate::transaction::concurrency::{
    self,
    catalog_test_support::{self as hook, Point, Resume},
};
use foundation::{codec::ContentHasher, identity::RowId};
use std::sync::mpsc::sync_channel;

/// The one value of T's row (`i`, Int64, not null).
const VALUE: i64 = 7;
/// The S2W-R06 author (stdlib Python that imports the pinned N1 author and
/// no product code) and its output, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c3/row-write";
/// SHA-256 of each live input the author re-encodes byte-equal before its
/// reserve: Root 1, Jobs 2, Status 11, RowLeaf 13 and the Free Undo 14 from
/// the pool, the 64 Job cells, the Row actor's ATT row (control slot 0) and
/// the Aging row, and the profile file.
const LIVE: [(&str, &str); 9] = [
    (
        "root1.bin",
        "700ee5b8e8ceb843789fad495a6bade5917c6d92ae21f3d2e4d2abdcd6b156bc",
    ),
    (
        "jobs2.bin",
        "a2aa825f6e3af848a07559ea6d15bc590825c28127e4862c80c13835d7b32312",
    ),
    (
        "status11.bin",
        "9d52d6d97261aec98b2050edba42173bbb385a56e90abb9ade352bb8a3e6c4b5",
    ),
    (
        "leaf13.bin",
        "30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396",
    ),
    (
        "undo14.bin",
        "7fe328b11b1d505fa2f35c0297e5cfcc369143ea7e8272298d564fe751774eab",
    ),
    (
        "bank.bin",
        "1f89dcdb2e244f6d2cbf33cf72a030df2daf18b72e8f7cd9b3a463de18382bdb",
    ),
    (
        "tx.bin",
        "20abdcb33ae7095e2c6a335b5f3a5020bd13f53668c2a8ab1ba8b4eefcf3c3a6",
    ),
    (
        "aging.bin",
        "7670867ac62398be66282adf9f8b670c73d56c571c700919a232f5d489f4b266",
    ),
    (
        "profile.bin",
        "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e",
    ),
];
/// The literal RowReserve frame (expected.json): its bytes and SHA-256.
const FRAME: (usize, &str) = (
    57_336,
    "6e3badc82e1864c405f846a05372e2367f6a266a10e0a4e0c8c3d28267870fbe",
);
/// SHA-256 of the literal's after images (Root 1, Jobs 2, the Free Undo 14)
/// and control (the 64 Job cells, the ATT row, the Aging row).
const AFTER: [(&str, &str); 6] = [
    (
        "root1",
        "b3d4f28c5b26dae538bfaec36e1ca8320ac868571ffd84482f0670f7fd5d8ba5",
    ),
    (
        "jobs2",
        "fabcd952b03412782bf444eceb5cc132ee75b43bdba6521b76046433e533bd1f",
    ),
    (
        "undo14",
        "b975e67e40072ca7edf708059869743acea9bd078e513cbd07d4e3d8e7d8de4e",
    ),
    (
        "bank",
        "e74d3a7218763061365b3261373e41506f2537f82fb07951000f8049e41438cf",
    ),
    (
        "tx",
        "024083b72dbc49f3e3f21cc147c3caf0b7ddd4a4d49b74aa31221cbb8e5a955d",
    ),
    (
        "aging",
        "001a88935dcabad5a90e9021e663cf8e3326f7f2a15fdbc779a573083aa2ce16",
    ),
];
/// SHA-256 of T's canonical row in N1's layout (column count 1, Int64 tag 2,
/// the value), which the author also writes.
const CANONICAL: &str = "fae2df78d78985674f9e83253255b062c3bbee012bc876f26e4342520ae04b38";
/// The Aging descriptor's control row (bank row 32 of 1664 bytes).
const AGING: usize = 32 * 1664;
/// The fixture's WAL complete LSN before the reserve; the RowReserve is the
/// next LSN.
const PREVIOUS_LSN: u64 = 14;
/// The literal's Undo sequence and planned mutation of the reserved row.
const SEQUENCE: u64 = 2;
const MUTATION: u64 = 1;

/// A refused reserve's error, with its PreflightRow back.
macro_rules! before {
    ($result:expr) => {
        match $result {
            Err(RowOwnedError::Before { error, owned }) => (error, owned),
            Err(RowOwnedError::Fatal(error)) => panic!("Fatal {error:?}"),
            Ok(_) => panic!("a refused reserve"),
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

/// (WAL file length, every file image) of the directory: an effect is any
/// change of either.
fn files(state: &State) -> (usize, Images) {
    let state_guard = state.lock().unwrap();
    let wal = state_guard.files["d3-wal-00000001"].bytes.len();
    drop(state_guard);
    (wal, state.lock().unwrap().images())
}

/// The bytes the WAL file gained from `from` on.
fn wal_since(state: &State, from: usize) -> Vec<u8> {
    state.lock().unwrap().files["d3-wal-00000001"].bytes[from..].to_vec()
}

/// A digest as lowercase hex (`hex` is the SHA-256 of its input).
fn raw(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A copy of one resident pool page.
fn page_of(pool: &SharedPool<'_, '_, '_, '_>, number: u64) -> Box<[u8; PAGE_BYTES]> {
    let (index, _) = resident(pool, number);
    let mut bytes = boxed::<PAGE_BYTES>();
    probe::diagnose(pool, index, &mut bytes);
    bytes
}

/// One byte (the middle of the page) of a clean resident page changed in its
/// frame, with no stamp change (S2W-R23(1)).
fn flip(pool: &SharedPool<'_, '_, '_, '_>, number: u64) {
    let (index, _) = resident(pool, number);
    status_load_test_support::edit_content(pool, index, |bytes| bytes[PAGE_BYTES / 2] ^= 1);
}

/// The commitment refusal: Before Unchanged (Prepare, Frame Stale).
fn stale(error: &RetentionError) -> bool {
    (error.phase, error.effect) == (RetentionPhase::Prepare, Effect::Unchanged)
        && matches!(error.cause, RetentionCause::Frame(frame)
            if frame.cause == dirty_frame::Cause::Stale)
}

/// The author's files agree with the literals above: expected.json states
/// each of them, and each live input file has its pin (the author checks the
/// same pins before it re-encodes them).
fn author_literal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR);
    let expected = String::from_utf8(read(&root.join("expected.json"), 1 << 16)).unwrap();
    let mut facts = vec![
        format!("\"frame_bytes\": {}", FRAME.0),
        format!("\"frame_sha256\": \"{}\"", FRAME.1),
        format!("\"canonical_sha256\": \"{CANONICAL}\""),
        format!("\"lsn\": {}", PREVIOUS_LSN + 1),
        "\"ordinal\": 3".to_string(),
        "\"row\": 1".to_string(),
        format!("\"sequence\": {SEQUENCE}"),
        format!("\"mutation\": {MUTATION}"),
    ];
    facts.extend(
        AFTER
            .iter()
            .map(|(name, digest)| format!("\"{name}\": \"{digest}\"")),
    );
    for fact in facts {
        assert!(expected.contains(&fact), "expected.json lacks {fact}");
    }
    for (name, digest) in LIVE {
        assert_eq!(
            hex(&read(&root.join("live").join(name), 1 << 17)),
            digest,
            "{name}"
        );
    }
}

/// One pool page of T: its registration, bytes and stamp.
type PoolTarget = (
    crate::storage::dirty_frame::TargetRequest,
    Box<[u8; PAGE_BYTES]>,
    crate::storage::dirty_frame::FrameStamp,
);

/// T (`FIXTURE_T`, the before order) as the pool holds it now.
fn pool_t(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<PoolTarget> {
    FIXTURE_T
        .iter()
        .map(|number| {
            let (index, request) = resident(pool, *number);
            let mut bytes = boxed::<PAGE_BYTES>();
            let snapshot = probe::diagnose(pool, index, &mut bytes);
            assert_eq!(snapshot.frame.identity, request.identity);
            (request, bytes, snapshot.frame)
        })
        .collect()
}

/// The XRWC kind-2 (draft) commitment of the fixture's reserved row,
/// recomputed from independent reads in the order the draft cut doc gives
/// (`RowDraftCut::source`, S2-b2 review A8): the prefix after the
/// RowReserve; T (`FIXTURE_T`, the before order) with each identity, kind,
/// level and stamp from the pool; the role map; the actor (48 B), the
/// RootJob slot (40 B), the RowId, the UndoRef (40 B), the literal's
/// sequence and planned mutation; the actor's ATT row (slot 0), then the
/// Aging presence byte and descriptor; the 64 Jobs rows; the page digests of
/// T from the pool; the control's bank, Jobs and inventory digests and its
/// issued and adopted locals. `pages` is `pool_t`'s read, taken while the
/// session is not fenced.
fn xrwc_draft(
    pages: &[PoolTarget],
    prefix: crate::storage::wal_io::WalPrefix,
    (control, (issued, adopted)): (&[u8], (u64, u64)),
    roles: u64,
    (actor, root_job, row, undo): (TxnRef, JobSlotRef, RowId, UndoRef),
) -> [u8; 32] {
    let mut hasher = ContentHasher::new();
    hasher.update(b"XRWC");
    hasher.update(&[1, 2]);
    hasher.update(&prefix.stream.database.bytes());
    hasher.update(&prefix.stream.timeline.to_le_bytes());
    hasher.update(&prefix.stream.writer_epoch.to_le_bytes());
    hasher.update(&prefix.file.device.to_le_bytes());
    hasher.update(&prefix.file.inode.to_le_bytes());
    hasher.update(&prefix.complete_lsn.to_le_bytes());
    hasher.update(&prefix.complete_offset.to_le_bytes());
    for (request, _, stamp) in pages {
        fold_identity(&mut hasher, request.identity);
        hasher.update(&[request.kind as u8, request.level]);
        fold_stamp(&mut hasher, stamp);
    }
    hasher.update(&roles.to_le_bytes());
    let mut actor_bytes = [0; 48];
    actor.encode(&mut actor_bytes).unwrap();
    hasher.update(&actor_bytes);
    let mut reference = [0; 40];
    root_job.encode(&mut reference).unwrap();
    hasher.update(&reference);
    hasher.update(&row.get().to_le_bytes());
    undo.encode(&mut reference).unwrap();
    hasher.update(&reference);
    hasher.update(&SEQUENCE.to_le_bytes());
    hasher.update(&MUTATION.to_le_bytes());
    hasher.update(&control[..1536]);
    // The Aging row's phase byte (1624 of its 1664) is its presence.
    if control[AGING + 1624] == 0 {
        hasher.update(&[0]);
    } else {
        hasher.update(&[1]);
        hasher.update(&control[AGING..AGING + 1536]);
    }
    hasher.update(&control[65_536..131_072]);
    for (_, bytes, _) in pages {
        hasher.update(&content_digest(&bytes[..]));
    }
    hasher.update(&content_digest(&control[..65_536]));
    hasher.update(&content_digest(&control[65_536..131_072]));
    hasher.update(&content_digest(&control[131_072..147_456]));
    hasher.update(&issued.to_le_bytes());
    hasher.update(&adopted.to_le_bytes());
    hasher.digest()
}

/// S2W-R04: a forged candidate plan (its digest, its length or its schema
/// epoch) is Before Unchanged (Prepare, RowSemantic InvalidInput) on the
/// same PreflightRow, registers no write bit and changes no WAL or file.
/// The real plan is accepted by the same checks: the PreflightRow then
/// finishes with no effect.
#[test]
fn s2w_r04_forged_candidate_is_before_with_no_effect() {
    case("S2W-R04", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (handle, slot, _) = table.test_identity();
                let effects = files(state);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let real = plan!(preflight);
                let (length, epoch, digest) = real.test_parts();
                println!(
                    "S2W_R04 plan length={length} epoch={} digest={}",
                    epoch.get(),
                    raw(&digest)
                );
                let mut forged_digest = digest;
                forged_digest[0] ^= 1;
                let next_epoch = foundation::identity::SchemaEpoch::new(epoch.get() + 1).unwrap();
                let forged = [
                    ("digest", RowInputPlan::new(length, epoch, forged_digest)),
                    ("length", RowInputPlan::new(length + 1, epoch, digest)),
                    ("short", RowInputPlan::new(4, epoch, digest)),
                    ("epoch", RowInputPlan::new(length, next_epoch, digest)),
                ];
                assert_eq!(epoch, handle.schema_epoch);
                for (name, plan) in forged {
                    let (error, owned) = before!(preflight.reserve(plan));
                    preflight = owned;
                    println!("S2W_R04 forged={name} refusal={error:?}");
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged)
                    );
                    assert!(matches!(
                        error.cause,
                        RetentionCause::RowSemantic(
                            crate::storage::semantic_row::Error::InvalidInput
                        )
                    ));
                    assert_eq!(view.test_table_masks().1, 0);
                    assert_eq!(files(state), effects);
                    assert!(!fence.is_fenced());
                }
                preflight.finish().unwrap();
                assert_eq!(files(state), effects);
                assert_eq!(view.test_table_masks(), (1 << slot, 0));
                let _ = reader;
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R05 (reserve half): after a Ready preflight the Free Undo 14's slot0
/// is made not Free in its resident frame (S2W-R05's slot0 corruption, no
/// stamp change). Reserve's commitment recompute sees the change before its
/// Free Undo recheck, so it is Before Unchanged (Prepare, Frame Stale) on the
/// same PreflightRow with no write bit, WAL or file change, and `finish`
/// returns the source, the gate and the lease.
#[test]
fn s2w_r05_reserve_recompute_change_is_before() {
    case("S2W-R05", || {
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
                let effects = files(state);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let (index, request) = resident(pool, FREE_UNDO);
                status_load_test_support::edit_content(pool, index, |bytes| {
                    let page = page::decode_page(bytes, request.identity, PageKind::Undo).unwrap();
                    let header = page.header();
                    let mut cell = page.cell(0).unwrap().to_vec();
                    cell[6] = 1;
                    page::encode_page(header, &[&cell], bytes).unwrap();
                });
                let (error, owned) = before!(preflight.reserve(input));
                println!("S2W_R05 reserve_half refusal={error:?}");
                assert!(stale(&error));
                assert_eq!(view.test_table_masks().1, 0);
                assert_eq!(files(state), effects);
                assert!(!fence.is_fenced());
                owned.finish().unwrap();
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(files(state), effects);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R06: a successful reserve. Every live input of the independent
/// literal has its pin; the RowReserve frame is the WAL's one new frame and
/// equals the literal (bytes and SHA-256); the table slot's write bit is
/// registered; the issued identity is the literal's (RootJob ordinal 3 =
/// Jobs page 2 slot 3 generation 2, RowId 1, UndoRef 14/0 generation 2,
/// LSN 15) and the actor is the transaction's; the draft cut is the next
/// step's (complete LSN 15, planned mutation 1, the preflight's role map, the
/// 13-byte canonical row of the plan, operation 0). The pool's after images
/// and the control's Job cells, ATT row and Aging row equal the literal's,
/// and the draft commitment equals its independent recompute from the pool
/// and the control (`xrwc_draft`, S2-b2 review A8).
#[test]
fn s2w_r06_reserve_installs_the_row_reserve_group() {
    case("S2W-R06", || {
        author_literal();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let (cut, _) = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let pool = reader.catalog_test_pool();
                let inputs = {
                    let bank = writer.recovery_test_bank();
                    [
                        hex(&page_of(pool, 1)[..]),
                        hex(&page_of(pool, 2)[..]),
                        hex(&page_of(pool, 11)[..]),
                        hex(&page_of(pool, 13)[..]),
                        hex(&page_of(pool, FREE_UNDO)[..]),
                        hex(writer.catalog_test_jobs().as_flattened()),
                        hex(&bank[..1536]),
                        hex(&bank[AGING..AGING + 1536]),
                        hex(&state.lock().unwrap().files["d3-profile"].bytes),
                    ]
                };
                for ((name, pin), live) in LIVE.iter().zip(&inputs) {
                    assert_eq!(live, pin, "live input {name}");
                }
                let prefix = writer.free_undo_test_state().unwrap().0;
                assert_eq!(prefix.complete_lsn, PREVIOUS_LSN);
                let (wal, _) = files(state);
                assert_eq!(prefix.complete_offset, wal as u64);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let (length, _, digest) = input.test_parts();
                let reserved = reserved!(preflight.reserve(input));
                let frame = wal_since(state, wal);
                let (actor, root_job, row_id, undo_ref, lsn) = reserved.test_issued();
                let draft = reserved.test_draft();
                println!(
                    "S2W_R06 frame_bytes={} frame_sha={} lsn={lsn} root_job=({},{},{}) row={} \
                     undo=({},{},{}) masks={:?} plan=({length},{}) draft=(source={} input={} \
                     complete_lsn={} mutation={} roles={:#x} input_len={} operation={})",
                    frame.len(),
                    hex(&frame),
                    root_job.page().page_no,
                    root_job.slot(),
                    root_job.generation().get(),
                    row_id.get(),
                    undo_ref.page().page_no,
                    undo_ref.slot(),
                    undo_ref.generation().get(),
                    view.test_table_masks(),
                    raw(&digest),
                    raw(&draft.0),
                    raw(&draft.1),
                    draft.2,
                    draft.3,
                    draft.4,
                    draft.5,
                    draft.6,
                );
                assert_eq!((frame.len(), hex(&frame).as_str()), FRAME);
                assert_eq!(lsn, PREVIOUS_LSN + 1);
                assert_eq!(Some(actor), row.test_write_identity().0);
                assert_eq!(root_job.ordinal().unwrap(), 3);
                assert_eq!(
                    (
                        root_job.page().page_no,
                        root_job.slot(),
                        root_job.generation().get()
                    ),
                    (2, 3, 2)
                );
                assert_eq!(row_id.get(), 1);
                assert_eq!(
                    (
                        undo_ref.page().page_no,
                        undo_ref.slot(),
                        undo_ref.generation().get()
                    ),
                    (FREE_UNDO, 0, 2)
                );
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert_eq!((length, raw(&digest).as_str()), (13, CANONICAL));
                assert_eq!(
                    (draft.2, draft.3, draft.4, draft.5, draft.6),
                    (PREVIOUS_LSN + 1, 1, cut.2, 13, 0)
                );
                let pages = [
                    hex(&page_of(pool, 1)[..]),
                    hex(&page_of(pool, 2)[..]),
                    hex(&page_of(pool, FREE_UNDO)[..]),
                ];
                let t = pool_t(pool);
                assert!(!fence.is_fenced());
                drop(reserved);
                assert!(fence.is_fenced());
                let bank = writer.recovery_test_bank();
                let after = [
                    &pages[0],
                    &pages[1],
                    &pages[2],
                    &hex(writer.catalog_test_jobs().as_flattened()),
                    &hex(&bank[..1536]),
                    &hex(&bank[AGING..AGING + 1536]),
                ];
                for ((name, pin), live) in AFTER.iter().zip(after) {
                    assert_eq!(live, pin, "after {name}");
                }
                // The draft commitment (kind 2) from independent reads of the
                // pool and the control after the install (S2-b2 review A8):
                // the prefix is the one before the reserve with the
                // RowReserve frame appended.
                let mut after_prefix = prefix;
                after_prefix.complete_lsn = PREVIOUS_LSN + 1;
                after_prefix.complete_offset += frame.len() as u64;
                let independent = xrwc_draft(
                    &t,
                    after_prefix,
                    (writer.recovery_test_bank(), writer.row_test_locals()),
                    cut.2,
                    (actor, root_job, row_id, undo_ref),
                );
                println!("S2W_R06 xrwc_draft={}", raw(&independent));
                assert_eq!(draft.0, independent);
                let _ = reader;
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R07: a one-shot cfg(test) stamp change seen only by reserve's
/// commitment recompute makes reserve Before Unchanged (Prepare, Frame
/// Stale) on the same PreflightRow, with no write bit, WAL or file change and
/// the SCHEMA and BEFORE copies as preflight wrote them (S2-b1 review A2(b)).
/// A retry from `validation_lane` on that PreflightRow then reserves the
/// literal's frame.
#[test]
fn s2w_r07_one_stale_stamp_is_before_then_retry_reserves() {
    case("S2W-R07", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let (_, slot, _) = table.test_identity();
                let effects = files(state);
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let copies = preflight.test_copy_digests().unwrap();
                let input = plan!(preflight);
                let once = preflight.test_stale_stamp_once();
                let (error, owned) = before!(preflight.reserve(input));
                drop(once);
                let mut preflight = owned;
                println!("S2W_R07 refusal={error:?}");
                assert!(stale(&error));
                assert_eq!(view.test_table_masks().1, 0);
                assert_eq!(files(state), effects);
                assert!(!fence.is_fenced());
                assert_eq!(preflight.test_copy_digests(), Some(copies));
                let input = plan!(preflight);
                assert_eq!(preflight.test_copy_digests(), Some(copies));
                let reserved = reserved!(preflight.reserve(input));
                let frame = wal_since(state, effects.0);
                println!(
                    "S2W_R07 retry frame_bytes={} frame_sha={}",
                    frame.len(),
                    hex(&frame)
                );
                assert_eq!((frame.len(), hex(&frame).as_str()), FRAME);
                assert_eq!(reserved.test_issued().4, PREVIOUS_LSN + 1);
                assert_eq!(view.test_table_masks().1, 1 << slot);
                drop(reserved);
                assert!(fence.is_fenced());
                let _ = reader;
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R14: a Row actor that has a Catalog Job is refused before Reserve.
/// After a Ready preflight, a one-shot cfg(test) view of the statement head
/// reads the fixture's first Job with a source (the committed CREATE's
/// Catalog Job) as the actor's own; reserve's entry check is then the deep
/// check's Fatal (Census, Invariant), CallerMustResolve, with the fence, no
/// write bit and no WAL or file change. The Fatal arm still releases the
/// read gate, and the RowWrite charge stays under the fence (a fenced ledger
/// refuses the return), as S2W-R19 shows for preflight (S2-b2 review A6).
#[test]
fn s2w_r14_catalog_job_actor_is_refused_before_reserve() {
    case("S2W-R14", || {
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
                let effects = files(state);
                let used = reader.catalog_test_source_used();
                let charge = writer.catalog_test_retirement_charge() as u64;
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                assert!(hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                let once = view.test_catalog_scope_once();
                let result = preflight.reserve(input);
                drop(once);
                let error = match result {
                    Err(RowOwnedError::Fatal(error)) => error,
                    Err(RowOwnedError::Before { error, .. }) => panic!("Before {error:?}"),
                    Ok(_) => panic!("a refused reserve"),
                };
                println!("S2W_R14 refusal={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Census, Effect::CallerMustResolve)
                );
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert!(fence.is_fenced());
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used + charge);
                assert_eq!(view.test_table_masks().1, 0);
                assert_eq!(files(state), effects);
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R11's pre-stage refusal at reserve (S2-b2 review A6): a one-shot
/// cfg(test) refusal after the table write bit and before the existing-group
/// install makes reserve Before Unchanged (Stage, Busy) on the same
/// PreflightRow. The write bit stays (a Before after the registration keeps
/// the conservative bit, §6), and no WAL or file byte changes; `finish` then
/// returns the source and the gate with no fence.
#[test]
fn s2w_r11_reserve_stage_refusal_keeps_the_write_bit() {
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
                let effects = files(state);
                let used = reader.catalog_test_source_used();
                let mut preflight = ready!(writer, owner, view, table, private, flush);
                let input = plan!(preflight);
                let once = preflight.test_stage_refusal_once();
                let (error, owned) = before!(preflight.reserve(input));
                drop(once);
                println!("S2W_R11 reserve_half refusal={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Stage, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::Busy));
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert_eq!(files(state), effects);
                assert!(!fence.is_fenced());
                assert!(hook::observe(core).read_gate_closed);
                owned.finish().unwrap();
                assert!(!hook::observe(core).read_gate_closed);
                assert_eq!(reader.catalog_test_source_used(), used);
                assert_eq!(view.test_table_masks(), (1 << slot, 1 << slot));
                assert_eq!(files(state), effects);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// S2W-R15 (the reserve-side half): a reader admitted before the preflight
/// holds the preflight at its read gate (ReadGateWaiting, the gate closed and
/// one read operation) until the reader returns; the preflight is then Ready.
/// While the ReservedRow is held, a new read operation on the same core is
/// Unchanged InstallationPending. The cancel that ends a reservation without
/// the fence is S2-d's.
#[test]
fn s2w_r15_reader_waits_and_is_installation_pending_while_reserved() {
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
                let effects = files(state);
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
                    println!("S2W_R15 at_gate_wait={waited:?}");
                    assert!(waited.read_gate_closed);
                    assert_eq!(waited.active_read_operations, 1);
                    assert!(!waited.fenced);
                    let input = plan!(preflight);
                    let reserved = reserved!(preflight.reserve(input));
                    let mut late = core.try_owner(OwnerRole::Foreground).unwrap();
                    let refused = late.try_read_operation(core).err().unwrap();
                    println!(
                        "S2W_R15 reserved observe={:?} reader={refused:?}",
                        hook::observe(core)
                    );
                    assert_eq!(refused.cause, concurrency::Cause::InstallationPending);
                    assert!(matches!(refused.effect, Effect::Unchanged));
                    assert_eq!(hook::observe(core).active_read_operations, 0);
                    late.check_idle().unwrap();
                    drop(late);
                    assert!(!fence.is_fenced());
                    assert_eq!(wal_since(state, effects.0).len(), FRAME.0);
                    drop(reserved);
                    assert!(fence.is_fenced());
                });
                let _ = reader;
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R22 (reserve half): with every target resident, a preflight alone
/// checks the replay out once; a preflight, `validation_lane` and reserve
/// together check it out the same once, so the lane and reserve open no
/// replay (S2-0b decision 1). Scopes are counted the same way.
#[test]
fn s2w_r22_reserve_checks_out_no_replay() {
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
                drop(reserved);
                assert!(fence.is_fenced());
                let through_reserve = writer.row_test_replay_counts();
                println!(
                    "S2W_R22 preflight scopes={} checkouts={} \
                     preflight_lane_reserve scopes={} checkouts={}",
                    preflight_only.0 - start.0,
                    preflight_only.1 - start.1,
                    through_reserve.0 - preflight_only.0,
                    through_reserve.1 - preflight_only.1
                );
                assert_eq!(preflight_only.1 - start.1, 1);
                assert_eq!(through_reserve.1 - preflight_only.1, 1);
                assert_eq!(
                    through_reserve.0 - preflight_only.0,
                    preflight_only.0 - start.0
                );
                let _ = reader;
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R23(1), the preflight half alone (S2-b1 review A1): one byte of the
/// clean resident Status 11 changed with no stamp change, then a preflight.
/// Its replay arena compare of T is Fatal (Census, Invariant),
/// CallerMustResolve, with the fence, no registration and no file change.
#[test]
fn s2w_r23_preflight_arena_mismatch_is_fatal() {
    case("S2W-R23", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 12,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = core;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                view.test_clear_reads();
                let effects = files(state);
                flip(reader.catalog_test_pool(), 11);
                let error = writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .err()
                    .unwrap();
                println!("S2W_R23 preflight_half refusal={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Census, Effect::CallerMustResolve)
                );
                assert!(matches!(error.cause, RetentionCause::Invariant));
                assert!(fence.is_fenced());
                assert_eq!(reads!(view), (0, 0, 0));
                assert_eq!(files(state), effects);
                drop(view);
                drop(row);
            }
        );
    });
}

/// S2W-R23(1) on the Root, the case's own page: reserve is Before (Stale)
/// with no effect and `finish` returns; the next preflight selects the
/// RootJob from the pool's Root (`row_group::reserve_job`) before its arena
/// compare, so it is Fatal (Census) at that decode with the page's own cause
/// (the Root's checksum: Queue Page Corrupt), CallerMustResolve, and the
/// fence stays.
#[test]
fn s2w_r23_root_byte_is_stale_then_preflight_fatal() {
    case("S2W-R23", || r23(1));
}

/// S2W-R23(1) on the Row Status 11, also in every row's T: reserve is Before
/// (Stale) with no effect and `finish` returns; the next preflight reaches
/// the arena compare and is Fatal (Census, Invariant), CallerMustResolve,
/// and the fence stays.
#[test]
fn s2w_r23_status_byte_is_stale_then_arena_fatal() {
    case("S2W-R23", || r23(11));
}

fn r23(number: u64) {
    let oracle = Oracle::new();
    with_fresh!(
        oracle,
        frames = 12,
        |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
            let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
            row_statement!(live, owner, flusher, private, reader_private, row_status
                => writer, reader, flush, row, view, table);
            let _ = settle!(writer, owner, view, table, private, flush, flusher);
            let effects = files(state);
            let mut preflight = ready!(writer, owner, view, table, private, flush);
            let input = plan!(preflight);
            flip(reader.catalog_test_pool(), number);
            let (error, owned) = before!(preflight.reserve(input));
            println!("S2W_R23 page={number} reserve={error:?}");
            assert!(stale(&error));
            assert_eq!(view.test_table_masks().1, 0);
            assert_eq!(files(state), effects);
            assert!(!fence.is_fenced());
            owned.finish().unwrap();
            assert!(!hook::observe(core).read_gate_closed);
            assert!(!fence.is_fenced());
            let error = writer
                .preflight_row(owner, &view, &table, private, &mut flush)
                .err()
                .unwrap();
            println!("S2W_R23 page={number} next_preflight={error:?}");
            assert_eq!(
                (error.phase, error.effect),
                (RetentionPhase::Census, Effect::CallerMustResolve)
            );
            if number == 1 {
                assert!(matches!(
                    error.cause,
                    RetentionCause::RowSemantic(crate::storage::semantic_row::Error::Queue(
                        crate::storage::retention_page::RetentionError::Page(
                            page::PageError::Corrupt
                        )
                    ))
                ));
            } else {
                assert!(matches!(error.cause, RetentionCause::Invariant));
            }
            assert!(fence.is_fenced());
            assert_eq!(files(state), effects);
            drop(view);
            drop(row);
        }
    );
}
