#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(d: &D, m: &mut [u8;12582912], f: &mut [u8;4194304]) {
    let owner = ClosedRecovery::acquire(d, None, m, f).unwrap();
    let second = &mut *m; // expected diagnostic
    std::hint::black_box(owner.observation());
    std::hint::black_box(second);
}
