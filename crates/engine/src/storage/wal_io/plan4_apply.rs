//! Data boundary for producers consuming the original plan4 owner.
//!
//! C0 supplies observations and disjoint borrowed buffers here. Actual source
//! capture, reservation and installation adapters are not yet implemented.
//! Private fields stay within this module and its future checked adapters;
//! neither observations nor freely constructed proposals confer authority.

use crate::{
    api::Namespace,
    storage::{
        catalog_record::CatalogExpectation,
        control::Profile,
        dirty_frame::FrameStamp,
        page::{PAGE_BYTES, PageIdentity, PageKind},
        transaction_record::{StatusSlotRef, TxnRef},
        tree_page::TreeExpectation,
        undo_record::UndoLink,
        undo_ref::UndoRef,
        wal_frame::{FrameExpectation, WalFrameHeader},
    },
};
use foundation::identity::{ObjectHandle, RowId, SchemaEpoch};

/// Metadata cut observed by the trusted preflight adapter. A Catalog decoder
/// alone cannot construct this cut, a checked table or a reservation.
#[derive(Debug)]
pub(crate) struct RowPreflightContext {
    profile: Profile,
    table: ObjectHandle,
    catalog_expected: CatalogExpectation,
    schema_cell: (u16, u16),
    leaf_expected: TreeExpectation,
    leaf_free_bytes: usize,
    canonical_capacity: usize,
    index_count: u16,
    catalog_source: (FrameStamp, [u8; 32]),
}

impl RowPreflightContext {
    pub(in crate::storage) fn profile(&self) -> Profile {
        self.profile
    }
    pub(in crate::storage) fn namespace(&self) -> Namespace {
        self.catalog_expected.namespace
    }
    pub(in crate::storage) fn table(&self) -> ObjectHandle {
        self.table
    }
    pub(in crate::storage) fn schema_epoch(&self) -> SchemaEpoch {
        self.table.schema_epoch
    }
    pub(in crate::storage) fn catalog_expected(&self) -> CatalogExpectation {
        self.catalog_expected
    }
    pub(in crate::storage) fn schema_cell(&self) -> (u16, u16) {
        self.schema_cell
    }
    pub(in crate::storage) fn leaf_expected(&self) -> TreeExpectation {
        self.leaf_expected
    }
    pub(in crate::storage) fn leaf_free_bytes(&self) -> usize {
        self.leaf_free_bytes
    }
    pub(in crate::storage) fn canonical_capacity(&self) -> usize {
        self.canonical_capacity
    }
    pub(in crate::storage) fn index_count(&self) -> u16 {
        self.index_count
    }
    pub(in crate::storage) fn catalog_source(&self) -> (FrameStamp, [u8; 32]) {
        self.catalog_source
    }
}

/// Logical roles can alias one physical before image, with distinct cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowSourceRole {
    Root,
    RootJob,
    TerminalJob,
    Status,
    Undo,
    RowLeaf,
    Allocator,
    TableAnchor,
    PreviousUndo,
}

impl RowSourceRole {
    const fn index(self) -> usize {
        match self {
            Self::Root => 0,
            Self::RootJob => 1,
            Self::TerminalJob => 2,
            Self::Status => 3,
            Self::Undo => 4,
            Self::RowLeaf => 5,
            Self::Allocator => 6,
            Self::TableAnchor => 7,
            Self::PreviousUndo => 8,
        }
    }
}

#[derive(Debug)]
pub(crate) struct RowSourceBinding {
    before_index: u8,
    identity: PageIdentity,
    kind: PageKind,
    level: u8,
    // All three are present together; partial cell coordinates are impossible.
    cell: Option<(u16, u16, u16)>,
}

impl RowSourceBinding {
    pub(in crate::storage) fn before_index(&self) -> u8 {
        self.before_index
    }
    pub(in crate::storage) fn expected(&self) -> (PageIdentity, PageKind, u8) {
        (self.identity, self.kind, self.level)
    }
    pub(in crate::storage) fn cell(&self) -> Option<(u16, u16, u16)> {
        self.cell
    }
}

/// The two mutations produced by A. These discriminants are not WAL opcodes;
/// common Begin/Reserve/terminal/maintenance effects remain with the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowOperation {
    Forward,
    Clr,
}

/// Changed physical-page roles for a Forward/CLR proposal, not wire tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowPageRole {
    Root,
    RootJob,
    Status,
    Undo,
    RowLeaf,
}

#[derive(Debug)]
pub(crate) enum RowProducerInput {
    Insert {
        canonical_len: usize,
        canonical_digest: [u8; 32],
    },
    Clr {
        current_row_len: usize,
        current_projection: [u8; 32],
        rollback_target: Option<UndoRef>,
        next: Option<UndoLink>,
    },
}

/// Read-only scalar cut. No field, getter or byte digest is an install grant.
#[derive(Debug)]
pub(crate) struct RowProducerContext {
    schema: RowPreflightContext,
    operation: RowOperation,
    frame_expected: FrameExpectation,
    header: WalFrameHeader,
    actor: TxnRef,
    status: StatusSlotRef,
    row: RowId,
    undo: UndoLink,
    statement_sequence: Option<u64>,
    planned_mutation: u64,
    statement_mark: Option<UndoRef>,
    predecessor: Option<UndoLink>,
    root_job_ordinal: u8,
    terminal_job_ordinal: u8,
    before_count: u8,
    sources: [Option<RowSourceBinding>; 9],
    after_order: [RowPageRole; 5],
    // Actual adapters must establish this bound before constructing the cut.
    after_count: u8,
    input: RowProducerInput,
}

impl RowProducerContext {
    pub(in crate::storage) fn schema(&self) -> &RowPreflightContext {
        &self.schema
    }
    pub(in crate::storage) fn operation(&self) -> RowOperation {
        self.operation
    }
    pub(in crate::storage) fn frame_expected(&self) -> FrameExpectation {
        self.frame_expected
    }
    pub(in crate::storage) fn header(&self) -> WalFrameHeader {
        self.header
    }
    pub(in crate::storage) fn actor(&self) -> TxnRef {
        self.actor
    }
    pub(in crate::storage) fn status(&self) -> StatusSlotRef {
        self.status
    }
    pub(in crate::storage) fn row(&self) -> RowId {
        self.row
    }
    pub(in crate::storage) fn undo(&self) -> UndoLink {
        self.undo
    }
    pub(in crate::storage) fn statement_sequence(&self) -> Option<u64> {
        self.statement_sequence
    }
    pub(in crate::storage) fn planned_mutation(&self) -> u64 {
        self.planned_mutation
    }
    pub(in crate::storage) fn statement_mark(&self) -> Option<UndoRef> {
        self.statement_mark
    }
    pub(in crate::storage) fn predecessor(&self) -> Option<UndoLink> {
        self.predecessor
    }
    pub(in crate::storage) fn root_job_ordinal(&self) -> u8 {
        self.root_job_ordinal
    }
    pub(in crate::storage) fn terminal_job_ordinal(&self) -> u8 {
        self.terminal_job_ordinal
    }
    pub(in crate::storage) fn before_count(&self) -> u8 {
        self.before_count
    }
    pub(in crate::storage) fn source(&self, role: RowSourceRole) -> Option<&RowSourceBinding> {
        self.sources[role.index()].as_ref()
    }
    pub(in crate::storage) fn after_order(&self) -> &[RowPageRole] {
        &self.after_order[..usize::from(self.after_count)]
    }
    pub(in crate::storage) fn input(&self) -> &RowProducerInput {
        &self.input
    }
}

/// Borrows the original ledger, not a cloned 64 KiB bank or a second owner.
#[derive(Debug)]
pub(crate) struct RowControlObservation<'l> {
    jobs: &'l [[u8; 1_024]; 64],
    transaction: Option<&'l [u8; 1_536]>,
    aging: Option<&'l [u8; 1_536]>,
}

impl<'l> RowControlObservation<'l> {
    pub(in crate::storage) fn jobs(&self) -> &'l [[u8; 1_024]; 64] {
        self.jobs
    }
    pub(in crate::storage) fn transaction(&self) -> Option<&'l [u8; 1_536]> {
        self.transaction
    }
    pub(in crate::storage) fn aging(&self) -> Option<&'l [u8; 1_536]> {
        self.aging
    }
}

/// Consumed to split a short preflight loan. This is not a reservation.
pub(crate) struct RowValidationLane<'l> {
    context: RowPreflightContext,
    schema: &'l [u8; PAGE_BYTES],
    leaf: &'l [u8; PAGE_BYTES],
    candidate: &'l mut [u8; PAGE_BYTES],
}

impl<'l> RowValidationLane<'l> {
    pub(in crate::storage) fn parts(
        self,
    ) -> (
        RowPreflightContext,
        &'l [u8; PAGE_BYTES],
        &'l [u8; PAGE_BYTES],
        &'l mut [u8; PAGE_BYTES],
    ) {
        (self.context, self.schema, self.leaf, self.candidate)
    }
}

/// An untrusted candidate. The actual reserve adapter must recheck these fields
/// against source/schema/leaf state and the caller's encoded bytes.
#[derive(Debug)]
pub(crate) struct RowInputPlan {
    encoded_len: usize,
    schema_epoch: SchemaEpoch,
    digest: [u8; 32],
}

impl RowInputPlan {
    pub(in crate::storage) fn new(
        encoded_len: usize,
        schema_epoch: SchemaEpoch,
        digest: [u8; 32],
    ) -> Self {
        Self {
            encoded_len,
            schema_epoch,
            digest,
        }
    }
}

/// Disjoint slices borrowed from one private loan. Consuming this lane neither
/// advances WAL nor installs pages; those effects require the original owner.
pub(crate) struct RowProducerLane<'l> {
    context: RowProducerContext,
    control: RowControlObservation<'l>,
    before: &'l [[u8; PAGE_BYTES]; 9],
    schema: &'l [u8; PAGE_BYTES],
    frame: &'l mut [u8; 98_304],
    after: &'l mut [[u8; PAGE_BYTES]; 5],
    candidate: &'l mut [u8; PAGE_BYTES],
}

impl<'l> RowProducerLane<'l> {
    #[allow(
        clippy::type_complexity,
        reason = "one consuming split preserves all disjoint loan lifetimes"
    )]
    pub(in crate::storage) fn parts(
        self,
    ) -> (
        RowProducerContext,
        RowControlObservation<'l>,
        &'l [[u8; PAGE_BYTES]; 9],
        &'l [u8; PAGE_BYTES],
        &'l mut [u8; 98_304],
        &'l mut [[u8; PAGE_BYTES]; 5],
        &'l mut [u8; PAGE_BYTES],
    ) {
        (
            self.context,
            self.control,
            self.before,
            self.schema,
            self.frame,
            self.after,
            self.candidate,
        )
    }
}

/// Producer-declared metadata, deliberately not validated by this constructor.
/// Trusted prepare/stage must validate bytes, physical order, count and source.
#[derive(Debug)]
pub(crate) struct RowProposal {
    operation: RowOperation,
    frame_len: usize,
    after_count: usize,
    after_roles: [RowPageRole; 5],
}

impl RowProposal {
    pub(in crate::storage) fn new(
        operation: RowOperation,
        frame_len: usize,
        after_count: usize,
        after_roles: [RowPageRole; 5],
    ) -> Self {
        Self {
            operation,
            frame_len,
            after_count,
            after_roles,
        }
    }
}
