//! Actual generic adapter admission and exact returned-owner reuse.
use super::*;
use foundation::{
    io::FileIo,
    namespace::{DirectoryEntry, FileMetadata, FileName, NamespaceResult, OpenMode, Selector},
};
use std::io;
struct PaddedWriter<const O: usize, const F: usize> {
    inner: port::Writer,
    padding: [u8; O],
}
struct PaddedFile<const F: usize> {
    inner: port::File,
    padding: [u8; F],
}
impl<const F: usize> FileIo for PaddedFile<F> {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        self.inner.read_at(bytes, offset)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        self.inner.write_at(bytes, offset)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.inner.sync_all()
    }
    fn set_len(&mut self, length: u64) -> io::Result<()> {
        self.inner.set_len(length)
    }
}
impl<const O: usize, const F: usize> WriterNamespace for PaddedWriter<O, F> {
    type File = PaddedFile<F>;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.inner.random_incarnation()
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<Self::File> {
        self.inner.create_new(name).map(|inner| PaddedFile {
            inner,
            padding: [0; F],
        })
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<Self::File> {
        self.inner
            .open_existing(name, mode)
            .map(|inner| PaddedFile {
                inner,
                padding: [0; F],
            })
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        self.inner
            .open_existing_with_metadata(name, mode)
            .map(|(inner, meta)| {
                (
                    PaddedFile {
                        inner,
                        padding: [0; F],
                    },
                    meta,
                )
            })
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.inner.sync_directory()
    }
    fn publish_selector(&mut self, name: &FileName, selector: Selector) -> NamespaceResult<()> {
        self.inner.publish_selector(name, selector)
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        self.inner.remove_retired(name)
    }
}
fn padded<'m, 'w, const O: usize, const F: usize>(
    storage: MutableStorageOwner<'m, 'w, port::Writer>,
) -> MutableStorageOwner<'m, 'w, PaddedWriter<O, F>> {
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
        _writer: Mutex::new(PaddedWriter {
            inner: _writer.into_inner().unwrap(),
            padding: [0; O],
        }),
        file: Mutex::new(PaddedFile {
            inner: file.into_inner().unwrap(),
            padding: [0; F],
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
const fn deep<W: WriterNamespace>() -> usize {
    size_of::<MutableStorageOwner<'_, '_, W>>()
        + size_of::<StatusControl<'_, '_, '_>>()
        + size_of::<StatusFlushControl<'_, '_, '_>>()
        + 4 * size_of::<W::File>()
        + wal::DURABILITY_CALL_BYTES
        + 8192
        + 8192
}
fn layout() -> Vec<(&'static str, usize, usize)> {
    type W = port::Writer;
    let b = size_of::<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>();
    let f = size_of::<StatusFlushControl<'_, '_, '_>>();
    let n = size_of::<BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>>();
    let e = size_of::<IntoFlushError<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>>();
    let q = size_of::<
        std::result::Result<
            BoundStatusWithFlush<'_, '_, '_, '_, '_, '_, '_, W>,
            IntoFlushError<BoundStatusBootstrap<'_, '_, '_, '_, '_, '_, '_, W>>,
        >,
    >();
    let (hot, cold) = metadata_peaks();
    vec![
        ("old_bound", b, 28672),
        ("flush_control", f, 4096),
        ("new_bound", n, 28672),
        ("before_error", e, 28672),
        ("adapter_result", q, 28672),
        ("adapter_preflight", b + 8192, 65536),
        ("adapter_construct", b + 4 * f + 8192, 65536),
        ("adapter_before", (b + e).max(e + q) + 8192, 65536),
        ("adapter_success", (b + n).max(n + q) + 8192, 65536),
        ("authority", size_of::<StatusFlushAuthority<'_, '_>>(), 256),
        ("flush_error", size_of::<StatusFlushError>(), 768),
        ("progress_result", size_of::<Result<FlushProgress>>(), 1024),
        (
            "flusher_view",
            size_of::<StatusFlusher<'_, '_, '_, '_, '_, '_, '_, W>>(),
            128,
        ),
        ("metadata_hot", hot, 65536),
        ("metadata_cold", cold, 65536),
        ("owner_deep", deep::<W>(), 65536),
        ("file", size_of::<<W as WriterNamespace>::File>(), 8192),
        ("workspace_used", flush_io::WORKSPACE_USED, 4194304),
    ]
}
fn admission<const O: usize, const F: usize>(expected: bool) {
    let mut slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0);
    let (namespace, state) = port::fresh();
    let fresh =
        FreshStorageSession::create_status(namespace, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let storage = padded::<O, F>(installed.into_mutable_storage());
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut a = boxed::<PAGE_BYTES>(0);
    let mut s = boxed::<PAGE_BYTES>(0);
    let mut work = boxed::<{ super::super::super::OPERATION_BYTES }>(0);
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
    let original = ok(storage.bind_status_bootstrap(&core, &mut frames, &mut work));
    reset(&state);
    let before = inventory(&state);
    let origin = std::ptr::from_ref(original.pool.bound_core());
    assert_eq!(fits::<PaddedWriter<O, F>>(), expected);
    match original.into_with_flush() {
        Ok(mut bound) => {
            assert!(expected);
            no_io(&state);
            assert_eq!(inventory(&state), before);
            assert!(storage.workspace.lock().unwrap().is_none());
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
            initialize(&mut writer, &mut owner, 0);
            let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
            assert!(
                done(ok(flusher.try_flush_status(&mut dedicated)))
                    .finalized
                    .cleaned
            );
        }
        Err(IntoFlushError::Before { error, mut bound }) => {
            assert!(!expected);
            assert!(matches!(error.cause, Cause::Capacity));
            assert_eq!(error.effect, Effect::Unchanged);
            assert_eq!(std::ptr::from_ref(bound.pool.bound_core()), origin);
            assert!(storage.workspace.lock().unwrap().is_some());
            assert!(!storage.session_fence().is_fenced());
            no_io(&state);
            assert_eq!(inventory(&state), before);
            // A real supported .a reserve/cancel consumes the returned original;
            // no repeated .a test selector or generic arithmetic substitute.
            let (mut writer, _) = bound.split();
            let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
            match ok(writer.reserve_one(&mut owner)).cancel() {
                Ok(_) => {}
                _ => panic!("returned owner must remain usable"),
            }
        }
        Err(IntoFlushError::Fatal(e)) => panic!("pre-effect admission unexpectedly fatal {e:?}"),
    }
    drop(storage);
    let state = state.lock().unwrap();
    assert_eq!((state.handles, state.writers), (0, 0));
}
fn busy_return() {
    original_fixture!(1, |original, _core, state, storage| {
        let guard = storage.workspace.lock().unwrap();
        let returned = match original.into_with_flush() {
            Err(IntoFlushError::Before { error, bound }) => {
                assert!(matches!(error.cause, Cause::Busy));
                assert_eq!(error.effect, Effect::Unchanged);
                bound
            }
            _ => panic!("try_lock contention must return original"),
        };
        assert!(!storage.session_fence().is_fenced());
        no_io(&state);
        assert!(guard.is_some());
        drop(guard);
        let converted = converted(returned.into_with_flush());
        no_io(&state);
        drop(converted);
    });
}
pub(super) fn run_rf() {
    let mut count = 0;
    if selected("RF01") {
        let mut within_caps = true;
        for (name, actual, cap) in layout() {
            println!("RESOURCE_STATUS_FLUSH {name} {actual} {cap}");
            within_caps &= actual <= cap;
        }
        for (name, actual, cap) in io_test::layout() {
            println!("RESOURCE_STATUS_FLUSH shared_{name} {actual} {cap}");
            within_caps &= actual <= cap;
        }
        for (name, actual, cap) in io_test::status_layout() {
            println!("RESOURCE_STATUS_FLUSH io_{name} {actual} {cap}");
            within_caps &= actual <= cap;
        }
        assert!(
            within_caps,
            "resource caps exceeded; all observations emitted above"
        );
        assert!(fits::<port::Writer>());
        assert!(super::super::super::super::allocator_flush::fits::<
            port::Writer,
        >());
        let () = AdapterLayout::<port::Writer>::CHECK;
        passed("RF01", &mut count);
    }
    if selected("RF02") {
        const F_AT: usize = 8192 - size_of::<port::File>();
        // The aggregate owner guard rejects these layouts before the isolated
        // File cap is reached; they are not successful cap-boundary examples.
        admission::<0, { F_AT - 8 }>(false);
        variant("RF02", "file-below");
        admission::<0, F_AT>(false);
        variant("RF02", "file-at");
        admission::<0, { F_AT + 8 }>(false);
        variant("RF02", "file-above");
        // File appears once in the owner and four times in the deep equation.
        // Align padding down to eight bytes, then cross the actual aggregate
        // boundary while every individual File still fits its own cap.
        const F_AGG: usize = ((65536 - deep::<PaddedWriter<0, 0>>()) / 5) & !7;
        admission::<0, { F_AGG - 8 }>(true);
        admission::<0, F_AGG>(true);
        admission::<0, { F_AGG + 8 }>(false);
        variant("RF02", "file-aggregate");
        const O_AT: usize = 65536 - deep::<PaddedWriter<0, 4096>>();
        admission::<{ O_AT - 8 }, 4096>(true);
        admission::<O_AT, 4096>(true);
        admission::<{ O_AT + 8 }, 4096>(false);
        variant("RF02", "owner-aggregate");
        busy_return();
        variant("RF02", "busy-return-retry");
        passed("RF02", &mut count);
    }
    complete("RF", count);
}

#[test]
fn status_load_padded_owner_gate() {
    let (bytes, metadata, fits) =
        super::super::operational::load_layout::<PaddedWriter<20000, 0>>();
    println!("padded_owner_load={bytes} metadata={metadata:?} admitted={fits}");
    assert!(bytes > 65536);
    assert!(!fits);
}
