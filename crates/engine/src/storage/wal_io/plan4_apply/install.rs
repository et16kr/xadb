//! Read-only installer observations. Constructors stay in the actual apply
//! producers; the legacy adapter consumes the original sealed permit.
use super::{InstallGrant, InstallKind};
use crate::{
    storage::{
        page::{PAGE_BYTES, PageIdentity, PageLsn},
        wal_io::WalRequirement,
    },
    transaction::concurrency::PageConcurrency,
};
impl<'s> InstallGrant<'_, '_, 's> {
    pub(in crate::storage) fn matches_core(&self, core: &PageConcurrency<'_>) -> bool {
        match &self.kind {
            InstallKind::Plan4 { core: original, .. } => std::ptr::eq(*original, core),
            InstallKind::Legacy(proof) => proof.matches_core(core),
        }
    }
    pub(in crate::storage) fn requirement(&self) -> WalRequirement<'s> {
        match &self.kind {
            InstallKind::Plan4 { requirement, .. } => *requirement,
            InstallKind::Legacy(proof) => proof.requirement(),
        }
    }
    pub(in crate::storage) fn identity(&self) -> PageIdentity {
        match &self.kind {
            InstallKind::Plan4 { stamp, .. } => stamp.identity,
            InstallKind::Legacy(proof) => proof.identity(),
        }
    }
    pub(in crate::storage) fn generation(&self) -> u64 {
        match &self.kind {
            InstallKind::Plan4 { stamp, .. } => stamp.generation,
            InstallKind::Legacy(proof) => proof.generation(),
        }
    }
    pub(in crate::storage) fn before_counter(&self) -> u64 {
        match &self.kind {
            InstallKind::Plan4 { stamp, .. } => stamp.modification_counter,
            InstallKind::Legacy(proof) => proof.before_counter(),
        }
    }
    pub(in crate::storage) fn after_counter(&self) -> u64 {
        match &self.kind {
            InstallKind::Plan4 { after_counter, .. } => *after_counter,
            InstallKind::Legacy(proof) => proof.after_counter(),
        }
    }
    pub(in crate::storage) fn after(&self) -> &[u8; PAGE_BYTES] {
        match &self.kind {
            InstallKind::Plan4 { after, .. } => after,
            InstallKind::Legacy(proof) => proof.after(),
        }
    }
    pub(in crate::storage) fn page_lsn(&self) -> PageLsn {
        match &self.kind {
            InstallKind::Plan4 { requirement, .. } => PageLsn(requirement.location().lsn),
            InstallKind::Legacy(proof) => proof.page_lsn(),
        }
    }
    pub(in crate::storage) fn prior_lsn(&self) -> PageLsn {
        match &self.kind {
            InstallKind::Plan4 { stamp, .. } => stamp.page_lsn,
            InstallKind::Legacy(proof) => proof.prior_lsn(),
        }
    }
    pub(in crate::storage) fn first_rec_lsn(&self) -> PageLsn {
        match &self.kind {
            InstallKind::Plan4 { stamp, .. } => stamp.rec_lsn.unwrap_or(self.page_lsn()),
            InstallKind::Legacy(proof) => proof.first_rec_lsn(),
        }
    }
}

// One sequential accessor chain borrows the held grant. Count its largest
// nested return (requirement/identity), discriminant and references. The grant
// itself belongs to its producer and is not copied into the installer.
impl InstallGrant<'_, '_, '_> {
    pub(in crate::storage) const fn observation_source_bytes() -> usize {
        use std::mem::size_of;
        2 * size_of::<WalRequirement<'_>>()
            + 2 * size_of::<PageIdentity>()
            + 8 * size_of::<&()>()
            + 4 * size_of::<u64>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn install_grant_and_legacy_conversion_fit_original_bounds() {
        use std::mem::size_of;
        let grant = size_of::<InstallGrant<'_, '_, '_>>();
        let observation = InstallGrant::observation_source_bytes();
        let conversion = super::super::legacy::conversion_source_bytes();
        let target2 = crate::storage::dirty_frame::target_set_call_bytes::<2>();
        assert!(grant <= 384, "original per-permit envelope");
        assert!(conversion <= 2048, "original allocator bridge allowance");
        assert!(observation <= 660);
        assert!(target2 <= crate::storage::dirty_frame::CALL_BYTES);
        println!(
            "C3_INSTALL_GRANT bytes={grant} observation={observation} legacy_conversion={conversion} target2={target2} retained_owner_delta=0 new_authority_ctor=0"
        );
    }
}
