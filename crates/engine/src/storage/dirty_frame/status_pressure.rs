//! One exclusive, finite scan of the original Status registration and DPT.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::storage) enum StatusCandidateKind {
    Resident,
    Load,
    Flush,
}

/// Observation only. The load/flush owner must validate its normal authority.
#[derive(Clone, Copy, Debug)]
pub(in crate::storage) struct StatusCandidate {
    pub(in crate::storage) kind: StatusCandidateKind,
    pub(in crate::storage) index: usize,
    pub(in crate::storage) frame: Option<FrameStamp>,
    pub(in crate::storage) next: usize,
}

impl SharedPool<'_, '_, '_, '_> {
    /// The mutable pool borrow excludes live pool actors. Pin/DPT mutex guards
    /// never escape their synchronous methods, so those original mutexes have
    /// no competing holder here; forgotten pins/claims remain visible as Busy.
    /// Content uses try_read, and the scan never performs I/O or changes state.
    pub(in crate::storage) fn status_candidate(
        &mut self,
        request: TargetRequest,
        start: usize,
    ) -> Result<StatusCandidate> {
        self.health()?;
        if !self.directory.resident_status() {
            return Err(unchanged(Cause::Shape));
        }
        self.payload_candidate(Some(request), start)
    }
    pub(in crate::storage) fn mixed_candidate(
        &mut self,
        request: TargetRequest,
        start: usize,
    ) -> Result<StatusCandidate> {
        self.health()?;
        if !self.directory.resident_mixed() || !self.directory.accepts_payload(request) {
            return Err(unchanged(Cause::Shape));
        }
        self.payload_candidate(Some(request), start)
    }
    pub(in crate::storage) fn mixed_init_candidate(
        &mut self,
        start: usize,
    ) -> Result<StatusCandidate> {
        self.health()?;
        if !self.directory.resident_mixed() {
            return Err(unchanged(Cause::Shape));
        }
        self.payload_candidate(None, start)
    }
    fn payload_candidate(
        &mut self,
        request: Option<TargetRequest>,
        start: usize,
    ) -> Result<StatusCandidate> {
        self.health()?;
        if !self.directory.resident_payloads() || start == 0 || self.frames.len() < 2 {
            return Err(unchanged(Cause::Shape));
        }
        if !self.handoff_quiescent()? {
            return Err(unchanged(Cause::Busy));
        }
        let count = self.frames.len() - 1;
        let start = (start - 1) % count;
        let mut resident = None;
        let mut clean = None;
        let mut dirty: Option<StatusCandidate> = None;
        let mut busy = false;
        for offset in 0..count {
            let index = 1 + (start + offset) % count;
            let content = match self.frames[index].content.try_read() {
                Ok(content) => content,
                Err(TryLockError::WouldBlock) => {
                    busy = true;
                    continue;
                }
                Err(TryLockError::Poisoned(_)) => return Err(self.fatal(Cause::Poisoned)),
            };
            let frame = if let Some(registration) = self.resolve_registration(index) {
                let frame = self.pair(index, &content)?.frame;
                if let Some(request) = request
                    && registration.request.same_address(&request)
                {
                    if registration.request != request {
                        return Err(unchanged(Cause::Stale));
                    }
                    resident = Some(StatusCandidate {
                        kind: StatusCandidateKind::Resident,
                        index,
                        frame: Some(frame),
                        next: 1 + index % count,
                    });
                }
                Some(frame)
            } else {
                let dpt = self.entry_dpt()?;
                if !content.fresh()
                    || dpt.entries[index].is_some()
                    || dpt.requirements[index].is_some()
                {
                    return Err(self.fatal(Cause::Invariant));
                }
                None
            };
            if self.directory.next_resident_generation(index).is_none() {
                continue;
            }
            let candidate = StatusCandidate {
                kind: if content.dirty {
                    StatusCandidateKind::Flush
                } else {
                    StatusCandidateKind::Load
                },
                index,
                frame,
                next: 1 + index % count,
            };
            if content.dirty {
                let rec_lsn = frame
                    .and_then(|f| f.rec_lsn)
                    .ok_or_else(|| self.fatal(Cause::Invariant))?;
                if dirty.is_none_or(|prior| {
                    rec_lsn.0
                        < prior
                            .frame
                            .expect("dirty frame")
                            .rec_lsn
                            .expect("dirty recLSN")
                            .0
                }) {
                    dirty = Some(candidate);
                }
            } else if clean.is_none() {
                clean = Some(candidate);
            }
        }
        self.health()?;
        // Do not hide a late inaccessible candidate or return CounterExhausted
        // while part of the directory could not be inspected.
        if busy {
            return Err(unchanged(Cause::Busy));
        }
        resident
            .or(clean)
            .or(dirty)
            .ok_or_else(|| unchanged(Cause::CounterExhausted))
    }
}

#[cfg(test)]
pub(in crate::storage) mod test_support {
    use super::*;

    pub(in crate::storage) fn duplicate_registration(pool: &mut SharedPool<'_, '_, '_, '_>) {
        let registration = pool.resolve_registration(1).unwrap();
        pool.directory.replace_resident_status(2, registration);
    }
    pub(in crate::storage) fn generation(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        index: usize,
        generation: u64,
    ) {
        let mut registration = pool.resolve_registration(index).unwrap();
        registration.generation = generation;
        pool.directory.replace_resident_status(index, registration);
        if let Some(entry) = &mut pool.dpt.lock().unwrap().entries[index] {
            entry.generation = generation;
        }
    }
    pub(in crate::storage) fn forget_content_write(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
    ) {
        std::mem::forget(pool.frames[index].content.write().unwrap());
    }
    pub(in crate::storage) fn conservative_rec_lsn(
        pool: &SharedPool<'_, '_, '_, '_>,
        index: usize,
        lsn: PageLsn,
    ) {
        pool.frames[index].content.write().unwrap().rec_lsn = Some(lsn);
        pool.dpt.lock().unwrap().entries[index]
            .as_mut()
            .unwrap()
            .rec_lsn = lsn;
    }
}
