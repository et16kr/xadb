//! A fresh three-frame producer, reopened by the existing recovery worker.
//! Each invocation has its own PID, writer lease, descriptor and FD receipts.
use super::*;

fn literal(pages: usize, name: &str) -> Vec<u8> {
    fs::read(format!(
        "{}/../../tests/fixtures/dev3-resident-status/p1-n{pages}/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn resident_native_resource_gate() {
    let census = crate::storage::wal_io::ResidentStatusBinding::<Writer>::resource_census();
    println!(
        "resident native activation={} wide={} dirty={} whole={} fits={}",
        census.0, census.1, census.2, census.3, census.4
    );
    assert!(census.4);
}

#[test]
#[ignore = "explicit three-frame producer; separate current-build recovery and reentry"]
fn worker_resident_status_native() {
    let out = PathBuf::from(std::env::var_os("XADB_OPERATIONAL_RECOVERY_OUT").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_OPERATIONAL_RECOVERY_DB").unwrap());
    let mode = std::env::var("XADB_OPERATIONAL_RECOVERY_MODE").unwrap();
    assert!(out.is_absolute() && db.is_absolute());
    assert_eq!(mode, "produce");
    println!(
        "OPERATIONAL_RECOVERY_NATIVE_BEGIN mode={mode} pid={}",
        std::process::id()
    );
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    let writer = directory.acquire_writer().unwrap();
    drop(directory);
    let mut wal_slot = boxed::<1048576>();
    let mut workspace = boxed::<4194304>();
    let mut a = boxed::<PAGE_BYTES>();
    let mut b = boxed::<PAGE_BYTES>();
    let mut c = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<114688>();
    let fresh =
        FreshStorageSession::create_status(writer, 1, &mut wal_slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [
        DirtyFrame::new(&mut a),
        DirtyFrame::new(&mut b),
        DirtyFrame::new(&mut c),
    ];
    let binding = storage
        .bind_resident_status(&core, &mut frames, &mut operation)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(value) => value,
        Err(_) => panic!("resident activation"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let a = {
        let (mut writer, _, _) = bound.split_with_flush();
        match writer.reserve_one(&mut owner).unwrap().initialize() {
            Ok(v) => v,
            Err(_) => panic!("INIT A"),
        }
    };
    bound.prepare_slot(2).unwrap();
    let b = {
        let (mut writer, reader, mut flush) = bound.split_with_flush();
        let b = match writer.reserve_one(&mut owner).unwrap().initialize() {
            Ok(v) => v,
            Err(_) => panic!("INIT B"),
        };
        let read = reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<3>(&[b.page, a.page])
            .unwrap();
        assert_eq!(
            read.bytes(0).unwrap().as_slice(),
            literal(2, "allocator.after")
        );
        assert_eq!(
            read.bytes(1).unwrap().as_slice(),
            literal(2, "status-1.after")
        );
        assert_eq!(
            read.bytes(2).unwrap().as_slice(),
            literal(2, "status-2.after")
        );
        read.release_checked().unwrap();
        flush.try_flush_page(&mut dedicated, a.page).unwrap();
        b
    };
    bound.prepare_slot(1).unwrap();
    let c = {
        let (mut writer, _, mut flush) = bound.split_with_flush();
        let c = match writer.reserve_one(&mut owner).unwrap().initialize() {
            Ok(v) => v,
            Err(_) => panic!("INIT C"),
        };
        flush.try_flush_page(&mut dedicated, b.page).unwrap();
        c
    };
    assert_eq!((a.page.page_no, b.page.page_no, c.page.page_no), (1, 2, 3));
    let loaded = bound.load_into(2, a.page).unwrap();
    assert_eq!(loaded.after.generation, 2);
    {
        let (_, reader, _) = bound.split_with_flush();
        let read = reader
            .try_admit(&mut owner)
            .unwrap()
            .read_set::<3>(&[c.page, a.page])
            .unwrap();
        for (position, name) in [
            (0, "allocator.after"),
            (1, "status-1.after"),
            (2, "status-3.after"),
        ] {
            assert_eq!(read.bytes(position).unwrap().as_slice(), literal(3, name));
            fs::write(out.join(name), read.bytes(position).unwrap()).unwrap();
        }
        read.release_checked().unwrap();
    }
    // Current allocator/C remain dirty; only A and B were flushed. The next PID
    // must reconstruct the full history from actual WAL and current home files.
    assert!(!storage.session_fence().is_fenced());
    drop(bound);
    drop(owner);
    drop(dedicated);
    drop(storage);
    let mut t = trace.lock().unwrap();
    t.sample("final");
    assert_eq!((t.handles, db_fds(&db)), (0, 0));
    assert!(t.fd_peak <= 5);
    fs::write(out.join("events.tsv"), t.events.join("\n") + "\n").unwrap();
    fs::write(out.join("fd.tsv"), t.fd_rows.join("\n") + "\n").unwrap();
    fs::write(out.join("resources.tsv"),format!("handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\ncut_hits\t{}\ncut_ordinal\t{}\n",t.peak,t.handles,t.fd_peak,db_fds(&db),t.cut_hits,t.cut_count)).unwrap();
    println!(
        "OPERATIONAL_RECOVERY_NATIVE_PASS mode={mode} pid={}",
        std::process::id()
    );
}
