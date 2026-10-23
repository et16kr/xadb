#![allow(dead_code)]
use crate::storage::wal_io::{ResidentStatus, EmptyTransactions, IntoEmptyTransactionsError, EMPTY_TRANSACTION_BANK_BYTES};
use crate::transaction::concurrency::Owner;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'b,'s,'c,'p,'d:'p,'o,'m,'w,W:WriterNamespace>(mut bound: ResidentStatus<'s,'c,'p,'d,'o,'m,'w,W>, owner: &mut Owner<'c,'s>, bank: &'b mut [u8;EMPTY_TRANSACTION_BANK_BYTES]) {
    let result = bound.into_empty_transactions(owner, bank);
    bank.fill(7); // expected diagnostic
    drop(result);
}
