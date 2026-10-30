//! Test-only observation of the actual retained byte bank. This lends no
//! authority constructor and copies no caller loan. The terminal Busy fixture
//! can explicitly submit decoded bytes to the original physical WAL stage.
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

impl<'s, 'c, 'p, 'd, W: WriterNamespace> RetentionReader<'_, '_, 's, 'c, 'p, 'd, '_, '_, W> {
    /// Borrow the original pool for a test's actual checked retained pin.
    pub(in crate::storage) fn catalog_test_pool(&self) -> &SharedPool<'c, 's, 'p, 'd> {
        self.pool
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

    /// Corrupt only the exposed Ready label or retained I/O origin. Existing
    /// authentic capture/receipts remain for the lease's validation to reject.
    /// Kind 2 exposes WaitingWal and kind 3 leaves the I/O no headroom for
    /// its next selector generation, so its preflight is Unchanged Capacity
    /// (S2W-R17).
    pub(in crate::storage) fn catalog_test_create_flush_fault(&mut self, kind: u8) {
        flush_fault(&mut self.flush, kind);
    }

    /// Fault injection: retain an authentic previous requirement as unexpected
    /// pending state. This cannot produce authority or a successful transition.
    pub(in crate::storage) fn catalog_test_create_pending_fault(&mut self) {
        assert!(self.control.ledger.pending.is_none());
        self.control.ledger.pending = Some(
            self.history
                .source_cut()
                .unwrap()
                .requirement()
                .expect("actual Begin"),
        );
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

    pub(in crate::storage) fn catalog_test_epochs(&self) -> (bool, u64, Vec<(u8, u64, u64)>) {
        self.catalog_visibility
            .as_ref()
            .expect("original visibility")
            .test_epochs()
    }

    pub(in crate::storage) fn catalog_test_epoch_backing(&self) -> (usize, bool) {
        self.catalog_visibility
            .as_ref()
            .expect("actual visibility binding")
            .test_epoch_backing()
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

impl<W: WriterNamespace> Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    // Scalar observation only; cannot issue a slot, credit or snapshot.
    pub(in crate::storage) fn catalog_test_isolation(&self) -> (u64, u64, u64) {
        let state = self.control.isolation.lock().unwrap();
        let n = |at| u64::from_le_bytes(state.bytes[at..at + 8].try_into().unwrap());
        (n(0), n(8), n(16))
    }
}

impl<W: WriterNamespace> RetentionReader<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// Synthetic slot competition: use the actual original decoder and stage.
    /// This supplies no semantic receipt and must not resume the live session.
    pub(in crate::storage) fn catalog_test_competing_stage(&self, bytes: &[u8]) {
        let wal = self.storage.wal();
        let prefix = wal.prefix().unwrap();
        let layout = crate::storage::semantic_catalog::decode_layout(
            bytes,
            crate::storage::wal_frame::FrameExpectation {
                stream: prefix.stream,
                previous_lsn: prefix.complete_lsn,
            },
            self.storage.profile(),
        )
        .unwrap();
        wal.stage_verified(prefix, layout.header(), bytes).unwrap();
    }
}

/// Read-only actual Commit state; no capability or recovery source is created.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::storage) struct CatalogCommitTestState {
    control: [u8; 32],
    issued: u64,
    adopted: u64,
    prefix: crate::storage::wal_io::WalPrefix,
    first_write: u64,
    q: u64,
    epochs: (bool, u64, Vec<(u8, u64, u64)>),
}
impl<W: WriterNamespace> RetentionWriter<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// S2W-R03: the control's (issued local, adopted local). Scalars only.
    pub(in crate::storage) fn row_test_locals(&self) -> (u64, u64) {
        (self.control.issued_local, self.control.adopted_local)
    }

    pub(in crate::storage) fn catalog_test_commit_state(&self) -> CatalogCommitTestState {
        let wal = self.storage.wal();
        let visibility = self.visibility.expect("original visibility");
        CatalogCommitTestState {
            control: foundation::codec::content_digest(self.control.bytes),
            issued: self.control.issued_local,
            adopted: self.control.adopted_local,
            prefix: wal.append.lock().unwrap().prefix,
            first_write: wal
                .first_write_lsn
                .load(std::sync::atomic::Ordering::Acquire),
            q: visibility.test_q(),
            epochs: visibility.test_epochs(),
        }
    }
}

/// Synthetic fault access borrows only the original physical owner, so the
/// fixture can stage at a checked boundary while the original bound is mutable.
impl<'s, 'm, 'w, W: WriterNamespace> Plan4Retention<'_, 's, '_, '_, '_, '_, 'm, 'w, W> {
    pub(in crate::storage) fn catalog_test_stage_competitor(
        &self,
    ) -> impl FnOnce(&[u8]) + 's + use<'s, 'm, 'w, W> {
        let storage = self.inner.storage;
        move |bytes| {
            let wal = storage.wal();
            let prefix = wal.prefix().unwrap();
            let layout = crate::storage::semantic_catalog::decode_layout(
                bytes,
                crate::storage::wal_frame::FrameExpectation {
                    stream: prefix.stream,
                    previous_lsn: prefix.complete_lsn,
                },
                storage.profile(),
            )
            .unwrap();
            wal.stage_verified(prefix, layout.header(), bytes).unwrap();
        }
    }
}

/// Test observations over the same resources before and during a CREATE loan.
/// No raw page/control mutation or authority construction is provided.
pub(in crate::storage) trait CatalogTestState {
    fn catalog_test_with_pool<T>(&self, run: impl FnOnce(&SharedPool<'_, '_, '_, '_>) -> T) -> T;
    fn recovery_test_bank(&self) -> &[u8];
    fn catalog_test_jobs(&self) -> &[[u8; 1024]; 64];
    fn free_undo_test_state(&self) -> Result<FreeUndoTestState>;
}
impl<W: WriterNamespace> CatalogTestState for Plan4Retention<'_, '_, '_, '_, '_, '_, '_, '_, W> {
    fn catalog_test_with_pool<T>(&self, run: impl FnOnce(&SharedPool<'_, '_, '_, '_>) -> T) -> T {
        run(&self.inner.pool)
    }
    fn recovery_test_bank(&self) -> &[u8] {
        self.recovery_test_bank()
    }
    fn catalog_test_jobs(&self) -> &[[u8; 1024]; 64] {
        self.catalog_test_jobs()
    }
    fn free_undo_test_state(&self) -> Result<FreeUndoTestState> {
        self.free_undo_test_state()
    }
}

impl<W: WriterNamespace> CatalogTestState for MutationPort<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    fn catalog_test_with_pool<T>(&self, run: impl FnOnce(&SharedPool<'_, '_, '_, '_>) -> T) -> T {
        run(self.pool)
    }
    fn recovery_test_bank(&self) -> &[u8] {
        self.control.bytes
    }
    fn catalog_test_jobs(&self) -> &[[u8; 1024]; 64] {
        self.control.jobs()
    }
    fn free_undo_test_state(&self) -> Result<FreeUndoTestState> {
        let prefix = self
            .storage
            .wal()
            .prefix()
            .map_err(|e| fatal(self.storage, RetentionPhase::Census, RetentionCause::Wal(e)))?;
        let (reserved, installation) = self.control.population.test_state();
        let (claim, pins) = dirty_frame::init_test_support::claim_state(self.pool);
        Ok((
            prefix,
            self.control
                .durable
                .as_ref()
                .map(|d| d.observation().frame.lsn),
            reserved,
            installation,
            self.control.pending.is_some(),
            claim,
            pins,
        ))
    }
}

fn flush_fault(control: &mut flush::RetentionFlushControl<'_, '_, '_>, kind: u8) {
    match kind {
        0 => control.phase = flush::RetentionFlushPhase::Ready,
        1 => control.io.test_pair_origin_fault(),
        2 => control.phase = flush::RetentionFlushPhase::WaitingWal,
        3 => super::super::super::flush_io::test_support::seed_headroom(
            &mut control.io,
            None,
            None,
            Some(u64::MAX),
        ),
        _ => panic!("unknown flush lease fault"),
    }
}

impl<W: WriterNamespace> MaintenancePort<'_, '_, '_, '_, '_, '_, '_, W> {
    /// `catalog_test_create_flush_fault` on a split's flush control, under an
    /// open Row statement (S2W-R17).
    pub(in crate::storage) fn row_test_flush_fault(&mut self, kind: u8) {
        flush_fault(self.control, kind);
    }
}
