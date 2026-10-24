//! A recovery-only initial binding. The selected owner supplies canonical bytes
//! and the original fence/binding; observations cannot manufacture this seed.
use super::*;
use crate::storage::{page::decode_page, wal_io::RecoveryPoolSeed};
use xadb_foundation::codec::content_digest;

fn prepare_allocator(
    core: &PageConcurrency<'_>,
    fence: &SessionFence,
    binding: &AtomicU8,
    frames: &mut [DirtyFrame<'_>],
    seed: &RecoveryPoolSeed<'_>,
) -> Result<()> {
    if fence.is_fenced() {
        return Err(DirtyError {
            cause: Cause::Fenced,
            effect: Effect::CallerMustResolve,
        });
    }
    if !core.uses_fence(fence)
        || !std::ptr::eq(seed.fence(), fence)
        || !std::ptr::eq(seed.binding(), binding)
    {
        return Err(unchanged(Cause::BindingMismatch));
    }
    if !(4..=MAX_FRAMES).contains(&frames.len()) {
        return Err(unchanged(Cause::Shape));
    }
    for frame in frames.iter_mut() {
        frame.check_empty()?;
    }
    let request = seed.request();
    if request.kind != PageKind::Allocation
        || request.level != 0
        || content_digest(seed.allocator()) != seed.digest()
    {
        return Err(unchanged(Cause::Invariant));
    }
    let page = decode_page(seed.allocator(), request.identity, request.kind)
        .map_err(|_| unchanged(Cause::Invariant))?;
    if page.header().lsn != seed.lsn() || page.header().level != request.level {
        return Err(unchanged(Cause::Invariant));
    }
    // Every fallible seed/frame check precedes the first payload copy. The
    // second empty check cannot acquire another borrow or change these frames.
    frames[0]
        .initial_payload()?
        .copy_from_slice(seed.allocator());
    frames[0].register(request)
}

impl<'c, 's, 'p, 'd> SharedPool<'c, 's, 'p, 'd> {
    pub(in crate::storage) fn new_recovered_plan4(
        core: &'c PageConcurrency<'s>,
        fence: &'s SessionFence,
        binding: &'s AtomicU8,
        frames: &'p mut [DirtyFrame<'d>],
        seed: &RecoveryPoolSeed<'_>,
    ) -> Result<Self> {
        prepare_allocator(core, fence, binding, frames, seed)?;
        let mut pool = Self::new(core, fence, binding, frames).map_err(|error| {
            fence.fence();
            DirtyError {
                cause: error.cause,
                effect: Effect::CallerMustResolve,
            }
        })?;
        // No view escapes the temporary clean directory. It is converted
        // before restoring metadata from the authenticated physical image.
        if !pool.directory.open_plan4(pool.frames)
            || !pool.directory.activate_retention(pool.frames)
        {
            return Err(pool.fatal(Cause::Invariant));
        }
        {
            let mut content = pool.frames[0]
                .content
                .try_write()
                .map_err(|_| pool.fatal(Cause::Invariant))?;
            if !content.fresh() || content.bytes.as_slice() != seed.allocator().as_slice() {
                return Err(pool.fatal(Cause::Invariant));
            }
            content.page_lsn = seed.lsn();
            content.modification_counter = u64::from(seed.lsn().0 != 0);
            content.rec_lsn = None;
            content.dirty = false;
            // The original DPT has no dirty entry or synthetic WAL requirement.
            pool.pair(0, &content)?;
        }
        pool.health()?;
        Ok(pool)
    }
}
