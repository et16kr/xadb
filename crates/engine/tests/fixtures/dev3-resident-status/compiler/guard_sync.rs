#![allow(dead_code)]
use crate::storage::wal_io::ReadStatusSet;
fn probe<'a,'b,'c,'s,'p,'d>(read: &ReadStatusSet<'a,'b,'c,'s,'p,'d,64>) {
    std::thread::scope(|scope| {
        scope.spawn(move || { std::hint::black_box(read); }); // expected diagnostic
    });
}
