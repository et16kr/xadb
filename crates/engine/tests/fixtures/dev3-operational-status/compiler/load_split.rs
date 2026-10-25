#![allow(dead_code)]
use crate::storage::{wal_io::OperationalStatus, page::PageRef};
use foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>, page: PageRef,
) {
    let views = operational.split_with_flush();
    let _ = operational.load_status(page); // expected diagnostic
    std::hint::black_box(views);
}
