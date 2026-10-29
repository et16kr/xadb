use super::{CatalogJob, Error, Operation, PAGE_BYTES, Result, RootContext};
use crate::storage::{
    allocation_page::{self, AllocationEntry, ReservationOwner},
    page::PageKind,
};
use foundation::identity::Generation;

pub(crate) fn change_pair<'a, J: CatalogJob<'a>>(
    operation: Operation,
    context: RootContext<J>,
    before: &[u8; PAGE_BYTES],
    after: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    context.validate(operation)?;
    if !matches!(
        operation,
        Operation::Reserve | Operation::Forward | Operation::Cancel | Operation::Clr
    ) {
        return Err(Error::InvalidInput);
    }
    let header = context.fixed_header(0, 1, PageKind::Allocation);
    let page = allocation_page::decode_allocation_page(before, header.identity, context.namespace)?;
    if page.header().lsn.0 >= context.lsn.0 {
        return Err(Error::OrderMismatch);
    }
    let pair = context.job.pair().ok_or(Error::IdentityMismatch)?;
    let object = context.job.object().ok_or(Error::IdentityMismatch)?;
    let actor = context.job.source().ok_or(Error::IdentityMismatch)?;
    let mut entries = [AllocationEntry::Free(Generation::new(1).expect("one")); 64];
    for (index, entry) in entries.iter_mut().enumerate() {
        *entry = page.entry(index)?;
    }
    for (ordinal, reference) in pair.into_iter().enumerate() {
        if reference.file != header.identity.page.file || !(9..64).contains(&reference.page_no) {
            return Err(Error::IdentityMismatch);
        }
        let index = reference.page_no as usize;
        let reserved = AllocationEntry::Reserved {
            generation: reference.incarnation,
            claimant: ReservationOwner::Transaction(actor),
        };
        let allocated = AllocationEntry::Allocated {
            generation: reference.incarnation,
            object: object.object,
            object_generation: object.generation,
            kind: if ordinal == 0 {
                PageKind::TreeAnchor
            } else {
                PageKind::RowLeaf
            },
        };
        entries[index] = match operation {
            Operation::Reserve
                if entries[index].next_reservation_generation()? == reference.incarnation =>
            {
                reserved
            }
            Operation::Forward if entries[index] == reserved => allocated,
            Operation::Cancel if entries[index] == reserved => {
                AllocationEntry::Free(reference.incarnation)
            }
            Operation::Clr if entries[index] == allocated => {
                AllocationEntry::Free(reference.incarnation)
            }
            _ => return Err(Error::IdentityMismatch),
        };
    }
    let mut cell = [0; allocation_page::ALLOCATION_CELL_BYTES];
    allocation_page::encode_allocation_cell(header, context.namespace, &entries, &mut cell)?;
    allocation_page::encode_allocation_page(header, context.namespace, &cell, after)?;
    Ok(())
}
