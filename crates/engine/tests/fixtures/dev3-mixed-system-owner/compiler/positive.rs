#![allow(dead_code)]
use super::status_init::{MixedSystem, SystemPage, SystemPageKind};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn use_mixed<'c, 's, W: WriterNamespace>(
    bound: &mut MixedSystem<'s, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>, target: SystemPage,
) {
    let (mut writer, reader, mut flush) = bound.split();
    let reserved = writer.reserve(owner, SystemPageKind::UndoRoot).unwrap();
    std::hint::black_box(reserved.target());
    drop(reserved.cancel());
    if let Ok(read) = reader.read(owner, target) {
        std::hint::black_box(read.bytes());
        drop(read.release_checked());
    }
    drop(flush.try_flush(owner, target));
}
