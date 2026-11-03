//! An explicit transaction's slot (contract
//! `dev3-module-boundaries.md#explicit-transaction`): the caller holds the
//! transaction between its statements and ends it itself, by Commit or by
//! its own rollback.
//!
//! The live entries (`live::begin_transaction_live`,
//! `live::transaction_insert_live`, `live::commit_transaction_live`,
//! `live::rollback_transaction_live`) run the autocommit executor's steps
//! (module `autocommit`) over this slot instead of `Autocommit`. The
//! transaction is in the caller's holder (`Option`), which may live off the
//! stack; every entry borrows the holder and writes the slot, and the entry
//! that ends the transaction takes it out. A failed statement is rolled back
//! to its own mark and finished, as in the autocommit executor, but the
//! transaction is not ended: the statements before it stand, and the caller
//! goes on with another statement, the Commit or its rollback. After
//! `Fenced` the entry has dropped the transaction into C's fence and
//! recovery obligations (the holder is empty).
//!
//! The cleanup of the ended transaction is the caller's, as after the
//! autocommit executor (`retire_finished`).
use super::{
    Failure,
    autocommit::{Committing, Ending},
};

/// What the last entry of an explicit transaction did; the entry writes it
/// once its outcome is known and the caller reads it after the call.
#[derive(Debug)]
pub(crate) enum Explicit<C> {
    /// No entry has written it.
    Idle,
    /// C began the transaction; it is in the caller's holder.
    Begun,
    /// No transaction: C refused Begin unchanged, or the entry was called
    /// with an empty holder (`OrderMismatch`, no C call).
    NoTransaction(Failure),
    /// Every row of the statement was applied and the statement finished.
    Done { rows: usize },
    /// The statement failed (the Failure is the first one): the rows it
    /// applied were rolled back to its mark and it was finished, or it never
    /// began or ran a row. The transaction goes on; its earlier statements
    /// stand.
    Failed(Failure),
    /// C committed the transaction; `rows` is the count the caller passed
    /// (the rows of its statements that stand).
    Committed { committed: C, rows: usize },
    /// The caller's rollback: the transaction was rolled back and ended.
    RolledBack,
    /// C refused the Commit (the Failure is the Commit's): the transaction
    /// was rolled back and ended, or C closed it (`Rejected`).
    Aborted(Failure),
    /// C answered Fatal or CallerMustResolve, or refused a step on the way
    /// to a statement's or the transaction's end: the entry dropped the
    /// statement and the transaction into C's fence (`Failure::must_resolve`).
    /// The Failure is the first one; a later refusal is its `cleanup` unless
    /// one is already there.
    Fenced(Failure),
}

impl<C> Ending for Explicit<C> {
    fn statement_failed(failure: Failure) -> Self {
        Self::Failed(failure)
    }
    fn failed(failure: Failure) -> Self {
        Self::Aborted(failure)
    }
    fn fenced(failure: Failure) -> Self {
        Self::Fenced(failure)
    }
    fn ending(&self) -> bool {
        matches!(self, Self::Aborted(_) | Self::RolledBack)
    }
    fn is_fenced(&self) -> bool {
        matches!(self, Self::Fenced(_))
    }
    fn take_first(&mut self) -> Option<Failure> {
        match std::mem::replace(self, Self::Idle) {
            Self::NoTransaction(failure)
            | Self::Failed(failure)
            | Self::Aborted(failure)
            | Self::Fenced(failure) => Some(failure),
            Self::Idle
            | Self::Begun
            | Self::Done { .. }
            | Self::Committed { .. }
            | Self::RolledBack => None,
        }
    }
}

impl<C> Committing<C> for Explicit<C> {
    fn committed(committed: C, rows: usize) -> Self {
        Self::Committed { committed, rows }
    }
}
