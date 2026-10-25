#![allow(dead_code)]
use crate::storage::wal_io::RecoveryPrefix;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoveryPrefix<'_, W>) {
    let image = owner.allocator_image();
    image[0] = 0; // expected diagnostic
}
