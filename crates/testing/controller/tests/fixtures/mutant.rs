//! Faulty worker variants for controller self-tests. Never built by default.
use std::io::{self, Write};
use test_protocol::{Observation, Operation, PROTOCOL_VERSION, Request, Value};

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "correct".into());
    if mode == "timeout" {
        loop {
            std::thread::park();
        }
    }
    if mode == "plain-pass" {
        println!("PASS");
        return;
    }
    if mode == "forged-pass" {
        println!("{{\"product_verdict\":\"PASS\"}}");
        return;
    }
    if mode == "malformed" {
        println!("{{broken");
        return;
    }
    if mode == "flood" {
        let mut out = io::stdout().lock();
        loop {
            if out.write_all(&[b'x'; 8192]).is_err() {
                return;
            }
        }
    }
    if mode == "stderr-flood" {
        let mut out = io::stderr().lock();
        loop {
            if out.write_all(&[b'x'; 8192]).is_err() {
                return;
            }
        }
    }
    let request: Request = serde_json::from_reader(io::stdin()).expect("test request");
    assert!(
        serde_json::to_string(&request)
            .unwrap()
            .find("expected")
            .is_none()
    );
    let mut out = io::stdout().lock();
    for (sequence, step) in request.steps.iter().enumerate() {
        if mode == "missing-step" && sequence == request.steps.len() - 1 {
            break;
        }
        let Operation::CheckedAdd { left, right } = step.action;
        let sum = i128::from(left) + i128::from(right);
        let mut value = i64::try_from(sum).map_or(Value::Overflow, Value::Int64);
        if mode.starts_with("wrong-value") {
            value = Value::Int64(123);
        }
        let mut witnesses = vec!["fixture.checked_add".to_string()];
        if value == Value::Overflow {
            witnesses.push("fixture.overflow".to_string());
        }
        if mode == "missing-witness" {
            witnesses.clear();
        }
        if mode == "unknown-witness" {
            witnesses.push("imaginary.event".into());
        }
        let observation = Observation {
            protocol_version: if mode == "wrong-version" {
                99
            } else {
                PROTOCOL_VERSION
            },
            run_id: if mode == "wrong-run" {
                "another-run".into()
            } else {
                request.run_id.clone()
            },
            case_sha256: if mode == "wrong-case" {
                "0".repeat(64)
            } else {
                request.case_sha256.clone()
            },
            sequence: if mode == "wrong-sequence" {
                sequence + 1
            } else {
                sequence
            },
            step_id: step.id.clone(),
            value,
            witnesses,
        };
        serde_json::to_writer(&mut out, &observation).unwrap();
        if mode == "truncated" {
            return;
        }
        writeln!(out).unwrap();
        if mode == "duplicate" {
            serde_json::to_writer(&mut out, &observation).unwrap();
            writeln!(out).unwrap();
        }
        out.flush().unwrap();
        if mode == "wrong-value-then-hang" {
            loop {
                std::thread::park();
            }
        }
    }
    if mode == "extra" {
        writeln!(out, "{{}}").unwrap();
    }
    if mode == "nonzero" {
        out.flush().unwrap();
        std::process::exit(7);
    }
}
