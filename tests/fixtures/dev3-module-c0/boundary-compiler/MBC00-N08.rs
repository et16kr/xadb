#![allow(dead_code)]
use crate::storage::disk::{MutationPort, RowTransaction};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>,
    transaction: RowTransaction<'b, 'a, 'c, 's>, private: &mut [u8; 425_984],
) {
    let Ok(view) = mutation.begin_statement(owner, &transaction) else { return };
    let _ = mutation.commit_row(owner, transaction, private); // expected diagnostic
    drop(view);
}
