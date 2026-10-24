//! New actual repeated producer and reopen/fault workers, sharing the Linux port.
use super::*;
fn input(name: &str) -> Vec<u8> {
    fs::read(format!(
        "{}/../../tests/fixtures/dev3-operational-status/p1/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn produce_repeated(directory: Directory, tail: bool) {
    let writer = directory.acquire_writer().unwrap();
    let trace = Arc::clone(&directory.trace);
    drop(directory);
    let mut slot = boxed::<1048576>();
    let mut workspace = boxed::<4194304>();
    let mut a = boxed::<PAGE_BYTES>();
    let mut s = boxed::<PAGE_BYTES>();
    let mut op = boxed::<114688>();
    let fresh = FreshStorageSession::create_status(writer, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
        .unwrap();
    let storage = fresh
        .install_doublewrite(&mut workspace)
        .unwrap()
        .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut frames = [DirtyFrame::new(&mut a), DirtyFrame::new(&mut s)];
    let closed = storage
        .bind_status_bootstrap(&core, &mut frames, &mut op)
        .unwrap();
    let flush = match closed.into_with_flush() {
        Ok(v) => v,
        Err(_) => panic!("flush owner"),
    };
    let mut bound = match flush.into_operational() {
        Ok(v) => v,
        Err(_) => panic!("operational owner"),
    };
    let mut owner = core.try_owner(OwnerRole::Maintenance).unwrap();
    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    {
        let (mut writer, reader, mut flush) = bound.split_with_flush();
        assert!(writer.reserve_one(&mut owner).unwrap().initialize().is_ok());
        let read = reader.read(&mut owner).unwrap();
        assert_eq!(
            read.bytes().unwrap().as_slice(),
            input("work1-status.after")
        );
        read.release_checked().unwrap();
        flush.try_flush_status(&mut dedicated).unwrap();
        flush.try_flush_allocator(&mut dedicated).unwrap();
    }
    bound.prepare_next_status().unwrap();
    {
        let (mut writer, _, _) = bound.split_with_flush();
        assert!(writer.reserve_one(&mut owner).unwrap().cancel().is_ok());
        let reservation = writer.reserve_one(&mut owner).unwrap();
        if tail {
            trace.lock().unwrap().tail_cut = true;
        }
        let result = reservation.initialize();
        if tail {
            match result {
                Err(crate::storage::wal_io::InitAttemptError::Fatal(error)) => {
                    let root = trace.lock().unwrap().root.clone();
                    fs::write(
                        root.parent().unwrap().join("producer-tail-error.txt"),
                        format!("{error:?}\n"),
                    )
                    .unwrap();
                }
                _ => panic!("missing partial INIT failure"),
            }
        } else {
            match result {
                Ok(value) => assert_eq!(value.lsn.0, 6),
                Err(_) => panic!("second actual INIT"),
            }
        }
    }
    drop(bound);
    drop(owner);
    drop(dedicated);
    drop(storage);
}
#[test]
fn operational_native_resource_gate() {
    let c = super::super::resources::source_census::<Writer>();
    println!(
        "operational native census outer={} prefix={} inventory={} work={} peak={} writer={} file={}",
        c.outer,
        c.prefix_nested,
        c.inventory,
        c.work_nested,
        c.peak,
        std::mem::size_of::<Writer>(),
        std::mem::size_of::<File>()
    );
    println!(
        "lifetime phases: borrowed_outer={} entry={} work={} return={} peak={}",
        c.outer,
        c.entry_phase,
        c.outer + c.work_nested.max(c.inventory),
        c.return_phase,
        c.peak
    );
    assert!(super::super::resources::fits::<Writer>(&c));
}
#[test]
#[ignore = "explicit repeated current-build producer/recovery in separate processes"]
fn worker_operational_recovery_native() {
    let out = PathBuf::from(std::env::var_os("XADB_OPERATIONAL_RECOVERY_OUT").unwrap());
    let db = PathBuf::from(std::env::var_os("XADB_OPERATIONAL_RECOVERY_DB").unwrap());
    let mode = std::env::var("XADB_OPERATIONAL_RECOVERY_MODE").unwrap();
    let fault = std::env::var("XADB_OPERATIONAL_RECOVERY_FAULT").unwrap_or_default();
    assert!(out.is_absolute() && db.is_absolute());
    let trace = Arc::new(Mutex::new(Trace {
        root: db.clone(),
        recovering: !mode.starts_with("produce"),
        fault: fault.clone(),
        ..Trace::default()
    }));
    let directory = Directory {
        inner: LinuxDirectory::open_existing(&db).unwrap(),
        trace: Arc::clone(&trace),
    };
    println!(
        "OPERATIONAL_RECOVERY_NATIVE_BEGIN mode={mode} pid={}",
        std::process::id()
    );
    if mode.starts_with("produce") {
        produce_repeated(directory, mode == "produce_tail");
    } else {
        let mut m = boxed::<12582912>();
        let mut f = boxed::<4194304>();
        m.fill(0xa5);
        f.fill(0x5a);
        let owner = ClosedRecovery::acquire(&directory, None, &mut m, &mut f).unwrap();
        match owner.recover() {
            Ok(r) => {
                assert_eq!(mode, "recover");
                dump(&out, r.prefix(), r.observation());
                fs::write(out.join("allocator.bin"), r.allocator_image()).unwrap();
                fs::write(out.join("descriptor.bin"), r.descriptor_bytes().unwrap()).unwrap();
                for index in 1..64 {
                    if let Some(page) = r.status_image_at(index) {
                        fs::write(out.join(format!("status-{index}.bin")), page).unwrap();
                    }
                }
                let p = &r.observation().physical;
                fs::write(
                    out.join("all-targets.txt"),
                    format!(
                        "mask={}\ninitial={:?}\nfinal={:?}\napplied={:?}\nskipped={:?}\n",
                        r.prefix().status_page_mask(),
                        p.initial_lsn,
                        p.final_lsn,
                        p.applied,
                        p.skipped
                    ),
                )
                .unwrap();
                drop(r);
            }
            Err(failure) => {
                assert_eq!(mode, "fail", "{:?}", failure.error());
                use super::super::{CallKind, RecoveryCause, RecoveryPhase};
                assert_eq!(failure.error().phase, RecoveryPhase::Flush);
                assert_eq!(
                    failure.error().effect,
                    xadb_foundation::error::Effect::CallerMustResolve
                );
                let o = failure.observation();
                assert_eq!(
                    (o.known.generation, o.batch, o.state),
                    (
                        12,
                        4,
                        crate::storage::doublewrite_control::ControlState::Pending
                    )
                );
                let call = failure.effects().last_call.unwrap();
                if fault == "operational_b_partial" {
                    assert!(
                        matches!(&failure.error().cause,RecoveryCause::Io(e) if e.code==xadb_foundation::error::ErrorCode::IoWriteUnknown)
                    );
                    assert_eq!(
                        (call.kind, call.name, call.offset, call.length),
                        (CallKind::Write, "d3-data-0000000100000001", 32768, 16384)
                    );
                    assert_eq!(
                        o.possible.map(|v| v.map(|s| s.generation)),
                        [Some(12), None]
                    );
                } else {
                    assert_eq!(fault, "operational_b_retirement_after");
                    assert!(
                        matches!(&failure.error().cause,RecoveryCause::Namespace(e) if e.kind==xadb_foundation::namespace::NamespaceErrorKind::Io && e.effect==xadb_foundation::error::Effect::CallerMustResolve)
                    );
                    assert_eq!(
                        (call.kind, call.name, call.offset, call.length),
                        (CallKind::Publish, "DWB", 0, 4096)
                    );
                    assert_eq!(
                        o.possible.map(|v| v.map(|s| s.generation)),
                        [Some(12), Some(13)]
                    );
                }
                dump(&out, failure.prefix(), failure.observation());
                dump_error(&out, failure.error(), failure.effects());
                drop(failure);
            }
        }
        drop(directory);
    }
    let mut t = trace.lock().unwrap();
    t.sample("final");
    assert_eq!(t.handles, 0);
    assert_eq!(db_fds(&db), 0);
    assert!(t.fd_peak <= 5);
    fs::write(out.join("events.tsv"), t.events.join("\n") + "\n").unwrap();
    fs::write(out.join("fd.tsv"), t.fd_rows.join("\n") + "\n").unwrap();
    fs::write(out.join("resources.tsv"),format!("handles_peak\t{}\nhandles_final\t{}\nfd_peak\t{}\nfd_final\t{}\ncut_hits\t{}\ncut_ordinal\t{}\n",t.peak,t.handles,t.fd_peak,db_fds(&db),t.cut_hits,t.cut_count)).unwrap();
    println!(
        "OPERATIONAL_RECOVERY_NATIVE_PASS mode={mode} pid={}",
        std::process::id()
    );
}
