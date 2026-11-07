//! Counted source terms of the scope3 Row transition's nested callees.
//!
//! This is not a compiler-stack/RSS estimate. Each term sums `size_of` over
//! the values its call chain keeps live: arguments, locals and both sides of
//! every return boundary (`returned`). A `max` joins completed calls; their
//! still-live parent values are added outside it. Page/status codecs are the
//! Catalog scope2 census terms over the same codecs; the Undo codec and the
//! leaf-edit terms repeat its `undo_codec`/`leaf_edit` inventories; both owners
//! delegate tree decode/search terms to tree_page::codec_source_bytes, so
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
            JOB_BYTES, JOB_COUNT, RetentionError,
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
            UNDO_RECORD_BYTES, UndoCell, UndoEdge, UndoError, UndoLink, UndoPayload, UndoRecord,
            UndoRecordExpectation, UndoRecordOutline, UndoRecordView, UndoTargetView,
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
/// call names `cells::row_target` (through `cells::expectation` for a DELETE
/// U), and `validate_record` refuses any record whose target differs or whose
/// before kind a Row target does not admit before its payload match, so the
/// CatalogValue arm with `decode_catalog_before` is unreachable here for any
/// input, valid or malformed. The RowValue arm's canonical check
/// (`BeforeSchema::check_row_before`) stays charged at its widest instance,
/// C's `&[Column]` over `decode_canonical_row`: entry (a)'s `&RowSchemaInput`
/// calls `decode_canonical_row_with_schema` directly, one frame less, and
/// entry (b)'s `MetadataOnly` parses nothing.
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
        // `BeforeSchema::check_row_before` (C's `&[Column]` instance) over
        // decode_canonical_row: its arguments and result, then the parse.
        + size_of::<(&[Column], &[u8])>()
        + size_of::<CodecResult<(), UndoError>>()
        + size_of::<(&[u8], &[Column])>()
        + returned::<CanonicalRowView<'_>, RowError>()
        + canonical_codec()
}

/// Tree-owned decode/search inventory with the shared page envelope cost.
pub(super) fn tree_codec() -> usize {
    tree_page::codec_source_bytes(page_codec())
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

/// `ledger::observe`: arguments, the scanned Jobs and links, then the
/// 64-cell actor scan over the Row codec.
fn ledger_observe() -> usize {
    size_of::<(Profile, StatusHome, StatusSlotRef, JobCells<'_>)>()
        + 2 * size_of::<JobView<'_>>()
        + 2 * size_of::<Option<JobView<'_>>>()
        + 2 * size_of::<Option<UndoRef>>()
        + size_of::<([u64; 8], bool, usize, usize)>()
        + returned::<JobView<'_>, RetentionError>()
        + row::source_bytes()
}

/// `ledger::remaining`: the five rows and their per-row scratch.
fn ledger_remaining() -> usize {
    size_of::<[RemainingBudget; 5]>()
        + 3 * size_of::<RemainingBudget>()
        + size_of::<(TransactionStatus, bool, u64)>()
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
    validate
        + ledger_observe()
            .max(obligation::descriptor_source_bytes())
            .max(ledger_remaining())
}

/// `ledger::transaction_remaining` alone (prepare's `descriptor_after`):
/// arguments and the observed `(status, ActorSummary)`, then the actor scan
/// or the residual rows. Its row return is the caller's.
pub(super) fn transaction_remaining() -> usize {
    type Summary<'a> = (JobView<'a>, Option<JobView<'a>>, [u64; 4]);
    size_of::<(Profile, StatusHome, StatusSlotRef, JobCells<'_>, bool)>()
        + returned::<(TransactionStatus, Summary<'_>), Error>()
        + ledger_observe().max(ledger_remaining())
}

/// `cells::undo_cell` (and `undo_before` over it): the one-slot Undo page
/// proof with its cell length, over a proof-page decode.
fn undo_page() -> usize {
    size_of::<(&[u8; PAGE_BYTES], Namespace, PageRef, FrameExpectation)>()
        + returned::<&[u8], Error>()
        + size_of::<(
            &[u8; PAGE_BYTES],
            Namespace,
            PageRef,
            FrameExpectation,
            usize,
        )>()
        + returned::<&[u8], Error>()
        + returned::<PageView<'_>, Error>()
        + size_of::<(usize, Option<Range<usize>>)>()
        + before_page()
}

/// `cells::material`: the Job's identities and the record it returns.
fn job_material() -> usize {
    size_of::<(JobView<'_>, UndoEdge, u64, u8)>()
        + returned::<UndoRecord<'static>, Error>()
        + size_of::<(TxnRef, ObjectHandle, RowId)>()
        + size_of::<UndoRecord<'static>>()
}

/// U material: `check_live` over `check_material`'s canonical cell over
/// `encode_material`, or over a DELETE U's `check_outline` (entry (b)
/// against the Job's material, dev3-row-delete.md#previous-proof), or a
/// DELETE U's release step (`encode_release_step`, decode then encode of one
/// record); then the Row-target Undo codec, the Job material or the Undo
/// page proof.
pub(super) fn material() -> usize {
    let live = size_of::<(Namespace, JobView<'_>, UndoEdge, &[u8])>() + size_of::<Result<()>>();
    let check = size_of::<[u8; UNDO_RECORD_BYTES]>()
        + size_of::<(Namespace, JobView<'_>, UndoCell<'_>, &[u8])>()
        + size_of::<(UndoRef, ObjectHandle, RowId, UndoRecordExpectation<'_>)>()
        + 2 * size_of::<UndoCell<'_>>()
        + returned::<UndoRecord<'static>, Error>()
        + returned::<usize, UndoError>()
        + returned::<UndoRecordView<'_>, UndoError>()
        + returned::<Generation, UndoError>();
    let outline = size_of::<(Namespace, JobView<'_>, bool, UndoEdge, u8, &[u8])>()
        + size_of::<Result<()>>()
        + returned::<UndoRecord<'static>, Error>()
        + returned::<usize, Error>()
        + returned::<UndoRecordExpectation<'static>, Error>()
        + returned::<UndoRecordOutline<'_>, UndoError>()
        + size_of::<UndoRecordOutline<'_>>()
        + returned::<UndoRecordView<'_>, UndoError>();
    let release = size_of::<(&[u8], UndoRecordExpectation<'_>, &mut [u8])>()
        + returned::<UndoRecordExpectation<'static>, Error>()
        + returned::<usize, UndoError>()
        + returned::<UndoRecordView<'_>, UndoError>()
        + size_of::<(UndoRecord<'_>, UndoCell<'_>)>();
    (live + check.max(outline)).max(release) + undo_row_codec().max(job_material()).max(undo_page())
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

/// Stable anchor → single RowLeaf (`table_leaf`): the page proof, then the
/// anchor decode (one page codec, or the XTR1 walk after it, UNL-C01).
fn anchor() -> usize {
    size_of::<(TableSource<'_>, PageIdentity, TreeAnchor, AnchorRoot)>()
        + returned::<TreeAnchor, PageError>()
        + returned::<PageRef, Error>()
        + before_page().max(crate::storage::anchor::decode_source_bytes(page_codec()))
}

/// `LeafSource`: expectation, schema, before image and the typed entries'
/// bytes.
type LeafSource<'a> = (
    TreeExpectation,
    RowSchemaInput<'a>,
    &'a [u8; PAGE_BYTES],
    usize,
);

/// Every row of the RowLeaf typed (`typed_leaf`) with its running sum, over
/// the anchor proof, a page proof, the tree codec or a record decode.
fn typed() -> usize {
    size_of::<(TableSource<'_>, PageRef, TreeExpectation, TreePageView<'_>)>()
        + size_of::<(usize, usize)>()
        + returned::<LeafSource<'_>, Error>()
        + returned::<TreeEntry<'_>, TreeError>()
        + size_of::<([u8; 8], RowId, Range<usize>)>()
        + returned::<RowRecordView<'_>, RowError>()
        + anchor()
            .max(before_page())
            .max(tree_codec())
            .max(row_codec())
}

/// Every row of the RowLeaf typed (`typed_leaf`), then the leaf reservation D
/// over the step's decoded view (`leaf_reserved!`, a callee of the step's
/// frame, which holds the view, D and its Result), then search/edit/record
/// work of `prepare_leaf` (Forward/CLR/Prune) or `reserve_leaf`.
pub(super) fn leaf() -> usize {
    let typed = typed();
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
        + returned::<(Option<u16>, Option<u16>, [u8; 32], [u8; 32]), Error>()
        + returned::<usize, Error>()
        + size_of::<usize>();
    edit + reservation()
        .max(typed)
        .max(leaf_edit())
        .max(row_codec())
        .max(projection())
}

/// `cells::delete_material`: its before, the record it returns and the Job
/// material under it.
fn delete_material() -> usize {
    size_of::<(JobView<'_>, UndoEdge, u64, VersionStamp, LogicalRowVersion)>()
        + size_of::<UndoPayload<'_>>()
        + returned::<UndoRecord<'_>, Error>()
        + size_of::<UndoRecord<'_>>()
        + job_material()
}

/// `value_source`: the typed leaf, its search, the target's record and
/// projection, the row's triple (a Forward's old row or UPDATE's new one,
/// with the effect/payload pair UPDATE's CLR and Prune match), over the typed
/// leaf, the tree codec, the record codec or a projection. D is the caller's
/// (`value_reserved`).
fn value_source() -> usize {
    type Old<'a> = Option<(VersionStamp, LogicalRowVersion, &'a [u8])>;
    size_of::<(&(), JobView<'_>, TxnRef, bool, usize)>()
        + size_of::<(LeafSource<'_>, ObjectHandle, RowId, [u8; 8])>()
        + size_of::<(TreePageView<'_>, SearchPosition, &[u8], RowRecordView<'_>)>()
        + 3 * size_of::<RecordVersion>()
        + size_of::<([u8; 32], Old<'_>, CanonicalRowView<'_>)>()
        + size_of::<(row::RowEffect, RowPayload<'_>)>()
        + returned::<LeafSource<'_>, Error>()
        + returned::<TreePageView<'_>, TreeError>()
        + returned::<SearchPosition, TreeError>()
        + returned::<TreeEntry<'_>, TreeError>()
        + returned::<RowRecordView<'_>, RowError>()
        + returned::<[u8; 32], Error>()
        + returned::<RecordVersion, Error>()
        + size_of::<CodecResult<(), Error>>()
        + typed().max(tree_codec()).max(row_codec()).max(projection())
}

/// The DELETE U frames, each over the DELETE material or the Row-target Undo
/// codec (dev3-row-delete.md#delete-undo): `delete_undo_live` (the before
/// triple, the Live record in its cell, the expectation, entry (a)'s header
/// encode), `delete_before` (the expectation, entry (a)'s decoded view and
/// Live record, the compared material, the before triple) or
/// `delete_release` (the cut edge, the Releasing record in its cell, the
/// expectation, entry (a)'s encode).
pub(super) fn delete_undo() -> usize {
    type Before<'a> = (VersionStamp, LogicalRowVersion, &'a [u8]);
    let live = size_of::<(Namespace, JobView<'_>, UndoEdge, &RowSchemaInput<'_>)>()
        + size_of::<(Before<'_>, &mut [u8; UNDO_RECORD_BYTES])>()
        + size_of::<CodecResult<Before<'_>, Error>>()
        + size_of::<Before<'_>>()
        + returned::<UndoRecord<'_>, Error>()
        + size_of::<UndoCell<'_>>()
        + returned::<UndoRecordExpectation<'static>, Error>()
        + returned::<usize, UndoError>()
        + size_of::<CodecResult<(), Error>>();
    let before = size_of::<(Namespace, JobView<'_>, UndoEdge, &RowSchemaInput<'_>, &[u8])>()
        + size_of::<CodecResult<Before<'_>, Error>>()
        + returned::<UndoRecordExpectation<'static>, Error>()
        + returned::<UndoRecordView<'_>, UndoError>()
        + returned::<UndoRecord<'_>, UndoError>()
        + returned::<UndoRecord<'_>, Error>()
        + size_of::<Before<'_>>()
        + size_of::<CodecResult<(), Error>>();
    let release = size_of::<(Namespace, JobView<'_>, UndoEdge, &RowSchemaInput<'_>)>()
        + size_of::<(
            (VersionStamp, LogicalRowVersion),
            &mut [u8; UNDO_RECORD_BYTES],
        )>()
        + size_of::<CodecResult<(), Error>>()
        + size_of::<UndoEdge>()
        + returned::<UndoRecord<'_>, Error>()
        + size_of::<UndoCell<'_>>()
        + returned::<UndoRecordExpectation<'static>, Error>()
        + returned::<usize, UndoError>();
    live.max(before).max(release) + undo_row_codec().max(delete_material())
}

/// UpdateForward's `update_value`: the candidate cell's framing, its record
/// decoded with the table schema and its version against the Job's
/// (`current_version`), over the record codec.
fn update_value() -> usize {
    size_of::<(
        Namespace,
        JobView<'_>,
        TxnRef,
        &RowSchemaInput<'_>,
        Option<&[u8]>,
    )>() + size_of::<(RowId, &[u8], RecordExpectation, RowRecordView<'_>)>()
        + returned::<&[u8], Error>()
        + returned::<RowRecordView<'_>, RowError>()
        + returned::<RecordVersion, Error>()
        + size_of::<(CanonicalRowView<'_>, RowPayload<'_>)>()
        + row_codec()
            .max(size_of::<(JobView<'_>, TxnRef)>() + returned::<LogicalRowVersion, PageError>())
}

/// `delete_leaf`: the edit scratch, the after record's version and payload,
/// the record scratch call, the cell length and projection, over the tree
/// codec, the record codec, a projection or the leaf edit. `room` is the D
/// after the step.
fn delete_leaf() -> usize {
    size_of::<(
        Namespace,
        &mut (),
        JobView<'_>,
        &LeafSource<'_>,
        Option<(RecordVersion, Option<&[u8]>)>,
        usize,
    )>() + size_of::<CodecResult<[u8; 32], Error>>()
        + size_of::<(ObjectHandle, RowId, [u8; 8], TreePageView<'_>)>()
        + size_of::<[&[u8]; ROW_EDIT_CELLS]>()
        + size_of::<(RecordVersion, RowPayload<'_>, [u8; 32])>()
        + size_of::<&mut [u8; MAX_RECORD]>()
        + size_of::<(
            &mut [[u8; 1536]; 2],
            &mut [u8],
            Option<&mut [u8; MAX_RECORD]>,
        )>()
        + returned::<TreePageView<'_>, TreeError>()
        + returned::<CanonicalRowView<'_>, RowError>()
        + returned::<usize, RowError>()
        + returned::<[u8; 32], Error>()
        + returned::<&[u8], TreeError>()
        + returned::<LeafEditPlan<'_, '_>, LeafEditError>()
        + tree_codec()
            .max(row_codec())
            .max(projection())
            .max(leaf_edit())
}

/// The delete or update RowReserve's leaf proof (`reserve_target`): its
/// arguments (effect, A's canonical view, the adopted Job, C'), the planned
/// row it builds from them, the reservation D, the typed leaf, its search,
/// the old row, the planned row (DELETE's tombstone, UPDATE's new row or
/// replay's placeholder) it encodes
/// into the record scratch and its projection, and UPDATE's fit: the planned
/// cell, the edit scratch and its Replace plan. Over the typed leaf, D over
/// its decoded view (`leaf_reserved!`), the tree or record codec, a
/// projection or the leaf edit.
fn reserve_target() -> usize {
    size_of::<(
        &(),
        TableSource<'_>,
        PageRef,
        RecordVersion,
        (
            row::RowEffect,
            Option<&CanonicalRowView<'_>>,
            Option<&[u8]>,
            usize,
        ),
        &mut (),
        &mut (),
    )>() + size_of::<super::prepare::Planned<'_>>()
        + size_of::<(Namespace, RowId, LeafSource<'_>, TreePageView<'_>)>()
        + size_of::<([u8; 8], SearchPosition, &[u8], RecordExpectation)>()
        + size_of::<(RowRecordView<'_>, usize, RowPayload<'_>, [u8; 32])>()
        + size_of::<[&[u8]; ROW_EDIT_CELLS]>()
        + returned::<(usize, [u8; 32]), Error>()
        + returned::<LeafSource<'_>, Error>()
        + returned::<TreePageView<'_>, TreeError>()
        + returned::<SearchPosition, TreeError>()
        + returned::<TreeEntry<'_>, TreeError>()
        + returned::<RowRecordView<'_>, RowError>()
        + returned::<usize, RowError>()
        + returned::<[u8; 32], Error>()
        + returned::<usize, Error>()
        + returned::<&[u8], TreeError>()
        + returned::<LeafEditPlan<'_, '_>, LeafEditError>()
        + typed()
            .max(reservation())
            .max(tree_codec())
            .max(row_codec())
            .max(projection())
            .max(leaf_edit())
}

/// DELETE's and UPDATE's prepare callees: `prepare_value` (its arguments, the
/// table, its reservation D and the D after (`room`), the leaf observation,
/// the before triple, the after version and digest, UpdatePrune's kept triple
/// and commit sequence, over `value_source`, D of its leaf (`value_reserved`),
/// a DELETE U frame, `update_value`, `delete_leaf` or `current_version`) or
/// the delete
/// or update RowReserve's `reserve_target` (dev3-row-delete.md#delete-states,
/// dev3-row-update.md#update-states).
pub(super) fn delete() -> usize {
    type Before<'a> = (VersionStamp, LogicalRowVersion, &'a [u8]);
    let prepare = size_of::<(
        &(),
        &mut (),
        JobView<'_>,
        TxnRef,
        UndoEdge,
        &[u8],
        Option<&[u8]>,
        u64,
    )>() + size_of::<(
        Operation,
        bool,
        Namespace,
        ObjectHandle,
        &RowSchemaInput<'_>,
    )>() + size_of::<(usize, RowId, Before<'_>)>()
        + returned::<CommitSeq, foundation::Error>()
        + returned::<usize, Error>()
        + returned::<([u8; 32], [u8; 32]), Error>()
        + size_of::<(LeafSource<'_>, u16, [u8; 32], Option<Before<'_>>)>()
        + size_of::<CodecResult<(LeafSource<'_>, u16, [u8; 32], Option<Before<'_>>), Error>>()
        + returned::<Before<'_>, Error>()
        + size_of::<Option<(RecordVersion, Option<&[u8]>)>>()
        + returned::<RecordVersion, Error>()
        + returned::<[u8; 32], Error>()
        + size_of::<CodecResult<(), Error>>()
        + value_source()
            .max(value_reserved())
            .max(delete_undo())
            .max(update_value())
            .max(delete_leaf())
            .max(
                size_of::<(JobView<'_>, TxnRef)>() + size_of::<CodecResult<RecordVersion, Error>>(),
            );
    prepare.max(reserve_target())
}

/// `value_reserved` (dev3-tree-smo.md#current-leaf-consumers): its arguments,
/// the typed leaf's view decoded again and its Result, the namespace, D and
/// its Result, over the tree decode or `cells::leaf_reservation`.
fn value_reserved() -> usize {
    size_of::<(&(), ObjectHandle, &LeafSource<'_>, Namespace)>()
        + returned::<TreePageView<'_>, TreeError>()
        + 2 * returned::<usize, Error>()
        + tree_codec().max(reservation())
}

/// `cells::leaf_reservation`: the Jobs it scans, the leaf it searches, the
/// sum, one decoded Job with its RowId key and that key's search, over the
/// Row census Job decode or the tree codec's `lower_bound`.
fn reservation() -> usize {
    size_of::<(
        &[[u8; JOB_BYTES]; JOB_COUNT],
        Namespace,
        ObjectHandle,
        &TreePageView<'_>,
    )>() + size_of::<(usize, Iter<'_, [u8; JOB_BYTES]>, &[u8; JOB_BYTES])>()
        + returned::<JobView<'_>, RetentionError>()
        + returned::<usize, Error>()
        + returned::<RowId, Error>()
        + size_of::<[u8; 8]>()
        + returned::<SearchPosition, TreeError>()
        + row::source_bytes().max(tree_codec())
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

/// `decode_layout`: arguments, view, envelope, shape/core, the native
/// section closure and a chain-return list's cell sizes
/// (dev3-row-overflow.md#opcodes), owner and native walks with their
/// digests, then the envelope/redo codec.
pub(super) fn layout_decode() -> usize {
    size_of::<(&[u8], FrameExpectation, Profile)>()
        + returned::<LayoutView<'_>, Error>()
        + size_of::<LayoutView<'_>>()
        + returned::<FrameEnvelope<'_>, WalError>()
        + size_of::<(Operation, Shape, &[u8], [usize; 5])>()
        + size_of::<(&usize, &usize, &&[u8])>()
        + returned::<(usize, usize), Error>()
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
