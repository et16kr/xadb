//! K0 recipe execution and independent checking of externally retained histories.
use crate::{file_sha256, native, sha256};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use test_protocol::{
    engine::{Action, Request, Response},
    frame,
};
use test_reference::catalog::{CatalogInputBudget, CatalogResult, check_catalog};

const RECIPE_BYTES: usize = 16 * 1024 * 1024;
const HISTORY_BYTES: usize = 64 * 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Versioned action-only input. Expected results never enter the worker request.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// Exactly one for the current K0 recipe.
    pub version: u32,
    /// Nonzero run identity, matched against every external envelope.
    pub run: [u8; 16],
    /// At most 1024 sequential actor-zero requests, beginning with Initialize.
    pub actions: Vec<Action>,
}
/// Explicit identities and owned artifact destination for a new native run.
pub struct Options {
    /// Versioned recipe JSON file.
    pub recipe: PathBuf,
    /// Expected SHA-256 of the original recipe bytes.
    pub recipe_sha256: String,
    /// Actual engine worker executable.
    pub worker: PathBuf,
    /// Expected SHA-256 of the worker bytes.
    pub worker_sha256: String,
    /// Parent of a fresh controller-owned run directory.
    pub artifacts: PathBuf,
}
/// K0 meaning verdict after transport completion and evidence identity validation.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    /// Report format version.
    pub schema_version: u32,
    /// Retained native session directory.
    pub directory: PathBuf,
    /// ACCEPTED, REJECTED, or INVALID; never a durability qualification.
    pub verdict: &'static str,
    /// Number of complete externally recorded request/response pairs.
    pub checked_pairs: usize,
    /// Optional zero-based failed pair.
    pub failed_pair: Option<usize>,
    /// Optional first failed requirement.
    pub reason: Option<&'static str>,
    /// Recipe input identity.
    pub recipe_sha256: String,
    /// Retained worker identity.
    pub worker_sha256: String,
    /// Observed external journal identity.
    pub history_sha256: String,
}
impl Report {
    /// Process status: success only for the bounded catalog verdict ACCEPTED.
    pub fn exit_code(&self) -> u8 {
        match self.verdict {
            "ACCEPTED" => 0,
            "REJECTED" => 1,
            _ => 2,
        }
    }
}
fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("artifact byte limit exceeded".into());
    }
    Ok(bytes)
}
fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}
fn json(path: &Path, limit: usize) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&bounded(path, limit)?)?)
}
fn parse_recipe(bytes: &[u8]) -> Result<Recipe> {
    let recipe: Recipe = serde_json::from_slice(bytes)?;
    if recipe.version != 1
        || recipe.run == [0; 16]
        || recipe.actions.is_empty()
        || recipe.actions.len() > 1024
        || recipe
            .actions
            .iter()
            .any(|action| matches!(action, Action::Performance { .. }))
        || !matches!(recipe.actions[0], Action::Initialize { .. })
    {
        return Err("invalid recipe envelope".into());
    }
    Ok(recipe)
}

/// Execute actual native requests, then check the saved history and save a verdict.
///
/// # Errors
/// Recipe identity/parse, I/O, transport or evidence failures return errors. The
/// native owner retains failure artifacts and cleans up its own worker process.
pub fn run(options: &Options) -> Result<Report> {
    let bytes = bounded(&options.recipe, RECIPE_BYTES)?;
    if sha256(&bytes) != options.recipe_sha256 {
        return Err("recipe identity mismatch".into());
    }
    let recipe = parse_recipe(&bytes)?;
    let mut session = native::Session::start(native::Options::new(
        options.worker.clone(),
        options.worker_sha256.clone(),
        options.artifacts.clone(),
        recipe.run,
    ))?;
    let directory = session.directory().to_path_buf();
    save(&directory.join("recipe.json"), &bytes)?;
    let binding = serde_json::json!({"schema_version":1,"recipe_sha256":options.recipe_sha256,
        "worker_sha256":options.worker_sha256});
    save(
        &directory.join("catalog-manifest.json"),
        &serde_json::to_vec_pretty(&binding)?,
    )?;
    for action in recipe.actions {
        session.invoke(0, action)?;
        session.receive()?;
    }
    session.finish()?;
    let report = verify(&directory, &options.recipe_sha256, &options.worker_sha256)?;
    save(
        &directory.join("catalog-result.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

struct Event<'a> {
    kind: u8,
    request: u64,
    payload: &'a [u8],
}
struct Journal<'a> {
    bytes: &'a [u8],
    sequence: u64,
    time: u64,
}
impl<'a> Journal<'a> {
    // Independent journal header reader. No calls into native::Session's recorder.
    fn next(&mut self, kind: u8, request: u64) -> Result<Event<'a>> {
        let header = self
            .bytes
            .get(..29)
            .ok_or("partial/missing journal header")?;
        let sequence = u64::from_be_bytes(header[1..9].try_into()?);
        let time = u64::from_be_bytes(header[9..17].try_into()?);
        let actual_request = u64::from_be_bytes(header[17..25].try_into()?);
        let length = u32::from_be_bytes(header[25..29].try_into()?) as usize;
        if header[0] != kind
            || sequence != self.sequence
            || time < self.time
            || actual_request != request
            || length > frame::MAX_FRAME_BYTES
        {
            return Err("journal order/header mismatch".into());
        }
        let payload = self
            .bytes
            .get(29..29 + length)
            .ok_or("partial journal payload")?;
        self.bytes = &self.bytes[29 + length..];
        self.sequence += 1;
        self.time = time;
        Ok(Event {
            kind,
            request,
            payload,
        })
    }
}
fn envelope<T: frame::Envelope + for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    let mut reader = bytes;
    let value = frame::read::<T>(&mut reader)?.ok_or("missing framed envelope")?;
    if !reader.is_empty() {
        return Err("multiple frames in one journal event".into());
    }
    Ok(value)
}

/// Independently recheck one retained normal-completion native history.
///
/// Expected identities come from the caller, not a candidate-controlled verdict.
/// A planned kill is not a normal sequential recipe completion.
///
/// # Errors
/// Missing/extra/partial records, wrong identities/hashes, unsupported format or
/// typed budget violations fail closed. This verifier never modifies evidence.
pub fn verify(directory: &Path, expected_recipe: &str, expected_worker: &str) -> Result<Report> {
    let recipe_bytes = bounded(&directory.join("recipe.json"), RECIPE_BYTES)?;
    if sha256(&recipe_bytes) != expected_recipe {
        return Err("retained recipe identity mismatch".into());
    }
    let recipe = parse_recipe(&recipe_bytes)?;
    let binding = json(&directory.join("catalog-manifest.json"), 4096)?;
    if binding["schema_version"] != 1
        || binding["recipe_sha256"] != expected_recipe
        || binding["worker_sha256"] != expected_worker
    {
        return Err("catalog identity binding mismatch".into());
    }
    let manifest = json(&directory.join("manifest.json"), 16384)?;
    if manifest["schema_version"] != 1
        || manifest["journal_format_version"] != 1
        || manifest["run"] != serde_json::to_value(recipe.run)?
        || manifest["worker_sha256"] != expected_worker
        || file_sha256(&directory.join("worker"))? != expected_worker
    {
        return Err("native identity mismatch".into());
    }
    let receipt = json(&directory.join("completion.json"), 16384)?;
    if directory.join("failure.json").exists()
        || directory.join("planned-kill.json").exists()
        || receipt["completion"] != "TRANSPORT_CLOSED_NO_PRODUCT_VERDICT"
        || receipt["worker_sha256"] != expected_worker
        || receipt["requests"] != recipe.actions.len()
        || receipt["observations"] != recipe.actions.len()
    {
        return Err("native completion mismatch".into());
    }
    let history = bounded(&directory.join("history.bin"), HISTORY_BYTES)?;
    let output = bounded(&directory.join("stdout.bin"), HISTORY_BYTES)?;
    let errors = bounded(&directory.join("stderr.bin"), 1024 * 1024)?;
    for (key, bytes) in [
        ("history_sha256", history.as_slice()),
        ("stdout_sha256", output.as_slice()),
        ("stderr_sha256", errors.as_slice()),
    ] {
        if receipt[key] != sha256(bytes) {
            return Err("external stream identity mismatch".into());
        }
    }
    let mut journal = Journal {
        bytes: &history,
        sequence: 0,
        time: 0,
    };
    let mut raw = output.as_slice();
    let mut pairs = Vec::new();
    let mut budget = CatalogInputBudget::default();
    let mut invalid_input = None;
    for (index, action) in recipe.actions.into_iter().enumerate() {
        let id = index as u64 + 1;
        let invoke = journal.next(1, id)?;
        let request: Request = envelope(invoke.payload)?;
        if request.version != 1
            || request.run != recipe.run
            || request.request != id
            || request.actor != 0
            || request.action != action
        {
            return Err("invocation differs from bound recipe".into());
        }
        let sent = journal.next(2, id)?;
        if !sent.payload.is_empty() {
            return Err("Sent event payload".into());
        }
        let observed = journal.next(3, id)?;
        let response: Response = envelope(observed.payload)?;
        if response.run != recipe.run || response.request != id || response.actor != 0 {
            return Err("response matching failure".into());
        }
        let actual = frame::read::<Response>(&mut raw)?.ok_or("missing raw response")?;
        if actual != response {
            return Err("raw response differs from external observation".into());
        }
        if invalid_input.is_none() {
            match budget.admit(&request, &response) {
                Ok(()) => pairs.push((request, response)),
                Err(reason) => invalid_input = Some((index, reason)),
            }
        }
    }
    let close = journal.next(5, 0)?;
    if close.kind != 5
        || close.request != 0
        || !close.payload.is_empty()
        || !journal.bytes.is_empty()
        || !raw.is_empty()
    {
        return Err("incomplete/extra close or stream".into());
    }
    let result = if let Some((pair, reason)) = invalid_input {
        CatalogResult::Invalid { pair, reason }
    } else {
        check_catalog(&pairs)
    };
    let (verdict, failed_pair, reason) = match result {
        CatalogResult::Accepted => ("ACCEPTED", None, None),
        CatalogResult::Rejected { pair, reason } => ("REJECTED", Some(pair), Some(reason)),
        CatalogResult::Invalid { pair, reason } => ("INVALID", Some(pair), Some(reason)),
    };
    Ok(Report {
        schema_version: 1,
        directory: directory.to_path_buf(),
        verdict,
        checked_pairs: journal.sequence.saturating_sub(1) as usize / 3,
        failed_pair,
        reason,
        recipe_sha256: expected_recipe.into(),
        worker_sha256: expected_worker.into(),
        history_sha256: sha256(&history),
    })
}
