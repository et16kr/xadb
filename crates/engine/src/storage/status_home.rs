//! Persisted status core and incoming count; no edge update or reuse authority.
//! Contract: `devdoc/contracts/dev3-status-home.md`.
#![allow(dead_code)] // Awaiting the status directory and semantic WAL consumer.

use super::{
    page::{DIGEST_OFFSET, HEADER_BYTES, PageError, PageIdentity, PageKind, PageView, decode_page},
    transaction_record::{StatusSlotRef, TransactionStatusCell, TxnRef},
};
use crate::api::Namespace;

pub(crate) const STATUS_HOME_BYTES: usize = 224;
pub(crate) const MAX_HOME_SLOTS: usize = (DIGEST_OFFSET - HEADER_BYTES) / (STATUS_HOME_BYTES + 4);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatusHome {
    core: TransactionStatusCell,
    incoming: u64,
}

impl StatusHome {
    pub(crate) fn new(core: TransactionStatusCell, incoming: u64) -> Result<Self, PageError> {
        if matches!(core, TransactionStatusCell::Free(_)) && incoming != 0 {
            return Err(PageError::InvalidInput);
        }
        Ok(Self { core, incoming })
    }

    pub(crate) fn core(self) -> TransactionStatusCell {
        self.core
    }

    pub(crate) fn incoming(self) -> u64 {
        self.incoming
    }

    /// Validate into fixed scratch before touching caller output.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != STATUS_HOME_BYTES {
            return Err(PageError::InvalidInput);
        }
        let mut bytes = [0; STATUS_HOME_BYTES];
        bytes[..4].copy_from_slice(b"XSH1");
        bytes[4..6].copy_from_slice(&1_u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&32_u16.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.incoming.to_le_bytes());
        self.core.encode(&mut bytes[32..])?;
        output.copy_from_slice(&bytes);
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != STATUS_HOME_BYTES || &input[..4] != b"XSH1" {
            return Err(PageError::Corrupt);
        }
        if input[4..6] != 1_u16.to_le_bytes() {
            return Err(PageError::UnsupportedFormat);
        }
        if input[6..8] != 32_u16.to_le_bytes() || input[16..32].iter().any(|byte| *byte != 0) {
            return Err(PageError::Corrupt);
        }
        let mut count = [0; 8];
        count.copy_from_slice(&input[8..16]);
        let core = TransactionStatusCell::decode(&input[32..])?;
        Self::new(core, u64::from_le_bytes(count)).map_err(|_| PageError::Corrupt)
    }
}

/// Removed edges must already exist even when the atomic net change is zero.
/// This computes a value only; target stability and actual graph proof are external.
pub(crate) fn adjust_incoming(old: u64, removed: u64, added: u64) -> Result<u64, PageError> {
    old.checked_sub(removed)
        .and_then(|remaining| remaining.checked_add(added))
        .ok_or(PageError::InvalidInput)
}

/// Every home is validated before this view is exposed. No lifetime pin acquired.
pub(crate) struct StatusHomePage<'a> {
    page: PageView<'a>,
}

impl StatusHomePage<'_> {
    pub(crate) fn len(&self) -> usize {
        self.page.len()
    }

    pub(crate) fn cell(&self, slot: usize) -> Result<StatusHome, PageError> {
        let bytes = self.page.cell(slot).ok_or(PageError::IdentityMismatch)?;
        StatusHome::decode(bytes)
    }

    pub(crate) fn lookup(
        &self,
        expected: TxnRef,
        location: StatusSlotRef,
    ) -> Result<StatusHome, PageError> {
        if self.page.header().identity.page != location.page()
            || location.generation() != expected.generation()
            || usize::from(location.slot()) >= MAX_HOME_SLOTS
        {
            return Err(PageError::IdentityMismatch);
        }
        let home = self.cell(usize::from(location.slot()))?;
        match home.core {
            TransactionStatusCell::Occupied(status) if status.reference() == expected => Ok(home),
            _ => Err(PageError::IdentityMismatch),
        }
    }
}

pub(crate) fn decode_status_home_page(
    input: &[u8],
    expected: PageIdentity,
    namespace: Namespace,
) -> Result<StatusHomePage<'_>, PageError> {
    if namespace.timeline == 0 {
        return Err(PageError::InvalidInput);
    }
    if expected.database != namespace.database {
        return Err(PageError::IdentityMismatch);
    }
    let page = decode_page(input, expected, PageKind::TransactionStatus)?;
    if page.len() > MAX_HOME_SLOTS {
        return Err(PageError::Corrupt);
    }
    let homes = StatusHomePage { page };
    for slot in 0..homes.len() {
        if let TransactionStatusCell::Occupied(status) = homes.cell(slot)?.core {
            let id = status.reference().id();
            if id.database != namespace.database || id.timeline != namespace.timeline {
                return Err(PageError::IdentityMismatch);
            }
            for previous in 0..slot {
                if let TransactionStatusCell::Occupied(other) = homes.cell(previous)?.core
                    && other.reference().id() == id
                {
                    return Err(PageError::Corrupt);
                }
            }
        }
    }
    Ok(homes)
}

#[cfg(test)]
#[path = "status_home_tests.rs"]
mod tests;
