#![allow(dead_code)]
use crate::storage::wal_io::ClosedRecovery;
use foundation::namespace::DirectoryPort;
fn probe<D: DirectoryPort>(d: &D, m: &mut [u8;12582912], f: &mut [u8;4194304]) {
    let owner = ClosedRecovery::acquire(d, None, m, f).unwrap();
    let result = owner.recover();
    match result { Ok(i) => { std::hint::black_box((i.prefix(), i.observation(), i.allocator_image(), i.status_image(), i.descriptor_bytes())); drop(i); }, Err(e) => { std::hint::black_box((e.error(), e.effects(), e.observation())); drop(e); } }
    m.fill(0); f.fill(0);
}
