#![allow(dead_code, unused_imports)]
use super::{MutableStorageOwner, plan4_pages::Plan4Pages};
use crate::{storage::dirty_frame::DirtyFrame, transaction::concurrency::PageConcurrency};
use foundation::namespace::WriterNamespace;
fn escape<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(storage: &'s MutableStorageOwner<'m, 'w, W>, core: &'c PageConcurrency<'s>, frames: &'p mut [DirtyFrame<'d>]) -> Plan4Pages<'s, 'c, 'p, 'd, 'o, 'm, 'w, W> {
    let mut operation = [0u8; 114688];
    storage.bind_plan4_pages(core, frames, &mut operation).unwrap() // expected diagnostic
}
