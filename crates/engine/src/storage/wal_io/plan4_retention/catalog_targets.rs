//! Internal dispatch preserves the authenticated key borrow around the one
//! original Catalog semantic body. No branch exposes raw inner pins or key.
use super::*;
use crate::storage::dirty_frame::{self, DirtyObservation, DptStamp, FrameStamp};
type DirtyResult<T> = dirty_frame::Result<T>;

#[derive(Clone, Copy)]
pub(super) enum CatalogTargetAccess<'k, 'c, 's> {
    Ordinary,
    Pair(&'k TxPairClaimKey<'c, 's>),
}
impl<'k, 'c, 's> CatalogTargetAccess<'k, 'c, 's> {
    pub(super) fn read<'a, 'b, 'p, 'd, const N: usize>(
        self,
        pool: &'b SharedPool<'c, 's, 'p, 'd>,
        owner: &'a mut Owner<'c, 's>,
        requests: &[TargetRequest; N],
    ) -> DirtyResult<CatalogRead<'a, 'b, 'k, 'c, 's, 'p, 'd, N>> {
        match self {
            Self::Ordinary => pool
                .reserve_target_set(owner, requests)?
                .try_read()
                .map(CatalogRead::Ordinary),
            Self::Pair(key) => pool
                .reserve_tx_pair_existing(owner, key, requests)?
                .try_read()
                .map(CatalogRead::Pair),
        }
    }
    pub(super) fn write<'a, 'b, 'p, 'd, const N: usize>(
        self,
        pool: &'b SharedPool<'c, 's, 'p, 'd>,
        owner: &'a mut Owner<'c, 's>,
        requests: &[TargetRequest; N],
    ) -> DirtyResult<CatalogWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, N>> {
        match self {
            Self::Ordinary => pool
                .reserve_target_set(owner, requests)?
                .try_write()
                .map(CatalogWrite::Ordinary),
            Self::Pair(key) => pool
                .reserve_tx_pair_existing(owner, key, requests)?
                .try_write()
                .map(CatalogWrite::Pair),
        }
    }
}

pub(super) enum CatalogRead<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize> {
    Ordinary(dirty_frame::ReadTargets<'a, 'b, 'c, 's, 'p, 'd, N>),
    Pair(dirty_frame::TxPairExistingRead<'a, 'b, 'k, 'c, 's, 'p, 'd, N>),
}
impl<const N: usize> CatalogRead<'_, '_, '_, '_, '_, '_, '_, N> {
    pub(super) fn bytes(&self, at: usize) -> DirtyResult<&[u8; PAGE_BYTES]> {
        match self {
            Self::Ordinary(inner) => inner.bytes(at),
            Self::Pair(inner) => inner.bytes(at),
        }
    }
    pub(super) fn observe(&self, at: usize) -> DirtyResult<DirtyObservation> {
        match self {
            Self::Ordinary(inner) => inner.observe(at),
            Self::Pair(inner) => inner.observe(at),
        }
    }
    pub(super) fn copy_snapshot(
        &self,
        at: usize,
        output: &mut [u8; PAGE_BYTES],
    ) -> DirtyResult<FrameStamp> {
        match self {
            Self::Ordinary(inner) => inner.copy_snapshot(at, output),
            Self::Pair(inner) => inner.copy_snapshot(at, output),
        }
    }
    pub(super) fn release_checked(self) -> DirtyResult<()> {
        match self {
            Self::Ordinary(inner) => inner.release_checked(),
            Self::Pair(inner) => inner.release_checked(),
        }
    }
}

pub(super) enum CatalogWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize> {
    Ordinary(dirty_frame::WriteTargets<'a, 'b, 'c, 's, 'p, 'd, N>),
    Pair(dirty_frame::TxPairExistingWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, N>),
}
impl<'a, 'b, 'k, 'c, 's, 'p, 'd, const N: usize> CatalogWrite<'a, 'b, 'k, 'c, 's, 'p, 'd, N> {
    pub(super) fn matches_stamp(&self, at: usize, expected: &FrameStamp) -> DirtyResult<()> {
        match self {
            Self::Ordinary(inner) => inner.matches_stamp(at, expected),
            Self::Pair(inner) => inner.matches_stamp(at, expected),
        }
    }
    pub(super) fn preflight_dpt(
        &self,
        at: usize,
        expected: &FrameStamp,
    ) -> DirtyResult<DptStamp<'b>> {
        match self {
            Self::Ordinary(inner) => inner.preflight_dpt(at, expected),
            Self::Pair(inner) => inner.preflight_dpt(at, expected),
        }
    }
    pub(super) fn install_page(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> DirtyResult<()> {
        match self {
            Self::Ordinary(inner) => inner.install_page(at, permit),
            Self::Pair(inner) => inner.install_page(at, permit),
        }
    }
    pub(super) fn install_metadata(
        &mut self,
        at: usize,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> DirtyResult<()> {
        match self {
            Self::Ordinary(inner) => inner.install_metadata(at, permit),
            Self::Pair(inner) => inner.install_metadata(at, permit),
        }
    }
    pub(super) fn install_dpt(
        &self,
        stamp: DptStamp<'_>,
        permit: &InstallGrant<'_, 'c, 's>,
    ) -> DirtyResult<()> {
        match self {
            Self::Ordinary(inner) => inner.install_dpt(stamp, permit),
            Self::Pair(inner) => inner.install_dpt(stamp, permit),
        }
    }
    pub(super) fn release_checked(self) -> DirtyResult<()> {
        match self {
            Self::Ordinary(inner) => inner.release_checked(),
            Self::Pair(inner) => inner.release_checked(),
        }
    }
}

/// The selected typed carrier and its Results may coexist with an inner
/// reserve/acquisition return. Count these enclosing values explicitly.
pub(super) fn call_bytes<const N: usize>() -> usize {
    use std::mem::size_of;
    let read = size_of::<CatalogRead<'_, '_, '_, '_, '_, '_, '_, N>>()
        + 2 * size_of::<DirtyResult<CatalogRead<'_, '_, '_, '_, '_, '_, '_, N>>>();
    let write = size_of::<CatalogWrite<'_, '_, '_, '_, '_, '_, '_, N>>()
        + 2 * size_of::<DirtyResult<CatalogWrite<'_, '_, '_, '_, '_, '_, '_, N>>>();
    // The inner helper retains its own return carriers. The enclosing typed
    // enum/Result are additional parent values even though ownership moves.
    dirty_frame::target_set_call_bytes::<N>().max(dirty_frame::tx_pair_existing_source_bytes::<N>())
        + read.max(write)
        + 8 * size_of::<&()>()
}
