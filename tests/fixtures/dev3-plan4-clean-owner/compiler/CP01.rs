#![allow(dead_code, unused_imports)]
use super::{MutableStorageOwner, plan4_pages::{Plan4LoadPermit, Plan4Page, Plan4Pages, Plan4ResidencyCursor, Plan4Target}};
use crate::{storage::{dirty_frame::{DirtyFrame, Plan4LoadSnapshot, SharedPool, TargetRequest}, page::PAGE_BYTES}, transaction::concurrency::{Owner, PageConcurrency}};
use foundation::{identity::ObjectId, namespace::WriterNamespace};
fn legal<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(storage: &'s MutableStorageOwner<'m, 'w, W>, core: &'c PageConcurrency<'s>, frames: &'p mut [DirtyFrame<'d>], operation: &'o mut [u8; 114688], owner: &mut Owner<'c, 's>) {
    let mut bound = storage.bind_plan4_pages(core, frames, operation).unwrap();
    let target = bound.target(Plan4Page::Root).unwrap();
    bound.load_into(1, &target).unwrap();
    let mut cursor = Plan4ResidencyCursor::new();
    bound.residency_step(&target, &mut cursor).unwrap();
    bound.inspect_genesis_retention(owner, &mut cursor).unwrap();
    bound.lookup_catalog_empty(owner, ObjectId::new(6).unwrap(), &mut cursor).unwrap();
    drop(bound);
    std::hint::black_box(&mut *frames);
    std::hint::black_box(&mut *operation);
}
fn legal_symbols<W: WriterNamespace>(target: Plan4Target, permit: Plan4LoadPermit<'_, '_, '_>, snapshot: &Plan4LoadSnapshot<'_, '_>, pool: &SharedPool<'_, '_, '_, '_>, storage: &MutableStorageOwner<'_, '_, W>, request: TargetRequest) {
    std::hint::black_box((target.identity(), target.kind(), target.level()));
    std::hint::black_box((permit.snapshot(), permit.index(), permit.allocator(), permit.bytes(), permit.request()));
    std::hint::black_box((snapshot, pool, storage, request));
}
