//! Private specialization of the one Commit protocol. Only the original
//! Catalog and Row preparers implement this boundary; no consumer can supply
//! a frame, observation, target set or installation receipt through it.
use super::*;
use crate::storage::wal_io::WalPrefix;
use crate::storage::{control::Profile, transaction_record::TxnRef, wal_frame::WalFrameHeader};

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::PreparedCatalog<'_, '_, '_> {}
    impl Sealed for super::super::super::catalog_live::lifecycle::row::PreparedRowGroup<'_, '_, '_> {}
}

pub(in super::super::super) trait CommitPreparation<'i, 'c, 's>:
    Sized + sealed::Sealed
{
    type Observation: Copy;
    type Image: CommitImage<'i, 'c, 's, Observation = Self::Observation>;
    fn prepare<W: WriterNamespace>(
        writer: &RetentionWriter<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; 425_984],
    ) -> Result<Self>;
    fn is_commit(&self) -> bool;
    fn core(&self) -> &'c PageConcurrency<'s>;
    fn binding(&self) -> &'s AtomicU8;
    fn actor(&self) -> Option<TxnRef>;
    fn fence(&self) -> &'s SessionFence;
    fn prefix(&self) -> WalPrefix;
    fn header(&self) -> WalFrameHeader;
    fn frame(&self) -> &'i [u8];
    fn frame_digest(&self) -> [u8; 32];
    fn check(
        &self,
        owner: &Owner<'c, 's>,
        pool: &SharedPool<'c, 's, '_, '_>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()>;
    fn targets(&self) -> &[TargetRequest];
    fn stamp(&self, at: usize) -> Option<FrameStamp>;
    fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]>;
    fn commit_q(&self, profile: Profile) -> Result<u64>;
    fn epochs(
        &self,
        visibility: &BoundVisibility<'_, 'c, 's>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        profile: Profile,
        q: u64,
    ) -> visibility::VisibilityResult<visibility::CommitEpochs>;
    fn mark_staged(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        requirement: WalRequirement<'s>,
    ) -> Result<()>;
    fn into_image(self) -> Self::Image;
}

pub(in super::super::super) trait CommitImage<'i, 'c, 's> {
    type Observation: Copy;
    fn core(&self) -> &'c PageConcurrency<'s>;
    fn binding(&self) -> &'s AtomicU8;
    fn fence(&self) -> &'s SessionFence;
    fn frame_digest(&self) -> [u8; 32];
    fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]>;
    fn observation(&self) -> Self::Observation;
    fn apply_control(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        profile: Profile,
        requirement: WalRequirement<'s>,
    ) -> Result<()>;
}

impl<'i, 'c, 's> CommitPreparation<'i, 'c, 's> for PreparedCatalog<'i, 'c, 's> {
    type Observation = Observation;
    type Image = CatalogCommitImage<'i, 'c, 's>;
    fn prepare<W: WriterNamespace>(
        writer: &RetentionWriter<'_, '_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        actor: TxnRef,
        private: &'i mut [u8; 425_984],
    ) -> Result<Self> {
        writer.prepare_catalog_commit(owner, actor, private)
    }
    fn is_commit(&self) -> bool {
        self.operation() == crate::storage::semantic_catalog::Operation::Commit
    }
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core()
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding()
    }
    fn actor(&self) -> Option<TxnRef> {
        self.observation().actor
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence()
    }
    fn prefix(&self) -> WalPrefix {
        self.prefix()
    }
    fn header(&self) -> WalFrameHeader {
        self.observation().header
    }
    fn frame(&self) -> &'i [u8] {
        self.frame()
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.frame_digest()
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
    fn commit_q(&self, profile: Profile) -> Result<u64> {
        super::commit_q(self, profile)
    }
    fn epochs(
        &self,
        visibility: &BoundVisibility<'_, 'c, 's>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        profile: Profile,
        q: u64,
    ) -> visibility::VisibilityResult<visibility::CommitEpochs> {
        visibility.prepare_commit_epochs(self, control, profile, q)
    }
    fn mark_staged(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        catalog_mark_staged(self, control, requirement)
    }
    fn into_image(self) -> Self::Image {
        self.into_commit_image()
    }
}
impl<'i, 'c, 's> CommitImage<'i, 'c, 's> for CatalogCommitImage<'i, 'c, 's> {
    type Observation = Observation;
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core()
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding()
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence()
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.frame_digest()
    }
    fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        self.after(at)
    }
    fn observation(&self) -> Observation {
        self.observation()
    }
    fn apply_control(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        profile: Profile,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        catalog_apply(self.payload(), control, profile, requirement)
    }
}
