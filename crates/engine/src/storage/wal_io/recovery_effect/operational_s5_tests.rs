//! Expanded first-extent fault inventory, authored from the I/O protocol before execution.
use super::*;
type Call = (&'static str, &'static str, u64, usize);

fn mutation_vector(mask: usize) -> Vec<Call> {
    let mut out = Vec::new();
    for name in ["d3-rebase-0", "d3-rebase-1"] {
        out.push(("create", name, 0, 0));
        for chunk in 0..16 {
            out.push(("write", name, chunk * 65536, 65536));
        }
        out.push(("sync", name, 0, 0));
    }
    out.extend([
        ("dirsync", "", 0, 0),
        ("sync", "d3-wal-00000001", 0, 0),
        ("dirsync", "", 0, 0),
    ]);
    let mut bank = 0;
    for target in 0..3 {
        if mask & (1 << target) != 0 {
            continue;
        }
        for selection in 0..3 {
            if selection == 1 {
                out.extend([
                    ("write", "d3-dwb-images", 0, 16544),
                    ("sync", "d3-dwb-images", 0, 0),
                ]);
            }
            bank = 1 - bank;
            let name = if bank == 0 { "d3-dwb-0" } else { "d3-dwb-1" };
            out.extend([
                ("write", "d3-dwb-stage", 0, 16384),
                ("sync", "d3-dwb-stage", 0, 0),
                ("write", name, 0, 16384),
                ("sync", name, 0, 0),
                ("create", "d3-dwb-select-stage", 0, 0),
                ("write", "d3-dwb-select-stage", 0, 4096),
                ("sync", "d3-dwb-select-stage", 0, 0),
                ("publish", "d3-dwb-select-stage", 0, 0),
                ("dirsync", "", 0, 0),
            ]);
            if selection == 1 {
                out.extend([
                    ("write", HOME_NAME, target * 16384, 16384),
                    ("sync", HOME_NAME, 0, 0),
                ]);
            }
        }
    }
    out
}

#[test]
#[ignore = "S5 only: profiles1/2/3 x all home subsets x dirty targets x 31 mutation calls x Before/After"]
fn operational_recovery_s5_home_fault_matrix() {
    // For one profile: sum(3-popcount(mask), mask=0..7) * 31 * 2 = 744.
    mark("ORF", 2232, |n| {
        let profile = 1 + (n / 744) as u32;
        let mut remaining = n % 744;
        let mut mask = 0usize;
        while remaining >= (3 - mask.count_ones() as usize) * 62 {
            remaining -= (3 - mask.count_ones() as usize) * 62;
            mask += 1;
            assert!(mask < 7);
        }
        let at = 39 + remaining / 2;
        let literal = |name: &str| read(REPEATED, &format!("p{profile}/{name}"));
        let d = fixture(profile, 2, &literal("wal.frames-only"));
        d.0.lock()
            .unwrap()
            .files
            .get_mut(HOME_NAME)
            .unwrap()
            .bytes
            .resize(TARGET_COUNT * PAGE_BYTES, 0);
        for (target, name) in [
            (0, "work3-allocator.after"),
            (1, "work1-status.after"),
            (2, "work3-status.after"),
        ] {
            if mask & (1 << target) != 0 {
                set_home(&d, &literal(name), target);
            }
        }
        let original_home = d.0.lock().unwrap().files[HOME_NAME].bytes.clone();
        let original_wal = d.0.lock().unwrap().files[wal::WAL_NAME].bytes.clone();
        let vector = mutation_vector(mask);
        assert_eq!(vector.len(), 39 + (3 - mask.count_ones() as usize) * 31);
        d.0.lock().unwrap().plan = Some(port::Plan {
            at,
            action: if remaining % 2 == 0 {
                port::Action::BeforeError
            } else {
                port::Action::AfterError
            },
        });
        recovered_error(&d, |failure| {
            assert_eq!(failure.error().phase, RecoveryPhase::Flush);
            let (operation, name, offset, length) = vector[at];
            let (kind, reported_name, reported_length) = match operation {
                "write" => (CallKind::Write, name, length as u64),
                "sync" => (CallKind::FileSync, name, 0),
                "create" => (CallKind::Create, name, 0),
                "publish" => (CallKind::Publish, "DWB", 4096),
                "dirsync" => (CallKind::DirectorySync, ".", 0),
                _ => unreachable!(),
            };
            let last = failure.effects().last_call.unwrap();
            assert_eq!(
                (last.phase, last.kind, last.name, last.offset, last.length),
                (
                    RecoveryPhase::Flush,
                    kind,
                    reported_name,
                    offset,
                    reported_length
                )
            );
            if operation == "write" || operation == "sync" {
                // Foundation classifies both write and sync uncertainty with this code.
                assert!(matches!(&failure.error().cause, RecoveryCause::Io(e)
                    if e.code == xadb_foundation::error::ErrorCode::IoWriteUnknown));
            } else {
                let expected_effect = if remaining % 2 == 0 && operation != "dirsync" {
                    Effect::Unchanged
                } else {
                    Effect::CallerMustResolve
                };
                assert!(matches!(&failure.error().cause, RecoveryCause::Namespace(e)
                    if e.kind == xadb_foundation::namespace::NamespaceErrorKind::Io
                    && e.effect == expected_effect));
            }
            assert!(failure.effects().mutation_attempted > failure.effects().mutation_completed);
            let state = d.0.lock().unwrap();
            assert_eq!(state.hit, 1);
            let observed: Vec<_> = state
                .events
                .iter()
                .map(|e| (e.kind, e.name.as_str(), e.offset, e.length))
                .collect();
            assert_eq!(observed, vector[..=at]);
            assert_eq!(state.files[wal::WAL_NAME].bytes, original_wal);
            for target in 0..64 {
                if target > 2 || mask & (1 << target) != 0 {
                    let range = target * PAGE_BYTES..(target + 1) * PAGE_BYTES;
                    assert_eq!(
                        state.files[HOME_NAME].bytes[range.clone()],
                        original_home[range]
                    );
                }
            }
        });
        let state = d.0.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
    });
}
