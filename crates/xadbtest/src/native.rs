//! Controller-owned native streams and external event evidence.
//! Successful transport closure is not an oracle verdict or durability qualification.
use crate::{file_sha256, process::OwnedChild};
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use xadbtest_protocol::{
    engine::{Action, MAX_PENDING, MAX_REQUESTS, Request, Response, VERSION},
    frame,
};

#[derive(Clone, Debug, Serialize)]
pub struct Limits {
    pub input_bytes: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub journal_bytes: usize,
    pub request_ms: u64,
    pub run_ms: u64,
    pub drain_ms: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            input_bytes: 64 * 1024 * 1024,
            stdout_bytes: 64 * 1024 * 1024,
            stderr_bytes: 1024 * 1024,
            journal_bytes: 64 * 1024 * 1024,
            request_ms: 5000,
            run_ms: 60000,
            drain_ms: 5000,
        }
    }
}
#[cfg(feature = "self-test")]
#[derive(Clone, Debug, Default)]
pub struct TestOptions {
    pub worker_args: Vec<String>,
    pub limits: Limits,
    pub fail_journal_at_event: Option<u64>,
    pub fail_cleanup: bool,
}

pub struct Options {
    pub worker: PathBuf,
    pub expected_worker_sha256: String,
    pub artifacts: PathBuf,
    pub run: [u8; 16],
    #[cfg(feature = "self-test")]
    pub test: TestOptions,
}
impl Options {
    pub fn new(
        worker: PathBuf,
        expected_worker_sha256: String,
        artifacts: PathBuf,
        run: [u8; 16],
    ) -> Self {
        Self {
            worker,
            expected_worker_sha256,
            artifacts,
            run,
            #[cfg(feature = "self-test")]
            test: TestOptions::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ErrorKind {
    Configuration,
    Identity,
    Protocol,
    Timeout,
    Limit,
    Io,
    Cleanup,
    Closed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Error {
    pub kind: ErrorKind,
    pub detail: String,
    /// Cleanup cannot replace the primary observation failure.
    pub cleanup: Option<String>,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        fault(ErrorKind::Io, value.to_string())
    }
}
fn fault(kind: ErrorKind, detail: impl Into<String>) -> Error {
    Error {
        kind,
        detail: detail.into(),
        cleanup: None,
    }
}
type Result<T> = std::result::Result<T, Error>;

fn create(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}
fn save(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let mut file = create(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.flush()
}
struct Pending {
    actor: u8,
    began: Instant,
    sent: bool,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub directory: PathBuf,
    pub worker_sha256: String,
    pub requests: u64,
    pub observations: u64,
    pub history_sha256: String,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub completion: &'static str,
}

/// Streaming process ownership. Failures are sticky and trigger explicit cleanup.
/// Drop is a best-effort fallback and leaves an incomplete run without a completion report.
pub struct Session {
    child: OwnedChild,
    input: UnixStream,
    output: UnixStream,
    errors: UnixStream,
    output_log: File,
    error_log: File,
    history: File,
    directory: PathBuf,
    worker_hash: String,
    run: [u8; 16],
    limits: Limits,
    began: Instant,
    decoder: frame::Decoder<Response>,
    pending: BTreeMap<u64, Pending>,
    inbox: VecDeque<Response>,
    issued: u64,
    observed: u64,
    input_bytes: usize,
    output_bytes: usize,
    error_bytes: usize,
    history_bytes: usize,
    event: u64,
    output_eof: bool,
    error_eof: bool,
    finished: bool,
    failure: Option<Error>,
    #[cfg(feature = "self-test")]
    test: TestOptions,
}
impl Session {
    /// Copy and verify the candidate before spawning it in an owned subdirectory.
    pub fn start(options: Options) -> Result<Self> {
        if options.run == [0; 16]
            || options.expected_worker_sha256.len() != 64
            || !options
                .expected_worker_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(fault(ErrorKind::Configuration, "invalid run/hash"));
        }
        #[cfg(feature = "self-test")]
        let (limits, args) = (
            options.test.limits.clone(),
            options.test.worker_args.clone(),
        );
        #[cfg(not(feature = "self-test"))]
        let (limits, args) = (Limits::default(), Vec::<String>::new());
        if [
            limits.input_bytes,
            limits.stdout_bytes,
            limits.stderr_bytes,
            limits.journal_bytes,
        ]
        .contains(&0)
            || [limits.request_ms, limits.run_ms, limits.drain_ms].contains(&0)
            || limits.run_ms > 60000
            || limits.request_ms > 5000
            || limits.drain_ms > 5000
            || limits.input_bytes > 64 * 1024 * 1024
            || limits.stdout_bytes > 64 * 1024 * 1024
            || limits.journal_bytes > 64 * 1024 * 1024
            || limits.stderr_bytes > 1024 * 1024
        {
            return Err(fault(
                ErrorKind::Configuration,
                "limits may only reduce the fixed profile",
            ));
        }
        fs::create_dir_all(&options.artifacts)?;
        let directory = tempfile::Builder::new()
            .prefix("native-")
            .tempdir_in(options.artifacts.canonicalize()?)?
            .keep();
        let work = directory.join("work");
        fs::create_dir(&work)?;
        let binary = directory.join("worker");
        fs::copy(&options.worker, &binary)?;
        let worker_hash = file_sha256(&binary)?;
        if worker_hash != options.expected_worker_sha256 {
            let error = fault(
                ErrorKind::Identity,
                "retained worker does not match expected SHA-256",
            );
            save(&directory.join("failure.json"), &error)?;
            return Err(error);
        }
        save(
            &directory.join("manifest.json"),
            &serde_json::json!({
                "scope":"NATIVE_TRANSPORT_NOT_PRODUCT_VERDICT", "run":options.run,
                "source_sha256":env!("XADBTEST_SOURCE_SHA256"), "worker_sha256":worker_hash,
                "toolchain":env!("XADBTEST_TOOLCHAIN"), "target":env!("XADBTEST_TARGET"),
                "profile":env!("XADBTEST_PROFILE"), "self_test":cfg!(feature="self-test"),
                "limits":limits, "worker_args":args,
            }),
        )?;
        let output_log = create(&directory.join("stdout.bin"))?;
        let error_log = create(&directory.join("stderr.bin"))?;
        let history = create(&directory.join("history.bin"))?;
        let began = Instant::now();
        let (child, input, output, errors) = OwnedChild::spawn_stream(&binary, &work, &args)?;
        Ok(Self {
            child,
            input,
            output,
            errors,
            output_log,
            error_log,
            history,
            directory,
            worker_hash,
            run: options.run,
            limits,
            began,
            decoder: frame::Decoder::new(),
            pending: BTreeMap::new(),
            inbox: VecDeque::new(),
            issued: 0,
            observed: 0,
            input_bytes: 0,
            output_bytes: 0,
            error_bytes: 0,
            history_bytes: 0,
            event: 0,
            output_eof: false,
            error_eof: false,
            finished: false,
            failure: None,
            #[cfg(feature = "self-test")]
            test: options.test,
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn checked<T>(&mut self, operation: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.finished {
            return Err(fault(ErrorKind::Closed, "session closed"));
        }
        match operation(self) {
            Ok(value) => Ok(value),
            Err(mut error) => {
                if let Err(cleanup) = self.cleanup() {
                    error.cleanup = Some(cleanup.detail);
                }
                let _ = save(&self.directory.join("failure.json"), &error);
                self.failure = Some(error.clone());
                Err(error)
            }
        }
    }

    /// Append an Invoke before any request byte, then send while draining all streams.
    pub fn invoke(&mut self, actor: u8, action: Action) -> Result<u64> {
        self.checked(|this| {
            this.deadlines()?;
            if usize::from(actor) >= MAX_PENDING
                || this.issued == MAX_REQUESTS
                || this.pending.len() == MAX_PENDING
                || this.pending.values().any(|p| p.actor == actor)
            {
                return Err(fault(ErrorKind::Protocol, "actor/request/pending bound"));
            }
            let id = this.issued + 1;
            let request = Request {
                version: VERSION,
                run: this.run,
                request: id,
                actor,
                action,
            };
            let bytes =
                frame::encode(&request).map_err(|e| fault(ErrorKind::Protocol, e.to_string()))?;
            if bytes.len() > this.limits.input_bytes - this.input_bytes {
                return Err(fault(ErrorKind::Limit, "input budget"));
            }
            let began = Instant::now();
            this.record(1, id, &bytes)?;
            this.issued = id;
            this.pending.insert(
                id,
                Pending {
                    actor,
                    began,
                    sent: false,
                },
            );
            let mut offset = 0;
            while offset != bytes.len() {
                this.deadlines()?;
                match this.input.write(&bytes[offset..]) {
                    Ok(0) => return Err(fault(ErrorKind::Io, "stdin made no progress")),
                    Ok(count) => {
                        offset += count;
                        this.input_bytes += count;
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
                if offset == bytes.len() {
                    this.record(2, id, &[])?;
                    this.pending
                        .get_mut(&id)
                        .expect("sending request pending")
                        .sent = true;
                }
                this.pump()?;
                if offset != bytes.len() {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            Ok(id)
        })
    }

    /// Receive the next recorded response, allowing other actors to complete first.
    pub fn receive(&mut self) -> Result<Response> {
        self.checked(|this| {
            loop {
                this.deadlines()?;
                if let Some(response) = this.inbox.pop_front() {
                    return Ok(response);
                }
                if this.pending.is_empty() {
                    return Err(fault(ErrorKind::Protocol, "no pending response"));
                }
                this.pump()?;
                thread::sleep(Duration::from_millis(2));
            }
        })
    }

    fn deadlines(&self) -> Result<()> {
        if self.began.elapsed() >= Duration::from_millis(self.limits.run_ms)
            || self
                .pending
                .values()
                .any(|p| p.began.elapsed() >= Duration::from_millis(self.limits.request_ms))
        {
            return Err(fault(ErrorKind::Timeout, "run/request deadline"));
        }
        Ok(())
    }

    // Binary journal v1: kind:u8, sequence:u64 BE, monotonic-nanos:u64 BE,
    // request:u64 BE, payload-length:u32 BE, payload (framed envelope for Invoke/Observe).
    fn record(&mut self, kind: u8, request: u64, payload: &[u8]) -> Result<()> {
        #[cfg(feature = "self-test")]
        if self.test.fail_journal_at_event == Some(self.event) {
            return Err(fault(ErrorKind::Io, "injected journal write failure"));
        }
        let length = 29usize
            .checked_add(payload.len())
            .ok_or_else(|| fault(ErrorKind::Limit, "journal overflow"))?;
        if length > self.limits.journal_bytes - self.history_bytes {
            return Err(fault(ErrorKind::Limit, "journal budget"));
        }
        self.history.write_all(&[kind])?;
        self.history.write_all(&self.event.to_be_bytes())?;
        let nanos = self.began.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        self.history.write_all(&nanos.to_be_bytes())?;
        self.history.write_all(&request.to_be_bytes())?;
        self.history
            .write_all(&(payload.len() as u32).to_be_bytes())?;
        self.history.write_all(payload)?;
        self.history.flush()?;
        self.history_bytes += length;
        self.event += 1;
        Ok(())
    }

    fn pump(&mut self) -> Result<()> {
        self.deadlines()?;
        let mut buffer = [0; 8192];
        for _ in 0..8 {
            if self.output_eof {
                break;
            }
            match self.output.read(&mut buffer) {
                Ok(0) => {
                    self.output_eof = true;
                    self.decoder
                        .finish()
                        .map_err(|e| fault(ErrorKind::Protocol, e.to_string()))?;
                    if !self.pending.is_empty() {
                        return Err(fault(ErrorKind::Protocol, "EOF with unanswered request"));
                    }
                    break;
                }
                Ok(count) => {
                    if count > self.limits.stdout_bytes - self.output_bytes {
                        return Err(fault(ErrorKind::Limit, "stdout budget"));
                    }
                    self.output_log.write_all(&buffer[..count])?;
                    self.output_bytes += count;
                    let mut offset = 0;
                    while offset != count {
                        let (used, message) = self
                            .decoder
                            .feed(&buffer[offset..count])
                            .map_err(|e| fault(ErrorKind::Protocol, e.to_string()))?;
                        offset += used;
                        if let Some(response) = message {
                            let pending = self.pending.get(&response.request).ok_or_else(|| {
                                fault(ErrorKind::Protocol, "unknown/duplicate response")
                            })?;
                            if response.run != self.run
                                || response.actor != pending.actor
                                || !pending.sent
                            {
                                return Err(fault(
                                    ErrorKind::Protocol,
                                    "response identity or send order",
                                ));
                            }
                            let bytes = frame::encode(&response)
                                .map_err(|e| fault(ErrorKind::Protocol, e.to_string()))?;
                            self.record(3, response.request, &bytes)?;
                            self.pending.remove(&response.request);
                            self.observed += 1;
                            self.inbox.push_back(response);
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        for _ in 0..8 {
            if self.error_eof {
                break;
            }
            match self.errors.read(&mut buffer) {
                Ok(0) => {
                    self.error_eof = true;
                    break;
                }
                Ok(count) => {
                    if count > self.limits.stderr_bytes - self.error_bytes {
                        return Err(fault(ErrorKind::Limit, "stderr budget"));
                    }
                    self.error_log.write_all(&buffer[..count])?;
                    self.error_bytes += count;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        self.deadlines()
    }

    fn cleanup(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        let result = self
            .child
            .finish()
            .map_err(|e| fault(ErrorKind::Cleanup, e.to_string()));
        self.finished = true;
        #[cfg(feature = "self-test")]
        if self.test.fail_cleanup {
            return Err(fault(ErrorKind::Cleanup, "injected cleanup failure"));
        }
        result.map(|_| ())
    }

    /// Planned worker loss is recorded after all preceding responses have been journaled.
    /// This proves process ownership/history ordering only, never power-loss persistence.
    pub fn kill_worker(&mut self) -> Result<()> {
        self.checked(|this| {
            this.pump()?;
            this.record(4, 0, &[])?;
            this.cleanup()
        })
    }

    /// Close after every response was delivered to the caller. Reap only after group signal.
    pub fn finish(mut self) -> Result<Report> {
        self.checked(|this| {
            if !this.pending.is_empty() || !this.inbox.is_empty() || this.issued == 0 {
                return Err(fault(
                    ErrorKind::Protocol,
                    "unconsumed requests/responses or empty run",
                ));
            }
            this.input.shutdown(Shutdown::Write)?;
            let drain = Instant::now();
            loop {
                this.pump()?;
                if this.output_eof && this.error_eof && this.child.exited()? {
                    break;
                }
                if drain.elapsed() >= Duration::from_millis(this.limits.drain_ms) {
                    return Err(fault(ErrorKind::Timeout, "normal close drain deadline"));
                }
                thread::sleep(Duration::from_millis(2));
            }
            let status = this
                .child
                .finish()
                .map_err(|e| fault(ErrorKind::Cleanup, e.to_string()))?;
            this.finished = true;
            #[cfg(feature = "self-test")]
            if this.test.fail_cleanup {
                return Err(fault(ErrorKind::Cleanup, "injected cleanup failure"));
            }
            if !status.success() {
                return Err(fault(ErrorKind::Protocol, "worker exit was not successful"));
            }
            this.record(5, 0, &[])?;
            let report = Report {
                directory: this.directory.clone(),
                worker_sha256: this.worker_hash.clone(),
                requests: this.issued,
                observations: this.observed,
                history_sha256: file_sha256(&this.directory.join("history.bin"))?,
                stdout_sha256: file_sha256(&this.directory.join("stdout.bin"))?,
                stderr_sha256: file_sha256(&this.directory.join("stderr.bin"))?,
                completion: "TRANSPORT_CLOSED_NO_PRODUCT_VERDICT",
            };
            save(&this.directory.join("completion.json"), &report)?;
            Ok(report)
        })
    }
}
