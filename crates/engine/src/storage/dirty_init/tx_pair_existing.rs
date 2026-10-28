//! Exact existing-target access while the original CREATE claim remains live.
//! The key borrow survives every content guard and exposes neither raw pins nor
//! the fresh pair. Original PinSet owns quota and checked/unwind cleanup.
use super::*;

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn reserve_tx_pair_existing<'a, 'b, 'k, const N: usize>(
        &'b self,
        owner: &'a mut Owner<'c, 's>,
        key: &'k TxPairClaimKey<'c, 's>,
        requests: &[TargetRequest; N],
    ) -> Result<TxPairExistingTargets<'a, 'b, 'k, 'c, 's, 'p, 'd, N>> {
        self.health()?;
        if !owner.check_binding(self.core).map_err(DirtyError::from)? {
            return Err(unchanged(Cause::BindingMismatch));
        }
        if !(1..=7).contains(&N) {
            return Err(unchanged(Cause::Shape));
        }
        let mut indices = [0; N];
        let mut generations = [0; N];
        {
            let mut pins = self.entry_registry()?;
            if !key.matches_pool_in(self, &pins) {
                return Err(unchanged(Cause::BindingMismatch));
            }
            if !pins.init_claim.as_ref().is_some_and(|slot| {
                slot.identity == InitClaimIdentity::TxPair(key.identity)
                    && !slot.initializer_pins_held
            }) {
                return Err(unchanged(Cause::Busy));
            }
            for (at, request) in requests.iter().enumerate() {
                if requests[..at]
                    .iter()
                    .any(|prior| prior.same_address(request))
                {
                    return Err(unchanged(Cause::DuplicateTarget));
                }
                let (index, registration) = pins
                    .directory
                    .lookup(self.frames, *request)
                    .ok_or_else(|| unchanged(Cause::MissingTarget))?;
                if key.identity.indices().contains(&index)
                    || key.identity.target_mask & (1 << index) == 0
                {
                    return Err(unchanged(Cause::MissingTarget));
                }
                if pins
                    .load
                    .as_ref()
                    .is_some_and(|claim| claim.blocks(*request, index))
                {
                    return Err(unchanged(Cause::Busy));
                }
                let mut position = at;
                while position != 0
                    && self
                        .registration_in(&pins, indices[position - 1])?
                        .request
                        .rank()
                        > request.rank()
                {
                    indices[position] = indices[position - 1];
                    generations[position] = generations[position - 1];
                    position -= 1;
                }
                indices[position] = index;
                generations[position] = registration.generation;
            }
            if pins.total > MAX_PINS - N as u16
                || indices
                    .iter()
                    .any(|index| pins.counts[*index] >= MAX_FRAME_PINS)
            {
                return Err(unchanged(Cause::Capacity));
            }
            for index in indices {
                pins.counts[index] += 1;
            }
            pins.total += N as u16;
        }
        Ok(TxPairExistingTargets {
            inner: PinnedTargets {
                owner: Some(owner),
                pins: Some(PinSet {
                    pool: self,
                    indices,
                    generations,
                    len: N,
                    active: true,
                    _thread: PhantomData,
                }),
            },
            key,
        })
    }
}

pub(in crate::storage) struct TxPairExistingTargets<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize> {
    inner: PinnedTargets<'a, 'b, 'c, 's, 'p, 'd, N>,
    key: &'k TxPairClaimKey<'c, 's>,
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize>
    TxPairExistingTargets<'a, 'b, 'k, 'c, 's, 'p, 'd, N>
{
    pub(in crate::storage) fn try_read(
        self,
    ) -> Result<TxPairExistingRead<'a, 'b, 'k, 'c, 's, 'p, 'd, N>> {
        Ok(TxPairExistingRead {
            inner: self.inner.try_read()?,
            _key: self.key,
        })
    }
    pub(in crate::storage) fn try_write(
        self,
    ) -> Result<TxPairExistingWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, N>> {
        Ok(TxPairExistingWrite {
            inner: self.inner.try_write()?,
            _key: self.key,
        })
    }
    pub(in crate::storage) fn release_checked(self) -> Result<()> {
        self.inner.release_checked()
    }
}

pub(in crate::storage) struct TxPairExistingRead<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize> {
    inner: ReadTargets<'a, 'b, 'c, 's, 'p, 'd, N>,
    _key: &'k TxPairClaimKey<'c, 's>,
}
impl<const N: usize> TxPairExistingRead<'_, '_, '_, '_, '_, '_, '_, N> {
    pub(in crate::storage) fn bytes(&self, at: usize) -> Result<&[u8; PAGE_BYTES]> {
        self.inner.bytes(at)
    }
    pub(in crate::storage) fn observe(&self, at: usize) -> Result<DirtyObservation> {
        self.inner.observe(at)
    }
    pub(in crate::storage) fn copy_snapshot(
        &self,
        at: usize,
        output: &mut [u8; PAGE_BYTES],
    ) -> Result<FrameStamp> {
        self.inner.copy_snapshot(at, output)
    }
    pub(in crate::storage) fn release_checked(self) -> Result<()> {
        self.inner.release_checked()
    }
}

pub(in crate::storage) struct TxPairExistingWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize> {
    inner: WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N>,
    _key: &'k TxPairClaimKey<'c, 's>,
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize>
    TxPairExistingWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, N>
{
    pub(in crate::storage) fn matches_stamp(&self, at: usize, expected: &FrameStamp) -> Result<()> {
        self.inner.matches_stamp(at, expected)
    }
    pub(in crate::storage) fn preflight_dpt(
        &self,
        at: usize,
        expected: &FrameStamp,
    ) -> Result<DptStamp<'b>> {
        self.inner.preflight_dpt(at, expected)
    }
    pub(in crate::storage) fn install_page(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.inner.install_page(at, permit)
    }
    pub(in crate::storage) fn install_metadata(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.inner.install_metadata(at, permit)
    }
    pub(in crate::storage) fn install_dpt(
        &self,
        stamp: DptStamp<'_>,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> Result<()> {
        self.inner.install_dpt(stamp, permit)
    }
    pub(in crate::storage) fn release_checked(self) -> Result<()> {
        self.inner.release_checked()
    }
}

/// Parent packets for authenticated subset admission, plus the original nested
/// registry validation. Content acquisition/install/cleanup retains the key
/// reference around the original fixed target-set source closure.
pub(in crate::storage) fn source_bytes<const N: usize>() -> usize {
    use std::mem::size_of;
    let entry = super::super::super::retention_residency::entry_source_phases();
    let admission = 2 * size_of::<[usize; N]>()
        + 2 * size_of::<[u64; N]>()
        + size_of::<TxPairExistingTargets<'_, '_, '_, '_, '_, '_, '_, N>>()
        + 2 * size_of::<Result<TxPairExistingTargets<'_, '_, '_, '_, '_, '_, '_, N>>>()
        + 2 * size_of::<Registration>()
        + 2 * size_of::<Result<Registration>>()
        + size_of::<Option<(usize, Registration)>>()
        + 2 * size_of::<TargetRequest>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<Result<bool>>()
        + 2 * size_of::<std::slice::Iter<'_, TargetRequest>>()
        + 2 * size_of::<std::slice::Iter<'_, usize>>()
        + size_of::<std::iter::Enumerate<std::slice::Iter<'_, TargetRequest>>>()
        + 4 * size_of::<(usize, u64, &[u8], [bool; 4])>()
        + entry.0.max(entry.1).max(entry.2);
    admission.max(target_set_call_bytes::<N>() + 8 * size_of::<&TxPairClaimKey<'_, '_>>())
}
