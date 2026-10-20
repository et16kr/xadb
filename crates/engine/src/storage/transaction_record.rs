//! Transaction identities and status cells without transaction or I/O effects.
//! Wire contract: `devdoc/contracts/dev3-transaction-format.md`.
// Remove when the first status directory or versioned row consumer is connected.
#![allow(dead_code)]

use super::{
    page::{
        DIGEST_OFFSET, HEADER_BYTES, PageError, PageIdentity, PageKind, PageRef, PageView,
        decode_page,
    },
    undo_ref::UndoRef,
};
use crate::{
    api::{DatabaseId, Namespace},
    transaction::TxnId,
};
use xadb_foundation::identity::{CommitSeq, Generation};

pub(crate) const TXN_ID_BYTES: usize = 40;
pub(crate) const TXN_REF_BYTES: usize = 48;
pub(crate) const STATUS_SLOT_REF_BYTES: usize = 40;
pub(crate) const LOGICAL_ROW_VERSION_BYTES: usize = 48;
pub(crate) const VERSION_STAMP_BYTES: usize = 56;
pub(crate) const STATUS_CELL_BYTES: usize = 192;
pub(crate) const MAX_STATUS_SLOTS: usize = (DIGEST_OFFSET - HEADER_BYTES) / (STATUS_CELL_BYTES + 4);

fn valid_txn_id(id: TxnId) -> bool {
    id.timeline != 0 && id.writer_epoch != 0 && id.local != 0
}

fn read_u64(input: &[u8], offset: usize) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&input[offset..offset + 8]);
    u64::from_le_bytes(bytes)
}

fn put_u64(output: &mut [u8], offset: usize, value: u64) {
    output[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn zero(input: &[u8]) -> bool {
    input.iter().all(|byte| *byte == 0)
}

/// The existing public value can contain invalid counters. Validate before write.
pub(crate) fn encode_txn_id(id: TxnId, output: &mut [u8]) -> Result<(), PageError> {
    if output.len() != TXN_ID_BYTES || !valid_txn_id(id) {
        return Err(PageError::InvalidInput);
    }
    output[..16].copy_from_slice(&id.database.bytes());
    put_u64(output, 16, id.timeline);
    output[24..28].copy_from_slice(&id.writer_epoch.to_le_bytes());
    output[28..32].fill(0);
    put_u64(output, 32, id.local);
    Ok(())
}

pub(crate) fn decode_txn_id(input: &[u8]) -> Result<TxnId, PageError> {
    if input.len() != TXN_ID_BYTES || !zero(&input[28..32]) {
        return Err(PageError::Corrupt);
    }
    let mut database = [0; 16];
    database.copy_from_slice(&input[..16]);
    let mut epoch = [0; 4];
    epoch.copy_from_slice(&input[24..28]);
    let id = TxnId {
        database: DatabaseId::new(database).map_err(|_| PageError::Corrupt)?,
        timeline: read_u64(input, 16),
        writer_epoch: u32::from_le_bytes(epoch),
        local: read_u64(input, 32),
    };
    if !valid_txn_id(id) {
        return Err(PageError::Corrupt);
    }
    Ok(id)
}

/// Logical identity. A bounded persistent directory must resolve its location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TxnRef {
    id: TxnId,
    generation: Generation,
}

impl TxnRef {
    pub(crate) fn new(id: TxnId, generation: Generation) -> Result<Self, PageError> {
        if !valid_txn_id(id) {
            return Err(PageError::InvalidInput);
        }
        Ok(Self { id, generation })
    }

    pub(crate) fn id(self) -> TxnId {
        self.id
    }

    pub(crate) fn generation(self) -> Generation {
        self.generation
    }

    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != TXN_REF_BYTES {
            return Err(PageError::InvalidInput);
        }
        let mut bytes = [0; TXN_REF_BYTES];
        encode_txn_id(self.id, &mut bytes[..TXN_ID_BYTES])?;
        put_u64(&mut bytes, 40, self.generation.get());
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != TXN_REF_BYTES {
            return Err(PageError::Corrupt);
        }
        let id = decode_txn_id(&input[..TXN_ID_BYTES])?;
        let generation = Generation::new(read_u64(input, 40)).map_err(|_| PageError::Corrupt)?;
        Self::new(id, generation).map_err(|_| PageError::Corrupt)
    }
}

/// Physical status location, deliberately distinct from UndoRef and TxnRef.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatusSlotRef {
    page: PageRef,
    slot: u16,
    generation: Generation,
}

impl StatusSlotRef {
    pub(crate) fn new(page: PageRef, slot: u16, generation: Generation) -> Result<Self, PageError> {
        if usize::from(slot) >= MAX_STATUS_SLOTS {
            return Err(PageError::InvalidInput);
        }
        Ok(Self {
            page,
            slot,
            generation,
        })
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

    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != STATUS_SLOT_REF_BYTES {
            return Err(PageError::InvalidInput);
        }
        let mut bytes = [0; STATUS_SLOT_REF_BYTES];
        self.page.encode(&mut bytes[..24])?;
        bytes[24..26].copy_from_slice(&self.slot.to_le_bytes());
        put_u64(&mut bytes, 32, self.generation.get());
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != STATUS_SLOT_REF_BYTES || !zero(&input[26..32]) {
            return Err(PageError::Corrupt);
        }
        let page = PageRef::decode(&input[..24])?;
        let slot = u16::from_le_bytes([input[24], input[25]]);
        let generation = Generation::new(read_u64(input, 32)).map_err(|_| PageError::Corrupt)?;
        Self::new(page, slot, generation).map_err(|_| PageError::Corrupt)
    }
}

/// A user mutation's identity, preserved by movement/stamping and restored by undo.
/// This value does not issue counters or grant an observed-row capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LogicalRowVersion {
    transaction: TxnId,
    mutation_sequence: u64,
}

impl LogicalRowVersion {
    pub(crate) fn new(transaction: TxnId, mutation_sequence: u64) -> Result<Self, PageError> {
        if !valid_txn_id(transaction) || mutation_sequence == 0 {
            return Err(PageError::InvalidInput);
        }
        Ok(Self {
            transaction,
            mutation_sequence,
        })
    }

    pub(crate) fn transaction(self) -> TxnId {
        self.transaction
    }

    pub(crate) fn mutation_sequence(self) -> u64 {
        self.mutation_sequence
    }

    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != LOGICAL_ROW_VERSION_BYTES {
            return Err(PageError::InvalidInput);
        }
        let mut bytes = [0; LOGICAL_ROW_VERSION_BYTES];
        encode_txn_id(self.transaction, &mut bytes[..TXN_ID_BYTES])?;
        put_u64(&mut bytes, 40, self.mutation_sequence);
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != LOGICAL_ROW_VERSION_BYTES {
            return Err(PageError::Corrupt);
        }
        let transaction = decode_txn_id(&input[..TXN_ID_BYTES])?;
        Self::new(transaction, read_u64(input, 40)).map_err(|_| PageError::Corrupt)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VersionStamp {
    Committed(CommitSeq),
    Transaction(TxnRef),
}

impl VersionStamp {
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != VERSION_STAMP_BYTES {
            return Err(PageError::InvalidInput);
        }
        let mut bytes = [0; VERSION_STAMP_BYTES];
        match self {
            Self::Committed(q) => {
                bytes[0] = 1;
                put_u64(&mut bytes, 8, q.get());
            }
            Self::Transaction(reference) => {
                bytes[0] = 2;
                reference.encode(&mut bytes[8..])?;
            }
        }
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != VERSION_STAMP_BYTES || !zero(&input[1..8]) {
            return Err(PageError::Corrupt);
        }
        match input[0] {
            1 if zero(&input[16..]) => {
                let q = CommitSeq::new(read_u64(input, 8)).map_err(|_| PageError::Corrupt)?;
                Ok(Self::Committed(q))
            }
            2 => Ok(Self::Transaction(TxnRef::decode(&input[8..])?)),
            _ => Err(PageError::Corrupt),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RollbackState {
    Statement {
        target: Option<UndoRef>,
        next: Option<UndoRef>,
    },
    Transaction {
        next: Option<UndoRef>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransactionState {
    Active,
    Aborting(RollbackState),
    Committed(CommitSeq),
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TransactionStatus {
    reference: TxnRef,
    state: TransactionState,
    undo_head: Option<UndoRef>,
}

impl TransactionStatus {
    pub(crate) fn new(
        reference: TxnRef,
        state: TransactionState,
        undo_head: Option<UndoRef>,
    ) -> Result<Self, PageError> {
        let invalid = match state {
            TransactionState::Aborting(RollbackState::Statement { target, next }) => {
                (target.is_some() && next.is_none())
                    || (undo_head.is_none() && (target.is_some() || next.is_some()))
            }
            TransactionState::Aborting(RollbackState::Transaction { next }) => {
                undo_head.is_none() && next.is_some()
            }
            _ => false,
        };
        if invalid {
            return Err(PageError::InvalidInput);
        }
        Ok(Self {
            reference,
            state,
            undo_head,
        })
    }

    pub(crate) fn reference(self) -> TxnRef {
        self.reference
    }

    pub(crate) fn state(self) -> TransactionState {
        self.state
    }

    pub(crate) fn undo_head(self) -> Option<UndoRef> {
        self.undo_head
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransactionStatusCell {
    Free(Generation),
    Occupied(TransactionStatus),
}

impl TransactionStatusCell {
    /// Retain the previous generation in Free; this does not authorize reuse.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != STATUS_CELL_BYTES {
            return Err(PageError::InvalidInput);
        }
        let mut bytes = [0; STATUS_CELL_BYTES];
        bytes[..4].copy_from_slice(b"XTS1");
        bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
        match self {
            Self::Free(generation) => put_u64(&mut bytes, 48, generation.get()),
            Self::Occupied(status) => {
                status.reference.encode(&mut bytes[8..56])?;
                let (target, next) = match status.state {
                    TransactionState::Active => {
                        bytes[6] = 1;
                        (None, None)
                    }
                    TransactionState::Aborting(rollback) => {
                        bytes[6] = 2;
                        match rollback {
                            RollbackState::Statement { target, next } => {
                                bytes[7] = 1;
                                (target, next)
                            }
                            RollbackState::Transaction { next } => {
                                bytes[7] = 2;
                                (None, next)
                            }
                        }
                    }
                    TransactionState::Committed(q) => {
                        bytes[6] = 3;
                        put_u64(&mut bytes, 56, q.get());
                        (None, None)
                    }
                    TransactionState::Aborted => {
                        bytes[6] = 4;
                        (None, None)
                    }
                };
                for (index, reference) in [status.undo_head, target, next].into_iter().enumerate() {
                    if let Some(reference) = reference {
                        bytes[64] |= 1 << index;
                        reference.encode(&mut bytes[72 + index * 40..112 + index * 40])?;
                    }
                }
            }
        }
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != STATUS_CELL_BYTES || &input[..4] != b"XTS1" {
            return Err(PageError::Corrupt);
        }
        if input[4..6] != 1_u16.to_le_bytes() {
            return Err(PageError::UnsupportedFormat);
        }
        let generation = Generation::new(read_u64(input, 48)).map_err(|_| PageError::Corrupt)?;
        if input[6] == 0 {
            if input[7] != 0 || !zero(&input[8..48]) || !zero(&input[56..]) {
                return Err(PageError::Corrupt);
            }
            return Ok(Self::Free(generation));
        }
        if input[64] & !7 != 0 || !zero(&input[65..72]) {
            return Err(PageError::Corrupt);
        }
        let reference = TxnRef::decode(&input[8..56])?;
        let mut refs = [None; 3];
        for (index, reference) in refs.iter_mut().enumerate() {
            let bytes = &input[72 + index * 40..112 + index * 40];
            if input[64] & (1 << index) != 0 {
                *reference = Some(UndoRef::decode(bytes)?);
            } else if !zero(bytes) {
                return Err(PageError::Corrupt);
            }
        }
        let [head, target, next] = refs;
        let q = read_u64(input, 56);
        let state = match input[6] {
            1 | 4 if q == 0 && input[7] == 0 && target.is_none() && next.is_none() => {
                if input[6] == 1 {
                    TransactionState::Active
                } else {
                    TransactionState::Aborted
                }
            }
            2 if q == 0 => TransactionState::Aborting(match input[7] {
                1 => RollbackState::Statement { target, next },
                2 if target.is_none() => RollbackState::Transaction { next },
                _ => return Err(PageError::Corrupt),
            }),
            3 if input[7] == 0 && target.is_none() && next.is_none() => {
                TransactionState::Committed(CommitSeq::new(q).map_err(|_| PageError::Corrupt)?)
            }
            _ => return Err(PageError::Corrupt),
        };
        let status =
            TransactionStatus::new(reference, state, head).map_err(|_| PageError::Corrupt)?;
        Ok(Self::Occupied(status))
    }
}

/// Borrowed bytes checked locally, without a pin or global directory validation.
pub(crate) struct StatusPage<'a> {
    page: PageView<'a>,
}

impl StatusPage<'_> {
    pub(crate) fn len(&self) -> usize {
        self.page.len()
    }

    pub(crate) fn cell(&self, slot: usize) -> Result<TransactionStatusCell, PageError> {
        let bytes = self.page.cell(slot).ok_or(PageError::IdentityMismatch)?;
        TransactionStatusCell::decode(bytes)
    }

    pub(crate) fn lookup(
        &self,
        expected: TxnRef,
        location: StatusSlotRef,
    ) -> Result<TransactionStatus, PageError> {
        if self.page.header().identity.page != location.page
            || location.generation != expected.generation
        {
            return Err(PageError::IdentityMismatch);
        }
        match self.cell(usize::from(location.slot))? {
            TransactionStatusCell::Occupied(status) if status.reference == expected => Ok(status),
            _ => Err(PageError::IdentityMismatch),
        }
    }
}

/// Validate every cell, including unselected slots. Historical writer epochs are
/// valid within the supplied DB/timeline. No live reference lifetime is acquired.
pub(crate) fn decode_status_page(
    input: &[u8],
    expected: PageIdentity,
    namespace: Namespace,
) -> Result<StatusPage<'_>, PageError> {
    if namespace.timeline == 0 {
        return Err(PageError::InvalidInput);
    }
    if expected.database != namespace.database {
        return Err(PageError::IdentityMismatch);
    }
    let page = decode_page(input, expected, PageKind::TransactionStatus)?;
    if page.len() > MAX_STATUS_SLOTS {
        return Err(PageError::Corrupt);
    }
    let status_page = StatusPage { page };
    for slot in 0..status_page.len() {
        if let TransactionStatusCell::Occupied(status) = status_page.cell(slot)? {
            let id = status.reference.id;
            if id.database != namespace.database || id.timeline != namespace.timeline {
                return Err(PageError::IdentityMismatch);
            }
        }
    }
    Ok(status_page)
}

#[cfg(test)]
#[path = "transaction_record_tests.rs"]
mod tests;
