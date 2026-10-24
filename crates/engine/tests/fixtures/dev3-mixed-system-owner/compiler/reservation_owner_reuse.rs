#![allow(dead_code)]
use super::status_init::{MixedSystem, SystemPageKind};
use crate::transaction::concurrency::Owner;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'c, 's, W: WriterNamespace>(bound: &mut MixedSystem<'s, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>) {
    let (mut writer, _, _) = bound.split();
    let reserved = writer.reserve(owner, SystemPageKind::UndoRoot).unwrap();
    std::hint::black_box(&mut *owner); // expected diagnostic
    std::hint::black_box(reserved.target());
}
