//! The live split of a full RowLeaf (dev3-tree-smo.md#automatic-split): the
//! C entry A calls when its INSERT's or UPDATE's value check refused
//! Capacity or its Reserve was refused for Capacity. Under the one
//! structural shell it routes the row's key from the table anchor's current
//! root, checks that the routed leaf really lacks the space
//! (`check_candidate`'s rule, `leaf_short`), checks the table's split debt,
//! picks the page to split, makes the split set resident and looks ahead at
//! the INIT victims, then runs the original split (`split_row_admitted`)
//! unchanged. A LOAD or a
//! victim that needs a dirty frame flushed ends the call with that frame,
//! before anything is prepared.
use super::super::super::super::super::lookup_load::RetentionLoadPort as _;
use super::super::super::CatalogTargetAccess;
use super::super::super::lifecycle::row::write::{MIN_CANONICAL, leaf_short};
use super::super::super::row_preflight::{RowRouteCursor, RowTree, leaf_free_bytes};
use super::*;
use crate::storage::{
    retention_page::row as job3,
    semantic_row::{self as row_semantic, transition as row_group},
    tree_page::{self, EntryPayload, TreeExpectation, search},
};
use foundation::identity::RowId;

/// What one live split call did (`split_row_tree`). It carries no split
/// observation: the caller only tries its row again, and the live entry's
/// result slots stay small (dev3-tree-smo.md#automatic-split, F4).
#[derive(Debug)]
pub(in crate::storage) enum RowSplitStep {
    /// One RootSplit or ChildSplit group is installed and its frame is
    /// durable. The caller tries its row again from preflight.
    Split,
    /// The residency of the route or of the split set needs this dirty
    /// victim flushed first. Nothing was prepared, staged or pinned.
    FlushRequired(Plan4Target),
    /// The routed RowLeaf already holds the row beside its reservation D: a
    /// split would not admit it, so nothing was done.
    NotNeeded,
}

/// What the residency step found.
enum SplitResidency {
    /// The page to split, resident with the anchor and its parent.
    Target(PageRef),
    Flush(Plan4Target),
    NotNeeded,
}

/// A route from the root to the leaf: the root's level is at most 15.
const ROUTE: usize = 16;

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    RetentionWriter<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// The live split for one refused row of `tree`
    /// (dev3-tree-smo.md#automatic-split): `row` is an UPDATE's target and
    /// None an INSERT's (Root `row_high+1`, the rightmost leaf), `length`
    /// the row's new value length (`RowInputPlan::encoded_len`). The split's
    /// admission and refusals are `split_row`'s. A refusal before the effect
    /// is Unchanged unless it is an invariant breach, which is Fatal.
    pub(in super::super::super::super::super) fn split_row_tree(
        &mut self,
        owner: &mut Owner<'c, 's>,
        tree: RowTree,
        row: Option<RowId>,
        length: usize,
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowSplitStep> {
        self.structural_shell(owner, flusher, |writer, owner, flusher| {
            writer.split_live_admitted(owner, tree, (row, length), private, flusher)
        })
    }

    /// The length, the residency step (with the need check and the table's
    /// split debt), then, for a target, the original split in its own frame.
    /// The residency's values do not exist during the split.
    #[inline(never)]
    fn split_live_admitted(
        &mut self,
        owner: &mut Owner<'c, 's>,
        tree: RowTree,
        need: (Option<RowId>, usize),
        private: &mut [u8; PRIVATE_BYTES],
        flusher: &mut RetentionFlusher<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowSplitStep> {
        // A's length is the value's canonical encoding; one outside the
        // Row value bounds names no row C would reserve.
        if !(MIN_CANONICAL..=crate::storage::overflow::INLINE_ROW_BYTES).contains(&need.1) {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::RowSemantic(row_semantic::Error::InvalidInput),
            ));
        }
        match self.split_resident(owner, tree, need, private)? {
            SplitResidency::Target(target) => self
                .split_row_admitted(owner, tree.handle, target, private, flusher)
                .map(|_| RowSplitStep::Split),
            SplitResidency::Flush(target) => Ok(RowSplitStep::FlushRequired(target)),
            SplitResidency::NotNeeded => Ok(RowSplitStep::NotNeeded),
        }
    }

    /// The anchor (and an INSERT's Root), the route of the key from the
    /// anchor's current root with each RowInternal made resident and read
    /// for its child and its separator room, the routed leaf and its need
    /// check, the table's split debt (`check_split_debt`: a split C would
    /// refuse is refused here, before the split set is read), the split set
    /// (anchor, target, parent) made resident with each LOAD protecting the
    /// set, then the INIT victim look-ahead. Each LOAD is the one retention
    /// LOAD body; no pin survives a step. A row whose leaf holds it is
    /// NotNeeded whatever the table's debt, so A keeps the row's own
    /// failure.
    #[inline(never)]
    fn split_resident(
        &self,
        owner: &mut Owner<'c, 's>,
        tree: RowTree,
        (row, length): (Option<RowId>, usize),
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<SplitResidency> {
        let storage = self.storage;
        let invariant = || fatal(storage, RetentionPhase::Census, RetentionCause::Invariant);
        let mut cursor = Plan4ResidencyCursor::new();
        let anchor = TargetRequest {
            identity: tree.anchor,
            kind: PageKind::TreeAnchor,
            level: 0,
        };
        if let Some(target) = self.split_load(owner, &mut cursor, anchor, &[anchor], private)? {
            return Ok(SplitResidency::Flush(target));
        }
        let key = match row {
            Some(row) => row,
            None => {
                let root = target_from_source(storage, Plan4Page::Root)?.request;
                if let Some(target) =
                    self.split_load(owner, &mut cursor, root, &[anchor, root], private)?
                {
                    return Ok(SplitResidency::Flush(target));
                }
                self.next_row_key(owner, root)?
            }
        };
        let mut route = RowRouteCursor::start(tree, self.table_root(owner, tree)?, key)
            .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Invariant))?;
        let mut path = [anchor; ROUTE];
        let mut room = [false; ROUTE];
        let mut depth = 0;
        while let Some(internal) = route.internal(tree) {
            if depth + 1 >= ROUTE {
                return Err(invariant());
            }
            if let Some(target) =
                self.split_load(owner, &mut cursor, internal, &[anchor, internal], private)?
            {
                return Ok(SplitResidency::Flush(target));
            }
            room[depth] = self.separator_room(owner, internal)?;
            self.route_step(owner, tree, internal, &mut route)?;
            path[depth] = internal;
            depth += 1;
        }
        let leaf = route.leaf(tree);
        path[depth] = leaf;
        if let Some(target) = self.split_load(owner, &mut cursor, leaf, &[anchor, leaf], private)? {
            return Ok(SplitResidency::Flush(target));
        }
        if !self.leaf_needs_split(owner, tree, leaf, (row, length))? {
            return Ok(SplitResidency::NotNeeded);
        }
        self.check_split_debt(tree.handle)?;
        let at = search::split_target(&room[..=depth]).ok_or_else(invariant)?;
        let target = path[at];
        let set = [anchor, target, path[at.saturating_sub(1)]];
        let set = &set[..if at == 0 { 2 } else { 3 }];
        for request in set {
            if let Some(flush) = self.split_load(owner, &mut cursor, *request, set, private)? {
                return Ok(SplitResidency::Flush(flush));
            }
        }
        let allocator = self
            .pool
            .target_request(0)
            .map_err(|e| frame(RetentionPhase::Load, e))?;
        let mut required = [allocator; 4];
        required[1..=set.len()].copy_from_slice(set);
        if let Some(victim) = self
            .pool
            .structural_flush_victim(&required[..=set.len()], if at == 0 { 2 } else { 1 })
            .map_err(|e| frame(RetentionPhase::Prepare, e))?
        {
            return Ok(SplitResidency::Flush(Plan4Target { request: victim }));
        }
        Ok(SplitResidency::Target(target.identity.page))
    }

    /// One LOAD step of `request` protecting `protected`: None once it is
    /// resident, else the dirty victim to flush.
    fn split_load(
        &self,
        owner: &mut Owner<'c, 's>,
        cursor: &mut Plan4ResidencyCursor,
        request: TargetRequest,
        protected: &[TargetRequest],
        private: &mut [u8; PRIVATE_BYTES],
    ) -> Result<Option<Plan4Target>> {
        Ok(
            match self.load_lookup_target(owner, request, cursor, protected, &mut private[..])? {
                RetentionResidencyStep::Resident { .. } => None,
                RetentionResidencyStep::FlushRequired { target, .. } => Some(target),
            },
        )
    }

    /// An INSERT's route key, `row_preflight`'s: Root's `row_high+1` under a
    /// brief typed read of the resident Root. Exhausted RowIds are Capacity.
    fn next_row_key(&self, owner: &mut Owner<'c, 's>, root: TargetRequest) -> Result<RowId> {
        let storage = self.storage;
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[root])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let key = (|| {
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let (_, root) = job3::decode_root_page(bytes, storage.profile().spec().namespace)
                .map_err(|e| {
                    fatal(
                        storage,
                        RetentionPhase::Census,
                        RetentionCause::Retention(e),
                    )
                })?;
            root.row_high()
                .checked_add(1)
                .and_then(|row| RowId::new(row).ok())
                .ok_or_else(|| unchanged(RetentionPhase::Prepare, RetentionCause::Capacity))
        })();
        merge(
            key,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// Whether the resident RowInternal `internal` takes one more separator
    /// (`search::separator_room`), under a brief typed read.
    fn separator_room(&self, owner: &mut Owner<'c, 's>, internal: TargetRequest) -> Result<bool> {
        let storage = self.storage;
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[internal])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let room = (|| {
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let view = tree_page::decode_tree_page(
                bytes,
                TreeExpectation {
                    identity: internal.identity,
                    kind: PageKind::RowInternal,
                    level: internal.level,
                    arity: 0,
                },
            )
            .map_err(|e| fatal(storage, RetentionPhase::Census, RetentionCause::Tree(e)))?;
            Ok(search::separator_room(view.len()))
        })();
        merge(
            room,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }

    /// The need check under a brief typed read of the resident routed leaf
    /// and the control Jobs: `leaf_short` with the leaf's free entry bytes,
    /// its reservation D and, for an UPDATE, the target's old cell. An
    /// UPDATE target the leaf does not hold needs no split.
    fn leaf_needs_split(
        &self,
        owner: &mut Owner<'c, 's>,
        tree: RowTree,
        leaf: TargetRequest,
        (row, length): (Option<RowId>, usize),
    ) -> Result<bool> {
        let storage = self.storage;
        let census = |cause| fatal(storage, RetentionPhase::Census, cause);
        let read = CatalogTargetAccess::Ordinary
            .read(self.pool, owner, &[leaf])
            .map_err(|e| frame(RetentionPhase::Prepare, e))?;
        let short = (|| {
            let bytes = read
                .bytes(0)
                .map_err(|e| frame(RetentionPhase::Prepare, e))?;
            let free = leaf_free_bytes(bytes, leaf).map_err(census)?;
            let reserved = row_group::leaf_reservation(
                self.control.jobs(),
                storage.profile().spec().namespace,
                tree.handle,
                (bytes, leaf.identity),
            )
            .map_err(|e| census(RetentionCause::RowSemantic(e)))?;
            let old_cell = match row {
                None => None,
                Some(row) => match row_cell(bytes, leaf, row).map_err(census)? {
                    Some(cell) => Some(cell),
                    None => return Ok(false),
                },
            };
            Ok(leaf_short(length, old_cell, reserved, free))
        })();
        merge(
            short,
            read.release_checked()
                .map_err(|e| frame(RetentionPhase::Prepare, e)),
        )
    }
}

/// The cell of `row` in the RowLeaf image: its 12-byte head (four header
/// bytes, the eight-byte key) and the record, as `delete_input` sizes it;
/// None when the leaf holds no such key.
fn row_cell(
    bytes: &[u8; PAGE_BYTES],
    leaf: TargetRequest,
    row: RowId,
) -> std::result::Result<Option<usize>, RetentionCause> {
    let view = tree_page::decode_tree_page(
        bytes,
        TreeExpectation {
            identity: leaf.identity,
            kind: PageKind::RowLeaf,
            level: 0,
            arity: 0,
        },
    )
    .map_err(RetentionCause::Tree)?;
    let position = view
        .lower_bound(&row.get().to_be_bytes())
        .map_err(RetentionCause::Tree)?;
    if !position.equal {
        return Ok(None);
    }
    match view
        .entry(position.index)
        .map_err(RetentionCause::Tree)?
        .payload
    {
        EntryPayload::Value(value) => Ok(Some(value.len() + 12)),
        EntryPayload::RightChild(_) => Err(RetentionCause::Invariant),
    }
}

/// The live split's C phases (dev3-tree-smo.md#automatic-split), each with
/// the structural shell's entry packet: `residency` is `split_resident`
/// under its caller, `effect` the original split body
/// (`split_row_admitted`, the effect census's peak) under the same caller,
/// and `peak` the larger. A's caller bound `ROW_SPLIT_CALLER_BYTES` is
/// added by the census, which checks `peak` against CatalogProgressCharge by
/// name; nothing folds it into the charge or `peak`.
#[derive(Debug)]
pub(in crate::storage) struct LiveSource {
    pub(in crate::storage) carriers: usize,
    pub(in crate::storage) residency: usize,
    pub(in crate::storage) effect: usize,
    pub(in crate::storage) peak: usize,
}

/// `LiveSource` for writer `W`. The carriers are what the live entry holds
/// through every phase besides the shell's entry packet: `split_row_live`'s
/// table source and tree, the need, the borrowed arguments and the body
/// closure's captures, the `Result<RowSplitStep>` of `split_row_live`,
/// `split_row_tree`, the shell's body and `split_live_admitted`, and its
/// slot for `split_resident`'s result. `split_resident`'s frame holds the
/// LOAD cursor, the anchor, Root, internal, leaf and target requests, the
/// route cursor, the path and room arrays, the split set and the victim
/// look-ahead's request array, the key, the Root source, the anchor's
/// current root, the route's next internal and the step results; its largest
/// call is one LOAD (the one retention LOAD body under `split_load`), a
/// route callee (`table_root`, `route_step`), a brief typed read with its
/// decode (`next_row_key`, `separator_room`, `leaf_needs_split`), the split
/// debt's Job decode (`check_split_debt`) or the victim look-ahead
/// (`structural_flush_victim`).
pub(in crate::storage) fn source_bytes<W: WriterNamespace>() -> LiveSource {
    use super::super::super::row_preflight::{
        RowTableSource, route_carrier_bytes, route_step_source_bytes, table_root_source_bytes,
    };
    use crate::storage::{
        dirty_frame, semantic_catalog::page_read_codec_bytes, tree_page::TreePageView,
    };
    use std::mem::size_of;
    type Read<'a> = targets::CatalogRead<'a, 'a, 'a, 'a, 'a, 'a, 'a, 1>;
    let reference = size_of::<&()>();
    let effect = super::source_census::<W>();
    let shell = super::shell_entry_bytes();
    let carriers = size_of::<RowTableSource>()
        + 3 * size_of::<RowTree>()
        + 3 * size_of::<(Option<RowId>, usize)>()
        + 4 * size_of::<Result<RowSplitStep>>()
        + size_of::<Result<SplitResidency>>()
        + size_of::<Result<split_frame::Observation>>()
        + 16 * reference;
    let frame = size_of::<Plan4ResidencyCursor>()
        + 5 * size_of::<TargetRequest>()
        + size_of::<Option<RowRouteCursor>>()
        + route_carrier_bytes()
        + size_of::<[TargetRequest; ROUTE]>()
        + size_of::<[bool; ROUTE]>()
        + size_of::<[TargetRequest; 3]>()
        + size_of::<[TargetRequest; 4]>()
        + size_of::<RowId>()
        + size_of::<Result<Plan4Target>>()
        + size_of::<Result<crate::storage::anchor::AnchorRoot>>()
        + size_of::<Option<TargetRequest>>()
        + 2 * size_of::<Result<RowId>>()
        + 2 * size_of::<Result<bool>>()
        + 2 * size_of::<Result<Option<Plan4Target>>>()
        + 2 * size_of::<dirty_frame::Result<Option<TargetRequest>>>()
        + 2 * size_of::<dirty_frame::Result<TargetRequest>>()
        + 2 * size_of::<RetentionError>()
        + size_of::<[usize; 8]>()
        + 16 * reference;
    let load = size_of::<Result<RetentionResidencyStep>>()
        + 6 * reference
        + super::super::super::super::super::lookup_load::source_bytes::<W>();
    // A brief typed read: the request, the read and its page, the closure's
    // values and Results, beside the tree decode of a leaf or internal page
    // (with a leaf's entry walk and key search), the leaf's reservation
    // scan, or the Root decode.
    let read = size_of::<[TargetRequest; 1]>()
        + 2 * size_of::<Result<bool>>()
        + 2 * size_of::<Result<RowId>>()
        + 4 * size_of::<usize>()
        + 2 * size_of::<Option<usize>>()
        + 2 * size_of::<std::result::Result<Option<usize>, RetentionCause>>()
        + 2 * size_of::<std::result::Result<u16, RetentionCause>>()
        + 8 * reference
        + targets::call_bytes::<1>().max(
            size_of::<Read<'_>>()
                + 2 * size_of::<dirty_frame::Result<&[u8; PAGE_BYTES]>>()
                + 2 * size_of::<Result<()>>()
                + (size_of::<TreePageView<'_>>()
                    + size_of::<TreeExpectation>()
                    + size_of::<[u8; 8]>()
                    + 4 * size_of::<usize>()
                    + tree_page::codec_source_bytes(page_read_codec_bytes()))
                .max(row_group::leaf_reservation_source_bytes())
                .max(job3::source_bytes()),
        );
    // `check_split_debt`, under `split_resident`'s frame: one Job decode at
    // a time.
    let debt = size_of::<job3::JobView<'_>>()
        + 2 * size_of::<
            std::result::Result<job3::JobView<'_>, crate::storage::retention_page::RetentionError>,
        >()
        + 8 * reference
        + job3::source_bytes();
    let callee = load
        .max(table_root_source_bytes())
        .max(route_step_source_bytes())
        .max(read)
        .max(debt)
        .max(dirty_frame::structural_selection_source_bytes());
    let residency = shell + carriers + frame + callee;
    // The original split body over the shell's entry (`source_census`'s
    // phases less its entry), under the live entry's slots in place of
    // `split_row`'s three `Result<Observation>`.
    let effect = shell + carriers + (effect.peak - effect.entry);
    LiveSource {
        carriers,
        residency,
        effect,
        peak: residency.max(effect),
    }
}
