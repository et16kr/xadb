//! UNL-R04 (dev3-tree-smo.md#unlink-u2): an inventory of the actual unlink
//! parse/prepare/emit call graph. The nested split chains (table read,
//! audit, previous owner, parent edit) are the split census's own,
//! separately inventoried in candidate/source_tests.rs; this checks that the
//! unlink frames keep their own values across those calls.
use super::{
    super::{
        Result,
        candidate::{
            Inputs, Table, allocation_codec, audit_chain, parent_chain, previous_chain, table_chain,
        },
    },
    CoreView, LayoutView, NativeView,
    candidate::{Candidate, Prepared},
    frame::Observation,
    source::{candidate_census, frame_census, layout_source_bytes, source_bytes},
};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{AllocationError, AllocationPage, SystemWorkRef},
        anchor::{self, AnchorRoot, RetiredList, RetiredPage, TreeAnchor},
        control::Profile,
        obligation::{
            self, DescriptorSpec, InventoryClaim, OwnerChain, RemainingBudget, SystemProgress,
        },
        page::{PAGE_BYTES, PageError, PageHeader, PageKind, PageRef, PageView},
        retention_page::row::JobView,
        semantic_catalog,
        semantic_row::{structure::frame::Cursor, transition},
        tree_page::{
            TreeExpectation, TreePageView,
            structure::{InternalEditPlan, StructureError},
        },
        wal_frame::{
            FrameEnvelope, FrameExpectation, RedoRecordView, RedoTarget, WalError, WalFrameHeader,
        },
    },
};
use std::mem::size_of;
type R<T, E> = std::result::Result<T, E>;

fn hash() -> usize {
    1536 + size_of::<foundation::codec::ContentHasher>() + 32
}
fn controls() -> usize {
    size_of::<([usize; 16], [&[u8]; 8], [PageRef; 4], [u8; 32])>()
}

fn parsing() -> usize {
    let entry = size_of::<(&[u8], FrameExpectation, Profile)>()
        + size_of::<FrameEnvelope<'_>>()
        + size_of::<R<FrameEnvelope<'_>, WalError>>()
        + size_of::<CoreView<'_>>()
        + size_of::<Result<CoreView<'_>>>()
        + size_of::<LayoutView<'_>>()
        + size_of::<Result<LayoutView<'_>>>()
        + 256;
    let owner = size_of::<(WalFrameHeader, &[u8; 1536], usize, &[u8], Profile)>()
        + size_of::<SystemWorkRef>()
        + size_of::<R<SystemWorkRef, AllocationError>>()
        + size_of::<DescriptorSpec>()
        + size_of::<R<DescriptorSpec, obligation::ObligationError>>()
        + size_of::<(
            SystemProgress,
            OwnerChain,
            InventoryClaim,
            [RemainingBudget; 5],
        )>()
        + 128
        + obligation::descriptor_source_bytes().max(hash());
    let records = size_of::<(LayoutView<'_>, CoreView<'_>)>()
        + size_of::<(&[u8], u16, Option<(u64, u64)>, u16)>()
        + size_of::<NativeView<'_>>()
        + size_of::<(RedoTarget, PageKind, u8, PageRef)>()
        + size_of::<RedoRecordView<'_>>()
        + size_of::<R<RedoRecordView<'_>, WalError>>()
        + size_of::<(WalFrameHeader, &[u8], PageView<'_>)>()
        + size_of::<R<PageView<'_>, PageError>>()
        + 256
        + (semantic_catalog::page_read_codec_bytes() + 1536).max(hash());
    entry + owner.max(records).max(hash())
}

fn emitting() -> usize {
    let body = size_of::<(
        &Inputs<'_>,
        &Candidate<'_>,
        &[[u8; PAGE_BYTES]; 2],
        &mut Cursor<'_>,
    )>() + size_of::<([u8; 1536], [u8; 192], [u8; 128], [(u8, usize); 4])>()
        + size_of::<(PageHeader, &[u8; PAGE_BYTES], u16, bool)>()
        + 256;
    let descriptor = size_of::<(Profile, WalFrameHeader, usize, &mut [u8; 1536])>()
        + size_of::<SystemWorkRef>()
        + size_of::<R<SystemWorkRef, AllocationError>>()
        + size_of::<DescriptorSpec>()
        + size_of::<R<DescriptorSpec, obligation::ObligationError>>()
        + size_of::<(
            SystemProgress,
            OwnerChain,
            InventoryClaim,
            [RemainingBudget; 5],
        )>()
        + size_of::<Result<()>>()
        + obligation::descriptor_source_bytes().max(hash());
    let redo = size_of::<(
        &mut Cursor<'_>,
        WalFrameHeader,
        RedoTarget,
        u64,
        &[u8; PAGE_BYTES],
    )>() + size_of::<(bool, &[u8], RedoTarget, usize)>()
        + size_of::<RedoRecordView<'_>>()
        + size_of::<R<RedoRecordView<'_>, WalError>>()
        + size_of::<(WalFrameHeader, PageHeader)>()
        + 256
        + semantic_catalog::page_read_codec_bytes()
        + 1536;
    let ordering = size_of::<[(u64, u64, u8, usize); 4]>()
        + size_of::<[u8; 24]>()
        + size_of::<[(u8, usize); 4]>()
        + 256;
    body + descriptor.max(redo).max(ordering).max(hash())
}

/// `build`'s values after the audit returns (the draft): the fixed arrays
/// come from the page format (TREE_METADATA_BYTES, the XTR1 cell of 55
/// entries) and the internal-edit refs of the split's SPLIT_REFS.
fn draft_values() -> usize {
    let arrays = size_of::<[u8; 64]>()
        + size_of::<[u8; crate::storage::tree_page::TREE_METADATA_BYTES]>()
        + size_of::<[&[u8]; super::super::candidate::SPLIT_REFS]>()
        + size_of::<[u8; 16 + 40 * 55]>()
        + size_of::<([PageHeader; 4], [u8; 256], [u8; 32], [(usize, PageRef); 5])>();
    let retained = size_of::<(&Inputs<'_>, &mut [[u8; PAGE_BYTES]; 2], Namespace, u64)>()
        + size_of::<AllocationPage<'_>>()
        + size_of::<Table<'_>>()
        + 3 * size_of::<TreePageView<'_>>()
        + 4 * size_of::<TreeExpectation>()
        + size_of::<Option<(TreeExpectation, usize)>>()
        + size_of::<[&[u8; PAGE_BYTES]; 4]>()
        + size_of::<RetiredList<'_>>()
        + size_of::<[RetiredPage; 4]>()
        + size_of::<[AnchorRoot; 2]>()
        + size_of::<[PageHeader; 4]>()
        + size_of::<InternalEditPlan<'_, '_>>()
        + size_of::<R<InternalEditPlan<'_, '_>, StructureError>>();
    arrays + retained + controls()
}

/// Each post-audit call of `build` with its own values.
fn draft_calls() -> usize {
    let reached = size_of::<(&Table<'_>, &AllocationPage<'_>, u8, &mut ())>()
        + size_of::<(PageView<'_>, TreeExpectation)>()
        + 256
        + allocation_codec().max(semantic_catalog::page_read_codec_bytes() + 1536);
    let tree =
        size_of::<(TreePageView<'_>, TreeExpectation)>() + 256 + transition::tree_source_bytes();
    // UNL-C01: the XTR1 merge and one whole `encode_listed_anchor`, and the
    // list read (one whole `retired`), each over the page codec with its
    // SHA scratch (anchor_tests.rs checks these call costs).
    let page = semantic_catalog::page_read_codec_bytes() + 1536;
    let anchor = size_of::<(PageHeader, AnchorRoot, &[u8], &mut [u8; PAGE_BYTES])>()
        + size_of::<[RetiredPage; 2]>()
        + size_of::<[RetiredList<'_>; 2]>()
        + 256
        + transition::tree_source_bytes()
            .max(hash())
            .max(anchor::extend_source_bytes())
            .max(anchor::listed_encode_source_bytes(page));
    let retired = anchor::retired_source_bytes(page);
    reached
        .max(tree)
        .max(anchor)
        .max(retired)
        .max(parent_chain())
}

/// `verify_after`'s anchor decode (UNL-C01): one whole `decode_anchor`.
fn after_decode() -> usize {
    size_of::<(TreeAnchor, Result<TreeAnchor>)>()
        + anchor::whole_decode_source_bytes(semantic_catalog::page_read_codec_bytes() + 1536)
}

#[test]
fn unl_r04_unlink_source_inventory_covers_parse_prepare_and_emit() {
    let f = frame_census();
    let c = candidate_census();
    let (parser, emitter) = (parsing(), emitting());
    println!(
        "unlink parser {parser}/{} emitter {emitter}/{}",
        f.layout, f.body
    );
    assert!(parser <= f.layout && f.layout == layout_source_bytes());
    assert!(emitter <= f.body);

    let args = size_of::<(&Inputs<'_>, &mut [[u8; PAGE_BYTES]; 2], &mut ())>();
    let prefix =
        args + size_of::<(
            Namespace,
            u64,
            [u8; 32],
            AllocationPage<'_>,
            Table<'_>,
            [u8; 64],
        )>() + size_of::<Result<Table<'_>>>()
            + size_of::<Result<(usize, [u8; 32])>>()
            + 256;
    let debt = size_of::<(&Inputs<'_>, usize, JobView<'_>, Result<JobView<'_>>)>()
        + 256
        + crate::storage::retention_page::row::source_bytes();
    let prefix_calls = table_chain()
        .max(audit_chain())
        .max(previous_chain())
        .max(debt);
    let (draft, calls) = (draft_values(), draft_calls());
    let wrapper = args + size_of::<(Prepared<'_>, [u8; 32])>() + size_of::<Result<()>>() + 128;
    let after = size_of::<(
        &Inputs<'_>,
        &Candidate<'_>,
        [u8; 32],
        &[[u8; PAGE_BYTES]; 2],
        &mut (),
    )>() + size_of::<(Table<'_>, AllocationPage<'_>, [u8; 64])>()
        + size_of::<(TreeAnchor, Option<PageHeader>)>()
        + size_of::<Result<(usize, [u8; 32])>>()
        + 256;
    println!(
        "unlink prefix {prefix}/{} draft {draft}/{} wrapper {wrapper}/{} after {after}/{} peak {}",
        c.before_outer, c.build_outer, c.outer, c.after_outer, c.peak
    );
    assert!(prefix <= c.before_outer && draft <= c.build_outer);
    assert!(wrapper <= c.outer && after <= c.after_outer);
    let actual = (args + prefix + prefix_calls)
        .max(args + draft + calls)
        .max(wrapper + after + audit_chain().max(table_chain()).max(after_decode()));
    assert!(actual <= c.peak);

    // produce/observe hold the decoded layout and the prepared draft across
    // prepare and body; the Observation is built only after both return.
    let frame_args = size_of::<(&Inputs<'_>, &[u8], &mut [[u8; PAGE_BYTES]; 2])>();
    let caller = frame_args
        + size_of::<(LayoutView<'_>, Result<LayoutView<'_>>)>()
        + size_of::<(Prepared<'_>, Result<Prepared<'_>>)>()
        + size_of::<Cursor<'_>>()
        + 256;
    let returning = frame_args
        + size_of::<(LayoutView<'_>, Candidate<'_>)>()
        + size_of::<(Observation, Result<Observation>)>()
        + 256;
    println!(
        "unlink frame outer {caller}/{} return {returning}/{} candidate {} peak {}",
        f.outer, f.outer, f.candidate, f.peak
    );
    assert!(caller <= f.outer && returning <= f.outer);
    assert!(caller + actual.max(parser).max(emitter) <= f.peak);
    assert_eq!(f.candidate, c.peak);
    assert_eq!(f.peak, source_bytes());
}
