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
use super::row_preflight::create_table;
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RetentionCause, RowInputPlan, RowOperation, RowOwnedError,
        RowPageRole, RowPreflightStep, RowProducerLane, RowProposal, RowValidationLane, TableStep,
        TransactionFinishError, TransactionOptions,
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
struct Literal {
    canonical: Vec<u8>,
    frame: Vec<u8>,
    after: Vec<(RowPageRole, Vec<u8>)>,
}

impl Literal {
    fn load() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(AUTHOR);
        let expected = String::from_utf8(read(&root.join("expected.json"), 1 << 16)).unwrap();
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
