//! Mixed recovery scenarios over independent raw inputs. This is a child of
//! recovery_effect::tests so the existing memory port can be reused unchanged.
use super::*;

const MIXED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-mixed-system-init"
);
const UNDO_BOOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-undo-bootstrap"
);
const GROUPS: [(&str, usize); 9] = [
    ("MR01", 1),
    ("MR02", 3),
    ("MR03", 9),
    ("MR04", 7),
    ("MR05", 10),
    ("MR06", 12),
    ("MR07", 11),
    ("MR08", 8),
    ("MR09", 3),
];

fn raw(profile: u32, path: &str) -> Vec<u8> {
    read(MIXED, &format!("p{profile}/{path}"))
}
fn sequence(profile: u32, work: usize, part: &str) -> Vec<u8> {
    raw(profile, &format!("sequence/work{work}-{part}"))
}
fn select(variable: &str) -> Option<BTreeSet<String>> {
    match std::env::var(variable) {
        Ok(value) => {
            assert!(!value.is_empty(), "empty {variable}");
            let mut ids = BTreeSet::new();
            for id in value.split(',') {
                assert!(GROUPS.iter().any(|(group, count)| (1..=*count)
                    .any(|n| id == format!("{group}-{n:03}"))), "unknown {variable} case {id}");
                assert!(ids.insert(id.to_owned()), "duplicate {variable} case {id}");
            }
            Some(ids)
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => panic!("invalid {variable}: {error}"),
    }
}
fn cases(group: &str, mut run: impl FnMut(usize)) {
    let count = GROUPS.iter().find(|(name, _)| *name == group).unwrap().1;
    let include = select("XADB_MIXED_RECOVERY_INCLUDE");
    let exclude = select("XADB_MIXED_RECOVERY_EXCLUDE");
    for n in 0..count {
        let id = format!("{group}-{:03}", n + 1);
        if include.as_ref().is_some_and(|ids| !ids.contains(&id))
            || exclude.as_ref().is_some_and(|ids| ids.contains(&id))
        {
            println!("MIXED_RECOVERY_CASE_SKIP\t{id}");
            continue;
        }
        println!("MIXED_RECOVERY_CASE_BEGIN\t{id}");
        run(n);
        println!("MIXED_RECOVERY_CASE_PASS\t{id}");
    }
}
fn mixed(profile: u32, frames: &[u8]) -> Directory {
    let directory = fixture(profile, 2, frames);
    install(
        &directory,
        "d3-bootstrap",
        literal(UNDO_BOOT, profile, "d3-bootstrap"),
        4096,
    );
    {
        let mut state = directory.0.lock().unwrap();
        let home = &mut state.files.get_mut(HOME_NAME).unwrap().bytes;
        home.resize(TARGET_COUNT * PAGE_BYTES, 0);
        home[4 * PAGE_BYTES..].fill(0x6d);
    }
    reset(&directory.0);
    directory
}
fn complete(profile: u32) -> Directory {
    mixed(profile, &raw(profile, "sequence/wal.frames-only"))
}
fn bytes(directory: &Directory, name: &str) -> Vec<u8> {
    directory.0.lock().unwrap().files[name].bytes.clone()
}
fn inventory(directory: &Directory) -> Vec<(String, u64, [u8; 32])> {
    directory
        .0
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, entry)| {
            assert!(entry.sparse_tail.is_empty());
            (name.clone(), entry.length, content_digest(&entry.bytes))
        })
        .collect()
}
fn run_recovery(
    directory: &Directory,
    check: impl FnOnce(&RecoveryInspection<'_, '_, port::Writer>),
) {
    let mut maintenance = boxed();
    let mut flush = boxed();
    maintenance.fill(0xa5);
    flush.fill(0x5a);
    let owner = ClosedRecovery::acquire_mixed(
        directory,
        Some(port::namespace()),
        &mut maintenance,
        &mut flush,
    )
    .unwrap();
    assert_eq!(owner.effects().mutation_attempted, 0);
    let result = owner.recover().unwrap();
    check(&result);
    assert!(result.observation().ledger.restored_once);
    assert!(result.observation().source_census.peak <= 65536);
    assert_eq!(result.observation().fd_peak, 5);
    drop(result);
    let state = directory.0.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
}
fn check_complete(profile: u32, result: &RecoveryInspection<'_, '_, port::Writer>) {
    assert_eq!(
        result.allocator_image().as_slice(),
        sequence(profile, 3, "allocator.after")
    );
    assert_eq!(
        result.status_image_at(1).unwrap().as_slice(),
        sequence(profile, 1, "payload.after")
    );
    assert_eq!(
        result.undo_image_at(2).unwrap().as_slice(),
        sequence(profile, 2, "payload.after")
    );
    assert_eq!(
        result.undo_image_at(3).unwrap().as_slice(),
        sequence(profile, 3, "payload.after")
    );
    assert_eq!(result.status_image(), result.status_image_at(1));
    for absent in [0, 2, 3, 4, 63, 64, u64::MAX] {
        assert!(result.status_image_at(absent).is_none());
    }
    for absent in [0, 1, 4, 63, 64, u64::MAX] {
        assert!(result.undo_image_at(absent).is_none());
    }
    assert_eq!(
        result.descriptor_bytes().unwrap().as_slice(),
        sequence(profile, 3, "terminal.descriptor")
    );
    let prefix = result.prefix();
    assert_eq!(
        (
            prefix.bootstrap_plan(),
            prefix.complete_lsn(),
            prefix.allocator_lsn(),
            prefix.work_high_water()
        ),
        (3, 6, 6, 3)
    );
    assert_eq!(
        (
            prefix.status_page_mask(),
            prefix.undo_page_mask(),
            prefix.active_mask()
        ),
        (2, 12, 0)
    );
    assert_eq!(prefix.mode(), Some(ProducerMode::Mixed));
    assert_eq!(
        (
            prefix.remaining_wal(),
            prefix.remaining_pages(),
            prefix.remaining_protected()
        ),
        (0, 0, 0)
    );
    assert_eq!(prefix.replay_page_topup_bytes(), 4 * PAGE_BYTES as u64);
    assert_eq!(
        &result.observation().physical.final_lsn[..4],
        &[Some(6), Some(2), Some(4), Some(6)]
    );
    assert!(
        result.observation().physical.final_lsn[4..]
            .iter()
            .all(Option::is_none)
    );
    let ledger = &result.observation().ledger;
    assert_eq!(ledger.replay_topup, 65536);
    assert_eq!(ledger.actual_owned + ledger.outstanding, ledger.owned_peak);
    assert_eq!(ledger.owned_peak, 24_182_784 + prefix.retained_wal_bytes());
    assert_eq!(
        ledger.protected,
        23_134_208 + prefix.retained_wal_bytes() + 65536
    );
    assert_eq!(result.observation().physical.canonical_passes, 1);
}
fn acquired_error(directory: &Directory) -> RecoveryAcquireError {
    let before = inventory(directory);
    let mut maintenance = boxed();
    let mut flush = boxed();
    let failure = match ClosedRecovery::acquire_mixed(
        directory,
        Some(port::namespace()),
        &mut maintenance,
        &mut flush,
    ) {
        Err(error) => error,
        Ok(_) => panic!("invalid mixed recovery input was accepted"),
    };
    assert_eq!(failure.error().phase, RecoveryPhase::Prefix);
    assert!(matches!(failure.error().cause, RecoveryCause::Prefix(_)));
    assert_eq!(failure.effects().mutation_attempted, 0);
    assert_eq!(inventory(directory), before);
    let state = directory.0.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
    drop(state);
    failure
}
fn home_error(directory: &Directory) {
    let before = bytes(directory, HOME_NAME);
    let wal_before = bytes(directory, wal::WAL_NAME);
    let mut maintenance = boxed();
    let mut flush = boxed();
    let owner = ClosedRecovery::acquire_mixed(
        directory,
        Some(port::namespace()),
        &mut maintenance,
        &mut flush,
    )
    .unwrap();
    let failure = match owner.recover() {
        Err(failure) => failure,
        Ok(_) => panic!("unsupported mixed home was repaired"),
    };
    assert_eq!(failure.error().phase, RecoveryPhase::Redo);
    assert!(matches!(
        failure.error().cause,
        RecoveryCause::PageIo(_) | RecoveryCause::Semantic(_) | RecoveryCause::PriorMismatch
    ));
    assert_eq!(failure.error().effect, Effect::CallerMustResolve);
    assert_eq!(failure.effects().home_write_attempted, 0);
    assert_eq!(bytes(directory, HOME_NAME), before);
    assert_eq!(bytes(directory, wal::WAL_NAME), wal_before);
    drop(failure);
    let state = directory.0.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let hash = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash);
}
/// Authored XDW1/XDS1/XCS1 bytes. No product DWB or selector encoder/decoder
/// prepares these inputs; only the shared SHA primitive seals explicit fields.
fn pending(directory: &Directory, profile: u32, image: &[u8], change: impl FnOnce(&mut [u8])) {
    assert_eq!(image.len(), PAGE_BYTES);
    let mut entry = [0; 88];
    entry[2..4].copy_from_slice(&image[6..8]);
    entry[8..32].copy_from_slice(&image[24..48]);
    entry[32..48].copy_from_slice(&image[48..64]);
    entry[48..56].copy_from_slice(&image[64..72]);
    entry[56..].copy_from_slice(&content_digest(image));
    change(&mut entry);
    let mut bank = vec![0; PAGE_BYTES];
    bank[..4].copy_from_slice(b"XDW1");
    bank[4] = 1;
    bank[6] = 1;
    bank[8..24].copy_from_slice(&image[8..24]);
    put64(&mut bank, 24, 1);
    put64(&mut bank, 32, 3);
    put64(&mut bank, 40, 1);
    bank[48] = 1;
    bank[56..64].copy_from_slice(&entry[48..56]);
    bank[128..216].copy_from_slice(&entry);
    seal(&mut bank[..248]);
    let mut slot = vec![0; 16544];
    slot[..4].copy_from_slice(b"XDS1");
    slot[4] = 1;
    slot[6..8].copy_from_slice(&entry[..2]);
    slot[8..24].copy_from_slice(&image[8..24]);
    put64(&mut slot, 24, 1);
    put64(&mut slot, 32, 1);
    slot[40..64].copy_from_slice(&entry[8..32]);
    slot[64..88].copy_from_slice(&entry[32..56]);
    slot[88..90].copy_from_slice(&entry[2..4]);
    slot[96..128].copy_from_slice(&entry[56..88]);
    slot[128..16512].copy_from_slice(image);
    seal(&mut slot);
    let mut selector = literal(DWB, profile, "DWB");
    put64(&mut selector, 32, 3);
    selector[81] = 0;
    put64(&mut selector, 88, 3);
    selector[96..100].copy_from_slice(&248u32.to_le_bytes());
    selector[112..144].copy_from_slice(&content_digest(&bank[..248]));
    seal(&mut selector);
    install(directory, "d3-dwb-0", bank, PAGE_BYTES as u64);
    install(directory, "d3-dwb-images", slot, 16_711_680);
    install(directory, "DWB", selector, 4096);
}

#[test]
fn mixed_recovery_resource_gate() {
    cases("MR01", |_| {
        let c = resources::source_census::<port::Writer>();
        println!(
            "MIXED_RECOVERY_RESOURCE writer=memory outer={} prefix={} entry={} work={} return={} peak={}",
            c.outer, c.prefix_nested, c.entry_phase, c.work_nested, c.return_phase, c.peak
        );
        #[cfg(target_os = "linux")]
        {
            type Native = super::super::native::Writer;
            let native = resources::source_census::<Native>();
            println!(
                "MIXED_RECOVERY_RESOURCE writer=native outer={} prefix={} entry={} work={} return={} peak={}",
                native.outer,
                native.prefix_nested,
                native.entry_phase,
                native.work_nested,
                native.return_phase,
                native.peak
            );
            assert!(resources::fits::<Native>(&native));
        }
        assert!(resources::fits::<port::Writer>(&c));
        assert_eq!(TARGET_BYTES, 48);
        assert_eq!(TARGET_TABLE_BYTES, 3072);
        const {
            assert!(WORKSPACE_BYTES + SAVED_TARGETS + TARGET_TABLE_BYTES <= MAINTENANCE_BYTES);
        }
    });
}
#[test]
fn mixed_recovery_complete_and_reentry() {
    cases("MR02", |n| {
        let profile = n as u32 + 1;
        let directory = complete(profile);
        let original_wal = bytes(&directory, wal::WAL_NAME);
        run_recovery(&directory, |result| {
            check_complete(profile, result);
            assert_eq!(result.observation().physical.dirty_mask, 15);
            assert_eq!(result.effects().home_written, 15);
        });
        let recovered = bytes(&directory, HOME_NAME);
        reset(&directory.0);
        run_recovery(&directory, |result| {
            check_complete(profile, result);
            assert_eq!(
                (
                    result.observation().physical.dirty_mask,
                    result.effects().home_written
                ),
                (0, 0)
            );
            assert!(
                result
                    .observation()
                    .physical
                    .applied
                    .iter()
                    .all(|n| *n == 0)
            );
            assert_eq!(&result.observation().physical.skipped[..4], &[6, 1, 1, 1]);
        });
        assert_eq!(bytes(&directory, HOME_NAME), recovered);
        assert_eq!(bytes(&directory, wal::WAL_NAME), original_wal);
    });
}
#[test]
fn mixed_recovery_older_mixed_and_torn_homes() {
    cases("MR03", |n| {
        let directory = complete(1);
        let (mask, allocator_lsn) = match n {
            0 => (2, 0),
            1 => (5, 6),
            2 => (10, 0),
            3 => (15, 6),
            4 => (2, 2),
            5 => (6, 4),
            6..=8 => (0, 0),
            _ => unreachable!(),
        };
        if allocator_lsn != 0 {
            set_home(
                &directory,
                &sequence(1, allocator_lsn / 2, "allocator.after"),
                0,
            );
        }
        for page in 1..=3 {
            if mask & (1 << page) != 0 {
                set_home(&directory, &sequence(1, page, "payload.after"), page);
            }
        }
        if n >= 6 {
            let page = n - 5;
            let mut image = sequence(1, page, "payload.after");
            image[100] ^= 1;
            set_home(&directory, &image, page);
        }
        let original_wal = bytes(&directory, wal::WAL_NAME);
        run_recovery(&directory, |result| {
            check_complete(1, result);
            let dirty = (14 ^ (mask & 14)) | u64::from(allocator_lsn != 6);
            assert_eq!(result.observation().physical.dirty_mask, dirty);
            assert_eq!(result.effects().home_written, dirty);
            assert_eq!(
                result.observation().physical.applied[0],
                6 - allocator_lsn as u64
            );
            for page in 1..=3 {
                assert_eq!(
                    result.observation().physical.applied[page],
                    u64::from(mask & (1 << page) == 0)
                );
            }
        });
        assert_eq!(bytes(&directory, wal::WAL_NAME), original_wal);
        assert!(
            bytes(&directory, HOME_NAME)[4 * PAGE_BYTES..]
                .iter()
                .all(|b| *b == 0x6d)
        );
    });
}
#[test]
fn mixed_recovery_supported_historical_pending() {
    cases("MR04", |n| {
        let directory = complete(1);
        let (page, image, dirty, applied_allocator) = match n {
            0 => (0, sequence(1, 2, "allocator.reserved"), 15, 3),
            1 | 6 => (0, sequence(1, 2, "allocator.after"), 15, 2),
            2 => (1, sequence(1, 1, "payload.after"), 13, 6),
            3 | 5 => (2, sequence(1, 2, "payload.after"), 11, 6),
            4 => (3, sequence(1, 3, "payload.after"), 7, 6),
            _ => unreachable!(),
        };
        if n == 5 {
            set_home(&directory, &image, page);
        }
        if n == 6 {
            let mut torn = raw(1, "allocator.genesis");
            torn[100] ^= 1;
            set_home(&directory, &torn, 0);
        }
        pending(&directory, 1, &image, |_| {});
        run_recovery(&directory, |result| {
            check_complete(1, result);
            assert_eq!(result.observation().physical.pending_repaired, n != 5);
            assert_eq!(result.observation().physical.pending_home_skipped, n == 5);
            assert!(result.observation().physical.pending_retired);
            assert_eq!(result.observation().physical.dirty_mask, dirty);
            assert_eq!(result.observation().physical.applied[0], applied_allocator);
            assert_eq!(result.observation().batch, 1 + dirty.count_ones() as u64);
            assert_eq!(
                result.effects().home_written,
                dirty | if n == 5 { 0 } else { 1 << page }
            );
        });
    });
}
#[test]
fn mixed_recovery_rejects_pending_mismatch() {
    cases("MR05", |n| {
        let directory = complete(1);
        let mut image = sequence(1, 2, "payload.after");
        if n == 8 {
            put64(&mut image, 64, 3);
            seal(&mut image);
        }
        if n == 9 {
            put64(&mut image, 15928, 6);
            seal(&mut image);
        }
        pending(&directory, 1, &image, |entry| match n {
            0 => entry[2] = 9,
            1 => put64(entry, 32, 2),
            2 => put64(entry, 16, 3),
            3 => put64(entry, 24, 3),
            4 => put64(entry, 48, 6),
            5 => entry[0] = 1,
            6 => entry[56] ^= 1,
            7 => put64(entry, 40, 2),
            8 | 9 => {}
            _ => unreachable!(),
        });
        let error = acquired_error(&directory);
        assert_eq!(error.effects().home_write_attempted, 0);
    });
}
#[test]
fn mixed_recovery_rejects_sha_valid_unsupported_homes() {
    cases("MR06", |n| {
        let directory = complete(1);
        let mut image = sequence(1, 2, "payload.after");
        let mut page = 2;
        match n {
            0 => put64(&mut image, 64, 3),
            1 => put64(&mut image, 64, 0),
            2 => put64(&mut image, 15928, 6),
            3 => put64(&mut image, 15928, u64::MAX),
            4 => image[15926] = 1,
            5 => image[15920..15924].copy_from_slice(b"XUC1"),
            6 => image[6] = 9,
            7 => put64(&mut image, 48, 2),
            8 => put64(&mut image, 56, 2),
            9 => image = sequence(1, 3, "payload.after"),
            10 => {
                image[72..74].copy_from_slice(&2u16.to_le_bytes());
            }
            11 => {
                page = 1;
                image = sequence(1, 1, "payload.after");
                put64(&mut image, 16168, 6);
            }
            _ => unreachable!(),
        }
        seal(&mut image);
        set_home(&directory, &image, page);
        home_error(&directory);
    });
}
#[test]
fn mixed_recovery_rejects_complete_unsupported_grammar() {
    cases("MR07", |n| {
        let mut parts: Vec<_> = (1..=3)
            .flat_map(|work| {
                [
                    sequence(1, work, "reserve.wal"),
                    sequence(1, work, "terminal.wal"),
                ]
            })
            .collect();
        match n {
            0 => parts[0] = raw(1, "old-status-init/work1-reserve.wal"),
            1 => parts[1] = raw(1, "old-status-init/work1-init.wal"),
            2 => {
                parts[3][14688] = 4;
                seal(&mut parts[3]);
            }
            3 => {
                parts[3][296..300].copy_from_slice(&12896u32.to_le_bytes());
                seal(&mut parts[3]);
            }
            4 => {
                parts[3][14690] = 0;
                seal(&mut parts[3]);
            }
            5 => {
                parts[3][15456 + 1] = 2;
                seal(&mut parts[3]);
            }
            6 => {
                parts[3][21728 + 40..21728 + 48].copy_from_slice(&2u64.to_le_bytes());
                seal(&mut parts[3]);
            }
            7 => {
                parts[3][38207] ^= 1;
            }
            8 | 9 => {}
            10 => {
                // A checksum-valid Begin envelope with a valid transaction
                // identity must be rejected by the closed mixed scope before
                // any interpretation of its carried system semantic payload.
                let frame = &mut parts[0];
                frame[6] = 1;
                frame.copy_within(24..40, 72);
                frame.copy_within(40..48, 88);
                frame.copy_within(48..52, 96);
                put64(frame, 104, 1);
                seal(&mut frame[..160]);
                seal(frame);
            }
            _ => unreachable!(),
        }
        let directory = mixed(1, &parts.concat());
        if n == 8 {
            let mut bootstrap = literal(UNDO_BOOT, 1, "d3-bootstrap");
            put64(&mut bootstrap, 120, 2);
            seal(&mut bootstrap);
            install(&directory, "d3-bootstrap", bootstrap, 4096);
        }
        if n == 9 {
            let mut wal = literal(GENESIS, 1, "d3-wal-00000001");
            wal.extend_from_slice(&parts[..5].concat());
            wal.extend_from_slice(&parts[5][..159]);
            let length = wal.len() as u64;
            install(&directory, wal::WAL_NAME, wal, length);
            pending(&directory, 1, &sequence(1, 3, "payload.after"), |_| {});
        }
        let failure = acquired_error(&directory);
        if n == 10 {
            assert!(matches!(
                failure.error().cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    cause: Cause::Semantic(SemanticError::UnsupportedOperation),
                    ..
                })
            ));
        }
    });
}
#[test]
fn mixed_recovery_partial_and_cancel_prefixes() {
    cases("MR08", |n| {
        let lsn = [0, 1, 2, 3, 4, 5, 7, 8][n];
        let mut parts: Vec<_> = (1..=3)
            .flat_map(|work| {
                [
                    sequence(1, work, "reserve.wal"),
                    sequence(1, work, "terminal.wal"),
                ]
            })
            .collect();
        parts.push(raw(1, "cancel/work4-reserve.wal"));
        parts.push(raw(1, "cancel/work4-terminal.wal"));
        let frames = parts[..lsn].concat();
        let directory = mixed(1, &frames);
        let work = lsn.div_ceil(2);
        let terminal = lsn % 2 == 0;
        let expected_allocator = if lsn == 0 {
            raw(1, "allocator.genesis")
        } else {
            raw(
                1,
                &format!(
                    "{}/work{work}-allocator.{}",
                    if work == 4 { "cancel" } else { "sequence" },
                    if terminal { "after" } else { "reserved" }
                ),
            )
        };
        let expected_descriptor = (lsn != 0).then(|| {
            raw(
                1,
                &format!(
                    "{}/work{work}-{}.descriptor",
                    if work == 4 { "cancel" } else { "sequence" },
                    if terminal { "terminal" } else { "reserve" }
                ),
            )
        });
        run_recovery(&directory, |result| {
            assert_eq!(result.allocator_image().as_slice(), expected_allocator);
            assert_eq!(
                result.descriptor_bytes().map(|bytes| bytes.as_slice()),
                expected_descriptor.as_deref()
            );
            assert_eq!(result.prefix().complete_lsn(), lsn as u64);
            assert_eq!(result.prefix().work_high_water(), work as u64);
            assert_eq!(result.prefix().inventory_items(), u64::from(!terminal));
            assert_eq!(
                result.prefix().status_page_mask(),
                if lsn >= 2 { 2 } else { 0 }
            );
            assert_eq!(
                result.prefix().undo_page_mask(),
                if lsn >= 6 {
                    12
                } else if lsn >= 4 {
                    4
                } else {
                    0
                }
            );
            assert_eq!(
                result.prefix().active_mask(),
                if terminal { 0 } else { 1u64 << work }
            );
            assert_eq!(
                result.prefix().replay_page_topup_bytes(),
                [0, 0, 32768, 16384, 49152, 32768, 49152, 65536][n]
            );
            if !terminal {
                assert_eq!(
                    (
                        result.prefix().remaining_wal(),
                        result.prefix().remaining_pages(),
                        result.prefix().remaining_protected()
                    ),
                    (38208, 1, 70976)
                );
            }
            for page in 1..=3 {
                let image = if page == 1 {
                    result.status_image_at(page as u64)
                } else {
                    result.undo_image_at(page as u64)
                };
                if lsn >= page * 2 {
                    assert_eq!(
                        image.unwrap().as_slice(),
                        sequence(1, page, "payload.after")
                    );
                } else {
                    assert!(image.is_none());
                }
            }
        });
        assert_eq!(&bytes(&directory, wal::WAL_NAME)[4096..], frames);
    });
}
#[test]
fn mixed_recovery_keeps_old_entries_closed() {
    cases("MR09", |n| {
        let directory = if n == 2 {
            fixture(1, 2, &raw(1, "old-status-init/wal.frames-only"))
        } else {
            complete(1)
        };
        let before = inventory(&directory);
        if n == 0 {
            let mut maintenance = boxed();
            let mut flush = boxed();
            let error = match ClosedRecovery::acquire(
                &directory,
                Some(port::namespace()),
                &mut maintenance,
                &mut flush,
            ) {
                Err(error) => error,
                Ok(_) => panic!("old effect entry accepted plan3"),
            };
            assert_eq!(error.effects().mutation_attempted, 0);
            assert!(matches!(
                error.error().cause,
                RecoveryCause::Prefix(RecoveryPrefixError {
                    cause: Cause::UnsupportedBootstrapPlan,
                    ..
                })
            ));
        } else if n == 1 {
            let mut workspace = boxed();
            let error = match RecoveryPrefix::acquire(
                &directory,
                Some(port::namespace()),
                &mut workspace,
            ) {
                Err(error) => error,
                Ok(_) => panic!("read-only entry accepted plan3"),
            };
            assert!(matches!(error.cause, Cause::UnsupportedBootstrapPlan));
        } else {
            acquired_error(&directory);
        }
        assert_eq!(inventory(&directory), before);
    });
}
