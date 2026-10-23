#![allow(dead_code)]
use crate::storage::{wal_io::OperationalStatus, page::PageRef};
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>, page: PageRef,
) {
    let views = operational.split_with_flush();
    drop(views);
    let _ = operational.load_status(page);
    let _ = operational.load_status(page);
    std::hint::black_box(operational.split_with_flush());
}
