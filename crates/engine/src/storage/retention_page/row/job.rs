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
        .then(|| get(self.budget(), 400) == 311_736 + 2 * u64::from(self.cell_bytes()))
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
                311_608
                    + 2 * u64::from(self.cell_bytes())
                    + 128 * u64::from(self.has_previous_transaction().unwrap_or(false)),
            ),
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
    if !zero(&bytes[122..128])
        || !zero(&bytes[284..288])
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
        if !zero(&bytes[200..248]) || !zero(&bytes[280..320]) {
            return Err(RetentionError::Corrupt);
        }
        if job.adoption_lsn() <= job.lease_lsn()
            || job.release_start() <= job.adoption_lsn()
            || job.release_cursor() > 3
            || (job.state == JobState::RootReleasingCommitted) != (job.commit_q() != 0)
            || (job.release_cursor() == 0 && job.last_step() != job.release_start())
            || (job.release_cursor() != 0 && job.last_step() <= job.release_start())
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
        if !cell_range.contains(&job.cell_bytes())
            || job.release_cursor() != 0
            || job.release_start() != 0
            || job.commit_q() != 0
        {
            return Err(RetentionError::Corrupt);
        }
        if job.state == JobState::RootReserved {
            if job.adoption_lsn() != 0 || !zero(job.sender_digest()) {
                return Err(RetentionError::Corrupt);
            }
            if job.last_step() != job.lease_lsn() {
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
