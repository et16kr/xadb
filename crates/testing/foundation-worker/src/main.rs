//! XADBTest adapter calling the production foundation API; no DB engine yet.
use std::io::{self, Read, Write};
use test_protocol::{
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
        let value = match foundation::value::checked_add(left, right) {
            Ok(value) => Value::Int64(value),
            Err(error) if error.code == foundation::ErrorCode::Overflow => Value::Overflow,
            Err(error) => return Err(error.into()),
        };
        let mut witnesses = vec!["foundation.checked_add".to_owned()];
        if value == Value::Overflow {
            witnesses.push("foundation.overflow".to_owned());
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
            eprintln!("foundation worker: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
