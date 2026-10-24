//! Independent raw sequence; fixture seed supplies a genuine borrowed plan3
//! role but is not fresh installation or mutable-owner execution evidence.
use super::super::super::super::allocation_reservation::operation as alloc;
use super::super::super::status_init_operation::{self as init, mixed};
use super::*;
use crate::storage::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    obligation,
    page::PageRef,
    wal_frame::FrameExpectation,
};
use xadb_foundation::identity::ObjectId;

const MIXED_INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-mixed-system-init"
);
const BUDGET: alloc::OperationBudget = alloc::OperationBudget::MixedInit114688;
const POP: alloc::AllocationPopulation = alloc::AllocationPopulation::MixedSystem;
fn raw(p: u32, group: &str, work: u64, part: &str) -> Vec<u8> {
    std::fs::read(format!("{MIXED_INPUT}/p{p}/{group}/work{work}-{part}")).unwrap()
}
fn fixed<const N: usize>(p: u32, group: &str, work: u64, part: &str) -> Box<[u8; N]> {
    raw(p, group, work, part)
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn mark_mixed(id: &str, p: u32, work: u64) {
    println!("XADB_MIXED_INIT_CASE {id} p{p} w{work}");
}
fn input(
    p: u32,
    work: u64,
    f: impl FnOnce(&mixed::Plan<'_, '_>, &wal::GenesisWalSession<'_, port::Writer>),
) {
    let mut slot = boxed::<{ wal::MAX_FRAME_BYTES }>(0);
    let (mut fresh, state) = undo_seed(&mut slot);
    fresh.plan = undo_plan(p, &mut [[0; PAGE_BYTES]; 3]);
    let before_files = images(&state);
    let allocator = fresh.plan.allocation_identity().unwrap();
    let role = fresh.undo_role().unwrap();
    let kind = if work == 1 {
        mixed::Kind::Status
    } else {
        mixed::Kind::UndoRoot
    };
    let descriptor = fixed::<1536>(p, "sequence", work, "reserve.descriptor");
    let mut prefix = fresh.wal.prefix().unwrap();
    prefix.complete_lsn = 2 * work - 1;
    prefix.synced_lsn = prefix.complete_lsn;
    prefix.complete_offset = 4096
        + 21200
        + if work == 1 {
            0
        } else {
            59200 + (work - 2) * 59408
        };
    prefix.synced_offset = prefix.complete_offset;
    let plan = mixed::Plan {
        profile: fresh.plan.profile,
        allocator,
        payload: PageIdentity {
            database: allocator.database,
            page: PageRef {
                page_no: work,
                incarnation: Generation::new(2).unwrap(),
                ..allocator.page
            },
            owner: ObjectId::new(if work == 1 { 2 } else { 3 }).unwrap(),
            owner_generation: Generation::new(1).unwrap(),
        },
        prefix,
        batch: alloc::Batch {
            owner: SystemWorkRef::new(
                fresh.plan.profile.spec().namespace,
                SystemWorkKind::AllocationMaintenance,
                work,
                Generation::new(1).unwrap(),
            )
            .unwrap(),
            mask: 1 << work,
            birth: 2 * work - 1,
        },
        before_descriptor: &descriptor,
        role: &role,
        kind,
    };
    f(&plan, &fresh.wal);
    assert_eq!(
        images(&state),
        before_files,
        "pure preparation has no file effects"
    );
    assert!(state.lock().unwrap().events.is_empty());
}
fn expectation<'a>(plan: &mixed::Plan<'a, '_>) -> mixed::Expected<'a> {
    mixed::Expected::new(
        plan.profile,
        plan.allocator,
        plan.payload,
        FrameExpectation {
            stream: plan.prefix.stream,
            previous_lsn: plan.prefix.complete_lsn,
        },
        plan.batch,
        plan.before_descriptor,
        plan.kind,
    )
}
fn rehash_frame(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn rewrite_entry(
    plan: &mixed::Plan<'_, '_>,
    before: &[u8; PAGE_BYTES],
    index: usize,
    entry: AllocationEntry,
) -> Box<[u8; PAGE_BYTES]> {
    let page = allocation_page::decode_allocation_page(
        before,
        plan.allocator,
        plan.profile.spec().namespace,
    )
    .unwrap();
    let mut entries = [AllocationEntry::Free(Generation::new(1).unwrap()); 64];
    for (i, dst) in entries.iter_mut().enumerate() {
        *dst = if i == index {
            entry
        } else {
            page.entry(i).unwrap()
        };
    }
    let mut cell = [0; 6208];
    allocation_page::encode_allocation_cell(
        page.header(),
        plan.profile.spec().namespace,
        &entries,
        &mut cell,
    )
    .unwrap();
    let mut after = boxed::<PAGE_BYTES>(0);
    allocation_page::encode_allocation_page(
        page.header(),
        plan.profile.spec().namespace,
        &cell,
        &mut after[..],
    )
    .unwrap();
    after
}

#[test]
fn mixed_init_mi01_sequence_producer_and_observer() {
    for p in 1..=3 {
        for work in 1..=3 {
            input(p, work, |plan, _session| {
                let before = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.reserved");
                let mut output = boxed::<65536>(0xa5);
                let mut allocator = boxed::<PAGE_BYTES>(0xa5);
                let mut payload = boxed::<PAGE_BYTES>(0xa5);
                let prepared =
                    mixed::prepare(plan, &before, &mut output, &mut allocator, &mut payload)
                        .unwrap();
                assert_eq!(
                    prepared.descriptor_bytes().as_slice(),
                    raw(p, "sequence", work, "terminal.descriptor")
                );
                assert_eq!(
                    prepared.allocator_after().as_slice(),
                    raw(p, "sequence", work, "allocator.after")
                );
                assert_eq!(
                    prepared.payload_after().as_slice(),
                    raw(p, "sequence", work, "payload.after")
                );
                let bytes = raw(p, "sequence", work, "terminal.wal");
                assert_eq!(&output[..bytes.len()], bytes);
                assert!(output[bytes.len()..].iter().all(|b| *b == 0xa5));
                mixed::validate(&expectation(plan), &before, &bytes, &allocator, &payload).unwrap();
                let mut replay_allocator = boxed::<PAGE_BYTES>(0xa5);
                let mut replay_payload = boxed::<PAGE_BYTES>(0xa5);
                mixed::materialize(
                    &expectation(plan),
                    &before,
                    &bytes,
                    &mut replay_allocator,
                    &mut replay_payload,
                )
                .unwrap();
                assert_eq!(replay_allocator, allocator);
                assert_eq!(replay_payload, payload);
                let page = allocation_page::decode_allocation_page(
                    &allocator[..],
                    plan.allocator,
                    plan.profile.spec().namespace,
                )
                .unwrap();
                alloc::validate_live_in_population(
                    &page,
                    plan.profile,
                    None,
                    &bytes[352..1888],
                    work,
                    BUDGET,
                    POP,
                    plan.kind.terminal(),
                )
                .unwrap();
                mark_mixed("MI01", p, work);
            });
        }
    }
}

#[test]
fn mixed_init_mi02_reserve_cancel_and_scope() {
    for p in 1..=3 {
        for work in 1..=3 {
            input(p, work, |plan, _session| {
                let before = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.before");
                let reserved = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.reserved");
                let mut prefix = plan.prefix;
                prefix.complete_lsn -= 1;
                prefix.synced_lsn -= 1;
                prefix.complete_offset -= 21200;
                prefix.synced_offset -= 21200;
                let reserve = alloc::Plan {
                    profile: plan.profile,
                    identity: plan.allocator,
                    prefix,
                    batch: plan.batch,
                    cancel: false,
                    before_descriptor: None,
                };
                let mut output = boxed::<65536>(0xa5);
                let mut after = boxed::<PAGE_BYTES>(0xa5);
                let produced = alloc::prepare_in_population(
                    reserve,
                    &before,
                    &mut output[..],
                    &mut after,
                    BUDGET,
                    POP,
                )
                .unwrap();
                assert_eq!(produced.descriptor_bytes(), plan.before_descriptor);
                assert_eq!(&output[..21200], raw(p, "sequence", work, "reserve.wal"));
                assert_eq!(after, reserved);
                let expected = alloc::AnalysisExpected {
                    profile: plan.profile,
                    identity: plan.allocator,
                    frame: FrameExpectation {
                        stream: prefix.stream,
                        previous_lsn: prefix.complete_lsn,
                    },
                    batch: plan.batch,
                    cancel: false,
                    before_descriptor: None,
                };
                alloc::materialize_analysis_in_population(
                    expected,
                    &before,
                    &output[..21200],
                    &mut after,
                    BUDGET,
                    POP,
                )
                .unwrap();
                assert_eq!(after, reserved);
                let page = allocation_page::decode_allocation_page(
                    &reserved[..],
                    plan.allocator,
                    plan.profile.spec().namespace,
                )
                .unwrap();
                for (budget, population) in [
                    (alloc::OperationBudget::StatusInit114688, POP),
                    (alloc::OperationBudget::Dirty98304, POP),
                    (BUDGET, alloc::AllocationPopulation::Status),
                    (BUDGET, alloc::AllocationPopulation::Empty),
                ] {
                    assert!(
                        alloc::prepare_in_population(
                            reserve,
                            &before,
                            &mut output[..],
                            &mut after,
                            budget,
                            population
                        )
                        .is_err()
                    );
                    assert!(
                        alloc::validate_live_in_population(
                            &page,
                            plan.profile,
                            Some(plan.batch),
                            plan.before_descriptor,
                            work,
                            budget,
                            population,
                            alloc::TerminalOutcome::Cancelled
                        )
                        .is_err()
                    );
                }
                // Active-batch validation intentionally ignores the terminal tag.
                for terminal in [
                    alloc::TerminalOutcome::Cancelled,
                    alloc::TerminalOutcome::InitializedStatus,
                    alloc::TerminalOutcome::InitializedMixedStatus,
                    alloc::TerminalOutcome::InitializedUndoRoot,
                ] {
                    alloc::validate_live_in_population(
                        &page,
                        plan.profile,
                        Some(plan.batch),
                        plan.before_descriptor,
                        work,
                        BUDGET,
                        POP,
                        terminal,
                    )
                    .unwrap();
                }
                mark_mixed("MI02-reserve", p, work);
                if work == 3 {
                    let before = fixed::<PAGE_BYTES>(p, "cancel", 4, "allocator.before");
                    let reserved = fixed::<PAGE_BYTES>(p, "cancel", 4, "allocator.reserved");
                    let desc = fixed::<1536>(p, "cancel", 4, "reserve.descriptor");
                    let mut prefix = plan.prefix;
                    prefix.complete_lsn = 7;
                    prefix.synced_lsn = 7;
                    prefix.complete_offset = 203312;
                    prefix.synced_offset = 203312;
                    let batch = alloc::Batch {
                        owner: SystemWorkRef::new(
                            plan.profile.spec().namespace,
                            SystemWorkKind::AllocationMaintenance,
                            4,
                            Generation::new(1).unwrap(),
                        )
                        .unwrap(),
                        mask: 16,
                        birth: 7,
                    };
                    let mut reserve_prefix = prefix;
                    reserve_prefix.complete_lsn = 6;
                    reserve_prefix.synced_lsn = 6;
                    reserve_prefix.complete_offset -= 21200;
                    reserve_prefix.synced_offset -= 21200;
                    alloc::prepare_in_population(
                        alloc::Plan {
                            profile: plan.profile,
                            identity: plan.allocator,
                            prefix: reserve_prefix,
                            batch,
                            cancel: false,
                            before_descriptor: None,
                        },
                        &before,
                        &mut output[..],
                        &mut after,
                        BUDGET,
                        POP,
                    )
                    .unwrap();
                    assert_eq!(after, reserved);
                    assert_eq!(&output[..21200], raw(p, "cancel", 4, "reserve.wal"));
                    let cancel = alloc::Plan {
                        profile: plan.profile,
                        identity: plan.allocator,
                        prefix,
                        batch,
                        cancel: true,
                        before_descriptor: Some(&desc[..]),
                    };
                    alloc::prepare_in_population(
                        cancel,
                        &reserved,
                        &mut output[..],
                        &mut after,
                        BUDGET,
                        POP,
                    )
                    .unwrap();
                    assert_eq!(&output[..21200], raw(p, "cancel", 4, "terminal.wal"));
                    assert_eq!(after.as_slice(), raw(p, "cancel", 4, "allocator.after"));
                    let page = allocation_page::decode_allocation_page(
                        &after[..],
                        plan.allocator,
                        plan.profile.spec().namespace,
                    )
                    .unwrap();
                    alloc::validate_live_in_population(
                        &page,
                        plan.profile,
                        None,
                        &output[352..1888],
                        4,
                        BUDGET,
                        POP,
                        alloc::TerminalOutcome::Cancelled,
                    )
                    .unwrap();
                    for i in 1..=3 {
                        assert_eq!(
                            page.entry(i).unwrap(),
                            allocation_page::decode_allocation_page(
                                &before[..],
                                plan.allocator,
                                plan.profile.spec().namespace
                            )
                            .unwrap()
                            .entry(i)
                            .unwrap()
                        );
                    }
                    assert_eq!(
                        page.entry(4).unwrap(),
                        AllocationEntry::Free(Generation::new(2).unwrap())
                    );
                    mark_mixed("MI02-cancel", p, 4);
                }
            });
        }
    }
}

#[test]
fn mixed_init_mi03_legacy_bytes_and_rejection() {
    for p in 1..=3 {
        input(p, 1, |plan, _session| {
            let before = fixed::<PAGE_BYTES>(p, "old-status-init", 1, "allocator.reserved");
            let bytes = raw(p, "old-status-init", 1, "init.wal");
            let allocator = fixed::<PAGE_BYTES>(p, "old-status-init", 1, "allocator.after");
            let payload = fixed::<PAGE_BYTES>(p, "old-status-init", 1, "status.after");
            let descriptor = fixed::<1536>(p, "old-status-init", 1, "reserve.descriptor");
            let expected = init::InitExpected {
                profile: plan.profile,
                allocator: plan.allocator,
                payload: plan.payload,
                frame: FrameExpectation {
                    stream: plan.prefix.stream,
                    previous_lsn: 1,
                },
                batch: plan.batch,
                before_descriptor: &descriptor,
            };
            init::validate_expected(&expected, &before, &bytes, &allocator, &payload).unwrap();
            let mut old_slot = boxed::<{ wal::MAX_FRAME_BYTES }>(0);
            let (mut old_session, _) = seed(&mut old_slot, true);
            old_session.plan = prepared(p, true, &mut [[0; PAGE_BYTES]; 3]);
            let old_role = old_session.status_role().unwrap();
            let legacy = init::InitPlan {
                profile: plan.profile,
                allocator: plan.allocator,
                payload: plan.payload,
                prefix: plan.prefix,
                batch: plan.batch,
                before_descriptor: &descriptor,
                role: &old_role,
            };
            let mut produced = boxed::<65536>(0xa5);
            let mut old_allocator = boxed::<PAGE_BYTES>(0xa5);
            let mut old_payload = boxed::<PAGE_BYTES>(0xa5);
            init::prepare_init(
                &legacy,
                &before,
                &mut produced,
                &mut old_allocator,
                &mut old_payload,
            )
            .unwrap();
            assert_eq!(&produced[..bytes.len()], bytes);
            assert_eq!(old_allocator, allocator);
            assert_eq!(old_payload, payload);
            let mut a = boxed::<PAGE_BYTES>(0xa5);
            let mut b = boxed::<PAGE_BYTES>(0xa5);
            init::materialize_analysis(&expected, &before, &bytes, &mut a, &mut b).unwrap();
            assert_eq!(a, allocator);
            assert_eq!(b, payload);
            assert_eq!(
                &bytes[352..1888],
                raw(p, "sequence", 1, "terminal.descriptor")
            );
            let mixed_before = fixed::<PAGE_BYTES>(p, "sequence", 1, "allocator.reserved");
            let mixed_bytes = raw(p, "sequence", 1, "terminal.wal");
            assert!(
                init::validate_expected(
                    &init::InitExpected {
                        before_descriptor: plan.before_descriptor,
                        ..expected
                    },
                    &mixed_before,
                    &mixed_bytes,
                    &allocator,
                    &payload
                )
                .is_err()
            );
            assert!(
                mixed::validate(&expectation(plan), &before, &bytes, &allocator, &payload).is_err()
            );
            mark_mixed("MI03", p, 1);
        });
    }
}

#[test]
fn mixed_init_mi04_semantic_corruption() {
    const OFFSETS: &[usize] = &[
        0, 4, 6, 8, 32, 40, 48, 128, 160, 224, 256, 352, 400, 512, 1024, 1888, 1920, 2080, 2176,
        2272, 2400, 8480, 8608, 14688, 14689, 14690, 14692, 14696, 14720, 14728, 14736, 14744,
        14752, 14784, 14816, 14822, 14824, 14900, 15039,
    ];
    for work in 1..=3 {
        input(1, work, |plan, _session| {
            let before = fixed::<PAGE_BYTES>(1, "sequence", work, "allocator.reserved");
            let allocator = fixed::<PAGE_BYTES>(1, "sequence", work, "allocator.after");
            let payload = fixed::<PAGE_BYTES>(1, "sequence", work, "payload.after");
            let bytes = raw(1, "sequence", work, "terminal.wal");
            let shift = if work == 1 { 0 } else { 208 };
            let offsets: Vec<usize> = OFFSETS
                .iter()
                .copied()
                .chain([
                    15040 + shift,
                    15104 + shift,
                    15248 + shift,
                    15312 + shift,
                    21520 + shift,
                    21584 + shift,
                    21648 + shift,
                    37967 + shift,
                ])
                .collect();
            for at in offsets {
                let mut bad = bytes.clone();
                bad[at] ^= 1;
                rehash_frame(&mut bad);
                assert!(
                    mixed::validate(&expectation(plan), &before, &bad, &allocator, &payload)
                        .is_err(),
                    "w{work} byte{at}"
                );
                let mut a = boxed::<PAGE_BYTES>(0);
                let mut b = boxed::<PAGE_BYTES>(0);
                assert!(
                    mixed::materialize(&expectation(plan), &before, &bad, &mut a, &mut b).is_err(),
                    "materialize w{work} byte{at}"
                );
                mark_mixed(&format!("MI04-byte{at}"), 1, work);
            }
            for len in [0, 127, 351, 1887, 2271, 14687, 14815, bytes.len() - 1] {
                assert!(
                    mixed::validate(
                        &expectation(plan),
                        &before,
                        &bytes[..len],
                        &allocator,
                        &payload
                    )
                    .is_err()
                );
                mark_mixed(&format!("MI04-short{len}"), 1, work);
            }
            let mut extra = bytes.clone();
            extra.push(0);
            assert!(
                mixed::validate(&expectation(plan), &before, &extra, &allocator, &payload).is_err()
            );
            mark_mixed("MI04-extra", 1, work);
        });
    }
}

#[test]
fn mixed_init_mi05_scope_membership_and_identity() {
    for work in 1..=3 {
        input(1, work, |plan, _session| {
            let before = fixed::<PAGE_BYTES>(1, "sequence", work, "allocator.reserved");
            let allocator = fixed::<PAGE_BYTES>(1, "sequence", work, "allocator.after");
            let payload = fixed::<PAGE_BYTES>(1, "sequence", work, "payload.after");
            let bytes = raw(1, "sequence", work, "terminal.wal");
            for payload in [
                PageIdentity {
                    owner: ObjectId::new(4).unwrap(),
                    ..plan.payload
                },
                PageIdentity {
                    owner_generation: Generation::new(2).unwrap(),
                    ..plan.payload
                },
                PageIdentity {
                    page: PageRef {
                        page_no: 63,
                        ..plan.payload.page
                    },
                    ..plan.payload
                },
                PageIdentity {
                    page: PageRef {
                        incarnation: Generation::new(3).unwrap(),
                        ..plan.payload.page
                    },
                    ..plan.payload
                },
            ] {
                let wrong = mixed::Plan { payload, ..*plan };
                let mut output = boxed::<65536>(0xa5);
                let mut a = boxed::<PAGE_BYTES>(0xa5);
                let mut b = boxed::<PAGE_BYTES>(0xa5);
                assert!(mixed::prepare(&wrong, &before, &mut output, &mut a, &mut b).is_err());
                assert!(
                    mixed::validate(
                        &expectation(&wrong),
                        &before,
                        &bytes,
                        &allocator,
                        &fixed::<PAGE_BYTES>(1, "sequence", work, "payload.after")
                    )
                    .is_err()
                );
                mark_mixed("MI05-identity", 1, work);
            }
            for kind in [mixed::Kind::Status, mixed::Kind::UndoRoot] {
                if kind == plan.kind {
                    continue;
                }
                let expected = mixed::Expected::new(
                    plan.profile,
                    plan.allocator,
                    plan.payload,
                    FrameExpectation {
                        stream: plan.prefix.stream,
                        previous_lsn: plan.prefix.complete_lsn,
                    },
                    plan.batch,
                    plan.before_descriptor,
                    kind,
                );
                assert!(mixed::validate(&expected, &before, &bytes, &allocator, &payload).is_err());
                mark_mixed("MI05-kind", 1, work);
            }
            for entry in [
                AllocationEntry::Allocated {
                    generation: Generation::new(2).unwrap(),
                    object: ObjectId::new(3).unwrap(),
                    object_generation: Generation::new(1).unwrap(),
                    kind: PageKind::TransactionStatus,
                },
                AllocationEntry::Allocated {
                    generation: Generation::new(2).unwrap(),
                    object: ObjectId::new(2).unwrap(),
                    object_generation: Generation::new(1).unwrap(),
                    kind: PageKind::Undo,
                },
                AllocationEntry::Allocated {
                    generation: Generation::new(2).unwrap(),
                    object: ObjectId::new(3).unwrap(),
                    object_generation: Generation::new(2).unwrap(),
                    kind: PageKind::Undo,
                },
                AllocationEntry::Allocated {
                    generation: Generation::new(1).unwrap(),
                    object: ObjectId::new(3).unwrap(),
                    object_generation: Generation::new(1).unwrap(),
                    kind: PageKind::Undo,
                },
                AllocationEntry::Reserved {
                    generation: Generation::new(2).unwrap(),
                    claimant: ReservationOwner::System(
                        SystemWorkRef::new(
                            plan.profile.spec().namespace,
                            SystemWorkKind::AllocationMaintenance,
                            99,
                            Generation::new(1).unwrap(),
                        )
                        .unwrap(),
                    ),
                },
            ] {
                let bad = rewrite_entry(plan, &before, 63, entry);
                let mut output = boxed::<65536>(0xa5);
                let mut a = boxed::<PAGE_BYTES>(0xa5);
                let mut b = boxed::<PAGE_BYTES>(0xa5);
                assert!(mixed::prepare(plan, &bad, &mut output, &mut a, &mut b).is_err());
                assert!(
                    mixed::validate(&expectation(plan), &bad, &bytes, &allocator, &payload)
                        .is_err()
                );
                mark_mixed("MI05-late", 1, work);
            }
            if work == 1 {
                for (stored, terminal) in [
                    (mixed::Kind::Status, mixed::Kind::UndoRoot),
                    (mixed::Kind::UndoRoot, mixed::Kind::Status),
                ] {
                    let candidate = rewrite_entry(
                        plan,
                        &allocator,
                        1,
                        AllocationEntry::Allocated {
                            generation: Generation::new(2).unwrap(),
                            object: ObjectId::new(if stored == mixed::Kind::Status {
                                2
                            } else {
                                3
                            })
                            .unwrap(),
                            object_generation: Generation::new(1).unwrap(),
                            kind: if stored == mixed::Kind::Status {
                                PageKind::TransactionStatus
                            } else {
                                PageKind::Undo
                            },
                        },
                    );
                    let page = allocation_page::decode_allocation_page(
                        &candidate[..],
                        plan.allocator,
                        plan.profile.spec().namespace,
                    )
                    .unwrap();
                    let spec = alloc::finished_mixed_descriptor(
                        &plan.profile,
                        plan.batch,
                        PageLsn(2),
                        terminal,
                    );
                    let mut descriptor = [0; 1536];
                    obligation::encode_descriptor(&spec, &mut descriptor).unwrap();
                    assert!(
                        alloc::validate_live_in_population(
                            &page,
                            plan.profile,
                            None,
                            &descriptor,
                            1,
                            BUDGET,
                            POP,
                            terminal.terminal()
                        )
                        .is_err()
                    );
                    mark_mixed("MI05-opposite-member", 1, work);
                }
            }
        });
    }
}

#[test]
fn mixed_init_mi06_source_value_census() {
    println!(
        "XADB_MIXED_INIT_CENSUS plans={} mixed_plans={} temps={} undo_codec={} old_status_codec={} caller_bytes=98304 retained_heap=0",
        init::plan_bytes(),
        mixed::plan_bytes(),
        init::temporary_bytes(),
        mixed::codec_bytes(),
        init::status_codec_bytes()
    );
    assert!(init::fits());
    assert!(mixed::plan_bytes() <= init::plan_bytes());
    assert!(mixed::codec_bytes() <= init::status_codec_bytes());
    assert!(super::super::super::status_init::fits::<port::Writer>());
    assert!(super::super::super::status_init::fits::<
        super::super::native::Writer,
    >());
    mark_mixed("MI06", 0, 0);
}

#[test]
#[ignore = "final DEV3 S5: every additional resealed terminal byte, all profiles"]
fn mixed_init_s5_expanded_resealed_bytes() {
    for p in 1..=3 {
        for work in 1..=3 {
            input(p, work, |plan, _session| {
                let before = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.reserved");
                let allocator = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.after");
                let payload = fixed::<PAGE_BYTES>(p, "sequence", work, "payload.after");
                let bytes = raw(p, "sequence", work, "terminal.wal");
                for at in 0..bytes.len() - 32 {
                    let mut bad = bytes.clone();
                    bad[at] ^= 1;
                    rehash_frame(&mut bad);
                    assert!(
                        mixed::validate(&expectation(plan), &before, &bad, &allocator, &payload)
                            .is_err(),
                        "p{p} w{work} byte{at}"
                    );
                }
            });
        }
    }
}

#[test]
fn mixed_init_mi07_before_effect_stage_guards() {
    for work in 1..=3 {
        input(1, work, |plan, session| {
            let before = fixed::<PAGE_BYTES>(1, "sequence", work, "allocator.reserved");
            let mut output = boxed::<65536>(0xa5);
            let mut a = boxed::<PAGE_BYTES>(0xa5);
            let mut b = boxed::<PAGE_BYTES>(0xa5);
            // Actual session is pristine LSN0, while independently specified input
            // describes a reserved prefix. No reserve or INIT is appended here.
            let prepared = mixed::prepare(plan, &before, &mut output, &mut a, &mut b).unwrap();
            let error = prepared.stage_required(session).err().unwrap();
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(
                error.cause,
                wal::Cause::Frame(crate::storage::wal_frame::WalError::OrderMismatch)
            ));
            assert_eq!(session.prefix().unwrap().complete_lsn, 0);
            mark_mixed("MI07-prefix", 1, work);
            let mut other_slot = boxed::<{ wal::MAX_FRAME_BYTES }>(0);
            let (other, state) = undo_seed(&mut other_slot);
            let original = images(&state);
            let prepared = mixed::prepare(plan, &before, &mut output, &mut a, &mut b).unwrap();
            let error = prepared.stage_required(&other.wal).err().unwrap();
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(error.cause, wal::Cause::IdentityMismatch));
            assert_eq!(images(&state), original);
            assert!(state.lock().unwrap().events.is_empty());
            mark_mixed("MI07-fence", 1, work);
            other.wal.fenced.fence();
            let prepared = mixed::prepare(plan, &before, &mut output, &mut a, &mut b).unwrap();
            let error = prepared.stage_required(&other.wal).err().unwrap();
            // A pre-existing fenced session reports its cumulative unresolved
            // effect, even though this rejected call writes nothing itself.
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, wal::Cause::Fenced));
            assert_eq!(images(&state), original);
            assert!(state.lock().unwrap().events.is_empty());
            mark_mixed("MI07-health-first", 1, work);
        });
    }
}

#[test]
fn mixed_init_mi08_coherent_free_substitution() {
    let free6 = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dev3-undo-free-page/free-g6.bin"
    ))
    .unwrap();
    assert_eq!(free6.len(), 432);
    for p in 1..=3 {
        for work in 2..=3 {
            input(p, work, |plan, _session| {
                let before = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.reserved");
                let allocator = fixed::<PAGE_BYTES>(p, "sequence", work, "allocator.after");
                let mut payload = fixed::<PAGE_BYTES>(p, "sequence", work, "payload.after");
                let original = raw(p, "sequence", work, "terminal.wal");
                // Independent valid Free generation6 record. Every duplicate body and
                // digest is coherent; only the initial-root generation1 rule rejects.
                payload[15920..16352].copy_from_slice(&free6);
                let digest = content_digest(&payload[..16352]);
                payload[16352..].copy_from_slice(&digest);
                let mut bytes = original.clone();
                bytes[14816..15248].copy_from_slice(&free6);
                bytes[14744..14776].copy_from_slice(&content_digest(&free6));
                bytes[21792..38176].copy_from_slice(&payload[..]);
                rehash_frame(&mut bytes);
                assert_eq!(
                    mixed::validate(&expectation(plan), &before, &bytes, &allocator, &payload),
                    Err(alloc::SemanticError::PageMismatch)
                );
                let mut a = boxed::<PAGE_BYTES>(0xa5);
                let mut b = boxed::<PAGE_BYTES>(0xa5);
                assert_eq!(
                    mixed::materialize(&expectation(plan), &before, &bytes, &mut a, &mut b),
                    Err(alloc::SemanticError::PageMismatch)
                );
                mark_mixed("MI08-coherent-free6", p, work);
                let payload = fixed::<PAGE_BYTES>(p, "sequence", work, "payload.after");
                let mut bytes = original;
                bytes[14688] = 4;
                rehash_frame(&mut bytes);
                assert!(
                    mixed::validate(&expectation(plan), &before, &bytes, &allocator, &payload)
                        .is_err()
                );
                assert!(
                    mixed::materialize(&expectation(plan), &before, &bytes, &mut a, &mut b)
                        .is_err()
                );
                mark_mixed("MI08-native4", p, work);
            });
        }
    }
}
