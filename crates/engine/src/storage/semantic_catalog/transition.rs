//! The closed CREATE state machine. Both live production and recovery compute
//! canonical successors from the original stable before images. The result is
//! a byte observation; original owner admission, retirement and installation
//! capabilities are deliberately outside this pure calculation.
use super::{
    Error, Operation, Result,
    core::{self, CoreInput},
    frame::{self, FrameSpec, PageChange},
    images::{self, CatalogChange, RootContext, StatusChange, StatusContext},
    inventory, ledger,
    queue::{self, QueueChange, ReservePlan},
    u64_at,
};
use crate::storage::{
    allocation_page::{self, AllocationEntry},
    catalog_record::CatalogTableInput,
    control::Profile,
    obligation,
    page::{FileId, PAGE_BYTES, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
    retention_obligation,
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        catalog::{self, JobState, JobView},
    },
    status_home::{self, StatusHome},
    transaction_record::{StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef},
    tree_page::{self, TreeExpectation},
    wal_frame::{FrameExpectation, WalFrameHeader},
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation, ObjectId},
};

const ALLOCATOR: usize = 0;
const ROOT: usize = 1;
const JOB: usize = 2;
const CATALOG: usize = 3;
const STATUS: usize = 4;
const UNDO: usize = 5;
const ANCHOR: usize = 6;
const ROW: usize = 7;

#[derive(Clone, Copy)]
pub(crate) enum Change<'a> {
    AgingStart,
    Begin {
        actor: TxnRef,
        status: StatusSlotRef,
    },
    Reserve(&'a ReservePlan),
    Forward {
        ordinal: usize,
        definition: CatalogTableInput<'a>,
    },
    Cancel {
        ordinal: usize,
    },
    RollbackStart {
        terminal: usize,
    },
    Clr {
        ordinal: usize,
    },
    Commit {
        terminal: usize,
    },
    AbortEnd {
        terminal: usize,
    },
    Prune {
        ordinal: usize,
    },
    ReleaseRole {
        ordinal: usize,
    },
    RootFree {
        ordinal: usize,
    },
    TerminalFinish {
        ordinal: usize,
    },
}
impl Change<'_> {
    pub(crate) fn operation(self) -> Operation {
        match self {
            Self::AgingStart => Operation::AgingStart,
            Self::Begin { .. } => Operation::Begin,
            Self::Reserve(_) => Operation::Reserve,
            Self::Forward { .. } => Operation::Forward,
            Self::Cancel { .. } => Operation::Cancel,
            Self::RollbackStart { .. } => Operation::RollbackStart,
            Self::Clr { .. } => Operation::Clr,
            Self::Commit { .. } => Operation::Commit,
            Self::AbortEnd { .. } => Operation::AbortEnd,
            Self::Prune { .. } => Operation::Prune,
            Self::ReleaseRole { .. } => Operation::ReleaseRole,
            Self::RootFree { .. } => Operation::RootFree,
            Self::TerminalFinish { .. } => Operation::TerminalFinish,
        }
    }
    fn selected(self) -> Option<usize> {
        match self {
            Self::AgingStart | Self::Begin { .. } | Self::Reserve(_) => None,
            Self::Forward { ordinal, .. }
            | Self::Cancel { ordinal }
            | Self::Clr { ordinal }
            | Self::Prune { ordinal }
            | Self::ReleaseRole { ordinal }
            | Self::RootFree { ordinal }
            | Self::TerminalFinish { ordinal } => Some(ordinal),
            Self::RollbackStart { terminal }
            | Self::Commit { terminal }
            | Self::AbortEnd { terminal } => Some(terminal),
        }
    }
}

/// All borrowed before images belong to one original stabilized version. A
/// caller cannot turn this structure, raw PageRefs or an Observation into a
/// physical/WAL capability. No callback can acquire I/O from this calculation.
pub(crate) struct Inputs<'a> {
    pub(crate) profile: Profile,
    pub(crate) frame: FrameExpectation,
    pub(crate) change: Change<'a>,
    pub(crate) jobs: &'a [[u8; JOB_BYTES]; JOB_COUNT],
    pub(crate) root: &'a [u8; PAGE_BYTES],
    pub(crate) jobs_page: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) allocator: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) catalog_anchor: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) catalog: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) status: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) undo: Option<&'a [u8; PAGE_BYTES]>,
    pub(crate) transaction: Option<&'a [u8; 1536]>,
    pub(crate) aging: Option<&'a [u8; 1536]>,
}

#[derive(Clone, Copy)]
pub(crate) struct Observation {
    pub(crate) operation: Operation,
    pub(crate) header: WalFrameHeader,
    /// Role indexes in exact physical redo order, followed by unused zeros.
    pub(crate) order: [u8; 8],
    pub(crate) count: u8,
    pub(crate) frame_bytes: usize,
    pub(crate) job_ordinal: Option<usize>,
    pub(crate) actor: Option<TxnRef>,
    pub(crate) status: Option<StatusSlotRef>,
}

fn required<T>(value: Option<T>) -> Result<T> {
    value.ok_or(Error::InvalidInput)
}
fn one() -> Generation {
    Generation::new(1).expect("one")
}
fn fixed(profile: Profile, page_no: u64, owner: u64, kind: PageKind, lsn: u64) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: profile.spec().namespace.database,
            page: PageRef {
                file: FileId::new(0x0000_0001_0000_0001).expect("first file"),
                page_no,
                incarnation: one(),
            },
            owner: ObjectId::new(owner).expect("fixed object"),
            owner_generation: one(),
        },
        kind,
        level: 0,
        lsn: PageLsn(lsn),
    }
}
fn source_job<'a>(inputs: &Inputs<'a>, ordinal: usize) -> Result<JobView<'a>> {
    if ordinal >= JOB_COUNT {
        return Err(Error::InvalidInput);
    }
    Ok(catalog::decode_job(
        &inputs.jobs[ordinal],
        inputs.profile.spec().namespace,
    )?)
}
fn root_for<'a>(inputs: &Inputs<'a>, actor: TxnRef) -> Result<Option<JobView<'a>>> {
    let mut found = None;
    for ordinal in 0..JOB_COUNT {
        let job = source_job(inputs, ordinal)?;
        if job.state().is_root() && job.source() == Some(actor) {
            if found.is_some() {
                return Err(Error::IdentityMismatch);
            }
            found = Some(job);
        }
    }
    Ok(found)
}
fn status_before(inputs: &Inputs<'_>, location: StatusSlotRef) -> Result<StatusHome> {
    let header = fixed(
        inputs.profile,
        location.page().page_no,
        2,
        PageKind::TransactionStatus,
        0,
    );
    let home = status_home::decode_status_home_page(
        required(inputs.status)?,
        PageIdentity {
            page: location.page(),
            ..header.identity
        },
        inputs.profile.spec().namespace,
    )?;
    if home.len() != 1
        || home.lsn() == 0
        || home.lsn() > inputs.frame.previous_lsn
        || location.slot() != 0
    {
        return Err(Error::OrderMismatch);
    }
    Ok(home.cell(0)?)
}

fn queue_change<'a>(change: Change<'a>, sender: &'a [u8; 32], q: u64) -> Result<QueueChange<'a>> {
    Ok(match change {
        Change::AgingStart => QueueChange::AgingStart,
        Change::Begin { actor, status } => QueueChange::Begin { actor, status },
        Change::Reserve(plan) => QueueChange::Reserve(plan),
        Change::Forward { ordinal, .. } => QueueChange::Forward { ordinal, sender },
        Change::Cancel { ordinal } => QueueChange::Cancel { ordinal },
        Change::Clr { ordinal } => QueueChange::Clr { ordinal },
        Change::Commit { terminal } => QueueChange::Commit {
            ordinal: terminal,
            sender,
        },
        Change::AbortEnd { terminal } => QueueChange::AbortEnd {
            ordinal: terminal,
            sender,
        },
        Change::Prune { ordinal } => QueueChange::Prune { ordinal, q },
        Change::ReleaseRole { ordinal } => QueueChange::ReleaseRole { ordinal },
        Change::RootFree { ordinal } => QueueChange::RootFree { ordinal },
        Change::TerminalFinish { ordinal } => QueueChange::TerminalFinish { ordinal },
        Change::RollbackStart { .. } => return Err(Error::InvalidInput),
    })
}

fn write_job_page(
    inputs: &Inputs<'_>,
    ordinal: usize,
    lsn: u64,
    job: &[u8; JOB_BYTES],
    output: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let index = ordinal / 15;
    let slot = ordinal % 15;
    let page = catalog::decode_jobs_page(
        required(inputs.jobs_page)?,
        inputs.profile.spec().namespace,
        index as u8,
    )?;
    if page.header().lsn.0 > inputs.frame.previous_lsn {
        return Err(Error::OrderMismatch);
    }
    let mut cells = [&[][..]; 15];
    for (current, cell) in cells.iter_mut().enumerate().take(page.len()) {
        if page.cell(current) != Some(&inputs.jobs[index * 15 + current][..]) {
            return Err(Error::IdentityMismatch);
        }
        *cell = if current == slot {
            job
        } else {
            page.cell(current).ok_or(Error::Corrupt)?
        };
    }
    catalog::encode_jobs_page(
        inputs.profile.spec().namespace,
        index as u8,
        lsn,
        &cells[..page.len()],
        output,
    )?;
    Ok(())
}

fn check_reservation(inputs: &Inputs<'_>, job: JobView<'_>) -> Result<()> {
    let identity = fixed(inputs.profile, 0, 1, PageKind::Allocation, 0).identity;
    let allocation = allocation_page::decode_allocation_page(
        required(inputs.allocator)?,
        identity,
        inputs.profile.spec().namespace,
    )?;
    if allocation.header().lsn.0 > inputs.frame.previous_lsn {
        return Err(Error::OrderMismatch);
    }
    let pair = required(job.pair())?;
    let mut selected = 0;
    for index in 9..64 {
        if let AllocationEntry::Free(generation) = allocation.entry(index)? {
            let candidate = PageRef {
                page_no: index as u64,
                incarnation: generation.next().map_err(|_| Error::Exhausted)?,
                ..identity.page
            };
            if pair[selected] != candidate {
                return Err(Error::IdentityMismatch);
            }
            selected += 1;
            if selected == 2 {
                break;
            }
        }
    }
    if selected != 2 {
        return Err(Error::Exhausted);
    }
    for (reference, object, kind) in [
        (
            required(job.status())?.page(),
            2,
            PageKind::TransactionStatus,
        ),
        (required(job.undo())?.page(), 3, PageKind::Undo),
    ] {
        if allocation.entry(reference.page_no as usize)?
            != (AllocationEntry::Allocated {
                generation: reference.incarnation,
                object: ObjectId::new(object).expect("fixed"),
                object_generation: one(),
                kind,
            })
        {
            return Err(Error::IdentityMismatch);
        }
    }
    Ok(())
}

/// Return only the scalar inventory cut. Predecessor descriptor decoding and
/// its return carriers finish before the next descriptor is constructed.
fn transaction_inventory(
    inputs: &Inputs<'_>,
    op: Operation,
    actor: TxnRef,
    root_before: Option<JobView<'_>>,
    updated_job: Option<JobView<'_>>,
    lsn: u64,
) -> Result<obligation::InventoryClaim> {
    let prior = inputs
        .transaction
        .map(|raw| {
            obligation::decode_descriptor(
                raw,
                allocation_page::ReservationOwner::Transaction(actor),
                retention_obligation::profile_reference(inputs.profile),
            )
        })
        .transpose()?;
    let inventory = match op {
        Operation::Reserve => {
            let mut items = [0; inventory::ITEMS_BYTES];
            inventory::items(required(updated_job)?, &mut items)?
        }
        Operation::Forward | Operation::Cancel => {
            let mut items = [0; inventory::ITEMS_BYTES];
            if inventory::items(required(root_before)?, &mut items)? != required(prior)?.inventory {
                return Err(Error::BudgetMismatch);
            }
            inventory::empty(lsn)?
        }
        Operation::Begin => inventory::empty(lsn)?,
        _ => required(prior)?.inventory,
    };
    Ok(inventory)
}

/// Output roles are fixed so the caller can retain only one eight-page loan.
/// Ordering for the WAL/pool is computed at the end from actual identities.
fn compute<'a>(
    inputs: &Inputs<'a>,
    after: &mut [[u8; PAGE_BYTES]; 8],
    descriptors: &mut [[u8; 1536]; 2],
    core: &mut [u8; 256],
) -> Result<(Observation, [bool; 8], Option<JobView<'a>>, usize)> {
    let op = inputs.change.operation();
    let namespace = inputs.profile.spec().namespace;
    if inputs.profile.format() != 2
        || namespace.database != inputs.frame.stream.database
        || namespace.timeline != inputs.frame.stream.timeline
    {
        return Err(Error::IdentityMismatch);
    }
    let lsn = inputs
        .frame
        .previous_lsn
        .checked_add(1)
        .ok_or(Error::Exhausted)?;
    let (_, root) = catalog::decode_root_page(inputs.root, namespace)?;
    if root.apply() > inputs.frame.previous_lsn {
        return Err(Error::OrderMismatch);
    }
    let selected = inputs
        .change
        .selected()
        .map(|ordinal| source_job(inputs, ordinal))
        .transpose()?;
    let (actor, location) = match inputs.change {
        Change::AgingStart => (None, None),
        Change::Begin { actor, status } => (Some(actor), Some(status)),
        Change::Reserve(plan) => (Some(plan.actor), Some(plan.status)),
        _ => (required(selected)?.source(), required(selected)?.status()),
    };
    let root_before = actor
        .map(|actor| root_for(inputs, actor))
        .transpose()?
        .flatten();
    let is_tx = op.handoff() || op == Operation::RollbackStart;
    if matches!(
        inputs.change,
        Change::RollbackStart { .. } | Change::Commit { .. } | Change::AbortEnd { .. }
    ) && required(selected)?.state() != JobState::TerminalLeased
    {
        return Err(Error::OrderMismatch);
    }
    let old_status = if is_tx || matches!(op, Operation::Prune | Operation::TerminalFinish) {
        Some(status_before(inputs, required(location)?)?)
    } else {
        None
    };
    if is_tx && op != Operation::Begin {
        ledger::validate_transaction(
            required(inputs.transaction)?,
            inputs.profile,
            required(old_status)?,
            root_before,
        )?;
        if !matches!(required(old_status)?.core(), TransactionStatusCell::Occupied(status) if Some(status.reference()) == actor)
        {
            return Err(Error::IdentityMismatch);
        }
    } else if op == Operation::Begin && inputs.transaction.is_some() {
        return Err(Error::OrderMismatch);
    }
    if op == Operation::TerminalFinish {
        let terminal = required(selected)?;
        let TransactionStatusCell::Occupied(status) = required(old_status)?.core() else {
            return Err(Error::OrderMismatch);
        };
        let exact_outcome = match status.state() {
            TransactionState::Committed(q) => {
                terminal.outcome() == 1 && terminal.commit_q() == q.get()
            }
            TransactionState::Aborted => terminal.outcome() == 2 && terminal.commit_q() == 0,
            _ => false,
        };
        if terminal.state() != JobState::TerminalReady
            || Some(status.reference()) != actor
            || !exact_outcome
        {
            return Err(Error::IdentityMismatch);
        }
    }
    let q = match op {
        Operation::Commit => root.commit_c().checked_add(1).ok_or(Error::Exhausted)?,
        Operation::Prune => match required(old_status)?.core() {
            TransactionStatusCell::Occupied(status) if Some(status.reference()) == actor => {
                match status.state() {
                    TransactionState::Committed(q) => q.get(),
                    _ => return Err(Error::OrderMismatch),
                }
            }
            _ => return Err(Error::IdentityMismatch),
        },
        _ => 0,
    };
    let sender = inputs
        .transaction
        .map_or([0; 32], |before| content_digest(before));
    let mut root_cell = [0; 512];
    let mut job_cell = [0; JOB_BYTES];
    let mut aggregate = [0; 640];
    let mut changed = [false; 8];
    let job_ordinal = if op == Operation::RollbackStart {
        catalog::verify_census(root, namespace, inputs.jobs, &mut aggregate)?;
        if root.aging().is_none()
            || root_before.is_some_and(|job| job.state() != JobState::RootRetained)
        {
            return Err(Error::OrderMismatch);
        }
        None
    } else {
        let ordinal = queue::prepare(
            queue_change(inputs.change, &sender, q)?,
            namespace,
            root,
            inputs.jobs,
            lsn,
            &mut root_cell,
            &mut job_cell,
            &mut aggregate,
        )?;
        catalog::encode_root_page(&root_cell, namespace, &mut after[ROOT])?;
        changed[ROOT] = true;
        if let Some(ordinal) = ordinal {
            write_job_page(inputs, ordinal, lsn, &job_cell, &mut after[JOB])?;
            changed[JOB] = true;
        }
        ordinal
    };
    let updated_job = job_ordinal
        .map(|_| catalog::decode_job(&job_cell, namespace))
        .transpose()?;
    let semantic_root = if op == Operation::Reserve {
        updated_job
    } else {
        root_before
    };
    if op == Operation::Reserve {
        check_reservation(inputs, required(semantic_root)?)?;
    }
    let ctx = semantic_root.map(|job| RootContext {
        namespace,
        job,
        lsn: PageLsn(lsn),
    });
    if op == Operation::RollbackStart
        && let Some(context) = ctx
    {
        images::validate_retained_undo(context, required(inputs.undo)?)?;
    }
    let mut projection = None;
    let mut catalog_bytes = 0;
    if matches!(
        op,
        Operation::Reserve | Operation::Forward | Operation::Cancel | Operation::Clr
    ) {
        images::change_pair(
            op,
            required(ctx)?,
            required(inputs.allocator)?,
            &mut after[ALLOCATOR],
        )?;
        changed[ALLOCATOR] = true;
    }
    if matches!(
        op,
        Operation::Reserve
            | Operation::Forward
            | Operation::Clr
            | Operation::Prune
            | Operation::ReleaseRole
            | Operation::RootFree
    ) {
        images::change_undo(op, required(ctx)?, required(inputs.undo)?, &mut after[UNDO])?;
        changed[UNDO] = true;
    }
    if matches!(op, Operation::Forward | Operation::Clr | Operation::Prune) {
        let change = match inputs.change {
            Change::Forward { definition, .. } => CatalogChange::Forward(definition),
            Change::Clr { .. } => CatalogChange::Clr,
            Change::Prune { .. } => {
                CatalogChange::Prune(CommitSeq::new(q).map_err(|_| Error::IdentityMismatch)?)
            }
            _ => return Err(Error::InvalidInput),
        };
        let result = images::change_catalog(
            change,
            required(ctx)?,
            required(inputs.catalog_anchor)?,
            required(inputs.catalog)?,
            &mut after[CATALOG],
        )?;
        projection = Some((result.before_projection, result.after_projection));
        catalog_bytes = result.cell_bytes;
        changed[CATALOG] = true;
    }
    if op == Operation::Forward {
        let (left, right) = after.split_at_mut(ROW);
        images::initialize_pair(required(ctx)?, &mut left[ANCHOR], &mut right[0])?;
        changed[ANCHOR] = true;
        changed[ROW] = true;
    }
    let status_change = match op {
        Operation::Begin => Some(StatusChange::Begin),
        Operation::Forward => Some(StatusChange::Forward(required(
            required(semantic_root)?.undo(),
        )?)),
        Operation::RollbackStart => Some(StatusChange::RollbackStart),
        Operation::Clr => Some(StatusChange::Clr(required(
            required(semantic_root)?.undo(),
        )?)),
        Operation::Commit => Some(StatusChange::Commit {
            undo: required(required(root_before)?.undo())?,
            q: CommitSeq::new(q).map_err(|_| Error::Exhausted)?,
        }),
        Operation::AbortEnd => Some(StatusChange::AbortEnd),
        Operation::Prune => Some(StatusChange::Prune {
            undo: required(required(semantic_root)?.undo())?,
            q: CommitSeq::new(q).map_err(|_| Error::IdentityMismatch)?,
        }),
        Operation::TerminalFinish => Some(StatusChange::TerminalFinish),
        _ => None,
    };
    let new_status = if let Some(change) = status_change {
        changed[STATUS] = true;
        Some(images::change_status(
            change,
            StatusContext {
                namespace,
                actor: required(actor)?,
                location: required(location)?,
                lsn: PageLsn(lsn),
            },
            required(inputs.status)?,
            &mut after[STATUS],
        )?)
    } else {
        old_status
    };
    let root_after = match op {
        Operation::Reserve | Operation::Forward | Operation::Clr => updated_job,
        Operation::Cancel => None,
        _ => root_before,
    };
    if is_tx {
        let actor = required(actor)?;
        let inventory = transaction_inventory(inputs, op, actor, root_before, updated_job, lsn)?;
        ledger::next_transaction(
            ledger::TransactionTransition {
                operation: op,
                profile: inputs.profile,
                lsn,
                before: inputs.transaction,
                status: required(new_status)?,
                root: root_after,
                inventory,
                catalog_bytes,
            },
            &mut descriptors[0],
        )?;
    }
    if op != Operation::RollbackStart {
        ledger::next_aging(
            inputs.profile,
            inputs.aging,
            inputs.root,
            &after[ROOT],
            &aggregate,
            &mut descriptors[usize::from(op.handoff())],
        )?;
    } else {
        retention_obligation::catalog::decode(
            required(inputs.aging)?,
            inputs.profile,
            inputs.root,
        )?;
    }
    let cursors = if matches!(
        op,
        Operation::Prune | Operation::ReleaseRole | Operation::RootFree | Operation::TerminalFinish
    ) {
        (root.cursor(), u64_at(&root_cell, 72))
    } else {
        (0, 0)
    };
    let core_root = if matches!(
        op,
        Operation::Reserve
            | Operation::Forward
            | Operation::Cancel
            | Operation::Clr
            | Operation::RollbackStart
            | Operation::Prune
            | Operation::ReleaseRole
            | Operation::RootFree
    ) {
        semantic_root
    } else {
        None
    };
    core::encode_core(
        CoreInput {
            operation: op,
            root: core_root,
            q: if op == Operation::Commit { q } else { 0 },
            cursors,
            projection,
        },
        core,
    )?;
    let header = WalFrameHeader {
        stream: inputs.frame.stream,
        kind: op.outer(),
        lsn,
        previous_lsn: inputs.frame.previous_lsn,
        txn: if is_tx {
            Some(required(actor)?.id())
        } else {
            None
        },
        previous_txn_lsn: if is_tx {
            inputs.transaction.map_or(0, |bytes| u64_at(bytes, 72))
        } else {
            0
        },
    };
    // Only borrowed original Jobs escape. Reserve's newly encoded Job is
    // recovered from the caller-owned after page by the serialization layer.
    Ok((
        Observation {
            operation: op,
            header,
            order: [0; 8],
            count: 0,
            frame_bytes: op.frame_bytes(catalog_bytes)?,
            job_ordinal,
            actor,
            status: location,
        },
        changed,
        root_before,
        catalog_bytes,
    ))
}

// Framing is completed below after all temporary queue/status/ledger decodes
// have left scope; this keeps their wide return carriers off the native phase.

fn catalog_slot(
    image: &[u8; PAGE_BYTES],
    header: PageHeader,
    object: ObjectId,
) -> Result<Option<u16>> {
    let view = tree_page::decode_tree_page(
        image,
        TreeExpectation {
            identity: header.identity,
            kind: PageKind::CatalogLeaf,
            level: 0,
            arity: 0,
        },
    )?;
    let found = view.lower_bound(&object.get().to_be_bytes())?;
    Ok(found.equal.then_some((found.index + 1) as u16))
}

fn page_change<'a>(
    inputs: &Inputs<'a>,
    observation: Observation,
    root: Option<JobView<'a>>,
    role: usize,
    image: &'a [u8; PAGE_BYTES],
) -> Result<PageChange<'a>> {
    let profile = inputs.profile;
    let lsn = observation.header.lsn;
    let (header, before, before_slot, after_slot) = match role {
        ALLOCATOR => (
            fixed(profile, 0, 1, PageKind::Allocation, lsn),
            inputs.allocator,
            Some(0),
            Some(0),
        ),
        ROOT => (
            fixed(profile, 1, 4, PageKind::RetentionRoot, lsn),
            Some(inputs.root),
            Some(0),
            Some(0),
        ),
        JOB => {
            let ordinal = required(observation.job_ordinal)?;
            (
                fixed(
                    profile,
                    2 + (ordinal / 15) as u64,
                    4,
                    PageKind::RetentionJobs,
                    lsn,
                ),
                inputs.jobs_page,
                Some((ordinal % 15) as u16),
                Some((ordinal % 15) as u16),
            )
        }
        CATALOG => {
            let header = fixed(profile, 8, 5, PageKind::CatalogLeaf, lsn);
            let object = required(required(root)?.object())?.object;
            (
                header,
                inputs.catalog,
                catalog_slot(required(inputs.catalog)?, header, object)?,
                catalog_slot(image, header, object)?,
            )
        }
        STATUS => {
            let location = required(observation.status)?;
            let mut header = fixed(
                profile,
                location.page().page_no,
                2,
                PageKind::TransactionStatus,
                lsn,
            );
            header.identity.page = location.page();
            (header, inputs.status, Some(0), Some(0))
        }
        UNDO => {
            let reference = required(required(root)?.undo())?;
            let mut header = fixed(profile, reference.page().page_no, 3, PageKind::Undo, lsn);
            header.identity.page = reference.page();
            (header, inputs.undo, Some(0), Some(0))
        }
        ANCHOR | ROW => {
            let root = required(root)?;
            let pair = required(root.pair())?;
            let object = required(root.object())?;
            let index = usize::from(role == ROW);
            let header = PageHeader {
                identity: PageIdentity {
                    database: profile.spec().namespace.database,
                    page: pair[index],
                    owner: object.object,
                    owner_generation: object.generation,
                },
                kind: if role == ANCHOR {
                    PageKind::TreeAnchor
                } else {
                    PageKind::RowLeaf
                },
                level: 0,
                lsn: PageLsn(lsn),
            };
            (header, None, None, Some(0))
        }
        _ => return Err(Error::InvalidInput),
    };
    if before.is_none() && !matches!(role, ANCHOR | ROW) {
        return Err(Error::InvalidInput);
    }
    Ok(PageChange {
        header,
        before,
        after: image,
        before_slot,
        after_slot,
    })
}

pub(super) enum Output<'a> {
    Encode(&'a mut [u8]),
    Observe(&'a [u8]),
}

fn run(
    inputs: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 8],
    output: Output<'_>,
) -> Result<Observation> {
    let mut descriptors = [[0; 1536]; 2];
    let mut core = [0; 256];
    run_with_buffers(inputs, after, output, &mut descriptors, &mut core)
}

fn run_with_buffers(
    inputs: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 8],
    output: Output<'_>,
    descriptors: &mut [[u8; 1536]; 2],
    core: &mut [u8; 256],
) -> Result<Observation> {
    let (mut observation, changed, original_root, catalog_bytes) =
        compute(inputs, after, descriptors, core)?;
    let root = if observation.operation == Operation::Reserve {
        let ordinal = required(observation.job_ordinal)?;
        let page = catalog::decode_jobs_page(
            &after[JOB],
            inputs.profile.spec().namespace,
            (ordinal / 15) as u8,
        )?;
        Some(catalog::decode_job(
            required(page.cell(ordinal % 15))?,
            inputs.profile.spec().namespace,
        )?)
    } else {
        original_root
    };
    let first = changed
        .iter()
        .position(|value| *value)
        .ok_or(Error::Corrupt)?;
    let initial = page_change(inputs, observation, root, first, &after[first])?;
    let mut pages = [(0usize, initial); 8];
    let mut count = 0;
    for (role, image) in after.iter().enumerate() {
        if changed[role] {
            pages[count] = (role, page_change(inputs, observation, root, role, image)?);
            count += 1;
        }
    }
    pages[..count].sort_unstable_by_key(|(_, change)| {
        let mut reference = [0; 24];
        change
            .header
            .identity
            .page
            .encode(&mut reference)
            .expect("checked page identity");
        (u64_at(&reference, 0), change.header.identity.page.page_no)
    });
    let mut ordered = [initial; 8];
    for (index, (role, change)) in pages[..count].iter().copied().enumerate() {
        observation.order[index] = role as u8;
        ordered[index] = change;
    }
    observation.count = count as u8;
    let op = observation.operation;
    let before = if op.handoff() {
        [inputs.transaction, inputs.aging]
    } else if op == Operation::RollbackStart {
        [inputs.transaction, None]
    } else {
        [inputs.aging, None]
    };
    let spec = FrameSpec {
        operation: op,
        profile: inputs.profile,
        header: observation.header,
        catalog_bytes,
        before,
        after: [&descriptors[0], &descriptors[1]],
        core,
        root_job: root,
        pages: &ordered[..count],
    };
    match output {
        Output::Encode(bytes) => {
            let selected = bytes
                .get_mut(..observation.frame_bytes)
                .ok_or(Error::InvalidInput)?;
            frame::encode(&spec, selected)?;
        }
        Output::Observe(bytes) => frame::verify(&spec, bytes)?,
    }
    Ok(observation)
}

/// Prepare one complete group into caller loans, with no I/O or shared state
/// effects. Unused tail bytes in the frame loan are not part of the group.
pub(crate) fn produce(
    inputs: &Inputs<'_>,
    after: &mut [[u8; PAGE_BYTES]; 8],
    frame: &mut [u8],
) -> Result<Observation> {
    run(inputs, after, Output::Encode(frame))
}

/// Replay computes the same canonical successor, then compares every original
/// descriptor, core, native image, edge/count/item and physical redo record.
pub(crate) fn observe(
    inputs: &Inputs<'_>,
    bytes: &[u8],
    after: &mut [[u8; PAGE_BYTES]; 8],
) -> Result<Observation> {
    run(inputs, after, Output::Observe(bytes))
}

/// Pure temporary storage in the original replay loan. Its bytes never contain
/// an install/WAL/visibility grant. The caller must lend a zeroed disjoint lane.
pub(super) struct ReplayScratch<'a> {
    descriptors: &'a mut [[u8; 1536]; 2],
    core: &'a mut [u8; 256],
}
impl<'a> ReplayScratch<'a> {
    fn new(bytes: &'a mut [u8; super::REPLAY_SCRATCH_BYTES]) -> Result<Self> {
        // Never hide an invalid original reserved tail by clearing it on entry.
        if !super::zero(bytes) {
            return Err(Error::InvalidInput);
        }
        let (descriptors, core) = bytes.split_at_mut(3072);
        Ok(Self {
            descriptors: descriptors
                .as_chunks_mut::<1536>()
                .0
                .try_into()
                .expect("two descriptors"),
            core: core.try_into().expect("semantic core"),
        })
    }
}
impl Drop for ReplayScratch<'_> {
    fn drop(&mut self) {
        for descriptor in &mut *self.descriptors {
            descriptor.fill(0);
        }
        self.core.fill(0);
    }
}

/// Same canonical observer with explicit caller-owned descriptor/core backing.
/// The zero tail is restored before any success/error reaches the replay owner.
pub(crate) fn observe_borrowed(
    inputs: &Inputs<'_>,
    bytes: &[u8],
    after: &mut [[u8; PAGE_BYTES]; 8],
    scratch: &mut [u8; super::REPLAY_SCRATCH_BYTES],
) -> Result<Observation> {
    let loan = ReplayScratch::new(scratch)?;
    run_with_buffers(
        inputs,
        after,
        Output::Observe(bytes),
        &mut *loan.descriptors,
        &mut *loan.core,
    )
}

#[cfg(test)]
mod scratch_tests {
    use super::*;

    #[test]
    fn original_scratch_is_restored_on_normal_drop_and_unwind() {
        for unwind in [false, true] {
            let mut surrounding = [0; super::super::REPLAY_SCRATCH_BYTES + 64];
            surrounding[..32].fill(0x97);
            surrounding[super::super::REPLAY_SCRATCH_BYTES + 32..].fill(0x53);
            let bytes: &mut [u8; super::super::REPLAY_SCRATCH_BYTES] = (&mut surrounding
                [32..super::super::REPLAY_SCRATCH_BYTES + 32])
                .try_into()
                .unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let loan = ReplayScratch::new(bytes).unwrap();
                loan.descriptors[0].fill(0xa7);
                loan.descriptors[1].fill(0x71);
                loan.core.fill(0x3a);
                if unwind {
                    panic!("unwind with original replay temporary loan");
                }
            }));
            assert_eq!(result.is_err(), unwind);
            assert!(bytes.iter().all(|byte| *byte == 0));
            assert!(surrounding[..32].iter().all(|byte| *byte == 0x97));
            assert!(
                surrounding[super::super::REPLAY_SCRATCH_BYTES + 32..]
                    .iter()
                    .all(|byte| *byte == 0x53)
            );
        }
        let mut bytes = [0; super::super::REPLAY_SCRATCH_BYTES];
        bytes[3072] = 1;
        assert_eq!(
            ReplayScratch::new(&mut bytes).err().unwrap(),
            Error::InvalidInput
        );
        assert_eq!(bytes[3072], 1);
    }
}
