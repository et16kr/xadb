#![allow(dead_code)]
use crate::storage::disk::ReadPort;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(port: &ReadPort<'_, '_, '_, '_, '_, '_, '_, '_, W>) {
    std::hint::black_box(port.pool); // expected diagnostic
}
