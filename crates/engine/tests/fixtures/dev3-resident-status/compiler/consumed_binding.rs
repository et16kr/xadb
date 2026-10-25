#![allow(dead_code)]
use crate::storage::wal_io::ResidentStatusBinding;
use foundation::namespace::WriterNamespace;
fn probe<'s,'c,'p,'d:'p,'o,'m,'w,W:WriterNamespace>(binding: ResidentStatusBinding<'s,'c,'p,'d,'o,'m,'w,W>) {
    let first = binding.activate();
    let second = binding.activate(); // expected diagnostic
    drop(first); drop(second);
}
