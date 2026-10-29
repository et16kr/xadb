//! Closed fresh plan4 preparation and readback. No live queue, transaction,
//! replay or mutable-page authority is constructed by this module.

use super::{
    BootstrapError, BootstrapPlan, BootstrapPlanKind, FreshStorageSession,
    InitialRecoveryExpectation, InstallNames, Phase, doublewrite_install,
};
use crate::{
    api::Namespace,
    storage::{
        allocation_page::{self, AllocationEntry, AllocationError, EXTENT_PAGES},
        anchor::{self, AnchorRoot, TreeKind},
        control::{self, ControlError, Profile, SelectedControl},
        page::{PAGE_BYTES, PageError, PageHeader, PageIdentity, PageKind, PageLsn, PageRef},
        page_io::{PageFileReader, PageIoError},
        retention_genesis::{self, RetentionGenesisError, RetentionGenesisPage},
        tree_page::{self, TreeError, TreeExpectation},
        wal_io::WalIoError,
    },
};
use foundation::{
    codec::{ContentHasher, content_digest},
    error::Effect,
    identity::{Generation, ObjectId},
    io::{read_exact_at, sync, write_all_at},
    namespace::{
        DirectoryEntry, FileMetadata, FileName, NamespaceError, NamespaceErrorKind,
        NamespaceResult, OpenMode, Selector, WriterNamespace,
    },
};
use std::mem::size_of;

pub(super) const IMAGE_BYTES: usize = 9 * PAGE_BYTES;
pub(super) const LOAN_BYTES: usize = IMAGE_BYTES + 3 * PAGE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan4GenesisPhase {
    Preflight,
    Genesis,
    Prepare,
    Install(Phase),
    Readback,
}

#[derive(Debug)]
pub(crate) enum Plan4GenesisCause {
    Bootstrap(BootstrapError),
    Wal(WalIoError),
    Namespace(NamespaceError),
    Io(foundation::Error),
    PageIo(PageIoError),
    Control(ControlError),
    Allocation(AllocationError),
    Retention(RetentionGenesisError),
    Tree(TreeError),
    Page(PageError),
    Doublewrite(super::DoublewriteInstallCause),
    InvalidInput,
    Capacity,
    IdentityMismatch,
    LengthMismatch,
    UnsupportedPlan,
    Poisoned,
    Fenced,
}

#[derive(Debug)]
pub(crate) struct Plan4GenesisError {
    pub(crate) phase: Plan4GenesisPhase,
    pub(crate) effect: Effect,
    pub(crate) cause: Plan4GenesisCause,
}
type PureResult<T> = std::result::Result<T, Plan4GenesisCause>;
type EffectResult<T> = std::result::Result<T, Plan4GenesisError>;

/// Private canonical seed. Neither this value nor its inspection grants a
/// session, selected-root, page-mutation or durable-recovery capability.
pub(in crate::storage) struct Plan4GenesisSeed {
    plan: BootstrapPlan,
}

pub(super) struct PreparedPlan4<'i> {
    seed: Plan4GenesisSeed,
    images: &'i [[u8; PAGE_BYTES]; 9],
    // Exact controls consumed by install. The mutable scratch loan itself ends
    // at prepare's return, so install must rebind these before its first create.
    control_digests: [[u8; 32]; 2],
}
impl PreparedPlan4<'_> {
    pub(super) fn into_plan(self) -> BootstrapPlan {
        self.seed.plan
    }
}

fn catalog_header(namespace: Namespace, page_no: u64) -> PageHeader {
    PageHeader {
        identity: PageIdentity {
            database: namespace.database,
            page: PageRef {
                file: super::fixed_entry().expect("fixed file geometry").file,
                page_no,
                incarnation: Generation::new(1).expect("fixed incarnation"),
            },
            owner: ObjectId::new(5).expect("fixed Catalog object"),
            owner_generation: Generation::new(1).expect("fixed Catalog generation"),
        },
        kind: if page_no == 7 {
            PageKind::TreeAnchor
        } else {
            PageKind::CatalogLeaf
        },
        level: 0,
        lsn: PageLsn(0),
    }
}

fn canonical_payload(
    namespace: Namespace,
    index: usize,
    output: &mut [u8; PAGE_BYTES],
) -> PureResult<()> {
    match index {
        1 => retention_genesis::encode_row_current(namespace, RetentionGenesisPage::Root, output)
            .map_err(Plan4GenesisCause::Retention),
        2..=6 => retention_genesis::encode_row_current(
            namespace,
            RetentionGenesisPage::Jobs {
                index: (index - 2) as u8,
            },
            output,
        )
        .map_err(Plan4GenesisCause::Retention),
        7 => anchor::encode_anchor(
            catalog_header(namespace, 7),
            AnchorRoot {
                tree: TreeKind::Catalog,
                page: catalog_header(namespace, 8).identity.page,
                level: 0,
                revision: 1,
            },
            output,
        )
        .map_err(Plan4GenesisCause::Page),
        8 => {
            let mut metadata = [0; tree_page::TREE_METADATA_BYTES];
            let cell = tree_page::encode_metadata(PageKind::CatalogLeaf, 0, None, &mut metadata)
                .map_err(Plan4GenesisCause::Tree)?;
            tree_page::encode_tree_page(catalog_header(namespace, 8), 0, &[cell], output)
                .map_err(Plan4GenesisCause::Tree)
        }
        _ => Err(Plan4GenesisCause::InvalidInput),
    }
}

/// Original plan4 Catalog bytes for the borrowed XRB1 observation. This does
/// not select an installed page or prove its original INIT/source lineage.
pub(in crate::storage) fn encode_plan4_catalog_genesis(
    namespace: Namespace,
    ordinal: usize,
    output: &mut [u8; PAGE_BYTES],
) -> PureResult<()> {
    if namespace.timeline != 1 || !matches!(ordinal, 7 | 8) {
        return Err(Plan4GenesisCause::InvalidInput);
    }
    canonical_payload(namespace, ordinal, output)
}

/// New bridge closure only; output page is a caller loan. Both old canonical
/// producer call paths remain unchanged. Empty Catalog has only its metadata
/// cell, so validate_cells never enters an entry-key decoder.
pub(in crate::storage) fn plan4_catalog_genesis_source_bytes() -> usize {
    8 * size_of::<PageHeader>()
        + 4 * size_of::<PageIdentity>()
        + 6 * size_of::<PageRef>()
        + 4 * size_of::<AnchorRoot>()
        + 3 * size_of::<Namespace>()
        + 4 * size_of::<PureResult<()>>()
        + 4 * size_of::<std::result::Result<(), PageError>>()
        + 4 * size_of::<std::result::Result<(), TreeError>>()
        + 2 * size_of::<std::result::Result<Option<PageRef>, TreeError>>()
        + 4 * size_of::<ContentHasher>()
        + 1536 // pinned SHA nested block/output/compression storage
        + 48 // anchor cell
        + tree_page::TREE_METADATA_BYTES
        + 2 * 24 // PageRef codec scratch
        + 32 * size_of::<usize>()
        + 24 * size_of::<&[u8]>()
        + 1024 // narrow tree layout/iterator/error and scalar return carriers
}

pub(super) fn prepare<'i>(
    profile: Profile,
    images: &'i mut [[u8; PAGE_BYTES]; 9],
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> PureResult<PreparedPlan4<'i>> {
    if profile.format() != 3 {
        return Err(Plan4GenesisCause::UnsupportedPlan);
    }
    let plan = BootstrapPlan::prepare_for_kind(
        profile,
        scratch,
        BootstrapPlanKind::RetentionCatalogBootstrap,
    )
    .map_err(Plan4GenesisCause::Bootstrap)?;
    images[0].copy_from_slice(&scratch[0]);
    for (index, image) in images.iter_mut().enumerate().skip(1) {
        canonical_payload(profile.spec().namespace, index, image)?;
    }
    let control_digests = [
        content_digest(&scratch[1][..super::ROOT_BYTES + super::CHUNK_BYTES]),
        content_digest(&scratch[2][..2 * super::super::CONTROL_BYTES]),
    ];
    Ok(PreparedPlan4 {
        seed: Plan4GenesisSeed { plan },
        images,
        control_digests,
    })
}

fn install_error(error: BootstrapError) -> Plan4GenesisError {
    Plan4GenesisError {
        phase: Plan4GenesisPhase::Install(error.phase),
        effect: Effect::CallerMustResolve,
        cause: Plan4GenesisCause::Bootstrap(error),
    }
}
fn data_error(phase: Phase, cause: Plan4GenesisCause) -> Plan4GenesisError {
    Plan4GenesisError {
        phase: Plan4GenesisPhase::Install(phase),
        effect: Effect::CallerMustResolve,
        cause,
    }
}

pub(super) fn install_prepared<W: WriterNamespace>(
    writer: &mut W,
    prepared: &PreparedPlan4<'_>,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> EffectResult<()> {
    let [page, controls, final_controls] = scratch;
    if content_digest(&controls[..super::ROOT_BYTES + super::CHUNK_BYTES])
        != prepared.control_digests[0]
        || content_digest(&final_controls[..2 * super::super::CONTROL_BYTES])
            != prepared.control_digests[1]
    {
        return Err(Plan4GenesisError {
            phase: Plan4GenesisPhase::Prepare,
            effect: Effect::CallerMustResolve,
            cause: Plan4GenesisCause::IdentityMismatch,
        });
    }
    let names = InstallNames::new(&prepared.seed.plan).map_err(install_error)?;
    super::install_descriptor(writer, &names, final_controls).map_err(install_error)?;
    {
        let mut file = writer
            .create_new(&names.data)
            .map_err(|error| data_error(Phase::DataCreate, Plan4GenesisCause::Namespace(error)))?;
        for (index, image) in prepared.images.iter().enumerate() {
            write_all_at(&mut file, image, (index * PAGE_BYTES) as u64)
                .map_err(|error| data_error(Phase::DataWrite, Plan4GenesisCause::Io(error)))?;
        }
        page.fill(0);
        for index in 9..EXTENT_PAGES {
            write_all_at(&mut file, page, (index * PAGE_BYTES) as u64)
                .map_err(|error| data_error(Phase::DataWrite, Plan4GenesisCause::Io(error)))?;
        }
        sync(&mut file)
            .map_err(|error| data_error(Phase::DataSync, Plan4GenesisCause::Io(error)))?;
    }
    // This shared helper also zeroes page before its first FILEMAP bank write.
    super::install_filemap(writer, &names, page, controls, final_controls).map_err(install_error)
}

pub(in crate::storage) fn inspect_bootstrap(
    profile: Profile,
    selected: SelectedControl,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> PureResult<Plan4GenesisSeed> {
    if profile.format() != 3 {
        return Err(Plan4GenesisCause::UnsupportedPlan);
    }
    let bytes = &scratch[0][..super::super::CONTROL_BYTES];
    super::super::check_control(bytes, profile, b"XBS1").map_err(Plan4GenesisCause::Control)?;
    if super::super::u64_at(bytes, 280) != 4 {
        return Err(Plan4GenesisCause::UnsupportedPlan);
    }
    let actual = content_digest(bytes);
    let plan = BootstrapPlan::prepare_for_kind(
        profile,
        scratch,
        BootstrapPlanKind::RetentionCatalogBootstrap,
    )
    .map_err(Plan4GenesisCause::Bootstrap)?;
    if selected != plan.selected {
        return Err(Plan4GenesisCause::IdentityMismatch);
    }
    let expected = plan
        .encode_descriptor(&mut scratch[2])
        .map_err(Plan4GenesisCause::Control)?;
    if actual != content_digest(expected) {
        return Err(Plan4GenesisCause::Control(ControlError::Corrupt));
    }
    Ok(Plan4GenesisSeed { plan })
}

fn open_bound<W: WriterNamespace>(
    writer: &mut W,
    name: &FileName,
    length: u64,
) -> PureResult<(W::File, FileMetadata)> {
    let (file, opened) = writer
        .open_existing_with_metadata(name, OpenMode::ReadOnly)
        .map_err(Plan4GenesisCause::Namespace)?;
    if opened.length != length {
        return Err(Plan4GenesisCause::LengthMismatch);
    }
    check_named(writer, name, opened)?;
    Ok((file, opened))
}
fn check_named<W: WriterNamespace>(
    writer: &W,
    name: &FileName,
    opened: FileMetadata,
) -> PureResult<()> {
    if writer
        .metadata(name)
        .map_err(Plan4GenesisCause::Namespace)?
        != Some(opened)
    {
        return Err(Plan4GenesisCause::IdentityMismatch);
    }
    Ok(())
}

/// Checks each open used by the FILEMAP reader against that same named object.
/// Only initial read paths are exposed; effect methods never forward.
struct CheckedPlan4Namespace<'a, W: WriterNamespace>(&'a mut W);
fn denied() -> NamespaceError {
    NamespaceError::new(
        NamespaceErrorKind::InvalidName,
        Effect::Unchanged,
        "closed plan4 read-only inspection",
    )
}
fn allowed_length(name: &FileName) -> NamespaceResult<u64> {
    match name.as_str() {
        "d3-profile" | "FILEMAP" => Ok(control::PROFILE_BYTES as u64),
        "d3-filemap-0" | "d3-filemap-1" => Ok(control::CONTAINER_BYTES),
        "d3-data-0000000100000001" => Ok(control::EXTENT_BYTES),
        _ => Err(denied()),
    }
}
impl<W: WriterNamespace> WriterNamespace for CheckedPlan4Namespace<'_, W> {
    type File = W::File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        Err(denied())
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        Err(denied())
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<Self::File> {
        self.open_existing_with_metadata(name, mode)
            .map(|(file, _)| file)
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        if mode != OpenMode::ReadOnly {
            return Err(denied());
        }
        let length = allowed_length(name)?;
        let (file, opened) = self.0.open_existing_with_metadata(name, mode)?;
        if opened.length != length {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "plan4 opened length mismatch",
            ));
        }
        let named = self.0.metadata(name)?.ok_or_else(denied)?;
        if named.identity != opened.identity {
            return Err(NamespaceError::new(
                NamespaceErrorKind::InvalidType,
                Effect::Unchanged,
                "plan4 opened identity mismatch",
            ));
        }
        if named.length != opened.length {
            return Err(NamespaceError::new(
                NamespaceErrorKind::LimitExceeded,
                Effect::Unchanged,
                "plan4 named length mismatch",
            ));
        }
        Ok((file, opened))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        allowed_length(name)?;
        self.0.metadata(name)
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        Err(denied())
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        Err(denied())
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        Err(denied())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        Err(denied())
    }
}

impl Plan4GenesisSeed {
    pub(in crate::storage) fn initial_recovery_expectation(
        &self,
        scratch: &mut [u8; PAGE_BYTES],
    ) -> PureResult<InitialRecoveryExpectation> {
        let (doublewrite, stage_digest) =
            doublewrite_install::initial_recovery_expectation(&self.plan, scratch)
                .map_err(Plan4GenesisCause::Doublewrite)?;
        Ok(InitialRecoveryExpectation {
            plan: 4,
            filemap: self.plan.selected,
            doublewrite,
            stage_digest,
        })
    }

    pub(in crate::storage) fn verify_data<W: WriterNamespace>(
        &self,
        writer: &mut W,
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> PureResult<()> {
        BorrowedPlan4 { plan: &self.plan }.verify_data(writer, scratch)
    }
}

// Borrowing the original private plan does not create a second canonical seed
// or a mutation/recovery capability. All old and new validators share this body.
struct BorrowedPlan4<'a> {
    plan: &'a BootstrapPlan,
}
impl BorrowedPlan4<'_> {
    fn verify_selected(
        &self,
        writer: &mut impl WriterNamespace,
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> PureResult<()> {
        let [root, work, output] = scratch;
        let mut checked = CheckedPlan4Namespace(writer);
        let mut reader = PageFileReader::open_profile(&mut checked, self.plan.profile, root, work)
            .map_err(Plan4GenesisCause::PageIo)?;
        if reader.profile() != self.plan.profile || reader.selected() != self.plan.selected {
            return Err(Plan4GenesisCause::IdentityMismatch);
        }
        let identity = self
            .plan
            .allocation_identity()
            .map_err(Plan4GenesisCause::Control)?;
        reader
            .read_page(identity, PageKind::Allocation, 0, output)
            .map_err(Plan4GenesisCause::PageIo)?;
        self.verify_allocator(output)
    }

    fn verify_allocator(&self, bytes: &[u8; PAGE_BYTES]) -> PureResult<()> {
        let identity = self
            .plan
            .allocation_identity()
            .map_err(Plan4GenesisCause::Control)?;
        allocation_page::decode_allocation_page(
            bytes,
            identity,
            self.plan.profile.spec().namespace,
        )
        .map_err(Plan4GenesisCause::Allocation)?;
        if content_digest(bytes) != self.plan.allocation_digest {
            return Err(Plan4GenesisCause::IdentityMismatch);
        }
        Ok(())
    }

    fn verify_payload(
        &self,
        index: usize,
        actual: &[u8; PAGE_BYTES],
        expected: &mut [u8; PAGE_BYTES],
    ) -> PureResult<()> {
        let namespace = self.plan.profile.spec().namespace;
        match index {
            1 => retention_genesis::validate_row_current(
                actual,
                namespace,
                RetentionGenesisPage::Root,
            )
            .map_err(Plan4GenesisCause::Retention)?,
            2..=6 => retention_genesis::validate_row_current(
                actual,
                namespace,
                RetentionGenesisPage::Jobs {
                    index: (index - 2) as u8,
                },
            )
            .map_err(Plan4GenesisCause::Retention)?,
            7 => {
                anchor::decode_anchor(actual, catalog_header(namespace, 7).identity)
                    .map_err(Plan4GenesisCause::Page)?;
            }
            8 => {
                tree_page::decode_tree_page(
                    actual,
                    TreeExpectation {
                        identity: catalog_header(namespace, 8).identity,
                        kind: PageKind::CatalogLeaf,
                        level: 0,
                        arity: 0,
                    },
                )
                .map_err(Plan4GenesisCause::Tree)?;
            }
            _ => return Err(Plan4GenesisCause::InvalidInput),
        }
        canonical_payload(namespace, index, expected)?;
        if actual != expected {
            return Err(Plan4GenesisCause::IdentityMismatch);
        }
        Ok(())
    }

    /// Two bounded canonical passes use one opened handle. Its original metadata
    /// is compared with the named file at both ends. Writer exclusion is the
    /// synchronization premise; these reads are not an arbitrary-writer snapshot.
    pub(in crate::storage) fn verify_data<W: WriterNamespace>(
        &self,
        writer: &mut W,
        scratch: &mut [[u8; PAGE_BYTES]; 3],
    ) -> PureResult<()> {
        self.verify_selected(writer, scratch)?;
        let name = control::data_file_name(
            super::fixed_entry()
                .map_err(Plan4GenesisCause::Control)?
                .file,
        );
        let (mut file, opened) = open_bound(writer, &name, control::EXTENT_BYTES)?;
        let [_, expected, actual] = scratch;
        for _ in 0..2 {
            for index in 0..EXTENT_PAGES {
                read_exact_at(&mut file, actual, (index * PAGE_BYTES) as u64)
                    .map_err(Plan4GenesisCause::Io)?;
                match index {
                    0 => self.verify_allocator(actual)?,
                    1..=8 => self.verify_payload(index, actual, expected)?,
                    _ if actual.iter().any(|byte| *byte != 0) => {
                        return Err(Plan4GenesisCause::IdentityMismatch);
                    }
                    _ => {}
                }
            }
        }
        check_named(writer, &name, opened)
    }

    fn verify_descriptor(
        &self,
        writer: &mut impl WriterNamespace,
        output: &mut [u8; PAGE_BYTES],
    ) -> PureResult<()> {
        let name = FileName::new(super::DESCRIPTOR_NAME).map_err(Plan4GenesisCause::Namespace)?;
        let (mut file, opened) = open_bound(writer, &name, super::super::CONTROL_BYTES as u64)?;
        read_exact_at(&mut file, &mut output[..super::super::CONTROL_BYTES], 0)
            .map_err(Plan4GenesisCause::Io)?;
        self.plan
            .decode_descriptor(&output[..super::super::CONTROL_BYTES])
            .map_err(Plan4GenesisCause::Control)?;
        check_named(writer, &name, opened)
    }
}

pub(super) fn readback_prepared<W: WriterNamespace>(
    writer: &mut W,
    prepared: &PreparedPlan4<'_>,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> EffectResult<()> {
    readback_seed(
        writer,
        &BorrowedPlan4 {
            plan: &prepared.seed.plan,
        },
        scratch,
    )
    .map_err(|cause| Plan4GenesisError {
        phase: Plan4GenesisPhase::Readback,
        effect: Effect::CallerMustResolve,
        cause,
    })
}

fn readback_seed<W: WriterNamespace>(
    writer: &mut W,
    seed: &BorrowedPlan4<'_>,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> PureResult<()> {
    let stamps = readback_stamps(writer)?;
    seed.verify_descriptor(writer, &mut scratch[0])?;
    // Unselected FILEMAP geometry remains authoritative, never its content.
    let name = FileName::new("d3-filemap-1").map_err(Plan4GenesisCause::Namespace)?;
    drop(open_bound(writer, &name, control::CONTAINER_BYTES)?);
    seed.verify_data(writer, scratch)?;
    seed.verify_selected(writer, scratch)?;
    seed.verify_descriptor(writer, &mut scratch[0])?;
    if readback_stamps(writer)? != stamps {
        return Err(Plan4GenesisCause::IdentityMismatch);
    }
    Ok(())
}

fn borrowed_owned(plan: &BootstrapPlan) -> PureResult<BorrowedPlan4<'_>> {
    if plan.kind != BootstrapPlanKind::RetentionCatalogBootstrap || plan.profile.format() != 3 {
        return Err(Plan4GenesisCause::UnsupportedPlan);
    }
    Ok(BorrowedPlan4 { plan })
}

pub(super) fn owned_header(plan: &BootstrapPlan, page_no: usize) -> PureResult<PageHeader> {
    let borrowed = borrowed_owned(plan)?;
    let namespace = borrowed.plan.profile.spec().namespace;
    match page_no {
        1 => retention_genesis::expected_header(namespace, RetentionGenesisPage::Root)
            .map_err(Plan4GenesisCause::Retention),
        2..=6 => retention_genesis::expected_header(
            namespace,
            RetentionGenesisPage::Jobs {
                index: (page_no - 2) as u8,
            },
        )
        .map_err(Plan4GenesisCause::Retention),
        7 | 8 => Ok(catalog_header(namespace, page_no as u64)),
        _ => Err(Plan4GenesisCause::InvalidInput),
    }
}

pub(super) fn verify_owned_allocator(
    plan: &BootstrapPlan,
    bytes: &[u8; PAGE_BYTES],
) -> PureResult<()> {
    borrowed_owned(plan)?.verify_allocator(bytes)
}

pub(super) fn verify_owned_payload(
    plan: &BootstrapPlan,
    page_no: usize,
    actual: &[u8; PAGE_BYTES],
    expected: &mut [u8; PAGE_BYTES],
) -> PureResult<()> {
    borrowed_owned(plan)?.verify_payload(page_no, actual, expected)
}

pub(super) fn verify_owned_genesis<W: WriterNamespace>(
    plan: &BootstrapPlan,
    writer: &mut W,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
    allocator: &mut [u8; PAGE_BYTES],
) -> PureResult<()> {
    readback_seed(writer, &borrowed_owned(plan)?, scratch)?;
    // verify_selected's last output survives the descriptor read in scratch[0].
    allocator.copy_from_slice(&scratch[2]);
    Ok(())
}

pub(super) fn read_owned_payload<W: WriterNamespace>(
    plan: &BootstrapPlan,
    writer: &mut W,
    request: crate::storage::dirty_frame::TargetRequest,
    scratch: &mut [[u8; PAGE_BYTES]; 2],
    output: &mut [u8; PAGE_BYTES],
) -> PureResult<()> {
    let seed = borrowed_owned(plan)?;
    let index = usize::try_from(request.identity.page.page_no)
        .map_err(|_| Plan4GenesisCause::InvalidInput)?;
    let header = owned_header(plan, index)?;
    if request.identity != header.identity
        || request.kind != header.kind
        || request.level != header.level
    {
        return Err(Plan4GenesisCause::IdentityMismatch);
    }
    let stamps = readback_stamps(writer)?;
    seed.verify_descriptor(writer, &mut scratch[0])?;
    {
        let [root, work] = scratch;
        let mut checked = CheckedPlan4Namespace(writer);
        let mut reader = PageFileReader::open_profile(&mut checked, plan.profile, root, work)
            .map_err(Plan4GenesisCause::PageIo)?;
        if reader.profile() != plan.profile || reader.selected() != plan.selected {
            return Err(Plan4GenesisCause::IdentityMismatch);
        }
        reader
            .read_page(request.identity, request.kind, request.level, output)
            .map_err(Plan4GenesisCause::PageIo)?;
    }
    seed.verify_descriptor(writer, &mut scratch[0])?;
    // Re-read the authoritative root after the home read. The caller performs
    // two complete helper calls and compares their canonical home images.
    {
        let [root, work] = scratch;
        let mut checked = CheckedPlan4Namespace(writer);
        let reader = PageFileReader::open_profile(&mut checked, plan.profile, root, work)
            .map_err(Plan4GenesisCause::PageIo)?;
        if reader.profile() != plan.profile || reader.selected() != plan.selected {
            return Err(Plan4GenesisCause::IdentityMismatch);
        }
    }
    if readback_stamps(writer)? != stamps {
        return Err(Plan4GenesisCause::IdentityMismatch);
    }
    Ok(())
}

/// G5 borrowed-plan helpers add typed argument/return values to the shared
/// canonical readback bound. Some headers overlap that bound conservatively.
pub(super) fn owned_source_bytes<W: WriterNamespace>() -> usize {
    source_census::<W>().readback_nested
        + 2 * size_of::<BorrowedPlan4<'_>>()
        + 2 * size_of::<PureResult<BorrowedPlan4<'_>>>()
        + 2 * size_of::<crate::storage::dirty_frame::TargetRequest>()
        + 2 * size_of::<PageHeader>()
        + 2 * size_of::<PureResult<PageHeader>>()
        + 16 * size_of::<&()>()
        + 64
}

fn readback_stamps(writer: &impl WriterNamespace) -> PureResult<[FileMetadata; 6]> {
    let observe = |name| {
        writer
            .metadata(&FileName::new(name).map_err(Plan4GenesisCause::Namespace)?)
            .map_err(Plan4GenesisCause::Namespace)?
            .ok_or(Plan4GenesisCause::IdentityMismatch)
    };
    Ok([
        observe("d3-profile")?,
        observe(super::DESCRIPTOR_NAME)?,
        observe("FILEMAP")?,
        observe("d3-filemap-0")?,
        observe("d3-filemap-1")?,
        observe("d3-data-0000000100000001")?,
    ])
}

/// Simultaneous source-value closure, not a claim about machine stack layout.
/// The nine images and three scratch pages are borrowed once from maintenance.
pub(in crate::storage) struct Census {
    pub(in crate::storage) prepare_nested: usize,
    pub(in crate::storage) install_nested: usize,
    pub(in crate::storage) readback_nested: usize,
    pub(in crate::storage) return_nested: usize,
    pub(in crate::storage) prepare: usize,
    pub(in crate::storage) install: usize,
    pub(in crate::storage) readback: usize,
    pub(in crate::storage) returned: usize,
    pub(in crate::storage) peak: usize,
}

pub(in crate::storage) fn source_census<W: WriterNamespace>() -> Census {
    let old = super::source_budget::<W>();
    let hash = 4 * size_of::<ContentHasher>() + 1536;
    let child_carriers = 2 * size_of::<Plan4GenesisSeed>()
        + 2 * size_of::<PreparedPlan4<'_>>()
        + 2 * size_of::<PureResult<PreparedPlan4<'_>>>()
        + 2 * size_of::<PureResult<Plan4GenesisSeed>>()
        + 4 * size_of::<EffectResult<()>>()
        + 4 * size_of::<PureResult<()>>()
        + 4 * size_of::<Plan4GenesisCause>()
        // Borrowed args/iterators/scalars include the shared readback forwarding:
        // 2 BorrowedPlan4 values + 6 references = 64 bytes on this target. No
        // Result<BorrowedPlan4> occurs on the unchanged old constructor path.
        + 1024;
    let payload = retention_genesis::row_current_source_bytes()
        + 6 * size_of::<PageHeader>()
        + 4 * size_of::<PageIdentity>()
        + 6 * size_of::<PageRef>()
        + 4 * size_of::<AnchorRoot>()
        + 4 * size_of::<anchor::TreeAnchor>()
        + 4 * size_of::<TreeExpectation>()
        + 4 * size_of::<tree_page::TreePageView<'_>>()
        + 4 * size_of::<std::result::Result<anchor::TreeAnchor, PageError>>()
        + 4 * size_of::<std::result::Result<tree_page::TreePageView<'_>, TreeError>>()
        + 80
        + 1024;
    // The 64-entry allocator array lives only in parent prepare_for_kind;
    // payload encoding and DWB expectation follow that call's return.
    let prepare_nested = child_carriers
        + (old.prepare_nested + size_of::<[AllocationEntry; EXTENT_PAGES]>() + 1032)
            .max(payload)
            .max(
                doublewrite_install::recovery_preparation_bytes()
                    + 2 * size_of::<InitialRecoveryExpectation>(),
            )
            .max(hash + 128);
    let install_nested = child_carriers
        + size_of::<InstallNames>()
        + 2 * size_of::<W::File>()
        + 4 * size_of::<std::result::Result<W::File, NamespaceError>>()
        + hash
        + 1024;
    let readback_nested = child_carriers
        + old.readback_nested.max(payload).max(hash)
        // Installed-grammar open over the legacy open inside old.readback_nested.
        + crate::storage::page_io::installed_open_source_bytes()
        + 2 * size_of::<W::File>()
        + 2 * size_of::<PureResult<(W::File, FileMetadata)>>()
        + 6 * size_of::<FileMetadata>()
        + 4 * size_of::<FileName>()
        + 2 * size_of::<CheckedPlan4Namespace<'_, W>>()
        + 4 * size_of::<NamespaceError>()
        + 4 * size_of::<NamespaceResult<(W::File, FileMetadata)>>()
        + 4 * size_of::<FileMetadata>()
        + 4 * size_of::<FileName>()
        + 2 * size_of::<[FileMetadata; 6]>()
        + 2 * size_of::<PureResult<[FileMetadata; 6]>>()
        + 1024;
    let return_nested = child_carriers + 2 * size_of::<PureResult<InitialRecoveryExpectation>>();
    let owner = size_of::<FreshStorageSession<'_, W>>();
    let constructor =
        size_of::<std::result::Result<FreshStorageSession<'_, W>, Plan4GenesisError>>()
            + 2 * size_of::<std::result::Result<BootstrapPlan, Plan4GenesisError>>()
            + size_of::<Plan4GenesisError>()
            + 512;
    let prepare = owner + constructor + prepare_nested;
    let install = owner + constructor + install_nested;
    let readback = owner + constructor + readback_nested;
    let returned = owner + constructor + return_nested;
    Census {
        prepare_nested,
        install_nested,
        readback_nested,
        return_nested,
        prepare,
        install,
        readback,
        returned,
        peak: prepare.max(install).max(readback).max(returned),
    }
}

pub(in crate::storage) fn fits<W: WriterNamespace>() -> bool {
    super::source_budget_fits::<W>() && source_census::<W>().peak <= 65536
}

#[cfg(test)]
#[path = "plan4_genesis/tests.rs"]
pub(in crate::storage) mod tests;
