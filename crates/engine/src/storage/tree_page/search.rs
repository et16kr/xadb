//! Bounded search of one immutable, owner-selected tree image set.
//!
//! Replay and recovery can borrow their canonical arena for the whole call.
//! A live pool must instead stabilize its anchor/path and pin/latch lifetimes;
//! this function does not turn arbitrary borrowed pages into live authority.
use super::*;

/// Search-owned source values, excluding callback captures and its nested
/// image/decoder closure. Includes the full sixteen-level address history.
pub(crate) fn frame_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<[Option<PageRef>; 16]>()
        + 4 * size_of::<TreeExpectation>()
        + 4 * size_of::<Option<&[u8]>>()
        + 4 * size_of::<TreePageView<'_>>()
        + 4 * size_of::<Result<TreePageView<'_>, TreeError>>()
        + 4 * size_of::<CheckedKey<'_>>()
        + 4 * size_of::<Result<CheckedKey<'_>, TreeError>>()
        + 4 * size_of::<TreeEntry<'_>>()
        + 4 * size_of::<Result<TreeEntry<'_>, TreeError>>()
        + 2 * size_of::<SearchPosition>()
        + 32 * size_of::<usize>()
}

/// Follow the selected root to the leaf for `query`, checking each exact
/// owner/incarnation/family/level/arity and inherited key range before using
/// its child edge. Only the selected path is checked; this is not full audit.
/// No allocation or page-sized scratch is used. The source must stay immutable
/// for this call, including the parent bytes borrowed as descendant bounds.
pub(crate) fn find_leaf_in_images<'images>(
    root: TreeExpectation,
    query: &[u8],
    mut image: impl FnMut(TreeExpectation) -> Result<&'images [u8], TreeError>,
) -> Result<TreePageView<'images>, TreeError> {
    validate_shape(root.kind, root.level, root.arity)?;
    checked_key(root.kind, root.arity, query).map_err(|_| TreeError::InvalidInput)?;
    let mut expected = root;
    let mut lower: Option<&'images [u8]> = None;
    let mut upper: Option<&'images [u8]> = None;
    let mut visited = [None; 16];
    for depth in 0..16 {
        if visited[..depth]
            .iter()
            .flatten()
            .any(|prior| same_address(*prior, expected.identity.page))
        {
            return Err(TreeError::Corrupt);
        }
        visited[depth] = Some(expected.identity.page);
        let view = decode_tree_page(image(expected)?, expected)?;
        // Strict local order is established by the decoder. Its extrema
        // therefore prove every key lies within the inherited subtree range.
        if view.len() != 0 {
            let first = checked_key(expected.kind, expected.arity, view.entry(0)?.key)?;
            let last = checked_key(
                expected.kind,
                expected.arity,
                view.entry(view.len() - 1)?.key,
            )?;
            if let Some(bound) = lower
                && first.compare(checked_key(expected.kind, expected.arity, bound)?)
                    == Ordering::Less
            {
                return Err(TreeError::Corrupt);
            }
            if let Some(bound) = upper
                && last.compare(checked_key(expected.kind, expected.arity, bound)?)
                    != Ordering::Less
            {
                return Err(TreeError::Corrupt);
            }
        }
        if expected.level == 0 {
            return Ok(view);
        }
        let at = view.lower_bound(query)?;
        let child = at.index + usize::from(at.equal);
        if child > 0 {
            lower = Some(view.entry(child - 1)?.key);
        }
        if child < view.len() {
            upper = Some(view.entry(child)?.key);
        }
        expected = view.child_expectation(child)?;
    }
    // Valid roots have level at most 15 and each exact child decreases it.
    // Reaching here cannot be accepted as a missing row.
    Err(TreeError::Corrupt)
}

/// The inherited key range of one Row path step: the separators of the
/// parents above it, copied out of their pages (Row keys are 8 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowRouteBounds {
    pub(crate) lower: Option<[u8; 8]>,
    pub(crate) upper: Option<[u8; 8]>,
}

/// One step of a Row route read page by page from a live pool
/// (dev3-tree-smo.md#current-leaf-consumers): decode `bytes` as `expected`,
/// check that its keys lie in `bounds`, and for a RowInternal page return the
/// child that routes `key` with `bounds` narrowed to that child. A RowLeaf
/// returns None. This is `find_leaf_in_images`' loop body for a caller that
/// holds each page only under its own brief read; the caller bounds the
/// depth and keeps its anchor revision stable across the steps.
pub(crate) fn route_row_step(
    bytes: &[u8],
    expected: TreeExpectation,
    key: &[u8; 8],
    bounds: &mut RowRouteBounds,
) -> Result<Option<TreeExpectation>, TreeError> {
    if !matches!(expected.kind, PageKind::RowLeaf | PageKind::RowInternal) || expected.arity != 0 {
        return Err(TreeError::InvalidInput);
    }
    let view = decode_tree_page(bytes, expected)?;
    if view.len() != 0 {
        let first = view.entry(0)?.key;
        let last = view.entry(view.len() - 1)?.key;
        if bounds.lower.is_some_and(|bound| first < bound.as_slice())
            || bounds.upper.is_some_and(|bound| last >= bound.as_slice())
        {
            return Err(TreeError::Corrupt);
        }
    }
    if expected.level == 0 {
        return Ok(None);
    }
    let at = view.lower_bound(key)?;
    let child = at.index + usize::from(at.equal);
    let copy = |key: &[u8]| -> Result<[u8; 8], TreeError> {
        key.try_into().map_err(|_| TreeError::Corrupt)
    };
    if child > 0 {
        bounds.lower = Some(copy(view.entry(child - 1)?.key)?);
    }
    if child < view.len() {
        bounds.upper = Some(copy(view.entry(child)?.key)?);
    }
    view.child_expectation(child).map(Some)
}

/// `route_row_step`'s own values beside one tree decode.
pub(crate) fn route_step_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<(&[u8], TreeExpectation, &[u8; 8], &mut RowRouteBounds)>()
        + size_of::<TreePageView<'_>>()
        + 2 * size_of::<Result<TreePageView<'_>, TreeError>>()
        + 3 * size_of::<&[u8]>()
        + 3 * size_of::<Result<TreeEntry<'_>, TreeError>>()
        + size_of::<Result<SearchPosition, TreeError>>()
        + 2 * size_of::<usize>()
        + 2 * size_of::<Result<[u8; 8], TreeError>>()
        + size_of::<Result<Option<TreeExpectation>, TreeError>>()
}

#[cfg(test)]
mod tests;
