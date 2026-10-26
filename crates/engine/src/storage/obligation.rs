//! Fixed recovery obligation descriptors, without reservation or replay effects.
//! Contract: `devdoc/contracts/dev3-obligation-format.md`, section 12.
#![allow(dead_code)] // Awaiting inventory closure and semantic WAL consumers.

use super::{
    allocation_page::{ReservationOwner, SystemWorkKind, SystemWorkRef},
    control::{ControlKind, ControlRef, EXTENT_BYTES},
    transaction_record::{TransactionState, TransactionStatus, TransactionStatusCell, TxnRef},
};
use foundation::codec::content_digest;

pub(crate) const OBLIGATION_BYTES: usize = 1536;
const DIGEST_OFFSET: usize = 1504;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObligationError {
    InvalidInput,
    Corrupt,
    UnsupportedFormat,
    UnsupportedWorkBody,
    IdentityMismatch,
}
type Result<T> = std::result::Result<T, ObligationError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum WorkPhase {
    Prepared = 1,
    Running = 2,
    Finishing = 3,
    Finished = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemProgress {
    pub(crate) reference: SystemWorkRef,
    pub(crate) phase: WorkPhase,
    pub(crate) sequence: u64,
    pub(crate) cursor: u64,
    pub(crate) limit: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnerState {
    Transaction(TransactionStatus),
    System(SystemProgress),
}
impl OwnerState {
    pub(crate) fn reference(self) -> ReservationOwner {
        match self {
            Self::Transaction(status) => ReservationOwner::Transaction(status.reference()),
            Self::System(progress) => ReservationOwner::System(progress.reference),
        }
    }

    fn tag(self) -> u8 {
        match self {
            Self::Transaction(status) => match status.state() {
                TransactionState::Active => 1,
                TransactionState::Aborting(_) => 2,
                TransactionState::Committed(_) => 3,
                TransactionState::Aborted => 4,
            },
            Self::System(progress) if progress.phase == WorkPhase::Finished => 6,
            Self::System(_) => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ResourcePeak {
    pub(crate) frames: u64,
    pub(crate) memory: u64,
    pub(crate) locks: u64,
    pub(crate) registry: u64,
    pub(crate) files: u64,
    pub(crate) doublewrite: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RemainingBudget {
    pub(crate) steps: u64,
    pub(crate) wal: u64,
    pub(crate) control: u64,
    pub(crate) growth: u64,
    pub(crate) pages: u64,
    pub(crate) undo_slots: u64,
    pub(crate) status_slots: u64,
    pub(crate) peak: ResourcePeak,
    pub(crate) protected: u64,
}
impl RemainingBudget {
    fn values(self) -> [u64; 14] {
        [
            self.steps,
            self.wal,
            self.control,
            self.growth,
            self.pages,
            self.undo_slots,
            self.status_slots,
            self.peak.frames,
            self.peak.memory,
            self.peak.locks,
            self.peak.registry,
            self.peak.files,
            self.peak.doublewrite,
            self.protected,
        ]
    }
    fn decode(input: &[u8]) -> Self {
        Self {
            steps: get(input, 8),
            wal: get(input, 16),
            control: get(input, 24),
            growth: get(input, 32),
            pages: get(input, 40),
            undo_slots: get(input, 48),
            status_slots: get(input, 56),
            peak: ResourcePeak {
                frames: get(input, 64),
                memory: get(input, 72),
                locks: get(input, 80),
                registry: get(input, 88),
                files: get(input, 96),
                doublewrite: get(input, 104),
            },
            protected: get(input, 112),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerChain {
    pub(crate) apply: u64,
    pub(crate) previous: u64,
    pub(crate) birth: u64,
    pub(crate) groups: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InventoryClaim {
    pub(crate) change_lsn: u64,
    pub(crate) items: u64,
    pub(crate) digest: [u8; 32],
    pub(crate) growth_extents: u64,
}

/// Unvalidated input values. Encoding always rechecks every field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DescriptorSpec {
    pub(crate) owner: OwnerState,
    pub(crate) chain: OwnerChain,
    pub(crate) profile: ControlRef,
    pub(crate) inventory: InventoryClaim,
    pub(crate) consumed_wal: u64,
    pub(crate) consumed_control: u64,
    /// Fixed category order: CLR, structural, terminal, status, control.
    pub(crate) remaining: [RemainingBudget; 5],
    pub(crate) minimum_wal_lsn: u64,
    pub(crate) last_mutation_sequence: u64,
}

struct Summary {
    growth: u64,
    peaks: [u64; 6],
    protected: u64,
}

/// Exact private validation return carriers for source-value accounting.
/// Caller scratch and encoded descriptor bytes are charged at their owner.
pub(crate) fn source_summary_carrier_bytes() -> (usize, usize) {
    (
        std::mem::size_of::<Summary>(),
        std::mem::size_of::<Result<Summary>>(),
    )
}

fn valid_profile(profile: ControlRef) -> Result<()> {
    if profile.kind != ControlKind::Profile || profile.encode(&mut [0; 64]).is_err() {
        return Err(ObligationError::InvalidInput);
    }
    Ok(())
}

fn validate(spec: &DescriptorSpec) -> Result<Summary> {
    valid_profile(spec.profile)?;
    let c = spec.chain;
    let first = c.previous == 0;
    if c.apply == 0
        || c.birth == 0
        || (first && (c.birth != c.apply || c.groups != 1))
        || (!first && (c.birth > c.previous || c.previous >= c.apply || c.groups < 2))
        || spec.inventory.change_lsn > c.apply
        || (spec.inventory.items > 0 && spec.inventory.change_lsn == 0)
        || (spec.inventory.items == 0 && spec.inventory.digest != content_digest(&[]))
        || spec.minimum_wal_lsn > c.apply
    {
        return Err(ObligationError::InvalidInput);
    }
    if let OwnerState::System(progress) = spec.owner
        && (progress.sequence == 0
            || progress.cursor > progress.limit
            || (progress.phase == WorkPhase::Finished && progress.cursor != progress.limit)
            || spec.last_mutation_sequence != 0)
    {
        return Err(ObligationError::InvalidInput);
    }
    let mut totals = [0_u64; 6];
    let mut peaks = [0; 6];
    let mut protected = 0_u64;
    let mut growth_categories = 0_u128;
    for row in spec.remaining {
        let values = row.values();
        if (row.steps == 0 && values.iter().any(|value| *value != 0))
            || !row.growth.is_multiple_of(EXTENT_BYTES)
        {
            return Err(ObligationError::InvalidInput);
        }
        for (total, value) in totals.iter_mut().zip(&values[1..7]) {
            *total = total
                .checked_add(*value)
                .ok_or(ObligationError::InvalidInput)?;
        }
        for (peak, value) in peaks.iter_mut().zip(&values[7..13]) {
            *peak = (*peak).max(*value);
        }
        protected = protected
            .checked_add(row.protected)
            .ok_or(ObligationError::InvalidInput)?;
        growth_categories += u128::from(row.growth > 0);
    }
    let growth = spec
        .inventory
        .growth_extents
        .checked_mul(EXTENT_BYTES)
        .ok_or(ObligationError::InvalidInput)?;
    let tokens = u128::from(totals[3]) + u128::from(totals[4]) + u128::from(totals[5]);
    let items = u128::from(spec.inventory.items);
    if totals[2] != growth
        || items < tokens + growth_categories
        || items > tokens + u128::from(spec.inventory.growth_extents)
    {
        return Err(ObligationError::InvalidInput);
    }
    Ok(Summary {
        growth,
        peaks,
        protected,
    })
}

/// Recheck a caller-assembled descriptor without allocating an encoding buffer.
pub(crate) fn validate_descriptor(spec: &DescriptorSpec) -> Result<()> {
    validate(spec).map(|_| ())
}

fn get(input: &[u8], offset: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&input[offset..offset + 8]);
    u64::from_le_bytes(value)
}
fn put(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
fn zero(input: &[u8]) -> bool {
    input.iter().all(|byte| *byte == 0)
}

// The two fixed entries expand the same body without another runtime call layer.
macro_rules! descriptor_encoder {
    ($visibility:vis, $name:ident, ($($work:ident)?)) => {
        $visibility fn $name(
            spec: &DescriptorSpec,
            $($work: &[u8; 96],)?
            output: &mut [u8],
        ) -> Result<()> {
            $(
                validate_aging_work(spec.owner, $work)
                    .map_err(|_| ObligationError::InvalidInput)?;
            )?
    if output.len() != OBLIGATION_BYTES {
        return Err(ObligationError::InvalidInput);
    }
    let summary = validate(spec)?;
    let mut bytes = [0; OBLIGATION_BYTES];
    bytes[..4].copy_from_slice(b"XOD1");
    bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
    bytes[6] = spec.owner.tag();
    bytes[8..12].copy_from_slice(&(OBLIGATION_BYTES as u32).to_le_bytes());
    match spec.owner {
        OwnerState::Transaction(status) => {
            bytes[16] = 1;
            status
                .reference()
                .encode(&mut bytes[24..72])
                .map_err(|_| ObligationError::InvalidInput)?;
            TransactionStatusCell::Occupied(status)
                .encode(&mut bytes[160..352])
                .map_err(|_| ObligationError::InvalidInput)?;
        }
        OwnerState::System(progress) => {
            bytes[16] = 2;
            progress
                .reference
                .encode(&mut bytes[24..72])
                .map_err(|_| ObligationError::InvalidInput)?;
            bytes[352] = progress.reference.kind() as u8;
            bytes[353] = progress.phase as u8;
            put(&mut bytes, 360, progress.sequence);
            put(&mut bytes, 368, progress.cursor);
            put(&mut bytes, 376, progress.limit);
            bytes[448..480].copy_from_slice(&content_digest(&[]));
            $(bytes[384..480].copy_from_slice($work);)?
        }
    }
    put(&mut bytes, 72, spec.chain.apply);
    put(&mut bytes, 80, spec.chain.previous);
    put(&mut bytes, 88, spec.chain.birth);
    spec.profile
        .encode(&mut bytes[96..160])
        .map_err(|_| ObligationError::InvalidInput)?;
    put(&mut bytes, 544, spec.inventory.change_lsn);
    put(&mut bytes, 552, spec.inventory.items);
    bytes[560..592].copy_from_slice(&spec.inventory.digest);
    put(&mut bytes, 592, spec.inventory.growth_extents);
    put(&mut bytes, 600, summary.growth);
    put(&mut bytes, 608, spec.consumed_wal);
    put(&mut bytes, 616, spec.consumed_control);
    put(&mut bytes, 624, spec.chain.groups);
    for (index, row) in spec.remaining.iter().enumerate() {
        let base = 640 + index * 128;
        bytes[base] = index as u8 + 1;
        for (field, value) in row.values().iter().enumerate() {
            put(&mut bytes, base + 8 + field * 8, *value);
        }
    }
    for (index, peak) in summary.peaks.iter().enumerate() {
        put(&mut bytes, 1280 + index * 8, *peak);
    }
    put(&mut bytes, 1328, summary.protected);
    put(&mut bytes, 1336, spec.minimum_wal_lsn);
    put(&mut bytes, 1344, spec.last_mutation_sequence);
    let digest = content_digest(&bytes[..DIGEST_OFFSET]);
    bytes[DIGEST_OFFSET..].copy_from_slice(&digest);
    output.copy_from_slice(&bytes);
    Ok(())
}

    };
}
descriptor_encoder!(pub(crate), encode_descriptor, ());
descriptor_encoder!(pub(super), encode_aging_descriptor, (work));

// Fixed local policies expand checks in place. The old entry gains neither
// local values nor a forwarding call, preserving its tight source-value bound.
macro_rules! descriptor_work_prefix {
    (empty, $input:ident) => {
        if !zero(&$input[384..448]) {
            return Err(ObligationError::UnsupportedWorkBody);
        }
        if $input[448..480] != content_digest(&[]) {
            return Err(ObligationError::Corrupt);
        }
    };
    (aging, $input:ident) => {};
}
macro_rules! descriptor_work_owner {
    (empty, $owner:ident, $input:ident) => {};
    (aging, $owner:ident, $input:ident) => {
        validate_aging_work($owner, &$input[384..480])?;
    };
}

/// Successful decoding proves local representation only, not inventory or replay.
macro_rules! descriptor_decoder {
    ($visibility:vis, $name:ident, $policy:ident) => {
        $visibility fn $name(
            input: &[u8],
            expected_owner: ReservationOwner,
            expected_profile: ControlRef,
        ) -> Result<DescriptorSpec> {
    valid_profile(expected_profile)?;
    if input.len() != OBLIGATION_BYTES
        || &input[..4] != b"XOD1"
        || input[8..12] != (OBLIGATION_BYTES as u32).to_le_bytes()
    {
        return Err(ObligationError::Corrupt);
    }
    if input[4..6] != 1_u16.to_le_bytes() {
        return Err(ObligationError::UnsupportedFormat);
    }
    if content_digest(&input[..DIGEST_OFFSET]) != input[DIGEST_OFFSET..]
        || input[7] != 0
        || !zero(&input[12..16])
        || !zero(&input[17..24])
        || !zero(&input[480..544])
        || !zero(&input[632..640])
        || !zero(&input[1352..1504])
    {
        return Err(ObligationError::Corrupt);
    }
    let owner = match input[16] {
        1 => {
            let reference = TxnRef::decode(&input[24..72]).map_err(|_| ObligationError::Corrupt)?;
            let TransactionStatusCell::Occupied(status) =
                TransactionStatusCell::decode(&input[160..352])
                    .map_err(|_| ObligationError::Corrupt)?
            else {
                return Err(ObligationError::Corrupt);
            };
            if reference != status.reference() || !zero(&input[352..480]) {
                return Err(ObligationError::Corrupt);
            }
            OwnerState::Transaction(status)
        }
        2 => {
            let reference =
                SystemWorkRef::decode(&input[24..72]).map_err(|_| ObligationError::Corrupt)?;
            if !zero(&input[160..352])
                || !zero(&input[354..360])
                || input[352] != reference.kind() as u8
            {
                return Err(ObligationError::Corrupt);
            }
            descriptor_work_prefix!($policy, input);
            let phase = match input[353] {
                1 => WorkPhase::Prepared,
                2 => WorkPhase::Running,
                3 => WorkPhase::Finishing,
                4 => WorkPhase::Finished,
                _ => return Err(ObligationError::Corrupt),
            };
            OwnerState::System(SystemProgress {
                reference,
                phase,
                sequence: get(input, 360),
                cursor: get(input, 368),
                limit: get(input, 376),
            })
        }
        _ => return Err(ObligationError::Corrupt),
    };
    if input[6] != owner.tag() {
        return Err(ObligationError::Corrupt);
    }
    descriptor_work_owner!($policy, owner, input);
    let mut remaining = [RemainingBudget::default(); 5];
    for (index, row) in remaining.iter_mut().enumerate() {
        let base = 640 + 128 * index;
        if input[base] != index as u8 + 1
            || !zero(&input[base + 1..base + 8])
            || !zero(&input[base + 120..base + 128])
        {
            return Err(ObligationError::Corrupt);
        }
        *row = RemainingBudget::decode(&input[base..base + 128]);
    }
    let mut inventory_digest = [0; 32];
    inventory_digest.copy_from_slice(&input[560..592]);
    let spec = DescriptorSpec {
        owner,
        chain: OwnerChain {
            apply: get(input, 72),
            previous: get(input, 80),
            birth: get(input, 88),
            groups: get(input, 624),
        },
        profile: ControlRef::decode(&input[96..160]).map_err(|_| ObligationError::Corrupt)?,
        inventory: InventoryClaim {
            change_lsn: get(input, 544),
            items: get(input, 552),
            digest: inventory_digest,
            growth_extents: get(input, 592),
        },
        consumed_wal: get(input, 608),
        consumed_control: get(input, 616),
        remaining,
        minimum_wal_lsn: get(input, 1336),
        last_mutation_sequence: get(input, 1344),
    };
    let summary = validate(&spec).map_err(|_| ObligationError::Corrupt)?;
    if get(input, 600) != summary.growth
        || get(input, 1328) != summary.protected
        || summary
            .peaks
            .iter()
            .enumerate()
            .any(|(index, peak)| get(input, 1280 + 8 * index) != *peak)
    {
        return Err(ObligationError::Corrupt);
    }
    if owner.reference() != expected_owner || spec.profile != expected_profile {
        return Err(ObligationError::IdentityMismatch);
    }
    Ok(spec)
}

    };
}
descriptor_decoder!(pub(crate), decode_descriptor, empty);
descriptor_decoder!(pub(super), decode_aging_descriptor, aging);

// Kind7 is local to the explicit Aging work body. Generic ControlRef decoding
// and control-file selection intentionally remain unchanged.
fn validate_aging_work(owner: OwnerState, work: &[u8]) -> Result<()> {
    let OwnerState::System(progress) = owner else {
        return Err(ObligationError::UnsupportedWorkBody);
    };
    if progress.reference.kind() != SystemWorkKind::Aging
        || progress.phase != WorkPhase::Running
        || progress.limit != 64
        || progress.cursor >= 64
        || work.len() != 96
        || work[0] != 7
        || !zero(&work[1..8])
        || get(work, 8) != progress.sequence
        || work[16..20] != 16384u32.to_le_bytes()
        || !zero(&work[20..32])
    {
        return Err(ObligationError::UnsupportedWorkBody);
    }
    Ok(())
}

/// Shared budget representation only; callers still validate owner accounting.
pub(super) fn encode_budget_rows(rows: &[RemainingBudget; 5], output: &mut [u8; 640]) {
    output.fill(0);
    for (index, row) in rows.iter().enumerate() {
        let base = index * 128;
        output[base] = index as u8 + 1;
        for (field, value) in row.values().iter().enumerate() {
            put(output, base + 8 + field * 8, *value);
        }
    }
}

#[cfg(test)]
#[path = "obligation_tests.rs"]
mod tests;
