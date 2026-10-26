use super::{Error, Operation, PAGE_BYTES, Result, RootContext};
use crate::storage::{
    page::{self, PageKind},
    undo_record::{
        self, UndoBeforeKind, UndoCell, UndoCreator, UndoEdge, UndoPayload, UndoRecord,
        UndoRecordExpectation, UndoRelease, UndoTargetKind, UndoTargetView,
    },
};
use foundation::identity::{Generation, ObjectId};

/// Canonical Missing material has no chunks or previous edges. Releasing keeps
/// that material until each of its three exact role steps has completed.
fn material(context: RootContext<'_>, incoming: u64, cursor: u8) -> UndoRecord<'static> {
    let actor = context.job.source().expect("validated root");
    UndoRecord {
        creator: UndoCreator {
            id: actor.id(),
            status_generation: actor.generation(),
        },
        sequence: context.job.undo_sequence(),
        mutation_sequence: 1,
        incoming,
        target: UndoTargetView {
            kind: UndoTargetKind::Catalog,
            owner: ObjectId::new(5).expect("Catalog"),
            generation: Generation::new(1).expect("one"),
            entity: context.job.object().expect("root object").object.get(),
            arity: 0,
            key: &[],
        },
        edges: [
            if cursor >= 2 {
                UndoEdge::Released
            } else {
                UndoEdge::Never
            },
            if cursor >= 1 {
                UndoEdge::Released
            } else {
                UndoEdge::Never
            },
        ],
        before_kind: UndoBeforeKind::Missing,
        before_stamp: None,
        before_logical: None,
        payload: UndoPayload::None,
        release: UndoRelease {
            cursor,
            chunks_released: 0,
            next: None,
        },
    }
}

pub(crate) fn change_undo(
    operation: Operation,
    context: RootContext<'_>,
    before: &[u8; PAGE_BYTES],
    after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    context.validate(operation)?;
    let reference = context.job.undo().ok_or(Error::IdentityMismatch)?;
    let header = context.dynamic_header(
        reference.page(),
        ObjectId::new(3).expect("Undo"),
        Generation::new(1).expect("one"),
        PageKind::Undo,
    );
    let page = page::decode_page(before, header.identity, PageKind::Undo)?;
    if reference.slot() != 0
        || page.len() != 1
        || page.header().level != 0
        || page.header().lsn.0 >= context.lsn.0
    {
        return Err(Error::OrderMismatch);
    }
    let old = page.cell(0).ok_or(Error::Corrupt)?;
    let expected = UndoRecordExpectation {
        namespace: context.namespace,
        reference,
        target: material(context, 2, 0).target,
    };
    let mut candidate = [0; 432];
    match operation {
        Operation::Reserve => {
            let expected_previous = Generation::new(
                reference
                    .generation()
                    .get()
                    .checked_sub(1)
                    .ok_or(Error::Exhausted)?,
            )
            .map_err(|_| Error::IdentityMismatch)?;
            let previous = undo_record::decode_free_undo_record(old, expected_previous)?;
            if previous.next().map_err(|_| Error::Exhausted)? != reference.generation() {
                return Err(Error::IdentityMismatch);
            }
            undo_record::encode_free_undo_record(reference.generation(), &mut candidate)?;
        }
        Operation::Forward => {
            if undo_record::decode_free_undo_record(old, reference.generation())?
                != reference.generation()
            {
                return Err(Error::IdentityMismatch);
            }
            undo_record::encode_undo_record(
                UndoCell::Live(material(context, 2, 0)),
                expected,
                &[],
                &mut candidate,
            )?;
        }
        Operation::Clr | Operation::Prune => {
            undo_record::encode_undo_record(
                UndoCell::Live(material(context, 2, 0)),
                expected,
                &[],
                &mut candidate,
            )?;
            if old != candidate {
                return Err(Error::IdentityMismatch);
            }
            undo_record::encode_undo_record(
                UndoCell::Releasing(material(context, 0, 0)),
                expected,
                &[],
                &mut candidate,
            )?;
        }
        Operation::ReleaseRole | Operation::RootFree => {
            let cursor = context.job.release_cursor();
            undo_record::encode_undo_record(
                UndoCell::Releasing(material(context, 0, cursor)),
                expected,
                &[],
                &mut candidate,
            )?;
            if old != candidate {
                return Err(Error::IdentityMismatch);
            }
            if operation == Operation::RootFree {
                undo_record::encode_free_undo_record(reference.generation(), &mut candidate)?;
            } else {
                undo_record::encode_undo_record(
                    UndoCell::Releasing(material(context, 0, cursor + 1)),
                    expected,
                    &[],
                    &mut candidate,
                )?;
            }
        }
        _ => return Err(Error::InvalidInput),
    }
    undo_record::decode_undo_record(&candidate, expected, &[])?;
    page::encode_page(header, &[&candidate], after)?;
    Ok(())
}

/// RollbackStart emits no Undo redo, but its head and sequence must be proved
/// by the actual retained Live cell before the Status may become Aborting.
pub(crate) fn validate_retained_undo(
    context: RootContext<'_>,
    before: &[u8; PAGE_BYTES],
) -> Result<()> {
    context.validate(Operation::RollbackStart)?;
    let reference = context.job.undo().ok_or(Error::IdentityMismatch)?;
    let header = context.dynamic_header(
        reference.page(),
        ObjectId::new(3).expect("Undo"),
        Generation::new(1).expect("one"),
        PageKind::Undo,
    );
    let page = page::decode_page(before, header.identity, PageKind::Undo)?;
    if reference.slot() != 0
        || page.len() != 1
        || page.header().level != 0
        || page.header().lsn.0 == 0
        || page.header().lsn.0 >= context.lsn.0
    {
        return Err(Error::OrderMismatch);
    }
    let record = material(context, 2, 0);
    let expected = UndoRecordExpectation {
        namespace: context.namespace,
        reference,
        target: record.target,
    };
    let old = page.cell(0).ok_or(Error::Corrupt)?;
    undo_record::decode_undo_record(old, expected, &[])?;
    let mut canonical = [0; 432];
    undo_record::encode_undo_record(UndoCell::Live(record), expected, &[], &mut canonical)?;
    if old != canonical {
        return Err(Error::IdentityMismatch);
    }
    Ok(())
}
