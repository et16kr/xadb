//! Canonical selected-home reads followed by one exclusive publication.
use super::*;
use crate::storage::dirty_frame::{Plan4LoadSnapshot, StatusCandidateKind};
use foundation::{error::Certainty, namespace::NamespaceErrorKind};

/// Only this module's completed read/recheck path constructs installation proof.
pub(in crate::storage) struct Plan4LoadPermit<'a, 'c, 's> {
    snapshot: &'a Plan4LoadSnapshot<'c, 's>,
    index: usize,
    allocator: &'a [u8; PAGE_BYTES],
    bytes: &'a [u8; PAGE_BYTES],
    request: TargetRequest,
}
impl<'a, 'c, 's> Plan4LoadPermit<'a, 'c, 's> {
    pub(in crate::storage) fn snapshot(&self) -> &'a Plan4LoadSnapshot<'c, 's> {
        self.snapshot
    }
    pub(in crate::storage) fn index(&self) -> usize {
        self.index
    }
    pub(in crate::storage) fn allocator(&self) -> &'a [u8; PAGE_BYTES] {
        self.allocator
    }
    pub(in crate::storage) fn bytes(&self) -> &'a [u8; PAGE_BYTES] {
        self.bytes
    }
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.request
    }
}

fn retryable_io(error: &foundation::Error) -> bool {
    error.effect() == Effect::Unchanged
        && error.certainty() == Certainty::Known
        && error
            .io_kind
            .is_some_and(|kind| kind != std::io::ErrorKind::UnexpectedEof)
}
fn source_error<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    phase: Plan4PagePhase,
    source: genesis::Plan4GenesisCause,
) -> Plan4PageError {
    let retryable = match &source {
        genesis::Plan4GenesisCause::Io(error)
        | genesis::Plan4GenesisCause::PageIo(PageIoError::Read(error)) => retryable_io(error),
        genesis::Plan4GenesisCause::Namespace(error)
        | genesis::Plan4GenesisCause::PageIo(PageIoError::Namespace(error)) => {
            error.kind == NamespaceErrorKind::Io && error.effect == Effect::Unchanged
        }
        _ => false,
    };
    let cause = match source {
        genesis::Plan4GenesisCause::PageIo(error) => Plan4PageCause::PageIo(error),
        genesis::Plan4GenesisCause::Allocation(error) => Plan4PageCause::Allocation(error),
        genesis::Plan4GenesisCause::Page(error) => Plan4PageCause::Page(error),
        genesis::Plan4GenesisCause::Retention(error) => Plan4PageCause::Retention(error),
        genesis::Plan4GenesisCause::Tree(error) => Plan4PageCause::Tree(error),
        error => Plan4PageCause::Genesis(error),
    };
    if retryable && !storage.session_fence().is_fenced() {
        unchanged(phase, cause)
    } else {
        fatal(storage, phase, cause)
    }
}

impl<W: WriterNamespace> Plan4Pages<'_, '_, '_, '_, '_, '_, '_, W> {
    pub(crate) fn load_into(
        &mut self,
        index: usize,
        expected: &Plan4Target,
    ) -> Result<Plan4LoadObservation> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let p = Plan4PagePhase::Load;
        self.ready(p)?;
        let request = self.checked_request(expected, p)?;
        let prefix = pristine(storage, p)?;
        let pages: &mut [[u8; PAGE_BYTES]; 7] = self
            .operation
            .as_chunks_mut::<PAGE_BYTES>()
            .0
            .try_into()
            .expect("seven operation pages");
        let (work, rest) = pages.split_at_mut(2);
        let work: &mut [[u8; PAGE_BYTES]; 2] = work.try_into().expect("two reader pages");
        let rest: &mut [[u8; PAGE_BYTES]; 5] = rest.try_into().expect("five load pages");
        let [allocator, first, canonical, reread, _] = rest;
        let snapshot = self
            .pool
            .prepare_plan4_load_at(index, allocator)
            .map_err(|error| frame(p, error))?;
        genesis::verify_owned_allocator(&storage.core.plan, allocator)
            .map_err(|error| source_error(storage, p, error))?;
        if self
            .pool
            .resident_address_present(request)
            .map_err(|error| frame(p, error))?
        {
            return Err(unchanged(p, Plan4PageCause::AlreadyResident));
        }
        {
            let mut writer = match storage.wal()._writer.try_lock() {
                Ok(writer) => writer,
                Err(TryLockError::WouldBlock) => return Err(unchanged(p, Plan4PageCause::Busy)),
                Err(TryLockError::Poisoned(_)) => {
                    return Err(fatal(storage, p, Plan4PageCause::Poisoned));
                }
            };
            read_twice(
                storage,
                &mut *writer,
                request,
                work,
                first,
                canonical,
                reread,
            )?;
        }
        if pristine(storage, p)? != prefix {
            return Err(fatal(storage, p, Plan4PageCause::Invariant));
        }
        let permit = Plan4LoadPermit {
            snapshot: &snapshot,
            index,
            allocator,
            bytes: reread,
            request,
        };
        let after = self
            .pool
            .install_plan4_load(permit)
            .map_err(|error| frame(p, error))?;
        Ok(Plan4LoadObservation {
            before: snapshot.payload(),
            after,
        })
    }

    pub(crate) fn residency_step(
        &mut self,
        expected: &Plan4Target,
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<Plan4Resident> {
        self.resident_with_set(expected, cursor, &[])
    }

    fn resident_with_set(
        &mut self,
        expected: &Plan4Target,
        cursor: &mut Plan4ResidencyCursor,
        required: &[TargetRequest],
    ) -> Result<Plan4Resident> {
        let storage = self.storage;
        let _unwind = storage.session_fence().unwind_guard();
        let p = Plan4PagePhase::Load;
        self.ready(p)?;
        let request = self.checked_request(expected, p)?;
        pristine(storage, p)?;
        {
            let allocator: &mut [u8; PAGE_BYTES] = (&mut self.operation
                [2 * PAGE_BYTES..3 * PAGE_BYTES])
                .try_into()
                .expect("allocator snapshot loan");
            self.pool
                .plan4_allocator_snapshot(&mut *allocator)
                .map_err(|error| frame(p, error))?;
            genesis::verify_owned_allocator(&storage.core.plan, allocator)
                .map_err(|error| source_error(storage, p, error))?;
        }
        let candidate = self
            .pool
            .plan4_candidate(request, cursor.next, required)
            .map_err(|error| frame(p, error))?;
        let (stamp, loaded) = match candidate.kind {
            StatusCandidateKind::Resident => (
                candidate
                    .frame
                    .ok_or_else(|| fatal(storage, p, Plan4PageCause::Invariant))?,
                false,
            ),
            StatusCandidateKind::Load => (self.load_into(candidate.index, expected)?.after, true),
            StatusCandidateKind::Flush => return Err(fatal(storage, p, Plan4PageCause::Invariant)),
        };
        cursor.next = candidate.next;
        Ok(Plan4Resident {
            index: candidate.index,
            frame: stamp,
            loaded,
        })
    }

    /// Caller retains the same exclusive borrow through the following whole-set pin.
    pub(super) fn prepare_set<const N: usize>(
        &mut self,
        requests: &[TargetRequest; N],
        cursor: &mut Plan4ResidencyCursor,
    ) -> Result<()> {
        const {
            assert!(N >= 3 && N <= 4);
        }
        let p = Plan4PagePhase::Load;
        self.ready(p)?;
        let allocator = self.allocator_request(p)?;
        if requests[0] != allocator {
            return Err(unchanged(p, Plan4PageCause::Membership));
        }
        for (at, request) in requests.iter().enumerate() {
            if requests[..at].iter().any(|old| {
                old.identity.database == request.identity.database
                    && old.identity.page.file == request.identity.page.file
                    && old.identity.page.page_no == request.identity.page.page_no
            }) {
                return Err(unchanged(p, Plan4PageCause::Membership));
            }
            if at != 0 {
                self.checked_request(&Plan4Target { request: *request }, p)?;
            }
        }
        // Every candidate sees the entire desired set, including not-yet-loaded pages.
        // A later load can therefore never evict an earlier required resident.
        let mut next = *cursor;
        for request in &requests[1..] {
            self.resident_with_set(&Plan4Target { request: *request }, &mut next, requests)?;
        }
        *cursor = next;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)] // Separate disjoint caller page loans.
fn read_twice<W: WriterNamespace>(
    storage: &MutableStorageOwner<'_, '_, W>,
    writer: &mut W,
    request: TargetRequest,
    work: &mut [[u8; PAGE_BYTES]; 2],
    first: &mut [u8; PAGE_BYTES],
    canonical: &mut [u8; PAGE_BYTES],
    reread: &mut [u8; PAGE_BYTES],
) -> Result<()> {
    let p = Plan4PagePhase::Load;
    let index = usize::try_from(request.identity.page.page_no)
        .map_err(|_| unchanged(p, Plan4PageCause::Membership))?;
    genesis::read_owned_payload(&storage.core.plan, writer, request, work, first)
        .map_err(|error| source_error(storage, p, error))?;
    genesis::verify_owned_payload(&storage.core.plan, index, first, canonical)
        .map_err(|error| source_error(storage, p, error))?;
    genesis::read_owned_payload(&storage.core.plan, writer, request, work, reread)
        .map_err(|error| source_error(storage, p, error))?;
    genesis::verify_owned_payload(&storage.core.plan, index, reread, canonical)
        .map_err(|error| source_error(storage, p, error))?;
    if first != reread {
        return Err(fatal(storage, p, Plan4PageCause::Invariant));
    }
    Ok(())
}
