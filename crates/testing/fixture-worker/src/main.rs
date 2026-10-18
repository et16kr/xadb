//! A deliberately small harness fixture, never a substitute for a production engine.
use std::io::{self, Read, Write};
use xadbtest_protocol::{
    MAX_INPUT_BYTES, MAX_STEPS, Observation, Operation, PROTOCOL_VERSION, Request, Value,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    io::stdin()
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("request exceeds input budget".into());
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    if request.protocol_version != PROTOCOL_VERSION
        || request.steps.is_empty()
        || request.steps.len() > MAX_STEPS
    {
        return Err("unsupported request".into());
    }
    let mut output = io::stdout().lock();
    for (sequence, step) in request.steps.iter().enumerate() {
        let Operation::CheckedAdd { left, right } = step.action;
        let value = left
            .checked_add(right)
            .map_or(Value::Overflow, Value::Int64);
        let mut witnesses = vec!["fixture.checked_add".to_owned()];
        if value == Value::Overflow {
            witnesses.push("fixture.overflow".to_owned());
        }
        let observation = Observation {
            protocol_version: PROTOCOL_VERSION,
            run_id: request.run_id.clone(),
            case_sha256: request.case_sha256.clone(),
            sequence,
            step_id: step.id.clone(),
            value,
            witnesses,
        };
        serde_json::to_writer(&mut output, &observation)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fixture worker: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
