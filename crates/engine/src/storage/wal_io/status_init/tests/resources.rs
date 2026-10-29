//! Test-only padded adapters exercise the real pre-CAS admission path. They are
//! seeded from an existing owner, so no generic bootstrap success is claimed.
use super::*;
use foundation::{
    io::FileIo,
    namespace::{DirectoryEntry, FileMetadata, FileName, NamespaceResult, OpenMode, Selector},
};
use std::{io, marker::PhantomData};
struct TrapWriter<const O: usize, P> {
    _inner: port::Writer,
    _padding: [u8; O],
    _file: PhantomData<P>,
}
struct TrapFile<P> {
    _inner: port::File,
    _padding: P,
}
impl<P> FileIo for TrapFile<P> {
    fn read_at(&mut self, _: &mut [u8], _: u64) -> io::Result<usize> {
        panic!("resource rejection must precede I/O")
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        panic!("resource rejection must precede I/O")
    }
    fn sync_all(&mut self) -> io::Result<()> {
        panic!("resource rejection must precede I/O")
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        panic!("resource rejection must precede I/O")
    }
}
impl<const O: usize, P> WriterNamespace for TrapWriter<O, P> {
    type File = TrapFile<P>;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        panic!("pre-effect trap")
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        panic!("pre-effect trap")
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<Self::File> {
        panic!("pre-effect trap")
    }
    fn open_existing_with_metadata(
        &mut self,
        _: &FileName,
        _: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        panic!("pre-effect trap")
    }
    fn metadata(&self, _: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        panic!("pre-effect trap")
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        panic!("pre-effect trap")
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        panic!("pre-effect trap")
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        panic!("pre-effect trap")
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        panic!("pre-effect trap")
    }
}
fn padded<'m, 'w, const O: usize, P>(
    storage: MutableStorageOwner<'m, 'w, port::Writer>,
    padding: P,
) -> MutableStorageOwner<'m, 'w, TrapWriter<O, P>> {
    let MutableStorageOwner {
        core,
        plan,
        workspace,
        binding,
    } = storage;
    let crate::storage::wal_io::bootstrap::InstalledStorageCore {
        wal: prior,
        plan: bootstrap,
        reader_active,
    } = core;
    let wal::WalRuntime {
        _writer,
        file,
        append,
        completion,
        first_write_lsn,
        durability_waits,
        durability_before_wait,
        fenced,
        profile,
    } = prior;
    let wal = wal::WalRuntime {
        _writer: Mutex::new(TrapWriter {
            _inner: _writer.into_inner().unwrap(),
            _padding: [0; O],
            _file: PhantomData,
        }),
        file: Mutex::new(TrapFile {
            _inner: file.into_inner().unwrap(),
            _padding: padding,
        }),
        append,
        completion,
        first_write_lsn,
        durability_waits,
        durability_before_wait,
        fenced,
        profile,
    };
    MutableStorageOwner {
        core: crate::storage::wal_io::bootstrap::InstalledStorageCore {
            wal,
            plan: bootstrap,
            reader_active,
        },
        plan,
        workspace,
        binding,
    }
}
#[repr(align(4096))]
struct Aligned([u8; 8192]);
fn reject_padded<const O: usize, P>(padding: P, id: &str) {
    let mut slot = port::slot();
    let mut flush = boxed::<{ 4 << 20 }>(0);
    let (namespace, state) = port::fresh();
    let fresh =
        FreshStorageSession::create_status(namespace, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let storage = fresh
        .install_doublewrite(&mut flush)
        .unwrap()
        .into_mutable_storage();
    let storage = padded::<O, P>(storage, padding);
    let owner = size_of::<MutableStorageOwner<'_, '_, TrapWriter<O, P>>>();
    let reader = size_of::<PageFileReader<'_, TrapWriter<O, P>>>();
    let file = size_of::<TrapFile<P>>();
    let constructor = budget::CONSTRUCTOR;
    let total = owner + reader + file + constructor + 32;
    println!(
        "XADB_STATUS_INIT_PADDED {id} owner={owner}/32768 reader={reader} file={file}/16384 constructor={constructor}/16384 carriers=32 total={total}/65536 alignment={}",
        std::mem::align_of::<TrapFile<P>>()
    );
    assert!(owner <= 32768 && file <= 16384 && constructor <= 16384);
    let control = size_of::<StatusControl<'_, '_, '_>>();
    let bound = size_of::<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, TrapWriter<O, P>>>();
    let bound_result = size_of::<
        super::super::super::Result<
            BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, TrapWriter<O, P>>,
        >,
    >();
    assert!(bound <= 28672 && bound_result <= 28672);
    let plans = operation::plan_bytes();
    let temporary = operation::temporary_bytes();
    let prepare = owner + control + plans + temporary + INIT_EXCLUSIVE + 2048;
    let stage = owner + control + plans + INIT_EXCLUSIVE + 4096 + 2048;
    let install = owner + control + INIT_EXCLUSIVE + READ_CALL.max(budget::TEMPORARIES);
    let read = owner + control + READ_CALL;
    let wait = owner + control + 4096 + 2048;
    assert!(
        [prepare, stage, install, read, wait]
            .into_iter()
            .all(|size| size <= 65536)
    );
    assert!(super::super::fits::<port::Writer>());
    println!(
        "XADB_STATUS_INIT_PADDED_OTHER {id} control={control} bound={bound}/28672 result={bound_result}/28672 prepare={prepare}/65536 stage={stage}/65536 install={install}/65536 read={read}/65536 wait={wait}/65536"
    );
    assert!(total > 65536);
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut allocator = boxed::<PAGE_BYTES>(0x3f);
    let mut payload = boxed::<PAGE_BYTES>(0xa5);
    let mut operation = boxed::<OPERATION_BYTES>(0x93);
    let mut frames = [
        DirtyFrame::new(&mut allocator),
        DirtyFrame::new(&mut payload),
    ];
    reset(&state);
    let failure = err(storage.bind_status_bootstrap(&core, &mut frames, &mut operation));
    assert_eq!(failure.effect, Effect::Unchanged);
    assert!(matches!(failure.cause, StorageCause::Capacity));
    assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
    assert!(!storage.session_fence().is_fenced());
    {
        let state = state.lock().unwrap();
        assert!(state.events.is_empty() && state.read_events.is_empty());
    }
    assert!(allocator.iter().all(|b| *b == 0x3f));
    assert!(payload.iter().all(|b| *b == 0xa5));
    assert!(operation.iter().all(|b| *b == 0x93));
    mark(id);
}
#[test]
fn status_init_cf17_padded_readback_admission() {
    if selected("CF17-02") {
        reject_padded::<18000, _>([0u8; 12000], "CF17-02");
    }
    if selected("CF17-03") {
        reject_padded::<4000, _>(Aligned([0; 8192]), "CF17-03");
    }
}
#[test]
fn status_init_cf17_trusted_credit_arithmetic() {
    if !selected("CF17-05") {
        return;
    }
    use super::super::super::super::allocation_reservation::operation::OperationBudget;
    let budget = OperationBudget::StatusInit114688;
    assert_eq!(budget.memory(), 114688);
    assert_eq!(budget.terminal_frames(), 2);
    assert_eq!(budget.terminal_wal(1), 38000);
    assert_eq!(budget.terminal_protected(1), 70768);
    let protected_base = super::super::super::super::PROTECTED_BYTES;
    assert_eq!(protected_base, 21024768);
    let max = protected_base + wal::WAL_FILE_CAP - 4096 + 2 * PAGE_BYTES as u64;
    assert_eq!(max, 289488896);
    assert!(max < 1 << 30);
    fixture!(1, |bound, core, state| {
        let (mut writer, _) = bound.split();
        let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
        let reserved = writer.reserve_one(&mut owner).unwrap();
        let descriptor = &reserved.writer.base.descriptor;
        let u64_at = |at: usize| u64::from_le_bytes(descriptor[at..at + 8].try_into().unwrap());
        assert_eq!(
            (u64_at(912), u64_at(960), u64_at(968), u64_at(1008)),
            (38000, 2, 114688, 70768)
        );
        assert_eq!(
            &descriptor[..],
            literal(1, 0, "init", "work1-reserve.descriptor")
        );
        cancel(reserved);
        assert!(state.lock().unwrap().events.len() >= 4);
    });
    println!(
        "XADB_STATUS_INIT_PROTECTED_LIMIT branch=NOT_APPLICABLE fixed=21024768 maximum=289488896 limit=1073741824 arithmetic_only=true"
    );
    mark("CF17-05");
}
