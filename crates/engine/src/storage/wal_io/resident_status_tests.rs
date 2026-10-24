//! Fresh multi-residency histories. Expected wire images are independent inputs.
use super::super::super::OPERATION_BYTES;
const HOME: &str = "d3-data-0000000100000001";
fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn initialized<W: WriterNamespace>(
    reserved: super::super::super::ReservedStatus<'_, '_, '_, '_, '_, '_, '_, '_, '_, W>,
) -> super::super::super::InitializedStatusObservation {
    match reserved.initialize() {
        Ok(value) => value,
        Err(super::super::super::InitAttemptError::Before(e)) => {
            panic!("INIT before {:?}", e.error)
        }
        Err(super::super::super::InitAttemptError::Fatal(e)) => panic!("INIT fatal {e:?}"),
    }
}
use super::*;
use crate::storage::{
    dirty_frame::{
        self, DirtyFrame, SharedPool, init_test_support as probe,
        status_load_test_support as residency,
    },
    wal_io::{self as wal, FreshStorageSession, bootstrap::test_support as port},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use std::{
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::mpsc,
    time::Duration,
};

fn literal(pages: usize, name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../tests/fixtures/dev3-resident-status/p1-n{pages}/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
macro_rules! fixture {
    (|$bound:ident, $core:ident, $state:ident, $storage:ident| $body:block) => {{
        let mut wal_slot = port::slot();
        let mut workspace = boxed::<{4 << 20}>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(namespace, 1, &mut wal_slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        let mut a = boxed::<PAGE_BYTES>(0xa1);
        let mut b = boxed::<PAGE_BYTES>(0xb2);
        let mut c = boxed::<PAGE_BYTES>(0xc3);
        let mut operation = boxed::<OPERATION_BYTES>(0x73);
        let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut b), DirtyFrame::new(&mut c)];
        let binding = $storage.bind_resident_status(&$core, &mut frames, &mut operation).unwrap();
        let mut $bound = match binding.activate() {
            Ok(bound) => bound,
            Err(IntoFlushError::Before { error, .. }) | Err(IntoFlushError::Fatal(error)) => panic!("activate {error:?}"),
        };
        $body
        drop($bound);
        assert!($storage.session_fence().is_fenced());
        drop($storage);
        let state = $state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
    }};
}

macro_rules! two_dirty {
    ($bound:ident, $owner:ident) => {{
        let first = {
            let (mut writer, _, _) = $bound.split_with_flush();
            initialized(writer.reserve_one(&mut $owner).unwrap())
        };
        assert!($bound.prepare_slot(2).unwrap().is_none());
        let second = {
            let (mut writer, _, _) = $bound.split_with_flush();
            initialized(writer.reserve_one(&mut $owner).unwrap())
        };
        (first, second)
    }};
}
type SlotSnapshot = (probe::RawSnapshot, Vec<u8>, Option<(TargetRequest, u64)>);
fn snapshot(pool: &SharedPool<'_, '_, '_, '_>) -> Vec<SlotSnapshot> {
    (0..3)
        .map(|index| {
            let mut bytes = boxed::<PAGE_BYTES>(0);
            let mut metadata = probe::snapshot(pool, index, &mut bytes);
            // RawSnapshot's old diagnostic publication describes the selected slot.
            // Use the actual directory entry separately for per-slot comparison.
            metadata.published = None;
            (metadata, bytes.to_vec(), residency::slot(pool, index))
        })
        .collect()
}

#[test]
fn resident_status_resource_gate() {
    type W = port::Writer;
    println!(
        "activation={} wide={} metadata={:?}",
        activation_bytes::<W>(),
        wide_call_bytes(),
        metadata_peaks::<W>()
    );
    println!(
        "wide_result={} admission_result={} binding={} resident={} snapshot={}",
        size_of::<Result<ReadStatusSet<'_, '_, '_, '_, '_, '_, 64>>>(),
        size_of::<Result<ResidentReadAdmission<'_, '_, '_, '_, '_, '_>>>(),
        size_of::<ResidentStatusBinding<'_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<ResidentStatus<'_, '_, '_, '_, '_, '_, '_, W>>(),
        size_of::<dirty_frame::StatusLoadSnapshot<'_, '_>>()
    );
    for (name, value) in probe::init_layout() {
        println!("{name}={value}");
    }
    assert!(fits::<W>());
    assert!(fits::<xadb_platform::namespace::LinuxWriterLease>());
}

#[test]
fn resident_status_two_dirty_pages_use_distinct_slots_and_lsns() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        assert_eq!(
            (
                a.page.page_no,
                b.page.page_no,
                a.lsn.0,
                b.lsn.0,
                a.status.generation,
                b.status.generation
            ),
            (1, 2, 2, 4, 1, 1)
        );
        let old = snapshot(&bound.inner.inner.pool);
        assert!(old.iter().all(|v| v.0.dirty));
        assert_eq!(
            (
                old[0].0.rec_lsn.unwrap().0,
                old[1].0.rec_lsn.unwrap().0,
                old[2].0.rec_lsn.unwrap().0
            ),
            (1, 2, 4)
        );
        assert_eq!(old[0].1, literal(2, "allocator.after"));
        assert_eq!(old[1].1, literal(2, "status-1.after"));
        assert_eq!(old[2].1, literal(2, "status-2.after"));
        let (_, reader, _) = bound.split_with_flush();
        let read = reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<3>(&[b.page, a.page])
            .unwrap();
        for (position, lsn) in [(0, 4), (1, 2), (2, 4)] {
            assert_eq!(read.stamp(position).unwrap().page_lsn.0, lsn);
            assert_eq!(read.bytes(position).unwrap().as_slice(), old[position].1);
        }
        assert_eq!(read.request(1).unwrap().identity.page, a.page);
        assert_eq!(read.request(2).unwrap().identity.page, b.page);
        read.release_checked().unwrap();
        assert_eq!(snapshot(&bound.inner.inner.pool), old);
        assert_eq!(
            &state.lock().unwrap().files[wal::WAL_NAME].bytes[4096..],
            literal(2, "wal.frames-only")
        );
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn resident_status_four_addresses_three_frames_reload_nonselected() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        {
            let (_, _, mut flush) = bound.split_with_flush();
            flush.try_flush_page(&mut dedicated, a.page).unwrap();
        }
        let before = snapshot(&bound.inner.inner.pool);
        assert!(!before[1].0.dirty && before[0].0.dirty && before[2].0.dirty);
        assert_eq!(
            bound.prepare_slot(1).unwrap().unwrap().identity,
            a.status.identity
        );
        let c = {
            let (mut writer, _, _) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap())
        };
        assert_eq!((c.page.page_no, c.lsn.0, c.status.generation), (3, 6, 2));
        let both = snapshot(&bound.inner.inner.pool);
        assert_eq!(both[2], before[2]);
        {
            let (_, _, mut flush) = bound.split_with_flush();
            flush.try_flush_page(&mut dedicated, b.page).unwrap();
        }
        // Load A into slot2 while selected INIT destination remains slot1 (C).
        let loaded = bound.load_into(2, a.page).unwrap();
        assert_eq!(
            (
                loaded.before.unwrap().identity,
                loaded.after.identity,
                loaded.after.generation
            ),
            (b.status.identity, a.status.identity, 2)
        );
        assert_eq!(bound.inner.inner.pool.init_payload_index().unwrap(), 1);
        let after = snapshot(&bound.inner.inner.pool);
        assert_eq!(after[0], both[0]);
        assert_eq!(after[1], both[1]);
        assert_eq!(after[2].1, literal(3, "status-1.after"));
        {
            let (_, reader, mut flush) = bound.split_with_flush();
            let read = reader
                .try_admit(&mut owner)
                .unwrap()
                .read_set::<3>(&[c.page, a.page])
                .unwrap();
            assert_eq!(read.request(1).unwrap().identity.page, a.page);
            assert_eq!(read.request(2).unwrap().identity.page, c.page);
            read.release_checked().unwrap();
            flush.try_flush_page(&mut dedicated, c.page).unwrap();
            flush.try_flush_allocator(&mut dedicated).unwrap();
        }
        assert_eq!(bound.load_into(1, b.page).unwrap().after.generation, 3);
        assert_eq!(
            bound.inner.inner.control.base.descriptor.as_slice(),
            literal(3, "terminal.descriptor")
        );
        let s = state.lock().unwrap();
        assert_eq!(
            &s.files[wal::WAL_NAME].bytes[4096..],
            literal(3, "wal.frames-only")
        );
        let home = &s.files[HOME].bytes;
        assert_eq!(&home[..PAGE_BYTES], literal(3, "allocator.after"));
        for page in 1..=3 {
            assert_eq!(
                &home[page * PAGE_BYTES..(page + 1) * PAGE_BYTES],
                literal(3, &format!("status-{page}.after"))
            );
        }
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn resident_status_prepare_refusals_preserve_all_frames() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let before = snapshot(&bound.inner.inner.pool);
        for index in [0, 1, 3, usize::MAX] {
            assert_eq!(
                bound.prepare_slot(index).unwrap_err().effect,
                Effect::Unchanged
            );
        }
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        let _ = two_dirty!(bound, owner);
        let before = snapshot(&bound.inner.inner.pool);
        let events = state.lock().unwrap().events.len();
        for index in [1, 2] {
            assert_eq!(
                bound.prepare_slot(index).unwrap_err().effect,
                Effect::Unchanged
            );
        }
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn resident_status_forgotten_reservation_blocks_prepare_and_load() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let page = reserved.page();
        std::mem::forget(reserved);
        let before = snapshot(&bound.inner.inner.pool);
        let reads = state.lock().unwrap().reads;
        assert_eq!(before[0].0.total, 0);
        assert!(!before[0].0.claim_present);
        assert_eq!(bound.prepare_slot(2).unwrap_err().effect, Effect::Unchanged);
        assert!(matches!(
            bound.load_into(2, page).unwrap_err().cause,
            super::super::operational::load::Cause::Busy
        ));
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().reads, reads);
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn resident_status_read_shape_alias_missing_and_admission_cleanup() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let before = snapshot(&bound.inner.inner.pool);
        let (_, reader, _) = bound.split_with_flush();
        let token = reader.try_admit(&mut owner).unwrap();
        assert!(reader.try_admit(&mut other).is_err());
        drop(token);
        assert!(
            reader
                .try_admit(&mut owner)
                .unwrap()
                .read_set::<3>(&[a.page])
                .is_err()
        );
        assert!(
            reader
                .try_admit(&mut owner)
                .unwrap()
                .read_set::<3>(&[a.page, a.page])
                .is_err()
        );
        let mut missing = b.page;
        missing.page_no = 3;
        assert!(
            reader
                .try_admit(&mut owner)
                .unwrap()
                .read_set::<3>(&[a.page, missing])
                .is_err()
        );
        let read = reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<3>(&[a.page, b.page])
            .unwrap();
        assert!(reader.try_admit(&mut other).is_err());
        assert!(read.bytes(3).is_err());
        assert!(read.stamp(3).is_err());
        read.release_checked().unwrap();
        reader
            .try_admit(&mut other)
            .unwrap()
            .read_set::<2>(&[a.page])
            .unwrap()
            .release_checked()
            .unwrap();
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert!(!storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().events.is_empty());
    });
}

#[test]
fn resident_status_duplicate_load_is_rejected_before_io() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        {
            let (_, _, mut f) = bound.split_with_flush();
            f.try_flush_page(&mut dedicated, a.page).unwrap();
            f.try_flush_page(&mut dedicated, b.page).unwrap();
        }
        let before = snapshot(&bound.inner.inner.pool);
        let reads = state.lock().unwrap().reads;
        for (index, page) in [(1, b.page), (2, a.page), (1, a.page), (2, b.page)] {
            let err = bound.load_into(index, page).unwrap_err();
            assert!(matches!(
                err.cause,
                super::super::operational::load::Cause::AlreadyResident
            ));
            assert_eq!(err.effect, Effect::Unchanged);
        }
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().reads, reads);
        assert!(!storage.session_fence().is_fenced());
    });
}

fn load_failure(at: usize) {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        {
            let (_, _, mut f) = bound.split_with_flush();
            f.try_flush_page(&mut dedicated, a.page).unwrap();
            f.try_flush_page(&mut dedicated, b.page).unwrap();
        }
        bound.prepare_slot(1).unwrap();
        let c = {
            let (mut writer, _, _) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap())
        };
        let before = snapshot(&bound.inner.inner.pool);
        let selected = bound.inner.inner.pool.init_payload_index().unwrap();
        {
            let mut s = state.lock().unwrap();
            s.read_plan = Some(port::Plan {
                at: s.reads + at,
                action: port::Action::BeforeError,
            });
        }
        assert_eq!(
            bound.load_into(2, a.page).unwrap_err().effect,
            Effect::Unchanged
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(
            bound.inner.inner.pool.init_payload_index().unwrap(),
            selected
        );
        {
            let mut s = state.lock().unwrap();
            assert_eq!(s.configured_read_hits, 1);
            s.read_plan = None;
        }
        assert_eq!(bound.load_into(2, a.page).unwrap().after.generation, 2);
        assert_eq!(
            residency::slot(&bound.inner.inner.pool, 1)
                .unwrap()
                .0
                .identity
                .page,
            c.page
        );
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn resident_status_load_failure_first_preserves_destination() {
    load_failure(0)
}
#[test]
fn resident_status_load_failure_last_preserves_destination() {
    load_failure(9)
}

#[test]
fn resident_status_forged_index_is_not_same_generation_authority() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        bound.prepare_slot(2).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let mut authority = reserved
            .writer
            .authority(
                reserved.batch,
                reserved.page,
                reserved.ordinal,
                reserved.kind,
            )
            .unwrap();
        assert_eq!((authority.payload_index, authority.generations[1]), (2, 1));
        authority.payload_index = 1;
        let before = snapshot(reserved.writer.pool);
        let events = state.lock().unwrap().events.len();
        assert!(matches!(
            reserved
                .writer
                .pool
                .claim_init_targets(reserved.owner, &authority),
            Err(dirty_frame::DirtyError {
                cause: dirty_frame::Cause::Stale,
                effect: Effect::Unchanged
            })
        ));
        assert_eq!(snapshot(reserved.writer.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert_eq!(initialized(reserved).status.generation, 1);
        assert!(residency::slot(&bound.inner.inner.pool, 1).is_none());
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn resident_status_wide_read_unwind_releases_pins_before_gate() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (_, reader, _) = bound.split_with_flush();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _guard = reader
                    .try_admit(&mut owner)
                    .unwrap()
                    .read_set::<3>(&[a.page, b.page])
                    .unwrap();
                panic!("resident read unwind");
            }))
            .is_err()
        );
        assert!(storage.session_fence().is_fenced());
        assert_eq!(probe::pin_state(&bound.inner.inner.pool).1, 0);
        assert!(!matches!(
            bound.read_gate.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        assert!(!state.lock().unwrap().events.is_empty());
    });
}

#[test]
fn resident_status_actual_thread_reader_and_flush_finalize() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (_, reader, mut flush) = bound.split_with_flush();
        let (tx, rx) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let reader_ref = &reader;
            let core_ref = &core;
            let worker = scope.spawn(move || {
                let mut owner = core_ref.try_owner(OwnerRole::Foreground).unwrap();
                let held = reader_ref
                    .try_admit(&mut owner)
                    .unwrap()
                    .read_set::<3>(&[b.page, a.page])
                    .unwrap();
                tx.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(5)).unwrap();
                held.release_checked().unwrap();
            });
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(reader.try_admit(&mut owner).is_err());
            let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            assert!(matches!(
                flush.try_flush_page(&mut dedicated, a.page).unwrap(),
                super::super::FlushProgress::RetiredPendingFinalize
            ));
            resume.send(()).unwrap();
            worker.join().unwrap();
            assert!(matches!(
                flush.continue_flush(&mut dedicated).unwrap(),
                super::super::FlushProgress::Complete(_)
            ));
            reader
                .try_admit(&mut owner)
                .unwrap()
                .read_set::<3>(&[a.page, b.page])
                .unwrap()
                .release_checked()
                .unwrap();
        });
        assert!(!storage.session_fence().is_fenced());
        assert!(!snapshot(&bound.inner.inner.pool)[1].0.dirty);
        assert!(snapshot(&bound.inner.inner.pool)[2].0.dirty);
        assert!(!state.lock().unwrap().events.is_empty());
    });
}

macro_rules! storage_fixture {
    (|$core:ident, $state:ident, $storage:ident| $body:block) => {{
        let mut slot=port::slot();let mut workspace=boxed::<{4<<20}>(0);
        let (namespace,$state)=port::fresh();
        let fresh=FreshStorageSession::create_status(namespace,1,&mut slot,&mut [[0;PAGE_BYTES];3]).unwrap();
        let installed=fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage=installed.into_mutable_storage();let $core=PageConcurrency::new($storage.session_fence()).unwrap();
        $body
        drop($storage);let state=$state.lock().unwrap();assert_eq!((state.handles,state.writers),(0,0));
    }};
}
#[test]
fn resident_status_binding_shape_claim_and_activation_retry() {
    storage_fixture!(|core, state, storage| {
        let mut bytes: Vec<_> = (0..65).map(|_| boxed::<PAGE_BYTES>(0x73)).collect();
        let mut frames: Vec<_> = bytes.iter_mut().map(|page| DirtyFrame::new(page)).collect();
        let mut operation = boxed::<OPERATION_BYTES>(0);
        let reads = state.lock().unwrap().reads;
        for count in [0, 1, 65] {
            assert!(matches!(
                storage.bind_resident_status(&core, &mut frames[..count], &mut operation),
                Err(wal::MutableStorageError {
                    effect: Effect::Unchanged,
                    ..
                })
            ));
        }
        assert_eq!(state.lock().unwrap().reads, reads);
        let binding = storage
            .bind_resident_status(&core, &mut frames[..3], &mut operation)
            .unwrap();
        let mut second_bytes = [boxed::<PAGE_BYTES>(0), boxed::<PAGE_BYTES>(0)];
        let mut second: Vec<_> = second_bytes
            .iter_mut()
            .map(|page| DirtyFrame::new(page))
            .collect();
        let mut second_operation = boxed::<OPERATION_BYTES>(0);
        let reads = state.lock().unwrap().reads;
        assert!(matches!(
            storage.bind_resident_status(&core, &mut second, &mut second_operation),
            Err(wal::MutableStorageError {
                cause: super::super::super::super::Cause::AlreadyBound,
                effect: Effect::Unchanged
            })
        ));
        assert_eq!(state.lock().unwrap().reads, reads);
        let held = storage.workspace.lock().unwrap();
        let binding = match binding.activate() {
            Err(IntoFlushError::Before { error, bound }) => {
                assert_eq!(error.effect, Effect::Unchanged);
                bound
            }
            _ => panic!("workspace rejection must return binding"),
        };
        assert!(!storage.session_fence().is_fenced());
        drop(held);
        let mut bound = match binding.activate() {
            Ok(bound) => bound,
            _ => panic!("same binding retry"),
        };
        assert_eq!(state.lock().unwrap().reads, reads);
        bound.prepare_slot(2).unwrap();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        initialized(writer.reserve_one(&mut owner).unwrap());
        drop(bound);
    });
}
#[test]
fn resident_status_binding_rejects_nonfresh_payload_before_io() {
    storage_fixture!(|core, state, storage| {
        let mut bytes = [
            boxed::<PAGE_BYTES>(0),
            boxed::<PAGE_BYTES>(0),
            boxed::<PAGE_BYTES>(0),
        ];
        let mut frames: Vec<_> = bytes.iter_mut().map(|page| DirtyFrame::new(page)).collect();
        let request = TargetRequest {
            identity: crate::storage::page_io::test_support::expected(),
            kind: crate::storage::page::PageKind::TransactionStatus,
            level: 0,
        };
        probe::seed_original_registration(&mut frames[2], request, 1);
        let mut operation = boxed::<OPERATION_BYTES>(0);
        let reads = state.lock().unwrap().reads;
        assert!(
            storage
                .bind_resident_status(&core, &mut frames, &mut operation)
                .is_err()
        );
        assert!(storage.session_fence().is_fenced());
        assert_eq!(state.lock().unwrap().reads, reads);
    });
}
#[test]
fn resident_status_generation_exhaustion_preserves_other_dirty_pages() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        {
            let (mut writer, _, _) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
        }
        residency::seed_generation(&mut bound.inner.inner.pool, 2, u64::MAX);
        let before = snapshot(&bound.inner.inner.pool);
        let events = state.lock().unwrap().events.len();
        let e = bound.prepare_slot(2).unwrap_err();
        assert_eq!(e.effect, Effect::Unchanged);
        assert!(matches!(
            e.cause,
            super::super::Cause::Frame(dirty_frame::DirtyError {
                cause: dirty_frame::Cause::CounterExhausted,
                ..
            })
        ));
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn resident_status_late_busy_read_returns_every_pin_and_gate() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (_, reader, _) = bound.split_with_flush();
        let pool = reader.inner.pool;
        residency::hold_write(pool, 2, || {
            assert!(
                reader
                    .try_admit(&mut owner)
                    .unwrap()
                    .read_set::<3>(&[b.page, a.page])
                    .is_err()
            );
            assert_eq!(probe::pin_state(pool).1, 0);
        });
        reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<3>(&[a.page, b.page])
            .unwrap()
            .release_checked()
            .unwrap();
        assert!(!storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().events.is_empty());
    });
}
#[test]
fn resident_status_forged_local_index_cannot_install_or_release_wrong_slot() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        bound.prepare_slot(2).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let authority = reserved
            .writer
            .authority(
                reserved.batch,
                reserved.page,
                reserved.ordinal,
                reserved.kind,
            )
            .unwrap();
        let pool = reserved.writer.pool;
        let mut write = pool
            .claim_init_targets(reserved.owner, &authority)
            .unwrap()
            .try_write()
            .unwrap();
        let mut before = boxed::<PAGE_BYTES>(0);
        let stamp = write.copy_allocator_snapshot(&mut before).unwrap();
        let pins = probe::pin_state(pool);
        assert_eq!((pins.0[0], pins.0[1], pins.0[2]), (1, 0, 1));
        probe::corrupt_local_init_index(&mut write, 1);
        let events = state.lock().unwrap().events.len();
        assert_eq!(
            write.preflight(&stamp).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert!(write.release_checked().is_err());
        assert_eq!(probe::pin_state(pool), pins);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(state.lock().unwrap().events.len(), events);
        drop(reserved);
    });
}

fn init_cut(point: probe::InitPoint) {
    use crate::storage::dirty_frame::test_support as ft;
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        {
            let (mut writer, _, _) = bound.split_with_flush();
            initialized(writer.reserve_one(&mut owner).unwrap());
        }
        let first = snapshot(&bound.inner.inner.pool)
            .into_iter()
            .nth(1)
            .unwrap();
        bound.prepare_slot(2).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let pool = reserved.writer.pool;
        let fence = storage.session_fence();
        let core_ref = &core;
        let (arrived, arrival) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        let (fenced, fence_arrival) = mpsc::sync_channel(1);
        let (unwind, unwind_release) = mpsc::sync_channel(1);
        probe::install_init_pause(pool, point, arrived, release);
        ft::install_fence_pause(pool, fenced, unwind_release);
        std::thread::scope(|scope| {
            let controller = scope.spawn(move || {
                arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(!fence.is_fenced());
                assert!(ft::content_busy(pool, 0) && ft::content_busy(pool, 2));
                assert!(!ft::content_busy(pool, 1));
                assert!(core_ref.diagnostic_version_busy());
                let pins = probe::pin_state(pool);
                assert_eq!((pins.0[0], pins.0[1], pins.0[2], pins.1), (1, 0, 1, 2));
                resume.send(ft::Resume::Panic).unwrap();
                fence_arrival.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(fence.is_fenced());
                assert!(ft::content_busy(pool, 0) && ft::content_busy(pool, 2));
                unwind.send(ft::Resume::Continue).unwrap();
            });
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = reserved.initialize();
                }))
                .is_err()
            );
            controller.join().unwrap();
        });
        let after = snapshot(pool);
        assert_eq!(after[1].1, first.1);
        assert_eq!(after[1].2, first.2);
        assert_eq!(after[1].0.entry, first.0.entry);
        assert_eq!(after[1].0.requirement, first.0.requirement);
        assert!(fence.is_fenced());
        assert!(!core.diagnostic_version_busy());
        assert_eq!(probe::pin_state(pool).1, 0);
        assert!(!state.lock().unwrap().events.is_empty());
    });
}
macro_rules! cut {
    ($name:ident,$point:ident) => {
        #[test]
        fn $name() {
            init_cut(probe::InitPoint::$point)
        }
    };
}
cut!(resident_status_cut_stage, StageOwned);
cut!(resident_status_cut_dpt_owned, DptGuardOwnedBeforePrecheck);
cut!(resident_status_cut_dpt_checked, DptPrecheckedBeforePrepared);
cut!(resident_status_cut_allocator, AllocatorImageInstalled);
cut!(resident_status_cut_payload, PayloadImageInstalled);
cut!(resident_status_cut_metadata, MetadataInstalled);
cut!(resident_status_cut_registry, RegistryInstalled);
cut!(resident_status_cut_publication, Published);

#[test]
fn resident_status_admission_preserves_poison_and_fenced_causes() {
    fixture!(|bound, core, state, storage| {
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _held = bound.read_gate.lock().unwrap();
                panic!("poison gate");
            }))
            .is_err()
        );
        assert!(!storage.session_fence().is_fenced());
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (_, reader, _) = bound.split_with_flush();
        for cause in [dirty_frame::Cause::Poisoned, dirty_frame::Cause::Fenced] {
            match reader.try_admit(&mut owner) {
                Err(e) => {
                    assert_eq!(e.effect, Effect::CallerMustResolve);
                    assert!(matches!(e.cause,Cause::Frame(source) if source.cause==cause));
                }
                Ok(_) => panic!("poisoned gate admitted"),
            }
            assert!(storage.session_fence().is_fenced());
        }
        assert!(!state.lock().unwrap().events.is_empty());
    });
}
#[test]
fn resident_status_foreign_owner_does_not_acquire_admission() {
    fixture!(|bound, core, state, storage| {
        let foreign_core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut foreign = foreign_core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut local = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (_, reader, _) = bound.split_with_flush();
        assert!(
            matches!(reader.try_admit(&mut foreign),Err(e) if matches!(e.cause,Cause::BindingMismatch) && e.effect==Effect::Unchanged)
        );
        drop(reader.try_admit(&mut local).unwrap());
        assert!(!storage.session_fence().is_fenced());
        assert!(!state.lock().unwrap().events.is_empty());
    });
}

#[test]
fn resident_status_forgotten_other_slot_pin_blocks_eviction_and_load() {
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        {
            let (_, _, mut flush) = bound.split_with_flush();
            flush.try_flush_page(&mut dedicated, a.page).unwrap();
            flush.try_flush_page(&mut dedicated, b.page).unwrap();
        }
        let request = residency::slot(&bound.inner.inner.pool, 1).unwrap().0;
        std::mem::forget(
            bound
                .inner
                .inner
                .pool
                .reserve_targets(&mut owner, &[request])
                .unwrap(),
        );
        let before = snapshot(&bound.inner.inner.pool);
        let reads = state.lock().unwrap().reads;
        assert_eq!(bound.prepare_slot(2).unwrap_err().effect, Effect::Unchanged);
        assert_eq!(
            bound.load_into(2, a.page).unwrap_err().effect,
            Effect::Unchanged
        );
        assert_eq!(snapshot(&bound.inner.inner.pool), before);
        assert_eq!(state.lock().unwrap().reads, reads);
        assert!(!storage.session_fence().is_fenced());
    });
}

fn staged_index_mismatch(completion: bool) {
    use super::super::super::super::super::allocation_reservation::operation::AllocationPopulation;
    use super::super::super::{
        CompletedInitAuthority, InitControlCommit, InitInstallPermit, InitInstallPhase, operation,
    };
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        bound.prepare_slot(2).unwrap();
        let (mut writer, _, _) = bound.split_with_flush();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let mut authority = reserved
            .writer
            .authority(
                reserved.batch,
                reserved.page,
                reserved.ordinal,
                reserved.kind,
            )
            .unwrap();
        let before = snapshot(reserved.writer.pool);
        let descriptor_before = reserved.writer.base.descriptor;
        let mut write = reserved
            .writer
            .pool
            .claim_init_targets(reserved.owner, &authority)
            .unwrap()
            .try_write()
            .unwrap();
        let mut old = boxed::<PAGE_BYTES>(0);
        let mut frame = boxed::<65536>(0);
        let mut allocator_after = boxed::<PAGE_BYTES>(0);
        let mut status_after = boxed::<PAGE_BYTES>(0);
        let stamp = write.copy_allocator_snapshot(&mut old).unwrap();
        let plan = operation::InitPlan {
            profile: storage.profile(),
            allocator: reserved.writer.base.identity,
            payload: authority.payload.identity,
            prefix: storage.wal().prefix().unwrap(),
            batch: reserved.batch,
            before_descriptor: &descriptor_before,
            role: &storage.status_role().unwrap(),
        };
        let prepared = operation::prepare_init_in_population(
            &plan,
            &old,
            &mut frame,
            &mut allocator_after,
            &mut status_after,
            AllocationPopulation::Status,
        )
        .unwrap();
        write.preflight(&stamp).unwrap();
        let allocator_image = prepared.allocator_after();
        let status_image = prepared.status_after();
        let descriptor_image = prepared.descriptor_bytes();
        let requirement = prepared.stage_required(storage.wal()).unwrap();
        let mut key = write.retain_claim();
        if !completion {
            probe::corrupt_init_key_index(&mut key, 1);
        }
        let permit = InitInstallPermit {
            key: &key,
            allocator_before: stamp,
            requirement,
            allocator_after: allocator_image,
            status_after: status_image,
        };
        let mut phase = InitInstallPhase::StageOwned;
        let commit = InitControlCommit {
            descriptor: &mut reserved.writer.base.descriptor,
            after_descriptor: descriptor_image,
            phase: &mut phase,
        };
        let events = state.lock().unwrap().events.len();
        if completion {
            write.prepare_install(&permit, commit).unwrap().install();
            write.release_checked().unwrap();
            storage.wal().wait_durable(&requirement).unwrap();
            let installed = snapshot(reserved.writer.pool);
            authority.payload_index = 1;
            assert_eq!(
                reserved
                    .writer
                    .pool
                    .finish_init_claim(&mut key, &CompletedInitAuthority { authority })
                    .unwrap_err()
                    .effect,
                Effect::CallerMustResolve
            );
            assert_eq!(snapshot(reserved.writer.pool), installed);
            assert!(installed[0].0.claim_present);
            assert_eq!(installed[0].0.total, 0);
        } else {
            match write.prepare_install(&permit, commit) {
                Err(e) => assert_eq!(e.effect, Effect::CallerMustResolve),
                Ok(_) => panic!("wrong index installed"),
            }
            assert_eq!(phase, InitInstallPhase::StageOwned);
            assert_eq!(reserved.writer.base.descriptor, descriptor_before);
            assert_eq!(state.lock().unwrap().events.len(), events);
            drop(write);
            let after = snapshot(reserved.writer.pool);
            for index in 0..3 {
                assert_eq!(after[index].1, before[index].1);
                assert_eq!(after[index].0.entry, before[index].0.entry);
                assert_eq!(after[index].2, before[index].2);
            }
        }
        assert!(storage.session_fence().is_fenced());
        drop(key);
        drop(reserved);
    });
}
#[test]
fn resident_status_staged_key_rejects_other_slot() {
    staged_index_mismatch(false)
}
#[test]
fn resident_status_completion_rejects_other_slot() {
    staged_index_mismatch(true)
}

// Final S5 inputs are generated one profile/capacity at a time by the existing
// independent dev3_operational_recovery_literals.py (--cancellations 0). The
// largest active input is <8MiB; no fixture is generated from product output.
fn s5_grid<const N: usize>(profile: u32) {
    let input = std::path::PathBuf::from(
        std::env::var_os("XADB_RESIDENT_S5_INPUT")
            .expect("explicit independent S5 input directory"),
    );
    let expected = |name: &str| std::fs::read(input.join(name)).unwrap();
    let mut slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0);
    let (namespace, state) = port::fresh();
    let fresh = FreshStorageSession::create_status(
        namespace,
        profile,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut backing: Vec<_> = (0..N).map(|_| boxed::<PAGE_BYTES>(0x73)).collect();
    let mut frames: Vec<_> = backing.iter_mut().map(|p| DirtyFrame::new(p)).collect();
    let mut operation = boxed::<OPERATION_BYTES>(0);
    let binding = storage
        .bind_resident_status(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(v) => v,
        Err(_) => panic!("S5 activation"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut pages = Vec::new();
    for index in 1..N {
        if index != 1 {
            bound.prepare_slot(index).unwrap();
        }
        let (mut writer, _, _) = bound.split_with_flush();
        let value = initialized(writer.reserve_one(&mut owner).unwrap());
        assert_eq!(value.page.page_no, index as u64);
        pages.push(value.page);
    }
    {
        let (_, reader, mut flush) = bound.split_with_flush();
        let read = reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<N>(&pages)
            .unwrap();
        assert_eq!(
            read.bytes(0).unwrap().as_slice(),
            expected("allocator.after")
        );
        for index in 1..N {
            assert_eq!(
                read.bytes(index).unwrap().as_slice(),
                expected(&format!("status-{index}.after"))
            );
        }
        read.release_checked().unwrap();
        for page in &pages {
            flush.try_flush_page(&mut dedicated, *page).unwrap();
        }
        flush.try_flush_allocator(&mut dedicated).unwrap();
    }
    let mut victims = 0;
    let mut late_cases = 0;
    for victim in 1..N {
        bound.prepare_slot(victim).unwrap();
        bound.load_into(victim, pages[victim - 1]).unwrap();
        victims += 1;
        let (_, reader, _) = bound.split_with_flush();
        let pool = reader.inner.pool;
        for late in 0..N {
            residency::hold_write(pool, late, || {
                assert!(
                    reader
                        .try_admit(&mut owner)
                        .unwrap()
                        .read_set::<N>(&pages)
                        .is_err()
                );
                assert_eq!(probe::pin_state(pool).1, 0);
            });
            late_cases += 1;
        }
        reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<N>(&pages)
            .unwrap()
            .release_checked()
            .unwrap();
    }
    assert_eq!(victims, N - 1);
    assert_eq!(late_cases, N * (N - 1));
    assert_eq!(
        &state.lock().unwrap().files[wal::WAL_NAME].bytes[4096..],
        expected("wal.frames-only")
    );
    assert_eq!(
        bound.inner.inner.control.base.descriptor.as_slice(),
        expected("terminal.descriptor")
    );
    println!("RESIDENT_S5 profile={profile} frames={N} victims={victims} late={late_cases} PASS");
    drop(bound);
    drop(owner);
    drop(dedicated);
    drop(storage);
    let state = state.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
}
macro_rules! s5 {
    ($name:ident,$profile:literal,$n:literal) => {
        #[test]
        #[ignore = "S5: explicit per-profile independent literals and full victim/late grid"]
        fn $name() {
            s5_grid::<$n>($profile)
        }
    };
}
s5!(resident_status_s5_p2_n4, 2, 4);
s5!(resident_status_s5_p2_n8, 2, 8);
s5!(resident_status_s5_p2_n64, 2, 64);
s5!(resident_status_s5_p3_n4, 3, 4);
s5!(resident_status_s5_p3_n8, 3, 8);
s5!(resident_status_s5_p3_n64, 3, 64);

#[test]
fn resident_status_late_semantic_failure_releases_whole_read() {
    use crate::storage::{page, status_home, transaction_record::TransactionStatusCell};
    use xadb_foundation::{codec::content_digest, identity::Generation};
    fixture!(|bound, core, state, storage| {
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let (a, b) = two_dirty!(bound, owner);
        let (_, reader, _) = bound.split_with_flush();
        let pool = reader.inner.pool;
        residency::edit_content(pool, 2, |bytes| {
            // Canonical Free generation 2 decodes, but this fresh INIT reader
            // requires generation 1. Preserve the header, pageLSN and checksum.
            let cell = usize::from(u16::from_le_bytes(bytes[128..130].try_into().unwrap()));
            bytes[cell + 80..cell + 88].copy_from_slice(&2_u64.to_le_bytes());
            let digest = content_digest(&bytes[..page::DIGEST_OFFSET]);
            bytes[page::DIGEST_OFFSET..].copy_from_slice(&digest);
            let identity = residency::slot(pool, 2).unwrap().0.identity;
            let view = status_home::decode_status_home_page(
                bytes,
                identity,
                reader.inner.role.namespace(),
            )
            .unwrap();
            assert_eq!(
                view.cell(0).unwrap().core(),
                TransactionStatusCell::Free(Generation::new(2).unwrap())
            );
        });
        reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<2>(&[a.page])
            .unwrap()
            .release_checked()
            .unwrap();
        let before = snapshot(pool);
        let error = match reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<3>(&[b.page, a.page])
        {
            Ok(_) => panic!("late semantic mismatch accepted"),
            Err(error) => error,
        };
        assert!(matches!(error.cause, Cause::Semantic(_)));
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(probe::pin_state(pool).1, 0);
        for index in 0..3 {
            residency::hold_write(pool, index, || {});
        }
        assert!(reader.gate.try_lock().is_ok());
        assert_eq!(snapshot(pool), before);
        assert!(!state.lock().unwrap().events.is_empty());
    });
}
