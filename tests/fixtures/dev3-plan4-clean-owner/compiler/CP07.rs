#![allow(dead_code, unused_imports)]
use super::plan4_pages::Plan4Pages;
use foundation::namespace::WriterNamespace;
fn extract<W: WriterNamespace>(bound: &Plan4Pages<'_, '_, '_, '_, '_, '_, '_, W>) {
    std::hint::black_box(&bound.pool); // expected diagnostic
}
