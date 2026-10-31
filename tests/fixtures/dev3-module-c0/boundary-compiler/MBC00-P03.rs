#![allow(dead_code)]
//! MBC00-P03: the recovered facade lends its port, Owner, private lane and
//! progress disjointly to one RowClr task; A's handler consumes it, a Before
//! returns the same task for retry, and the same facade's next_action is
//! called again until READY.
use crate::storage::disk::{
    Plan4Recovered, Plan4RecoveryError, RecoveryAction, RecoveryOwnedError,
};
use crate::storage::row::insert::driver::recovered::{
    RowRecoveryHandlerError, handle_recovered_clr,
};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;

fn drive<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    recovered: &mut Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) -> Result<u64, Plan4RecoveryError> {
    let mut clrs = 0;
    loop {
        match recovered.next_action(owner, flusher)? {
            RecoveryAction::Progress(progress) => {
                let _ = (progress.phase, progress.progressed);
            }
            RecoveryAction::RowClr(task) => {
                let _ = task.actor();
                match handle_recovered_clr(task) {
                    Ok(_) => clrs += 1,
                    Err(RowRecoveryHandlerError::Source { error, task }) => {
                        drop(task);
                        return Err(error);
                    }
                    Err(RowRecoveryHandlerError::Producer { task, .. }) => {
                        drop(task);
                        return Ok(clrs);
                    }
                    Err(RowRecoveryHandlerError::Prepare(RecoveryOwnedError::Before {
                        owned,
                        ..
                    })) => {
                        if handle_recovered_clr(owned).is_ok() {
                            clrs += 1;
                        }
                    }
                    Err(RowRecoveryHandlerError::Apply(RecoveryOwnedError::Before {
                        owned,
                        ..
                    })) => {
                        if handle_recovered_clr(owned.reopen()).is_ok() {
                            clrs += 1;
                        }
                    }
                    Err(RowRecoveryHandlerError::Prepare(RecoveryOwnedError::Fatal(error)))
                    | Err(RowRecoveryHandlerError::Apply(RecoveryOwnedError::Fatal(error))) => {
                        return Err(error);
                    }
                }
            }
            RecoveryAction::Ready => return Ok(clrs),
        }
    }
}
