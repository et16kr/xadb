//! Actual namespace cut handshakes, never power-loss simulation. Expected scalar
//! fixtures are independent literals; the external ACK/WAL oracle is a separate
//! program. The owning root runner decides when these source cases are executed.
use std::{
    fs,
    path::{Path, PathBuf},
};
use xadbtest::{
    durable::{self, Session},
    fault::{CapturedImage, Control},
    file_sha256,
    native::Options,
};
use xadbtest_protocol::{
    durable::{Action as A, Cell, Column, DataType, Mode, Mutation, Observation as O, TableSpec},
    fault::{Boundary as B, Cut, Decision, Plan, Target as T},
};

fn start(artifacts: &Path, run: u8, work: Option<PathBuf>) -> Session {
    let binary = std::env::var_os("XADBTEST_DURABLE_WORKER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-durable-worker").into());
    let mut options = Options::new(
        binary.clone(),
        file_sha256(&binary).unwrap(),
        artifacts.into(),
        [run; 16],
    );
    options.work_directory = work;
    Session::start(options).unwrap()
}
fn call(session: &mut Session, action: A) -> O {
    durable::call(session, 0, action).unwrap().observation
}
fn done(value: O) {
    assert_eq!(value, O::Done {});
}
fn committed(value: O) {
    assert!(matches!(value, O::Committed { .. }));
}
fn begin(session: &mut Session, transaction: u64, mode: Mode) {
    assert!(matches!(
        call(session, A::Begin { transaction, mode }),
        O::Begun { .. }
    ));
}
fn fixture(session: &mut Session) {
    assert!(matches!(
        call(session, A::CreateDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    assert!(
        matches!(call(session,A::CreateTable{table:1,spec:TableSpec{name:"items".into(),columns:vec![Column{name:"value".into(),data_type:DataType::Int64,nullable:false}]}}),O::Table{commit:Some(receipt),..}if receipt.position==1&&receipt.wrote)
    );
    begin(session, 1, Mode::ReadWrite);
    assert!(matches!(
        call(
            session,
            A::Statement {
                transaction: 1,
                mutations: vec![Mutation::Insert {
                    table: 1,
                    row: 1,
                    values: vec![Cell::Int64(0)]
                }]
            }
        ),
        O::Statement {
            affected_rows: 1,
            ..
        }
    ));
    assert!(matches!(
        call(session, A::Commit { transaction: 1 }),
        O::Committed {
            position: 2,
            wrote: true,
            ..
        }
    ));
}
fn replace(session: &mut Session) {
    begin(session, 2, Mode::ReadWrite);
    assert!(matches!(
        call(
            session,
            A::Statement {
                transaction: 2,
                mutations: vec![Mutation::Replace {
                    table: 1,
                    row: 1,
                    values: vec![Cell::Int64(7)]
                }]
            }
        ),
        O::Statement {
            affected_rows: 1,
            ..
        }
    ));
}
fn read(session: &mut Session, expected: i64) {
    begin(session, 3, Mode::ReadOnly);
    assert!(matches!(
        call(
            session,
            A::Lookup {
                transaction: 3,
                table: 2,
                name: "items".into()
            }
        ),
        O::Table { commit: None, .. }
    ));
    done(call(
        session,
        A::OpenScan {
            transaction: 3,
            table: 2,
            cursor: 1,
        },
    ));
    match call(
        session,
        A::NextPage {
            cursor: 1,
            limit: 2,
        },
    ) {
        O::Page {
            rows, eof: true, ..
        } => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].values, vec![Cell::Int64(expected)]);
        }
        other => panic!("unexpected recovered rows {other:?}"),
    }
    done(call(session, A::CloseScan { cursor: 1 }));
    committed(call(session, A::Commit { transaction: 3 }));
}
fn plan(target: T, boundary: B) -> Plan {
    Plan {
        cut: Cut { target, boundary },
        occurrence: 1,
        actor: Some(0),
        directory: Some(0),
    }
}
fn root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = std::env::var_os("XADBTEST_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    (temp, path)
}
fn expected(session: &Session, name: &str, value: i64, cut: Plan) {
    // Saved before arming/invoking the adverse operation, never computed from its reply.
    let path = session.directory().join("fault-case.json");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file,&serde_json::json!({"schema_version":1,"scope":"NATIVE_OS_FAULT_NOT_POWER_LOSS","case_kind":"recovery","case":name,"plan":cut,"initial_position":2,"initial_values":[0],"expected_reopen_values":[value]})).unwrap();
    file.sync_all().unwrap();
}
fn image_link(image: &CapturedImage) -> serde_json::Value {
    serde_json::json!({"directory":image.directory,"receipt":image.receipt,"receipt_sha256":file_sha256(&image.receipt).unwrap(),"inventory_sha256":image.inventory_sha256,"files":image.files,"bytes":image.bytes})
}
fn save_link(path: &Path, value: serde_json::Value) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.sync_all().unwrap();
    fs::File::open(path.parent().unwrap())
        .unwrap()
        .sync_all()
        .unwrap();
}
fn reopen(artifacts: &Path, run: u8, origin: &Path, image: &CapturedImage, expected: i64) {
    let mut session = start(artifacts, run, Some(origin.join("work")));
    assert!(matches!(
        call(&mut session, A::OpenDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    read(&mut session, expected);
    done(call(&mut session, A::CloseDatabase {}));
    let reopened = session.finish().unwrap().directory;
    save_link(
        &origin.join("reopen-run.json"),
        serde_json::json!({"schema_version":1,"origin_run":origin,"reopen_run":reopened,"image":image_link(image),"expected_values":[expected]}),
    );
}
#[test]
fn partial_write_continue_returns_short_prefix_then_real_commit_ack() {
    let (_temp, artifacts) = root();
    let mut session = start(&artifacts, 31, None);
    fixture(&mut session);
    replace(&mut session);
    let cut = plan(T::WalCommit, B::WritePartial);
    expected(&session, "partial_continue", 7, cut);
    let mut control = Control::connect(&mut session, [31; 16], cut).unwrap();
    let request = session
        .invoke(0, A::Commit { transaction: 2 }.into())
        .unwrap();
    let reached = control.reached(&mut session, request, 0).unwrap();
    assert_eq!(reached.completed_bytes, Some(1));
    assert!(reached.requested_bytes.unwrap() > 1);
    assert_eq!(reached.frame_written_bytes, Some(1));
    control.release(&mut session, Decision::Continue).unwrap();
    let response = session.receive().unwrap();
    assert_eq!(response.request, request);
    assert!(matches!(
        response.observation,
        O::Committed {
            position: 3,
            wrote: true,
            ..
        }
    ));
    let origin = session.directory().to_owned();
    let image = control.close_kill_capture(&mut session).unwrap();
    reopen(&artifacts, 131, &origin, &image, 7);
}
#[test]
fn synced_unacknowledged_commit_survives_kill_and_recovery_sync_refault() {
    let (_temp, artifacts) = root();
    let mut first = start(&artifacts, 32, None);
    fixture(&mut first);
    replace(&mut first);
    let work = first.directory().join("work");
    let first_directory = first.directory().to_owned();
    let cut = plan(T::WalCommit, B::SyncAfter);
    expected(&first, "synced_unacknowledged_then_refault", 7, cut);
    let mut control = Control::connect(&mut first, [32; 16], cut).unwrap();
    let request = first
        .invoke(0, A::Commit { transaction: 2 }.into())
        .unwrap();
    let reached = control.reached(&mut first, request, 0).unwrap();
    assert_eq!(
        reached.frame_end.unwrap() - reached.frame_start.unwrap(),
        reached.frame_written_bytes.unwrap()
    );
    control.kill_reached(&mut first).unwrap();
    let first_image = control.capture_after_kill(&first).unwrap();
    let mut second = start(&artifacts, 33, Some(work.clone()));
    let second_directory = second.directory().to_owned();
    let cut = plan(T::RecoveryOldFile, B::SyncAfter);
    expected(&second, "recovery_stabilize_refault", 7, cut);
    let mut control = Control::connect(&mut second, [33; 16], cut).unwrap();
    let request = second
        .invoke(0, A::OpenDatabase { directory: 0 }.into())
        .unwrap();
    let witness = control.reached(&mut second, request, 0).unwrap();
    assert_eq!(witness.phase, xadbtest_protocol::fault::Phase::Open);
    control.kill_reached(&mut second).unwrap();
    let second_image = control.capture_after_kill(&second).unwrap();
    let mut third = start(&artifacts, 34, Some(work));
    assert!(matches!(
        call(&mut third, A::OpenDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    read(&mut third, 7);
    done(call(&mut third, A::CloseDatabase {}));
    let third_directory = third.finish().unwrap().directory;
    save_link(
        &first_directory.join("refault-chain.json"),
        serde_json::json!({"schema_version":1,"runs":[first_directory,second_directory,third_directory],"images":[image_link(&first_image),image_link(&second_image)],"expected_values":[7],"required_reached":2,"qualified_kills":2}),
    );
}
#[test]
fn every_declared_error_cut_has_a_flushed_reached_and_release_receipt() {
    let (_temp, artifacts) = root();
    let mut cuts = vec![plan(T::Any, B::CreateBefore), plan(T::Any, B::CreateAfter)];
    for boundary in [
        B::WriteBefore,
        B::WritePartial,
        B::WriteAfter,
        B::SyncBefore,
        B::SyncAfter,
    ] {
        cuts.push(plan(T::WalCommit, boundary));
    }
    for boundary in [B::SyncBefore, B::SyncAfter] {
        cuts.push(plan(T::RecoveryOldFile, boundary));
    }
    for target in [T::Current, T::Writer] {
        for boundary in [
            B::WriteBefore,
            B::WritePartial,
            B::WriteAfter,
            B::SyncBefore,
            B::SyncAfter,
            B::RenameBefore,
            B::RenameAfter,
            B::DirectorySyncBefore,
            B::DirectorySyncAfter,
        ] {
            cuts.push(plan(target, boundary));
        }
    }
    for boundary in [
        B::UnlinkBefore,
        B::UnlinkAfter,
        B::DirectorySyncBefore,
        B::DirectorySyncAfter,
    ] {
        cuts.push(plan(T::Retirement, boundary));
    }
    assert_eq!(cuts.len(), 31);
    println!(
        "XADB_NATIVE_FAULT_PROFILE {{\"cases\":31,\"decision\":\"return_error\",\"scope\":\"native_os_not_power_loss\"}}"
    );
    let mut completed = 0;
    for (index, cut) in cuts.into_iter().enumerate() {
        let mut session = start(&artifacts, 64 + index as u8, None);
        fixture(&mut session);
        let action = match cut.cut.target {
            T::WalCommit => {
                replace(&mut session);
                A::Commit { transaction: 2 }
            }
            T::RecoveryOldFile | T::Writer => {
                done(call(&mut session, A::CloseDatabase {}));
                A::OpenDatabase { directory: 0 }
            }
            T::Retirement => {
                for _ in 0..2 {
                    assert!(matches!(
                        call(&mut session, A::Checkpoint {}),
                        O::Checkpoint { .. }
                    ));
                }
                A::CollectGarbage {}
            }
            T::Current | T::Any => A::Checkpoint {},
        };
        let expected_value = if cut.cut.target == T::WalCommit
            && matches!(
                cut.cut.boundary,
                B::WriteAfter | B::SyncBefore | B::SyncAfter
            ) {
            7
        } else {
            0
        };
        expected(&session, "return_error_matrix", expected_value, cut);
        let mut control = Control::connect(&mut session, [64 + index as u8; 16], cut).unwrap();
        let request = session.invoke(0, action.clone().into()).unwrap();
        control.reached(&mut session, request, 0).unwrap();
        control
            .release(&mut session, Decision::ReturnError)
            .unwrap();
        let response = session.receive().unwrap();
        assert_eq!(response.request, request);
        assert!(
            matches!(response.observation, O::Error { .. }),
            "cut {cut:?}: {:?}",
            response.observation
        );
        let origin = session.directory().to_owned();
        let image = control.close_kill_capture(&mut session).unwrap();
        reopen(
            &artifacts,
            160 + index as u8,
            &origin,
            &image,
            expected_value,
        );
        completed += 1;
    }
    assert_eq!(completed, 31);
    println!(
        "XADB_NATIVE_FAULT_RESULT {{\"cases\":31,\"reached\":31,\"released\":31,\"preopen_images\":31,\"reopened_processes\":31,\"missing\":0}}"
    );
}

#[test]
fn control_from_another_session_never_qualifies_or_cleans_up_its_process() {
    let (_temp, artifacts) = root();
    let mut first = start(&artifacts, 120, None);
    fixture(&mut first);
    save_link(
        &first.directory().join("fault-case.json"),
        serde_json::json!({"schema_version":1,"scope":"CONTROLLER_OWNER_NEGATIVE","case_kind":"controller_negative","case":"owner_mismatch_does_not_kill","plan":plan(T::WalCommit,B::SyncAfter),"required_reached":0,"qualified_kills":0}),
    );
    let mut control =
        Control::connect(&mut first, [120; 16], plan(T::WalCommit, B::SyncAfter)).unwrap();
    let mut unrelated = start(&artifacts, 121, None);
    assert_eq!(
        control.kill_reached(&mut unrelated).unwrap_err().kind,
        xadbtest::native::ErrorKind::Configuration
    );
    // The ownership error must leave the unrelated process alive and usable.
    assert!(matches!(
        call(&mut unrelated, A::CreateDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    done(call(&mut unrelated, A::CloseDatabase {}));
    unrelated.finish().unwrap();
    done(call(&mut first, A::CloseDatabase {}));
    first.finish().unwrap();
}
