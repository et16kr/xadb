use crate::{
    Result,
    api::Metrics,
    storage::volatile::{ARC_HEADER_BYTES, reserve},
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use xadb_foundation::runtime::{Quota, Reservation};

pub(crate) struct Counters {
    operations: AtomicU64,
    rows_read: AtomicU64,
    row_references_copied: AtomicU64,
    _charge: Reservation,
}
impl Counters {
    pub fn new(quota: &Quota) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            operations: AtomicU64::new(0),
            rows_read: AtomicU64::new(0),
            row_references_copied: AtomicU64::new(0),
            _charge: reserve(quota, std::mem::size_of::<Self>() + ARC_HEADER_BYTES)?,
        }))
    }
    fn add(counter: &AtomicU64, amount: u64) {
        let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            Some(old.saturating_add(amount))
        });
    }
    pub fn operation(&self) {
        Self::add(&self.operations, 1);
    }
    pub fn read(&self, count: usize) {
        Self::add(&self.rows_read, count as u64);
    }
    pub fn copied(&self, count: usize) {
        Self::add(&self.row_references_copied, count as u64);
    }
    pub fn snapshot(&self, resident: &Quota, pins: &Quota) -> Metrics {
        Metrics {
            operations: self.operations.load(Ordering::Relaxed),
            rows_read: self.rows_read.load(Ordering::Relaxed),
            row_references_copied: self.row_references_copied.load(Ordering::Relaxed),
            resident_bytes: resident.used(),
            snapshot_pins: pins.used(),
        }
    }
}
