#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use xadb_foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(d: &D, m: &mut [u8;12582912], f: &mut [u8;4194304]) {
    let i = ClosedRecovery::acquire(d, None, m, f).unwrap().recover().unwrap();
    std::hint::black_box((i.status_image_at(1), i.status_image_at(2), i.status_image_at(63)));
    drop(i);
    m.fill(0); f.fill(0);
}
