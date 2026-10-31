//! Final home/control barriers and the retained original flush continuation.
use super::super::super::plan4_pages::{
    Plan4Page, RetentionCause, RetentionFlushPhase, RetentionFlushStep, RetentionPhase,
};
use super::literals::Oracle;
use super::observations;
use super::physical::future_empty;
use super::support::*;
use super::*;
use crate::{
    storage::dirty_frame::DirtyFrame,
    transaction::concurrency::{OwnerRole, PageConcurrency},
};

fn flush_closed<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    closed: &mut Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    flusher: &mut crate::transaction::concurrency::Owner<'c, 's>,
) {
    drive_flush(&mut closed.inner, flusher, None);
    for target in [
        closed.inner.target(Plan4Page::Root).unwrap(),
        closed.inner.target(Plan4Page::Jobs { index: 0 }).unwrap(),
        closed.inner.status_target(page_ref(9, 2)).unwrap(),
    ] {
        drive_flush(&mut closed.inner, flusher, Some(&target));
    }
}

#[test]
fn recovery_closure() {
    let mut h = Harness::new(&["P4-REC-V01", "P4-REC-V02", "P4-REC-V03", "P4-REC-V04"]);
    for case in 1..=4 {
        h.case(&format!("P4-REC-V{case:02}"), || {
            let d = clone_directory(&fresh_memory(4)); let state = d.state(); clear(&state);
            recovered!(&d, |runtime, closed, owner, flusher, _core, _backing| {
                if case == 3 {
                    // Actual cleanup creates a dirty Status@5. Its WAL has
                    // completed before the original File lock is occupied.
                    closed.step(&mut owner, &mut flusher).unwrap();
                    assert_eq!(runtime.storage.wal().prefix().unwrap().complete_lsn, 5);
                    let target = closed.inner.status_target(page_ref(9, 2)).unwrap();
                    clear(&state);
                    let held = runtime.storage.wal().file.lock().unwrap();
                    {
                        let (_, _, mut flush) = closed.inner.split();
                        assert_eq!(flush.try_flush(&mut flusher, &target).unwrap(), RetentionFlushStep::WaitingSource);
                        assert_eq!(flush.phase(), RetentionFlushPhase::WaitingSource);
                        assert_eq!(runtime.storage.wal().prefix().unwrap().complete_lsn, 5);
                        let before = file(&state, "d3-wal-00000002");
                        assert_eq!(state.lock().unwrap().mutations(), 0);
                        drop(held);
                        let result = flush.continue_flush(&mut flusher).unwrap();
                        assert!(matches!(result, RetentionFlushStep::Complete { captured, finalized } if captured.page_lsn.0 == 5 && finalized.cleaned));
                        assert_eq!(flush.phase(), RetentionFlushPhase::Ready);
                        assert_eq!(file(&state, "d3-wal-00000002"), before);
                    }
                    finish_closed(&mut closed, &mut owner, &mut flusher);
                    assert!(closed.into_live().is_ok());
                    return;
                }
                if case == 4 {
                    finish_closed(&mut closed, &mut owner, &mut flusher);
                    let mut live = match closed.into_live() { Ok(live) => live, Err(_) => panic!("verified owner remained closed") };
                    clear(&state);
                    let before = images(&state);
                    let bank = live.recovery_test_bank().to_vec();
                    let error = live.status_target(page_ref(9, 3)).unwrap_err();
                    assert_eq!((error.phase, error.effect), (RetentionPhase::Load, Effect::Unchanged));
                    assert!(matches!(error.cause, RetentionCause::Membership));
                    assert_eq!(images(&state), before);
                    assert_eq!(live.recovery_test_bank(), bank);
                    assert_eq!(state.lock().unwrap().mutations(), 0);
                    assert!(!runtime.session_fence().is_fenced());
                    owner.check_idle().unwrap();
                    assert_eq!(live.status_target(page_ref(9, 2)).unwrap().identity().page, page_ref(9, 2));
                    return;
                }
                for _ in 0..32 {
                    if runtime.storage.wal().prefix().unwrap().complete_lsn == 7 { break; }
                    assert!(!closed.step(&mut owner, &mut flusher).unwrap().ready);
                }
                assert_eq!(runtime.storage.wal().prefix().unwrap().complete_lsn, 7);
                observations::bank(closed.inner.recovery_test_bank(), &Oracle::new(), 7, None);
                flush_closed(&mut closed, &mut flusher);
                clear(&state);
                if case == 1 {
                    // Same inode/length, valid historical page, wrong final
                    // canonical bytes; this is not merely a checksum rejection.
                    state.lock().unwrap().files.get_mut(HOME_NAME).unwrap().bytes[PAGE_BYTES..2 * PAGE_BYTES].copy_from_slice(Oracle::new().page("root-abort.page"));
                } else {
                    let mut future = images(&state); future_empty(&mut future);
                    let mut s = state.lock().unwrap();
                    for name in ["DWB", "d3-dwb-0", "d3-dwb-1"] { s.files.get_mut(name).unwrap().bytes = future.remove(name).unwrap(); }
                }
                let before = images(&state);
                let error = closed.step(&mut owner, &mut flusher).unwrap_err();
                assert_eq!((error.effect, error.database_effect), (Effect::CallerMustResolve, Effect::CallerMustResolve));
                if case == 1 {
                    assert_eq!(error.phase, Plan4RecoveryPhase::Losers);
                    assert!(matches!(error.cause, Plan4RecoveryCause::Retention(RetentionCause::Invariant)));
                } else {
                    assert_eq!(error.phase, Plan4RecoveryPhase::Stabilize);
                    assert!(matches!(error.cause, Plan4RecoveryCause::Invariant));
                }
                assert!(runtime.session_fence().is_fenced());
                assert_eq!(images(&state), before);
                assert!(!closed.progress.ready);
                assert!(closed.into_live().is_err());
            });
            assert_closed(&state);
        });
    }
    h.finish();
}
