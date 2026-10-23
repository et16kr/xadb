//! One bounded lookup for bootstrap registrations and the closed INIT publication.
//! Copy snapshots carry identity across pin/content/flush boundaries. This fixed
//! directory supplies no replacement, load, reuse or generation-advance authority.

use super::{DirtyFrame, MAX_FRAMES, PageKind, Registration, TargetRequest};
use std::sync::OnceLock;

pub(super) struct RegistrationDirectory {
    entries: [Option<Registration>; MAX_FRAMES],
    closed_index: Option<usize>,
    pub(super) publication: OnceLock<Registration>,
}
impl RegistrationDirectory {
    pub(super) fn from_bootstrap(frames: &[DirtyFrame<'_>]) -> Option<Self> {
        let mut entries = [None; MAX_FRAMES];
        for (index, frame) in frames.iter().enumerate() {
            let entry = entries.get_mut(index)?;
            *entry = frame.registration;
            if let Some(registration) = frame.registration
                && (registration.generation != 1
                    || frames[..index]
                        .iter()
                        .filter_map(|frame| frame.registration)
                        .any(|prior| prior.request.same_address(&registration.request)))
            {
                return None;
            }
        }
        Some(Self {
            entries,
            closed_index: None,
            publication: OnceLock::new(),
        })
    }

    pub(super) fn close_status_slot(&mut self) {
        self.closed_index = Some(1);
    }
    pub(super) fn closed_index(&self) -> Option<usize> {
        self.closed_index
    }
    pub(super) fn published(&self) -> Option<Registration> {
        self.publication.get().copied()
    }
    // The exclusive INIT writer prechecks None before stage. Equality also
    // protects the final publication from silently accepting another value.
    pub(super) fn publish(&self, registration: Registration) -> bool {
        *self.publication.get_or_init(|| registration) == registration
    }

    pub(super) fn resolve(&self, frames: &[DirtyFrame<'_>], index: usize) -> Option<Registration> {
        let frame = frames.get(index)?;
        let original = *self.entries.get(index)?;
        if self.closed_index == Some(index) {
            if original.is_some() || frame.registration.is_some() {
                return None;
            }
            self.published().filter(|value| value.generation != 0)
        } else {
            original.filter(|value| value.generation != 0 && frame.registration == Some(*value))
        }
    }

    pub(super) fn lookup(
        &self,
        frames: &[DirtyFrame<'_>],
        request: TargetRequest,
    ) -> Option<(usize, Registration)> {
        (0..frames.len()).find_map(|index| {
            self.resolve(frames, index)
                .filter(|entry| entry.request == request)
                .map(|entry| (index, entry))
        })
    }

    /// Fixed-adapter provenance, including its generation-one restriction.
    /// This is not a future movable-directory or page reuse invariant.
    pub(super) fn valid(&self, frames: &[DirtyFrame<'_>]) -> bool {
        if self.closed_index.is_none() && self.published().is_some() {
            return false;
        }
        if let Some(index) = self.closed_index {
            if index != 1
                || frames.len() != 2
                || self.entries[index].is_some()
                || frames[index].registration.is_some()
            {
                return false;
            }
            if let Some(published) = self.published() {
                let Some(allocator) = self.resolve(frames, 0) else {
                    return false;
                };
                if published.generation != 1
                    || published.request.kind != PageKind::TransactionStatus
                    || published.request.level != 0
                    || published.request.identity.database != allocator.request.identity.database
                    || published.request.identity.page.file != allocator.request.identity.page.file
                    || published.request.identity.page.page_no != 1
                    || published.request.identity.page.incarnation.get() < 2
                    || published.request.identity.owner.get() != 2
                    || published.request.identity.owner_generation.get() != 1
                {
                    return false;
                }
            }
        }
        self.entries.iter().enumerate().all(|(index, entry)| {
            if self.closed_index == Some(index) {
                return entry.is_none();
            }
            match (entry, frames.get(index)) {
                (Some(entry), Some(_)) => {
                    entry.generation == 1 && self.resolve(frames, index) == Some(*entry)
                }
                (None, Some(frame)) => frame.registration.is_none(),
                (None, None) => true,
                (Some(_), None) => false,
            }
        })
    }
}
