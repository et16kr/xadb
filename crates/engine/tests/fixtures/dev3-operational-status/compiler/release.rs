#![allow(dead_code)]
use crate::storage::wal_io::OperationalStatus;
use foundation::namespace::WriterNamespace;
fn probe<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    mut operational: OperationalStatus<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    let views = operational.split_with_flush();
    drop(views);
    let _ = operational.prepare_next_status();
    std::hint::black_box(operational.split_with_flush());
}

fn returned_owner<'s, 'c, 'p, 'd: 'p, 'o, 'm, 'w, W: WriterNamespace>(
    original: crate::storage::wal_io::BoundStatusWithFlush<'s, 'c, 'p, 'd, 'o, 'm, 'w, W>,
) {
    use crate::storage::wal_io::IntoOperationalStatusError;
    match original.into_operational() {
        Ok(value) => drop(value),
        Err(IntoOperationalStatusError::Before { mut bound, .. }) => {
            std::hint::black_box(bound.split_with_flush());
        }
        Err(IntoOperationalStatusError::Fatal(_)) => (),
    }
}
fn released_scratch<W: WriterNamespace>(
    installed: crate::storage::wal_io::DoublewriteStorageSession<'_, '_, W>,
    allocator: &mut [u8; 16384], payload: &mut [u8; 16384], operation: &mut [u8; 114688],
) {
    use crate::storage::dirty_frame::DirtyFrame;
    use crate::transaction::concurrency::PageConcurrency;
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(allocator), DirtyFrame::new(payload)];
    let original = storage.bind_status_bootstrap(&core, &mut frames, operation).unwrap();
    let flush = match original.into_with_flush() { Ok(value) => value, Err(_) => panic!() };
    let operational = match flush.into_operational() { Ok(value) => value, Err(_) => panic!() };
    drop(operational);
    operation.fill(0);
}
