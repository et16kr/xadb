//! Strict operation core shape; actual references and projection digests are
//! subsequently compared with stable native before/after state.
use super::{
    Error, Operation, Result, format::CatalogJob, put16, put32, put64, u16_at, u32_at, u64_at, zero,
};
use crate::storage::undo_ref::UndoRef;

pub(super) struct CoreInput<J> {
    pub(super) operation: Operation,
    /// Reserved after-image for Reserve; the stable before Job otherwise.
    pub(super) root: Option<J>,
    pub(super) q: u64,
    pub(super) cursors: (u64, u64),
    pub(super) projection: Option<([u8; 32], [u8; 32])>,
}

pub(super) fn encode_core<'a, J: CatalogJob<'a>>(
    input: CoreInput<J>,
    output: &mut [u8; 256],
) -> Result<()> {
    let operation = input.operation;
    output.fill(0);
    put16(output, 0, operation as u16);
    put32(output, 4, 256);
    let planned = matches!(
        operation,
        Operation::Reserve | Operation::Forward | Operation::Cancel | Operation::Clr
    );
    let root_required = planned
        || matches!(
            operation,
            Operation::Prune | Operation::ReleaseRole | Operation::RootFree
        );
    if root_required || operation == Operation::RollbackStart && input.root.is_some() {
        let job = input.root.ok_or(Error::IdentityMismatch)?;
        let undo = job.undo().ok_or(Error::IdentityMismatch)?;
        undo.encode(&mut output[32..72])?;
        put64(output, 24, job.undo_sequence());
        put16(output, 2, 1);
        if planned {
            put64(output, 8, job.statement());
            put64(output, 16, job.mutation());
        }
        if operation == Operation::RollbackStart {
            undo.encode(&mut output[72..112])?;
            put16(output, 2, 3);
        }
    } else if input.root.is_some() {
        return Err(Error::InvalidInput);
    }
    put64(output, 152, input.q);
    put64(output, 160, input.cursors.0);
    put64(output, 168, input.cursors.1);
    if let Some((before, after)) = input.projection {
        let flags = u16_at(output, 2) | 24;
        put16(output, 2, flags);
        output[176..208].copy_from_slice(&before);
        output[208..240].copy_from_slice(&after);
    }
    decode_core(output, operation)?;
    Ok(())
}

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
    pub(crate) fn sequence(self) -> u64 {
        u64_at(self.bytes, 24)
    }
    pub(crate) fn current(self) -> Option<UndoRef> {
        self.reference(0, 32)
    }
    pub(crate) fn next(self) -> Option<UndoRef> {
        self.reference(1, 72)
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
    let view = CoreView { bytes, operation };
    let planned = matches!(
        operation,
        Operation::Reserve | Operation::Forward | Operation::Cancel | Operation::Clr
    );
    let aging = matches!(
        operation,
        Operation::Prune | Operation::ReleaseRole | Operation::RootFree
    );
    if u64_at(bytes, 8) != u64::from(planned) || u64_at(bytes, 16) != u64::from(planned) {
        return Err(Error::Corrupt);
    }
    let expected_flags = match operation {
        Operation::Forward | Operation::Clr | Operation::Prune => 25,
        Operation::Reserve | Operation::Cancel | Operation::ReleaseRole | Operation::RootFree => 1,
        Operation::RollbackStart if view.current().is_some() => 3,
        _ => 0,
    };
    if flags != expected_flags || (view.sequence() != 0) != (planned || aging || flags == 3) {
        return Err(Error::Corrupt);
    }
    if operation == Operation::RollbackStart && view.current() != view.next() {
        return Err(Error::OrderMismatch);
    }
    if (view.q() != 0) != (operation == Operation::Commit) {
        return Err(Error::Corrupt);
    }
    let cursors = view.cursors();
    if aging || operation == Operation::TerminalFinish {
        if cursors.0 >= 64 || cursors.1 >= 64 {
            return Err(Error::Corrupt);
        }
    } else if cursors != (0, 0) {
        return Err(Error::Corrupt);
    }
    Ok(view)
}
