//! Borrowed current-bank adapter shared by original live cache verification and
//! System projected updates. No row or image here grants ownership.
use super::{ARENA_END, BANK_END, JOB_END, PAGE_BYTES, arena_page, control};
use crate::storage::{
    plan4_recovery_base::{self as base, catalog::inventory as xoi},
    retention_page::{JOB_BYTES, JOB_COUNT},
};

pub(in super::super) struct Rows<'a> {
    pub(in super::super) bank: &'a [u8],
    pub(in super::super) cells: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    pub(in super::super) root: &'a [u8; PAGE_BYTES],
    pub(in super::super) allocator: &'a [u8; PAGE_BYTES],
    pub(in super::super) allocation_after: Option<&'a [u8; 1536]>,
}
impl Rows<'_> {
    fn descriptor(&self, slot: usize) -> Option<&[u8; 1536]> {
        let bytes = &control::row(self.bank, slot)[..1536];
        bytes
            .iter()
            .any(|byte| *byte != 0)
            .then(|| bytes.try_into().expect("descriptor"))
    }
}
impl xoi::InventoryRows for Rows<'_> {
    fn transaction(&self, index: usize) -> Option<&[u8; 1536]> {
        self.descriptor(index)
    }
    fn allocation(&self) -> Option<&[u8; 1536]> {
        self.allocation_after
            .or_else(|| self.descriptor(control::ALLOCATION_SLOT))
    }
    fn aging(&self) -> Option<&[u8; 1536]> {
        self.descriptor(control::AGING_SLOT)
    }
    fn root(&self) -> &[u8; PAGE_BYTES] {
        self.root
    }
    fn allocator(&self) -> &[u8; PAGE_BYTES] {
        self.allocator
    }
    fn job(&self, ordinal: usize) -> std::result::Result<&[u8], base::Error> {
        self.cells
            .get(ordinal)
            .map(|cell| &cell[..])
            .ok_or(base::Error::InvalidInput)
    }
}
impl<'a> Rows<'a> {
    pub(super) fn replay(canonical: &'a [u8]) -> Self {
        Self {
            bank: &canonical[JOB_END..BANK_END],
            cells: canonical[ARENA_END..JOB_END]
                .as_chunks::<JOB_BYTES>()
                .0
                .try_into()
                .expect("Jobs"),
            root: arena_page(canonical, 1),
            allocator: arena_page(canonical, 0),
            allocation_after: None,
        }
    }
}
pub(in super::super) fn source_bytes() -> usize {
    std::mem::size_of::<Rows<'_>>()
        + 2 * std::mem::size_of::<std::result::Result<(u64, u64), base::Error>>()
        + xoi::current_source_bytes()
}
