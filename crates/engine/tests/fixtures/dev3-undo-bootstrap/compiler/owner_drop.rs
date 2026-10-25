#![allow(dead_code)]
use crate::storage::wal_io::FreshStorageSession;
use foundation::namespace::WriterNamespace;

fn probe<W: WriterNamespace>(owner: FreshStorageSession<'_, W>) {
    let role = owner.undo_role().unwrap();
    drop(owner); // expected diagnostic
    std::hint::black_box(role.namespace());
}
