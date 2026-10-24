#![allow(dead_code)]
use super::status_init::{MixedSystem, StatusBootstrapWriter};
use xadb_foundation::namespace::WriterNamespace;
fn old<W: WriterNamespace>(_: StatusBootstrapWriter<'_, '_, '_, '_, '_, '_, '_, '_, W>) {}
fn probe<W: WriterNamespace>(bound: &mut MixedSystem<'_, '_, '_, '_, '_, '_, '_, W>) {
    old(bound.split().0); // expected diagnostic
}
