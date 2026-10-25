use super::*;
use crate::storage::page_io::test_support::expected;
use crate::transaction::concurrency::OwnerRole;
use foundation::identity::{Generation, ObjectId};
use std::mem::size_of;

fn request(page: u64) -> TargetRequest {
    let mut identity = expected();
    identity.page.page_no = page;
    TargetRequest {
        identity,
        kind: PageKind::RowLeaf,
        level: 0,
    }
}
fn with_pool(run: impl FnOnce(&SharedPool<'_, '_, '_, '_>)) {
    let mut a = [0; PAGE_BYTES];
    let mut b = [0; PAGE_BYTES];
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut b)];
    frames[0].register(request(1)).unwrap();
    frames[1].register(request(2)).unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let pool = SharedPool::new(&core, &fence, &binding, &frames).unwrap();
    run(&pool);
}
fn counts(pool: &SharedPool<'_, '_, '_, '_>) -> ([u16; 2], u16) {
    let pins = pool.pins.lock().unwrap();
    ([pins.counts[0], pins.counts[1]], pins.total)
}
fn invariant<T>(result: Result<T>) {
    match result {
        Err(error) => assert_eq!(
            error,
            DirtyError {
                cause: Cause::Invariant,
                effect: Effect::CallerMustResolve
            }
        ),
        Ok(_) => panic!("stale registration token accepted"),
    }
}

#[test]
fn registration_layout_receipt() {
    let registration = size_of::<Registration>();
    let prior_directory = size_of::<[Option<Registration>; MAX_FRAMES]>()
        + size_of::<Option<usize>>()
        + size_of::<std::sync::OnceLock<Registration>>();
    let directory = size_of::<RegistrationDirectory>();
    let pin = size_of::<PinSet<'_, '_, '_, '_, '_>>();
    let read = size_of::<ReadTargets<'_, '_, '_, '_, '_, '_>>();
    let write = size_of::<WriteTargets<'_, '_, '_, '_, '_, '_>>();
    let read_result = size_of::<Result<ReadTargets<'_, '_, '_, '_, '_, '_>>>();
    let scratch = 4 * 192
        + 4 * 112
        + 2 * 96
        + 6 * size_of::<Result<Registration>>()
        + 2 * 32
        + 4 * 32
        + 32 * 8
        + 64;
    println!(
        "REGISTRATION_LAYOUT registration={registration} directory={directory} prior_directory={prior_directory} pin={pin} read={read} write={write} read_result={read_result} generic={scratch} ceiling={CALL_BYTES}"
    );
    // Per-slot OnceLock adds at most eight bytes per entry; the former global
    // publication cell is removed. Keep independent whole-operation ceilings.
    assert!(directory <= prior_directory + MAX_FRAMES * 8);
    assert!(registration <= 112 && scratch <= CALL_BYTES);
    assert!(
        2 * read_result + read + pin + registration + size_of::<DirtyObservation>() + 64
            <= CALL_BYTES
    );
    assert!(fits());
}

#[test]
fn registration_reverse_rank_tokens_and_copy_requests() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let pinned = pool
            .reserve_targets(&mut owner, &[request(2), request(1)])
            .unwrap();
        let pins = pinned.pins.as_ref().unwrap();
        assert_eq!(pins.indices, [0, 1]);
        assert_eq!(pins.generations, [1, 1]);
        let read = pinned.try_read().unwrap();
        assert_eq!(read.target_request(0).unwrap(), request(1));
        assert_eq!(read.target_request(1).unwrap(), request(2));
        assert_eq!(read.observe(0).unwrap().frame.identity, request(1).identity);
        assert_eq!(read.observe(1).unwrap().frame.identity, request(2).identity);
        read.release_checked().unwrap();
        assert_eq!(counts(pool), ([0, 0], 0));
    });
}

#[test]
fn registration_pin_set_rejection_is_atomic() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        for requests in [[request(1), request(3)], [request(1), request(1)]] {
            assert!(pool.reserve_targets(&mut owner, &requests).is_err());
            assert_eq!(counts(pool), ([0, 0], 0));
        }
        {
            let mut pins = pool.pins.lock().unwrap();
            pins.counts[1] = MAX_FRAME_PINS;
            pins.total = MAX_FRAME_PINS;
        }
        assert!(matches!(
            pool.reserve_targets(&mut owner, &[request(1), request(2)]),
            Err(DirtyError {
                cause: Cause::Capacity,
                effect: Effect::Unchanged
            })
        ));
        assert_eq!(counts(pool), ([0, MAX_FRAME_PINS], MAX_FRAME_PINS));
    });
}

#[test]
fn registration_stale_second_release_preserves_every_count() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut pinned = pool
            .reserve_targets(&mut owner, &[request(1), request(2)])
            .unwrap();
        pinned.pins.as_mut().unwrap().generations[1] = 2;
        invariant(pinned.release_checked());
        assert!(pool.fence.is_fenced());
        assert_eq!(counts(pool), ([1, 1], 2));
    });
}

#[test]
fn registration_stale_read_acquisition_fences() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut pinned = pool.reserve_targets(&mut owner, &[request(1)]).unwrap();
        pinned.pins.as_mut().unwrap().generations[0] = 0;
        invariant(pinned.try_read());
        assert!(pool.fence.is_fenced());
        assert_eq!(counts(pool), ([1, 0], 1));
        assert!(pool.frames[0].content.try_write().is_ok());
    });
}

#[test]
fn registration_stale_write_acquisition_fences() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut pinned = pool.reserve_targets(&mut owner, &[request(1)]).unwrap();
        pinned.pins.as_mut().unwrap().generations[0] = u64::MAX;
        invariant(pinned.try_write());
        assert!(pool.fence.is_fenced());
        assert_eq!(counts(pool), ([1, 0], 1));
        assert!(pool.frames[0].content.try_write().is_ok());
    });
}

#[test]
fn registration_stale_read_observation_and_request_fence() {
    for entry in 0..3 {
        with_pool(|pool| {
            let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
            let mut read = pool
                .reserve_targets(&mut owner, &[request(1)])
                .unwrap()
                .try_read()
                .unwrap();
            read.pins.as_mut().unwrap().generations[0] = 2;
            match entry {
                0 => invariant(read.target_request(0)),
                1 => invariant(read.bytes(0)),
                2 => invariant(read.observe(0)),
                _ => unreachable!(),
            }
            assert!(pool.fence.is_fenced());
        });
    }
}

#[test]
fn registration_stale_write_stamp_fences() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::Foreground).unwrap();
        let mut write = pool
            .reserve_targets(&mut owner, &[request(1)])
            .unwrap()
            .try_write()
            .unwrap();
        let stamp = pool.stamp(0, write.guards[0].as_ref().unwrap()).unwrap();
        write.pins.as_mut().unwrap().generations[0] = 2;
        invariant(write.matches_stamp(0, &stamp));
        assert!(pool.fence.is_fenced());
    });
}

#[test]
fn registration_stale_finalize_acquisition_fences() {
    with_pool(|pool| {
        let mut owner = pool.core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut pinned = pool.reserve_targets(&mut owner, &[request(1)]).unwrap();
        pinned.pins.as_mut().unwrap().generations[0] = 2;
        invariant(flush::test_support::acquire_finalize(pinned));
        assert!(pool.fence.is_fenced());
        assert_eq!(counts(pool), ([1, 0], 1));
    });
}

#[test]
fn registration_directory_rejects_address_aliases_and_bootstrap_generation() {
    let mut a = [0; PAGE_BYTES];
    let mut b = [0; PAGE_BYTES];
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut b)];
    frames[0].register(request(1)).unwrap();
    let mut alias = request(1);
    alias.identity.page.incarnation = Generation::new(2).unwrap();
    frames[1].register(alias).unwrap();
    assert!(RegistrationDirectory::from_bootstrap(&frames).is_none());
    frames[1].registration = Some(Registration {
        request: request(2),
        generation: 2,
    });
    assert!(RegistrationDirectory::from_bootstrap(&frames).is_none());
}

#[test]
fn registration_closed_publication_is_single_and_checked() {
    let mut a = [0; PAGE_BYTES];
    let mut b = [0; PAGE_BYTES];
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut b)];
    let mut allocator = request(0);
    allocator.kind = PageKind::Allocation;
    frames[0].register(allocator).unwrap();
    let mut directory = RegistrationDirectory::from_bootstrap(&frames).unwrap();
    directory.close_status_slot();
    assert!(directory.valid(&frames));
    let mut status = request(1);
    status.kind = PageKind::TransactionStatus;
    status.identity.owner = ObjectId::new(2).unwrap();
    status.identity.owner_generation = Generation::new(1).unwrap();
    status.identity.page.incarnation = Generation::new(2).unwrap();
    assert!(directory.lookup(&frames, status).is_none());
    let registration = Registration {
        request: status,
        generation: 1,
    };
    assert!(directory.publish(registration));
    assert!(directory.valid(&frames));
    assert_eq!(directory.lookup(&frames, status), Some((1, registration)));
    assert!(directory.publish(registration));
    assert!(!directory.publish(Registration {
        generation: 2,
        ..registration
    }));
    assert_eq!(directory.published(), Some(registration));
    status.identity.page.incarnation = Generation::new(3).unwrap();
    assert!(directory.lookup(&frames, status).is_none());
}

#[test]
fn registration_copy_request_rejects_invalid_fixed_publication() {
    let mut a = [0; PAGE_BYTES];
    let mut b = [0; PAGE_BYTES];
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut b)];
    let mut allocator = request(0);
    allocator.kind = PageKind::Allocation;
    frames[0].register(allocator).unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let pool = SharedPool::new_with_closed_init(&core, &fence, &binding, &frames).unwrap();
    let mut status = request(1);
    status.kind = PageKind::TransactionStatus;
    status.identity.owner = ObjectId::new(2).unwrap();
    status.identity.owner_generation = Generation::new(1).unwrap();
    status.identity.page.incarnation = Generation::new(2).unwrap();
    status.identity.owner = ObjectId::new(3).unwrap();
    assert!(pool.directory.publish(Registration {
        request: status,
        generation: 1
    }));
    invariant(pool.target_request(1));
    assert!(fence.is_fenced());
}
