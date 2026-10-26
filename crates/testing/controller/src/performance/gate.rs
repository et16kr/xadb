//! Optional supervisor-owned slot gate for measurement profile B campaigns.
//! The controller never computes environment validity. It records every exchanged
//! line and binds the supervisor's verdict digest into the bundle before analysis.
use super::{Order, Recipe, Result};
use crate::{file_sha256, sha256};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub const PROTOCOL: &str = "xadb-slot-gate-v1";
pub const PROFILE_BYTES: usize = 64 * 1024;
/// Supervisor reads this FIFO; the controller writes READY and DONE lines.
pub const REQUEST_FIFO: &str = "controller.fifo";
/// Supervisor writes GO, ABORT and ACK lines; the controller reads them.
pub const REPLY_FIFO: &str = "supervisor.fifo";
pub const NOT_RUN: &str = "NOT_RUN: slot gate ended before this slot";
pub const ABORTED: &str = "NOT_RUN: slot gate ABORT before worker start";
// Longer than the supervisor's 10-minute idle wait, so ABORT arrives before expiry.
const GO_DEADLINE_MS: u64 = 660_000;
const ACK_DEADLINE_MS: u64 = 60_000;
const LINE_BYTES: usize = 128;
const TRANSCRIPT_BYTES: usize = 64 * 1024;
// Sleeping poll keeps the waiting controller from adding busy ticks on the
// measured CPU during the supervisor's pre-pair idle window.
const POLL: Duration = Duration::from_millis(5);

#[derive(Clone)]
pub struct Options {
    pub profile: PathBuf,
    pub profile_sha256: String,
    /// Supervisor-owned directory holding both FIFOs.
    pub directory: PathBuf,
}

/// Frozen profile; deadlines are the fixed protocol values in release builds.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub protocol: String,
    pub go_deadline_ms: u64,
    pub ack_deadline_ms: u64,
    /// Supervisor-owned environment rules, retained and hashed but not interpreted here.
    pub environment: serde_json::Value,
}
impl Profile {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > PROFILE_BYTES {
            return Err("slot gate profile exceeds 64 KiB".into());
        }
        let profile: Self = serde_json::from_slice(bytes)?;
        // Self-test builds may only shorten deadlines, like native limits.
        let deadline = |value: u64, fixed: u64| {
            if cfg!(feature = "self-test") {
                (1..=fixed).contains(&value)
            } else {
                value == fixed
            }
        };
        if profile.schema_version != 1
            || profile.protocol != PROTOCOL
            || !profile.environment.is_object()
            || !deadline(profile.go_deadline_ms, GO_DEADLINE_MS)
            || !deadline(profile.ack_deadline_ms, ACK_DEADLINE_MS)
        {
            return Err("unsupported slot gate profile".into());
        }
        Ok(profile)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Party {
    Controller,
    Supervisor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub from: Party,
    /// Controller monotonic time since the gate opened; never compared with worker clocks.
    pub ns: u64,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transcript {
    pub schema_version: u32,
    pub protocol: String,
    pub profile_sha256: String,
    pub lines: Vec<Line>,
    pub environment_sha256: Option<String>,
    pub termination: Option<String>,
}
/// Bundle index binding; `environment_sha256` is absent only after a gate failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub profile_sha256: String,
    pub environment_sha256: Option<String>,
}

pub enum Admission {
    Go,
    Abort,
}

fn hex(run: [u8; 16]) -> String {
    run.iter().map(|b| format!("{b:02x}")).collect()
}

fn fifo(directory: &Path, name: &str) -> Result<PathBuf> {
    let path = directory.join(name);
    let metadata = fs::symlink_metadata(&path)?;
    // `/proc/self` is owned by this process's effective UID.
    let user = fs::metadata("/proc/self")?.uid();
    if !metadata.file_type().is_fifo() || metadata.uid() != user || metadata.mode() & 0o077 != 0 {
        return Err("slot gate FIFO must be an owner-only FIFO of this user".into());
    }
    Ok(path)
}

pub struct Gate {
    profile: Profile,
    request: File,
    reply: File,
    pending: Vec<u8>,
    began: Instant,
    transcript: Transcript,
}
impl Gate {
    /// Verify the frozen profile and attach to an already listening supervisor.
    /// Returns the exact profile bytes for retention in the bundle.
    pub fn open(options: &Options) -> Result<(Self, Vec<u8>)> {
        let mut bytes = Vec::new();
        File::open(&options.profile)?
            .take(PROFILE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        if sha256(&bytes) != options.profile_sha256 {
            return Err("slot gate profile identity mismatch".into());
        }
        let profile = Profile::parse(&bytes)?;
        // Without a reader, a nonblocking FIFO write open fails instead of waiting.
        let request = OpenOptions::new()
            .write(true)
            .custom_flags(nix::libc::O_NONBLOCK)
            .open(fifo(&options.directory, REQUEST_FIFO)?)
            .map_err(|e| format!("slot gate supervisor is not reading: {e}"))?;
        let reply = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NONBLOCK)
            .open(fifo(&options.directory, REPLY_FIFO)?)?;
        Ok((
            Self {
                profile,
                request,
                reply,
                pending: Vec::new(),
                began: Instant::now(),
                transcript: Transcript {
                    schema_version: 1,
                    protocol: PROTOCOL.into(),
                    profile_sha256: options.profile_sha256.clone(),
                    lines: Vec::new(),
                    environment_sha256: None,
                    termination: None,
                },
            },
            bytes,
        ))
    }
    fn now(&self) -> u64 {
        self.began.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }
    // Returns false at EOF. Any byte is retained for the unsolicited-reply check.
    fn fill(&mut self) -> Result<bool> {
        let mut buffer = [0; LINE_BYTES];
        loop {
            match self.reply.read(&mut buffer) {
                Ok(0) => return Ok(false),
                Ok(count) => {
                    self.pending.extend_from_slice(&buffer[..count]);
                    if self.pending.len() > LINE_BYTES {
                        return Err("slot gate reply exceeds line bound".into());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(true),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    fn send(&mut self, text: String, deadline: Duration) -> Result<()> {
        // A reply before its request would let a stale line bind a later slot.
        if !self.fill()? {
            return Err("slot gate EOF".into());
        }
        if !self.pending.is_empty() {
            return Err("unsolicited slot gate bytes".into());
        }
        let started = Instant::now();
        let bytes = format!("{text}\n");
        loop {
            // Lines are below PIPE_BUF, so a FIFO write is all-or-nothing.
            match self.request.write(bytes.as_bytes()) {
                Ok(count) if count == bytes.len() => break,
                Ok(_) => return Err("partial slot gate write".into()),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(format!("slot gate write failed: {error}").into()),
            }
            if started.elapsed() >= deadline {
                return Err("slot gate write deadline".into());
            }
            thread::sleep(POLL);
        }
        let ns = self.now();
        self.transcript.lines.push(Line {
            from: Party::Controller,
            ns,
            text,
        });
        Ok(())
    }
    fn receive(&mut self, deadline: Duration) -> Result<String> {
        let started = Instant::now();
        loop {
            let open = self.fill()?;
            if let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
                if end + 1 != self.pending.len() {
                    return Err("unsolicited slot gate bytes".into());
                }
                let line = String::from_utf8(self.pending[..end].to_vec())?;
                self.pending.clear();
                if !line.bytes().all(|b| b.is_ascii_graphic() || b == b' ') {
                    return Err("slot gate reply is not printable ASCII".into());
                }
                let ns = self.now();
                self.transcript.lines.push(Line {
                    from: Party::Supervisor,
                    ns,
                    text: line.clone(),
                });
                return Ok(line);
            }
            if !open {
                return Err("slot gate EOF".into());
            }
            if started.elapsed() >= deadline {
                return Err("slot gate deadline".into());
            }
            thread::sleep(POLL);
        }
    }
    /// Announce a slot and wait for the supervisor's admission before any worker starts.
    pub fn ready(&mut self, ordinal: usize, run: [u8; 16]) -> Result<Admission> {
        let deadline = Duration::from_millis(self.profile.go_deadline_ms);
        let slot = format!("{ordinal} {}", hex(run));
        self.send(format!("READY {slot}"), deadline)?;
        let reply = self.receive(deadline)?;
        if reply == format!("GO {slot}") {
            Ok(Admission::Go)
        } else if reply == format!("ABORT {slot}") {
            Ok(Admission::Abort)
        } else {
            Err("slot gate admission does not match ordinal/run".into())
        }
    }
    /// Close a slot. The last slot's ACK, or the ACK after ABORT, carries the verdict digest.
    pub fn done(&mut self, ordinal: usize, run: [u8; 16], last: bool) -> Result<()> {
        let deadline = Duration::from_millis(self.profile.ack_deadline_ms);
        let slot = format!("{ordinal} {}", hex(run));
        self.send(format!("DONE {slot}"), deadline)?;
        let reply = self.receive(deadline)?;
        let expected = format!("ACK {slot}");
        if !last {
            return if reply == expected {
                Ok(())
            } else {
                Err("slot gate ACK does not match ordinal/run".into())
            };
        }
        match reply
            .strip_prefix(&expected)
            .and_then(|rest| rest.strip_prefix(' '))
        {
            Some(digest) if super::digest(digest) => {
                self.transcript.environment_sha256 = Some(digest.into());
                Ok(())
            }
            _ => Err("final slot gate ACK lacks an environment verdict digest".into()),
        }
    }
    pub fn terminate(&mut self, reason: String) {
        self.transcript.termination = Some(reason);
    }
    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }
}

/// Worker scheduling priority from `/proc/<pid>/stat`; fields after the final ')' start at 3.
pub fn worker_nice(pid: u32) -> Result<i64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    nice_field(&stat)
}
pub fn nice_field(stat: &str) -> Result<i64> {
    let (_, fields) = stat.rsplit_once(')').ok_or("malformed /proc stat")?;
    Ok(fields
        .split_whitespace()
        .nth(16)
        .ok_or("missing nice field")?
        .parse()?)
}

/// Re-check a retained gated bundle without trusting any saved verdict.
/// `runs` maps execution ordinal to (run ID, directory present, failure).
pub fn check(
    directory: &Path,
    binding: &Binding,
    recipe: &Recipe,
    runs: &BTreeMap<usize, ([u8; 16], bool, Option<String>)>,
) -> Result<()> {
    let profile = fs::read(directory.join("gate-profile.json"))?;
    if sha256(&profile) != binding.profile_sha256
        || file_sha256(&directory.join("gate-profile.json"))? != binding.profile_sha256
    {
        return Err("retained slot gate profile identity mismatch".into());
    }
    Profile::parse(&profile)?;
    let mut bytes = Vec::new();
    File::open(directory.join("gate-transcript.json"))?
        .take(TRANSCRIPT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > TRANSCRIPT_BYTES {
        return Err("slot gate transcript exceeds 64 KiB".into());
    }
    let transcript: Transcript = serde_json::from_slice(&bytes)?;
    let Some(environment) = binding.environment_sha256.as_deref() else {
        return Err("gated bundle lacks an environment verdict; analysis withheld".into());
    };
    if transcript.schema_version != 1
        || transcript.protocol != PROTOCOL
        || transcript.profile_sha256 != binding.profile_sha256
        || transcript.environment_sha256.as_deref() != Some(environment)
        || !super::digest(environment)
        || transcript.termination.is_some()
    {
        return Err("slot gate transcript differs from bundle binding".into());
    }
    if transcript.lines.windows(2).any(|w| w[1].ns < w[0].ns) {
        return Err("slot gate transcript time regressed".into());
    }
    // Rebuild the only valid sequence from the frozen recipe and compare literally.
    let mut expected = Vec::new();
    let mut ended = false;
    for ordinal in 0..20 {
        let pair = &recipe.pairs[ordinal / 2];
        let candidate = (ordinal % 2 == 1) != (pair.order == Order::BA);
        let run = if candidate {
            pair.candidate_run
        } else {
            pair.baseline_run
        };
        let (record_run, has_directory, failure) =
            runs.get(&ordinal).ok_or("missing run for gate ordinal")?;
        if *record_run != run {
            return Err("slot gate ordinal differs from recipe order".into());
        }
        let slot = format!("{ordinal} {}", hex(run));
        if ended {
            if *has_directory || failure.as_deref() != Some(NOT_RUN) {
                return Err("slot after gate end was executed".into());
            }
            continue;
        }
        let aborted = transcript
            .lines
            .get(expected.len() + 1)
            .is_some_and(|line| line.text == format!("ABORT {slot}"));
        let consistent = if aborted {
            failure.as_deref() == Some(ABORTED) && !*has_directory
        } else {
            !matches!(failure.as_deref(), Some(NOT_RUN) | Some(ABORTED))
        };
        if !consistent {
            return Err("slot gate admission differs from run record".into());
        }
        expected.push((Party::Controller, format!("READY {slot}")));
        expected.push((
            Party::Supervisor,
            format!("{} {slot}", if aborted { "ABORT" } else { "GO" }),
        ));
        expected.push((Party::Controller, format!("DONE {slot}")));
        if aborted || ordinal == 19 {
            expected.push((Party::Supervisor, format!("ACK {slot} {environment}")));
            ended = true;
        } else {
            expected.push((Party::Supervisor, format!("ACK {slot}")));
        }
    }
    if transcript.lines.len() != expected.len()
        || transcript
            .lines
            .iter()
            .zip(&expected)
            .any(|(line, (from, text))| line.from != *from || line.text != *text)
    {
        return Err("slot gate transcript sequence mismatch".into());
    }
    Ok(())
}
