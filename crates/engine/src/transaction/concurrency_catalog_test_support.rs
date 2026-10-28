//! Actual consumer observations only. No grant, private receipt, owner, pin,
//! counter, or semantic state can be constructed or changed through this API.
use super::{Installation, PageConcurrency, test_support};
use std::sync::mpsc::{Receiver, SyncSender};

pub(crate) use test_support::Resume;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Point {
    CommitPreparing,
    CommitPrepared,
    CommitCompleted,
    StageCallStarted,
    ExistingStageCallStarted,
    Pending,
    Durable,
    Draining,
    DrainWaiting,
    DrainedBeforeResume,
    DrainedDrop,
    InstalledSecondPage,
    InstalledBeforeFinish,
    BeforePublish,
    LookupCaptured,
    LookupBeforeCatalogRecord,
    LookupBeforeSourceReserve,
    LookupBeforeSourceReturn,
    ResidentAllocatorCopied,
    ResidentStatusLocated,
    ResidentUndoLocated,
    RetirementIntent,
    RetirementPreparing,
    RetirementPrepared,
    RetirementCompleted,
    LifecyclePreparing,
    LifecyclePrepared,
    LifecycleCompleted,
    RetirementFirstPage,
    ReadGateWaiting,
}

pub(crate) fn install(
    core: &PageConcurrency<'_>,
    point: Point,
    arrived: SyncSender<()>,
    release: Receiver<Resume>,
) {
    test_support::install_catalog_pause(core, point, arrived, release);
}

pub(crate) fn hit(core: &PageConcurrency<'_>, point: Point) {
    test_support::hit_catalog_pause(core, point);
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Observation {
    pub(crate) active_apply: u16,
    pub(crate) active_content: u16,
    pub(crate) active_read_operations: usize,
    pub(crate) installation: &'static str,
    pub(crate) fenced: bool,
    pub(crate) read_gate_closed: bool,
}

pub(crate) fn observe(core: &PageConcurrency<'_>) -> Observation {
    let state = core.state.lock().unwrap_or_else(|e| e.into_inner());
    let installation = match state.installation {
        Installation::Open => "Open",
        Installation::Intent(_) => "Intent",
        Installation::Pending(_, _) => "Pending",
        Installation::Draining(_, _) => "Draining",
        Installation::Installing(_, _) => "Installing",
        Installation::Abandoned(_, _) => "Abandoned",
    };
    Observation {
        active_apply: state.active_apply,
        active_content: state.active_content,
        active_read_operations: state
            .slots
            .iter()
            .filter(|slot| slot.read_operation)
            .count(),
        installation,
        fenced: core.fence.is_fenced(),
        read_gate_closed: state.read_gate.is_some(),
    }
}

/// Request normal unfinished Drop of an actual Drained carrier. This creates
/// no grant or state; the producer drops its original value before returning.
pub(crate) fn drop_drained(core: &PageConcurrency<'_>) -> bool {
    test_support::drop_catalog_drained(core)
}
