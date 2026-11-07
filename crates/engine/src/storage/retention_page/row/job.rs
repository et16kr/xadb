//! Exact Job3 cells. No version rewriting or legacy decoder fallback.
use super::{
    CATALOG_PEAK, Generation, JOB_BYTES, MAX_CATALOG_CELL, MIN_CATALOG_CELL, Namespace,
    ObjectHandle, ObjectId, PageRef, RELEASE_BYTES, ROW_PEAK, RemainingBudget, Result,
    RetentionError, RowId, SchemaEpoch, StatusSlotRef, TxnRef, UndoRef, dynamic_page, get,
    same_address, zero,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobState {
    Free,
    TerminalLeased,
    TerminalReady,
    RootReserved,
    RootRetained,
    RootReleasingCommitted,
    RootReleasingCompensated,
}
impl JobState {
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::TerminalLeased | Self::TerminalReady)
    }
    pub(crate) fn is_root(self) -> bool {
        !matches!(
            self,
            Self::Free | Self::TerminalLeased | Self::TerminalReady
        )
    }
    pub(crate) fn is_releasing(self) -> bool {
        matches!(
            self,
            Self::RootReleasingCommitted | Self::RootReleasingCompensated
        )
    }
    pub(crate) fn mask_index(self) -> usize {
        match self {
            Self::Free => 0,
            Self::TerminalLeased | Self::RootReserved => 1,
            _ => 2,
        }
    }
}

/// Closed cleanup recipe observed in the cell, not descriptor authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobScope {
    Catalog,
    Row,
}

/// The Row root's effect byte `@284` (dev3-row-delete.md#job-effect,
/// dev3-row-update.md#job-effect): which cost recipe and Undo before kind the
/// root carries. Reserved, Retained and Releasing keep it; Free, Terminal and
/// Catalog roots are always Insert (0). An Update (2) Reserved/Retained root
/// also carries its new cell C' in `[286,288)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowEffect {
    Insert,
    Delete,
    Update,
}

/// Fixed WAL terms of the Row cost recipes (contract row-shape, row-delete
/// #delete-costs, row-update#update-costs): the Forward, CLR and Prune frames
/// without their cell and h terms, and the four release steps after a Prune
/// or CLR.
const FORWARD_FIXED: u64 = 91_304;
const CLR_FIXED: u64 = 91_096;
const PRUNE_FIXED: u64 = 89_496;
const RELEASE_STEPS: u64 = 4 * RELEASE_BYTES;
const _: () = assert!(PRUNE_FIXED + RELEASE_STEPS == 311_608);
/// DELETE's 180-byte tombstone, whose place an UPDATE's new cell C' takes.
const TOMBSTONE: u64 = 180;
const _: () = assert!(FORWARD_FIXED - TOMBSTONE == 91_124);
const _: () = assert!(CLR_FIXED - TOMBSTONE == 90_916);
const _: () = assert!(PRUNE_FIXED + RELEASE_STEPS - TOMBSTONE == 311_428);

/// Each recipe's cell term: INSERT's Forward and CLR carry the cell once and
/// its Prune twice; DELETE's Forward and CLR carry both the old cell and its U
/// payload, and its Prune the U payload with the removed tombstone. UPDATE is
/// DELETE with the tombstone replaced by the new cell C' (`new_cell`, which
/// only UPDATE reads): Forward/CLR `2C+C'−180`, Prune `C+2C'−180`.
fn cell_terms(effect: RowEffect, cell: u32, new_cell: u32) -> (u64, u64, u64) {
    let (c, n) = (u64::from(cell), u64::from(new_cell));
    match effect {
        RowEffect::Insert => (c, c, 2 * c),
        RowEffect::Delete => (2 * c, 2 * c, c),
        RowEffect::Update => (2 * c + n, 2 * c + n, c + 2 * n),
    }
}
/// The fixed part of a recipe after UPDATE takes back the tombstone term.
fn fixed(effect: RowEffect, fixed: u64) -> u64 {
    if effect == RowEffect::Update {
        fixed - TOMBSTONE
    } else {
        fixed
    }
}

/// The exact Row root recipes of one effect, cells and h. Reserve's budget,
/// the ledger and the census all take these, never their own copies.
/// `new_cell` is an UPDATE root's C'; INSERT and DELETE ignore it.
pub(crate) fn row_forward_wal(effect: RowEffect, cell: u32, new_cell: u32, previous: bool) -> u64 {
    fixed(effect, FORWARD_FIXED) + cell_terms(effect, cell, new_cell).0 + 128 * u64::from(previous)
}
pub(crate) fn row_clr_wal(effect: RowEffect, cell: u32, new_cell: u32, previous: bool) -> u64 {
    fixed(effect, CLR_FIXED) + cell_terms(effect, cell, new_cell).1 + 128 * u64::from(previous)
}
pub(crate) fn row_cleanup_wal(effect: RowEffect, cell: u32, new_cell: u32, previous: bool) -> u64 {
    fixed(effect, PRUNE_FIXED + RELEASE_STEPS)
        + cell_terms(effect, cell, new_cell).2
        + 128 * u64::from(previous)
}

/// An overflow INSERT root's cell: 12-byte cell head, 168-byte record
/// header and the 64-byte head (dev3-row-overflow.md#overflow-row).
pub(crate) const OVERFLOW_CELL: u32 = 244;
/// ChunkWrite's frame W and ChainRetire's largest frame X
/// (dev3-row-overflow.md#opcodes, #ledger).
pub(crate) const CHUNK_WRITE_WAL: u64 = 56_976;
pub(crate) const CHAIN_RETIRE_WAL: u64 = 59_056;
const OVERFLOW_TOTAL: std::ops::RangeInclusive<u32> = 2_049..=131_072;
const CHUNK_PAYLOAD: u32 = 16_092;

/// A Reserved overflow root's chain so far (`[328,360)`): the last written
/// chunk (lowest index), the row's total and the chunks written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingChain {
    pub(crate) front: Option<PageRef>,
    pub(crate) total: u32,
    pub(crate) written: u16,
}
impl PendingChain {
    pub(crate) fn count(self) -> u16 {
        self.total.div_ceil(CHUNK_PAYLOAD) as u16
    }
}

/// The chain a ReleasingCompensated overflow root still has to return
/// (`[200,248)`, `[288,292)`): its table anchor, chunk 0 and the total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReturnChain {
    pub(crate) anchor: PageRef,
    pub(crate) root: PageRef,
    pub(crate) total: u32,
}
impl ReturnChain {
    pub(crate) fn count(self) -> u16 {
        self.total.div_ceil(CHUNK_PAYLOAD) as u16
    }
}

#[derive(Clone, Copy)]
pub(crate) struct JobView<'a> {
    bytes: &'a [u8; JOB_BYTES],
    state: JobState,
    scope: Option<JobScope>,
}
impl<'a> JobView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; JOB_BYTES] {
        self.bytes
    }
    pub(crate) fn state(self) -> JobState {
        self.state
    }
    pub(crate) fn scope(self) -> Option<JobScope> {
        self.scope
    }
    pub(crate) fn row_id(self) -> Option<RowId> {
        (self.scope == Some(JobScope::Row) && self.state.is_root())
            .then(|| RowId::new(get(self.bytes, 320)).expect("validated RowId"))
    }
    /// The exact budget recipe observes h; the semantic owner must match it
    /// against the actual Status head / Undo previous-transaction edge.
    pub(crate) fn has_previous_transaction(self) -> Option<bool> {
        (self.scope == Some(JobScope::Row)
            && matches!(self.state, JobState::RootReserved | JobState::RootRetained))
        .then(|| {
            get(self.budget(), 400)
                == row_cleanup_wal(self.effect(), self.cell_bytes(), self.new_cell(), true)
        })
    }
    /// The root's effect byte; validated by `decode_job`.
    pub(crate) fn effect(self) -> RowEffect {
        match self.bytes[284] {
            1 => RowEffect::Delete,
            2 => RowEffect::Update,
            _ => RowEffect::Insert,
        }
    }
    /// An Update Reserved/Retained root's new row cell C' (`[286,288)`,
    /// dev3-row-update.md#job-effect); 0 for every other Job and state.
    pub(crate) fn new_cell(self) -> u32 {
        u32::from(u16::from_le_bytes([self.bytes[286], self.bytes[287]]))
    }
    /// The leaf bytes this Reserved/Retained Row root needs to restore its
    /// old row without a split once Retained: DELETE's `C−180` (the old row
    /// over its tombstone), UPDATE's `max(0, C−C')`, INSERT's 0
    /// (dev3-row-delete.md#leaf-reservation, dev3-row-update.md
    /// #leaf-reservation). D of a leaf is the sum over its table's Retained
    /// roots. 0 for every other Job; decode keeps a live Row C ≥ 185.
    pub(crate) fn restore_bytes(self) -> usize {
        if self.scope != Some(JobScope::Row)
            || !matches!(self.state, JobState::RootReserved | JobState::RootRetained)
        {
            return 0;
        }
        let cell = self.cell_bytes() as usize;
        match self.effect() {
            RowEffect::Insert => 0,
            RowEffect::Delete => cell.saturating_sub(TOMBSTONE as usize),
            RowEffect::Update => cell.saturating_sub(self.new_cell() as usize),
        }
    }
    /// The payload byte `@285` (dev3-row-overflow.md#job-overflow): 1 for an
    /// overflow INSERT root in Reserved, Retained or ReleasingCompensated,
    /// 0 for every other Job.
    pub(crate) fn payload(self) -> u8 {
        self.bytes[285]
    }
    pub(crate) fn overflow(self) -> bool {
        self.payload() == 1
    }
    /// A Reserved overflow root's pending chain; None for every other Job.
    pub(crate) fn pending(self) -> Option<PendingChain> {
        (self.state == JobState::RootReserved && self.overflow()).then(|| PendingChain {
            front: (!zero(&self.bytes[328..352]))
                .then(|| PageRef::decode(&self.bytes[328..352]).expect("validated front")),
            total: u32::from_le_bytes(self.bytes[352..356].try_into().expect("checked field")),
            written: u16::from_le_bytes([self.bytes[356], self.bytes[357]]),
        })
    }
    /// A ReleasingCompensated overflow root's chain still to return; None
    /// once ChainRetire returned it and for every other Job.
    pub(crate) fn chain(self) -> Option<ReturnChain> {
        (self.state == JobState::RootReleasingCompensated
            && self.overflow()
            && !zero(&self.bytes[200..248]))
        .then(|| ReturnChain {
            anchor: PageRef::decode(&self.bytes[200..224]).expect("validated anchor"),
            root: PageRef::decode(&self.bytes[224..248]).expect("validated root"),
            total: u32::from_le_bytes(self.bytes[288..292].try_into().expect("checked field")),
        })
    }
    /// The Reserved root's remaining Forward and a live root's remaining CLR
    /// (ledger rows 1 and 0); `None` outside a Row Reserved/Retained root.
    /// A Reserved overflow root's Forward also carries its unwritten chunks
    /// (dev3-row-overflow.md#ledger): `(k−written)·W + F`.
    pub(crate) fn forward_wal(self) -> Option<u64> {
        let chunks = self
            .pending()
            .map_or(0, |p| u64::from(p.count() - p.written));
        self.has_previous_transaction().map(|h| {
            chunks * CHUNK_WRITE_WAL
                + row_forward_wal(self.effect(), self.cell_bytes(), self.new_cell(), h)
        })
    }
    /// The steps of `forward_wal`: 1, plus a Reserved overflow root's
    /// unwritten chunks.
    pub(crate) fn forward_steps(self) -> Option<u64> {
        let chunks = self
            .pending()
            .map_or(0, |p| u64::from(p.count() - p.written));
        self.has_previous_transaction().map(|_| 1 + chunks)
    }
    pub(crate) fn clr_wal(self) -> Option<u64> {
        self.has_previous_transaction()
            .map(|h| row_clr_wal(self.effect(), self.cell_bytes(), self.new_cell(), h))
    }
    pub(crate) fn generation(self) -> Generation {
        Generation::new(get(self.bytes, 8)).expect("validated Job generation")
    }
    pub(crate) fn source(self) -> Option<TxnRef> {
        (self.state != JobState::Free)
            .then(|| TxnRef::decode(&self.bytes[16..64]).expect("validated source"))
    }
    pub(crate) fn status(self) -> Option<StatusSlotRef> {
        (self.state != JobState::Free)
            .then(|| StatusSlotRef::decode(&self.bytes[80..120]).expect("validated Status"))
    }
    pub(crate) fn undo(self) -> Option<UndoRef> {
        self.state
            .is_root()
            .then(|| UndoRef::decode(&self.bytes[128..168]).expect("validated Undo"))
    }
    pub(crate) fn object(self) -> Option<ObjectHandle> {
        self.state
            .is_root()
            .then(|| object(self.bytes).expect("validated object"))
    }
    pub(crate) fn pair(self) -> Option<[PageRef; 2]> {
        matches!(self.state, JobState::RootReserved | JobState::RootRetained).then(|| {
            [
                PageRef::decode(&self.bytes[200..224]).expect("validated Anchor"),
                PageRef::decode(&self.bytes[224..248]).expect("validated RowRoot"),
            ]
        })
    }
    pub(crate) fn lease_lsn(self) -> u64 {
        get(self.bytes, 64)
    }
    pub(crate) fn adoption_lsn(self) -> u64 {
        get(self.bytes, 72)
    }
    pub(crate) fn outcome(self) -> u8 {
        self.bytes[120]
    }
    pub(crate) fn release_cursor(self) -> u8 {
        self.bytes[121]
    }
    pub(crate) fn undo_sequence(self) -> u64 {
        get(self.bytes, 168)
    }
    pub(crate) fn statement(self) -> u64 {
        get(self.bytes, 248)
    }
    pub(crate) fn mutation(self) -> u64 {
        get(self.bytes, 256)
    }
    pub(crate) fn commit_q(self) -> u64 {
        get(self.bytes, 264)
    }
    pub(crate) fn release_start(self) -> u64 {
        get(self.bytes, 272)
    }
    pub(crate) fn cell_bytes(self) -> u32 {
        u32::from_le_bytes(self.bytes[280..284].try_into().expect("checked field"))
    }
    pub(crate) fn projection_digest(self) -> &'a [u8; 32] {
        self.bytes[288..320].try_into().expect("checked field")
    }
    pub(crate) fn sender_digest(self) -> &'a [u8; 32] {
        self.bytes[328..360].try_into().expect("checked field")
    }
    pub(crate) fn last_step(self) -> u64 {
        get(self.bytes, 376)
    }
    pub(crate) fn budget(self) -> &'a [u8; 640] {
        self.bytes[384..1024].try_into().expect("checked field")
    }
    pub(crate) fn cleanup(self) -> RemainingBudget {
        if self.state == JobState::Free {
            return RemainingBudget::default();
        }
        let (steps, wal) = match self.state {
            JobState::TerminalLeased | JobState::TerminalReady => (1, 55_112),
            JobState::RootReserved | JobState::RootRetained => (
                5,
                row_cleanup_wal(
                    self.effect(),
                    self.cell_bytes(),
                    self.new_cell(),
                    self.has_previous_transaction().unwrap_or(false),
                ),
            ),
            // A chain still to return adds ChainRetire before the four
            // release steps (dev3-row-overflow.md#ledger).
            _ if self.chain().is_some() => (5, RELEASE_STEPS + CHAIN_RETIRE_WAL),
            _ => {
                let steps = 4 - u64::from(self.release_cursor());
                (steps, steps * RELEASE_BYTES)
            }
        };
        RemainingBudget {
            steps,
            wal,
            protected: wal,
            peak: self.peak(),
            ..RemainingBudget::default()
        }
    }
    fn peak(self) -> super::ResourcePeak {
        match self.scope {
            Some(JobScope::Row) => ROW_PEAK,
            _ => CATALOG_PEAK,
        }
    }
}

fn object(bytes: &[u8; JOB_BYTES]) -> Result<ObjectHandle> {
    Ok(ObjectHandle {
        object: ObjectId::new(get(bytes, 176)).map_err(|_| RetentionError::IdentityMismatch)?,
        generation: Generation::new(get(bytes, 184))
            .map_err(|_| RetentionError::IdentityMismatch)?,
        schema_epoch: SchemaEpoch::new(get(bytes, 192))
            .map_err(|_| RetentionError::IdentityMismatch)?,
    })
}

/// A candidate byte cell is validated completely before copying. No owned
/// decoded JobSpec/budget is retained across the next cell's decode.
pub(crate) fn encode_job(candidate: &[u8], namespace: Namespace, output: &mut [u8]) -> Result<()> {
    let job = decode_job(candidate, namespace)?;
    if output.len() != JOB_BYTES {
        return Err(RetentionError::InvalidInput);
    }
    output.copy_from_slice(job.bytes());
    Ok(())
}

pub(crate) fn decode_job(input: &[u8], namespace: Namespace) -> Result<JobView<'_>> {
    let bytes: &[u8; JOB_BYTES] = input.try_into().map_err(|_| RetentionError::Corrupt)?;
    if &bytes[..4] != b"XQJ1" {
        return Err(RetentionError::Corrupt);
    }
    if bytes[4..6] != [3, 0] {
        return Err(RetentionError::UnsupportedFormat);
    }
    if namespace.timeline != 1 {
        return Err(RetentionError::InvalidInput);
    }
    let state = match (bytes[6], bytes[7]) {
        (0, 0) => JobState::Free,
        (1, 1) => JobState::TerminalLeased,
        (2, 1) => JobState::TerminalReady,
        (1, 2 | 3) => JobState::RootReserved,
        (2, 2 | 3) => JobState::RootRetained,
        (3, 2 | 3) => JobState::RootReleasingCommitted,
        (4, 2 | 3) => JobState::RootReleasingCompensated,
        _ => return Err(RetentionError::UnsupportedState),
    };
    let generation = Generation::new(get(bytes, 8)).map_err(|_| RetentionError::Corrupt)?;
    let scope = match bytes[7] {
        0 => None,
        2 => Some(JobScope::Catalog),
        3 => Some(JobScope::Row),
        // Terminal has no domain field: only these complete peak recipes are
        // valid. validate_budget rejects every other differing budget field.
        1 => Some(match get(bytes, 384 + 3 * 128 + 88) {
            9 => JobScope::Catalog,
            16 => JobScope::Row,
            _ => return Err(RetentionError::BudgetMismatch),
        }),
        _ => unreachable!("closed state/kind above"),
    };
    let job = JobView {
        bytes,
        state,
        scope,
    };
    if state == JobState::Free {
        if !zero(&bytes[16..]) {
            return Err(RetentionError::Corrupt);
        }
        return Ok(job);
    }
    let source = TxnRef::decode(&bytes[16..64])?;
    let status = StatusSlotRef::decode(&bytes[80..120])?;
    if source.id().database != namespace.database
        || source.id().timeline != namespace.timeline
        || source.generation() != status.generation()
        || status.slot() != 0
        || !dynamic_page(status.page())
    {
        return Err(RetentionError::IdentityMismatch);
    }
    if generation.get() < 2
        || source.generation().get() < 2
        || job.lease_lsn() == 0
        || job.last_step() < job.lease_lsn()
    {
        return Err(RetentionError::OrderMismatch);
    }
    // `@284` is the Row root effect byte (0 INSERT, 1 DELETE, 2 UPDATE); every
    // other Job keeps it zero. `@285` is the payload byte: 1 only for an
    // INSERT Row root in Reserved, Retained or ReleasingCompensated
    // (dev3-row-overflow.md#job-overflow). `[286,288)` is an UPDATE root's
    // C' (checked by `validate_root`), zero everywhere else.
    let overflow_state = matches!(
        state,
        JobState::RootReserved | JobState::RootRetained | JobState::RootReleasingCompensated
    );
    if !zero(&bytes[122..128])
        || bytes[284]
            > if bytes[7] == 3 && state.is_root() {
                2
            } else {
                0
            }
        || bytes[285] > u8::from(bytes[7] == 3 && overflow_state && bytes[284] == 0)
        || (bytes[284] != 2 && !zero(&bytes[286..288]))
        || (bytes[7] != 3 && !zero(&bytes[320..328]))
        || !zero(&bytes[360..376])
    {
        return Err(RetentionError::Corrupt);
    }
    if state.is_terminal() {
        validate_terminal(job)?;
    } else {
        validate_root(job, status)?;
    }
    validate_budget(job)?;
    Ok(job)
}

fn validate_terminal(job: JobView<'_>) -> Result<()> {
    let bytes = job.bytes;
    if job.release_cursor() != 0 || !zero(&bytes[128..264]) || !zero(&bytes[272..328]) {
        return Err(RetentionError::Corrupt);
    }
    if job.state == JobState::TerminalLeased {
        if job.adoption_lsn() != 0
            || job.outcome() != 0
            || job.commit_q() != 0
            || !zero(job.sender_digest())
        {
            return Err(RetentionError::Corrupt);
        }
        if job.last_step() != job.lease_lsn() {
            return Err(RetentionError::OrderMismatch);
        }
    } else {
        if !matches!(job.outcome(), 1 | 2) {
            return Err(RetentionError::UnsupportedState);
        }
        if (job.outcome() == 1) != (job.commit_q() != 0)
            || job.adoption_lsn() <= job.lease_lsn()
            || job.last_step() != job.adoption_lsn()
        {
            return Err(RetentionError::OrderMismatch);
        }
    }
    Ok(())
}

fn validate_root(job: JobView<'_>, status: StatusSlotRef) -> Result<()> {
    let bytes = job.bytes;
    let undo = UndoRef::decode(&bytes[128..168])?;
    let object = object(bytes)?;
    if undo.slot() != 0
        || undo.generation().get() < 2
        || !dynamic_page(undo.page())
        || same_address(undo.page(), status.page())
        || object.object.get() < 6
        || object.generation.get() != 1
        || object.schema_epoch.get() != 1
    {
        return Err(RetentionError::IdentityMismatch);
    }
    if job.scope == Some(JobScope::Row) && get(bytes, 320) == 0 {
        return Err(RetentionError::IdentityMismatch);
    }
    if job.undo_sequence() == 0
        || job.statement() == 0
        || job.mutation() == 0
        || (job.scope == Some(JobScope::Catalog) && (job.statement() != 1 || job.mutation() != 1))
    {
        return Err(RetentionError::OrderMismatch);
    }
    if job.outcome() != 0 {
        return Err(RetentionError::Corrupt);
    }
    if job.state.is_releasing() {
        // Releasing keeps only the effect and payload bytes of `[280,320)`;
        // a ReleasingCompensated overflow root also its chain still to
        // return: `[200,248)` anchor and root, `[288,292)` total.
        let parts = [&bytes[200..224], &bytes[224..248], &bytes[288..292]].map(|b| !zero(b));
        let chain = parts.contains(&true);
        if !zero(&bytes[280..284])
            || !zero(&bytes[286..288])
            || !zero(&bytes[292..320])
            || (chain && (!job.overflow() || parts.contains(&false)))
        {
            return Err(RetentionError::Corrupt);
        }
        if chain {
            let anchor = PageRef::decode(&bytes[200..224])?;
            let root = PageRef::decode(&bytes[224..248])?;
            let total = u32::from_le_bytes(bytes[288..292].try_into().expect("checked field"));
            if !OVERFLOW_TOTAL.contains(&total) {
                return Err(RetentionError::Corrupt);
            }
            if same_address(anchor, root)
                || [anchor, root].iter().any(|p| {
                    !dynamic_page(*p)
                        || same_address(*p, status.page())
                        || same_address(*p, undo.page())
                })
            {
                return Err(RetentionError::IdentityMismatch);
            }
            // ChainRetire is the first cleanup step (cursor 0).
            if job.release_cursor() != 0 {
                return Err(RetentionError::OrderMismatch);
            }
        }
        // At cursor 0 the last step is the CLR/Prune itself, except for an
        // overflow root whose ChainRetire already returned its chain.
        let retired = job.overflow() && !chain;
        if job.adoption_lsn() <= job.lease_lsn()
            || job.release_start() <= job.adoption_lsn()
            || job.release_cursor() > 3
            || (job.state == JobState::RootReleasingCommitted) != (job.commit_q() != 0)
            || (job.release_cursor() == 0 && !retired && job.last_step() != job.release_start())
            || ((job.release_cursor() != 0 || retired) && job.last_step() <= job.release_start())
        {
            return Err(RetentionError::OrderMismatch);
        }
    } else {
        let pair = [
            PageRef::decode(&bytes[200..224])?,
            PageRef::decode(&bytes[224..248])?,
        ];
        if same_address(pair[0], pair[1])
            || pair.iter().any(|p| {
                !dynamic_page(*p)
                    || same_address(*p, status.page())
                    || same_address(*p, undo.page())
            })
        {
            return Err(RetentionError::IdentityMismatch);
        }
        let cell_range = match job.scope {
            Some(JobScope::Row) => 185..=2228,
            _ => MIN_CATALOG_CELL..=MAX_CATALOG_CELL,
        };
        // An UPDATE root names its new cell C' beside the old C.
        let new_cell_ok = if job.effect() == RowEffect::Update {
            (185..=2228).contains(&job.new_cell())
        } else {
            job.new_cell() == 0
        };
        if !cell_range.contains(&job.cell_bytes())
            || (job.overflow() && job.cell_bytes() != OVERFLOW_CELL)
            || !new_cell_ok
            || job.release_cursor() != 0
            || job.release_start() != 0
            || job.commit_q() != 0
        {
            return Err(RetentionError::Corrupt);
        }
        if job.state == JobState::RootReserved {
            // An overflow root's pending chain takes the sender digest's
            // place until Forward (dev3-row-overflow.md#job-overflow).
            // `pending` reads a front that this check decoded first.
            if job.overflow() && !zero(&bytes[328..352]) {
                PageRef::decode(&bytes[328..352])?;
            }
            let written = match job.pending() {
                None if !zero(job.sender_digest()) => return Err(RetentionError::Corrupt),
                None => 0,
                Some(pending) => {
                    if !OVERFLOW_TOTAL.contains(&pending.total)
                        || pending.written > pending.count()
                        || pending.front.is_some() != (pending.written != 0)
                        || !zero(&bytes[358..360])
                    {
                        return Err(RetentionError::Corrupt);
                    }
                    if let Some(front) = pending.front
                        && (!dynamic_page(front)
                            || same_address(front, status.page())
                            || same_address(front, undo.page())
                            || pair.iter().any(|p| same_address(front, *p)))
                    {
                        return Err(RetentionError::IdentityMismatch);
                    }
                    pending.written
                }
            };
            if job.adoption_lsn() != 0 {
                return Err(RetentionError::Corrupt);
            }
            // Each ChunkWrite is the root's last step.
            if (written == 0) != (job.last_step() == job.lease_lsn())
                || job.last_step() < job.lease_lsn()
            {
                return Err(RetentionError::OrderMismatch);
            }
        } else if job.adoption_lsn() <= job.lease_lsn() || job.last_step() != job.adoption_lsn() {
            return Err(RetentionError::OrderMismatch);
        }
    }
    Ok(())
}

fn validate_budget(job: JobView<'_>) -> Result<()> {
    let debt = job.cleanup();
    let peak = job.peak();
    for row in 0..5 {
        let bytes = &job.budget()[row * 128..(row + 1) * 128];
        if bytes[0] != row as u8 + 1 || !zero(&bytes[1..8]) || !zero(&bytes[120..128]) {
            return Err(RetentionError::BudgetMismatch);
        }
        if row != 3 {
            if !zero(&bytes[8..120]) {
                return Err(RetentionError::BudgetMismatch);
            }
            continue;
        }
        for (at, value) in [
            (8, debt.steps),
            (16, debt.wal),
            (24, 0),
            (32, 0),
            (40, 0),
            (48, 0),
            (56, 0),
            (64, peak.frames),
            (72, peak.memory),
            (80, peak.locks),
            (88, peak.registry),
            (96, peak.files),
            (104, peak.doublewrite),
            (112, debt.protected),
        ] {
            if get(bytes, at) != value {
                return Err(RetentionError::BudgetMismatch);
            }
        }
    }
    Ok(())
}
