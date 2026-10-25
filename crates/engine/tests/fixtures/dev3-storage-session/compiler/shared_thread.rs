#![allow(dead_code, unused_imports, unused_variables)]
use crate::storage::{wal_io::FreshStorageSession, page::{PageIdentity, PageKind}, read_buffer::{ReadBuffer, ReadFrame}};
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace + Send>(writer: W, slot: &mut [u8; 1048576], init: &mut [[u8; 16384]; 3], root: &mut [u8; 16384], scratch: &mut [u8; 16384], frames: &mut [ReadFrame], load: &mut [u8; 16384], identity: PageIdentity)
where W::File: Send {
    let session = FreshStorageSession::create(writer, 1, slot, init).unwrap();
    let reader = session.open_reader(root, scratch).unwrap();
    std::thread::scope(|scope| {
        let reading = scope.spawn(move || std::hint::black_box(reader.selector_generation()));
        let writing = scope.spawn(|| std::hint::black_box(session.prefix()));
        std::hint::black_box(reading.join().unwrap());
        std::hint::black_box(writing.join().unwrap());
    });
}
