#![allow(dead_code)]
use crate::storage::{page::PAGE_BYTES, recovery_select::RecoverySelection};
use xadb_foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(directory: &D, scratch: &mut [[u8; PAGE_BYTES]; 3]) {
    let owner = RecoverySelection::acquire(directory, None, scratch).unwrap();
    scratch[0][0] = 0;
    {
        let observation = owner.observation();
        std::hint::black_box((observation.namespace(), observation.profile_id(), observation.bootstrap_plan(), observation.filemap_generation(), observation.doublewrite_generation(), observation.epoch_digest()));
    }
    let moved = owner;
    drop(moved);
    scratch[1][0] = 0;
}
