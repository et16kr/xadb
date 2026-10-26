#![allow(dead_code)]
use super::{Phase, WalPrefix, WalRequirement, WalRuntime};
use crate::storage::wal_frame::WalFrameHeader;
use foundation::namespace::WriterNamespace;

// Compile-only sibling access: no constructed runtime and no executed I/O.
fn physical_calls<'m, 's, W: WriterNamespace>(
    runtime: &'s WalRuntime<'m, W>,
    prefix: WalPrefix,
    header: WalFrameHeader,
    bytes: &[u8],
    requirement: &WalRequirement<'s>,
) {
    std::hint::black_box(runtime.is_fenced());
    std::hint::black_box(runtime.prefix()).ok();
    std::hint::black_box(runtime.state(Phase::Observe)).ok();
    std::hint::black_box(runtime.try_stage(bytes)).ok();
    #[cfg(not(test))]
    std::hint::black_box(runtime.stage_inner(bytes, None)).ok();
    #[cfg(test)]
    std::hint::black_box(runtime.stage_inner(bytes, None, || {})).ok();
    std::hint::black_box(runtime.stage_verified(prefix, header, bytes)).ok();
    std::hint::black_box(runtime.stage_verified_required(prefix, header, bytes)).ok();
    std::hint::black_box(runtime.requirement_invariant());
    std::hint::black_box(runtime.first_write_started(requirement)).ok();
    std::hint::black_box(runtime.ensure_durable(requirement)).ok();
    std::hint::black_box(runtime.wait_durable(requirement)).ok();
    std::hint::black_box(runtime.sync_pending()).ok();
}
