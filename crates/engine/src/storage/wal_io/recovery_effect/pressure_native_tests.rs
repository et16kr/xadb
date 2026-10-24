//! Automatic N+1 Status residency, original actors and fresh recovery processes.
use super::*;
use crate::storage::wal_io as wal;

const HOME: &str = "d3-data-0000000100000001";

fn literal(name: &str) -> Vec<u8> {
    fs::read(format!(
        "{}/tests/fixtures/dev3-status-pressure/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn pressure_recovery_native_resource_gate() {
    let layout = wal::EmptyTransactions::<Writer>::pressure_test_layout();
    println!("pressure_recovery_native_wrapper={layout:?}");
    assert!(layout.2);
}

fn produce(directory: Directory, out: &Path) {
    let writer = directory.acquire_writer().unwrap();
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut workspace = boxed::<4194304>();
    let mut bytes = vec![[0; PAGE_BYTES]; 3];
    let mut operation = boxed::<114688>();
    let mut bank = boxed::<{ wal::EMPTY_TRANSACTION_BANK_BYTES }>();
    let fresh = FreshStorageSession::create_status(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
        .unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
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
    let mut cursor = live.residency_cursor();
    let actor_b = live.begin(&mut owner, pages[1]).unwrap().actor;
    let _actor_c = live.begin(&mut owner, pages[2]).unwrap().actor;
    let first = live
        .residency_step(&mut owner, pages[0], &mut cursor)
        .unwrap();
    assert_eq!(first.flush_page(), Some(pages[1]));
    live.flush_status(&mut dedicated, pages[1]).unwrap();
    for (page, index) in [(pages[0], 2), (pages[1], 2)] {
        let (actual, stamp, loaded) = live
            .residency_step(&mut owner, page, &mut cursor)
            .unwrap()
            .resident()
            .unwrap();
        assert_eq!((actual, stamp.identity.page, loaded), (index, page, true));
    }
    let mut image = boxed::<PAGE_BYTES>();
    live.load_test_snapshot(2, &mut image);
    assert_eq!(image.as_slice(), literal("step1.page"));
    fs::write(out.join("active-loaded.page"), image.as_slice()).unwrap();
    live.rollback_start(&mut owner, actor_b).unwrap();
    let second = live
        .residency_step(&mut owner, pages[0], &mut cursor)
        .unwrap();
    assert_eq!(second.flush_page(), Some(pages[2]));
    live.flush_status(&mut dedicated, pages[2]).unwrap();
    for (page, index) in [(pages[0], 1), (pages[2], 1)] {
        let (actual, stamp, loaded) = live
            .residency_step(&mut owner, page, &mut cursor)
            .unwrap()
            .resident()
            .unwrap();
        assert_eq!((actual, stamp.identity.page, loaded), (index, page, true));
    }
    // Clean-first selection can retain the dirty B actor. Explicitly service
    // that actor before demanding its eviction; this is not a fairness claim.
    live.flush_status(&mut dedicated, pages[1]).unwrap();
    for (page, index) in [(pages[0], 2), (pages[1], 1)] {
        let (actual, stamp, loaded) = live
            .residency_step(&mut owner, page, &mut cursor)
            .unwrap()
            .resident()
            .unwrap();
        assert_eq!((actual, stamp.identity.page, loaded), (index, page, true));
    }
    live.load_test_snapshot(1, &mut image);
    assert_eq!(image.as_slice(), literal("step3.page"));
    fs::write(out.join("aborting-loaded.page"), image.as_slice()).unwrap();
    live.abort_end(&mut owner, actor_b).unwrap();
    live.load_test_snapshot(1, &mut image);
    assert_eq!(image.as_slice(), literal("step4.page"));
    fs::write(out.join("aborted-dirty.page"), image.as_slice()).unwrap();
    assert_eq!(storage.wal().prefix().unwrap().complete_lsn, 10);
    assert!(!storage.session_fence().is_fenced());
    drop(live);
    assert!(bank[..1664].iter().all(|b| *b == 0));
    assert_eq!(&bank[1664..3200], literal("step2.descriptor").as_slice());
    assert_eq!(bank[1664 + 1624], 1);
    assert!(bank[1664 + 1625..].iter().all(|b| *b == 0));
    fs::write(out.join("retained-bank.bin"), bank.as_slice()).unwrap();
    drop(owner);
    drop(dedicated);
    drop(storage);
}

#[test]
#[ignore = "explicit fresh bounded residency producer/recovery/reentry worker"]
fn worker_status_pressure_native() {
    let out = PathBuf::from(std::env::var_os("XADB_STATUS_PRESSURE_OUT").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_STATUS_PRESSURE_DB").unwrap());
    let mode = std::env::var("XADB_STATUS_PRESSURE_MODE").unwrap();
    assert!(out.is_absolute() && db.is_absolute());
    assert!(matches!(mode.as_str(), "produce" | "recover" | "reentry"));
    println!(
        "STATUS_PRESSURE_NATIVE_BEGIN mode={mode} pid={}",
        std::process::id()
    );
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        recovering: mode != "produce",
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    if mode == "produce" {
        produce(directory, &out);
    } else {
        let mut m = boxed::<12582912>();
        let mut f = boxed::<4194304>();
        m.fill(0xa5);
        f.fill(0x5a);
        let owner = ClosedRecovery::acquire(&directory, None, &mut m, &mut f).unwrap();
        assert_eq!(owner.effects().mutation_attempted, 0);
        let r = match owner.recover() {
            Ok(v) => v,
            Err(e) => panic!("recovery {:?}", e.error()),
        };
        assert_eq!(r.prefix().allocator_lsn(), 6);
        assert_eq!(r.prefix().complete_lsn(), 10);
        assert_eq!(r.prefix().transaction_high_water(), 2);
        assert_eq!(r.prefix().active_transactions(), 1);
        assert_eq!(r.prefix().remaining_wal(), 6080);
        assert!(r.observation().ledger.restored_once);
        if mode == "reentry" {
            assert_eq!(r.effects().home_write_attempted, 0);
        }
        dump(&out, r.prefix(), r.observation());
        fs::write(out.join("allocator.page"), r.allocator_image()).unwrap();
        for page in 1..=3 {
            fs::write(
                out.join(format!("status-{page}.page")),
                r.status_image_at(page).unwrap(),
            )
            .unwrap();
        }
        fs::write(out.join("system.descriptor"), r.descriptor_bytes().unwrap()).unwrap();
        let mut descriptors = 0;
        for index in 0..32 {
            if let Some(d) = r.transaction_descriptor_at(index) {
                assert_eq!(d.as_slice(), literal("step2.descriptor"));
                fs::write(out.join(format!("transaction-{index}.descriptor")), d).unwrap();
                descriptors += 1;
            }
        }
        assert_eq!(descriptors, 1);
        fs::write(
            out.join("transactions.tsv"),
            "highwater\t2\nactive\t1\nallocator_lsn\t6\n",
        )
        .unwrap();
        drop(r);
        drop(directory);
    }
    let mut t = trace.lock().unwrap();
    t.sample("final");
    fs::write(out.join("events.tsv"), t.events.join("\n") + "\n").unwrap();
    fs::write(out.join("fd.tsv"), t.fd_rows.join("\n") + "\n").unwrap();
    fs::write(out.join("resources.tsv"), format!(
        "handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\ncut_hits\t{}\ncut_ordinal\t{}\n",
        t.peak, t.handles, t.fd_peak, db_fds(&db), t.cut_hits, t.cut_count)).unwrap();
    assert_eq!((t.handles, db_fds(&db)), (0, 0));
    assert!(t.fd_peak <= 5);
    assert!(db.join(HOME).exists());
    println!(
        "STATUS_PRESSURE_NATIVE_PASS mode={mode} pid={}",
        std::process::id()
    );
}
