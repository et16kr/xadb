use crate::storage::disk::{Plan4Recovered, RecoveryAction};
use crate::transaction::concurrency::Owner;
fn held<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: foundation::namespace::WriterNamespace>(r: &mut Plan4Recovered<'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>, o: &mut Owner<'c, 's>, f: &mut Owner<'c, 's>) {
    if let Ok(RecoveryAction::RowClr(task)) = r.next_action(o, f) {
        let _ = r.next_action(o, f); // expected diagnostic
        drop(task);
    }
}
