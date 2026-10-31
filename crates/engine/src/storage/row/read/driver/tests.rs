//! MCB00-D01..12 are C0_ONLY: the driver runs over independently written
//! RowLeaf cells and logical Status/Missing-before answers of a test-only
//! source. No RowReadOperation, pin, snapshot, CallerCredit, observation, ACK
//! or READY exists here. In the double's C role the literal cells are wrapped
//! by the production `encode_page` and read back with the production codecs;
//! every expected value is a literal. The frozen inputs are in
//! `driver-manifest-r2.json` (r1: `driver-manifest.json`).
use super::*;
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        disk::{RetentionCause, RetentionPhase},
        page::{
            FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef, encode_page,
        },
        row_record::{
            RecordExpectation, RowPayload, RowSchemaInput, decode_row_record_with_schema,
        },
        transaction_record::VersionStamp,
        tree_page::{EntryPayload, TreeExpectation, decode_tree_page},
    },
    transaction::TxnId,
};
use Call::{Capture, CopyOut, Finish, Leaf, MissingBefore, Page, Release, ReleasePage, Status};
use foundation::identity::{CommitSeq, Generation, ObjectId};
use std::{
    cell::{Cell, RefCell},
    fmt::Debug,
    ptr,
};

const CELLS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-read/driver-cells.txt"
));

// Literal canonical Int64 rows: u32 column count, tag 2, i64 LE.
const V100: [u8; 13] = [1, 0, 0, 0, 2, 0x64, 0, 0, 0, 0, 0, 0, 0];
const V200: [u8; 13] = [1, 0, 0, 0, 2, 0xc8, 0, 0, 0, 0, 0, 0, 0];
const V300: [u8; 13] = [1, 0, 0, 0, 2, 0x2c, 1, 0, 0, 0, 0, 0, 0];
const V400: [u8; 13] = [1, 0, 0, 0, 2, 0x90, 1, 0, 0, 0, 0, 0, 0];
const FILL: u8 = 0xcc;

fn row(value: u64) -> RowId {
    RowId::new(value).unwrap()
}

fn q(value: u64) -> CommitSeq {
    CommitSeq::new(value).unwrap()
}

fn fixture(name: &str) -> &'static str {
    CELLS
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
        .unwrap()
}

fn hex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn identity() -> PageIdentity {
    PageIdentity {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        page: PageRef {
            file: FileId::new(0x0000_0001_0000_0002).unwrap(),
            page_no: 6,
            incarnation: Generation::new(2).unwrap(),
        },
        owner: ObjectId::new(3).unwrap(),
        owner_generation: Generation::new(4).unwrap(),
    }
}

/// Only the unchanged generic envelope/CRC encoder wraps the literal cells.
fn leaf_page(name: &str) -> Vec<u8> {
    let cells: Vec<Vec<u8>> = fixture(name)
        .split(' ')
        .map(|cell| hex(fixture(cell)))
        .collect();
    let mut metadata = [0; 32];
    metadata[..6].copy_from_slice(b"XBT1\x01\x00");
    let mut slots: Vec<&[u8]> = vec![&metadata];
    slots.extend(cells.iter().map(Vec::as_slice));
    let header = PageHeader {
        identity: identity(),
        kind: PageKind::RowLeaf,
        level: 0,
        lsn: PageLsn(0),
    };
    let mut bytes = vec![0; PAGE_BYTES];
    encode_page(header, &slots, &mut bytes).unwrap();
    bytes
}

fn own() -> TxnRef {
    let id = TxnId {
        database: DatabaseId::new([0x11; 16]).unwrap(),
        timeline: 7,
        writer_epoch: 3,
        local: 9,
    };
    TxnRef::new(id, Generation::new(5).unwrap()).unwrap()
}

fn source_error(effect: Effect) -> RetentionError {
    let cause = match effect {
        Effect::Unchanged => RetentionCause::Busy,
        Effect::CallerMustResolve => RetentionCause::Fenced,
    };
    RetentionError {
        phase: RetentionPhase::Source,
        cause,
        effect,
    }
}

fn invariant() -> RetentionError {
    RetentionError {
        phase: RetentionPhase::Source,
        cause: RetentionCause::Invariant,
        effect: Effect::Unchanged,
    }
}

/// Source phases in call order, recorded by the literal source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Page,
    Leaf,
    ReleasePage,
    Capture(u64),
    Status(u64),
    MissingBefore(u64),
    CopyOut(u64),
    Release(u64),
    Finish,
}

#[derive(Clone, Copy)]
enum Answer<T> {
    Ready(T),
    Flush(u32),
    Fail(Effect),
}

fn committed(value: u64) -> Answer<TransactionState> {
    Answer::Ready(TransactionState::Committed(q(value)))
}

fn answer<T>(answer: Answer<T>) -> Result<Step<T, FlushTarget>, RetentionError> {
    match answer {
        Answer::Ready(value) => Ok(Step::Ready(value)),
        Answer::Flush(target) => Ok(Step::FlushRequired {
            target: FlushTarget(target),
        }),
        Answer::Fail(effect) => Err(source_error(effect)),
    }
}

/// Stands in for C's target, which cannot be built outside the boundary.
#[derive(Debug, PartialEq, Eq)]
struct FlushTarget(u32);

/// A logical Missing-before answer, not a MissingRowEvidence.
#[derive(Clone, Copy)]
struct MissingAnswer;

struct LiteralCandidate<'x> {
    version: RecordVersion,
    canonical: &'x [u8],
    sequence: u64,
}

impl Observed for LiteralCandidate<'_> {
    fn version(&self) -> RecordVersion {
        self.version
    }
    fn canonical_len(&self) -> usize {
        self.canonical.len()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LiteralCopy {
    version: RecordVersion,
    len: usize,
}

impl Observed for LiteralCopy {
    fn version(&self) -> RecordVersion {
        self.version
    }
    fn canonical_len(&self) -> usize {
        self.len
    }
}

/// Test-only source over literal page bytes. It decodes the chosen cell with
/// the unchanged codecs and answers Status/Missing-before from a script.
struct LiteralSource<'x> {
    bytes: &'x [u8],
    log: &'x RefCell<Vec<Call>>,
    page_answer: Answer<()>,
    status: Vec<(u64, Answer<TransactionState>)>,
    missing: Vec<(u64, Answer<MissingAnswer>)>,
    fail_release: Option<(u64, Effect)>,
    fail_finish: Option<Effect>,
    cancel_at: Option<(Call, Cancellation)>,
    sequence: u64,
}

impl<'x> LiteralSource<'x> {
    fn new(bytes: &'x [u8], log: &'x RefCell<Vec<Call>>) -> Self {
        Self {
            bytes,
            log,
            page_answer: Answer::Ready(()),
            status: Vec::new(),
            missing: Vec::new(),
            fail_release: None,
            fail_finish: None,
            cancel_at: None,
            sequence: 0,
        }
    }
    fn with_page(mut self, answer: Answer<()>) -> Self {
        self.page_answer = answer;
        self
    }
    fn with_status(mut self, row: u64, answer: Answer<TransactionState>) -> Self {
        self.status.push((row, answer));
        self
    }
    fn with_missing(mut self, row: u64, answer: Answer<MissingAnswer>) -> Self {
        self.missing.push((row, answer));
        self
    }
    fn failing_release(mut self, row: u64, effect: Effect) -> Self {
        self.fail_release = Some((row, effect));
        self
    }
    fn failing_finish(mut self, effect: Effect) -> Self {
        self.fail_finish = Some(effect);
        self
    }
    fn cancelling_at(mut self, call: Call, token: &Cancellation) -> Self {
        self.cancel_at = Some((call, token.clone()));
        self
    }
    fn note(&self, call: Call) {
        self.log.borrow_mut().push(call);
        if let Some((at, token)) = &self.cancel_at
            && *at == call
        {
            token.cancel();
        }
    }
}

fn decode_leaf(bytes: &[u8]) -> Result<TreePageView<'_>, RetentionError> {
    let expected = TreeExpectation {
        identity: identity(),
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    };
    decode_tree_page(bytes, expected).map_err(|error| RetentionError {
        phase: RetentionPhase::Source,
        cause: RetentionCause::Tree(error),
        effect: Effect::Unchanged,
    })
}

impl<'x> ReadSource for LiteralSource<'x> {
    type Target = FlushTarget;
    type Candidate = LiteralCandidate<'x>;
    type Missing = MissingAnswer;
    type Copied = LiteralCopy;
    type Page<'g>
        = LiteralPage<'g, 'x>
    where
        Self: 'g;
    fn page(&mut self) -> Result<Step<LiteralPage<'_, 'x>, FlushTarget>, RetentionError> {
        self.note(Page);
        Ok(match answer(self.page_answer)? {
            Step::Ready(()) => Step::Ready(LiteralPage { source: self }),
            Step::FlushRequired { target } => Step::FlushRequired { target },
        })
    }
    fn status(
        &mut self,
        candidate: &LiteralCandidate<'x>,
    ) -> Result<Step<TransactionState, FlushTarget>, RetentionError> {
        let row = candidate.version.row.get();
        self.note(Status(row));
        // An unscripted Status is a test input error, never a default state.
        match self.status.iter().find(|(at, _)| *at == row) {
            Some((_, scripted)) => answer(*scripted),
            None => Err(invariant()),
        }
    }
    fn missing_before(
        &mut self,
        candidate: &LiteralCandidate<'x>,
    ) -> Result<Step<MissingAnswer, FlushTarget>, RetentionError> {
        let row = candidate.version.row.get();
        self.note(MissingBefore(row));
        let scripted = self.missing.iter().find(|(at, _)| *at == row);
        answer(scripted.map_or(Answer::Ready(MissingAnswer), |(_, scripted)| *scripted))
    }
    fn copy_candidate(
        &mut self,
        candidate: &LiteralCandidate<'x>,
        output: &mut [u8],
    ) -> Result<LiteralCopy, RetentionError> {
        self.note(CopyOut(candidate.version.row.get()));
        // B must have rejected a short output before this call.
        let target = output
            .get_mut(..candidate.canonical.len())
            .ok_or_else(invariant)?;
        target.copy_from_slice(candidate.canonical);
        Ok(LiteralCopy {
            version: candidate.version,
            len: candidate.canonical.len(),
        })
    }
    fn release_candidate(&mut self, candidate: LiteralCandidate<'x>) -> Result<(), RetentionError> {
        let row = candidate.version.row.get();
        self.note(Release(row));
        if candidate.sequence != self.sequence {
            return Err(invariant());
        }
        match self.fail_release {
            Some((at, effect)) if at == row => Err(source_error(effect)),
            _ => Ok(()),
        }
    }
    fn finish(self) -> Result<(), RetentionError> {
        self.note(Finish);
        self.fail_finish
            .map_or(Ok(()), |effect| Err(source_error(effect)))
    }
}

struct LiteralPage<'g, 'x> {
    source: &'g mut LiteralSource<'x>,
}

impl<'x> SearchPage for LiteralPage<'_, 'x> {
    type Candidate = LiteralCandidate<'x>;
    fn leaf(&self) -> Result<TreePageView<'_>, RetentionError> {
        self.source.note(Leaf);
        decode_leaf(self.source.bytes)
    }
    fn capture(self, at: usize) -> Result<LiteralCandidate<'x>, RetentionError> {
        let source = self.source;
        let bytes: &'x [u8] = source.bytes;
        let entry = decode_leaf(bytes)?.entry(at).map_err(|_| invariant())?;
        let key: [u8; 8] = entry.key.try_into().map_err(|_| invariant())?;
        let row = RowId::new(u64::from_be_bytes(key)).map_err(|_| invariant())?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(invariant());
        };
        let columns = [Column {
            name: "value".into(),
            data_type: DataType::Int64,
            nullable: false,
        }];
        let expected = RecordExpectation {
            namespace: Namespace {
                database: DatabaseId::new([0x11; 16]).unwrap(),
                timeline: 7,
            },
            row,
        };
        let record =
            decode_row_record_with_schema(value, expected, RowSchemaInput::Columns(&columns))
                .map_err(|_| invariant())?;
        let RowPayload::Inline(canonical) = record.payload() else {
            return Err(invariant());
        };
        source.note(Capture(row.get()));
        source.sequence += 1;
        Ok(LiteralCandidate {
            version: record.version(),
            canonical: canonical.as_bytes(),
            sequence: source.sequence,
        })
    }
    fn release_checked(self) -> Result<(), RetentionError> {
        self.source.note(ReleasePage);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Returned {
    Close,
    Drop,
}

/// Test-only view: literal q/actor/revision and a counted charge per credit.
struct LiteralView {
    q: u64,
    own_actor: Option<TxnRef>,
    revision: Cell<u64>,
    ended: Cell<bool>,
    capacity: u32,
    issued: Cell<u32>,
    charged: Cell<u32>,
    returned: RefCell<Vec<(u32, Returned)>>,
}

impl LiteralView {
    fn new(q: u64, own_actor: Option<TxnRef>) -> Self {
        Self {
            q,
            own_actor,
            revision: Cell::new(0),
            ended: Cell::new(false),
            capacity: 4,
            issued: Cell::new(0),
            charged: Cell::new(0),
            returned: RefCell::new(Vec::new()),
        }
    }
    fn check(&self) -> Result<(), RetentionError> {
        if self.ended.get() {
            return Err(RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::WrongState,
                effect: Effect::Unchanged,
            });
        }
        Ok(())
    }
}

impl ReadView for LiteralView {
    type Credit<'r>
        = LiteralCredit<'r>
    where
        Self: 'r;
    fn q(&self) -> Result<u64, RetentionError> {
        self.check().map(|()| self.q)
    }
    fn own_actor(&self) -> Option<TxnRef> {
        self.own_actor
    }
    fn revision(&self) -> u64 {
        self.revision.get()
    }
    fn reserve_cursor_credit(&self) -> Result<LiteralCredit<'_>, RetentionError> {
        self.check()?;
        if self.charged.get() == self.capacity {
            return Err(RetentionError {
                phase: RetentionPhase::Census,
                cause: RetentionCause::Capacity,
                effect: Effect::Unchanged,
            });
        }
        self.charged.set(self.charged.get() + 1);
        self.issued.set(self.issued.get() + 1);
        Ok(LiteralCredit {
            view: self,
            id: self.issued.get(),
            active: true,
        })
    }
}

/// Counts one literal charge; it is not a CallerCredit and has no slot.
struct LiteralCredit<'r> {
    view: &'r LiteralView,
    id: u32,
    active: bool,
}

impl LiteralCredit<'_> {
    fn give_back(&mut self, how: Returned) {
        if self.active {
            self.active = false;
            self.view.charged.set(self.view.charged.get() - 1);
            self.view.returned.borrow_mut().push((self.id, how));
        }
    }
}

impl CursorCredit for LiteralCredit<'_> {
    fn release_checked(mut self) -> Result<(), RetentionError> {
        self.give_back(Returned::Close);
        Ok(())
    }
}

impl Drop for LiteralCredit<'_> {
    fn drop(&mut self) {
        self.give_back(Returned::Drop);
    }
}

type PointResult = Result<Step<PointRead<LiteralCopy>, FlushTarget>, RowReadError>;
type NextResult = Result<Step<CursorStep<LiteralCopy>, FlushTarget>, RowReadError>;

fn found(result: PointResult) -> LiteralCopy {
    match result {
        Ok(Step::Ready(PointRead::Found(copy))) => copy,
        other => panic!("expected Found, got {other:?}"),
    }
}

fn missing(result: PointResult) {
    assert!(
        matches!(result, Ok(Step::Ready(PointRead::Missing))),
        "{result:?}"
    );
}

fn returned(result: NextResult) -> LiteralCopy {
    match result {
        Ok(Step::Ready(CursorStep::Row(copy))) => copy,
        other => panic!("expected a row, got {other:?}"),
    }
}

fn end(result: NextResult) {
    assert!(
        matches!(result, Ok(Step::Ready(CursorStep::End))),
        "{result:?}"
    );
}

fn flushed<T: Debug>(result: Result<Step<T, FlushTarget>, RowReadError>) -> u32 {
    match result {
        Ok(Step::FlushRequired {
            target: FlushTarget(target),
        }) => target,
        other => panic!("expected FlushRequired, got {other:?}"),
    }
}

fn read_error<T: Debug>(result: Result<T, RowReadError>) -> ReadError {
    match result {
        Err(RowReadError::Read(error)) => error,
        other => panic!("expected a B error, got {other:?}"),
    }
}

fn source_failure<T: Debug>(result: Result<T, RowReadError>) -> RetentionError {
    match result {
        Err(RowReadError::Source(error)) => error,
        other => panic!("expected a source error, got {other:?}"),
    }
}

/// Stands in for the CheckedTable a cursor is opened on; it only counts the
/// operations opened against it.
#[derive(Default)]
struct LiteralTable {
    opened: Cell<u32>,
}

type Cursor<'r> = RowCursor<'r, LiteralView, LiteralTable>;

/// No operation was opened: an opened one always logs its Finish.
const UNOPENED: [Call; 0] = [];

/// Point read whose operation the driver opens on the read's own view.
fn point(
    view: &LiteralView,
    source: LiteralSource<'_>,
    at: RowId,
    output: &mut [u8],
    cancel: &Cancellation,
) -> PointResult {
    read_point(
        view,
        move |opened| {
            assert!(ptr::eq(opened, view));
            Ok(source)
        },
        at,
        output,
        cancel,
    )
}

/// One cursor step whose operation the driver opens on the cursor's own view
/// and table.
fn advance(
    cursor: &mut Cursor<'_>,
    source: LiteralSource<'_>,
    output: &mut [u8],
    cancel: &Cancellation,
) -> NextResult {
    cursor.next(
        move |_, table| {
            table.opened.set(table.opened.get() + 1);
            Ok(source)
        },
        output,
        cancel,
    )
}

/// Calls of a scan that first judges 10 invisible, then `tail`.
fn examined_then(tail: &[Call]) -> Vec<Call> {
    let mut calls = vec![Page, Leaf, Capture(10), MissingBefore(10), Release(10)];
    calls.extend_from_slice(tail);
    calls
}

fn progress<V: ReadView, T>(cursor: &RowCursor<'_, V, T>) -> (Option<u64>, Option<u64>) {
    (
        cursor.progress.last_examined.map(RowId::get),
        cursor.progress.last_returned.map(RowId::get),
    )
}

#[test]
fn mcb00_d01_point_found_and_absent_key() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);

    let mut output = [FILL; 16];
    let copy = found(point(
        &view,
        LiteralSource::new(&leaf_a, &log),
        row(30),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(30), 13));
    assert_eq!(copy.version.stamp, VersionStamp::Committed(q(5)));
    assert_eq!(output[..13], V300);
    assert_eq!(output[13..], [FILL; 3]);
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(30), CopyOut(30), Release(30), Finish]
    );

    // Committed exactly at q is current: no Status or Missing-before.
    let at_twelve = LiteralView::new(12, None);
    let copy = found(point(
        &at_twelve,
        LiteralSource::new(&leaf_a, &log),
        row(10),
        &mut output,
        &none,
    ));
    assert_eq!((copy.version.row, copy.len), (row(10), 13));
    assert_eq!(output[..13], V100);
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(10), CopyOut(10), Release(10), Finish]
    );

    // A key the leaf does not hold is an ordinary checked absence.
    for absent in [1, 15, 35, 41, u64::MAX] {
        let mut output = [FILL; 16];
        missing(point(
            &view,
            LiteralSource::new(&leaf_a, &log),
            row(absent),
            &mut output,
            &none,
        ));
        assert_eq!(output, [FILL; 16]);
        assert_eq!(log.take(), [Page, Leaf, ReleasePage, Finish]);
    }
}

#[test]
fn mcb00_d02_point_status_and_missing_before_select_the_version() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let foreign = LiteralView::new(10, None);
    let mine = LiteralView::new(10, Some(own()));

    // A transaction stamp is visible only after C's exact Status.
    for (view, state) in [
        (&foreign, committed(8)),
        (&foreign, committed(10)),
        (&mine, Answer::Ready(TransactionState::Active)),
    ] {
        let mut output = [FILL; 16];
        let source = LiteralSource::new(&leaf_a, &log).with_status(20, state);
        let copy = found(point(view, source, row(20), &mut output, &none));
        assert_eq!((copy.version.row, copy.len), (row(20), 13));
        assert_eq!(output[..13], V200);
        assert_eq!(
            log.take(),
            [
                Page,
                Leaf,
                Capture(20),
                Status(20),
                CopyOut(20),
                Release(20),
                Finish
            ]
        );
    }

    // Invisible after Status: Missing only with C's Missing-before answer.
    for (view, state) in [
        (&foreign, committed(11)),
        (&foreign, Answer::Ready(TransactionState::Active)),
        (&mine, Answer::Ready(TransactionState::Aborted)),
    ] {
        let mut output = [FILL; 16];
        let source = LiteralSource::new(&leaf_a, &log).with_status(20, state);
        missing(point(view, source, row(20), &mut output, &none));
        assert_eq!(output, [FILL; 16]);
        assert_eq!(
            log.take(),
            [
                Page,
                Leaf,
                Capture(20),
                Status(20),
                MissingBefore(20),
                Release(20),
                Finish
            ]
        );
    }

    // Committed after q with an Undo: Missing-before without Status.
    let mut output = [FILL; 16];
    missing(point(
        &foreign,
        LiteralSource::new(&leaf_a, &log),
        row(10),
        &mut output,
        &none,
    ));
    let missing_calls = [
        Page,
        Leaf,
        Capture(10),
        MissingBefore(10),
        Release(10),
        Finish,
    ];
    assert_eq!(log.take(), missing_calls);

    // A failed Missing-before is an error, never Missing; cleanup still runs.
    let source =
        LiteralSource::new(&leaf_a, &log).with_missing(10, Answer::Fail(Effect::Unchanged));
    let error = source_failure(point(&foreign, source, row(10), &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(log.take(), missing_calls);

    // An invisible committed version without Undo is corruption, not absence.
    let early = LiteralView::new(2, None);
    assert_eq!(
        read_error(point(
            &early,
            LiteralSource::new(&leaf_a, &log),
            row(40),
            &mut output,
            &none,
        )),
        ReadError::MissingUndo
    );
    assert_eq!(log.take(), [Page, Leaf, Capture(40), Release(40), Finish]);
    assert_eq!(output, [FILL; 16]);
}

#[test]
fn mcb00_d03_flush_required_passes_through_after_cleanup() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut output = [FILL; 16];

    // Point: each phase's own target comes back unchanged after release.
    let source = LiteralSource::new(&leaf_a, &log).with_page(Answer::Flush(7));
    assert_eq!(
        flushed(point(&view, source, row(30), &mut output, &none)),
        7
    );
    assert_eq!(log.take(), [Page, Finish]);
    let source = LiteralSource::new(&leaf_a, &log).with_status(20, Answer::Flush(8));
    assert_eq!(
        flushed(point(&view, source, row(20), &mut output, &none)),
        8
    );
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(20), Status(20), Release(20), Finish]
    );
    let source = LiteralSource::new(&leaf_a, &log).with_missing(10, Answer::Flush(9));
    assert_eq!(
        flushed(point(&view, source, row(10), &mut output, &none)),
        9
    );
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(10),
            MissingBefore(10),
            Release(10),
            Finish
        ]
    );
    assert_eq!(output, [FILL; 16]);

    // Cursor: only a fully judged invisible row stays examined.
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let source = LiteralSource::new(&leaf_a, &log).with_missing(10, Answer::Flush(9));
    assert_eq!(flushed(advance(&mut cursor, source, &mut output, &none)), 9);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(10),
            MissingBefore(10),
            Release(10),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (None, None));
    let source = LiteralSource::new(&leaf_a, &log).with_status(20, Answer::Flush(8));
    assert_eq!(flushed(advance(&mut cursor, source, &mut output, &none)), 8);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(10),
            MissingBefore(10),
            Release(10),
            Page,
            Leaf,
            Capture(20),
            Status(20),
            Release(20),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(10), None));
    let source = LiteralSource::new(&leaf_a, &log).with_page(Answer::Flush(7));
    assert_eq!(flushed(advance(&mut cursor, source, &mut output, &none)), 7);
    assert_eq!(log.take(), [Page, Finish]);
    assert_eq!(progress(&cursor), (Some(10), None));
    assert_eq!(output, [FILL; 16]);

    let source = LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    returned(advance(&mut cursor, source, &mut output, &none));
    assert_eq!(output[..13], V200);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(20),
            Status(20),
            CopyOut(20),
            Release(20),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(20), Some(20)));
}

#[test]
fn mcb00_d04_point_output_cancel_and_cleanup_never_publish() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);

    // One byte short: no copy, and the candidate and operation are released.
    let mut short = [FILL; 12];
    assert_eq!(
        read_error(point(
            &view,
            LiteralSource::new(&leaf_a, &log),
            row(30),
            &mut short,
            &none,
        )),
        ReadError::OutputTooSmall { required: 13 }
    );
    assert_eq!(short, [FILL; 12]);
    let released = [Page, Leaf, Capture(30), Release(30), Finish];
    assert_eq!(log.take(), released);
    let mut exact = [FILL; 13];
    found(point(
        &view,
        LiteralSource::new(&leaf_a, &log),
        row(30),
        &mut exact,
        &none,
    ));
    assert_eq!(exact, V300);
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(30), CopyOut(30), Release(30), Finish]
    );

    // Cancelled before the call: no operation is opened.
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let mut output = [FILL; 16];
    assert_eq!(
        read_error(point(
            &view,
            LiteralSource::new(&leaf_a, &log),
            row(30),
            &mut output,
            &cancelled,
        )),
        ReadError::Cancelled
    );
    assert_eq!(log.take(), UNOPENED);
    // Cancelled during the call: sampled again before C copies.
    for (target, hook) in [(30, Capture(30)), (20, Status(20))] {
        let token = Cancellation::default();
        let source = LiteralSource::new(&leaf_a, &log)
            .with_status(20, committed(8))
            .cancelling_at(hook, &token);
        assert_eq!(
            read_error(point(&view, source, row(target), &mut output, &token)),
            ReadError::Cancelled
        );
        let mut calls = vec![Page, Leaf, Capture(target)];
        if target == 20 {
            calls.push(Status(20));
        }
        calls.extend([Release(target), Finish]);
        assert_eq!(log.take(), calls);
    }
    assert_eq!(output, [FILL; 16]);

    // A copy is published only after checked cleanup: a failed release wins.
    let source = LiteralSource::new(&leaf_a, &log).failing_release(30, Effect::Unchanged);
    let error = source_failure(point(&view, source, row(30), &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(30), CopyOut(30), Release(30), Finish]
    );
    // A caller-must-resolve finish replaces an ordinary error.
    let source = LiteralSource::new(&leaf_a, &log).failing_finish(Effect::CallerMustResolve);
    let error = source_failure(point(&view, source, row(30), &mut short, &none));
    assert!(matches!(error.cause, RetentionCause::Fenced));
    assert_eq!(error.effect, Effect::CallerMustResolve);
    assert_eq!(log.take(), released);
    // An earlier caller-must-resolve error survives a failed cleanup.
    let source = LiteralSource::new(&leaf_a, &log)
        .with_status(20, Answer::Fail(Effect::CallerMustResolve))
        .failing_finish(Effect::Unchanged);
    let error = source_failure(point(&view, source, row(20), &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Fenced));
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(20), Status(20), Release(20), Finish]
    );
}

#[test]
fn mcb00_d05_cursor_next_returns_rows_in_order_then_end() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let source = || LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let mut output = [FILL; 16];

    // 10 is invisible at q10: examined and skipped in the same call as 20.
    let copy = returned(advance(&mut cursor, source(), &mut output, &none));
    assert_eq!((copy.version.row, copy.len), (row(20), 13));
    assert_eq!(output[..13], V200);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(10),
            MissingBefore(10),
            Release(10),
            Page,
            Leaf,
            Capture(20),
            Status(20),
            CopyOut(20),
            Release(20),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(20), Some(20)));
    for (value, bytes) in [(30, V300), (40, V400)] {
        let copy = returned(advance(&mut cursor, source(), &mut output, &none));
        assert_eq!((copy.version.row, copy.len), (row(value), 13));
        assert_eq!(output[..13], bytes);
        assert_eq!(
            log.take(),
            [
                Page,
                Leaf,
                Capture(value),
                CopyOut(value),
                Release(value),
                Finish
            ]
        );
        assert_eq!(progress(&cursor), (Some(value), Some(value)));
    }
    end(advance(&mut cursor, source(), &mut output, &none));
    assert_eq!(log.take(), [Page, Leaf, ReleasePage, Finish]);
    assert!(cursor.progress.exhausted);
    // The same valid cursor answers End again without an operation.
    end(advance(&mut cursor, source(), &mut output, &none));
    assert_eq!(log.take(), UNOPENED);
    assert_eq!(progress(&cursor), (Some(40), Some(40)));
    cursor.close().unwrap();
}

#[test]
fn mcb00_d06_cursor_reseeks_by_full_row_id_after_a_gap() {
    let leaf_a = leaf_page("leaf_a");
    let leaf_b = leaf_page("leaf_b");
    let leaf_c = leaf_page("leaf_c");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut output = [FILL; 16];
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let source = LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    returned(advance(&mut cursor, source, &mut output, &none));
    log.take();

    // Leaf B gained 05, 15 and 25 and shifted every slot. The reseek from 20
    // skips the lower rows, judges foreign Active 25 invisible, returns 30.
    let source =
        LiteralSource::new(&leaf_b, &log).with_status(25, Answer::Ready(TransactionState::Active));
    let copy = returned(advance(&mut cursor, source, &mut output, &none));
    assert_eq!(copy.version.row, row(30));
    assert_eq!(output[..13], V300);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(25),
            Status(25),
            MissingBefore(25),
            Release(25),
            Page,
            Leaf,
            Capture(30),
            CopyOut(30),
            Release(30),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (Some(30), Some(30)));

    // Leaf C no longer holds 30: its lower bound is already the next row.
    let copy = returned(advance(
        &mut cursor,
        LiteralSource::new(&leaf_c, &log),
        &mut output,
        &none,
    ));
    assert_eq!(copy.version.row, row(40));
    assert_eq!(output[..13], V400);
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(40), CopyOut(40), Release(40), Finish]
    );
    end(advance(
        &mut cursor,
        LiteralSource::new(&leaf_c, &log),
        &mut output,
        &none,
    ));
    assert_eq!(log.take(), [Page, Leaf, ReleasePage, Finish]);

    // Bounded [20, 40) over the shifted leaf: 40 is outside the range.
    let mut bounded = RowCursor::open(&view, &table, Some(row(20)), Some(row(40))).unwrap();
    let source = || {
        LiteralSource::new(&leaf_b, &log)
            .with_status(20, committed(8))
            .with_status(25, Answer::Ready(TransactionState::Active))
    };
    let copy = returned(advance(&mut bounded, source(), &mut output, &none));
    assert_eq!(copy.version.row, row(20));
    assert_eq!(output[..13], V200);
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(20),
            Status(20),
            CopyOut(20),
            Release(20),
            Finish
        ]
    );
    let copy = returned(advance(&mut bounded, source(), &mut output, &none));
    assert_eq!(copy.version.row, row(30));
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(25),
            Status(25),
            MissingBefore(25),
            Release(25),
            Page,
            Leaf,
            Capture(30),
            CopyOut(30),
            Release(30),
            Finish
        ]
    );
    end(advance(&mut bounded, source(), &mut output, &none));
    assert_eq!(log.take(), [Page, Leaf, ReleasePage, Finish]);
    assert_eq!(progress(&bounded), (Some(30), Some(30)));
}

#[test]
fn mcb00_d07_cursor_output_cancel_and_cleanup_keep_the_visible_row() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let source = || LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    let retried = [
        Page,
        Leaf,
        Capture(20),
        Status(20),
        CopyOut(20),
        Release(20),
        Finish,
    ];

    // Short output: 10 stays examined, 20 is neither copied nor skipped.
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let mut short = [FILL; 12];
    assert_eq!(
        read_error(advance(&mut cursor, source(), &mut short, &none)),
        ReadError::OutputTooSmall { required: 13 }
    );
    assert_eq!(short, [FILL; 12]);
    assert_eq!(
        log.take(),
        examined_then(&[Page, Leaf, Capture(20), Status(20), Release(20), Finish])
    );
    assert_eq!(progress(&cursor), (Some(10), None));
    let mut exact = [FILL; 13];
    returned(advance(&mut cursor, source(), &mut exact, &none));
    assert_eq!(exact, V200);
    assert_eq!(log.take(), retried);
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    // Cancelled after 10 was judged (sampled by the next reseek) or after
    // 20's Status (sampled before copy): 20 is retried, not skipped.
    for (hook, tail) in [
        (MissingBefore(10), vec![Page, Leaf, ReleasePage, Finish]),
        (
            Status(20),
            vec![Page, Leaf, Capture(20), Status(20), Release(20), Finish],
        ),
    ] {
        let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
        let token = Cancellation::default();
        let mut output = [FILL; 16];
        assert_eq!(
            read_error(advance(
                &mut cursor,
                source().cancelling_at(hook, &token),
                &mut output,
                &token
            )),
            ReadError::Cancelled
        );
        assert_eq!(output, [FILL; 16]);
        assert_eq!(log.take(), examined_then(&tail));
        assert_eq!(progress(&cursor), (Some(10), None));
        // Still cancelled: refused before an operation is opened.
        assert_eq!(
            read_error(advance(&mut cursor, source(), &mut output, &token)),
            ReadError::Cancelled
        );
        assert_eq!(log.take(), UNOPENED);
        returned(advance(&mut cursor, source(), &mut output, &none));
        assert_eq!(output[..13], V200);
        assert_eq!(log.take(), retried);
        assert_eq!(progress(&cursor), (Some(20), Some(20)));
    }

    // A copy whose release or finish fails is not published or advanced.
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let mut output = [FILL; 16];
    let failing = source().failing_release(20, Effect::Unchanged);
    let error = source_failure(advance(&mut cursor, failing, &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(
        log.take(),
        examined_then(&[
            Page,
            Leaf,
            Capture(20),
            Status(20),
            CopyOut(20),
            Release(20),
            Finish
        ])
    );
    assert_eq!(progress(&cursor), (Some(10), None));
    let failing = source().failing_finish(Effect::Unchanged);
    let error = source_failure(advance(&mut cursor, failing, &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(log.take(), retried);
    assert_eq!(progress(&cursor), (Some(10), None));
    returned(advance(&mut cursor, source(), &mut output, &none));
    assert_eq!(log.take(), retried);
    assert_eq!(progress(&cursor), (Some(20), Some(20)));
}

#[test]
fn mcb00_d08_statement_and_revision_precede_end_and_cancel() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let source = || LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    let mut output = [FILL; 16];
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    returned(advance(&mut cursor, source(), &mut output, &none));
    log.take();

    // C's own mutation moved the view revision: no operation, keys unchanged.
    view.revision.set(1);
    let mut untouched = [FILL; 16];
    for cancel in [&none, &cancelled] {
        assert_eq!(
            read_error(advance(&mut cursor, source(), &mut untouched, cancel)),
            ReadError::CursorInvalidated
        );
        assert_eq!(log.take(), UNOPENED);
    }
    assert_eq!(untouched, [FILL; 16]);
    assert_eq!(progress(&cursor), (Some(20), Some(20)));

    // A cursor reopened at revision 1 reads the same snapshot to its end.
    let mut reopened = RowCursor::open(&view, &table, None, None).unwrap();
    for value in [20, 30, 40] {
        let copy = returned(advance(&mut reopened, source(), &mut output, &none));
        assert_eq!(copy.version.row, row(value));
    }
    end(advance(&mut reopened, source(), &mut output, &none));
    log.take();
    // After End, a revision change still wins over End and cancellation.
    view.revision.set(2);
    assert_eq!(
        read_error(advance(&mut reopened, source(), &mut output, &cancelled)),
        ReadError::CursorInvalidated
    );
    assert_eq!(log.take(), UNOPENED);
    assert!(reopened.progress.exhausted);

    // An ended statement is refused before revision, End or any operation.
    view.ended.set(true);
    for cursor in [&mut cursor, &mut reopened] {
        let error = source_failure(advance(cursor, source(), &mut output, &none));
        assert!(matches!(error.cause, RetentionCause::WrongState));
        assert_eq!(log.take(), UNOPENED);
    }
    assert_eq!(progress(&cursor), (Some(20), Some(20)));
    assert_eq!(progress(&reopened), (Some(40), Some(40)));
}

#[test]
fn mcb00_d09_close_returns_only_its_own_credit() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView {
        capacity: 2,
        ..LiteralView::new(10, None)
    };
    let table = LiteralTable::default();

    // On a live statement the range is checked before the credit, the only
    // effect, is reserved.
    let Err(RowReadError::Read(error)) =
        RowCursor::open(&view, &table, Some(row(40)), Some(row(20)))
    else {
        panic!("a reversed range must be refused");
    };
    assert_eq!(error, ReadError::InvalidRange);
    assert_eq!((view.charged.get(), view.issued.get()), (0, 0));
    let first = RowCursor::open(&view, &table, None, None).unwrap();
    let mut second = RowCursor::open(&view, &table, Some(row(20)), None).unwrap();
    let Err(RowReadError::Source(error)) = RowCursor::open(&view, &table, None, None) else {
        panic!("a third credit must be refused");
    };
    assert!(matches!(error.cause, RetentionCause::Capacity));
    assert_eq!((view.charged.get(), view.issued.get()), (2, 2));

    // Explicit close returns credit 1 only; the other cursor keeps working.
    first.close().unwrap();
    assert_eq!(view.charged.get(), 1);
    assert_eq!(*view.returned.borrow(), [(1, Returned::Close)]);
    let mut output = [FILL; 16];
    let source = LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    let copy = returned(advance(&mut second, source, &mut output, &none));
    assert_eq!(copy.version.row, row(20));
    assert_eq!(view.charged.get(), 1);

    // Drop returns the remaining credit through the credit's own Drop.
    drop(second);
    assert_eq!(view.charged.get(), 0);
    assert_eq!(
        *view.returned.borrow(),
        [(1, Returned::Close), (2, Returned::Drop)]
    );
    // A new cursor takes a new credit; closing never touches the view.
    let third = RowCursor::open(&view, &table, None, None).unwrap();
    assert_eq!((view.charged.get(), view.issued.get()), (1, 3));
    third.close().unwrap();
    assert_eq!(
        *view.returned.borrow(),
        [
            (1, Returned::Close),
            (2, Returned::Drop),
            (3, Returned::Close)
        ]
    );
    assert_eq!((view.revision.get(), view.ended.get()), (0, false));
}

#[test]
fn mcb00_d10_ended_statement_precedes_range_credit_and_operation() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    view.ended.set(true);

    // Open: the statement is checked before a reversed range, and no credit
    // is reserved.
    for (low, high) in [(Some(row(40)), Some(row(20))), (None, None)] {
        let Err(RowReadError::Source(error)) = RowCursor::open(&view, &table, low, high) else {
            panic!("an ended statement must be refused first");
        };
        assert!(matches!(error.cause, RetentionCause::WrongState));
        assert_eq!(error.effect, Effect::Unchanged);
    }
    assert_eq!((view.charged.get(), view.issued.get()), (0, 0));
    assert!(view.returned.borrow().is_empty());

    // Point: refused before cancellation, and no operation is opened.
    let mut output = [FILL; 16];
    for cancel in [&none, &cancelled] {
        let source = LiteralSource::new(&leaf_a, &log);
        let error = source_failure(point(&view, source, row(30), &mut output, cancel));
        assert!(matches!(error.cause, RetentionCause::WrongState));
        assert_eq!(log.take(), UNOPENED);
    }
    assert_eq!(output, [FILL; 16]);
    assert_eq!(table.opened.get(), 0);
}

#[test]
fn mcb00_d11_cursor_operations_open_on_its_own_view_and_table() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let cancelled = Cancellation::default();
    cancelled.cancel();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let source = || LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    let mut output = [FILL; 16];

    // Each call opens one operation on the view and table the cursor was
    // opened with; the caller passes neither again.
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    for value in [20, 30, 40] {
        let copy = returned(cursor.next(
            |opened_view, opened_table| {
                assert!(ptr::eq(opened_view, &view) && ptr::eq(opened_table, &table));
                opened_table.opened.set(opened_table.opened.get() + 1);
                Ok(source())
            },
            &mut output,
            &none,
        ));
        assert_eq!(copy.version.row, row(value));
    }
    end(advance(&mut cursor, source(), &mut output, &none));
    assert_eq!(table.opened.get(), 4);
    log.take();

    // Exhausted at the same revision: End wins over a cancelled token, and
    // no operation is opened.
    end(advance(&mut cursor, source(), &mut output, &cancelled));
    assert_eq!(log.take(), UNOPENED);
    assert_eq!(table.opened.get(), 4);
    assert_eq!(progress(&cursor), (Some(40), Some(40)));
    cursor.close().unwrap();

    // C's refusal to open comes back unchanged before any page; keys stay.
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    returned(advance(&mut cursor, source(), &mut output, &none));
    log.take();
    let refused = cursor.next(
        |_, _| {
            Err::<LiteralSource<'_>, _>(RetentionError {
                phase: RetentionPhase::Source,
                cause: RetentionCause::Unsupported,
                effect: Effect::Unchanged,
            })
        },
        &mut output,
        &none,
    );
    assert!(matches!(
        source_failure(refused).cause,
        RetentionCause::Unsupported
    ));
    assert_eq!(log.take(), UNOPENED);
    assert_eq!(progress(&cursor), (Some(20), Some(20)));
    let copy = returned(advance(&mut cursor, source(), &mut output, &none));
    assert_eq!(copy.version.row, row(30));
    assert_eq!(output[..13], V300);
}

#[test]
fn mcb00_d12_cleanup_failure_replaces_flush_and_scan_errors_keep_progress() {
    let leaf_a = leaf_page("leaf_a");
    let log = RefCell::new(Vec::new());
    let none = Cancellation::default();
    let view = LiteralView::new(10, None);
    let table = LiteralTable::default();
    let mut output = [FILL; 16];

    // Point: a failed checked cleanup replaces a pending FlushRequired.
    let source = LiteralSource::new(&leaf_a, &log)
        .with_status(20, Answer::Flush(8))
        .failing_release(20, Effect::Unchanged);
    let error = source_failure(point(&view, source, row(20), &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(
        log.take(),
        [Page, Leaf, Capture(20), Status(20), Release(20), Finish]
    );
    let source = LiteralSource::new(&leaf_a, &log)
        .with_page(Answer::Flush(7))
        .failing_finish(Effect::CallerMustResolve);
    let error = source_failure(point(&view, source, row(30), &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Fenced));
    assert_eq!(error.effect, Effect::CallerMustResolve);
    assert_eq!(log.take(), [Page, Finish]);

    // Cursor: the same priority, and no key moves.
    let mut cursor = RowCursor::open(&view, &table, None, None).unwrap();
    let source = LiteralSource::new(&leaf_a, &log)
        .with_missing(10, Answer::Flush(9))
        .failing_finish(Effect::Unchanged);
    let error = source_failure(advance(&mut cursor, source, &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(
        log.take(),
        [
            Page,
            Leaf,
            Capture(10),
            MissingBefore(10),
            Release(10),
            Finish
        ]
    );
    assert_eq!(progress(&cursor), (None, None));

    // A Status failure after 10 was judged keeps 10 examined and 20 unread.
    let source = LiteralSource::new(&leaf_a, &log).with_status(20, Answer::Fail(Effect::Unchanged));
    let error = source_failure(advance(&mut cursor, source, &mut output, &none));
    assert!(matches!(error.cause, RetentionCause::Busy));
    assert_eq!(
        log.take(),
        examined_then(&[Page, Leaf, Capture(20), Status(20), Release(20), Finish])
    );
    assert_eq!(progress(&cursor), (Some(10), None));

    // At q4, 10 and 20 are invisible with Missing-before, but 30 (committed
    // at 5 without Undo) is corruption: the scan stops on it, never skips it.
    let early = LiteralView::new(4, None);
    let mut cursor = RowCursor::open(&early, &table, None, None).unwrap();
    let source = || LiteralSource::new(&leaf_a, &log).with_status(20, committed(8));
    assert_eq!(
        read_error(advance(&mut cursor, source(), &mut output, &none)),
        ReadError::MissingUndo
    );
    assert_eq!(
        log.take(),
        examined_then(&[
            Page,
            Leaf,
            Capture(20),
            Status(20),
            MissingBefore(20),
            Release(20),
            Page,
            Leaf,
            Capture(30),
            Release(30),
            Finish
        ])
    );
    assert_eq!(progress(&cursor), (Some(20), None));
    // The next call meets the same row and the same error.
    assert_eq!(
        read_error(advance(&mut cursor, source(), &mut output, &none)),
        ReadError::MissingUndo
    );
    assert_eq!(log.take(), [Page, Leaf, Capture(30), Release(30), Finish]);
    assert_eq!(progress(&cursor), (Some(20), None));
    assert_eq!(output, [FILL; 16]);
}
