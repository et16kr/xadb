//! S9 C+B literal rollback transitions (consumers §8.3 live fixture,
//! boundaries `#row-mi03-s9`): a failed statement (MCB05-L04) and a full
//! rollback (MCB05-L05) of the S8 literal row, with no A. C's actual Begin,
//! RowReserve and the S8 literal Forward write RowId 1 (LSN 15, 16); C's
//! `start_row_rollback` writes RollbackStart and selects the rollback task;
//! the independent literal `tests/fixtures/dev3-module-c2/row-read-rollback/`
//! (the N1 History model, never an XADB encoder) proposes the CLR frame and
//! its five after images through the task's producer lane; C checks and
//! applies it, then writes StatementFinish (and later the Commit, q 2) or
//! AbortEnd. Every frame C writes equals the author's, and the pages C
//! installs equal its after images. B observes the failed statement: its
//! own row is Missing on the next statement and after the end, by point and
//! cursor. The live Prune of a committed root is not observable here: C has
//! no live Row retirement (boundaries `#row-mi03-s9`), only recovery's.
use super::row_literal::{Literal as Forward, int_of, wal};
use super::row_literal_two::page;
use super::row_preflight::create_table;
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RollbackScope, RollbackStep, RowOperation, RowOwnedError,
        RowPageRole, RowPreflightStep, RowProducerLane, RowProposal, TableStep, TransactionOptions,
    },
    row::read::driver::{
        CursorStep, PointRead,
        live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
    },
};
use foundation::{identity::RowId, runtime::Cancellation};
use std::collections::BTreeMap;

/// The literal directory, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c2/row-read-rollback";
/// The row the S8 literal writes and the WAL complete LSN after its Forward.
const ROW: u64 = 1;
const VALUE: i64 = 7;
const PREVIOUS_LSN: u64 = 16;
/// The page each CLR after image replaces (expected.json `pages`).
const AFTER: [(RowPageRole, u64, &str); 5] = [
    (RowPageRole::Root, 1, "after-root.bin"),
    (RowPageRole::RootJob, 2, "after-jobs.bin"),
    (RowPageRole::Status, 11, "after-status.bin"),
    (RowPageRole::Undo, 14, "after-undo.bin"),
    (RowPageRole::RowLeaf, 13, "after-leaf.bin"),
];

/// One case's pins: (case id, directory, the q after the case, its frames
/// in LSN order, its files).
struct Pins {
    id: &'static str,
    dir: &'static str,
    q: u64,
    frames: &'static [(&'static str, usize, &'static str)],
    files: &'static [(&'static str, usize, &'static str)],
}

const STATEMENT: Pins = Pins {
    id: "MCB05-L04",
    dir: "statement",
    q: 2,
    frames: &[
        (
            "rollback-start",
            3_040,
            "879123a519126eb797da5ba7de9311713b0fde2bc9b76db9706f10bab0c3c3e1",
        ),
        (
            "clr",
            91_289,
            "99d0d9fbd79b5153d8b65a952ed607db203a80605aafc961e33d9a458b4c56ef",
        ),
        (
            "statement-finish",
            3_040,
            "7d8f2a4ce14e9072db5f8a452de092744a3e7eea064797f5d7b3cfbf0c96cdde",
        ),
        (
            "commit",
            56_712,
            "caa9afda0d0c256ac86b18cd02f6549dad6c76b03666591f8f78e941cb9dccda",
        ),
    ],
    files: &[
        (
            "after-jobs.bin",
            PAGE_BYTES,
            "7fcf9f27db4218bf0b17dc2e6c8421749bfab0cfec344c635b18d5151f3d7417",
        ),
        (
            "after-leaf.bin",
            PAGE_BYTES,
            "86870861f1449695f314f938012b94cef5d33ccde1dc10fbe881b14b8bb9375d",
        ),
        (
            "after-root.bin",
            PAGE_BYTES,
            "2319d1e3237bcf87bc42e859445eb90fec6ba07e6e9ca604c654419cb16d51a1",
        ),
        (
            "after-status.bin",
            PAGE_BYTES,
            "209211315623dbc5bffbfcaa7fc891583289800e4f9fda2c4c2aca369b98f52e",
        ),
        (
            "after-undo.bin",
            PAGE_BYTES,
            "bf067dc8b60f3bc67b5fd5ff11dc1a49c57e6b33de188d0d0b31ba3dca407524",
        ),
    ],
};

const FULL: Pins = Pins {
    id: "MCB05-L05",
    dir: "full",
    q: 1,
    frames: &[
        (
            "rollback-start",
            3_040,
            "b4af98e28211d0ef990d2e54767b9e046fed100197b91f01f29df9b9ba156d92",
        ),
        (
            "clr",
            91_289,
            "caa18deac3fecd9b9c51d861c12b25aa4ca547f087a766cf2947ec3a19f9f4e0",
        ),
        (
            "abort-end",
            56_712,
            "6d83741e6afd832e93fa0ca76eba65054f0cce56b380f7721c8e57d8fef4eaf5",
        ),
    ],
    files: &[
        (
            "after-jobs.bin",
            PAGE_BYTES,
            "7fcf9f27db4218bf0b17dc2e6c8421749bfab0cfec344c635b18d5151f3d7417",
        ),
        (
            "after-leaf.bin",
            PAGE_BYTES,
            "86870861f1449695f314f938012b94cef5d33ccde1dc10fbe881b14b8bb9375d",
        ),
        (
            "after-root.bin",
            PAGE_BYTES,
            "2319d1e3237bcf87bc42e859445eb90fec6ba07e6e9ca604c654419cb16d51a1",
        ),
        (
            "after-status.bin",
            PAGE_BYTES,
            "c8bade9f1d0e99040e3956c4130b8f2dfea78cd2197a923b4db88d4df7780d13",
        ),
        (
            "after-undo.bin",
            PAGE_BYTES,
            "bf067dc8b60f3bc67b5fd5ff11dc1a49c57e6b33de188d0d0b31ba3dca407524",
        ),
    ],
};

/// The author's directory.
fn author() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR)
}

/// One case's literal files by name, each checked against its pin here and
/// in expected.json.
struct Rollback(BTreeMap<String, Vec<u8>>);

impl Rollback {
    fn load(pins: &Pins) -> Self {
        let root = author();
        let expected = String::from_utf8(read(&root.join("expected.json"), 1 << 16)).unwrap();
        for fact in [
            format!("\"case\": \"{}\"", pins.id),
            format!("\"previous_lsn\": {PREVIOUS_LSN}"),
            format!("\"row\": {ROW}"),
            format!("\"value\": {VALUE}"),
        ] {
            assert!(expected.contains(&fact), "expected.json lacks {fact}");
        }
        let dir = root.join("literal").join(pins.dir);
        let mut files = BTreeMap::new();
        for (index, (name, bytes, digest)) in pins.frames.iter().enumerate() {
            let fact = format!(
                "\"bytes\": {bytes},\n          \"lsn\": {},\n          \"name\": \"{name}\",\n          \"sha256\": \"{digest}\"",
                PREVIOUS_LSN + 1 + index as u64
            );
            assert!(expected.contains(&fact), "expected.json frame {name}");
            files.insert(format!("{name}.bin"), (*bytes, *digest));
        }
        for (name, bytes, digest) in pins.files {
            files.insert(name.to_string(), (*bytes, *digest));
        }
        Self(
            files
                .into_iter()
                .map(|(name, (bytes, digest))| {
                    let data = read(&dir.join(&name), 1 << 17);
                    assert_eq!((data.len(), hex(&data).as_str()), (bytes, digest), "{name}");
                    assert!(
                        expected.contains(&format!(
                            "\"{name}\": {{\n          \"bytes\": {bytes},\n          \"sha256\": \"{digest}\""
                        )),
                        "expected.json pin of {} {name}",
                        pins.dir
                    );
                    (name, data)
                })
                .collect(),
        )
    }

    fn frame(&self, name: &str) -> &[u8] {
        &self.0[&format!("{name}.bin")]
    }

    /// The literal CLR frame and after images in the rollback task's
    /// producer lane, the images in C's declared physical order.
    fn clr(&self, lane: RowProducerLane<'_>) -> RowProposal {
        let (context, _control, _before, _schema, frame, after, _candidate, _scratch) =
            lane.parts();
        assert_eq!(context.operation(), RowOperation::Clr);
        let bytes = self.frame("clr");
        frame[..bytes.len()].copy_from_slice(bytes);
        let order = context.after_order();
        assert_eq!(order.len(), 5, "a CLR of a first root changes five pages");
        let mut roles = [RowPageRole::Root; 5];
        for (index, role) in order.iter().enumerate() {
            let (_, _, name) = AFTER.iter().find(|(r, ..)| r == role).unwrap();
            after[index].copy_from_slice(&self.0[*name]);
            roles[index] = *role;
        }
        RowProposal::new(RowOperation::Clr, bytes.len(), 5, roles)
    }
}

/// A successful Row step.
macro_rules! ok {
    ($result:expr, $what:literal) => {
        match $result {
            Ok(done) => done,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("{} {error:?}", $what)
            }
        }
    };
}

/// The WAL bytes from `from` on, split into the case's next frames: each
/// must equal the author's.
fn frames_since(state: &State, from: usize, literal: &Rollback, names: &[&str]) -> usize {
    let wal = wal(state);
    let mut at = from;
    for name in names {
        let expected = literal.frame(name);
        assert_eq!(
            hex(&wal[at..(at + expected.len()).min(wal.len())]),
            hex(expected),
            "C's {name} frame"
        );
        at += expected.len();
    }
    assert_eq!(at, wal.len(), "no other frame");
    at
}

/// MCB05-L04 (`statement`) or L05 (`full`): the S8 literal row, then C's
/// rollback with the literal CLR; every C frame is the author's and the
/// CLR's pages its after images. B reads RowId 1 as Missing after it.
fn rollback(pins: &Pins) {
    case(pins.id, || {
        let forward = Forward::load();
        let literal = Rollback::load(pins);
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, tx, view, table);
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                let mut preflight = match writer
                    .preflight_row(owner, &view, &table, private, &mut flush)
                    .unwrap()
                {
                    RowPreflightStep::Ready(preflight) => preflight,
                    RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
                };
                let plan = forward.plan(preflight.validation_lane().unwrap());
                let mut reserved = ok!(preflight.reserve(plan), "reserve");
                let (_, _, row_id, _, lsn) = reserved.test_issued();
                assert_eq!((row_id.get(), lsn), (ROW, PREVIOUS_LSN - 1));
                let proposal = forward.forward(reserved.producer_lane().unwrap());
                let prepared = ok!(reserved.prepare(proposal), "prepare");
                let _ = ok!(prepared.apply(), "apply");
                assert_eq!(
                    point!(reader, owner, reader_private, &view, &table, ROW),
                    Some(VALUE),
                    "own Active write"
                );
                let pool = reader.catalog_test_pool();
                let statement = pins.dir == "statement";
                // C's rollback of `$view`'s scope with the literal CLR: the
                // WAL gains the author's RollbackStart, CLR and finish frame,
                // and the CLR's pages are its after images. Returns the
                // finish frame's name and the WAL length after it.
                macro_rules! literal_rollback {
                    ($view:expr, $scope:expr, $finish:literal) => {{
                        let from = wal(state).len();
                        let mut rollback = writer
                            .start_row_rollback(
                                owner, &tx, $view, $scope, private, &mut flush, flusher,
                            )
                            .unwrap();
                        let at = frames_since(state, from, &literal, &["rollback-start"]);
                        let mut clrs = 0;
                        loop {
                            let target = match rollback.next().unwrap() {
                                RollbackStep::Candidate(mut task) => {
                                    let proposal = literal.clr(task.producer_lane().unwrap());
                                    let prepared = ok!(task.prepare(proposal), "CLR prepare");
                                    let applied = ok!(prepared.apply(), "CLR apply");
                                    assert_eq!(applied.operation(), RowOperation::Clr);
                                    clrs += 1;
                                    None
                                }
                                RollbackStep::FlushRequired { target } => Some(target),
                                RollbackStep::Complete => break,
                            };
                            if let Some(target) = target {
                                rollback.flush(&target).unwrap();
                            }
                            assert!(clrs <= 1);
                        }
                        assert_eq!(clrs, 1);
                        let at = frames_since(state, at, &literal, &["clr"]);
                        for (_, number, name) in AFTER {
                            assert_eq!(page(pool, number), hex(&literal.0[name]), "{name}");
                        }
                        ok!(rollback.finish(), "rollback finish");
                        ($finish, frames_since(state, at, &literal, &[$finish]))
                    }};
                }
                let (finish, q) = if statement {
                    let (finish, at) = literal_rollback!(
                        Some(&view),
                        RollbackScope::Statement,
                        "statement-finish"
                    );
                    assert!(view.q().is_err(), "the failed statement is not reused");
                    assert!(writer.finish_statement(owner, view).is_ok());
                    // B on the transaction's next statement: the failed
                    // statement's row is gone.
                    let next = writer.begin_statement(owner, &tx).unwrap();
                    assert_eq!(
                        point!(reader, owner, reader_private, &next, &table, ROW),
                        None
                    );
                    assert_eq!(
                        scan!(reader, owner, reader_private, &next, &table, flush, flusher),
                        vec![]
                    );
                    assert!(writer.finish_statement(owner, next).is_ok());
                    let q = match writer.commit_row(owner, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(_) => panic!("Commit after the failed statement"),
                    };
                    frames_since(state, at, &literal, &["commit"]);
                    (finish, q)
                } else {
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let (finish, _) =
                        literal_rollback!(None, RollbackScope::Transaction, "abort-end");
                    assert!(writer.finish_aborted(owner, tx).is_ok());
                    (finish, 1)
                };
                assert_eq!(q, pins.q);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b_owner = &mut b_owner;
                let b = read_only!(
                    writer,
                    b_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let new = writer.begin_statement(b_owner, &b).unwrap();
                assert_eq!(new.q().unwrap(), pins.q);
                assert_eq!(
                    point!(reader, b_owner, reader_private, &new, &table, ROW),
                    None
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
                    vec![]
                );
                assert!(writer.finish_statement(b_owner, new).is_ok());
                assert!(writer.finish_read_only(b_owner, b).is_ok());
                b_owner.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                println!("{} finish={finish} q={q} rows=[]", pins.id);
            }
        );
    });
}

/// MCB05-L04: a failed statement of the literal row, compensated by the
/// literal CLR; the transaction then commits with no row (q 2).
#[test]
fn mcb05_l04_failed_statement_literal_clr_then_commit() {
    rollback(&STATEMENT);
}

/// MCB05-L05: the full rollback of the literal row with the literal CLR,
/// then AbortEnd.
#[test]
fn mcb05_l05_full_rollback_literal_clr_then_abort_end() {
    rollback(&FULL);
}
