//! Source values of the unlink candidate, layout and frame
//! (dev3-tree-smo.md#empty-leaf-unlink) in the split census's model
//! (candidate/source.rs, frame.rs): caller images, after2, the WAL frame and
//! callback captures are charged by the caller. The never-inlined `build`
//! and `verify_after` do not coexist; `prepare` retains the built draft
//! during `verify_after` only. This is not an ABI stack measurement.
use super::{
    CoreView, LayoutView, NativeView,
    candidate::{Candidate, Prepared},
    frame::Observation,
};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{AllocationEntry, AllocationPage, SystemWorkRef},
        anchor::{AnchorRoot, RETIRED_CELL_BYTES, RetiredList, RetiredPage, TreeAnchor},
        obligation::{
            self, DescriptorSpec, InventoryClaim, OwnerChain, RemainingBudget, SystemProgress,
        },
        page::{PAGE_BYTES, PageHeader, PageRef, PageView},
        retention_page::row::JobView,
        semantic_catalog,
        semantic_row::{
            structure::{
                Operation, Result,
                candidate::{
                    Inputs, SPLIT_REFS, Table, allocation_codec, audit_chain, digest, parent_chain,
                    previous_chain, slack, table_chain,
                },
                frame::{Cursor, descriptor_chain, redo_chain},
            },
            transition,
        },
        tree_page::{
            TREE_METADATA_BYTES, TreeExpectation, TreePageView,
            structure::{InternalEditPlan, StructureError},
        },
        wal_frame::{FrameEnvelope, RedoRecordView, RedoTarget, WalFrameHeader},
    },
};
use foundation::codec::ContentHasher;
use std::mem::size_of;

/// `check_debt`: one Job cell decoded at a time.
fn debt_chain() -> usize {
    size_of::<(&Inputs<'_>, usize)>()
        + size_of::<JobView<'_>>()
        + size_of::<Result<JobView<'_>>>()
        + 2 * slack()
        + crate::storage::retention_page::row::source_bytes()
}
/// `candidate::reached`: one allocation entry and one page header decode.
fn reached_chain() -> usize {
    size_of::<(&Table<'_>, &AllocationPage<'_>, u8, &mut ())>()
        + size_of::<AllocationEntry>()
        + size_of::<TreeExpectation>()
        + size_of::<PageView<'_>>()
        + 2 * slack()
        + allocation_codec().max(semantic_catalog::page_read_codec_bytes() + 1536)
}
/// The listed anchor encode: the XTR1 merge, then the sealed page.
fn anchor_chain() -> usize {
    size_of::<(PageHeader, AnchorRoot, &[u8], &mut [u8; PAGE_BYTES])>()
        + 2 * size_of::<RetiredPage>()
        + 2 * size_of::<RetiredList<'_>>()
        + 2 * slack()
        + transition::tree_source_bytes().max(digest())
}
/// Tree page decodes of the leaf, the level-1 pages and the parent.
fn tree_chain() -> usize {
    size_of::<TreePageView<'_>>()
        + size_of::<TreeExpectation>()
        + slack()
        + transition::tree_source_bytes()
        + 1536
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CandidateCensus {
    pub(crate) outer: usize,
    pub(crate) before_outer: usize,
    pub(crate) build_outer: usize,
    pub(crate) after_outer: usize,
    pub(crate) returning: usize,
    pub(crate) peak: usize,
}
/// The split candidate census's phases (candidate/source.rs): `build`'s
/// prefix values (before the audit returns) with its debt, table, audit and
/// previous calls; then its draft values with the post-audit calls; the
/// wrapper retaining the draft during `verify_after`; and the return.
pub(crate) fn candidate_census() -> CandidateCensus {
    let args = size_of::<(&Inputs<'_>, &mut [[u8; PAGE_BYTES]; 2], &mut ())>();
    let outer = args
        + size_of::<Prepared<'_>>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<Result<()>>()
        + 128;
    let before_outer = args
        + size_of::<(Namespace, u64, [u8; 32])>()
        + size_of::<AllocationPage<'_>>()
        + size_of::<Table<'_>>()
        + size_of::<Result<Table<'_>>>()
        + size_of::<[u8; 64]>()
        + 2 * size_of::<Result<(usize, [u8; 32])>>()
        + slack();
    let build_outer = args
        + size_of::<(Namespace, u64, [u8; 32])>()
        + size_of::<AllocationPage<'_>>()
        + size_of::<Table<'_>>()
        + size_of::<[u8; 64]>()
        + size_of::<(usize, [u8; 32], u8)>()
        + 4 * size_of::<TreeExpectation>()
        + 4 * size_of::<&[u8; PAGE_BYTES]>()
        + 3 * size_of::<TreePageView<'_>>()
        + size_of::<Option<(TreeExpectation, usize)>>()
        + size_of::<(usize, Operation, usize, u64)>()
        + size_of::<RetiredList<'_>>()
        + 4 * size_of::<PageHeader>()
        + size_of::<[u8; TREE_METADATA_BYTES]>()
        + size_of::<[&[u8]; SPLIT_REFS]>()
        + size_of::<InternalEditPlan<'_, '_>>()
        + size_of::<std::result::Result<InternalEditPlan<'_, '_>, StructureError>>()
        + 2 * size_of::<AnchorRoot>()
        + 4 * size_of::<RetiredPage>()
        + size_of::<[u8; RETIRED_CELL_BYTES]>()
        + size_of::<(usize, Result<usize>)>()
        + size_of::<[PageHeader; 4]>()
        + size_of::<[u8; 256]>()
        + size_of::<[(usize, PageRef); 5]>()
        + 2 * slack();
    let after_outer = size_of::<(
        &Inputs<'_>,
        &Candidate<'_>,
        [u8; 32],
        &[[u8; PAGE_BYTES]; 2],
        &mut (),
    )>() + size_of::<Namespace>()
        + size_of::<AllocationPage<'_>>()
        + size_of::<Table<'_>>()
        + size_of::<Result<Table<'_>>>()
        + size_of::<[u8; 64]>()
        + size_of::<Result<(usize, [u8; 32])>>()
        + size_of::<(
            &[PageHeader; 4],
            Option<PageHeader>,
            &[[u8; PAGE_BYTES]; 2],
            &mut (),
        )>()
        + 2 * size_of::<RetiredList<'_>>()
        + slack();
    let returning = build_outer
        + size_of::<Candidate<'_>>()
        + size_of::<Result<(Prepared<'_>, [u8; 32])>>()
        + size_of::<Result<Prepared<'_>>>()
        + outer;
    let prefix_calls = table_chain()
        .max(debt_chain())
        .max(audit_chain())
        .max(previous_chain())
        .max(allocation_codec());
    let draft_calls = reached_chain()
        .max(tree_chain())
        .max(parent_chain())
        .max(anchor_chain())
        .max(size_of::<CoreView<'_>>() + slack());
    let anchor_decode = size_of::<TreeAnchor>()
        + size_of::<Result<TreeAnchor>>()
        + semantic_catalog::page_read_codec_bytes()
        + 1536;
    let building =
        (args + 128 + before_outer + prefix_calls).max(args + 128 + build_outer + draft_calls);
    let verifying = outer
        + after_outer
        + audit_chain()
            .max(table_chain())
            .max(allocation_codec())
            .max(anchor_decode);
    CandidateCensus {
        outer,
        before_outer,
        build_outer,
        after_outer,
        returning,
        peak: building.max(verifying).max(returning),
    }
}

/// `unlink::decode_layout` with `validate_finished_owner` and
/// `validate_records`, like the split layout census.
pub(crate) fn layout_source_bytes() -> usize {
    let own = 3 * size_of::<LayoutView<'_>>()
        + 3 * size_of::<Result<LayoutView<'_>>>()
        + 2 * size_of::<CoreView<'_>>()
        + 3 * size_of::<NativeView<'_>>()
        + 3 * size_of::<RedoRecordView<'_>>()
        + 3 * size_of::<FrameEnvelope<'_>>()
        + 3 * size_of::<WalFrameHeader>()
        + 3 * size_of::<RedoTarget>()
        + 2 * size_of::<DescriptorSpec>()
        + 2 * size_of::<std::result::Result<DescriptorSpec, obligation::ObligationError>>()
        + 2 * size_of::<[RemainingBudget; 5]>()
        + 3 * size_of::<(SystemWorkRef, SystemProgress, OwnerChain, InventoryClaim)>()
        + 2048;
    own + obligation::descriptor_source_bytes()
        .max(semantic_catalog::page_read_codec_bytes() + 1536)
        .max(1536 + size_of::<ContentHasher>())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameCensus {
    pub(crate) outer: usize,
    pub(crate) layout: usize,
    pub(crate) candidate: usize,
    pub(crate) body: usize,
    pub(crate) peak: usize,
}
/// `frame::{produce, observe}`. In observe the decoded LayoutView survives
/// prepare and body; the Observation is built after both return.
pub(crate) fn frame_census() -> FrameCensus {
    let args = size_of::<(&Inputs<'_>, &[u8], &mut [[u8; PAGE_BYTES]; 2])>();
    let calls = args
        + size_of::<Prepared<'_>>()
        + size_of::<Result<Prepared<'_>>>()
        + size_of::<LayoutView<'_>>()
        + size_of::<Result<LayoutView<'_>>>()
        + size_of::<Cursor<'_>>()
        + 2 * size_of::<Result<()>>()
        + 256;
    let returning = args
        + size_of::<Candidate<'_>>()
        + size_of::<LayoutView<'_>>()
        + size_of::<Observation>()
        + size_of::<Result<Observation>>()
        + 256;
    let outer = calls.max(returning);
    let order = size_of::<(&Candidate<'_>, [(u64, u64, u8, usize); 4], [u8; 24])>()
        + size_of::<[(u8, usize); 4]>()
        + size_of::<Result<[(u8, usize); 4]>>()
        + slack();
    let body = size_of::<(
        &Inputs<'_>,
        &Candidate<'_>,
        &[[u8; PAGE_BYTES]; 2],
        &mut Cursor<'_>,
    )>() + size_of::<[u8; 1536]>()
        + size_of::<[u8; 192]>()
        + size_of::<[u8; 128]>()
        + size_of::<[(u8, usize); 4]>()
        + size_of::<(Operation, usize, usize, u16, PageHeader)>()
        + size_of::<Cursor<'_>>()
        + 1024
        + descriptor_chain()
            .max(redo_chain())
            .max(order)
            .max(digest());
    let (layout, candidate) = (layout_source_bytes(), candidate_census().peak);
    FrameCensus {
        outer,
        layout,
        candidate,
        body,
        peak: outer + layout.max(candidate).max(body),
    }
}
pub(crate) fn source_bytes() -> usize {
    frame_census().peak
}
