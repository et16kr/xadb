//! Current empty producer histories, checked against independent raw literals.
use super::*;
const EMPTY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-empty-transactions/live"
);
fn empty_input(name: &str) -> Vec<u8> {
    read(EMPTY, name)
}
fn empty_fixture(depth: usize) -> Directory {
    let wal = empty_input("wal-through-abort.bin");
    assert_eq!(wal.len(), 4096 + 59200 + 3 * 3040);
    fixture(1, 2, &wal[4096..4096 + 59200 + depth * 3040])
}

#[test]
fn empty_recovery_three_literal_states_preserve_obligations() {
    for (depth, state) in [(1, "begin"), (2, "rollback"), (3, "abort")] {
        let d = empty_fixture(depth);
        let source = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        let mut maintenance = boxed();
        let mut flush = boxed();
        maintenance.fill(0xa5);
        flush.fill(0x5a);
        let owner =
            ClosedRecovery::acquire(&d, Some(port::namespace()), &mut maintenance, &mut flush)
                .unwrap();
        assert_eq!(owner.effects().mutation_attempted, 0);
        assert_eq!(owner.prefix().allocator_lsn(), 2);
        assert_eq!(owner.prefix().complete_lsn(), (2 + depth) as u64);
        let result = owner.recover().unwrap();
        assert_eq!(
            result.status_image_at(1).unwrap().as_slice(),
            empty_input(&format!("{state}.after.page"))
        );
        assert_eq!(
            result.allocator_image().as_slice(),
            status_vector(1, 0, "init", 1, "allocator.after")
        );
        assert_eq!(
            result.descriptor_bytes().unwrap().as_slice(),
            status_vector(1, 0, "init", 1, "init.descriptor")
        );
        assert_eq!(result.prefix().transaction_high_water(), 1);
        assert_eq!(result.prefix().active_transactions(), u32::from(depth != 3));
        let remaining = result.observation().ledger.remaining;
        assert_eq!(remaining.wal, (3 - depth) as u64 * 3040);
        assert_eq!(remaining.protected, remaining.wal);
        assert_eq!(remaining.peak.frames, if depth == 3 { 0 } else { 2 });
        assert_eq!(remaining.peak.memory, if depth == 3 { 0 } else { 114688 });
        assert_eq!(remaining.peak.files, if depth == 3 { 0 } else { 5 });
        assert_eq!(
            result.prefix().replay_page_topup_bytes(),
            2 * PAGE_BYTES as u64
        );
        if depth != 3 {
            assert_eq!(
                result.transaction_descriptor_at(0).unwrap().as_slice(),
                empty_input(&format!("{state}.descriptor"))
            );
        } else {
            assert!(result.transaction_descriptor_at(0).is_none());
        }
        for at in 1..33 {
            assert!(result.transaction_descriptor_at(at).is_none());
        }
        let physical = &result.observation().physical;
        assert_eq!(
            &physical.final_lsn[..2],
            &[Some(2), Some((2 + depth) as u64)]
        );
        assert_eq!(&physical.applied[..2], &[2, 1 + depth as u64]);
        assert_eq!(physical.dirty_mask, 3);
        assert!(result.observation().ledger.restored_once);
        assert_eq!(result.observation().physical.canonical_passes, 1);
        drop(result);
        assert_eq!(d.0.lock().unwrap().files[wal::WAL_NAME].bytes, source);
        let owner =
            ClosedRecovery::acquire(&d, Some(port::namespace()), &mut maintenance, &mut flush)
                .unwrap();
        let result = owner.recover().unwrap();
        assert_eq!(result.observation().physical.dirty_mask, 0);
        assert_eq!(result.effects().home_write_attempted, 0);
        assert_eq!(result.effects().slot_chunks_done, [16, 16]);
        assert_eq!(result.prefix().remaining_wal(), (3 - depth) as u64 * 3040);
        drop(result);
        let s = d.0.lock().unwrap();
        assert_eq!((s.handles, s.writers), (0, 0));
        assert_eq!(s.files[wal::WAL_NAME].bytes, source);
        println!("EMPTY_RECOVERY_LITERAL_PASS depth={depth}");
    }
}

const RAW: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-empty-transactions/recovery"
);
fn raw(name: &str) -> Vec<u8> {
    read(RAW, name)
}
fn interleaved(steps: usize) -> Directory {
    let mut frames = read(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/dev3-operational-status/p1"
        ),
        "wal.frames-only",
    );
    frames.extend_from_slice(&raw("interleaved/transaction.frames-only")[..steps * 3040]);
    let d = fixture(1, 2, &frames);
    d.0.lock()
        .unwrap()
        .files
        .get_mut(HOME_NAME)
        .unwrap()
        .bytes
        .resize(3 * PAGE_BYTES, 0);
    d
}
fn finish_empty(d: &Directory, check: impl FnOnce(&RecoveryInspection<'_, '_, port::Writer>)) {
    let mut m = boxed();
    let mut f = boxed();
    m.fill(0xa5);
    f.fill(0x5a);
    let owner = ClosedRecovery::acquire(d, Some(port::namespace()), &mut m, &mut f).unwrap();
    let result = owner.recover().unwrap();
    check(&result);
    assert!(result.observation().ledger.restored_once);
    drop(result);
    let s = d.0.lock().unwrap();
    assert_eq!((s.handles, s.writers), (0, 0));
}

#[test]
fn empty_recovery_interleaved_gap_and_distinct_page_tips() {
    for steps in [3, 5] {
        let d = interleaved(steps);
        finish_empty(&d, |r| {
            assert_eq!(
                (r.prefix().allocator_lsn(), r.prefix().complete_lsn()),
                (6, 6 + steps as u64)
            );
            assert_eq!(
                (
                    r.prefix().transaction_high_water(),
                    r.prefix().active_transactions()
                ),
                (5, if steps == 3 { 2 } else { 1 })
            );
            assert_eq!(
                r.prefix().remaining_wal(),
                if steps == 3 { 9120 } else { 3040 }
            );
            assert_eq!(
                r.status_image_at(1).unwrap().as_slice(),
                raw(if steps == 3 {
                    "interleaved/step3.page"
                } else {
                    "interleaved/step4.page"
                })
            );
            assert_eq!(
                r.status_image_at(2).unwrap().as_slice(),
                raw(if steps == 3 {
                    "interleaved/step2.page"
                } else {
                    "interleaved/step5.page"
                })
            );
            if steps == 3 {
                assert_eq!(
                    r.transaction_descriptor_at(0).unwrap().as_slice(),
                    raw("interleaved/step3.descriptor")
                );
            } else {
                assert!(r.transaction_descriptor_at(0).is_none());
            }
            assert_eq!(
                r.transaction_descriptor_at(1).unwrap().as_slice(),
                raw(if steps == 3 {
                    "interleaved/step2.descriptor"
                } else {
                    "interleaved/step5.descriptor"
                })
            );
            assert_eq!(
                &r.observation().physical.final_lsn[..3],
                &[
                    Some(6),
                    Some(if steps == 3 { 9 } else { 10 }),
                    Some(if steps == 3 { 8 } else { 11 })
                ]
            );
            assert_eq!(
                &r.observation().physical.applied[..3],
                if steps == 3 { &[6, 3, 2] } else { &[6, 4, 3] }
            );
            assert_eq!(r.prefix().remaining_memory(), 114688);
        });
        println!("EMPTY_RECOVERY_INTERLEAVED_PASS steps={steps}");
    }
}

#[test]
fn empty_recovery_32_owner_limit_and_terminal_releases_for_page63() {
    let base = read(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/dev3-operational-recovery/p1-extent"
        ),
        "wal.frames-only",
    );
    for variant in 0..3 {
        let mut frames = base.clone();
        let steps = if variant == 2 { 35 } else { 32 };
        frames.extend_from_slice(&raw("capacity/transaction.frames-only")[..steps * 3040]);
        if variant == 1 {
            frames.extend_from_slice(&raw("capacity/rejected33.wal"));
        }
        let d = fixture(1, 2, &frames);
        d.0.lock()
            .unwrap()
            .files
            .get_mut(HOME_NAME)
            .unwrap()
            .bytes
            .resize(TARGET_COUNT * PAGE_BYTES, 0);
        if variant == 1 {
            let before = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
            let mut m = boxed();
            let mut f = boxed();
            let error = ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f)
                .err()
                .unwrap();
            assert!(matches!(
                error.error.cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    phase: Phase::Ledger,
                    cause: Cause::Capacity,
                    ..
                })
            ));
            assert_eq!(error.effects.mutation_attempted, 0);
            let s = d.0.lock().unwrap();
            assert_eq!(s.files[wal::WAL_NAME].bytes, before);
            assert!(s.events.is_empty());
            println!("EMPTY_RECOVERY_CAPACITY_PASS variant={variant}");
            continue;
        }
        finish_empty(&d, |r| {
            assert_eq!(r.prefix().active_transactions(), 32);
            assert_eq!(r.prefix().remaining_wal(), 32 * 6080);
            assert_eq!(r.prefix().remaining_frames(), 2);
            assert_eq!(r.prefix().remaining_files(), 5);
            assert_eq!(r.prefix().remaining_memory(), 114688);
            assert_eq!(
                r.prefix().remaining_protected() + r.prefix().replay_page_topup_bytes(),
                1_243_136
            );
            assert_eq!(r.prefix().allocator_lsn(), 250);
            assert_eq!(r.status_image(), r.status_image_at(63));
            assert_eq!(
                r.prefix().transaction_high_water(),
                if variant == 2 { 34 } else { 32 }
            );
            if variant == 2 {
                assert_eq!(
                    r.status_image_at(63).unwrap().as_slice(),
                    raw("capacity/step35.page")
                );
                assert_eq!(
                    r.status_image_at(1).unwrap().as_slice(),
                    raw("capacity/step34.page")
                );
                assert_eq!(
                    r.transaction_descriptor_at(0).unwrap().as_slice(),
                    raw("capacity/step35.descriptor")
                );
            }
            assert_eq!(r.observation().physical.dirty_mask, u64::MAX);
        });
        println!("EMPTY_RECOVERY_CAPACITY_PASS variant={variant}");
    }
}

#[test]
fn empty_recovery_mixed_homes_and_torn_status() {
    for allocator_latest in [false, true] {
        for home in 0..6 {
            let d = empty_fixture(3);
            if allocator_latest {
                set_home(&d, &status_vector(1, 0, "init", 1, "allocator.after"), 0);
            }
            if home > 0 {
                let mut bytes = match home {
                    1 => empty_input("begin.before.page"),
                    2 => empty_input("begin.after.page"),
                    3 => empty_input("rollback.after.page"),
                    _ => empty_input("abort.after.page"),
                };
                if home == 5 {
                    bytes[140] ^= 1;
                }
                set_home(&d, &bytes, 1);
            }
            finish_empty(&d, |r| {
                let physical = &r.observation().physical;
                assert_eq!(&physical.final_lsn[..2], &[Some(2), Some(5)]);
                assert_eq!(physical.applied[0], if allocator_latest { 0 } else { 2 });
                assert_eq!(
                    physical.applied[1],
                    match home {
                        0 | 5 => 4,
                        1 => 3,
                        2 => 2,
                        3 => 1,
                        4 => 0,
                        _ => unreachable!(),
                    }
                );
                assert_eq!(
                    r.status_image_at(1).unwrap().as_slice(),
                    empty_input("abort.after.page")
                );
                assert_eq!(r.prefix().remaining_wal(), 0);
            });
        }
    }
}

fn reseal_frame(frame: &mut [u8]) {
    let header = content_digest(&frame[..128]);
    frame[128..160].copy_from_slice(&header);
    let end = frame.len() - 32;
    let seal = content_digest(&frame[..end]);
    frame[end..].copy_from_slice(&seal);
}

#[test]
fn empty_recovery_rejects_complete_semantic_corruption() {
    // Complete frames with valid outer seals must still satisfy each semantic
    // binding. The latest page2 frame does not hide corrupted page1 history.
    mark("ERW", 24, |n| {
        let d = interleaved(5);
        let mut source = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        let base = source.len() - 5 * 3040;
        let step = if n < 12 { 0 } else { 2 };
        let frame = &mut source[base + step * 3040..base + (step + 1) * 3040];
        let fields = [
            166, 200, 240, 248, 358, 960, 176, 2148, 2168, 2272, 2736, 2768,
        ];
        frame[fields[n % 12]] ^= 1;
        // Descriptor corruption has its own valid seal, independently of the
        // enclosing frame; this probes owner/budget semantics, not merely SHA.
        if matches!(n % 12, 4 | 5) {
            let seal = content_digest(&frame[352..1856]);
            frame[1856..1888].copy_from_slice(&seal);
        }
        reseal_frame(frame);
        install(&d, wal::WAL_NAME, source.clone(), source.len() as u64);
        let e = acquire_error(&d);
        assert_eq!(e.error.phase, RecoveryPhase::Prefix);
        assert_eq!(e.effects.mutation_attempted, 0);
        let s = d.0.lock().unwrap();
        assert_eq!(s.files[wal::WAL_NAME].bytes, source);
        assert!(s.events.is_empty());
        assert_eq!((s.handles, s.writers), (0, 0));
    });
}

#[test]
fn empty_recovery_rejects_reordered_retired_and_post_transaction_system() {
    mark("ERO", 5, |n| {
        let d = interleaved(5);
        let mut source = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        let base = source.len() - 5 * 3040;
        let parts: Vec<_> = source[base..]
            .chunks_exact(3040)
            .map(<[u8]>::to_vec)
            .collect();
        source.truncate(base);
        let order: &[usize] = match n {
            0 => &[1, 0],          // issuer5 before issuer2 (and original global chain)
            1 => &[0, 0],          // duplicate Begin
            2 => &[0, 1, 2, 3, 2], // continuation after terminal retirement
            3 => &[0, 1, 3],       // AbortEnd without RollbackStart
            4 => &[0],
            _ => unreachable!(),
        };
        for (i, part) in order.iter().enumerate() {
            let mut frame = parts[*part].clone();
            // Repair only the global envelope so the semantic owner/issuer
            // chain, rather than an outer previous-LSN mismatch, rejects it.
            let lsn = 7 + i as u64;
            frame[56..64].copy_from_slice(&lsn.to_le_bytes());
            frame[64..72].copy_from_slice(&(lsn - 1).to_le_bytes());
            if *part < 2 {
                // Rebase the complete independent Begin, including its birth
                // descriptor, so issuer reversal reaches the issuer guard.
                frame[176..184].copy_from_slice(&lsn.to_le_bytes());
                for at in [424, 440, 896, 1688] {
                    frame[at..at + 8].copy_from_slice(&lsn.to_le_bytes());
                }
                let seal = content_digest(&frame[352..1856]);
                frame[1856..1888].copy_from_slice(&seal);
            }
            reseal_frame(&mut frame);
            source.extend_from_slice(&frame);
        }
        if n == 4 {
            let mut system = status_vector(1, 0, "init", 1, "reserve.wal");
            system[56..64].copy_from_slice(&8u64.to_le_bytes());
            system[64..72].copy_from_slice(&7u64.to_le_bytes());
            reseal_frame(&mut system);
            source.extend_from_slice(&system);
        }
        install(&d, wal::WAL_NAME, source.clone(), source.len() as u64);
        let e = acquire_error(&d);
        assert_eq!(e.error.phase, RecoveryPhase::Prefix);
        assert_eq!(e.effects.mutation_attempted, 0);
        let state = d.0.lock().unwrap();
        assert_eq!(state.files[wal::WAL_NAME].bytes, source);
        if n == 0 {
            assert!(
                state
                    .read_events
                    .iter()
                    .any(|event| event.name == wal::WAL_NAME
                        && event.offset == (base + 3040 + 160) as u64
                        && event.length == 2880)
            );
        }
    });
}

#[test]
fn empty_recovery_historical_pending_and_later_home() {
    mark("ERD", 6, |n| {
        let d = empty_fixture(3);
        let pending = empty_input(if n < 3 {
            "begin.after.page"
        } else {
            "rollback.after.page"
        });
        let home = n % 3;
        if home == 1 {
            set_home(&d, &pending, 1);
        } else if home == 2 {
            set_home(&d, &empty_input("abort.after.page"), 1);
        }
        install_pending(
            &d,
            1,
            &pending,
            PageKind::TransactionStatus,
            if n < 3 { 3 } else { 4 },
            1,
            ControlState::Pending,
        );
        if home == 2 {
            recovered_error(&d, |f| {
                assert_eq!(f.error().phase, RecoveryPhase::PendingValidate);
                assert_eq!(f.effects().home_write_attempted, 0);
                assert_eq!(f.effects().dwb_publish_attempted, 0);
            });
            assert_eq!(
                &d.0.lock().unwrap().files[HOME_NAME].bytes[PAGE_BYTES..2 * PAGE_BYTES],
                empty_input("abort.after.page")
            );
            return;
        }
        finish_empty(&d, |r| {
            assert_eq!(
                r.status_image_at(1).unwrap().as_slice(),
                empty_input("abort.after.page")
            );
            assert!(r.observation().physical.pending_retired);
            assert_eq!(r.observation().physical.pending_repaired, home == 0);
            assert_eq!(r.observation().physical.pending_home_skipped, home != 0);
            assert_eq!(r.prefix().remaining_wal(), 0);
            assert_eq!(r.prefix().transaction_high_water(), 1);
        });
    });
}

#[test]
fn empty_recovery_rejects_intact_home_conflicts() {
    mark("ERH", 9, |n| {
        let d = empty_fixture(3);
        let index = usize::from(n != 0 && n != 8);
        let mut home = if index == 0 {
            status_vector(1, 0, "init", 1, "allocator.after")
        } else {
            empty_input(if n == 1 {
                "abort.after.page"
            } else {
                "begin.after.page"
            })
        };
        match n {
            0 => home[64..72].copy_from_slice(&3u64.to_le_bytes()), // allocator2/global5
            1 => home[64..72].copy_from_slice(&6u64.to_le_bytes()),
            2 => home[16200..16208].copy_from_slice(&2u64.to_le_bytes()), // different actor local
            3 => home[200] ^= 1, // typed free-gap corruption outside the Status cell
            4 => home[40..48].copy_from_slice(&3u64.to_le_bytes()),
            5 => home[48..56].copy_from_slice(&3u64.to_le_bytes()),
            6 => home[64..72].copy_from_slice(&4u64.to_le_bytes()), // Active at Rollback's LSN
            7 => {
                home[16166] = 3;
                home[16216..16224].copy_from_slice(&1u64.to_le_bytes());
            } // valid Commit, unsupported empty lifecycle
            8 => home[100] ^= 1, // torn allocator cannot be reconstructed by patches
            _ => unreachable!(),
        }
        if n != 8 {
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
fn empty_recovery_incomplete_suffix_retains_last_complete_owner() {
    mark("ERT", 15, |n| {
        let depth = n / 5;
        let cut = [1, 127, 159, 160, 3039][n % 5];
        let d = empty_fixture(depth);
        let wal = empty_input("wal-through-abort.bin");
        let at = 4096 + 59200 + depth * 3040;
        let source = wal[..at + cut].to_vec();
        install(&d, wal::WAL_NAME, source.clone(), source.len() as u64);
        finish_empty(&d, |r| {
            assert_eq!(r.prefix().complete_lsn(), 2 + depth as u64);
            assert_eq!(r.prefix().transaction_high_water(), u64::from(depth != 0));
            assert_eq!(
                r.prefix().remaining_wal(),
                if depth == 0 {
                    0
                } else {
                    (3 - depth) as u64 * 3040
                }
            );
            assert_eq!(
                r.status_image_at(1).unwrap().as_slice(),
                empty_input(match depth {
                    0 => "begin.before.page",
                    1 => "begin.after.page",
                    _ => "rollback.after.page",
                })
            );
        });
        assert_eq!(d.0.lock().unwrap().files[wal::WAL_NAME].bytes, source);
    });
}

#[test]
fn empty_recovery_pending_requires_exact_complete_transaction() {
    mark("ERP", 3, |n| {
        let d = empty_fixture(if n == 0 { 1 } else { 3 });
        let mut pending = empty_input("rollback.after.page");
        if n == 1 {
            pending[16200] ^= 1;
            seal_page(&mut pending);
        }
        if n == 2 {
            pending[16166] = 1; // valid Active home at the RollbackStart LSN
            pending[16167] = 0;
            seal_page(&mut pending);
        }
        install_pending(
            &d,
            1,
            &pending,
            PageKind::TransactionStatus,
            4,
            1,
            ControlState::Pending,
        );
        let e = acquire_error(&d);
        assert_eq!(e.effects.mutation_attempted, 0);
        assert_eq!(e.error.phase, RecoveryPhase::Prefix);
    });
}

#[test]
fn empty_recovery_required_transaction_body_read_failures() {
    mark("ERF", 8, |n| {
        let d = empty_fixture(1);
        let source = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        let action = [
            port::Action::BeforeError,
            port::Action::Zero,
            port::Action::InvalidCount,
            port::Action::PrefixEof,
        ][n % 4];
        let plan = (wal::WAL_NAME, 4096 + 59200 + 160, 2880, action);
        if n < 4 {
            d.0.lock().unwrap().named_read_plan = Some(plan);
            let e = acquire_error(&d);
            assert_eq!(e.error.phase, RecoveryPhase::Prefix);
            assert!(matches!(
                e.error.cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    phase: Phase::Scan,
                    cause: Cause::Io(_),
                    ..
                })
            ));
            assert_eq!(e.effects.mutation_attempted, 0);
        } else {
            let mut m = boxed();
            let mut f = boxed();
            let owner = ClosedRecovery::acquire(&d, None, &mut m, &mut f).unwrap();
            d.0.lock().unwrap().named_read_plan = Some(plan);
            let e = owner
                .recover()
                .err()
                .expect("frozen complete body read must fail");
            assert_eq!(e.error().phase, RecoveryPhase::Redo);
            assert!(matches!(
                e.error().cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    phase: Phase::Scan,
                    cause: Cause::Io(_),
                    ..
                })
            ));
            assert_eq!(e.effects().home_write_attempted, 0);
            assert_eq!(e.effects().dwb_publish_attempted, 0);
        }
        let s = d.0.lock().unwrap();
        assert!(s.named_read_plan.is_none());
        assert_eq!(s.configured_read_hits, 1);
        assert_eq!(s.continuation_read_hits, usize::from(n % 4 == 3));
        assert_eq!(s.files[wal::WAL_NAME].bytes, source);
        assert_eq!((s.handles, s.writers), (0, 0));
    });
}

#[test]
fn empty_recovery_bidirectional_bank_census_rejects_drift() {
    mark("ERB", 9, |n| {
        let d = interleaved(3);
        let mut m = boxed();
        let mut f = boxed();
        let mut owner = ClosedRecovery::acquire(&d, None, &mut m, &mut f).unwrap();
        let identity = super::super::physical::identity_of(
            owner.prefix.allocator_image(),
            port::namespace().database,
        )
        .unwrap();
        let arena = owner.prefix.transaction_arena.as_deref_mut().unwrap();
        let bank = transactions::PAGE_AREA;
        match n {
            0 => arena[bank..bank + 1664].fill(0), // canonical active home without row
            1 => arena.copy_within(bank..bank + 1664, bank + 2 * 1664), // duplicate owner
            2 => arena[bank + 1536 + 8..bank + 1536 + 16].copy_from_slice(&63u64.to_le_bytes()), // orphan home
            3 => arena[bank + 1625] = 1,        // noncanonical padding
            4 => arena[bank + 1664 + 1624] = 0, // absent row retains bytes
            5 => arena[bank + 1576 + 32] ^= 1,  // actor and descriptor disagree
            6 => owner.prefix.observation.transactions.high_water = 1,
            7 => owner.prefix.observation.transactions.active = 1,
            8 => owner.prefix.observation.remaining.wal += 1,
            _ => unreachable!(),
        }
        assert!(
            transactions::census(
                &owner.prefix.observation,
                identity,
                arena,
                &owner.prefix.workspace[TARGETS..TARGETS + TARGET_TABLE_BYTES]
            )
            .is_err()
        );
        assert_eq!(owner.effects().mutation_attempted, 0);
    });
}

#[test]
fn empty_recovery_second_pass_seal_rejects_drift() {
    mark("ERS", 5, |n| {
        let d = interleaved(3);
        let mut m = boxed();
        let mut f = boxed();
        let mut owner = ClosedRecovery::acquire(&d, None, &mut m, &mut f).unwrap();
        match n {
            0 => owner.final_transaction_hash[0] ^= 1,
            1 => {
                owner.prefix.transaction_arena.as_deref_mut().unwrap()
                    [transactions::PAGE_AREA + 10] ^= 1
            }
            2 => owner.prefix.observation.transactions.high_water += 1,
            3 => owner.prefix.observation.allocator_lsn += 1,
            4 => owner.maintenance_tail[SAVED_TARGETS + TARGET_BYTES + 16] ^= 1,
            _ => unreachable!(),
        }
        let failure = owner
            .recover()
            .err()
            .expect("stale seal/census must reject");
        assert_eq!(failure.error().effect, Effect::CallerMustResolve);
        assert_eq!(
            failure.error().phase,
            if n < 2 {
                RecoveryPhase::ReserveRestore
            } else {
                RecoveryPhase::Redo
            }
        );
        assert!(if n < 2 {
            matches!(failure.error().cause, RecoveryCause::IdentityMismatch)
        } else {
            matches!(failure.error().cause, RecoveryCause::Invariant)
        });
        assert_eq!(failure.effects().home_write_attempted, 0);
        assert_eq!(failure.effects().dwb_publish_attempted, 0);
    });
}

#[test]
#[ignore = "DEV3 S5 only: three profiles, four transaction states, four physical home combinations"]
fn empty_recovery_s5_profiles_states_and_homes() {
    use crate::storage::{dirty_frame::DirtyFrame, wal_io::FreshStorageSession};
    use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
    for profile in 1..=3 {
        for depth in 0..=3 {
            for homes in 0..4 {
                let (writer, state) = port::fresh();
                let mut slot = boxed::<1048576>();
                let mut workspace = boxed::<4194304>();
                let mut operation = boxed::<114688>();
                let mut bank = boxed::<{ crate::storage::wal_io::EMPTY_TRANSACTION_BANK_BYTES }>();
                let mut a = boxed::<PAGE_BYTES>();
                let mut s = boxed::<PAGE_BYTES>();
                let fresh = FreshStorageSession::create_status(
                    writer,
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
                let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
                let binding = storage
                    .bind_resident_status(&core, &mut frames, &mut operation)
                    .unwrap();
                let mut bound = match binding.activate() {
                    Ok(v) => v,
                    Err(_) => panic!("S5 activation"),
                };
                let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
                let page = match bound
                    .split_with_flush()
                    .0
                    .reserve_one(&mut owner)
                    .unwrap()
                    .initialize()
                {
                    Ok(v) => v.page,
                    Err(_) => panic!("S5 INIT"),
                };
                let mut live = match bound.into_empty_transactions(&mut owner, &mut bank) {
                    Ok(v) => v,
                    Err(_) => panic!("S5 handoff"),
                };
                let actor = (depth > 0).then(|| live.begin(&mut owner, page).unwrap().actor);
                if depth >= 2 {
                    live.rollback_start(&mut owner, actor.unwrap()).unwrap();
                }
                if depth == 3 {
                    live.abort_end(&mut owner, actor.unwrap()).unwrap();
                }
                drop(live);
                drop(owner);
                drop(storage);
                let d = Directory(state);
                // Current profile changes reservation identity, not these
                // namespace/page/state bytes. Raw expected images are external.
                let final_status = empty_input(
                    [
                        "begin.before.page",
                        "begin.after.page",
                        "rollback.after.page",
                        "abort.after.page",
                    ][depth],
                );
                let final_allocator = status_vector(1, 0, "init", 1, "allocator.after");
                if homes & 1 != 0 {
                    set_home(&d, &final_allocator, 0);
                }
                if homes & 2 != 0 {
                    set_home(&d, &final_status, 1);
                }
                finish_empty(&d, |r| {
                    assert_eq!(r.allocator_image().as_slice(), final_allocator);
                    assert_eq!(r.status_image_at(1).unwrap().as_slice(), final_status);
                    assert_eq!(
                        r.prefix().remaining_wal(),
                        if depth == 0 {
                            0
                        } else {
                            (3 - depth) as u64 * 3040
                        }
                    );
                    assert_eq!(r.observation().physical.dirty_mask, 3 ^ homes);
                });
                println!("EMPTY_RECOVERY_S5_PASS profile={profile} depth={depth} homes={homes}");
            }
        }
    }
}
