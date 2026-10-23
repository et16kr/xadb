#![allow(dead_code)]
use crate::storage::wal_io::OperationalAllocation;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    let mut guard = operational.evict_allocator().unwrap();
    let views = operational.split_with_flush(); // expected diagnostic
    guard.load_allocator(0).unwrap();
    std::hint::black_box(views);
}
