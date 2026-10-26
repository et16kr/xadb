//! Sealed inputs of the original CREATE producer. Constructors belong to the
//! actual source-validation/Commit/source-latch/semantic-install paths supplied with it.
//! In particular there is no raw-q, raw-reference or raw-LSN constructor.
use super::*;
use std::marker::PhantomData;

pub(in crate::storage) struct SourceFrontier<'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) q: u64,
}
pub(in crate::storage) struct InstalledFrontier<'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) q: u64,
}
impl<'c, 's> InstalledFrontier<'c, 's> {
    pub(in crate::storage::wal_io) fn catalog(
        completed: &super::super::create_pair::CatalogCommitInstalled<'c, 's>,
    ) -> Self {
        let (core, binding, fence) = completed.origin();
        Self {
            origin: Origin {
                core,
                binding,
                fence,
            },
            q: completed.q(),
        }
    }
}

/// The actual Catalog source guard remains borrowed until this input is
/// consumed by atomic reference registration.
pub(in crate::storage) struct CatalogSource<'g, 'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) transaction: Option<TxnRef>,
    pub(super) undo: Option<UndoRef>,
    pub(super) source: PhantomData<&'g ()>,
}
/// Captured from the actual status/undo source latch of this rollback traversal.
pub(in crate::storage) struct RollbackSource<'g, 'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) actor: TxnRef,
    pub(super) current: UndoRef,
    pub(super) source: PhantomData<&'g ()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum RetirementScope {
    MissingPrune = 1,
    MissingClr = 2,
    UndoFree = 3,
    TerminalFree = 4,
}
impl RetirementScope {
    pub(super) fn decode(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MissingPrune),
            2 => Some(Self::MissingClr),
            3 => Some(Self::UndoFree),
            4 => Some(Self::TerminalFree),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReferenceTarget {
    Undo(UndoRef),
    Transaction(TxnRef),
}
impl ReferenceTarget {
    pub(super) fn kind(self) -> u8 {
        match self {
            Self::Undo(_) => 1,
            Self::Transaction(_) => 2,
        }
    }
    pub(super) fn encode(self, output: &mut [u8]) {
        output.fill(0);
        match self {
            Self::Undo(reference) => reference.encode(&mut output[..40]).expect("typed UndoRef"),
            Self::Transaction(reference) => reference.encode(output).expect("typed TxnRef"),
        }
    }
    pub(super) fn decode(kind: u8, input: &[u8]) -> Result<Self, ()> {
        match kind {
            1 if input[40..].iter().all(|v| *v == 0) => UndoRef::decode(&input[..40])
                .map(Self::Undo)
                .map_err(|_| ()),
            2 => TxnRef::decode(input).map(Self::Transaction).map_err(|_| ()),
            _ => Err(()),
        }
    }
}

/// Issued only after latest exact semantic Before, Job/credit, original target
/// identity and the same VersionIdentity have been checked together.
pub(in crate::storage) struct VerifiedRetirement<'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) version: VersionIdentity,
    pub(super) scope: RetirementScope,
    pub(super) target: ReferenceTarget,
    pub(super) actor: TxnRef,
    pub(super) committed_q: Option<u64>,
    pub(super) group_digest: [u8; 32],
}

/// The actual semantic producer binds the original successfully staged frame
/// to this exact retirement. A decoder or raw LSN cannot issue this receipt.
pub(in crate::storage) struct StagedRetirement<'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) generation: u64,
    pub(super) scope: RetirementScope,
    pub(super) target: ReferenceTarget,
    pub(super) group_digest: [u8; 32],
    pub(super) requirement: WalRequirement<'s>,
}

/// The same producer issues this after the complete actual group installation.
/// A page mask, physical durability proof or decoder alone cannot issue it.
pub(in crate::storage) struct AppliedRetirement<'c, 's> {
    pub(super) origin: Origin<'c, 's>,
    pub(super) generation: u64,
    pub(super) scope: RetirementScope,
    pub(super) target: ReferenceTarget,
    pub(super) group_digest: [u8; 32],
    pub(super) requirement: WalRequirement<'s>,
}
