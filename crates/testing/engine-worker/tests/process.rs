//! Trusted worker-binary smoke, not the adversarial controller/descendant harness.
mod support;
use std::{
    io::Write,
    net::Shutdown,
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};
use support::initialize;
use xadbtest_protocol::{
    engine::{Action, Cell, Column, DataType, Observation, Request, Response, TableSpec},
    frame,
};

struct Worker {
    child: Child,
    input: UnixStream,
    output: UnixStream,
    reaped: bool,
}
impl Worker {
    fn new() -> Self {
        let binary = std::env::var_os("XADBTEST_ENGINE_WORKER_BIN")
            .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-engine-worker").into());
        let (input, child_input) = UnixStream::pair().unwrap();
        let (output, child_output) = UnixStream::pair().unwrap();
        input
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        output
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let child = Command::new(binary)
            .env_clear()
            .stdin(Stdio::from(OwnedFd::from(child_input)))
            .stdout(Stdio::from(OwnedFd::from(child_output)))
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self {
            child,
            input,
            output,
            reaped: false,
        }
    }
    fn send(&mut self, id: u64, action: Action) -> Response {
        self.input
            .write_all(&frame::encode(&request(id, action)).unwrap())
            .unwrap();
        let response: Response = frame::read(&mut self.output)
            .unwrap()
            .expect("response before stdin EOF");
        assert_eq!(
            (
                response.version,
                response.run,
                response.request,
                response.actor
            ),
            (1, [1; 16], id, 0)
        );
        response
    }
    fn close_and_wait(&mut self) -> ExitStatus {
        self.input.shutdown(Shutdown::Write).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                self.reaped = true;
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "worker failed to terminate after EOF"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn request(id: u64, action: Action) -> Request {
    Request {
        version: 1,
        run: [1; 16],
        request: id,
        actor: 0,
        action,
    }
}

#[test]
fn process_streams_real_observations_before_stdin_eof() {
    let mut worker = Worker::new();
    let ready = worker.send(1, initialize());
    assert_eq!(
        ready,
        Response {
            version: 1,
            run: [1; 16],
            request: 1,
            actor: 0,
            observation: Observation::Ready {}
        }
    );
    let metrics = worker.send(2, Action::Metrics {});
    let Observation::Metrics { counters } = metrics.observation else {
        panic!("metrics")
    };
    assert_eq!(
        (
            counters.operations,
            counters.rows_read,
            counters.snapshot_pins
        ),
        (0, 0, 0)
    );
    assert!(counters.resident_bytes > 0);
    assert!(worker.close_and_wait().success());
    assert_eq!(frame::read::<Response>(&mut worker.output).unwrap(), None);
}

#[test]
fn malformed_partial_and_oversized_frames_terminate_without_response() {
    let good = frame::encode(&request(1, initialize())).unwrap();
    for input in [
        vec![],
        good[..1].to_vec(),
        good[..good.len() - 1].to_vec(),
        vec![0, 0x40, 0, 0],
        vec![0, 0, 0, 1, b'{'],
    ] {
        let mut worker = Worker::new();
        worker.input.write_all(&input).unwrap();
        assert_eq!(worker.close_and_wait().code(), Some(2));
        assert_eq!(frame::read::<Response>(&mut worker.output).unwrap(), None);
    }
}

#[test]
fn initialize_failure_is_reported_then_worker_exits_before_a_second_request() {
    let mut worker = Worker::new();
    let mut action = initialize();
    if let Action::Initialize { database, .. } = &mut action {
        *database = [0; 16];
    }
    let response = worker.send(1, action);
    assert_eq!(
        response.observation,
        Observation::ProductError {
            code: "XADB-ENGINE-0001".into()
        }
    );
    // Read EOF without sending stdin EOF: failure closes admission immediately.
    assert_eq!(frame::read::<Response>(&mut worker.output).unwrap(), None);
    assert_eq!(worker.close_and_wait().code(), Some(2));
}

#[test]
fn duplicate_request_terminates_without_processing_later_frames() {
    let mut worker = Worker::new();
    assert_eq!(
        worker.send(1, initialize()).observation,
        Observation::Ready {}
    );
    let bytes = [
        frame::encode(&request(1, Action::Metrics {})).unwrap(),
        frame::encode(&request(2, Action::Metrics {})).unwrap(),
    ]
    .concat();
    worker.input.write_all(&bytes).unwrap();
    assert_eq!(frame::read::<Response>(&mut worker.output).unwrap(), None);
    assert_eq!(worker.close_and_wait().code(), Some(2));
}

fn generated(seed: u64, requests: u64) {
    let mut worker = Worker::new();
    support::generated(seed, requests, |id, action| worker.send(id, action));
    assert!(worker.close_and_wait().success());
}

#[test]
fn actual_64_mib_run_input_limit_rejects_next_frame_before_response() {
    let mut worker = Worker::new();
    let mut bytes = 0usize;
    let schema = TableSpec {
        name: "text".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Utf8,
            nullable: false,
        }],
    };
    let large = Cell::Utf8("\0".repeat(65536));
    let setup = [
        initialize(),
        Action::Create {
            table: 1,
            spec: schema,
        },
        Action::Insert {
            table: 1,
            row: 1,
            values: vec![large.clone()],
        },
    ];
    for (index, action) in setup.into_iter().enumerate() {
        bytes += frame::encode(&request(index as u64 + 1, action.clone()))
            .unwrap()
            .len();
        worker.send(index as u64 + 1, action);
    }
    let mut reached = false;
    for id in 4..=1024 {
        let encoded = frame::encode(&request(
            id,
            Action::Replace {
                table: 1,
                row: 1,
                values: vec![large.clone()],
            },
        ))
        .unwrap();
        bytes += encoded.len();
        worker.input.write_all(&encoded).unwrap();
        let response = frame::read::<Response>(&mut worker.output).unwrap();
        if bytes > 64 * 1024 * 1024 {
            assert_eq!(response, None);
            assert_eq!(worker.close_and_wait().code(), Some(2));
            reached = true;
            break;
        }
        assert_eq!(response.unwrap().observation, Observation::Done {});
    }
    assert!(reached, "actual 64 MiB boundary was not reached");
}

#[test]
fn actual_64_mib_run_output_limit_never_emits_a_partial_next_frame() {
    let mut worker = Worker::new();
    let schema = TableSpec {
        name: "text".into(),
        columns: vec![Column {
            name: "v".into(),
            data_type: DataType::Utf8,
            nullable: false,
        }],
    };
    let setup = [
        initialize(),
        Action::Create {
            table: 1,
            spec: schema,
        },
        Action::Insert {
            table: 1,
            row: 1,
            values: vec![Cell::Utf8("\0".repeat(65536))],
        },
        Action::Snapshot { snapshot: 1 },
    ];
    let mut bytes = 0usize;
    for (index, action) in setup.into_iter().enumerate() {
        let response = worker.send(index as u64 + 1, action);
        bytes += frame::encode(&response).unwrap().len();
    }
    let mut prior = worker.send(
        5,
        Action::Read {
            snapshot: 1,
            table: 1,
            row: 1,
        },
    );
    bytes += frame::encode(&prior).unwrap().len();
    let mut reached = false;
    for id in 6..=1024 {
        prior.request = id;
        bytes += frame::encode(&prior).unwrap().len();
        worker
            .input
            .write_all(
                &frame::encode(&request(
                    id,
                    Action::Read {
                        snapshot: 1,
                        table: 1,
                        row: 1,
                    },
                ))
                .unwrap(),
            )
            .unwrap();
        let response = frame::read::<Response>(&mut worker.output).unwrap();
        if bytes > 64 * 1024 * 1024 {
            assert_eq!(response, None);
            assert_eq!(worker.close_and_wait().code(), Some(2));
            reached = true;
            break;
        }
        assert_eq!(response, Some(prior.clone()));
    }
    assert!(reached, "actual 64 MiB boundary was not reached");
}

#[test]
fn exact_64_mib_input_succeeds_and_one_extra_byte_is_rejected() {
    for extra in [false, true] {
        let mut worker = Worker::new();
        for id in 1..=16 {
            let action = if id == 1 {
                initialize()
            } else {
                Action::Metrics {}
            };
            let mut encoded = frame::encode(&request(id, action)).unwrap();
            encoded.resize(frame::MAX_FRAME_BYTES, b' '); // Legal trailing JSON whitespace.
            encoded[..4].copy_from_slice(&(frame::MAX_BODY_BYTES as u32).to_be_bytes());
            worker.input.write_all(&encoded).unwrap();
            let response = frame::read::<Response>(&mut worker.output)
                .unwrap()
                .unwrap();
            assert_eq!(response.request, id);
        }
        if extra {
            worker.input.write_all(b"x").unwrap();
        }
        assert_eq!(
            worker.close_and_wait().code(),
            Some(if extra { 2 } else { 0 })
        );
        assert_eq!(frame::read::<Response>(&mut worker.output).unwrap(), None);
    }
}

#[test]
fn maximum_canonical_rows_survive_wire_escaping_and_page_byte_limits() {
    let mut worker = Worker::new();
    worker.send(1, initialize());
    let schema = TableSpec {
        name: "text".into(),
        columns: ["x", "y"]
            .into_iter()
            .map(|name| Column {
                name: name.into(),
                data_type: DataType::Utf8,
                nullable: false,
            })
            .collect(),
    };
    worker.send(
        2,
        Action::Create {
            table: 1,
            spec: schema,
        },
    );
    // 4-byte column count + two tags/lengths + 65,536 + 65,522 = 128 KiB.
    let values = vec![
        Cell::Utf8("\0".repeat(65536)),
        Cell::Utf8("\0".repeat(65522)),
    ];
    worker.send(
        3,
        Action::Insert {
            table: 1,
            row: 1,
            values: values.clone(),
        },
    );
    worker.send(
        4,
        Action::Insert {
            table: 1,
            row: 2,
            values: values.clone(),
        },
    );
    worker.send(5, Action::Snapshot { snapshot: 1 });
    let Observation::Row { value: Some(row) } = worker
        .send(
            6,
            Action::Read {
                snapshot: 1,
                table: 1,
                row: 1,
            },
        )
        .observation
    else {
        panic!("row")
    };
    assert_eq!(row.values, values);
    worker.send(
        7,
        Action::OpenScan {
            snapshot: 1,
            table: 1,
            cursor: 1,
        },
    );
    for (id, eof, row_id) in [
        (8, false, 0x0000_0001_0000_0001),
        (9, true, 0x0000_0001_0000_0002),
    ] {
        let Observation::Page {
            rows,
            eof: actual_eof,
            canonical_bytes,
        } = worker
            .send(
                id,
                Action::NextPage {
                    cursor: 1,
                    limit: 1024,
                },
            )
            .observation
        else {
            panic!("page")
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values, values);
        assert_eq!(rows[0].identity.row, row_id);
        assert_eq!(actual_eof, eof);
        assert_eq!(canonical_bytes, 131128);
    }
    assert!(worker.close_and_wait().success());
}

#[test]
fn native_v2_64_seeds_each_execute_128_requests_against_independent_values() {
    for seed in 0..64 {
        generated(seed, 128);
    }
}
#[test]
fn native_maximum_1024_requests_preserves_independent_values() {
    generated(u64::MAX, 1024);
}
#[test]
#[ignore = "explicit V3 native worker process profile"]
fn native_v3_256_seeds_each_execute_1024_requests_against_independent_values() {
    for seed in 0..256 {
        generated(seed, 1024);
    }
}
