//! Native durable API wiring and literal concurrency expectations. This is not a
//! power-loss/crash-cut acceptance suite: no injected FileIo or ACK/WAL oracle.
use std::{collections::BTreeMap, fs, path::PathBuf};
use test_controller::{
    durable::{self, Session},
    file_sha256,
    native::Options,
};
use test_protocol::{
    durable::{
        Action as A, Cell, Column, DataType, Failure, Mode, Mutation, Observation as O, TableSpec,
    },
    frame,
};

fn call(session: &mut Session, actor: u8, action: A) -> O {
    durable::call(session, actor, action).unwrap().observation
}
fn done(value: O) {
    assert_eq!(value, O::Done {});
}
fn begin(session: &mut Session, actor: u8, transaction: u64, mode: Mode) {
    assert!(matches!(
        call(session, actor, A::Begin { transaction, mode }),
        O::Begun { .. }
    ));
}
fn commit(session: &mut Session, actor: u8, transaction: u64) {
    assert!(matches!(
        call(session, actor, A::Commit { transaction }),
        O::Committed { .. }
    ));
}
fn scalar(value: O, expected: i64) {
    match value {
        O::Row { value: Some(row) } => assert_eq!(row.values, vec![Cell::Int64(expected)]),
        other => panic!("unexpected row: {other:?}"),
    }
}
fn lookup(session: &mut Session, transaction: u64, table: u64) {
    assert!(matches!(
        call(
            session,
            0,
            A::Lookup {
                transaction,
                table,
                name: "items".into()
            }
        ),
        O::Table { .. }
    ));
}

// Independently decode retained event bytes, preserving observed response order.
// For gate 1 both invokes must precede either return; the same framed responses
// must also occur verbatim on stdout. No production/reference implementation used.
fn journal(directory: &std::path::Path) {
    use test_protocol::durable::{Request, Response};
    let bytes = fs::read(directory.join("history.bin")).unwrap();
    let mut offset = 0;
    let mut pending = BTreeMap::new();
    let mut gate_invokes = 0;
    let mut gate_returns = 0;
    let mut stdout = Vec::new();
    let mut sequence = 0;
    while offset < bytes.len() {
        let header = &bytes[offset..offset + 29];
        offset += 29;
        assert_eq!(
            u64::from_be_bytes(header[1..9].try_into().unwrap()),
            sequence
        );
        sequence += 1;
        let request = u64::from_be_bytes(header[17..25].try_into().unwrap());
        let size = u32::from_be_bytes(header[25..29].try_into().unwrap()) as usize;
        let payload = &bytes[offset..offset + size];
        offset += size;
        match header[0] {
            1 => {
                let input = frame::read::<Request>(&mut &*payload).unwrap().unwrap();
                assert_eq!(input.request, request);
                if input.command.gate == Some(1) {
                    gate_invokes += 1;
                }
                assert!(pending.insert(request, input).is_none());
            }
            2 => assert!(pending.contains_key(&request)),
            3 => {
                let output = frame::read::<Response>(&mut &*payload).unwrap().unwrap();
                let input = pending.remove(&request).unwrap();
                assert_eq!(
                    (output.request, output.actor, output.run),
                    (input.request, input.actor, input.run)
                );
                assert_eq!(output.passed_gate, input.command.gate);
                if output.passed_gate == Some(1) {
                    assert_eq!(gate_invokes, 2);
                    gate_returns += 1;
                }
                stdout.extend_from_slice(payload);
            }
            5 => assert!(pending.is_empty()),
            other => panic!("unexpected event kind {other}"),
        }
    }
    assert_eq!(gate_returns, 2);
    assert!(pending.is_empty());
    assert_eq!(fs::read(directory.join("stdout.bin")).unwrap(), stdout);
}

#[test]
fn actual_actor_conflict_old_cursor_and_checkpoint_archive_restore() {
    let root = tempfile::tempdir().unwrap();
    let binary = std::env::var_os("XADBTEST_DURABLE_WORKER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-durable-worker").into());
    let mut session = Session::start(Options::new(
        binary.clone(),
        file_sha256(&binary).unwrap(),
        std::env::var_os("XADBTEST_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.path().into()),
        [7; 16],
    ))
    .unwrap();
    assert!(matches!(
        call(&mut session, 0, A::CreateDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    assert!(matches!(
        call(
            &mut session,
            0,
            A::CreateTable {
                table: 1,
                spec: TableSpec {
                    name: "items".into(),
                    columns: vec![Column {
                        name: "value".into(),
                        data_type: DataType::Int64,
                        nullable: false
                    }]
                }
            }
        ),
        O::Table { .. }
    ));
    begin(&mut session, 0, 1, Mode::ReadWrite);
    assert!(matches!(
        call(
            &mut session,
            0,
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
    commit(&mut session, 0, 1);
    begin(&mut session, 2, 2, Mode::ReadOnly);
    done(call(
        &mut session,
        2,
        A::OpenScan {
            transaction: 2,
            table: 1,
            cursor: 1,
        },
    ));
    for (actor, transaction, value) in [(0, 3, 1), (1, 4, 2)] {
        begin(&mut session, actor, transaction, Mode::ReadWrite);
        assert!(matches!(
            call(
                &mut session,
                actor,
                A::Statement {
                    transaction,
                    mutations: vec![Mutation::Replace {
                        table: 1,
                        row: 1,
                        values: vec![Cell::Int64(value)]
                    }]
                }
            ),
            O::Statement {
                affected_rows: 1,
                ..
            }
        ));
    }
    let race = durable::rendezvous(
        &mut session,
        1,
        vec![
            (0, A::Commit { transaction: 3 }),
            (1, A::Commit { transaction: 4 }),
        ],
    )
    .unwrap();
    let mut winner = None;
    let mut conflicts = 0;
    for response in race.returned {
        match response.observation {
            O::Committed { wrote: true, .. } => {
                assert!(winner.replace(i64::from(response.actor) + 1).is_none());
            }
            O::Error {
                failure:
                    Failure::Transaction {
                        code,
                        effect,
                        certainty,
                        retry,
                        ..
                    },
            } => {
                assert_eq!(code, "SerializationConflict");
                assert_eq!(effect, test_protocol::durable::Effect::Aborted);
                assert_eq!(certainty, test_protocol::durable::Certainty::Known);
                assert_eq!(retry, test_protocol::durable::Retry::Transaction);
                conflicts += 1;
            }
            other => panic!("unexpected commit result {other:?}"),
        }
    }
    assert_eq!(conflicts, 1);
    let winner = winner.unwrap();
    match call(
        &mut session,
        2,
        A::NextPage {
            cursor: 1,
            limit: 1,
        },
    ) {
        O::Page {
            rows, eof: true, ..
        } => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].values, vec![Cell::Int64(0)]);
        }
        other => panic!("old cursor changed: {other:?}"),
    }
    done(call(&mut session, 2, A::CloseScan { cursor: 1 }));
    done(call(&mut session, 2, A::Rollback { transaction: 2 }));
    begin(&mut session, 0, 5, Mode::ReadOnly);
    scalar(
        call(
            &mut session,
            0,
            A::Read {
                transaction: 5,
                table: 1,
                row: 1,
            },
        ),
        winner,
    );
    commit(&mut session, 0, 5);
    let cut = match call(&mut session, 0, A::Checkpoint {}) {
        O::Checkpoint { position, .. } => position,
        other => panic!("checkpoint {other:?}"),
    };
    assert!(
        matches!(call(&mut session,0,A::Backup{directory:1}),O::Backup{position,..} if position==cut)
    );
    assert!(matches!(
        call(&mut session, 0, A::CollectGarbage {}),
        O::GarbageCollected { .. }
    ));
    done(call(&mut session, 0, A::CloseDatabase {}));
    assert!(matches!(
        call(&mut session, 0, A::OpenDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    begin(&mut session, 0, 6, Mode::ReadOnly);
    assert!(matches!(
        call(
            &mut session,
            0,
            A::Read {
                transaction: 6,
                table: 1,
                row: 1
            }
        ),
        O::Error {
            failure: Failure::Transaction { code, effect:test_protocol::durable::Effect::Active,
                certainty:test_protocol::durable::Certainty::Known,
                retry:test_protocol::durable::Retry::Never, .. }
        } if code == "XADB-ENGINE-0005"
    ));
    lookup(&mut session, 6, 2);
    scalar(
        call(
            &mut session,
            0,
            A::Read {
                transaction: 6,
                table: 2,
                row: 1,
            },
        ),
        winner,
    );
    commit(&mut session, 0, 6);
    done(call(&mut session, 0, A::CloseDatabase {}));
    assert!(matches!(
        call(
            &mut session,
            0,
            A::RestoreDatabase {
                source: 1,
                target: 2
            }
        ),
        O::Ready { .. }
    ));
    begin(&mut session, 0, 7, Mode::ReadOnly);
    lookup(&mut session, 7, 3);
    done(call(
        &mut session,
        0,
        A::OpenScan {
            transaction: 7,
            table: 3,
            cursor: 2,
        },
    ));
    let identity = match call(
        &mut session,
        0,
        A::NextPage {
            cursor: 2,
            limit: 1,
        },
    ) {
        O::Page {
            rows, eof: true, ..
        } => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].values, vec![Cell::Int64(winner)]);
            rows[0].identity.clone()
        }
        other => panic!("restored scan {other:?}"),
    };
    done(call(&mut session, 0, A::CloseScan { cursor: 2 }));
    done(call(&mut session, 0, A::BindRow { row: 2, identity }));
    scalar(
        call(
            &mut session,
            0,
            A::Read {
                transaction: 7,
                table: 3,
                row: 2,
            },
        ),
        winner,
    );
    commit(&mut session, 0, 7);
    done(call(&mut session, 0, A::CloseDatabase {}));
    let report = session.finish().unwrap();
    journal(&report.directory);
}

#[test]
fn closed_configuration_reaches_create_open_and_restore_without_zero_fill() {
    let root = tempfile::tempdir().unwrap();
    let binary = std::env::var_os("XADBTEST_DURABLE_WORKER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-durable-worker").into());
    let mut session = Session::start(Options::new(
        binary.clone(),
        file_sha256(&binary).unwrap(),
        std::env::var_os("XADBTEST_NATIVE_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.path().into()),
        [8; 16],
    ))
    .unwrap();
    done(call(
        &mut session,
        0,
        A::ConfigureEngine {
            observations: false,
            clock: false,
        },
    ));
    assert!(matches!(
        call(&mut session, 0, A::CreateDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    match call(&mut session, 0, A::DurabilityMetrics {}) {
        O::DurabilityMetrics { counters } => {
            assert!(counters.operations.is_none());
            assert!(!counters.clock_configured);
        }
        other => panic!("off metrics {other:?}"),
    }
    assert!(matches!(
        call(&mut session, 0, A::Backup { directory: 1 }),
        O::Backup { .. }
    ));
    done(call(&mut session, 0, A::CloseDatabase {}));
    done(call(
        &mut session,
        0,
        A::ConfigureEngine {
            observations: true,
            clock: false,
        },
    ));
    assert!(matches!(
        call(&mut session, 0, A::OpenDatabase { directory: 0 }),
        O::Ready { .. }
    ));
    match call(&mut session, 0, A::DurabilityMetrics {}) {
        O::DurabilityMetrics { counters } => {
            assert!(!counters.clock_configured);
            let operations = counters.operations.unwrap();
            assert_eq!(operations.len(), 21);
            assert!(
                operations
                    .iter()
                    .all(|operation| operation.elapsed.is_none())
            );
        }
        other => panic!("clockless metrics {other:?}"),
    }
    done(call(&mut session, 0, A::CloseDatabase {}));
    done(call(
        &mut session,
        0,
        A::ConfigureEngine {
            observations: false,
            clock: false,
        },
    ));
    assert!(matches!(
        call(
            &mut session,
            0,
            A::RestoreDatabase {
                source: 1,
                target: 2
            }
        ),
        O::Ready { .. }
    ));
    match call(&mut session, 0, A::DurabilityMetrics {}) {
        O::DurabilityMetrics { counters } => {
            assert!(counters.operations.is_none());
            assert!(!counters.clock_configured);
        }
        other => panic!("restored off metrics {other:?}"),
    }
    done(call(&mut session, 0, A::CloseDatabase {}));
    session.finish().unwrap();
}
