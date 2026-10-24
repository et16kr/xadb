#![allow(dead_code)]
use crate::storage::{page::PageRef, wal_io::EmptyTransactions};
use crate::transaction::concurrency::Owner;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'c, 's, W: WriterNamespace>(value: &mut EmptyTransactions<'_, 's, 'c, '_, '_, '_, '_, '_, W>, owner: &mut Owner<'c, 's>, page: PageRef) {
    let _ = value.flush_status(owner, page);
    let _ = value.continue_status_flush(owner);
}
