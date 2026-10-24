//! New Linux live-history consumer; existing native selectors are not invoked.
use super::*;

fn input(name: &str) -> Vec<u8> {
    fs::read(format!(
        "{}/tests/fixtures/dev3-empty-transactions/live/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
#[ignore = "explicit fresh Linux live Status history/flush worker"]
fn worker_empty_history_native() {
    let output = PathBuf::from(std::env::var_os("XADB_EMPTY_HISTORY_NATIVE_DIR").expect("output"));
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
    let mut operation = boxed::<114688>();
    let mut bank = boxed::<{ wal::EMPTY_TRANSACTION_BANK_BYTES }>();
    let fresh =
        wal::FreshStorageSession::create_status(port, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [
        DirtyFrame::new(&mut allocator),
        DirtyFrame::new(&mut payload),
    ];
    let binding = storage
        .bind_resident_status(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(v) => v,
        Err(_) => panic!("activate"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let page = match bound
        .split_with_flush()
        .0
        .reserve_one(&mut owner)
        .unwrap()
        .initialize()
    {
        Ok(v) => v.page,
        Err(_) => panic!("INIT"),
    };
    let mut live = match bound.into_empty_transactions(&mut owner, &mut bank) {
        Ok(v) => v,
        Err(_) => panic!("handoff"),
    };
    {
        let mut t = trace.lock().unwrap();
        t.enabled = true;
        t.load_reads = true;
        t.peak = t.handles;
        t.fd_baseline = db_fds(&db);
        t.fd_peak = t.fd_baseline;
        sample_fd(&mut t, "baseline");
    }
    let actor = live.begin(&mut owner, page).unwrap().actor;
    for (step, literal) in [
        "begin.after.page",
        "rollback.after.page",
        "abort.after.page",
    ]
    .into_iter()
    .enumerate()
    {
        if step == 1 {
            live.rollback_start(&mut owner, actor).unwrap();
        }
        if step == 2 {
            live.abort_end(&mut owner, actor).unwrap();
        }
        let prefix = storage.wal().prefix().unwrap();
        let wal_before = fs::read(db.join(wal::WAL_NAME)).unwrap();
        let observation = match live.flush_status(&mut dedicated, page).unwrap() {
            wal::FlushProgress::Complete(v) => v,
            other => panic!("flush {other:?}"),
        };
        assert!(observation.finalized.cleaned);
        assert_eq!(
            (observation.io.attempted, observation.io.completed),
            (31, 31)
        );
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert_eq!(fs::read(db.join(wal::WAL_NAME)).unwrap(), wal_before);
        let home = fs::read(db.join(HOME)).unwrap();
        assert_eq!(&home[PAGE_BYTES..2 * PAGE_BYTES], input(literal));
        fs::write(output.join(literal), &home[PAGE_BYTES..2 * PAGE_BYTES]).unwrap();
    }
    assert_eq!(
        fs::read(db.join(wal::WAL_NAME)).unwrap(),
        input("wal-through-abort.bin")
    );
    assert!(!storage.session_fence().is_fenced());
    drop(live);
    assert!(storage.session_fence().is_fenced());
    assert!(bank.iter().all(|b| *b == 0));
    drop(owner);
    drop(dedicated);
    drop(storage);
    let mut t = trace.lock().unwrap();
    sample_fd(&mut t, "drop");
    assert_eq!(t.handles, 0);
    assert_eq!(db_fds(&db), 0);
    assert!(t.fd_peak <= 5);
    assert!(
        t.events
            .iter()
            .any(|e| e.starts_with(&format!("read\t{}\t4096\t160\t160", wal::WAL_NAME)))
    );
    fs::write(output.join("io.tsv"), t.events.join("\n") + "\n").unwrap();
    fs::write(output.join("fd.tsv"), t.fd_events.join("\n") + "\n").unwrap();
    fs::write(output.join("summary.txt"), format!("actual_live_flushes=3\nfull_pages_compared=3\nwal_frames_compared=5\nhandles_peak={}\nfds_peak={}\nremaining_handles=0\nremaining_fds=0\nclosed_recovery=NOT_RUN\n", t.peak, t.fd_peak)).unwrap();
}

#[test]
fn empty_pressure_native_resource_gate() {
    let layout = wal::EmptyTransactions::<Writer>::pressure_test_layout();
    println!("pressure_native_wrapper={layout:?}");
    assert!(layout.2);
}
