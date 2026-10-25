#![allow(dead_code, unused_imports)]
use super::{MutableStorageOwner, plan4_pages::{Plan4Page, Plan4Target}};
use crate::{storage::dirty_frame::DirtyFrame, transaction::concurrency::PageConcurrency};
use foundation::namespace::WriterNamespace;
fn expected_value<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(storage: &'s MutableStorageOwner<'m, 'w, W>, core: &'c PageConcurrency<'s>, frames: &'p mut [DirtyFrame<'d>], operation: &'o mut [u8; 114688]) -> Plan4Target {
    let bound = storage.bind_plan4_pages(core, frames, operation).unwrap();
    let target = bound.target(Plan4Page::Root).unwrap();
    drop(bound);
    std::hint::black_box((target.identity(), target.kind(), target.level()));
    target
}
