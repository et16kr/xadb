//! Separate RAM bookkeeping owned by the original session. Initialization does
//! not issue an actor, snapshot, transaction, or operation admission.
use super::{ISOLATION_BYTES, IsolationLedger, MutableStorageOwner};
use crate::transaction::concurrency::PageConcurrency;
use foundation::namespace::WriterNamespace;

impl<'a, 'c, 's> IsolationLedger<'a, 'c, 's> {
    /// Called only after the original fresh/recovered activation checks. Old
    /// process metadata is discarded; slot generation starts at one, with no
    /// live slots or charged handles/operations. No I/O or allocation occurs.
    pub(super) fn new<W: WriterNamespace>(
        storage: &'s MutableStorageOwner<'_, '_, W>,
        core: &'c PageConcurrency<'s>,
        bytes: &'a mut [u8; ISOLATION_BYTES],
    ) -> Self {
        bytes.fill(0);
        bytes[..8].copy_from_slice(&1_u64.to_le_bytes());
        Self {
            bytes,
            core,
            binding: &storage.binding,
            fence: storage.session_fence(),
        }
    }
}
