//! Deliberately wrong native streams. Only available in the self-test feature.
use std::io::{self, Read, Write};
use test_protocol::{
    engine::{Cell, Observation, Request, Response, Row, RowIdentity, TableIdentity},
    frame,
};

fn response(request: &Request) -> Response {
    Response {
        version: 1,
        run: request.run,
        request: request.request,
        actor: request.actor,
        observation: Observation::Ready {},
    }
}
fn raw(response: &Response) -> Vec<u8> {
    let body = serde_json::to_vec(response).unwrap();
    let mut bytes = (body.len() as u32).to_be_bytes().to_vec();
    bytes.extend(body);
    bytes
}
fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "correct".into());
    if mode == "never-read" {
        loop {
            std::thread::park();
        }
    }
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    if mode == "before-invoke" || mode == "before-invoke-partial" {
        let reply = Response {
            version: 1,
            run: [9; 16],
            request: 1,
            actor: 0,
            observation: Observation::Ready {},
        };
        let bytes = raw(&reply);
        let cut = if mode == "before-invoke" {
            bytes.len()
        } else {
            8
        };
        output.write_all(&bytes[..cut]).unwrap();
        output.flush().unwrap();
        std::fs::write("emitted", b"ready").unwrap();
        if frame::read::<Request>(&mut input).unwrap().is_some() {
            output.write_all(&bytes[cut..]).unwrap();
            output.flush().unwrap();
        }
        return;
    }
    if mode == "premature" {
        let mut header = [0; 4];
        input.read_exact(&mut header).unwrap();
        output
            .write_all(
                &frame::encode(&Response {
                    version: 1,
                    run: [9; 16],
                    request: 1,
                    actor: 0,
                    observation: Observation::Ready {},
                })
                .unwrap(),
            )
            .unwrap();
        output.flush().unwrap();
        loop {
            std::thread::park();
        }
    }
    let mut queued = Vec::new();
    while let Some(request) = frame::read::<Request>(&mut input).unwrap() {
        if mode == "missing" {
            loop {
                std::thread::park();
            }
        }
        if mode == "late" {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if mode == "stderr-flood" {
            let mut error = io::stderr().lock();
            loop {
                if error.write_all(&[b'x'; 8192]).is_err() {
                    return;
                }
            }
        }
        let mut observation = response(&request);
        if mode == "queued-next-request" {
            observation.observation = Observation::Row {
                value: Some(Row {
                    identity: RowIdentity {
                        table: TableIdentity {
                            database: [1; 16],
                            timeline: 1,
                            object: 1,
                            generation: 1,
                            schema_epoch: 1,
                        },
                        row: 1,
                    },
                    values: vec![Cell::Utf8("a".repeat(3 * 65536))],
                }),
            };
            let predicted = Response {
                request: 2,
                actor: 1,
                observation: Observation::Ready {},
                ..response(&request)
            };
            output
                .write_all(&[raw(&observation), raw(&predicted)].concat())
                .unwrap();
            output.flush().unwrap();
            std::fs::write("emitted", b"ready").unwrap();
            let _ = frame::read::<Request>(&mut input).unwrap();
            return;
        }
        if mode == "partial-next-request" {
            let mut predicted = observation.clone();
            predicted.request = 2;
            predicted.actor = 1;
            let bytes = raw(&predicted);
            output.write_all(&bytes[..8]).unwrap();
            output.flush().unwrap();
            std::fs::write("emitted", b"ready").unwrap();
            let _ = frame::read::<Request>(&mut input).unwrap();
            output.write_all(&bytes[8..]).unwrap();
            output.write_all(&raw(&observation)).unwrap();
            output.flush().unwrap();
            return;
        }
        match mode.as_str() {
            "wrong-run" => observation.run = [4; 16],
            "wrong-actor" => observation.actor = 31,
            "wrong-request" => observation.request = 999,
            "wrong-version" => observation.version = 2,
            "large-response" => {
                observation.observation = Observation::Row {
                    value: Some(Row {
                        identity: RowIdentity {
                            table: TableIdentity {
                                database: [1; 16],
                                timeline: 1,
                                object: 1,
                                generation: 1,
                                schema_epoch: 1,
                            },
                            row: 1,
                        },
                        values: vec![Cell::Utf8("a".repeat(65536))],
                    }),
                };
            }
            _ => (),
        }
        if mode == "reverse" || mode == "queue32" {
            queued.push(observation);
            if queued.len() != if mode == "reverse" { 2 } else { 32 } {
                continue;
            }
            for response in queued.drain(..).rev() {
                output.write_all(&raw(&response)).unwrap();
            }
            output.flush().unwrap();
            continue;
        }
        if mode == "oversized" {
            output
                .write_all(&(frame::MAX_BODY_BYTES as u32 + 1).to_be_bytes())
                .unwrap();
            return;
        }
        if mode == "malformed" {
            output.write_all(b"\0\0\0\x01{").unwrap();
            return;
        }
        let bytes = raw(&observation);
        if mode == "ack-then-overflow" {
            let combined = [bytes.as_slice(), &[b'x'; 8192]].concat();
            output.write_all(&combined).unwrap();
            output.flush().unwrap();
            loop {
                std::thread::park();
            }
        }
        if mode == "partial" {
            output.write_all(&bytes[..bytes.len() - 1]).unwrap();
            return;
        }
        output.write_all(&bytes).unwrap();
        if mode == "duplicate" {
            output.write_all(&bytes).unwrap();
        }
        if mode == "unsolicited" {
            observation.request = 999;
            output.write_all(&raw(&observation)).unwrap();
        }
        output.flush().unwrap();
        if mode == "nonzero" {
            std::process::exit(7);
        }
        if mode == "descendant" {
            // Deliberate orphan fixture: the controller must kill the process group
            // after this leader exits with its descendant still holding the streams.
            #[allow(clippy::zombie_processes)]
            let child = std::process::Command::new("/bin/sleep")
                .arg("20")
                .stdin(std::process::Stdio::null())
                .spawn()
                .unwrap();
            eprintln!("DESCENDANT={}", child.id());
            return; // The descendant deliberately holds the inherited stdout/stderr open.
        }
    }
}
