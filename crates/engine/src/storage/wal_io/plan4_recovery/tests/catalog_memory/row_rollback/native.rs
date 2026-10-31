//! Same-ELF, separate-process native Commit and current restart gate.
use super::super::super::super::super::plan4_pages::native;
use super::super::super::support::namespace;
use super::super::row_forward::VALUE;
use super::*;
use crate::storage::row::insert::driver::live::{LiveCall, insert_live};
use std::{fs, path::Path};

#[test]
#[ignore = "bounded S4 native controller supplies isolated paths and identity"]
fn s4_n01_native_worker() {
    let env = |name| std::env::var(format!("XADB_S4_NATIVE_{name}")).unwrap();
    let phase = env("PHASE");
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    assert!(matches!(phase.as_str(), "produce" | "restart"));
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf));
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        phase == "restart",
    );
    if phase == "produce" {
        produce(&directory, &db);
    } else {
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let mut slot = boxed::<1_048_576>();
        let failed = match Plan4Recovery::acquire(
            &directory,
            Some(namespace()),
            &mut maintenance,
            &mut dwb,
            &mut slot,
        ) {
            Err(Plan4RecoveryAcquireFailure::Acquired(failed)) => failed,
            Err(Plan4RecoveryAcquireFailure::Before { error, .. }) => {
                panic!("prelease failure {error:?}")
            }
            Ok(owner) => match owner.recover() {
                Err(failed) => failed,
                Ok(_) => panic!("S6 READY not supplied"),
            },
        };
        assert_eq!(
            (
                failed.error().phase,
                failed.error().database_effect,
                failed.error().effect
            ),
            (
                Plan4RecoveryPhase::Analysis,
                Effect::Unchanged,
                Effect::CallerMustResolve
            )
        );
        assert!(matches!(
            failed.error().cause,
            Plan4RecoveryCause::Unsupported
        ));
        assert_eq!(
            (
                failed.observation().mutations_attempted,
                failed.observation().mutations_completed
            ),
            (0, 0)
        );
        println!(
            "S4_N01_CURRENT_GATE {:?} {:?}",
            failed.error(),
            failed.observation()
        );
        drop(failed);
    }
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    if phase == "restart" {
        assert_eq!((receipt.mutations, receipt.entropy), (0, 0));
    }
    fs::write(output.join("receipt.tsv"),format!("phase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\nhandles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\n",std::process::id(),receipt.handles,receipt.peak,receipt.fd_peak,receipt.mutations,receipt.log_bytes)).unwrap();
}

fn produce<D: DirectoryPort>(directory: &D, db: &Path) {
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
    let mut backing = vec![[0xa7; PAGE_BYTES]; 12];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let (prefix, visibility) = maintenance.split_at_mut(12_574_720);
    let (prefix, private) = prefix.split_at_mut(12_148_736);
    let (prefix, isolation) = prefix.split_at_mut(12_144_576);
    let (prefix, epochs) = prefix.split_at_mut(12_142_976);
    let (prefix, control) = prefix.split_at_mut(6_815_744);
    let (reader_private, prefix) = prefix.split_at_mut(PRIVATE_BYTES);
    let replay: &mut [u8; RETENTION_REPLAY_BYTES] = (&mut prefix
        [4_194_304 - PRIVATE_BYTES..5_767_168 - PRIVATE_BYTES])
        .try_into()
        .unwrap();
    let control: &mut [u8; RETENTION_CONTROL_BYTES] = (&mut control[..RETENTION_CONTROL_BYTES])
        .try_into()
        .unwrap();
    let private: &mut [u8; PRIVATE_BYTES] = private.try_into().unwrap();
    let reader_private: &mut [u8; PRIVATE_BYTES] = reader_private.try_into().unwrap();
    let pages = storage
        .bind_plan4_pages(&core, &mut frames, &mut operation)
        .unwrap();
    let mut live = match pages.into_empty_retention(
        &mut owner,
        EmptyRetentionControlLoan {
            control,
            replay,
            isolation: isolation.try_into().unwrap(),
        },
    ) {
        Ok(live) => live,
        Err(IntoRetentionError::Before { error, .. }) | Err(IntoRetentionError::Fatal(error)) => {
            panic!("{error:?}")
        }
    };
    live.enable_catalog_visibility(
        &mut owner,
        VisibilityLoan {
            bytes: visibility.try_into().unwrap(),
            epochs: epochs.try_into().unwrap(),
        },
    )
    .unwrap();
    let root = live.target(Plan4Page::Root).unwrap();
    assert!(matches!(
        live.residency_step(&root, &mut Plan4ResidencyCursor::new())
            .unwrap(),
        RetentionResidencyStep::Resident { .. }
    ));
    assert_eq!(
        live.catalog_aging_start(&mut owner, private)
            .unwrap()
            .header
            .lsn,
        1
    );
    let status = initialize(&mut live, &mut owner, &mut flusher, false);
    let undo = initialize(&mut live, &mut owner, &mut flusher, true);
    let (row_status, _) = create_table(
        &mut live,
        &mut owner,
        &mut flusher,
        &status,
        &undo,
        private,
        true,
    );
    let (mut writer, reader, mut flush) = live.split();
    let tx = writer
        .begin_row_transaction(
            &mut owner,
            IsolationLevel::Serializable,
            &row_status,
            private,
            &mut flush,
        )
        .unwrap();
    let view = writer.begin_statement(&mut owner, &tx).unwrap();
    let owner = &mut owner;
    let flusher = &mut flusher;
    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
    let _ = settle!(writer, owner, view, table, private, flush, flusher);
    let rows = [ValueRef::Int64(VALUE)];
    let input: [&[ValueRef<'_>]; 1] = [&rows];
    let mut output = [None];
    {
        let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
        assert!(matches!(
            insert_live(&mut call, view, &table, private, &input, &mut output),
            Ok(1)
        ));
    }
    let before = fs::metadata(db.join("d3-wal-00000001")).unwrap().len() as usize;
    {
        let mut rollback = writer
            .start_row_rollback(
                owner,
                &tx,
                None,
                RollbackScope::Transaction,
                private,
                &mut flush,
                flusher,
            )
            .unwrap();
        assert_eq!(drain_rollback!(rollback), 1);
        applied!(rollback.finish());
    }
    assert!(writer.finish_aborted(owner, tx).is_ok());
    let wal = read(&db.join("d3-wal-00000001"), 4 * 1024 * 1024);
    assert_eq!(&wal[before..], ROLLBACK_FRAME);
    assert_eq!(reader.catalog_test_horizon().0, 1);
    assert_eq!(reader.catalog_test_source_used(), 0);
    let prefix = writer.free_undo_test_state().unwrap().0;
    assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (19, 19));
    assert_eq!(
        hex(writer.catalog_test_jobs().as_flattened()),
        "e09396dd03843250647b07db68ecb5eb27d43dbe2b33b0bd5984aef8010a80c3"
    );
    assert!(!storage.session_fence().is_fenced());
    println!(
        "S4_N01_ABORT complete=19 synced=19 rollback_sha256={} wal_bytes={}",
        hex(ROLLBACK_FRAME),
        wal.len()
    );
}

const ROLLBACK_FRAME: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-module-c3/row-rollback/rollback.bin"
));
