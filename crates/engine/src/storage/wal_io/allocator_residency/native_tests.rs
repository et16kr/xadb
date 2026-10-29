//! One fresh Linux allocator history crossing two actual residency reloads.
//! Literal bytes are reused inputs; this worker never replays an old result.
use super::*;
use crate::storage::{
    dirty_frame::{DirtyFrame, test_support as frame_test},
    page::{PAGE_BYTES, PageLsn},
    wal_io::{self as wal, bootstrap::doublewrite_install::mutable_storage::FlushProgress},
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use foundation::{
    codec::content_digest,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
    },
};
use platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocator-flush-one"
);
const DIRTY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
);
const HOME: &str = "d3-data-0000000100000001";
#[derive(Default)]
struct Trace {
    enabled: bool,
    events: Vec<String>,
    reads: Vec<String>,
    handles: usize,
    peak: usize,
    root: PathBuf,
    fd_baseline: usize,
    fd_peak: usize,
    fd_events: Vec<String>,
}
struct Writer {
    inner: LinuxWriterLease,
    trace: Arc<Mutex<Trace>>,
}
struct File {
    inner: Option<LinuxNamespaceFile>,
    name: String,
    trace: Arc<Mutex<Trace>>,
}

// Layout-only types retain the concrete traced adapter's fields. No value is
// constructed, and their ports panic if accidentally used for product effects.
struct PaddedFile<const PADDING: usize> {
    _native: File,
    _padding: [u8; PADDING],
}
struct PaddedWriter<const PADDING: usize> {
    _native: Writer,
    _file_padding: std::marker::PhantomData<[u8; PADDING]>,
}
impl<const PADDING: usize> FileIo for PaddedFile<PADDING> {
    fn read_at(&mut self, _: &mut [u8], _: u64) -> io::Result<usize> {
        unreachable!("layout-only file")
    }
    fn write_at(&mut self, _: &[u8], _: u64) -> io::Result<usize> {
        unreachable!("layout-only file")
    }
    fn sync_all(&mut self) -> io::Result<()> {
        unreachable!("layout-only file")
    }
    fn set_len(&mut self, _: u64) -> io::Result<()> {
        unreachable!("layout-only file")
    }
}
impl<const PADDING: usize> WriterNamespace for PaddedWriter<PADDING> {
    type File = PaddedFile<PADDING>;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        unreachable!("layout-only writer")
    }
    fn create_new(&mut self, _: &FileName) -> NamespaceResult<Self::File> {
        unreachable!("layout-only writer")
    }
    fn open_existing(&mut self, _: &FileName, _: OpenMode) -> NamespaceResult<Self::File> {
        unreachable!("layout-only writer")
    }
    fn open_existing_with_metadata(
        &mut self,
        _: &FileName,
        _: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        unreachable!("layout-only writer")
    }
    fn metadata(&self, _: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        unreachable!("layout-only writer")
    }
    fn list_owned(&self, _: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        unreachable!("layout-only writer")
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        unreachable!("layout-only writer")
    }
    fn publish_selector(&mut self, _: &FileName, _: Selector) -> NamespaceResult<()> {
        unreachable!("layout-only writer")
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        unreachable!("layout-only writer")
    }
}

fn layout_case<W: WriterNamespace>(name: &str, expected: bool) {
    use super::super::super::allocation_reservation::budget;
    let writer = size_of::<W>();
    let file = size_of::<W::File>();
    let reader = size_of::<PageFileReader<'_, W>>();
    let bound = size_of::<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>();
    let wrapper = size_of::<OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, W>>();
    let before_payload = size_of::<(ReloadError, BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>)>();
    let error = size_of::<IntoOperationalError<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>>();
    let result = size_of::<
        std::result::Result<
            OperationalAllocation<'_, '_, '_, '_, '_, '_, '_, W>,
            IntoOperationalError<BoundAllocation<'_, '_, '_, '_, '_, '_, '_, W>>,
        >,
    >();
    let conversion = bound + wrapper + result + 2048;
    let semantic = budget::CONSTRUCTOR.max(budget::TEMPORARIES);
    let reload = 2 * reader + 4 * file + dirty_frame::CALL_BYTES + 4096 + semantic + 4096;
    let admitted = fits::<W>();
    println!(
        "ALLOCATOR_RELOAD_NATIVE_LAYOUT name={name} writer={writer} file={file} reader={reader} bound={bound} wrapper={wrapper} before_payload={before_payload} error={error} result={result} conversion={conversion} constructor={} temporaries={} call_bytes={} reload={reload} admitted={admitted}",
        budget::CONSTRUCTOR,
        budget::TEMPORARIES,
        dirty_frame::CALL_BYTES,
    );
    assert_eq!(admitted, expected, "layout admission for {name}");
    if expected {
        assert!(file <= 4096 && reader <= 8192);
        assert!(bound <= 20800 && wrapper <= 20800 && result <= 21504);
        assert!(conversion <= 65536 && semantic <= 16384 && reload <= 65536);
    }
}

/// Static F05 census called by the sibling layout gate; never creates a DB or
/// executes the ignored native history. The large-reader case also records the
/// explicit PageFileReader ceiling rather than inferring it from file size.
pub(super) fn assert_native_layout_gate() {
    const EXACT_PADDING: usize = 4096 - size_of::<File>();
    const OVER_PADDING: usize = 4097 - size_of::<File>();
    const READER_OVER_PADDING: usize = 8193 - size_of::<File>();
    assert_eq!(size_of::<PaddedFile<EXACT_PADDING>>(), 4096);
    assert!(size_of::<PaddedFile<OVER_PADDING>>() > 4096);
    assert!(size_of::<PageFileReader<'_, PaddedWriter<READER_OVER_PADDING>>>() > 8192);
    layout_case::<Writer>("linux_traced", true);
    layout_case::<PaddedWriter<EXACT_PADDING>>("file_4096", true);
    layout_case::<PaddedWriter<OVER_PADDING>>("file_over_4096", false);
    layout_case::<PaddedWriter<READER_OVER_PADDING>>("reader_over_8192", false);
}

impl Drop for File {
    fn drop(&mut self) {
        drop(self.inner.take());
        let mut trace = self.trace.lock().unwrap();
        trace.handles -= 1;
        if trace.enabled {
            sample_fd(&mut trace, "close");
        }
    }
}
fn db_fds(root: &Path) -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .filter(|path| path.starts_with(root))
        .count()
}
fn sample_fd(trace: &mut Trace, tag: &str) {
    let count = db_fds(&trace.root);
    trace.fd_peak = trace.fd_peak.max(count);
    trace.fd_events.push(format!("{tag}\t{count}"));
}
fn event(
    trace: &Arc<Mutex<Trace>>,
    op: &str,
    name: &str,
    offset: u64,
    requested: usize,
    returned: usize,
) {
    let mut trace = trace.lock().unwrap();
    if trace.enabled {
        sample_fd(&mut trace, op);
        assert!(trace.events.len() < 64);
        trace
            .events
            .push(format!("{op}\t{name}\t{offset}\t{requested}\t{returned}"));
    }
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        let count = self.inner.as_mut().unwrap().read_at(bytes, offset)?;
        let mut trace = self.trace.lock().unwrap();
        if trace.enabled {
            sample_fd(&mut trace, "read");
            assert!(trace.reads.len() < 64);
            trace.reads.push(format!(
                "read\t{}\t{offset}\t{}\t{count}",
                self.name,
                bytes.len()
            ));
        }
        Ok(count)
    }
    fn write_at(&mut self, bytes: &[u8], offset: u64) -> io::Result<usize> {
        let count = self.inner.as_mut().unwrap().write_at(bytes, offset)?;
        event(&self.trace, "write", &self.name, offset, bytes.len(), count);
        Ok(count)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.inner.as_mut().unwrap().sync_all()?;
        event(&self.trace, "sync", &self.name, 0, 0, 0);
        Ok(())
    }
    fn set_len(&mut self, n: u64) -> io::Result<()> {
        assert!(!self.trace.lock().unwrap().enabled, "flush cannot resize");
        self.inner.as_mut().unwrap().set_len(n)
    }
}
impl Writer {
    fn wrap(&self, inner: LinuxNamespaceFile, name: &FileName) -> File {
        let mut t = self.trace.lock().unwrap();
        t.handles += 1;
        t.peak = t.peak.max(t.handles);
        if t.enabled {
            sample_fd(&mut t, "open");
        }
        File {
            inner: Some(inner),
            name: name.to_string(),
            trace: Arc::clone(&self.trace),
        }
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        assert!(!self.trace.lock().unwrap().enabled);
        Ok(std::array::from_fn(|n| (n + 1) as u8))
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        let file = self.inner.create_new(name)?;
        event(&self.trace, "create", name.as_str(), 0, 0, 0);
        Ok(self.wrap(file, name))
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        let file = self.inner.open_existing(name, mode)?;
        Ok(self.wrap(file, name))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let (file, metadata) = self.inner.open_existing_with_metadata(name, mode)?;
        Ok((self.wrap(file, name), metadata))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.inner.metadata(name)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        self.inner.list_owned(limit)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.inner.sync_directory()?;
        event(&self.trace, "dirsync", "", 0, 0, 0);
        Ok(())
    }
    fn publish_selector(
        &mut self,
        temporary: &FileName,
        selected: Selector,
    ) -> NamespaceResult<()> {
        self.inner.publish_selector(temporary, selected)?;
        event(&self.trace, "publish", temporary.as_str(), 0, 0, 0);
        Ok(())
    }
    fn remove_retired(&mut self, _: &FileName) -> NamespaceResult<()> {
        panic!("flush1 never removes an owned file")
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn literal(name: &str) -> Vec<u8> {
    fs::read(format!("{INPUT}/{name}")).unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn inventory(path: &Path) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    let mut total = 0;
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let meta = fs::symlink_metadata(entry.path()).unwrap();
        assert!(meta.file_type().is_file());
        assert_eq!(meta.nlink(), 1);
        total += meta.len();
        result.insert(
            entry.file_name().into_string().unwrap(),
            format!(
                "{}\t{}\t{}\t{}\t{}",
                meta.len(),
                meta.dev(),
                meta.ino(),
                meta.nlink(),
                hex(&content_digest(&fs::read(entry.path()).unwrap()))
            ),
        );
    }
    assert!(total <= 24 << 20);
    result
}
fn save_inventory(path: &Path, files: &BTreeMap<String, String>) {
    fs::write(
        path,
        files
            .iter()
            .map(|(n, v)| format!("{n}\t{v}\n"))
            .collect::<String>(),
    )
    .unwrap();
}
fn validate_disk(db: &Path, stem: &str, batch: u64) {
    let images = fs::read(db.join("d3-dwb-images")).unwrap();
    assert_eq!(&images[..16544], literal(&format!("{stem}-slot.bin")));
    assert!(images[16544..].iter().all(|b| *b == 0));
    let home = fs::read(db.join(HOME)).unwrap();
    assert_eq!(&home[..PAGE_BYTES], literal(&format!("{stem}-home.after")));
    assert!(home[PAGE_BYTES..].iter().all(|b| *b == 0));
    let bank = 3 * batch % 2;
    for (name, suffix) in [
        ("d3-dwb-stage".to_owned(), "retirement.bank"),
        (format!("d3-dwb-{bank}"), "retirement.bank"),
        (format!("d3-dwb-{}", 1 - bank), "pending.bank"),
        ("DWB".to_owned(), "retirement.selector"),
    ] {
        assert_eq!(
            fs::read(db.join(name)).unwrap(),
            literal(&format!("{stem}-{suffix}"))
        );
    }
    assert!(!db.join("d3-dwb-select-stage").exists());
}

fn start_trace(trace: &Arc<Mutex<Trace>>) {
    let mut trace = trace.lock().unwrap();
    trace.events.clear();
    trace.reads.clear();
    trace.enabled = true;
}
fn finish_trace(trace: &Arc<Mutex<Trace>>, output: &Path, stem: &str) {
    let mut trace = trace.lock().unwrap();
    trace.enabled = false;
    for (suffix, rows) in [("events", &trace.events), ("reads", &trace.reads)] {
        fs::write(
            output.join(format!("{stem}.{suffix}.tsv")),
            rows.iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "one fresh Linux history selected by the allocator residency native runner"]
fn worker_allocator_residency_native() {
    let output = PathBuf::from(
        std::env::var_os("XADB_ALLOCATOR_RESIDENCY_NATIVE_DIR").expect("native output directory"),
    );
    assert!(output.is_absolute());
    let profile = 1;
    let db = output.join("database");
    fs::create_dir(&db).unwrap();
    let directory = LinuxDirectory::open_existing(&db).unwrap();
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        ..Trace::default()
    }));
    let port = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut page = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<98304>();
    // A dedicated diagnostic output is never used as a product image/oracle.
    let mut actual = boxed::<PAGE_BYTES>();
    let fresh =
        wal::FreshStorageSession::create(port, profile, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let storage = fresh
        .install_doublewrite(&mut flush)
        .unwrap()
        .into_mutable_storage();
    let before = inventory(&db);
    save_inventory(&output.join("before.tsv"), &before);
    let mut expected_wal = fs::read(db.join(wal::WAL_NAME)).unwrap();
    assert_eq!(expected_wal.len(), 4096);
    fs::write(output.join("header.bin"), &expected_wal).unwrap();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(&mut page)];
    let bound = storage
        .bind_allocator(&core, &mut frames, &mut operation)
        .unwrap();
    let mut operational = match bound.into_operational() {
        Ok(value) => value,
        Err(_) => panic!("fresh allocator operational admission"),
    };
    assert_eq!(inventory(&db), before);
    {
        let mut t = trace.lock().unwrap();
        t.peak = t.handles;
        t.fd_baseline = db_fds(&db);
        t.fd_peak = t.fd_baseline;
        sample_fd(&mut t, "baseline");
    }
    let mut writer_owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flush_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut original_reserved = None;
    for (batch, part) in [(1, "reserve"), (2, "cancel")] {
        let (before_stamp, before_observation, before_flush, before_descriptor, reserved_before) = {
            let (mut writer, _, mut flusher) = operational.split_with_flush();
            let changed = if batch == 1 {
                writer.reserve_pages(&mut writer_owner, 1)
            } else {
                writer.cancel_reserved(&mut writer_owner)
            }
            .unwrap();
            assert_eq!(changed.rec_lsn, Some(PageLsn(batch)));
            if batch == 1 {
                original_reserved = writer.reserved_page(&mut writer_owner, 0).unwrap();
                assert!(original_reserved.is_some());
            }
            expected_wal.extend(fs::read(format!("{DIRTY}/p1-n1-{part}.frame")).unwrap());
            assert_eq!(fs::read(db.join(wal::WAL_NAME)).unwrap(), expected_wal);
            start_trace(&trace);
            let FlushProgress::Complete(done) = flusher.try_flush_one(&mut flush_owner).unwrap()
            else {
                panic!("expected actual complete flush");
            };
            finish_trace(&trace, &output, &format!("{part}.flush"));
            assert!(done.finalized.cleaned);
            assert_eq!((done.io.attempted, done.io.completed), (31, 31));
            validate_disk(&db, &format!("p1-n1-{part}"), batch);
            let snapshot = frame_test::diagnose(flusher.pool, 0, &mut actual);
            assert_eq!(&actual[..], literal(&format!("p1-n1-{part}-home.after")));
            assert!(!snapshot.dirty && snapshot.entry.is_none() && snapshot.total == 0);
            fs::write(output.join(format!("{part}.before.page")), &actual[..]).unwrap();
            fs::write(
                output.join(format!("{part}.before.descriptor")),
                writer.control.descriptor,
            )
            .unwrap();
            (
                snapshot.frame,
                writer.observe(&mut writer_owner).unwrap(),
                flusher.snapshot(),
                writer.control.descriptor,
                writer.reserved_page(&mut writer_owner, 0).unwrap(),
            )
        };
        let inventory_before_reload = inventory(&db);
        save_inventory(
            &output.join(format!("{part}.before.tsv")),
            &inventory_before_reload,
        );
        start_trace(&trace);
        let reloaded = operational.reload_allocator().unwrap();
        finish_trace(&trace, &output, &format!("{part}.reload"));
        assert_eq!(reloaded.before, before_stamp);
        assert_eq!(reloaded.before.generation, batch);
        assert_eq!(reloaded.after.generation, batch + 1);
        assert_eq!(reloaded.after.modification_counter, 1);
        assert_eq!(reloaded.after.page_lsn, PageLsn(batch));
        assert!(reloaded.after.rec_lsn.is_none());
        {
            let (mut writer, _, flusher) = operational.split_with_flush();
            let after = writer.observe(&mut writer_owner).unwrap();
            let snapshot = frame_test::diagnose(flusher.pool, 0, &mut actual);
            assert_eq!(snapshot.frame, reloaded.after);
            assert!(!snapshot.dirty && snapshot.entry.is_none() && snapshot.total == 0);
            let registry = frame_test::flush_diagnose(flusher.pool);
            assert!(registry.claim.is_none() && registry.requirements[0].is_none());
            assert_eq!(flusher.snapshot(), before_flush);
            assert_eq!(writer.control.descriptor, before_descriptor);
            assert_eq!(after.active, before_observation.active);
            assert_eq!(after.reserved, before_observation.reserved);
            assert_eq!(after.work_high_water, before_observation.work_high_water);
            assert_eq!(after.protected_bytes, before_observation.protected_bytes);
            assert_eq!(after.wal, before_observation.wal);
            let reserved_after = writer.reserved_page(&mut writer_owner, 0).unwrap();
            assert_eq!(reserved_after, reserved_before);
            assert_eq!(
                reserved_after,
                if batch == 1 { original_reserved } else { None }
            );
            fs::write(output.join(format!("{part}.after.page")), &actual[..]).unwrap();
            fs::write(
                output.join(format!("{part}.after.descriptor")),
                writer.control.descriptor,
            )
            .unwrap();
            let reserved = reserved_after.map_or_else(
                || "none\tnone\tnone".to_owned(),
                |page| {
                    // Existing wire encoding exposes the opaque FileId only
                    // for the actual witness; expected numbers stay external.
                    let mut encoded = [0; 24];
                    page.encode(&mut encoded).unwrap();
                    let file_id = u64::from_le_bytes(encoded[..8].try_into().unwrap());
                    format!("{file_id}\t{}\t{}", page.page_no, page.incarnation.get())
                },
            );
            fs::write(
                output.join(format!("{part}.reload.tsv")),
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    reloaded.before.generation,
                    reloaded.after.generation,
                    reloaded.before.page_lsn.0,
                    reloaded.after.page_lsn.0,
                    reloaded.before.modification_counter,
                    reloaded.after.modification_counter,
                    after.active.is_some(),
                    after.reserved,
                    after.work_high_water,
                    after.wal.complete_lsn,
                    after.wal.complete_offset,
                    before_flush.io.known.generation,
                    reserved,
                ),
            )
            .unwrap();
        }
        assert_eq!(inventory(&db), inventory_before_reload);
        assert_eq!(fs::read(db.join(wal::WAL_NAME)).unwrap(), expected_wal);
        save_inventory(&output.join(format!("{part}.after.tsv")), &inventory(&db));
        assert!(!storage.session_fence().is_fenced());
    }
    drop(flush_owner);
    drop(writer_owner);
    drop(operational);
    drop(core);
    drop(storage);
    let after = inventory(&db);
    save_inventory(&output.join("after.tsv"), &after);
    let mut t = trace.lock().unwrap();
    sample_fd(&mut t, "final");
    assert_eq!(db_fds(&db), 0);
    assert_eq!(t.handles, 0);
    assert!(t.peak <= 3 && t.fd_peak <= 5);
    fs::write(
        output.join("fd.tsv"),
        format!("{}\t{}\t0\n", t.fd_baseline, t.fd_peak),
    )
    .unwrap();
    fs::write(
        output.join("fd-events.tsv"),
        t.fd_events
            .iter()
            .map(|row| format!("{row}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let bytes: u64 = fs::read_dir(&db)
        .unwrap()
        .map(|entry| entry.unwrap().metadata().unwrap().len())
        .sum();
    println!(
        "ALLOCATOR_RESIDENCY_NATIVE_COMPLETE profile=1 batches=2 reloads=2 effects=62 bytes={bytes} peak_files={}",
        t.peak,
    );
}

#[test]
#[ignore = "selected DEV3 allocator eviction native history, fresh output directory required"]
fn worker_allocator_eviction_native() {
    let output = PathBuf::from(
        std::env::var_os("XADB_ALLOCATOR_EVICTION_NATIVE_DIR").expect("native output"),
    );
    assert!(output.is_absolute());
    let db = output.join("database");
    fs::create_dir(&db).unwrap();
    let directory = LinuxDirectory::open_existing(&db).unwrap();
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        ..Trace::default()
    }));
    let port = Writer {
        inner: directory.acquire_writer().unwrap(),
        trace: Arc::clone(&trace),
    };
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut workspace = boxed::<4194304>();
    let mut pages = [boxed::<PAGE_BYTES>(), boxed::<PAGE_BYTES>()];
    let mut operation = boxed::<98304>();
    let mut actual = boxed::<PAGE_BYTES>();
    let fresh =
        wal::FreshStorageSession::create(port, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3]).unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames: Vec<_> = pages.iter_mut().map(|page| DirtyFrame::new(page)).collect();
    let bound = storage
        .bind_allocator(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match bound.into_operational() {
        Ok(v) => v,
        Err(_) => panic!("admission"),
    };
    let mut expected_wal = fs::read(db.join(wal::WAL_NAME)).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    for (batch, part, source_slot, target_slot) in [(1, "reserve", 0, 1), (2, "cancel", 1, 0)] {
        {
            let (mut writer, _, mut flusher) = bound.split_with_flush();
            if batch == 1 {
                writer.reserve_pages(&mut owner, 1)
            } else {
                writer.cancel_reserved(&mut owner)
            }
            .unwrap();
            assert!(matches!(
                flusher.try_flush_one(&mut flusher_owner).unwrap(),
                FlushProgress::Complete(_)
            ));
            let observed = frame_test::diagnose(writer.pool, source_slot, &mut actual);
            assert_eq!(observed.frame.page_lsn, PageLsn(batch));
            assert_eq!(&actual[..], literal(&format!("p1-n1-{part}-home.after")));
        }
        expected_wal.extend(fs::read(format!("{DIRTY}/p1-n1-{part}.frame")).unwrap());
        assert_eq!(fs::read(db.join(wal::WAL_NAME)).unwrap(), expected_wal);
        validate_disk(&db, &format!("p1-n1-{part}"), batch);
        let before_files = inventory(&db);
        let descriptor = bound.inner.writer.descriptor;
        start_trace(&trace);
        let mut guard = bound.evict_allocator().unwrap();
        let result = guard.load_allocator(target_slot).unwrap();
        assert_eq!(
            result.after.generation,
            if target_slot == 0 { 2 } else { 1 }
        );
        drop(guard);
        finish_trace(&trace, &output, part);
        assert_eq!(inventory(&db), before_files);
        assert_eq!(bound.inner.writer.descriptor, descriptor);
        assert!(trace.lock().unwrap().events.is_empty());
        assert_eq!(trace.lock().unwrap().reads.len(), 6);
        let snap = frame_test::diagnose(&bound.inner.pool, target_slot, &mut actual);
        assert_eq!(snap.frame, result.after);
        assert_eq!(&actual[..], literal(&format!("p1-n1-{part}-home.after")));
        fs::write(output.join(format!("{part}.page")), &actual[..]).unwrap();
        save_inventory(&output.join(format!("{part}.tsv")), &before_files);
    }
    assert!(!bound.inner.storage.session_fence().is_fenced());
    println!(
        "EVICTION_NATIVE two_frames=2 evictions=2 actual_loads=2 reads=12 batches=2 target_generations=1,2 literal_home_and_wal=PASS"
    );
}
