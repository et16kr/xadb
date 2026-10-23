#![allow(dead_code)]
use crate::storage::wal_io::ResidentStatusReader;
use crate::transaction::concurrency::Owner;
fn probe<'b,'c,'s,'p,'d>(reader: &ResidentStatusReader<'b,'c,'s,'p,'d>, owner: &mut Owner<'c,'s>) {
    let failed = reader.try_admit(owner).unwrap().read_set::<64>(&[]);
    let next = reader.try_admit(owner); // expected diagnostic
    drop(next); drop(failed);
}
