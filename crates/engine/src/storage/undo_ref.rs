//! A stable undo slot's wire identity, without loading, pinning or reusing it.
//! See `devdoc/contracts/dev3-reference-format.md`.
// Remove when the first undo page/record consumer connects this expression.
#![allow(dead_code)]

use super::page::{MAX_PAGE_SLOTS, PAGE_REF_BYTES, PageError, PageRef};
use foundation::identity::Generation;

pub(crate) const UNDO_REF_BYTES: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UndoRef {
    page: PageRef,
    slot: u16,
    generation: Generation,
}

impl UndoRef {
    /// Validate the envelope's conservative slot bound. Actual record existence
    /// and its generation must be rechecked while holding its owner guard.
    pub(crate) fn new(page: PageRef, slot: u16, generation: Generation) -> Result<Self, PageError> {
        if usize::from(slot) >= MAX_PAGE_SLOTS {
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

    /// Encode into exactly 40 caller-owned bytes. Errors preserve all output.
    /// No allocation, I/O, pin, or reference-count effect occurs.
    pub(crate) fn encode(self, output: &mut [u8]) -> Result<(), PageError> {
        if output.len() != UNDO_REF_BYTES {
            return Err(PageError::InvalidInput);
        }
        self.page.encode(&mut output[..PAGE_REF_BYTES])?;
        output[24..26].copy_from_slice(&self.slot.to_le_bytes());
        output[26..32].fill(0);
        output[32..40].copy_from_slice(&self.generation.get().to_le_bytes());
        Ok(())
    }

    /// Decode the wire identity. This does not attest that the target is live.
    pub(crate) fn decode(input: &[u8]) -> Result<Self, PageError> {
        if input.len() != UNDO_REF_BYTES || input[26..32].iter().any(|b| *b != 0) {
            return Err(PageError::Corrupt);
        }
        let page = PageRef::decode(&input[..PAGE_REF_BYTES])?;
        let slot = u16::from_le_bytes([input[24], input[25]]);
        let mut generation = [0; 8];
        generation.copy_from_slice(&input[32..40]);
        let generation =
            Generation::new(u64::from_le_bytes(generation)).map_err(|_| PageError::Corrupt)?;
        Self::new(page, slot, generation).map_err(|_| PageError::Corrupt)
    }
}

#[cfg(test)]
#[path = "undo_ref_tests.rs"]
mod tests;
