//! The live split of a full RowLeaf for an issued table
//! (dev3-tree-smo.md#automatic-split): A's entry, after its INSERT's or
//! UPDATE's value check refused Capacity or its Reserve was refused
//! `Unchanged(Prepare, Capacity)`, and the preflight finished. The table's
//! issue is checked here; the residency,
//! the need check and the original split run in C's structural body
//! (`split_row_tree`) under the one structural shell.
use super::super::create_pair::catalog_live::row_preflight::RowTableSource;
pub(in crate::storage) use super::super::create_pair::catalog_live::structural::effect::live::RowSplitStep;
use super::*;

/// Upper bound of A's driver share at the live split call of a Row write
/// (`split_row_live`, dev3-tree-smo.md#automatic-split): what the driver
/// holds while the split runs, with the row loop's frame and no Row
/// wrapper. C checks `split body + carriers + ROW_SPLIT_CALLER_BYTES ≤
/// CatalogProgressCharge` in both writers, and A checks its split call-site
/// shares against it. The value is the preflight's (A's share at the split
/// is the preflight's frames less the attempt and plus the split step); it
/// is not the largest value that fits. Changing it reruns both censuses.
pub(crate) const ROW_SPLIT_CALLER_BYTES: usize = 6144;

impl<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W: WriterNamespace>
    MutationPort<'b, 'a, 's, 'c, 'p, 'd, 'o, 'm, 'w, W>
{
    /// One live split step for a row of `table` that Reserve refused for
    /// Capacity: `row` is an UPDATE's target and None an INSERT's, `length`
    /// the row's new value length (`RowInputPlan::encoded_len`). `Split` is
    /// one installed, durable RootSplit or ChildSplit, after which the caller
    /// tries the row again from preflight; `FlushRequired` names a dirty
    /// victim to flush before the same call again; `NotNeeded` did nothing,
    /// since the routed leaf already holds the row beside its reservation.
    /// A table of another session or pool is Unchanged BindingMismatch. The
    /// split's own admission and refusals are `split_row`'s.
    pub(in crate::storage) fn split_row_live(
        &mut self,
        owner: &mut Owner<'c, 's>,
        table: &CheckedTable<'c, 's>,
        row: Option<RowId>,
        length: usize,
        private: &mut [u8; 425_984],
        flusher: &mut MaintenancePort<'_, 's, 'c, 'p, 'd, 'm, 'w, W>,
    ) -> Result<RowSplitStep> {
        let storage = self.storage;
        if !std::ptr::eq(table.core, self.pool.bound_core())
            || !std::ptr::eq(table.binding, &storage.binding)
            || !std::ptr::eq(table.fence, storage.session_fence())
        {
            return Err(unchanged(
                RetentionPhase::Prepare,
                RetentionCause::BindingMismatch,
            ));
        }
        let tree = RowTableSource {
            handle: table.table,
            slot: table.slot,
            anchor: table.anchor,
        }
        .tree();
        self.split_row_tree(owner, tree, row, length, private, flusher)
    }
}
