#![allow(dead_code)]
use crate::storage::wal_io::OperationalAllocation;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    let views = operational.split_with_flush();
    let _ = operational.evict_allocator(); // expected diagnostic
    std::hint::black_box(views);
}
