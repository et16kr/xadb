//! Canonical candidate images for scope2, with exact native preconditions.
//! Inputs are borrowed observations. No function installs a page or grants a
//! reservation; the original producer couples all candidates before staging.
use super::{Error, Operation, Result};
use crate::{
    api::Namespace,
    storage::{
        page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
        retention_page::catalog::{JobState, JobView},
    },
};
use foundation::identity::{Generation, ObjectId};

mod allocator;
mod catalog;
mod status;
mod undo;
pub(crate) use allocator::change_pair;
pub(crate) use catalog::{CatalogChange, change_catalog, initialize_pair};
pub(crate) use status::{StatusChange, StatusContext, change_status};
pub(crate) use undo::{change_undo, validate_retained_undo};

#[derive(Clone, Copy)]
pub(crate) struct RootContext<'a> {
    pub(crate) namespace: Namespace,
    pub(crate) job: JobView<'a>,
    pub(crate) lsn: PageLsn,
}
impl RootContext<'_> {
    fn validate(self, operation: Operation) -> Result<()> {
        let state = self.job.state();
        let valid = match operation {
            Operation::Reserve | Operation::Forward | Operation::Cancel => {
                state == JobState::RootReserved
            }
            Operation::Clr | Operation::Prune | Operation::RollbackStart => {
                state == JobState::RootRetained
            }
            Operation::ReleaseRole => state.is_releasing() && self.job.release_cursor() < 3,
            Operation::RootFree => state.is_releasing() && self.job.release_cursor() == 3,
            _ => false,
        };
        if !valid
            || self.namespace.timeline != 1
            || self.lsn.0 == 0
            || self.lsn.0 < self.job.last_step()
            || (operation == Operation::Reserve) != (self.lsn.0 == self.job.last_step())
            || self.job.source().is_none_or(|actor| {
                actor.id().database != self.namespace.database
                    || actor.id().timeline != self.namespace.timeline
            })
        {
            return Err(Error::OrderMismatch);
        }
        Ok(())
    }
    fn dynamic_header(
        self,
        reference: PageRef,
        owner: ObjectId,
        generation: Generation,
        kind: PageKind,
    ) -> PageHeader {
        PageHeader {
            identity: PageIdentity {
                database: self.namespace.database,
                page: reference,
                owner,
                owner_generation: generation,
            },
            kind,
            level: 0,
            lsn: self.lsn,
        }
    }
    fn fixed_header(self, page_no: u64, owner: u64, kind: PageKind) -> PageHeader {
        self.dynamic_header(
            PageRef {
                file: FileId::new(0x0000_0001_0000_0001).expect("first file"),
                page_no,
                incarnation: Generation::new(1).expect("genesis"),
            },
            ObjectId::new(owner).expect("fixed owner"),
            Generation::new(1).expect("fixed generation"),
            kind,
        )
    }
}
