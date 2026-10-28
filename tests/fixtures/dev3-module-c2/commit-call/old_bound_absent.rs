#![allow(dead_code)]
use crate::storage::disk::DiskSession;
use crate::storage::transaction_record::TxnRef;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, W: WriterNamespace>(
    bound: &mut DiskSession<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>, actor: TxnRef, private: &mut [u8; 425984],
) {
    let _ = bound.prepare_catalog_commit(owner, actor, private);
}
