#![allow(dead_code)]
use crate::storage::wal_io::{ResidentStatusBinding, IntoFlushError};
use xadb_foundation::namespace::WriterNamespace;
fn probe<'s,'c,'p,'d:'p,'o,'m,'w,W:WriterNamespace>(binding: ResidentStatusBinding<'s,'c,'p,'d,'o,'m,'w,W>) {
    match binding.activate() {
        Err(IntoFlushError::Before {bound,..}) => { let _ = bound.activate(); },
        _ => (),
    }
}
