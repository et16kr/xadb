//! S8 C+B (consumers §8.3, boundaries `#row-single-source-s8`): a Row
//! written without A. C's actual Begin and RowReserve issue the row; an
//! independent literal (`tests/fixtures/dev3-module-c2/row-read/`, authored
//! on the N1 History model, never by an XADB encoder) proposes the canonical
//! row and the Forward under that reservation; C checks every byte and
//! installs it, then C's own Commit publishes q. B's point and cursor
//! drivers read the row. Nothing here links A's encoder, INSERT, CLR or
//! driver: the module is in the C+B single-source export.
//!
//! The literal is the S2 fixture's row (T object 6, RowId 1, Int64 7,
//! RowReserve LSN 15, Forward LSN 16). The fixture fills the lanes exactly
//! as any producer does: the candidate bytes and an untrusted RowInputPlan
//! in the validation lane, the frame and the five after images in the
//! producer lane, placed by C's declared after order. Expected values come
//! from the author's expected.json and the history, never from C or B.
//!
//! MCB05-K (consumers §8.3 known-bad): the same reservation is offered one
//! known-bad Forward per case, each the author's single semantic change to
//! that literal with its own pinned SHA-256 (`known-bad/Kxx.patch`). C's
//! prepare refuses it Before (Prepare, Unchanged, RowSemantic) on the same
//! ReservedRow: no stage, no WAL, file or page change, the issued identity
//! and draft cut kept. The explicit cancel that follows lands the S2 Cancel
//! literal of that reservation.
use super::row_preflight::{create_table, t_frames};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RetentionCause, RetentionPhase, RowInputPlan, RowOperation,
        RowOwnedError, RowPageRole, RowPreflightStep, RowProducerLane, RowProposal,
        RowValidationLane, TableStep, TransactionFinishError, TransactionOptions,
    },
    row::read::driver::{
        CursorStep, PointRead,
        live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
    },
    row_record::ValueRef,
};
use foundation::{identity::RowId, runtime::Cancellation};

/// The literal directory, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c2/row-read";
/// The row the literal writes (expected.json `row`, `value`).
const ROW: u64 = 1;
const VALUE: i64 = 7;
/// The WAL complete LSN before the RowReserve (expected.json
/// `reserve_lsn` − 1); the Forward is the one after the RowReserve.
const PREVIOUS_LSN: u64 = 14;
/// The S2 RowReserve literal C writes for this reservation
/// (`dev3-module-c3/row-write/expected.json`): bytes and SHA-256.
const RESERVE_FRAME: (usize, &str) = (
    57_336,
    "6e3badc82e1864c405f846a05372e2367f6a266a10e0a4e0c8c3d28267870fbe",
);
/// expected.json `files`: (name, bytes, SHA-256).
const FILES: [(&str, usize, &str); 7] = [
    (
        "canonical.bin",
        13,
        "fae2df78d78985674f9e83253255b062c3bbee012bc876f26e4342520ae04b38",
    ),
    (
        "forward-frame.bin",
        91_497,
        "0ee305722bff97a71161e2111bb7a15059b3a35b8a941c603a45e2fc5de81043",
    ),
    (
        "after-root.bin",
        PAGE_BYTES,
        "32e2fe6450e9b2fe0cbf07707e6a568cf6d193b6f5383365240e979f3fe49a40",
    ),
    (
        "after-jobs.bin",
        PAGE_BYTES,
        "f5627aa6bf03b8b05c068f99d3405624fad788d64fbda3557307d319128356df",
    ),
    (
        "after-status.bin",
        PAGE_BYTES,
        "8371f123e181f5bc9d4c437a7b51282f84a24f2c9dd350ef07b577c7d14f5162",
    ),
    (
        "after-undo.bin",
        PAGE_BYTES,
        "801bbcfb6457e6e86916422054ef035475038c644786723d7cd600ee2b954cb8",
    ),
    (
        "after-leaf.bin",
        PAGE_BYTES,
        "0c3689d98f178c24eb75ff3276bbed2ee7bb410ca38103a00d6708d1ee4e8ff4",
    ),
];
/// The S2 Cancel literal of this reservation (expected.json `cancel`, from
/// `dev3-module-c3/row-write/expected-cancel.json`): bytes and SHA-256.
const CANCEL_FRAME: (usize, &str) = (
    40_000,
    "c2b02eaa12afa2c12eb0427ad8a6deb9746ccb884571ab3157928fa6ba6aee4b",
);
/// The page a Forward after image of each role replaces (expected.json
/// `pages`), with its literal file.
const AFTER: [(RowPageRole, u64, &str); 5] = [
    (RowPageRole::Root, 1, "after-root.bin"),
    (RowPageRole::RootJob, 2, "after-jobs.bin"),
    (RowPageRole::Status, 11, "after-status.bin"),
    (RowPageRole::Undo, 14, "after-undo.bin"),
    (RowPageRole::RowLeaf, 13, "after-leaf.bin"),
];

/// The literal's bytes, each checked against its pin when loaded.
#[derive(Clone)]
struct Literal {
    canonical: Vec<u8>,
    frame: Vec<u8>,
    after: Vec<(RowPageRole, Vec<u8>)>,
}

/// The author's directory.
fn author() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR)
}

/// The author's expected.json.
fn expected() -> String {
    String::from_utf8(read(&author().join("expected.json"), 1 << 16)).unwrap()
}

/// Lowercase hex to bytes.
fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "hex length");
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

impl Literal {
    fn load() -> Self {
        let root = author();
        let expected = expected();
        for fact in [
            "\"case\": \"MCB05-L01\"".to_string(),
            format!("\"row\": {ROW}"),
            format!("\"value\": {VALUE}"),
            format!("\"reserve_lsn\": {}", PREVIOUS_LSN + 1),
            format!("\"forward_lsn\": {}", PREVIOUS_LSN + 2),
        ] {
            assert!(expected.contains(&fact), "expected.json lacks {fact}");
        }
        let file = |name: &str| {
            let (_, bytes, digest) = FILES.iter().find(|(file, ..)| *file == name).unwrap();
            let data = read(&root.join("literal").join(name), 1 << 17);
            assert_eq!(
                (data.len(), hex(&data).as_str()),
                (*bytes, *digest),
                "{name}"
            );
            assert!(
                expected.contains(&format!(
                    "\"{name}\": {{\n      \"bytes\": {bytes},\n      \"sha256\": \"{digest}\""
                )),
                "expected.json pin of {name}"
            );
            data
        };
        Self {
            canonical: file("canonical.bin"),
            frame: file("forward-frame.bin"),
            after: AFTER
                .iter()
                .map(|(role, _, name)| (*role, file(name)))
                .collect(),
        }
    }

    /// The known-bad variant `case`: its patch file pinned by `digest` here
    /// and in expected.json, applied to this literal's files. A same-length
    /// file is byte runs `start end hex` over the literal; a file whose
    /// length changes is named whole. Each patched file must have the
    /// length and SHA-256 its `file` line states.
    fn known_bad(&self, case: &str, digest: &str) -> Self {
        let root = author();
        let text = read(
            &root.join("known-bad").join(format!("{case}.patch")),
            1 << 15,
        );
        assert_eq!(hex(&text), digest, "{case}.patch");
        let pin = format!("\"sha256\": \"{digest}\"");
        assert!(expected().contains(&pin), "expected.json pin of {case}");
        let text = String::from_utf8(text).unwrap();
        let mut lines = text.lines().filter(|line| !line.starts_with('#'));
        assert_eq!(
            lines.next(),
            Some(format!("XADB-S8-KNOWN-BAD-V1 {case}").as_str())
        );
        // (file, its stated length and SHA-256, the patched bytes)
        let mut files: Vec<(String, usize, String, Vec<u8>)> = Vec::new();
        // The literal offset the current file is copied up to.
        let mut at = 0;
        let mut out = self.clone();
        for line in lines {
            let fields: Vec<&str> = line.split(' ').collect();
            if fields[0] == "file" || fields[0] == "end" {
                if let Some((name, _, _, data)) = files.last_mut()
                    && at != usize::MAX
                {
                    data.extend_from_slice(&out.file(name)[at..]);
                }
                if fields[0] == "end" {
                    break;
                }
                let (name, bytes, sha) = (fields[1], fields[2].parse().unwrap(), fields[3]);
                let data = match fields.get(4) {
                    Some(&"whole") => {
                        at = usize::MAX;
                        read(&root.join("known-bad").join(fields[5]), 1 << 17)
                    }
                    None => {
                        at = 0;
                        Vec::new()
                    }
                    Some(mode) => panic!("patch mode {mode}"),
                };
                files.push((name.into(), bytes, sha.into(), data));
                continue;
            }
            let (start, end): (usize, usize) =
                (fields[0].parse().unwrap(), fields[1].parse().unwrap());
            let (name, _, _, data) = files.last_mut().expect("a run inside a file");
            assert!(at <= start && start <= end, "ordered runs");
            data.extend_from_slice(&out.file(name)[at..start]);
            data.extend_from_slice(&unhex(fields[2]));
            at = end;
        }
        assert!(!files.is_empty(), "{case} changes a file");
        for (name, bytes, sha, data) in files {
            assert_eq!((data.len(), hex(&data)), (bytes, sha), "{case} {name}");
            *out.file(&name) = data;
        }
        out
    }

    /// The literal file `name` (frame or an after image).
    fn file(&mut self, name: &str) -> &mut Vec<u8> {
        if name == "forward-frame.bin" {
            return &mut self.frame;
        }
        let (role, ..) = AFTER
            .iter()
            .find(|(_, _, file)| *file == name)
            .unwrap_or_else(|| panic!("patched file {name}"));
        &mut self.after.iter_mut().find(|(r, _)| r == role).unwrap().1
    }

    /// The validation lane gets the literal canonical row; the plan names
    /// its length, its SHA-256 and the lane's SchemaEpoch (the plan is
    /// untrusted: C's reserve rechecks it).
    fn plan(&self, lane: RowValidationLane<'_>) -> RowInputPlan {
        let (context, _schema, _leaf, candidate) = lane.parts();
        candidate[..self.canonical.len()].copy_from_slice(&self.canonical);
        RowInputPlan::new(
            self.canonical.len(),
            context.schema_epoch(),
            foundation::codec::content_digest(&self.canonical),
        )
    }

    /// The producer lane gets the literal frame and after images, the
    /// images in C's declared physical order; the proposal declares them.
    fn forward(&self, lane: RowProducerLane<'_>) -> RowProposal {
        let (context, _control, _before, _schema, frame, after, _candidate, _scratch) =
            lane.parts();
        assert_eq!(context.operation(), RowOperation::Forward);
        frame[..self.frame.len()].copy_from_slice(&self.frame);
        let order = context.after_order();
        assert_eq!(order.len(), 5, "a first Forward changes five pages");
        let mut roles = [RowPageRole::Root; 5];
        for (index, role) in order.iter().enumerate() {
            let (_, bytes) = self.after.iter().find(|(r, _)| r == role).unwrap();
            after[index].copy_from_slice(bytes);
            roles[index] = *role;
        }
        RowProposal::new(RowOperation::Forward, self.frame.len(), 5, roles)
    }
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

/// The Int64 value of T's one column in a copied canonical row.
fn int_of(output: &[u8], len: usize) -> i64 {
    let row = crate::storage::row_record::decode_canonical_row(
        &output[..len],
        &[Column {
            name: "i".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    )
    .unwrap();
    match row.value(0).unwrap() {
        ValueRef::Int64(value) => value,
        other => panic!("column value {other:?}"),
    }
}

/// B's point read of `row` on `view`: its value, or None for Missing.
macro_rules! point {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let mut output = [0xA5; 64];
        match read_point_live(
            ReadCall {
                port: &$reader,
                owner: $owner,
                private: $private,
                residency: &mut residency,
            },
            $view,
            $table,
            RowId::new($row).unwrap(),
            &mut output,
            &Cancellation::default(),
        )
        .unwrap()
        {
            ReadStep::Ready(PointRead::Found(copy)) => {
                assert_eq!(copy.version().row.get(), $row);
                Some(int_of(&output, copy.len()))
            }
            ReadStep::Ready(PointRead::Missing) => {
                assert_eq!(output, [0xA5; 64], "Missing publishes no bytes");
                None
            }
            ReadStep::FlushRequired { .. } => panic!("resident read sources"),
        }
    }};
}

/// B's cursor over T on `view`: (RowId, value) in cursor order.
macro_rules! scan {
    ($reader:ident, $owner:expr, $private:expr, $view:expr, $table:expr, $flush:ident, $flusher:ident) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let cancel = Cancellation::default();
        let mut output = [0u8; 64];
        let mut rows = Vec::new();
        let mut cursor = open_cursor_live($view, $table, None, None).unwrap();
        for _ in 0..16 {
            match next_live(
                &mut cursor,
                ReadCall {
                    port: &$reader,
                    owner: $owner,
                    private: $private,
                    residency: &mut residency,
                },
                &mut output,
                &cancel,
            )
            .unwrap()
            {
                ReadStep::Ready(CursorStep::Row(copy)) => {
                    rows.push((copy.version().row.get(), int_of(&output, copy.len())))
                }
                ReadStep::Ready(CursorStep::End) => break,
                ReadStep::FlushRequired { target } => flush_target!($flush, $flusher, target),
            }
        }
        cursor.close().unwrap();
        rows
    }};
}

/// A read-only transaction of `mode` on `owner`.
macro_rules! read_only {
    ($writer:expr, $owner:expr, $mode:expr, $private:expr) => {
        $writer
            .begin_transaction(
                $owner,
                TransactionOptions {
                    read_only: true,
                    isolation: $mode,
                },
                $private,
            )
            .unwrap()
    };
}

/// The WAL file's length and the bytes it gained from `from` on.
fn wal(state: &State) -> Vec<u8> {
    state.lock().unwrap().files["d3-wal-00000001"].bytes.clone()
}

/// MCB05-L01: C Begin and RowReserve, then the literal Forward through C's
/// prepare and apply. The WAL gains exactly the S2 RowReserve literal and
/// the author's Forward frame (LSN 15, 16); the slot observes a Forward of
/// RowId 1. Before the Commit, B reads the row on the writer's own
/// statement (own Active write) and not on a foreign Serializable statement
/// taken before the write. C's Commit returns q 2. After it, the foreign
/// statement still reads nothing (its snapshot q 1) and its transaction
/// ends Rejected(SerializationConflict); a new read-only transaction reads
/// (1, 7) by point and by cursor.
#[test]
fn mcb05_l01_literal_forward_is_read_by_b_after_c_commit() {
    case("MCB05-L01", || {
        let literal = Literal::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                {
                    let (mut writer, reader, mut flush) = live.split();
                    let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let b_owner = &mut b_owner;
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap();
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    let _ = settle!(writer, owner, view, table, private, flush, flusher);
                    let b = read_only!(
                        writer,
                        b_owner,
                        IsolationLevel::Serializable,
                        reader_private
                    );
                    let old = writer.begin_statement(b_owner, &b).unwrap();
                    assert_eq!(old.q().unwrap(), 1);
                    let from = wal(state).len();
                    let mut preflight = match writer
                        .preflight_row(owner, &view, &table, private, &mut flush)
                        .unwrap()
                    {
                        RowPreflightStep::Ready(preflight) => preflight,
                        RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
                    };
                    let plan = literal.plan(preflight.validation_lane().unwrap());
                    let mut reserved = done!(preflight.reserve(plan));
                    let (_, _, row_id, undo_ref, _) = reserved.test_issued();
                    assert_eq!(row_id.get(), ROW);
                    let proposal = literal.forward(reserved.producer_lane().unwrap());
                    let prepared = done!(reserved.prepare(proposal));
                    let applied = done!(prepared.apply());
                    let (observation, _) = applied.into_parts();
                    assert_eq!(observation.operation(), RowOperation::Forward);
                    assert_eq!((observation.row(), observation.undo()), (row_id, undo_ref));
                    let gained = wal(state)[from..].to_vec();
                    let (reserve, forward) = gained.split_at(RESERVE_FRAME.0);
                    assert_eq!((reserve.len(), hex(reserve).as_str()), RESERVE_FRAME);
                    assert_eq!(
                        forward,
                        &literal.frame[..],
                        "the WAL holds the literal Forward"
                    );
                    // Own Active write and a foreign snapshot taken before it.
                    assert_eq!(
                        point!(reader, owner, reader_private, &view, &table, ROW),
                        Some(VALUE)
                    );
                    assert_eq!(
                        scan!(reader, owner, reader_private, &view, &table, flush, flusher),
                        vec![(ROW, VALUE)]
                    );
                    assert_eq!(
                        point!(reader, b_owner, reader_private, &old, &table, ROW),
                        None
                    );
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let q = match writer.commit_row(owner, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(_) => panic!("C's Commit of the literal row refused"),
                    };
                    assert_eq!(q, 2);
                    assert_eq!(
                        scan!(
                            reader,
                            b_owner,
                            reader_private,
                            &old,
                            &table,
                            flush,
                            flusher
                        ),
                        vec![],
                        "the old snapshot keeps q 1"
                    );
                    assert!(writer.finish_statement(b_owner, old).is_ok());
                    // B's Serializable transaction read T, which a later
                    // Commit changed: it ends Rejected (S7 MCI01-V06).
                    let end = writer.finish_read_only(b_owner, b);
                    assert!(matches!(
                        end,
                        Err(TransactionFinishError::Rejected(ref error))
                            if matches!(error.cause, RetentionCause::SerializationConflict)
                    ));
                    let c = read_only!(
                        writer,
                        b_owner,
                        IsolationLevel::Serializable,
                        reader_private
                    );
                    let new = writer.begin_statement(b_owner, &c).unwrap();
                    assert_eq!(new.q().unwrap(), 2);
                    assert_eq!(
                        point!(reader, b_owner, reader_private, &new, &table, ROW),
                        Some(VALUE)
                    );
                    assert_eq!(
                        scan!(
                            reader,
                            b_owner,
                            reader_private,
                            &new,
                            &table,
                            flush,
                            flusher
                        ),
                        vec![(ROW, VALUE)]
                    );
                    assert!(writer.finish_statement(b_owner, new).is_ok());
                    assert!(writer.finish_read_only(b_owner, c).is_ok());
                    b_owner.check_idle().unwrap();
                    println!(
                        "MCB05_L01 row={} q={q} reserve_lsn={} forward_lsn={} forward_bytes={}",
                        row_id.get(),
                        PREVIOUS_LSN + 1,
                        PREVIOUS_LSN + 2,
                        literal.frame.len()
                    );
                }
                assert!(!fence.is_fenced());
                owner.check_idle().unwrap();
            }
        );
    });
}

/// The WAL length and every file image: a refusal's effect is none.
fn effects(state: &State) -> (usize, Images) {
    let state = state.lock().unwrap();
    (state.files["d3-wal-00000001"].bytes.len(), state.images())
}

/// One MCB05-K case: C Begin and RowReserve, then the known-bad Forward
/// through C's prepare on that reservation: Before (Prepare, Unchanged,
/// RowSemantic) with the same ReservedRow, its issued identity and draft
/// cut, and no WAL, file, T page (bytes and stamp) or fence change. The
/// explicit cancel then lands the S2 Cancel literal, and the statement and
/// the now empty transaction end (RollbackStart, AbortEnd).
fn known_bad(name: &str, digest: &str) {
    case(&format!("MCB05-{name}"), || {
        let bad = Literal::load().known_bad(name, digest);
        let expected = expected();
        assert!(expected.contains(&format!("\"frame_bytes\": {}", CANCEL_FRAME.0)));
        assert!(expected.contains(&format!("\"frame_sha256\": \"{}\"", CANCEL_FRAME.1)));
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, _core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let pool = reader.catalog_test_pool();
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(preflight) => preflight,
                    RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
                };
                let plan = bad.plan(preflight.validation_lane().unwrap());
                let mut reserved = done!(preflight.reserve(plan));
                let issued = reserved.test_issued();
                let draft = reserved.test_draft();
                assert_eq!(issued.2.get(), ROW);
                let before = effects(state);
                let pages = t_frames(pool);
                let proposal = bad.forward(reserved.producer_lane().unwrap());
                let (error, reserved) = match reserved.prepare(proposal) {
                    Err(RowOwnedError::Before { error, owned }) => (error, owned),
                    Err(RowOwnedError::Fatal(error)) => panic!("{name} Fatal {error:?}"),
                    Ok(_) => panic!("{name} prepared"),
                };
                println!("MCB05_{name} prepare={error:?}");
                assert_eq!(
                    (error.phase, error.effect),
                    (RetentionPhase::Prepare, Effect::Unchanged)
                );
                assert!(matches!(error.cause, RetentionCause::RowSemantic(_)));
                assert_eq!(reserved.test_issued(), issued);
                assert_eq!(reserved.test_draft(), draft);
                assert_eq!(effects(state), before, "no WAL or file effect");
                for (old, new) in pages.iter().zip(t_frames(pool)) {
                    assert_eq!(*old, new, "page {}", old.0);
                }
                assert!(!fence.is_fenced());
                if reserved.cancel().is_err() {
                    panic!("{name} cancel refused");
                }
                let cancel = wal(state)[before.0..].to_vec();
                println!(
                    "MCB05_{name} cancel frame_bytes={} frame_sha={}",
                    cancel.len(),
                    hex(&cancel)
                );
                assert_eq!((cancel.len(), hex(&cancel).as_str()), CANCEL_FRAME);
                close_row!(writer, owner, row, view, private, flush);
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// MCB05-K01: foreign table generation (Job, U target, projection).
#[test]
fn mcb05_k01_foreign_table_generation_is_before_then_cancel() {
    known_bad(
        "K01",
        "e6168679a6320f6c131808db1dd82ced695a8c0d60762cac3f9d8201c661fc19",
    );
}

/// MCB05-K02: forged RowId 2, the Reserve issued 1.
#[test]
fn mcb05_k02_forged_row_id_is_before_then_cancel() {
    known_bad(
        "K02",
        "7977833f604796c72eb5ae42ac532a5f45cdd4740a7152b4243a13d07bbc22bd",
    );
}

/// MCB05-K03: wrong U generation.
#[test]
fn mcb05_k03_wrong_undo_generation_is_before_then_cancel() {
    known_bad(
        "K03",
        "96a478a7bb5fa2b3fd25af09fb5c88c63c44ec5a42cbf36eb1364d8cc17495dd",
    );
}

/// MCB05-K04: planned mutation consumed again (descriptor last mutation 2).
#[test]
fn mcb05_k04_planned_mutation_consumed_again_is_before_then_cancel() {
    known_bad(
        "K04",
        "c9e825ba9853c1947a0913b758d62e40ea09f84b53e840cdb5c221dd911249b5",
    );
}

/// MCB05-K05: predecessor H claimed, the actual predecessor is none.
#[test]
fn mcb05_k05_predecessor_claimed_without_one_is_before_then_cancel() {
    known_bad(
        "K05",
        "ed20accd34f86e3e8a2ed284e8e394d1913fbdb41bc133cc164254f1d556605e",
    );
}

/// MCB05-K06: U incoming 1 (the graph gives 2).
#[test]
fn mcb05_k06_undo_incoming_one_is_before_then_cancel() {
    known_bad(
        "K06",
        "a0d74cc80326f48a74f8f46f244824ac80a54a90737c0657bc9a047a25a51c05",
    );
}

/// MCB05-K08: row Transaction stamp is not the actor.
#[test]
fn mcb05_k08_row_stamp_not_the_actor_is_before_then_cancel() {
    known_bad(
        "K08",
        "8d613595e008bb96edc00c6a1af93f523d2e87a5bd8b8381addf4476cd65b30c",
    );
}

/// MCB05-K09: leaf redo row bytes differ from the native after cell.
#[test]
fn mcb05_k09_redo_row_bytes_differ_from_native_is_before_then_cancel() {
    known_bad(
        "K09",
        "b7d29da13b55fb0ee4231ddfd58bd375597a0bd5a214a2d48784f96b864e1da9",
    );
}

/// MCB05-K10: Reserved inventory item not consumed (no InventoryDelta).
#[test]
fn mcb05_k10_reserved_inventory_not_consumed_is_before_then_cancel() {
    known_bad(
        "K10",
        "9092e06a334ff8eae6d9a1c8d27e01bc639e10a94a8637fd94a11b315ebbb2eb",
    );
}

/// MCB05-K11: full-abort CLR WAL debt decremented by one.
#[test]
fn mcb05_k11_full_abort_debt_decremented_is_before_then_cancel() {
    known_bad(
        "K11",
        "373410e106ab63809d1f75e9e8571281aaaae3bf2233422b97df82cd1d08e0c9",
    );
}

/// MCB05-K12: valid frame resubmitted as a scope2 group.
#[test]
fn mcb05_k12_scope2_group_is_before_then_cancel() {
    known_bad(
        "K12",
        "5c2699f29446fe500acdf0ad92d4d6e34921d3ceb5563fcdb5dc7f5f9504c98d",
    );
}
