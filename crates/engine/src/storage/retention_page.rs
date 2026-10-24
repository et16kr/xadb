//! Closed N=0 retention bytes. Local views confer no pin, reservation or reuse
//! authority; the live owner must prove the full Status/ATT/job census.
#![allow(dead_code)] // Awaiting the original live retention owner and recovery consumer.

use super::{
    allocation_page::{SystemWorkKind, SystemWorkRef},
    obligation::{RemainingBudget, ResourcePeak, encode_budget_rows},
    page::{self, FileId, PAGE_BYTES, PageError, PageHeader, PageKind, PageLsn, PageRef, PageView},
    retention_genesis::{self, RetentionGenesisPage},
    transaction_record::{StatusSlotRef, TxnRef},
};
use crate::api::Namespace;
use xadb_foundation::{
    codec::{ContentHasher, content_digest},
    identity::Generation,
};

pub(crate) const ROOT_BYTES: usize = 512;
pub(crate) const JOB_BYTES: usize = 1024;
pub(crate) const JOB_COUNT: usize = 64;
pub(crate) const STATUS_PAGES: usize = 55;
pub(crate) const FINISH_BYTES: usize = 55_112;

// Closed recipe candidate, subject to actual live-caller F05 before any effect.
// These values do not issue a reservation and are not a generic peak factory.
pub(crate) const EMPTY_PEAK: ResourcePeak = ResourcePeak {
    frames: 4,
    memory: 114_688,
    locks: 0,
    registry: 4,
    files: 5,
    doublewrite: 1,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetentionError {
    InvalidInput,
    UnsupportedFormat,
    UnsupportedState,
    Corrupt,
    IdentityMismatch,
    OrderMismatch,
    BudgetMismatch,
    Page(PageError),
}
impl From<PageError> for RetentionError {
    fn from(value: PageError) -> Self {
        Self::Page(value)
    }
}
pub(super) type Result<T> = std::result::Result<T, RetentionError>;

pub(super) fn get(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked field"))
}
pub(super) fn put(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
pub(super) fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|value| *value == 0)
}

pub(crate) fn cleanup_rows(count: usize) -> Result<[RemainingBudget; 5]> {
    if count > STATUS_PAGES {
        return Err(RetentionError::InvalidInput);
    }
    let mut rows = [RemainingBudget::default(); 5];
    if count != 0 {
        rows[3] = RemainingBudget {
            steps: count as u64,
            wal: count as u64 * FINISH_BYTES as u64,
            protected: count as u64 * FINISH_BYTES as u64,
            peak: EMPTY_PEAK,
            ..RemainingBudget::default()
        };
    }
    Ok(rows)
}

pub(crate) fn budget_digest(rows: &[RemainingBudget; 5]) -> [u8; 32] {
    let mut bytes = [0; 640];
    encode_budget_rows(rows, &mut bytes);
    content_digest(&bytes)
}

fn header(namespace: Namespace, role: RetentionGenesisPage, lsn: u64) -> Result<PageHeader> {
    let mut result = retention_genesis::expected_header(namespace, role)
        .map_err(|_| RetentionError::InvalidInput)?;
    result.lsn = PageLsn(lsn);
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JobSlotRef {
    page: PageRef,
    slot: u16,
    generation: Generation,
}
impl JobSlotRef {
    pub(crate) fn new(page: PageRef, slot: u16, generation: Generation) -> Result<Self> {
        let result = Self {
            page,
            slot,
            generation,
        };
        result.ordinal()?;
        Ok(result)
    }
    pub(crate) fn page(self) -> PageRef {
        self.page
    }
    pub(crate) fn slot(self) -> u16 {
        self.slot
    }
    pub(crate) fn generation(self) -> Generation {
        self.generation
    }
    pub(crate) fn ordinal(self) -> Result<usize> {
        if self.page.file != FileId::new(0x0000_0001_0000_0001).expect("first file")
            || self.page.incarnation.get() != 1
            || !(2..=6).contains(&self.page.page_no)
            || self.slot >= if self.page.page_no == 6 { 4 } else { 15 }
        {
            return Err(RetentionError::IdentityMismatch);
        }
        Ok((self.page.page_no as usize - 2) * 15 + usize::from(self.slot))
    }
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<()> {
        self.ordinal()?;
        if output.len() != 40 {
            return Err(RetentionError::InvalidInput);
        }
        let mut bytes = [0; 40];
        self.page.encode(&mut bytes[..24])?;
        bytes[24..26].copy_from_slice(&self.slot.to_le_bytes());
        put(&mut bytes, 32, self.generation.get());
        output.copy_from_slice(&bytes);
        Ok(())
    }
    pub(crate) fn decode(input: &[u8]) -> Result<Self> {
        if input.len() != 40 || !zero(&input[26..32]) {
            return Err(RetentionError::Corrupt);
        }
        Self::new(
            PageRef::decode(&input[..24])?,
            u16::from_le_bytes([input[24], input[25]]),
            Generation::new(get(input, 32)).map_err(|_| RetentionError::Corrupt)?,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobState {
    Free,
    Leased {
        source: TxnRef,
        status: StatusSlotRef,
        lease_lsn: u64,
    },
    Ready {
        source: TxnRef,
        status: StatusSlotRef,
        lease_lsn: u64,
        handoff_lsn: u64,
        sender_digest: [u8; 32],
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JobSpec {
    pub(crate) generation: Generation,
    pub(crate) state: JobState,
}
#[derive(Clone, Copy)]
pub(crate) struct JobView<'a> {
    bytes: &'a [u8; JOB_BYTES],
    spec: JobSpec,
}
impl<'a> JobView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; JOB_BYTES] {
        self.bytes
    }
    pub(crate) fn spec(self) -> JobSpec {
        self.spec
    }
    pub(crate) fn last_step(self) -> u64 {
        get(self.bytes, 376)
    }
}

fn validate_job(spec: JobSpec, namespace: Namespace) -> Result<()> {
    if namespace.timeline != 1 {
        return Err(RetentionError::InvalidInput);
    }
    let (source, status, lease, handoff) = match spec.state {
        JobState::Free => return Ok(()),
        JobState::Leased {
            source,
            status,
            lease_lsn,
        } => (source, status, lease_lsn, None),
        JobState::Ready {
            source,
            status,
            lease_lsn,
            handoff_lsn,
            ..
        } => (source, status, lease_lsn, Some(handoff_lsn)),
    };
    if source.id().database != namespace.database
        || source.id().timeline != namespace.timeline
        || source.generation() != status.generation()
        || status.slot() != 0
        || status.page().file != FileId::new(0x0000_0001_0000_0001).expect("first file")
        || !(9..64).contains(&status.page().page_no)
    {
        return Err(RetentionError::IdentityMismatch);
    }
    if spec.generation.get() < 2
        || source.generation().get() < 2
        || lease == 0
        || handoff.is_some_and(|value| value <= lease)
    {
        return Err(RetentionError::OrderMismatch);
    }
    Ok(())
}

/// Local encoding only. Complete output is preserved on every error.
pub(crate) fn encode_job(spec: JobSpec, namespace: Namespace, output: &mut [u8]) -> Result<()> {
    validate_job(spec, namespace)?;
    if output.len() != JOB_BYTES {
        return Err(RetentionError::InvalidInput);
    }
    let mut bytes = [0; JOB_BYTES];
    bytes[..4].copy_from_slice(b"XQJ1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    put(&mut bytes, 8, spec.generation.get());
    let (source, status, lease, handoff, sender) = match spec.state {
        JobState::Free => {
            output.copy_from_slice(&bytes);
            return Ok(());
        }
        JobState::Leased {
            source,
            status,
            lease_lsn,
        } => {
            bytes[6] = 1;
            (source, status, lease_lsn, 0, [0; 32])
        }
        JobState::Ready {
            source,
            status,
            lease_lsn,
            handoff_lsn,
            sender_digest,
        } => {
            bytes[6] = 2;
            bytes[120] = 2;
            bytes[121] = 1;
            (source, status, lease_lsn, handoff_lsn, sender_digest)
        }
    };
    bytes[7] = 1;
    source.encode(&mut bytes[16..64])?;
    put(&mut bytes, 64, lease);
    put(&mut bytes, 72, handoff);
    status.encode(&mut bytes[80..120])?;
    bytes[122] = 16;
    bytes[328..360].copy_from_slice(&sender);
    put(&mut bytes, 376, if handoff == 0 { lease } else { handoff });
    let rows = cleanup_rows(1)?;
    encode_budget_rows(
        &rows,
        (&mut bytes[384..1024]).try_into().expect("budget span"),
    );
    output.copy_from_slice(&bytes);
    Ok(())
}

pub(crate) fn decode_job(input: &[u8], namespace: Namespace) -> Result<JobView<'_>> {
    let bytes: &[u8; JOB_BYTES] = input.try_into().map_err(|_| RetentionError::Corrupt)?;
    if &bytes[..4] != b"XQJ1" {
        return Err(RetentionError::Corrupt);
    }
    if bytes[4..6] != [1, 0] {
        return Err(RetentionError::UnsupportedFormat);
    }
    let generation = Generation::new(get(bytes, 8)).map_err(|_| RetentionError::Corrupt)?;
    let state = match bytes[6] {
        0 => JobState::Free,
        1 | 2 => {
            let source = TxnRef::decode(&bytes[16..64])?;
            let status = StatusSlotRef::decode(&bytes[80..120])?;
            let lease_lsn = get(bytes, 64);
            if bytes[6] == 1 {
                JobState::Leased {
                    source,
                    status,
                    lease_lsn,
                }
            } else {
                let mut sender_digest = [0; 32];
                sender_digest.copy_from_slice(&bytes[328..360]);
                JobState::Ready {
                    source,
                    status,
                    lease_lsn,
                    handoff_lsn: get(bytes, 72),
                    sender_digest,
                }
            }
        }
        _ => return Err(RetentionError::UnsupportedState),
    };
    let spec = JobSpec { generation, state };
    let mut canonical = [0; JOB_BYTES];
    encode_job(spec, namespace, &mut canonical)?;
    if bytes != &canonical {
        return Err(RetentionError::Corrupt);
    }
    Ok(JobView { bytes, spec })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootSpec {
    pub(crate) aging: Option<SystemWorkRef>,
    pub(crate) revision: u64,
    pub(crate) cursor: u64,
    pub(crate) apply: u64,
    pub(crate) issued: u64,
    pub(crate) free: u64,
    pub(crate) leased: u64,
    pub(crate) ready: u64,
    pub(crate) job_digest: [u8; 32],
    pub(crate) completed: u64,
}
#[derive(Clone, Copy)]
pub(crate) struct RootView<'a> {
    bytes: &'a [u8; ROOT_BYTES],
    spec: RootSpec,
}
impl<'a> RootView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; ROOT_BYTES] {
        self.bytes
    }
    pub(crate) fn spec(self) -> RootSpec {
        self.spec
    }
    pub(crate) fn non_free(self) -> usize {
        (self.spec.leased | self.spec.ready).count_ones() as usize
    }
}

fn initial_jobs_digest(namespace: Namespace) -> Result<[u8; 32]> {
    let mut job = [0; JOB_BYTES];
    encode_job(
        JobSpec {
            generation: Generation::new(1).expect("one"),
            state: JobState::Free,
        },
        namespace,
        &mut job,
    )?;
    let mut hash = ContentHasher::new();
    for _ in 0..JOB_COUNT {
        hash.update(&job);
    }
    Ok(hash.digest())
}

fn validate_root(spec: RootSpec, namespace: Namespace) -> Result<()> {
    if namespace.timeline != 1 || spec.revision == 0 || spec.cursor >= 64 {
        return Err(RetentionError::InvalidInput);
    }
    if spec.free & spec.leased != 0
        || spec.free & spec.ready != 0
        || spec.leased & spec.ready != 0
        || spec.free | spec.leased | spec.ready != u64::MAX
        || spec.leased.count_ones() > 32
        || (spec.leased | spec.ready).count_ones() > 55
    {
        return Err(RetentionError::Corrupt);
    }
    match spec.aging {
        None => {
            if spec.revision != 1
                || spec.cursor != 0
                || spec.apply != 0
                || spec.issued != 0
                || spec.free != u64::MAX
                || spec.completed != 0
                || spec.job_digest != initial_jobs_digest(namespace)?
            {
                return Err(RetentionError::Corrupt);
            }
        }
        Some(aging) => {
            if aging.namespace() != namespace
                || aging.kind() != SystemWorkKind::Aging
                || aging.id() != 1
                || aging.generation().get() != 1
            {
                return Err(RetentionError::IdentityMismatch);
            }
            let outstanding = (spec.leased | spec.ready).count_ones() as u64;
            let revision = spec
                .issued
                .checked_sub(spec.leased.count_ones() as u64)
                .and_then(|value| value.checked_add(spec.issued))
                .and_then(|value| value.checked_add(spec.completed))
                .and_then(|value| value.checked_add(2));
            if spec.apply == 0
                || spec.completed.checked_add(outstanding) != Some(spec.issued)
                || revision != Some(spec.revision)
            {
                return Err(RetentionError::OrderMismatch);
            }
        }
    }
    Ok(())
}

pub(crate) fn encode_root(spec: RootSpec, namespace: Namespace, output: &mut [u8]) -> Result<()> {
    validate_root(spec, namespace)?;
    if output.len() != ROOT_BYTES {
        return Err(RetentionError::InvalidInput);
    }
    let mut bytes = [0; ROOT_BYTES];
    bytes[..4].copy_from_slice(b"XQR1");
    bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
    bytes[8..12].copy_from_slice(&512u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&64u32.to_le_bytes());
    if let Some(aging) = spec.aging {
        bytes[6] = 1;
        aging
            .encode(&mut bytes[16..64])
            .map_err(|_| RetentionError::IdentityMismatch)?;
    }
    for (at, value) in [
        (64, spec.revision),
        (72, spec.cursor),
        (80, spec.apply),
        (88, spec.issued),
        (96, spec.free),
        (104, spec.leased),
        (112, spec.ready),
        (208, namespace.timeline),
        (216, 4),
        (224, 1),
        (376, spec.completed),
    ] {
        put(&mut bytes, at, value);
    }
    bytes[128..160].copy_from_slice(&spec.job_digest);
    let rows = cleanup_rows((spec.leased | spec.ready).count_ones() as usize)?;
    bytes[160..192].copy_from_slice(&budget_digest(&rows));
    bytes[192..208].copy_from_slice(&namespace.database.bytes());
    header(namespace, RetentionGenesisPage::Root, 0)?
        .identity
        .page
        .encode(&mut bytes[232..256])?;
    for index in 0..5 {
        header(namespace, RetentionGenesisPage::Jobs { index }, 0)?
            .identity
            .page
            .encode(&mut bytes[256 + usize::from(index) * 24..280 + usize::from(index) * 24])?;
    }
    output.copy_from_slice(&bytes);
    Ok(())
}

pub(crate) fn decode_root(input: &[u8], namespace: Namespace) -> Result<RootView<'_>> {
    let bytes: &[u8; ROOT_BYTES] = input.try_into().map_err(|_| RetentionError::Corrupt)?;
    if &bytes[..4] != b"XQR1" {
        return Err(RetentionError::Corrupt);
    }
    if bytes[4..6] != [1, 0] {
        return Err(RetentionError::UnsupportedFormat);
    }
    let aging = match bytes[6] {
        0 => None,
        1 => Some(
            SystemWorkRef::decode(&bytes[16..64]).map_err(|_| RetentionError::IdentityMismatch)?,
        ),
        _ => return Err(RetentionError::UnsupportedState),
    };
    let mut job_digest = [0; 32];
    job_digest.copy_from_slice(&bytes[128..160]);
    let spec = RootSpec {
        aging,
        revision: get(bytes, 64),
        cursor: get(bytes, 72),
        apply: get(bytes, 80),
        issued: get(bytes, 88),
        free: get(bytes, 96),
        leased: get(bytes, 104),
        ready: get(bytes, 112),
        job_digest,
        completed: get(bytes, 376),
    };
    let mut canonical = [0; ROOT_BYTES];
    encode_root(spec, namespace, &mut canonical)?;
    if bytes != &canonical {
        return Err(RetentionError::Corrupt);
    }
    Ok(RootView { bytes, spec })
}

pub(crate) fn decode_root_page(
    input: &[u8],
    namespace: Namespace,
) -> Result<(PageHeader, RootView<'_>)> {
    let expected = header(namespace, RetentionGenesisPage::Root, 0)?;
    let page = page::decode_page(input, expected.identity, PageKind::RetentionRoot)?;
    if page.len() != 1 || page.header().level != 0 {
        return Err(RetentionError::Corrupt);
    }
    let root = decode_root(page.cell(0).ok_or(RetentionError::Corrupt)?, namespace)?;
    if page.header().lsn.0 != root.spec.apply {
        return Err(RetentionError::OrderMismatch);
    }
    Ok((page.header(), root))
}

pub(crate) fn encode_root_page(
    spec: RootSpec,
    namespace: Namespace,
    output: &mut [u8],
) -> Result<()> {
    let mut cell = [0; ROOT_BYTES];
    encode_root(spec, namespace, &mut cell)?;
    page::encode_page(
        header(namespace, RetentionGenesisPage::Root, spec.apply)?,
        &[&cell],
        output,
    )?;
    Ok(())
}

pub(crate) fn decode_jobs_page(
    input: &[u8],
    namespace: Namespace,
    index: u8,
) -> Result<PageView<'_>> {
    let expected = header(namespace, RetentionGenesisPage::Jobs { index }, 0)?;
    let page = page::decode_page(input, expected.identity, PageKind::RetentionJobs)?;
    if page.header().level != 0 || page.len() != if index == 4 { 4 } else { 15 } {
        return Err(RetentionError::Corrupt);
    }
    for slot in 0..page.len() {
        let job = decode_job(page.cell(slot).ok_or(RetentionError::Corrupt)?, namespace)?;
        if job.last_step() > page.header().lsn.0 {
            return Err(RetentionError::OrderMismatch);
        }
        if page.header().lsn.0 == 0
            && (job.spec.state != JobState::Free || job.spec.generation.get() != 1)
        {
            return Err(RetentionError::OrderMismatch);
        }
    }
    Ok(page)
}

pub(crate) fn encode_jobs_page(
    namespace: Namespace,
    index: u8,
    lsn: u64,
    cells: &[&[u8]],
    output: &mut [u8],
) -> Result<()> {
    let expected = header(namespace, RetentionGenesisPage::Jobs { index }, lsn)?;
    if cells.len() != if index == 4 { 4 } else { 15 } || output.len() != PAGE_BYTES {
        return Err(RetentionError::InvalidInput);
    }
    for cell in cells {
        let job = decode_job(cell, namespace)?;
        if job.last_step() > lsn
            || (lsn == 0 && (job.spec.state != JobState::Free || job.spec.generation.get() != 1))
        {
            return Err(RetentionError::OrderMismatch);
        }
    }
    page::encode_page(expected, cells, output)?;
    Ok(())
}

/// Ordered full64-cell hash, masks and unique source/Status bindings. The caller
/// supplies stable borrowed cells under its revision/gate and separately proves
/// the bidirectional binding to actual Status pages and ATT descriptors.
pub(crate) fn verify_census(
    root: RootView<'_>,
    namespace: Namespace,
    cells: &[[u8; JOB_BYTES]; JOB_COUNT],
) -> Result<()> {
    let mut hash = ContentHasher::new();
    let mut masks = [0u64; 3];
    for (ordinal, cell) in cells.iter().enumerate() {
        let job = decode_job(cell, namespace)?;
        let (state, selected) = match job.spec.state {
            JobState::Free => (0, None),
            JobState::Leased { source, status, .. } => (1, Some((source, status))),
            JobState::Ready { source, status, .. } => (2, Some((source, status))),
        };
        masks[state] |= 1u64 << ordinal;
        if job.last_step() > root.spec.apply {
            return Err(RetentionError::OrderMismatch);
        }
        if root.spec.aging.is_none() && job.spec.generation.get() != 1 {
            return Err(RetentionError::Corrupt);
        }
        if let Some((source, status)) = selected {
            for previous in &cells[..ordinal] {
                let other = decode_job(previous, namespace)?.spec().state;
                if let JobState::Leased {
                    source: other_source,
                    status: other_status,
                    ..
                }
                | JobState::Ready {
                    source: other_source,
                    status: other_status,
                    ..
                } = other
                    && (source.id() == other_source.id()
                        || status.page().file == other_status.page().file
                            && status.page().page_no == other_status.page().page_no)
                {
                    return Err(RetentionError::IdentityMismatch);
                }
            }
        }
        hash.update(cell);
    }
    if masks != [root.spec.free, root.spec.leased, root.spec.ready]
        || hash.digest() != root.spec.job_digest
    {
        return Err(RetentionError::Corrupt);
    }
    Ok(())
}

#[cfg(test)]
#[path = "retention_page_tests.rs"]
pub(in crate::storage) mod tests;
