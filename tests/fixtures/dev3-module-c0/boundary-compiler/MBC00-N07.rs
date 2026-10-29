#![allow(dead_code)]
use crate::storage::disk::{CallerCredit, MutationPort, RowStatementView};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
struct Cursor<'r, 't, 'v, 'a, 'c, 's> { view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>, credit: CallerCredit<'r, 'a, 'c, 's> }
fn probe<'t, 'v, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>(
    mutation: &mut MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, owner: &mut Owner<'c, 's>,
    view: RowStatementView<'t, 'v, 'a, 'c, 's>,
) {
    let Ok(credit) = view.reserve_cursor_credit() else { return };
    let cursor = Cursor { view: &view, credit };
    let _ = mutation.finish_statement(owner, view); // expected diagnostic
    drop(cursor);
}
