//! A single residency step; dirty selection and durable flush remain distinct.
use super::super::super::super::super::{
    super::super::allocation_reservation::operation::AllocationPopulation, operation,
};
use super::*;
use crate::storage::dirty_frame::{StatusCandidate, StatusCandidateKind};

/// A scheduling hint, with no binding, page, generation or mutation authority.
#[derive(Debug, Default)]
pub(crate) struct StatusResidencyCursor(pub(super) Option<std::num::NonZeroUsize>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusResidencyStep {
    /// Registration observation only; this is not a pin or a content guard.
    Resident {
        index: usize,
        frame: FrameStamp,
        loaded: bool,
    },
    /// Use the original dedicated flusher, then request another residency step.
    FlushRequired { page: PageRef, rec_lsn: PageLsn },
}
impl StatusResidencyStep {
    pub(crate) fn resident(self) -> Option<(usize, FrameStamp, bool)> {
        match self {
            Self::Resident {
                index,
                frame,
                loaded,
            } => Some((index, frame, loaded)),
            Self::FlushRequired { .. } => None,
        }
    }
    pub(crate) fn flush_page(self) -> Option<PageRef> {
        match self {
            Self::FlushRequired { page, .. } => Some(page),
            Self::Resident { .. } => None,
        }
    }
}

impl<'s, 'c, W: WriterNamespace> EmptyTransactions<'_, 's, 'c, '_, '_, '_, '_, '_, W> {
    pub(crate) fn residency_cursor(&self) -> StatusResidencyCursor {
        StatusResidencyCursor::default()
    }

    pub(crate) fn residency_step(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
        cursor: &mut StatusResidencyCursor,
    ) -> Result<StatusResidencyStep> {
        let storage = self.inner.inner.inner.storage;
        let _unwind = storage.session_fence().unwind_guard();
        if !fits::<W>() {
            return Err(unchanged(Cause::Capacity));
        }
        let candidate = self.residency_candidate(owner, page, cursor.0.map_or(1, |n| n.get()))?;
        let result = match candidate.kind {
            StatusCandidateKind::Resident => StatusResidencyStep::Resident {
                index: candidate.index,
                frame: candidate.frame.expect("registered resident"),
                loaded: false,
            },
            StatusCandidateKind::Load => {
                let observation = self.load_into(owner, candidate.index, page)?;
                StatusResidencyStep::Resident {
                    index: candidate.index,
                    frame: observation.after,
                    loaded: true,
                }
            }
            StatusCandidateKind::Flush => {
                let frame = candidate.frame.expect("dirty candidate");
                return Ok(StatusResidencyStep::FlushRequired {
                    page: frame.identity.page,
                    rec_lsn: frame.rec_lsn.expect("dirty recLSN"),
                });
            }
        };
        cursor.0 = std::num::NonZeroUsize::new(candidate.next);
        Ok(result)
    }

    fn residency_candidate(
        &mut self,
        owner: &mut Owner<'c, 's>,
        page: PageRef,
        start: usize,
    ) -> Result<StatusCandidate> {
        self.health()?;
        let bound = &self.inner.inner.inner;
        let storage = bound.storage;
        if !owner
            .check_binding(bound.pool.bound_core())
            .map_err(|e| frame(e.into()))?
        {
            return Err(unchanged(Cause::BindingMismatch));
        }
        let prefix = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, Cause::Wal(e)))?;
        let allocator = self.allocator_snapshot(owner, prefix)?;
        let bound = &mut self.inner.inner.inner;
        let request = TargetRequest {
            identity: PageIdentity {
                database: bound.control.role.namespace().database,
                page,
                owner: bound.control.role.object(),
                owner_generation: bound.control.role.generation(),
            },
            kind: PageKind::TransactionStatus,
            level: 0,
        };
        operation::fixed::membership(
            &storage.profile(),
            &allocator.identity,
            &request.identity,
            &bound.control.role,
            bound.control.base.before(),
            AllocationPopulation::Status,
            operation::mixed::Kind::Status,
        )
        .map_err(|_| unchanged(Cause::Membership))?;
        bound.pool.status_candidate(request, start).map_err(frame)
    }

    #[cfg(test)]
    pub(crate) fn pressure_test_layout() -> (usize, usize, bool) {
        (
            call_bytes::<W>(),
            super::super::retained_bytes::<W>(),
            fits::<W>(),
        )
    }
}

fn call_bytes<W: WriterNamespace>() -> usize {
    // The candidate helper returns before load begins. The outer result slot,
    // candidate and input references coexist with the load's complete census.
    let outer = size_of::<Result<StatusResidencyStep>>()
        + size_of::<StatusCandidate>()
        + size_of::<PageRef>()
        + size_of::<StatusResidencyCursor>()
        + 6 * size_of::<usize>();
    let select = dirty_frame::target_set_call_bytes::<1>()
        + 6 * size_of::<StatusCandidate>()
        + 4 * size_of::<FrameStamp>()
        + 3 * size_of::<TargetRequest>()
        + 2 * size_of::<WalPrefix>()
        + super::super::super::super::super::budget::TEMPORARIES;
    outer + super::load::call_bytes::<W>().max(select)
}
fn fits<W: WriterNamespace>() -> bool {
    call_bytes::<W>() <= 65536
        && super::super::retained_bytes::<W>() + call_bytes::<W>() <= 196608
        && super::super::retained_bytes::<W>() + call_bytes::<W>() + 65536 <= 262144
        && super::load::fits::<W>()
}

#[cfg(test)]
#[test]
fn mixed_class_delta_preserves_old_empty_transaction_bounds() {
    type Memory = crate::storage::wal_io::bootstrap::test_support::Writer;
    type Native = crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::native_test_support::Writer;
    for (kind, conversion, load, pressure, retained, valid) in [
        (
            "memory",
            super::super::conversion_bytes::<Memory>(),
            super::load::call_bytes::<Memory>(),
            call_bytes::<Memory>(),
            super::super::retained_bytes::<Memory>(),
            fits::<Memory>() && super::super::handoff_fits::<Memory>(),
        ),
        (
            "native",
            super::super::conversion_bytes::<Native>(),
            super::load::call_bytes::<Native>(),
            call_bytes::<Native>(),
            super::super::retained_bytes::<Native>(),
            fits::<Native>() && super::super::handoff_fits::<Native>(),
        ),
    ] {
        println!(
            "MIXED_CLASS_OLD_BOUND kind={kind} conversion={conversion} load={load} pressure={pressure} retained={retained} valid={valid}"
        );
        assert!(valid);
    }
}
