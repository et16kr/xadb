//! Actual producer -> isolated closed recovery/failure/reentry subprocesses.
use crate::{
    storage::{
        dirty_frame::DirtyFrame,
        page::PAGE_BYTES,
        wal_io::{ClosedRecovery, FreshStorageSession},
    },
    transaction::concurrency::{OwnerRole, PageConcurrency},
};
use foundation::{
    codec::content_digest,
    io::{FileIo, write_all_at},
    namespace::{
        DirectoryEntry, DirectoryPort, FileMetadata, FileName, NamespaceResult, OpenMode, Selector,
        WriterNamespace,
    },
};
use platform::namespace::{LinuxDirectory, LinuxNamespaceFile, LinuxWriterLease};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

#[derive(Default)]
pub(in crate::storage::wal_io) struct Trace {
    pub(in crate::storage::wal_io) root: PathBuf,
    pub(in crate::storage::wal_io) recovering: bool,
    pub(in crate::storage::wal_io) handles: usize,
    pub(in crate::storage::wal_io) peak: usize,
    pub(in crate::storage::wal_io) fd_peak: usize,
    pub(in crate::storage::wal_io) events: Vec<String>,
    pub(in crate::storage::wal_io) fd_rows: Vec<String>,
    cut: usize,
    cut_count: usize,
    cut_hits: usize,
    fault: String,
    tail_cut: bool,
    operational_home_offset: Option<u64>,
    operational_b_synced: bool,
    empty_status_synced: bool,
    pause: Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>,
}
impl Trace {
    pub(in crate::storage::wal_io) fn sample(&mut self, tag: &str) {
        let n = db_fds(&self.root);
        self.fd_peak = self.fd_peak.max(n);
        self.fd_rows.push(format!("{tag}\t{n}"));
    }
    fn mutation(&mut self, name: &str) {
        if self.cut > 0 && name != "d3-wal-00000001" {
            self.cut_count += 1;
        }
        if self.recovering {
            assert!(matches!(
                name,
                "d3-rebase-0"
                    | "d3-rebase-1"
                    | "d3-dwb-stage"
                    | "d3-dwb-0"
                    | "d3-dwb-1"
                    | "d3-dwb-images"
                    | "d3-dwb-select-stage"
                    | "d3-data-0000000100000001"
                    | "namespace"
                    | "d3-wal-00000001"
            ));
        }
    }
    fn event(&mut self, op: &str, name: &str, offset: u64, length: usize, result: &str) {
        self.events
            .push(format!("{op}\t{name}\t{offset}\t{length}\t{result}"));
        self.sample(op);
    }
}
pub(in crate::storage::wal_io) fn db_fds(root: &Path) -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap()
        .filter_map(|e| fs::read_link(e.ok()?.path()).ok())
        .filter(|p| p.starts_with(root))
        .count()
}
pub(in crate::storage::wal_io) struct Directory {
    pub(in crate::storage::wal_io) inner: LinuxDirectory,
    pub(in crate::storage::wal_io) trace: Arc<Mutex<Trace>>,
}
pub(in crate::storage::wal_io) struct Writer {
    inner: LinuxWriterLease,
    trace: Arc<Mutex<Trace>>,
}
pub(in crate::storage::wal_io) struct File {
    inner: Option<LinuxNamespaceFile>,
    name: String,
    trace: Arc<Mutex<Trace>>,
}
impl DirectoryPort for Directory {
    type Writer = Writer;
    fn acquire_writer(&self) -> NamespaceResult<Writer> {
        let inner = self.inner.acquire_writer()?;
        self.trace.lock().unwrap().sample("lease");
        Ok(Writer {
            inner,
            trace: Arc::clone(&self.trace),
        })
    }
}
impl Writer {
    fn wrap(&self, file: LinuxNamespaceFile, name: &FileName) -> File {
        let mut t = self.trace.lock().unwrap();
        t.handles += 1;
        t.peak = t.peak.max(t.handles);
        t.sample("open");
        File {
            inner: Some(file),
            name: name.to_string(),
            trace: Arc::clone(&self.trace),
        }
    }
    fn after(&self, op: &str, name: &str) -> NamespaceResult<()> {
        let mut t = self.trace.lock().unwrap();
        t.event(op, name, 0, 0, "OK");
        let fault = (t.cut > 0 && t.cut_count == t.cut)
            || (t.recovering && t.fault == "retirement_publish_after" && op == "publish")
            || (t.recovering && t.fault == "boot_dir_after" && op == "dirsync")
            || (t.recovering
                && t.fault == "operational_b_retirement_after"
                && t.operational_b_synced
                && op == "publish")
            || (t.recovering
                && t.fault == "empty_status_retirement_after"
                && t.empty_status_synced
                && op == "publish");
        if fault {
            t.cut_hits += 1;
            t.fault.clear();
            return Err(foundation::namespace::NamespaceError::new(
                foundation::namespace::NamespaceErrorKind::Io,
                foundation::error::Effect::CallerMustResolve,
                "closed recovery native after-effect unknown",
            ));
        }
        Ok(())
    }
}
impl Drop for File {
    fn drop(&mut self) {
        drop(self.inner.take());
        let mut t = self.trace.lock().unwrap();
        t.handles -= 1;
        t.sample("close");
    }
}
impl FileIo for File {
    fn read_at(&mut self, out: &mut [u8], offset: u64) -> io::Result<usize> {
        let n = self.inner.as_mut().unwrap().read_at(out, offset)?;
        self.trace.lock().unwrap().event(
            "read",
            &self.name,
            offset,
            out.len(),
            &format!("{n}:{}", hex(&content_digest(&out[..n]))),
        );
        Ok(n)
    }
    fn write_at(&mut self, input: &[u8], offset: u64) -> io::Result<usize> {
        let (partial, pause) = {
            let mut t = self.trace.lock().unwrap();
            t.mutation(&self.name);
            if self.name == "d3-data-0000000100000001" {
                t.operational_home_offset = Some(offset);
            }
            if t.recovering {
                assert_ne!(self.name, "d3-wal-00000001");
            }
            // Keep slot-image and home fault predicates as separate authored
            // cut points even though both deliberately write 8192 bytes.
            #[allow(clippy::if_same_then_else)]
            let partial = if t.tail_cut && self.name == "d3-wal-00000001" && input.len() == 38000 {
                t.tail_cut = false;
                19000
            } else if t.recovering
                && ((t.fault == "boot0_partial" && self.name == "d3-rebase-0" && offset == 0)
                    || (t.fault == "boot1_partial"
                        && self.name == "d3-rebase-1"
                        && offset == 131072))
            {
                t.fault.clear();
                32768
            } else if t.recovering
                && t.fault == "empty_status_slot_partial"
                && self.name == "d3-dwb-images"
                && offset == 0
                && input.len() == 16544
                && &input[..4] == b"XDS1"
                && input[48..56] == 1u64.to_le_bytes()
            {
                t.fault.clear();
                8192
            } else if t.recovering
                && self.name == "d3-data-0000000100000001"
                && (((t.fault == "repair_partial" || t.fault == "redo_second_partial")
                    && offset == 16384)
                    || (t.fault == "operational_b_partial" && offset == 32768))
            {
                t.fault.clear();
                8192
            } else {
                0
            };
            let pause = if self.name == "d3-dwb-images" {
                t.pause.take()
            } else {
                None
            };
            (partial, pause)
        };
        if let Some((arrived, release)) = pause {
            arrived.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        if partial > 0 {
            write_all_at(self.inner.as_mut().unwrap(), &input[..partial], offset)
                .map_err(io::Error::other)?;
            let mut t = self.trace.lock().unwrap();
            t.cut_hits += 1;
            t.event(
                "write",
                &self.name,
                offset,
                input.len(),
                &format!("ERR_PARTIAL:{partial}:{}", hex(&content_digest(input))),
            );
            return Err(io::Error::other("fixed actual partial write"));
        }
        let n = self.inner.as_mut().unwrap().write_at(input, offset)?;
        self.trace.lock().unwrap().event(
            "write",
            &self.name,
            offset,
            input.len(),
            &format!("{n}:{}", hex(&content_digest(input))),
        );
        Ok(n)
    }
    fn sync_all(&mut self) -> io::Result<()> {
        self.trace.lock().unwrap().mutation(&self.name);
        self.inner.as_mut().unwrap().sync_all()?;
        let mut t = self.trace.lock().unwrap();
        t.event("sync", &self.name, 0, 0, "OK");
        if self.name == "d3-data-0000000100000001" && t.operational_home_offset == Some(32768) {
            t.operational_b_synced = true;
        }
        if self.name == "d3-data-0000000100000001" && t.operational_home_offset == Some(16384) {
            t.empty_status_synced = true;
        }
        if (t.cut > 0 && t.cut_count == t.cut)
            || (t.recovering && t.fault == "boot_sync_after" && self.name == "d3-rebase-1")
        {
            t.cut_hits += 1;
            t.fault.clear();
            return Err(io::Error::other("fixed native sync after-effect unknown"));
        }
        Ok(())
    }
    fn set_len(&mut self, length: u64) -> io::Result<()> {
        assert!(
            !self.trace.lock().unwrap().recovering,
            "recovery must not set_len"
        );
        self.inner.as_mut().unwrap().set_len(length)
    }
}
impl WriterNamespace for Writer {
    type File = File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        assert!(!self.trace.lock().unwrap().recovering);
        Ok(std::array::from_fn(|i| (i + 1) as u8))
    }
    fn create_new(&mut self, name: &FileName) -> NamespaceResult<File> {
        self.trace.lock().unwrap().mutation(name.as_str());
        let f = self.inner.create_new(name)?;
        let f = self.wrap(f, name);
        self.after("create", name.as_str())?;
        Ok(f)
    }
    fn open_existing(&mut self, name: &FileName, mode: OpenMode) -> NamespaceResult<File> {
        let f = self.inner.open_existing(name, mode)?;
        self.trace
            .lock()
            .unwrap()
            .event("open", name.as_str(), 0, 0, &format!("{mode:?}"));
        Ok(self.wrap(f, name))
    }
    fn open_existing_with_metadata(
        &mut self,
        name: &FileName,
        mode: OpenMode,
    ) -> NamespaceResult<(File, FileMetadata)> {
        let (f, m) = self.inner.open_existing_with_metadata(name, mode)?;
        self.trace.lock().unwrap().event(
            "open_meta",
            name.as_str(),
            0,
            0,
            &format!(
                "{mode:?}:{}:{}:{}",
                m.identity.device, m.identity.inode, m.length
            ),
        );
        Ok((self.wrap(f, name), m))
    }
    fn metadata(&self, name: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        let m = self.inner.metadata(name)?;
        self.trace
            .lock()
            .unwrap()
            .event("metadata", name.as_str(), 0, 0, &format!("{m:?}"));
        Ok(m)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        assert!(
            self.trace.lock().unwrap().handles <= 1,
            "source only during enumeration"
        );
        let entries = self.inner.list_owned(limit)?;
        self.trace.lock().unwrap().event(
            "list",
            "namespace",
            0,
            limit,
            &format!("{}:{}", entries.len(), entries.capacity()),
        );
        Ok(entries)
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.trace.lock().unwrap().mutation("namespace");
        self.inner.sync_directory()?;
        self.after("dirsync", "namespace")
    }
    fn publish_selector(&mut self, name: &FileName, role: Selector) -> NamespaceResult<()> {
        self.trace.lock().unwrap().mutation("namespace");
        self.inner.publish_selector(name, role)?;
        self.after("publish", name.as_str())
    }
    fn remove_retired(&mut self, name: &FileName) -> NamespaceResult<()> {
        assert!(!self.trace.lock().unwrap().recovering, "no recovery unlink");
        self.inner.remove_retired(name)
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn hex(raw: &[u8]) -> String {
    raw.iter().map(|b| format!("{b:02x}")).collect()
}

fn produce(
    directory: Directory,
    p: u32,
    kind: &str,
    count: usize,
    cycles: usize,
    context: &str,
    out: &Path,
) {
    let w = directory.acquire_writer().unwrap();
    let trace = Arc::clone(&directory.trace);
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut flush = boxed::<4194304>();
    let mut scratch = [[0; PAGE_BYTES]; 3];
    let fresh = if kind.starts_with('S') {
        FreshStorageSession::create_status(w, p, &mut slot, &mut scratch)
    } else {
        FreshStorageSession::create(w, p, &mut slot, &mut scratch)
    }
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    if kind.starts_with('E') {
        let mut frame = boxed::<PAGE_BYTES>();
        let mut op = boxed::<81920>();
        let mut s = installed
            .into_allocation_session(&mut frame, &mut op)
            .unwrap();
        for prior in [1, 8, 63].into_iter().filter(|n| *n < count) {
            s.reserve_pages(prior).unwrap();
            s.cancel_reserved().unwrap();
        }
        s.reserve_pages(count).unwrap();
        if kind == "EC" {
            s.cancel_reserved().unwrap();
        }
        return;
    }
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut a = boxed::<PAGE_BYTES>();
    let mut s = boxed::<PAGE_BYTES>();
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    if kind.starts_with('D') {
        let mut op = boxed::<98304>();
        let mut frames = [DirtyFrame::new(&mut a)];
        let mut bound = storage.bind_allocator(&core, &mut frames, &mut op).unwrap();
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        for prior in [1, 8, 63].into_iter().filter(|n| *n < count) {
            writer.reserve_pages(&mut owner, prior).unwrap();
            writer.cancel_reserved(&mut owner).unwrap();
        }
        writer.reserve_pages(&mut owner, count).unwrap();
        if context == "historical" {
            let (arrived, ack) = mpsc::sync_channel(0);
            let (resume, release) = mpsc::sync_channel(0);
            {
                let mut t = trace.lock().unwrap();
                t.cut = 19;
                t.cut_count = 0;
                t.pause = Some((arrived, release));
            }
            std::thread::scope(|scope| {
                let task = scope.spawn(|| {
                    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    flusher.try_flush_one(&mut dedicated).unwrap_err()
                });
                ack.recv_timeout(Duration::from_secs(10)).unwrap();
                writer.cancel_reserved(&mut owner).unwrap();
                resume.send(()).unwrap();
                let e = task.join().unwrap();
                fs::write(out.join("producer-error.txt"), format!("{e:?}\n")).unwrap();
            });
        } else {
            if kind == "DC" {
                writer.cancel_reserved(&mut owner).unwrap();
            }
            if context == "a_pending_torn" {
                {
                    let mut t = trace.lock().unwrap();
                    t.cut = 19;
                    t.cut_count = 0;
                }
                let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let e = flusher.try_flush_one(&mut dedicated).unwrap_err();
                fs::write(out.join("producer-error.txt"), format!("{e:?}\n")).unwrap();
            }
        }
    } else {
        let mut op = boxed::<114688>();
        let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
        let old = storage
            .bind_status_bootstrap(&core, &mut frames, &mut op)
            .unwrap();
        let mut bound = match old.into_with_flush() {
            Ok(v) => v,
            Err(_) => panic!("Status flush assembly"),
        };
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        for _ in 0..cycles {
            assert!(writer.reserve_one(&mut owner).unwrap().cancel().is_ok());
        }
        let reserved = writer.reserve_one(&mut owner).unwrap();
        if kind == "SC" {
            assert!(reserved.cancel().is_ok());
            return;
        }
        if kind == "SA" {
            drop(reserved);
            return;
        }
        if context == "tail" {
            trace.lock().unwrap().tail_cut = true;
        }
        let result = reserved.initialize();
        if context == "tail" {
            let e = match result {
                Err(crate::storage::wal_io::InitAttemptError::Fatal(e)) => e,
                _ => panic!("INIT tail cut missing"),
            };
            fs::write(out.join("producer-error.txt"), format!("{e:?}\n")).unwrap();
            return;
        }
        assert!(result.is_ok());
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        if matches!(
            context,
            "a_flushed" | "both_flushed" | "raw_corrupt" | "retirement_unknown"
        ) || context.starts_with("boot")
        {
            flusher.try_flush_allocator(&mut dedicated).unwrap();
        }
        if matches!(context, "s_flushed" | "both_flushed" | "raw_corrupt")
            || context.starts_with("boot")
        {
            flusher.try_flush_status(&mut dedicated).unwrap();
        }
        if matches!(
            context,
            "s_pending_torn"
                | "future_pending"
                | "repair_partial"
                | "slot_corrupt"
                | "retirement_unknown"
                | "stage0"
                | "stage4096"
        ) {
            {
                let mut t = trace.lock().unwrap();
                t.cut = if context == "stage0" {
                    16
                } else if context == "stage4096" {
                    18
                } else {
                    19
                };
                t.cut_count = 0;
            }
            let e = flusher.try_flush_status(&mut dedicated).unwrap_err();
            fs::write(out.join("producer-error.txt"), format!("{e:?}\n")).unwrap();
        }
    }
}

pub(in crate::storage::wal_io) fn dump(
    out: &Path,
    p: &super::super::RecoveryPrefixObservation,
    o: &super::RecoveryObservation,
) {
    let l = &o.ledger;
    let ph = &o.physical;
    let b = &o.boot;
    let pairs = vec![
        ("complete_lsn", p.complete_lsn().to_string()),
        ("complete_offset", p.complete_offset().to_string()),
        ("source_length", p.source_length().to_string()),
        ("highwater", p.work_high_water().to_string()),
        ("active_mask", p.active_mask().to_string()),
        ("terminal_hold_mask", p.terminal_hold_mask().to_string()),
        ("inventory", p.inventory_items().to_string()),
        ("inventory_digest", hex(p.inventory_digest())),
        ("remaining_wal", l.remaining.wal.to_string()),
        ("remaining_pages", l.remaining.pages.to_string()),
        ("remaining_frames", l.remaining.peak.frames.to_string()),
        ("remaining_memory", l.remaining.peak.memory.to_string()),
        ("remaining_files", l.remaining.peak.files.to_string()),
        ("remaining_protected", l.remaining.protected.to_string()),
        ("replay_topup", l.replay_topup.to_string()),
        ("actual_owned", l.actual_owned.to_string()),
        ("outstanding", l.outstanding.to_string()),
        ("owned_peak", l.owned_peak.to_string()),
        ("protected", l.protected.to_string()),
        ("epoch_credit", l.epoch_credit.to_string()),
        ("restored", u8::from(l.restored_once).to_string()),
        ("generation", o.known.generation.to_string()),
        ("batch", o.batch.to_string()),
        ("bank", o.known.reference.slot.to_string()),
        ("state", format!("{:?}", o.state)),
        ("phase", format!("{:?}", o.phase)),
        ("source_stabilized", o.source_stabilized.to_string()),
        ("lease_effect", format!("{:?}", o.lease_effect)),
        ("source_device", p.source_identity().device.to_string()),
        ("source_inode", p.source_identity().inode.to_string()),
        ("slots_provisioned", b.slots_provisioned.to_string()),
        ("initial_lengths", format!("{:?}", b.initial_lengths)),
        ("initial_stamps", format!("{:?}", b.initial_stamps)),
        ("provision_stamps", format!("{:?}", b.provision_stamps)),
        ("inventory_entries", b.inventory_entries.to_string()),
        ("inventory_capacity", b.inventory_capacity.to_string()),
        (
            "inventory_backing_bytes",
            b.inventory_backing_bytes.to_string(),
        ),
        (
            "physical_initial_lsn",
            format!("{:?}", &ph.initial_lsn[..2]),
        ),
        ("physical_final_lsn", format!("{:?}", &ph.final_lsn[..2])),
        ("applied", format!("{:?}", &ph.applied[..2])),
        ("skipped", format!("{:?}", &ph.skipped[..2])),
        ("dirty_mask", ph.dirty_mask.to_string()),
        ("flushed_mask", ph.flushed_mask.to_string()),
        ("pending_repaired", ph.pending_repaired.to_string()),
        ("pending_home_skipped", ph.pending_home_skipped.to_string()),
        ("pending_retired", ph.pending_retired.to_string()),
        ("canonical_passes", ph.canonical_passes.to_string()),
        ("possible", format!("{:?}", o.possible)),
        ("guard_fd_peak", o.fd_peak.to_string()),
    ];
    fs::write(
        out.join("observation.tsv"),
        pairs
            .iter()
            .map(|(k, v)| format!("{k}\t{v}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(out.join("effects.txt"), format!("{:#?}\n", o.effects)).unwrap();
    let c = &o.source_census;
    fs::write(out.join("source-census.tsv"),format!("outer\t{}\nprefix_nested\t{}\ninventory\t{}\nwork_nested\t{}\nentry_phase\t{}\nreturn_phase\t{}\npeak\t{}\nwriter\t{}\nfile\t{}\nmaintenance\t12582912\nflush\t4194304\n",c.outer,c.prefix_nested,c.inventory,c.work_nested,c.entry_phase,c.return_phase,c.peak,std::mem::size_of::<Writer>(),std::mem::size_of::<File>())).unwrap();
}
fn dump_error(out: &Path, e: &super::RecoveryError, effects: &super::RecoveryEffects) {
    fs::write(out.join("error.txt"), format!("{e:#?}\n")).unwrap();
    fs::write(
        out.join("error.tsv"),
        format!(
            "phase\t{:?}\ncause\t{:?}\neffect\t{:?}\nlease_effect\t{:?}\nlast_call\t{:?}\n",
            e.phase, e.cause, e.effect, e.lease_effect, effects.last_call
        ),
    )
    .unwrap();
}

#[test]
#[ignore = "new actual closed-recovery producer/recovery/reentry subprocesses only"]
fn worker_closed_recovery_native() {
    let out = PathBuf::from(std::env::var_os("XADB_RECOVERY_EFFECTS_NATIVE_DIR").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_RECOVERY_EFFECTS_NATIVE_DB").unwrap());
    let mode = std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_MODE").unwrap();
    let kind = std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_KIND").unwrap();
    let context = std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_CONTEXT").unwrap();
    let p: u32 = std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_PROFILE")
        .unwrap()
        .parse()
        .unwrap();
    let count: usize = std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_COUNT")
        .unwrap()
        .parse()
        .unwrap();
    let cycles: usize = std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_CYCLES")
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        out.is_absolute()
            && db.is_absolute()
            && (1..=3).contains(&p)
            && matches!(count, 1 | 8 | 63)
    );
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        recovering: mode != "produce",
        fault: std::env::var("XADB_RECOVERY_EFFECTS_NATIVE_FAULT").unwrap_or_default(),
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    trace.lock().unwrap().sample("directory");
    println!(
        "RECOVERY_EFFECTS_NATIVE_BEGIN mode={mode} pid={}",
        std::process::id()
    );
    let outcome;
    if mode == "produce" {
        produce(directory, p, &kind, count, cycles, &context, &out);
        outcome = "produced";
    } else {
        let mut maintenance = boxed::<12582912>();
        let mut flush = boxed::<4194304>();
        match ClosedRecovery::acquire(&directory, None, &mut maintenance, &mut flush) {
            Err(e) => {
                assert_eq!(mode, "reject_acquire", "{e:?}");
                dump_error(&out, e.error(), e.effects());
                fs::write(out.join("effects.txt"), format!("{:#?}\n", e.effects())).unwrap();
                outcome = "rejected_acquire";
            }
            Ok(owner) => match owner.recover() {
                Ok(inspection) => {
                    assert!(mode == "recover" || mode == "verify");
                    dump(&out, inspection.prefix(), inspection.observation());
                    fs::write(out.join("allocator.bin"), inspection.allocator_image()).unwrap();
                    if let Some(s) = inspection.status_image() {
                        fs::write(out.join("status.bin"), s).unwrap();
                    }
                    if let Some(d) = inspection.descriptor_bytes() {
                        fs::write(out.join("descriptor.bin"), d).unwrap();
                    }
                    drop(inspection);
                    outcome = "recovered";
                }
                Err(failure) => {
                    assert!(
                        mode == "fail" || mode == "reject_recover",
                        "{:?}",
                        failure.error()
                    );
                    dump(&out, failure.prefix(), failure.observation());
                    dump_error(&out, failure.error(), failure.effects());
                    drop(failure);
                    outcome = "failed_recover";
                }
            },
        }
        maintenance[0] = 0xa5;
        flush[0] = 0x5a;
        drop(directory);
    }
    let mut t = trace.lock().unwrap();
    t.sample("final");
    fs::write(
        out.join("events.tsv"),
        t.events
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(
        out.join("fd.tsv"),
        t.fd_rows
            .iter()
            .map(|r| format!("{r}\n"))
            .collect::<String>(),
    )
    .unwrap();
    fs::write(out.join("resources.tsv"),format!("handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\ncut_hits\t{}\ncut_ordinal\t{}\n",t.peak,t.handles,t.fd_peak,db_fds(&db),t.cut_hits,t.cut_count)).unwrap();
    assert_eq!(t.handles, 0);
    assert_eq!(db_fds(&db), 0);
    assert!(t.fd_peak <= 5);
    if t.recovering {
        assert!(t.peak <= 2);
    }
    if mode == "fail" {
        assert_eq!(t.cut_hits, 1);
    }
    println!(
        "RECOVERY_EFFECTS_NATIVE_CASE mode={mode} pid={} outcome={outcome} PASS",
        std::process::id()
    );
    println!("RECOVERY_EFFECTS_NATIVE_COMPLETE count=1");
}

#[path = "operational_native_tests.rs"]
mod operational;

#[path = "resident_native_tests.rs"]
mod resident;

#[path = "empty_native_tests.rs"]
mod empty;

#[path = "pressure_native_tests.rs"]
mod pressure;
