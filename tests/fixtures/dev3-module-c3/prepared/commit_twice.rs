#![allow(dead_code)]
use crate::storage::disk::{MutationPort, PreparedCatalog};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, W: WriterNamespace>(
    writer: &mut MutationPort<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>, value: PreparedCatalog<'_, 'c, 's>,
) {
    let _ = writer.commit_catalog(owner, value);
    let _ = writer.commit_catalog(owner, value);
}
