//! S9 C+B no-loser inputs MCB05-N01..06 (consumers §8.3 `#acceptance`,
//! boundaries `#row-mi03-s9`): each input is C's actual writer run on a
//! fresh namespace stopped at its cut, made without A. N01's Forward is the
//! S8 independent literal (`dev3-module-c2/row-read/`) through C's checked
//! ingress; N02 stops after C's RowReserve of that literal, N03 after the
//! Row Begin and the first statement with no reservation, N04 after the
//! transaction RollbackStart of the empty head, N05 after a fresh
//! installation's original SystemPage reservation (no Row or Catalog actor,
//! no root), N06 after the original Catalog CREATE reservation of T.
//! `RejectInitialLosers` refuses each at Analysis (Unsupported,
//! CallerMustResolve with the lease acquired, database Unchanged) before
//! any physical, REBASE, Cancel, Abort or CLR step, and no file changes.
//!
//! The same producer runs in memory (`s9b_m08_*`) and as the native
//! `produce` process of `native::s9b_no_loser_worker`, whose cut is the
//! process exit at that point (no flush, no shutdown, no drop). S6R-M08
//! keeps its C+A+B inputs (A's INSERT) unchanged.
use super::super::super::RowRecoveryPolicy;
use super::super::support::{assert_closed, clone_directory};
use super::row_literal::Literal;
use super::row_preflight::create_table;
use super::row_reopen::refused_in;
use super::*;
use crate::storage::disk::{
    IsolationLevel, RollbackScope, RowOperation, RowOwnedError, RowPreflightStep, TableStep,
};
use std::time::Duration;

#[path = "row_no_loser/native.rs"]
mod native;

/// The no-loser inputs, in the consumers' order.
pub(super) const CASES: [&str; 6] = [
    "MCB05-N01",
    "MCB05-N02",
    "MCB05-N03",
    "MCB05-N04",
    "MCB05-N05",
    "MCB05-N06",
];

/// Each restart's bound (S6's READY bound).
const BOUND: Duration = Duration::from_secs(30);

/// A successful Row step.
macro_rules! step {
    ($result:expr, $what:literal) => {
        match $result {
            Ok(done) => done,
            Err(RowOwnedError::Before { error, .. }) | Err(RowOwnedError::Fatal(error)) => {
                panic!("{} {error:?}", $what)
            }
        }
    };
}

/// C's actual run of the no-loser input `id` on a fresh namespace over
/// `directory`, stopped at its cut: `cut` runs at the crash point (memory:
/// the files are taken; native: the receipt is written and the process
/// exits). In memory the run then ends as S6's builders do: a reservation
/// is cancelled, everything else is dropped.
pub(super) fn produce<D: DirectoryPort>(id: &str, directory: &D, cut: &mut dyn FnMut()) {
    with_directory!(directory, |live,
                                owner,
                                flusher,
                                status,
                                undo,
                                private,
                                reader_private,
                                _core,
                                _fence| {
        match id {
            "MCB05-N05" => {
                let _ = (status, undo, private, reader_private);
                let mut cursor = Plan4ResidencyCursor::new();
                for _ in 0..32 {
                    match live.prepare_undo_slot(owner, &mut cursor).unwrap() {
                        None => break,
                        Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                            drive_flush(live, flusher, Some(&target))
                        }
                        Some(_) => panic!("unexpected System slot preparation"),
                    }
                }
                let reserved = live.reserve_undo(owner).unwrap();
                cut();
                drop(reserved);
            }
            "MCB05-N06" => {
                let _ = reader_private;
                prepare_create(live, owner, flusher, status, undo);
                let (mut writer, _, mut flush) = live.split();
                let tx = writer
                    .begin_catalog_transaction(
                        owner,
                        IsolationLevel::Serializable,
                        status,
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let view = writer.begin_statement(owner, &tx).unwrap();
                let definition = table_spec("T");
                let reserved = writer
                    .reserve_catalog_create(owner, &view, undo, &definition, private, &mut flush)
                    .ok()
                    .unwrap();
                cut();
                let _ = reserved.cancel();
                drop(view);
                drop(tx);
            }
            _ => {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                row_statement!(live, owner, flusher, private, reader_private, row_status
                    => writer, reader, flush, row, view, table);
                let _ = reader;
                match id {
                    "MCB05-N03" => cut(),
                    "MCB05-N04" => {
                        assert!(writer.finish_statement(owner, view).is_ok());
                        let rollback = writer
                            .start_row_rollback(
                                owner,
                                &row,
                                None,
                                RollbackScope::Transaction,
                                private,
                                &mut flush,
                                flusher,
                            )
                            .unwrap();
                        cut();
                        drop(rollback);
                        drop(row);
                        return;
                    }
                    "MCB05-N01" | "MCB05-N02" => {
                        let literal = Literal::load();
                        let _ = settle!(writer, owner, view, table, private, flush, flusher);
                        let mut preflight = match writer
                            .preflight_row(owner, &view, &table, private, &mut flush)
                            .unwrap()
                        {
                            RowPreflightStep::Ready(preflight) => preflight,
                            RowPreflightStep::FlushRequired { .. } => panic!("settled sources"),
                        };
                        let plan = literal.plan(preflight.validation_lane().unwrap());
                        let mut reserved = step!(preflight.reserve(plan), "reserve");
                        if id == "MCB05-N02" {
                            cut();
                            step!(reserved.cancel(), "cancel");
                        } else {
                            let proposal = literal.forward(reserved.producer_lane().unwrap());
                            let prepared = step!(reserved.prepare(proposal), "prepare");
                            let applied = step!(prepared.apply(), "apply");
                            assert_eq!(applied.into_parts().0.operation(), RowOperation::Forward);
                            cut();
                        }
                    }
                    _ => panic!("no-loser input {id}"),
                }
                drop(view);
                drop(row);
            }
        }
    })
}

/// The memory files of `produce(id)` at its cut.
fn input(id: &str) -> Images {
    let directory = port::Directory::fresh();
    let state = directory.state();
    {
        let mut state = state.lock().unwrap();
        state.live_writes = true;
        state.recovery_writes = true;
    }
    let mut images = None;
    produce(id, &directory, &mut || {
        images = Some(state.lock().unwrap().images())
    });
    images.expect("the run reached its cut")
}

/// MCB05-N01..06 in memory (the C+B form of S6R-M08): RejectInitialLosers
/// refuses each C+B input after the canonical analysis, every file
/// unchanged.
#[test]
fn s9b_m08_reject_initial_losers_c_b_inputs() {
    for id in CASES {
        case_within(&format!("S9B-M08-{id}"), BOUND, || {
            let images = input(id);
            let directory = clone_directory(&images);
            let state = directory.state();
            refused_in(&directory, Some(RowRecoveryPolicy::RejectInitialLosers));
            assert_closed(&state);
            assert_eq!(
                &state.lock().unwrap().images(),
                &images,
                "every file unchanged"
            );
            println!("S9B_M08 {id} refused");
        });
    }
}
