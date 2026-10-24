//! Historical catalog definitions for Undo, without duplicated version edges.
//! XCB1 is a borrowed value codec. It grants no handle, pin, reservation,
//! anchor lifetime, or authority to apply a catalog mutation.

use super::{
    CatalogDefinition, CatalogDefinitionView, CatalogError, CatalogExpectation, PageRef,
    decode_identity, decode_index, decode_table, definition_len, put_u64, u32_at, u64_at,
    write_definition, zero,
};
use crate::api::Namespace;
use xadb_foundation::identity::{Generation, ObjectHandle, SchemaEpoch};

pub(crate) const CATALOG_BEFORE_HEADER_BYTES: usize = 96;
pub(crate) const MAX_CATALOG_BEFORE_BYTES: usize = 4452;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogBeforeHeader {
    pub(crate) identity: ObjectHandle,
    pub(crate) retired: bool,
    pub(crate) anchor: PageRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogBeforeView<'a> {
    namespace: Namespace,
    header: CatalogBeforeHeader,
    definition: CatalogDefinitionView<'a>,
}

impl<'a> CatalogBeforeView<'a> {
    pub(crate) fn header(self) -> CatalogBeforeHeader {
        self.header
    }

    pub(crate) fn definition(self) -> CatalogDefinitionView<'a> {
        self.definition
    }

    /// Compare the historical value identity, without acquiring its lifetime.
    pub(crate) fn validate_handle(self, expected: ObjectHandle) -> Result<(), CatalogError> {
        if self.header.identity == expected {
            Ok(())
        } else {
            Err(CatalogError::IdentityMismatch)
        }
    }
}

pub(crate) fn catalog_before_encoded_len(
    expected: CatalogExpectation,
    header: CatalogBeforeHeader,
    definition: CatalogDefinition<'_>,
) -> Result<usize, CatalogError> {
    if expected.namespace.timeline == 0 {
        return Err(CatalogError::InvalidInput);
    }
    if header.identity.object != expected.object {
        return Err(CatalogError::IdentityMismatch);
    }
    Ok(CATALOG_BEFORE_HEADER_BYTES + definition_len(header.identity.object, definition)?)
}

/// Validate all inputs and header bytes before the first caller-buffer write.
/// Errors preserve the entire output; success preserves the unused tail.
pub(crate) fn encode_catalog_before(
    expected: CatalogExpectation,
    header: CatalogBeforeHeader,
    definition: CatalogDefinition<'_>,
    output: &mut [u8],
) -> Result<usize, CatalogError> {
    let length = catalog_before_encoded_len(expected, header, definition)?;
    if output.len() < length {
        return Err(CatalogError::NoSpace);
    }
    let mut fixed = [0; CATALOG_BEFORE_HEADER_BYTES];
    fixed[..4].copy_from_slice(b"XCB1");
    fixed[4..6].copy_from_slice(&1_u16.to_le_bytes());
    fixed[7] = u8::from(header.retired);
    put_u64(&mut fixed, 8, header.identity.generation.get());
    put_u64(&mut fixed, 16, header.identity.schema_epoch.get());
    header
        .anchor
        .encode(&mut fixed[24..48])
        .map_err(|_| CatalogError::InvalidInput)?;
    match definition {
        CatalogDefinition::Table(_) => fixed[6] = 1,
        CatalogDefinition::Index(index) => {
            fixed[6] = 2;
            put_u64(&mut fixed, 48, index.parent.object.get());
            put_u64(&mut fixed, 56, index.parent.generation.get());
            put_u64(&mut fixed, 64, index.parent.schema_epoch.get());
        }
    }
    fixed[72..76].copy_from_slice(&((length - CATALOG_BEFORE_HEADER_BYTES) as u32).to_le_bytes());
    output[..CATALOG_BEFORE_HEADER_BYTES].copy_from_slice(&fixed);
    write_definition(definition, &mut output[CATALOG_BEFORE_HEADER_BYTES..length]);
    Ok(length)
}

pub(crate) fn decode_catalog_before(
    input: &[u8],
    expected: CatalogExpectation,
) -> Result<CatalogBeforeView<'_>, CatalogError> {
    if expected.namespace.timeline == 0 {
        return Err(CatalogError::InvalidInput);
    }
    if input.len() < CATALOG_BEFORE_HEADER_BYTES || &input[..4] != b"XCB1" {
        return Err(CatalogError::Corrupt);
    }
    if input[4..6] != 1_u16.to_le_bytes() {
        return Err(CatalogError::UnsupportedFormat);
    }
    if input[7] & !1 != 0
        || !zero(&input[76..96])
        || u32_at(input, 72) as usize != input.len() - CATALOG_BEFORE_HEADER_BYTES
        || input.len() > MAX_CATALOG_BEFORE_BYTES
    {
        return Err(CatalogError::Corrupt);
    }
    let header = CatalogBeforeHeader {
        identity: ObjectHandle {
            object: expected.object,
            generation: Generation::new(u64_at(input, 8)).map_err(|_| CatalogError::Corrupt)?,
            schema_epoch: SchemaEpoch::new(u64_at(input, 16)).map_err(|_| CatalogError::Corrupt)?,
        },
        retired: input[7] & 1 != 0,
        anchor: PageRef::decode(&input[24..48]).map_err(|_| CatalogError::Corrupt)?,
    };
    let body = &input[CATALOG_BEFORE_HEADER_BYTES..];
    let definition = match input[6] {
        1 if zero(&input[48..72]) => CatalogDefinitionView::Table(decode_table(body)?),
        2 => {
            let parent = decode_identity(input, 48)?;
            if parent.object == expected.object {
                return Err(CatalogError::Corrupt);
            }
            CatalogDefinitionView::Index(decode_index(body, parent)?)
        }
        _ => return Err(CatalogError::Corrupt),
    };
    Ok(CatalogBeforeView {
        namespace: expected.namespace,
        header,
        definition,
    })
}

/// Conservative sum of source-level values, including sequential branches.
/// This is a test observation, not an ABI stack/RSS or live-owner bound. The
/// nominal caller has exact-length String/Vec storage; arbitrary capacities
/// remain its owner's responsibility. Page envelopes and page loans are out
/// of this pure-codec census. No phase-max or scratch alias discount is used.
#[cfg(test)]
pub(super) fn source_census() -> [(&'static str, usize); 10] {
    use super::{
        CatalogColumn, CatalogHeader, CatalogRecordView, IndexDefinition, IndexMode,
        TableDefinitionView,
    };
    use crate::{
        api::{Column, DataType, TableSpec},
        storage::{
            key::{CompositeKey, KeyError, PhysicalKey},
            page::{FileId, PageError, PageIdentity},
            row_record::{CanonicalRowView, RowError, ValueRef},
            transaction_record::{LogicalRowVersion, TxnRef, VersionStamp},
            undo_record::{
                UndoBeforeKind, UndoCell, UndoChunkContext, UndoChunkVerifier, UndoChunkView,
                UndoCreator, UndoEdge, UndoEdgeRole, UndoError, UndoLink, UndoPayload,
                UndoPayloadHead, UndoRecord, UndoRecordExpectation, UndoRecordMetadata,
                UndoRecordView, UndoRelease, UndoReleaseProgress, UndoTargetView,
            },
            undo_ref::UndoRef,
        },
        transaction::TxnId,
    };
    use std::mem::size_of;
    use xadb_foundation::{
        codec::{ContentHasher, KeyPrefix},
        identity::{CommitSeq, ObjectId, RowId},
    };

    macro_rules! values {
        ($($value:ty),* $(,)?) => { 0 $(+ size_of::<$value>())* };
    }

    // Actual caller byte backing fixed by the accepted input, plus actual
    // schema carriers and exact-length names. All five buffers remain counted.
    let nominal_caller = 4452
        + 4452
        + 2480
        + 432
        + 4612
        + size_of::<TableSpec>()
        + 32 * size_of::<Column>()
        + 4224
        + 128
        + 4
        + size_of::<IndexDefinition<'static>>();

    // Retained inputs/outputs at the prepared-codec caller, independent of all
    // by-value argument/return carriers counted again below.
    let retained = values!(
        CatalogExpectation, CatalogBeforeHeader, CatalogDefinition<'static>,
        CatalogBeforeView<'static>, CatalogDefinitionView<'static>, CatalogHeader,
        CatalogRecordView<'static>, UndoRecordExpectation<'static>, UndoRecord<'static>,
        UndoCell<'static>, UndoRecordView<'static>, UndoPayloadHead, UndoChunkContext,
        UndoChunkView<'static>, UndoChunkVerifier, [Option<(UndoEdgeRole, UndoLink)>; 2],
        UndoRecordMetadata<'static>, UndoReleaseProgress,
        Result<CatalogBeforeView<'static>, CatalogError>,
        Result<Option<CatalogBeforeView<'static>>, UndoError>,
        Result<CatalogBeforeView<'static>, UndoError>, Result<UndoRecordView<'static>, UndoError>,
        Result<usize, CatalogError>, Result<usize, UndoError>, Result<(), UndoError>
    );

    // length -> definition_len; encode -> length -> shared writer; decode ->
    // shared definition decoder; the three view accessors. Child values are
    // below, and the caller's own return Result remains counted here too.
    let before_codecs = values!(
        CatalogExpectation, CatalogBeforeHeader, CatalogDefinition<'static>,
        Result<usize, CatalogError>,
        CatalogExpectation, CatalogBeforeHeader, CatalogDefinition<'static>, &mut [u8],
        usize, [u8; 96], IndexDefinition<'static>, Result<usize, CatalogError>,
        &[u8], CatalogExpectation, CatalogBeforeHeader, &[u8], CatalogDefinitionView<'static>,
        ObjectHandle, CatalogBeforeView<'static>, Result<CatalogBeforeView<'static>, CatalogError>,
        CatalogBeforeView<'static>, ObjectHandle, Result<(), CatalogError>,
        CatalogBeforeView<'static>, CatalogBeforeHeader,
        CatalogBeforeView<'static>, CatalogDefinitionView<'static>
    );

    // Shared definition_len/write_definition/decode_table/decode_index and
    // TableDefinitionView::column. Loop cursor/element values are simultaneous
    // locals, not multiplied by loop iterations. The offsets are counted both
    // inside the actual TableDefinitionView and as explicit 66-byte scratch.
    let definition_codecs = values!(
        ObjectId, CatalogDefinition<'static>, &TableSpec, usize, usize, &Column, &Column,
        IndexDefinition<'static>, &u8, Result<usize, CatalogError>,
        CatalogDefinition<'static>, &mut [u8], &TableSpec, usize, &Column,
        IndexDefinition<'static>, DataType, u8,
        &[u8], usize, &str, TableDefinitionView<'static>, [u16; 33], usize, usize,
        &[u8], usize, &str, usize, Result<TableDefinitionView<'static>, CatalogError>,
        &[u8], ObjectHandle, IndexMode, usize, &[u8], usize, &u8,
        IndexDefinition<'static>, Result<IndexDefinition<'static>, CatalogError>,
        TableDefinitionView<'static>, usize, usize, usize, CatalogColumn<'static>,
        Result<CatalogColumn<'static>, CatalogError>,
        &[u8], usize, ObjectHandle, Result<ObjectHandle, CatalogError>,
        &str, bool, &[u8], &str, Result<&str, CatalogError>,
        DataType, u8, u8, Result<DataType, CatalogError>
    );

    // The modified XCT1 wrapper shares only definitions: retain its existing
    // full header, child argument values, decode return, and header validator.
    let catalog_bridge = values!(
        CatalogExpectation, CatalogHeader, CatalogDefinition<'static>,
        Result<usize, CatalogError>,
        CatalogExpectation, CatalogHeader, CatalogDefinition<'static>, &mut [u8], usize,
        [u8; 232], IndexDefinition<'static>, &mut [u8], Result<usize, CatalogError>,
        CatalogHeader, CatalogExpectation, TxnId, Result<(), CatalogError>,
        &[u8], CatalogExpectation, Option<UndoRef>, CatalogHeader, &[u8],
        CatalogDefinitionView<'static>, ObjectHandle, CatalogRecordView<'static>,
        Result<CatalogRecordView<'static>, CatalogError>
    );

    // encode_undo_record -> length -> validate_record -> typed XCB1 decode;
    // decode_undo_record -> validate_record; free inspection shares the entry.
    // Every record/cell copy and fallible return is counted, not a guessed
    // maximum of the encode/decode branches.
    let undo_roots = values!(
        UndoCell<'static>, UndoRecordExpectation<'static>, &[Column], &mut [u8], usize,
        [u8; 432], &[u8], &[u8], UndoRecord<'static>, usize, UndoEdge, UndoLink, usize,
        VersionStamp, LogicalRowVersion, &[u8], UndoPayloadHead, Result<usize, UndoError>,
        UndoCell<'static>, UndoRecordExpectation<'static>, &[Column],
        UndoRecord<'static>, Result<usize, UndoError>,
        UndoRecord<'static>, bool, UndoRecordExpectation<'static>, &[Column], UndoEdge,
        UndoLink, UndoRelease, UndoEdge, UndoEdge, bool, bool, bool, UndoEdge,
        LogicalRowVersion, TxnRef, usize, &[u8], UndoPayloadHead, usize, PageRef,
        Result<usize, UndoError>,
        &[u8], UndoRecordExpectation<'static>, &[Column], Generation, UndoCell<'static>,
        usize, UndoTargetView<'static>, UndoBeforeKind, usize, UndoPayload<'static>,
        UndoPayloadHead, UndoRecord<'static>, UndoRecordView<'static>,
        Result<UndoRecordView<'static>, UndoError>, UndoError,
        UndoRecordExpectation<'static>, Result<(), UndoError>,
        &[u8], Generation, Generation, Result<Generation, UndoError>,
        &[u8], Result<(), UndoError>, Generation, &mut [u8], Result<usize, UndoError>
    );

    // Live accessors, domain guards, catalog expectation and loaded-edge
    // validation; include the legacy row materializer's actual return carrier
    // even though its newly guarded Catalog path exits before row decoding.
    let undo_accessors = values!(
        UndoRecordView<'static>, UndoRecord<'static>, Result<UndoRecord<'static>, UndoError>,
        UndoRecordView<'static>, UndoRecord<'static>,
        Result<Option<CatalogBeforeView<'static>>, UndoError>,
        UndoRecordView<'static>, &[u8], UndoRecord<'static>, UndoPayloadHead,
        Result<CatalogBeforeView<'static>, UndoError>,
        UndoRecordView<'static>, &[u8], &[Column], UndoRecord<'static>, UndoPayloadHead,
        Result<CanonicalRowView<'static>, UndoError>,
        UndoRecord<'static>, Namespace, CatalogExpectation, Result<CatalogExpectation, UndoError>,
        UndoRecordView<'static>, UndoRecord<'static>,
        UndoRecordMetadata<'static>, Result<UndoRecordMetadata<'static>, UndoError>,
        UndoRecordView<'static>, UndoRecord<'static>, UndoReleaseProgress,
        Result<UndoReleaseProgress, UndoError>,
        UndoRecordView<'static>, UndoRecord<'static>, UndoEdgeRole, usize, UndoLink,
        [Option<(UndoEdgeRole, UndoLink)>; 2],
        UndoRecordView<'static>, UndoRecord<'static>, TxnRef, Option<TxnRef>,
        UndoRecordView<'static>, UndoEdgeRole, UndoRecordView<'static>, UndoRecord<'static>,
        UndoRecord<'static>, UndoLink, Result<(), UndoError>,
        UndoRecordView<'static>, UndoChunkView<'static>, UndoRecord<'static>, UndoChunkContext,
        Result<(), UndoError>
    );

    // Chunk context, cell encoder/decoder and positional validator. Page
    // encoder/decoder are deliberately outside this accepted pure-cell input.
    let chunk_codecs = values!(
        UndoRecordView<'static>, PageIdentity, UndoRecord<'static>, UndoPayloadHead,
        UndoChunkContext, Result<UndoChunkContext, UndoError>,
        UndoChunkContext, PageRef, usize, Option<PageRef>, &[u8], &mut [u8], usize,
        [u8; 160], PageRef, Result<usize, UndoError>,
        &[u8], UndoChunkContext, PageRef, usize, usize, Option<PageRef>,
        UndoChunkView<'static>, Result<UndoChunkView<'static>, UndoError>,
        UndoChunkContext, PageRef, usize, Option<PageRef>, usize, UndoPayloadHead, PageRef,
        Result<(), UndoError>,
        UndoPayloadHead, &mut [u8], Result<usize, UndoError>,
        &[u8], [u8; 32], UndoPayloadHead, Result<UndoPayloadHead, UndoError>,
        PageRef, usize, [u8; 32], UndoPayloadHead, Result<UndoPayloadHead, UndoError>,
        UndoChunkView<'static>, &[u8], UndoChunkView<'static>, Option<PageRef>
    );

    // Verifier new/push/finish, push's copied hash, digest's finalize clone,
    // and independent materializer hashing. Include hash Result/return arrays.
    let chunk_verifier = values!(
        UndoChunkContext, UndoChunkVerifier,
        &mut UndoChunkVerifier, UndoChunkView<'static>, &PageRef, ContentHasher,
        Result<(), UndoError>, &UndoChunkVerifier, Result<(), UndoError>,
        ContentHasher, &mut ContentHasher, &[u8], &ContentHasher, ContentHasher, [u8; 32],
        &[u8], ContentHasher, [u8; 32], [u8; 32]
    );

    // Wire/identity child paths used above: numeric read/write, namespace/
    // address predicates, target parse, edge codec, reference codecs, TxnId/
    // TxnRef/stamp/logical codecs and checked identity/error conversions.
    // Explicit arrays include the real TxnRef48/stamp56/logical48 scratch.
    let wire_helpers = values!(
        &[u8], usize, [u8; 2], u16, &[u8], usize, [u8; 4], u32,
        &[u8], usize, [u8; 8], u64, &mut [u8], usize, u64, [u8; 8],
        &mut [u8], usize, u16, [u8; 2], &mut [u8], usize, usize, [u8; 4],
        &[u8], &u8, bool, PageRef, PageRef, bool, TxnId, Namespace, bool,
        UndoTargetView<'static>, Result<(), UndoError>, &[u8], &[u8],
        UndoTargetView<'static>, Result<UndoTargetView<'static>, UndoError>,
        UndoTargetView<'static>, &mut [u8],
        UndoEdge, u8, u8, &[u8], UndoLink, UndoEdge, Result<UndoEdge, UndoError>,
        UndoRef, &mut [u8], Result<(), PageError>, &[u8], PageRef, u16, [u8; 8],
        Generation, UndoRef, Result<UndoRef, PageError>,
        PageRef, &mut [u8], Result<(), PageError>, &[u8], PageRef, Result<PageRef, PageError>,
        FileId, u64, Result<FileId, PageError>,
        TxnId, &mut [u8], Result<(), PageError>, &[u8], [u8; 16], [u8; 4],
        TxnId, Result<TxnId, PageError>, TxnId, bool,
        TxnId, Generation, TxnRef, Result<TxnRef, PageError>,
        TxnRef, &mut [u8], [u8; 48], Result<(), PageError>,
        &[u8], TxnId, Generation, Result<TxnRef, PageError>,
        VersionStamp, &mut [u8], [u8; 56], Result<(), PageError>,
        &[u8], CommitSeq, TxnRef, VersionStamp, Result<VersionStamp, PageError>,
        LogicalRowVersion, &mut [u8], [u8; 48], Result<(), PageError>,
        &[u8], TxnId, u64, LogicalRowVersion, Result<LogicalRowVersion, PageError>,
        u64, std::num::NonZeroU64, Option<std::num::NonZeroU64>,
        xadb_foundation::Error, xadb_foundation::Result<Generation>,
        xadb_foundation::Result<ObjectId>, xadb_foundation::Result<SchemaEpoch>,
        xadb_foundation::Result<CommitSeq>, CatalogError, UndoError, PageError, RowError,
        CatalogError, UndoError, PageError, UndoError,
        UndoCreator, Result<CanonicalRowView<'static>, RowError>,
        // A valid different-domain input reaches these existing children
        // before the new target/kind guards. Include those value paths too.
        &[u8], usize, usize, CompositeKey<'static>, [u8; 8], RowId,
        PhysicalKey<'static>, Result<PhysicalKey<'static>, KeyError>,
        &[u8], usize, &[u8], u8, KeyPrefix, CompositeKey<'static>,
        Result<CompositeKey<'static>, KeyError>, usize, Result<(), KeyError>,
        &[u8], u8, usize, KeyPrefix, xadb_foundation::Result<KeyPrefix>,
        xadb_foundation::Result<RowId>, KeyError,
        &[u8], &[Column], usize, [u32; 33], usize, usize, &Column,
        ValueRef<'static>, usize, CanonicalRowView<'static>,
        Result<CanonicalRowView<'static>, RowError>,
        &[Column], Result<(), RowError>, &Column, ValueRef<'static>, bool,
        &[u8], u8, [u8; 8], usize, usize, &[u8], ValueRef<'static>,
        Result<(ValueRef<'static>, usize), RowError>,
        std::str::Utf8Error, Result<&str, std::str::Utf8Error>
    );

    [
        ("nominal_caller", nominal_caller),
        ("retained_inputs_returns", retained),
        ("xcb1_codecs_accessors", before_codecs),
        ("shared_definition_helpers", definition_codecs),
        ("xct1_bridge", catalog_bridge),
        ("undo_root_codecs", undo_roots),
        ("undo_accessors_edges", undo_accessors),
        ("chunk_cell_codecs", chunk_codecs),
        ("chunk_verifier_hash", chunk_verifier),
        ("wire_identity_helpers", wire_helpers),
    ]
}
