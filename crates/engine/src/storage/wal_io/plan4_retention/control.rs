//! Byte-backed private live ledger. Encoded descriptors are retained in the
//! caller loan; only bounded indices/counters and authentic requirements live
//! inline in the owner.
use super::*;
use crate::storage::{
    allocation_page::ReservationOwner,
    obligation::{self, OwnerState},
    retention_page::{JOB_BYTES, JOB_COUNT, JobSpec, JobState},
    transaction_record::TransactionState,
    wal_io::WalPrefix,
};

pub(super) const SLOT_BYTES: usize = 1664;
pub(super) const ATT_SLOTS: usize = 32;
pub(super) const AGING_SLOT: usize = 32;
pub(super) const ALLOCATION_SLOT: usize = 33;
const BANK_END: usize = 65_536;
const JOB_END: usize = 131_072;
pub(super) const LOCATION: std::ops::Range<usize> = 1536..1576;
pub(super) const ACTOR: std::ops::Range<usize> = 1576..1624;
pub(super) const PHASE: usize = 1624;
pub(super) const STABLE: u8 = 1;
pub(super) const STAGED: u8 = 2;

pub(super) struct RetentionControl<'a, 'c, 's> {
    pub(super) bytes: &'a mut [u8; RETENTION_CONTROL_BYTES],
    pub(super) replay: &'a mut [u8; RETENTION_REPLAY_BYTES],
    pub(super) issued_local: u64,
    pub(super) adopted_local: u64,
    pub(super) pending: Option<WalRequirement<'s>>,
    pub(super) durable: Option<DurableWal<'s>>,
    pub(super) population: super::population::Population<'c, 's>,
    pub(super) source: super::recovery::HistorySource<'s>,
}
impl<'a, 'c, 's> RetentionControl<'a, 'c, 's> {
    pub(super) fn new(loan: EmptyRetentionControlLoan<'a>) -> Self {
        loan.control.fill(0);
        Self {
            bytes: loan.control,
            replay: loan.replay,
            issued_local: 0,
            adopted_local: 0,
            pending: None,
            durable: None,
            population: super::population::Population::new(),
            source: super::recovery::HistorySource::Genesis,
        }
    }
    pub(super) fn row(&self, index: usize) -> &[u8] {
        row(self.bytes, index)
    }
    pub(super) fn row_mut(&mut self, index: usize) -> &mut [u8] {
        row_mut(self.bytes, index)
    }
    pub(super) fn jobs(&self) -> &[[u8; JOB_BYTES]; JOB_COUNT] {
        jobs(self.bytes)
    }
    pub(super) fn initialize_jobs(&mut self, namespace: crate::api::Namespace) {
        let free = JobSpec {
            generation: foundation::identity::Generation::new(1).expect("genesis generation"),
            state: JobState::Free,
        };
        for cell in jobs_mut(self.bytes) {
            retention_page::encode_job(free, namespace, cell).expect("checked genesis namespace");
        }
    }
    pub(super) fn slot(&self, actor: TxnRef) -> Option<usize> {
        (0..ATT_SLOTS).find(|&i| {
            self.row(i)[PHASE] == STABLE && TxnRef::decode(&self.row(i)[ACTOR]).ok() == Some(actor)
        })
    }
    pub(super) fn free_slot(&self) -> Option<usize> {
        (0..ATT_SLOTS).find(|&i| self.row(i).iter().all(|v| *v == 0))
    }
    pub(super) fn check_ledger(
        &self,
        profile: crate::storage::control::Profile,
        prefix: WalPrefix,
    ) -> Result<(u64, u64)> {
        if self.pending.is_some() || self.adopted_local > self.issued_local {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
        }
        let mut wal = 0u64;
        let mut steps = 0u64;
        for i in 0..ATT_SLOTS {
            let row = self.row(i);
            if row.iter().all(|v| *v == 0) {
                continue;
            }
            let actor = TxnRef::decode(&row[ACTOR])
                .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Page(e)))?;
            let status = StatusSlotRef::decode(&row[LOCATION])
                .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Page(e)))?;
            let d = obligation::decode_descriptor(
                &row[..1536],
                ReservationOwner::Transaction(actor),
                crate::storage::retention_obligation::profile_reference(profile),
            )
            .map_err(|_| unchanged(RetentionPhase::Census, RetentionCause::Invariant))?;
            if row[PHASE] != STABLE
                || row[PHASE + 1..].iter().any(|v| *v != 0)
                || !self
                    .source
                    .allows_actor(actor, prefix.stream.writer_epoch, self.adopted_local)
                || actor.generation() != status.generation()
                || status.slot() != 0
                || d.chain.apply > prefix.complete_lsn
            {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
            }
            let OwnerState::Transaction(tx) = d.owner else {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
            };
            if tx.undo_head().is_some()
                || !matches!(
                    tx.state(),
                    TransactionState::Active
                        | TransactionState::Aborting(
                            crate::storage::transaction_record::RollbackState::Transaction {
                                next: None
                            }
                        )
                )
            {
                return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
            }
            for prior in 0..i {
                if self.row(prior)[PHASE] != 0
                    && (TxnRef::decode(&self.row(prior)[ACTOR])
                        .ok()
                        .is_some_and(|p| p.id() == actor.id())
                        || StatusSlotRef::decode(&self.row(prior)[LOCATION])
                            .ok()
                            .is_some_and(|p| p.page() == status.page()))
                {
                    return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
                }
            }
            for remaining in d.remaining {
                wal = wal
                    .checked_add(remaining.wal)
                    .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Exhausted))?;
                steps = steps
                    .checked_add(remaining.steps)
                    .ok_or_else(|| unchanged(RetentionPhase::Census, RetentionCause::Exhausted))?;
            }
        }
        let mut jobs_count = 0u64;
        for cell in self.jobs() {
            if !matches!(
                retention_page::decode_job(cell, profile.spec().namespace)
                    .map_err(|e| unchanged(RetentionPhase::Census, RetentionCause::Retention(e)))?
                    .spec()
                    .state,
                JobState::Free
            ) {
                jobs_count += 1;
            }
        }
        if jobs_count > 55 {
            return Err(unchanged(RetentionPhase::Census, RetentionCause::Invariant));
        }
        Ok((
            wal + jobs_count * retention_page::FINISH_BYTES as u64,
            steps + jobs_count,
        ))
    }
}
pub(super) fn row(bytes: &[u8], index: usize) -> &[u8] {
    &bytes[index * SLOT_BYTES..(index + 1) * SLOT_BYTES]
}
pub(super) fn row_mut(bytes: &mut [u8], index: usize) -> &mut [u8] {
    &mut bytes[index * SLOT_BYTES..(index + 1) * SLOT_BYTES]
}
pub(super) fn jobs(bytes: &[u8]) -> &[[u8; JOB_BYTES]; JOB_COUNT] {
    bytes[BANK_END..JOB_END]
        .as_chunks::<JOB_BYTES>()
        .0
        .try_into()
        .expect("64 job loan")
}
pub(super) fn jobs_mut(bytes: &mut [u8]) -> &mut [[u8; JOB_BYTES]; JOB_COUNT] {
    bytes[BANK_END..JOB_END]
        .as_chunks_mut::<JOB_BYTES>()
        .0
        .try_into()
        .expect("64 job loan")
}
pub(super) fn before_mut(bytes: &mut [u8]) -> &mut [[u8; PAGE_BYTES]; 4] {
    bytes[JOB_END..]
        .as_chunks_mut::<PAGE_BYTES>()
        .0
        .try_into()
        .expect("four before loans")
}
