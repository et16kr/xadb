//! S9 C-only reopen (boundaries `#row-mi03-s9`): C's recovery to READY with
//! `RejectInitialLosers` and no Row loser handler, then a caller-supplied
//! step on the READY Live. C+B has no A, so a RowClr action is a failure
//! here; the module links neither A nor B and is in both single-source
//! exports.
use super::super::super::{CATALOG_PRIVATE_BYTES, Plan4RecoveryAcquireFailure, RowRecoveryPolicy};
use super::super::faults::unchanged_error;
use super::super::support::namespace;
use super::*;
use crate::storage::disk::RecoveryAction;
use std::{fs, path::Path};

/// The step a reopen runs on the READY Live.
pub(super) trait AtReady {
    type Out;
    fn at_ready<'s, 'c, W: WriterNamespace>(
        &mut self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        private: &mut [u8; CATALOG_PRIVATE_BYTES],
    ) -> Self::Out;
}

/// C's recovery of `directory` with `RejectInitialLosers` to READY (every
/// action a Progress, none a RowClr), then `at` on the READY Live. Returns
/// its output and the recovery observation's READY Q.
pub(super) fn reopen<D: DirectoryPort, R: AtReady>(directory: &D, at: &mut R) -> R::Out {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let owner = match Plan4Recovery::acquire(
        directory,
        Some(namespace()),
        &mut maintenance,
        &mut dwb,
        &mut slot,
    ) {
        Ok(owner) => owner,
        Err(_) => panic!("acquire"),
    };
    let runtime = match owner.recover_with_rows(RowRecoveryPolicy::RejectInitialLosers) {
        Ok(runtime) => runtime,
        Err(failed) => panic!("C+B recovery refused {:?}", failed.error()),
    };
    let core = PageConcurrency::new(runtime.session_fence()).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
    let mut backing = vec![[0xa7; PAGE_BYTES]; 16];
    let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
    let mut operation = boxed::<114_688>();
    let mut closed = runtime
        .bind(&owner, &core, &mut frames, &mut operation)
        .unwrap();
    let mut ready = false;
    for _ in 0..512 {
        match closed.next_action(&mut owner, &mut flusher).unwrap() {
            RecoveryAction::Ready => {
                ready = true;
                break;
            }
            RecoveryAction::Progress(progress) => assert!(!progress.ready),
            RecoveryAction::RowClr(task) => {
                panic!("C+B has no Row loser handler: {:?}", task.actor())
            }
        }
    }
    assert!(ready, "bounded C recovery exhausted");
    let (mut live, private) = match closed.into_live_with_private() {
        Ok(value) => value,
        Err(_) => panic!("original READY handoff"),
    };
    let out = at.at_ready(&mut live, &mut owner, &mut flusher, private);
    assert!(!runtime.session_fence().is_fenced());
    out
}

/// After READY, the Row actor's Begin sources (Root, first Jobs page and
/// Status 11 resident through the original empty-transaction preparation)
/// and the Catalog anchor and leaf B's table issue reads. Returns the Row
/// Status target.
pub(super) fn ready_row_sources<'s, 'c, W: WriterNamespace>(
    live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
    owner: &mut Owner<'c, 's>,
    flusher: &mut Owner<'c, 's>,
) -> Plan4Target {
    let row_status = live.recovery_test_expected_target(11).unwrap();
    let first_jobs = crate::storage::retention_page::JobSlotRef::new(
        page_ref(2, 1),
        0,
        foundation::identity::Generation::new(1).unwrap(),
    )
    .unwrap();
    let mut cursor = Plan4ResidencyCursor::new();
    for _ in 0..16 {
        match live
            .prepare_empty_targets(owner, &row_status, first_jobs, &mut cursor)
            .unwrap()
        {
            None => break,
            Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                drive_flush(live, flusher, Some(&target))
            }
            Some(_) => panic!("unexpected Begin preparation"),
        }
    }
    prepare_catalog(live, owner, flusher);
    row_status
}

/// A fresh retention catalog over `directory`, with the original Status and
/// Undo INIT of every fresh case (S5 native produce's setup).
macro_rules! with_directory {
    ($directory:expr, |$live:ident, $owner:ident, $flusher:ident, $status:ident, $undo:ident,
        $private:ident, $reader_private:ident, $core:ident, $fence:ident| $body:block) => {{
        let mut slot = boxed::<1_048_576>();
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let storage = FreshStorageSession::create_retention_catalog(
            $directory.acquire_writer().unwrap(),
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
        let mut backing = vec![[0xa7; PAGE_BYTES]; 16];
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
            Err(IntoRetentionError::Before { error, .. })
            | Err(IntoRetentionError::Fatal(error)) => panic!("{error:?}"),
        };
        live.enable_catalog_visibility(
            &mut owner,
            VisibilityLoan {
                bytes: visibility.try_into().unwrap(),
                epochs: epochs.try_into().unwrap(),
            },
        )
        .unwrap();
        assert_eq!(
            live.catalog_aging_start(&mut owner, private)
                .unwrap()
                .header
                .lsn,
            1
        );
        let status = initialize(&mut live, &mut owner, &mut flusher, false);
        let undo = initialize(&mut live, &mut owner, &mut flusher, true);
        let ($live, $owner, $flusher, $status, $undo, $private, $reader_private, $core, $fence) = (
            &mut live,
            &mut owner,
            &mut flusher,
            &status,
            &undo,
            private,
            reader_private,
            &core,
            storage.session_fence(),
        );
        $body
    }};
}

/// SHA-256 of the namespace's WAL file and its length (the exact-prefix
/// witness the controller compares with its own digest of the file).
pub(super) fn wal_witness(db: &Path) -> (String, usize) {
    let mut names: Vec<_> = fs::read_dir(db)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("d3-wal-"))
        .collect();
    names.sort();
    let bytes = fs::read(db.join(names.last().unwrap())).unwrap();
    (hex(&bytes), bytes.len())
}

/// The namespace's files (the READY read's input), read from `db`.
pub(super) fn db_images(db: &Path) -> Images {
    fs::read_dir(db)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

/// The restart refusal of `policy` (None: the legacy entry) over
/// `directory`: Analysis/Unsupported, CallerMustResolve with the lease
/// acquired, database Unchanged and no mutation (S6, moved here so the C+B
/// export keeps it); the caller checks its files.
pub(super) fn refused_in<D: DirectoryPort>(directory: &D, policy: Option<RowRecoveryPolicy>) {
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let owner = match Plan4Recovery::acquire(
        directory,
        Some(namespace()),
        &mut maintenance,
        &mut dwb,
        &mut slot,
    ) {
        Ok(owner) => owner,
        Err(Plan4RecoveryAcquireFailure::Acquired(_))
        | Err(Plan4RecoveryAcquireFailure::Before { .. }) => panic!("acquire"),
    };
    let result = match policy {
        None => owner.recover(),
        Some(policy) => owner.recover_with_rows(policy),
    };
    let failed = match result {
        Err(failed) => failed,
        Ok(_) => panic!("{policy:?} admitted a refused Row image"),
    };
    assert_eq!(failed.error().effect, Effect::CallerMustResolve);
    assert!(failed.error().lease_acquired);
    unchanged_error(
        failed.error(),
        failed.observation(),
        Plan4RecoveryPhase::Analysis,
    );
    assert!(matches!(
        failed.error().cause,
        Plan4RecoveryCause::Unsupported
    ));
    println!("S6R_REFUSED policy={policy:?} {:?}", failed.error());
}
