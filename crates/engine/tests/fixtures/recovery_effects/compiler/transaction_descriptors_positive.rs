#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use xadb_foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(d: &D, m: &mut [u8;12582912], f: &mut [u8;4194304]) {
    let i = ClosedRecovery::acquire(d, None, m, f).unwrap().recover().unwrap();
    std::hint::black_box((i.transaction_descriptor_at(0), i.transaction_descriptor_at(31), i.prefix().transaction_high_water()));
    drop(i);
    m.fill(0); f.fill(0);
}
