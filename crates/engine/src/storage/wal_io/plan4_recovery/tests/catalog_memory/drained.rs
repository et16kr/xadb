//! Actual durable Drain handoff before content guards are reconstructed.
use super::*;
use crate::{
    storage::dirty_frame::init_test_support as raw,
    transaction::concurrency::catalog_test_support::{self as hook, Point, Resume},
};
use std::{sync::mpsc::sync_channel, time::Duration};
const WAIT: Duration = Duration::from_secs(5);

fn handoff(fenced: bool, unfinished_drop: bool) {
    case(
        if unfinished_drop {
            "C2_DRAIN03"
        } else if fenced {
            "C2_DRAIN02"
        } else {
            "C2_DRAIN01"
        },
        || {
            let oracle = Oracle::new();
            let images =
                with_fresh!(oracle, |live,
                                     owner,
                                     flusher,
                                     status,
                                     undo,
                                     private,
                                     reader_private,
                                     core,
                                     fence,
                                     state| {
                    let _ = reader_private;
                    let actor = create_forward(
                        live, owner, flusher, status, undo, private, &oracle, state, 1,
                    );
                    let indices = [
                        resident_index(live, 1).unwrap(),
                        resident_index(live, 2).unwrap(),
                        resident_index(live, 9).unwrap(),
                    ];
                    let prepared = live.prepare_catalog_commit(owner, actor, private).unwrap();
                    let (entered, arrived) = sync_channel(1);
                    let (resume, release) = sync_channel(1);
                    hook::install(
                        core,
                        if unfinished_drop {
                            Point::DrainedDrop
                        } else {
                            Point::DrainedBeforeResume
                        },
                        entered,
                        release,
                    );
                    {
                        let (mut writer, reader, maintenance) = live.split();
                        let pool = maintenance.catalog_test_pool();
                        let reader_ref = &reader;
                        let oracle_ref = &oracle;
                        std::thread::scope(|scope| {
                            let controller = scope.spawn(move || {
                                arrived.recv_timeout(WAIT).unwrap();
                                let observed = hook::observe(core);
                                assert_eq!(
                                    (
                                        observed.installation,
                                        observed.active_apply,
                                        observed.active_content,
                                        observed.active_read_operations
                                    ),
                                    ("Installing", 1, 0, 0)
                                );
                                assert!(!fence.is_fenced());
                                assert!(core.diagnostic_version_busy());
                                assert_eq!(raw::claim_state(pool).1, 4);
                                assert_eq!(reader_ref.catalog_test_source_used(), 0);
                                for (index, role) in indices.into_iter().zip([1, 2, 4]) {
                                    assert!(!probe::content_busy(pool, index));
                                    let mut bytes = boxed::<PAGE_BYTES>();
                                    let actual = raw::snapshot(pool, index, &mut bytes);
                                    assert_eq!(
                                        bytes.as_slice(),
                                        oracle_ref
                                            .transition
                                            .raw(&oracle_ref.row("C04")[23 + role])
                                    );
                                    assert_eq!(actual.total, 4);
                                }
                                if unfinished_drop {
                                    let (returning, return_arrived) = sync_channel(1);
                                    let (continue_return, release_return) = sync_channel(1);
                                    probe::install_release_pause(pool, returning, release_return);
                                    resume.send(Resume::Continue).unwrap();
                                    return_arrived.recv_timeout(WAIT).unwrap();
                                    // The caller has neither panicked nor fenced. The
                                    // new carrier must abandon before its PinSet hook.
                                    assert!(fence.is_fenced());
                                    let abandoned = hook::observe(core);
                                    assert_eq!(
                                        (
                                            abandoned.installation,
                                            abandoned.active_apply,
                                            abandoned.active_content
                                        ),
                                        ("Abandoned", 0, 0)
                                    );
                                    assert!(!core.diagnostic_version_busy());
                                    assert_eq!(raw::claim_state(pool).1, 4);
                                    continue_return.send(probe::Resume::Continue).unwrap();
                                } else {
                                    if fenced {
                                        fence.fence();
                                    }
                                    resume.send(Resume::Continue).unwrap();
                                }
                            });
                            let result = writer.commit_catalog(owner, prepared);
                            if fenced || unfinished_drop {
                                assert_eq!(result.err().unwrap().effect, Effect::CallerMustResolve);
                            } else {
                                result.unwrap();
                            }
                            controller.join().unwrap();
                            assert_eq!(writer.catalog_test_physical_pins(), 0);
                            assert_eq!(reader_ref.catalog_test_source_used(), 0);
                            assert!(!core.diagnostic_version_busy());
                        });
                    }
                    assert_eq!(
                        live.catalog_test_q(),
                        u64::from(!fenced && !unfinished_drop)
                    );
                    assert_eq!(fence.is_fenced(), fenced || unfinished_drop);
                    if !fenced && !unfinished_drop {
                        oracle.check(live, state, "C04");
                    }
                    state.lock().unwrap().images()
                });
            if fenced || unfinished_drop {
                recover_check(&images, &oracle, "N03");
            }
        },
    );
}

#[test]
fn drained_handoff_has_original_pins_and_no_content_guards() {
    handoff(false, false);
}
#[test]
fn drained_handoff_fence_abandons_before_install() {
    handoff(true, false);
}
#[test]
fn drained_normal_unfinished_drop_fences_before_pin_return() {
    handoff(false, true);
}
