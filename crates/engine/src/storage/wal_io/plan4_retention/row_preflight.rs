//! The Row write preflight body (S2-0 Row write amendment §1 "order" and
//! "residency", S2-0b amendment `#row-write-validate-once`). Its caller,
//! `MutationPort::preflight_row`, already holds the terminal read gate, the
//! RowWrite admission and the flush control borrow; this body adds the deep
//! checks, the residency of the write target set T through the one retention
//! LOAD body, the Free Undo selection, the row's one `history::validate_live`,
//! the typed read of T compared with the replay arena, the read-set
//! registration and the `RowPreflightCut`. It opens no read operation: the
//! LOAD claims and typed reads use content admission only, and the replay
//! scope is a checkout of the replay buffer. Every refusal returns before the
//! registration, so a refused or `FlushRequired` preflight registers nothing.
use super::super::super::lookup_load::RetentionLoadPort as _;
use super::super::super::row::{RowPreflightCut, RowRoleMap};
use super::super::super::visibility::TableSlot;
use super::*;
use crate::storage::{
    anchor::{self, TreeKind},
    catalog_record::CatalogDefinitionView,
    retention_page::row as job3,
    semantic_row::transition as row_group,
    tree_page::{self, EntryPayload, TREE_ENTRY_CAPACITY, TreeExpectation},
    undo_record,
    undo_ref::UndoRef,
};
use foundation::codec::ContentHasher;

/// Jobs per Jobs page: a Job ordinal's page is `Plan4Page::Jobs { index:
/// ordinal / JOBS_PER_PAGE }`, as Row Begin and AbortEnd select it.
const JOBS_PER_PAGE: usize = 15;
/// Nibble value of a role with no before image in `RowRoleMap`.
const ABSENT_ROLE: u64 = 0xF;

/// What the body returns to the shell. `FlushRequired` names the dirty victim
/// the residency of a target needs; nothing was registered.
#[allow(
    clippy::large_enum_variant,
    reason = "the cut is the Ready value the shell moves into the wrapper"
)]
pub(in super::super::super) enum RowPreflightOutcome {
    Ready(RowPreflightCut),
    FlushRequired(Plan4Target),
}

/// The scalars of an issued `CheckedTable` the body rechecks.
#[derive(Clone, Copy)]
pub(in super::super::super) struct RowTableSource {
    pub(in super::super::super) handle: ObjectHandle,
    pub(in super::super::super) slot: u8,
    pub(in super::super::super) anchor: PageIdentity,
}

/// The selected Free Undo container: its request (the page incarnation is the
/// allocator entry generation) and the slot0 Free record's generation.
#[derive(Clone, Copy)]
struct FreeUndo {
    request: TargetRequest,
    slot0: Generation,
}

/// Every role absent in a `RowRoleMap` (nine nibbles, count zero).
const NO_ROLES: RowRoleMap = 0xF_FFFF_FFFF;

/// The row's sources: the allocator (frame 0, resident and never a LOAD
/// victim) and the others in residency order, the Catalog leaf first, then
/// each T member as it is selected. The first `resident` requests are
/// resident and are, as they stand, the LOAD claim's protected list; the
/// requests up to `queued` wait for their LOAD step. No two sources name one
/// page, so the list holds at most nine. `roles` maps each T role but the
/// allocator's to its position in the list.
struct RowSources {
    allocator: TargetRequest,
    requests: [TargetRequest; 9],
    resident: usize,
    queued: usize,
    roles: RowRoleMap,
}
impl RowSources {
    fn new(catalog: TargetRequest, allocator: TargetRequest) -> Self {
        Self {
            allocator,
            requests: [catalog; 9],
            resident: 0,
            queued: 1,
            roles: NO_ROLES,
        }
    }
    fn catalog(&self) -> TargetRequest {
        self.requests[0]
    }
    fn protected(&self) -> &[TargetRequest] {
        &self.requests[..self.resident]
    }
    fn queued(&self) -> &[TargetRequest] {
        &self.requests[..self.queued]
    }
    fn get(&self, role: RowSourceRole) -> Option<TargetRequest> {
        let at = (self.roles >> (4 * role.index())) & 0xF;
        (at != ABSENT_ROLE).then(|| self.requests[at as usize])
    }
    /// Queues `request` for `role`, or maps the role to the same request
    /// queued before (physical de-duplication of the logical roles). Two
    /// different requests for one page, or a source on the allocator's page,
    /// are a census contradiction.
    fn queue<W: WriterNamespace>(
        &mut self,
        storage: &MutableStorageOwner<'_, '_, W>,
        request: TargetRequest,
        role: RowSourceRole,
    ) -> Result<()> {
        let at = match self.queued().iter().position(|queued| *queued == request) {
            Some(at) => at,
            None => {
                let page_no = request.identity.page.page_no;
                if self.queued == self.requests.len()
                    || page_no == self.allocator.identity.page.page_no
                    || self
                        .queued()
                        .iter()
                        .any(|queued| queued.identity.page.page_no == page_no)
                {
                    return Err(fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    ));
                }
                self.requests[self.queued] = request;
                self.queued += 1;
                self.queued - 1
            }
        };
        let shift = 4 * role.index();
        self.roles = (self.roles & !(0xF << shift)) | ((at as u64) << shift);
        Ok(())
    }
    /// Drops the last queued request, which `role` alone names: a Free Undo
    /// candidate that failed its check leaves the protected list.
    fn unqueue(&mut self, role: RowSourceRole) {
        self.queued -= 1;
        self.resident = self.resident.min(self.queued);
        self.roles |= 0xF << (4 * role.index());
    }
}

/// T in before order (the original pool's rank order) and its role map.
struct RowTargets {
    requests: [TargetRequest; 9],
    count: usize,
    roles: RowRoleMap,
}
impl RowTargets {
    /// T is the allocator and the queued sources without the Catalog leaf,
    /// which `RowSources::queue` keeps on distinct pages.
    fn new(sources: &RowSources) -> Self {
        let allocator = sources.allocator;
        let mut requests = [allocator; 9];
        let mut count = 1;
        for request in &sources.queued()[1..] {
            requests[count] = *request;
            count += 1;
        }
        requests[..count].sort_unstable_by_key(|request| target_rank(*request));
        let position = |request: TargetRequest| {
            requests[..count]
                .iter()
                .position(|actual| *actual == request)
                .expect("a queued source is in T") as u64
        };
        let mut roles = (count as u64) << 36;
        for index in 0..9 {
            let queued = (sources.roles >> (4 * index)) & 0xF;
            let at = if index == RowSourceRole::Allocator.index() {
                position(allocator)
            } else if queued == ABSENT_ROLE {
                ABSENT_ROLE
            } else {
                position(sources.requests[queued as usize])
            };
            roles |= at << (4 * index);
        }
        Self {
            requests,
            count,
            roles,
        }
    }
    fn before(&self) -> &[TargetRequest] {
        &self.requests[..self.count]
    }
    fn at(&self, role: RowSourceRole) -> Option<usize> {
        let at = (self.roles >> (4 * role.index())) & 0xF;
        (at != ABSENT_ROLE).then_some(at as usize)
    }
}

/// The private ranges preflight uses (`RowLoanLayout`): the LOAD scratch in
/// the frame range, the before images of T, the Catalog leaf copy in the
/// schema range and the allocator copy of the Free Undo selection in the XOI
/// range. No other range is written.
struct PreflightLanes<'l> {
    scratch: &'l mut [u8],
    before: &'l mut [[u8; PAGE_BYTES]; 9],
    schema: &'l mut [u8; PAGE_BYTES],
    allocator: &'l mut [u8; PAGE_BYTES],
}
impl<'l> PreflightLanes<'l> {
    fn split(private: &'l mut [u8; 425_984]) -> Self {
        let (scratch, rest) = private.split_at_mut(RowLoanLayout::BEFORE.start);
        let (before, rest) =
            rest.split_at_mut(RowLoanLayout::BEFORE.end - RowLoanLayout::BEFORE.start);
        let (_, rest) = rest.split_at_mut(RowLoanLayout::SCHEMA.start - RowLoanLayout::BEFORE.end);
        let (schema, rest) = rest.split_at_mut(PAGE_BYTES);
        let (allocator, _) = rest.split_at_mut(PAGE_BYTES);
        Self {
            scratch,
            before: before
                .as_chunks_mut::<PAGE_BYTES>()
                .0
                .try_into()
                .expect("nine before images"),
            schema: schema.try_into().expect("schema copy page"),
            allocator: allocator.try_into().expect("allocator copy page"),
        }
    }
}

/// The values the preflight commitment folds (`RowPreflightCut::source`), in
/// the order the comment before the cuts gives.
struct PreflightSource<'x> {
    prefix: WalPrefix,
    targets: &'x [TargetRequest],
    stamps: &'x [FrameStamp],
    roles: RowRoleMap,
    undo: FreeUndo,
    root_job: u8,
    catalog: (FrameStamp, [u8; 32]),
    leaf_free: u16,
    pages: &'x [[u8; PAGE_BYTES]],
    control: ControlCut,
}
impl PreflightSource<'_> {
    /// Kind 1 of the XRWC commitment. WAL prefix: stream, file and the
    /// complete position (the synced position is durability progress, not
    /// source content).
    fn commitment(&self) -> [u8; 32] {
        let mut hasher = ContentHasher::new();
        hasher.update(b"XRWC");
        hasher.update(&[1, 1]);
        let prefix = self.prefix;
        hasher.update(&prefix.stream.database.bytes());
        hasher.update(&prefix.stream.timeline.to_le_bytes());
        hasher.update(&prefix.stream.writer_epoch.to_le_bytes());
        hasher.update(&prefix.file.device.to_le_bytes());
        hasher.update(&prefix.file.inode.to_le_bytes());
        hasher.update(&prefix.complete_lsn.to_le_bytes());
        hasher.update(&prefix.complete_offset.to_le_bytes());
        for (request, stamp) in self.targets.iter().zip(self.stamps) {
            fold_identity(&mut hasher, request.identity);
            hasher.update(&[request.kind as u8, request.level]);
            fold_stamp(&mut hasher, stamp);
        }
        hasher.update(&self.roles.to_le_bytes());
        // The Free Undo's allocator entry generation, then its page
        // incarnation. They are one value: the request's incarnation is the
        // entry generation, which the membership proof found in the allocator
        // BEFORE image and the allocator page digest below fixes for every
        // later step. It is folded twice, once per item the cut doc names, so
        // that every recomputation reproduces this exact byte sequence.
        let incarnation = self.undo.request.identity.page.incarnation.get();
        hasher.update(&incarnation.to_le_bytes());
        hasher.update(&incarnation.to_le_bytes());
        hasher.update(&self.undo.slot0.get().to_le_bytes());
        hasher.update(&[self.root_job]);
        fold_stamp(&mut hasher, &self.catalog.0);
        hasher.update(&self.catalog.1);
        hasher.update(&self.leaf_free.to_le_bytes());
        for page in self.pages {
            hasher.update(&content_digest(page));
        }
        hasher.update(&self.control.bank);
        hasher.update(&self.control.jobs);
        hasher.update(&self.control.inventory);
        hasher.update(&self.control.issued.to_le_bytes());
        hasher.update(&self.control.adopted.to_le_bytes());
        hasher.digest()
    }
}

/// The WAL prefix fields the commitment folds: stream, file and the complete
/// position. A sync moves only the synced position, which is durability
/// progress and not source content, so it is no source change.
pub(super) fn same_source_prefix(current: WalPrefix, expected: WalPrefix) -> bool {
    current.stream == expected.stream
        && current.file == expected.file
        && current.complete_lsn == expected.complete_lsn
        && current.complete_offset == expected.complete_offset
}

fn fold_identity(hasher: &mut ContentHasher, identity: PageIdentity) {
    let mut page = [0; 24];
    identity
        .page
        .encode(&mut page)
        .expect("fixed PageRef output");
    hasher.update(&identity.database.bytes());
    hasher.update(&page);
    hasher.update(&identity.owner.get().to_le_bytes());
    hasher.update(&identity.owner_generation.get().to_le_bytes());
}

fn fold_stamp(hasher: &mut ContentHasher, stamp: &FrameStamp) {
    fold_identity(hasher, stamp.identity);
    hasher.update(&stamp.generation.to_le_bytes());
    hasher.update(&stamp.page_lsn.0.to_le_bytes());
    hasher.update(&stamp.modification_counter.to_le_bytes());
    match stamp.rec_lsn {
        None => hasher.update(&[0; 9]),
        Some(lsn) => {
            hasher.update(&[1]);
            hasher.update(&lsn.0.to_le_bytes());
        }
    }
}

/// An Undo container's request at the allocator's file.
fn undo_request(allocator: PageIdentity, page: PageRef) -> TargetRequest {
    request(
        PageIdentity {
            database: allocator.database,
            page,
            owner: ObjectId::new(3).expect("Undo owner"),
            owner_generation: one(),
        },
        PageKind::Undo,
    )
}

/// The slot0 Free generation of a one-slot Undo page whose only cell is the
/// initial Free record, or None for any other page.
fn free_slot0(bytes: &[u8; PAGE_BYTES], identity: PageIdentity) -> Option<Generation> {
    let page = crate::storage::page::decode_page(bytes, identity, PageKind::Undo).ok()?;
    if page.len() != 1 || page.cell_range(0) != Some(undo_record::INITIAL_UNDO_CELL) {
        return None;
    }
    let cell = page.cell(0)?;
    let generation = Generation::new(u64::from_le_bytes(cell.get(8..16)?.try_into().ok()?)).ok()?;
    undo_record::decode_free_undo_record(cell, generation).ok()
}

/// What `row_shape` reads from Root and the table anchor.
struct RowShape {
    root_job: usize,
    leaf: TargetRequest,
}

/// What the observation under the replay checkout returns.
struct Observed {
    source: [u8; 32],
    leaf_free: u16,
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The body under the shell's gate, admission and flush borrow. The order
    /// is the amendment's: deep checks, residency, Free Undo selection, the
    /// one `validate_live`, the typed read of T compared with its arena,
    /// registration, cut. Every LOAD step is made from this frame, and the
    /// steps that select sources are calls of their own, so a LOAD holds
    /// none of their values; the observation is a call of its own too.
    pub(in super::super::super) fn row_preflight(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        table: RowTableSource,
        private: &mut [u8; 425_984],
    ) -> Result<RowPreflightOutcome> {
        let storage = self.storage;
        let preparation = self.catalog_preparation();
        // Entry checks of an existing-group preparation, then the actor: an
        // Active Row actor with no RootReserved root and its one Terminal.
        let (_, prefix) = preparation.existing_entry(owner)?;
        let actor = statement
            .own_actor()
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::WrongState))?;
        let head = self.row_statement_head(actor, RetentionPhase::Prepare)?;
        let (terminal, status) = preparation.row_terminal(actor)?;
        self.check_table_slot(table)?;
        let mut lanes = PreflightLanes::split(private);
        let mut cursor = Plan4ResidencyCursor::new();
        // The Catalog leaf, Root and the table anchor.
        let mut sources = RowSources::new(
            target_from_source(storage, Plan4Page::CatalogLeaf)?.request,
            self.pool
                .target_request(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?,
        );
        self.queue_root_anchor(&mut sources, table)?;
        if let Some(target) = self.make_resident(owner, &mut sources, &mut cursor, lanes.scratch)? {
            return Ok(RowPreflightOutcome::FlushRequired(target));
        }
        // The RootJob and Terminal Jobs pages, Status, the RowLeaf and the
        // head's Undo.
        let status = preparation.catalog_status_request(status);
        let root_job =
            self.queue_row_sources(owner, &mut sources, table, (head, terminal, status))?;
        if let Some(target) = self.make_resident(owner, &mut sources, &mut cursor, lanes.scratch)? {
            return Ok(RowPreflightOutcome::FlushRequired(target));
        }
        // The Free Undo: the first candidate in allocator order, made
        // resident, whose slot0 is the initial Free record.
        self.copy_allocator(owner, sources.allocator, lanes.allocator)?;
        let leased = self.leased_undo_pages(&sources)?;
        let mut next = 1;
        let slot0 = loop {
            let Some(candidate) = self.next_undo_candidate(
                lanes.allocator,
                sources.allocator.identity,
                leased,
                &mut next,
            )?
            else {
                return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Capacity));
            };
            sources.queue(storage, candidate, RowSourceRole::Undo)?;
            if let Some(target) =
                self.make_resident(owner, &mut sources, &mut cursor, lanes.scratch)?
            {
                return Ok(RowPreflightOutcome::FlushRequired(target));
            }
            if let Some(slot0) = self.read_free_slot0(owner, candidate)? {
                break slot0;
            }
            // A candidate that fails its check leaves the protected list.
            sources.unqueue(RowSourceRole::Undo);
        };
        self.row_observe_cut(
            owner,
            statement,
            prefix,
            &sources,
            (slot0, root_job, table),
            &mut lanes,
        )
        .map(RowPreflightOutcome::Ready)
    }

    /// Queues Root and the table anchor after the Catalog leaf.
    fn queue_root_anchor(&self, sources: &mut RowSources, table: RowTableSource) -> Result<()> {
        let storage = self.storage;
        let root = target_from_source(storage, Plan4Page::Root)?.request;
        sources.queue(storage, root, RowSourceRole::Root)?;
        sources.queue(
            storage,
            request(table.anchor, PageKind::TreeAnchor),
            RowSourceRole::TableAnchor,
        )
    }

    /// With Root and the anchor resident: the RootJob the RowReserve will
    /// lease and the RowLeaf, then queues the RootJob and Terminal Jobs
    /// pages, Status, the RowLeaf and the head's Undo. Returns the RootJob.
    fn queue_row_sources(
        &self,
        owner: &mut Owner<'c, 's>,
        sources: &mut RowSources,
        table: RowTableSource,
        (head, terminal, status): (Option<UndoRef>, usize, TargetRequest),
    ) -> Result<u8> {
        let storage = self.storage;
        let root = target_from_source(storage, Plan4Page::Root)?.request;
        let shape = self.row_shape(
            owner,
            root,
            request(table.anchor, PageKind::TreeAnchor),
            table,
        )?;
        let root_job = u8::try_from(shape.root_job)
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        sources.queue(
            storage,
            self.jobs_request(shape.root_job)?,
            RowSourceRole::RootJob,
        )?;
        sources.queue(
            storage,
            self.jobs_request(terminal)?,
            RowSourceRole::TerminalJob,
        )?;
        sources.queue(storage, status, RowSourceRole::Status)?;
        sources.queue(storage, shape.leaf, RowSourceRole::RowLeaf)?;
        if let Some(head) = head {
            let previous = undo_request(sources.allocator.identity, head.page());
            sources.queue(storage, previous, RowSourceRole::PreviousUndo)?;
        }
        Ok(root_job)
    }

    /// The Jobs page of a Job ordinal, as Row Begin and AbortEnd select it.
    fn jobs_request(&self, ordinal: usize) -> Result<TargetRequest> {
        let index = u8::try_from(ordinal / JOBS_PER_PAGE)
            .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        Ok(target_from_source(self.storage, Plan4Page::Jobs { index })?.request)
    }

    /// After the residency: T in before order, the one `validate_live` and
    /// the typed read of the Catalog leaf and T, the WAL prefix recheck, the
    /// read-set registration and the cut. Every refusal before the
    /// registration registers nothing.
    fn row_observe_cut(
        &self,
        owner: &mut Owner<'c, 's>,
        statement: &RowStatementView<'_, '_, 'a, 'c, 's>,
        prefix: WalPrefix,
        sources: &RowSources,
        (slot0, root_job, table): (Generation, u8, RowTableSource),
        lanes: &mut PreflightLanes<'_>,
    ) -> Result<RowPreflightCut> {
        let storage = self.storage;
        let targets = RowTargets::new(sources);
        #[cfg(test)]
        crate::transaction::concurrency::catalog_test_support::hit(
            self.pool.bound_core(),
            crate::transaction::concurrency::catalog_test_support::Point::RowPreflightResident,
        );
        let observed = self.observe_sources(
            owner,
            prefix,
            sources.catalog(),
            &targets,
            (slot0, root_job, table),
            lanes,
        )?;
        let current = storage
            .wal()
            .prefix()
            .map_err(|error| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(error)))?;
        if !same_source_prefix(current, prefix) {
            return Err(unchanged(RetentionPhase::Prepare, RetentionCause::Busy));
        }
        // Registration, then the cut: every refusal above registered nothing.
        statement.check_catalog_read()?;
        statement.record_table_read(table.slot)?;
        Ok(RowPreflightCut {
            source: observed.source,
            complete_lsn: prefix.complete_lsn,
            roles: targets.roles,
            leaf_free: observed.leaf_free,
            root_job,
        })
    }

    /// The table's epoch slot is still the issued one. A CheckedTable is
    /// issued only for a Published slot, which this open never unpublishes,
    /// so any other answer is the issuer's census contradiction.
    fn check_table_slot(&self, table: RowTableSource) -> Result<()> {
        let storage = self.storage;
        let visibility = self
            .visibility
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::BindingMismatch))?;
        let slot = visibility
            .table_slot(table.handle)
            .map_err(|e| RetentionError {
                phase: RetentionPhase::Census,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })?;
        if !matches!(slot, TableSlot::Published(actual) if actual == table.slot) {
            return Err(fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            ));
        }
        Ok(())
    }

    /// The LOAD steps of the queued sources, in order, through the original
    /// retention LOAD body; each LOAD protects the sources resident before
    /// it. A step that needs a dirty victim ends with it.
    fn make_resident(
        &self,
        owner: &mut Owner<'c, 's>,
        sources: &mut RowSources,
        cursor: &mut Plan4ResidencyCursor,
        scratch: &mut [u8],
    ) -> Result<Option<Plan4Target>> {
        while sources.resident < sources.queued {
            let source = sources.requests[sources.resident];
            match self.load_lookup_target(owner, source, cursor, sources.protected(), scratch)? {
                RetentionResidencyStep::Resident { .. } => sources.resident += 1,
                RetentionResidencyStep::FlushRequired { target, .. } => return Ok(Some(target)),
            }
        }
        Ok(None)
    }

    /// The RootJob RowReserve will lease (from Root and the live Jobs, as the
    /// producer selects it) and the RowLeaf the table anchor names.
    fn row_shape(
        &self,
        owner: &mut Owner<'c, 's>,
        root: TargetRequest,
        anchor: TargetRequest,
        table: RowTableSource,
    ) -> Result<RowShape> {
        let storage = self.storage;
        let mut sources = [root, anchor];
        sources.sort_unstable_by_key(|request| target_rank(*request));
        let (root_at, anchor_at) = if sources[0] == root { (0, 1) } else { (1, 0) };
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &sources)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let shape = (|| {
            let root = read
                .bytes(root_at)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            // No Free Job is the producer's own refusal and changes nothing;
            // a Root or Job that does not decode is the resident page's or
            // the control's corruption, Fatal as a deep check's decode.
            let root_job = row_group::reserve_job(
                root,
                self.control.jobs(),
                storage.profile().spec().namespace,
            )
            .map_err(|e| match e {
                crate::storage::semantic_row::Error::BudgetMismatch => {
                    unchanged(RetentionPhase::Prepare, RetentionCause::RowSemantic(e))
                }
                _ => fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::RowSemantic(e),
                ),
            })?;
            let anchor = read
                .bytes(anchor_at)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let decoded = anchor::decode_anchor(anchor, table.anchor)
                .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Page(e)))?
                .root();
            if decoded.tree != TreeKind::Row || decoded.level != 0 {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            }
            if decoded.page == table.anchor.page
                || decoded.page.file != table.anchor.page.file
                || !(9..64).contains(&decoded.page.page_no)
            {
                return Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Invariant,
                ));
            }
            Ok(RowShape {
                root_job,
                leaf: request(
                    PageIdentity {
                        database: table.anchor.database,
                        page: decoded.page,
                        owner: table.handle.object,
                        owner_generation: table.handle.generation,
                    },
                    PageKind::RowLeaf,
                ),
            })
        })();
        merge(
            shape,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// The allocator page into the XOI range, under a brief typed read.
    fn copy_allocator(
        &self,
        owner: &mut Owner<'c, 's>,
        allocator: TargetRequest,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<()> {
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[allocator])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let copied = read
            .copy_snapshot(0, output)
            .map(|_| ())
            .map_err(|e| frame(RetentionPhase::Prepare, e));
        merge(
            copied,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// The pages (below 64) a Free Undo candidate must not be: every Undo
    /// page a root Job of any actor leases, and every queued source. A Job
    /// that does not decode is the control's corruption, Fatal as every
    /// other deep check's decode.
    fn leased_undo_pages(&self, sources: &RowSources) -> Result<u64> {
        let storage = self.storage;
        let namespace = storage.profile().spec().namespace;
        let mut leased = 0u64;
        for source in sources.queued() {
            if source.identity.page.page_no < 64 {
                leased |= 1 << source.identity.page.page_no;
            }
        }
        for bytes in self.control.jobs() {
            let job = job3::decode_job(bytes, namespace).map_err(|error| {
                fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Retention(error),
                )
            })?;
            if job.state().is_root()
                && let Some(undo) = job.undo()
                && undo.page().page_no < 64
            {
                leased |= 1 << undo.page().page_no;
            }
        }
        Ok(leased)
    }

    /// The next page from `*next` that is not leased and is an Allocated
    /// Undo container of the Undo object, in the allocator copy.
    fn next_undo_candidate(
        &self,
        allocator: &[u8; PAGE_BYTES],
        identity: PageIdentity,
        leased: u64,
        next: &mut u64,
    ) -> Result<Option<TargetRequest>> {
        let storage = self.storage;
        let census = |error| {
            fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(error),
            )
        };
        let page = allocation_page::decode_allocation_page(
            allocator,
            identity,
            storage.profile().spec().namespace,
        )
        .map_err(census)?;
        while *next < 64 {
            let page_no = *next;
            *next += 1;
            if leased & (1 << page_no) != 0 {
                continue;
            }
            if let AllocationEntry::Allocated {
                generation,
                object,
                object_generation,
                kind: PageKind::Undo,
            } = page.entry(page_no as usize).map_err(census)?
                && object.get() == 3
                && object_generation == one()
            {
                return Ok(Some(undo_request(
                    identity,
                    PageRef {
                        file: identity.page.file,
                        page_no,
                        incarnation: generation,
                    },
                )));
            }
        }
        Ok(None)
    }

    /// The slot0 Free generation of a resident candidate, under a brief
    /// one-page typed read.
    fn read_free_slot0(
        &self,
        owner: &mut Owner<'c, 's>,
        candidate: TargetRequest,
    ) -> Result<Option<Generation>> {
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[candidate])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let slot0 = read
            .bytes(0)
            .map(|bytes| free_slot0(bytes, candidate.identity))
            .map_err(|e| frame(RetentionPhase::Prepare, e));
        merge(
            slot0,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// The row's one `history::validate_live`, then, under the same replay
    /// checkout, the typed read of the Catalog leaf and T.
    fn observe_sources(
        &self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        catalog: TargetRequest,
        targets: &RowTargets,
        selected: (Generation, u8, RowTableSource),
        lanes: &mut PreflightLanes<'_>,
    ) -> Result<Observed> {
        let storage = self.storage;
        let mut audit = self.history.replay_scope(owner)?;
        let mut replay = audit.checkout()?;
        if let Err(error) =
            history::validate_live(storage, self.control, replay.bytes_mut(), prefix)
        {
            let rejected = merge(Err(error), replay.return_checked());
            return merge(rejected, audit.finish_checked());
        }
        let arena = replay.bytes();
        let observed = match targets.count {
            7 => self.observe_n::<8>(owner, prefix, catalog, targets, selected, arena, lanes),
            8 => self.observe_n::<9>(owner, prefix, catalog, targets, selected, arena, lanes),
            9 => self.observe_n::<10>(owner, prefix, catalog, targets, selected, arena, lanes),
            _ => Err(fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Invariant,
            )),
        };
        let observed = merge(observed, replay.return_checked());
        merge(observed, audit.finish_checked())
    }

    /// Under one typed read of the Catalog leaf and T (`N` = |T| + 1): copy
    /// the leaf into the schema range and T into the before range in before
    /// order, compare each T page with the replay arena, prove allocator
    /// membership, recheck the table record and the Free Undo, measure the
    /// RowLeaf and fold the commitment.
    #[allow(
        clippy::too_many_arguments,
        reason = "one const-N body over the borrowed sources and loans"
    )]
    fn observe_n<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        prefix: WalPrefix,
        catalog: TargetRequest,
        targets: &RowTargets,
        (slot0, root_job, table): (Generation, u8, RowTableSource),
        arena: &[u8],
        lanes: &mut PreflightLanes<'_>,
    ) -> Result<Observed> {
        let storage = self.storage;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let mut requests = [catalog; N];
        requests[1..].copy_from_slice(targets.before());
        requests.sort_unstable_by_key(|request| target_rank(*request));
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &requests)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let observed = (|| {
            let mut stamps = [None; 9];
            let mut catalog_stamp = None;
            let mut at_before = 0;
            for (at, request) in requests.iter().enumerate() {
                if *request == catalog {
                    catalog_stamp = Some(
                        read.copy_snapshot(at, lanes.schema)
                            .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                    );
                    continue;
                }
                if targets.before().get(at_before) != Some(request) {
                    return Err(invariant());
                }
                let output = &mut lanes.before[at_before];
                stamps[at_before] = Some(
                    read.copy_snapshot(at, output)
                        .map_err(|e| frame(RetentionPhase::Prepare, e))?,
                );
                let page_no =
                    usize::try_from(request.identity.page.page_no).map_err(|_| invariant())?;
                if output != history::arena_page(arena, page_no) {
                    return Err(invariant());
                }
                at_before += 1;
            }
            let catalog_stamp = catalog_stamp.ok_or_else(invariant)?;
            let stamps: [FrameStamp; 9] =
                std::array::from_fn(|at| stamps[at].unwrap_or(catalog_stamp));
            if at_before != targets.count {
                return Err(invariant());
            }
            let allocator = targets.at(RowSourceRole::Allocator).ok_or_else(invariant)?;
            self.check_membership(
                &lanes.before[allocator],
                targets.before()[allocator].identity,
                &requests,
            )?;
            let schema = self.check_table_record(lanes.schema, catalog.identity, table)?;
            let leaf_at = targets.at(RowSourceRole::RowLeaf).ok_or_else(invariant)?;
            let leaf_free = self.leaf_free(&lanes.before[leaf_at], targets.before()[leaf_at])?;
            let undo_at = targets.at(RowSourceRole::Undo).ok_or_else(invariant)?;
            let undo = FreeUndo {
                request: targets.before()[undo_at],
                slot0,
            };
            if free_slot0(&lanes.before[undo_at], undo.request.identity) != Some(slot0) {
                return Err(invariant());
            }
            let source = PreflightSource {
                prefix,
                targets: targets.before(),
                stamps: &stamps[..targets.count],
                roles: targets.roles,
                undo,
                root_job,
                catalog: (catalog_stamp, schema),
                leaf_free,
                pages: &lanes.before[..targets.count],
                control: ControlCut::capture(self.control),
            }
            .commitment();
            Ok(Observed { source, leaf_free })
        })();
        merge(
            observed,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// Every read source is the allocator's current Allocated entry, from the
    /// allocator before copy (`observe_targets`' membership rule).
    fn check_membership(
        &self,
        allocator: &[u8; PAGE_BYTES],
        identity: PageIdentity,
        requests: &[TargetRequest],
    ) -> Result<()> {
        let storage = self.storage;
        let census = |error| {
            fatal(
                storage,
                RetentionPhase::Census,
                RetentionCause::Allocation(error),
            )
        };
        let page = allocation_page::decode_allocation_page(
            allocator,
            identity,
            storage.profile().spec().namespace,
        )
        .map_err(census)?;
        for request in requests {
            let index = usize::try_from(request.identity.page.page_no)
                .map_err(|_| unchanged(RetentionPhase::Prepare, RetentionCause::Membership))?;
            if page.entry(index).map_err(census)?
                != (AllocationEntry::Allocated {
                    generation: request.identity.page.incarnation,
                    object: request.identity.owner,
                    object_generation: request.identity.owner_generation,
                    kind: request.kind,
                })
            {
                return Err(fatal(
                    storage,
                    RetentionPhase::Census,
                    RetentionCause::Membership,
                ));
            }
        }
        Ok(())
    }

    /// The table record in the Catalog leaf copy is the issued Table, live and
    /// at the issued anchor. Returns the schema cell digest.
    fn check_table_record(
        &self,
        schema: &[u8; PAGE_BYTES],
        identity: PageIdentity,
        table: RowTableSource,
    ) -> Result<[u8; 32]> {
        let storage = self.storage;
        let namespace = storage.profile().spec().namespace;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        let view = read::catalog_leaf_view(schema, identity, namespace).map_err(census)?;
        let position = view
            .lower_bound(&table.handle.object.get().to_be_bytes())
            .map_err(|e| census(RetentionCause::Tree(e)))?;
        if !position.equal {
            return Err(census(RetentionCause::Invariant));
        }
        let entry = view
            .entry(position.index)
            .map_err(|e| census(RetentionCause::Tree(e)))?;
        let EntryPayload::Value(value) = entry.payload else {
            return Err(census(RetentionCause::Invariant));
        };
        let record = read::catalog_leaf_record(entry, namespace).map_err(census)?;
        let header = record.header();
        if header.identity != table.handle
            || header.retired
            || header.anchor != table.anchor.page
            || !matches!(record.definition(), CatalogDefinitionView::Table(_))
        {
            return Err(census(RetentionCause::Invariant));
        }
        Ok(content_digest(value))
    }

    /// The RowLeaf's free entry bytes, by A's leaf rule (each Value costs its
    /// length plus 16). A non-Value entry is a leaf A does not write.
    fn leaf_free(&self, bytes: &[u8; PAGE_BYTES], leaf: TargetRequest) -> Result<u16> {
        let storage = self.storage;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        let view = tree_page::decode_tree_page(
            bytes,
            TreeExpectation {
                identity: leaf.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )
        .map_err(|e| census(RetentionCause::Tree(e)))?;
        let mut free = TREE_ENTRY_CAPACITY;
        for index in 0..view.len() {
            let entry = view
                .entry(index)
                .map_err(|e| census(RetentionCause::Tree(e)))?;
            let EntryPayload::Value(value) = entry.payload else {
                return Err(unchanged(
                    RetentionPhase::Prepare,
                    RetentionCause::Unsupported,
                ));
            };
            free = free
                .checked_sub(value.len() + 16)
                .ok_or_else(|| census(RetentionCause::Invariant))?;
        }
        u16::try_from(free).map_err(|_| census(RetentionCause::Invariant))
    }
}

/// Source-bound inventory of the actual preflight, in the repository's
/// size_of sense (not a stack measurement). A call's own values are held
/// throughout it and added; the calls it makes one after another are
/// max-combined. The phase's receiver, argument, cut, wrapper and step
/// carriers are the Row write census's `carriers` (`row_write_phases`), not
/// repeated here.
#[derive(Clone, Copy, Debug)]
pub(in super::super::super) struct RowPreflightBody {
    /// `preflight_row`'s own values: the admission, the gate and their
    /// carriers and unwind fences (the S2-a′ `shell`), then the body's
    /// Result, the table scalars and the release carriers.
    pub(in super::super::super) shell: usize,
    /// `row_preflight`'s own values: the preparation view, prefix, actor,
    /// head and Terminal, lanes, the residency cursor, the source list, the
    /// Free Undo scan state and each step's Result. Every LOAD holds them.
    pub(in super::super::super) frame: usize,
    /// `check_catalog_pair_io` before the gate: the S2-a′ lease check, a
    /// completed call (d2).
    pub(in super::super::super) lease_check: usize,
    /// The entry, actor head, Terminal and view checks (the callee part of
    /// the S2-a′ `check`) and the table slot's publication observation.
    pub(in super::super::super) check: usize,
    /// One `make_resident`: its source, step Results and the retention LOAD
    /// body. The protected list is the source list itself, never copied.
    pub(in super::super::super) load: usize,
    /// `queue_row_sources`' own values beside `row_shape` (the Root/anchor
    /// typed read, `reserve_job` and the anchor decode), the Jobs requests
    /// and the queue steps.
    pub(in super::super::super) select: usize,
    /// `row_shape` alone.
    pub(in super::super::super) shape: usize,
    /// The Free Undo steps other than its LOAD: the allocator copy, the Job
    /// scan, the candidate scan of the allocator copy and the slot0 read.
    pub(in super::super::super) undo_scan: usize,
    /// `row_observe_cut`'s own values: T, the observation, the prefix
    /// recheck and the cut's Result.
    pub(in super::super::super) finish_frame: usize,
    /// `observe_sources`: the replay scope and loan, then `validate_live` or
    /// the ten-target observation with its membership, table record, leaf,
    /// slot0 and commitment calls.
    pub(in super::super::super) observe: usize,
    /// `check_catalog_read` and `record_table_read` (the callee part of the
    /// S2-a′ `register`).
    pub(in super::super::super) register: usize,
    /// `finish_frame + max(T, observe, register)`.
    pub(in super::super::super) finish: usize,
    /// `shell + frame + max(lease_check, check, load, select, undo_scan,
    /// finish)`.
    pub(in super::super::super) body: usize,
}

/// The actual preflight body census. The S2-a′ terms it takes are given
/// without their estimated body scalars (`check`, `register`), since this
/// census counts the body's own values in its frames.
pub(in super::super) fn row_preflight_body<W: WriterNamespace>(
    shell: usize,
    lease_check: usize,
    check: usize,
    register: usize,
    scope_carriers: usize,
    history_peak: usize,
) -> RowPreflightBody {
    use crate::storage::{
        allocation_page::{AllocationError, AllocationPage},
        page::PageError,
    };
    use std::mem::size_of;
    type Read<'a, const N: usize> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, N>;
    type Allocated = std::result::Result<AllocationEntry, AllocationError>;
    type Allocation<'a> = std::result::Result<AllocationPage<'a>, AllocationError>;
    let reference = size_of::<&()>();
    let shell = shell
        + 2 * size_of::<Result<RowPreflightOutcome>>()
        + size_of::<RowTableSource>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + 2 * reference;
    let frame = size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + size_of::<WalPrefix>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + size_of::<TxnRef>()
        + 2 * size_of::<Result<TxnRef>>()
        + size_of::<Result<Option<UndoRef>>>()
        + size_of::<Result<(usize, StatusSlotRef)>>()
        + size_of::<RowTableSource>()
        + size_of::<PreflightLanes<'_>>()
        + size_of::<Plan4ResidencyCursor>()
        + size_of::<RowSources>()
        + 2 * size_of::<Result<Plan4Target>>()
        + size_of::<(Option<UndoRef>, usize, TargetRequest)>()
        + 2 * size_of::<Result<Option<Plan4Target>>>()
        + 2 * size_of::<Result<u8>>()
        // The allocator's request step, the leased pages, the scan position,
        // the candidate and its slot0.
        + 2 * size_of::<dirty_frame::Result<TargetRequest>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<u64>()
        + 2 * size_of::<Result<u64>>()
        + size_of::<TargetRequest>()
        + 2 * size_of::<Result<Option<TargetRequest>>>()
        + size_of::<Generation>()
        + 2 * size_of::<Result<Option<Generation>>>()
        + size_of::<(Generation, u8, RowTableSource)>()
        + 2 * size_of::<Result<RowPreflightCut>>()
        + 4 * size_of::<RetentionError>()
        // Receiver, four arguments, storage and preparation.
        + 8 * reference;
    let check = check
        + 2 * size_of::<visibility::VisibilityResult<TableSlot>>()
        + size_of::<RetentionError>()
        + 4 * reference;
    let load = size_of::<TargetRequest>()
        + 2 * size_of::<Result<RetentionResidencyStep>>()
        + size_of::<Result<Option<Plan4Target>>>()
        + 6 * reference
        + super::super::super::lookup_load::source_bytes::<W>();
    // One page decode as the Undo decode term counts it, for the anchor
    // decode, beside the decoded anchor values.
    let anchor_decode = 2 * size_of::<anchor::TreeAnchor>()
        + 2 * size_of::<anchor::AnchorRoot>()
        + 2 * size_of::<std::result::Result<anchor::TreeAnchor, PageError>>()
        + super::super::super::read::undo_decode_source_bytes();
    let shape = size_of::<[TargetRequest; 2]>()
        + 2 * size_of::<usize>()
        + targets::call_bytes::<2>().max(
            size_of::<Read<'_, 2>>()
                + 2 * size_of::<Result<RowShape>>()
                + 2 * size_of::<dirty_frame::Result<&[u8; PAGE_BYTES]>>()
                + 4 * reference
                + row_group::reserve_job_source_bytes().max(anchor_decode),
        );
    let copy_allocator = targets::call_bytes::<1>().max(
        size_of::<Read<'_, 1>>()
            + 2 * size_of::<dirty_frame::Result<FrameStamp>>()
            + 2 * size_of::<Result<()>>(),
    );
    let leased_undo = size_of::<u64>()
        + 2 * size_of::<job3::JobView<'_>>()
        + 2 * size_of::<
            std::result::Result<job3::JobView<'_>, crate::storage::retention_page::RetentionError>,
        >()
        + 4 * reference
        + job3::source_bytes();
    let candidate_scan = size_of::<AllocationPage<'_>>()
        + 2 * size_of::<Allocation<'_>>()
        + size_of::<AllocationEntry>()
        + 2 * size_of::<Allocated>()
        + size_of::<TargetRequest>()
        + 6 * reference;
    let slot0 = targets::call_bytes::<1>().max(
        size_of::<Read<'_, 1>>()
            + 2 * size_of::<dirty_frame::Result<Option<Generation>>>()
            + 2 * size_of::<Result<Option<Generation>>>()
            + 2 * size_of::<Option<Generation>>()
            + super::super::super::read::undo_decode_source_bytes(),
    );
    let undo_scan = copy_allocator
        .max(leased_undo)
        .max(candidate_scan)
        .max(slot0);
    // RowSources::queue: the position, the shift and the slice scans.
    let queue = 3 * size_of::<usize>()
        + 2 * size_of::<std::slice::Iter<'_, TargetRequest>>()
        + size_of::<RetentionError>()
        + 4 * reference;
    let select = size_of::<TargetRequest>()
        + 2 * size_of::<Result<Plan4Target>>()
        + size_of::<RowShape>()
        + 2 * size_of::<Result<RowShape>>()
        + size_of::<u8>()
        + 2 * size_of::<Result<TargetRequest>>()
        // The head's Undo.
        + size_of::<TargetRequest>()
        + size_of::<(Option<UndoRef>, usize, TargetRequest)>()
        + size_of::<RowTableSource>()
        + 2 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 6 * reference
        + shape.max(queue).max(
            // jobs_request: the index and the source's Plan4Target.
            size_of::<u8>() + 2 * size_of::<Result<Plan4Target>>() + 2 * reference,
        );
    let finish_frame = size_of::<RowTargets>()
        + size_of::<Observed>()
        + 2 * size_of::<Result<Observed>>()
        + size_of::<WalPrefix>()
        + 2 * size_of::<std::result::Result<WalPrefix, crate::storage::wal_io::WalIoError>>()
        + size_of::<(Generation, u8, RowTableSource)>()
        + 4 * size_of::<RetentionError>()
        + 8 * reference;
    // RowTargets::new: the allocator, the rank-sorted list, its count and
    // roles, the position closure's captures and the sort.
    let row_targets = size_of::<TargetRequest>()
        + size_of::<[TargetRequest; 9]>()
        + 4 * size_of::<usize>()
        + 3 * size_of::<u64>()
        + size_of::<std::slice::Iter<'_, TargetRequest>>()
        + 6 * reference;
    // The observation under the typed read of the Catalog leaf and T.
    let observe_frame = size_of::<[TargetRequest; 10]>()
        + size_of::<[Option<FrameStamp>; 9]>()
        + size_of::<[FrameStamp; 9]>()
        + 2 * size_of::<Option<FrameStamp>>()
        + 6 * size_of::<usize>()
        + size_of::<[u8; 32]>()
        + size_of::<FreeUndo>()
        + size_of::<PreflightSource<'_>>()
        + size_of::<ControlCut>()
        + 2 * size_of::<Result<Observed>>()
        + 2 * size_of::<dirty_frame::Result<FrameStamp>>()
        + 8 * reference;
    let observe_calls = (size_of::<AllocationPage<'_>>()
        + 2 * size_of::<Allocation<'_>>()
        + 2 * size_of::<Allocated>())
    .max(super::super::super::read::catalog_source_bytes::<W>() + 2 * size_of::<Result<[u8; 32]>>())
    .max(row_group::tree_source_bytes() + 2 * size_of::<Result<u16>>())
    .max(super::super::super::read::undo_decode_source_bytes())
    // ControlCut::capture and the commitment: hashers, the pinned SHA block
    // closure and the digests.
    .max(2 * size_of::<ContentHasher>() + 1536 + 24 + 4 * size_of::<[u8; 32]>());
    let observe = scope_carriers
        + size_of::<WalPrefix>()
        + size_of::<(Generation, u8, RowTableSource)>()
        + 2 * size_of::<Result<Observed>>()
        + 6 * reference
        + (history_peak + 2 * size_of::<Result<()>>()).max(
            targets::call_bytes::<10>()
                .max(size_of::<Read<'_, 10>>() + observe_frame + observe_calls),
        );
    let finish = finish_frame + row_targets.max(observe).max(register);
    let body = shell
        + frame
        + lease_check
            .max(check)
            .max(load)
            .max(select)
            .max(undo_scan)
            .max(finish);
    RowPreflightBody {
        shell,
        frame,
        lease_check,
        check,
        load,
        select,
        shape,
        undo_scan,
        finish_frame,
        observe,
        register,
        finish,
        body,
    }
}

#[cfg(test)]
impl<W: WriterNamespace> RetentionWriter<'_, '_, '_, '_, '_, '_, '_, '_, '_, W> {
    /// S2W-R22: (replay scopes opened on this thread, replay checkouts of the
    /// original history so far). Scalars only.
    pub(in crate::storage) fn row_test_replay_counts(&self) -> (u64, u64) {
        (
            history::runtime::test_replay_scopes(),
            self.history.test_replay_checkouts(),
        )
    }
}
