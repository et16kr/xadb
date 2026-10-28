//! Canonical scope3 scalar grammar. Actual head order and statement marks need
//! the original source observer; a well-formed core is not their authority.
use super::{Error, Operation, Result, u16_at, u32_at, u64_at, zero};
use crate::storage::undo_ref::UndoRef;

#[derive(Clone, Copy)]
pub(crate) struct CoreView<'a> {
    bytes: &'a [u8; 256],
    operation: Operation,
}
impl<'a> CoreView<'a> {
    pub(crate) fn bytes(self) -> &'a [u8; 256] {
        self.bytes
    }
    pub(crate) fn operation(self) -> Operation {
        self.operation
    }
    pub(crate) fn statement(self) -> u64 {
        u64_at(self.bytes, 8)
    }
    pub(crate) fn mutation(self) -> u64 {
        u64_at(self.bytes, 16)
    }
    pub(crate) fn sequence(self) -> u64 {
        u64_at(self.bytes, 24)
    }
    pub(crate) fn current(self) -> Option<UndoRef> {
        self.reference(0, 32)
    }
    pub(crate) fn next(self) -> Option<UndoRef> {
        self.reference(1, 72)
    }
    pub(crate) fn target(self) -> Option<UndoRef> {
        self.reference(2, 112)
    }
    pub(crate) fn q(self) -> u64 {
        u64_at(self.bytes, 152)
    }
    pub(crate) fn cursors(self) -> (u64, u64) {
        (u64_at(self.bytes, 160), u64_at(self.bytes, 168))
    }
    pub(crate) fn before_digest(self) -> Option<&'a [u8; 32]> {
        self.digest(3, 176)
    }
    pub(crate) fn after_digest(self) -> Option<&'a [u8; 32]> {
        self.digest(4, 208)
    }
    fn reference(self, bit: u16, at: usize) -> Option<UndoRef> {
        (u16_at(self.bytes, 2) & (1 << bit) != 0)
            .then(|| UndoRef::decode(&self.bytes[at..at + 40]).expect("validated core"))
    }
    fn digest(self, bit: u16, at: usize) -> Option<&'a [u8; 32]> {
        (u16_at(self.bytes, 2) & (1 << bit) != 0)
            .then(|| self.bytes[at..at + 32].try_into().expect("fixed digest"))
    }
}

pub(crate) fn decode_core(bytes: &[u8], operation: Operation) -> Result<CoreView<'_>> {
    let bytes: &[u8; 256] = bytes.try_into().map_err(|_| Error::Corrupt)?;
    if u16_at(bytes, 0) != operation as u16 || u32_at(bytes, 4) != 256 || !zero(&bytes[240..]) {
        return Err(Error::Corrupt);
    }
    let flags = u16_at(bytes, 2);
    if flags & !31 != 0 {
        return Err(Error::Corrupt);
    }
    for (bit, at) in [(0, 32), (1, 72), (2, 112)] {
        if flags & (1 << bit) != 0 {
            UndoRef::decode(&bytes[at..at + 40])?;
        } else if !zero(&bytes[at..at + 40]) {
            return Err(Error::Corrupt);
        }
    }
    for (bit, at) in [(3, 176), (4, 208)] {
        if flags & (1 << bit) == 0 && !zero(&bytes[at..at + 32]) {
            return Err(Error::Corrupt);
        }
    }
    let v = CoreView { bytes, operation };
    let planned = matches!(
        operation,
        Operation::Reserve | Operation::Forward | Operation::Cancel | Operation::Clr
    );
    if planned {
        if v.statement() == 0 || v.mutation() == 0 {
            return Err(Error::Corrupt);
        }
    } else if v.mutation() != 0
        || match operation {
            Operation::RollbackStart => false,
            Operation::StatementFinish => v.statement() == 0,
            _ => v.statement() != 0,
        }
    {
        return Err(Error::Corrupt);
    }
    let refs = flags & 7;
    let valid = match operation {
        Operation::Reserve | Operation::Cancel | Operation::ReleaseRole | Operation::RootFree => {
            refs == 1
        }
        Operation::Forward | Operation::Prune => matches!(refs, 1 | 3) && v.current() != v.next(),
        Operation::Clr => {
            matches!(refs, 1 | 3 | 7) && v.current() != v.next() && v.current() != v.target()
        }
        Operation::RollbackStart => {
            matches!(refs, 0 | 3 | 7)
                && v.current() == v.next()
                && (v.statement() != 0 || v.target().is_none())
        }
        Operation::StatementFinish => {
            matches!(refs, 0 | 7) && v.current() == v.next() && v.next() == v.target()
        }
        _ => refs == 0,
    };
    if !valid || (v.sequence() != 0) != v.current().is_some() {
        return Err(Error::OrderMismatch);
    }
    let projection = matches!(
        operation,
        Operation::Forward | Operation::Clr | Operation::Prune
    );
    if flags & 24 != if projection { 24 } else { 0 }
        || (v.q() != 0) != (operation == Operation::Commit)
    {
        return Err(Error::Corrupt);
    }
    let cursors = v.cursors();
    if matches!(
        operation,
        Operation::Prune | Operation::ReleaseRole | Operation::RootFree | Operation::TerminalFinish
    ) {
        if cursors.0 >= 64 || cursors.1 >= 64 {
            return Err(Error::Corrupt);
        }
    } else if cursors != (0, 0) {
        return Err(Error::Corrupt);
    }
    Ok(v)
}
