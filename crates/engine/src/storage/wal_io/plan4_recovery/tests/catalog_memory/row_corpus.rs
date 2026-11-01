//! S10 C+B six-row corpus (consumers §9.2 MCB01, MCB02; boundaries
//! `#row-mi03-s10`): rows a..f of table T joined to C's actual Commit
//! receipt, written without A. C's actual Begin, seven RowReserves and one
//! Cancel issue every identity; the independent literal
//! `tests/fixtures/dev3-module-c2/row-read-six/` (the N1 History model,
//! never an XADB encoder) proposes the six canonical rows and Forwards. C
//! checks every byte and installs it, and C's Commit publishes q 2. B's
//! point and cursor drivers then read the corpus. Nothing here links A: the
//! module is in the C+B single-source export.
//!
//! The corpus, in issue order (expected.json `steps`): a RowId 1 Int64 10,
//! b 2/20, the cancelled reservation RowId 3 (the gap), c 4/30, d 5/40,
//! e 6/50, f 7/60. The live state before the first RowReserve is C's actual
//! state (table T, the Row actor on Status 11, RowLeaf 13 empty, Free Undo
//! 14..20, complete LSN 26), pinned by the author and checked here against
//! the pool and control first. The author's expected.json is pinned here by
//! SHA-256 and pins every live and literal file; every RowReserve and the
//! Cancel C writes are the model's frames, every Forward and after image
//! the literal's.
//!
//! Expected reads are the author's rows and the gap, never C's or B's
//! output: order, values, RowIds and the absence of duplicates and gaps are
//! compared as whole typed lists.
use super::row_literal::{Literal as Single, effects, int_of, wal};
use super::row_literal_rollback::{Rollback, STATEMENT, frames_since};
use super::row_literal_two::page;
use super::row_preflight::{create_table, frames, system_init, with_pins};
use super::*;
use crate::storage::{
    disk::{
        IsolationLevel, ReadStep, RetentionCause, RetentionError, RetentionPhase, RollbackScope,
        RollbackStep, RowInputPlan, RowOperation, RowOwnedError, RowPageRole, RowPreflightStep,
        RowProducerLane, RowProposal, RowValidationLane, TableStep, TransactionFinishError,
        TransactionOptions,
    },
    row::read::{
        ReadError,
        driver::{
            CursorStep, PointRead, RowReadError,
            live::{LiveCall as ReadCall, next_live, open_cursor_live, read_point_live},
        },
    },
};
use crate::transaction::concurrency::catalog_test_support::{self as hook, Point, Resume};
use foundation::{identity::RowId, runtime::Cancellation};
use std::{
    collections::BTreeMap,
    sync::{Mutex, mpsc::sync_channel},
    time::Duration,
};

/// The literal directory, from the repository root.
const AUTHOR: &str = "tests/fixtures/dev3-module-c2/row-read-six";
/// SHA-256 of the author's expected.json, which pins every other file.
const EXPECTED: &str = "1d5a961628e7cd80b52742df100d267a4746920e3c08d5be7c5d9952c34e42dc";
/// The WAL complete LSN before the first RowReserve (expected.json
/// `previous_lsn`).
const PREVIOUS_LSN: u64 = 26;
/// The Free Undo containers of the live state (expected.json `pages`).
const FREE_UNDO: [u64; 7] = [14, 15, 16, 17, 18, 19, 20];
/// The corpus (expected.json `rows`): symbol, RowId, Int64 value.
pub(super) const ROWS: [(char, u64, i64); 6] = [
    ('a', 1, 10),
    ('b', 2, 20),
    ('c', 4, 30),
    ('d', 5, 40),
    ('e', 6, 50),
    ('f', 7, 60),
];
/// The cancelled reservation's RowId (expected.json `gap`).
pub(super) const GAP: u64 = 3;
/// Issue order (expected.json `steps`): the Forward's index into ROWS, or
/// None for the cancelled reservation, and the Free Undo it reserves.
const STEPS: [(Option<usize>, u64); 7] = [
    (Some(0), 14),
    (Some(1), 15),
    (None, 16),
    (Some(2), 16),
    (Some(3), 17),
    (Some(4), 18),
    (Some(5), 19),
];
/// The live inputs the author re-encodes (its `LIVE` pins), in the order
/// `live_inputs!` reads them.
const LIVE: [&str; 15] = [
    "root1.bin",
    "jobs2.bin",
    "status11.bin",
    "leaf13.bin",
    "undo14.bin",
    "undo15.bin",
    "undo16.bin",
    "undo17.bin",
    "undo18.bin",
    "undo19.bin",
    "undo20.bin",
    "bank.bin",
    "tx.bin",
    "aging.bin",
    "profile.bin",
];
/// The Aging row's control slot.
const AGING: usize = 32 * 1664;
/// The pages a Forward changes, by role and file stem; the Undo page is the
/// reservation's.
const AFTER: [(RowPageRole, &str, u64); 5] = [
    (RowPageRole::Root, "root", 1),
    (RowPageRole::RootJob, "jobs", 2),
    (RowPageRole::Status, "status", 11),
    (RowPageRole::Undo, "undo", 0),
    (RowPageRole::RowLeaf, "leaf", 13),
];
/// C's Commit of the corpus.
pub(super) const Q: u64 = 2;

fn author() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(AUTHOR)
}

/// The author's files, each checked against expected.json, which is
/// checked against `EXPECTED`.
pub(super) struct Corpus {
    expected: String,
    files: BTreeMap<String, Vec<u8>>,
}

impl Corpus {
    pub(super) fn load() -> Self {
        let root = author();
        let bytes = read(&root.join("expected.json"), 1 << 16);
        assert_eq!(hex(&bytes), EXPECTED, "expected.json pin");
        let expected = String::from_utf8(bytes).unwrap();
        let rows = ROWS
            .iter()
            .map(|(symbol, row, value)| {
                format!("    [\n      \"{symbol}\",\n      {row},\n      {value}\n    ]")
            })
            .collect::<Vec<_>>()
            .join(",\n");
        for fact in [
            "\"case\": \"MCB01\"".to_string(),
            format!("\"previous_lsn\": {PREVIOUS_LSN}"),
            format!("\"rows\": [\n{rows}\n  ]"),
            format!("\"gap\": [\n    {GAP}\n  ]"),
        ] {
            assert!(expected.contains(&fact), "expected.json lacks {fact}");
        }
        let mut files = BTreeMap::new();
        for name in LIVE {
            let data = read(&root.join("live").join(name), 1 << 17);
            assert!(
                expected.contains(&format!(
                    "\"dev3-module-c2/row-read-six/live/{name}\": \"{}\"",
                    hex(&data)
                )),
                "expected.json pin of live {name}"
            );
            files.insert(format!("live/{name}"), data);
        }
        let mut names = vec!["canonical-gap.bin".to_string()];
        for r in 1..=ROWS.len() {
            names.push(format!("canonical{r}.bin"));
            names.push(format!("forward{r}-frame.bin"));
            for (_, stem, _) in AFTER {
                names.push(format!("after{r}-{stem}.bin"));
            }
        }
        for name in names {
            let data = read(&root.join("literal").join(&name), 1 << 17);
            assert!(
                expected.contains(&format!(
                    "\"{name}\": {{\n      \"bytes\": {},\n      \"sha256\": \"{}\"",
                    data.len(),
                    hex(&data)
                )),
                "expected.json pin of {name}"
            );
            files.insert(name, data);
        }
        Self { expected, files }
    }

    fn file(&self, name: &str) -> &[u8] {
        &self.files[name]
    }

    /// The model's frame of step `op` at `lsn` for `row` (expected.json
    /// `steps`) is `bytes`. A Forward also names its H: None (the first
    /// root) or the previous Forward's RowId (`h`); a Reserve and a Forward
    /// name their Undo container.
    fn step(
        &self,
        op: &str,
        lsn: u64,
        row: u64,
        h: Option<Option<u64>>,
        undo: Option<u64>,
        bytes: &[u8],
    ) {
        let h = h.map_or(String::new(), |h| {
            format!(
                "\n      \"h\": {},",
                h.map_or("null".to_string(), |row| row.to_string())
            )
        });
        let undo = undo.map_or(String::new(), |undo| format!(",\n      \"undo\": {undo}"));
        let fact = format!(
            "\"bytes\": {},{h}\n      \"lsn\": {lsn},\n      \"op\": \"{op}\",\n      \"row\": {row},\n      \"sha256\": \"{}\"{undo}\n    }}",
            bytes.len(),
            hex(bytes)
        );
        assert!(
            self.expected.contains(&fact),
            "the model's {op} at LSN {lsn}"
        );
    }

    /// The canonical row of step `forward` (None: the gap) in the
    /// validation lane; the untrusted plan names its length, SHA-256 and the
    /// lane's SchemaEpoch.
    fn plan(&self, forward: Option<usize>, lane: RowValidationLane<'_>) -> RowInputPlan {
        let canonical = match forward {
            Some(index) => self.file(&format!("canonical{}.bin", index + 1)),
            None => self.file("canonical-gap.bin"),
        };
        let (context, _schema, _leaf, candidate) = lane.parts();
        candidate[..canonical.len()].copy_from_slice(canonical);
        RowInputPlan::new(
            canonical.len(),
            context.schema_epoch(),
            foundation::codec::content_digest(canonical),
        )
    }

    /// Forward `index`'s frame and after images in the producer lane, the
    /// images in C's declared physical order.
    fn forward(&self, index: usize, lane: RowProducerLane<'_>) -> RowProposal {
        let r = index + 1;
        let (context, _control, _before, _schema, frame, after, _candidate, _scratch) =
            lane.parts();
        assert_eq!(context.operation(), RowOperation::Forward);
        let bytes = self.file(&format!("forward{r}-frame.bin"));
        frame[..bytes.len()].copy_from_slice(bytes);
        let order = context.after_order();
        assert_eq!(order.len(), 5, "a Forward changes five pages");
        let mut roles = [RowPageRole::Root; 5];
        for (at, role) in order.iter().enumerate() {
            let (_, stem, _) = AFTER.iter().find(|(r, ..)| r == role).unwrap();
            after[at].copy_from_slice(self.file(&format!("after{r}-{stem}.bin")));
            roles[at] = *role;
        }
        RowProposal::new(RowOperation::Forward, bytes.len(), 5, roles)
    }
}

/// The files the corpus statement checks: C's WAL and the profile, from the
/// memory namespace's state or a native namespace's directory.
pub(super) trait CorpusFiles {
    fn wal_bytes(&self) -> Vec<u8>;
    fn profile(&self) -> Vec<u8>;
}

impl CorpusFiles for State {
    fn wal_bytes(&self) -> Vec<u8> {
        wal(self)
    }
    fn profile(&self) -> Vec<u8> {
        self.lock().unwrap().files["d3-profile"].bytes.clone()
    }
}

/// B's point read of `row` on `view`: Ok(value or None) or B's error.
macro_rules! point_of {
    ($reader:expr, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let mut output = [0xA5; 64];
        match read_point_live(
            ReadCall {
                port: $reader,
                owner: $owner,
                private: $private,
                residency: &mut residency,
            },
            $view,
            $table,
            RowId::new($row).unwrap(),
            &mut output,
            &Cancellation::default(),
        ) {
            Ok(ReadStep::Ready(PointRead::Found(copy))) => {
                assert_eq!(copy.version().row.get(), $row, "the row B selected");
                Ok(Some(int_of(&output, copy.len())))
            }
            Ok(ReadStep::Ready(PointRead::Missing)) => {
                assert_eq!(output, [0xA5; 64], "Missing publishes no bytes");
                Ok(None)
            }
            Ok(ReadStep::FlushRequired { .. }) => panic!("resident read sources"),
            Err(error) => Err(error),
        }
    }};
}

/// B's cursor over `[start, end)` on `view`: Ok((RowId, value) in cursor
/// order) or B's error. After End, one more `next` must be End again.
macro_rules! range_of {
    ($reader:expr, $owner:expr, $private:expr, $view:expr, $table:expr, $start:expr, $end:expr) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let cancel = Cancellation::default();
        let mut output = [0u8; 64];
        let start: Option<u64> = $start;
        let end: Option<u64> = $end;
        match open_cursor_live(
            $view,
            $table,
            start.map(|row| RowId::new(row).unwrap()),
            end.map(|row| RowId::new(row).unwrap()),
        ) {
            Err(error) => Err(error),
            Ok(mut cursor) => {
                let mut rows = Vec::new();
                let mut result = Ok(());
                let mut ended = false;
                for _ in 0..16 {
                    match next_live(
                        &mut cursor,
                        ReadCall {
                            port: $reader,
                            owner: $owner,
                            private: $private,
                            residency: &mut residency,
                        },
                        &mut output,
                        &cancel,
                    ) {
                        Ok(ReadStep::Ready(CursorStep::Row(copy))) => {
                            rows.push((copy.version().row.get(), int_of(&output, copy.len())))
                        }
                        Ok(ReadStep::Ready(CursorStep::End)) => {
                            ended = true;
                            break;
                        }
                        Ok(ReadStep::FlushRequired { .. }) => panic!("resident read sources"),
                        Err(error) => {
                            result = Err(error);
                            break;
                        }
                    }
                }
                if result.is_ok() {
                    assert!(ended, "bounded cursor");
                    assert!(matches!(
                        next_live(
                            &mut cursor,
                            ReadCall {
                                port: $reader,
                                owner: $owner,
                                private: $private,
                                residency: &mut residency,
                            },
                            &mut output,
                            &cancel,
                        ),
                        Ok(ReadStep::Ready(CursorStep::End))
                    ));
                }
                cursor.close().unwrap();
                result.map(|()| rows)
            }
        }
    }};
}

/// The corpus statement: T, seven Free Undo containers, the Row actor's
/// statement and table (the live state, checked against the author's pins),
/// then the seven steps through C's checked ingress. The statement stays
/// open; the six rows are the actor's own Active writes.
macro_rules! corpus {
    ($corpus:expr, $live:ident, $owner:ident, $flusher:ident, $status:ident, $undo:ident,
     $private:ident, $reader_private:ident, $state:ident
     => $writer:ident, $reader:ident, $flush:ident, $tx:ident, $view:ident, $table:ident,
     $issued:ident, $targets:ident, $spare:ident) => {
        corpus!($corpus, $live, $owner, $flusher, $status, $undo, $private, $reader_private,
            $state => $writer, $reader, $flush, $tx, $view, $table, $issued, $targets, $spare,
            lsn, 0..STEPS.len());
    };
    ($corpus:expr, $live:ident, $owner:ident, $flusher:ident, $status:ident, $undo:ident,
     $private:ident, $reader_private:ident, $state:ident
     => $writer:ident, $reader:ident, $flush:ident, $tx:ident, $view:ident, $table:ident,
     $issued:ident, $targets:ident, $spare:ident, $lsn:ident, $steps:expr) => {
        let (row_status, first) =
            create_table($live, $owner, $flusher, $status, $undo, $private, true);
        let mut undo_pages = vec![first.unwrap().identity().page.page_no];
        for _ in 1..FREE_UNDO.len() {
            undo_pages.push(
                system_init($live, $owner, $flusher, true)
                    .identity()
                    .page
                    .page_no,
            );
        }
        assert_eq!(undo_pages, FREE_UNDO);
        // C's own targets of every page it can name before the split (each
        // resident page, Root and the five Jobs pages), for the residency
        // cases (MCB03): the page to evict and a non-resident one to LOAD.
        let mut $targets: BTreeMap<u64, Plan4Target> = BTreeMap::new();
        for number in 1..=FREE_UNDO[FREE_UNDO.len() - 1] {
            if let Some(index) = resident_index($live, number) {
                let target = $live.row_test_resident_target(index).unwrap();
                assert_eq!(target.identity().page.page_no, number);
                $targets.insert(number, target);
            }
        }
        for page in [Plan4Page::Root]
            .into_iter()
            .chain((0..5).map(|index| Plan4Page::Jobs { index }))
        {
            let target = $live.target(page).unwrap();
            $targets.insert(target.identity().page.page_no, target);
        }
        let $spare = ();
        let mut $issued: Vec<(RowId, crate::storage::undo_ref::UndoRef)> = Vec::new();
        row_statement!($live, $owner, $flusher, $private, $reader_private, row_status
            => $writer, $reader, $flush, $tx, $view, $table);
        let _ = settle!($writer, $owner, $view, $table, $private, $flush, $flusher);
        {
            let pool = $reader.catalog_test_pool();
            let bank = $writer.recovery_test_bank();
            let mut inputs: Vec<Vec<u8>> = [1, 2, 11, 13]
                .iter()
                .chain(FREE_UNDO.iter())
                .map(|number| page_bytes(pool, *number))
                .collect();
            inputs.push($writer.catalog_test_jobs().as_flattened().to_vec());
            inputs.push(bank[..1536].to_vec());
            inputs.push(bank[AGING..AGING + 1536].to_vec());
            inputs.push($state.profile());
            for (name, live) in LIVE.iter().zip(&inputs) {
                assert_eq!(
                    hex(live),
                    hex($corpus.file(&format!("live/{name}"))),
                    "live input {name}"
                );
            }
        }
        assert_eq!(
            $writer.free_undo_test_state().unwrap().0.complete_lsn,
            PREVIOUS_LSN
        );
        #[allow(unused_mut)]
        let mut $lsn = PREVIOUS_LSN;
        corpus_steps!($corpus, $state, $writer, $reader, $owner, $view, $table, $private, $flush,
            $flusher, $issued, $lsn, $steps);
    };
}

/// Steps `$steps` (a range of `STEPS`) of the corpus statement through C's
/// checked ingress, `$lsn` the complete LSN before them: every RowReserve
/// and the Cancel are the model's frames, every Forward and after image the
/// literal's.
macro_rules! corpus_steps {
    ($corpus:expr, $state:ident, $writer:ident, $reader:ident, $owner:ident, $view:ident,
     $table:ident, $private:ident, $flush:ident, $flusher:ident, $issued:ident, $lsn:ident,
     $steps:expr) => {
        for (step, undo) in STEPS[$steps].iter().copied() {
            let from = $state.wal_bytes().len();
            let _ = settle!($writer, $owner, $view, $table, $private, $flush, $flusher);
            let mut preflight = match $writer
                .preflight_row($owner, &$view, &$table, $private, &mut $flush)
                .unwrap()
            {
                RowPreflightStep::Ready(preflight) => preflight,
                RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
            };
            let plan = $corpus.plan(step, preflight.validation_lane().unwrap());
            let mut reserved = done!(preflight.reserve(plan));
            $lsn += 1;
            let (_, _, row_id, undo_ref, issued) = reserved.test_issued();
            let row = row_id.get();
            assert_eq!(
                (row, undo_ref.page().page_no, issued),
                (step.map_or(GAP, |index| ROWS[index].1), undo, $lsn)
            );
            $corpus.step(
                "reserve",
                $lsn,
                row,
                None,
                Some(undo),
                &$state.wal_bytes()[from..],
            );
            let from = $state.wal_bytes().len();
            $lsn += 1;
            let Some(index) = step else {
                if reserved.cancel().is_err() {
                    panic!("C's Cancel of the gap reservation refused");
                }
                $corpus.step("cancel", $lsn, row, None, None, &$state.wal_bytes()[from..]);
                continue;
            };
            let proposal = $corpus.forward(index, reserved.producer_lane().unwrap());
            let prepared = done!(reserved.prepare(proposal));
            let applied = done!(prepared.apply());
            let (observation, _) = applied.into_parts();
            assert_eq!(observation.operation(), RowOperation::Forward);
            assert_eq!((observation.row(), observation.undo()), (row_id, undo_ref));
            $issued.push((row_id, undo_ref));
            let gained = $state.wal_bytes()[from..].to_vec();
            assert_eq!(
                gained,
                $corpus.file(&format!("forward{}-frame.bin", index + 1)),
                "the WAL holds the literal Forward {}",
                ROWS[index].0
            );
            let h = index.checked_sub(1).map(|previous| ROWS[previous].1);
            $corpus.step("forward", $lsn, row, Some(h), Some(undo), &gained);
            let pool = $reader.catalog_test_pool();
            for (_, stem, number) in AFTER {
                let number = if number == 0 { undo } else { number };
                assert_eq!(
                    page(pool, number),
                    hex($corpus.file(&format!("after{}-{stem}.bin", index + 1))),
                    "after{}-{stem}",
                    index + 1
                );
            }
        }
    };
}

/// One pool page's bytes, through the diagnostic probe.
fn page_bytes(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    number: u64,
) -> Vec<u8> {
    let index =
        probe::registered_index(pool, number).unwrap_or_else(|| panic!("page {number} resident"));
    let mut bytes = boxed::<PAGE_BYTES>();
    probe::diagnose(pool, index, &mut bytes);
    bytes.to_vec()
}

/// The corpus as (RowId, value), optionally within `[start, end)`.
fn expected(start: Option<u64>, end: Option<u64>) -> Vec<(u64, i64)> {
    ROWS.iter()
        .filter(|(_, row, _)| start.is_none_or(|s| *row >= s) && end.is_none_or(|e| *row < e))
        .map(|(_, row, value)| (*row, *value))
        .collect()
}

/// The symbol's (RowId, value).
fn of(symbol: char) -> (u64, i64) {
    let (_, row, value) = ROWS.iter().find(|(s, ..)| *s == symbol).unwrap();
    (*row, *value)
}

/// The corpus committed (q 2), then `read` under a new read-only
/// Serializable snapshot (q 2) on a separate owner: MCB01's committed
/// corpus joined to the actual receipt.
macro_rules! committed {
    ($id:expr, |$reader:ident, $b:ident, $private:ident, $new:ident, $table:ident| $read:block) => {
        case($id, || {
            let corpus = Corpus::load();
            let oracle = Oracle::new();
            with_fresh!(
                oracle,
                frames = 28,
                |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                    corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                        state => writer, reader, flush, tx, view, table, issued, targets, spare);
                    let _ = (&issued, &targets, &spare);
                    assert!(writer.finish_statement(owner, view).is_ok());
                    let q = match writer.commit_row(owner, tx, private) {
                        Ok(committed) => committed.q(),
                        Err(_) => panic!("C's Commit of the corpus refused"),
                    };
                    assert_eq!(q, Q);
                    let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let $b = &mut b_owner;
                    let c = read_only!(writer, $b, IsolationLevel::Serializable, reader_private);
                    let $new = writer.begin_statement($b, &c).unwrap();
                    assert_eq!($new.q().unwrap(), Q);
                    {
                        let $reader = &reader;
                        let $private = &mut *reader_private;
                        let $table = &table;
                        let _ = &flush;
                        $read
                    }
                    assert!(writer.finish_statement($b, $new).is_ok());
                    assert!(writer.finish_read_only($b, c).is_ok());
                    $b.check_idle().unwrap();
                    owner.check_idle().unwrap();
                    assert!(!fence.is_fenced());
                    let _ = flusher;
                }
            );
        });
    };
}

/// MCB01-V01: T committed with no row (before the corpus): a read-only
/// snapshot reads every corpus RowId and the gap Missing, and the full and
/// `[b, e)` cursors end at once.
#[test]
fn mcb01_v01_empty_table_point_and_cursor() {
    case("MCB01-V01", || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let _ = create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                let new = writer.begin_statement(b, &c).unwrap();
                assert_eq!(new.q().unwrap(), 1);
                let table = issue_table!(reader, b, new, reader_private, flush, flusher);
                for (_, row, _) in ROWS {
                    assert_eq!(
                        point_of!(&reader, b, reader_private, &new, &table, row).unwrap(),
                        None
                    );
                }
                assert_eq!(
                    point_of!(&reader, b, reader_private, &new, &table, GAP).unwrap(),
                    None
                );
                assert_eq!(
                    range_of!(&reader, b, reader_private, &new, &table, None, None).unwrap(),
                    vec![]
                );
                assert_eq!(
                    range_of!(
                        &reader,
                        b,
                        reader_private,
                        &new,
                        &table,
                        Some(of('b').0),
                        Some(of('e').0)
                    )
                    .unwrap(),
                    vec![]
                );
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, c).is_ok());
                b.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                println!("MCB01_V01 rows=[]");
            }
        );
    });
}

/// MCB01-V02: the gap (RowId 3, issued and cancelled between b and c) is
/// Missing by point; its neighbours b and c are found.
#[test]
fn mcb01_v02_gap_point_is_missing() {
    committed!("MCB01-V02", |reader, b, private, new, table| {
        let gap = point_of!(reader, b, private, &new, table, GAP).unwrap();
        let around = [of('b'), of('c')]
            .map(|(row, _)| point_of!(reader, b, private, &new, table, row).unwrap());
        assert_eq!(gap, None);
        assert_eq!(around, [Some(of('b').1), Some(of('c').1)]);
        println!("MCB01_V02 gap={GAP} point={gap:?} around={around:?}");
    });
}

/// MCB01-V03: the first row a by point.
#[test]
fn mcb01_v03_first_row_point() {
    committed!("MCB01-V03", |reader, b, private, new, table| {
        let (row, value) = of('a');
        assert_eq!(
            point_of!(reader, b, private, &new, table, row).unwrap(),
            Some(value)
        );
        println!("MCB01_V03 a=({row},{value})");
    });
}

/// MCB01-V04: a middle row d by point.
#[test]
fn mcb01_v04_middle_row_point() {
    committed!("MCB01-V04", |reader, b, private, new, table| {
        let (row, value) = of('d');
        assert_eq!(
            point_of!(reader, b, private, &new, table, row).unwrap(),
            Some(value)
        );
        println!("MCB01_V04 d=({row},{value})");
    });
}

/// MCB01-V05: the last row f by point; the RowId after it is Missing.
#[test]
fn mcb01_v05_last_row_point() {
    committed!("MCB01-V05", |reader, b, private, new, table| {
        let (row, value) = of('f');
        assert_eq!(
            point_of!(reader, b, private, &new, table, row).unwrap(),
            Some(value)
        );
        assert_eq!(
            point_of!(reader, b, private, &new, table, row + 1).unwrap(),
            None
        );
        println!("MCB01_V05 f=({row},{value})");
    });
}

/// MCB01-V06: the full cursor returns a..f in RowId order, once each, with
/// no gap row, and every point read agrees with it.
#[test]
fn mcb01_v06_full_scan_in_order() {
    committed!("MCB01-V06", |reader, b, private, new, table| {
        let rows = range_of!(reader, b, private, &new, table, None, None).unwrap();
        assert_eq!(rows, expected(None, None));
        for (row, value) in &rows {
            assert_eq!(
                point_of!(reader, b, private, &new, table, *row).unwrap(),
                Some(*value)
            );
        }
        println!("MCB01_V06 rows={rows:?}");
    });
}

/// MCB01-V07: the `[b, e)` cursor returns b, c, d (the gap skipped, e
/// excluded).
#[test]
fn mcb01_v07_half_open_range_scan() {
    committed!("MCB01-V07", |reader, b, private, new, table| {
        let (start, end) = (of('b').0, of('e').0);
        let rows = range_of!(reader, b, private, &new, table, Some(start), Some(end)).unwrap();
        assert_eq!(rows, expected(Some(start), Some(end)));
        assert_eq!(rows, vec![of('b'), of('c'), of('d')]);
        println!("MCB01_V07 [{start},{end}) rows={rows:?}");
    });
}

/// MCB01-V08: the empty range `[c, c)` opens and ends at once (End); the
/// reversed range `[e, b)` is refused at open (InvalidRange), with no
/// cursor credit taken.
#[test]
fn mcb01_v08_empty_and_reversed_range() {
    committed!("MCB01-V08", |reader, b, private, new, table| {
        let (c, e, bb) = (of('c').0, of('e').0, of('b').0);
        let empty = range_of!(reader, b, private, &new, table, Some(c), Some(c)).unwrap();
        assert_eq!(empty, vec![]);
        let reversed = range_of!(reader, b, private, &new, table, Some(e), Some(bb));
        assert!(
            matches!(reversed, Err(RowReadError::Read(ReadError::InvalidRange))),
            "{reversed:?}"
        );
        // The refused open left nothing: the full cursor still opens.
        assert_eq!(
            range_of!(reader, b, private, &new, table, None, None).unwrap(),
            expected(None, None)
        );
        println!("MCB01_V08 empty=End reversed={reversed:?}");
    });
}

/// One row read by point and by a one-row cursor `[row, row + 1)` on the
/// same view: the two answers must agree. Returns the point answer.
macro_rules! both_of {
    ($reader:expr, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr) => {{
        let row: u64 = $row;
        let point = point_of!($reader, $owner, $private, $view, $table, row).unwrap();
        let cursor = range_of!(
            $reader,
            $owner,
            $private,
            $view,
            $table,
            Some(row),
            Some(row + 1)
        )
        .unwrap();
        assert_eq!(
            cursor,
            point
                .map(|value| (row, value))
                .into_iter()
                .collect::<Vec<_>>(),
            "point and cursor of RowId {row}"
        );
        point
    }};
}

/// MCB02-V01..V05: the corpus statement, with a foreign read-only
/// Serializable transaction begun after the six Forwards and before the
/// Commit (snapshot q 1). V01: the writer's own statement reads c (own
/// Active). V02: the foreign statement reads c Missing (foreign Active).
/// C's Commit publishes q 2. V03: the foreign statement still reads c
/// Missing (committed q 2 above its snapshot q 1), and its transaction ends
/// Rejected (SerializationConflict). V04: a new snapshot (q 2) reads c
/// (q equal). V05: an empty Row transaction then commits q 3, and a new
/// snapshot (q 3) reads c (q below). Each answer is the point read and the
/// one-row cursor, which must agree.
fn visibility(id: &str, variant: u8) {
    case(id, || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&issued, &targets, &spare);
                let (c_row, c_value) = of('c');
                let own = both_of!(&reader, owner, reader_private, &view, &table, c_row);
                if variant == 1 {
                    assert_eq!(own, Some(c_value), "own Active");
                    println!("MCB02_V01 own c={own:?}");
                }
                let mut f_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let f = &mut f_owner;
                let mut f_private = boxed::<PRIVATE_BYTES>();
                let f_private = &mut *f_private;
                let foreign = read_only!(writer, f, IsolationLevel::Serializable, f_private);
                let old = writer.begin_statement(f, &foreign).unwrap();
                assert_eq!(old.q().unwrap(), 1);
                let f_table = issue_table!(reader, f, old, f_private, flush, flusher);
                let active = both_of!(&reader, f, f_private, &old, &f_table, c_row);
                if variant == 2 {
                    assert_eq!(active, None, "foreign Active");
                    println!("MCB02_V02 foreign c={active:?}");
                }
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = match writer.commit_row(owner, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("C's Commit of the corpus refused"),
                };
                assert_eq!(q, Q);
                let above = both_of!(&reader, f, f_private, &old, &f_table, c_row);
                if variant == 3 {
                    assert_eq!(above, None, "committed q 2 above snapshot q 1");
                    println!("MCB02_V03 q={q} snapshot=1 c={above:?}");
                }
                assert!(writer.finish_statement(f, old).is_ok());
                let end = writer.finish_read_only(f, foreign);
                assert!(matches!(
                    end,
                    Err(TransactionFinishError::Rejected(ref error))
                        if matches!(error.cause, RetentionCause::SerializationConflict)
                ));
                f.check_idle().unwrap();
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                let new = writer.begin_statement(b, &c).unwrap();
                assert_eq!(new.q().unwrap(), Q);
                let equal = both_of!(&reader, b, reader_private, &new, &table, c_row);
                if variant == 4 {
                    assert_eq!(equal, Some(c_value), "q 2 equal to snapshot q 2");
                    println!("MCB02_V04 q={q} snapshot={q} c={equal:?}");
                }
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, c).is_ok());
                b.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mcb02_v01_own_active_point_and_cursor() {
    visibility("MCB02-V01", 1);
}

#[test]
fn mcb02_v02_foreign_active_point_and_cursor() {
    visibility("MCB02-V02", 2);
}

#[test]
fn mcb02_v03_committed_q_above_snapshot_point_and_cursor() {
    visibility("MCB02-V03", 3);
}

#[test]
fn mcb02_v04_committed_q_equal_snapshot_point_and_cursor() {
    visibility("MCB02-V04", 4);
}

/// MCB02-V05: the corpus committed (q 2); then a new Status (C's actual
/// INIT) and an empty Row transaction on it, whose Commit publishes q 3. A
/// new read-only snapshot (q 3) reads c, committed at q 2 below it, by
/// point and one-row cursor.
#[test]
fn mcb02_v05_committed_q_below_snapshot_point_and_cursor() {
    case("MCB02-V05", || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&issued, &targets, &spare);
                let _ = &table;
                assert!(writer.finish_statement(owner, view).is_ok());
                match writer.commit_row(owner, tx, private) {
                    Ok(committed) => assert_eq!(committed.q(), Q),
                    Err(_) => panic!("C's Commit of the corpus refused"),
                }
                let _ = (reader, flush);
                let later = system_init(live, owner, flusher, false);
                let (mut writer, reader, mut flush) = live.split();
                let empty = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        &later,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let statement = writer.begin_statement(owner, &empty).unwrap();
                assert!(writer.finish_statement(owner, statement).is_ok());
                let q = match writer.commit_row(owner, empty, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("the empty Row Commit refused"),
                };
                assert_eq!(q, Q + 1);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                let new = writer.begin_statement(b, &c).unwrap();
                assert_eq!(new.q().unwrap(), Q + 1);
                let table = issue_table!(reader, b, new, reader_private, flush, flusher);
                let (c_row, c_value) = of('c');
                let below = both_of!(&reader, b, reader_private, &new, &table, c_row);
                assert_eq!(below, Some(c_value), "q 2 below snapshot q 3");
                assert_eq!(
                    range_of!(&reader, b, reader_private, &new, &table, None, None).unwrap(),
                    expected(None, None)
                );
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, c).is_ok());
                b.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                println!("MCB02_V05 q_row={Q} snapshot={q} c={below:?}");
            }
        );
    });
}

/// MCB02-V08 (the publication gap): the corpus statement finished, then
/// C's Commit stopped after its WAL is durable and its Status installed,
/// before Q is published (`Point::BeforePublish`): c's status q 2 is above
/// the published Q 1. A foreign read-only Serializable statement taken
/// before the Commit (q 1) reads c Missing by point and one-row cursor on
/// its own thread there and after the Commit returns; its transaction ends
/// Rejected (SerializationConflict); a new snapshot (q 2) then reads c.
#[test]
fn mcb02_v08_publication_gap_point_and_cursor() {
    const WAIT: Duration = Duration::from_secs(10);
    case("MCB02-V08", || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&issued, &targets, &spare);
                assert!(writer.finish_statement(owner, view).is_ok());
                let (c_row, c_value) = of('c');
                let mut b_private = boxed::<PRIVATE_BYTES>();
                let cell = Mutex::new(&mut writer);
                let mut q = 0;
                std::thread::scope(|scope| {
                    let (cell, reader) = (&cell, &reader);
                    let (publish_in, publishing) = sync_channel(1);
                    let (publish_go, publish_release) = sync_channel(1);
                    hook::install(core, Point::BeforePublish, publish_in, publish_release);
                    let (ready_tx, ready) = sync_channel(1);
                    let (done_tx, done) = sync_channel::<()>(1);
                    let b_private = &mut *b_private;
                    let before = wal(state).len();
                    let b = scope.spawn(move || {
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::Serializable,
                            b_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        assert_eq!(view.q().unwrap(), 1);
                        let table = match reader
                            .catalog_statement_call(&mut o, &view)
                            .unwrap()
                            .finish_table(
                                ObjectId::new(6).unwrap(),
                                &mut Plan4ResidencyCursor::new(),
                                b_private,
                            )
                            .unwrap()
                        {
                            TableStep::Table(table) => table,
                            _ => panic!("resident committed T"),
                        };
                        ready_tx.send(()).unwrap();
                        publishing.recv_timeout(WAIT).unwrap();
                        assert!(wal(state).len() > before, "the Commit frame is durable");
                        let during = both_of!(reader, &mut o, b_private, &view, &table, c_row);
                        publish_go.send(Resume::Continue).unwrap();
                        done.recv_timeout(WAIT).unwrap();
                        let after = both_of!(reader, &mut o, b_private, &view, &table, c_row);
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        let end = w.finish_read_only(&mut o, tx);
                        assert!(matches!(
                            end,
                            Err(TransactionFinishError::Rejected(ref error))
                                if matches!(error.cause, RetentionCause::SerializationConflict)
                        ));
                        o.check_idle().unwrap();
                        (during, after)
                    });
                    ready.recv_timeout(WAIT).unwrap();
                    {
                        let mut w = cell.lock().unwrap();
                        q = match w.commit_row(owner, tx, private) {
                            Ok(committed) => committed.q(),
                            Err(_) => panic!("C's Commit of the corpus refused"),
                        };
                    }
                    done_tx.send(()).unwrap();
                    let (during, after) = b.join().unwrap();
                    println!("MCB02_V08 during={during:?} after={after:?}");
                    assert_eq!(during, None, "status q 2 above published Q 1");
                    assert_eq!(after, None, "snapshot q 1 after the Commit");
                });
                assert_eq!(q, Q);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                let new = writer.begin_statement(b, &c).unwrap();
                assert_eq!(new.q().unwrap(), Q);
                assert_eq!(
                    both_of!(&reader, b, reader_private, &new, &table, c_row),
                    Some(c_value)
                );
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, c).is_ok());
                b.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                let _ = (flush, flusher);
            }
        );
    });
}

/// The census of B's read source at a return: (source bytes in use, Txn/
/// Undo reference pins and whether a retirement marker is published,
/// physical page pins, admitted read operations, fence). Every MCB03 return
/// is compared with its exact value; idle is `IDLE`.
macro_rules! census {
    ($reader:expr, $writer:expr, $core:expr, $fence:expr) => {
        (
            $reader.catalog_test_source_used(),
            $reader.catalog_test_references(),
            $writer.catalog_test_physical_pins(),
            hook::observe($core).active_read_operations,
            $fence.is_fenced(),
        )
    };
}
/// The residency cases' pool: 19 frames for the corpus database's 20 pages.
const FRAMES: usize = 20;
/// The `census!` tuple (its fields are named there).
type Census = (u64, ((usize, usize, usize), bool), u16, usize, bool);
const IDLE: Census = (0, ((0, 0, 0), false), 0, 0, false);

/// Makes the resident page `number` non-resident inside the split: C's own
/// flush of its target (now clean), then, with every other clean frame
/// pinned by `holder`, one actual checked LOAD of the non-resident `spare`
/// (`RowReadOperation::test_load`), whose only eligible victim is that page.
/// The pool has no empty frame (`FRAMES` holds one page fewer than the
/// corpus database has).
macro_rules! evict_in_split {
    ($reader:ident, $owner:expr, $holder:expr, $flush:ident, $flusher:ident, $view:expr,
     $private:expr, $targets:expr, $spare:expr, $number:expr) => {{
        let number: u64 = $number;
        let _ = $spare;
        flush_target!($flush, $flusher, $targets[&number]);
        let pool = $reader.catalog_test_pool();
        let all = frames(pool, FRAMES);
        assert_eq!(all.len(), FRAMES - 1, "no empty frame: {all:?}");
        let spare = $targets
            .values()
            .find(|target| probe::registered_index(pool, target.identity().page.page_no).is_none())
            .copied()
            .unwrap_or_else(|| panic!("a non-resident page among {:?}", $targets.keys()));
        let others: Vec<usize> = all
            .iter()
            .filter(|(_, page, dirty)| !*dirty && *page != number)
            .map(|(index, ..)| *index)
            .collect();
        with_pins(pool, $holder, &others, || {
            let mut op = $reader.row_operation($owner, $view, $private).unwrap();
            let step = op.test_load(&spare).unwrap();
            assert!(
                matches!(step, RetentionResidencyStep::Resident { .. }),
                "{step:?}"
            );
            op.finish().unwrap();
        });
        assert!(
            probe::registered_index(pool, number).is_none(),
            "page {number} evicted"
        );
    }};
}

/// B's point read that may LOAD: Ok((value or None, the FlushRequired
/// targets it returned, each flushed by C's flusher before the retry)).
macro_rules! point_loading {
    ($reader:expr, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr,
     $flush:ident, $flusher:ident) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let mut output = [0xA5; 64];
        let mut flushed = Vec::new();
        loop {
            assert!(flushed.len() < 4, "bounded LOAD retries");
            match read_point_live(
                ReadCall {
                    port: $reader,
                    owner: $owner,
                    private: $private,
                    residency: &mut residency,
                },
                $view,
                $table,
                RowId::new($row).unwrap(),
                &mut output,
                &Cancellation::default(),
            ) {
                Ok(ReadStep::Ready(PointRead::Found(copy))) => {
                    assert_eq!(copy.version().row.get(), $row);
                    break Ok((Some(int_of(&output, copy.len())), flushed));
                }
                Ok(ReadStep::Ready(PointRead::Missing)) => {
                    assert_eq!(output, [0xA5; 64]);
                    break Ok((None, flushed));
                }
                Ok(ReadStep::FlushRequired { target }) => {
                    assert_eq!(output, [0xA5; 64], "FlushRequired publishes no bytes");
                    flushed.push(target.identity().page.page_no);
                    flush_target!($flush, $flusher, target);
                }
                Err(error) => break Err(error),
            }
        }
    }};
}

/// MCB03-V03..V06 on the corpus statement (`FRAMES`: the pool holds all
/// but one page of the corpus database). `page` is made non-resident by `evict_in_split!`; then B
/// reads c: the writer's own statement for the RowLeaf (V03) and for the
/// dirty-victim case (V06), a foreign read-only ReadCommitted statement for
/// Status (V04) and Undo (V05), which need c's Active status and its Undo
/// for Missing-before. V06 reads with every clean frame pinned, so the only
/// victims are dirty: B returns FlushRequired with no effect, C flushes it,
/// and the retry on the same view (same q) LOADs the page. The census is
/// IDLE at every return, the page is resident again, and the WAL and files
/// are unchanged by the reads.
fn residency(id: &str, page: u64, foreign: bool, dirty_only: bool) {
    case(id, || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 20,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = &issued;
                let (c_row, c_value) = of('c');
                let mut holder = core.try_owner(OwnerRole::Foreground).unwrap();
                evict_in_split!(
                    reader,
                    owner,
                    &mut holder,
                    flush,
                    flusher,
                    &view,
                    reader_private,
                    targets,
                    spare,
                    page
                );
                holder.check_idle().unwrap();
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                let q = view.q().unwrap();
                let before = effects(state);
                let (read, flushed) = if foreign {
                    let mut f_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                    let f = &mut f_owner;
                    let mut f_private = boxed::<PRIVATE_BYTES>();
                    let f_private = &mut *f_private;
                    let other = read_only!(writer, f, IsolationLevel::ReadCommitted, f_private);
                    let other_view = writer.begin_statement(f, &other).unwrap();
                    let f_table = issue_table!(reader, f, other_view, f_private, flush, flusher);
                    let read = point_loading!(
                        &reader,
                        f,
                        f_private,
                        &other_view,
                        &f_table,
                        c_row,
                        flush,
                        flusher
                    )
                    .unwrap();
                    assert_eq!(census!(reader, writer, core, fence), IDLE);
                    assert!(writer.finish_statement(f, other_view).is_ok());
                    assert!(writer.finish_read_only(f, other).is_ok());
                    f.check_idle().unwrap();
                    read
                } else if dirty_only {
                    let pool = reader.catalog_test_pool();
                    let clean: Vec<usize> = frames(pool, FRAMES)
                        .into_iter()
                        .filter(|(_, _, dirty)| !*dirty)
                        .map(|(index, ..)| index)
                        .collect();
                    let mut pins = core.try_owner(OwnerRole::Foreground).unwrap();
                    let first = with_pins(pool, &mut pins, &clean, || {
                        let mut residency = Plan4ResidencyCursor::new();
                        let mut output = [0xA3; 64];
                        let step = read_point_live(
                            ReadCall {
                                port: &reader,
                                owner,
                                private: reader_private,
                                residency: &mut residency,
                            },
                            &view,
                            &table,
                            RowId::new(c_row).unwrap(),
                            &mut output,
                            &Cancellation::default(),
                        )
                        .unwrap();
                        assert_eq!(output, [0xA3; 64]);
                        match step {
                            ReadStep::FlushRequired { target } => target,
                            ReadStep::Ready(_) => panic!("only dirty victims"),
                        }
                    });
                    pins.check_idle().unwrap();
                    assert_eq!(census!(reader, writer, core, fence), IDLE);
                    assert_eq!(effects(state), before, "FlushRequired has no effect");
                    assert_eq!(view.q().unwrap(), q);
                    let victim = first.identity().page.page_no;
                    flush_target!(flush, flusher, first);
                    let (value, more) = point_loading!(
                        &reader,
                        owner,
                        reader_private,
                        &view,
                        &table,
                        c_row,
                        flush,
                        flusher
                    )
                    .unwrap();
                    let mut flushed = vec![victim];
                    flushed.extend(more);
                    (value, flushed)
                } else {
                    point_loading!(
                        &reader,
                        owner,
                        reader_private,
                        &view,
                        &table,
                        c_row,
                        flush,
                        flusher
                    )
                    .unwrap()
                };
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                assert_eq!(view.q().unwrap(), q, "the same snapshot");
                let expected = if foreign { None } else { Some(c_value) };
                assert_eq!(read, expected);
                if dirty_only {
                    assert_eq!(flushed.len(), 1, "one dirty victim flushed: {flushed:?}");
                } else {
                    assert!(flushed.is_empty(), "a clean victim: {flushed:?}");
                    assert_eq!(
                        effects(state),
                        before,
                        "a read LOAD has no WAL or file effect"
                    );
                }
                assert!(
                    probe::registered_index(reader.catalog_test_pool(), page).is_some(),
                    "page {page} loaded"
                );
                println!(
                    "{id} page={page} foreign={foreign} read={read:?} flushed={flushed:?} q={q}"
                );
                let _ = settle!(writer, owner, view, table, private, flush, flusher);
                assert!(writer.finish_statement(owner, view).is_ok());
                match writer.commit_row(owner, tx, private) {
                    Ok(committed) => assert_eq!(committed.q(), Q),
                    Err(_) => panic!("C's Commit of the corpus refused"),
                }
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mcb03_v03_row_leaf_nonresident_loads() {
    residency("MCB03-V03", 13, false, false);
}

#[test]
fn mcb03_v04_status_nonresident_loads() {
    residency("MCB03-V04", 11, true, false);
}

#[test]
fn mcb03_v05_undo_nonresident_loads() {
    residency("MCB03-V05", 16, true, false);
}

#[test]
fn mcb03_v06_dirty_victim_flush_required_then_same_snapshot() {
    residency("MCB03-V06", 13, false, true);
}

/// MCB03-V01: c's Undo U carries a published retirement marker (C's
/// `row_test_with_retiring_undo`, the retirement registry's own state) when
/// B's own-statement read captures c: Busy (Visibility, Unchanged), no
/// output, no pin installed, the marker seen, no fence. Without the marker
/// the same read finds c. The census is exact at both returns.
#[test]
fn mcb03_v01_capture_under_retirement_is_busy() {
    case("MCB03-V01", || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&targets, &spare);
                let (c_row, c_value) = of('c');
                let (_, c_undo) = issued.iter().find(|(row, _)| row.get() == c_row).unwrap();
                let before = effects(state);
                let mut busy = None;
                reader.row_test_with_retiring_undo(*c_undo, || {
                    let read = point_of!(&reader, owner, reader_private, &view, &table, c_row);
                    assert_eq!(
                        census!(reader, writer, core, fence),
                        (0, ((0, 0, 0), true), 0, 0, false)
                    );
                    busy = Some(read);
                });
                let busy = busy.unwrap();
                assert!(
                    matches!(
                        busy,
                        Err(RowReadError::Source(RetentionError {
                            effect: Effect::Unchanged,
                            cause: RetentionCause::Visibility(ref e),
                            ..
                        })) if e.cause == crate::storage::wal_io::VisibilityCause::Busy
                    ),
                    "{busy:?}"
                );
                assert_eq!(effects(state), before);
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                assert_eq!(
                    point_of!(&reader, owner, reader_private, &view, &table, c_row).unwrap(),
                    Some(c_value)
                );
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                println!("MCB03_V01 busy={busy:?}");
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.commit_row(owner, tx, private).is_ok());
                let _ = (flush, flusher);
            }
        );
    });
}

/// MCB03-V07: c's Undo cell is damaged in its frame (grammar changed, page
/// identity, LSN, envelope and checksum valid; no replacement authority).
/// A foreign read-only ReadCommitted read of c needs that U for c's
/// Missing-before: C's census refuses it (Census, CallerMustResolve, Undo)
/// and fences; B returns that error, not Missing, with no output, no WAL or
/// file effect and no pin or operation left.
#[test]
fn mcb03_v07_damaged_required_undo_is_not_missing() {
    use crate::storage::{
        dirty_frame::status_load_test_support,
        page::{PageKind, decode_page, encode_page},
    };
    case("MCB03-V07", || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&targets, &spare, &flush, &flusher);
                let (c_row, _) = of('c');
                let (_, c_undo) = *issued.iter().find(|(row, _)| row.get() == c_row).unwrap();
                let pool = reader.catalog_test_pool();
                let number = c_undo.page().page_no;
                let index = probe::registered_index(pool, number).unwrap();
                let identity = pool.target_request(index).unwrap().identity;
                status_load_test_support::edit_content(pool, index, |bytes| {
                    let page = decode_page(bytes, identity, PageKind::Undo).unwrap();
                    let header = page.header();
                    let mut cells = (0..page.len())
                        .map(|i| page.cell(i).unwrap().to_vec())
                        .collect::<Vec<_>>();
                    cells[c_undo.slot() as usize][0] ^= 1;
                    let refs = cells.iter().map(Vec::as_slice).collect::<Vec<_>>();
                    encode_page(header, &refs, bytes).unwrap();
                    decode_page(bytes, identity, PageKind::Undo).unwrap();
                });
                let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                let transaction = writer
                    .begin_transaction(
                        &mut other,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let other_view = writer.begin_statement(&mut other, &transaction).unwrap();
                let before = effects(state);
                let read = point_of!(
                    &reader,
                    &mut other,
                    reader_private,
                    &other_view,
                    &table,
                    c_row
                );
                assert!(
                    matches!(
                        read,
                        Err(RowReadError::Source(RetentionError {
                            phase: RetentionPhase::Census,
                            effect: Effect::CallerMustResolve,
                            cause: RetentionCause::Undo(_),
                        }))
                    ),
                    "{read:?}"
                );
                assert!(fence.is_fenced());
                assert_eq!(effects(state), before);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                println!("MCB03_V07 read={read:?}");
                drop(other_view);
                drop(transaction);
                drop(view);
                drop(tx);
            }
        );
    });
}

/// B's thread table issue on its own view (the committed T is resident).
macro_rules! thread_table {
    ($reader:expr, $owner:expr, $view:expr, $private:expr) => {
        match $reader
            .catalog_statement_call($owner, $view)
            .unwrap()
            .finish_table(
                ObjectId::new(6).unwrap(),
                &mut Plan4ResidencyCursor::new(),
                $private,
            )
            .unwrap()
        {
            TableStep::Table(table) => table,
            _ => panic!("resident committed T"),
        }
    };
}

fn installation_pending(error: &RowReadError) -> bool {
    matches!(
        error,
        RowReadError::Source(RetentionError {
            cause: RetentionCause::Concurrency(error),
            effect: Effect::Unchanged,
            ..
        }) if error.cause == crate::transaction::concurrency::Cause::InstallationPending
    )
}

/// MCB03-V02 and V08 (C+B, under the S2-0 terminal read gate, as MCI03-V01
/// reads it; boundaries `#row-mi03-s10`): the S8 literal row (RowId 1,
/// Active) and its L04 literal CLR, no A. B1's foreign read of row 1 is
/// paused after its source capture (`Point::RowReadCaptured`), holding the
/// row's Txn and U pins. C's statement rollback (RollbackStart with the
/// literal CLR) closes the terminal read gate and waits for B1
/// (`Point::ReadGateWaiting`): no WAL while B1 holds its pins. A late
/// reader B2 is refused Unchanged InstallationPending meanwhile. When B1
/// returns, the rollback writes the author's RollbackStart, CLR and
/// StatementFinish; B2's retry reads row 1 Missing; the transaction commits
/// q 2 with no row. V02 checks the CLR's wait and its frames; V08 checks
/// B1's two pins and the refused extra claim.
fn gate(id: &str, variant: u8) {
    const WAIT: Duration = Duration::from_secs(10);
    case(id, || {
        let single = Single::load();
        let literal = Rollback::load(&STATEMENT);
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
                let plan = single.plan(preflight.validation_lane().unwrap());
                let mut reserved = done!(preflight.reserve(plan));
                let proposal = single.forward(reserved.producer_lane().unwrap());
                let prepared = done!(reserved.prepare(proposal));
                let applied = done!(prepared.apply());
                let (observation, _) = applied.into_parts();
                let row = observation.row();
                assert_eq!(row.get(), 1);
                let mut b1_private = boxed::<PRIVATE_BYTES>();
                let mut b2_private = boxed::<PRIVATE_BYTES>();
                let cell = Mutex::new(&mut writer);
                let used = reader.catalog_test_source_used();
                let from = wal(state).len();
                std::thread::scope(|scope| {
                    let (cell, reader) = (&cell, &reader);
                    let (b1_in, b1_captured) = sync_channel(1);
                    let (b1_go, b1_release) = sync_channel(1);
                    let (gate_in, gate_waiting) = sync_channel(1);
                    let (gate_go, gate_release) = sync_channel(1);
                    let (ready_tx, ready) = sync_channel(1);
                    let (done_tx, done) = sync_channel::<()>(1);
                    let b2_private = &mut *b2_private;
                    let b2 = scope.spawn(move || {
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::ReadCommitted,
                            b2_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        let table = thread_table!(reader, &mut o, &view, b2_private);
                        ready_tx.send(()).unwrap();
                        gate_waiting.recv_timeout(WAIT).unwrap();
                        let at_wait = hook::observe(core);
                        assert_eq!(at_wait.active_read_operations, 1);
                        assert!(at_wait.read_gate_closed && !at_wait.fenced);
                        let late = point_of!(reader, &mut o, b2_private, &view, &table, 1)
                            .err()
                            .unwrap();
                        assert!(installation_pending(&late), "{late:?}");
                        let pins_at_late = reader.catalog_test_references();
                        gate_go.send(Resume::Continue).unwrap();
                        let wal_len = wal(state).len();
                        std::thread::sleep(Duration::from_millis(200));
                        assert_eq!(wal(state).len(), wal_len, "the rollback waits for B1");
                        let still = hook::observe(core);
                        assert!(still.read_gate_closed && still.active_read_operations == 1);
                        b1_go.send(Resume::Continue).unwrap();
                        done.recv_timeout(WAIT).unwrap();
                        let retry = point_of!(reader, &mut o, b2_private, &view, &table, 1);
                        assert!(matches!(retry, Ok(None)), "{retry:?}");
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        assert!(w.finish_read_only(&mut o, tx).is_ok());
                        o.check_idle().unwrap();
                        (at_wait, wal_len, pins_at_late, format!("{late:?}"))
                    });
                    ready.recv_timeout(WAIT).unwrap();
                    hook::install(core, Point::RowReadCaptured, b1_in, b1_release);
                    let b1_private = &mut *b1_private;
                    let b1 = scope.spawn(move || {
                        use super::super::super::super::plan4_pages::apply::{
                            test_arm_capture_claim, test_capture_claim,
                        };
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::ReadCommitted,
                            b1_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        let table = thread_table!(reader, &mut o, &view, b1_private);
                        if variant == 8 {
                            test_arm_capture_claim();
                        }
                        let read = point_of!(reader, &mut o, b1_private, &view, &table, 1);
                        let claim = test_capture_claim();
                        assert!(matches!(read, Ok(None)), "foreign Active: {read:?}");
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        assert!(w.finish_read_only(&mut o, tx).is_ok());
                        o.check_idle().unwrap();
                        claim
                    });
                    b1_captured.recv_timeout(WAIT).unwrap();
                    let at_capture = hook::observe(core);
                    assert_eq!(at_capture.active_read_operations, 1);
                    assert!(!at_capture.read_gate_closed);
                    let pins = reader.catalog_test_references();
                    hook::install(core, Point::ReadGateWaiting, gate_in, gate_release);
                    {
                        let mut w = cell.lock().unwrap();
                        let w = &mut **w;
                        let mut rollback = w
                            .start_row_rollback(
                                owner,
                                &tx,
                                Some(&view),
                                RollbackScope::Statement,
                                private,
                                &mut flush,
                                flusher,
                            )
                            .unwrap();
                        let mut clrs = 0;
                        loop {
                            let target = match rollback.next().unwrap() {
                                RollbackStep::Candidate(mut task) => {
                                    let proposal = literal.clr(task.producer_lane().unwrap());
                                    let prepared = done!(task.prepare(proposal));
                                    let applied = done!(prepared.apply());
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
                        if rollback.finish().is_err() {
                            panic!("rollback finish");
                        }
                        assert!(!hook::observe(core).read_gate_closed);
                    }
                    done_tx.send(()).unwrap();
                    let claim = b1.join().unwrap();
                    let (at_wait, wal_len, pins_at_late, late) = b2.join().unwrap();
                    assert_eq!(wal_len, from, "no frame while B1 held its pins");
                    // The author's frames, in order, after B1 returned.
                    let _ = frames_since(
                        state,
                        from,
                        &literal,
                        &["rollback-start", "clr", "statement-finish"],
                    );
                    if variant == 2 {
                        println!("MCB03_V02 pins={pins:?} at_wait={at_wait:?} clr=1");
                    }
                    if variant == 8 {
                        // B1's capture installed both the Txn and the U pin.
                        // B1's own Owner then claimed its references again at
                        // the capture: the original `reserve_references`
                        // answered Busy (its two rows are in use), Unchanged,
                        // and pinned nothing. B2's late read during the wait
                        // installed none either.
                        assert_eq!(pins, ((2, 0, 0), false), "Txn and U pinned");
                        assert_eq!(
                            claim,
                            Some(Err(crate::storage::wal_io::VisibilityError {
                                cause: crate::storage::wal_io::VisibilityCause::Busy,
                                effect: Effect::Unchanged,
                            })),
                            "the same Owner's extra claim"
                        );
                        assert_eq!(pins_at_late, pins, "the refused claim pinned nothing");
                        println!("MCB03_V08 pins={pins:?} own_claim={claim:?} late={late}");
                    } else {
                        assert_eq!(claim, None, "no claim armed");
                    }
                });
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(view.q().is_err(), "the failed statement is not reused");
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = match writer.commit_row(owner, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("Commit after the failed statement"),
                };
                assert_eq!(q, 2);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                let new = writer.begin_statement(b, &c).unwrap();
                assert_eq!(
                    point_of!(&reader, b, reader_private, &new, &table, 1).unwrap(),
                    None
                );
                assert_eq!(
                    range_of!(&reader, b, reader_private, &new, &table, None, None).unwrap(),
                    vec![]
                );
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, c).is_ok());
                b.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

#[test]
fn mcb03_v02_clr_of_pinned_undo_waits_for_the_read() {
    gate("MCB03-V02", 2);
}

#[test]
fn mcb03_v08_two_pins_then_extra_claim_refused() {
    gate("MCB03-V08", 8);
}

/// MCB03-V01 in the frozen order, supporting evidence only (S11; boundaries
/// `#row-mi03-s11`: PARTIAL, the receipt below is a test-built authority the
/// consumers contract forbids as a PASS basis, review R2): the S8
/// literal row (RowId 1, Active, no A) and B1's foreign ReadCommitted read
/// of it, paused after its source capture (`Point::RowReadCaptured`) with
/// the row's Txn and U pins held. A retirement of that U is then tried
/// through the registry's original `begin_retirement` (C's cfg(test)
/// `row_test_begin_undo_free`: the writer's actual version hold, a
/// test-built UndoFree receipt, past the S2-0 terminal read gate that keeps
/// every live retirement from this point): Busy, Unchanged, with no
/// marker, no pin change, no WAL and no fence. B1 then returns its answer
/// (foreign Active: none) and its pins; the same attempt is then accepted
/// (and cancelled at once), so the pin census was the refusal. The
/// transaction commits q 2 and row 1 is read.
#[test]
fn mcb03_v01_retirement_after_capture_is_busy() {
    const WAIT: Duration = Duration::from_secs(10);
    case("MCB03-V01", || {
        let single = Single::load();
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
                let plan = single.plan(preflight.validation_lane().unwrap());
                let mut reserved = done!(preflight.reserve(plan));
                let proposal = single.forward(reserved.producer_lane().unwrap());
                let prepared = done!(reserved.prepare(proposal));
                let applied = done!(prepared.apply());
                let (observation, _) = applied.into_parts();
                assert_eq!(observation.row().get(), 1);
                let u = observation.undo();
                let actor = view.own_actor().unwrap();
                let idle = reader.catalog_test_references();
                let mut b1_private = boxed::<PRIVATE_BYTES>();
                let cell = Mutex::new(&mut writer);
                let used = reader.catalog_test_source_used();
                let from = wal(state).len();
                let (busy, pins) = std::thread::scope(|scope| {
                    let (cell, reader) = (&cell, &reader);
                    let (b1_in, b1_captured) = sync_channel(1);
                    let (b1_go, b1_release) = sync_channel(1);
                    hook::install(core, Point::RowReadCaptured, b1_in, b1_release);
                    let b1_private = &mut *b1_private;
                    let b1 = scope.spawn(move || {
                        let mut o = core.try_owner(OwnerRole::Foreground).unwrap();
                        let tx = read_only!(
                            cell.lock().unwrap(),
                            &mut o,
                            IsolationLevel::ReadCommitted,
                            b1_private
                        );
                        let view = cell.lock().unwrap().begin_statement(&mut o, &tx).unwrap();
                        let table = thread_table!(reader, &mut o, &view, b1_private);
                        let read = point_of!(reader, &mut o, b1_private, &view, &table, 1);
                        assert!(matches!(read, Ok(None)), "foreign Active: {read:?}");
                        let mut w = cell.lock().unwrap();
                        assert!(w.finish_statement(&mut o, view).is_ok());
                        assert!(w.finish_read_only(&mut o, tx).is_ok());
                        o.check_idle().unwrap();
                    });
                    b1_captured.recv_timeout(WAIT).unwrap();
                    let pins = reader.catalog_test_references();
                    assert_eq!(pins, ((2, 0, 0), false), "Txn and U pinned");
                    let busy = reader.row_test_begin_undo_free(owner, u, actor);
                    assert_eq!(
                        reader.catalog_test_references(),
                        pins,
                        "no marker and no pin change"
                    );
                    assert_eq!(wal(state).len(), from, "no WAL");
                    assert!(!fence.is_fenced());
                    let at_capture = hook::observe(core);
                    assert_eq!(at_capture.active_read_operations, 1);
                    b1_go.send(Resume::Continue).unwrap();
                    b1.join().unwrap();
                    (busy, pins)
                });
                println!("MCB03_V01 pins={pins:?} retirement={busy:?}");
                assert_eq!(
                    busy,
                    Err(crate::storage::wal_io::VisibilityError {
                        cause: crate::storage::wal_io::VisibilityCause::Busy,
                        effect: Effect::Unchanged,
                    })
                );
                assert_eq!(reader.catalog_test_references(), idle, "B1's pins returned");
                assert_eq!(
                    reader.row_test_begin_undo_free(owner, u, actor),
                    Ok(()),
                    "with no pin the same attempt is accepted"
                );
                assert_eq!(reader.catalog_test_references(), idle, "cancelled at once");
                assert_eq!(wal(state).len(), from, "no WAL");
                assert_eq!(reader.catalog_test_source_used(), used);
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = match writer.commit_row(owner, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("Commit after the refused retirement"),
                };
                assert_eq!(q, 2);
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                let new = writer.begin_statement(b, &c).unwrap();
                assert!(matches!(
                    point_of!(&reader, b, reader_private, &new, &table, 1),
                    Ok(Some(_))
                ));
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, c).is_ok());
                b.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// C's refusal of a CheckedTable of another origin (the core identity
/// check: Census, Unchanged, BindingMismatch).
fn binding_refused(error: &RowReadError) -> bool {
    matches!(
        error,
        RowReadError::Source(RetentionError {
            phase: RetentionPhase::Census,
            cause: RetentionCause::BindingMismatch,
            effect: Effect::Unchanged,
        })
    )
}

/// One cursor step with an output of `len` bytes: Ok(Some((RowId, value)))
/// for a row, Ok(None) for End, or B's error.
macro_rules! step_with {
    ($cursor:expr, $reader:expr, $owner:expr, $private:expr, $len:expr, $cancel:expr) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let mut output = vec![0xA5u8; $len];
        match next_live(
            $cursor,
            ReadCall {
                port: $reader,
                owner: $owner,
                private: $private,
                residency: &mut residency,
            },
            &mut output,
            $cancel,
        ) {
            Ok(ReadStep::Ready(CursorStep::Row(copy))) => Ok(Some((
                copy.version().row.get(),
                int_of(&output, copy.len()),
            ))),
            Ok(ReadStep::Ready(CursorStep::End)) => Ok(None),
            Ok(ReadStep::FlushRequired { .. }) => panic!("resident read sources"),
            Err(error) => Err(error),
        }
    }};
}

/// B's point read of `row` with an output of `len` bytes.
macro_rules! point_with {
    ($reader:expr, $owner:expr, $private:expr, $view:expr, $table:expr, $row:expr, $len:expr) => {{
        let mut residency = Plan4ResidencyCursor::new();
        let mut output = vec![0xA5u8; $len];
        match read_point_live(
            ReadCall {
                port: $reader,
                owner: $owner,
                private: $private,
                residency: &mut residency,
            },
            $view,
            $table,
            RowId::new($row).unwrap(),
            &mut output,
            &Cancellation::default(),
        ) {
            Ok(ReadStep::Ready(PointRead::Found(copy))) => {
                assert_eq!(copy.version().row.get(), $row);
                Ok(Some(int_of(&output, copy.len())))
            }
            Ok(ReadStep::Ready(PointRead::Missing)) => Ok(None),
            Ok(ReadStep::FlushRequired { .. }) => panic!("resident read sources"),
            Err(error) => Err(error),
        }
    }};
}

/// MCB04-V01 (foreign DB): database X (T only, no row) and the corpus
/// database Y (the corpus committed, q 2) are open at once. X's CheckedTable
/// of T has Y's T's identity scalars and another origin. On Y's read-only
/// statement, B's point reads (a, the gap, f) and cursor with X's table are
/// refused by C's core identity check (Census, Unchanged, BindingMismatch):
/// no output, no file of either database changed, neither fenced, the census
/// idle. Y's own table reads the corpus; X's table still reads X's empty T.
#[test]
fn mcb04_v01_foreign_database_table_is_refused() {
    case("MCB04-V01", || {
        let corpus = Corpus::load();
        let oracle_x = Oracle::new();
        with_fresh!(
            oracle_x,
            frames = 16,
            |live_x,
             owner_x,
             flusher_x,
             status_x,
             undo_x,
             private_x,
             reader_private_x,
             core_x,
             fence_x,
             state_x| {
                let _ = create_table(
                    live_x, owner_x, flusher_x, status_x, undo_x, private_x, true,
                );
                let (mut writer_x, reader_x, mut flush_x) = live_x.split();
                let x = read_only!(
                    writer_x,
                    owner_x,
                    IsolationLevel::Serializable,
                    reader_private_x
                );
                let view_x = writer_x.begin_statement(owner_x, &x).unwrap();
                let foreign = issue_table!(
                    reader_x,
                    owner_x,
                    view_x,
                    reader_private_x,
                    flush_x,
                    flusher_x
                );
                let files_x = state_x.lock().unwrap().images();
                let oracle = Oracle::new();
                with_fresh!(
                    oracle,
                    frames = 28,
                    |live,
                     owner,
                     flusher,
                     status,
                     undo,
                     private,
                     reader_private,
                     core,
                     fence,
                     state| {
                        corpus!(corpus, live, owner, flusher, status, undo, private,
                            reader_private, state => writer, reader, flush, tx, view, table,
                            issued, targets, spare);
                        let _ = (&issued, &targets, &spare);
                        assert!(writer.finish_statement(owner, view).is_ok());
                        let q = match writer.commit_row(owner, tx, private) {
                            Ok(committed) => committed.q(),
                            Err(_) => panic!("C's Commit of the corpus refused"),
                        };
                        assert_eq!(q, Q);
                        let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                        let b = &mut b_owner;
                        let c = read_only!(writer, b, IsolationLevel::Serializable, reader_private);
                        let new = writer.begin_statement(b, &c).unwrap();
                        assert_eq!(foreign.test_identity(), table.test_identity());
                        assert!(table.test_same_origin(core, fence));
                        assert!(!foreign.test_same_origin(core, fence));
                        assert!(foreign.test_same_origin(core_x, fence_x));
                        let before = state.lock().unwrap().images();
                        let mut refusals = Vec::new();
                        for row in [of('a').0, GAP, of('f').0] {
                            let point = point_of!(&reader, b, reader_private, &new, &foreign, row);
                            match point {
                                Err(ref error) if binding_refused(error) => {}
                                other => panic!("B's point of {row} with X's table: {other:?}"),
                            }
                            refusals.push(format!("{point:?}"));
                            assert_eq!(census!(reader, writer, core, fence), IDLE);
                        }
                        let range =
                            range_of!(&reader, b, reader_private, &new, &foreign, None, None);
                        match range {
                            Err(ref error) if binding_refused(error) => {}
                            other => panic!("B's cursor with X's table: {other:?}"),
                        }
                        assert_eq!(census!(reader, writer, core, fence), IDLE);
                        assert_eq!(
                            state.lock().unwrap().images(),
                            before,
                            "no file of Y changed"
                        );
                        assert_eq!(
                            state_x.lock().unwrap().images(),
                            files_x,
                            "no file of X changed"
                        );
                        assert!(!fence.is_fenced() && !fence_x.is_fenced());
                        assert_eq!(
                            range_of!(&reader, b, reader_private, &new, &table, None, None)
                                .unwrap(),
                            expected(None, None)
                        );
                        println!("MCB04_V01 point={refusals:?} cursor={range:?}");
                        assert!(writer.finish_statement(b, new).is_ok());
                        assert!(writer.finish_read_only(b, c).is_ok());
                        b.check_idle().unwrap();
                        owner.check_idle().unwrap();
                        let _ = (flush, flusher);
                    }
                );
                assert_eq!(
                    range_of!(
                        &reader_x,
                        owner_x,
                        reader_private_x,
                        &view_x,
                        &foreign,
                        None,
                        None
                    )
                    .unwrap(),
                    vec![]
                );
                assert!(writer_x.finish_statement(owner_x, view_x).is_ok());
                assert!(writer_x.finish_read_only(owner_x, x).is_ok());
                owner_x.check_idle().unwrap();
                assert!(!fence_x.is_fenced());
            }
        );
    });
}

/// C's recovery of `$directory` to READY with `RejectInitialLosers` (no Row
/// loser handler), as `row_reopen::reopen` runs it, inlined so that `$body`
/// can hold handles of the open that wrote the files.
macro_rules! reopened {
    ($directory:expr, |$live:ident, $owner:ident, $flusher:ident, $core:ident, $fence:ident| $body:block) => {{
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let recovery = match Plan4Recovery::acquire(
            &$directory,
            Some(namespace()),
            &mut maintenance,
            &mut dwb,
            &mut slot,
        ) {
            Ok(recovery) => recovery,
            Err(_) => panic!("acquire"),
        };
        let runtime = match recovery.recover_with_rows(RowRecoveryPolicy::RejectInitialLosers) {
            Ok(runtime) => runtime,
            Err(failed) => panic!("C+B recovery refused {:?}", failed.error()),
        };
        let core = PageConcurrency::new(runtime.session_fence()).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 28];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut closed = runtime
            .bind(&owner, &core, &mut frames, &mut operation)
            .unwrap();
        let mut ready = false;
        for _ in 0..512 {
            match closed.next_action(&mut owner, &mut flusher).unwrap() {
                RecoveryAction::Ready => {
                    ready = true;
                    break;
                }
                RecoveryAction::Progress(progress) => assert!(!progress.ready),
                RecoveryAction::RowClr(task) => {
                    panic!("C+B has no Row loser handler: {:?}", task.actor())
                }
            }
        }
        assert!(ready, "bounded C recovery exhausted");
        let (mut live, _) = match closed.into_live_with_private() {
            Ok(value) => value,
            Err(_) => panic!("original READY handoff"),
        };
        let $live = &mut live;
        let $owner = &mut owner;
        let $flusher = &mut flusher;
        let $core = &core;
        let $fence = runtime.session_fence();
        $body
    }};
}

/// MCB04-V02 (old open): open X writes the S8 literal row (RowId 1, Int64
/// 7) through C's checked ingress and commits it (q 2); open Y is C's
/// recovery of X's files to READY (the same database and history) while X
/// and its CheckedTable of T are still alive. X's table has Y's own T's
/// identity scalars and X's origin. On Y's read-only statement B's point
/// reads (the row and RowId 2) and cursor with X's table are refused
/// (Census, Unchanged, BindingMismatch), no file changed, nothing fenced,
/// the census idle. Y's own table reads the row; X's table still reads it
/// in X. (A handle of an open that has ended cannot be named: the session's
/// borrow ends it. The six-row corpus is not reopened here: C's recovery of
/// its 40-LSN history alone takes about 28 s of the 30 s bound.)
#[test]
fn mcb04_v02_old_open_table_is_refused() {
    use super::super::super::RowRecoveryPolicy;
    use super::super::support::{clone_directory, namespace};
    use super::row_literal::{ROW, VALUE};
    use super::row_reopen::ready_row_sources;
    use crate::storage::disk::RecoveryAction;
    case("MCB04-V02", || {
        let single = Single::load();
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
                let plan = single.plan(preflight.validation_lane().unwrap());
                let mut reserved = done!(preflight.reserve(plan));
                let proposal = single.forward(reserved.producer_lane().unwrap());
                let prepared = done!(reserved.prepare(proposal));
                let applied = done!(prepared.apply());
                assert_eq!(applied.into_parts().0.row().get(), ROW);
                assert!(writer.finish_statement(owner, view).is_ok());
                let q = match writer.commit_row(owner, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("C's Commit of the literal row refused"),
                };
                assert_eq!(q, 2);
                let mut x_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let x_owner = &mut x_owner;
                let x = read_only!(
                    writer,
                    x_owner,
                    IsolationLevel::Serializable,
                    reader_private
                );
                let view_x = writer.begin_statement(x_owner, &x).unwrap();
                let images = state.lock().unwrap().images();
                let directory = clone_directory(&images);
                let state_y = directory.state();
                let mut y_private = boxed::<PRIVATE_BYTES>();
                let y_private = &mut *y_private;
                reopened!(directory, |live_y, owner_y, flusher_y, core_y, fence_y| {
                    assert_eq!(live_y.catalog_test_q(), q);
                    let _ = ready_row_sources(live_y, owner_y, flusher_y);
                    let (mut writer_y, reader_y, mut flush_y) = live_y.split();
                    let y = read_only!(writer_y, owner_y, IsolationLevel::Serializable, y_private);
                    let view_y = writer_y.begin_statement(owner_y, &y).unwrap();
                    assert_eq!(view_y.q().unwrap(), q);
                    let own =
                        issue_table!(reader_y, owner_y, view_y, y_private, flush_y, flusher_y);
                    assert_eq!(table.test_identity(), own.test_identity());
                    assert!(own.test_same_origin(core_y, fence_y));
                    assert!(!table.test_same_origin(core_y, fence_y));
                    let before = state_y.lock().unwrap().images();
                    let mut refusals = Vec::new();
                    for row in [ROW, ROW + 1] {
                        let point = point_of!(&reader_y, owner_y, y_private, &view_y, &table, row);
                        match point {
                            Err(ref error) if binding_refused(error) => {}
                            other => panic!("B's point of {row} with X's table: {other:?}"),
                        }
                        refusals.push(format!("{point:?}"));
                        assert_eq!(census!(reader_y, writer_y, core_y, fence_y), IDLE);
                    }
                    let range =
                        range_of!(&reader_y, owner_y, y_private, &view_y, &table, None, None);
                    match range {
                        Err(ref error) if binding_refused(error) => {}
                        other => panic!("B's cursor with X's table: {other:?}"),
                    }
                    assert_eq!(census!(reader_y, writer_y, core_y, fence_y), IDLE);
                    assert_eq!(
                        state_y.lock().unwrap().images(),
                        before,
                        "no file of Y changed"
                    );
                    assert!(!fence_y.is_fenced() && !fence.is_fenced());
                    assert_eq!(
                        scan!(
                            reader_y, owner_y, y_private, &view_y, &own, flush_y, flusher_y
                        ),
                        vec![(ROW, VALUE)]
                    );
                    println!("MCB04_V02 point={refusals:?} cursor={range:?}");
                    assert!(writer_y.finish_statement(owner_y, view_y).is_ok());
                    assert!(writer_y.finish_read_only(owner_y, y).is_ok());
                    owner_y.check_idle().unwrap();
                });
                assert_eq!(
                    state.lock().unwrap().images(),
                    images,
                    "no file of X changed"
                );
                assert_eq!(
                    range_of!(
                        &reader,
                        x_owner,
                        reader_private,
                        &view_x,
                        &table,
                        None,
                        None
                    )
                    .unwrap(),
                    vec![(ROW, VALUE)]
                );
                assert!(writer.finish_statement(x_owner, view_x).is_ok());
                assert!(writer.finish_read_only(x_owner, x).is_ok());
                x_owner.check_idle().unwrap();
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                let _ = (flush, flusher);
            }
        );
    });
}

/// MCB04-V05 (ended statement): the S8 literal row (RowId 1, Int64 7) in
/// the writer's statement, which the writer's own point read and a cursor
/// opened then (`early`) see. C's statement rollback with the L04 literal
/// CLR then ends that statement (RollbackStart, CLR, StatementFinish, the
/// author's frames). On the ended statement B's point read, a new cursor
/// and `early`'s next are each refused with no output, no WAL or file
/// effect, the census idle and nothing fenced; the transaction's next
/// statement reads RowId 1 Missing and commits q 2. (A statement ended by
/// `finish_statement` is consumed by it, so no read can name it.)
#[test]
fn mcb04_v05_ended_statement_is_refused() {
    use super::row_literal::{ROW, VALUE};
    case("MCB04-V05", || {
        let single = Single::load();
        let literal = Rollback::load(&STATEMENT);
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
                let plan = single.plan(preflight.validation_lane().unwrap());
                let mut reserved = done!(preflight.reserve(plan));
                let proposal = single.forward(reserved.producer_lane().unwrap());
                let prepared = done!(reserved.prepare(proposal));
                let applied = done!(prepared.apply());
                assert_eq!(applied.into_parts().0.row().get(), ROW);
                assert_eq!(
                    point_of!(&reader, owner, reader_private, &view, &table, ROW).unwrap(),
                    Some(VALUE)
                );
                let cancel = Cancellation::default();
                let mut early = open_cursor_live(&view, &table, None, None).unwrap();
                let from = wal(state).len();
                {
                    let mut rollback = writer
                        .start_row_rollback(
                            owner,
                            &tx,
                            Some(&view),
                            RollbackScope::Statement,
                            private,
                            &mut flush,
                            flusher,
                        )
                        .unwrap();
                    let mut clrs = 0;
                    loop {
                        let target = match rollback.next().unwrap() {
                            RollbackStep::Candidate(mut task) => {
                                let proposal = literal.clr(task.producer_lane().unwrap());
                                let prepared = done!(task.prepare(proposal));
                                let applied = done!(prepared.apply());
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
                    if rollback.finish().is_err() {
                        panic!("rollback finish");
                    }
                }
                let _ = frames_since(
                    state,
                    from,
                    &literal,
                    &["rollback-start", "clr", "statement-finish"],
                );
                assert!(view.q().is_err(), "the statement has ended");
                let before = effects(state);
                let point = point_of!(&reader, owner, reader_private, &view, &table, ROW);
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                let late = range_of!(&reader, owner, reader_private, &view, &table, None, None);
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                let next = step_with!(&mut early, &reader, owner, reader_private, 64, &cancel);
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                println!("MCB04_V05 point={point:?} new_cursor={late:?} early_next={next:?}");
                for refused in [&point.map(|_| ()), &late.map(|_| ()), &next.map(|_| ())] {
                    assert!(
                        matches!(
                            refused,
                            Err(RowReadError::Source(RetentionError {
                                phase: RetentionPhase::Census,
                                cause: RetentionCause::WrongState,
                                effect: Effect::Unchanged,
                            }))
                        ),
                        "{refused:?}"
                    );
                }
                early.close().unwrap();
                assert_eq!(effects(state), before, "the refused reads have no effect");
                assert!(!fence.is_fenced());
                assert!(writer.finish_statement(owner, view).is_ok());
                let next = writer.begin_statement(owner, &tx).unwrap();
                assert_eq!(
                    point_of!(&reader, owner, reader_private, &next, &table, ROW).unwrap(),
                    None
                );
                assert!(writer.finish_statement(owner, next).is_ok());
                let q = match writer.commit_row(owner, tx, private) {
                    Ok(committed) => committed.q(),
                    Err(_) => panic!("Commit after the ended statement"),
                };
                assert_eq!(q, 2);
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
            }
        );
    });
}

/// MCB04-V06 (own revision change) and V11 (End, then an own mutation):
/// the corpus statement after its first two steps (a and b Forwarded). The
/// writer's own cursor over T returns a (V06), or a, b and End twice (V11).
/// The statement's remaining five steps (the gap's Reserve and Cancel, c..f)
/// then change its own writes. The cursor's next is CursorInvalidated (for
/// V11 before End), again on a second call, with no output and no effect;
/// a new cursor on the same statement reads all six rows.
fn revision(id: &str, variant: u8) {
    case(id, || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare,
                    lsn, 0..2);
                let _ = (&targets, &spare);
                let cancel = Cancellation::default();
                let mut cursor = open_cursor_live(&view, &table, None, None).unwrap();
                let mut seen = vec![
                    step_with!(&mut cursor, &reader, owner, reader_private, 64, &cancel).unwrap(),
                ];
                if variant == 11 {
                    for _ in 0..3 {
                        seen.push(
                            step_with!(&mut cursor, &reader, owner, reader_private, 64, &cancel)
                                .unwrap(),
                        );
                    }
                    assert_eq!(seen, vec![Some(of('a')), Some(of('b')), None, None]);
                } else {
                    assert_eq!(seen, vec![Some(of('a'))]);
                }
                corpus_steps!(
                    corpus,
                    state,
                    writer,
                    reader,
                    owner,
                    view,
                    table,
                    private,
                    flush,
                    flusher,
                    issued,
                    lsn,
                    2..STEPS.len()
                );
                assert_eq!(issued.len(), ROWS.len());
                let before = effects(state);
                let mut after = Vec::new();
                for _ in 0..2 {
                    let step = step_with!(&mut cursor, &reader, owner, reader_private, 64, &cancel);
                    assert!(
                        matches!(step, Err(RowReadError::Read(ReadError::CursorInvalidated))),
                        "{step:?}"
                    );
                    assert_eq!(census!(reader, writer, core, fence), IDLE);
                    after.push(format!("{step:?}"));
                }
                assert_eq!(effects(state), before);
                cursor.close().unwrap();
                assert_eq!(
                    range_of!(&reader, owner, reader_private, &view, &table, None, None).unwrap(),
                    expected(None, None)
                );
                println!("MCB04_V{variant:02} before={seen:?} after={after:?}");
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.commit_row(owner, tx, private).is_ok());
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                let _ = (flush, flusher);
            }
        );
    });
}

#[test]
fn mcb04_v06_own_revision_change_invalidates_the_cursor() {
    revision("MCB04-V06", 6);
}

#[test]
fn mcb04_v11_own_mutation_after_end_invalidates_before_end() {
    revision("MCB04-V11", 11);
}

/// MCB04-V07 (output one byte short, then the same row) and V08 (output
/// exact) on the committed corpus (q 2) under a new read-only snapshot. A
/// canonical row is 13 bytes. Point: b with 12 bytes is OutputTooSmall
/// {required: 13}; with 13 it is b. Cursor `[b, e)`: 12 bytes is
/// OutputTooSmall at b, the retry with 64 returns b (not c), 12 bytes again
/// is OutputTooSmall at c, then 13, 13 and 13 return c, d and End. No
/// reference or source byte stays in use after any return.
fn output(id: &str, variant: u8) {
    committed!(id, |reader, b, private, new, table| {
        let short = |step: &std::result::Result<Option<(u64, i64)>, RowReadError>| {
            matches!(
                step,
                Err(RowReadError::Read(ReadError::OutputTooSmall {
                    required: 13
                }))
            )
        };
        let (b_row, b_value) = of('b');
        let small = point_with!(reader, b, private, &new, table, b_row, 12);
        assert!(
            matches!(
                small,
                Err(RowReadError::Read(ReadError::OutputTooSmall {
                    required: 13
                }))
            ),
            "{small:?}"
        );
        assert_eq!(
            point_with!(reader, b, private, &new, table, b_row, 13).unwrap(),
            Some(b_value)
        );
        let cancel = Cancellation::default();
        let mut cursor = open_cursor_live(
            &new,
            table,
            Some(RowId::new(b_row).unwrap()),
            Some(RowId::new(of('e').0).unwrap()),
        )
        .unwrap();
        let mut steps = Vec::new();
        for len in [12, 64, 12, 13, 13, 13] {
            steps.push(step_with!(&mut cursor, reader, b, private, len, &cancel));
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(reader.catalog_test_source_used(), 0);
        }
        cursor.close().unwrap();
        assert!(short(&steps[0]) && short(&steps[2]), "{steps:?}");
        let rows: Vec<_> = [1, 3, 4, 5]
            .iter()
            .map(|at| *steps[*at].as_ref().unwrap())
            .collect();
        assert_eq!(
            rows,
            vec![Some(of('b')), Some(of('c')), Some(of('d')), None]
        );
        println!("MCB04_V{variant:02} point_short={small:?} cursor={steps:?}");
    });
}

#[test]
fn mcb04_v07_output_one_short_then_the_same_row() {
    output("MCB04-V07", 7);
}

#[test]
fn mcb04_v08_output_exact() {
    output("MCB04-V08", 8);
}

/// Runs one cursor step that C's hook pauses at its first Row Status
/// capture (`Point::RowReadCaptured`), where a helper thread cancels
/// `$token` and resumes it.
macro_rules! cancelled_at_capture {
    ($core:expr, $token:expr, $step:expr) => {{
        const WAIT: Duration = Duration::from_secs(10);
        std::thread::scope(|scope| {
            let (hit_in, hit) = sync_channel(1);
            let (go, release) = sync_channel(1);
            hook::install($core, Point::RowReadCaptured, hit_in, release);
            let canceller = $token.clone();
            let helper = scope.spawn(move || {
                hit.recv_timeout(WAIT).unwrap();
                canceller.cancel();
                go.send(Resume::Continue).unwrap();
            });
            let step = $step;
            helper.join().unwrap();
            step
        })
    }};
}

/// MCB04-V09 (invisible rows first, then cancel; the progress key kept) on
/// the Active corpus statement. A foreign read-only ReadCommitted cursor
/// over T (every row invisible) is cancelled while its first call is
/// paused at a's capture: the call completes a's Missing-before, then
/// returns Cancelled. Its next call (a new token) runs while a's U is
/// retiring, where a capture of a would be Busy (MCB03-V01): it returns
/// End, so a was not examined again. The writer's own cursor (every row
/// visible), cancelled the same way at a, returns Cancelled; its next call
/// returns a, then b..f and End: the incomplete visible a is not skipped.
/// Every return leaves the census idle and no effect.
#[test]
fn mcb04_v09_cancel_after_invisible_rows_keeps_progress() {
    case("MCB04-V09", || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&targets, &spare);
                let (a_row, a_undo) = issued[0];
                assert_eq!(a_row.get(), of('a').0);
                let mut f_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let f = &mut f_owner;
                let mut f_private = boxed::<PRIVATE_BYTES>();
                let f_private = &mut *f_private;
                let foreign = read_only!(writer, f, IsolationLevel::ReadCommitted, f_private);
                let fv = writer.begin_statement(f, &foreign).unwrap();
                let f_table = issue_table!(reader, f, fv, f_private, flush, flusher);
                let before = effects(state);
                let token = Cancellation::default();
                let mut cursor = open_cursor_live(&fv, &f_table, None, None).unwrap();
                let first = cancelled_at_capture!(
                    core,
                    token,
                    step_with!(&mut cursor, &reader, f, f_private, 64, &token)
                );
                assert!(
                    matches!(first, Err(RowReadError::Read(ReadError::Cancelled))),
                    "{first:?}"
                );
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                let mut second = None;
                reader.row_test_with_retiring_undo(a_undo, || {
                    second = Some(step_with!(
                        &mut cursor,
                        &reader,
                        f,
                        f_private,
                        64,
                        &Cancellation::default()
                    ));
                });
                let second = second.unwrap();
                assert!(matches!(second, Ok(None)), "{second:?}");
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                cursor.close().unwrap();
                let own_token = Cancellation::default();
                let mut own = open_cursor_live(&view, &table, None, None).unwrap();
                let own_first = cancelled_at_capture!(
                    core,
                    own_token,
                    step_with!(&mut own, &reader, owner, reader_private, 64, &own_token)
                );
                assert!(
                    matches!(own_first, Err(RowReadError::Read(ReadError::Cancelled))),
                    "{own_first:?}"
                );
                assert_eq!(census!(reader, writer, core, fence), IDLE);
                let fresh = Cancellation::default();
                let mut rows = Vec::new();
                for _ in 0..=ROWS.len() {
                    rows.push(
                        step_with!(&mut own, &reader, owner, reader_private, 64, &fresh).unwrap(),
                    );
                }
                own.close().unwrap();
                let mut all: Vec<_> = expected(None, None).into_iter().map(Some).collect();
                all.push(None);
                assert_eq!(
                    rows, all,
                    "a is returned after the cancel, then b..f and End"
                );
                assert_eq!(effects(state), before);
                println!(
                    "MCB04_V09 foreign={first:?} then={second:?} own={own_first:?} then={:?}",
                    rows[0]
                );
                assert!(writer.finish_statement(f, fv).is_ok());
                assert!(writer.finish_read_only(f, foreign).is_ok());
                f.check_idle().unwrap();
                assert!(writer.finish_statement(owner, view).is_ok());
                assert!(writer.commit_row(owner, tx, private).is_ok());
                owner.check_idle().unwrap();
                assert!(!fence.is_fenced());
                let _ = (flush, flusher);
            }
        );
    });
}

/// MCB04-V10 (End called again): on the committed corpus under a new
/// read-only snapshot, a cursor `[e, ∞)` returns e, f, then End on three
/// calls, the census idle at each.
#[test]
fn mcb04_v10_end_then_end_again() {
    committed!("MCB04-V10", |reader, b, private, new, table| {
        let cancel = Cancellation::default();
        let mut cursor =
            open_cursor_live(&new, table, Some(RowId::new(of('e').0).unwrap()), None).unwrap();
        let mut steps = Vec::new();
        for _ in 0..5 {
            steps.push(step_with!(&mut cursor, reader, b, private, 64, &cancel).unwrap());
            assert_eq!(reader.catalog_test_references(), ((0, 0, 0), false));
            assert_eq!(reader.catalog_test_source_used(), 0);
        }
        cursor.close().unwrap();
        assert_eq!(steps, vec![Some(of('e')), Some(of('f')), None, None, None]);
        println!("MCB04_V10 steps={steps:?}");
    });
}

/// A resident page's cells after `edit`, re-encoded in its frame: the page
/// identity, LSN, envelope and checksum stay valid.
fn edit_cells(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    number: u64,
    kind: crate::storage::page::PageKind,
    edit: impl FnOnce(&mut Vec<Vec<u8>>),
) {
    use crate::storage::{
        dirty_frame::status_load_test_support,
        page::{decode_page, encode_page},
    };
    let index = probe::registered_index(pool, number).unwrap();
    let identity = pool.target_request(index).unwrap().identity;
    status_load_test_support::edit_content(pool, index, |bytes| {
        let page = decode_page(bytes, identity, kind).unwrap();
        let header = page.header();
        let mut cells = (0..page.len())
            .map(|i| page.cell(i).unwrap().to_vec())
            .collect::<Vec<_>>();
        edit(&mut cells);
        let refs = cells.iter().map(Vec::as_slice).collect::<Vec<_>>();
        encode_page(header, &refs, bytes).unwrap();
        decode_page(bytes, identity, kind).unwrap();
    });
}

/// RowLeaf 13's cell of `row` (key: big-endian RowId at 4..12; the record
/// from 12: its RowId at 20..28, version stamp 28..84, logical version
/// 84..132, Undo reference 132..172).
fn leaf_cell(cells: &mut [Vec<u8>], row: u64) -> &mut Vec<u8> {
    let cell = cells
        .iter_mut()
        .find(|cell| cell[4..12] == row.to_be_bytes())
        .unwrap_or_else(|| panic!("a leaf cell for RowId {row}"));
    assert_eq!(cell[20..28], row.to_le_bytes(), "the record's RowId");
    cell
}

/// MCB04-V12 (valid page, wrong RowId), V13 (wrong Undo target) and V14
/// (occupied before) on the Active corpus statement, read by a foreign
/// read-only ReadCommitted statement (which needs c's U for Missing-before)
/// after one edit of a resident page, re-encoded so that its identity, LSN,
/// envelope and checksum stay valid. V12: c's record names RowId 5 (d's)
/// under key 4. V13: c's record names d's U. V14: c's U, otherwise
/// unchanged and valid, records an occupied before (RowDeleted, with a
/// before stamp and a logical version of the same actor's earlier
/// mutation), which this scope does not support. Each read of c (point and
/// cursor `[c, e)`) is refused with C's or B's typed error, never Found or
/// Missing.
fn damaged(id: &str, variant: u8) {
    use crate::storage::page::PageKind;
    case(id, || {
        let corpus = Corpus::load();
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 28,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                corpus!(corpus, live, owner, flusher, status, undo, private, reader_private,
                    state => writer, reader, flush, tx, view, table, issued, targets, spare);
                let _ = (&targets, &spare, &flush, &flusher);
                let (c_row, _) = of('c');
                let (d_row, _) = of('d');
                let undo_of = |row: u64| issued.iter().find(|(r, _)| r.get() == row).unwrap().1;
                let encoded = |reference: crate::storage::undo_ref::UndoRef| {
                    let mut bytes = [0u8; 40];
                    reference.encode(&mut bytes).unwrap();
                    bytes
                };
                let pool = reader.catalog_test_pool();
                let mut leaf = {
                    let bytes = page_bytes(pool, 13);
                    let identity = pool
                        .target_request(probe::registered_index(pool, 13).unwrap())
                        .unwrap()
                        .identity;
                    let page =
                        crate::storage::page::decode_page(&bytes, identity, PageKind::RowLeaf)
                            .unwrap();
                    (0..page.len())
                        .map(|i| page.cell(i).unwrap().to_vec())
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    leaf_cell(&mut leaf, c_row)[132..172],
                    encoded(undo_of(c_row))
                );
                match variant {
                    12 => edit_cells(pool, 13, PageKind::RowLeaf, |cells| {
                        leaf_cell(cells, c_row)[20..28].copy_from_slice(&d_row.to_le_bytes());
                    }),
                    13 => edit_cells(pool, 13, PageKind::RowLeaf, |cells| {
                        leaf_cell(cells, c_row)[132..172].copy_from_slice(&encoded(undo_of(d_row)));
                    }),
                    _ => {
                        use crate::storage::undo_record::{
                            UndoBeforeKind, UndoRecordExpectation, UndoTargetKind, UndoTargetView,
                            decode_undo_record,
                        };
                        let c_undo = undo_of(c_row);
                        let handle = table.test_identity().0;
                        let stamp = leaf_cell(&mut leaf, c_row)[28..84].to_vec();
                        let logical = leaf_cell(&mut leaf, of('a').0)[84..132].to_vec();
                        edit_cells(pool, c_undo.page().page_no, PageKind::Undo, |cells| {
                            let cell = &mut cells[usize::from(c_undo.slot())];
                            assert_eq!((cell[324], &cell[216..320]), (0, &[0u8; 104][..]));
                            cell[216..272].copy_from_slice(&stamp);
                            cell[272..320].copy_from_slice(&logical);
                            cell[324] = UndoBeforeKind::RowDeleted as u8;
                            // C's own expectation of c's U (its Missing-before
                            // read): Row target T, entity c, no key.
                            let target = UndoTargetView {
                                kind: UndoTargetKind::Row,
                                owner: handle.object,
                                generation: handle.generation,
                                entity: c_row,
                                arity: 0,
                                key: &[],
                            };
                            let record = decode_undo_record(
                                cell,
                                UndoRecordExpectation {
                                    namespace: super::super::support::namespace(),
                                    reference: c_undo,
                                    target,
                                },
                                &[],
                            )
                            .and_then(|view| view.live_before())
                            .unwrap();
                            assert_eq!(record.before_kind, UndoBeforeKind::RowDeleted);
                        });
                    }
                }
                let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
                let transaction = writer
                    .begin_transaction(
                        &mut other,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let other_view = writer.begin_statement(&mut other, &transaction).unwrap();
                let before = effects(state);
                let point = point_of!(
                    &reader,
                    &mut other,
                    reader_private,
                    &other_view,
                    &table,
                    c_row
                );
                let fenced_by_point = fence.is_fenced();
                let range = range_of!(
                    &reader,
                    &mut other,
                    reader_private,
                    &other_view,
                    &table,
                    Some(c_row),
                    Some(of('e').0)
                );
                println!(
                    "MCB04_V{variant} point={point:?} cursor={range:?} fenced={fenced_by_point}/{}",
                    fence.is_fenced()
                );
                // V12/V13: corruption the read needs, never Found or Missing:
                // C's typed fatal error fences the session, and the cursor
                // then meets the fence (CallerMustResolve kept). V14: the
                // contract's out-of-scope refusal (consumers §9.2, F02),
                // Unsupported with no effect and no fence.
                let fatal = |phase, cause: fn(&RetentionCause) -> bool| {
                    matches!(&point, Err(RowReadError::Source(RetentionError {
                        phase: p, cause: c, effect: Effect::CallerMustResolve,
                    })) if *p == phase && cause(c))
                        && matches!(
                            &range,
                            Err(RowReadError::Source(RetentionError {
                                phase: RetentionPhase::Census,
                                cause: RetentionCause::Fenced,
                                effect: Effect::CallerMustResolve,
                            }))
                        )
                        && fenced_by_point
                };
                match variant {
                    12 => assert!(
                        fatal(RetentionPhase::Source, |c| matches!(
                            c,
                            RetentionCause::Invariant
                        )),
                        "{point:?} {range:?}"
                    ),
                    13 => assert!(
                        fatal(RetentionPhase::Census, |c| matches!(
                            c,
                            RetentionCause::Undo(
                                crate::storage::undo_record::UndoError::IdentityMismatch
                            )
                        )),
                        "{point:?} {range:?}"
                    ),
                    _ => {
                        for refused in [&point.map(|_| ()), &range.map(|_| ())] {
                            assert!(
                                matches!(
                                    refused,
                                    Err(RowReadError::Source(RetentionError {
                                        phase: RetentionPhase::Census,
                                        cause: RetentionCause::Unsupported,
                                        effect: Effect::Unchanged,
                                    }))
                                ),
                                "{refused:?}"
                            );
                        }
                        assert!(!fence.is_fenced());
                    }
                }
                assert_eq!(effects(state), before);
                assert_eq!(writer.catalog_test_physical_pins(), 0);
                assert_eq!(hook::observe(core).active_read_operations, 0);
                drop(other_view);
                drop(transaction);
                drop(view);
                drop(tx);
            }
        );
    });
}

#[test]
fn mcb04_v12_valid_page_wrong_row_id_is_refused() {
    damaged("MCB04-V12", 12);
}

#[test]
fn mcb04_v13_wrong_undo_target_is_refused() {
    damaged("MCB04-V13", 13);
}

#[test]
fn mcb04_v14_occupied_before_is_out_of_scope() {
    damaged("MCB04-V14", 14);
}

// After every macro above, which the native worker uses.
#[path = "row_corpus/native.rs"]
mod native;
