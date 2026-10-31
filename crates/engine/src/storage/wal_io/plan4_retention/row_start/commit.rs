//! Row specialization of the original Commit barrier. Preparation retains the
//! mandatory canonical replay; only the compact immutable image crosses Pending.
use super::super::super::super::catalog_commit::{CommitImage, CommitPreparation};
use super::*;

/// Private completion packet extracted only from the installed Commit image.
/// It is not a durability authority; the capsule still checks terminal/ATT/Q.
#[derive(Clone, Copy)]
pub(in super::super::super::super::super) struct RowCommitSummary<'i> {
    pub(in super::super::super::super::super) jobs: &'i [u8; PAGE_BYTES],
    pub(in super::super::super::super::super) terminal: usize,
    pub(in super::super::super::super::super) lsn: u64,
}

pub(in super::super::super::super::super) struct RowCommitImage<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    prefix: WalPrefix,
    frame: &'i [u8],
    digest: [u8; 32],
    cut: ControlCut,
    actor: TxnRef,
    status: StatusSlotRef,
    terminal: usize,
    slot: Option<usize>,
    lsn: u64,
    order: [u8; 5],
    count: u8,
    inventory: &'i [u8; PAGE_BYTES],
    jobs: &'i [u8; PAGE_BYTES],
    after: [Option<&'i [u8; PAGE_BYTES]>; 4],
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    fn prepare_row_commit<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<PreparedRowGroup<'i, 'c, 's>> {
        let (terminal, status) = self.row_terminal(actor)?;
        let slot = self.control.slot(actor).ok_or_else(|| {
            unchanged(RetentionPhase::Prepare, RetentionCause::MissingTransaction)
        })?;
        self.prepare_existing_row(
            owner,
            row_group::Change::Commit { terminal },
            Some(slot),
            [
                (
                    self.pool
                        .target_request(0)
                        .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                    ALLOCATOR_ROLE,
                ),
                (
                    target_from_source(self.storage, Plan4Page::Root)?.request,
                    ROOT,
                ),
                (
                    target_from_source(
                        self.storage,
                        Plan4Page::Jobs {
                            index: (terminal / 15) as u8,
                        },
                    )?
                    .request,
                    JOB,
                ),
                (self.catalog_status_request(status), STATUS),
            ],
            private,
        )
    }
}

impl<'i, 'c, 's> CommitPreparation<'i, 'c, 's> for PreparedRowGroup<'i, 'c, 's> {
    type Observation = RowCommitSummary<'i>;
    type Image = RowCommitImage<'i, 'c, 's>;
    fn prepare<W: WriterNamespace>(
        writer: &RetentionWriter<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<Self> {
        writer
            .catalog_preparation()
            .prepare_row_commit(owner, actor, private)
    }
    fn is_commit(&self) -> bool {
        self.payload.observation.operation == row_semantic::Operation::Commit
    }
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.payload.core
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.payload.binding
    }
    fn fence(&self) -> &'s SessionFence {
        self.payload.fence
    }
    fn actor(&self) -> Option<TxnRef> {
        self.payload.observation.actor
    }
    fn prefix(&self) -> WalPrefix {
        self.payload.prefix
    }
    fn header(&self) -> crate::storage::wal_frame::WalFrameHeader {
        self.payload.observation.header
    }
    fn frame(&self) -> &'i [u8] {
        self.payload.frame
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.payload.frame_digest
    }
    fn check(
        &self,
        owner: &Owner<'c, 's>,
        pool: &SharedPool<'c, 's, '_, '_>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()> {
        self.check(owner, pool, control, prefix)
    }
    fn targets(&self) -> &[TargetRequest] {
        self.targets()
    }
    fn stamp(&self, at: usize) -> Option<FrameStamp> {
        self.stamp(at)
    }
    fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        self.after(at)
    }
    fn commit_q(&self, profile: crate::storage::control::Profile) -> Result<u64> {
        let bad = || unchanged(RetentionPhase::Prepare, RetentionCause::Invariant);
        if !self.is_commit()
            || self.count != 4
            || self.payload.observation.count != 3
            || self.payload.observation.job_ordinal.is_none()
        {
            return Err(bad());
        }
        let mut kinds = [false; 4];
        let mut q = None;
        for (at, target) in self.targets().iter().enumerate() {
            let role = match (target.kind, self.after(at)) {
                (PageKind::Allocation, None) => 0,
                (PageKind::RetentionRoot, Some(after)) => {
                    let (header, root) = queue::decode_root_page(after, profile.spec().namespace)
                        .map_err(|e| {
                        unchanged(RetentionPhase::Prepare, RetentionCause::Retention(e))
                    })?;
                    if header.lsn.0 != self.header().lsn {
                        return Err(bad());
                    }
                    q = Some(root.commit_c());
                    1
                }
                (PageKind::RetentionJobs, Some(_)) => 2,
                (PageKind::TransactionStatus, Some(_)) => 3,
                _ => return Err(bad()),
            };
            if kinds[role] {
                return Err(bad());
            }
            kinds[role] = true;
        }
        if kinds != [true; 4] {
            return Err(bad());
        }
        q.filter(|q| *q != 0).ok_or_else(bad)
    }
    fn epochs(
        &self,
        visibility: &BoundVisibility<'_, 'c, 's>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        profile: crate::storage::control::Profile,
        q: u64,
    ) -> visibility::VisibilityResult<visibility::CommitEpochs> {
        visibility.prepare_row_commit_epochs(self, control, profile, q)
    }
    fn mark_staged(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        self.mark_staged(control, requirement)
    }
    fn into_image(self) -> Self::Image {
        let after = std::array::from_fn(|at| self.after(at));
        let p = self.payload;
        assert!(
            p.observation.operation == row_semantic::Operation::Commit
                && p.observation.issued.is_none()
        );
        RowCommitImage {
            core: p.core,
            binding: p.binding,
            fence: p.fence,
            prefix: p.prefix,
            frame: p.frame,
            digest: p.frame_digest,
            cut: p.control,
            actor: p.observation.actor.expect("checked Commit actor"),
            status: p.observation.status.expect("checked Commit Status"),
            terminal: p.observation.job_ordinal.expect("checked Commit terminal"),
            slot: p.slot,
            lsn: p.observation.header.lsn,
            order: p.observation.order,
            count: p.observation.count,
            inventory: p.inventory,
            jobs: &p.after[row_group::JOB],
            after,
        }
    }
}
impl<'c, 's> GroupPayload<'c, 's> for RowCommitImage<'_, 'c, 's> {
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    fn prefix(&self) -> WalPrefix {
        self.prefix
    }
    fn lsn(&self) -> u64 {
        self.lsn
    }
    fn frame(&self) -> &[u8] {
        self.frame
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.digest
    }
    fn cut(&self) -> ControlCut {
        self.cut
    }
}
impl<'i, 'c, 's> CommitImage<'i, 'c, 's> for RowCommitImage<'i, 'c, 's> {
    type Observation = RowCommitSummary<'i>;
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.digest
    }
    fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        self.after.get(at).copied().flatten()
    }
    fn observation(&self) -> RowCommitSummary<'i> {
        RowCommitSummary {
            jobs: self.jobs,
            terminal: self.terminal,
            lsn: self.lsn,
        }
    }
    fn apply_control(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        profile: crate::storage::control::Profile,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        check_staged_group(self, control, requirement)?;
        let layout = row_semantic::decode_layout(
            self.frame,
            FrameExpectation {
                stream: self.prefix.stream,
                previous_lsn: self.prefix.complete_lsn,
            },
            profile,
        )
        .map_err(|e| unchanged(RetentionPhase::Install, RetentionCause::RowSemantic(e)))?;
        if layout.operation() != row_semantic::Operation::Commit || layout.header().lsn != self.lsn
        {
            return Err(unchanged(
                RetentionPhase::Install,
                RetentionCause::Invariant,
            ));
        }
        // Header is read from the exact sealed frame. The projected fields are
        // copied from its checked producer, not re-derived from mutable state.
        // Commit has no issued row/Undo numbers. record uses operation/header,
        // actor/Status and terminal; order/count/frame length are exact constants.
        let observation = row_group::Observation {
            operation: row_semantic::Operation::Commit,
            header: layout.header(),
            order: self.order,
            count: self.count,
            frame_bytes: self.frame.len(),
            job_ordinal: Some(self.terminal),
            actor: Some(self.actor),
            status: Some(self.status),
            issued: None,
        };
        apply_row_control(
            control,
            profile,
            layout,
            &observation,
            self.jobs,
            self.slot,
            self.inventory,
        )
    }
}
