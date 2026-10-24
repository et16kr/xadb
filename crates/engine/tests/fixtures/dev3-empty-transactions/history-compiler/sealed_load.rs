#![allow(dead_code)]
use super::resident::EmptyLoadInstall;
use crate::storage::{dirty_frame::{StatusLoadSnapshot, TargetRequest}, page::{PAGE_BYTES, PageLsn}};
fn probe<'a, 'c, 's>(snapshot: &'a StatusLoadSnapshot<'c, 's>, bytes: &'a [u8; PAGE_BYTES], request: TargetRequest) -> EmptyLoadInstall<'a, 'c, 's> {
    EmptyLoadInstall { snapshot, index: 1, allocator: bytes, bytes, request, lsn: PageLsn(1) }
}
