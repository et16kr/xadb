//! Terminal receipts issued by the actual Catalog stage/durable/install chain.
//! A physical requirement alone is never semantic Commit installation authority.
use super::bootstrap::{CatalogCommitAttempt, CatalogCommitInstalled};
use super::{DurableWal, WalRequirement, WalRuntime, WriterNamespace};
use crate::transaction::concurrency::{InstallationKey, PageConcurrency};
use foundation::error::Effect;

#[derive(Clone, Copy)]
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

/// Only the actual terminal WAL adapter may seal an unchanged stage rejection.
/// An upper error code cannot clear the core's stage-call marker. Construction
/// is private to the original stage call below.
pub(crate) struct UnstagedCommit<'c, 'f> {
    core: &'c PageConcurrency<'f>,
    key: InstallationKey,
}

/// The companion of the actual StagedCommit. Its private binding is retained
/// while the original stage is consumed by PinSet suspension. It contains no
/// file, append, page, version or replay lock.
pub(super) struct CatalogCommitWait<'c, 's> {
    binding: Binding<'c, 's>,
}
impl<'s> CatalogCommitWait<'_, 's> {
    pub(super) fn requirement(&self) -> WalRequirement<'s> {
        self.binding.requirement
    }
}

pub(super) struct CatalogStageError<'c, 's> {
    pub(super) error: super::WalIoError,
    pub(super) unstaged: Option<UnstagedCommit<'c, 's>>,
}

impl<W: WriterNamespace> WalRuntime<'_, W> {
    /// Only the actual adapter can create `attempt`, after the original core
    /// has entered StageCallStarted. The unchanged receipt is constructed in
    /// exactly the branch returning the original WAL stage's Unchanged error.
    pub(super) fn stage_catalog_commit<'c, 's>(
        &'s self,
        attempt: CatalogCommitAttempt<'_, '_, 'c, 's>,
    ) -> std::result::Result<
        (StagedCommit<'c, 's>, CatalogCommitWait<'c, 's>),
        CatalogStageError<'c, 's>,
    > {
        let (prepared, key) = attempt.into_parts();
        if prepared.operation() != crate::storage::semantic_catalog::Operation::Commit
            || !std::ptr::eq(prepared.fence(), &self.fenced)
            || !prepared.core().uses_fence(&self.fenced)
        {
            return Err(CatalogStageError {
                error: self.requirement_invariant(),
                unstaged: None,
            });
        }
        let requirement = match self.stage_verified_required(
            prepared.prefix(),
            prepared.observation().header,
            prepared.frame(),
        ) {
            Ok(requirement) => requirement,
            Err(error) => {
                let unstaged = if error.effect == Effect::Unchanged {
                    Some(UnstagedCommit {
                        core: prepared.core(),
                        key,
                    })
                } else {
                    None
                };
                return Err(CatalogStageError { error, unstaged });
            }
        };
        let binding = Binding {
            core: prepared.core(),
            key,
            requirement,
            semantic_digest: prepared.frame_digest(),
        };
        Ok((StagedCommit { binding }, CatalogCommitWait { binding }))
    }

    /// This calls the original shared WAL drainer with the exact stage-issued
    /// interval. A scalar DurableWal cannot substitute for this sealed pair.
    pub(super) fn wait_catalog_commit<'c, 's>(
        &self,
        wait: CatalogCommitWait<'c, 's>,
    ) -> super::Result<DurableCommit<'c, 's>> {
        if !wait.binding.requirement.uses_fence(&self.fenced) {
            return Err(self.requirement_invariant());
        }
        let durable = self.wait_durable(&wait.binding.requirement)?;
        if !durable.confirms(&wait.binding.requirement) {
            return Err(self.requirement_invariant());
        }
        Ok(DurableCommit {
            binding: wait.binding,
            durable,
        })
    }
}

impl<'c, 's> DurableCommit<'c, 's> {
    /// The token is created only after whole physical/control/history install.
    /// It is not constructible from a page mask or an observed WAL prefix.
    pub(super) fn complete_catalog(
        self,
        installed: &CatalogCommitInstalled<'c, 's>,
    ) -> super::Result<(InstalledCommit<'c, 's>, DurableWal<'s>)> {
        if !installed.matches(
            self.binding.core,
            self.binding.key,
            self.binding.requirement,
            self.binding.semantic_digest,
        ) || !self.durable.confirms(&self.binding.requirement)
        {
            self.binding.requirement.origin.fence();
            return Err(super::attempted(
                super::Phase::Publish,
                super::Cause::IdentityMismatch,
            ));
        }
        Ok((
            InstalledCommit {
                binding: self.binding,
            },
            self.durable,
        ))
    }
}

pub(super) fn catalog_source_bytes() -> usize {
    use std::mem::size_of;
    type Stage<'c, 's> = std::result::Result<
        (StagedCommit<'c, 's>, CatalogCommitWait<'c, 's>),
        CatalogStageError<'c, 's>,
    >;
    size_of::<Binding<'_, '_>>()
        + size_of::<CatalogCommitWait<'_, '_>>()
        + size_of::<StagedCommit<'_, '_>>()
        + 2 * size_of::<Stage<'_, '_>>()
        + 2 * size_of::<super::Result<WalRequirement<'_>>>()
        + 2 * size_of::<super::Result<DurableCommit<'_, '_>>>()
        + 2 * size_of::<super::Result<DurableWal<'_>>>()
        + 2 * size_of::<super::Result<(InstalledCommit<'_, '_>, DurableWal<'_>)>>()
        + super::DURABILITY_CALL_BYTES
}
impl UnstagedCommit<'_, '_> {
    pub(crate) fn matches(&self, core: &PageConcurrency<'_>, key: InstallationKey) -> bool {
        std::ptr::eq(self.core, core) && self.key == key
    }
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

/// Actual wait adapter closure, separately from stage and completion.
#[cfg(test)]
pub(super) fn catalog_wait_source_observation<W: WriterNamespace>() -> usize {
    use std::mem::size_of;
    let physical = super::physical::wait_source_observations::<W>()[6].1;
    // The physical call has ended before the semantic receipt is initialized.
    // Keep its Result in the physical closure and the extracted DurableWal in
    // the handoff. This max uses source initialization order, not stack reuse.
    let call = physical + size_of::<DurableWal<'_>>();
    let returning = size_of::<DurableWal<'_>>()
        + size_of::<DurableCommit<'_, '_>>()
        + size_of::<super::Result<DurableCommit<'_, '_>>>();
    size_of::<CatalogCommitWait<'_, '_>>()
        + 4 * size_of::<&()>()
        + 2 * size_of::<super::WalIoError>()
        + call.max(returning)
}
