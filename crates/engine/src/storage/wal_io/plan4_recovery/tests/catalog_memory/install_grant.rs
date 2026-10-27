//! Current Profile2 cannot reach the retained legacy scope1 producer. Test its
//! real entry boundary without manufacturing a Profile1 storage capability.
use super::*;
use crate::storage::{
    disk::{RetentionCause, RetentionPhase},
    retention_page::JobSlotRef,
};

#[test]
fn current_profile_rejects_legacy_scope1_before_installation() {
    case("C3_GRANT_SCOPE", || {
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
            let _ = (undo, private, reader_private, core);
            let job = JobSlotRef::new(page_ref(2, 1), 0, Generation::new(2).unwrap()).unwrap();
            let actor = actor(1, 1, 2);
            let before = state.lock().unwrap().images();
            let wal = live.catalog_test_wal_state();
            let isolation = live.catalog_test_isolation();
            let epochs = live.catalog_test_epochs();
            state.lock().unwrap().events.clear();
            {
                let (mut writer, _, _) = live.split();
                for selected in 0..5 {
                    let result = match selected {
                        0 => writer.aging_start(owner),
                        1 => writer.begin_empty(owner, status, job),
                        2 => writer.rollback_start(owner, actor),
                        3 => writer.abort_end_empty(owner, actor),
                        4 => writer.finish_empty_terminal(owner, job),
                        _ => unreachable!(),
                    };
                    let error = result.err().expect("legacy scope rejected");
                    assert_eq!(
                        (error.phase, error.effect),
                        (RetentionPhase::Prepare, Effect::Unchanged)
                    );
                    assert!(matches!(error.cause, RetentionCause::WrongState));
                    owner.check_idle().unwrap();
                }
            }
            assert!(state.lock().unwrap().events.is_empty());
            assert_eq!(state.lock().unwrap().images(), before);
            assert_eq!(live.catalog_test_wal_state(), wal);
            assert_eq!(live.catalog_test_isolation(), isolation);
            assert_eq!(live.catalog_test_epochs(), epochs);
            assert!(!fence.is_fenced());
            flusher.check_idle().unwrap();
        });
    });
}
