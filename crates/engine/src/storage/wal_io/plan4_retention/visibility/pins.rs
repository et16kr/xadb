use super::{authority::ReferenceTarget, *};

const RESERVED: u8 = 1;
const LIVE: u8 = 2;
pub(super) const TRANSFERRED: u8 = 3;
const RETIRED: u8 = 4;
const CATALOG: u8 = 1;
const ROLLBACK: u8 = 2;

pub(in crate::storage) struct ReferenceLease<'v, 'a, 'c, 's> {
    visibility: &'v BoundVisibility<'a, 'c, 's>,
    owner: OwnerIdentity,
    generations: [u64; 2],
    active: bool,
}
pub(super) struct Pin<'v, 'a, 'c, 's> {
    pub(super) visibility: &'v BoundVisibility<'a, 'c, 's>,
    pub(super) owner: OwnerIdentity,
    pub(super) row: usize,
    pub(super) generation: u64,
    pub(super) target: ReferenceTarget,
    pub(super) role: u8,
    pub(super) active: bool,
}
pub(in crate::storage) struct UndoPin<'v, 'a, 'c, 's> {
    pub(super) pin: Pin<'v, 'a, 'c, 's>,
    pub(super) rollback_actor: Option<TxnRef>,
}
pub(in crate::storage) struct TxnPin<'v, 'a, 'c, 's> {
    pin: Pin<'v, 'a, 'c, 's>,
}
pub(in crate::storage) struct CapturedCatalogRefs<'v, 'a, 'c, 's> {
    pub(in crate::storage) transaction: Option<TxnPin<'v, 'a, 'c, 's>>,
    pub(in crate::storage) undo: Option<UndoPin<'v, 'a, 'c, 's>>,
}

impl<'a, 'c, 's> BoundVisibility<'a, 'c, 's> {
    /// Reserve both movement rows while the original owner is Idle. The private
    /// resolver keeps borrowing that same Owner throughout its read/load phases.
    pub(in crate::storage) fn reserve_references(
        &self,
        owner: &Owner<'c, 's>,
    ) -> VisibilityResult<ReferenceLease<'_, 'a, 'c, 's>> {
        let owner = owner.visibility_identity(self.origin.core)?;
        let mut bytes = self.entry()?;
        let base = owner.index() * 2;
        let mut generations = [0; 2];
        for (at, generation) in generations.iter_mut().enumerate() {
            let row = reference_row(&bytes[..], base + at);
            if row[0] != 0 {
                return Err(unchanged(if row[0] == RETIRED {
                    VisibilityCause::Exhausted
                } else {
                    VisibilityCause::Busy
                }));
            }
            *generation = u64_at(row, 8)
                .checked_add(1)
                .ok_or_else(|| unchanged(VisibilityCause::Exhausted))?;
        }
        for (at, generation) in generations.into_iter().enumerate() {
            let row = reference_mut(&mut bytes[..], base + at);
            row.fill(0);
            row[0] = RESERVED;
            row[3] = owner.index() as u8;
            put_u64(row, 8, generation);
            put_u64(row, 16, owner.generation());
        }
        Ok(ReferenceLease {
            visibility: self,
            owner,
            generations,
            active: true,
        })
    }
}
impl<'v, 'a, 'c, 's> ReferenceLease<'v, 'a, 'c, 's> {
    /// Observation for a candidate; this scalar alone grants no pin authority.
    pub(in super::super) fn row_generation(&self) -> u64 {
        self.generations[0]
    }

    fn check(&self, bytes: &[u8], at: usize, expected: u8) -> VisibilityResult<()> {
        let row = reference_row(bytes, self.owner.index() * 2 + at);
        if !self.active
            || row[0] != expected
            || row[3] != self.owner.index() as u8
            || u64_at(row, 8) != self.generations[at]
            || u64_at(row, 16) != self.owner.generation()
        {
            return Err(self.visibility.origin.fatal(VisibilityCause::Invariant));
        }
        Ok(())
    }
    /// Both target validations precede either registration. The input keeps the
    /// actual source latch borrowed until this method consumes it.
    pub(in crate::storage) fn capture_catalog(
        &mut self,
        source: CatalogSource<'_, 'c, 's>,
    ) -> VisibilityResult<CapturedCatalogRefs<'v, 'a, 'c, 's>> {
        if !self.visibility.origin.same(&source.origin) {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        let mut bytes = self.visibility.entry()?;
        for at in 0..2 {
            self.check(&bytes[..], at, RESERVED)?;
        }
        let targets = [
            source.transaction.map(ReferenceTarget::Transaction),
            source.undo.map(ReferenceTarget::Undo),
        ];
        for target in targets.into_iter().flatten() {
            check_retiring(&bytes[..], target)?;
        }
        let transaction = targets[0].map(|target| TxnPin {
            pin: self.install(&mut bytes[..], 0, target, CATALOG),
        });
        let undo = targets[1].map(|target| UndoPin {
            pin: self.install(&mut bytes[..], 1, target, CATALOG),
            rollback_actor: None,
        });
        Ok(CapturedCatalogRefs { transaction, undo })
    }
    /// Current or next rollback reference, captured under its actual source
    /// latch. A second free row permits next-before-current handoff.
    pub(in crate::storage) fn capture_rollback(
        &mut self,
        source: RollbackSource<'_, 'c, 's>,
    ) -> VisibilityResult<UndoPin<'v, 'a, 'c, 's>> {
        if !self.visibility.origin.same(&source.origin) {
            return Err(unchanged(VisibilityCause::BindingMismatch));
        }
        let mut bytes = self.visibility.entry()?;
        let at = (0..2)
            .find(|&at| reference_row(&bytes[..], self.owner.index() * 2 + at)[0] == RESERVED)
            .ok_or_else(|| unchanged(VisibilityCause::Capacity))?;
        self.check(&bytes[..], at, RESERVED)?;
        let target = ReferenceTarget::Undo(source.current);
        check_retiring(&bytes[..], target)?;
        Ok(UndoPin {
            pin: self.install(&mut bytes[..], at, target, ROLLBACK),
            rollback_actor: Some(source.actor),
        })
    }
    fn install(
        &self,
        bytes: &mut [u8],
        at: usize,
        target: ReferenceTarget,
        role: u8,
    ) -> Pin<'v, 'a, 'c, 's> {
        let index = self.owner.index() * 2 + at;
        let row = reference_mut(bytes, index);
        row[0] = LIVE;
        row[1] = target.kind();
        row[2] = role;
        target.encode(&mut row[24..72]);
        Pin {
            visibility: self.visibility,
            owner: self.owner,
            row: index,
            generation: self.generations[at],
            target,
            role,
            active: true,
        }
    }
    fn release(&mut self) -> VisibilityResult<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let mut bytes = self.visibility.cleanup()?;
        for at in 0..2 {
            let row = reference_row(&bytes[..], self.owner.index() * 2 + at);
            if row[0] != RESERVED
                || row[3] != self.owner.index() as u8
                || u64_at(row, 8) != self.generations[at]
                || u64_at(row, 16) != self.owner.generation()
            {
                return Err(self.visibility.origin.fatal(VisibilityCause::Invariant));
            }
        }
        for at in 0..2 {
            let row = reference_mut(&mut bytes[..], self.owner.index() * 2 + at);
            row.fill(0);
            put_u64(row, 8, self.generations[at]);
            row[0] = if self.generations[at] == u64::MAX {
                RETIRED
            } else {
                0
            };
        }
        Ok(())
    }
    pub(in crate::storage) fn release_checked(mut self) -> VisibilityResult<()> {
        self.release()
    }
}
impl Drop for ReferenceLease<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

impl Pin<'_, '_, '_, '_> {
    pub(super) fn check(&self, bytes: &[u8], expected: u8) -> VisibilityResult<()> {
        let row = reference_row(bytes, self.row);
        if row[0] != expected
            || row[1] != self.target.kind()
            || row[2] != self.role
            || row[3] != self.owner.index() as u8
            || self.row / 2 != self.owner.index()
            || u64_at(row, 8) != self.generation
            || u64_at(row, 16) != self.owner.generation()
            || ReferenceTarget::decode(row[1], &row[24..72]).ok() != Some(self.target)
        {
            return Err(self.visibility.origin.fatal(VisibilityCause::Invariant));
        }
        Ok(())
    }
    fn release(&mut self) -> VisibilityResult<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let mut bytes = self.visibility.cleanup()?;
        self.check(&bytes[..], LIVE)?;
        clear_target(reference_mut(&mut bytes[..], self.row));
        Ok(())
    }
}
impl Drop for Pin<'_, '_, '_, '_> {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
impl UndoPin<'_, '_, '_, '_> {
    pub(in crate::storage) fn reference(&self) -> UndoRef {
        let ReferenceTarget::Undo(reference) = self.pin.target else {
            unreachable!("typed Undo pin")
        };
        reference
    }
    pub(in crate::storage) fn release_checked(mut self) -> VisibilityResult<()> {
        self.pin.release()
    }
    pub(super) fn is_rollback_for(&self, actor: TxnRef) -> bool {
        self.pin.active && self.pin.role == ROLLBACK && self.rollback_actor == Some(actor)
    }
}
impl TxnPin<'_, '_, '_, '_> {
    pub(in crate::storage) fn reference(&self) -> TxnRef {
        let ReferenceTarget::Transaction(reference) = self.pin.target else {
            unreachable!("typed transaction pin")
        };
        reference
    }
    pub(in crate::storage) fn release_checked(mut self) -> VisibilityResult<()> {
        self.pin.release()
    }
}

pub(super) fn clear_target(row: &mut [u8]) {
    row[0] = RESERVED;
    row[1..3].fill(0);
    row[24..72].fill(0);
}
pub(super) fn count(bytes: &[u8], target: ReferenceTarget) -> usize {
    (0..REFERENCES)
        .filter(|&index| {
            let row = reference_row(bytes, index);
            row[0] == LIVE && ReferenceTarget::decode(row[1], &row[24..72]).ok() == Some(target)
        })
        .count()
}
fn check_retiring(bytes: &[u8], target: ReferenceTarget) -> VisibilityResult<()> {
    let row = retirement_row(bytes);
    if row[0] != 0 && ReferenceTarget::decode(row[1], &row[64..112]).ok() == Some(target) {
        return Err(unchanged(VisibilityCause::Busy));
    }
    Ok(())
}
pub(super) fn validate(bytes: &[u8]) -> Result<(), ()> {
    for index in 0..REFERENCES {
        let row = reference_row(bytes, index);
        let generation = u64_at(row, 8);
        if row[4..8].iter().chain(row[72..80].iter()).any(|v| *v != 0) {
            return Err(());
        }
        match row[0] {
            0 | RETIRED
                if row[1..8].iter().chain(row[16..].iter()).all(|v| *v == 0)
                    && ((row[0] == 0 && generation < u64::MAX)
                        || (row[0] == RETIRED && generation == u64::MAX)) => {}
            RESERVED
                if generation != 0
                    && row[1..3].iter().all(|v| *v == 0)
                    && row[3] as usize == index / 2
                    && u64_at(row, 16) != 0
                    && row[24..72].iter().all(|v| *v == 0) => {}
            LIVE | TRANSFERRED
                if generation != 0
                    && matches!(row[2], CATALOG | ROLLBACK)
                    && row[3] as usize == index / 2
                    && u64_at(row, 16) != 0 =>
            {
                let target = ReferenceTarget::decode(row[1], &row[24..72])?;
                if row[2] == ROLLBACK && !matches!(target, ReferenceTarget::Undo(_)) {
                    return Err(());
                }
                if row[0] == TRANSFERRED {
                    let marker = retirement_row(bytes);
                    if marker[0] == 0
                        || u64_at(marker, 16) != index as u64
                        || u64_at(marker, 24) != generation
                        || ReferenceTarget::decode(marker[1], &marker[64..112])? != target
                    {
                        return Err(());
                    }
                }
            }
            _ => return Err(()),
        }
        if index % 2 == 1 {
            let prior = reference_row(bytes, index - 1);
            let reserved = matches!(row[0], RESERVED | LIVE | TRANSFERRED);
            let prior_reserved = matches!(prior[0], RESERVED | LIVE | TRANSFERRED);
            if reserved != prior_reserved || (reserved && u64_at(row, 16) != u64_at(prior, 16)) {
                return Err(());
            }
        }
    }
    Ok(())
}

/// Read-only source pin census: (Catalog live, rollback live, transferred).
/// Physical frame pins are a separate pool count and are never inferred here.
#[cfg(test)]
pub(super) fn test_census(bytes: &[u8]) -> (usize, usize, usize) {
    let mut census = (0, 0, 0);
    for index in 0..REFERENCES {
        let row = reference_row(bytes, index);
        match (row[0], row[2]) {
            (LIVE, CATALOG) => census.0 += 1,
            (LIVE, ROLLBACK) => census.1 += 1,
            (TRANSFERRED, _) => census.2 += 1,
            _ => {}
        }
    }
    census
}

impl<'v, 'a, 'c, 's> ReferenceLease<'v, 'a, 'c, 's> {
    /// Capture the current head under the original pool's actual Status latch.
    /// Used after CLR too: the old U pin remains live until this H pin exists.
    pub(in crate::storage) fn capture_row_head(
        &mut self,
        pool: &crate::storage::dirty_frame::SharedPool<'c, 's, '_, '_>,
        owner: &mut Owner<'c, 's>,
        target: crate::storage::dirty_frame::TargetRequest,
        actor: TxnRef,
    ) -> super::super::Result<Option<UndoPin<'v, 'a, 'c, 's>>> {
        use super::super::{RetentionCause, RetentionPhase, frame, unchanged as refusal};
        use crate::storage::{
            page::{self, PageKind},
            status_home::StatusHome,
            transaction_record::{RollbackState, TransactionState, TransactionStatusCell},
        };
        let bad = || refusal(RetentionPhase::Prepare, RetentionCause::Invariant);
        if !std::ptr::eq(pool.bound_core(), self.visibility.origin.core)
            || !pool
                .uses_storage_origin(self.visibility.origin.binding, self.visibility.origin.fence)
        {
            return Err(refusal(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let read = pool
            .reserve_target_set(owner, &[target])
            .and_then(|set| set.try_read())
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let result = (|| {
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let page = page::decode_page(bytes, target.identity, PageKind::TransactionStatus)
                .map_err(|_| bad())?;
            let home = StatusHome::decode(page.cell(0).ok_or_else(bad)?).map_err(|_| bad())?;
            let TransactionStatusCell::Occupied(status) = home.core() else {
                return Err(bad());
            };
            if status.reference() != actor
                || !matches!(status.state(),TransactionState::Aborting(RollbackState::Statement{next,..}|RollbackState::Transaction{next}) if next==status.undo_head())
            {
                return Err(bad());
            }
            status
                .undo_head()
                .map(|current| {
                    self.capture_rollback(RollbackSource {
                        origin: self.visibility.origin,
                        actor,
                        current,
                        source: std::marker::PhantomData,
                    })
                })
                .transpose()
                .map_err(|e| super::super::RetentionError {
                    phase: RetentionPhase::Prepare,
                    cause: RetentionCause::Visibility(e),
                    effect: e.effect,
                })
        })();
        super::super::merge(
            result,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }
}

impl<'v, 'a, 'c, 's> ReferenceLease<'v, 'a, 'c, 's> {
    pub(in super::super) fn row_identity(&self) -> (usize, usize, u64) {
        (
            std::ptr::from_ref(self.visibility).addr(),
            self.owner.index(),
            self.generations[0],
        )
    }

    /// Only a held original RowLeaf can issue these pins. Decode and retirement
    /// validation finish before either registry row is changed.
    pub(in super::super) fn capture_row<'g>(
        &mut self,
        read: &'g crate::storage::dirty_frame::ReadTargets<'_, '_, '_, '_, '_, '_, 3>,
        at: usize,
        namespace: crate::api::Namespace,
        schema: crate::storage::row_record::RowSchemaInput<'_>,
    ) -> super::super::Result<(
        CapturedCatalogRefs<'v, 'a, 'c, 's>,
        crate::storage::row_record::RowRecordView<'g>,
    )> {
        use super::super::{RetentionCause, RetentionError, RetentionPhase, frame};
        use crate::storage::{
            page::PageKind,
            row_record::{self, RecordExpectation, RowPayload},
            transaction_record::VersionStamp,
            tree_page::{self, EntryPayload, TreeExpectation},
        };
        let bad = || RetentionError {
            phase: RetentionPhase::Source,
            effect: Effect::CallerMustResolve,
            cause: RetentionCause::Invariant,
        };
        let result = (|| {
            if !read.confirms_origin(
                self.visibility.origin.core,
                self.visibility.origin.binding,
                self.visibility.origin.fence,
            ) {
                return Err(bad());
            }
            let request = read
                .target_request(2)
                .map_err(|e| frame(RetentionPhase::Source, e))?;
            if request.kind != PageKind::RowLeaf || request.level != 0 {
                return Err(bad());
            }
            let leaf = tree_page::decode_tree_page(
                read.bytes(2)
                    .map_err(|e| frame(RetentionPhase::Source, e))?,
                TreeExpectation {
                    identity: request.identity,
                    kind: PageKind::RowLeaf,
                    level: 0,
                    arity: 0,
                },
            )
            .map_err(|_| bad())?;
            let entry = leaf.entry(at).map_err(|_| bad())?;
            let key: [u8; 8] = entry.key.try_into().map_err(|_| bad())?;
            let row =
                foundation::identity::RowId::new(u64::from_be_bytes(key)).map_err(|_| bad())?;
            let EntryPayload::Value(bytes) = entry.payload else {
                return Err(bad());
            };
            let record = row_record::decode_row_record_with_schema(
                bytes,
                RecordExpectation { namespace, row },
                schema,
            )
            .map_err(|_| bad())?;
            if !matches!(record.payload(),RowPayload::Inline(value) if (5..=2048).contains(&value.as_bytes().len()))
            {
                return Err(bad());
            }
            let version = record.version();
            let source = CatalogSource {
                origin: self.visibility.origin,
                transaction: match version.stamp {
                    VersionStamp::Transaction(tx) => Some(tx),
                    VersionStamp::Committed(_) => None,
                },
                undo: version.undo,
                source: std::marker::PhantomData,
            };
            let refs = self.capture_catalog(source).map_err(|e| RetentionError {
                phase: RetentionPhase::Census,
                effect: e.effect,
                cause: RetentionCause::Visibility(e),
            })?;
            Ok((refs, record))
        })();
        if result
            .as_ref()
            .is_err_and(|e| e.effect == Effect::CallerMustResolve)
        {
            self.visibility.origin.fence.fence();
        }
        result
    }
}
