#![allow(dead_code)]
use crate::storage::disk::ReservedRow;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(reserved: &ReservedRow<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>) {
    let _ = &reserved.reservation; // expected diagnostic
}
