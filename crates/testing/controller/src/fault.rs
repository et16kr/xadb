//! Controller-owned native cut handshake. Only a flushed, matched Reached permits
//! a cut-qualified kill. Missing witnesses are incomplete, never rejected or PASS.
//! Native fault execution perturbs scheduling and is excluded from PF evidence.
mod image;
pub use image::CapturedImage;

use crate::{
    durable::Session,
    native::{Error, ErrorKind},
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    thread,
    time::{Duration, Instant},
};
use xadbtest_protocol::{
    durable::{Action, Observation as Product},
    fault as wire, frame,
};
fn error(kind: ErrorKind, detail: impl Into<String>) -> Error {
    Error {
        kind,
        detail: detail.into(),
        cleanup: None,
    }
}
fn io_error(error: io::Error) -> Error {
    self::error(ErrorKind::Io, error.to_string())
}
fn protocol(detail: &str) -> Error {
    error(ErrorKind::Protocol, detail)
}
fn create(path: impl AsRef<std::path::Path>) -> Result<File, Error> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)
}

/// One control connection attached to the exact process owned by a native session.
/// Dropping this object does not kill arbitrary processes; the native owner keeps
/// its usual process-group cleanup obligation and pending invocation evidence.
pub struct Control {
    stream: UnixStream,
    run: [u8; 16],
    worker_id: u32,
    directory: std::path::PathBuf,
    token: [u8; 16],
    plan: wire::Plan,
    case_sha256: String,
    qualified_kill: bool,
    journal: File,
    raw: File,
    sent: File,
    sequence: u64,
    began: Instant,
    bytes: usize,
    buffer: Vec<u8>,
    reached: Option<wire::Reached>,
    released: bool,
}
impl Control {
    /// Create a new socket in this retained run directory, then install one plan.
    /// Existing paths are never unlinked/reused, including on failure. The worker
    /// receives the socket location from the controller-owned environment only.
    ///
    /// # Errors
    /// Identity/framing/timeout/socket errors fail the native run and leave pending
    /// product effects unobserved. This function never manufactures an abort.
    pub fn connect(
        session: &mut Session,
        token: [u8; 16],
        plan: wire::Plan,
    ) -> Result<Self, Error> {
        let result = Self::connect_inner(session, token, plan);
        match result {
            Ok(control) => Ok(control),
            Err(error) => session.reject(error),
        }
    }
    fn connect_inner(
        session: &mut Session,
        token: [u8; 16],
        plan: wire::Plan,
    ) -> Result<Self, Error> {
        plan.validate().map_err(|e| protocol(&e.to_string()))?;
        if token == [0; 16] {
            return Err(protocol("zero fault control token"));
        }
        let listener =
            UnixListener::bind(session.directory().join("fault-control.sock")).map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        let journal = create(session.directory().join("fault-control.jsonl"))?;
        let raw = create(session.directory().join("fault-control.bin"))?;
        let sent = create(session.directory().join("fault-release.bin"))?;
        let run = session.run_identity();
        let began = Instant::now();
        let case_sha256 = crate::file_sha256(&session.directory().join("fault-case.json"))?;
        // Predeclared intent is retained before the ConfigureFault invocation.
        let mut initial=serde_json::to_vec(&serde_json::json!({"sequence":0,"elapsed_ns":0,"kind":"plan","version":wire::VERSION,"run":run,"token":token,"plan":plan,"case_sha256":case_sha256})).map_err(|e|protocol(&e.to_string()))?;
        initial.push(b'\n');
        let mut journal = journal;
        journal
            .write_all(&initial)
            .and_then(|()| journal.flush())
            .map_err(io_error)?;
        let request = session.invoke(0, Action::ConfigureFault { token, plan }.into())?;
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                Err(error) => return Err(io_error(error)),
            }
            session.poll()?;
            if began.elapsed() > Duration::from_secs(5) {
                return Err(error(
                    ErrorKind::Timeout,
                    "fault control connection deadline",
                ));
            }
            thread::sleep(Duration::from_millis(1));
        };
        stream.set_nonblocking(true).map_err(io_error)?;
        let mut control = Self {
            stream,
            run,
            worker_id: session.worker_id(),
            directory: session.directory().to_owned(),
            token,
            plan,
            case_sha256,
            qualified_kill: false,
            journal,
            raw,
            sent,
            sequence: 1,
            began,
            bytes: initial.len(),
            buffer: Vec::new(),
            reached: None,
            released: false,
        };
        match control.next(session)?.event {
            wire::Event::Connected { process_id } if process_id == session.worker_id() => (),
            _ => return Err(protocol("fault control process identity")),
        }
        let response = session.receive()?;
        if response.request != request
            || response.actor != 0
            || response.passed_gate.is_some()
            || response.observation != (Product::Done {})
        {
            return Err(protocol("fault configure response"));
        }
        Ok(control)
    }
    fn owner(&self, session: &Session) -> Result<(), Error> {
        if self.run != session.run_identity()
            || self.worker_id != session.worker_id()
            || self.directory != session.directory()
        {
            // Do not route this through Session::reject: the supplied session is
            // not ours, so even cleanup would target an unrelated process group.
            return Err(error(
                ErrorKind::Configuration,
                "fault control belongs to a different native session",
            ));
        }
        Ok(())
    }
    fn record(&mut self, kind: &str, value: serde_json::Value) -> Result<(), Error> {
        let mut bytes=serde_json::to_vec(&serde_json::json!({"sequence":self.sequence,"elapsed_ns":self.began.elapsed().as_nanos().min(u64::MAX as u128) as u64,"kind":kind,"value":value})).map_err(|e|protocol(&e.to_string()))?;
        bytes.push(b'\n');
        if self
            .bytes
            .checked_add(bytes.len())
            .is_none_or(|count| count > wire::MAX_CONTROL_BYTES)
        {
            return Err(error(ErrorKind::Limit, "fault journal byte limit"));
        }
        self.journal
            .write_all(&bytes)
            .and_then(|()| self.journal.flush())
            .map_err(io_error)?;
        self.bytes += bytes.len();
        self.sequence += 1;
        Ok(())
    }
    fn next(&mut self, session: &mut Session) -> Result<wire::Observation, Error> {
        let began = Instant::now();
        loop {
            session.poll()?;
            if self.buffer.len() >= 4 {
                let length =
                    u32::from_be_bytes(self.buffer[..4].try_into().expect("four bytes")) as usize;
                if length == 0 || length > wire::MAX_MESSAGE_BYTES - 4 {
                    return Err(error(ErrorKind::Limit, "fault control frame limit"));
                }
                if self.buffer.len() >= length + 4 {
                    let bytes: Vec<u8> = self.buffer.drain(..length + 4).collect();
                    let event: wire::Observation = frame::read(&mut bytes.as_slice())
                        .map_err(|e| protocol(&e.to_string()))?
                        .ok_or_else(|| protocol("empty fault observation"))?;
                    if event.run != self.run || event.token != self.token {
                        return Err(protocol("fault observation identity"));
                    }
                    // Every observation is flushed before being handed to callers.
                    self.record(
                        "observe",
                        serde_json::to_value(&event).map_err(|e| protocol(&e.to_string()))?,
                    )?;
                    return Ok(event);
                }
            }
            let mut bytes = [0; 1024];
            match self.stream.read(&mut bytes) {
                Ok(0) => return Err(protocol("fault control EOF before expected witness")),
                Ok(count) => {
                    if self
                        .bytes
                        .checked_add(count)
                        .is_none_or(|total| total > wire::MAX_CONTROL_BYTES)
                    {
                        return Err(error(ErrorKind::Limit, "fault raw stream byte limit"));
                    }
                    self.raw
                        .write_all(&bytes[..count])
                        .and_then(|()| self.raw.flush())
                        .map_err(io_error)?;
                    self.bytes += count;
                    self.buffer.extend_from_slice(&bytes[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                Err(error) => return Err(io_error(error)),
            }
            if began.elapsed() > Duration::from_secs(5) {
                return Err(error(ErrorKind::Timeout, "fault witness deadline"));
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
    /// Wait for this exact externally invoked operation to reach the selected cut.
    ///
    /// # Errors
    /// A different request/actor/cut, timeout or EOF is incomplete evidence and
    /// triggers only the native owner's cleanup, never a cut-qualified kill.
    pub fn reached(
        &mut self,
        session: &mut Session,
        request: u64,
        actor: u8,
    ) -> Result<wire::Reached, Error> {
        self.owner(session)?;
        let result = (|| {
            if self.reached.is_some() {
                return Err(protocol("fault witness already consumed"));
            }
            let event = self.next(session)?;
            let wire::Event::Reached { witness } = event.event else {
                return Err(protocol("expected reached fault witness"));
            };
            if witness.plan != self.plan
                || witness.request != request
                || witness.actor != actor
                || self.plan.actor.is_some_and(|expected| expected != actor)
                || self
                    .plan
                    .directory
                    .is_some_and(|expected| expected != witness.directory)
            {
                return Err(protocol("fault witness mismatch"));
            }
            self.reached = Some(witness.clone());
            Ok(witness)
        })();
        match result {
            Ok(witness) => Ok(witness),
            Err(error) => session.reject(error),
        }
    }
    /// Resume the blocked port or return an injected I/O error after its observed
    /// prefix/effect. Continue after Partial returns the actual short count so the
    /// production write-all loop performs its normal suffix retry.
    ///
    /// # Errors
    /// No Reached, a repeated release or missing/mismatched Released is incomplete.
    pub fn release(
        &mut self,
        session: &mut Session,
        decision: wire::Decision,
    ) -> Result<(), Error> {
        self.owner(session)?;
        let result = (|| {
            if self.reached.is_none() || self.released {
                return Err(protocol("fault release without a fresh reached witness"));
            }
            let release = wire::Release {
                version: wire::VERSION,
                run: self.run,
                token: self.token,
                decision,
            };
            let bytes = frame::encode(&release).map_err(|e| protocol(&e.to_string()))?;
            self.record(
                "release_intent",
                serde_json::to_value(&release).map_err(|e| protocol(&e.to_string()))?,
            )?;
            self.sent
                .write_all(&bytes)
                .and_then(|()| self.sent.flush())
                .map_err(io_error)?;
            let began = Instant::now();
            let mut offset = 0;
            while offset < bytes.len() {
                match self.stream.write(&bytes[offset..]) {
                    Ok(0) => return Err(protocol("fault release made no progress")),
                    Ok(count) => offset += count,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => return Err(io_error(error)),
                };
                session.poll()?;
                if began.elapsed() > Duration::from_secs(5) {
                    return Err(error(ErrorKind::Timeout, "fault release deadline"));
                }
            }
            match self.next(session)?.event {
                wire::Event::Released { decision: actual } if actual == decision => {
                    self.released = true;
                    Ok(())
                }
                _ => Err(protocol("fault release acknowledgement mismatch")),
            }
        })();
        match result {
            Ok(()) => Ok(()),
            Err(error) => session.reject(error),
        }
    }
    /// Kill only the native owner's process group while the matched port remains
    /// blocked. The reached and kill-intent receipts are flushed first.
    ///
    /// # Errors
    /// An absent/released witness cannot authorize a cut-qualified kill. Owned
    /// process cleanup failure remains failure, even with a valid reached witness.
    pub fn kill_reached(&mut self, session: &mut Session) -> Result<wire::Reached, Error> {
        self.owner(session)?;
        let result = (|| {
            let witness = self
                .reached
                .clone()
                .filter(|_| !self.released)
                .ok_or_else(|| protocol("cut-qualified kill requires blocked reached witness"))?;
            self.record(
                "kill_intent",
                serde_json::to_value(&witness).map_err(|e| protocol(&e.to_string()))?,
            )?;
            session.kill_worker()?;
            self.qualified_kill = true;
            self.released = true;
            Ok(witness)
        })();
        match result {
            Ok(witness) => Ok(witness),
            Err(error) => session.reject(error),
        }
    }
}
