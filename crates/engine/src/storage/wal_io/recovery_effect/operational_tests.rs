//! Repeated current-build grammar: independent histories and page-local boundaries.
use super::*;
const REPEATED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-operational-status"
);
const EXTENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-operational-recovery/p1-extent"
);
fn input(name: &str) -> Vec<u8> {
    read(REPEATED, &format!("p1/{name}"))
}
fn repeated(frames: &[u8]) -> Directory {
    let d = fixture(1, 2, frames);
    d.0.lock()
        .unwrap()
        .files
        .get_mut(HOME_NAME)
        .unwrap()
        .bytes
        .resize(TARGET_COUNT * PAGE_BYTES, 0);
    d
}
fn complete() -> Directory {
    repeated(&input("wal.frames-only"))
}
fn with_recovery(d: &Directory, check: impl FnOnce(&RecoveryInspection<'_, '_, port::Writer>)) {
    let mut m = boxed();
    let mut f = boxed();
    // Neither canonical pass can rely on the caller's prior arena contents.
    m.fill(0xa5);
    f.fill(0x5a);
    let owner = ClosedRecovery::acquire(d, Some(port::namespace()), &mut m, &mut f).unwrap();
    assert_eq!(owner.effects().mutation_attempted, 0);
    let result = owner.recover().unwrap();
    check(&result);
    assert!(result.observation().ledger.restored_once);
    assert!(result.observation().source_census.peak <= 65536);
    assert_eq!(result.observation().fd_peak, 5);
    drop(result);
    let state = d.0.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
}
fn check_complete(r: &RecoveryInspection<'_, '_, port::Writer>) {
    assert_eq!(
        r.allocator_image().as_slice(),
        input("work3-allocator.after")
    );
    assert_eq!(
        r.status_image_at(1).unwrap().as_slice(),
        input("work1-status.after")
    );
    assert_eq!(
        r.status_image_at(2).unwrap().as_slice(),
        input("work3-status.after")
    );
    assert_eq!(r.status_image(), r.status_image_at(2));
    for absent in [0, 3, 63, 64, u64::MAX] {
        assert!(r.status_image_at(absent).is_none());
    }
    assert_eq!(
        r.descriptor_bytes().unwrap().as_slice(),
        input("work3-terminal.descriptor")
    );
    assert_eq!(r.prefix().status_page_mask(), 6);
    assert_eq!(
        (
            r.prefix().complete_lsn(),
            r.prefix().work_high_water(),
            r.prefix().active_mask()
        ),
        (6, 3, 0)
    );
    assert_eq!(r.prefix().replay_page_topup_bytes(), 3 * PAGE_BYTES as u64);
    assert_eq!(
        &r.observation().physical.final_lsn[..3],
        &[Some(6), Some(2), Some(6)]
    );
    assert!(
        r.observation().physical.final_lsn[3..]
            .iter()
            .all(Option::is_none)
    );
}
#[test]
fn operational_recovery_resource_gate() {
    let c = resources::source_census::<port::Writer>();
    println!(
        "operational recovery census outer={} prefix={} work={} inventory={} peak={}",
        c.outer, c.prefix_nested, c.work_nested, c.inventory, c.peak
    );
    println!(
        "lifetime phases: borrowed_outer={} entry={} work={} return={} peak={}",
        c.outer,
        c.entry_phase,
        c.outer + c.work_nested.max(c.inventory),
        c.return_phase,
        c.peak
    );
    assert!(resources::fits::<port::Writer>(&c));
    assert_eq!(TARGETS, 181760);
    const { assert!(TARGETS + TARGET_TABLE_BYTES <= WORKSPACE_BYTES) };
    const { assert!(WORKSPACE_BYTES + SAVED_TARGETS + TARGET_TABLE_BYTES <= MAINTENANCE_BYTES) };
    assert_eq!(size_of::<PhysicalObservation>(), 3104);
}
#[test]
fn operational_recovery_mixed_targets() {
    mark("ORM", 9, |n| {
        let d = complete();
        let mask = if n == 8 { 0 } else { n as u64 };
        if mask & 1 != 0 {
            set_home(&d, &input("work3-allocator.after"), 0);
        }
        if mask & 2 != 0 {
            set_home(&d, &input("work1-status.after"), 1);
        }
        if mask & 4 != 0 {
            set_home(&d, &input("work3-status.after"), 2);
        }
        if n == 8 {
            let mut torn = input("work3-status.after");
            torn[100] ^= 1;
            set_home(&d, &torn, 2);
        }
        {
            let mut s = d.0.lock().unwrap();
            s.files.get_mut(HOME_NAME).unwrap().bytes[3 * PAGE_BYTES..].fill(0x6d);
        }
        let wal_before = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        with_recovery(&d, |r| {
            check_complete(r);
            let p = &r.observation().physical;
            assert_eq!((p.dirty_mask, p.flushed_mask), (7 ^ mask, 7 ^ mask));
            assert_eq!(
                &p.applied[..3],
                &[
                    if mask & 1 == 0 { 6 } else { 0 },
                    u64::from(mask & 2 == 0),
                    u64::from(mask & 4 == 0)
                ]
            );
            assert_eq!(
                &p.skipped[..3],
                &[
                    if mask & 1 == 0 { 0 } else { 6 },
                    u64::from(mask & 2 != 0),
                    u64::from(mask & 4 != 0)
                ]
            );
            assert_eq!(r.observation().batch, (7 ^ mask).count_ones() as u64);
        });
        let s = d.0.lock().unwrap();
        assert_eq!(s.files[wal::WAL_NAME].bytes, wal_before);
        assert!(
            s.files[HOME_NAME].bytes[3 * PAGE_BYTES..]
                .iter()
                .all(|v| *v == 0x6d)
        );
    });
}
#[test]
fn operational_recovery_complete_and_partial_prefixes() {
    mark("ORP", 20, |n| {
        let parts: Vec<_> = (1..=3)
            .flat_map(|w| {
                [
                    input(&format!("work{w}-reserve.wal")),
                    input(&format!("work{w}-terminal.wal")),
                ]
            })
            .collect();
        let (lsn, tail) = if n < 4 {
            (n + 3, None)
        } else {
            let lsn = 2 + (n - 4) / 4;
            let cuts = [1, 159, 160, parts[lsn].len() - 1];
            (lsn, Some(cuts[(n - 4) % 4]))
        };
        let mut frames = parts[..lsn].concat();
        if let Some(tail) = tail {
            frames.extend_from_slice(&parts[lsn][..tail]);
        }
        let d = repeated(&frames);
        let work = lsn.div_ceil(2);
        let terminal = lsn % 2 == 0;
        with_recovery(&d, |r| {
            assert_eq!(r.prefix().complete_lsn(), lsn as u64);
            assert_eq!(
                r.prefix().tail().map(|t| t.physical_bytes()),
                tail.map(|v| v as u64)
            );
            assert_eq!(r.prefix().work_high_water(), work as u64);
            assert_eq!(
                r.allocator_image().as_slice(),
                input(&format!(
                    "work{work}-allocator.{}",
                    if terminal { "after" } else { "reserved" }
                ))
            );
            assert_eq!(
                r.descriptor_bytes().unwrap().as_slice(),
                input(&format!(
                    "work{work}-{}.descriptor",
                    if terminal { "terminal" } else { "reserve" }
                ))
            );
            assert_eq!(
                r.status_image_at(1).unwrap().as_slice(),
                input("work1-status.after")
            );
            assert_eq!(r.prefix().status_page_mask(), if lsn == 6 { 6 } else { 2 });
            assert_eq!(r.prefix().active_mask(), if terminal { 0 } else { 4 });
            assert_eq!(r.prefix().inventory_items(), u64::from(!terminal));
            assert_eq!(
                r.prefix().replay_page_topup_bytes(),
                (if !terminal {
                    1
                } else if lsn == 6 {
                    3
                } else {
                    2
                }) * PAGE_BYTES as u64
            );
            assert_eq!(r.observation().physical.applied[1], 1);
            assert_eq!(r.observation().physical.applied[2], u64::from(lsn == 6));
        });
        assert_eq!(
            &d.0.lock().unwrap().files[wal::WAL_NAME].bytes[4096..],
            frames
        );
    });
}
#[test]
fn operational_recovery_historical_pending() {
    mark("ORD", 6, |n| {
        let d = complete();
        let (index, image, lsn, kind) = match n {
            0 | 1 => (
                1,
                input("work1-status.after"),
                2,
                PageKind::TransactionStatus,
            ),
            2 | 3 => (
                2,
                input("work3-status.after"),
                6,
                PageKind::TransactionStatus,
            ),
            4 => (0, input("work2-allocator.after"), 4, PageKind::Allocation),
            5 => (0, input("work3-allocator.after"), 6, PageKind::Allocation),
            _ => unreachable!(),
        };
        if n == 1 || n == 3 {
            set_home(&d, &image, index);
        }
        install_pending(&d, 1, &image, kind, lsn, 1, ControlState::Pending);
        with_recovery(&d, |r| {
            check_complete(r);
            assert!(r.observation().physical.pending_retired);
            assert_eq!(r.observation().physical.pending_repaired, n != 1 && n != 3);
            assert_eq!(
                r.observation().physical.pending_home_skipped,
                n == 1 || n == 3
            );
            assert_eq!(r.effects().home_synced & (1 << index), 1 << index);
        });
    });
}
#[test]
fn operational_recovery_rejects_intact_conflicts() {
    mark("ORH", 7, |n| {
        let d = complete();
        let index = if n == 0 {
            1
        } else if n == 6 {
            0
        } else {
            2
        };
        let mut home = input(if index == 1 {
            "work1-status.after"
        } else if index == 0 {
            "work3-allocator.after"
        } else {
            "work3-status.after"
        });
        match n {
            0 => home[64..72].copy_from_slice(&3u64.to_le_bytes()), // below global end, above A's own end
            1 => home[64..72].copy_from_slice(&7u64.to_le_bytes()),
            2 => home[64..72].copy_from_slice(&5u64.to_le_bytes()), // INIT prior must be zero
            3 => home[40..48].copy_from_slice(&2u64.to_le_bytes()),
            4 => home[48..56].copy_from_slice(&3u64.to_le_bytes()),
            5 => home[200] ^= 1, // valid generic padding, wrong immutable full image
            6 => home[100] ^= 1, // Patch-only allocator is not reconstructable
            _ => unreachable!(),
        }
        if n != 6 {
            seal_page(&mut home);
        }
        set_home(&d, &home, index);
        recovered_error(&d, |f| {
            assert_eq!(f.error().phase, RecoveryPhase::Redo);
            assert_eq!(f.effects().home_write_attempted, 0);
            assert_eq!(f.effects().dwb_publish_attempted, 0);
        });
        assert_eq!(
            &d.0.lock().unwrap().files[HOME_NAME].bytes
                [index * PAGE_BYTES..(index + 1) * PAGE_BYTES],
            home
        );
    });
}
#[test]
fn operational_recovery_rejects_complete_wal_corruption() {
    mark("ORW", 8, |n| {
        let mut frames = input("wal.frames-only");
        let first = input("work1-reserve.wal").len() + input("work1-terminal.wal").len();
        let third = input("work2-reserve.wal").len();
        let at = first + if n >= 4 { third } else { 0 };
        let len = if n >= 4 {
            input("work2-terminal.wal").len()
        } else {
            third
        };
        // IDs/old descriptor hash/native shape/owner/remaining are all complete,
        // outer-checksummed input failures after a formerly terminal INIT.
        let field = [200, 248, 2144, 352 + 64, 200, 248, 2144, 352 + 64][n];
        frames[at + field] ^= 1;
        let sha = content_digest(&frames[at..at + len - 32]);
        frames[at + len - 32..at + len].copy_from_slice(&sha);
        let d = repeated(&frames);
        let error = acquire_error(&d);
        assert_eq!(error.error.phase, RecoveryPhase::Prefix);
        assert_eq!(error.effects.mutation_attempted, 0);
    });
}
#[test]
fn operational_recovery_readonly_scope_remains_closed() {
    let d = complete();
    let mut workspace = boxed();
    let error = match RecoveryPrefix::acquire(&d, Some(port::namespace()), &mut workspace) {
        Err(e) => e,
        Ok(_) => panic!("readonly scope widened"),
    };
    assert!(matches!(
        error.cause,
        super::super::super::Cause::Semantic(SemanticError::StateMismatch)
    ));
    assert_eq!(d.0.lock().unwrap().events.len(), 0);
}
fn check_extent(root: &str, profile: u32) {
    let frames = read(root, "wal.frames-only");
    let d = fixture(profile, 2, &frames);
    d.0.lock()
        .unwrap()
        .files
        .get_mut(HOME_NAME)
        .unwrap()
        .bytes
        .resize(TARGET_COUNT * PAGE_BYTES, 0);
    with_recovery(&d, |r| {
        assert_eq!(
            r.allocator_image().as_slice(),
            read(root, "allocator.after")
        );
        assert_eq!(
            r.descriptor_bytes().unwrap().as_slice(),
            read(root, "terminal.descriptor")
        );
        assert_eq!(r.prefix().status_page_mask(), u64::MAX - 1);
        assert_eq!(r.observation().physical.flushed_mask, u64::MAX);
        assert_eq!(r.effects().home_written, u64::MAX);
        assert_eq!(r.observation().batch, 64);
        assert_eq!(r.observation().known.generation, 193);
        assert_eq!(r.prefix().replay_page_topup_bytes(), 64 * PAGE_BYTES as u64);
        for index in 1..64 {
            assert_eq!(
                r.status_image_at(index).unwrap().as_slice(),
                read(root, &format!("status-{index}.after"))
            );
            assert_eq!(r.observation().physical.applied[index as usize], 1);
        }
    });
}
#[test]
fn operational_recovery_page63_and_full_extent_reservation() {
    check_extent(EXTENT, 1);
}
#[test]
fn operational_recovery_target_count_generation_overflow() {
    let d = complete();
    // Two targets would fit, three cannot issue the final retirement.
    let batch = (u64::MAX - 1) / 3 - 2;
    install_pending(
        &d,
        1,
        &input("work1-status.after"),
        PageKind::TransactionStatus,
        2,
        batch,
        ControlState::Empty,
    );
    let e = acquire_error(&d);
    assert!(matches!(e.error.cause, RecoveryCause::ArithmeticOverflow));
    assert_eq!(e.effects.mutation_attempted, 0);
}
#[test]
fn operational_recovery_second_pass_rejects_census_drift() {
    let d = complete();
    let mut m = boxed();
    let mut f = boxed();
    let owner = ClosedRecovery::acquire(&d, None, &mut m, &mut f).unwrap();
    owner.maintenance_tail[SAVED_TARGETS + TARGET_BYTES + 16] ^= 1;
    let failure = owner.recover().err().unwrap();
    assert!(matches!(failure.error().cause, RecoveryCause::Invariant));
    assert_eq!(failure.effects().home_write_attempted, 0);
}
#[test]
#[ignore = "S5 only: generate profile2/3 first-extent literal directories, then select explicitly"]
fn operational_recovery_s5_profiles() {
    let root = std::env::var("XADB_OPERATIONAL_RECOVERY_INPUT").expect("S5 prepared literal root");
    for profile in [2, 3] {
        check_extent(&format!("{root}/p{profile}-extent"), profile);
    }
}
#[test]
#[ignore = "S5 only: independent page2 history with 4096 cancellations"]
fn operational_recovery_s5_long_history() {
    let root = std::env::var("XADB_OPERATIONAL_RECOVERY_INPUT").expect("S5 prepared literal root");
    let root = format!("{root}/p1-long");
    let d = repeated(&read(&root, "wal.frames-only"));
    with_recovery(&d, |r| {
        assert_eq!(r.prefix().work_high_water(), 4098);
        assert_eq!(r.prefix().complete_lsn(), 8196);
        assert_eq!(
            r.allocator_image().as_slice(),
            read(&root, "allocator.after")
        );
        for index in [1, 2] {
            assert_eq!(
                r.status_image_at(index).unwrap().as_slice(),
                read(&root, &format!("status-{index}.after"))
            );
        }
    });
}

#[test]
fn operational_recovery_retained_terminal_is_revalidated() {
    mark("ORV", 3, |n| {
        let d = complete();
        let mut m = boxed();
        let mut f = boxed();
        let mut owner = ClosedRecovery::acquire(&d, None, &mut m, &mut f).unwrap();
        let identity = super::super::physical::identity_of(
            owner.prefix.allocator_image(),
            port::namespace().database,
        )
        .unwrap();
        match n {
            0 => owner.prefix.observation.terminal = TerminalOutcome::Cancelled,
            1 => owner.prefix.observation.high_water += 1,
            2 => owner.prefix.workspace[DESCRIPTOR + 1535] ^= 1,
            _ => unreachable!(),
        }
        assert!(
            super::super::super::validate_operational_before(
                &owner.prefix.observation,
                identity,
                owner.prefix.allocator_image(),
                owner.prefix.descriptor_bytes().unwrap()
            )
            .is_err()
        );
        assert_eq!(owner.effects().mutation_attempted, 0);
    });
}

#[test]
fn operational_recovery_codec_valid_semantic_rejections() {
    mark("ORS", 8, |n| {
        if n < 5 {
            let d = complete();
            let mut m = boxed();
            let mut f = boxed();
            let owner = ClosedRecovery::acquire(&d, None, &mut m, &mut f).unwrap();
            let identity = super::super::physical::identity_of(
                owner.prefix.allocator_image(),
                port::namespace().database,
            )
            .unwrap();
            if n < 4 {
                let page = &mut owner.prefix.workspace[ALLOC..ALLOC + PAGE_BYTES];
                let entry = 10144 + 64 + 96;
                match n {
                    0 => page[entry + 16..entry + 24].copy_from_slice(&3u64.to_le_bytes()),
                    1 => page[entry + 1] = 8,
                    2 => page[entry + 24..entry + 32].copy_from_slice(&2u64.to_le_bytes()),
                    3 => page[entry + 8..entry + 16].copy_from_slice(&1u64.to_le_bytes()),
                    _ => unreachable!(),
                }
                seal_page(page);
                allocation_page::decode_allocation_page(page, identity, port::namespace()).unwrap();
            } else {
                let raw = &mut owner.prefix.workspace[DESCRIPTOR..DESCRIPTOR + 1536];
                raw[608..616].copy_from_slice(&59201u64.to_le_bytes());
                let digest = content_digest(&raw[..1504]);
                raw[1504..].copy_from_slice(&digest);
                let work = SystemWorkRef::new(
                    port::namespace(),
                    SystemWorkKind::AllocationMaintenance,
                    3,
                    Generation::new(1).unwrap(),
                )
                .unwrap();
                obligation::decode_descriptor(
                    raw,
                    ReservationOwner::System(work),
                    super::super::super::profile_ref(owner.prefix.observation.profile),
                )
                .unwrap();
            }
            assert!(
                super::super::super::validate_operational_before(
                    &owner.prefix.observation,
                    identity,
                    owner.prefix.allocator_image(),
                    owner.prefix.descriptor_bytes().unwrap()
                )
                .is_err()
            );
            assert_eq!(owner.effects().mutation_attempted, 0);
        } else if n == 5 {
            let mut frame = input("work3-terminal.wal");
            let mut page = input("work1-status.after");
            page[64..72].copy_from_slice(&6u64.to_le_bytes());
            seal_page(&mut page);
            frame[21528..21568].copy_from_slice(&page[24..64]);
            frame[21584..37968].copy_from_slice(&page);
            let digest = content_digest(&frame[..37968]);
            frame[37968..].copy_from_slice(&digest);
            let mut frames = input("wal.frames-only");
            let at = frames.len() - 38000;
            frames[at..].copy_from_slice(&frame);
            let d = repeated(&frames);
            let e = acquire_error(&d);
            assert!(matches!(
                e.error.cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    cause: super::super::super::Cause::Semantic(SemanticError::PageMismatch),
                    ..
                })
            ));
            assert_eq!(e.effects.mutation_attempted, 0);
        } else if n == 6 {
            let mut frames = input("wal.frames-only");
            frames.truncate(frames.len() - 19000);
            let d = repeated(&frames);
            install_pending(
                &d,
                1,
                &input("work3-status.after"),
                PageKind::TransactionStatus,
                6,
                1,
                ControlState::Pending,
            );
            let e = acquire_error(&d);
            assert!(matches!(
                e.error.cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    cause: super::super::super::Cause::RequiredPrefixMissing,
                    ..
                })
            ));
            assert_eq!(e.effects.mutation_attempted, 0);
        } else {
            let mut frames = input("wal.frames-only");
            let at = input("work1-reserve.wal").len() + input("work1-terminal.wal").len();
            let len = input("work2-reserve.wal").len();
            let raw = &mut frames[at + 352..at + 1888];
            for offset in [968, 1288] {
                raw[offset..offset + 8].copy_from_slice(&98304u64.to_le_bytes());
            }
            let digest = content_digest(&raw[..1504]);
            raw[1504..].copy_from_slice(&digest);
            let digest = content_digest(&frames[at..at + len - 32]);
            frames[at + len - 32..at + len].copy_from_slice(&digest);
            let d = repeated(&frames);
            let e = acquire_error(&d);
            assert!(matches!(
                e.error.cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    cause: super::super::super::Cause::Semantic(SemanticError::BudgetMismatch),
                    ..
                })
            ));
            assert_eq!(e.effects.mutation_attempted, 0);
        }
    });
}

#[test]
#[ignore = "S5 only: profiles1/2/3 x all eight physical home subsets x six selected Pending histories"]
fn operational_recovery_s5_home_pending_matrix() {
    mark("ORS5", 144, |n| {
        let profile = 1 + (n / 48) as u32;
        let pending = (n % 48) / 8;
        let mask = n % 8;
        let literal = |name: &str| read(REPEATED, &format!("p{profile}/{name}"));
        let d = fixture(profile, 2, &literal("wal.frames-only"));
        d.0.lock()
            .unwrap()
            .files
            .get_mut(HOME_NAME)
            .unwrap()
            .bytes
            .resize(TARGET_COUNT * PAGE_BYTES, 0);
        for (index, name) in [
            (0, "work3-allocator.after"),
            (1, "work1-status.after"),
            (2, "work3-status.after"),
        ] {
            if mask & (1 << index) != 0 {
                set_home(&d, &literal(name), index);
            }
        }
        if pending > 0 {
            let (name, kind, lsn) = match pending {
                1 => ("work1-status.after", PageKind::TransactionStatus, 2),
                2 => ("work3-status.after", PageKind::TransactionStatus, 6),
                3 => ("work1-allocator.after", PageKind::Allocation, 2),
                4 => ("work2-allocator.after", PageKind::Allocation, 4),
                5 => ("work3-allocator.after", PageKind::Allocation, 6),
                _ => unreachable!(),
            };
            install_pending(
                &d,
                profile,
                &literal(name),
                kind,
                lsn,
                1,
                ControlState::Pending,
            );
        }
        if (pending == 3 || pending == 4) && mask & 1 != 0 {
            recovered_error(&d, |failure| {
                assert_eq!(failure.error().phase, RecoveryPhase::PendingValidate);
                assert!(matches!(
                    failure.error().cause,
                    RecoveryCause::PriorMismatch
                ));
            });
        } else {
            with_recovery(&d, |r| {
                assert_eq!(
                    r.allocator_image().as_slice(),
                    literal("work3-allocator.after")
                );
                assert_eq!(
                    r.status_image_at(1).unwrap().as_slice(),
                    literal("work1-status.after")
                );
                assert_eq!(
                    r.status_image_at(2).unwrap().as_slice(),
                    literal("work3-status.after")
                );
            });
        }
    });
}

#[path = "operational_s5_tests.rs"]
mod expanded;
