//! New original-owner Free Undo integration. Expected bytes come from the
//! independent fixture; no completed test function supplies setup or an oracle.
use super::super::super::plan4_pages::{
    EmptyRetentionControlLoan, FreeUndoOracle as Oracle, IntoRetentionError, Plan4Page,
    Plan4ResidencyCursor, Plan4Target, RETENTION_CONTROL_BYTES, RETENTION_REPLAY_BYTES,
    RetentionReservationError, RetentionResidencyStep,
};
use super::support::{
    Live, actual_runtime, boxed, drive_flush, finish_closed, page_ref, recovered,
};
use super::*;
use crate::{
    storage::{dirty_frame::DirtyFrame, page::PageKind, wal_io::FreshStorageSession},
    transaction::concurrency::{Owner, OwnerRole, PageConcurrency},
};
use std::{fs, io::Write, path::PathBuf};

fn prepare_slot<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    undo: bool,
) {
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..8 {
        let step = if undo {
            live.prepare_undo_slot(owner, &mut cursor)
        } else {
            live.prepare_status_slot(owner, &mut cursor)
        }
        .unwrap();
        match step {
            None => return,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("slot preparation did not expose an actual flush"),
        }
    }
    panic!("bounded SystemInit slot preparation");
}
fn initialize<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    oracle: &Oracle,
    number: usize,
    undo: bool,
) -> Plan4Target {
    prepare_slot(live, owner, flusher, undo);
    let reserved = if undo {
        live.reserve_undo(owner)
    } else {
        live.reserve_status(owner)
    }
    .unwrap();
    assert_eq!(reserved.page(), page_ref(number as u64, 2));
    let target = match reserved.initialize() {
        Ok(target) => target,
        Err(RetentionReservationError::Before { error, .. })
        | Err(RetentionReservationError::Fatal(error)) => panic!("original SystemInit: {error:?}"),
    };
    assert_eq!(
        target.kind(),
        if undo {
            PageKind::Undo
        } else {
            PageKind::TransactionStatus
        }
    );
    {
        let (_, reader, _) = live.split();
        let guard = reader.read(owner, &target).unwrap();
        assert_eq!(
            guard.bytes().unwrap().as_slice(),
            oracle.native_page(number as u64)
        );
        guard.release_checked().unwrap();
    }
    owner.check_idle().unwrap();
    target
}
fn flush_evict_reload<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
    target: &Plan4Target,
    expected: &[u8],
) {
    drive_flush(live, flusher, Some(target));
    let index = match live
        .residency_step(target, &mut Plan4ResidencyCursor::new())
        .unwrap()
    {
        RetentionResidencyStep::Resident {
            index,
            loaded: false,
            frame,
        } => {
            assert!(frame.rec_lsn.is_none());
            index
        }
        _ => panic!("flushed target must retain original clean registration"),
    };
    let anchor = live.target(Plan4Page::CatalogAnchor).unwrap();
    let evicted = live.load_into(index, &anchor).unwrap();
    assert!(
        evicted.before.is_some(),
        "a real clean registration must be evicted"
    );
    let loaded = live.load_into(index, target).unwrap();
    assert!(loaded.before.is_some());
    assert!(loaded.after.rec_lsn.is_none());
    let (_, reader, _) = live.split();
    let guard = reader.read(owner, target).unwrap();
    assert_eq!(guard.bytes().unwrap().as_slice(), expected);
    guard.release_checked().unwrap();
    owner.check_idle().unwrap();
}
fn produce<D: DirectoryPort>(
    directory: &D,
    read: impl Fn(&str) -> Vec<u8>,
    reserve_only: bool,
    arm: impl FnOnce(),
) {
    let oracle = Oracle::new();
    let mut slot = boxed::<1_048_576>();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let storage = FreshStorageSession::create_retention_catalog(
        directory.acquire_writer().unwrap(),
        1,
        &mut slot,
        &mut maintenance,
    )
    .unwrap()
    .install_doublewrite(&mut dwb)
    .unwrap()
    .into_mutable_storage();
    let core = PageConcurrency::new(storage.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 4];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let mut isolation = boxed::<4_160>();
    let mut control = boxed::<RETENTION_CONTROL_BYTES>();
    let mut replay = boxed::<RETENTION_REPLAY_BYTES>();
    let pages = storage
        .bind_plan4_pages(&core, &mut frames, &mut operation)
        .unwrap();
    let mut live = match pages.into_empty_retention(
        &mut owner,
        EmptyRetentionControlLoan {
            isolation: &mut isolation,
            control: &mut control,
            replay: &mut replay,
        },
    ) {
        Ok(live) => live,
        Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => {
            panic!("original activation: {error:?}")
        }
    };
    initialize(&mut live, &mut owner, &mut flusher, &oracle, 9, false);
    let undo = initialize(&mut live, &mut owner, &mut flusher, &oracle, 10, true);
    flush_evict_reload(
        &mut live,
        &mut owner,
        &mut flusher,
        &undo,
        oracle.native_page(10),
    );
    if reserve_only {
        prepare_slot(&mut live, &owner, &mut flusher, true);
        let reserved = live.reserve_undo(&mut owner).unwrap();
        assert_eq!(reserved.page(), page_ref(11, 2));
        let wal = read("d3-wal-00000001");
        let prefix_len = oracle.native_suffix(1).len() - oracle.native_frame(6).len();
        assert_eq!(&wal[4096..], &oracle.native_suffix(1)[..prefix_len]);
        drop(reserved);
        assert!(storage.session_fence().is_fenced());
        return;
    }
    let undo = initialize(&mut live, &mut owner, &mut flusher, &oracle, 11, true);
    assert_eq!(
        &read("d3-wal-00000001")[4096..],
        oracle.raw("native-wal-e1-suffix.bin")
    );
    arm();
    drive_flush(&mut live, &mut flusher, Some(&undo));
}
fn continuation<D: DirectoryPort>(
    directory: &D,
    read: impl Fn(&str) -> Vec<u8>,
    epoch: u32,
    cut: impl FnOnce(),
) {
    let oracle = Oracle::new();
    recovered!(directory, |runtime,
                           closed,
                           owner,
                           flusher,
                           _core,
                           _backing| {
        assert_eq!(
            runtime.observation().source_lsn,
            if epoch == 2 { 6 } else { 10 }
        );
        finish_closed(&mut closed, &mut owner, &mut flusher);
        let mut live = match closed.into_live() {
            Ok(live) => live,
            Err(_) => panic!("verified recovered owner did not open"),
        };
        let capture_lsn = if epoch == 2 { 6 } else { 10 };
        let last_page = if epoch == 2 { 11 } else { 13 };
        let home = read(HOME_NAME);
        assert_eq!(&home[..PAGE_BYTES], oracle.native_allocator(capture_lsn));
        for number in 1..=8 {
            assert_eq!(
                &home[number * PAGE_BYTES..(number + 1) * PAGE_BYTES],
                oracle.reference(&format!("P{number:02}.bin"))
            );
        }
        for number in 9..=last_page {
            let expected = oracle.native_page(number);
            let offset = number as usize * PAGE_BYTES;
            assert_eq!(&home[offset..offset + PAGE_BYTES], expected);
            let target = if matches!(number, 9 | 12) {
                live.status_target(page_ref(number, 2)).unwrap()
            } else {
                live.undo_target(page_ref(number, 2)).unwrap()
            };
            let mut cursor = Plan4ResidencyCursor::new();
            match live.residency_step(&target, &mut cursor).unwrap() {
                RetentionResidencyStep::Resident { .. } => {}
                RetentionResidencyStep::FlushRequired { .. } => {
                    panic!("recovered canonical pages must be clean before new writes")
                }
            }
            let (_, reader, _) = live.split();
            let guard = reader.read(&mut owner, &target).unwrap();
            assert_eq!(guard.bytes().unwrap().as_slice(), expected);
            guard.release_checked().unwrap();
        }
        let bank = live.recovery_test_bank();
        assert!(bank[..33 * 1664].iter().all(|byte| *byte == 0));
        assert_eq!(
            &bank[33 * 1664..33 * 1664 + 1536],
            oracle.native_descriptor(capture_lsn)
        );
        assert_eq!(
            u64::from_le_bytes(bank[33 * 1664 + 1536..33 * 1664 + 1544].try_into().unwrap()),
            if epoch == 2 { 3 } else { 5 }
        );
        assert_eq!(bank[33 * 1664 + 1544], 3);
        let (status_no, undo_no) = if epoch == 2 { (12, 13) } else { (14, 15) };
        initialize(
            &mut live,
            &mut owner,
            &mut flusher,
            &oracle,
            status_no,
            false,
        );
        let undo = initialize(&mut live, &mut owner, &mut flusher, &oracle, undo_no, true);
        let final_lsn = if epoch == 2 { 10 } else { 14 };
        assert_eq!(
            &read(&format!("d3-wal-{epoch:08x}"))[4096..],
            oracle.raw(&format!("native-wal-e{epoch}-suffix.bin"))
        );
        match live
            .residency_step(&undo, &mut Plan4ResidencyCursor::new())
            .unwrap()
        {
            RetentionResidencyStep::Resident {
                frame,
                loaded: false,
                ..
            } => {
                assert!(frame.rec_lsn.is_some());
                assert_eq!(frame.page_lsn.0, final_lsn);
                assert_eq!(frame.rec_lsn.unwrap().0, final_lsn);
            }
            _ => panic!("no-force INIT must remain in its original dirty frame"),
        }
        let (prefix, durable, reservation, installation, pending, claim, pins) =
            live.free_undo_test_state().unwrap();
        assert_eq!(
            (prefix.complete_lsn, prefix.synced_lsn, durable),
            (final_lsn, final_lsn, Some(final_lsn))
        );
        assert!(!reservation && !installation && !pending && !claim);
        assert_eq!(pins, 0);
        // The cut is after the actual successful INIT receipt; it must not be
        // moved before registration/durable/claim completion to fit a trace.
        cut();
        flush_evict_reload(
            &mut live,
            &mut owner,
            &mut flusher,
            &undo,
            oracle.native_page(undo_no as u64),
        );
        owner.check_idle().unwrap();
        flusher.check_idle().unwrap();
        assert!(!runtime.session_fence().is_fenced());
    });
}

#[test]
fn free_undo_recovery_source_admission() {
    // A new assertion of changed source liveness, without running old cases.
    super::static_source::<port::Writer>("free-undo-memory");
    #[cfg(target_os = "linux")]
    super::static_source::<super::super::super::plan4_pages::native::Writer>("free-undo-native");
}

#[test]
fn free_undo_reserved_recovery_uses_original_scope_cancel() {
    let original = port::Directory::fresh();
    original.state().lock().unwrap().live_writes = true;
    let state = original.state();
    produce(
        &original,
        |name| super::support::file(&state, name),
        true,
        || {},
    );
    super::support::assert_closed(&state);
    let directory = super::support::clone_directory(&super::support::images(&state));
    let state = directory.state();
    let oracle = Oracle::new();
    recovered!(&directory, |runtime,
                            closed,
                            owner,
                            flusher,
                            _core,
                            _backing| {
        assert_eq!(runtime.observation().source_lsn, 5);
        finish_closed(&mut closed, &mut owner, &mut flusher);
        let mut live = match closed.into_live() {
            Ok(live) => live,
            Err(_) => panic!("reserved recovery did not finish its original cancel"),
        };
        let wal = super::support::file(&state, "d3-wal-00000002");
        assert_eq!(&wal[4096..], oracle.raw("reserved-cancel-l6.wal"));
        let bank = live.recovery_test_bank();
        assert_eq!(
            &bank[33 * 1664..33 * 1664 + 1536],
            &oracle.raw("reserved-cancel-l6.wal")[352..1888]
        );
        assert_eq!(bank[33 * 1664 + 1544], 1);
        drive_flush(&mut live, &mut flusher, None);
        let home = super::support::file(&state, HOME_NAME);
        assert_eq!(&home[..PAGE_BYTES], oracle.raw("reserved-cancel-l6.page"));
        assert_eq!(
            &home[10 * PAGE_BYTES..11 * PAGE_BYTES],
            oracle.native_page(10)
        );
        let (prefix, durable, reservation, installation, pending, claim, pins) =
            live.free_undo_test_state().unwrap();
        assert_eq!(
            (prefix.complete_lsn, prefix.synced_lsn, durable),
            (6, 6, Some(6))
        );
        assert!(!reservation && !installation && !pending && !claim && pins == 0);
    });
    super::support::assert_closed(&state);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "explicit same-ELF SIGKILL/rebase/old-WAL-discard/recrash controller"]
fn free_undo_three_process_worker() {
    use super::super::super::plan4_pages::native;
    let phase = std::env::var("XADB_FREE_UNDO_NATIVE_PHASE").unwrap();
    assert!(matches!(phase.as_str(), "produce" | "recover" | "reentry"));
    let root = PathBuf::from(std::env::var("XADB_FREE_UNDO_NATIVE_DB").unwrap());
    let output = PathBuf::from(std::env::var("XADB_FREE_UNDO_NATIVE_OUTPUT").unwrap());
    assert!(root.is_absolute() && output.is_absolute() && root != output);
    let (directory, trace) = native::open_native(
        &root,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let read = |name: &str| {
        assert!(!name.contains('/'));
        fs::read(root.join(name)).unwrap()
    };
    let marker = output.join("cut-reached.tsv");
    match phase.as_str() {
        "produce" => produce(&directory, read, false, || {
            native::arm_undo_home_cut(
                &trace,
                &marker,
                content_digest(Oracle::new().native_page(11)),
            );
        }),
        "recover" => continuation(&directory, read, 2, || {
            assert!(!root.join("d3-wal-00000001").exists());
            native::crash_barrier(&trace, &marker, "A_FU_AFTER_UNDO_DURABLE");
        }),
        "reentry" => {
            assert!(!root.join("d3-wal-00000001").exists());
            continuation(&directory, read, 3, || {});
        }
        _ => unreachable!(),
    }
    assert_eq!(
        phase, "reentry",
        "crash phase returned without external SIGKILL"
    );
    drop(directory);
    let r = native::native_receipt(&trace);
    assert_eq!(r.handles, 0);
    assert!(r.peak <= 3 && r.fd_peak <= 6 && r.log_bytes <= 4 * 1024 * 1024);
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("worker.tsv"))
        .unwrap();
    writeln!(
        out,
        "pid\t{}\nhandles\t{}\npeak\t{}\nfd_peak\t{}\nlog_bytes\t{}",
        std::process::id(),
        r.handles,
        r.peak,
        r.fd_peak,
        r.log_bytes
    )
    .unwrap();
    out.sync_all().unwrap();
    println!("FREE_UNDO_NATIVE_COMPLETE\t{}", std::process::id());
}
