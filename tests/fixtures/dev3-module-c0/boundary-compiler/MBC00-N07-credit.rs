#![allow(dead_code)]
use crate::storage::disk::{MutationPort, RowStatementView};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'t, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>,
    view: RowStatementView<'t, 'v, 'a, 'c, 's>,
) {
    let Ok(credit) = view.reserve_cursor_credit() else { return };
    let _ = mutation.finish_statement(owner, view); // expected diagnostic
    let _ = credit.release_checked();
}
