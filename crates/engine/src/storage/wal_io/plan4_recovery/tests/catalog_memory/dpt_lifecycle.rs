//! The compact DPT keeps original identity diagnostics through real pair
//! installation cuts. Fenced diagnostic views never publish the hidden pair.
use super::*;
use crate::storage::dirty_frame::{init_test_support as init, test_support as frames};
use crate::storage::page::PageLsn;
use std::sync::mpsc::sync_channel;

#[test]
fn pair_dpt_unwind_keeps_original_targets_before_publication() {
    case("C2_DPT01", || {
        for point in [
            init::InitPoint::PairDptInstalled,
            init::InitPoint::PairFirstRegistered,
            init::InitPoint::PairBothRegisteredBeforeGate,
            init::InitPoint::PairGatePublished,
        ] {
            let oracle = Oracle::new();
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
                let _ = (reader_private, core);
                let (entered, arrived) = sync_channel(1);
                let (resume, release) = sync_channel(1);
                init::install_init_pause(live.catalog_test_pool(), point, entered, release);
                std::thread::scope(|scope| {
                    let controller = scope.spawn(move || {
                        arrived.recv_timeout(WAIT).unwrap();
                        resume.send(frames::Resume::Panic).unwrap();
                    });
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            create_forward(
                                live, owner, flusher, status, undo, private, &oracle, state, 1,
                            )
                        }))
                        .is_err()
                    );
                    controller.join().unwrap();
                });
                assert!(fence.is_fenced());
                let pool = live.catalog_test_pool();
                let mut found = [false; 2];
                let mut output = boxed::<PAGE_BYTES>();
                for index in 0..9 {
                    let observed = init::snapshot(pool, index, &mut output);
                    let Some(entry) = observed.entry else {
                        continue;
                    };
                    let at = match entry.identity.page.page_no {
                        11 => 0,
                        12 => 1,
                        _ => continue,
                    };
                    assert!(!found[at]);
                    found[at] = true;
                    assert_eq!(entry.identity.page, page_ref(11 + at as u64, 2));
                    assert_eq!(entry.identity.owner.get(), 6);
                    assert_eq!(entry.identity.owner_generation.get(), 1);
                    assert!(entry.generation > 1);
                    assert_eq!(entry.rec_lsn, PageLsn(8));
                    assert_eq!(entry.modification_counter, 1);
                    assert_eq!(observed.requirement.unwrap().lsn, 8);
                    assert!(observed.claim_present);
                    assert!(!observed.initializer_pins_held);
                    if point != init::InitPoint::PairGatePublished {
                        assert_eq!(frames::registered_index(pool, 11 + at as u64), None);
                    }
                }
                assert_eq!(found, [true, true]);
            });
        }
    });
}
