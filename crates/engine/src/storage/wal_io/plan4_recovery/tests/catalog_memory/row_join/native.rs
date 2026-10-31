//! MCI02-N01..03 native worker (boundaries `#row-join-s7`): one same-ELF
//! process per phase over controller-owned paths. `produce` runs the actual
//! native writer — CREATE of T, then A's driver — and stops the process at
//! its cut without any shutdown: after the Commit ACK (N01, N02) or after
//! the first durable CLR of A's full rollback (N03). The session's Drop
//! then only releases the lease and closes the handles; the controller
//! asserts from the effect trace that no write follows the last successful
//! sync, and that the in-process WAL witness at the cut equals its own
//! digest of the WAL file (the exact durable prefix). `recover` runs C's
//! recovery with A's recovered CLR handler to READY, B's cursor reads T, then
//! A's new row and its Commit ACK; `reenter` is a third process whose C+B
//! recovery (RejectInitialLosers) and B cursor read both ACKs' rows. Every
//! expected value is fixed by the case's history before the run.
use super::super::super::super::super::plan4_pages::native;
use super::*;
use std::{fs, path::Path};

/// The value of the row each recovery process commits after READY.
const NEW: i64 = VALUE + 10;

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
fn wal_witness(db: &Path) -> (String, usize) {
    let mut names: Vec<_> = fs::read_dir(db)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("d3-wal-"))
        .collect();
    names.sort();
    let bytes = fs::read(db.join(names.last().unwrap())).unwrap();
    (hex(&bytes), bytes.len())
}

/// PID1: the actual native writer history of `id`, stopped at its cut. The
/// receipt holds the ACKed q and the WAL witness at the cut; the process
/// then exits with no flush or further write (Drop releases the lease and
/// closes the handles only, which the controller checks).
fn produce<D: DirectoryPort>(id: &str, directory: &D, db: &Path) -> String {
    with_directory!(directory, |live,
                                owner,
                                flusher,
                                status,
                                undo,
                                private,
                                reader_private,
                                core,
                                fence| {
        let (row_status, _) = create_table(live, owner, flusher, status, undo, private, true);
        let other_status = (id == "MCI02-N03").then(|| system_init(live, owner, flusher, false));
        for _ in 0..match id {
            "MCI02-N01" => 0,
            "MCI02-N02" => 2,
            _ => 3,
        } {
            system_init(live, owner, flusher, true);
        }
        let (mut writer, reader, mut flush) = live.split();
        let a = writer
            .begin_row_transaction(
                owner,
                IsolationLevel::Serializable,
                &row_status,
                private,
                &mut flush,
            )
            .unwrap();
        let s1 = writer.begin_statement(owner, &a).unwrap();
        let table = issue_table!(reader, owner, s1, reader_private, flush, flusher);
        let summary = match id {
            "MCI02-N01" => {
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE]
                );
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                let cut = wal_witness(db);
                format!(
                    "result\tacked\nq\t{q}\nrows\t{}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                    rows[0].row().get(),
                    cut.0,
                    cut.1
                )
            }
            "MCI02-N02" => {
                let kept = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE]
                );
                let s2 = writer.begin_statement(owner, &a).unwrap();
                let good = [ValueRef::Int64(VALUE + 1)];
                let bad = [ValueRef::Null];
                let input: [&[ValueRef<'_>]; 2] = [&good, &bad];
                let mut slots: [Option<RowMutationObservation>; 2] = [None, None];
                {
                    let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                    let mut pending =
                        match insert_live(&mut call, s2, &table, private, &input, &mut slots) {
                            Err(DriveError { failure, after }) => {
                                assert!(
                                    matches!(failure.error, DriverError::Insert(_)),
                                    "{failure:?}"
                                );
                                rollback_required(after)
                            }
                            Ok(n) => panic!("{n} rows inserted"),
                        };
                    {
                        let mut rollback = match start_statement_rollback_live(
                            &mut call,
                            &a,
                            &mut pending,
                            private,
                        ) {
                            Ok(rollback) => rollback,
                            Err(failure) => panic!("statement RollbackStart {failure:?}"),
                        };
                        assert!(matches!(
                            rollback_step_live(&mut rollback),
                            Ok(Stepped::Compensated)
                        ));
                        assert!(matches!(
                            rollback_step_live(&mut rollback),
                            Ok(Stepped::Complete)
                        ));
                        assert!(finish_rollback_live(rollback).is_ok());
                    }
                    let s2 = match pending.rolled_back() {
                        Ok(view) => view,
                        Err(_) => panic!("statement rollback incomplete"),
                    };
                    assert!(finish_statement_live(&mut call, s2).is_ok());
                }
                let q = a_commit!(&mut writer, owner, flush, flusher, a, private);
                let cut = wal_witness(db);
                format!(
                    "result\tacked\nq\t{q}\nrows\t{},{}\nwal_cut\t{}\nwal_cut_bytes\t{}\n",
                    kept[0].row().get(),
                    slots[0].as_ref().unwrap().row().get(),
                    cut.0,
                    cut.1
                )
            }
            "MCI02-N03" => {
                let rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    s1,
                    &table,
                    private,
                    [VALUE, VALUE + 1, VALUE + 2]
                );
                let y = writer
                    .begin_row_transaction(
                        owner,
                        IsolationLevel::ReadCommitted,
                        other_status.as_ref().unwrap(),
                        private,
                        &mut flush,
                    )
                    .unwrap();
                let yv = writer.begin_statement(owner, &y).unwrap();
                let y_rows = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    yv,
                    &table,
                    private,
                    [VALUE + 3]
                );
                let y_q = a_commit!(&mut writer, owner, flush, flusher, y, private);
                let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                let mut pending = match commit_live(&mut call, a, private) {
                    Err(DriveError { failure, after }) => {
                        assert!(matches!(
                            disk(&failure).cause,
                            RetentionCause::SerializationConflict
                        ));
                        rollback_required(after)
                    }
                    Ok(_) => panic!("conflicting Commit succeeded"),
                };
                let mut rollback = match start_full_rollback_live(&mut call, &mut pending, private)
                {
                    Ok(rollback) => rollback,
                    Err(failure) => panic!("full RollbackStart {failure:?}"),
                };
                let before = wal_witness(db);
                assert!(matches!(
                    rollback_step_live(&mut rollback),
                    Ok(Stepped::Compensated)
                ));
                // The CLR's apply returned after its frame was durable: the
                // cut is D (RollbackStart) + F (CLR1); the process stops here.
                let after = wal_witness(db);
                assert_ne!(before, after, "CLR1 reached the WAL file");
                let summary = format!(
                    "result\tcrashed\nq\t{y_q}\nrows\t{},{},{},{}\nwal_before_clr\t{}\n\
                     wal_cut\t{}\nwal_cut_bytes\t{}\n",
                    rows[0].row().get(),
                    rows[1].row().get(),
                    rows[2].row().get(),
                    y_rows[0].row().get(),
                    before.0,
                    after.0,
                    after.1
                );
                let _ = (fence, core);
                return summary;
            }
            _ => panic!("case {id}"),
        };
        assert!(!fence.is_fenced());
        let _ = core;
        summary
    })
}

/// The rows B must read after each phase, fixed by the case's history:
/// (value) in RowId order; the new row's value is NEW.
fn expected(id: &str, phase: &str) -> Vec<i64> {
    let committed = match id {
        "MCI02-N01" => vec![VALUE],
        "MCI02-N02" => vec![VALUE],
        "MCI02-N03" => vec![VALUE + 3],
        _ => panic!("case {id}"),
    };
    match phase {
        "recover" => committed,
        "reenter" => committed.into_iter().chain([NEW]).collect(),
        _ => panic!("phase {phase}"),
    }
}

#[test]
#[ignore = "bounded S7 native controller supplies isolated paths and identity"]
fn s7j_native_worker() {
    let env = |name| std::env::var(format!("XADB_S7_NATIVE_{name}")).unwrap();
    let (id, phase) = (env("CASE"), env("PHASE"));
    let db = PathBuf::from(env("DB"));
    let output = PathBuf::from(env("OUTPUT"));
    assert!(db.is_absolute() && output.is_absolute() && db != output);
    let source = std::env::var("XADB_CATALOG_SOURCE_SHA256").unwrap();
    let elf = std::env::var("XADB_CATALOG_ELF_SHA256").unwrap();
    assert!(valid_hash(&source) && valid_hash(&elf));
    let (directory, trace) = native::open_native(
        &db,
        &output.join("effects.tsv"),
        &output.join("entropy.tsv"),
        false,
    );
    let head = format!(
        "case\t{id}\nphase\t{phase}\npid\t{}\nsource\t{source}\nelf\t{elf}\n",
        std::process::id()
    );
    if phase == "produce" {
        let mut summary = String::new();
        case_within(&format!("{id}-produce"), Duration::from_secs(180), || {
            summary = produce(&id, &directory, &db);
        });
        fs::write(output.join("receipt.tsv"), format!("{head}{summary}")).unwrap();
        println!("S7J_NATIVE {id} produce {summary:?}");
        // The cut: no flush or shutdown write after the ACK/CLR (the
        // controller checks the effect trace and the WAL witness).
        std::process::exit(0);
    }
    let mut summary = String::new();
    case_within(&format!("{id}-{phase}"), Duration::from_secs(180), || {
        let (policy, ack) = match phase.as_str() {
            "recover" => (RowRecoveryPolicy::WithRowClr, Some(NEW)),
            "reenter" => (RowRecoveryPolicy::RejectInitialLosers, None),
            _ => panic!("phase {phase}"),
        };
        let joined = join_restart(&directory, policy, ack);
        let Joined {
            steps,
            clrs,
            ref rows,
            acked,
            ref ready,
            row_cells,
            ref after,
        } = joined;
        println!(
            "S7J_NATIVE {id} {phase} steps={steps} clrs={clrs} rows={rows:?} acked={acked:?} \
             ready={ready:?} row_cells={row_cells} after={after:?}"
        );
        let values: Vec<i64> = rows.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, expected(&id, &phase));
        assert!(rows.windows(2).all(|pair| pair[0].0 < pair[1].0));
        let expected_clrs = match (id.as_str(), phase.as_str()) {
            // CLR1 was durable before the cut: only the other two are new.
            ("MCI02-N03", "recover") => 2,
            _ => 0,
        };
        assert_eq!(clrs, expected_clrs);
        if let Some((row, _)) = acked {
            assert!(rows.iter().all(|(previous, _)| *previous < row));
        }
        // MCI01-V11 across processes: the READY Q is the history's last
        // ACKed q (the controller also checks it against the previous
        // receipt), whatever cleanup and CLRs ran before READY.
        let q = match phase.as_str() {
            // The produce ACK: A's Commit (N01, N02) or Y's (N03).
            "recover" => 2,
            // The recover process's ACK of NEW.
            _ => 3,
        };
        check_epochs(&joined, q);
        summary = format!(
            "result\tready\nsteps\t{steps}\nclrs\t{clrs}\nrows\t{rows:?}\nq_ready\t{}\n\
             epochs_ready\t{:?}\nepochs_after\t{after:?}\n{}",
            ready.0,
            ready.1,
            acked.map_or(String::new(), |(row, q)| format!(
                "ack_row\t{row}\nack_q\t{q}\n"
            ))
        );
    });
    drop(directory);
    let receipt = native::native_receipt(&trace);
    assert_eq!(receipt.handles, 0);
    assert!(receipt.peak <= 3 && receipt.fd_peak <= 6);
    assert!(receipt.log_bytes <= 4 * 1024 * 1024);
    fs::write(
        output.join("receipt.tsv"),
        format!(
            "{head}handles\t{}\npeak\t{}\nfd_peak\t{}\nmutations\t{}\nlog_bytes\t{}\n{summary}",
            receipt.handles, receipt.peak, receipt.fd_peak, receipt.mutations, receipt.log_bytes
        ),
    )
    .unwrap();
}
