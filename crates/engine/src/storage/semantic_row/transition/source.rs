//! Counted source terms of the scope3 Row transition's nested callees.
//!
//! This is not a compiler-stack/RSS estimate. Each term sums `size_of` over
//! the values its call chain keeps live: arguments, locals and both sides of
//! every return boundary (`returned`). A `max` joins completed calls; their
//! still-live parent values are added outside it. Page/status codecs are the
//! Catalog scope2 census terms over the same codecs; the Undo codec and the
//! tree terms repeat its `undo_codec`/`tree_codec`/`leaf_edit` inventories, so
//! neither owner charges a smaller codec (the Undo one without the
//! CatalogValue before parse a Row target cannot reach, see `undo_row_codec`).
//! `slack` is the repo's standard scalar allowance
//! (`retention_page::row::source_bytes`). transition_tests measures each
//! codec chain function by function and requires these terms to cover it.
use super::super::{CoreView, Error, LayoutView, Operation, Result, Shape};
use super::{MAX_RECORD, TableSource};
use crate::{
    api::{Column, DataType, DatabaseId, Namespace},
    storage::{
        allocation_page::{ReservationOwner, SystemWorkRef},
        anchor::{AnchorRoot, TreeAnchor},
        catalog_record::{CatalogColumn, CatalogError},
        control::Profile,
        obligation::{self, DescriptorSpec, ObligationError, RemainingBudget},
        overflow::{OVERFLOW_HEAD_BYTES, OverflowError, OverflowHead},
        page::{PAGE_BYTES, PageError, PageHeader, PageIdentity, PageRef, PageView},
        retention_page::{
            RetentionError,
            row::{self, JobCells, JobView},
        },
        row_record::{
            CanonicalRowView, MAX_ROW_COLUMNS, ROW_HEADER_BYTES, RecordExpectation, RecordVersion,
            RowError, RowPayload, RowRecordView, RowSchemaInput, ValueRef,
        },
        semantic_catalog,
        status_home::StatusHome,
        transaction_record::{
            LogicalRowVersion, StatusSlotRef, TransactionStatus, TxnRef, VersionStamp,
        },
        tree_page::{
            self, SearchPosition, TreeEntry, TreeError, TreeExpectation, TreePageView,
            edit::{LeafEdit, LeafEditError, LeafEditPlan},
        },
        undo_record::{
            UNDO_RECORD_BYTES, UndoCell, UndoError, UndoLink, UndoRecord, UndoRecordExpectation,
            UndoRecordView, UndoTargetView,
        },
        undo_ref::UndoRef,
        wal_frame::{
            FrameEnvelope, FrameExpectation, RedoRecordView, RedoTarget, WalError, WalFrameHeader,
        },
    },
    transaction::TxnId,
};
use foundation::{
    codec::ContentHasher,
    identity::{CommitSeq, Generation, ObjectHandle, RowId},
};
use std::{
    iter::Enumerate,
    mem::size_of,
    ops::Range,
    slice::{Iter, IterMut},
};

/// The codebase's pinned SHA-256 block/output/compression closure.
pub(super) const SHA: usize = 1536;

/// Validated rows occupy at least 189 bytes including their slot, plus the
/// metadata cell and one inserted cell.
pub(super) const ROW_EDIT_CELLS: usize =
    tree_page::TREE_ENTRY_CAPACITY / (ROW_HEADER_BYTES + 5 + 16) + 2;

type CodecResult<T, E> = std::result::Result<T, E>;

/// Callee result and the caller's extracted value coexist at a return boundary.
pub(super) fn returned<T, E>() -> usize {
    size_of::<T>() + size_of::<CodecResult<T, E>>()
}

/// Ranges, indexes, flags and scalar carriers not itemized in a frame.
pub(super) fn slack() -> usize {
    16 * size_of::<&()>() + 8 * size_of::<Result<()>>() + 512
}

pub(super) fn page_codec() -> usize {
    semantic_catalog::page_read_codec_bytes()
}

/// Status home page decode/encode (page, StatusHome and transaction cells).
pub(super) fn status_codec() -> usize {
    semantic_catalog::status_read_codec_bytes()
}

/// Undo record encode/decode over a Row target: the Catalog census's
/// `undo_codec` inventory (header copy, records, cells, view, expectations,
/// target, version identities, edge/stamp scratch, hashers, results) with the
/// Row before parse instead of its CatalogValue one. Every Row material
/// call names `cells::row_target`, and `validate_record` refuses any record
/// whose target differs (undo_record.rs:498) or whose before kind a Row target
/// does not admit (:534-550) before its payload match (:589), so the
/// CatalogValue arm with `decode_catalog_before` (:608) is unreachable here
/// for any input, valid or malformed. The RowValue arm's canonical parse
/// (:602, `decode_canonical_row` over its schema form) stays charged.
pub(super) fn undo_row_codec() -> usize {
    size_of::<[u8; UNDO_RECORD_BYTES]>()
        + 3 * size_of::<UndoRecord<'_>>()
        + 2 * size_of::<UndoCell<'_>>()
        + returned::<UndoRecordView<'_>, UndoError>()
        + 3 * size_of::<UndoRecordExpectation<'_>>()
        + returned::<UndoTargetView<'_>, UndoError>()
        + size_of::<(TxnRef, VersionStamp, LogicalRowVersion, Option<UndoRef>)>()
        + size_of::<([u8; 40], [u8; 32], [u8; 32])>()
        + size_of::<(&[u8], &[u8], &mut [u8], [usize; 8], [u64; 4])>()
        + 2 * size_of::<ContentHasher>()
        + 4 * size_of::<CodecResult<(), UndoError>>()
        + size_of::<(&[u8], &[Column])>()
        + returned::<CanonicalRowView<'_>, RowError>()
        + canonical_codec()
}

/// Tree page decode, `validate_cells`, search and entry parsing (the
/// `semantic_catalog::resources::tree_codec` inventory).
pub(super) fn tree_codec() -> usize {
    let (key, key_result, layout, layout_result) = tree_page::catalog_source_carrier_bytes();
    page_codec()
        + 3 * key // lower_bound query/current; validate_cells previous
        + 2 * key_result
        + layout
        + layout_result
        + returned::<TreePageView<'_>, TreeError>()
        + returned::<TreeEntry<'_>, TreeError>()
        + returned::<SearchPosition, TreeError>()
        + size_of::<(TreeExpectation, PageHeader, Option<PageRef>, Option<&[u8]>)>()
        + size_of::<([usize; 8], bool, u8)>()
        + size_of::<(&[u8], &[u8], &[u8], &[u8])>()
        + 3 * size_of::<CodecResult<(), TreeError>>()
}

/// `prepare_leaf_edit` and the plan's encode (`resources::leaf_edit`). The
/// reference scratch belongs to its caller and is charged there.
pub(super) fn leaf_edit() -> usize {
    tree_codec()
        + size_of::<LeafEdit<'_>>()
        + size_of::<(&TreePageView<'_>, &mut [&[u8]])>()
        + size_of::<(PageHeader, SearchPosition, [usize; 3], &[u8])>()
        + size_of::<(&TreePageView<'_>, LeafEdit<'_>, usize)>()
        + size_of::<Enumerate<IterMut<'_, &[u8]>>>()
        + returned::<LeafEditPlan<'_, '_>, LeafEditError>()
        + size_of::<CodecResult<(), TreeError>>()
}

/// Record identity codecs under `decode_version`/`version_bytes`: the
/// VersionStamp → TxnRef → `decode_txn_id` decode nest (LogicalRowVersion,
/// UndoRef and PageRef nest less deeply) or the encoders' stamp/reference
/// scratch, each with both return sides.
fn identity_codec() -> usize {
    let decode = 3 * size_of::<&[u8]>()
        + returned::<VersionStamp, PageError>()
        + 2 * returned::<TxnRef, PageError>()
        + 2 * returned::<TxnId, PageError>()
        + returned::<CommitSeq, foundation::Error>()
        + returned::<Generation, foundation::Error>()
        + size_of::<([u8; 16], [u8; 4])>()
        + returned::<DatabaseId, crate::Error>();
    let encode = size_of::<(VersionStamp, TxnRef, TxnId)>()
        + 3 * size_of::<&mut [u8]>()
        + size_of::<([u8; 56], [u8; 48])>()
        + 5 * size_of::<CodecResult<(), PageError>>();
    decode.max(encode)
}

/// The canonical row check: an inline value parse and a Catalog-schema
/// column lookup.
fn canonical_codec() -> usize {
    size_of::<(&[u8], RowSchemaInput<'_>)>()
        + returned::<CanonicalRowView<'_>, RowError>()
        + size_of::<[u32; MAX_ROW_COLUMNS + 1]>()
        + size_of::<([usize; 4], ValueRef<'_>, [u8; 8])>()
        + returned::<(ValueRef<'_>, usize), RowError>()
        + returned::<(DataType, bool), RowError>()
        + returned::<CatalogColumn<'_>, CatalogError>()
        + 2 * size_of::<CodecResult<(), RowError>>()
}

/// row_record decode/encode with the canonical check, the version codecs
/// over their identity codecs, an inline value parse and a Catalog-schema
/// column lookup, all summed.
pub(super) fn row_codec() -> usize {
    let version = size_of::<(&[u8], bool, RecordExpectation)>()
        + 2 * size_of::<RecordVersion>()
        + returned::<RowId, foundation::Error>()
        + returned::<VersionStamp, PageError>()
        + returned::<LogicalRowVersion, PageError>()
        + returned::<UndoRef, PageError>()
        + 2 * size_of::<CodecResult<(), RowError>>()
        + identity_codec();
    let decode = size_of::<(&[u8], RecordExpectation, RowSchemaInput<'_>)>()
        + returned::<RowRecordView<'_>, RowError>()
        + size_of::<(&[u8], RowPayload<'_>)>()
        + returned::<OverflowHead, OverflowError>();
    let encode = size_of::<(
        RecordExpectation,
        RowSchemaInput<'_>,
        RecordVersion,
        RowPayload<'_>,
        &mut [u8],
    )>() + size_of::<[u8; ROW_HEADER_BYTES]>()
        + size_of::<[u8; OVERFLOW_HEAD_BYTES]>()
        + size_of::<(&[u8], usize)>()
        + returned::<usize, RowError>()
        + size_of::<CodecResult<(), OverflowError>>();
    decode.max(encode) + canonical_codec() + version
}

/// `cells::projection`: XRP1 prefix, target copies and the hasher.
pub(super) fn projection() -> usize {
    size_of::<(ObjectHandle, RowId, Option<&[u8]>)>()
        + size_of::<[u8; 48]>()
        + 3 * size_of::<UndoTargetView<'_>>()
        + size_of::<ContentHasher>()
        + returned::<[u8; 32], Error>()
        + returned::<usize, UndoError>()
}

/// Proof-page decode (`cells::before_page` and its page view).
pub(super) fn before_page() -> usize {
    size_of::<(&[u8; PAGE_BYTES], PageIdentity, FrameExpectation, bool)>()
        + returned::<PageView<'_>, Error>()
        + returned::<PageView<'_>, PageError>()
        + page_codec()
}

/// `cells::status_before` over the one-home Status page.
pub(super) fn status() -> usize {
    size_of::<(&[u8; PAGE_BYTES], Namespace, StatusSlotRef, PageIdentity)>()
        + returned::<StatusHome, Error>()
        + returned::<StatusHome, PageError>()
        + before_page().max(status_codec())
}

/// `ledger::validate_transaction`/`validate_actor`/`transaction_remaining`:
/// arguments, the observed `(status, ActorSummary)`, the decoded spec and the
/// rows, then the 64-cell actor scan (Row codec), the descriptor codec or the
/// residual rows.
pub(super) fn ledger() -> usize {
    type Summary<'a> = (JobView<'a>, Option<JobView<'a>>, [u64; 4]);
    let validate = size_of::<(
        &[u8; 1536],
        Profile,
        StatusHome,
        StatusSlotRef,
        JobCells<'_>,
    )>() + returned::<(TransactionStatus, Summary<'_>), Error>()
        + returned::<DescriptorSpec, ObligationError>()
        + returned::<[RemainingBudget; 5], Error>()
        + 2 * size_of::<Result<()>>();
    let observe = size_of::<(Profile, StatusHome, StatusSlotRef, JobCells<'_>)>()
        + 2 * size_of::<JobView<'_>>()
        + 2 * size_of::<Option<JobView<'_>>>()
        + 2 * size_of::<Option<UndoRef>>()
        + size_of::<([u64; 8], bool, usize, usize)>()
        + returned::<JobView<'_>, RetentionError>()
        + row::source_bytes();
    let remaining = size_of::<[RemainingBudget; 5]>()
        + 3 * size_of::<RemainingBudget>()
        + size_of::<(TransactionStatus, bool, u64)>();
    validate
        + observe
            .max(obligation::descriptor_source_bytes())
            .max(remaining)
}

/// U material: `check_material`'s canonical cell over `encode_material`, then
/// the Row-target Undo codec or a proof-page decode.
pub(super) fn material() -> usize {
    size_of::<[u8; UNDO_RECORD_BYTES]>()
        + size_of::<(Namespace, JobView<'_>, UndoCell<'_>, &[u8])>()
        + size_of::<(UndoRef, ObjectHandle, RowId, UndoRecordExpectation<'_>)>()
        + 2 * size_of::<UndoCell<'_>>()
        + returned::<UndoRecord<'static>, Error>()
        + returned::<usize, UndoError>()
        + returned::<UndoRecordView<'_>, UndoError>()
        + returned::<Generation, UndoError>()
        + undo_row_codec().max(before_page())
}

/// Job cell edits: lease, Undo item and the budget rows of `set_budget`.
pub(super) fn job_edit() -> usize {
    size_of::<[RemainingBudget; 5]>()
        + size_of::<(JobView<'_>, TxnRef, StatusSlotRef, u64, u8)>()
        + size_of::<([u8; 96], UndoRef, obligation::InventoryClaim)>()
        + returned::<Generation, foundation::Error>()
        + 4 * size_of::<CodecResult<(), PageError>>()
        + size_of::<ContentHasher>()
}

/// Edges/counts/delta and the core: the edge table, both count rows, the
/// physical order and the core's own decode check.
pub(super) fn graph() -> usize {
    size_of::<[(u16, u8, Option<UndoLink>, Option<UndoLink>); 4]>()
        + size_of::<[(Option<UndoLink>, (u64, u64), u16); 2]>()
        + size_of::<([(u64, u64, usize); 5], [u8; 24], [Option<usize>; 5])>()
        + returned::<CoreView<'_>, Error>()
        + size_of::<(TxnRef, UndoLink, Option<UndoLink>, u64)>()
        + 4 * size_of::<Result<()>>()
}

/// Stable anchor → single RowLeaf (`table_leaf`).
fn anchor() -> usize {
    size_of::<(TableSource<'_>, PageIdentity, TreeAnchor, AnchorRoot)>()
        + returned::<TreeAnchor, PageError>()
        + returned::<PageRef, Error>()
        + before_page()
}

/// Every row of the RowLeaf typed (`typed_leaf`), then search/edit/record
/// work of `prepare_leaf` (Forward/CLR/Prune) or `reserve_leaf`.
pub(super) fn leaf() -> usize {
    type LeafSource<'a> = (TreeExpectation, RowSchemaInput<'a>, &'a [u8; PAGE_BYTES]);
    let typed = size_of::<(TableSource<'_>, PageRef, TreeExpectation, TreePageView<'_>)>()
        + returned::<LeafSource<'_>, Error>()
        + returned::<TreeEntry<'_>, TreeError>()
        + size_of::<([u8; 8], RowId, Range<usize>)>()
        + returned::<RowRecordView<'_>, RowError>()
        + anchor()
            .max(before_page())
            .max(tree_codec())
            .max(row_codec());
    let edit = size_of::<LeafSource<'_>>()
        + size_of::<(
            [u8; 8],
            TreePageView<'_>,
            SearchPosition,
            [u8; 32],
            [u8; 32],
        )>()
        + size_of::<[&[u8]; ROW_EDIT_CELLS]>()
        // Prune's record is backing scratch: its reference and the
        // `record_scratch` call that lends it.
        + size_of::<&mut [u8; MAX_RECORD]>()
        + size_of::<(
            &mut [[u8; 1536]; 2],
            &mut [u8],
            Option<&mut [u8; MAX_RECORD]>,
        )>()
        + 2 * size_of::<RecordVersion>()
        + 2 * size_of::<RowRecordView<'_>>()
        + returned::<LeafEditPlan<'_, '_>, LeafEditError>()
        + returned::<&[u8], TreeError>()
        + returned::<(Option<u16>, Option<u16>, [u8; 32], [u8; 32]), Error>();
    edit + typed.max(leaf_edit()).max(row_codec()).max(projection())
}

/// wal_frame envelope/redo codec (`resources::wal_codec` over the same types).
fn wal_codec() -> usize {
    size_of::<([u8; 160], [u8; 64], [u8; 40], [u8; 32])>()
        + 2 * size_of::<WalFrameHeader>()
        + size_of::<(FrameExpectation, RedoTarget, PageIdentity)>()
        + returned::<FrameEnvelope<'_>, WalError>()
        + returned::<RedoRecordView<'_>, WalError>()
        + returned::<[u8; 64], WalError>()
        + returned::<[u8; 160], WalError>()
        + 2 * size_of::<ContentHasher>()
        + size_of::<(&[u8], &mut [u8], [usize; 6], [u64; 2])>()
        + 4 * size_of::<CodecResult<(), WalError>>()
        + page_codec()
}

/// `decode_layout`: arguments, view, envelope, shape/core, owner and native
/// walks with their digests, then the envelope/redo codec.
pub(super) fn layout_decode() -> usize {
    size_of::<(&[u8], FrameExpectation, Profile)>()
        + returned::<LayoutView<'_>, Error>()
        + size_of::<LayoutView<'_>>()
        + returned::<FrameEnvelope<'_>, WalError>()
        + size_of::<(Operation, Shape, &[u8], [usize; 5])>()
        + returned::<CoreView<'_>, Error>()
        + size_of::<(ReservationOwner, TxnRef, Namespace, SystemWorkRef)>()
        + size_of::<[(bool, usize, usize, usize, usize, &[u8]); 2]>()
        + size_of::<([u8; 24], [usize; 4], Option<(u64, u64)>, [u8; 32])>()
        + returned::<RedoRecordView<'_>, WalError>()
        + 2 * size_of::<ContentHasher>()
        + 4 * size_of::<Result<()>>()
        + wal_codec()
}

/// Deepest callee of framing: layout decode, envelope finish or a page view
/// with its native cell ranges.
pub(super) fn frame_callee() -> usize {
    let envelope = size_of::<(WalFrameHeader, u16, &mut [u8])>()
        + returned::<[u8; 160], WalError>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<ContentHasher>()
        + 2 * size_of::<CodecResult<(), WalError>>();
    let native = returned::<PageView<'_>, PageError>()
        + size_of::<(Option<Range<usize>>, Option<&[u8]>, Iter<'_, u8>)>()
        + page_codec();
    layout_decode().max(envelope).max(native)
}
