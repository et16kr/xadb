//! One immutable committed view. Allocation and old-root destruction belong to
//! the coordinator outside the publication lock.
use crate::{
    catalog::Root,
    storage::volatile::{ARC_HEADER_BYTES, reserve},
    transaction::{TransactionError, TransactionErrorCode, TransactionResult},
};
use std::{
    mem::size_of,
    sync::{Arc, RwLock},
};
use xadb_foundation::runtime::{Quota, Reservation};

pub(crate) struct CommittedRoot {
    pub position: u64,
    pub root: Arc<Root>,
    _charge: Reservation,
}
impl CommittedRoot {
    pub fn prepare(position: u64, root: Arc<Root>, quota: &Quota) -> TransactionResult<Arc<Self>> {
        let charge = reserve(quota, size_of::<Self>() + ARC_HEADER_BYTES)?;
        Ok(Arc::new(Self {
            position,
            root,
            _charge: charge,
        }))
    }
}

pub(crate) struct Publication(RwLock<Arc<CommittedRoot>>);
impl Publication {
    pub fn new(root: Arc<CommittedRoot>) -> Self {
        Self(RwLock::new(root))
    }
    pub fn snapshot(&self) -> TransactionResult<Arc<CommittedRoot>> {
        self.0.read().map(|root| Arc::clone(&root)).map_err(|_| {
            TransactionError::new(TransactionErrorCode::Poisoned, "publication lock poisoned")
        })
    }
    pub fn replace(&self, prepared: Arc<CommittedRoot>) -> TransactionResult<Arc<CommittedRoot>> {
        let mut current = self.0.write().map_err(|_| {
            TransactionError::new(TransactionErrorCode::Poisoned, "publication lock poisoned")
        })?;
        Ok(std::mem::replace(&mut *current, prepared))
    }
}

#[cfg(test)]
impl Publication {
    /// Deterministic test-only poison without changing the root or running I/O.
    pub(crate) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(|| {
            let _guard = self.0.write().unwrap();
            panic!("publication poison fixture");
        });
    }
}
