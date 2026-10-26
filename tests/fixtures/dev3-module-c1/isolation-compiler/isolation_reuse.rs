#![allow(dead_code)]
use crate::storage::disk::{EmptyRetentionControlLoan, ISOLATION_BYTES};
use crate::storage::wal_io::Plan4Pages;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn reuse<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(old: Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>, control: &'a mut [u8; 196_608], replay: &'a mut [u8; 1_572_864], isolation: &'a mut [u8; ISOLATION_BYTES]) {
    let session = old.into_disk_session(owner, EmptyRetentionControlLoan { control, replay, isolation });
    isolation[0] = 0; // expected diagnostic
    drop(session);
}
