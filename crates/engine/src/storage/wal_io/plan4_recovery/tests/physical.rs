//! Selected Pending and redo inputs built from fixed wire fields and literals.
use super::faults::{rejected, unchanged_error};
use super::literals::{Oracle, put, seal};
use super::observations::get;
use super::support::*;
use super::*;

/// A complete one-image Pending control/slot. Only the immutable profile part
/// of the existing selector is reused; sequence, image and all digests are new
/// independent expectations, never product encoder output.
pub(super) fn pending(files: &mut Images, image: &[u8], bad_class: bool) {
    let old = &files["DWB"];
    let old_bank = &files[&format!("d3-dwb-{}", old[81])];
    let batch = get(old_bank, 40, 8) + 1;
    let generation = 3 * batch;
    let slot = ((generation - 1) % 2) as u8;
    let mut control = vec![0; 248];
    control[..4].copy_from_slice(b"XDW1");
    put(&mut control, 4, 1, 2);
    control[6] = 1;
    control[8..24].copy_from_slice(&image[8..24]);
    for (at, value, width) in [
        (24, 1, 8),
        (32, generation, 8),
        (40, batch, 8),
        (48, 1, 2),
        (56, get(image, 64, 8), 8),
    ] {
        put(&mut control, at, value, width);
    }
    control[130] = if bad_class { 1 } else { image[6] };
    control[131] = image[7];
    control[136..160].copy_from_slice(&image[24..48]);
    control[160..176].copy_from_slice(&image[48..64]);
    control[176..184].copy_from_slice(&image[64..72]);
    control[184..216].copy_from_slice(&content_digest(image));
    seal(&mut control);
    let mut bank = vec![0; 16384];
    bank[..248].copy_from_slice(&control);
    files.insert(format!("d3-dwb-{slot}"), bank);
    let selector = files.get_mut("DWB").unwrap();
    put(selector, 32, generation, 8);
    selector[81] = slot;
    put(selector, 88, generation, 8);
    put(selector, 96, 248, 4);
    selector[112..144].copy_from_slice(&content_digest(&control));
    seal(selector);
    let mut encoded = vec![0; 16_544];
    encoded[..4].copy_from_slice(b"XDS1");
    put(&mut encoded, 4, 1, 2);
    encoded[8..24].copy_from_slice(&image[8..24]);
    put(&mut encoded, 24, 1, 8);
    put(&mut encoded, 32, batch, 8);
    encoded[40..64].copy_from_slice(&image[24..48]);
    encoded[64..80].copy_from_slice(&image[48..64]);
    encoded[80..88].copy_from_slice(&image[64..72]);
    encoded[88] = control[130];
    encoded[89] = image[7];
    encoded[96..128].copy_from_slice(&content_digest(image));
    encoded[128..16_512].copy_from_slice(image);
    seal(&mut encoded);
    files.get_mut("d3-dwb-images").unwrap()[..16_544].copy_from_slice(&encoded);
}

pub(super) fn future_empty(files: &mut Images) {
    let slot = usize::from(files["DWB"][81]);
    let bank = files.get_mut(&format!("d3-dwb-{slot}")).unwrap();
    let generation = get(bank, 32, 8) + 6;
    let batch = get(bank, 40, 8) + 2;
    put(bank, 32, generation, 8);
    put(bank, 40, batch, 8);
    seal(&mut bank[..160]);
    let digest = content_digest(&bank[..160]);
    let selector = files.get_mut("DWB").unwrap();
    put(selector, 32, generation, 8);
    put(selector, 88, generation, 8);
    selector[112..144].copy_from_slice(&digest);
    seal(selector);
}

#[test]
fn recovery_physical() {
    let ids: Vec<_> = (1..=6).map(|i| format!("P4-REC-P{i:02}")).collect();
    let names: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&names);
    for case in 1..=6 {
        h.case(&format!("P4-REC-P{case:02}"), || {
            let oracle = Oracle::new();
            // P01 covers repair, exact home, healthy newer home and both
            // uncertain sync outcomes within its frozen single-image input.
            for variant in 0..if case == 1 { 5 } else { 1 } {
                let mut files = fresh_memory(if case == 1 && variant == 2 { 6 } else { 4 });
                let target = if case <= 2 { 1 } else { 9 };
                if case <= 2 {
                    pending(&mut files, oracle.page("root-begin.page"), case == 2);
                    if case == 1 && variant != 0 {
                        let canonical = oracle.page(if variant == 2 {
                            "root-abort.page"
                        } else {
                            "root-begin.page"
                        });
                        files.get_mut(HOME_NAME).unwrap()[PAGE_BYTES..2 * PAGE_BYTES]
                            .copy_from_slice(canonical);
                    }
                }
                if matches!(case, 3 | 5) {
                    files.get_mut(HOME_NAME).unwrap()[9 * PAGE_BYTES + 16000] ^= 1;
                }
                if case == 4 {
                    files.get_mut(HOME_NAME).unwrap()[9 * PAGE_BYTES..10 * PAGE_BYTES]
                        .copy_from_slice(oracle.page("status-rollback.page"));
                }
                if case == 6 {
                    let slot = usize::from(files["DWB"][81]);
                    let mut bank = files[&format!("d3-dwb-{slot}")].clone();
                    put(&mut bank, 32, 2, 8);
                    put(&mut bank, 40, 2, 8);
                    seal(&mut bank[..160]);
                    let digest = content_digest(&bank[..160]);
                    files.insert("d3-dwb-1".into(), bank);
                    let selector = files.get_mut("DWB").unwrap();
                    selector[81] = 1;
                    put(selector, 32, 2, 8);
                    put(selector, 88, 2, 8);
                    selector[112..144].copy_from_slice(&digest);
                    seal(selector);
                }
                let d = clone_directory(&files);
                let state = d.state();
                clear(&state);
                if case == 5 || (case == 1 && variant >= 3) {
                    state.lock().unwrap().fault = Some(port::Fault {
                        key: port::Key::new(
                            if case == 5 { "write" } else { "sync" },
                            HOME_NAME,
                            if case == 5 { 9 * PAGE_BYTES as u64 } else { 0 },
                            1,
                        ),
                        action: if variant == 3 {
                            port::Action::BeforeError
                        } else {
                            port::Action::AfterError
                        },
                    });
                }
                if matches!(case, 2 | 4 | 5 | 6) || (case == 1 && variant >= 3) {
                    rejected(&d, &state, |error, observation| {
                        let phase = match case {
                            1 => Plan4RecoveryPhase::Pending,
                            5 => Plan4RecoveryPhase::Redo,
                            6 => Plan4RecoveryPhase::Selection,
                            _ => Plan4RecoveryPhase::Analysis,
                        };
                        if matches!(case, 1 | 5) {
                            assert_eq!(
                                (error.phase, error.database_effect),
                                (phase, Effect::CallerMustResolve)
                            );
                            assert!(matches!(error.cause, Plan4RecoveryCause::Io(_)));
                            assert_eq!(
                                observation.mutations_attempted,
                                observation.mutations_completed + 1
                            );
                        } else {
                            unchanged_error(error, observation, phase);
                            assert!(match case {
                                2 => matches!(error.cause, Plan4RecoveryCause::IdentityMismatch),
                                4 => matches!(error.cause, Plan4RecoveryCause::ContentMismatch),
                                _ => matches!(error.cause, Plan4RecoveryCause::Unsupported),
                            });
                        }
                    });
                    if case == 1 {
                        assert_eq!(file(&state, "DWB"), files["DWB"]);
                        assert_eq!(file(&state, HOME_NAME), files[HOME_NAME]);
                        let s = state.lock().unwrap();
                        assert_eq!(s.fault_hits, 1);
                        assert!(!s.events.iter().any(|e| e.op == "publish"));
                        assert!(
                            !s.events
                                .iter()
                                .any(|e| e.op == "write" && e.name == HOME_NAME)
                        );
                    } else if case == 5 {
                        assert_eq!(state.lock().unwrap().fault_hits, 1);
                        assert_eq!(
                            &file(&state, HOME_NAME)[9 * PAGE_BYTES..10 * PAGE_BYTES],
                            oracle.page("status-begin.page")
                        );
                        assert_eq!(file(&state, "d3-wal-00000001"), files["d3-wal-00000001"]);
                    } else {
                        assert_eq!(images(&state), files);
                    }
                } else {
                    let mut maintenance = boxed::<12_582_912>();
                    let mut dwb = boxed::<4_194_304>();
                    let mut slot = boxed::<1_048_576>();
                    {
                        let runtime = actual_runtime(&d, &mut maintenance, &mut dwb, &mut slot);
                        assert_eq!(
                            runtime.observation().mutations_attempted,
                            runtime.observation().mutations_completed
                        );
                    }
                    let expected = oracle.page(if case == 3 {
                        "status-begin.page"
                    } else if variant == 2 {
                        "root-abort.page"
                    } else {
                        "root-begin.page"
                    });
                    assert_eq!(
                        &file(&state, HOME_NAME)[target * PAGE_BYTES..(target + 1) * PAGE_BYTES],
                        expected
                    );
                    let events = state.lock().unwrap().events.clone();
                    if case == 1 {
                        let retire = events
                            .iter()
                            .position(|e| e.op == "publish" && e.name == "d3-dwb-select-stage")
                            .unwrap();
                        assert!(
                            events[..retire]
                                .iter()
                                .any(|e| e.op == "sync" && e.name == HOME_NAME && e.completed)
                        );
                        let writes = events[..retire]
                            .iter()
                            .filter(|e| e.op == "write" && e.name == HOME_NAME)
                            .collect::<Vec<_>>();
                        assert_eq!(writes.len(), usize::from(variant == 0));
                        if variant == 0 {
                            assert_eq!(
                                (writes[0].offset, writes[0].requested),
                                (PAGE_BYTES as u64, PAGE_BYTES)
                            );
                        }
                    }
                    assert_closed(&state);
                }
                println!("P4_RECOVERY_VARIANT\tP4-REC-P{case:02}\t{variant}");
            }
        });
    }
    h.finish();
}
