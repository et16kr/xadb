//! Actual fresh live consumers. Independent committed literals are expected
//! bytes; no completed test function or product encoder supplies an oracle.
use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, init_test_support as probe, status_load_test_support as slots},
    page::{PageKind, PageRef},
    retention_page::JobSlotRef,
    wal_frame::MAX_FRAME_BYTES,
    wal_io::FreshStorageSession,
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
    namespace::DirectoryPort,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[allow(
    clippy::duplicate_mod,
    reason = "literal recipe binds to this module's distinct lifecycle Oracle type"
)]
#[path = "literal_tests.rs"]
mod literals;

type State = Arc<Mutex<port::State>>;
type Live<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w> =
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, port::Writer>;
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().ok().unwrap()
}
fn hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

struct Oracle {
    files: BTreeMap<String, Vec<u8>>,
}
impl Oracle {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/dev3-plan4-empty-retention/vectors/p1-o00");
        let index = std::fs::read(root.join("raw-index.tsv")).unwrap();
        assert_eq!(
            hex(&index),
            "ce3fd367007a6d798dbb6c2f18367a714d4f105611fb190fe57a1aa72d6d99a8"
        );
        let mut files = BTreeMap::new();
        let mut total = 0;
        for line in std::str::from_utf8(&index).unwrap().lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            let path = fields[0];
            assert!(!path.starts_with('/') && !path.split('/').any(|s| s == ".."));
            let size = fields[1].parse::<usize>().unwrap();
            assert!(size <= 56_712);
            let bytes = std::fs::read(root.join(path)).unwrap();
            assert_eq!(bytes.len(), size);
            assert_eq!(hex(&bytes), fields[2]);
            total += size;
            assert!(total <= 550_160);
            assert!(files.insert(path.to_owned(), bytes).is_none());
        }
        assert_eq!((files.len(), total), (34, 550_160));
        Self { files }
    }
    fn raw(&self, name: &str) -> &[u8] {
        &self.files[&format!("raw/{name}")]
    }
    fn page(&self, name: &str) -> &[u8; PAGE_BYTES] {
        self.raw(name).try_into().unwrap()
    }
    fn genesis(&self, number: usize) -> &[u8; PAGE_BYTES] {
        self.files[&format!("references/P{number:02}.bin")]
            .as_slice()
            .try_into()
            .unwrap()
    }
}

fn clear(state: &State) {
    let mut s = state.lock().unwrap();
    s.events.clear();
    s.fault_hits = 0;
    s.hook_hits = 0;
}
fn effects(state: &State) -> (usize, usize, usize) {
    let s = state.lock().unwrap();
    (s.events.len(), s.mutations(), s.entropy_calls)
}
fn rejected<T>(result: Result<T>) -> RetentionError {
    match result {
        Ok(_) => panic!("expected new boundary rejection"),
        Err(e) => e,
    }
}
fn frame_error(
    error: RetentionError,
    phase: RetentionPhase,
    cause: dirty_frame::Cause,
    effect: Effect,
) {
    assert_eq!((error.phase, error.effect), (phase, effect));
    assert!(
        matches!(error.cause, RetentionCause::Frame(e) if e.cause == cause && e.effect == effect)
    );
}
#[allow(
    clippy::type_complexity,
    reason = "test oracle retains exact metadata, full bytes and registration together"
)]
fn snapshots(
    bound: &Live<'_, '_, '_, '_, '_, '_, '_, '_>,
) -> Vec<(probe::RawSnapshot, Vec<u8>, Option<(TargetRequest, u64)>)> {
    (0..4)
        .map(|i| {
            let mut bytes = vec![0; PAGE_BYTES];
            let metadata = probe::snapshot(
                &bound.inner.pool,
                i,
                bytes.as_mut_slice().try_into().unwrap(),
            );
            (metadata, bytes, slots::slot(&bound.inner.pool, i))
        })
        .collect()
}
fn inject(
    state: &State,
    op: &'static str,
    name: &str,
    offset: u64,
    occurrence: usize,
    action: port::Action,
) {
    state.lock().unwrap().fault = Some(port::Fault {
        key: port::Key::new(op, name, offset, occurrence),
        action,
    });
}
fn fault_hits(state: &State) -> usize {
    state.lock().unwrap().fault_hits
}
fn data(state: &State, page: usize) -> Vec<u8> {
    let s = state.lock().unwrap();
    s.files["d3-data-0000000100000001"].bytes[page * PAGE_BYTES..(page + 1) * PAGE_BYTES].to_vec()
}
fn wal(state: &State) -> Vec<u8> {
    state.lock().unwrap().files[crate::storage::wal_io::WAL_NAME]
        .bytes
        .clone()
}
fn page_bytes(
    bound: &Live<'_, '_, '_, '_, '_, '_, '_, '_>,
    number: u64,
) -> (probe::RawSnapshot, Vec<u8>) {
    for i in 0..4 {
        if slots::slot(&bound.inner.pool, i).is_some_and(|s| s.0.identity.page.page_no == number) {
            let mut bytes = vec![0; PAGE_BYTES];
            let metadata = probe::snapshot(
                &bound.inner.pool,
                i,
                bytes.as_mut_slice().try_into().unwrap(),
            );
            return (metadata, bytes);
        }
    }
    panic!("page {number} not resident")
}
fn assert_page(bound: &Live<'_, '_, '_, '_, '_, '_, '_, '_>, number: u64, expected: &[u8]) {
    let (s, actual) = page_bytes(bound, number);
    assert_eq!(actual, expected);
    assert_eq!(s.total, 0);
}
fn job(generation: u64) -> JobSlotRef {
    JobSlotRef::new(
        PageRef {
            file: crate::storage::page::FileId::new(0x100000001).unwrap(),
            page_no: 2,
            incarnation: Generation::new(1).unwrap(),
        },
        0,
        Generation::new(generation).unwrap(),
    )
    .unwrap()
}
fn flush<'s, 'c>(
    bound: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    core: &'c PageConcurrency<'s>,
    target: &Plan4Target,
) {
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let (_, _, mut f) = bound.split();
    assert!(matches!(
        f.try_flush(&mut dedicated, target).unwrap(),
        RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
    ));
    assert_eq!(f.phase(), RetentionFlushPhase::Ready);
}
fn prepare<'s, 'c>(
    bound: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    owner: &mut Owner<'c, 's>,
    core: &'c PageConcurrency<'s>,
    status: &Plan4Target,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..8 {
        match bound
            .prepare_empty_targets(owner, status, job(2), &mut cursor)
            .unwrap()
        {
            None => return,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                flush(bound, core, &target)
            }
            Some(_) => panic!("unexpected prepared step"),
        }
    }
    panic!("finite target preparation did not finish")
}
fn init<'s, 'c>(
    bound: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    owner: &mut Owner<'c, 's>,
    core: &'c PageConcurrency<'s>,
) -> Plan4Target {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..8 {
        match bound.prepare_status_slot(owner, &mut cursor).unwrap() {
            None => {
                let reserved = bound.reserve_status(owner).unwrap();
                assert_eq!(
                    (reserved.page().page_no, reserved.page().incarnation.get()),
                    (9, 2)
                );
                return match reserved.initialize() {
                    Ok(target) => target,
                    Err(RetentionReservationError::Before { error, .. })
                    | Err(RetentionReservationError::Fatal(error)) => panic!("INIT: {error:?}"),
                };
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                flush(bound, core, &target)
            }
            Some(_) => panic!("unexpected prepared INIT step"),
        }
    }
    panic!("finite INIT preparation did not finish")
}

macro_rules! fixture {
    (|$bound:ident, $owner:ident, $core:ident, $foreign:ident, $state:ident, $storage:ident| $body:block) => {{
        let directory = port::Directory::fresh();
        directory.state().lock().unwrap().live_writes = true;
        let $state = directory.state();
        let mut slot = boxed::<MAX_FRAME_BYTES>();
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let fresh = FreshStorageSession::create_retention_catalog(directory.acquire_writer().unwrap(), 1, &mut slot, &mut maintenance).unwrap();
        let $storage = fresh.install_doublewrite(&mut dwb).unwrap().into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        let $foreign = PageConcurrency::new($storage.session_fence()).unwrap();
        let mut $owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut isolation = boxed::<4_160>();
        let mut control = boxed::<RETENTION_CONTROL_BYTES>();
        let mut replay = boxed::<RETENTION_REPLAY_BYTES>();
        let old = $storage.bind_plan4_pages(&$core, &mut frames, &mut operation).unwrap();
        let mut $bound = match old.into_empty_retention(&mut $owner, EmptyRetentionControlLoan { isolation: &mut isolation, control: &mut control, replay: &mut replay }) {
            Ok(v) => v,
            Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => panic!("activation: {error:?}"),
        };
        clear(&$state);
        $body
        drop($bound);
        drop($owner);
        drop($storage);
        let remaining = { let s = $state.lock().unwrap(); (s.writers, s.handles) };
        assert_eq!(remaining, (0, 0));
    }};
}

fn complete<'s, 'c>(
    bound: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_>,
    owner: &mut Owner<'c, 's>,
    core: &'c PageConcurrency<'s>,
    state: &State,
    oracle: &Oracle,
) -> Plan4Target {
    let reserve = literals::population(oracle, oracle.genesis(0), 1, 1, 0, &[]);
    let initialized =
        literals::population(oracle, &reserve.allocator, 1, 1, 2, &reserve.descriptor);
    let status = init(bound, owner, core);
    let bytes = wal(state);
    assert_eq!(&bytes[4096..25_296], reserve.frame);
    assert_eq!(&bytes[25_296..63_296], initialized.frame);
    assert_page(bound, 0, &initialized.allocator);
    assert_eq!(
        &bound.control.ledger.row(control::ALLOCATION_SLOT)[..1536],
        initialized.descriptor
    );
    drop((bytes, reserve, initialized));
    assert_page(bound, 9, oracle.page("status-free.page"));
    assert_eq!(bound.inner.storage.wal().prefix().unwrap().complete_lsn, 2);
    prepare(bound, owner, core, &status);
    let root = bound.target(Plan4Page::Root).unwrap();
    let mut expected_offset = 4096 + 21_200 + 38_000;
    for (index, (frame, root_name, jobs_name, status_name)) in [
        (
            "aging-start.wal",
            "root-start.page",
            None,
            "status-free.page",
        ),
        (
            "begin.wal",
            "root-begin.page",
            Some("jobs-begin.page"),
            "status-begin.page",
        ),
        (
            "rollback.wal",
            "root-begin.page",
            Some("jobs-begin.page"),
            "status-rollback.page",
        ),
        (
            "abort.wal",
            "root-abort.page",
            Some("jobs-abort.page"),
            "status-abort.page",
        ),
        (
            "finish.wal",
            "root-finish.page",
            Some("jobs-finish.page"),
            "status-finish.page",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let actor = crate::storage::retention_page::tests::actor(1, 2);
        let observation = {
            let (mut w, _, _) = bound.split();
            match index {
                0 => w.aging_start(owner),
                1 => w.begin_empty(owner, &status, job(2)),
                2 => w.rollback_start(owner, actor),
                3 => w.abort_end_empty(owner, actor),
                4 => w.finish_empty_terminal(owner, job(2)),
                _ => unreachable!(),
            }
            .unwrap()
        };
        assert_eq!(observation.lsn.0, index as u64 + 3);
        assert_eq!(
            (observation.issued_local, observation.adopted_local),
            if index == 0 { (0, 0) } else { (1, 1) }
        );
        let bytes = wal(state);
        assert_eq!(
            &bytes[expected_offset..expected_offset + oracle.raw(frame).len()],
            oracle.raw(frame)
        );
        expected_offset += oracle.raw(frame).len();
        assert_eq!(bytes.len(), expected_offset);
        assert_page(bound, 1, oracle.page(root_name));
        if let Some(name) = jobs_name {
            assert_page(bound, 2, oracle.page(name));
        }
        assert_page(bound, 9, oracle.page(status_name));
        let aging_descriptor = match index {
            0 => "aging-start.descriptor",
            1 | 2 => "aging-begin.descriptor",
            3 => "aging-abort.descriptor",
            4 => "aging-finish.descriptor",
            _ => unreachable!(),
        };
        assert_eq!(
            &bound.control.ledger.row(control::AGING_SLOT)[..1536],
            oracle.raw(aging_descriptor)
        );
        if matches!(index, 1 | 2) {
            let slot = bound
                .control
                .ledger
                .slot(actor)
                .expect("actual stable ATT row");
            assert_eq!(
                &bound.control.ledger.row(slot)[..1536],
                oracle.raw(if index == 1 {
                    "tx-begin.descriptor"
                } else {
                    "tx-rollback.descriptor"
                })
            );
        } else {
            // Successful AbortEnd retires its live ATT row before returning.
            // Its terminal descriptor remains in the real staged operation
            // frame, so compare that transient result and the cleared bank.
            assert!(
                bound.control.ledger.bytes[..32 * control::SLOT_BYTES]
                    .iter()
                    .all(|b| *b == 0)
            );
            if index == 3 {
                assert_eq!(
                    &bound.inner.operation[416..1952],
                    oracle.raw("tx-abort.descriptor")
                );
            }
        }
        owner.check_idle().unwrap();
        assert!(bound.control.ledger.pending.is_none());
    }
    assert_eq!(expected_offset, 254_544);
    assert!(
        bound.control.ledger.bytes[..32 * control::SLOT_BYTES]
            .iter()
            .all(|b| *b == 0)
    );
    assert_eq!(
        &bound.control.ledger.row(control::AGING_SLOT)[..1536],
        oracle.raw("aging-finish.descriptor")
    );
    let (_, reader, _) = bound.split();
    let read = reader.read(owner, &root).unwrap();
    assert_eq!(read.bytes().unwrap(), oracle.page("root-finish.page"));
    read.release_checked().unwrap();
    status
}

#[test]
fn retention_live_lifecycle() {
    let mut h = Harness::new(&["P4-LIVE-L01", "P4-LIVE-L02", "P4-LIVE-L03", "P4-LIVE-L04"]);
    for case in 1..=4 {
        h.case(&format!("P4-LIVE-L{case:02}"), || {
            let oracle = Oracle::new();
            fixture!(|bound, owner, core, _foreign, state, storage| {
                match case {
                    1 => {
                        let _ = complete(&mut bound, &mut owner, &core, &state, &oracle);
                        let (writer, reader, flusher) = bound.split();
                        assert!(
                            std::ptr::eq(writer.pool, reader.pool)
                                && std::ptr::eq(reader.pool, flusher.pool)
                        );
                        assert!(std::ptr::eq(writer.storage, &storage));
                        assert!(
                            reader
                                .pool
                                .uses_storage_origin(&storage.binding, storage.session_fence())
                        );
                    }
                    2 => {
                        let mut expected_allocator = oracle.genesis(0).to_vec();
                        let mut expected_frames = Vec::new();
                        let mut cursor = Plan4ResidencyCursor::new();
                        assert!(
                            bound
                                .prepare_status_slot(&owner, &mut cursor)
                                .unwrap()
                                .is_none()
                        );
                        let reserved = bound.reserve_status(&mut owner).unwrap();
                        assert_eq!(reserved.page().incarnation.get(), 2);
                        match reserved.cancel() {
                            Ok(()) => {}
                            Err(RetentionReservationError::Before { error, .. })
                            | Err(RetentionReservationError::Fatal(error)) => {
                                panic!("cancel: {error:?}")
                            }
                        }
                        let expected_reserve =
                            literals::population(&oracle, &expected_allocator, 1, 1, 0, &[]);
                        let expected_cancel = literals::population(
                            &oracle,
                            &expected_reserve.allocator,
                            1,
                            1,
                            1,
                            &expected_reserve.descriptor,
                        );
                        expected_frames.extend_from_slice(&expected_reserve.frame);
                        expected_frames.extend_from_slice(&expected_cancel.frame);
                        expected_allocator = expected_cancel.allocator;
                        assert_eq!(&wal(&state)[4096..], expected_frames);
                        assert_page(&bound, 0, &expected_allocator);
                        assert!(data(&state, 9).iter().all(|b| *b == 0));
                        assert!(
                            bound
                                .prepare_status_slot(&owner, &mut cursor)
                                .unwrap()
                                .is_none()
                        );
                        let reserved = bound.reserve_status(&mut owner).unwrap();
                        assert_eq!(
                            (reserved.page().page_no, reserved.page().incarnation.get()),
                            (9, 3)
                        );
                        match reserved.cancel() {
                            Ok(()) => {}
                            Err(_) => panic!("second real cancel"),
                        }
                        let expected_reserve =
                            literals::population(&oracle, &expected_allocator, 2, 3, 0, &[]);
                        let expected_cancel = literals::population(
                            &oracle,
                            &expected_reserve.allocator,
                            2,
                            3,
                            1,
                            &expected_reserve.descriptor,
                        );
                        expected_frames.extend_from_slice(&expected_reserve.frame);
                        expected_frames.extend_from_slice(&expected_cancel.frame);
                        assert_eq!(&wal(&state)[4096..], expected_frames);
                        assert_page(&bound, 0, &expected_cancel.allocator);
                        assert_eq!(storage.wal().prefix().unwrap().complete_lsn, 4);
                        assert_eq!(wal(&state).len(), 4096 + 4 * 21_200);
                        owner.check_idle().unwrap();
                    }
                    3 | 4 => {
                        let status = complete(&mut bound, &mut owner, &core, &state, &oracle);
                        let mut cursor = Plan4ResidencyCursor::new();
                        let mut flushed = 0;
                        for _ in 0..8 {
                            match bound.prepare_catalog(&owner, &mut cursor).unwrap() {
                                None => break,
                                Some(RetentionResidencyStep::FlushRequired { target, rec_lsn }) => {
                                    assert!(rec_lsn.0 > 0);
                                    flush(&mut bound, &core, &target);
                                    flushed += 1;
                                }
                                Some(_) => panic!("unexpected Catalog step"),
                            }
                        }
                        assert!(flushed >= 1);
                        {
                            let (_, reader, _) = bound.split();
                            assert_eq!(
                                reader
                                    .lookup_catalog_empty(&mut owner, ObjectId::new(6).unwrap())
                                    .unwrap(),
                                super::super::super::Plan4CatalogLookup::NotFound
                            );
                        }
                        if case == 3 {
                            prepare(&mut bound, &mut owner, &core, &status);
                            assert_page(&bound, 1, oracle.page("root-finish.page"));
                            assert_page(&bound, 2, oracle.page("jobs-finish.page"));
                            assert_page(&bound, 9, oracle.page("status-finish.page"));
                        } else {
                            assert_eq!(data(&state, 7), oracle.genesis(7));
                            assert_eq!(data(&state, 8), oracle.genesis(8));
                            let before = effects(&state);
                            let (_, reader, _) = bound.split();
                            assert_eq!(
                                reader
                                    .lookup_catalog_empty(
                                        &mut owner,
                                        ObjectId::new(u64::MAX).unwrap()
                                    )
                                    .unwrap(),
                                super::super::super::Plan4CatalogLookup::NotFound
                            );
                            assert_eq!(effects(&state), before);
                        }
                    }
                    _ => unreachable!(),
                }
            });
        });
    }
    h.finish();
}

#[test]
fn retention_live_reader_boundaries() {
    let mut h = Harness::new(&["P4-LIVE-R01", "P4-LIVE-R02", "P4-LIVE-R03", "P4-LIVE-R04"]);
    for case in 1..=4 {
        h.case(&format!("P4-LIVE-R{case:02}"), || {
            fixture!(|bound, owner, core, foreign, state, storage| {
                let target = init(&mut bound, &mut owner, &core);
                let mut bad = target;
                if case == 1 {
                    bad.request.identity.owner_generation = Generation::new(2).unwrap();
                }
                if case == 4 {
                    let mut cursor = Plan4ResidencyCursor::new();
                    for _ in 0..8 {
                        match bound.prepare_catalog(&owner, &mut cursor).unwrap() {
                            None => break,
                            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                                flush(&mut bound, &core, &target)
                            }
                            Some(_) => panic!("unexpected preparation"),
                        }
                    }
                    bad = bound.target(Plan4Page::CatalogAnchor).unwrap();
                }
                let mut foreign_owner = foreign.try_owner(OwnerRole::Foreground).unwrap();
                if case == 3 {
                    std::mem::forget(owner.try_apply().unwrap());
                }
                let before = snapshots(&bound);
                clear(&state);
                let io = effects(&state);
                let error = {
                    let (_, reader, _) = bound.split();
                    rejected(reader.read(
                        if case == 2 {
                            &mut foreign_owner
                        } else {
                            &mut owner
                        },
                        &bad,
                    ))
                };
                match case {
                    1 => frame_error(
                        error,
                        RetentionPhase::Census,
                        dirty_frame::Cause::MissingTarget,
                        Effect::Unchanged,
                    ),
                    2 => frame_error(
                        error,
                        RetentionPhase::Census,
                        dirty_frame::Cause::BindingMismatch,
                        Effect::Unchanged,
                    ),
                    3 => {
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        assert!(storage.session_fence().is_fenced());
                    }
                    4 => {
                        assert_eq!(error.effect, Effect::Unchanged);
                        assert!(matches!(error.cause, RetentionCause::Membership));
                    }
                    _ => unreachable!(),
                }
                assert_eq!(effects(&state), io);
                assert_eq!(snapshots(&bound), before);
                if case != 3 {
                    owner.check_idle().unwrap();
                    assert!(!storage.session_fence().is_fenced());
                }
                if case == 3 {
                    let error = foreign_owner.check_idle().unwrap_err();
                    assert_eq!(error.cause, crate::transaction::concurrency::Cause::Fenced);
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                } else {
                    foreign_owner.check_idle().unwrap();
                }
            });
        });
    }
    h.finish();
}

#[test]
fn retention_live_latest_load() {
    let mut h = Harness::new(&[
        "P4-LIVE-D01",
        "P4-LIVE-D02",
        "P4-LIVE-D03",
        "P4-LIVE-D04",
        "P4-LIVE-D05",
        "P4-LIVE-D06",
    ]);
    for case in 1..=6 {
        h.case(&format!("P4-LIVE-D{case:02}"), || {
            fixture!(|bound, owner, core, _foreign, state, storage| {
                let target = bound.target(Plan4Page::CatalogLeaf).unwrap();
                let before = snapshots(&bound);
                clear(&state);
                match case {
                    1 => inject(
                        &state,
                        "read",
                        "d3-data-0000000100000001",
                        8 * PAGE_BYTES as u64,
                        1,
                        port::Action::Short(7),
                    ),
                    2 => inject(
                        &state,
                        "open",
                        "d3-data-0000000100000001",
                        0,
                        1,
                        port::Action::OpenedIdentity,
                    ),
                    3 => {
                        let mut changed = state.lock().unwrap().files["d3-data-0000000100000001"]
                            .bytes
                            .clone();
                        changed[8 * PAGE_BYTES + 80] ^= 1;
                        state.lock().unwrap().hook = Some(port::Hook {
                            key: port::Key::new(
                                "read",
                                "d3-data-0000000100000001",
                                8 * PAGE_BYTES as u64,
                                2,
                            ),
                            edit: port::Edit::Bytes("d3-data-0000000100000001".into(), changed),
                        });
                    }
                    4 => inject(
                        &state,
                        "read",
                        "d3-data-0000000100000001",
                        8 * PAGE_BYTES as u64,
                        1,
                        port::Action::PrefixEof,
                    ),
                    _ => {}
                }
                if case == 6 {
                    let mut bad = target;
                    bad.request.kind = PageKind::RowLeaf;
                    let mut cursor = Plan4ResidencyCursor::new();
                    let error = rejected(bound.residency_step(&bad, &mut cursor));
                    frame_error(
                        error,
                        RetentionPhase::Load,
                        dirty_frame::Cause::Shape,
                        Effect::Unchanged,
                    );
                    assert_eq!(cursor.next, 1);
                } else {
                    let target = if case == 5 {
                        bound.target(Plan4Page::Root).unwrap()
                    } else {
                        target
                    };
                    let result = bound.load_into(1, &target);
                    if case == 1 {
                        let observed = result.unwrap();
                        assert_eq!(observed.after.page_lsn, PageLsn(0));
                        assert_eq!(observed.after.modification_counter, 0);
                        assert_eq!(observed.after.rec_lsn, None);
                        assert!(
                            observed.after.generation > observed.before.map_or(0, |v| v.generation)
                        );
                        assert_eq!(page_bytes(&bound, 8).1, data(&state, 8));
                    } else {
                        let error = rejected(result);
                        assert_eq!(error.phase, RetentionPhase::Load);
                        if case == 5 {
                            assert_eq!(error.effect, Effect::Unchanged);
                            assert!(matches!(error.cause, RetentionCause::Membership));
                        } else {
                            assert_eq!(error.effect, Effect::CallerMustResolve);
                            assert!(storage.session_fence().is_fenced());
                            if case == 3 {
                                assert!(matches!(error.cause, RetentionCause::Invariant));
                            } else {
                                assert!(matches!(error.cause, RetentionCause::PageIo(_)));
                            }
                        }
                    }
                }
                if matches!(case, 1 | 2 | 4) {
                    assert_eq!(fault_hits(&state), 1);
                }
                if case == 3 {
                    let hits = state.lock().unwrap().hook_hits;
                    assert_eq!(hits, 1);
                }
                if case != 1 {
                    assert_eq!(snapshots(&bound), before);
                }
                assert_eq!(effects(&state).1, 0);
            });
        });
    }
    h.finish();
}

#[test]
fn retention_live_writer_boundaries() {
    let mut h = Harness::new(&[
        "P4-LIVE-W01",
        "P4-LIVE-W02",
        "P4-LIVE-W03",
        "P4-LIVE-W04",
        "P4-LIVE-W05",
        "P4-LIVE-W06",
    ]);
    for case in 1..=6 {
        h.case(&format!("P4-LIVE-W{case:02}"), || {
            fixture!(|bound, owner, core, _foreign, state, storage| {
                if case <= 2 {
                    let status = init(&mut bound, &mut owner, &core);
                    prepare(&mut bound, &mut owner, &core, &status);
                    {
                        let (mut writer, _, _) = bound.split();
                        writer.aging_start(&mut owner).unwrap();
                    }
                    clear(&state);
                    let before = snapshots(&bound);
                    let error = {
                        let (mut writer, _, _) = bound.split();
                        if case == 1 {
                            rejected(writer.abort_end_empty(
                                &mut owner,
                                crate::storage::retention_page::tests::actor(99, 2),
                            ))
                        } else {
                            let mut wrong = status;
                            wrong.request.identity.owner = ObjectId::new(3).unwrap();
                            rejected(writer.begin_empty(&mut owner, &wrong, job(2)))
                        }
                    };
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(if case == 1 {
                        matches!(error.cause, RetentionCause::MissingTransaction)
                    } else {
                        matches!(error.cause, RetentionCause::Membership)
                    });
                    assert_eq!(snapshots(&bound), before);
                    assert_eq!(effects(&state).1, 0);
                    assert_eq!(
                        (
                            bound.control.ledger.issued_local,
                            bound.control.ledger.adopted_local
                        ),
                        if case == 1 { (0, 0) } else { (1, 0) }
                    );
                    if case == 2 {
                        let observation = {
                            let (mut writer, _, _) = bound.split();
                            writer.begin_empty(&mut owner, &status, job(2)).unwrap()
                        };
                        assert_eq!(
                            (observation.issued_local, observation.adopted_local),
                            (2, 2)
                        );
                        assert_eq!(observation.actor.unwrap().id().local, 2);
                    }
                } else {
                    let mut cursor = Plan4ResidencyCursor::new();
                    assert!(
                        bound
                            .prepare_status_slot(&owner, &mut cursor)
                            .unwrap()
                            .is_none()
                    );
                    clear(&state);
                    if case == 4 {
                        inject(
                            &state,
                            "write",
                            crate::storage::wal_io::WAL_NAME,
                            4096,
                            1,
                            port::Action::BeforeError,
                        );
                        let error = rejected(bound.reserve_status(&mut owner));
                        assert_eq!(error.phase, RetentionPhase::Durable);
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                    } else {
                        let reserved = bound.reserve_status(&mut owner).unwrap();
                        if case == 3 {
                            drop(reserved);
                        } else {
                            clear(&state);
                            if case == 5 {
                                inject(
                                    &state,
                                    "write",
                                    crate::storage::wal_io::WAL_NAME,
                                    4096 + 21_200,
                                    1,
                                    port::Action::PrefixError,
                                );
                            } else {
                                inject(
                                    &state,
                                    "sync",
                                    crate::storage::wal_io::WAL_NAME,
                                    0,
                                    1,
                                    port::Action::BeforeError,
                                );
                            }
                            match reserved.initialize() {
                                Err(RetentionReservationError::Fatal(error)) => {
                                    assert_eq!(error.phase, RetentionPhase::Durable);
                                    assert_eq!(error.effect, Effect::CallerMustResolve);
                                }
                                _ => panic!("expected retained INIT fatal"),
                            }
                        }
                    }
                    assert!(storage.session_fence().is_fenced());
                    if case >= 4 {
                        assert!(bound.control.ledger.pending.is_some());
                        assert_eq!(fault_hits(&state), 1);
                    }
                    assert_eq!(page_bytes(&bound, 0).0.total, 0);
                    assert!(data(&state, 9).iter().all(|b| *b == 0));
                }
            });
        });
    }
    h.finish();
}

#[test]
fn retention_live_activation() {
    let mut h = Harness::new(&["P4-LIVE-A01", "P4-LIVE-A02", "P4-LIVE-A03", "P4-LIVE-A04"]);
    for case in 1..=4 {
        h.case(&format!("P4-LIVE-A{case:02}"), || {
            let directory = port::Directory::fresh();
        directory.state().lock().unwrap().live_writes = true;
            let state = directory.state();
            let mut slot = boxed::<MAX_FRAME_BYTES>();
            let mut maintenance = boxed::<12_582_912>();
            let mut dwb = boxed::<4_194_304>();
            let fresh = FreshStorageSession::create_retention_catalog(directory.acquire_writer().unwrap(), 1, &mut slot, &mut maintenance).unwrap();
            let storage = fresh.install_doublewrite(&mut dwb).unwrap().into_mutable_storage();
            let core = PageConcurrency::new(storage.session_fence()).unwrap();
            let foreign = PageConcurrency::new(storage.session_fence()).unwrap();
            let mut owner = if case == 2 { foreign.try_owner(OwnerRole::Foreground).unwrap() } else { core.try_owner(OwnerRole::Foreground).unwrap() };
            let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
            let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
            let mut operation = boxed::<114_688>();
            let mut isolation = boxed::<4_160>();
            let mut control = boxed::<RETENTION_CONTROL_BYTES>();
            let mut replay = boxed::<RETENTION_REPLAY_BYTES>();
            let old = storage.bind_plan4_pages(&core, &mut frames, &mut operation).unwrap();
            if case == 3 { std::mem::forget(owner.try_apply().unwrap()); }
            clear(&state);
            let io = effects(&state);
            let result = old.into_empty_retention(&mut owner, EmptyRetentionControlLoan { isolation: &mut isolation, control: &mut control, replay: &mut replay });
            match (case, result) {
                (1 | 4, Ok(bound)) => {
                    assert!(bound.inner.pool.retention_payloads().unwrap());
                    assert!(std::ptr::eq(bound.inner.storage, &storage));
                    assert_eq!((bound.control.ledger.issued_local, bound.control.ledger.adopted_local), (0, 0));
                    assert!(bound.control.ledger.pending.is_none() && bound.control.ledger.durable.is_none());
                    assert!(bound.control.ledger.bytes[..65_536].iter().all(|b| *b == 0));
                    assert_eq!(effects(&state).1, 0);
                    if case == 1 {
                        let oracle = Oracle::new();
                        for ordinal in 0..64 {
                            let page = 2 + ordinal / 15;
                            // The fixed plan4 Job cell region ends at the page digest.
                            let first = if page == 6 { 12_256 } else { 992 };
                            let offset = first + (ordinal % 15) * 1024;
                            assert_eq!(bound.control.ledger.jobs()[ordinal].as_slice(), &oracle.genesis(page)[offset..offset + 1024]);
                        }
                    }
                    let before_drop = effects(&state);
                    drop(bound);
                    assert!(storage.session_fence().is_fenced());
                    assert_eq!(effects(&state), before_drop);
                }
                (2, Err(IntoRetentionError::Before { error, bound })) => {
                    assert_eq!(error.effect, Effect::Unchanged);
                    assert!(matches!(error.cause, RetentionCause::Plan4(super::super::super::Plan4PageError { cause: super::super::super::Plan4PageCause::Frame(e), .. }) if e.cause == dirty_frame::Cause::BindingMismatch));
                    assert_eq!(effects(&state), io);
                    assert!(!storage.session_fence().is_fenced());
                    assert!(!bound.pool.retention_payloads().unwrap());
                    drop(bound);
                }
                (3, Err(IntoRetentionError::Fatal(error))) => {
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    assert!(storage.session_fence().is_fenced());
                    assert_eq!(effects(&state), io);
                }
                (_, Ok(_)) => panic!("unexpected accepted activation"),
                (_, Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error))) => panic!("activation result {error:?}"),
            }
            drop(owner);
            drop(storage);
            let remaining = { let s = state.lock().unwrap(); (s.writers, s.handles) };
            assert_eq!(remaining, (0, 0));
        });
    }
    h.finish();
}

#[test]
fn retention_live_flush_boundaries() {
    use crate::storage::dirty_frame::test_support as flush_probe;
    use std::{sync::mpsc, time::Duration};
    let mut h = Harness::new(&[
        "P4-LIVE-F01",
        "P4-LIVE-F02",
        "P4-LIVE-F03",
        "P4-LIVE-F04",
        "P4-LIVE-F05",
        "P4-LIVE-F06",
    ]);
    for case in 1..=6 {
        h.case(&format!("P4-LIVE-F{case:02}"), || {
            fixture!(|bound, owner, core, _foreign, state, storage| {
                let status = init(&mut bound, &mut owner, &core);
                prepare(&mut bound, &mut owner, &core, &status);
                { let (mut writer, _, _) = bound.split(); writer.aging_start(&mut owner).unwrap(); }
                let root = bound.target(Plan4Page::Root).unwrap();
                clear(&state);
                if matches!(case, 1 | 3) {
                    let (arrive, arrived) = mpsc::sync_channel(1);
                    let (resume, release) = mpsc::sync_channel(1);
                    install_after_capture(&bound.inner.pool, arrive, release);
                    let root_index = (0..4).find(|&i| slots::slot(&bound.inner.pool, i).is_some_and(|s| s.0 == root.request)).unwrap();
                    let (mut writer, _, mut flusher) = bound.split();
                    let pool = writer.pool;
                    std::thread::scope(|scope| {
                        let core_ref = &core;
                        let flushing = scope.spawn(move || {
                            let mut dedicated = core_ref.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                            let outcome = flusher.try_flush(&mut dedicated, &root);
                            (flusher, outcome)
                        });
                        arrived.recv_timeout(Duration::from_secs(5)).unwrap();
                        if case == 1 {
                            let (install_arrive, waited) = mpsc::sync_channel(1);
                            let (finish_wait, install_release) = mpsc::sync_channel(1);
                            install_after_install(pool, install_arrive, install_release);
                            let writing = scope.spawn(move || {
                                let mut foreground = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                                let result = writer.begin_empty(&mut foreground, &status, job(2));
                                assert!(result.is_ok(), "concurrent Begin failed: {result:?}");
                                result
                            });
                            waited.recv_timeout(Duration::from_secs(5)).unwrap();
                            // L=3 is durable. L+1=4 is genuinely staged/adopted
                            // in RAM but its bytes have not reached the WAL File.
                            assert_eq!(storage.wal().prefix().unwrap().complete_lsn, 4);
                            assert_eq!(wal(&state).len(), 4096 + 21_200 + 38_000 + 19_672);
                            resume.send(flush_probe::Resume::Continue).unwrap();
                            let (_, result) = flushing.join().unwrap();
                            match result.unwrap() {
                                RetentionFlushStep::Complete { captured, finalized } => {
                                    assert_eq!(captured.page_lsn, PageLsn(3));
                                    assert!(!finalized.cleaned);
                                    assert_eq!(finalized.frame.page_lsn, PageLsn(4));
                                    assert_eq!(finalized.frame.rec_lsn, Some(PageLsn(3)));
                                }
                                other => panic!("fixed durable capture did not complete {other:?}"),
                            }
                            finish_wait.send(flush_probe::Resume::Continue).unwrap();
                            assert_eq!(writing.join().unwrap().unwrap().lsn, PageLsn(4));
                        } else {
                            let (held, held_ack) = mpsc::sync_channel(1);
                            let (unhold, hold_release) = mpsc::sync_channel(1);
                            let holding = scope.spawn(move || probe::hold_content(pool, root_index, held, hold_release));
                            held_ack.recv_timeout(Duration::from_secs(5)).unwrap();
                            resume.send(flush_probe::Resume::Continue).unwrap();
                            let (mut flusher, result) = flushing.join().unwrap();
                            assert_eq!(result.unwrap(), RetentionFlushStep::RetiredPendingFinalize);
                            assert_eq!(flusher.phase(), RetentionFlushPhase::RetiredPendingFinalize);
                            assert!(flush_probe::flush_diagnose(pool).claim.is_some());
                            unhold.send(()).unwrap();
                            holding.join().unwrap();
                            let before = effects(&state);
                            let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                            assert!(matches!(flusher.continue_flush(&mut dedicated).unwrap(), RetentionFlushStep::Complete { finalized, .. } if finalized.cleaned));
                            assert_eq!(effects(&state), before);
                            assert!(flush_probe::flush_diagnose(pool).claim.is_none());
                        }
                    });
                } else if case == 2 {
                    // A later complete/durable group changes Root, not Status.
                    // Its existence cannot extend or invalidate Status's P@2.
                    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    let (_, _, mut flusher) = bound.split();
                    match flusher.try_flush(&mut dedicated, &status).unwrap() {
                        RetentionFlushStep::Complete { captured, finalized } => {
                            assert_eq!(captured.page_lsn, PageLsn(2));
                            assert!(finalized.cleaned);
                        }
                        other => panic!("later complete rejected {other:?}"),
                    }
                    assert_eq!(storage.wal().prefix().unwrap().complete_lsn, 3);
                    assert_eq!(data(&state, 9), Oracle::new().page("status-free.page"));
                } else {
                    if case == 4 { inject(&state, "write", "d3-dwb-stage", 0, 1, port::Action::BeforeError); }
                    if case == 5 { inject(&state, "write", "d3-data-0000000100000001", PAGE_BYTES as u64, 1, port::Action::BeforeError); }
                    if case == 6 {
                        let mut changed = wal(&state);
                        changed[4096 + 128] ^= 1;
                        state.lock().unwrap().hook = Some(port::Hook {
                            // Header read, first hash, then second hash. Frame
                            // replay reads at4096+ and cannot match this key.
                            key: port::Key::new("read", crate::storage::wal_io::WAL_NAME, 0, 3),
                            edit: port::Edit::Bytes(crate::storage::wal_io::WAL_NAME.into(), changed),
                        });
                    }
                    let home_before = data(&state, 1);
                    let image_before = page_bytes(&bound, 1).1;
                    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    let error = {
                        let (_, _, mut flusher) = bound.split();
                        rejected(flusher.try_flush(&mut dedicated, &root))
                    };
                    assert_eq!(error.effect, Effect::CallerMustResolve);
                    assert_eq!(error.phase, if case == 6 { RetentionPhase::Source } else { RetentionPhase::Flush });
                    if case == 6 {
                        assert!(matches!(error.cause, RetentionCause::Invariant));
                        let hits = state.lock().unwrap().hook_hits;
                        assert_eq!(hits, 1);
                        assert_eq!(effects(&state).1, 0);
                    } else {
                        assert!(matches!(error.cause, RetentionCause::Flush(_)));
                        assert_eq!(fault_hits(&state), 1);
                    }
                    assert!(storage.session_fence().is_fenced());
                    assert!(flush_probe::flush_diagnose(&bound.inner.pool).claim.is_some());
                    assert_eq!(data(&state, 1), home_before);
                    let (metadata, image) = page_bytes(&bound, 1);
                    assert!(metadata.dirty);
                    assert_eq!(image, image_before);
                    assert_eq!(metadata.total, 0);
                }
            });
        });
    }
    h.finish();
}

#[path = "s5_tests.rs"]
mod s5;
