#![allow(dead_code)]
use super::status_init::{MixedSystem, SystemPage};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn probe<'c, 's, W: WriterNamespace>(bound: &mut MixedSystem<'s, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>, target: SystemPage) {
    let (_, reader, _) = bound.split();
    let first = reader.read(owner, target).unwrap();
    let second = reader.read(owner, target); // expected diagnostic
    std::hint::black_box(first.bytes());
    drop(second);
}
