#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(d: &D, m: &mut [u8;12582912], f: &mut [u8;4194304]) {
    let owner = ClosedRecovery::acquire(d, None, m, f).unwrap();
    let failure = match owner.recover() { Err(e) => e, Ok(_) => return };
    let second = &mut *f; // expected diagnostic
    std::hint::black_box(failure.error());
    std::hint::black_box(second);
}
