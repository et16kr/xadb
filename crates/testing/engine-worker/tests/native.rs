//! Actual engine worker through the controller, with external evidence checked independently.
mod support;
use std::{
    fs,
    path::{Path, PathBuf},
};
use test_controller::{
    file_sha256,
    native::{Options, Session},
};
use test_protocol::{
    engine::{Action, Cell, Column, DataType, Observation, Request, Response, TableSpec},
    frame,
};

fn session(root: &Path) -> Session {
    let binary = std::env::var_os("XADBTEST_ENGINE_WORKER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-engine-worker").into());
    let artifacts = std::env::var_os("XADBTEST_NATIVE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.to_path_buf());
    Session::start(Options::new(
        binary.clone(),
        file_sha256(&binary).unwrap(),
        artifacts,
        [1; 16],
    ))
    .unwrap()
}

// Independent journal reader. A successful generated observation is compared again
// against its externally retained Invoke/Sent/Observe, and against raw wire bytes.
fn verify_history(path: &Path, pairs: &[(Request, Response)], final_kinds: &[u8]) {
    let bytes = fs::read(path.join("history.bin")).unwrap();
    let mut offset = 0;
    let mut sequence = 0;
    let mut previous_time = 0;
    let mut raw = Vec::new();
    for (request, response) in pairs {
        for (kind, payload) in [
            (1u8, frame::encode(request).unwrap()),
            (2, Vec::new()),
            (3, frame::encode(response).unwrap()),
        ] {
            let header = &bytes[offset..offset + 29];
            assert_eq!(header[0], kind);
            assert_eq!(
                u64::from_be_bytes(header[1..9].try_into().unwrap()),
                sequence
            );
            let time = u64::from_be_bytes(header[9..17].try_into().unwrap());
            assert!(time >= previous_time);
            previous_time = time;
            assert_eq!(
                u64::from_be_bytes(header[17..25].try_into().unwrap()),
                request.request
            );
            let length = u32::from_be_bytes(header[25..29].try_into().unwrap()) as usize;
            assert_eq!(length, payload.len());
            offset += 29;
            assert_eq!(bytes[offset..offset + length], payload);
            offset += length;
            sequence += 1;
        }
        raw.extend(frame::encode(response).unwrap());
    }
    for &kind in final_kinds {
        let header = &bytes[offset..offset + 29];
        assert_eq!(header[0], kind);
        assert_eq!(
            u64::from_be_bytes(header[1..9].try_into().unwrap()),
            sequence
        );
        let time = u64::from_be_bytes(header[9..17].try_into().unwrap());
        assert!(time >= previous_time);
        previous_time = time;
        assert_eq!(&header[17..], &[0; 12]);
        sequence += 1;
        offset += 29;
    }
    assert_eq!(bytes.len(), offset);
    assert_eq!(fs::read(path.join("stdout.bin")).unwrap(), raw);
    assert!(fs::read(path.join("stderr.bin")).unwrap().is_empty());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["journal_format_version"], 1);
    let (report_name, completion_kind) = if final_kinds == [5] {
        ("completion.json", "TRANSPORT_CLOSED_NO_PRODUCT_VERDICT")
    } else {
        assert_eq!(final_kinds, [4, 6]);
        assert!(!path.join("completion.json").exists());
        ("planned-kill.json", "WORKER_KILLED_NO_PRODUCT_VERDICT")
    };
    let completion: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join(report_name)).unwrap()).unwrap();
    for (name, file) in [
        ("worker_sha256", "worker"),
        ("history_sha256", "history.bin"),
        ("stdout_sha256", "stdout.bin"),
        ("stderr_sha256", "stderr.bin"),
    ] {
        assert_eq!(completion[name], file_sha256(&path.join(file)).unwrap());
    }
    assert_eq!(completion["requests"], pairs.len());
    assert_eq!(completion["observations"], pairs.len());
    assert_eq!(completion["completion"], completion_kind);
}

fn exchange(
    session: &mut Session,
    pairs: &mut Vec<(Request, Response)>,
    action: Action,
) -> Response {
    let id = session.invoke(0, action.clone()).unwrap();
    let response = session.receive().unwrap();
    let request = Request {
        version: 1,
        run: [1; 16],
        request: id,
        actor: 0,
        action,
    };
    assert_eq!(
        (
            response.version,
            response.run,
            response.request,
            response.actor
        ),
        (1, [1; 16], id, 0)
    );
    pairs.push((request, response.clone()));
    response
}

#[test]
fn controller_records_actual_old_snapshot_after_delete_and_drop_recreate() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let mut pairs = Vec::new();
    let schema = TableSpec {
        name: "t".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Utf8,
            nullable: true,
        }],
    };
    let setup = [
        support::initialize(),
        Action::Create {
            table: 1,
            spec: schema.clone(),
        },
        Action::Insert {
            table: 1,
            row: 1,
            values: vec![Cell::Utf8("old\0row".into())],
        },
        Action::Snapshot { snapshot: 1 },
        Action::Delete { table: 1, row: 1 },
        Action::DropTable { table: 1 },
        Action::Create {
            table: 2,
            spec: schema,
        },
        Action::Snapshot { snapshot: 2 },
    ];
    for action in setup {
        exchange(&mut session, &mut pairs, action);
    }
    let Observation::Inserted { identity } = pairs[2].1.observation.clone() else {
        panic!("insert")
    };
    let Observation::Row { value: Some(row) } = exchange(
        &mut session,
        &mut pairs,
        Action::Read {
            snapshot: 1,
            table: 1,
            row: 1,
        },
    )
    .observation
    else {
        panic!("old row")
    };
    assert_eq!(row.identity, identity);
    assert_eq!(row.values, vec![Cell::Utf8("old\0row".into())]);
    assert_eq!(
        exchange(
            &mut session,
            &mut pairs,
            Action::Read {
                snapshot: 2,
                table: 1,
                row: 1
            }
        )
        .observation,
        Observation::ProductError {
            code: "XADB-ENGINE-0005".into()
        }
    );
    let report = session.finish().unwrap();
    verify_history(&report.directory, &pairs, &[5]);
}

fn generated(seed: u64, requests: u64) {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let mut pairs = Vec::new();
    support::generated(seed, requests, |id, action| {
        let response = exchange(&mut session, &mut pairs, action);
        assert_eq!(response.request, id);
        response
    });
    let report = session.finish().unwrap();
    verify_history(&report.directory, &pairs, &[5]);
}

#[test]
fn controller_v2_64_seeds_execute_128_requests_and_verify_external_history() {
    for seed in 0..64 {
        generated(seed, 128);
    }
}
#[test]
fn controller_maximum_1024_requests_verify_external_history() {
    generated(u64::MAX, 1024);
}
#[test]
#[ignore = "explicit V3 actual controller/worker profile"]
fn controller_v3_256_seeds_execute_1024_requests_and_verify_external_history() {
    for seed in 0..256 {
        generated(seed, 1024);
    }
}

#[test]
fn actual_worker_kill_receipt_follows_recorded_insert_and_read() {
    let root = tempfile::tempdir().unwrap();
    let mut session = session(root.path());
    let directory = session.directory().to_path_buf();
    let mut pairs = Vec::new();
    let schema = TableSpec {
        name: "kill".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Int64,
            nullable: false,
        }],
    };
    for action in [
        support::initialize(),
        Action::Create {
            table: 1,
            spec: schema,
        },
        Action::Insert {
            table: 1,
            row: 1,
            values: vec![Cell::Int64(42)],
        },
        Action::Snapshot { snapshot: 1 },
    ] {
        exchange(&mut session, &mut pairs, action);
    }
    let Observation::Row { value: Some(row) } = exchange(
        &mut session,
        &mut pairs,
        Action::Read {
            snapshot: 1,
            table: 1,
            row: 1,
        },
    )
    .observation
    else {
        panic!("read before kill")
    };
    assert_eq!(row.values, vec![Cell::Int64(42)]);
    session.kill_worker().unwrap();
    verify_history(&directory, &pairs, &[4, 6]);
}
