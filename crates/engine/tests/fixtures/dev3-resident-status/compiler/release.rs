#![allow(dead_code)]
use crate::storage::{wal_io::ResidentStatusReader, page::PageRef};
use crate::transaction::concurrency::Owner;
fn probe<'b,'c,'s,'p,'d>(reader: &ResidentStatusReader<'b,'c,'s,'p,'d>, owner: &mut Owner<'c,'s>, pages: &[PageRef]) {
    let result = reader.try_admit(owner).unwrap().read_set::<64>(pages);
    drop(result);
    let _ = reader.try_admit(owner);
}
