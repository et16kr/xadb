//! Actual current-build empty producer and separate-process recovery workers.
use super::*;
use crate::storage::wal_io as wal;

fn produce(directory: Directory, depth: usize) {
    let writer = directory.acquire_writer().unwrap();
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut workspace = boxed::<4194304>();
    let mut a = boxed::<PAGE_BYTES>();
    let mut s = boxed::<PAGE_BYTES>();
    let mut op = boxed::<114688>();
    let mut bank = boxed::<{ wal::EMPTY_TRANSACTION_BANK_BYTES }>();
    let fresh = FreshStorageSession::create_status(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
        .unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
    let binding = storage
        .bind_resident_status(&core, &mut frames, &mut op)
        .unwrap();
    let mut bound = match binding.activate() {
        Ok(v) => v,
        Err(_) => panic!("resident activation"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let page = match bound
        .split_with_flush()
        .0
        .reserve_one(&mut owner)
        .unwrap()
        .initialize()
    {
        Ok(v) => v.page,
        Err(_) => panic!("Status INIT"),
    };
    let mut live = match bound.into_empty_transactions(&mut owner, &mut bank) {
        Ok(v) => v,
        Err(_) => panic!("empty handoff"),
    };
    let actor = live.begin(&mut owner, page).unwrap().actor;
    if depth >= 2 {
        live.rollback_start(&mut owner, actor).unwrap();
    }
    if depth == 3 {
        live.abort_end(&mut owner, actor).unwrap();
    }
    // Each live transition returned only after its actual WAL requirement was
    // durable. Neither allocator nor Status home has been flushed in this PID.
    assert!(!storage.session_fence().is_fenced());
    drop(live);
    drop(owner);
    drop(storage);
}

#[test]
#[ignore = "explicit fresh producer/recovery/reentry and actual fault worker"]
fn worker_empty_recovery_native() {
    let out = PathBuf::from(std::env::var_os("XADB_EMPTY_RECOVERY_OUT").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_EMPTY_RECOVERY_DB").unwrap());
    let mode = std::env::var("XADB_EMPTY_RECOVERY_MODE").unwrap();
    let depth: usize = std::env::var("XADB_EMPTY_RECOVERY_DEPTH")
        .unwrap()
        .parse()
        .unwrap();
    let fault = std::env::var("XADB_EMPTY_RECOVERY_FAULT").unwrap_or_default();
    assert!(out.is_absolute() && db.is_absolute() && (1..=3).contains(&depth));
    println!(
        "EMPTY_RECOVERY_NATIVE_BEGIN mode={mode} depth={depth} pid={}",
        std::process::id()
    );
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        recovering: mode != "produce",
        fault: fault.clone(),
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    if mode == "produce" {
        produce(directory, depth);
    } else {
        let mut m = boxed::<12582912>();
        let mut f = boxed::<4194304>();
        m.fill(0xa5);
        f.fill(0x5a);
        let owner = ClosedRecovery::acquire(&directory, None, &mut m, &mut f).unwrap();
        assert_eq!(owner.effects().mutation_attempted, 0);
        match owner.recover() {
            Ok(r) => {
                assert!(mode == "recover" || mode == "reentry");
                assert_eq!(r.prefix().allocator_lsn(), 2);
                assert_eq!(r.prefix().complete_lsn(), (2 + depth) as u64);
                assert_eq!(r.prefix().transaction_high_water(), 1);
                assert_eq!(r.prefix().active_transactions(), u32::from(depth != 3));
                assert_eq!(r.prefix().remaining_wal(), (3 - depth) as u64 * 3040);
                assert!(r.observation().ledger.restored_once);
                if mode == "reentry" {
                    assert_eq!(r.effects().home_write_attempted, 0);
                }
                dump(&out, r.prefix(), r.observation());
                fs::write(out.join("allocator.page"), r.allocator_image()).unwrap();
                fs::write(out.join("status.page"), r.status_image_at(1).unwrap()).unwrap();
                fs::write(out.join("system.descriptor"), r.descriptor_bytes().unwrap()).unwrap();
                if let Some(d) = r.transaction_descriptor_at(0) {
                    fs::write(out.join("transaction.descriptor"), d).unwrap();
                }
                fs::write(
                    out.join("transactions.tsv"),
                    format!(
                        "highwater\t{}\nactive\t{}\nallocator_lsn\t{}\n",
                        r.prefix().transaction_high_water(),
                        r.prefix().active_transactions(),
                        r.prefix().allocator_lsn()
                    ),
                )
                .unwrap();
                drop(r);
            }
            Err(e) => {
                assert_eq!(mode, "fail", "{:?}", e.error());
                assert_eq!(
                    e.error().effect,
                    foundation::error::Effect::CallerMustResolve
                );
                assert_eq!(e.error().phase, super::super::RecoveryPhase::Flush);
                assert!(e.observation().ledger.restored_once);
                assert_eq!(e.prefix().remaining_wal(), (3 - depth) as u64 * 3040);
                if fault == "empty_status_slot_partial" {
                    assert_eq!(e.effects().home_write_attempted & 2, 0);
                    assert_eq!(
                        (e.observation().known.generation, e.observation().batch),
                        (5, 2)
                    );
                } else {
                    assert_ne!(e.effects().home_write_attempted & 2, 0);
                }
                dump(&out, e.prefix(), e.observation());
                dump_error(&out, e.error(), e.effects());
                drop(e);
            }
        }
        drop(directory);
    }
    let mut t = trace.lock().unwrap();
    t.sample("final");
    fs::write(out.join("events.tsv"), t.events.join("\n") + "\n").unwrap();
    fs::write(out.join("fd.tsv"), t.fd_rows.join("\n") + "\n").unwrap();
    fs::write(
        out.join("resources.tsv"),
        format!(
            "handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\ncut_hits\t{}\ncut_ordinal\t{}\n",
            t.peak,
            t.handles,
            t.fd_peak,
            db_fds(&db),
            t.cut_hits, t.cut_count
        ),
    )
    .unwrap();
    assert_eq!((t.handles, db_fds(&db)), (0, 0));
    assert!(t.fd_peak <= 5);
    if mode == "fail" {
        assert_eq!(t.cut_hits, 1);
    }
    println!(
        "EMPTY_RECOVERY_NATIVE_PASS mode={mode} depth={depth} pid={}",
        std::process::id()
    );
}
