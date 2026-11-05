//! Independent inventories of the parser and emitter's nested source values.
use super::*;
use crate::storage::{
    allocation_page::AllocationError,
    control::Profile,
    page::{PageError, PageView},
    semantic_catalog,
    wal_frame::{FrameEnvelope, FrameExpectation, RedoRecordView, WalError},
};
use std::mem::size_of;
type R<T, E> = std::result::Result<T, E>;

fn hash() -> usize {
    1536 + size_of::<foundation::codec::ContentHasher>() + 32
}
fn parsing() -> usize {
    let entry = size_of::<(&[u8], FrameExpectation, Profile)>()
        + size_of::<FrameEnvelope<'_>>()
        + size_of::<R<FrameEnvelope<'_>, WalError>>()
        + size_of::<super::super::CoreView<'_>>()
        + size_of::<Result<super::super::CoreView<'_>>>()
        + size_of::<super::super::LayoutView<'_>>()
        + size_of::<Result<super::super::LayoutView<'_>>>()
        + 256;
    let owner = size_of::<(super::super::LayoutView<'_>, &[u8], Profile)>()
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
    let records = size_of::<(super::super::LayoutView<'_>, super::super::CoreView<'_>)>()
        + size_of::<(&[u8], u8, Option<(u64, u64)>)>()
        + size_of::<super::super::NativeView<'_>>()
        + size_of::<(RedoTarget, PageRef, u8, usize)>()
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
        &[[u8; PAGE_BYTES]; 5],
        &mut Cursor<'_>,
    )>() + size_of::<([u8; 1536], [u8; 192], [u8; 128], [usize; 5])>()
        + size_of::<(PageHeader, Cursor<'_>, usize)>()
        + 256;
    let descriptor = size_of::<(&Inputs<'_>, &Candidate<'_>, &mut [u8; 1536])>()
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
    let redo = size_of::<(&mut Cursor<'_>, &Candidate<'_>, usize, &[u8; PAGE_BYTES])>()
        + size_of::<(bool, &[u8], RedoTarget, usize)>()
        + size_of::<RedoRecordView<'_>>()
        + size_of::<R<RedoRecordView<'_>, WalError>>()
        + size_of::<(WalFrameHeader, PageHeader)>()
        + 256
        + semantic_catalog::page_read_codec_bytes()
        + 1536;
    let ordering = size_of::<[(u64, u64, usize); 5]>()
        + size_of::<[u8; 24]>()
        + 2 * size_of::<[usize; 5]>()
        + 256;
    body + descriptor.max(redo).max(ordering).max(hash())
}

#[test]
fn structural_frame_source_inventory_covers_parser_and_emit_nesting() {
    let actual = source_census();
    let (parser, emitter) = (parsing(), emitting());
    println!(
        "frame parser {parser}/{} emitter {emitter}/{}",
        actual.layout, actual.body
    );
    assert!(parser <= actual.layout);
    assert!(emitter <= actual.body);
    assert!(actual.outer + parser.max(emitter).max(actual.candidate) <= source_bytes());
}
