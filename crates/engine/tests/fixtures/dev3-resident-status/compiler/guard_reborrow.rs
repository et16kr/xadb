#![allow(dead_code)]
use crate::storage::{wal_io::ResidentStatusReader, page::PageRef};
use crate::transaction::concurrency::Owner;
fn probe<'b,'c,'s,'p,'d>(reader: &ResidentStatusReader<'b,'c,'s,'p,'d>, owner: &mut Owner<'c,'s>, pages: &[PageRef]) {
    let read = reader.try_admit(owner).unwrap().read_set::<3>(pages).unwrap();
    let next = reader.try_admit(owner); // expected diagnostic
    drop(next); drop(read);
}
