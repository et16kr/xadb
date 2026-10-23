#![allow(dead_code)]
use crate::storage::wal_io::BoundStatusWithFlush;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut original: BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    let result = original.into_operational();
    let views = original.split_with_flush(); // expected diagnostic
    std::hint::black_box(views);
    drop(result);
}
