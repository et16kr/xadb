#![allow(dead_code, unused_imports)]
use crate::storage::{dirty_frame::{DirtyFrame, SharedPool, TargetRequest, FrameStamp, ReadTargets, WriteTargets}, page::{PAGE_BYTES, PageLsn}, wal_io::{DoublewriteStorageSession, DirtyInstallPermit, WalFrameLocation}};
use crate::transaction::concurrency::{PageConcurrency, Owner, OwnerRole};
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace>(installed: DoublewriteStorageSession<'_, '_, W>, payload: &mut [u8; PAGE_BYTES], workspace: &mut [u8; 98304], request: TargetRequest) {
    let storage = installed.into_mutable_storage(); let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(payload)];
    let mut bound = storage.bind_allocator(&core, &mut frames, workspace).unwrap();
    {
        let (mut writer, reader) = bound.split();
        fn sync_view<T: Sync>(_: &T) {}
        sync_view(&reader);
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        writer.reserve_pages(&mut owner, 1).unwrap();
        let read = reader.read(&mut owner, &request).unwrap();
        let mut observed = [0; PAGE_BYTES];
        read.copy_snapshot(0, &mut observed).unwrap();
        read.release_checked().unwrap();
        writer.cancel_reserved(&mut owner).unwrap();
        std::thread::scope(|scope| {
            let reader = &reader; let core = &core;
            scope.spawn(move || {
                let mut owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                reader.observe_dirty(&mut owner, &request).unwrap();
            });
        });
        drop(owner);
    }
    { let (writer, reader) = bound.split(); drop((writer, reader)); }
    drop(bound); drop(frames); drop(core); drop(storage);
    payload[0] = 0; workspace[0] = 0;
}
fn consume_permit(mut write: WriteTargets<'_, '_, '_, '_, '_, '_>, permit: DirtyInstallPermit<'_, '_, '_>, stamp: FrameStamp) {
    let dpt = write.preflight_dpt(0, &stamp).unwrap();
    write.install_page(0, &permit).unwrap();
    write.install_metadata(0, &permit).unwrap();
    write.install_dpt(dpt, &permit).unwrap();
    write.release_checked().unwrap();
}
