#![allow(dead_code)]
use crate::storage::dirty_frame::CommitStageTargets;

fn probe(stage: CommitStageTargets<'_, '_, '_, '_, '_, '_, 4>) {
    drop(stage.cancel_unstaged());
    drop(stage.cancel_unstaged()); // expected diagnostic
}
