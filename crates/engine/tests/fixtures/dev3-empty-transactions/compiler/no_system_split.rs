#![allow(dead_code)]
use crate::storage::wal_io::{ResidentStatus, EmptyTransactions, IntoEmptyTransactionsError, EMPTY_TRANSACTION_BANK_BYTES};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<W:WriterNamespace>(mut value: EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>) {
    let _ = value.split_with_flush(); // expected diagnostic
}
