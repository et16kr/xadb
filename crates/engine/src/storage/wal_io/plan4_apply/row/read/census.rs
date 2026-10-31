//! Source-value inventories of the non-LOAD Row read phases. Private page and
//! copy backing are the separately charged caller loan. The original port is
//! already in source::shared_bytes; the operation retains only its reference.
use super::super::super::{read as physical, source, transaction, visibility};
use super::*;
use crate::storage::{
    catalog_record::{CatalogHeader, CatalogRecordView, TableDefinitionView},
    dirty_frame,
    page::{PageHeader, PageView},
    row_record::{CanonicalRowView, RowPayload, RowRecordView},
    semantic_catalog,
    semantic_row::transition,
    status_home::{StatusHome, StatusHomePage},
    transaction_record::{StatusSlotRef, TransactionStatus, TxnRef},
    undo_record::{UndoRecord, UndoRecordExpectation, UndoRecordView, UndoTargetView},
};
use std::mem::size_of as n;

/// Admission, schema, capture, location, decode, copy, and cleanup respectively.
/// These exclude the retained operation and B frames, which the join adds once.
pub(super) fn nonload<W: WriterNamespace>() -> [usize; 7] {
    let r = n::<&()>();
    // Small closures, scalar loop/length values, error conversions and merge
    // arguments in these non-LOAD phases. LOAD uses a separately audited bound.
    let temporaries = 32 * r + 16 * n::<Result<()>>() + 512;
    let refs = visibility::source_census();
    let ve = visibility::entry_source_phases();
    let pe = dirty_frame::retention_entry_source_phases();
    let control = source::leaf_source_bytes()
        .max(crate::transaction::concurrency::read_operation_source_bytes())
        .max(transaction::source_bytes())
        .max(refs.peak_leaf);
    // Factory return plus the two wide error-merge results on admission
    // failure; the eventual held operation is added separately by the join.
    let admission = 3 * n::<
        Result<RowReadOperation<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>>,
    >() + n::<Result<OperationSourceAdmission<'_, '_, '_, '_>>>()
        + n::<Result<ReadOperationAdmission<'_, '_>>>()
        + n::<Result<ReferenceLease<'_, '_, '_, '_>>>()
        + 6 * n::<Result<()>>()
        + 4 * n::<&mut [u8]>()
        + control
            .max(ve.0 + ve.1 + ve.2)
            .max(pe.0 + pe.1 + pe.2)
            .max(refs.reference);
    // guard lookup/header/definition/value/index scan and checked release.
    // Include Row anchor/root/leaf whole-set validation as well as Catalog
    // preparation; the two kinds of source never overlap, so this is a sum
    // bound with intentionally retained extra carriers.
    let schema = n::<ReadTargets<'_, '_, '_, '_, '_, '_, 3>>()
        + n::<Result<ReadTargets<'_, '_, '_, '_, '_, '_, 3>>>()
        + 4 * n::<TargetRequest>()
        + 2 * n::<PageHeader>()
        + 2 * n::<dirty_frame::FrameStamp>()
        + 2 * n::<anchor::AnchorRoot>()
        + n::<Result<anchor::AnchorRoot>>()
        + 4 * n::<Result<()>>()
        + n::<physical::RetentionCatalogGuard<'_, '_, '_, '_, '_, '_, '_, '_, W>>()
        + n::<Result<physical::RetentionCatalogGuard<'_, '_, '_, '_, '_, '_, '_, '_, W>>>()
        + n::<CatalogRecordView<'_>>()
        + n::<CatalogHeader>()
        + n::<TableDefinitionView<'_>>()
        + n::<CatalogDefinitionView<'_>>()
        + 2 * n::<Result<Option<CatalogRecordView<'_>>>>()
        + 2 * n::<Result<Option<&[u8]>>>()
        + 4 * n::<Result<()>>()
        + physical::catalog_source_bytes::<W>()
            .max(semantic_catalog::catalog_read_codec_bytes())
            .max(dirty_frame::target_set_call_bytes::<3>());
    // The schema and actual record views survive the pin registration. Count
    // both the capture_row tuple and its caller extraction conservatively.
    let capture = n::<RowSearchPage<'_, '_, '_, '_, '_, '_, '_>>()
        + n::<CatalogRecordView<'_>>()
        + n::<TableDefinitionView<'_>>()
        + 2 * n::<RecordExpectation>()
        + n::<TreeExpectation>()
        + n::<TreePageView<'_>>()
        + n::<crate::storage::tree_page::TreeEntry<'_>>()
        + n::<[u8; 8]>()
        + n::<RowId>()
        + 2 * n::<RowRecordView<'_>>()
        + n::<RowPayload<'_>>()
        + n::<CanonicalRowView<'_>>()
        + n::<RecordVersion>()
        + n::<visibility::CapturedCatalogRefs<'_, '_, '_, '_>>()
        + n::<
            Result<(
                visibility::CapturedCatalogRefs<'_, '_, '_, '_>,
                RowRecordView<'_>,
            )>,
        >()
        + n::<(usize, usize, u64)>()
        + 2 * n::<Result<RowCandidate>>()
        + 4 * n::<Result<()>>()
        + transition::tree_source_bytes()
            .max(transition::row_read_codec_bytes())
            .max(semantic_catalog::catalog_read_codec_bytes())
            .max(refs.reference)
            .max(dirty_frame::target_set_call_bytes::<3>());
    // Status/Undo location searches use the unchanged shared lookup census.
    let location = physical::reference_location_source_bytes::<W>();
    // Both typed decode families are bounded together, including their error
    // paths. Their physical read and pin release leaves execute sequentially.
    let decode = 2 * n::<physical::RetentionReadGuard<'_, '_, '_, '_, '_, '_>>()
        + 2 * n::<Result<physical::RetentionReadGuard<'_, '_, '_, '_, '_, '_>>>()
        + n::<StatusHomePage<'_>>()
        + 2 * n::<StatusHome>()
        + n::<TransactionStatus>()
        + 2 * n::<Result<TransactionState>>()
        + n::<TxnRef>()
        + n::<StatusSlotRef>()
        + n::<UndoRef>()
        + n::<ObjectHandle>()
        + 2 * n::<TargetRequest>()
        + n::<PageView<'_>>()
        + n::<PageHeader>()
        + n::<&[u8]>()
        + n::<UndoRecordExpectation<'_>>()
        + n::<UndoTargetView<'_>>()
        + 2 * n::<UndoRecordView<'_>>()
        + 2 * n::<UndoRecord<'_>>()
        + 4 * n::<Result<UndoRecordView<'_>>>()
        + 2 * n::<Result<MissingRowEvidence>>()
        + 4 * n::<Result<()>>()
        + physical::catalog_reference_read_source_bytes()
            .max(transition::row_undo_read_codec_bytes())
            .max(semantic_catalog::status_read_codec_bytes())
            .max(semantic_catalog::page_read_codec_bytes())
            .max(dirty_frame::target_set_call_bytes::<3>())
            .max(control);
    let copy = 4 * r
        + n::<&mut [u8]>()
        + n::<Result<()>>()
        + n::<Result<RowCopyObservation>>()
        + transaction::source_bytes();
    // The operation already contains captured pins and lease storage. Result
    // carriers from all four sequential cleanups remain in its finish frame.
    let cleanup = 4 * n::<Result<()>>()
        + 4 * n::<Result<()>>()
        + n::<CapturedRowRefs<'_, '_, '_, '_>>()
        + n::<ReferenceLease<'_, '_, '_, '_>>()
        + control
            .max(refs.reference)
            .max(dirty_frame::target_set_call_bytes::<3>());
    // LOCATION's helper already includes its complete error/carrier envelope;
    // its Row parent is the explicit status/missing frame in the LOAD table.
    [
        admission + temporaries,
        schema + temporaries,
        capture + temporaries,
        location,
        decode + temporaries,
        copy + temporaries,
        cleanup + temporaries,
    ]
}
