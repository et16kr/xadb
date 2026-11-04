//! Whole root-reachable immutable tree inspection. Allocation membership and
//! orphan detection require the caller to compare the result with its owned
//! page set. This supplies no live pin, INIT or recovery adoption authority.
use super::*;

struct Frame<'images> {
    view: TreePageView<'images>,
    lower: Option<&'images [u8]>,
    upper: Option<&'images [u8]>,
    next_child: usize,
}

/// Traversal-owned source values, excluding the caller's visited workspace,
/// image callback captures and nested page decoder/callback closure.
pub(crate) fn frame_source_bytes() -> usize {
    use std::mem::size_of;
    size_of::<[Option<Frame<'_>>; 16]>()
        + 4 * size_of::<Frame<'_>>()
        + 4 * size_of::<Result<Frame<'_>, TreeError>>()
        + 6 * size_of::<TreeExpectation>()
        + 6 * size_of::<Option<&[u8]>>()
        + 4 * size_of::<CheckedKey<'_>>()
        + 4 * size_of::<Result<CheckedKey<'_>, TreeError>>()
        + 4 * size_of::<TreeEntry<'_>>()
        + 4 * size_of::<Result<TreeEntry<'_>, TreeError>>()
        + 32 * size_of::<usize>()
        // Public wrapper survives private walk: copied root, visit/image
        // borrows and bounded registry counters/seen mask. Its output loan
        // remains caller-owned (generic expectations or compact ordinals).
        + size_of::<TreeExpectation>()
        + 4 * size_of::<&()>()
        + 3 * size_of::<usize>()
}

fn frame<'images>(
    expected: TreeExpectation,
    lower: Option<&'images [u8]>,
    upper: Option<&'images [u8]>,
    image: &mut impl FnMut(TreeExpectation) -> Result<&'images [u8], TreeError>,
) -> Result<Frame<'images>, TreeError> {
    let view = decode_tree_page(image(expected)?, expected)?;
    // The decoder establishes strict local key order. Checking its extrema
    // establishes the inherited subtree range for every local key. Keep this
    // small check local: the existing point-search source census is unchanged.
    if view.len() != 0 {
        let first = checked_key(expected.kind, expected.arity, view.entry(0)?.key)?;
        let last = checked_key(
            expected.kind,
            expected.arity,
            view.entry(view.len() - 1)?.key,
        )?;
        if let Some(bound) = lower
            && first.compare(checked_key(expected.kind, expected.arity, bound)?) == Ordering::Less
        {
            return Err(TreeError::Corrupt);
        }
        if let Some(bound) = upper
            && last.compare(checked_key(expected.kind, expected.arity, bound)?) != Ordering::Less
        {
            return Err(TreeError::Corrupt);
        }
    }
    Ok(Frame {
        view,
        lower,
        upper,
        next_child: 0,
    })
}

/// Inspect every page reachable from an owner-selected root once, in preorder.
/// `visited` supplies the page limit and records exact expectations. Success
/// returns the populated prefix length; its remaining entries are None.
/// On failure the workspace may contain a partial prefix, which is not an
/// accepted tree. Input images must remain immutable for the entire call.
/// Physical aliases (including different incarnations) are rejected before
/// loading the repeated address. No heap allocation or recursive call is used.
pub(crate) fn audit_tree_in_images<'images>(
    root: TreeExpectation,
    visited: &mut [Option<TreeExpectation>],
    image: impl FnMut(TreeExpectation) -> Result<&'images [u8], TreeError>,
) -> Result<usize, TreeError> {
    validate_shape(root.kind, root.level, root.arity)?;
    if visited.is_empty() {
        return Err(TreeError::NoSpace);
    }
    visited.fill(None);
    let mut count = 0;
    walk(
        root,
        |expected| {
            if visited[..count]
                .iter()
                .flatten()
                .any(|prior| same_address(prior.identity.page, expected.identity.page))
            {
                return Err(TreeError::Corrupt);
            }
            if count == visited.len() {
                return Err(TreeError::NoSpace);
            }
            visited[count] = Some(expected);
            count += 1;
            Ok(())
        },
        image,
    )
}

/// Same immutable traversal restricted to physical pages0..63 in the root's
/// file. The populated prefix records preorder page ordinals, not authority.
/// Only the returned count defines this prefix; the unused tail is zero.
/// Aliases across incarnations are rejected before a repeated address loads.
pub(crate) fn audit_tree_in_first_extent<'images>(
    root: TreeExpectation,
    visited: &mut [u8; 64],
    image: impl FnMut(TreeExpectation) -> Result<&'images [u8], TreeError>,
) -> Result<usize, TreeError> {
    validate_shape(root.kind, root.level, root.arity)?;
    visited.fill(0);
    let (mut seen, mut count) = (0u64, 0usize);
    walk(
        root,
        |expected| {
            let page = expected.identity.page;
            if page.file != root.identity.page.file || page.page_no >= 64 {
                return Err(TreeError::Corrupt);
            }
            let bit = 1u64 << page.page_no;
            if seen & bit != 0 {
                return Err(TreeError::Corrupt);
            }
            seen |= bit;
            visited[count] = page.page_no as u8;
            count += 1;
            Ok(())
        },
        image,
    )
}

fn walk<'images>(
    root: TreeExpectation,
    mut visit: impl FnMut(TreeExpectation) -> Result<(), TreeError>,
    mut image: impl FnMut(TreeExpectation) -> Result<&'images [u8], TreeError>,
) -> Result<usize, TreeError> {
    let mut stack: [Option<Frame<'images>>; 16] = [const { None }; 16];
    visit(root)?;
    stack[0] = Some(frame(root, None, None, &mut image)?);
    let mut count = 1;
    let mut depth = 0;
    loop {
        let child = {
            let parent = stack[depth].as_mut().expect("active DFS frame");
            if parent.view.header().level == 0 || parent.next_child > parent.view.len() {
                None
            } else {
                let at = parent.next_child;
                parent.next_child += 1;
                Some((
                    parent.view.child_expectation(at)?,
                    if at == 0 {
                        parent.lower
                    } else {
                        Some(parent.view.entry(at - 1)?.key)
                    },
                    if at == parent.view.len() {
                        parent.upper
                    } else {
                        Some(parent.view.entry(at)?.key)
                    },
                ))
            }
        };
        let Some((expected, lower, upper)) = child else {
            stack[depth] = None;
            if depth == 0 {
                return Ok(count);
            }
            depth -= 1;
            continue;
        };
        visit(expected)?;
        if depth + 1 >= stack.len() {
            return Err(TreeError::Corrupt);
        }
        count += 1;
        depth += 1;
        stack[depth] = Some(frame(expected, lower, upper, &mut image)?);
    }
}

#[cfg(test)]
mod tests;
