#![allow(dead_code, unused_imports, unused_variables)]
use crate::storage::wal_io::FreshStorageSession;
use foundation::namespace::WriterNamespace;
fn probe<W: WriterNamespace + Send>(writer: W, slot: &mut [u8; 1048576], init: &mut [[u8; 16384]; 3], workspace: &mut [u8; 4194304], root: &mut [u8; 16384], scratch: &mut [u8; 16384])
where W::File: Send {
    let fresh = FreshStorageSession::create(writer, 1, slot, init).unwrap();
    let installed = fresh.install_doublewrite(workspace).unwrap();
    let reader = installed.open_reader(root, scratch).unwrap();
    std::thread::scope(|scope| {
        let reading = scope.spawn(move || std::hint::black_box(reader.selector_generation()));
        let observing = scope.spawn(|| std::hint::black_box(installed.is_fenced()));
        std::hint::black_box(reading.join().unwrap());
        std::hint::black_box(observing.join().unwrap());
    });
}
