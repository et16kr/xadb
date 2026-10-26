#![allow(dead_code)]
use crate::storage::disk::{MutationPort, TransactionOptions};
use crate::transaction::concurrency::Owner;
use foundation::namespace::WriterNamespace;
fn consume<'b,'a,'s,'c,'p,'d,'o,'m,'w,W:WriterNamespace>(writer: &mut MutationPort<'b,'a,'s,'c,'p,'d,'o,'m,'w,W>, owner: &mut Owner<'c,'s>, private: &mut [u8;425_984]) {
    let transaction = writer.begin_transaction(owner, TransactionOptions { read_only:true, ..TransactionOptions::default() }, private).unwrap();
    let view = writer.begin_statement(owner, &transaction).unwrap();
    drop(writer.finish_read_only(owner,transaction)); // REJECT_ACTIVE_VIEW
    let _ = view.q();
}
