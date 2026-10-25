#![allow(dead_code)]
use crate::transaction::concurrency::Owner;

fn probe<'c, 's>(owner: &mut Owner<'c, 's>) {
    let version = owner.try_apply().unwrap().try_version().unwrap();
    let stage = match version.prepare_commit() {
        Ok(stage) => stage,
        Err(failure) => {
            drop(failure.version.release_checked());
            return;
        }
    };
    std::hint::black_box(&mut *owner); // expected diagnostic
    drop(stage.cancel_unstaged());
}
