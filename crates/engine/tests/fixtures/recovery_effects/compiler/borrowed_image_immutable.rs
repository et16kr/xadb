#![allow(dead_code)]
use crate::storage::wal_io::RecoveryInspection;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(owner: RecoveryInspection<'_, '_, W>) {
    let image = owner.allocator_image();
    image[0] = 0; // expected diagnostic
}
