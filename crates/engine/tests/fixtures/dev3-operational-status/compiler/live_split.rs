#![allow(dead_code)]
use crate::storage::wal_io::OperationalStatus;
use foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    let views = operational.split_with_flush();
    let _ = operational.prepare_next_status(); // expected diagnostic
    std::hint::black_box(views);
}
