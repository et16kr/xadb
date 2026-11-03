//! Consuming Row write states exposed without the wrappers' internal lifetimes.
//!
//! C alone implements this protocol. The associated next state is the original
//! wrapper, including its owner, statement, loan, admission and Drop obligation.
//! A still supplies validation and candidate bytes through the borrowed lanes.
//! No factory, allocation, erased lifetime or new effect is introduced here.
use super::*;

mod sealed {
    pub trait State {}
}

/// Checked preflight before the first RowReserve effect.
pub(crate) trait RowPreflightOperation: sealed::State + Sized {
    type Reserved: RowReservedOperation;
    fn validation_lane(&mut self) -> Result<RowValidationLane<'_>>;
    fn reserve(
        self,
        input: RowInputPlan,
    ) -> std::result::Result<Self::Reserved, RowOwnedError<Self>>;
    /// A DELETE preflight's first effect (dev3-row-delete.md#api): the
    /// delete RowReserve of its classified target, with no candidate.
    fn reserve_delete(self) -> std::result::Result<Self::Reserved, RowOwnedError<Self>>;
    /// Checked release of the source/gate/loan. The observed read set stays.
    fn finish(self) -> Result<()>;
}

/// One original durable reservation, discharged only by apply or cancel.
pub(crate) trait RowReservedOperation: sealed::State + Sized {
    type Prepared: RowPreparedOperation<Reserved = Self>;
    fn producer_lane(&mut self) -> Result<RowProducerLane<'_>>;
    fn prepare(
        self,
        proposal: RowProposal,
    ) -> std::result::Result<Self::Prepared, RowOwnedError<Self>>;
    fn cancel(self) -> std::result::Result<(), RowOwnedError<Self>>;
}

/// A checked candidate on the same reservation; reopen discards only its cut.
pub(crate) trait RowPreparedOperation: sealed::State + Sized {
    type Reserved: RowReservedOperation<Prepared = Self>;
    fn apply(self) -> std::result::Result<RowMutationObservation, RowOwnedError<Self>>;
    fn reopen(self) -> Self::Reserved;
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> sealed::State
    for PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
}
impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> RowPreflightOperation
    for PreflightRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Reserved = ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn validation_lane(&mut self) -> Result<RowValidationLane<'_>> {
        PreflightRow::validation_lane(self)
    }
    fn reserve(
        self,
        input: RowInputPlan,
    ) -> std::result::Result<Self::Reserved, RowOwnedError<Self>> {
        PreflightRow::reserve(self, input)
    }
    fn reserve_delete(self) -> std::result::Result<Self::Reserved, RowOwnedError<Self>> {
        PreflightRow::reserve_delete(self)
    }
    fn finish(self) -> Result<()> {
        PreflightRow::finish(self).map(|_| ())
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> sealed::State
    for ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
}
impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> RowReservedOperation
    for ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Prepared = PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn producer_lane(&mut self) -> Result<RowProducerLane<'_>> {
        ReservedRow::producer_lane(self)
    }
    fn prepare(
        self,
        proposal: RowProposal,
    ) -> std::result::Result<Self::Prepared, RowOwnedError<Self>> {
        ReservedRow::prepare(self, proposal)
    }
    fn cancel(self) -> std::result::Result<(), RowOwnedError<Self>> {
        ReservedRow::cancel(self).map(|_| ())
    }
}

impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> sealed::State
    for PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
}
impl<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace> RowPreparedOperation
    for PreparedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    type Reserved = ReservedRow<'r, 't, 'v, 'i, 'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>;
    fn apply(self) -> std::result::Result<RowMutationObservation, RowOwnedError<Self>> {
        PreparedRow::apply(self).map(|applied| applied.into_parts().0)
    }
    fn reopen(self) -> Self::Reserved {
        PreparedRow::reopen(self)
    }
}

/// Source-bound operation inventory on top of the unchanged eight wrapper
/// phases. CLR/recovery call the wrappers directly and keep that base census.
#[cfg(test)]
pub(super) fn operation_phases<W: WriterNamespace>() -> [super::write::RowWritePhase; 9] {
    use std::mem::size_of;
    type Pre<'a, W> = PreflightRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Res<'a, W> = ReservedRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Prep<'a, W> = PreparedRow<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    // One bridge output per call; lanes also add a receiver reference.
    // Moved Self/input/proposal do not coexist with their forwarded value.
    // Retain A's old map/return slots conservatively after moving maps to C.
    let extra = [
        0,
        size_of::<&()>() + size_of::<Result<RowValidationLane<'_>>>(),
        size_of::<std::result::Result<Res<'_, W>, RowOwnedError<Pre<'_, W>>>>(),
        size_of::<&()>() + size_of::<Result<RowProducerLane<'_>>>(),
        size_of::<std::result::Result<Prep<'_, W>, RowOwnedError<Res<'_, W>>>>(),
        size_of::<std::result::Result<RowMutationObservation, RowOwnedError<Prep<'_, W>>>>(),
        size_of::<std::result::Result<(), RowOwnedError<Res<'_, W>>>>(),
        size_of::<Result<()>>(),
    ];
    // A's insert.reopen counts the moved prepared state and two reservation
    // returns. This new C bridge adds just its return, with no deeper body.
    let reopen = super::write::RowWritePhase {
        name: "reopen",
        body: 0,
        carriers: size_of::<Res<'_, W>>(),
        bound: ROW_WRITE_CALLER_BYTES,
    };
    let names = [
        "preflight",
        "validation_lane",
        "reserve",
        "producer_lane",
        "prepare",
        "apply",
        "cancel",
        "finish",
    ];
    let base = super::write::row_write_phases::<W>();
    let mut phases = [reopen; 9];
    for (index, mut phase) in base.into_iter().enumerate() {
        assert_eq!(phase.name, names[index], "wrapper phase inventory changed");
        phase.carriers += extra[index];
        phases[index] = phase;
    }
    phases
}
