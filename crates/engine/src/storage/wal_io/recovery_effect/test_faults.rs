//! Authored logical source-call inventory and directly related fault cases.
use super::*;

// Authored before any execution: one exclusive count1 reserve, absent slots.
const PREFIX_READS: [(&str, &str, u64, usize); 57] = [
    ("metadata", "CURRENT", 0, 0),
    ("metadata", "d3-current-stage", 0, 0),
    ("metadata", "d3-rebase-0", 0, 0),
    ("metadata", "d3-rebase-1", 0, 0),
    ("metadata", "d3-writer-stage", 0, 0),
    ("metadata", "d3-filemap-select-stage", 0, 0),
    ("open", "d3-profile", 0, 0),
    ("metadata", "d3-profile", 0, 0),
    ("read", "d3-profile", 0, 4096),
    ("open", "FILEMAP", 0, 0),
    ("metadata", "FILEMAP", 0, 0),
    ("read", "FILEMAP", 0, 4096),
    ("open", "d3-profile", 0, 0),
    ("metadata", "d3-profile", 0, 0),
    ("read", "d3-profile", 0, 4096),
    ("open", "FILEMAP", 0, 0),
    ("metadata", "FILEMAP", 0, 0),
    ("read", "FILEMAP", 0, 4096),
    ("open", "d3-filemap-0", 0, 0),
    ("metadata", "d3-filemap-0", 0, 0),
    ("read", "d3-filemap-0", 0, 320),
    ("read", "d3-filemap-0", 16384, 256),
    ("open", "d3-bootstrap", 0, 0),
    ("metadata", "d3-bootstrap", 0, 0),
    ("read", "d3-bootstrap", 0, 4096),
    ("open", "DWB", 0, 0),
    ("metadata", "DWB", 0, 0),
    ("read", "DWB", 0, 4096),
    ("open", "d3-dwb-0", 0, 0),
    ("metadata", "d3-dwb-0", 0, 0),
    ("read", "d3-dwb-0", 0, 160),
    ("open", "d3-data-0000000100000001", 0, 0),
    ("metadata", "d3-data-0000000100000001", 0, 0),
    ("open", "d3-filemap-0", 0, 0),
    ("metadata", "d3-filemap-0", 0, 0),
    ("open", "d3-filemap-1", 0, 0),
    ("metadata", "d3-filemap-1", 0, 0),
    ("open", "d3-dwb-0", 0, 0),
    ("metadata", "d3-dwb-0", 0, 0),
    ("open", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-1", 0, 0),
    ("open", "d3-dwb-stage", 0, 0),
    ("metadata", "d3-dwb-stage", 0, 0),
    ("open", "d3-dwb-images", 0, 0),
    ("metadata", "d3-dwb-images", 0, 0),
    ("open", "d3-dwb-reserved", 0, 0),
    ("metadata", "d3-dwb-reserved", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-wal-00000001", 0, 0),
    ("metadata", "d3-wal-00000001", 0, 0),
    ("read", "d3-wal-00000001", 0, 4096),
    ("open", "WRITER", 0, 0),
    ("metadata", "WRITER", 0, 0),
    ("read", "WRITER", 0, 4096),
    ("read", "d3-wal-00000001", 4096, 160),
    ("read", "d3-wal-00000001", 4256, 21040),
    ("metadata", "d3-wal-00000001", 0, 0),
];
const EFFECT_READS: [(&str, &str, u64, usize); 56] = [
    ("open", "d3-rebase-0", 0, 0),
    ("metadata", "d3-rebase-0", 0, 0),
    ("open", "d3-rebase-0", 0, 0),
    ("metadata", "d3-rebase-0", 0, 0),
    ("open", "d3-rebase-1", 0, 0),
    ("metadata", "d3-rebase-1", 0, 0),
    ("open", "d3-rebase-1", 0, 0),
    ("metadata", "d3-rebase-1", 0, 0),
    ("metadata", "d3-rebase-0", 0, 0),
    ("metadata", "d3-rebase-1", 0, 0),
    ("metadata", "d3-wal-00000001", 0, 0),
    ("metadata", "d3-wal-00000001", 0, 0),
    ("open", "d3-data-0000000100000001", 0, 0),
    ("metadata", "d3-data-0000000100000001", 0, 0),
    ("read", "d3-data-0000000100000001", 0, 16384),
    ("read", "d3-wal-00000001", 4096, 160),
    ("read", "d3-wal-00000001", 4256, 21040),
    ("metadata", "d3-wal-00000001", 0, 0),
    ("open", "d3-dwb-stage", 0, 0),
    ("metadata", "d3-dwb-stage", 0, 0),
    ("open", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-dwb-images", 0, 0),
    ("metadata", "d3-dwb-images", 0, 0),
    ("open", "d3-dwb-stage", 0, 0),
    ("metadata", "d3-dwb-stage", 0, 0),
    ("open", "d3-dwb-0", 0, 0),
    ("metadata", "d3-dwb-0", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-data-0000000100000001", 0, 0),
    ("metadata", "d3-data-0000000100000001", 0, 0),
    ("open", "d3-dwb-stage", 0, 0),
    ("metadata", "d3-dwb-stage", 0, 0),
    ("open", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-data-0000000100000001", 0, 0),
    ("metadata", "d3-data-0000000100000001", 0, 0),
    ("read", "d3-data-0000000100000001", 0, 16384),
    ("open", "DWB", 0, 0),
    ("metadata", "DWB", 0, 0),
    ("read", "DWB", 0, 4096),
    ("open", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-1", 0, 0),
    ("read", "d3-dwb-1", 0, 160),
    ("metadata", "d3-wal-00000001", 0, 0),
];
const MUTATIONS: [(&str, &str, u64, usize); 70] = [
    ("create", "d3-rebase-0", 0, 0),
    ("write", "d3-rebase-0", 0, 65536),
    ("write", "d3-rebase-0", 65536, 65536),
    ("write", "d3-rebase-0", 131072, 65536),
    ("write", "d3-rebase-0", 196608, 65536),
    ("write", "d3-rebase-0", 262144, 65536),
    ("write", "d3-rebase-0", 327680, 65536),
    ("write", "d3-rebase-0", 393216, 65536),
    ("write", "d3-rebase-0", 458752, 65536),
    ("write", "d3-rebase-0", 524288, 65536),
    ("write", "d3-rebase-0", 589824, 65536),
    ("write", "d3-rebase-0", 655360, 65536),
    ("write", "d3-rebase-0", 720896, 65536),
    ("write", "d3-rebase-0", 786432, 65536),
    ("write", "d3-rebase-0", 851968, 65536),
    ("write", "d3-rebase-0", 917504, 65536),
    ("write", "d3-rebase-0", 983040, 65536),
    ("sync", "d3-rebase-0", 0, 0),
    ("create", "d3-rebase-1", 0, 0),
    ("write", "d3-rebase-1", 0, 65536),
    ("write", "d3-rebase-1", 65536, 65536),
    ("write", "d3-rebase-1", 131072, 65536),
    ("write", "d3-rebase-1", 196608, 65536),
    ("write", "d3-rebase-1", 262144, 65536),
    ("write", "d3-rebase-1", 327680, 65536),
    ("write", "d3-rebase-1", 393216, 65536),
    ("write", "d3-rebase-1", 458752, 65536),
    ("write", "d3-rebase-1", 524288, 65536),
    ("write", "d3-rebase-1", 589824, 65536),
    ("write", "d3-rebase-1", 655360, 65536),
    ("write", "d3-rebase-1", 720896, 65536),
    ("write", "d3-rebase-1", 786432, 65536),
    ("write", "d3-rebase-1", 851968, 65536),
    ("write", "d3-rebase-1", 917504, 65536),
    ("write", "d3-rebase-1", 983040, 65536),
    ("sync", "d3-rebase-1", 0, 0),
    ("dirsync", "", 0, 0),
    ("sync", "d3-wal-00000001", 0, 0),
    ("dirsync", "", 0, 0),
    ("write", "d3-dwb-stage", 0, 16384),
    ("sync", "d3-dwb-stage", 0, 0),
    ("write", "d3-dwb-1", 0, 16384),
    ("sync", "d3-dwb-1", 0, 0),
    ("create", "d3-dwb-select-stage", 0, 0),
    ("write", "d3-dwb-select-stage", 0, 4096),
    ("sync", "d3-dwb-select-stage", 0, 0),
    ("publish", "d3-dwb-select-stage", 0, 0),
    ("dirsync", "", 0, 0),
    ("write", "d3-dwb-images", 0, 16544),
    ("sync", "d3-dwb-images", 0, 0),
    ("write", "d3-dwb-stage", 0, 16384),
    ("sync", "d3-dwb-stage", 0, 0),
    ("write", "d3-dwb-0", 0, 16384),
    ("sync", "d3-dwb-0", 0, 0),
    ("create", "d3-dwb-select-stage", 0, 0),
    ("write", "d3-dwb-select-stage", 0, 4096),
    ("sync", "d3-dwb-select-stage", 0, 0),
    ("publish", "d3-dwb-select-stage", 0, 0),
    ("dirsync", "", 0, 0),
    ("write", "d3-data-0000000100000001", 0, 16384),
    ("sync", "d3-data-0000000100000001", 0, 0),
    ("write", "d3-dwb-stage", 0, 16384),
    ("sync", "d3-dwb-stage", 0, 0),
    ("write", "d3-dwb-1", 0, 16384),
    ("sync", "d3-dwb-1", 0, 0),
    ("create", "d3-dwb-select-stage", 0, 0),
    ("write", "d3-dwb-select-stage", 0, 4096),
    ("sync", "d3-dwb-select-stage", 0, 0),
    ("publish", "d3-dwb-select-stage", 0, 0),
    ("dirsync", "", 0, 0),
];
const READ_PHASES: [RecoveryPhase; 56] = [
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::SourceStabilize,
    RecoveryPhase::Redo,
    RecoveryPhase::Redo,
    RecoveryPhase::Redo,
    RecoveryPhase::Redo,
    RecoveryPhase::Redo,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Verify,
    RecoveryPhase::Snapshot,
];
const MUTATION_PHASES: [RecoveryPhase; 70] = [
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::BootstrapProvision,
    RecoveryPhase::SourceStabilize,
    RecoveryPhase::SourceStabilize,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
    RecoveryPhase::Flush,
];
fn check_events(actual: &[port::Event], expected: &[(&str, &str, u64, usize)]) {
    let got: Vec<_> = actual
        .iter()
        .map(|e| (e.kind, e.name.as_str(), e.offset, e.length))
        .collect();
    assert_eq!(got, expected);
}
#[test]
fn new_mutation_faults_keep_failed_owner() {
    mark("EF", 140, |n| {
        let at = n / 2;
        let (frames, _, _) = pair(1, 81920, 1, false);
        let d = fixture(1, 1, &frames);
        d.0.lock().unwrap().plan = Some(port::Plan {
            at,
            action: if n % 2 == 0 {
                port::Action::BeforeError
            } else {
                port::Action::AfterError
            },
        });
        recovered_error(&d, |e| {
            assert_eq!(e.error().phase, MUTATION_PHASES[at]);
            assert!(e.effects().mutation_attempted > e.effects().mutation_completed);
            assert!(matches!(
                e.error().cause,
                RecoveryCause::Io(_) | RecoveryCause::Namespace(_)
            ));
            let s = d.0.lock().unwrap();
            assert_eq!(s.hit, 1);
            check_events(&s.events, &MUTATIONS[..=at]);
        });
    });
}
#[test]
fn new_read_metadata_faults_preserve_typed_first_error() {
    mark("ER", 56, |n| {
        let (frames, _, _) = pair(1, 81920, 1, false);
        let d = fixture(1, 1, &frames);
        d.0.lock().unwrap().read_plan = Some(port::Plan {
            at: PREFIX_READS.len() + n,
            action: port::Action::BeforeError,
        });
        recovered_error(&d, |e| {
            assert_eq!(e.error().phase, READ_PHASES[n]);
            assert!(matches!(
                e.error().cause,
                RecoveryCause::Io(_) | RecoveryCause::Namespace(_) | RecoveryCause::Prefix(_)
            ));
            let s = d.0.lock().unwrap();
            assert_eq!(s.hit, 1);
            check_events(&s.read_events[..PREFIX_READS.len()], &PREFIX_READS);
            check_events(&s.read_events[PREFIX_READS.len()..], &EFFECT_READS[..=n]);
        });
    });
}
#[test]
fn existing_only_prevalidation_faults_make_no_writes() {
    mark("EE", 36, |n| {
        let d = fixture(1, 1, &[]);
        for path in SLOT_NAMES {
            install(&d, path, vec![0; 1048576], 1048576);
        }
        // Header-only PREINSPECT removes the two frame reads from the authored57.
        let baseline = PREFIX_READS.len() - 2;
        d.0.lock().unwrap().read_plan = Some(port::Plan {
            at: baseline + n,
            action: port::Action::BeforeError,
        });
        let e = acquire_error(&d);
        assert_eq!(e.error.phase, RecoveryPhase::Inventory);
        assert_eq!(e.error.effect, Effect::CallerMustResolve);
        assert_eq!(e.effects.mutation_attempted, 0);
        let s = d.0.lock().unwrap();
        assert_eq!(s.hit, 1);
        check_events(&s.read_events[baseline..], &ZERO_PREVALIDATION[..=n]);
        assert!(s.events.is_empty());
        assert_eq!(s.writers, 0);
        assert_eq!(s.handles, 0);
    });
}
#[test]
#[ignore = "DEV3 final S5 expanded profiles, lengths and fault actions"]
fn expanded_closed_recovery_matrix() {
    mark("EX", 2856, |n| {
        let profile = (n / (7 * 136) + 1) as u32;
        let length = [0, 1, 65535, 65536, 65537, 1048575, 1048576][(n / 136) % 7];
        let local = n % 136;
        let at = local / 2;
        let (frames, _, _) = pair(profile, 81920, 1, false);
        let d = fixture(profile, 1, &frames);
        for path in SLOT_NAMES {
            install(&d, path, vec![0; length], length as u64);
        }
        // Existing branches omit the two creates in MUTATIONS. Map the authored
        // absent ordinal to the corresponding semantic event, omitting create rows.
        let logical: Vec<_> = MUTATIONS
            .iter()
            .filter(|(k, name, _, _)| !(*k == "create" && SLOT_NAMES.contains(name)))
            .collect();
        let index = at;
        d.0.lock().unwrap().plan = Some(port::Plan {
            at: index,
            action: if local % 2 == 0 {
                port::Action::BeforeError
            } else {
                port::Action::AfterError
            },
        });
        recovered_error(&d, |_| {
            let s = d.0.lock().unwrap();
            assert_eq!(s.hit, 1);
            let wanted = logical[index];
            let got = s.events.last().unwrap();
            assert_eq!(
                (got.kind, got.name.as_str(), got.offset, got.length),
                *wanted
            );
        });
    });
}
const PENDING_READS: [(&str, &str, u64, usize); 22] = [
    ("open", "DWB", 0, 0),
    ("metadata", "DWB", 0, 0),
    ("read", "DWB", 0, 4096),
    ("open", "d3-dwb-0", 0, 0),
    ("metadata", "d3-dwb-0", 0, 0),
    ("read", "d3-dwb-0", 0, 248),
    ("open", "d3-dwb-images", 0, 0),
    ("metadata", "d3-dwb-images", 0, 0),
    ("read", "d3-dwb-images", 0, 16544),
    ("open", "d3-data-0000000100000001", 0, 0),
    ("metadata", "d3-data-0000000100000001", 0, 0),
    ("read", "d3-data-0000000100000001", 0, 16384),
    ("open", "d3-data-0000000100000001", 0, 0),
    ("metadata", "d3-data-0000000100000001", 0, 0),
    ("open", "d3-dwb-stage", 0, 0),
    ("metadata", "d3-dwb-stage", 0, 0),
    ("open", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-1", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("open", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
    ("metadata", "d3-dwb-select-stage", 0, 0),
];
#[test]
fn selected_historical_pending_faults() {
    mark("EP", 44, |n| {
        let (frames, _, _) = pair(1, 98304, 1, true);
        let d = fixture(1, 1, &frames);
        let historical = vector(DIRTY, 1, 1, "reserve.page");
        install_pending(
            &d,
            1,
            &historical,
            PageKind::Allocation,
            1,
            1,
            ControlState::Pending,
        );
        if n < 22 {
            d.0.lock().unwrap().read_plan = Some(port::Plan {
                at: 71 + n,
                action: port::Action::BeforeError,
            });
        } else {
            d.0.lock().unwrap().plan = Some(port::Plan {
                at: 39 + (n - 22) / 2,
                action: if n % 2 == 0 {
                    port::Action::BeforeError
                } else {
                    port::Action::AfterError
                },
            });
        }
        recovered_error(&d, |e| {
            let p = if n < 12 {
                RecoveryPhase::PendingValidate
            } else if n < 14 {
                RecoveryPhase::PendingHome
            } else if n < 22 {
                RecoveryPhase::Retirement
            } else if (n - 22) / 2 < 2 {
                RecoveryPhase::PendingHome
            } else {
                RecoveryPhase::Retirement
            };
            assert_eq!(e.error().phase, p);
            let s = d.0.lock().unwrap();
            assert_eq!(s.hit, 1);
            if n < 22 {
                check_events(&s.read_events[71..], &PENDING_READS[..=n]);
            } else {
                let at = 39 + (n - 22) / 2;
                let want = if at < 41 {
                    if at == 39 {
                        ("write", HOME_NAME, 0, 16384)
                    } else {
                        ("sync", HOME_NAME, 0, 0)
                    }
                } else {
                    MUTATIONS[61 + at - 41]
                };
                let got = s.events.last().unwrap();
                assert_eq!((got.kind, got.name.as_str(), got.offset, got.length), want);
            }
        });
    });
}
#[test]
fn second_status_home_faults() {
    mark("EV", 12, |n| {
        let (frames, _, _, _) = status_case(1, 0, "init");
        let d = fixture(1, 2, &frames);
        if n < 8 {
            let at = [74, 75, 76, 128, 129, 141, 142, 143][n];
            d.0.lock().unwrap().read_plan = Some(port::Plan {
                at,
                action: port::Action::BeforeError,
            });
        } else {
            d.0.lock().unwrap().plan = Some(port::Plan {
                at: 90 + (n - 8) / 2,
                action: if n % 2 == 0 {
                    port::Action::BeforeError
                } else {
                    port::Action::AfterError
                },
            });
        }
        recovered_error(&d, |e| {
            let p = if n < 3 {
                RecoveryPhase::Redo
            } else if !(5..8).contains(&n) {
                RecoveryPhase::Flush
            } else {
                RecoveryPhase::Verify
            };
            assert_eq!(e.error().phase, p);
            assert_eq!(d.0.lock().unwrap().hit, 1);
        });
    });
}
#[test]
#[ignore = "DEV3 final S5 existing-zero read lengths and Before/After faults"]
fn expanded_zero_prevalidation_faults() {
    mark("EY", 648, |n| {
        let profile = (n / 216 + 1) as u32;
        let mut ordinal = n % 216;
        let lengths: [usize; 8] = [0, 1, 17, 65535, 65536, 65537, 1048575, 1048576];
        let mut picked = None;
        for length in lengths {
            let calls = 2 * (2 + length.div_ceil(65536));
            let cases = 2 * calls;
            if ordinal < cases {
                picked = Some((length, ordinal / 2, ordinal % 2));
                break;
            }
            ordinal -= cases;
        }
        let (length, at, action) = picked.expect("fixed authored matrix");
        let d = fixture(profile, 1, &[]);
        for path in SLOT_NAMES {
            install(&d, path, vec![0; length], length as u64);
        }
        d.0.lock().unwrap().read_plan = Some(port::Plan {
            at: 55 + at,
            action: if action == 0 {
                port::Action::BeforeError
            } else {
                port::Action::AfterError
            },
        });
        let e = acquire_error(&d);
        assert_eq!(e.error.phase, RecoveryPhase::Inventory);
        assert_eq!(e.error.effect, Effect::CallerMustResolve);
        assert_eq!(e.effects.mutation_attempted, 0);
        assert_eq!(d.0.lock().unwrap().hit, 1);
    });
}

const ZERO_PREVALIDATION: [(&str, &str, u64, usize); 36] = [
    ("open", "d3-rebase-0", 0, 0),
    ("metadata", "d3-rebase-0", 0, 0),
    ("read", "d3-rebase-0", 0, 65536),
    ("read", "d3-rebase-0", 65536, 65536),
    ("read", "d3-rebase-0", 131072, 65536),
    ("read", "d3-rebase-0", 196608, 65536),
    ("read", "d3-rebase-0", 262144, 65536),
    ("read", "d3-rebase-0", 327680, 65536),
    ("read", "d3-rebase-0", 393216, 65536),
    ("read", "d3-rebase-0", 458752, 65536),
    ("read", "d3-rebase-0", 524288, 65536),
    ("read", "d3-rebase-0", 589824, 65536),
    ("read", "d3-rebase-0", 655360, 65536),
    ("read", "d3-rebase-0", 720896, 65536),
    ("read", "d3-rebase-0", 786432, 65536),
    ("read", "d3-rebase-0", 851968, 65536),
    ("read", "d3-rebase-0", 917504, 65536),
    ("read", "d3-rebase-0", 983040, 65536),
    ("open", "d3-rebase-1", 0, 0),
    ("metadata", "d3-rebase-1", 0, 0),
    ("read", "d3-rebase-1", 0, 65536),
    ("read", "d3-rebase-1", 65536, 65536),
    ("read", "d3-rebase-1", 131072, 65536),
    ("read", "d3-rebase-1", 196608, 65536),
    ("read", "d3-rebase-1", 262144, 65536),
    ("read", "d3-rebase-1", 327680, 65536),
    ("read", "d3-rebase-1", 393216, 65536),
    ("read", "d3-rebase-1", 458752, 65536),
    ("read", "d3-rebase-1", 524288, 65536),
    ("read", "d3-rebase-1", 589824, 65536),
    ("read", "d3-rebase-1", 655360, 65536),
    ("read", "d3-rebase-1", 720896, 65536),
    ("read", "d3-rebase-1", 786432, 65536),
    ("read", "d3-rebase-1", 851968, 65536),
    ("read", "d3-rebase-1", 917504, 65536),
    ("read", "d3-rebase-1", 983040, 65536),
];
