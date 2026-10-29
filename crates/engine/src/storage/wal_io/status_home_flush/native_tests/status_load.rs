//! Fresh Linux owner history; compares complete independent page/WAL literals.
use super::*;
use crate::storage::wal_io::IntoOperationalStatusError;

fn input(name: &str) -> Vec<u8> {
    fs::read(format!(
        "{}/../../tests/fixtures/dev3-operational-status/p1/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
#[ignore = "explicit fresh Linux P3 actual selected home reads"]
fn worker_status_load_native() {
    let output =
        PathBuf::from(std::env::var_os("XADB_STATUS_LOAD_NATIVE_DIR").expect("isolated output"));
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
    let mut allocator = boxed::<PAGE_BYTES>();
    let mut payload = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<INIT_OPERATION_BYTES>();
    let fresh =
        wal::FreshStorageSession::create_status(port, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
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
    let original = converted(
        storage
            .bind_status_bootstrap(&core, &mut frames, &mut operation)
            .unwrap()
            .into_with_flush(),
    );
    let mut bound = match original.into_operational() {
        Ok(bound) => bound,
        Err(IntoOperationalStatusError::Before { error, .. })
        | Err(IntoOperationalStatusError::Fatal(error)) => panic!("conversion {error:?}"),
    };
    {
        let mut t = trace.lock().unwrap();
        t.enabled = true;
        t.peak = t.handles;
        t.fd_baseline = db_fds(&db);
        t.fd_peak = t.fd_baseline;
        sample_fd(&mut t, "baseline");
    }
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    {
        let (mut writer, reader, mut flush) = bound.split_with_flush();
        let first = match writer.reserve_one(&mut owner).unwrap().initialize() {
            Ok(value) => value,
            Err(_) => panic!("first actual INIT"),
        };
        assert_eq!(
            (first.page.page_no, first.lsn.0, first.status.generation),
            (1, 2, 1)
        );
        let read = reader.read(&mut owner).unwrap();
        assert_eq!(
            read.bytes().unwrap().as_slice(),
            input("work1-status.after")
        );
        read.release_checked().unwrap();
        flush.try_flush_status(&mut dedicated).unwrap();
        flush.try_flush_allocator(&mut dedicated).unwrap();
    }
    let first_page = bound
        .split_with_flush()
        .1
        .read(&mut owner)
        .unwrap()
        .stamps()
        .unwrap()
        .1
        .identity
        .page;
    let old_home = fs::read(db.join(HOME)).unwrap()[PAGE_BYTES..2 * PAGE_BYTES].to_vec();
    let events = trace.lock().unwrap().events.len();
    bound.prepare_next_status().unwrap();
    assert_eq!(trace.lock().unwrap().events.len(), events);
    {
        let (mut writer, reader, mut flush) = bound.split_with_flush();
        assert!(writer.reserve_one(&mut owner).unwrap().cancel().is_ok());
        let next = match writer.reserve_one(&mut owner).unwrap().initialize() {
            Ok(value) => value,
            Err(_) => panic!("second actual INIT"),
        };
        assert_eq!(
            (
                next.page.page_no,
                next.page.incarnation.get(),
                next.lsn.0,
                next.status.generation
            ),
            (2, 3, 6, 2)
        );
        let read = reader.read(&mut owner).unwrap();
        assert_eq!(
            read.bytes().unwrap().as_slice(),
            input("work3-status.after")
        );
        read.release_checked().unwrap();
        flush.try_flush_status(&mut dedicated).unwrap();
    }
    let second_page = bound
        .split_with_flush()
        .1
        .read(&mut owner)
        .unwrap()
        .stamps()
        .unwrap()
        .1
        .identity
        .page;
    assert_eq!(
        &fs::read(db.join(HOME)).unwrap()[..PAGE_BYTES],
        input("work1-allocator.after")
    );
    let before_load = inventory(&db);
    let start = trace.lock().unwrap().events.len();
    trace.lock().unwrap().load_reads = true;
    for (page, literal, generation) in [
        (first_page, "work1-status.after", 3),
        (second_page, "work3-status.after", 4),
        (first_page, "work1-status.after", 5),
    ] {
        assert_eq!(
            bound.load_status(page).unwrap().after.generation,
            generation
        );
        let (_, reader, _) = bound.split_with_flush();
        let guard = reader.read(&mut owner).unwrap();
        assert_eq!(guard.bytes().unwrap().as_slice(), input(literal));
        guard.release_checked().unwrap();
    }
    trace.lock().unwrap().load_reads = false;
    assert_eq!(inventory(&db), before_load);
    let reads = trace.lock().unwrap().events[start..].to_vec();
    assert!(reads.iter().all(|line| line.starts_with("read\t")));
    for offset in [16384, 32768] {
        assert!(
            reads
                .iter()
                .any(|line| line == &format!("read\t{HOME}\t{offset}\t16384\t16384"))
        );
    }
    fs::write(output.join("load-reads.tsv"), reads.join("\n") + "\n").unwrap();
    bound
        .split_with_flush()
        .2
        .try_flush_allocator(&mut dedicated)
        .unwrap();
    let home = fs::read(db.join(HOME)).unwrap();
    assert_eq!(&home[..PAGE_BYTES], input("work3-allocator.after"));
    assert_eq!(&home[PAGE_BYTES..2 * PAGE_BYTES], old_home);
    assert_eq!(old_home, input("work1-status.after"));
    assert_eq!(
        &home[2 * PAGE_BYTES..3 * PAGE_BYTES],
        input("work3-status.after")
    );
    assert!(home[3 * PAGE_BYTES..].iter().all(|b| *b == 0));
    assert_eq!(
        fs::read(db.join(wal::WAL_NAME)).unwrap(),
        [header, input("wal.frames-only")].concat()
    );
    let after = inventory(&db);
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    save_inventory(&output.join("after.tsv"), &after);
    assert!(!storage.session_fence().is_fenced());
    drop(bound);
    assert!(storage.session_fence().is_fenced());
    drop(owner);
    drop(dedicated);
    drop(storage);
    let mut t = trace.lock().unwrap();
    sample_fd(&mut t, "drop");
    assert_eq!(t.handles, 0);
    assert_eq!(db_fds(&db), 0);
    assert!(t.fd_peak <= 5);
    fs::write(output.join("io.tsv"), t.events.join("\n") + "\n").unwrap();
    fs::write(output.join("fd.tsv"), t.fd_events.join("\n") + "\n").unwrap();
    fs::write(output.join("summary.txt"), format!("actual_loads=3\nactual_initializations=2\ncancellations=1\nhome_pages_compared=3\nwal_frames_compared=6\nhandles_peak={}\nfds_peak={}\nremaining_handles=0\nremaining_fds=0\nrecovery=NOT_RUN\n", t.peak, t.fd_peak)).unwrap();
}

#[test]
fn status_load_native_resource_gate() {
    let (bytes, metadata, fits) = super::super::super::operational::load_layout::<Writer>();
    println!("native_status_load_call_bytes={bytes} dirty_and_flush_metadata={metadata:?}");
    assert!(fits);
}
