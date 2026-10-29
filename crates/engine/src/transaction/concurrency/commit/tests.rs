//! Actual pre-stage admission only. No fake semantic Commit receipt is issued.
use super::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn version<'o, 'c, 'f>(owner: &'o mut Owner<'c, 'f>) -> VersionEditGuard<'o, 'c, 'f> {
    owner.try_apply().unwrap().try_version().unwrap()
}
fn prepare<'o, 'c, 'f>(guard: VersionEditGuard<'o, 'c, 'f>) -> CommitStage<'o, 'c, 'f> {
    match guard.prepare_commit() {
        Ok(stage) => stage,
        Err(_) => panic!("healthy foreground intent"),
    }
}

#[test]
fn commit_common_prestage_intent_and_checked_cleanup() {
    // Independent fixed states: successful intent/cancel, maintenance rejection,
    // exhausted generation, explicit abandon, unwind. All are before WAL stage.
    for case in 0..5 {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let role = if case == 1 {
            OwnerRole::Maintenance
        } else {
            OwnerRole::Foreground
        };
        let mut owner = core.try_owner(role).unwrap();
        if case == 2 {
            core.state.lock().unwrap().installation_generation = u64::MAX;
        }
        if case == 1 || case == 2 {
            let error = match version(&mut owner).prepare_commit() {
                Ok(_) => panic!("preflight must reject"),
                Err(error) => error,
            };
            assert_eq!(error.error.effect, Effect::Unchanged);
            assert_eq!(
                error.error.cause,
                if case == 1 {
                    Cause::InvalidRole
                } else {
                    Cause::GenerationExhausted
                }
            );
            assert!(matches!(
                core.version.try_lock(),
                Err(TryLockError::WouldBlock)
            ));
            {
                let state = core.state.lock().unwrap();
                assert_eq!(state.installation, Installation::Open);
                assert_eq!((state.active_apply, state.active_content), (1, 0));
            }
            error.version.release_checked().unwrap();
        } else if case == 4 {
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _stage = prepare(version(&mut owner));
                    panic!("new pre-stage intent unwind");
                }))
                .is_err()
            );
        } else {
            let stage = prepare(version(&mut owner));
            assert!(stage.matches_core(&core));
            let foreign = PageConcurrency::new(&fence).unwrap();
            assert!(!stage.matches_core(&foreign));
            {
                let state = core.state.lock().unwrap();
                assert_eq!(state.installation_generation, 1);
                assert_eq!(
                    state.installation,
                    Installation::Intent(InstallationKey {
                        slot: 0,
                        owner_generation: 1,
                        installation_generation: 1
                    })
                );
                assert_eq!((state.active_apply, state.active_content), (1, 0));
            }
            let mut other = core.try_owner(OwnerRole::Foreground).unwrap();
            let rejected = match other.try_apply() {
                Ok(_) => panic!("intent excludes apply"),
                Err(error) => error,
            };
            assert_eq!(rejected.cause, Cause::InstallationPending);
            other.try_content().unwrap().release_checked().unwrap();
            drop(other);
            if case == 0 {
                stage.cancel_unstaged().unwrap();
            } else {
                assert_eq!(
                    stage.abandon().unwrap_err().effect,
                    Effect::CallerMustResolve
                );
            }
        }
        let state = core.state.lock().unwrap();
        assert_eq!((state.active_apply, state.active_content), (0, 0));
        assert_eq!(state.slots[owner.index].mode, Mode::Idle);
        if case >= 3 {
            assert!(fence.is_fenced());
            assert_eq!(
                state.installation,
                Installation::Abandoned(
                    InstallationKey {
                        slot: 0,
                        owner_generation: 1,
                        installation_generation: 1
                    },
                    None
                )
            );
        } else {
            assert!(!fence.is_fenced());
            assert_eq!(state.installation, Installation::Open);
        }
        drop(state);
        drop(owner);
    }
    println!("B1_CORE_PRESTAGE cases=5 actual_commit=NOT_RUN");
}
