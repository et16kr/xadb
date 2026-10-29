//! Source/type census for the actual scope2 pure consumer.
//!
//! This is not a compiler-stack/RSS estimate. Caller page, Job, after-image,
//! frame and schema backing stays in the original loans. The caller also owns
//! its Inputs/ReservePlan and any retained candidate LayoutView/schema view.
//! Only borrowed argument carriers and copies made inside this call tree are
//! included here. Nothing is subtracted on an optimizer-liveness assumption.
//! A `max` joins completed function calls or explicit lexical branches; their
//! still-live parent values are added outside that `max`.
use super::{
    Change, CoreView, Error, Inputs, LayoutView, Observation, Operation, Result, Shape,
    core::CoreInput,
    effects::EffectIndices,
    frame::{FrameSpec, Native, PageChange},
    images::{
        CatalogChange, RootContext, StatusChange, StatusContext, catalog::CatalogChangeObservation,
    },
    ledger::TransactionTransition,
    queue::QueueChange,
    transition::Output,
    wire::{NativeRecords, NativeView},
};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{
            AllocationEntry, AllocationError, AllocationPage, ReservationOwner, SystemWorkRef,
        },
        anchor::{AnchorRoot, TreeAnchor},
        catalog_record::{
            CatalogColumn, CatalogDefinition, CatalogDefinitionView, CatalogError,
            CatalogExpectation, CatalogHeader, CatalogRecordView, CatalogTableInput,
            IndexDefinition, TableDefinitionView, before::CatalogBeforeView,
        },
        control::Profile,
        obligation::{
            self, DescriptorSpec, InventoryClaim, ObligationError, OwnerChain, RemainingBudget,
            SystemProgress,
        },
        page::{PageError, PageHeader, PageIdentity, PageKind, PageRef, PageView},
        retention_obligation::{self, AgingDescriptorView, AgingObligationError},
        retention_page::{
            RetentionError,
            catalog::{self, JobCells, JobView, RootView},
            catalog3,
        },
        status_home::{StatusHome, StatusHomePage},
        transaction_record::{
            LogicalRowVersion, StatusSlotRef, TransactionState, TransactionStatus,
            TransactionStatusCell, TxnRef, VersionStamp,
        },
        tree_page::{
            self, SearchPosition, TreeEntry, TreeError, TreeExpectation, TreePageView,
            edit::{LeafEdit, LeafEditError, LeafEditPlan},
        },
        undo_record::{
            UndoCell, UndoError, UndoRecord, UndoRecordExpectation, UndoRecordView, UndoTargetView,
        },
        undo_ref::UndoRef,
        wal_frame::{
            FrameEnvelope, FrameExpectation, RedoRecordView, RedoTarget, WalError, WalFrameHeader,
        },
    },
};
use foundation::{
    codec::ContentHasher,
    identity::{CommitSeq, ObjectHandle, ObjectId},
};
use std::{
    iter::Enumerate,
    mem::size_of,
    ops::Range,
    slice::{ChunksExact, ChunksExactMut, Iter, IterMut},
};

type CodecResult<T, E> = std::result::Result<T, E>;
type Computed<'a> = (Observation, [bool; 8], Option<JobView<'a>>, usize);

/// Callee result and caller's extracted value coexist at a return boundary.
/// Including both deliberately does not rely on return-value optimization.
fn returned<T, E>() -> usize {
    size_of::<T>() + size_of::<CodecResult<T, E>>()
}

/// The codec terms that differ between the exact Profile2 and Profile3
/// instances of the one transition body. Every type term below is written
/// with the Profile2 views; the Profile3 views have the same sizes, which the
/// Profile3 census test checks, so only these function terms change.
///
/// Neither instance itemizes the shared format seam's own forwarding frames:
/// `run_with_buffers` -> `run_format`, the `AgingStep` carrier of
/// `Format::next_aging` and the codec Result conversions. The Profile2 values
/// equal the recorded pre-seam branch values only because those frames are
/// not counted; that equality is not an upper bound on the seam itself.
#[derive(Clone, Copy)]
struct Codecs {
    /// Root/Job cells, both page wrappers and the selected 64-cell census.
    queue: usize,
    /// Root-bound Aging descriptor encode/decode.
    aging: usize,
    /// The Root-bound Aging successor that compute calls.
    aging_next: fn(Codecs) -> usize,
}
impl Codecs {
    fn catalog2() -> Self {
        Self {
            queue: catalog::source_bytes(),
            aging: retention_obligation::catalog::source_bytes(),
            aging_next,
        }
    }
    fn row3() -> Self {
        Self {
            queue: catalog3::source_bytes(),
            aging: retention_obligation::row::source_bytes(),
            aging_next: row_aging_next,
        }
    }
}

/// Scalar/reference temporaries in the common page envelope, including the
/// complete hash state, decoded identity/header, slot walk and both returns.
fn page_codec() -> usize {
    size_of::<[u8; 16]>()
        + size_of::<[u8; 32]>()
        + 2 * size_of::<ContentHasher>() // content_digest -> digest return
        + 2 * size_of::<PageHeader>() // caller input and decoded header
        + 2 * size_of::<PageIdentity>() // expected and actual
        + returned::<PageRef, PageError>()
        + returned::<PageView<'_>, PageError>()
        + 2 * size_of::<CodecResult<(), PageError>>()
        + size_of::<(&[u8], &mut [u8], &[&[u8]])>()
        // kind/level; slots/lower/upper/end; slot/entry/start/length; digest span.
        + size_of::<(PageKind, u8, [usize; 9], Range<usize>)>()
        + size_of::<Enumerate<Iter<'_, &[u8]>>>()
}

/// XCT1 header/schema paths. Spec and borrowed View are separate branches, but
/// the actual CatalogTableInput enum and each returned definition retain their
/// full compiler-selected widths in either branch.
fn catalog_codec() -> usize {
    let shared = size_of::<CatalogTableInput<'_>>()
        + size_of::<CatalogDefinition<'_>>()
        + 2 * size_of::<CatalogHeader>()
        + 2 * size_of::<CatalogExpectation>()
        + returned::<CatalogRecordView<'_>, CatalogError>()
        + returned::<CatalogDefinitionView<'_>, CatalogError>()
        + returned::<IndexDefinition<'_>, CatalogError>() // malformed/non-table candidate path
        + returned::<TableDefinitionView<'_>, CatalogError>()
        + returned::<CatalogColumn<'_>, CatalogError>()
        + returned::<ObjectHandle, CatalogError>()
        + returned::<VersionStamp, PageError>()
        + returned::<LogicalRowVersion, PageError>()
        + returned::<UndoRef, PageError>()
        + returned::<PageRef, PageError>()
        + size_of::<(Option<UndoRef>, Option<ObjectHandle>, TxnRef)>()
        + size_of::<([usize; 7], u8, bool)>() // length/name_end/offset/end/i/previous/arity
        + size_of::<(&[u8], &[u8], &str, &str, &mut [u8])>()
        + 4 * size_of::<CodecResult<(), CatalogError>>();
    // encode_catalog_table_record's fixed header stays live while encode_header
    // encodes reference fields. Decode's offsets are already inside the actual
    // TableDefinitionView above, rather than an additional hidden allocation.
    shared
        + size_of::<[u8; 232]>()
        + size_of::<(Option<VersionStamp>, Option<LogicalRowVersion>)>()
        + size_of::<([u8; 48], [u8; 40], [u8; 24])>()
}

fn tree_codec() -> usize {
    let (key, key_result, layout, layout_result) = tree_page::catalog_source_carrier_bytes();
    page_codec()
        + 3 * key // lower_bound query/current; validate_cells previous
        + 2 * key_result
        + layout + layout_result
        + returned::<TreePageView<'_>, TreeError>()
        + returned::<TreeEntry<'_>, TreeError>()
        + returned::<SearchPosition, TreeError>()
        + size_of::<(TreeExpectation, PageHeader, Option<PageRef>, Option<&[u8]>)>()
        + size_of::<([usize; 8], bool, u8)>() // count/capacity/slot/cost/entry_bytes/left/right/middle
        + size_of::<(&[u8], &[u8], &[u8], &[u8])>()
        + 3 * size_of::<CodecResult<(), TreeError>>()
}

fn leaf_edit() -> usize {
    // `cells` itself belongs to change_catalog and is charged there once.
    tree_codec()
        + size_of::<LeafEdit<'_>>()
        + size_of::<(&TreePageView<'_>, &mut [&[u8]])>()
        + size_of::<(PageHeader, SearchPosition, [usize; 3], &[u8])>()
        // cell_at captures self/edit/at, and validate_cells borrows that closure.
        + size_of::<(&TreePageView<'_>, LeafEdit<'_>, usize)>()
        + size_of::<Enumerate<IterMut<'_, &[u8]>>>()
        + returned::<LeafEditPlan<'_, '_>, LeafEditError>()
        + size_of::<CodecResult<(), TreeError>>()
}

fn projection() -> usize {
    size_of::<(Namespace, ObjectId, Option<&[u8]>)>()
        + size_of::<[u8; 48]>()
        + 2 * size_of::<ContentHasher>()
        + 2 * size_of::<UndoTargetView<'_>>() // target plus encode target/expected copies
        + returned::<[u8; 32], Error>()
        + returned::<CatalogRecordView<'_>, CatalogError>()
        + catalog_codec()
}

fn catalog_image() -> usize {
    let common = size_of::<CatalogChange<'_>>()
        + size_of::<RootContext<JobView<'_>>>()
        + size_of::<(&[u8; 16384], &[u8; 16384], &mut [u8; 16384])>()
        + size_of::<(Operation, PageHeader, TreeAnchor, TreePageView<'_>)>()
        + size_of::<(
            ObjectHandle,
            TxnRef,
            [PageRef; 2],
            UndoRef,
            CatalogExpectation,
        )>()
        + size_of::<([u8; 8], SearchPosition, [u8; 32])>()
        + size_of::<[u8; super::MAX_CELL]>()
        + size_of::<[&[u8]; super::images::CATALOG_EDIT_CELLS]>()
        + size_of::<(LeafEdit<'_>, CatalogChangeObservation)>()
        + returned::<CatalogChangeObservation, Error>();
    // Forward/Prune borrow the existing after-page loan for record scratch.
    // encode_leaf_cell copies it into the owned cell before plan.encode writes
    // the final page. Count the slice carrier, not a second owned value array.
    let forward = size_of::<(CatalogTableInput<'_>, CatalogHeader, usize, [u8; 32], &[u8])>()
        + size_of::<&mut [u8]>()
        + returned::<usize, CatalogError>()
        + returned::<&[u8], TreeError>()
        + catalog_codec().max(projection()).max(tree_codec());
    let remove_or_prune = size_of::<(TreeEntry<'_>, &[u8], CatalogRecordView<'_>, CatalogHeader)>()
        + size_of::<&mut [u8]>()
        + size_of::<([u8; 32], [u8; 32], CommitSeq, &[u8], &[u8])>()
        + returned::<CatalogRecordView<'_>, CatalogError>()
        + catalog_codec().max(projection()).max(tree_codec());
    // Match-arm record borrows end before prepare_leaf_edit; the returned edit
    // borrows only cell. Keep the common cell/reference arrays and result.
    common
        + forward
            .max(remove_or_prune)
            .max(leaf_edit())
            // All prior leaf values are validated before the owned cell array
            // is declared. Keeping that array here is conservative.
            .max(
                catalog_codec()
                    + returned::<CatalogRecordView<'_>, CatalogError>()
                    + size_of::<(TreeEntry<'_>, [u8; 8], CatalogExpectation)>(),
            )
            .max(page_codec())
}

fn undo_codec() -> usize {
    // encode_undo_record owns a second 432-byte header while the caller's
    // candidate lives. Decode/material/encode pass the actual wide enums.
    size_of::<[u8; 432]>()
        + 3 * size_of::<UndoRecord<'_>>() // material result, encoder record, validation input
        + 2 * size_of::<UndoCell<'_>>() // encode argument and matched cell
        + returned::<UndoRecordView<'_>, UndoError>()
        + 3 * size_of::<UndoRecordExpectation<'_>>()
        + returned::<UndoTargetView<'_>, UndoError>()
        + size_of::<(TxnRef, VersionStamp, LogicalRowVersion, Option<UndoRef>)>()
        + size_of::<([u8; 40], [u8; 32], [u8; 32])>()
        + size_of::<(&[u8], &[u8], &mut [u8], [usize; 8], [u64; 4])>()
        + 2 * size_of::<ContentHasher>()
        + 4 * size_of::<CodecResult<(), UndoError>>()
        // A malformed retained before can reach CatalogValue validation before
        // the exact Missing comparison rejects it. Its schema parse is charged
        // too; a valid-success-only path is not a safe resource upper bound.
        + returned::<CatalogBeforeView<'_>, CatalogError>()
        + catalog_codec()
}

fn undo_image() -> usize {
    size_of::<(Operation, RootContext<JobView<'_>>, &[u8; 16384], &mut [u8; 16384])>()
        + size_of::<(UndoRef, PageHeader, PageView<'_>, &[u8], UndoRecordExpectation<'_>)>()
        + size_of::<[u8; 432]>() // candidate or retained canonical, in different functions
        + size_of::<UndoRecord<'_>>() // validate_retained_undo::record
        + returned::<UndoRecordView<'_>, UndoError>()
        + size_of::<(u8, u64, Result<()>)>()
        + undo_codec().max(page_codec())
}

fn transaction_codec() -> usize {
    crate::storage::transaction_record::status_codec_source_bytes()
}

fn status_codec() -> usize {
    // change_status::write cell and StatusHome::encode bytes are simultaneous.
    size_of::<[u8; 224]>() * 2
        + 3 * size_of::<StatusHome>() // write arg, encode self, decoder result
        + returned::<StatusHome, PageError>()
        + returned::<StatusHomePage<'_>, PageError>()
        + returned::<TransactionStatusCell, PageError>()
        + returned::<TransactionStatus, PageError>()
        + size_of::<(PageHeader, Namespace, TransactionState, Option<UndoRef>, TxnRef)>()
        + size_of::<([u8; 8], [u8; 48], [u8; 40])>()
        + size_of::<(&[u8], &mut [u8], [usize; 3])>()
        + transaction_codec()
        + page_codec()
}

fn status_image() -> usize {
    size_of::<(StatusChange, StatusContext, &[u8; 16384], &mut [u8; 16384])>()
        + size_of::<(TxnRef, StatusSlotRef, PageHeader, StatusHomePage<'_>)>()
        + 3 * size_of::<StatusHome>() // old, updated, TerminalFinish free branch
        + size_of::<(TransactionStatus, TransactionState, Option<UndoRef>, u64)>()
        + returned::<StatusHome, Error>()
        + status_codec()
}

fn allocation_image() -> usize {
    size_of::<(Operation, RootContext<JobView<'_>>, &[u8; 16384], &mut [u8; 16384])>()
        + size_of::<(PageHeader, AllocationPage<'_>, [PageRef; 2], ObjectHandle, TxnRef)>()
        + size_of::<[AllocationEntry; 64]>()
        + size_of::<[u8; crate::storage::allocation_page::ALLOCATION_CELL_BYTES]>()
        + 3 * size_of::<AllocationEntry>() // iterator current/reserved/allocated
        + returned::<AllocationEntry, AllocationError>()
        + returned::<AllocationPage<'_>, AllocationError>()
        + size_of::<(ReservationOwner, PageRef, [u8; 24], [usize; 3])>()
        + size_of::<Enumerate<IterMut<'_, AllocationEntry>>>()
        + size_of::<Enumerate<std::array::IntoIter<PageRef, 2>>>()
        + size_of::<Enumerate<Iter<'_, AllocationEntry>>>()
        + 3 * size_of::<CodecResult<(), AllocationError>>()
        + page_codec()
}

fn initialize_pair() -> usize {
    size_of::<(RootContext<JobView<'_>>, &mut [u8; 16384], &mut [u8; 16384])>()
        + size_of::<([PageRef; 2], ObjectHandle, AnchorRoot, PageHeader)>()
        + size_of::<[u8; 48]>() // anchor encoder cell
        + size_of::<[u8; 32]>() // row metadata
        + size_of::<[&[u8]; 1]>()
        + 3 * size_of::<Result<()>>()
        + tree_codec().max(page_codec())
}

fn descriptor_codec() -> usize {
    obligation::descriptor_source_bytes()
}

fn transaction_remaining() -> usize {
    transaction_remaining_with::<JobView<'static>>()
}

/// `ledger::transaction_remaining::<J>`; the Job view is its only J term.
fn transaction_remaining_with<J>() -> usize {
    size_of::<(TransactionStatus, Option<J>, bool, usize)>()
        + size_of::<[RemainingBudget; 5]>()
        + returned::<[RemainingBudget; 5], Error>()
        + 2 * size_of::<RemainingBudget>() // debt return and selected assignment
        + size_of::<(Option<UndoRef>, TransactionState, Shape)>()
}

fn transaction_validate() -> usize {
    transaction_validate_with::<JobView<'static>>()
}

/// `ledger::validate_transaction::<J>` with its nested remaining closure.
fn transaction_validate_with<J>() -> usize {
    size_of::<(&[u8; 1536], Profile, StatusHome, Option<J>)>()
        + size_of::<(TransactionStatus, DescriptorSpec, bool, u64)>()
        + returned::<DescriptorSpec, ObligationError>()
        + 2 * size_of::<Result<()>>()
        + descriptor_codec().max(transaction_remaining_with::<J>())
}

fn transaction_next_encode() -> usize {
    size_of::<(&TransactionTransition<'_>, &mut [u8; 1536])>()
        + size_of::<(TransactionStatus, u64)>()
        + size_of::<DescriptorSpec>() // one mutable decoded/initial candidate
        + returned::<DescriptorSpec, ObligationError>()
        + size_of::<(TransactionStatus, InventoryClaim, bool, [usize; 2])>()
        + returned::<[RemainingBudget; 5], Error>()
        + 3 * size_of::<Result<()>>()
        + descriptor_codec().max(transaction_remaining())
}

fn transaction_next() -> usize {
    // The owned transition packet belongs to the wrapper. The encoder borrows
    // it and returns () before validation decodes its own descriptor.
    size_of::<(TransactionTransition<'_>, &mut [u8; 1536])>()
        + 2 * size_of::<Result<()>>()
        + transaction_next_encode().max(transaction_validate())
}

fn transaction_inventory() -> usize {
    size_of::<(
        &Inputs<'_>,
        Operation,
        TxnRef,
        Option<JobView<'_>>,
        Option<JobView<'_>>,
        u64,
    )>() + size_of::<Option<DescriptorSpec>>()
        + size_of::<Option<CodecResult<DescriptorSpec, ObligationError>>>()
        + size_of::<CodecResult<Option<DescriptorSpec>, ObligationError>>()
        + size_of::<[u8; 288]>()
        + returned::<InventoryClaim, Error>()
        + 2 * size_of::<Result<()>>()
        + descriptor_codec().max(inventory())
}

fn aging_budget() -> usize {
    size_of::<(RootView<'_>, &[u8; 640], [u64; 2])>()
        + size_of::<[RemainingBudget; 5]>()
        + size_of::<[u8; 640]>()
        + returned::<RemainingBudget, Error>()
        + 2 * size_of::<Result<()>>()
        // Conservative page/hash closure includes aggregate digest temporaries.
        + page_codec()
}

fn aging_predecessor(codecs: Codecs) -> usize {
    size_of::<(Profile, Option<&[u8; 1536]>, &[u8; 16384], RootView<'_>)>()
        + size_of::<DescriptorSpec>()
        + returned::<DescriptorSpec, ObligationError>()
        + returned::<AgingDescriptorView<'_>, AgingObligationError>()
        + returned::<(OwnerChain, InventoryClaim), Error>()
        + 2 * size_of::<RootView<'_>>()
        + 3 * size_of::<Result<()>>()
        // catalog::decode validates the predecessor, then descriptor() decodes
        // it again. Both calls finish in this helper, but their real return
        // carriers remain charged rather than treating the view as the value.
        + descriptor_codec().max(codecs.aging)
}

fn aging_next(codecs: Codecs) -> usize {
    let wrapper = size_of::<(
        Profile,
        Option<&[u8; 1536]>,
        &[u8; 16384],
        &[u8; 16384],
        &[u8; 640],
        &mut [u8; 1536],
    )>() + size_of::<(RootView<'_>, RemainingBudget)>()
        + size_of::<(OwnerChain, InventoryClaim, SystemProgress)>()
        + returned::<RemainingBudget, Error>()
        + returned::<(OwnerChain, InventoryClaim), Error>()
        + 3 * size_of::<Result<()>>();
    // The final descriptor is constructed only after both preparation helpers
    // return. Its encode phase retains the wrapper's scalar cut and budget
    // row, but no predecessor descriptor or canonical aggregate buffer.
    let encode = size_of::<DescriptorSpec>()
        + size_of::<[RemainingBudget; 5]>() // final descriptor array construction
        + codecs.aging;
    wrapper + aging_budget().max(aging_predecessor(codecs)).max(encode)
}

/// Profile3 successor, counted like `aging_next`: format::Row3::next_aging
/// builds the selected Job cells (its `AgingStep` carrier, like Profile2's,
/// is one of the uncounted seam frames named at `Codecs`) and calls
/// retention_obligation::row::next. That function's arguments, decoded after
/// Root, reference, chain/inventory pair and budget scalars stay live; its
/// Job3 census, predecessor decode and final encode are separate calls. The
/// predecessor's decoded spec or before Root is branch-local, and the final
/// descriptor exists only for the encode call, as in `aging_next`.
fn row_aging_next(codecs: Codecs) -> usize {
    let wrapper = 2 * size_of::<JobCells<'_>>()
        + size_of::<(
            retention_obligation::row::AgingTransition<'_>,
            &mut [u8; 640],
            &mut [u8],
        )>()
        + size_of::<(Namespace, PageHeader, RootView<'_>, SystemWorkRef)>()
        + size_of::<(OwnerChain, InventoryClaim)>()
        + size_of::<([u64; 2], RemainingBudget)>()
        + returned::<(), AgingObligationError>()
        + 3 * size_of::<Result<()>>();
    let predecessor = size_of::<(PageHeader, RootView<'_>)>()
        + size_of::<DescriptorSpec>()
        + returned::<DescriptorSpec, ObligationError>()
        + returned::<AgingDescriptorView<'_>, AgingObligationError>()
        + descriptor_codec().max(codecs.aging);
    let encode = size_of::<DescriptorSpec>() + size_of::<[RemainingBudget; 5]>() + codecs.aging;
    wrapper + codecs.queue.max(predecessor).max(encode)
}

fn queue(codecs: Codecs) -> usize {
    size_of::<(QueueChange<'_>, Namespace, RootView<'_>, &[[u8; 1024]; 64], u64, &mut [u8; 512], &mut [u8; 1024], &mut [u8; 640])>()
        + 2 * size_of::<JobCells<'_>>() // original/selected
        + size_of::<(Operation, Option<usize>, usize, JobView<'_>, RootView<'_>)>()
        + size_of::<([u64; 3], [u8; 32])>() // returned census masks/digest
        + returned::<Option<usize>, Error>()
        + returned::<JobView<'_>, RetentionError>()
        + size_of::<(JobView<'_>, TxnRef, StatusSlotRef, [u64; 3], bool)>() // lease/terminal walk
        + size_of::<[RemainingBudget; 5]>() // fill_budget rows, not caller 640 aggregate
        + size_of::<RemainingBudget>()
        + 4 * size_of::<Result<()>>()
        + codecs.queue
}

fn inventory() -> usize {
    size_of::<(Operation, JobView<'_>, &mut [u8; 624])>()
        + size_of::<[u8; 288]>() // deltas' encoded items, distinct from caller auxiliary
        + size_of::<([PageRef; 2], UndoRef, u8, usize)>()
        + returned::<InventoryClaim, Error>()
        + 2 * size_of::<ContentHasher>()
        + size_of::<Enumerate<ChunksExactMut<'_, u8>>>()
        + size_of::<std::iter::Zip<ChunksExactMut<'_, u8>, ChunksExact<'_, u8>>>()
        + 3 * size_of::<Result<()>>()
}

fn core() -> usize {
    size_of::<(CoreInput<JobView<'_>>, &mut [u8; 256])>()
        + size_of::<(Operation, JobView<'_>, UndoRef, u16, [bool; 3])>()
        + size_of::<([u8; 32], [u8; 32])>() // copied projection tuple
        + returned::<CoreView<'_>, Error>()
        + returned::<UndoRef, PageError>()
        + size_of::<([u64; 2], Option<UndoRef>, Option<UndoRef>)>()
        + 3 * size_of::<Result<()>>()
}

fn compute(codecs: Codecs) -> usize {
    // All function-scope named values, including later small result holders,
    // remain charged throughout. The 1536-byte descriptor arguments and eight
    // output pages are references here: run/caller already own that backing.
    let common = size_of::<(&Inputs<'_>, &mut [[u8; 16384]; 8], &mut [[u8; 1536]; 2], &mut [u8; 256])>()
        + size_of::<(Operation, Namespace, u64, RootView<'_>)>()
        + 6 * size_of::<Option<JobView<'_>>>() // selected/root_before/updated/semantic/root_after/core_root
        + size_of::<(Option<TxnRef>, Option<StatusSlotRef>, bool)>()
        + 2 * size_of::<Option<StatusHome>>() // old_status/new_status
        + size_of::<(u64, [u8; 32], [u8; 512], [u8; 1024], [u8; 640], [bool; 8])>()
        + size_of::<(Option<usize>, Option<RootContext<JobView<'_>>>, Option<([u8; 32], [u8; 32])>, usize)>()
        + size_of::<(Option<StatusChange>, [u64; 2], WalFrameHeader)>()
        + size_of::<Change<'_>>() // queue_change/match by-value copy
        // map + transpose + extracted output; not one Result substituted for
        // the nested Option<Result<...>> and Result<Option<...>> widths.
        + size_of::<Option<CodecResult<JobView<'_>, RetentionError>>>()
        + size_of::<CodecResult<Option<JobView<'_>>, RetentionError>>()
        + size_of::<Option<Result<JobView<'_>>>>()
        + size_of::<Result<Option<JobView<'_>>>>()
        + size_of::<Option<Result<Option<JobView<'_>>>>>()
        + size_of::<Result<Option<Option<JobView<'_>>>>>()
        + returned::<StatusHome, Error>()
        + returned::<Computed<'_>, Error>()
        + 6 * size_of::<Result<()>>(); // image/queue/ledger/core result boundaries
    let precondition = size_of::<(StatusSlotRef, PageHeader, StatusHomePage<'_>)>()
        + size_of::<(JobView<'_>, TransactionStatus, bool)>()
        + status_codec().max(transaction_validate()).max(codecs.queue);
    let queue_call = size_of::<QueueChange<'_>>() + queue(codecs);
    let job_page = size_of::<[&[u8]; 15]>()
        + size_of::<(PageView<'_>, [usize; 4], &[u8; 1024])>()
        + size_of::<Enumerate<IterMut<'_, &[u8]>>>()
        + codecs.queue;
    let reserve_check = size_of::<(
        JobView<'_>,
        PageIdentity,
        AllocationPage<'_>,
        [PageRef; 2],
        usize,
    )>() + returned::<AllocationEntry, AllocationError>()
        + size_of::<[(PageRef, u64, PageKind); 2]>()
        + page_codec();
    let image_call = size_of::<CatalogChange<'_>>()
        + returned::<CatalogChangeObservation, Error>()
        + catalog_image()
            .max(allocation_image())
            .max(undo_image())
            .max(initialize_pair())
            .max(status_image());
    // The inventory helper returns only InventoryClaim; its decoded prior and
    // item array cannot overlap the subsequent next_transaction call. The
    // parent actor/inventory and both helper returns remain charged here.
    let transaction_call = size_of::<TxnRef>()
        + size_of::<InventoryClaim>()
        + 2 * size_of::<Result<()>>()
        + transaction_inventory().max(transaction_next());
    common
        + precondition
            .max(queue_call)
            .max(job_page)
            .max(reserve_check)
            .max(image_call)
            .max(transaction_call)
            .max((codecs.aging_next)(codecs))
            .max(core())
}

fn effects() -> usize {
    size_of::<(Operation, JobView<'_>, EffectIndices, &mut [u8; 384], &mut [u8; 192])>()
        + size_of::<(bool, Shape, UndoRef, TxnRef, [u8; 56], [u8; 56])>()
        + size_of::<[(u16, u8, &[u8; 56]); 3]>()
        + size_of::<((u16, u8, &[u8; 56]), (u16, u8))>() // sort/iteration element and key
        + size_of::<std::iter::Zip<ChunksExactMut<'_, u8>, std::array::IntoIter<(u16, u8, &[u8; 56]), 3>>>()
        + size_of::<(&mut [u8], usize, Result<()>)>()
}

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
        + page_codec() // Init validation includes the complete nested page view
}

fn layout() -> usize {
    size_of::<(&[u8], FrameExpectation, Profile)>()
        + returned::<LayoutView<'_>, Error>()
        + size_of::<LayoutView<'_>>() // validation helper's by-value view
        + returned::<FrameEnvelope<'_>, WalError>()
        + size_of::<(Operation, Shape, &[u8], [usize; 5])>()
        + returned::<CoreView<'_>, Error>()
        + size_of::<(ReservationOwner, TxnRef, Namespace, Option<(u16, u16, u8)>)>()
        // Native hash/length/flag validation's actual two-element work packet.
        + size_of::<[(bool, usize, usize, usize, usize, &[u8]); 2]>()
        + size_of::<([u8; 24], [usize; 4], Option<(u64, u64)>)>()
        + returned::<RedoRecordView<'_>, WalError>()
        + 2 * size_of::<ContentHasher>()
        + 4 * size_of::<Result<()>>()
        + wal_codec().max(core())
}

fn native() -> usize {
    size_of::<(PageChange<'_>, Operation, usize)>()
        + size_of::<PageChange<'_>>() // views(self) by-value argument inside native
        + returned::<Native<'_>, Error>()
        + returned::<(Option<PageView<'_>>, PageView<'_>), Error>()
        + size_of::<Option<CodecResult<PageView<'_>, PageError>>>()
        + size_of::<CodecResult<Option<PageView<'_>>, PageError>>()
        + size_of::<
            [(
                u8,
                usize,
                usize,
                usize,
                usize,
                Option<&PageView<'_>>,
                Option<u16>,
            ); 2],
        >()
        + size_of::<(Range<usize>, &[u8], u16)>()
        + 2 * size_of::<ContentHasher>()
        + page_codec()
}

fn frame() -> usize {
    let common = size_of::<(&FrameSpec<'_>, &[u8], &mut [u8])>()
        + size_of::<[u8; 256]>()
        + size_of::<[u8; 1200]>()
        + size_of::<([usize; 6], FrameExpectation)>()
        + size_of::<(LayoutView<'_>, &[u8], &[u8], PageChange<'_>, usize)>()
        + returned::<Native<'_>, Error>()
        + returned::<RedoRecordView<'_>, WalError>()
        + size_of::<[&[u8]; 3]>()
        + size_of::<Enumerate<std::iter::Copied<Iter<'_, PageChange<'_>>>>>()
        + 4 * size_of::<Result<()>>();
    let group = size_of::<(Operation, Shape, usize, [usize; 3], [u8; 32])>()
        + size_of::<[(usize, u32); 6]>()
        + size_of::<[(usize, u16); 5]>()
        + 2 * size_of::<ContentHasher>()
        + core();
    let auxiliary = size_of::<(Shape, usize, EffectIndices, Option<u16>, &FrameSpec<'_>)>()
        + size_of::<(&mut [u8], &mut [u8])>()
        + effects().max(inventory());
    common
        + group
            .max(auxiliary)
            .max(native())
            .max(wal_codec())
            .max(layout())
}

fn page_change() -> usize {
    size_of::<(
        &Inputs<'_>,
        Observation,
        Option<JobView<'_>>,
        usize,
        &[u8; 16384],
    )>() + size_of::<(
        Profile,
        u64,
        PageHeader,
        Option<&[u8; 16384]>,
        Option<u16>,
        Option<u16>,
    )>() + size_of::<(
        PageHeader,
        ObjectId,
        StatusSlotRef,
        UndoRef,
        [PageRef; 2],
        ObjectHandle,
    )>() + returned::<PageChange<'_>, Error>()
        + returned::<Option<u16>, Error>()
        + size_of::<(TreePageView<'_>, SearchPosition, [u8; 8])>()
        + tree_codec()
}

/// Diagnostic subcounts are source values only, never admission grants.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceCensus {
    pub(crate) compute: usize,
    pub(crate) frame: usize,
    pub(crate) catalog_image: usize,
    pub(crate) descriptor: usize,
    pub(crate) run_retained: usize,
    pub(crate) schema: usize,
    pub(crate) total: usize,
}

pub(crate) fn source_census() -> SourceCensus {
    census(false, Codecs::catalog2())
}

/// Actual replay entry borrows its original descriptor/core lane. It retains
/// the loan, checked constructor and return carriers; it owns no local arrays.
pub(crate) fn borrowed_source_bytes() -> usize {
    census(true, Codecs::catalog2()).total
}

/// The same census for the explicit Profile3 adapter (Root3/Job3 through the
/// Catalog-scope Job3 view and the Root3-bound Aging successor). Uncalled
/// until the single current-format switch charges it instead of Profile2's.
pub(crate) fn profile3_source_census() -> SourceCensus {
    census(false, Codecs::row3())
}
pub(crate) fn profile3_borrowed_source_bytes() -> usize {
    census(true, Codecs::row3()).total
}

fn census(borrowed: bool, codecs: Codecs) -> SourceCensus {
    let compute = compute(codecs);
    let frame = frame();
    let run_retained = size_of::<(
        &Inputs<'_>,
        &mut [[u8; 16384]; 8],
        Output<'_>,
        &mut [[u8; 1536]; 2],
        &mut [u8; 256],
    )>() + returned::<Computed<'_>, Error>()
        + returned::<Observation, Error>();
    let backing = if borrowed {
        size_of::<super::transition::ReplayScratch<'_>>()
            + returned::<super::transition::ReplayScratch<'_>, Error>()
            + size_of::<(&mut [u8; super::REPLAY_SCRATCH_BYTES], &mut [u8], &mut [u8])>()
            + size_of::<std::slice::IterMut<'_, [u8; 1536]>>()
            + 4 * size_of::<&[u8]>()
            + 2 * size_of::<Result<()>>()
    } else {
        size_of::<[[u8; 1536]; 2]>()
            + size_of::<[u8; 256]>()
            + size_of::<(&Inputs<'_>, &mut [[u8; 16384]; 8], Output<'_>)>()
            + size_of::<Result<Observation>>()
    };
    let run_retained = backing + run_retained;
    // These actual arrays are first declared after compute has returned.
    // They cannot overlap compute's queue/image/ledger call frames, but both
    // stay live through the final frame encode/verify call.
    let ordered = size_of::<[(usize, PageChange<'_>); 8]>()
        + size_of::<[PageChange<'_>; 8]>()
        + size_of::<(Option<JobView<'_>>, PageChange<'_>, [usize; 3], Operation)>()
        + size_of::<[Option<&[u8; 1536]>; 2]>()
        + size_of::<FrameSpec<'_>>()
        + size_of::<([u8; 24], (u64, u64), (usize, PageChange<'_>))>()
        + size_of::<Enumerate<Iter<'_, [u8; 16384]>>>()
        + size_of::<Enumerate<std::iter::Copied<Iter<'_, (usize, PageChange<'_>)>>>>()
        + size_of::<(&mut [u8], Result<()>)>();
    let entry = size_of::<(&Inputs<'_>, &mut [[u8; 16384]; 8], &[u8], Output<'_>)>()
        + size_of::<Result<Observation>>();
    SourceCensus {
        compute,
        frame,
        catalog_image: catalog_image(),
        descriptor: descriptor_codec(),
        run_retained,
        schema: schema_source_bytes(),
        total: entry
            + run_retained
            + compute.max(ordered + frame.max(page_change()).max(codecs.queue)),
    }
}

/// Internal closure of produce/observe. The owner adds its retained Inputs,
/// runtime and registry state and verifies the unchanged component cap.
pub(crate) fn source_bytes() -> usize {
    source_census().total
}

/// Current-state descriptor validation used after replay's queue census. This
/// excludes the unrelated image/frame producer phases and the caller's retained
/// Catalog adapter, while retaining actual descriptor/remaining return copies.
pub(crate) fn transaction_source_bytes() -> usize {
    transaction_validate()
}

/// The same current-state validation for the explicit Profile3 adapter
/// (`validate_transaction::<catalog3::JobView>`), as a base4 Catalog-scope
/// actor's residual check calls it. Its terms are the Profile2 ones with the
/// Catalog-scope Job3 view in place of the Job2 view.
pub(crate) fn profile3_transaction_source_bytes() -> usize {
    transaction_validate_with::<catalog3::JobView<'static>>()
}

/// Separate LayoutView::forward_table phase. Replay retains the returned schema
/// in its own Inputs and uses max(extraction, observe), with its parent carriers
/// included in both phases. This does not grant semantic or replay authority.
pub(crate) fn schema_source_bytes() -> usize {
    let extraction = size_of::<(
        LayoutView<'_>,
        CatalogExpectation,
        Option<&[u8]>,
        NativeRecords<'_>,
        NativeView<'_>,
    )>() + returned::<CatalogRecordView<'_>, CatalogError>()
        + returned::<CatalogDefinitionView<'_>, CatalogError>()
        + returned::<TableDefinitionView<'_>, Error>()
        + size_of::<(&[u8], usize, Result<()>)>()
        + catalog_codec();
    layout().max(extraction)
}

/// Codec-only bounds for original Catalog reads. No LayoutView, native WAL
/// layout, image producer or transition validator is entered by these calls.
/// The Undo bound keeps malformed CatalogValue before-image schema parsing.
pub(crate) fn catalog_read_codec_bytes() -> usize {
    catalog_codec()
}
pub(crate) fn status_read_codec_bytes() -> usize {
    status_codec()
}
pub(crate) fn undo_read_codec_bytes() -> usize {
    undo_codec()
}
pub(crate) fn page_read_codec_bytes() -> usize {
    page_codec()
}

/// Layout parsing without the unrelated semantic transition/image producer.
pub(crate) fn layout_source_bytes() -> usize {
    layout()
}

/// Test-only branch observations for selecting the next actual admission work.
/// They are not reservations and do not replace the enclosing source census.
#[cfg(test)]
pub(crate) fn source_branches() -> [(&'static str, usize); 12] {
    [
        ("catalog_image", catalog_image()),
        ("undo_image", undo_image()),
        ("allocation_image", allocation_image()),
        ("transaction_inventory", transaction_inventory()),
        ("transaction_encode", transaction_next_encode()),
        ("transaction_next", transaction_next()),
        ("transaction_validate", transaction_validate()),
        ("aging_budget", aging_budget()),
        ("aging_predecessor", aging_predecessor(Codecs::catalog2())),
        ("aging_next", aging_next(Codecs::catalog2())),
        ("compute", compute(Codecs::catalog2())),
        ("frame", frame()),
    ]
}
