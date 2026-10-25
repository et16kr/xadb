//! Separate, explicitly selected producer/recovery/reentry phases. The existing
//! native tracing adapter is reused without invoking any earlier test worker.
use super::*;
use crate::storage::wal_io::ClosedRecovery;
use crate::storage::wal_io::bootstrap::doublewrite_install::mutable_storage::recovery_prefix::native_test_support;
use foundation::namespace::DirectoryPort;
use native_test_support::{Directory, File, Trace, Writer, db_fds};
use platform::namespace::LinuxDirectory;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

fn fresh_write(path: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
fn digest(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn execution_identity() -> String {
    let source = std::env::var("XADB_MIXED_OWNER_SOURCE_SHA256").unwrap();
    assert_eq!(source.len(), 64);
    assert!(source.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let exe = std::env::current_exe().unwrap();
    format!(
        "source_sha256\t{source}\nelf_sha256\t{}\n",
        digest(&fs::read(exe).unwrap())
    )
}
fn disk_snapshot(db: &Path, out: &Path) {
    let directory = out.join("disk");
    fs::create_dir(&directory).unwrap();
    let mut names: Vec<_> = fs::read_dir(db)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    let mut inventory = String::from("name\tdevice\tinode\tlength\tsha256\n");
    for name in names {
        let path = db.join(&name);
        let metadata = fs::metadata(&path).unwrap();
        assert!(metadata.is_file());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, metadata.len());
        fresh_write(&directory.join(&name), &bytes);
        inventory.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            name.to_str().unwrap(),
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            digest(&bytes)
        ));
    }
    fresh_write(&out.join("disk-inventory.tsv"), inventory.as_bytes());
}
fn produce(directory: Directory, db: &Path, out: &Path) {
    let writer = directory.acquire_writer().unwrap();
    drop(directory);
    let mut wal_slot = boxed::<1048576>(0);
    let mut workspace = boxed::<4194304>(0);
    let mut operation = boxed::<INIT_OPERATION_BYTES>(0x73);
    let mut allocator = boxed::<PAGE_BYTES>(0xa1);
    let mut first = boxed::<PAGE_BYTES>(0xb2);
    let mut second = boxed::<PAGE_BYTES>(0xc3);
    let fresh =
        FreshStorageSession::create_undo(writer, 1, &mut wal_slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [
        DirtyFrame::new(&mut allocator),
        DirtyFrame::new(&mut first),
        DirtyFrame::new(&mut second),
    ];
    let binding = storage
        .bind_mixed_system(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(bound) => bound,
        Err(_) => panic!("mixed native activation"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let (a, b, c) = three_dirty_history!(bound, owner, flusher);
    bound.split().2.try_flush(&mut flusher, b.target).unwrap();
    assert_eq!(bound.load_into(2, a.target).unwrap().after.generation, 2);
    assert_eq!(
        snapshot(&bound.inner.inner.pool)[2].1,
        literal(1, "sequence/work1-payload.after")
    );
    fresh_write(
        &out.join("status-a.loaded.page"),
        &snapshot(&bound.inner.inner.pool)[2].1,
    );
    assert_eq!(bound.load_into(2, b.target).unwrap().after.generation, 3);
    let current = snapshot(&bound.inner.inner.pool);
    assert_eq!(current[0].1, literal(1, "sequence/work3-allocator.after"));
    assert_eq!(current[1].1, literal(1, "sequence/work3-payload.after"));
    assert_eq!(current[2].1, literal(1, "sequence/work2-payload.after"));
    assert_eq!(
        (current[0].0.dirty, current[1].0.dirty, current[2].0.dirty),
        (true, true, false)
    );
    fresh_write(&out.join("allocator.dirty.page"), &current[0].1);
    fresh_write(&out.join("undo-c.dirty.page"), &current[1].1);
    fresh_write(&out.join("undo-b.loaded.page"), &current[2].1);
    fresh_write(
        &out.join("system.descriptor"),
        &bound.inner.inner.control.base.descriptor,
    );
    assert_eq!(
        bound.inner.inner.control.base.descriptor.as_slice(),
        literal(1, "sequence/work3-terminal.descriptor")
    );
    let prefix = storage.wal().prefix().unwrap();
    assert_eq!(prefix.complete_lsn, 6);
    assert_eq!(prefix.synced_lsn, 6);
    {
        let (_, reader, _) = bound.split();
        let guard = reader.read(&mut owner, c.target).unwrap();
        assert_eq!(
            guard.bytes().unwrap().as_slice(),
            literal(1, "sequence/work3-payload.after")
        );
        guard.release_checked().unwrap();
    }
    assert!(!storage.session_fence().is_fenced());
    drop(bound);
    drop(owner);
    drop(flusher);
    drop(storage);
    let home = fs::read(db.join(HOME)).unwrap();
    assert_eq!(&home[..PAGE_BYTES], literal(1, "allocator.genesis"));
    assert_eq!(
        &home[PAGE_BYTES..2 * PAGE_BYTES],
        literal(1, "sequence/work1-payload.after")
    );
    assert_eq!(
        &home[2 * PAGE_BYTES..3 * PAGE_BYTES],
        literal(1, "sequence/work2-payload.after")
    );
    assert!(home[3 * PAGE_BYTES..].iter().all(|byte| *byte == 0));
    assert_eq!(
        &fs::read(db.join(wal::WAL_NAME)).unwrap()[4096..],
        literal(1, "sequence/wal.frames-only")
    );
}
fn recover(directory: &Directory, db: &Path, out: &Path, reentry: bool) {
    let mut maintenance = boxed::<12582912>(0xa5);
    let mut flush = boxed::<4194304>(0x5a);
    let owner =
        ClosedRecovery::acquire_mixed(directory, None, &mut maintenance, &mut flush).unwrap();
    assert_eq!(owner.effects().mutation_attempted, 0);
    let inspection = match owner.recover() {
        Ok(inspection) => inspection,
        Err(error) => panic!("mixed native recovery {:?}", error.error()),
    };
    let prefix = inspection.prefix();
    assert_eq!(
        (
            prefix.allocator_lsn(),
            prefix.complete_lsn(),
            prefix.work_high_water()
        ),
        (6, 6, 3)
    );
    assert_eq!(prefix.status_page_mask(), 2);
    assert_eq!(prefix.undo_page_mask(), 12);
    assert_eq!(prefix.active_mask(), 0);
    assert_eq!(prefix.terminal_hold_mask(), 14);
    assert_eq!(prefix.inventory_items(), 0);
    assert_eq!(prefix.transaction_high_water(), 0);
    assert_eq!(prefix.active_transactions(), 0);
    let observation = inspection.observation();
    assert!(observation.ledger.restored_once);
    assert_eq!(
        (
            observation.ledger.remaining.wal,
            observation.ledger.remaining.pages,
            observation.ledger.remaining.protected
        ),
        (0, 0, 0)
    );
    assert_eq!(observation.ledger.replay_topup, 4 * PAGE_BYTES as u64);
    assert_eq!(
        inspection.allocator_image().as_slice(),
        literal(1, "sequence/work3-allocator.after")
    );
    assert_eq!(
        inspection.status_image_at(1).unwrap().as_slice(),
        literal(1, "sequence/work1-payload.after")
    );
    assert_eq!(
        inspection.undo_image_at(2).unwrap().as_slice(),
        literal(1, "sequence/work2-payload.after")
    );
    assert_eq!(
        inspection.undo_image_at(3).unwrap().as_slice(),
        literal(1, "sequence/work3-payload.after")
    );
    assert!(inspection.status_image_at(2).is_none());
    assert!(inspection.undo_image_at(1).is_none());
    assert_eq!(
        inspection.descriptor_bytes().unwrap().as_slice(),
        literal(1, "sequence/work3-terminal.descriptor")
    );
    if reentry {
        assert_eq!(inspection.effects().home_write_attempted, 0);
    } else {
        // A/B already equal their canonical INIT. Only allocator and C need redo homes.
        assert_eq!(
            inspection.effects().home_write_attempted,
            (1 << 0) | (1 << 3)
        );
    }
    fresh_write(&out.join("allocator.page"), inspection.allocator_image());
    fresh_write(
        &out.join("status-1.page"),
        inspection.status_image_at(1).unwrap(),
    );
    fresh_write(
        &out.join("undo-2.page"),
        inspection.undo_image_at(2).unwrap(),
    );
    fresh_write(
        &out.join("undo-3.page"),
        inspection.undo_image_at(3).unwrap(),
    );
    fresh_write(
        &out.join("system.descriptor"),
        inspection.descriptor_bytes().unwrap(),
    );
    assert!(!out.join("observation.tsv").exists());
    native_test_support::dump(out, prefix, observation);
    let home = fs::read(db.join(HOME)).unwrap();
    assert_eq!(&home[..PAGE_BYTES], inspection.allocator_image());
    assert_eq!(
        &home[PAGE_BYTES..2 * PAGE_BYTES],
        inspection.status_image_at(1).unwrap()
    );
    assert_eq!(
        &home[2 * PAGE_BYTES..3 * PAGE_BYTES],
        inspection.undo_image_at(2).unwrap()
    );
    assert_eq!(
        &home[3 * PAGE_BYTES..4 * PAGE_BYTES],
        inspection.undo_image_at(3).unwrap()
    );
    assert!(home[4 * PAGE_BYTES..].iter().all(|byte| *byte == 0));
}
fn worker(mode: &str) {
    let db = PathBuf::from(std::env::var_os("XADB_MIXED_OWNER_NATIVE_DB").unwrap());
    let out = PathBuf::from(std::env::var_os("XADB_MIXED_OWNER_NATIVE_OUT").unwrap());
    let producer = PathBuf::from(std::env::var_os("XADB_MIXED_OWNER_NATIVE_PRODUCER_OUT").unwrap());
    assert!(db.is_absolute() && out.is_absolute() && producer.is_absolute());
    assert!(out.is_dir());
    let identity = execution_identity();
    if mode != "produce" {
        let original = fs::read_to_string(producer.join("execution.tsv")).unwrap();
        let approved_source = std::env::var("XADB_MIXED_OWNER_APPROVED_PRODUCER_SOURCE_SHA256");
        let approved_elf = std::env::var("XADB_MIXED_OWNER_APPROVED_PRODUCER_ELF_SHA256");
        let expected = match (approved_source, approved_elf) {
            (Ok(source), Ok(elf)) => {
                assert_eq!((source.len(), elf.len()), (64, 64));
                assert!(
                    source
                        .bytes()
                        .chain(elf.bytes())
                        .all(|byte| byte.is_ascii_hexdigit())
                );
                format!("source_sha256\t{source}\nelf_sha256\t{elf}\n")
            }
            (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => {
                identity.clone()
            }
            other => panic!("incomplete approved producer identity: {other:?}"),
        };
        assert_eq!(original, expected);
        // The original identity remains visible; a harness-only continuation
        // does not rewrite a prior producer into this executable's identity.
        fresh_write(&out.join("producer-execution.tsv"), original.as_bytes());
        assert_ne!(
            fs::read_to_string(producer.join("pid.txt"))
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap(),
            std::process::id()
        );
    }
    if mode == "reentry" {
        let recovered =
            PathBuf::from(std::env::var_os("XADB_MIXED_OWNER_NATIVE_RECOVERY_OUT").unwrap());
        assert_eq!(
            fs::read_to_string(recovered.join("execution.tsv")).unwrap(),
            identity
        );
        assert_ne!(
            fs::read_to_string(recovered.join("pid.txt"))
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap(),
            std::process::id()
        );
        // Verify the source/home device+inode ledger immediately before entry.
        for line in fs::read_to_string(recovered.join("disk-inventory.tsv"))
            .unwrap()
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = line.split('\t').collect();
            let metadata = fs::metadata(db.join(fields[0])).unwrap();
            assert_eq!(
                (metadata.dev(), metadata.ino()),
                (fields[1].parse().unwrap(), fields[2].parse().unwrap())
            );
            assert_eq!(digest(&fs::read(db.join(fields[0])).unwrap()), fields[4]);
        }
    }
    fresh_write(&out.join("execution.tsv"), identity.as_bytes());
    fresh_write(
        &out.join("pid.txt"),
        format!("{}\n", std::process::id()).as_bytes(),
    );
    println!("MIXED_OWNER_NATIVE_BEGIN\t{mode}\t{}", std::process::id());
    let mut initial_trace = Trace::default();
    initial_trace.root = db.clone();
    initial_trace.recovering = mode != "produce";
    let trace = Arc::new(Mutex::new(initial_trace));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if mode == "produce" {
            produce(directory, &db, &out);
        } else {
            recover(&directory, &db, &out, mode == "reentry");
            drop(directory);
        }
    }));
    let mut trace = trace.lock().unwrap();
    trace.sample("final");
    fresh_write(
        &out.join("events.tsv"),
        (trace.events.join("\n") + "\n").as_bytes(),
    );
    fresh_write(
        &out.join("fd.tsv"),
        (trace.fd_rows.join("\n") + "\n").as_bytes(),
    );
    fresh_write(&out.join("resources.tsv"), format!("writer_bytes\t{}\nfile_bytes\t{}\nhandles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\n", size_of::<Writer>(), size_of::<File>(), trace.peak, trace.handles, trace.fd_peak, db_fds(&db)).as_bytes());
    disk_snapshot(&db, &out);
    if let Err(panic) = result {
        println!("MIXED_OWNER_NATIVE_FAILED\t{mode}\t{}", std::process::id());
        std::panic::resume_unwind(panic);
    }
    assert_eq!((trace.handles, db_fds(&db)), (0, 0));
    assert!(trace.fd_peak <= 5);
    println!("MIXED_OWNER_NATIVE_PASS\t{mode}\t{}", std::process::id());
}
#[test]
#[ignore = "explicit fresh native mixed producer phase"]
fn mixed_owner_native_producer() {
    run_case("MO-NATIVE-PRODUCE", || worker("produce"));
}
#[test]
#[ignore = "explicit fresh native mixed recovery phase"]
fn mixed_owner_native_recovery() {
    run_case("MO-NATIVE-RECOVER", || worker("recover"));
}
#[test]
#[ignore = "explicit third-process native mixed reentry phase"]
fn mixed_owner_native_reentry() {
    run_case("MO-NATIVE-REENTRY", || worker("reentry"));
}
