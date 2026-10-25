#![allow(dead_code)]
use crate::storage::wal_io::OperationalAllocation;
use foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalAllocation<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    let views = operational.split_with_flush();
    drop(views);
    let _ = operational.reload_allocator();
    let next = operational.split_with_flush();
    std::hint::black_box(next);
}
