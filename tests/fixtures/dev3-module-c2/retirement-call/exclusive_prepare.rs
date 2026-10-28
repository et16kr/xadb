#![allow(dead_code)]
use crate::storage::disk::DiskSession;
use crate::storage::semantic_catalog::Operation;
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, W: WriterNamespace>(
    live: &mut DiskSession<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>, private: &mut [u8; 425984],
) {
    let _ = live.prepare_catalog_retirement(owner, 0, Operation::Clr, private); // expected diagnostic
}
