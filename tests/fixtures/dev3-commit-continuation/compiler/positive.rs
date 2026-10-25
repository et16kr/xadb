#![allow(dead_code)]
use crate::storage::dirty_frame::WriteTargets;
use crate::transaction::concurrency::Owner;

fn physical(targets: WriteTargets<'_, '_, '_, '_, '_, '_, 4>) {
    let stage = targets.prepare_commit().unwrap();
    drop(stage.cancel_unstaged());
}

fn core<'c, 's>(owner: &mut Owner<'c, 's>) {
    let version = owner.try_apply().unwrap().try_version().unwrap();
    let stage = match version.prepare_commit() {
        Ok(stage) => stage,
        Err(failure) => {
            drop(failure.version.release_checked());
            return;
        }
    };
    drop(stage.cancel_unstaged());
    drop(owner.try_apply().unwrap().try_version().unwrap().release_checked());
}
