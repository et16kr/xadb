//! Bounded blocking worker side. The owning controller imposes stream deadlines.
use std::io::{self, Read, Write};
use xadbtest_engine_worker::Session;
use xadbtest_protocol::{
    engine::{Action, Observation, Request},
    frame,
};

const RUN_BYTES: usize = 64 * 1024 * 1024;
struct Input<R> {
    reader: R,
    remaining: usize,
}
impl<R: Read> Read for Input<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(self.remaining.saturating_add(1));
        let read = self.reader.read(&mut output[..count])?;
        if read > self.remaining {
            return Err(io::Error::other("run input byte limit"));
        }
        self.remaining -= read;
        Ok(read)
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = Input {
        reader: io::stdin().lock(),
        remaining: RUN_BYTES,
    };
    let mut output = io::stdout().lock();
    let mut session = Session::new();
    let mut output_bytes = 0;
    let mut any_request = false;
    while let Some(request) = frame::read::<Request>(&mut input)? {
        any_request = true;
        let initialize = matches!(request.action, Action::Initialize { .. });
        let response = session.dispatch(request)?;
        let initialize_failed =
            initialize && matches!(response.observation, Observation::ProductError { .. });
        let encoded = frame::encode(&response)?;
        if encoded.len() > RUN_BYTES - output_bytes {
            return Err("run output byte limit".into());
        }
        output_bytes += encoded.len();
        output.write_all(&encoded)?;
        output.flush()?;
        if initialize_failed {
            return Err("initialize failed; session closed".into());
        }
    }
    if !any_request {
        return Err("empty worker run".into());
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("engine worker: {error}");
            std::process::ExitCode::from(2)
        }
    }
}
