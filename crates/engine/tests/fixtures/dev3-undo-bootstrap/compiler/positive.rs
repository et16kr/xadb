#![allow(dead_code)]
use crate::storage::{
    page::PAGE_BYTES,
    session_fence::SessionFence,
    wal_frame::MAX_FRAME_BYTES,
    wal_io::{FreshStorageSession, UndoRole},
};
use foundation::namespace::WriterNamespace;

fn inspect(role: &UndoRole<'_>, fence: &SessionFence) {
    std::hint::black_box((
        role.namespace(),
        role.object(),
        role.generation(),
        role.kind(),
        role.level(),
        role.issued_object_high_water(),
        role.uses_fence(fence),
    ));
}

fn positive<W: WriterNamespace>(owner: FreshStorageSession<'_, W>, fence: &SessionFence) {
    {
        let role = owner.undo_role().unwrap();
        inspect(&role, fence);
    }
    let moved = owner;
    {
        let role = moved.undo_role().unwrap();
        inspect(&role, fence);
    }
    drop(moved);
}

fn constructor<'m, W: WriterNamespace>(
    writer: W,
    slot: &'m mut [u8; MAX_FRAME_BYTES],
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> FreshStorageSession<'m, W> {
    FreshStorageSession::create_undo(writer, 1, slot, scratch).unwrap()
}
