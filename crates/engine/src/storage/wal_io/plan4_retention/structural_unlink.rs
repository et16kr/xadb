//! The live unlink of an empty RowLeaf (dev3-tree-smo.md#empty-leaf-unlink):
//! the C entry A's cleanup drain calls for one RAM note. Under the one
//! structural shell it routes the note's key from the table anchor's
//! current root, checks that the routed leaf is an empty non-root leaf whose
//! parent admits LeafUnlink or RootCollapse, checks the table's split debt,
//! makes the group's pages resident, then prepares the group from the same
//! cut's canonical replay (`prepare_unlink`) and installs it through the one
//! existing-group body (`ExistingGroup::Structural`). A LOAD that needs a
//! dirty frame flushed ends the call with that frame, before anything is
//! prepared. The group has no INIT target, so no Structural claim, I/O
//! lease or post-durable canonical replay is used: the same-cut canonical
//! preparation and the frame comparison are its proof.
use super::super::super::row_preflight::{RowRouteCursor, RowTree};
use super::super::super::{CatalogTargetAccess, lifecycle::ExistingGroup};
use super::*;
use crate::storage::{
    page::PageIdentity,
    retention_page::row as job3,
    semantic_row::structure::unlink::{
        self as unlink_smo,
        candidate::{ANCHOR, LEAF, PARENT, SURVIVOR},
        frame as unlink_frame,
    },
    tree_page::{self, TreeExpectation},
};
use foundation::identity::{Generation, ObjectId, RowId, SchemaEpoch};

/// What one live unlink call did (`unlink_leaf_live`).
#[derive(Debug)]
pub(in crate::storage) enum RowUnlinkStep {
    /// One LeafUnlink or RootCollapse group is installed and its frame is
    /// durable; its note is gone.
    Unlinked(smo::Operation),
    /// The residency of the route or of the group's pages needs this dirty
    /// victim flushed first. Nothing was prepared or staged; the note stays.
    FlushRequired(Plan4Target),
    /// The routed leaf is the root or holds a cell, or its non-root parent
    /// has one separator: the tree already is a valid state. The note is gone.
    NotNeeded,
}

/// What the residency step found.
enum UnlinkResidency {
    /// The empty leaf to unlink, resident with the anchor, its parent and,
    /// for a RootCollapse, the survivor.
    Target(PageRef),
    Flush(Plan4Target),
    NotNeeded,
}

/// A route from the root to the leaf: the root's level is at most 15.
const ROUTE: usize = 16;

/// RAM unlink notes (dev3-tree-smo.md#empty-leaf-unlink, 호출자): at most
/// eight per writer, as the last 448 B of the control loan's bank region
/// (up to `BANK_END`, where the Jobs start). The bank names rows up to
/// `STRUCTURAL_SLOT` and the canonical comparison of `validate_live` covers
/// its first 36 rows; the notes start past every whole 1,664-B row the
/// region can hold (39 rows, 64,896 B), so no bank row reaches them. A note
/// is a hint, never authority: `unlink_leaf_live` checks every condition
/// again. The ledger checks read named rows, and a new session or a recovery
/// restore starts the region from the canonical bank, so notes vanish at
/// restart. The region lies inside `ControlCut`'s bank digest, so a note
/// written between a group's preparation and its install would refuse that
/// install (Busy). A note is written only after a group installed and
/// removed only by the drain's steps, each its own call on the exclusively
/// borrowed writer, so no prepared cut is held across either write.
pub(in super::super::super::super) mod note {
    use super::super::super::super::BANK_END;
    use super::*;

    pub(in crate::storage) const COUNT: usize = 8;
    const BYTES: usize = 56;
    /// The end of the last whole row the bank region can hold.
    const ROWS_END: usize = BANK_END / control::SLOT_BYTES * control::SLOT_BYTES;
    const START: usize = BANK_END - COUNT * BYTES;
    const _: () = assert!(
        (control::STRUCTURAL_SLOT + 1) * control::SLOT_BYTES <= ROWS_END && ROWS_END <= START
    );

    /// One note: the table, its anchor and the RowId of the row whose
    /// removal left its routed leaf empty.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::storage) struct Note {
        pub(in crate::storage) table: ObjectHandle,
        pub(in crate::storage) anchor: PageRef,
        pub(in crate::storage) row: RowId,
    }

    fn slot<'x>(control: &'x control::RetentionLedger<'_, '_, '_>, at: usize) -> &'x [u8] {
        &control.bytes[START + at * BYTES..START + (at + 1) * BYTES]
    }
    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().expect("fixed note field"))
    }

    /// Note `at`, or None for an empty slot. Only `record` writes a slot, so
    /// a slot that does not decode is an invariant breach.
    pub(in crate::storage) fn get(
        control: &control::RetentionLedger<'_, '_, '_>,
        at: usize,
    ) -> std::result::Result<Option<Note>, RetentionCause> {
        let bytes = slot(control, at);
        if bytes.iter().all(|byte| *byte == 0) {
            return Ok(None);
        }
        let bad = |_| RetentionCause::Invariant;
        Ok(Some(Note {
            table: ObjectHandle {
                object: ObjectId::new(u64_at(bytes, 0)).map_err(bad)?,
                generation: Generation::new(u64_at(bytes, 8)).map_err(bad)?,
                schema_epoch: SchemaEpoch::new(u64_at(bytes, 16)).map_err(bad)?,
            },
            anchor: PageRef::decode(&bytes[24..48]).map_err(|_| RetentionCause::Invariant)?,
            row: RowId::new(u64_at(bytes, 48)).map_err(bad)?,
        }))
    }

    /// Keeps `note` unless it is already kept or eight notes are kept (a new
    /// note is then dropped). Notes stay in arrival order.
    pub(in crate::storage) fn record(
        control: &mut control::RetentionLedger<'_, '_, '_>,
        note: Note,
    ) -> std::result::Result<(), RetentionCause> {
        for at in 0..COUNT {
            match get(control, at)? {
                Some(kept) if kept == note => return Ok(()),
                Some(_) => {}
                None => {
                    let out = &mut control.bytes[START + at * BYTES..START + (at + 1) * BYTES];
                    out[0..8].copy_from_slice(&note.table.object.get().to_le_bytes());
                    out[8..16].copy_from_slice(&note.table.generation.get().to_le_bytes());
                    out[16..24].copy_from_slice(&note.table.schema_epoch.get().to_le_bytes());
                    note.anchor
                        .encode(&mut out[24..48])
                        .map_err(|_| RetentionCause::Invariant)?;
                    out[48..56].copy_from_slice(&note.row.get().to_le_bytes());
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// UNL-C03: the notes' bytes in the control loan, and from the control
    /// layout's constants the end of the bank's named rows (through
    /// `STRUCTURAL_SLOT`), the end of its whole rows, and `BANK_END` (the end
    /// of `ControlCut`'s bank digest, where the Jobs begin).
    #[cfg(test)]
    pub(in crate::storage) fn region() -> (std::ops::Range<usize>, [usize; 3]) {
        (
            START..START + COUNT * BYTES,
            [
                (control::STRUCTURAL_SLOT + 1) * control::SLOT_BYTES,
                ROWS_END,
                BANK_END,
            ],
        )
    }

    /// The kept note of (`table`, `row`), if any.
    pub(in crate::storage) fn find(
        control: &control::RetentionLedger<'_, '_, '_>,
        table: ObjectHandle,
        row: RowId,
    ) -> std::result::Result<Option<Note>, RetentionCause> {
        for at in 0..COUNT {
            if let Some(note) = get(control, at)?
                && note.table == table
                && note.row == row
            {
                return Ok(Some(note));
            }
        }
        Ok(None)
    }

    /// Drops the note of (`table`, `row`); later notes move up one slot.
    pub(in crate::storage) fn remove(
        control: &mut control::RetentionLedger<'_, '_, '_>,
        table: ObjectHandle,
        row: RowId,
    ) -> std::result::Result<(), RetentionCause> {
        for at in 0..COUNT {
            match get(control, at)? {
                Some(note) if note.table == table && note.row == row => {
                    let end = START + COUNT * BYTES;
                    control
                        .bytes
                        .copy_within(START + (at + 1) * BYTES..end, START + at * BYTES);
                    control.bytes[end - BYTES..end].fill(0);
                    return Ok(());
                }
                Some(_) => {}
                None => return Ok(()),
            }
        }
        Ok(())
    }
}

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The first kept unlink note, or None. A note of a table with a
    /// non-Free Catalog Job (until schedule 10-1, every product table: its
    /// CREATE Job) is dropped silently first: neither a split nor an unlink
    /// admits that table. Reads the control Jobs and notes only.
    pub(in crate::storage) fn unlink_next(&mut self) -> Result<Option<(ObjectHandle, RowId)>> {
        let storage = self.storage;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        while let Some(kept) = note::get(self.control, 0).map_err(census)? {
            if !self.catalog_debt(kept.table)? {
                return Ok(Some((kept.table, kept.row)));
            }
            note::remove(self.control, kept.table, kept.row).map_err(census)?;
        }
        Ok(None)
    }

    /// UNL-L07: the kept unlink notes, (object, RowId) in arrival order.
    #[cfg(test)]
    pub(in crate::storage) fn test_unlink_notes(&self) -> Vec<(u64, u64)> {
        (0..note::COUNT)
            .filter_map(|at| note::get(self.control, at).unwrap())
            .map(|kept| (kept.table.object.get(), kept.row.get()))
            .collect()
    }

    /// Whether `table` has a non-Free Catalog Job.
    fn catalog_debt(&self, table: ObjectHandle) -> Result<bool> {
        let namespace = self.storage.profile().spec().namespace;
        for bytes in self.control.jobs() {
            let job = job3::decode_job(bytes, namespace).map_err(queue_error)?;
            if job.object().is_some_and(|object| {
                object.object == table.object && object.generation == table.generation
            }) && job.scope() == Some(job3::JobScope::Catalog)
                && job.state() != job3::JobState::Free
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Keeps a note of (`table`, `anchor`, `row`) when the installed
    /// group's RowLeaf (a page of `table`) has an after image that holds no
    /// cell: the live DeletePrune and INSERT CLR call it after their install
    /// returned. Whether the leaf is the root is the drain's route to see
    /// (`NotNeeded`). The group is installed and durable, so a note that
    /// cannot be kept is dropped: a note is only a hint. A slot that does
    /// not decode drops every new note whose `record` reaches it; the
    /// drain's `unlink_next` (slot 0 only) and `unlink_leaf_live` (`find`,
    /// from slot 0 to the first match) reach it only once the notes before
    /// it are used up, and then end Fatal (Census).
    #[inline(never)]
    pub(in super::super::super::super) fn note_emptied_leaf<'x>(
        &mut self,
        (table, anchor, row): (ObjectHandle, PageRef, RowId),
        targets: &[TargetRequest],
        after: impl Fn(usize) -> Option<&'x [u8; PAGE_BYTES]>,
    ) {
        let Some((leaf, image)) = targets
            .iter()
            .enumerate()
            .find(|(_, target)| target.kind == PageKind::RowLeaf)
            .and_then(|(at, target)| Some((*target, after(at)?)))
        else {
            return;
        };
        let empty = tree_page::decode_tree_page(
            image,
            TreeExpectation {
                identity: leaf.identity,
                kind: PageKind::RowLeaf,
                level: 0,
                arity: 0,
            },
        )
        .is_ok_and(|view| view.len() == 0);
        if empty
            && leaf.identity.owner == table.object
            && leaf.identity.owner_generation == table.generation
        {
            let _ = note::record(self.control, note::Note { table, anchor, row });
        }
    }

    /// One live unlink step for the kept note of (`table`, `key`)
    /// (dev3-tree-smo.md#empty-leaf-unlink): `Unlinked` is one installed,
    /// durable LeafUnlink or RootCollapse; `FlushRequired` names a dirty
    /// victim to flush before the same call again; `NotNeeded` did nothing.
    /// A key with no kept note is Unchanged `InvalidInput`. The note is
    /// dropped after `Unlinked`, `NotNeeded` and every Unchanged refusal that
    /// is not of the Busy class (`RetentionError::unlink_busy`); it stays
    /// after `FlushRequired`, a Busy-class refusal and a Fatal one. A refusal
    /// before the stage is Unchanged unless it is an invariant breach, which
    /// is Fatal; a failure after the stage is Fatal.
    pub(in crate::storage) fn unlink_leaf_live(
        &mut self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        key: RowId,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowUnlinkStep> {
        let storage = self.storage;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        let Some(kept) = note::find(self.control, table, key).map_err(census)? else {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::RowSemantic(row::Error::InvalidInput),
            ));
        };
        let tree = RowTree {
            handle: table,
            anchor: PageIdentity {
                database: storage.profile().spec().namespace.database,
                page: kept.anchor,
                owner: table.object,
                owner_generation: table.generation,
            },
        };
        let result = self.structural_shell(owner, flusher, |writer, owner, _| {
            writer.unlink_live_admitted(owner, tree, key, private)
        });
        let drop = match &result {
            Ok(RowUnlinkStep::Unlinked(_) | RowUnlinkStep::NotNeeded) => true,
            Ok(RowUnlinkStep::FlushRequired(_)) => false,
            Err(error) => error.effect == Effect::Unchanged && !error.unlink_busy(),
        };
        if drop {
            note::remove(self.control, table, key).map_err(census)?;
        }
        result
    }

    /// The residency step, then, for a target, the preparation and install
    /// in their own frame. The residency's values do not exist during them.
    #[inline(never)]
    fn unlink_live_admitted(
        &mut self,
        owner: &mut Owner<'c, 's>,
        tree: RowTree,
        key: RowId,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<RowUnlinkStep> {
        match self.unlink_resident(owner, tree, key, private)? {
            UnlinkResidency::Target(leaf) => {
                self.unlink_row_admitted(owner, tree.handle, leaf, private)
            }
            UnlinkResidency::Flush(target) => Ok(RowUnlinkStep::FlushRequired(target)),
            UnlinkResidency::NotNeeded => Ok(RowUnlinkStep::NotNeeded),
        }
    }

    /// The same-cut canonical preparation, the table's split debt at that
    /// cut, then the existing-group install. The preparation writes the
    /// group into the one `Result` slot below and the install borrows it
    /// there, as `retire_resident` does.
    #[inline(never)]
    fn unlink_row_admitted(
        &mut self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        leaf: PageRef,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<RowUnlinkStep> {
        let result = self
            .catalog_preparation()
            .prepare_unlink(owner, table, leaf, private);
        let prepared = match result {
            Ok(Some(ref prepared)) => prepared,
            Ok(None) => return Ok(RowUnlinkStep::NotNeeded),
            Err(error) => return Err(error),
        };
        self.check_split_debt(table)?;
        #[cfg(test)]
        dirty_frame::init_test_support::structural_hit(
            self.pool,
            dirty_frame::init_test_support::InitPoint::UnlinkPrepared,
        );
        self.catalog_application()
            .apply_existing(owner, ExistingGroup::Structural(prepared))?;
        Ok(RowUnlinkStep::Unlinked(prepared.observation.operation))
    }

    /// The anchor, the route of the key from the anchor's current root with
    /// each RowInternal made resident and read for its child, the routed
    /// leaf and its emptiness, its parent's separator count (and, under a
    /// root parent of one separator, the survivor), the table's split debt
    /// (`check_split_debt`), then the group's pages made resident with each
    /// LOAD protecting the set. Each LOAD is the one retention LOAD body; no
    /// pin survives a step, so every page read after a LOAD is one that LOAD
    /// protected (the leaf's LOAD keeps the parent read after it). A route
    /// that ends at the root, a leaf that holds a cell and a non-root parent
    /// of one separator are NotNeeded, whatever the table's debt.
    #[inline(never)]
    fn unlink_resident(
        &self,
        owner: &mut Owner<'c, 's>,
        tree: RowTree,
        key: RowId,
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<UnlinkResidency> {
        let storage = self.storage;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let mut cursor = Plan4ResidencyCursor::new();
        let anchor = TargetRequest {
            identity: tree.anchor,
            kind: PageKind::TreeAnchor,
            level: 0,
        };
        if let Some(target) = self.split_load(owner, &mut cursor, anchor, &[anchor], private)? {
            return Ok(UnlinkResidency::Flush(target));
        }
        let mut route = RowRouteCursor::start(tree, self.table_root(owner, tree)?, key)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut path = [anchor; ROUTE];
        let mut depth = 0;
        while let Some(internal) = route.internal(tree) {
            if depth + 1 >= ROUTE {
                return Err(invariant());
            }
            if let Some(target) =
                self.split_load(owner, &mut cursor, internal, &[anchor, internal], private)?
            {
                return Ok(UnlinkResidency::Flush(target));
            }
            self.route_step(owner, tree, internal, &mut route)?;
            path[depth] = internal;
            depth += 1;
        }
        if depth == 0 {
            return Ok(UnlinkResidency::NotNeeded);
        }
        let leaf = route.leaf(tree);
        // The parent is read after this LOAD (`unlink_parent_shape`), so the
        // LOAD keeps it beside the anchor: it can never be the victim.
        let parent = path[depth - 1];
        if let Some(target) =
            self.split_load(owner, &mut cursor, leaf, &[anchor, parent, leaf], private)?
        {
            return Ok(UnlinkResidency::Flush(target));
        }
        if !self.unlink_leaf_empty(owner, leaf)? {
            return Ok(UnlinkResidency::NotNeeded);
        }
        let mut set = [anchor, parent, leaf, leaf];
        let count = match self.unlink_parent_shape(owner, parent, leaf)? {
            (separators, _) if separators >= 2 => 3,
            (1, Some(survivor)) if depth == 1 => {
                set[3] = survivor;
                4
            }
            _ => return Ok(UnlinkResidency::NotNeeded),
        };
        self.check_split_debt(tree.handle)?;
        let set = &set[..count];
        for request in set {
            if let Some(flush) = self.split_load(owner, &mut cursor, *request, set, private)? {
                return Ok(UnlinkResidency::Flush(flush));
            }
        }
        Ok(UnlinkResidency::Target(leaf.identity.page))
    }

    /// Whether the resident RowLeaf `leaf` holds no cell, under a brief
    /// typed read.
    fn unlink_leaf_empty(&self, owner: &mut Owner<'c, 's>, leaf: TargetRequest) -> Result<bool> {
        let storage = self.storage;
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[leaf])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let empty = (|| {
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let view = tree_page::decode_tree_page(
                bytes,
                TreeExpectation {
                    identity: leaf.identity,
                    kind: PageKind::RowLeaf,
                    level: 0,
                    arity: 0,
                },
            )
            .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Tree(e)))?;
            Ok(view.len() == 0)
        })();
        merge(
            empty,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// The resident level-1 parent's separator count and, when it has one
    /// separator, its other child (the survivor of a RootCollapse), under a
    /// brief typed read. `leaf` must be one of its children.
    fn unlink_parent_shape(
        &self,
        owner: &mut Owner<'c, 's>,
        parent: TargetRequest,
        leaf: TargetRequest,
    ) -> Result<(usize, Option<TargetRequest>)> {
        let storage = self.storage;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[parent])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let shape = (|| {
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let view = tree_page::decode_tree_page(
                bytes,
                TreeExpectation {
                    identity: parent.identity,
                    kind: PageKind::RowInternal,
                    level: parent.level,
                    arity: 0,
                },
            )
            .map_err(|e| census(RetentionCause::Tree(e)))?;
            let child = |at| {
                view.child_expectation(at)
                    .map_err(|e| census(RetentionCause::Tree(e)))
            };
            let mut at = None;
            for index in 0..=view.len() {
                if child(index)?.identity == leaf.identity {
                    at = Some(index);
                }
            }
            let at = at.ok_or_else(|| census(RetentionCause::Invariant))?;
            if parent.level != 1 || view.len() != 1 {
                return Ok((view.len(), None));
            }
            let survivor = child(usize::from(at == 0))?;
            if survivor.kind != PageKind::RowLeaf || survivor.level != 0 {
                return Err(census(RetentionCause::Invariant));
            }
            Ok((
                1,
                Some(TargetRequest {
                    identity: survivor.identity,
                    kind: PageKind::RowLeaf,
                    level: 0,
                }),
            ))
        })();
        merge(
            shape,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }
}

/// The prepared live unlink group: its frame, the two after images (the
/// anchor; LeafUnlink's parent) and the XOI successor in the caller's Row
/// loan, its physical targets in lock order with each target's candidate
/// slot and before stamp. No Clone/Copy and no raw constructor: only
/// `prepare_unlink` builds it, and the existing-group body borrows it once.
pub(in crate::storage) struct PreparedUnlink<'i, 'c, 's> {
    core: &'c PageConcurrency<'s>,
    binding: &'s AtomicU8,
    fence: &'s SessionFence,
    owner: OwnerIdentity,
    prefix: WalPrefix,
    control: ControlCut,
    observation: unlink_frame::Observation,
    frame: &'i [u8],
    frame_digest: [u8; 32],
    after: &'i [[u8; PAGE_BYTES]; 2],
    inventory_after: &'i [u8; PAGE_BYTES],
    requests: [TargetRequest; 4],
    slots: [usize; 4],
    stamps: [Option<FrameStamp>; 4],
    count: usize,
}

impl<'c, 's> GroupPayload<'c, 's> for PreparedUnlink<'_, 'c, 's> {
    fn core(&self) -> &'c PageConcurrency<'s> {
        self.core
    }
    fn binding(&self) -> &'s AtomicU8 {
        self.binding
    }
    fn fence(&self) -> &'s SessionFence {
        self.fence
    }
    fn prefix(&self) -> WalPrefix {
        self.prefix
    }
    fn lsn(&self) -> u64 {
        self.observation.header.lsn
    }
    fn frame(&self) -> &[u8] {
        self.frame
    }
    fn frame_digest(&self) -> [u8; 32] {
        self.frame_digest
    }
    fn cut(&self) -> ControlCut {
        self.control
    }
}

impl<'i, 'c, 's> PreparedUnlink<'i, 'c, 's> {
    pub(in super::super::super) fn check(
        &self,
        owner: &Owner<'c, 's>,
        pool: &SharedPool<'c, 's, '_, '_>,
        control: &control::RetentionLedger<'_, 'c, 's>,
        prefix: WalPrefix,
    ) -> Result<()> {
        check_group(self, self.owner, owner, pool, control, prefix)
    }
    pub(in super::super::super) fn targets(&self) -> &[TargetRequest] {
        &self.requests[..self.count]
    }
    pub(in super::super::super) fn stamp(&self, at: usize) -> Option<FrameStamp> {
        self.stamps
            .get(at)
            .copied()
            .flatten()
            .filter(|_| at < self.count)
    }
    /// The anchor's after image, and LeafUnlink's parent's; the retired and
    /// survivor pages only have their stamps matched.
    pub(in super::super::super) fn after(&self, at: usize) -> Option<&'i [u8; PAGE_BYTES]> {
        match *self.slots.get(at).filter(|_| at < self.count)? {
            ANCHOR => Some(&self.after[0]),
            PARENT if self.observation.operation == smo::Operation::LeafUnlink => {
                Some(&self.after[1])
            }
            _ => None,
        }
    }
    pub(in super::super::super) fn stage_parts(
        &self,
    ) -> (
        WalPrefix,
        crate::storage::wal_frame::WalFrameHeader,
        &'i [u8],
    ) {
        (self.prefix, self.observation.header, self.frame)
    }
    pub(in super::super::super) fn mark_staged(
        &self,
        control: &mut control::RetentionLedger<'_, '_, 's>,
        requirement: WalRequirement<'s>,
    ) -> Result<()> {
        mark_staged_group(self, control, requirement)
    }
    /// The control part, `StructuralControlCommit`'s rule: the Structural
    /// bank row becomes the frame's Finished descriptor with the group's LSN
    /// as high-water, STABLE, and the XOI successor encoded before the
    /// stage replaces the cache. The canonical replay's unlink arm writes the
    /// same row (`history::structure::apply_unlink`).
    pub(in super::super::super) fn apply_control(
        &self,
        control: &mut control::RetentionLedger<'_, '_, '_>,
        profile: crate::storage::control::Profile,
        requirement: WalRequirement<'_>,
    ) -> Result<()> {
        check_staged_group(self, control, requirement)?;
        let view = unlink_smo::decode_layout(
            self.frame,
            FrameExpectation {
                stream: self.prefix.stream,
                previous_lsn: self.prefix.complete_lsn,
            },
            profile,
        )
        .map_err(semantic)?;
        let row = control.row_mut(control::STRUCTURAL_SLOT);
        row.fill(0);
        row[..1536].copy_from_slice(view.descriptor());
        row[1536..1544].copy_from_slice(&self.observation.header.lsn.to_le_bytes());
        row[control::PHASE] = control::STABLE;
        control.bytes[JOB_END..INVENTORY_END].copy_from_slice(self.inventory_after);
        Ok(())
    }
    #[cfg(test)]
    pub(in crate::storage) fn test_operation(&self) -> smo::Operation {
        self.observation.operation
    }
}

/// The group's candidate slots in candidate order: anchor, parent, leaf,
/// and a RootCollapse's survivor.
const SLOTS: [usize; 4] = [ANCHOR, PARENT, LEAF, SURVIVOR];

impl<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W: WriterNamespace>
    CatalogPreparation<'b, 'a, 's, 'c, 'p, 'd, 'm, 'w, W>
{
    /// The unlink of the exact current leaf `leaf` of `table`, recomputed by
    /// `unlink::frame::produce` from the canonical replay of this cut with
    /// the original Jobs and Structural bank row; None when that cut needs
    /// no unlink. The group's resident pages must equal the canonical images;
    /// their stamps are read under one typed read. `check_progress_room` and
    /// the XOI successor are checked before the first effect.
    #[inline(never)]
    pub(super) fn prepare_unlink<'i>(
        &self,
        owner: &mut Owner<'c, 's>,
        table: ObjectHandle,
        leaf: PageRef,
        private: &'i mut [u8; PRIVATE_BYTES],
    ) -> Result<Option<PreparedUnlink<'i, 'c, 's>>> {
        let storage = self.storage;
        let profile = storage.profile();
        let _unwind = storage.session_fence().unwind_guard();
        let (owner_identity, prefix) = self.existing_entry(owner)?;
        let cut = ControlCut::capture(self.control);
        let (frame_space, rest) = private.split_at_mut(RowLoanLayout::FRAME.end);
        let (before_space, rest) = rest.split_at_mut(range_len(RowLoanLayout::BEFORE));
        let (after_space, _) = rest.split_at_mut(range_len(RowLoanLayout::AFTER));
        let after: &mut [[u8; PAGE_BYTES]; 2] = (&mut after_space.as_chunks_mut::<PAGE_BYTES>().0
            [..2])
            .try_into()
            .expect("two Row after loans");
        let inventory: &mut [u8; PAGE_BYTES] = (&mut before_space[..PAGE_BYTES])
            .try_into()
            .expect("one Row before loan");
        let prepared = {
            let mut audit = self.history.replay_scope(owner)?;
            let mut replay = audit.checkout()?;
            if let Err(error) =
                history::validate_live(storage, self.control, replay.bytes_mut(), prefix)
            {
                let rejected = merge(Err(error), replay.return_checked());
                return merge(rejected, audit.finish_checked());
            }
            let prepared = (|| {
                let bank_row = self.control.row(control::STRUCTURAL_SLOT);
                let previous = control::observe_structural_bank_row(
                    profile,
                    prefix.complete_lsn,
                    prefix.stream.writer_epoch,
                    bank_row,
                )
                .map_err(|cause| fatal(storage, RetentionPhase::Census, cause))?;
                let inputs = candidate::Inputs {
                    profile,
                    frame: FrameExpectation {
                        stream: prefix.stream,
                        previous_lsn: prefix.complete_lsn,
                    },
                    table,
                    source: leaf,
                    jobs: JobCells::original(self.control.jobs()),
                    previous_high_water: previous.map_or(0, |p| p.owner.id()),
                    previous: previous.map(|p| {
                        (
                            p.owner,
                            bank_row[..1536]
                                .try_into()
                                .expect("checked bank descriptor"),
                        )
                    }),
                };
                let produced = unlink_frame::produce(&inputs, after, frame_space, |reference| {
                    canonical_image(replay.bytes(), reference)
                        .map_err(|_| row::Error::IdentityMismatch)
                })
                .map_err(semantic)?;
                let unlink_frame::Produced::Frame(observed) = produced else {
                    return Ok(None);
                };
                let count = observed.operation.natives();
                let mut slots = SLOTS;
                slots[..count]
                    .sort_unstable_by_key(|slot| target_rank(request(observed.targets[*slot])));
                let requests = slots.map(|slot| request(observed.targets[slot]));
                let stamps = match count {
                    3 => self.read_unlink_before::<3>(owner, &requests, replay.bytes()),
                    4 => self.read_unlink_before::<4>(owner, &requests, replay.bytes()),
                    _ => Err(fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Invariant,
                    )),
                }?;
                let root = history::arena_page(replay.bytes(), 1);
                let future = prepare_inventory(
                    self.control,
                    profile,
                    observed.header.lsn,
                    GroupRows {
                        transaction: None,
                        aging: None,
                        job_ordinal: None,
                    },
                    root,
                    history::arena_page(replay.bytes(), 0),
                    None,
                    inventory,
                )?;
                // Only the anchor and a LeafUnlink's parent take counter + 1.
                let order: &[u8] = if observed.operation == smo::Operation::LeafUnlink {
                    &[ANCHOR as u8, PARENT as u8]
                } else {
                    &[ANCHOR as u8]
                };
                check_progress_room(
                    prefix,
                    (observed.header.lsn, observed.frame_bytes),
                    future,
                    (&slots.map(|slot| slot as u8)[..count], &stamps[..count]),
                    order,
                    root,
                    profile,
                )?;
                Ok(Some((observed, requests, slots, stamps, count)))
            })();
            let prepared = merge(prepared, replay.return_checked());
            merge(prepared, audit.finish_checked())?
        };
        let Some((observation, requests, slots, stamps, count)) = prepared else {
            return Ok(None);
        };
        let current = storage
            .wal()
            .prefix()
            .map_err(|e| fatal(storage, RetentionPhase::Prepare, RetentionCause::Wal(e)))?;
        unchanged_cut(prefix, current, cut, self.control)?;
        let bytes = &frame_space[..observation.frame_bytes];
        Ok(Some(PreparedUnlink {
            core: self.pool.bound_core(),
            binding: &storage.binding,
            fence: storage.session_fence(),
            owner: owner_identity,
            prefix,
            control: cut,
            observation,
            frame: bytes,
            frame_digest: content_digest(bytes),
            after,
            inventory_after: inventory,
            requests,
            slots,
            stamps,
            count,
        }))
    }

    /// The group's `N` resident targets under one typed read: each page
    /// must equal its canonical replay image at the expected level, and its
    /// stamp is the install's expected stamp.
    #[inline(never)]
    fn read_unlink_before<const N: usize>(
        &self,
        owner: &mut Owner<'c, 's>,
        requests: &[TargetRequest; 4],
        arena: &[u8],
    ) -> Result<[Option<FrameStamp>; 4]> {
        let storage = self.storage;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        let requests: &[TargetRequest; N] = requests[..N]
            .try_into()
            .map_err(|_| census(RetentionCause::Invariant))?;
        let read = self
            .access
            .read(self.pool, owner, requests)
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let result = (|| {
            let mut stamps = [None; 4];
            for (at, target) in requests.iter().enumerate() {
                let bytes = read
                    .bytes(at)
                    .map_err(|e| frame(RetentionPhase::Prepare, e))?;
                stamps[at] = Some(
                    read.observe(at)
                        .map_err(|e| frame(RetentionPhase::Prepare, e))?
                        .frame,
                );
                let canonical = canonical_image(arena, target.identity.page).map_err(census)?;
                let header = page::decode_page(bytes, target.identity, target.kind)
                    .map_err(|e| census(RetentionCause::Page(e)))?
                    .header();
                if bytes != canonical || header.level != target.level {
                    return Err(census(RetentionCause::Invariant));
                }
            }
            Ok(stamps)
        })();
        merge(
            result,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }
}

/// The live unlink's C phases (dev3-tree-smo.md#empty-leaf-unlink, 호출자),
/// each with the structural shell's entry packet: `residency` is
/// `unlink_resident` under its caller, `prepare` the same-cut preparation
/// (`prepare_unlink`) and `install` the existing-group install at four
/// targets with the unlink control part, both in `unlink_row_admitted`'s
/// frame under the same caller, and `peak` the largest. A's caller bound
/// `LIVE_ROW_CLEANUP` is added by the census, which checks `peak` against
/// CatalogProgressCharge by name; nothing folds it into the charge or a
/// `peak`.
#[derive(Debug)]
pub(in crate::storage) struct UnlinkSource {
    pub(in crate::storage) carriers: usize,
    pub(in crate::storage) residency: usize,
    pub(in crate::storage) prepare: usize,
    pub(in crate::storage) install: usize,
    pub(in crate::storage) peak: usize,
}

/// `UnlinkSource` for writer `W`. The carriers are what the live entry
/// holds through every phase besides the shell's entry packet:
/// `unlink_leaf_live`'s note and its lookup Results, the tree, the table and
/// key, the borrowed arguments and the body closure's captures, the
/// `Result<RowUnlinkStep>` of `unlink_leaf_live`, the shell's body,
/// `unlink_live_admitted` and `unlink_row_admitted`, the drop flag and
/// `unlink_live_admitted`'s slot for `unlink_resident`'s result.
/// `unlink_resident`'s frame holds the LOAD cursor, the anchor, internal,
/// leaf and parent requests, the route cursor, the path, the group set, the
/// anchor's current root, the step results and the parent shape; its
/// largest call is one LOAD (`split_load`), a route callee (`table_root`,
/// `route_step`), a brief typed read with its tree decode and child walk
/// (`unlink_leaf_empty`, `unlink_parent_shape`) or the split debt's Job
/// decode (`check_split_debt`). `unlink_row_admitted`'s frame holds the
/// preparation's one Result slot (the install borrows the group there), the
/// borrow, a refused preparation's error and the install's group argument;
/// its calls are the preparation, the split debt and the install.
pub(in crate::storage) fn source_bytes<W: WriterNamespace>() -> UnlinkSource {
    use super::super::super::row_preflight::{
        route_carrier_bytes, route_step_source_bytes, table_root_source_bytes,
    };
    use crate::storage::{
        dirty_frame, semantic_catalog::page_read_codec_bytes, tree_page::TreePageView,
    };
    use std::mem::size_of;
    type Read<'a> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, 1>;
    type Shape = (usize, Option<TargetRequest>);
    let reference = size_of::<&()>();
    let shell = super::shell_entry_bytes();
    let carriers = size_of::<Option<note::Note>>()
        + 2 * size_of::<std::result::Result<Option<note::Note>, RetentionCause>>()
        + 2 * size_of::<RowTree>()
        + 2 * size_of::<(ObjectHandle, RowId)>()
        + 4 * size_of::<Result<RowUnlinkStep>>()
        + size_of::<Result<UnlinkResidency>>()
        + size_of::<std::result::Result<(), RetentionCause>>()
        + size_of::<bool>()
        + 16 * reference;
    let frame = size_of::<Plan4ResidencyCursor>()
        + 4 * size_of::<TargetRequest>()
        + size_of::<Option<RowRouteCursor>>()
        + route_carrier_bytes()
        + size_of::<[TargetRequest; ROUTE]>()
        + size_of::<[TargetRequest; 4]>()
        + size_of::<Result<crate::storage::anchor::AnchorRoot>>()
        + size_of::<Option<TargetRequest>>()
        + 2 * size_of::<Result<bool>>()
        + 2 * size_of::<Result<Shape>>()
        + 2 * size_of::<Result<Option<Plan4Target>>>()
        + 2 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>()
        + size_of::<[usize; 8]>()
        // The LOADs' protected sets ([anchor], [anchor, internal],
        // [anchor, parent, leaf]) and the set loop's request copy.
        + size_of::<[TargetRequest; 7]>()
        + 16 * reference;
    // A brief typed read: the request, the read and its page, the closure's
    // values and Results (the child walk's expectations and position),
    // beside the tree decode of the leaf or the parent.
    let read = size_of::<[TargetRequest; 1]>()
        + 2 * size_of::<Result<bool>>()
        + 2 * size_of::<Result<Shape>>()
        + 2 * size_of::<Result<TreeExpectation>>()
        + size_of::<TreeExpectation>()
        + size_of::<TargetRequest>()
        + 2 * size_of::<Option<usize>>()
        + 4 * size_of::<usize>()
        + 8 * reference
        + targets::call_bytes::<1>().max(
            size_of::<Read<'_>>()
                + 2 * size_of::<dirty_frame::Result<&[u8; PAGE_BYTES]>>()
                + 2 * size_of::<Result<()>>()
                + size_of::<TreePageView<'_>>()
                + size_of::<TreeExpectation>()
                + 4 * size_of::<usize>()
                + tree_page::codec_source_bytes(page_read_codec_bytes()),
        );
    let callee = super::live::load_step_bytes::<W>()
        .max(table_root_source_bytes())
        .max(route_step_source_bytes())
        .max(read)
        .max(super::live::split_debt_bytes());
    let residency = shell + carriers + frame + callee;
    let admitted = size_of::<Result<Option<PreparedUnlink<'_, '_, '_>>>>()
        + size_of::<&PreparedUnlink<'_, '_, '_>>()
        + size_of::<RetentionError>()
        + size_of::<ExistingGroup<'_, '_, '_, '_>>()
        + size_of::<smo::Operation>()
        + 2 * size_of::<Result<()>>()
        + 8 * reference;
    let prepare = shell + carriers + admitted + prepare_source_bytes::<W>();
    let install = shell
        + carriers
        + admitted
        + install_source_bytes::<W>().max(super::live::split_debt_bytes());
    UnlinkSource {
        carriers,
        residency,
        prepare,
        install,
        peak: residency.max(prepare).max(install),
    }
}

/// `prepare_unlink` and its calls, the split preparation's phases
/// (`structural_prepare.rs`'s `source_census`) with the unlink producer: the
/// outer frame with the loan slices and the scope's packet, the replay scope
/// and either the canonical replay (`validate_live`), the produce call with
/// the candidate inputs and the previous descriptor, the typed before read
/// at four targets, the XOI successor and progress-room check, the bank
/// observation or the scope's return; then the group's return. The
/// unchanged entry is the shared original entry; its prepare/replay bound
/// covers `check_ledger`.
fn prepare_source_bytes<W: WriterNamespace>() -> usize {
    use crate::storage::semantic_row::structure::bank::FinishedStructural;
    use std::mem::size_of;
    type Packet = (
        unlink_frame::Observation,
        [TargetRequest; 4],
        [usize; 4],
        [Option<FrameStamp>; 4],
        usize,
    );
    let outer = size_of::<CatalogPreparation<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + 2 * size_of::<WalPrefix>()
        + size_of::<OwnerIdentity>()
        + size_of::<Result<(OwnerIdentity, WalPrefix)>>()
        + size_of::<ControlCut>()
        + size_of::<ObjectHandle>()
        + size_of::<PageRef>()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 2 * size_of::<Result<Option<Packet>>>()
        + 16 * size_of::<&[u8]>()
        + 12 * size_of::<usize>()
        + 4 * size_of::<Result<()>>()
        + 2 * size_of::<RetentionError>();
    let scope = size_of::<history::runtime::ReplayScope<'_, '_, '_, '_>>()
        + size_of::<history::runtime::ReplayLoan<'_, '_, '_, '_>>()
        + 2 * size_of::<Result<history::runtime::ReplayScope<'_, '_, '_, '_>>>()
        + 2 * size_of::<Result<history::runtime::ReplayLoan<'_, '_, '_, '_>>>();
    let produced = size_of::<unlink_frame::Produced>()
        + size_of::<std::result::Result<unlink_frame::Produced, row::Error>>()
        + size_of::<Result<unlink_frame::Produced>>();
    let semantic = size_of::<candidate::Inputs<'_>>()
        + 2 * size_of::<Option<FinishedStructural>>()
        + produced
        + 8 * size_of::<&[u8]>()
        + unlink_smo::source_bytes();
    // The typed before read: the inputs and the Observation stay live, the
    // sorted slots and requests, the stamps' Results, then the read's own
    // values beside its acquisition or one page decode at a time.
    let physical = size_of::<candidate::Inputs<'_>>()
        + 2 * size_of::<Option<FinishedStructural>>()
        + produced
        + size_of::<[usize; 4]>()
        + size_of::<[TargetRequest; 4]>()
        + 3 * size_of::<Result<[Option<FrameStamp>; 4]>>()
        + size_of::<targets::CatalogRead<'_, '_, '_, '_, '_, '_, '_, 4>>()
        + size_of::<[Option<FrameStamp>; 4]>()
        + 2 * size_of::<TargetRequest>()
        + size_of::<page::PageHeader>()
        + size_of::<page::PageView<'_>>()
        + 2 * size_of::<std::result::Result<page::PageView<'_>, page::PageError>>()
        + 2 * size_of::<crate::storage::dirty_frame::Result<&[u8; PAGE_BYTES]>>()
        + 2 * size_of::<
            crate::storage::dirty_frame::Result<crate::storage::dirty_frame::DirtyObservation>,
        >()
        + 2 * size_of::<std::result::Result<&[u8; PAGE_BYTES], RetentionCause>>()
        + 16 * size_of::<&[u8]>()
        + 512
        + targets::call_bytes::<4>()
            .max(crate::storage::semantic_catalog::page_read_codec_bytes() + 1536);
    let cleanup = size_of::<Packet>()
        + 3 * size_of::<Result<Option<Packet>>>()
        + history::runtime::lookup_source_census().returning;
    // The candidate inputs and the Observation remain live while the
    // original XOI encoder and progress checker run sequentially.
    let inventory = size_of::<Packet>()
        + size_of::<candidate::Inputs<'_>>()
        + 2 * size_of::<Option<FinishedStructural>>()
        + produced
        + size_of::<ProjectedInventory<'_, '_, '_, '_, '_>>()
        + size_of::<GroupRows<'_>>()
        + size_of::<[u8; 4]>()
        + size_of::<&[u8]>()
        + 3 * size_of::<Result<(u64, u64)>>()
        + 24 * size_of::<&[u8]>()
        + 24 * size_of::<u64>()
        + crate::storage::plan4_recovery_base::catalog::inventory::row_current_source_bytes()
            .max(queue::source_bytes());
    let processing = outer
        + scope
        + semantic
            .max(physical)
            .max(inventory)
            .max(control::structural_bank_source_bytes())
            .max(cleanup);
    let replay = outer + scope + history::source_bytes::<W>().peak;
    let returning = outer
        + size_of::<Packet>()
        + size_of::<PreparedUnlink<'_, '_, '_>>()
        + 2 * size_of::<Result<Option<PreparedUnlink<'_, '_, '_>>>>()
        + 2048; // hashing closure and check/return scalars, no page copy
    replay
        .max(processing)
        .max(returning)
        .max(super::super::super::catalog_live_source_bytes::<W>().prepare_replay)
}

/// The existing-group install at four targets (`apply_existing_n::<4>`)
/// beside the application view: the payload kind and its forwarders, the
/// DPT stamps, requirement, durable and grant values, then the consecutive
/// typed write, the owner/cut/digest check, the unlink control part, the
/// physical stage/durable calls and the (empty) epoch change.
fn install_source_bytes<W: WriterNamespace>() -> usize {
    use crate::storage::dirty_frame;
    use std::mem::size_of;
    let physical = crate::storage::wal_io::DURABILITY_CALL_BYTES
        + 2 * size_of::<std::result::Result<WalRequirement<'_>, crate::storage::wal_io::WalIoError>>(
        )
        + 2 * size_of::<std::result::Result<DurableWal<'_>, crate::storage::wal_io::WalIoError>>();
    // apply_control: the staged prelude, the layout decode, the bank row and
    // the XOI copy.
    let control = 2 * size_of::<ControlCut>()
        + size_of::<(
            &mut control::RetentionLedger<'_, '_, '_>,
            crate::storage::control::Profile,
            WalRequirement<'_>,
        )>()
        + size_of::<FrameExpectation>()
        + size_of::<unlink_smo::LayoutView<'_>>()
        + 2 * size_of::<std::result::Result<unlink_smo::LayoutView<'_>, row::Error>>()
        + 2 * size_of::<WalRequirement<'_>>()
        + 4 * size_of::<Result<()>>()
        + 6 * size_of::<&()>()
        + unlink_smo::layout_source_bytes();
    let check = size_of::<OwnerIdentity>()
        + 2 * size_of::<Result<OwnerIdentity>>()
        + size_of::<foundation::codec::ContentHasher>()
        + 1536;
    super::super::super::lifecycle::application_carrier_bytes::<W>()
        + visibility::epoch_change_carriers()
        + size_of::<crate::storage::session_fence::UnwindFence<'_>>()
        + 2 * size_of::<WalPrefix>()
        + size_of::<[Option<dirty_frame::DptStamp<'_>>; 4]>()
        + 3 * size_of::<WalRequirement<'_>>()
        + 2 * size_of::<DurableWal<'_>>()
        + 2 * size_of::<InstallGrant<'_, '_, '_>>()
        + 4 * size_of::<Result<()>>()
        + 4 * size_of::<RetentionError>()
        + 16 * size_of::<usize>()
        + 16 * size_of::<&()>()
        + targets::call_bytes::<4>()
        + control
            .max(check)
            .max(physical)
            .max(visibility::epoch_transition_source_bytes())
}

/// UNL-C03: `note::region` for the census beside the shared layout.
#[cfg(test)]
pub(in crate::storage) fn note_region() -> (std::ops::Range<usize>, [usize; 3]) {
    note::region()
}

/// The note writes after a live DeletePrune's or INSERT CLR's install:
/// `note_emptied_leaf`'s arguments and closure, the found RowLeaf and its
/// after image, the tree decode (or one `note::record`: a slot decode and
/// the encode), and `delete_prune_note`'s Job decode before a DeletePrune's
/// preparation. Each caller adds it beside its install (or preparation).
pub(in crate::storage) fn note_source_bytes() -> usize {
    use crate::storage::{semantic_catalog::page_read_codec_bytes, tree_page::TreePageView};
    use std::mem::size_of;
    let reference = size_of::<&()>();
    let emptied = size_of::<(ObjectHandle, PageRef, RowId)>()
        + size_of::<&[TargetRequest]>()
        + 2 * size_of::<Option<(TargetRequest, &[u8; PAGE_BYTES])>>()
        + size_of::<TreeExpectation>()
        + size_of::<bool>()
        + size_of::<note::Note>()
        + 8 * reference
        + (size_of::<TreePageView<'_>>()
            + size_of::<std::result::Result<TreePageView<'_>, tree_page::TreeError>>()
            + tree_page::codec_source_bytes(page_read_codec_bytes()))
        .max(
            2 * size_of::<std::result::Result<Option<note::Note>, RetentionCause>>()
                + size_of::<note::Note>()
                + size_of::<std::result::Result<PageRef, crate::storage::page::PageError>>()
                + size_of::<[usize; 8]>()
                + 8 * reference,
        );
    let prune = size_of::<Result<Option<(ObjectHandle, PageRef, RowId)>>>()
        + size_of::<job3::JobView<'_>>()
        + 2 * size_of::<
            std::result::Result<job3::JobView<'_>, crate::storage::retention_page::RetentionError>,
        >()
        + 8 * reference
        + job3::source_bytes();
    emptied.max(prune)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(object: u64) -> ObjectHandle {
        ObjectHandle {
            object: ObjectId::new(object).unwrap(),
            generation: Generation::new(1).unwrap(),
            schema_epoch: SchemaEpoch::new(1).unwrap(),
        }
    }

    fn anchor(page: u64) -> PageRef {
        PageRef {
            file: crate::storage::page::FileId::new(0x1_0000_0002).unwrap(),
            page_no: page,
            incarnation: Generation::new(1).unwrap(),
        }
    }

    fn kept(control: &control::RetentionLedger<'_, '_, '_>) -> Vec<(u64, u64)> {
        (0..note::COUNT)
            .filter_map(|at| note::get(control, at).unwrap())
            .map(|kept| (kept.table.object.get(), kept.row.get()))
            .collect()
    }

    /// UNL-A02 (dev3-tree-smo.md#empty-leaf-unlink, 호출자): the RAM notes
    /// are at most eight, in arrival order, in the bank region's last 448 B
    /// past its whole rows; a kept note is not kept twice, a ninth is
    /// dropped, a removed note's later notes move up, and an all-zero slot is
    /// empty. The integer fields are checked against independent
    /// little-endian bytes, the anchor reference against `PageRef::encode`.
    #[test]
    fn unl_a02_notes_keep_eight_and_drop_the_ninth() {
        let mut bytes = Box::new([0u8; RETENTION_CONTROL_BYTES]);
        let mut control = control::RetentionLedger::new(&mut bytes);
        assert_eq!(note::get(&control, 0).unwrap(), None);
        for row in 1..=9u64 {
            note::record(
                &mut control,
                note::Note {
                    table: handle(40 + row),
                    anchor: anchor(2),
                    row: RowId::new(row).unwrap(),
                },
            )
            .unwrap();
        }
        let first = note::Note {
            table: handle(41),
            anchor: anchor(2),
            row: RowId::new(1).unwrap(),
        };
        note::record(&mut control, first).unwrap();
        assert_eq!(
            kept(&control),
            (1..=8).map(|row| (40 + row, row)).collect::<Vec<_>>()
        );
        // The first slot's bytes: object, generation, schema epoch, the
        // anchor reference, the row.
        let start = 65_536 - 8 * 56;
        assert!(start >= (control::STRUCTURAL_SLOT + 1) * control::SLOT_BYTES);
        let slot = &control.bytes[start..start + 56];
        assert_eq!(&slot[0..8], &41u64.to_le_bytes());
        assert_eq!(&slot[8..16], &1u64.to_le_bytes());
        assert_eq!(&slot[16..24], &1u64.to_le_bytes());
        let mut reference = [0u8; 24];
        anchor(2).encode(&mut reference).unwrap();
        assert_eq!(&slot[24..48], &reference);
        assert_eq!(&slot[48..56], &1u64.to_le_bytes());
        assert!(control.bytes[..start].iter().all(|byte| *byte == 0));
        assert_eq!(
            note::find(&control, handle(43), RowId::new(3).unwrap())
                .unwrap()
                .map(|n| n.anchor),
            Some(anchor(2))
        );
        assert_eq!(
            note::find(&control, handle(43), RowId::new(4).unwrap()).unwrap(),
            None
        );

        note::remove(&mut control, handle(43), RowId::new(3).unwrap()).unwrap();
        assert_eq!(
            kept(&control),
            [
                (41, 1),
                (42, 2),
                (44, 4),
                (45, 5),
                (46, 6),
                (47, 7),
                (48, 8)
            ]
        );
        assert!(
            control.bytes[65_536 - 56..65_536]
                .iter()
                .all(|byte| *byte == 0)
        );
        // A removed note's slot takes the next new note.
        note::record(
            &mut control,
            note::Note {
                table: handle(49),
                anchor: anchor(3),
                row: RowId::new(9).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(kept(&control).last(), Some(&(49, 9)));
        // Removing an absent note changes nothing.
        let before = control.bytes[start..].to_vec();
        note::remove(&mut control, handle(43), RowId::new(3).unwrap()).unwrap();
        assert_eq!(control.bytes[start..], before[..]);
        // An undecodable slot is an invariant breach.
        control.bytes[start + 8..start + 16].fill(0);
        assert!(matches!(
            note::get(&control, 0),
            Err(RetentionCause::Invariant)
        ));
    }
}
