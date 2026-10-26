#![allow(dead_code)]
use crate::storage::disk::{DiskSession, VisibilityLoan, EPOCH_BYTES, VISIBILITY_BYTES};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn bind<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(mut session: DiskSession<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>, bytes: &'a mut [u8; VISIBILITY_BYTES], epochs: &'a mut [u8; EPOCH_BYTES]) {
    let result = session.enable_catalog_visibility(owner, VisibilityLoan { bytes, epochs });
    drop((result, session));
}
