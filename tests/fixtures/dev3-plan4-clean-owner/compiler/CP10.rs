#![allow(dead_code, unused_imports)]
use super::plan4_pages::Plan4LoadPermit;
use crate::storage::{dirty_frame::{Plan4LoadSnapshot, TargetRequest}, page::PAGE_BYTES};
fn fabricate<'a, 'c, 's>(snapshot: &'a Plan4LoadSnapshot<'c, 's>, allocator: &'a [u8; PAGE_BYTES], bytes: &'a [u8; PAGE_BYTES], request: TargetRequest) -> Plan4LoadPermit<'a, 'c, 's> {
    Plan4LoadPermit { snapshot, index: 1, allocator, bytes, request } // expected diagnostic
}
