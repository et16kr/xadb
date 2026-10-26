//! Opt-in, bounded diagnostic gate. The normal fixture performs no extra clocks
//! or OS observation. Socket ownership ends on every error; no I/O occurs in Drop.
use crate::ProtocolError;
use platform::performance::{calling_thread_id, monotonic_ns, observation_stream, thread_usage};
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(2);
const LINE_CAP: usize = 512;
const SOCKET: &str = "XADBTEST_WINDOW_SOCKET";
const TOKEN: &str = "XADBTEST_WINDOW_TOKEN";
const BOUNDARY: &str = "XADBTEST_WINDOW_BOUNDARY";
const SUFFIX: &str = ".diagnostic_window.v1";
type Result<T> = std::result::Result<T, ProtocolError>;
fn fail() -> ProtocolError {
    ProtocolError("measurement window protocol/configuration/deadline failure")
}

fn config(socket: Option<String>, token: Option<String>) -> Result<Option<(String, String)>> {
    match (socket, token) {
        (None, None) => Ok(None),
        (Some(path), Some(token))
            if Path::new(&path).is_absolute()
                && path.len() < 108
                && token.len() == 32
                && token
                    .bytes()
                    .all(|v| v.is_ascii_hexdigit() && !v.is_ascii_uppercase()) =>
        {
            Ok(Some((path, token)))
        }
        _ => Err(fail()),
    }
}
fn environment(key: &str) -> Result<Option<String>> {
    std::env::var_os(key)
        .map(|v| v.into_string().map_err(|_| fail()))
        .transpose()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    Measurement,
    Continuous,
    Wait,
}
impl Boundary {
    fn parse(value: Option<&str>, workload: &str, configured: bool) -> Result<Self> {
        match value {
            None => Ok(Self::Measurement),
            Some("prewarmup-continuous-v1") if configured && workload == "k0" => {
                Ok(Self::Continuous)
            }
            Some("prewarmup-wait-v1") if configured && workload == "k0" => Ok(Self::Wait),
            _ => Err(fail()),
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Measurement => "measurement",
            Self::Continuous => "prewarmup-continuous-v1",
            Self::Wait => "prewarmup-wait-v1",
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::Measurement => SUFFIX,
            Self::Continuous => ".diagnostic_prewarmup_continuous.v1",
            Self::Wait => ".diagnostic_prewarmup_wait.v1",
        }
    }
}
fn remaining(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(fail())
    } else {
        Ok(())
    }
}
fn retry(error: io::Error, deadline: Instant) -> Result<()> {
    if !matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) {
        return Err(fail());
    }
    remaining(deadline)?;
    std::thread::sleep(Duration::from_millis(1));
    Ok(())
}
fn exchange(
    stream: &mut UnixStream,
    message: &str,
    expected: &[String],
    timeout: Duration,
) -> Result<usize> {
    if message.len() + 1 > LINE_CAP || message.contains('\n') {
        return Err(fail());
    }
    let deadline = Instant::now() + timeout;
    let mut bytes = message.as_bytes().to_vec();
    bytes.push(b'\n');
    let mut offset = 0;
    while offset < bytes.len() {
        remaining(deadline)?;
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(fail()),
            Ok(count) => offset += count,
            Err(error) => retry(error, deadline)?,
        }
    }
    let mut line = Vec::new();
    loop {
        remaining(deadline)?;
        let mut byte = [0];
        match stream.read(&mut byte) {
            Ok(0) => return Err(fail()),
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => {
                line.push(byte[0]);
                if line.len() >= LINE_CAP {
                    return Err(fail());
                }
            }
            Err(error) => retry(error, deadline)?,
        }
    }
    expected
        .iter()
        .position(|v| v.as_bytes() == line)
        .ok_or_else(fail)
}

struct Gate {
    stream: UnixStream,
    token: String,
    mode: &'static str,
    boundary: Boundary,
    armed_ns: Option<u64>,
    start: Option<(u64, Option<[u64; 6]>)>,
    finished: bool,
}
pub(crate) struct Window(Option<Gate>);
impl Window {
    pub(crate) fn prepare(workload: &str) -> Result<Self> {
        let configured = config(environment(SOCKET)?, environment(TOKEN)?)?;
        let boundary = Boundary::parse(
            environment(BOUNDARY)?.as_deref(),
            workload,
            configured.is_some(),
        )?;
        let Some((path, token)) = configured else {
            return Ok(Self(None));
        };
        let pid = std::process::id();
        let tid = calling_thread_id();
        if pid != tid {
            return Err(fail());
        }
        let mut stream = observation_stream(Path::new(&path)).map_err(|_| fail())?;
        let hello = if boundary == Boundary::Measurement {
            format!(
                "HELLO 1 {token} {pid} {tid} {} {workload}",
                env!("XADBTEST_SOURCE_SHA256")
            )
        } else {
            format!(
                "HELLO 2 {token} {pid} {tid} {} {workload} {}",
                env!("XADBTEST_SOURCE_SHA256"),
                boundary.name()
            )
        };
        let mode = exchange(
            &mut stream,
            &hello,
            &[
                format!("READY {token} PMU"),
                format!("READY {token} RUSAGE_ONLY"),
            ],
            TIMEOUT,
        )?;
        Ok(Self(Some(Gate {
            stream,
            token,
            mode: ["PMU", "RUSAGE_ONLY"][mode],
            boundary,
            armed_ns: None,
            start: None,
            finished: false,
        })))
    }
    pub(crate) fn path(&self, ordinary: &str) -> String {
        if let Some(gate) = &self.0 {
            format!("{ordinary}{}", gate.boundary.suffix())
        } else {
            ordinary.into()
        }
    }
    /// Arm the explicit K0 diagnostic before warmup. Legacy/disabled are no-ops.
    pub(crate) fn before_warmup(&mut self) -> Result<()> {
        if let Some(gate) = &mut self.0
            && gate.boundary != Boundary::Measurement
        {
            if gate.armed_ns.is_some() || gate.start.is_some() || gate.finished {
                return Err(fail());
            }
            exchange(
                &mut gate.stream,
                &format!("ARM {}", gate.token),
                &[format!("ARMED {} {}", gate.token, gate.mode)],
                TIMEOUT,
            )?;
            gate.armed_ns = Some(monotonic_ns().ok_or_else(fail)?);
        }
        Ok(())
    }
    pub(crate) fn begin(&mut self) -> Result<()> {
        if let Some(gate) = &mut self.0 {
            if gate.start.is_some() || gate.finished {
                return Err(fail());
            }
            if gate.boundary != Boundary::Measurement && gate.armed_ns.is_none() {
                return Err(fail());
            }
            if gate.boundary != Boundary::Continuous {
                exchange(
                    &mut gate.stream,
                    &format!("START {}", gate.token),
                    &[format!("GO {} {}", gate.token, gate.mode)],
                    TIMEOUT,
                )?;
            }
            let start = monotonic_ns().ok_or_else(fail)?;
            gate.start = Some((start, thread_usage()));
        }
        Ok(())
    }
    pub(crate) fn end(&mut self, elapsed_ns: u64) -> Result<()> {
        if let Some(gate) = &mut self.0 {
            let after = thread_usage();
            let end = monotonic_ns().ok_or_else(fail)?;
            let (start, before) = gate.start.take().ok_or_else(fail)?;
            if end
                .checked_sub(start)
                .is_none_or(|length| length < elapsed_ns)
            {
                return Err(fail());
            }
            let mut message = serde_json::json!({"phase":"STOP", "token":gate.token,
                "start_ns":start, "end_ns":end, "elapsed_ns":elapsed_ns,
                "before":before, "after":after});
            if gate.boundary != Boundary::Measurement {
                let armed = gate.armed_ns.ok_or_else(fail)?;
                if armed > start {
                    return Err(fail());
                }
                message["armed_ns"] = armed.into();
            }
            let message = message.to_string();
            exchange(
                &mut gate.stream,
                &message,
                &[format!("DONE {} {}", gate.token, gate.mode)],
                TIMEOUT,
            )?;
            gate.finished = true;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "measurement_window_tests.rs"]
mod tests;
