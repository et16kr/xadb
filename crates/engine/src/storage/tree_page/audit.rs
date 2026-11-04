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
    mut image: impl FnMut(TreeExpectation) -> Result<&'images [u8], TreeError>,
) -> Result<usize, TreeError> {
    validate_shape(root.kind, root.level, root.arity)?;
    if visited.is_empty() {
        return Err(TreeError::NoSpace);
    }
    visited.fill(None);
    let mut stack: [Option<Frame<'images>>; 16] = [const { None }; 16];
    visited[0] = Some(root);
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
        if depth + 1 >= stack.len() {
            return Err(TreeError::Corrupt);
        }
        visited[count] = Some(expected);
        count += 1;
        depth += 1;
        stack[depth] = Some(frame(expected, lower, upper, &mut image)?);
    }
}

#[cfg(test)]
mod tests;
