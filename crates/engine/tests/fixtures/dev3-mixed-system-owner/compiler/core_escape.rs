#![allow(dead_code)]
use super::status_init::MixedSystem;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(bound: MixedSystem<'_, '_, '_, '_, '_, '_, '_, W>) {
    std::hint::black_box(bound.inner); // expected diagnostic
}
