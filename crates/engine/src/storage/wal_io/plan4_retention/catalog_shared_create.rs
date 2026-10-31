//! Statement-bound CREATE admission, carried by the one original reservation.
use super::*;
use crate::transaction::concurrency::TerminalReadGate;
use transaction::catalog::CreateStatementBinding;

pub(crate) struct CatalogCreated<'i> {
    object: ObjectHandle,
    private: &'i mut [u8; PRIVATE_BYTES],
}
impl<'i> CatalogCreated<'i> {
    pub(super) fn new(object: ObjectHandle, private: &'i mut [u8; PRIVATE_BYTES]) -> Self {
        Self { object, private }
    }
    pub(crate) fn into_parts(self) -> (ObjectHandle, &'i mut [u8; PRIVATE_BYTES]) {
        (self.object, self.private)
    }
}
pub(crate) struct CatalogCancelled<'i> {
    private: &'i mut [u8; PRIVATE_BYTES],
}
impl<'i> CatalogCancelled<'i> {
    pub(super) fn new(private: &'i mut [u8; PRIVATE_BYTES]) -> Self {
        Self { private }
    }
    pub(crate) fn into_private(self) -> &'i mut [u8; PRIVATE_BYTES] {
        self.private
    }
}
pub(crate) enum CatalogCreateError<'i> {
    Before {
        error: RetentionError,
        private: &'i mut [u8; PRIVATE_BYTES],
    },
    Fatal(RetentionError),
}
impl<'i> CatalogCreateError<'i> {
    pub(super) fn from_error(error: RetentionError, private: &'i mut [u8; PRIVATE_BYTES]) -> Self {
        if error.effect == Effect::Unchanged {
            Self::Before { error, private }
        } else {
            Self::Fatal(error)
        }
    }
    #[cfg(test)]
    pub(super) fn error(self) -> RetentionError {
        match self {
            Self::Before { error, .. } | Self::Fatal(error) => error,
        }
    }
}

/// Only constructed with all three original loans present. Options permit
/// checked consuming release; no partially admitted reservation is exposed.
pub(super) struct CatalogCreateAdmission<'r, 't, 'v, 'a, 'c, 's> {
    pub(super) statement: CreateStatementBinding<'r, 't, 'v, 'a, 'c, 's>,
    source: Option<source::OperationSourceAdmission<'r, 'a, 'c, 's>>,
    gate: Option<TerminalReadGate<'c, 's>>,
}
fn reopen(gate: TerminalReadGate<'_, '_>) -> Result<()> {
    gate.reopen_checked().map_err(|error| RetentionError {
        phase: RetentionPhase::Census,
        cause: RetentionCause::Concurrency(error),
        effect: error.effect,
    })
}
impl CatalogCreateAdmission<'_, '_, '_, '_, '_, '_> {
    pub(super) fn finish(&mut self) -> Result<()> {
        // Attempt both returns, including when the source return detects fault.
        let source = self
            .source
            .take()
            .expect("original source admission")
            .finish_checked()
            .map_err(RetentionError::from);
        let gate = reopen(self.gate.take().expect("original read gate"));
        merge(source, gate)
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    #[allow(
        clippy::too_many_arguments,
        clippy::result_large_err,
        reason = "explicit original resources and linear reservation returned inline"
    )]
    pub(crate) fn reserve_catalog_create<'r, 't, 'v, 'i>(
        &'r mut self,
        owner: &'r mut Owner<'c, 's>,
        view: &'r RowStatementView<'t, 'v, 'a, 'c, 's>,
        undo: &Plan4Target,
        definition: &'i crate::api::TableSpec,
        private: &'i mut [u8; PRIVATE_BYTES],
        flusher: &'r mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> std::result::Result<
        ReservedCatalogCreate<'r, 't, 'v, 'i, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>,
        CatalogCreateError<'i>,
    > {
        let _unwind = self.storage.session_fence().unwind_guard();
        let admitted = (|| {
            // A Row capsule's view is refused by kind before the gate, the
            // source reservation or any read-set registration.
            if view.row_scope() {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
            self.storage.health().map_err(|error| {
                fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::Storage(error),
                )
            })?;
            self.pool
                .check_retention_entry_health()
                .map_err(|error| frame(RetentionPhase::Census, error))?;
            if !self
                .pool
                .uses_storage_origin(&self.storage.binding, self.storage.session_fence())
            {
                return Err(fatal(
                    self.storage,
                    RetentionPhase::Census,
                    RetentionCause::BindingMismatch,
                ));
            }
            if !std::ptr::eq(flusher.pool, self.pool)
                || !std::ptr::eq(flusher.storage, self.storage)
            {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::BindingMismatch,
                ));
            }
            let gate = owner
                .close_reads_for_retirement(self.pool.bound_core())
                .map_err(|error| RetentionError {
                    phase: RetentionPhase::Prepare,
                    cause: RetentionCause::Concurrency(error),
                    effect: error.effect,
                })?;
            let source = match source::OperationSourceAdmission::catalog_progress(
                self.isolation,
                &self.commit_charges,
                &self.progress_charge,
                &gate,
            ) {
                Ok(source) => source,
                Err(error) => return merge(Err(RetentionError::from(error)), reopen(gate)),
            };
            match CreateStatementBinding::admit(self, owner, view) {
                Ok(statement) => Ok(CatalogCreateAdmission {
                    statement,
                    source: Some(source),
                    gate: Some(gate),
                }),
                Err(error) => {
                    let returned = source.finish_checked().map_err(RetentionError::from);
                    let reopened = reopen(gate);
                    merge(merge(Err(error), returned), reopened)
                }
            }
        })();
        let admission = match admitted {
            Ok(admission) => admission,
            Err(error) => return Err(CatalogCreateError::from_error(error, private)),
        };
        let actor = admission.statement.actor();
        CatalogCreateAccess {
            storage: self.storage,
            pool: self.pool,
            control: self.control,
            history: self.history,
            visibility: self.visibility,
            flush: flusher.control,
        }
        .reserve(owner, actor, undo, definition, private, Some(admission))
    }
}

/// CREATE-only outer carriers, in addition to the original token, planner,
/// prepared/apply and lease costs. No recursive census runs during entry.
pub(in super::super::super) fn carrier_bytes() -> usize {
    use std::mem::size_of;
    3 * size_of::<CatalogCreateAdmission<'_, '_, '_, '_, '_, '_>>()
        + 3 * size_of::<Result<CatalogCreateAdmission<'_, '_, '_, '_, '_, '_>>>()
        + 3 * size_of::<CatalogCreateError<'_>>()
        + 3 * size_of::<Result<()>>()
        + 2 * size_of::<CatalogCreated<'_>>()
        + 2 * size_of::<CatalogCancelled<'_>>()
        + 2 * size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 16 * size_of::<&()>()
        + source::commit_carrier_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_create_phases_fit_original_live_and_recovered_limits() {
        fn check<W: WriterNamespace>() {
            let phases = catalog_live_source_bytes::<W>();
            let fixed = source::progress_test_bytes(&source::CatalogProgressCharge::new::<W>());
            let live = resources::source_census::<W>();
            let recovered =
                super::super::super::super::super::super::plan4_recovery::source_census::<W>();
            println!(
                "CREATE_ADMISSION_SOURCE carrier={} reserve={} forward={} cancel={} fixed={} live={} recovered={}",
                carrier_bytes(),
                phases.reserve,
                phases.forward,
                phases.cancel,
                fixed,
                live.peak,
                recovered.peak
            );
            assert!(fixed >= phases.reserve.max(phases.forward).max(phases.cancel));
            assert!(live.peak <= 65536 && recovered.peak <= 65536);
        }
        check::<crate::storage::wal_io::plan4_test_support::Writer>();
        check::<super::super::super::super::super::native::Writer>();
    }
}
