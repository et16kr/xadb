#![allow(dead_code)]
use crate::storage::wal_io::RecoveryPrefix;
use foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(directory: &D, workspace: &mut [u8; 262144]) {
    let owner = RecoveryPrefix::acquire(directory, None, workspace).unwrap();
    drop(workspace); // expected diagnostic
    std::hint::black_box(owner.observation());
}
