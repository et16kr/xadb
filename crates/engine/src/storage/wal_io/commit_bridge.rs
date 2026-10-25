//! Opaque future terminal receipts. B1 deliberately supplies no constructors.
//! A physical requirement alone is never semantic Commit installation authority.
use super::{DurableWal, WalRequirement};
use crate::transaction::concurrency::{InstallationKey, PageConcurrency};

struct Binding<'c, 'f> {
    core: &'c PageConcurrency<'f>,
    key: InstallationKey,
    requirement: WalRequirement<'f>,
    semantic_digest: [u8; 32],
}
impl Binding<'_, '_> {
    fn same(&self, other: &Self) -> bool {
        std::ptr::eq(self.core, other.core)
            && self.key == other.key
            && std::ptr::eq(self.requirement.origin, other.requirement.origin)
            && self.requirement.exact == other.requirement.exact
            && self.semantic_digest == other.semantic_digest
    }
}

pub(crate) struct StagedCommit<'c, 'f> {
    binding: Binding<'c, 'f>,
}
pub(crate) struct DurableCommit<'c, 'f> {
    binding: Binding<'c, 'f>,
    durable: DurableWal<'f>,
}
pub(crate) struct InstalledCommit<'c, 'f> {
    binding: Binding<'c, 'f>,
}

impl StagedCommit<'_, '_> {
    pub(crate) fn matches(&self, core: &PageConcurrency<'_>, key: InstallationKey) -> bool {
        std::ptr::eq(self.binding.core, core)
            && self.binding.key == key
            && core.uses_fence(self.binding.requirement.origin)
    }
    pub(crate) fn lsn(&self) -> u64 {
        self.binding.requirement.exact.lsn
    }
    pub(crate) fn confirmed_by(&self, durable: &DurableCommit<'_, '_>) -> bool {
        self.binding.same(&durable.binding) && durable.durable.confirms(&self.binding.requirement)
    }
    pub(crate) fn completed_by(&self, installed: &InstalledCommit<'_, '_>) -> bool {
        self.binding.same(&installed.binding)
    }
    pub(in crate::storage) fn confirms_requirement(
        &self,
        requirement: &WalRequirement<'_>,
    ) -> bool {
        std::ptr::eq(self.binding.requirement.origin, requirement.origin)
            && self.binding.requirement.exact == requirement.exact
    }
}
