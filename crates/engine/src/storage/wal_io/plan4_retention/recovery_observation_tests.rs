//! Test-only observation of the actual retained byte bank. This lends no
//! mutation, constructor, requirement, or authority and copies no caller loan.
use super::*;

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
        let (reserved, installation) = self.control.population.test_state();
        let (claim, pins) = dirty_frame::init_test_support::claim_state(&self.inner.pool);
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

    pub(in crate::storage) fn recovery_test_bank(&self) -> &[u8] {
        &self.control.bytes[..65_536]
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
