//! New recovery setup/consumer drivers. These invoke actual owner methods;
//! completed test functions and synthetic authority constructors are absent.
use super::super::super::plan4_pages::{
    EmptyRetentionControlLoan, IntoRetentionError, Plan4CatalogLookup, Plan4Page,
    Plan4ResidencyCursor, Plan4Retention, Plan4Target, RETENTION_CONTROL_BYTES,
    RETENTION_REPLAY_BYTES, RetentionFlushPhase, RetentionFlushStep, RetentionReservationError,
    RetentionResidencyStep,
};
use super::literals::Oracle;
use super::*;
use crate::{
    storage::{
        dirty_frame::DirtyFrame,
        page::{FileId, PageRef},
        retention_page::JobSlotRef,
        transaction_record::TxnRef,
        wal_io::FreshStorageSession,
    },
    transaction::{
        TxnId,
        concurrency::{Owner, OwnerRole, PageConcurrency},
    },
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use xadb_foundation::identity::{Generation, ObjectId};

pub(super) type State = Arc<Mutex<port::State>>;
pub(super) type Images = BTreeMap<String, Vec<u8>>;
pub(super) type Live<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W> =
    Plan4Retention<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
pub(super) fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().ok().unwrap()
}
pub(super) fn namespace() -> Namespace {
    Namespace {
        database: crate::api::DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
pub(super) fn page_ref(number: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x100000001).unwrap(),
        page_no: number,
        incarnation: Generation::new(incarnation).unwrap(),
    }
}
pub(super) fn job(generation: u64) -> JobSlotRef {
    JobSlotRef::new(page_ref(2, 1), 0, Generation::new(generation).unwrap()).unwrap()
}
pub(super) fn actor(epoch: u32, generation: u64) -> TxnRef {
    TxnRef::new(
        TxnId {
            database: namespace().database,
            timeline: 1,
            writer_epoch: epoch,
            local: 1,
        },
        Generation::new(generation).unwrap(),
    )
    .unwrap()
}
pub(super) fn file(state: &State, name: &str) -> Vec<u8> {
    state.lock().unwrap().files[name].bytes.clone()
}
pub(super) fn images(state: &State) -> Images {
    state.lock().unwrap().images()
}
pub(super) fn clear(state: &State) {
    let mut s = state.lock().unwrap();
    s.events.clear();
    s.fault_hits = 0;
    s.hook_hits = 0;
    s.fault = None;
    s.hook = None;
}
pub(super) fn clone_directory(source: &Images) -> port::Directory {
    let d = port::Directory::from_files(source.clone());
    {
        let state = d.state();
        let mut state = state.lock().unwrap();
        state.live_writes = true;
        state.recovery_writes = true;
    }
    d
}
pub(super) fn assert_closed(state: &State) {
    let observed = {
        let s = state.lock().unwrap();
        (s.writers, s.handles)
    };
    assert_eq!(observed, (0, 0));
}
pub(super) fn drive_flush<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    flusher: &mut Owner<'c, 's>,
    target: Option<&Plan4Target>,
) {
    let (_, _, mut f) = live.split();
    let mut step = match target {
        Some(target) => f.try_flush(flusher, target),
        None => f.try_flush_allocator(flusher),
    }
    .unwrap();
    for _ in 0..8 {
        if matches!(
            step,
            RetentionFlushStep::Complete { .. } | RetentionFlushStep::NoDirty
        ) {
            assert_eq!(f.phase(), RetentionFlushPhase::Ready);
            return;
        }
        step = f.continue_flush(flusher).unwrap();
    }
    panic!("bounded original flush continuation did not finish");
}
pub(super) fn prepare<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    status: &Plan4Target,
    generation: u64,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..8 {
        match live
            .prepare_empty_targets(owner, status, job(generation), &mut cursor)
            .unwrap()
        {
            None => return,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected prepare observation"),
        }
    }
    panic!("finite four-target preparation exhausted");
}
pub(super) fn catalog<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..8 {
        match live.prepare_catalog(owner, &mut cursor).unwrap() {
            None => {
                let (_, reader, _) = live.split();
                assert!(matches!(
                    reader
                        .lookup_catalog_empty(owner, ObjectId::new(6).unwrap())
                        .unwrap(),
                    Plan4CatalogLookup::NotFound
                ));
                return;
            }
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected catalog preparation"),
        }
    }
    panic!("finite three-target catalog preparation exhausted");
}
pub(super) fn assert_resident<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    target: &Plan4Target,
    expected: &[u8],
    backing: &[usize],
) {
    let (_, reader, _) = live.split();
    let read = reader.read(owner, target).unwrap();
    let bytes = read.bytes().unwrap();
    assert_eq!(bytes.as_slice(), expected);
    assert!(
        backing.contains(&(bytes.as_ptr() as usize)),
        "handoff replaced caller pool backing"
    );
    read.release_checked().unwrap();
    owner.check_idle().unwrap();
}
pub(super) fn fresh<D: DirectoryPort>(directory: &D, stop: u64, read: impl Fn(&str) -> Vec<u8>) {
    assert!([0, 1, 4, 5, 6, 7].contains(&stop));
    let oracle = Oracle::new();
    let mut slot = boxed::<1_048_576>();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let storage = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        1,
        &mut slot,
        &mut maintenance,
    )
    .unwrap()
    .install_doublewrite(&mut dwb)
    .unwrap()
    .into_mutable_storage();
    if stop == 0 {
        return;
    }
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let mut control = boxed::<RETENTION_CONTROL_BYTES>();
    let mut replay = boxed::<RETENTION_REPLAY_BYTES>();
    let pages = storage
        .bind_plan4_pages(&core, &mut frames, &mut operation)
        .unwrap();
    let mut live = match pages.into_empty_retention(
        &mut owner,
        EmptyRetentionControlLoan {
            control: &mut control,
            replay: &mut replay,
        },
    ) {
        Ok(value) => value,
        Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => {
            panic!("actual fresh activation: {error:?}")
        }
    };
    let mut cursor = Plan4ResidencyCursor::new();
    assert!(
        live.prepare_status_slot(&owner, &mut cursor)
            .unwrap()
            .is_none()
    );
    let reserved = live.reserve_status(&mut owner).unwrap();
    assert_eq!(reserved.page(), page_ref(9, 2));
    let reserve = oracle.allocation(0);
    assert_eq!(&read(wal::WAL_NAME)[4096..], reserve.0);
    if stop == 1 {
        drop(reserved);
        return;
    }
    let status = match reserved.initialize() {
        Ok(value) => value,
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("actual INIT: {error:?}"),
    };
    let initialized = oracle.allocation(2);
    assert_eq!(&read(wal::WAL_NAME)[25_296..], initialized.0);
    prepare(&mut live, &owner, &mut flusher, &status, 2);
    let mut offset = 63_296;
    for (i, name) in [
        "aging-start.wal",
        "begin.wal",
        "rollback.wal",
        "abort.wal",
        "finish.wal",
    ]
    .into_iter()
    .enumerate()
    {
        let observation = {
            let (mut writer, _, _) = live.split();
            match i {
                0 => writer.aging_start(&mut owner),
                1 => writer.begin_empty(&mut owner, &status, job(2)),
                2 => writer.rollback_start(&mut owner, actor(1, 2)),
                3 => writer.abort_end_empty(&mut owner, actor(1, 2)),
                4 => writer.finish_empty_terminal(&mut owner, job(2)),
                _ => unreachable!(),
            }
            .unwrap()
        };
        assert_eq!(observation.lsn.0, i as u64 + 3);
        let raw = oracle.raw(name);
        let actual = read(wal::WAL_NAME);
        assert_eq!(&actual[offset..], raw);
        offset += raw.len();
        assert_eq!(actual.len(), offset);
        if stop == i as u64 + 3 {
            break;
        }
    }
    if stop == 7 {
        drive_flush(&mut live, &mut flusher, None);
        for target in [
            live.target(Plan4Page::Root).unwrap(),
            live.target(Plan4Page::Jobs { index: 0 }).unwrap(),
            status,
        ] {
            drive_flush(&mut live, &mut flusher, Some(&target));
        }
    }
}
pub(super) fn fresh_memory(stop: u64) -> Images {
    let d = port::Directory::fresh();
    d.state().lock().unwrap().live_writes = true;
    let state = d.state();
    fresh(&d, stop, |name| file(&state, name));
    assert_closed(&state);
    images(&state)
}
pub(super) fn actual_runtime<'a, 'm, 'w, D: DirectoryPort>(
    d: &D,
    maintenance: &'a mut [u8; 12_582_912],
    dwb: &'w mut [u8; 4_194_304],
    slot: &'m mut [u8; 1_048_576],
) -> Plan4RecoveryRuntime<'a, 'm, 'w, D::Writer> {
    Plan4Recovery::acquire(d, Some(namespace()), maintenance, dwb, slot)
        .unwrap()
        .recover()
        .unwrap()
}
pub(super) fn finish_closed<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    closed: &mut Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) {
    for _ in 0..64 {
        if closed.step(owner, flusher).unwrap().ready {
            return;
        }
    }
    panic!("bounded closed recovery did not reach verified handoff");
}

macro_rules! recovered {
    ($directory:expr, |$runtime:ident,$closed:ident,$owner:ident,$flusher:ident,$core:ident,$backing:ident| $body:block) => {{
        recovered!($directory, before_bind {}, |$runtime,$closed,$owner,$flusher,$core,$backing| $body)
    }};
    ($directory:expr, before_bind $before:block, |$runtime:ident,$closed:ident,$owner:ident,$flusher:ident,$core:ident,$backing:ident| $body:block) => {{
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let $runtime = actual_runtime($directory, &mut maintenance, &mut dwb, &mut slot);
        $before
        let $core = PageConcurrency::new($runtime.session_fence()).unwrap();
        let mut $owner = $core.try_owner(OwnerRole::Foreground).unwrap();
        let mut $flusher = $core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut backing_pages = vec![[0xa7; PAGE_BYTES]; 4];
        let $backing: Vec<_> = backing_pages.iter().map(|p| p.as_ptr() as usize).collect();
        let mut frames: Vec<_> = backing_pages.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let mut $closed = $runtime
            .bind(&$owner, &$core, &mut frames, &mut operation)
            .unwrap();
        $body
    }};
}
pub(super) use recovered;
