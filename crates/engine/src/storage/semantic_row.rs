//! Scope3 Row observations. These pure inputs do not grant admission, statement
//! ownership, WAL staging, installation, history or selected recovery authority.
#![allow(dead_code)] // The current-profile semantic and live owners are pending.

pub(crate) mod ledger;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
    Exhausted,
    Queue(super::retention_page::RetentionError),
    Descriptor(super::obligation::ObligationError),
}
type Result<T> = std::result::Result<T, Error>;
impl From<super::retention_page::RetentionError> for Error {
    fn from(value: super::retention_page::RetentionError) -> Self {
        Self::Queue(value)
    }
}
impl From<super::obligation::ObligationError> for Error {
    fn from(value: super::obligation::ObligationError) -> Self {
        Self::Descriptor(value)
    }
}
