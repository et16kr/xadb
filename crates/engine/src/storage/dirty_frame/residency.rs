//! Same-address clean allocator epoch renewal under an exclusive pool borrow.
//! Source I/O/semantic checks belong to the consuming storage owner; only its
//! sealed loaded-image permit can install a new resident generation here.
use super::*;
use crate::storage::wal_io::ReloadInstallPermit;

pub(in crate::storage) struct ReloadSnapshot<'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    registration: Registration,
    stamp: FrameStamp,
}
impl ReloadSnapshot<'_, '_> {
    pub(in crate::storage) fn request(&self) -> TargetRequest {
        self.registration.request
    }
    pub(in crate::storage) fn stamp(&self) -> FrameStamp {
        self.stamp
    }
}
impl<'c, 's> SharedPool<'c, 's, '_, '_> {
    fn reload_quiescent(&self) -> Result<()> {
        {
            let pins = self.entry_pins()?;
            if pins.total != 0 || pins.init_claim.is_some() {
                return Err(unchanged(Cause::Busy));
            }
        }
        let dpt = self.entry_dpt()?;
        if dpt.claim.is_some()
            || dpt.entries.iter().any(Option::is_some)
            || dpt.requirements.iter().any(Option::is_some)
        {
            return Err(unchanged(Cause::Busy));
        }
        Ok(())
    }
    pub(in crate::storage) fn enable_allocator_reload(&mut self) -> Result<()> {
        self.health()?;
        self.reload_quiescent()?;
        {
            let content = self.frames[0]
                .content
                .try_read()
                .map_err(|error| match error {
                    TryLockError::WouldBlock => unchanged(Cause::Busy),
                    TryLockError::Poisoned(_) => self.fatal(Cause::Poisoned),
                })?;
            self.pair(0, &content)?;
        }
        if !self.directory.enable_allocator_reload(self.frames) {
            return Err(unchanged(Cause::Shape));
        }
        Ok(())
    }
    /// No new generation is published. The output is caller scratch only.
    pub(in crate::storage) fn prepare_allocator_reload(
        &mut self,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<ReloadSnapshot<'c, 's>> {
        self.health()?;
        self.reload_quiescent()?;
        let registration = self.registration(0)?;
        if !self.directory.can_renew_allocator(registration) {
            return Err(unchanged(Cause::CounterExhausted));
        }
        let content = match self.frames[0].content.try_read() {
            Ok(content) => content,
            Err(TryLockError::WouldBlock) => return Err(unchanged(Cause::Busy)),
            Err(TryLockError::Poisoned(error)) => {
                let failure = self.fatal(Cause::Poisoned);
                drop(error.into_inner());
                return Err(failure);
            }
        };
        let before = self.pair(0, &content)?;
        if content.dirty || before.entry.is_some() || before.frame.rec_lsn.is_some() {
            return Err(unchanged(Cause::Busy));
        }
        output.copy_from_slice(content.bytes);
        drop(content);
        self.health()?;
        Ok(ReloadSnapshot {
            core: self.core,
            binding: self.binding,
            registration,
            stamp: before.frame,
        })
    }
    /// All fallible checks precede renewal/copy. An unwind fences the original
    /// storage; no old-generation completion may then service the new epoch.
    pub(in crate::storage) fn install_allocator_reload(
        &mut self,
        permit: ReloadInstallPermit<'_, 'c, 's>,
    ) -> Result<FrameStamp> {
        self.health()?;
        let snapshot = permit.snapshot();
        if !std::ptr::eq(self.core, snapshot.core)
            || !std::ptr::eq(self.binding, snapshot.binding)
            || self.registration(0)? != snapshot.registration
            || !self.directory.can_renew_allocator(snapshot.registration)
        {
            return Err(self.fatal(Cause::Invariant));
        }
        self.reload_quiescent()?;
        let mut content = match self.frames[0].content.try_write() {
            Ok(content) => content,
            Err(TryLockError::WouldBlock) => return Err(unchanged(Cause::Busy)),
            Err(TryLockError::Poisoned(error)) => {
                let failure = self.fatal(Cause::Poisoned);
                drop(error.into_inner());
                return Err(failure);
            }
        };
        if self.pair(0, &content)?.frame != snapshot.stamp || content.dirty {
            return Err(self.fatal(Cause::Invariant));
        }
        self.health()?;
        let Some(generation) = self.directory.renew_allocator(snapshot.registration) else {
            return Err(self.fatal(Cause::Invariant));
        };
        content.bytes.copy_from_slice(permit.bytes());
        content.page_lsn = snapshot.stamp.page_lsn;
        content.modification_counter = u64::from(content.page_lsn.0 != 0);
        content.dirty = false;
        content.rec_lsn = None;
        Ok(FrameStamp {
            generation,
            modification_counter: content.modification_counter,
            ..snapshot.stamp
        })
    }
}

#[cfg(test)]
pub(in crate::storage) mod test_support {
    use super::*;
    pub(in crate::storage) fn seed_generation(
        pool: &mut SharedPool<'_, '_, '_, '_>,
        generation: u64,
    ) {
        pool.directory.test_seed_reload_generation(generation);
    }
}
