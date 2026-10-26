//! Test-only observation of the actual retained byte bank. This lends no
//! mutation, constructor, requirement, or authority and copies no caller loan.
use super::*;

impl<'s, 'c, 'p, 'd, W: WriterNamespace> Plan4Retention<'_, 's, 'c, 'p, 'd, '_, '_, '_, W> {
    /// The original pool is only borrowed for test observations or its existing
    /// checked methods. No copied registration or capability is constructed.
    pub(in crate::storage) fn catalog_test_pool(&self) -> &SharedPool<'c, 's, 'p, 'd> {
        &self.inner.pool
    }
}

impl<W: WriterNamespace> RetentionReader<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Actual published Q and registered snapshot horizon H of the original
    /// visibility binding; a reader in a Commit gap cannot borrow the Live.
    pub(in crate::storage) fn catalog_test_horizon(&self) -> (u64, u64) {
        self.visibility
            .expect("actual visibility binding")
            .test_horizon()
    }
    pub(in crate::storage) fn catalog_test_references(&self) -> ((usize, usize, usize), bool) {
        self.visibility
            .expect("actual visibility binding")
            .test_references()
    }
}

impl<W: WriterNamespace> RetentionWriter<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn catalog_test_references(&self) -> ((usize, usize, usize), bool) {
        self.visibility
            .expect("actual visibility binding")
            .test_references()
    }
    /// Physical frame pins of the original pool, separate from source pins.
    pub(in crate::storage) fn catalog_test_physical_pins(&self) -> u16 {
        dirty_frame::init_test_support::claim_state(self.pool).1
    }
}

impl<'s, 'c, 'p, 'd, W: WriterNamespace> RetentionWriter<'_, '_, 's, 'c, 'p, 'd, '_, '_, '_, W> {
    /// Borrow-only observation of the original pool, as the Live accessor.
    pub(in crate::storage) fn catalog_test_pool(&self) -> &SharedPool<'c, 's, 'p, 'd> {
        self.pool
    }
}

impl<'s, 'c, 'p, 'd, W: WriterNamespace> RetentionFlusher<'_, 's, 'c, 'p, 'd, '_, '_, W> {
    /// Borrow-only observation of the original pool, as the Live accessor.
    pub(in crate::storage) fn catalog_test_pool(&self) -> &SharedPool<'c, 's, 'p, 'd> {
        self.pool
    }
}

impl<W: WriterNamespace> Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn catalog_test_references(&self) -> ((usize, usize, usize), bool) {
        self.catalog_visibility
            .as_ref()
            .expect("actual visibility binding")
            .test_references()
    }
}

pub(in crate::storage) type FreeUndoTestState = (
    crate::storage::wal_io::WalPrefix,
    Option<u64>,
    bool,
    bool,
    bool,
    bool,
    u16,
);

impl<W: WriterNamespace> Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    pub(in crate::storage) fn free_undo_test_state(&self) -> Result<FreeUndoTestState> {
        let prefix = self.inner.storage.wal().prefix().map_err(|e| {
            fatal(
                self.inner.storage,
                RetentionPhase::Census,
                RetentionCause::Wal(e),
            )
        })?;
        let (reserved, installation) = self.control.ledger.population.test_state();
        let (claim, pins) = dirty_frame::init_test_support::claim_state(&self.inner.pool);
        Ok((
            prefix,
            self.control
                .ledger
                .durable
                .as_ref()
                .map(|d| d.observation().frame.lsn),
            reserved,
            installation,
            self.control.ledger.pending.is_some(),
            claim,
            pins,
        ))
    }

    pub(in crate::storage) fn catalog_test_jobs(&self) -> &[[u8; 1024]; 64] {
        self.control.ledger.jobs()
    }

    /// Original physical state after a fence; observations issue no requirement.
    pub(in crate::storage) fn catalog_test_wal_state(
        &self,
    ) -> (crate::storage::wal_io::WalPrefix, u64) {
        let wal = self.inner.storage.wal();
        let prefix = wal
            .append
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .prefix;
        (
            prefix,
            wal.first_write_lsn
                .load(std::sync::atomic::Ordering::Acquire),
        )
    }

    pub(in crate::storage) fn catalog_test_locals(&self) -> (u64, u64) {
        (
            self.control.ledger.issued_local,
            self.control.ledger.adopted_local,
        )
    }

    pub(in crate::storage) fn catalog_test_q(&self) -> u64 {
        self.catalog_visibility
            .as_ref()
            .expect("actual visibility binding")
            .test_q()
    }

    pub(in crate::storage) fn recovery_test_bank(&self) -> &[u8] {
        &self.control.ledger.bytes[..65_536]
    }

    pub(in crate::storage) fn recovery_test_credit(&self) -> Option<(u64, u64, [u8; 32])> {
        self.recovery_binding.as_ref().map(|binding| {
            (
                binding.remaining_wal(),
                binding.remaining_steps(),
                binding.state_digest(),
            )
        })
    }
}
