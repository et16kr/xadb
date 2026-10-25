#![allow(dead_code)]
use crate::storage::wal_io::RecoveryPrefix;
use foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(directory: &D, workspace: &mut [u8; 262144]) {
    let owner = RecoveryPrefix::acquire(directory, None, workspace).unwrap();
    std::hint::black_box((owner.observation(), owner.allocator_image(), owner.status_image(), owner.descriptor_bytes()));
    let moved = owner;
    std::hint::black_box(moved.allocator_image());
    drop(moved);
    workspace.fill(0);
}
