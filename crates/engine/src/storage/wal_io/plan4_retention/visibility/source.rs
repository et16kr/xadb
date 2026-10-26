//! Only the held original Catalog target guard can create a source capability.
use super::super::{RetentionCause, RetentionPhase, read::RetentionCatalogGuard};
use super::*;
use crate::storage::transaction_record::VersionStamp;
use foundation::{identity::ObjectId, namespace::WriterNamespace};

impl<'g, 'c, 's> CatalogSource<'g, 'c, 's> {
    pub(in super::super) fn from_guard<W: WriterNamespace>(
        guard: &'g RetentionCatalogGuard<'_, '_, 'c, 's, '_, '_, '_, '_, W>,
        object: ObjectId,
    ) -> super::super::Result<Self> {
        let (core, binding, fence) = guard.source_origin();
        let origin = Origin {
            core,
            binding,
            fence,
        };
        origin
            .health()
            .map_err(|error| super::super::RetentionError {
                phase: RetentionPhase::Census,
                effect: error.effect,
                cause: RetentionCause::Visibility(error),
            })?;
        let header = guard.lookup(object)?.map(|value| value.header());
        Ok(Self {
            origin,
            transaction: header.and_then(|h| match h.stamp {
                VersionStamp::Transaction(reference) => Some(reference),
                VersionStamp::Committed(_) => None,
            }),
            undo: header.and_then(|h| h.undo),
            source: std::marker::PhantomData,
        })
    }
}
