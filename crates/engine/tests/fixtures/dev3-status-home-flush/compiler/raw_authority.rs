#![allow(dead_code, unused_imports)]
use super::{flush_io::FlushIo, MutableStorageOwner};
use crate::storage::{dirty_frame::CapturedPage, page::PageLsn};
use foundation::namespace::WriterNamespace;
fn probe<W:WriterNamespace>(io: &mut FlushIo<'_,'_>, storage: &MutableStorageOwner<'_,'_,W>, capture: &CapturedPage<'_,'_>, lsn: PageLsn) {
    // A diagnostic LSN is not a sealed Status target authority.
    // This scope can name the lower bridge without widening it.
    // The other arguments are correctly typed borrowed owners.
    // One E0308 must point to the raw third argument.
    let result = io.prepare_status(storage, capture, &lsn); // expected diagnostic
    drop(result);
}
