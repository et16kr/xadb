#![allow(dead_code)]
use crate::storage::disk::DiskSession;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(mut session: DiskSession<'_, '_, '_, '_, '_, '_, '_, '_, W>) {
    let (_, read, _) = session.split();
    drop(session); // expected diagnostic
    std::hint::black_box(read);
}
