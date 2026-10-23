#![allow(dead_code)]
use crate::storage::{dirty_frame::FrameStamp,page::PAGE_BYTES,wal_io::{EmptyTransactionInstall,WalRequirement}};
use crate::transaction::concurrency::PageConcurrency;
fn probe<'a,'c,'s>(core:&'c PageConcurrency<'s>,stamp:FrameStamp,requirement:WalRequirement<'s>,after:&'a [u8;PAGE_BYTES]) {
    let _proof = EmptyTransactionInstall {core,stamp,requirement,after}; // expected diagnostic
}
