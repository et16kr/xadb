//! S9 C+B literal ingress after a reopen (consumers §8.3 native C+B P1–P3,
//! boundaries `#row-mi03-s9`). P1 is C's actual install, CREATE of T and
//! its Commit (q 1). P2 is C's recovery of those files with
//! `RejectInitialLosers` to READY; its Row Begin, statement, B's table issue
//! and a settled preflight; then the independent literal
//! `tests/fixtures/dev3-module-c2/row-read-reopen/` (the N1 History model,
//! never an XADB encoder) proposes the canonical row under C's RowReserve
//! and the Forward; C checks and installs it and C's Commit publishes q 2.
//! P3 is a second recovery to READY, after which B's point and cursor
//! drivers read the row under a new read-only snapshot. Nothing here links
//! A: the module is in the C+B single-source export.
//!
//! The literal is late-bound to P2's actual identity. The live state before
//! the RowReserve is C's actual state after one recovery (WAL stream 2,
//! complete LSN 20, Free Undo 10), pinned by the author and checked here
//! against the pool and control before the reserve. The author's RowReserve
//! (the model's frame with only the header stream bound to 2) is what C must
//! write; the Forward and the five after images are the literal's.
//!
//! The same three phases run in memory (`mcb05_l03_*`) and as three native
//! processes (`native::s9b_native_worker`, controller
//! `scripts/check_dev3_s9_native.py` case S9B-N01).
use super::super::super::CATALOG_PRIVATE_BYTES;
use super::super::support::clone_directory;
use super::row_literal::int_of;
use super::row_literal_two::page;
use super::row_preflight::create_table;
use super::row_reopen::{AtReady, ready_row_sources, reopen};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RowInputPlan, RowOperation, RowOwnedError, RowPageRole,
        RowPreflightStep, RowProducerLane, RowProposal, RowValidationLane, TableStep,
        TransactionOptions,
    },
    row::read::driver::{
        CursorStep, PointRead,
        live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
    },
};
use foundation::{identity::RowId, runtime::Cancellation};
use std::collections::BTreeMap;

#[path = "row_reopen_literal/native.rs"]
mod native;

/// The literal directory, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c2/row-read-reopen";
/// The WAL file of the first recovery's stream (expected.json `stream`).
pub(super) const WAL: &str = "d3-wal-00000002";
/// The WAL complete LSN at READY (expected.json `previous_lsn`).
const PREVIOUS_LSN: u64 = 20;
/// The row the literal writes (expected.json `row`, `value`).
const ROW: u64 = 1;
const VALUE: i64 = 7;
/// The Free Undo C reserves and the Root Job ordinal it issues
/// (expected.json `free_undo`, `ordinal`).
const FREE_UNDO: u64 = 10;
/// The model's RowReserve C must write (expected.json `reserve`).
const RESERVE: (usize, &str) = (
    57_336,
    "4094194f8f08c5c9c520b49be2382efc2a016f8d439b07a229568d36b55672ee",
);
/// expected.json `files`: (name, bytes, SHA-256).
const FILES: [(&str, usize, &str); 7] = [
    (
        "after-jobs.bin",
        PAGE_BYTES,
        "09d1a594f289507b0581c2e4359593ada44250c3ab9c1a4322b1649b7c3971f2",
    ),
    (
        "after-leaf.bin",
        PAGE_BYTES,
        "684456f31c8884bfb6c758ce4999ea644e60746c72a72be9ccc78816019cb06e",
    ),
    (
        "after-root.bin",
        PAGE_BYTES,
        "03f74b8656b5341c8ce0f3c400bcb984cfaf46de236b49bc7e01e3d3de50ff0c",
    ),
    (
        "after-status.bin",
        PAGE_BYTES,
        "86be0d3d17ec4578bc5629d906dcacff5b080fbebe2ccf4cc29da2cc0373a53b",
    ),
    (
        "after-undo.bin",
        PAGE_BYTES,
        "f963c0783d3c9ec7ae646de0f00e1a74b3ae42ca7cf0e8a15f4b410ba37dbe93",
    ),
    (
        "canonical.bin",
        13,
        "fae2df78d78985674f9e83253255b062c3bbee012bc876f26e4342520ae04b38",
    ),
    (
        "forward-frame.bin",
        91_497,
        "5f84263ce475066475c095e76a9a0d9693afeab9af3422af02d30552db88c2d7",
    ),
];
/// The live inputs the author re-encodes (its `LIVE` pins): Root 1, Jobs
/// 2, Status 11, RowLeaf 13 and the Free Undo 10 from the pool, the 64 Job
/// cells, the Row actor's ATT row, the Aging row and the profile.
const LIVE: [(&str, &str); 9] = [
    (
        "root1.bin",
        "808882c278e81121c51d285dd690f6e4d35475b43cf6a6ff71f5b81d8c8c409e",
    ),
    (
        "jobs2.bin",
        "fa17df47a7e82af55001274aa717481c0e0a3b04a9e010ea7eb51ab95d251a0e",
    ),
    (
        "status11.bin",
        "6d2ef6534c8229c097c47c79f9c3333a50593382a7d58933f9f512763dc19b82",
    ),
    (
        "leaf13.bin",
        "30b625850232dac524dda502796d7e3b0c2679cfc55346cab807b1579b465396",
    ),
    (
        "undo10.bin",
        "8a4a1464e63518e84b23042d936990dca3d29e3da59e0842e0bd5b6cf748eb6c",
    ),
    (
        "bank.bin",
        "63984ecee4d90f49a7838bba2c5f5d748c6a2829059bb79dd17d5376df0f23d5",
    ),
    (
        "tx.bin",
        "5007191f2a5b4ee8cf932a532e286cc4bfb782d89eaf4b6ebebdfd251b2356a3",
    ),
    (
        "aging.bin",
        "df632162d6196501cc70ecc310ebbf48d43ec6b65b8d39545fd06e611abf9d9a",
    ),
    (
        "profile.bin",
        "f49d4bb26a5c0e1bd66dec9a2b0f77aa24e3fafadf54f27e5db9ab9a960e204e",
    ),
];
/// The Aging row's control slot.
const AGING: usize = 32 * 1664;
/// The page each after image replaces (expected.json `pages`).
const AFTER: [(RowPageRole, u64, &str); 5] = [
    (RowPageRole::Root, 1, "after-root.bin"),
    (RowPageRole::RootJob, 2, "after-jobs.bin"),
    (RowPageRole::Status, 11, "after-status.bin"),
    (RowPageRole::Undo, FREE_UNDO, "after-undo.bin"),
    (RowPageRole::RowLeaf, 13, "after-leaf.bin"),
];

/// The author's directory.
fn author() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR)
}

/// The literal's files by name, each checked against its pin.
pub(super) struct Literal(BTreeMap<String, Vec<u8>>);

impl Literal {
    pub(super) fn load() -> Self {
        let root = author();
        let expected = String::from_utf8(read(&root.join("expected.json"), 1 << 16)).unwrap();
        for fact in [
            "\"case\": \"MCB05-L03\"".to_string(),
            "\"stream\": 2".to_string(),
            format!("\"previous_lsn\": {PREVIOUS_LSN}"),
            format!("\"row\": {ROW}"),
            format!("\"value\": {VALUE}"),
            format!("\"free_undo\": {FREE_UNDO}"),
            format!("\"forward_lsn\": {}", PREVIOUS_LSN + 2),
            format!(
                "\"reserve\": {{\n    \"bytes\": {},\n    \"lsn\": {},\n    \"sha256\": \"{}\"",
                RESERVE.0,
                PREVIOUS_LSN + 1,
                RESERVE.1
            ),
        ] {
            assert!(expected.contains(&fact), "expected.json lacks {fact}");
        }
        for (name, digest) in LIVE {
            assert_eq!(
                hex(&read(&root.join("live").join(name), 1 << 17)),
                digest,
                "live {name}"
            );
            assert!(
                expected.contains(&format!(
                    "\"dev3-module-c2/row-read-reopen/live/{name}\": \"{digest}\""
                )),
                "expected.json pin of live {name}"
            );
        }
        Self(
            FILES
                .iter()
                .map(|(name, bytes, digest)| {
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
                    (name.to_string(), data)
                })
                .collect(),
        )
    }

    fn file(&self, name: &str) -> &[u8] {
        &self.0[name]
    }

    /// The canonical row in the validation lane; the untrusted plan names
    /// its length, SHA-256 and the lane's SchemaEpoch.
    fn plan(&self, lane: RowValidationLane<'_>) -> RowInputPlan {
        let canonical = self.file("canonical.bin");
        let (context, _schema, _leaf, candidate) = lane.parts();
        candidate[..canonical.len()].copy_from_slice(canonical);
        RowInputPlan::new(
            canonical.len(),
            context.schema_epoch(),
            foundation::codec::content_digest(canonical),
        )
    }

    /// The Forward frame and after images in the producer lane, the images
    /// in C's declared physical order.
    fn forward(&self, lane: RowProducerLane<'_>) -> RowProposal {
        let (context, _control, _before, _schema, frame, after, _candidate, _scratch) =
            lane.parts();
        assert_eq!(context.operation(), RowOperation::Forward);
        let bytes = self.file("forward-frame.bin");
        frame[..bytes.len()].copy_from_slice(bytes);
        let order = context.after_order();
        assert_eq!(order.len(), 5, "a first Forward changes five pages");
        let mut roles = [RowPageRole::Root; 5];
        for (index, role) in order.iter().enumerate() {
            let (_, _, name) = AFTER.iter().find(|(r, ..)| r == role).unwrap();
            after[index].copy_from_slice(self.file(name));
            roles[index] = *role;
        }
        RowProposal::new(RowOperation::Forward, bytes.len(), 5, roles)
    }
}

/// P2 at READY: the live inputs equal the author's pins, C's RowReserve is
/// the author's late-bound frame, the literal Forward lands with its after
/// images, the writer's own statement reads the row, and C's Commit
/// publishes q 2. `file` reads a namespace file (memory or native).
pub(super) struct Write<'a> {
    pub(super) literal: &'a Literal,
    pub(super) file: &'a dyn Fn(&str) -> Vec<u8>,
}

/// What P2 observed: the READY Q, the issued RowId and the Commit q.
#[derive(Debug, PartialEq)]
pub(super) struct Written {
    pub(super) ready_q: u64,
    pub(super) row: u64,
    pub(super) q: u64,
}

impl AtReady for Write<'_> {
    type Out = Written;
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        private: &mut [u8; CATALOG_PRIVATE_BYTES],
    ) -> Written {
        let ready_q = live.catalog_test_q();
        let row_status = ready_row_sources(live, owner, flusher);
        let mut reader_private = boxed::<PRIVATE_BYTES>();
        let reader_private = &mut *reader_private;
        let (mut writer, reader, mut flush) = live.split();
        let row = writer
            .begin_row_transaction(
                owner,
                IsolationLevel::Serializable,
                &row_status,
                private,
                &mut flush,
            )
            .unwrap();
        let view = writer.begin_statement(owner, &row).unwrap();
        let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
        let _ = settle!(writer, owner, view, table, private, flush, flusher);
        let pool = reader.catalog_test_pool();
        let inputs = {
            let bank = writer.recovery_test_bank();
            [
                page(pool, 1),
                page(pool, 2),
                page(pool, 11),
                page(pool, 13),
                page(pool, FREE_UNDO),
                hex(writer.catalog_test_jobs().as_flattened()),
                hex(&bank[..1536]),
                hex(&bank[AGING..AGING + 1536]),
                hex(&(self.file)("d3-profile")),
            ]
        };
        for ((name, pin), live) in LIVE.iter().zip(&inputs) {
            assert_eq!(live, pin, "live input {name}");
        }
        assert_eq!(
            writer.free_undo_test_state().unwrap().0.complete_lsn,
            PREVIOUS_LSN
        );
        let from = (self.file)(WAL).len();
        let mut preflight = match writer
            .preflight_row(owner, &view, &table, private, &mut flush)
            .unwrap()
        {
            RowPreflightStep::Ready(preflight) => preflight,
            RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
        };
        let plan = self.literal.plan(preflight.validation_lane().unwrap());
        let mut reserved = match preflight.reserve(plan) {
            Ok(reserved) => reserved,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("reserve {error:?}")
            }
        };
        let (_, _, row_id, undo_ref, lsn) = reserved.test_issued();
        assert_eq!(
            (row_id.get(), undo_ref.page().page_no, lsn),
            (ROW, FREE_UNDO, PREVIOUS_LSN + 1)
        );
        let reserve = (self.file)(WAL)[from..].to_vec();
        assert_eq!(
            (reserve.len(), hex(&reserve).as_str()),
            RESERVE,
            "the author's late-bound RowReserve"
        );
        let proposal = self.literal.forward(reserved.producer_lane().unwrap());
        let prepared = match reserved.prepare(proposal) {
            Ok(prepared) => prepared,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("prepare {error:?}")
            }
        };
        let applied = match prepared.apply() {
            Ok(applied) => applied,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("apply {error:?}")
            }
        };
        let (observation, _) = applied.into_parts();
        assert_eq!(observation.operation(), RowOperation::Forward);
        assert_eq!((observation.row(), observation.undo()), (row_id, undo_ref));
        assert_eq!(
            (self.file)(WAL)[from + RESERVE.0..],
            *self.literal.file("forward-frame.bin"),
            "the WAL holds the literal Forward"
        );
        for (_, number, name) in AFTER {
            assert_eq!(page(pool, number), hex(self.literal.file(name)), "{name}");
        }
        assert_eq!(
            point!(reader, owner, reader_private, &view, &table, ROW),
            Some(VALUE)
        );
        assert_eq!(
            scan!(reader, owner, reader_private, &view, &table, flush, flusher),
            vec![(ROW, VALUE)]
        );
        assert!(writer.finish_statement(owner, view).is_ok());
        let q = match writer.commit_row(owner, row, private) {
            Ok(committed) => committed.q(),
            Err(_) => panic!("C's Commit of the literal row refused"),
        };
        owner.check_idle().unwrap();
        Written {
            ready_q,
            row: row_id.get(),
            q,
        }
    }
}

/// P3 at READY: B's point read of RowId 1 and 2 and its cursor over T
/// under a new read-only Serializable snapshot. Returns the READY Q, the
/// snapshot q, the point reads and the cursor rows.
pub(super) struct Read;

/// What P3 read.
#[derive(Debug, PartialEq)]
pub(super) struct Reread {
    pub(super) ready_q: u64,
    pub(super) q: u64,
    pub(super) points: Vec<Option<i64>>,
    pub(super) rows: Vec<(u64, i64)>,
}

impl AtReady for Read {
    type Out = Reread;
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        _: &mut [u8; CATALOG_PRIVATE_BYTES],
    ) -> Reread {
        let ready_q = live.catalog_test_q();
        let _ = ready_row_sources(live, owner, flusher);
        let mut reader_private = boxed::<PRIVATE_BYTES>();
        let reader_private = &mut *reader_private;
        let (mut writer, reader, mut flush) = live.split();
        let b = read_only!(writer, owner, IsolationLevel::Serializable, reader_private);
        let view = writer.begin_statement(owner, &b).unwrap();
        let q = view.q().unwrap();
        let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
        let points = [ROW, ROW + 1]
            .iter()
            .map(|row| point!(reader, owner, reader_private, &view, &table, *row))
            .collect();
        let rows = scan!(reader, owner, reader_private, &view, &table, flush, flusher);
        assert!(writer.finish_statement(owner, view).is_ok());
        assert!(writer.finish_read_only(owner, b).is_ok());
        owner.check_idle().unwrap();
        Reread {
            ready_q,
            q,
            points,
            rows,
        }
    }
}

/// P2's and P3's expected observations (the history: CREATE is q 1, the
/// literal's Commit q 2).
pub(super) fn expected_written() -> Written {
    Written {
        ready_q: 1,
        row: ROW,
        q: 2,
    }
}

pub(super) fn expected_reread() -> Reread {
    Reread {
        ready_q: 2,
        q: 2,
        points: vec![Some(VALUE), None],
        rows: vec![(ROW, VALUE)],
    }
}

/// MCB05-L03 in memory: P1's files after C's CREATE and Commit, P2's
/// recovery and literal ingress, P3's recovery and B read.
#[test]
fn mcb05_l03_literal_after_reopen_is_read_by_b_after_recovery() {
    case("MCB05-L03", || {
        let literal = Literal::load();
        let oracle = Oracle::new();
        let images = with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, _core, _fence, state| {
                let _ = reader_private;
                let _ = create_table(live, owner, flusher, status, undo, private, true);
                state.lock().unwrap().images()
            }
        );
        let directory = clone_directory(&images);
        let state = directory.state();
        let file = |name: &str| state.lock().unwrap().files[name].bytes.clone();
        let written = reopen(
            &directory,
            &mut Write {
                literal: &literal,
                file: &file,
            },
        );
        assert_eq!(written, expected_written());
        let after = state.lock().unwrap().images();
        let directory = clone_directory(&after);
        let reread = reopen(&directory, &mut Read);
        assert_eq!(reread, expected_reread());
        println!("MCB05_L03 written={written:?} reread={reread:?}");
    });
}
