//! Actual Linux disk-input to resident-image equality through occupied reload.
use super::*;

#[test]
#[ignore = "explicit fresh Linux live Status load worker"]
fn worker_empty_load_native() {
    let layout = wal::EmptyTransactions::<Writer>::load_test_layout();
    println!("native_load_layout={layout:?}");
    assert!(layout.2);
    let output = PathBuf::from(std::env::var_os("XADB_EMPTY_LOAD_NATIVE_DIR").expect("output"));
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
    let mut bytes = vec![[0; PAGE_BYTES]; 3];
    let mut operation = boxed::<114688>();
    let mut bank = boxed::<{ wal::EMPTY_TRANSACTION_BANK_BYTES }>();
    let fresh =
        wal::FreshStorageSession::create_status(port, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let storage = installed.into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
    let binding = storage
        .bind_resident_status(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(v) => v,
        Err(_) => panic!("activate"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut pages = Vec::new();
    for index in [1, 2, 1] {
        if !pages.is_empty() {
            bound.prepare_slot(index).unwrap();
        }
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
        bound
            .split_with_flush()
            .2
            .try_flush_page(&mut dedicated, page)
            .unwrap();
        pages.push(page);
    }
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
    let actor = live.begin(&mut owner, pages[1]).unwrap().actor;
    for step in 0..3 {
        if step == 1 {
            live.rollback_start(&mut owner, actor).unwrap();
        }
        if step == 2 {
            live.abort_end(&mut owner, actor).unwrap();
        }
        live.flush_status(&mut dedicated, pages[1]).unwrap();
        let home = fs::read(db.join(HOME)).unwrap();
        let expected = home[2 * PAGE_BYTES..3 * PAGE_BYTES].to_vec();
        let before = inventory(&db);
        let index = if step % 2 == 0 { 2 } else { 1 };
        let other = if index == 2 { 1 } else { 2 };
        let start = trace.lock().unwrap().events.len();
        live.load_into(
            &mut owner,
            index,
            if step % 2 == 0 { pages[0] } else { pages[2] },
        )
        .unwrap();
        live.load_into(&mut owner, other, pages[1]).unwrap();
        let mut observed = boxed::<PAGE_BYTES>();
        let stamp = live.load_test_snapshot(other, &mut observed);
        assert_eq!(stamp.identity.page, pages[1]);
        assert_eq!(stamp.rec_lsn, None);
        assert_eq!(observed.as_slice(), expected);
        assert_eq!(inventory(&db), before);
        fs::write(output.join(format!("step{step}-input.page")), &expected).unwrap();
        fs::write(
            output.join(format!("step{step}-loaded.page")),
            observed.as_slice(),
        )
        .unwrap();
        let mut t = trace.lock().unwrap();
        let reads = &t.events[start..];
        assert!(reads.iter().all(|e| e.starts_with("read\t")));
        assert!(
            reads
                .iter()
                .any(|e| e == &format!("read\t{HOME}\t32768\t16384\t16384"))
        );
        fs::write(
            output.join(format!("step{step}-load.tsv")),
            reads.join("\n") + "\n",
        )
        .unwrap();
        fs::write(
            output.join(format!("step{step}-io.tsv")),
            t.events.join("\n") + "\n",
        )
        .unwrap();
        t.events.clear();
    }
    assert!(!storage.session_fence().is_fenced());
    drop(live);
    assert!(bank.iter().all(|b| *b == 0));
    drop(owner);
    drop(dedicated);
    drop(storage);
    let mut t = trace.lock().unwrap();
    sample_fd(&mut t, "drop");
    assert_eq!(t.handles, 0);
    assert_eq!(db_fds(&db), 0);
    assert!(t.fd_peak <= 5);
    fs::write(output.join("fd.tsv"), t.fd_events.join("\n") + "\n").unwrap();
    fs::write(output.join("summary.txt"), format!("actual_loads=6\noccupied_pages_compared=3\nhandles_peak={}\nfds_peak={}\nremaining_handles=0\nremaining_fds=0\nclosed_recovery=NOT_RUN\n", t.peak, t.fd_peak)).unwrap();
}
