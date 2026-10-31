use crate::storage::disk::{Plan4Recovered, RecoveryAction};
use crate::storage::row::insert::driver::recovered::handle_recovered_clr;
use crate::transaction::concurrency::Owner;
fn reuse<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: foundation::namespace::WriterNamespace>(r: &mut Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, o: &mut Owner<'c, 's>, f: &mut Owner<'c, 's>) {
    if let Ok(RecoveryAction::RowClr(task)) = r.next_action(o, f) {
        let _ = handle_recovered_clr(task);
        let _ = handle_recovered_clr(task); // expected diagnostic
    }
}
