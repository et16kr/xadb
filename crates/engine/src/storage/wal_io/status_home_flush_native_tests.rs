//! Three Linux Status home flush workers; fresh full-byte inputs are external.
use super::*;
use crate::storage::{
    dirty_frame::DirtyFrame,
    page::{PAGE_BYTES, PageLsn},
    wal_io as wal,
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use std::{
    collections::BTreeMap,
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use xadb_foundation::{
    codec::content_digest,
    io::FileIo,
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
    },
};
use xadb_platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};

#[derive(Default)]
struct Trace {
    enabled: bool,
    load_reads: bool,
    events: Vec<String>,
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
        assert!(trace.events.len() < 256);
        trace
            .events
            .push(format!("{op}\t{name}\t{offset}\t{requested}\t{returned}"));
    }
}
impl FileIo for File {
    fn read_at(&mut self, bytes: &mut [u8], offset: u64) -> io::Result<usize> {
        let result = self.inner.as_mut().unwrap().read_at(bytes, offset);
        if self.trace.lock().unwrap().load_reads {
            event(
                &self.trace,
                "read",
                &self.name,
                offset,
                bytes.len(),
                result.as_ref().copied().unwrap_or(0),
            );
        } else {
            let mut trace = self.trace.lock().unwrap();
            if trace.enabled {
                sample_fd(&mut trace, "read");
            }
        }
        result
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
        panic!("Status home flush never removes an owned file")
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
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

fn disk_state(db: &Path, key: &str, before: &BTreeMap<String, String>) {
    let table = fs::read_to_string(format!("{INPUT}/states.tsv")).unwrap();
    let expected: Vec<_> = table
        .lines()
        .skip(1)
        .filter_map(|row| {
            let f: Vec<_> = row.split('\t').collect();
            (f[0] == key).then(|| (f[1].to_owned(), literal(f[2])))
        })
        .collect();
    assert_eq!(expected.len(), 7);
    for (name, bytes) in expected {
        let actual = match name.as_str() {
            "home:page0" => fs::read(db.join(HOME)).unwrap()[..PAGE_BYTES].to_vec(),
            "home:page1" => fs::read(db.join(HOME)).unwrap()[PAGE_BYTES..2 * PAGE_BYTES].to_vec(),
            "d3-dwb-images:slot0" => fs::read(db.join("d3-dwb-images")).unwrap()[..16544].to_vec(),
            name => fs::read(db.join(name)).unwrap(),
        };
        assert_eq!(actual, bytes, "{key}/{name}");
    }
    let home = fs::read(db.join(HOME)).unwrap();
    let images = fs::read(db.join("d3-dwb-images")).unwrap();
    assert_eq!((home.len(), images.len()), (1048576, 16711680));
    assert!(home[2 * PAGE_BYTES..].iter().all(|b| *b == 0));
    assert!(images[16544..].iter().all(|b| *b == 0));
    assert!(!db.join("d3-dwb-select-stage").exists());
    let current = inventory(db);
    assert_eq!(
        current.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    for (name, expected) in before {
        if ![
            HOME,
            wal::WAL_NAME,
            "d3-dwb-stage",
            "d3-dwb-0",
            "d3-dwb-1",
            "d3-dwb-images",
            "DWB",
        ]
        .contains(&name.as_str())
        {
            assert_eq!(current.get(name), Some(expected), "untouched {name}");
        }
    }
}

struct BatchEvidence<'a> {
    output: &'a Path,
    db: &'a Path,
    before: &'a BTreeMap<String, String>,
    trace: &'a Arc<Mutex<Trace>>,
}

fn flush_batch<'c, 's>(
    flusher: &mut StatusFlusher<'_, 's, 'c, '_, '_, '_, '_, Writer>,
    owner: &mut Owner<'c, 's>,
    target: &str,
    batch: u64,
    key: &str,
    evidence: &BatchEvidence<'_>,
) {
    let BatchEvidence {
        output,
        db,
        before,
        trace,
    } = evidence;
    let start = trace.lock().unwrap().events.len();
    let observed = done(ok(if target == "A" {
        flusher.try_flush_allocator(owner)
    } else {
        flusher.try_flush_status(owner)
    }));
    assert!(observed.finalized.cleaned);
    assert_eq!(
        (
            observed.io.last_issued,
            observed.io.known.generation,
            observed.io.known.reference.generation
        ),
        (batch, 3 * batch + 1, 3 * batch + 1)
    );
    assert_eq!((observed.io.attempted, observed.io.completed), (31, 31));
    assert!(observed.io.homes_synced && observed.io.retirement_selected);
    let events = {
        let trace = trace.lock().unwrap();
        trace.events[start..].to_vec()
    };
    assert_eq!(events.len(), 31);
    fs::write(
        output.join(format!("batch{batch}.events.tsv")),
        events.iter().map(|e| format!("{e}\n")).collect::<String>(),
    )
    .unwrap();
    disk_state(db, &format!("{key}/after"), before);
    save_inventory(
        &output.join(format!("batch{batch}.after.tsv")),
        &inventory(db),
    );
    let index = usize::from(target == "S");
    let mut page = boxed::<PAGE_BYTES>();
    let snap = frame_test::diagnose(flusher.pool, index, &mut page);
    let registry = frame_test::flush_diagnose(flusher.pool);
    assert_eq!(&page[..], literal(&format!("{key}/capture")));
    assert!(!snap.dirty && snap.entry.is_none() && snap.frame.rec_lsn.is_none());
    assert!(registry.claim.is_none() && registry.requirements[index].is_none());
    assert_eq!(snap.total, 0);
    fs::write(
        output.join(format!("batch{batch}.clean.tsv")),
        format!(
            "{target}\t{}\t{}\ttrue\ttrue\ttrue\t{batch}\t{}\t0\n",
            snap.frame.page_lsn.0,
            snap.frame.modification_counter,
            3 * batch + 1
        ),
    )
    .unwrap();
    assert!(!flusher.storage.session_fence().is_fenced());
}

#[test]
#[ignore = "three isolated Linux Status flush databases selected by the new native runner"]
fn worker_status_home_flush_native() {
    let output = PathBuf::from(
        std::env::var_os("XADB_STATUS_FLUSH_NATIVE_DIR").expect("isolated native output"),
    );
    assert!(output.is_absolute());
    let profile: u32 = std::env::var("XADB_STATUS_FLUSH_NATIVE_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..=3).contains(&profile));
    let cancellations = usize::from(profile == 3);
    let batches = if profile == 3 { 3 } else { 2 };
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
    let mut allocator = boxed::<PAGE_BYTES>();
    let mut payload = boxed::<PAGE_BYTES>();
    payload.fill(0xa5);
    let mut work = boxed::<INIT_OPERATION_BYTES>();
    let fresh = wal::FreshStorageSession::create_status(
        port,
        profile,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    let before = inventory(&db);
    save_inventory(&output.join("before.tsv"), &before);
    let header = fs::read(db.join(wal::WAL_NAME)).unwrap();
    assert_eq!(header.len(), 4096);
    fs::write(output.join("wal-header.bin"), &header).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [
        DirtyFrame::new(&mut allocator),
        DirtyFrame::new(&mut payload),
    ];
    let old = ok(storage.bind_status_bootstrap(&core, &mut frames, &mut work));
    let mut bound = converted(old.into_with_flush());
    assert_eq!(inventory(&db), before);
    {
        let mut trace = trace.lock().unwrap();
        trace.enabled = true;
        trace.peak = trace.handles;
        trace.fd_baseline = db_fds(&db);
        trace.fd_peak = trace.fd_baseline;
        sample_fd(&mut trace, "baseline");
    }
    {
        let evidence = BatchEvidence {
            output: &output,
            db: &db,
            before: &before,
            trace: &trace,
        };
        let (mut writer, reader, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        if cancellations == 1 {
            match ok(writer.reserve_one(&mut owner)).cancel() {
                Ok(_) => {}
                Err(InitAttemptError::Before(e)) => panic!("cancel before {:?}", e.error),
                Err(InitAttemptError::Fatal(e)) => panic!("cancel fatal {e:?}"),
            }
        }
        let reserved = ok(writer.reserve_one(&mut owner));
        if profile == 3 {
            flush_batch(
                &mut flusher,
                &mut dedicated,
                "A",
                1,
                "native-P3-b1",
                &evidence,
            );
        }
        let observed = match reserved.initialize() {
            Ok(value) => value,
            Err(InitAttemptError::Before(e)) => panic!("INIT before {:?}", e.error),
            Err(InitAttemptError::Fatal(e)) => panic!("INIT fatal {e:?}"),
        };
        let lsn = 2 * (cancellations + 1) as u64;
        assert_eq!(observed.lsn, PageLsn(lsn));
        let guard = ok(reader.read(&mut owner));
        assert_eq!(
            &guard.bytes().unwrap()[..],
            literal(&format!(
                "p{profile}-c{cancellations}/work{}-status.after",
                cancellations + 1
            ))
        );
        ok(guard.release_checked());
        let order = if profile == 1 { "AS" } else { "SA" };
        for (index, target) in order.chars().enumerate() {
            let batch = index as u64 + 1 + u64::from(profile == 3);
            let key = if profile == 3 {
                format!("native-P3-b{batch}")
            } else {
                format!("p{profile}-c0-{order}-b{batch}")
            };
            flush_batch(
                &mut flusher,
                &mut dedicated,
                &target.to_string(),
                batch,
                &key,
                &evidence,
            );
        }
        let (a, s, snap) = diagnose(flusher.pool);
        assert_eq!(
            &a[..],
            literal(&format!(
                "p{profile}-c{cancellations}/work{}-allocator.after",
                cancellations + 1
            ))
        );
        assert_eq!(
            &s[..],
            literal(&format!(
                "p{profile}-c{cancellations}/work{}-status.after",
                cancellations + 1
            ))
        );
        assert_eq!(snap.dirty, [false, false]);
        assert_eq!(snap.rec_lsns, [None, None]);
        assert_eq!(snap.page_lsns, [PageLsn(lsn), PageLsn(lsn)]);
        assert_eq!(snap.counters, [lsn, 1]);
        assert_eq!(snap.total, 0);
        assert!(snap.claim_payload.is_none());
        assert!(
            snap.entries.iter().all(Option::is_none)
                && snap.requirements.iter().all(Option::is_none)
        );
        let owner_state = [
            ("status_installation_none", writer.installation.is_none()),
            ("base_installation_none", writer.base.installation.is_none()),
            ("base_batch_none", writer.base.batch.is_none()),
            ("terminal_pending_false", !writer.base.terminal_pending),
            ("flush_capture_none", flusher.control.capture.is_none()),
            ("flush_retired_none", flusher.control.retired.is_none()),
            ("flush_authority_none", flusher.control.authority.is_none()),
            ("flush_phase_ready", flusher.control.phase == Phase::Ready),
            ("healthy", !flusher.storage.session_fence().is_fenced()),
        ];
        assert!(owner_state.iter().all(|(_, observed)| *observed));
        fs::write(
            output.join("owner-state.tsv"),
            owner_state
                .iter()
                .map(|(name, observed)| format!("{name}\t{observed}\n"))
                .collect::<String>(),
        )
        .unwrap();
        fs::write(output.join("allocator.after"), &a[..]).unwrap();
        fs::write(output.join("status.after"), &s[..]).unwrap();
        fs::write(output.join("owner.descriptor"), writer.base.descriptor).unwrap();
        assert_eq!(
            &writer.base.descriptor[..],
            literal(&format!(
                "p{profile}-c{cancellations}/work{}-init.descriptor",
                cancellations + 1
            ))
        );
        let actual = fs::read(db.join(wal::WAL_NAME)).unwrap();
        assert_eq!(&actual[..4096], header);
        assert_eq!(
            &actual[4096..],
            literal(&format!("p{profile}-c{cancellations}/wal.frames-only"))
        );
        fs::write(output.join("wal.complete"), actual).unwrap();
        fs::write(
            output.join("ram.tsv"),
            format!("{lsn}\t{lsn}\t1\tfalse\tfalse\ttrue\ttrue\t0\ttrue\n"),
        )
        .unwrap();
    }
    drop(bound);
    drop(core);
    drop(storage);
    let mut trace = trace.lock().unwrap();
    sample_fd(&mut trace, "final");
    assert_eq!(trace.handles, 0);
    assert_eq!(db_fds(&db), 0);
    assert!(trace.peak <= 2 && trace.fd_peak <= 5);
    assert_eq!(trace.events.len(), 4 * (cancellations + 1) + 31 * batches);
    fs::write(
        output.join("events.tsv"),
        trace
            .events
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        output.join("fd-events.tsv"),
        trace
            .fd_events
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        output.join("fd.tsv"),
        format!("{}\t{}\t0\n", trace.fd_baseline, trace.fd_peak),
    )
    .unwrap();
    fs::write(
        output.join("resources.tsv"),
        format!(
            "wrapped_peak\t{}\nwrapped_final\t0\nobserved_fd_peak\t{}\nobserved_fd_final\t0\n",
            trace.peak, trace.fd_peak
        ),
    )
    .unwrap();
    drop(trace);
    let after = inventory(&db);
    save_inventory(&output.join("after.tsv"), &after);
    let bytes: u64 = fs::read_dir(&db)
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    println!(
        "STATUS_FLUSH_NATIVE_COMPLETE profile={profile} cancellations={cancellations} batches={batches} effects={} bytes={bytes}",
        31 * batches
    );
}

#[path = "operational_status_native_tests.rs"]
mod operational;

#[path = "status_load_native_tests.rs"]
mod status_load;
