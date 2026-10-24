//! Status-role subset only. Seeded owners below are NOT installation evidence.
//! The separate native worker executes the actual successful full creator.
use super::*;
use crate::storage::wal_io::bootstrap::{self as boot, test_support as port};
use std::{
    collections::BTreeMap,
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};
use xadb_foundation::codec::content_digest;

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-bootstrap"
);
const OLD_INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-storage-session/literal"
);
type State = Arc<Mutex<port::State>>;

#[path = "status_bootstrap_native_tests.rs"]
mod native;

#[path = "undo_bootstrap_tests.rs"]
mod undo;

fn mark(id: &str) {
    println!("\nXADB_STATUS_BOOTSTRAP_CASE {id}");
}
fn literal(profile: u32, status: bool) -> Vec<u8> {
    std::fs::read(format!(
        "{}/profile{profile}-d3-bootstrap.bin",
        if status { INPUT } else { OLD_INPUT }
    ))
    .unwrap()
}
fn boxed<const N: usize>(byte: u8) -> Box<[u8; N]> {
    vec![byte; N].into_boxed_slice().try_into().unwrap()
}
fn prepared(
    profile: u32,
    status: bool,
    scratch: &mut [[u8; PAGE_BYTES]; 3],
) -> boot::BootstrapPlan {
    let raw = std::fs::read(format!("{OLD_INPUT}/profile{profile}-d3-profile.bin")).unwrap();
    let profile =
        crate::storage::control::decode_bootstrap_profile(&raw, Some(port::namespace())).unwrap();
    boot::BootstrapPlan::prepare_for_kind(
        profile,
        scratch,
        if status {
            boot::BootstrapPlanKind::StatusBootstrap
        } else {
            boot::BootstrapPlanKind::AllocatorOnly
        },
    )
    .unwrap()
}
fn reseal(raw: &mut [u8]) {
    let digest = content_digest(&raw[..4064]);
    raw[4064..].copy_from_slice(&digest);
}
fn changed(mut raw: Vec<u8>, offset: usize, value: u64) -> Vec<u8> {
    raw[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    reseal(&mut raw);
    raw
}

// Private test seed: genesis ownership + prepared plan, without bootstrap I/O.
// This isolates role/adapter guards; only native P1 proves successful creation.
fn seed<'m>(
    slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    status: bool,
) -> (boot::FreshStorageSession<'m, port::Writer>, State) {
    let (writer, state) = port::fresh();
    let mut scratch = [[0; PAGE_BYTES]; 3];
    let wal = wal::GenesisWalSession::create(writer, 1, slot, &mut scratch[0]).unwrap();
    let plan = prepared(1, status, &mut scratch);
    let fresh = boot::FreshStorageSession {
        core: boot::InstalledStorageCore {
            wal: wal.runtime,
            plan,
            reader_active: std::sync::atomic::AtomicBool::new(false),
        },
    };
    reset(&state);
    (fresh, state)
}
fn installed_seed<'m, 'w>(
    slot: &'m mut [u8; wal::MAX_FRAME_BYTES],
    workspace: &'w mut [u8; super::super::WORKSPACE_BYTES],
    status: bool,
) -> (DoublewriteStorageSession<'m, 'w, port::Writer>, State) {
    let (fresh, state) = seed(slot, status);
    let plan = super::super::InstallPlan::prepare(&fresh.core.plan, workspace).unwrap();
    (
        DoublewriteStorageSession {
            fresh,
            _workspace: workspace,
            _plan: plan,
        },
        state,
    )
}
fn reset(state: &State) {
    let mut state = state.lock().unwrap();
    state.events.clear();
    state.read_events.clear();
    state.read_effect_counts.clear();
    state.reads = 0;
    state.open_modes.clear();
    state.hit = 0;
}
fn images(state: &State) -> BTreeMap<String, Vec<u8>> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(name, entry)| {
            assert_eq!(entry.length, entry.bytes.len() as u64);
            assert!(entry.sparse_tail.is_empty());
            (name.clone(), entry.bytes.clone())
        })
        .collect()
}
fn no_io(state: &State) {
    let state = state.lock().unwrap();
    assert!(state.events.is_empty());
    assert!(state.read_events.is_empty());
}
fn assert_partial(case: usize, state: &port::State) {
    let expected = std::fs::read_to_string(format!("{INPUT}/partial-files.tsv")).unwrap();
    let id = format!("SB04-{case:02}");
    let mut count = 0;
    for line in expected.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] != id {
            continue;
        }
        let entry = &state.files[fields[1]];
        assert_eq!(entry.length, fields[2].parse::<u64>().unwrap());
        assert_eq!(entry.bytes.len() as u64, entry.length);
        let sha: String = content_digest(&entry.bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(sha, fields[3], "{id} {}", fields[1]);
        count += 1;
    }
    assert_eq!(count, state.files.len());
    let events = std::fs::read_to_string(format!("{INPUT}/effects.tsv")).unwrap();
    for (actual, expected) in state.events.iter().zip(events.lines()) {
        let fields: Vec<_> = expected.split('\t').collect();
        assert_eq!(
            actual.kind,
            if fields[0] == "directory_sync" {
                "dirsync"
            } else {
                fields[0]
            }
        );
        assert_eq!(actual.name, if fields[1] == "-" { "" } else { fields[1] });
        assert_eq!(actual.offset, fields[2].parse::<u64>().unwrap());
        assert_eq!(actual.length, fields[3].parse::<usize>().unwrap());
        let sha: String = content_digest(&actual.data)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(sha, fields[4]);
    }
}
fn poison<T>(mutex: &Mutex<T>) {
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            let _guard = mutex.lock().unwrap();
            panic!("intentional status-bootstrap poison");
        }))
        .is_err()
    );
}
fn role_exact(role: boot::StatusRole<'_>, fence: &SessionFence) {
    assert_eq!(role.namespace(), port::namespace());
    assert_eq!(role.object().get(), 2);
    assert_eq!(role.generation().get(), 1);
    assert_eq!(role.kind(), PageKind::TransactionStatus);
    assert_eq!(role.level(), 0);
    assert_eq!(role.issued_object_high_water(), 2);
    assert!(role.uses_fence(fence));
    let other = SessionFence::new();
    assert!(!role.uses_fence(&other));
}

#[test]
fn status_bootstrap_sb01_literals_and_layout() {
    for profile in 1..=3 {
        let mut scratch = [[0xa5; PAGE_BYTES]; 3];
        let plan = prepared(profile, true, &mut scratch);
        let expected = literal(profile, true);
        assert_eq!(&scratch[2][..4096], expected);
        let mut encoded = [0xa5; 4097];
        assert_eq!(plan.encode_descriptor(&mut encoded).unwrap(), expected);
        assert_eq!(encoded[4096], 0xa5);
        assert_eq!(plan.decode_descriptor(&expected), Ok(()));
        let old = literal(profile, false);
        for at in 0..4064 {
            if !(120..128).contains(&at) && !(280..288).contains(&at) {
                assert_eq!(old[at], expected[at], "unexpected field {at}");
            }
        }
        mark(&format!("SB01-{profile:02}"));
    }
    let rows = [
        ("kind", size_of::<boot::BootstrapPlanKind>(), 8),
        ("role", size_of::<boot::StatusRole<'_>>(), 32),
        ("plan", size_of::<boot::BootstrapPlan>(), 256),
        (
            "fresh",
            size_of::<boot::FreshStorageSession<'_, port::Writer>>(),
            16384,
        ),
        (
            "mutable",
            size_of::<MutableStorageOwner<'_, '_, port::Writer>>(),
            16384,
        ),
        ("bootstrap_error", size_of::<boot::BootstrapError>(), 256),
        ("mutable_error", size_of::<MutableStorageError>(), 256),
        (
            "role_result",
            size_of::<std::result::Result<boot::StatusRole<'_>, boot::BootstrapError>>(),
            256,
        ),
    ];
    for (name, actual, bound) in rows {
        println!("XADB_STATUS_BOOTSTRAP_LAYOUT {name} {actual} {bound}");
        assert!(actual <= bound, "{name}");
    }
    // Source-owned nested census, independent of the production admission
    // predicate. Non-Copy owner/File moves count once; ABI stack is not measured.
    use crate::storage::{
        allocation_page::{AllocationPage, ExtentAddress},
        control::{
            ChunkDescriptor, ChunkLocation, ControlRef, FileMapChunk, FileMapCursor, FileMapEntry,
            FileMapRoot, MapIdentity, MapSummary, ProfileSpec, SelectedControl,
        },
        page::{PageHeader, PageView},
    };
    use xadb_foundation::{
        codec::ContentHasher,
        namespace::{FileMetadata, FileName},
    };
    type RoleResult = std::result::Result<boot::StatusRole<'static>, boot::BootstrapError>;
    type MutableRoleResult = Result<boot::StatusRole<'static>>;
    let type_rows = [
        ("profile", size_of::<Profile>(), 128),
        ("profile_spec", size_of::<ProfileSpec>(), 64),
        ("namespace", size_of::<crate::api::Namespace>(), 32),
        ("map_identity", size_of::<MapIdentity>(), 64),
        ("map_summary", size_of::<MapSummary>(), 64),
        ("map_entry", size_of::<FileMapEntry>(), 64),
        ("chunk_descriptor", size_of::<ChunkDescriptor>(), 128),
        ("control_ref", size_of::<ControlRef>(), 64),
        ("selected", size_of::<SelectedControl>(), 96),
        ("page_header", size_of::<PageHeader>(), 96),
        ("page_identity", size_of::<PageIdentity>(), 64),
        ("allocation_entry", size_of::<AllocationEntry>(), 128),
        ("extent", size_of::<ExtentAddress>(), 32),
        ("page_view", size_of::<PageView<'_>>(), 128),
        ("allocation_page", size_of::<AllocationPage<'_>>(), 192),
        ("map_root", size_of::<FileMapRoot<'_>>(), 256),
        ("map_cursor", size_of::<FileMapCursor<'_>>(), 320),
        ("map_chunk", size_of::<FileMapChunk<'_>>(), 128),
        ("chunk_location", size_of::<ChunkLocation>(), 96),
        ("file_metadata", size_of::<FileMetadata>(), 64),
        ("file_name", size_of::<FileName>(), 129),
        ("hash_state", size_of::<ContentHasher>(), 128),
        ("wal_cause_option", size_of::<Option<wal::Cause>>(), 256),
        ("wal_error", size_of::<wal::WalIoError>(), 256),
        ("mutable_role_result", size_of::<MutableRoleResult>(), 256),
    ];
    for (name, actual, bound) in type_rows {
        println!("XADB_STATUS_BOOTSTRAP_LAYOUT {name} {actual} {bound}");
        assert!(actual <= bound, "{name}");
    }
    let result_carrier = [
        size_of::<boot::BootstrapError>(),
        size_of::<std::result::Result<boot::BootstrapPlan, boot::BootstrapError>>(),
        size_of::<std::result::Result<FileMapRoot<'_>, ControlError>>(),
        size_of::<std::result::Result<FileMapChunk<'_>, ControlError>>(),
        size_of::<std::result::Result<ChunkDescriptor, ControlError>>(),
        size_of::<std::result::Result<PageView<'_>, PageIoError>>(),
        size_of::<std::result::Result<AllocationPage<'_>, AllocationError>>(),
    ]
    .into_iter()
    .max()
    .unwrap();
    let hash = 4 * size_of::<ContentHasher>() + 1536;
    let common = 2 * size_of::<boot::BootstrapPlan>()
        + 12 * size_of::<Profile>()
        + 8 * size_of::<ProfileSpec>()
        + 8 * size_of::<crate::api::Namespace>()
        + 8 * size_of::<MapIdentity>()
        + 6 * size_of::<MapSummary>()
        + 8 * size_of::<FileMapEntry>()
        + 6 * size_of::<ChunkDescriptor>()
        + 8 * size_of::<ControlRef>()
        + 4 * size_of::<SelectedControl>()
        + 6 * size_of::<PageHeader>()
        + 6 * size_of::<PageIdentity>()
        + 8 * size_of::<AllocationEntry>()
        + 4 * size_of::<ExtentAddress>()
        + 4 * size_of::<PageView<'_>>()
        + 2 * size_of::<AllocationPage<'_>>()
        + 1024 // Fixed codec byte arrays/field tables; no page image.
        + 1536 // Bounded refs, scalar, iterator, envelope and enum carriers.
        + 6 * result_carrier
        + hash;
    let prepare_nested = common + 672;
    let reader_control = size_of::<PageFileReader<'_, port::Writer>>()
        .checked_sub(size_of::<port::File>())
        .unwrap();
    let readback_nested = common
        + 160
        + 4 * size_of::<SelectedControl>()
        + 4 * size_of::<FileMapRoot<'_>>()
        + 2 * size_of::<FileMapCursor<'_>>()
        + 3 * size_of::<FileMapChunk<'_>>()
        + 4 * size_of::<ChunkLocation>()
        + 2 * size_of::<MapSummary>()
        + 4 * size_of::<FileName>()
        + 4 * size_of::<FileMetadata>()
        + reader_control;
    let fresh_role = 2 * size_of::<boot::StatusRole<'_>>()
        + 3 * size_of::<RoleResult>()
        + size_of::<Option<wal::Cause>>()
        + size_of::<wal::WalIoError>()
        + size_of::<boot::BootstrapError>()
        + 16 * size_of::<usize>();
    let mutable_role = fresh_role
        + 2 * size_of::<MutableStorageError>()
        + 2 * size_of::<MutableRoleResult>()
        + size_of::<boot::BootstrapError>()
        + 12 * size_of::<usize>();
    for (name, actual, bound) in [
        ("result_carrier", result_carrier, 320),
        ("hash_nested", hash, 2048),
        ("prepare_nested", prepare_nested, 16384),
        ("readback_nested", readback_nested, 16384),
        ("fresh_role_nested", fresh_role, 1024),
        ("mutable_role_nested", mutable_role, 2048),
    ] {
        println!("XADB_STATUS_BOOTSTRAP_LAYOUT {name} {actual} {bound}");
        assert!(actual <= bound, "{name}");
    }
    let actual_source = boot::source_budget::<port::Writer>();
    assert_eq!(actual_source.prepare_nested, prepare_nested);
    assert_eq!(actual_source.readback_nested, readback_nested);
    assert_eq!(actual_source.fresh_role, fresh_role);
    assert_eq!(actual_source.mutable_role, mutable_role);
    assert!(boot::source_budget_fits::<port::Writer>());
    println!("XADB_STATUS_BOOTSTRAP_RESOURCE_VARIANT normal_admit");

    // A zero-byte File with large alignment leaves the old owner cap valid but
    // forces reader-control padding above the nested cap. No port call may run.
    #[repr(align(4096))]
    struct AlignedFile;
    struct AlignedWriter;
    impl xadb_foundation::io::FileIo for AlignedFile {
        fn read_at(&mut self, _: &mut [u8], _: u64) -> std::io::Result<usize> {
            panic!("alignment preflight must precede read")
        }
        fn write_at(&mut self, _: &[u8], _: u64) -> std::io::Result<usize> {
            panic!("alignment preflight must precede write")
        }
        fn sync_all(&mut self) -> std::io::Result<()> {
            panic!("alignment preflight must precede sync")
        }
        fn set_len(&mut self, _: u64) -> std::io::Result<()> {
            panic!("alignment preflight must precede resize")
        }
    }
    impl WriterNamespace for AlignedWriter {
        type File = AlignedFile;
        fn random_incarnation(&mut self) -> xadb_foundation::namespace::NamespaceResult<[u8; 16]> {
            panic!("alignment preflight must precede namespace")
        }
        fn create_new(
            &mut self,
            _: &FileName,
        ) -> xadb_foundation::namespace::NamespaceResult<Self::File> {
            panic!("alignment preflight must precede namespace")
        }
        fn open_existing(
            &mut self,
            _: &FileName,
            _: xadb_foundation::namespace::OpenMode,
        ) -> xadb_foundation::namespace::NamespaceResult<Self::File> {
            panic!("alignment preflight must precede namespace")
        }
        fn open_existing_with_metadata(
            &mut self,
            _: &FileName,
            _: xadb_foundation::namespace::OpenMode,
        ) -> xadb_foundation::namespace::NamespaceResult<(Self::File, FileMetadata)> {
            panic!("alignment preflight must precede namespace")
        }
        fn metadata(
            &self,
            _: &FileName,
        ) -> xadb_foundation::namespace::NamespaceResult<Option<FileMetadata>> {
            panic!("alignment preflight must precede namespace")
        }
        fn list_owned(
            &self,
            _: usize,
        ) -> xadb_foundation::namespace::NamespaceResult<
            Vec<xadb_foundation::namespace::DirectoryEntry>,
        > {
            panic!("alignment preflight must precede namespace")
        }
        fn sync_directory(&mut self) -> xadb_foundation::namespace::NamespaceResult<()> {
            panic!("alignment preflight must precede namespace")
        }
        fn publish_selector(
            &mut self,
            _: &FileName,
            _: xadb_foundation::namespace::Selector,
        ) -> xadb_foundation::namespace::NamespaceResult<()> {
            panic!("alignment preflight must precede namespace")
        }
        fn remove_retired(
            &mut self,
            _: &FileName,
        ) -> xadb_foundation::namespace::NamespaceResult<()> {
            panic!("alignment preflight must precede namespace")
        }
    }
    assert!(size_of::<boot::FreshStorageSession<'_, AlignedWriter>>() <= 16384);
    assert!(boot::source_budget::<AlignedWriter>().readback_nested > 16384);
    assert!(!boot::source_budget_fits::<AlignedWriter>());
    let mut rejected_slot = boxed::<{ wal::MAX_FRAME_BYTES }>(0x91);
    let mut rejected_scratch = [[0x92; PAGE_BYTES]; 3];
    let error = boot::FreshStorageSession::create_status(
        AlignedWriter,
        1,
        &mut rejected_slot,
        &mut rejected_scratch,
    )
    .err()
    .expect("aligned nested carrier must be rejected");
    assert_eq!(error.phase, boot::Phase::Preflight);
    assert!(matches!(error.cause, boot::Cause::InvalidInput));
    assert_eq!(error.effect, Effect::Unchanged);
    assert!(rejected_slot.iter().all(|&byte| byte == 0x91));
    assert!(rejected_scratch.iter().flatten().all(|&byte| byte == 0x92));
    println!("XADB_STATUS_BOOTSTRAP_RESOURCE_VARIANT alignment_reject");

    // Existing aggregate checks use the changed concrete owner/error layouts.
    assert!(fits::<port::Writer>());
    assert!(allocator_flush::fits::<port::Writer>());
    println!("XADB_STATUS_BOOTSTRAP_SOURCE_ALLOWANCE owner16384 files32768 nested16384 total65536");
    mark("SB01-04");
}

#[test]
fn status_bootstrap_sb02_plan_decoder() {
    let mut scratch = [[0; PAGE_BYTES]; 3];
    let status = prepared(1, true, &mut scratch);
    let old = prepared(1, false, &mut scratch);
    assert_eq!(
        status.decode_descriptor(&literal(1, false)),
        Err(ControlError::UnsupportedFormat)
    );
    mark("SB02-01");
    assert_eq!(
        old.decode_descriptor(&literal(1, true)),
        Err(ControlError::UnsupportedFormat)
    );
    mark("SB02-02");
    for (index, value) in [0, 3, u64::MAX].into_iter().enumerate() {
        assert_eq!(
            status.decode_descriptor(&changed(literal(1, true), 280, value)),
            Err(ControlError::UnsupportedFormat)
        );
        mark(&format!("SB02-{:02}", index + 3));
    }
    for (index, value) in [0, 1, 3, u64::MAX].into_iter().enumerate() {
        assert_eq!(
            status.decode_descriptor(&changed(literal(1, true), 120, value)),
            Err(ControlError::Corrupt)
        );
        mark(&format!("SB02-{:02}", index + 6));
    }
    let mut raw = literal(1, true);
    raw[4095] ^= 1;
    assert_eq!(status.decode_descriptor(&raw), Err(ControlError::Corrupt));
    mark("SB02-10");
    let mut raw = literal(1, true);
    raw[288] = 1;
    reseal(&mut raw);
    assert_eq!(status.decode_descriptor(&raw), Err(ControlError::Corrupt));
    mark("SB02-11");
    assert_eq!(
        status.decode_descriptor(&changed(literal(1, true), 280, 1)),
        Err(ControlError::UnsupportedFormat)
    );
    mark("SB02-12");
}

#[test]
fn status_bootstrap_sb03_role_health() {
    for (index, status, fenced, poisoned) in [
        (1, false, false, false),
        (2, true, false, false),
        (3, true, true, false),
        (4, true, true, true),
    ] {
        let mut slot = port::slot();
        let (fresh, state) = seed(&mut slot, status);
        if poisoned {
            poison(&fresh.core.wal._writer);
        }
        if fenced {
            fresh.core.wal.fenced.fence();
        }
        match index {
            1 => {
                let error = fresh.status_role().err().unwrap();
                assert_eq!(
                    (error.phase, error.effect),
                    (boot::Phase::Role, Effect::Unchanged)
                );
                assert!(matches!(error.cause, boot::Cause::UnsupportedPlan));
                #[cfg(feature = "test-support")]
                {
                    let projected: crate::testing::storage_session::StorageFailure = error.into();
                    assert_eq!(
                        (projected.phase, projected.cause, projected.effect),
                        ("role", "unsupported_plan", Effect::Unchanged)
                    );
                    assert!(projected.wal.is_none());
                }
                assert!(!fresh.is_fenced());
            }
            2 => role_exact(fresh.status_role().unwrap(), &fresh.core.wal.fenced),
            _ => {
                let error = fresh.status_role().err().unwrap();
                assert_eq!(
                    (error.phase, error.effect),
                    (boot::Phase::Role, Effect::CallerMustResolve)
                );
                let boot::Cause::Wal(inner) = error.cause else {
                    panic!("wrong cause")
                };
                assert_eq!(inner.effect, Effect::CallerMustResolve);
                if poisoned {
                    assert!(matches!(inner.cause, wal::Cause::Poisoned));
                } else {
                    assert!(matches!(inner.cause, wal::Cause::Fenced));
                }
                assert!(fresh.is_fenced());
            }
        }
        no_io(&state);
        mark(&format!("SB03-{index:02}"));
    }
    for status in [true, false] {
        let mut slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0xa5);
        let (installed, state) = installed_seed(&mut slot, &mut workspace, status);
        let storage = installed.into_mutable_storage();
        if status {
            role_exact(storage.status_role().unwrap(), storage.session_fence());
            println!("XADB_STATUS_BOOTSTRAP_VARIANT SB03-05 healthy");
            poison(&storage.workspace);
            let error = storage.status_role().err().unwrap();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, Cause::Poisoned));
            assert!(storage.session_fence().is_fenced());
            println!("XADB_STATUS_BOOTSTRAP_VARIANT SB03-05 workspace_poison");
        } else {
            let error = storage.status_role().err().unwrap();
            assert_eq!(error.effect, Effect::Unchanged);
            let Cause::Bootstrap(inner) = error.cause else {
                panic!("wrong wrapper")
            };
            assert_eq!(
                (inner.phase, inner.effect),
                (boot::Phase::Role, Effect::Unchanged)
            );
            assert!(matches!(inner.cause, boot::Cause::UnsupportedPlan));
            assert!(!storage.session_fence().is_fenced());
            println!("XADB_STATUS_BOOTSTRAP_VARIANT SB03-05 plan1");
        }
        no_io(&state);
    }
    mark("SB03-05");
}

#[test]
fn status_bootstrap_sb04_incomplete_creation() {
    for case in 1..=7 {
        let (writer, state) = port::fresh();
        match case {
            2 => {
                state.lock().unwrap().plan = Some(port::Plan {
                    at: 16,
                    action: port::Action::BeforeError,
                })
            }
            3 => {
                state.lock().unwrap().plan = Some(port::Plan {
                    at: 348,
                    action: port::Action::AfterError,
                })
            }
            4 => {
                state.lock().unwrap().plan = Some(port::Plan {
                    at: 349,
                    action: port::Action::AfterError,
                })
            }
            5 => state.lock().unwrap().read_edits.push((
                11,
                "d3-bootstrap".into(),
                literal(1, false),
            )),
            6 => {
                state.lock().unwrap().read_plan = Some(port::Plan {
                    at: 11,
                    action: port::Action::BeforeError,
                })
            }
            7 => {
                state.lock().unwrap().plan = Some(port::Plan {
                    at: 13,
                    action: port::Action::AfterPanic,
                })
            }
            _ => (),
        }
        let mut slot = port::slot();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            boot::FreshStorageSession::create_status(
                writer,
                if case == 1 { 0 } else { 1 },
                &mut slot,
                &mut [[0; PAGE_BYTES]; 3],
            )
            .map(drop)
        }));
        if case == 7 {
            assert!(outcome.is_err());
        } else {
            let error = outcome.unwrap().unwrap_err();
            let phase = match case {
                1 => boot::Phase::Preflight,
                2 => boot::Phase::DataCreate,
                3 => boot::Phase::SelectorPublish,
                4 => boot::Phase::SelectorDirectory,
                _ => boot::Phase::Readback,
            };
            assert_eq!(error.phase, phase);
            assert_eq!(
                error.effect,
                if case == 1 {
                    Effect::Unchanged
                } else {
                    Effect::CallerMustResolve
                }
            );
            match case {
                1 => assert!(matches!(error.cause, boot::Cause::InvalidInput)),
                2..=4 => {
                    let boot::Cause::Namespace(inner) = error.cause else {
                        panic!("wrong namespace cause")
                    };
                    assert_eq!(
                        inner.kind,
                        xadb_foundation::namespace::NamespaceErrorKind::Io
                    );
                    assert_eq!(inner.io_kind, Some(std::io::ErrorKind::Other));
                    assert_eq!(
                        inner.effect,
                        if case == 2 {
                            Effect::Unchanged
                        } else {
                            Effect::CallerMustResolve
                        }
                    );
                }
                5 => assert!(matches!(
                    error.cause,
                    boot::Cause::Control(ControlError::UnsupportedFormat)
                )),
                6 => {
                    let boot::Cause::Io(inner) = error.cause else {
                        panic!("wrong read cause")
                    };
                    assert_eq!(inner.code, xadb_foundation::error::ErrorCode::IoRead);
                    assert_eq!(inner.effect(), Effect::Unchanged);
                    assert_eq!(inner.io_kind, Some(std::io::ErrorKind::Other));
                }
                _ => unreachable!(),
            }
        }
        let state = state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
        assert_partial(case, &state);
        let effects = match case {
            1 => 0,
            2 => 17,
            3 => 349,
            7 => 14,
            _ => 350,
        };
        assert_eq!(state.events.len(), effects);
        assert_eq!(state.hit, usize::from(matches!(case, 2 | 3 | 4 | 6 | 7)));
        if case != 1 {
            assert_eq!(state.files["d3-bootstrap"].bytes, literal(1, case != 5));
            assert!(state.events.iter().all(|event| event.kind != "remove"));
        }
        if case == 2 {
            assert!(!state.files.contains_key("d3-data-0000000100000001"));
        }
        if matches!(case, 3..=6) {
            assert!(state.files.contains_key("FILEMAP"));
        }
        if matches!(case, 5 | 6) {
            assert_eq!(state.reads, 12);
            let read = &state.read_events[11];
            assert_eq!(
                (read.kind, read.name.as_str(), read.offset, read.length),
                ("read", "d3-bootstrap", 0, 4096)
            );
            assert_eq!(state.continuation_read_hits, 0);
            assert_eq!(state.configured_read_hits, usize::from(case == 6));
        }
        mark(&format!("SB04-{case:02}"));
    }
}

#[test]
fn status_bootstrap_sb05_legacy_adapters() {
    for fenced in [false, true] {
        let mut slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0xa5);
        let (installed, state) = installed_seed(&mut slot, &mut workspace, true);
        let storage = installed.into_mutable_storage();
        let before = images(&state);
        let workspace_before = storage
            .workspace
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .as_slice()
            .to_vec();
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut page = [0x3c; PAGE_BYTES];
        let mut operation = boxed::<OPERATION_BYTES>(0x5a);
        let mut frames = [DirtyFrame::new(&mut page)];
        if fenced {
            storage.session_fence().fence();
        }
        let error = storage
            .bind_allocator(&core, &mut frames, &mut operation)
            .err()
            .unwrap();
        if fenced {
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, Cause::Fenced));
        } else {
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(matches!(error.cause, Cause::UnsupportedPlan));
        }
        assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
        frames[0].check_empty().unwrap();
        assert_eq!(page, [0x3c; PAGE_BYTES]);
        assert!(operation.iter().all(|byte| *byte == 0x5a));
        assert_eq!(
            storage
                .workspace
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .as_slice(),
            workspace_before
        );
        assert_eq!(images(&state), before);
        no_io(&state);
        mark(if fenced { "SB05-02" } else { "SB05-01" });
    }
    for poisoned in [false, true] {
        let mut slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0xa5);
        let (installed, state) = installed_seed(&mut slot, &mut workspace, true);
        let before = images(&state);
        let workspace_before = installed._workspace.as_slice().to_vec();
        if poisoned {
            poison(&installed.fresh.core.wal._writer);
        }
        let mut frame = [0x3c; PAGE_BYTES];
        let mut operation = boxed::<81920>(0x5a);
        let error = installed
            .into_allocation_session(&mut frame, &mut operation)
            .err()
            .unwrap();
        assert_eq!(
            (error.phase, error.effect),
            (
                wal::AllocationSessionPhase::Constructor,
                Effect::CallerMustResolve
            )
        );
        if poisoned {
            assert!(matches!(error.cause, wal::AllocationSessionCause::Poisoned));
        } else {
            assert!(matches!(
                error.cause,
                wal::AllocationSessionCause::UnsupportedPlan
            ));
        }
        assert_eq!(frame, [0x3c; PAGE_BYTES]);
        assert!(operation.iter().all(|byte| *byte == 0x5a));
        assert_eq!(workspace.as_slice(), workspace_before);
        assert_eq!(images(&state), before);
        no_io(&state);
        let state = state.lock().unwrap();
        assert_eq!((state.handles, state.writers), (0, 0));
        mark(if poisoned { "SB05-04" } else { "SB05-03" });
    }
}

fn mandatory_alias(profile: u32, expected: u64, wire: u64, high: u64) -> bool {
    (expected == 2 && wire == 2 && high == 2)
        || (profile == 1
            && ((expected == 1 && wire == 2 && high == 2)
                || (expected == 2
                    && ((wire == 1 && matches!(high, 1 | 2))
                        || (wire == 2 && high != 2)
                        || matches!(wire, 0 | 3 | u64::MAX)))))
}

#[test]
#[ignore = "DEV3 S5 expanded parser matrix only"]
fn status_bootstrap_s5_expanded_matrix() {
    let mut count = 0;
    for profile in 1..=3 {
        for expected in [1, 2] {
            let mut scratch = [[0; PAGE_BYTES]; 3];
            let plan = prepared(profile, expected == 2, &mut scratch);
            for wire in [0, 1, 2, 3, u64::MAX] {
                let highwaters: &[u64] = if matches!(wire, 1 | 2) {
                    &[0, 1, 2, 3, u64::MAX]
                } else {
                    &[2]
                };
                for &high in highwaters {
                    if mandatory_alias(profile, expected, wire, high) {
                        continue;
                    }
                    let raw = changed(changed(literal(profile, true), 280, wire), 120, high);
                    let wanted = if wire != expected {
                        Err(ControlError::UnsupportedFormat)
                    } else if high != expected {
                        Err(ControlError::Corrupt)
                    } else {
                        Ok(())
                    };
                    assert_eq!(plan.decode_descriptor(&raw), wanted);
                    mark(&format!("SB-S5-P{profile}-E{expected}-W{wire}-H{high}"));
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 65);
}
