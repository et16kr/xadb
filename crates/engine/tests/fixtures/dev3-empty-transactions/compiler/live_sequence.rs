#![allow(dead_code)]
use crate::storage::{page::PageRef,wal_io::EmptyTransactions};
use crate::transaction::concurrency::Owner;
use xadb_foundation::namespace::WriterNamespace;
fn probe<'c,'s,W:WriterNamespace>(value:&mut EmptyTransactions<'_,'s,'c,'_,'_,'_,'_,'_,W>,owner:&mut Owner<'c,'s>,page:PageRef) {
    if let Ok(first)=value.begin(owner,page) {
        let retained_observation=first;
        let _=value.rollback_start(owner,first.actor);
        let _=value.abort_end(owner,first.actor);
        let _=retained_observation;
    }
}
