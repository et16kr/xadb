use super::{BuildReceipt, CampaignKind, Order, Recipe, Result, RunSample, Side, analyze};
use crate::{file_sha256, native, sha256};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use test_protocol::{
    engine::{Action, Observation, Request, Response},
    frame,
    performance::{BuildIdentity, Input, Output},
};

const ARTIFACT_BYTES: usize = 64 * 1024 * 1024;
const INDEX_BYTES: usize = 256 * 1024;

pub struct Options {
    pub recipe: PathBuf,
    pub recipe_sha256: String,
    pub baseline_worker: PathBuf,
    pub candidate_worker: PathBuf,
    pub baseline_receipt: PathBuf,
    pub candidate_receipt: PathBuf,
    pub baseline_config: PathBuf,
    pub candidate_config: PathBuf,
    pub artifacts: PathBuf,
}
pub struct ReplayOptions {
    pub directory: PathBuf,
    pub recipe_sha256: String,
    pub artifacts: PathBuf,
    pub campaign_id: [u8; 16],
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub directory: PathBuf,
    pub recipe_sha256: String,
    pub index_sha256: String,
    /// Controller that recorded the original campaign's native evidence.
    pub original_controller: BuildIdentity,
    pub original_controller_sha256: String,
    /// Executable actually performing this analysis, which may be a later build.
    pub analyzer_build: BuildIdentity,
    pub analyzer_sha256: String,
    pub comparison: super::Comparison,
}
impl Report {
    /// Harness/correctness status only; effect/guardrail/promotion remain separate axes.
    pub fn exit_code(&self) -> u8 {
        if self.comparison.validity != "VALID" {
            2
        } else if self.comparison.correctness != "PASS" {
            1
        } else {
            0
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRecord {
    ordinal: usize,
    run: [u8; 16],
    candidate: bool,
    directory: Option<String>,
    failure: Option<String>,
    files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    schema_version: u32,
    recipe_sha256: String,
    controller: BuildIdentity,
    files: BTreeMap<String, String>,
    runs: Vec<RunRecord>,
    original_recipe_sha256: Option<String>,
}
fn build() -> BuildIdentity {
    BuildIdentity {
        source_sha256: env!("XADBTEST_SOURCE_SHA256").into(),
        toolchain: env!("XADBTEST_TOOLCHAIN").into(),
        target: env!("XADBTEST_TARGET").into(),
        profile: env!("XADBTEST_PROFILE").into(),
        self_test: cfg!(feature = "self-test"),
    }
}
fn bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err("artifact must be an ordinary file".into());
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("performance artifact byte limit".into());
    }
    Ok(bytes)
}
fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}
fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    save(path, &serde_json::to_vec_pretty(value)?)
}
fn json(path: &Path, limit: usize) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&bounded(path, limit)?)?)
}
fn path(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("non-relative bundle path".into());
    }
    let target = root.join(relative);
    // Symlinks cannot redirect a verifier to evidence outside its owned bundle.
    let mut cursor = root.to_path_buf();
    for part in relative.components() {
        cursor.push(part);
        if fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Err("symlink in bundle path".into());
        }
    }
    Ok(target)
}
fn receipt(bytes: &[u8], side: &Side) -> Result<()> {
    if sha256(bytes) != side.receipt_sha256 {
        return Err("build receipt identity mismatch".into());
    }
    let receipt: BuildReceipt = serde_json::from_slice(bytes)?;
    if receipt.schema_version != 1
        || receipt.worker_sha256 != side.worker_sha256
        || receipt.build != side.build
        || receipt.config_sha256 != side.config_sha256
        || receipt.command.is_empty()
        || !receipt.features.is_empty()
    {
        return Err("build receipt differs from frozen release identity".into());
    }
    Ok(())
}
fn initialize() -> Action {
    Action::Initialize {
        database: [2; 16],
        timeline: 7,
        epoch: 3,
        resident_bytes: 64 * 1024 * 1024,
        snapshot_pins: 32,
    }
}
fn files(directory: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for name in [
        "manifest.json",
        "worker",
        "history.bin",
        "stdout.bin",
        "stderr.bin",
        "completion.json",
        "failure.json",
        "planned-kill.json",
    ] {
        let file = directory.join(name);
        if file.is_file() {
            result.insert(name.into(), file_sha256(&file)?);
        }
    }
    Ok(result)
}
fn execute(
    directory: &Path,
    artifacts: &Path,
    worker: &Path,
    side: &Side,
    run: [u8; 16],
    input: Input,
) -> RunRecord {
    let mut record = RunRecord {
        ordinal: 0,
        run,
        candidate: false,
        directory: None,
        failure: None,
        files: BTreeMap::new(),
    };
    let result = (|| -> Result<()> {
        let mut session = native::Session::start(native::Options::new(
            worker.into(),
            side.worker_sha256.clone(),
            artifacts.into(),
            run,
        ))?;
        let native = session.directory().to_path_buf();
        record.directory = Some(
            native
                .strip_prefix(directory)?
                .to_string_lossy()
                .into_owned(),
        );
        session.invoke(0, initialize())?;
        session.receive()?;
        session.invoke(0, Action::Performance { input })?;
        session.receive()?;
        session.finish()?;
        Ok(())
    })();
    if let Err(error) = result {
        record.failure = Some(error.to_string());
    }
    // A start failure can still have allocated a native evidence directory.
    if record.directory.is_none()
        && let Ok(entries) = fs::read_dir(artifacts)
    {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                record.directory = entry
                    .path()
                    .strip_prefix(directory)
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned());
                break;
            }
        }
    }
    if let Some(relative) = &record.directory {
        match files(&directory.join(relative)) {
            Ok(files) => record.files = files,
            Err(error) => record.failure = Some(format!("evidence inventory failed: {error}")),
        }
    }
    record
}

/// Run every fixed slot and retain raw native evidence, including all failures.
/// Input/identity/I/O setup errors return Err; completed campaigns retain separate axes.
pub fn run(options: &Options) -> Result<Report> {
    run_internal(options, None)
}
fn run_internal(options: &Options, original: Option<&Path>) -> Result<Report> {
    let bytes = bounded(&options.recipe, super::RECIPE_BYTES)?;
    if sha256(&bytes) != options.recipe_sha256 {
        return Err("recipe identity mismatch".into());
    }
    let recipe = Recipe::parse(&bytes)?;
    if (recipe.kind == CampaignKind::ReplayAudit) != original.is_some() {
        return Err("ReplayAudit requires preserved original bundle".into());
    }
    let a_receipt = bounded(&options.baseline_receipt, super::RECIPE_BYTES)?;
    let b_receipt = bounded(&options.candidate_receipt, super::RECIPE_BYTES)?;
    receipt(&a_receipt, &recipe.baseline)?;
    receipt(&b_receipt, &recipe.candidate)?;
    let a_config = bounded(&options.baseline_config, super::RECIPE_BYTES)?;
    let b_config = bounded(&options.candidate_config, super::RECIPE_BYTES)?;
    if sha256(&a_config) != recipe.baseline.config_sha256
        || sha256(&b_config) != recipe.candidate.config_sha256
    {
        return Err("input configuration identity mismatch".into());
    }
    for (worker, side) in [
        (&options.baseline_worker, &recipe.baseline),
        (&options.candidate_worker, &recipe.candidate),
    ] {
        if file_sha256(worker)? != side.worker_sha256 {
            return Err("input worker identity mismatch".into());
        }
    }
    fs::create_dir_all(&options.artifacts)?;
    let directory = tempfile::Builder::new()
        .prefix("performance-")
        .tempdir_in(options.artifacts.canonicalize()?)?
        .keep();
    save(&directory.join("recipe.json"), &bytes)?;
    save(&directory.join("baseline-receipt.json"), &a_receipt)?;
    save(&directory.join("candidate-receipt.json"), &b_receipt)?;
    save(&directory.join("baseline-config.json"), &a_config)?;
    save(&directory.join("candidate-config.json"), &b_config)?;
    fs::copy(std::env::current_exe()?, directory.join("controller"))?;
    fs::copy(&options.baseline_worker, directory.join("baseline-worker"))?;
    fs::copy(
        &options.candidate_worker,
        directory.join("candidate-worker"),
    )?;
    let environment = serde_json::json!({"schema_version":1,"architecture":std::env::consts::ARCH,
        "os":std::env::consts::OS,"kernel":fs::read_to_string("/proc/sys/kernel/osrelease").ok(),
        "cpuinfo":fs::read_to_string("/proc/cpuinfo").ok(),"mountinfo":fs::read_to_string("/proc/self/mountinfo").ok(),
        "numa_online":fs::read_to_string("/sys/devices/system/node/online").ok(),
        "filesystem_probe_path":directory,"host_tuning":"UNCHANGED","arrival":"single-thread closed-loop",
        "cpu_scope":"calling thread CLOCK_THREAD_CPUTIME_ID delta","rss_scope":"process VmHWM at sampling time",
        "controller_build":build(),"baseline_build":recipe.baseline.build,"candidate_build":recipe.candidate.build,
        "metric_catalog":super::profile()});
    save_json(&directory.join("environment.json"), &environment)?;
    save(&directory.join("resume.md"),format!("# K0 performance bundle\n\n{}\n\nFrom this copied campaign directory, run the retained `./controller verify-performance --directory . --recipe-sha256 {}` to reanalyze raw evidence. Run the retained `./controller replay-performance` with `--directory .`, this recipe hash, a fresh `--campaign-id` and a new `--artifacts` directory to start new processes. Both configuration files and workers are retained here. The report separates the recording controller identity from the executable performing analysis. No optimization or durable DB claim. Investigate measured copying only after valid evidence; cause remains unresolved.\n",recipe.hypothesis,options.recipe_sha256).as_bytes())?;
    if let Some(original) = original {
        copy_tree(original, &directory.join("original"), 0)?;
    }
    let mut index = Index {
        schema_version: 1,
        recipe_sha256: options.recipe_sha256.clone(),
        controller: build(),
        files: BTreeMap::new(),
        runs: Vec::new(),
        original_recipe_sha256: recipe.provenance.original_recipe_sha256.clone(),
    };
    for name in [
        "recipe.json",
        "baseline-receipt.json",
        "candidate-receipt.json",
        "baseline-config.json",
        "candidate-config.json",
        "controller",
        "baseline-worker",
        "candidate-worker",
        "environment.json",
        "resume.md",
    ] {
        index
            .files
            .insert(name.into(), file_sha256(&directory.join(name))?);
    }
    for (number, pair) in recipe.pairs.iter().enumerate() {
        let mut records: [Option<RunRecord>; 2] = [None, None];
        for (position, candidate) in (if pair.order == Order::AB {
            [false, true]
        } else {
            [true, false]
        })
        .into_iter()
        .enumerate()
        {
            let (side, id, name) = if candidate {
                (&recipe.candidate, pair.candidate_run, "candidate")
            } else {
                (&recipe.baseline, pair.baseline_run, "baseline")
            };
            let parent = directory.join(format!("pair-{number:02}-{name}"));
            fs::create_dir(&parent)?;
            let worker = directory.join(format!("{name}-worker"));
            let mut record = execute(
                &directory,
                &parent,
                &worker,
                side,
                id,
                recipe.input(candidate),
            );
            record.candidate = candidate;
            record.ordinal = number * 2 + position;
            records[usize::from(candidate)] = Some(record);
        }
        for record in records {
            index.runs.push(record.ok_or("unexecuted campaign slot")?);
        }
    }
    save_json(&directory.join("index.json"), &index)?;
    let report = verify(&directory, &options.recipe_sha256)?;
    save_json(&directory.join("comparison.json"), &report)?;
    Ok(report)
}

struct Journal<'a> {
    bytes: &'a [u8],
    sequence: u64,
    time: u64,
}
impl<'a> Journal<'a> {
    // Independent fixed header reader, intentionally separate from native's recorder.
    // Revisit this bounded format duplication when journal version or owner changes.
    fn next(&mut self, kind: u8, request: u64) -> Result<(u64, &'a [u8])> {
        let header = self.bytes.get(..29).ok_or("missing journal header")?;
        let sequence = u64::from_be_bytes(header[1..9].try_into()?);
        let time = u64::from_be_bytes(header[9..17].try_into()?);
        let actual = u64::from_be_bytes(header[17..25].try_into()?);
        let length = u32::from_be_bytes(header[25..29].try_into()?) as usize;
        if header[0] != kind
            || sequence != self.sequence
            || time < self.time
            || actual != request
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
        Ok((time, payload))
    }
}
fn envelope<T: frame::Envelope + for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    let mut reader = bytes;
    let value = frame::read::<T>(&mut reader)?.ok_or("missing envelope")?;
    if !reader.is_empty() {
        return Err("extra envelope bytes".into());
    }
    Ok(value)
}
fn inspect(
    directory: &Path,
    run: [u8; 16],
    input: &Input,
    side: &Side,
    controller: &BuildIdentity,
) -> Result<(Output, u64)> {
    let manifest = json(&directory.join("manifest.json"), 16384)?;
    if manifest["schema_version"] != 1
        || manifest["journal_format_version"] != 1
        || manifest["run"] != serde_json::to_value(run)?
        || manifest["worker_sha256"] != side.worker_sha256
        || manifest["source_sha256"] != controller.source_sha256
        || manifest["toolchain"] != controller.toolchain
        || manifest["target"] != controller.target
        || manifest["profile"] != controller.profile
        || manifest["self_test"] != controller.self_test
        || file_sha256(&directory.join("worker"))? != side.worker_sha256
    {
        return Err("native manifest identity mismatch".into());
    }
    let receipt = json(&directory.join("completion.json"), 16384)?;
    if directory.join("failure.json").exists()
        || directory.join("planned-kill.json").exists()
        || receipt["completion"] != "TRANSPORT_CLOSED_NO_PRODUCT_VERDICT"
        || receipt["worker_sha256"] != side.worker_sha256
        || receipt["requests"] != 2
        || receipt["observations"] != 2
    {
        return Err("native run failed/incomplete".into());
    }
    let history = bounded(&directory.join("history.bin"), ARTIFACT_BYTES)?;
    let output = bounded(&directory.join("stdout.bin"), ARTIFACT_BYTES)?;
    let stderr = bounded(&directory.join("stderr.bin"), 1024 * 1024)?;
    for (name, bytes) in [
        ("history_sha256", history.as_slice()),
        ("stdout_sha256", output.as_slice()),
        ("stderr_sha256", stderr.as_slice()),
    ] {
        if receipt[name] != sha256(bytes) {
            return Err("native raw stream identity mismatch".into());
        }
    }
    let mut journal = Journal {
        bytes: &history,
        sequence: 0,
        time: 0,
    };
    let mut raw = output.as_slice();
    let mut measured = None;
    for (index, action) in [
        initialize(),
        Action::Performance {
            input: input.clone(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let id = index as u64 + 1;
        let (invoke, payload) = journal.next(1, id)?;
        let request: Request = envelope(payload)?;
        if request.run != run
            || request.request != id
            || request.actor != 0
            || request.action != action
        {
            return Err("external request differs from frozen fixture".into());
        }
        if !journal.next(2, id)?.1.is_empty() {
            return Err("Sent payload".into());
        }
        let (observe, payload) = journal.next(3, id)?;
        let response: Response = envelope(payload)?;
        if response.run != run
            || response.request != id
            || response.actor != 0
            || frame::read::<Response>(&mut raw)? != Some(response.clone())
        {
            return Err("external/raw response mismatch".into());
        }
        if id == 1 {
            if response.observation != (Observation::Ready {}) {
                return Err("initialization failed".into());
            }
        } else if let Observation::Performance { output } = response.observation {
            measured = Some((
                *output,
                observe
                    .checked_sub(invoke)
                    .ok_or("external clock regressed")?,
            ));
        } else {
            return Err("missing performance observation".into());
        }
    }
    if !journal.next(5, 0)?.1.is_empty() || !journal.bytes.is_empty() || !raw.is_empty() {
        return Err("extra/missing journal close or raw output".into());
    }
    measured.ok_or_else(|| "missing measured response".into())
}

/// Independently re-read bound raw streams and recompute all result axes.
/// Saved comparison.json is never used as a verdict input.
pub fn verify(directory: &Path, expected_recipe: &str) -> Result<Report> {
    verify_depth(directory, expected_recipe, 0)
}
fn verify_depth(directory: &Path, expected_recipe: &str, depth: usize) -> Result<Report> {
    if depth > 1 {
        return Err("nested replay lineage exceeds this profile".into());
    }
    let bytes = bounded(&directory.join("recipe.json"), super::RECIPE_BYTES)?;
    if sha256(&bytes) != expected_recipe {
        return Err("retained recipe identity mismatch".into());
    }
    let recipe = Recipe::parse(&bytes)?;
    let index_bytes = bounded(&directory.join("index.json"), INDEX_BYTES)?;
    let index: Index = serde_json::from_slice(&index_bytes)?;
    if index.schema_version != 1
        || index.recipe_sha256 != expected_recipe
        || index.runs.len() != 20
        || index.original_recipe_sha256 != recipe.provenance.original_recipe_sha256
        || index.files.len() != 10
    {
        return Err("bundle index identity/inventory mismatch".into());
    }
    for required in [
        "recipe.json",
        "baseline-receipt.json",
        "candidate-receipt.json",
        "baseline-config.json",
        "candidate-config.json",
        "controller",
        "baseline-worker",
        "candidate-worker",
        "environment.json",
        "resume.md",
    ] {
        if !index.files.contains_key(required) {
            return Err("missing required bundle file".into());
        }
    }
    for (name, expected) in &index.files {
        if file_sha256(&path(directory, name)?)? != *expected {
            return Err("bundle file identity mismatch".into());
        }
    }
    for (name, side) in [
        ("baseline", &recipe.baseline),
        ("candidate", &recipe.candidate),
    ] {
        let receipt_bytes = bounded(
            &directory.join(format!("{name}-receipt.json")),
            super::RECIPE_BYTES,
        )?;
        receipt(&receipt_bytes, side)?;
        let config_bytes = bounded(
            &directory.join(format!("{name}-config.json")),
            super::RECIPE_BYTES,
        )?;
        if sha256(&config_bytes) != side.config_sha256 {
            return Err("retained configuration differs from frozen identity".into());
        }
        if file_sha256(&directory.join(format!("{name}-worker")))? != side.worker_sha256 {
            return Err("retained baseline/candidate worker mismatch".into());
        }
    }
    if recipe.kind == CampaignKind::ReplayAudit {
        let expected = recipe
            .provenance
            .original_recipe_sha256
            .as_deref()
            .ok_or("missing original recipe")?;
        let original = directory.join("original");
        let _original_result = verify_depth(&original, expected, depth + 1)?;
        let original_recipe = Recipe::parse(&bounded(
            &original.join("recipe.json"),
            super::RECIPE_BYTES,
        )?)?;
        let mut expected_replay = original_recipe.clone();
        expected_replay.kind = CampaignKind::ReplayAudit;
        expected_replay.campaign_id = recipe.campaign_id;
        expected_replay.pairs = recipe.pairs.clone();
        expected_replay.provenance.original_recipe_sha256 = Some(expected.into());
        expected_replay
            .provenance
            .previous_confirmation_runs
            .extend(
                original_recipe
                    .pairs
                    .iter()
                    .flat_map(|p| [p.baseline_run, p.candidate_run]),
            );
        if expected_replay != recipe {
            return Err("replay changed original fixture/policy/build/lineage".into());
        }
    }
    let mut samples = Vec::new();
    for (slot, record) in index.runs.iter().enumerate() {
        let candidate = slot % 2 == 1;
        let pair = &recipe.pairs[slot / 2];
        let expected = if candidate {
            pair.candidate_run
        } else {
            pair.baseline_run
        };
        let ordinal = slot / 2 * 2 + usize::from(candidate != (pair.order == Order::BA));
        if record.run != expected || record.candidate != candidate || record.ordinal != ordinal {
            return Err("bundle run slot identity mismatch".into());
        }
        let side = if candidate {
            &recipe.candidate
        } else {
            &recipe.baseline
        };
        let inspected = (|| -> Result<(Output, u64)> {
            let native = path(
                directory,
                record
                    .directory
                    .as_deref()
                    .ok_or("run has no evidence directory")?,
            )?;
            for (name, hash) in &record.files {
                if file_sha256(&path(&native, name)?)? != *hash {
                    return Err("retained run artifact changed".into());
                }
            }
            if record.files != files(&native)? {
                return Err("run artifact inventory mismatch".into());
            }
            if let Some(error) = &record.failure {
                return Err(error.clone().into());
            }
            inspect(
                &native,
                record.run,
                &recipe.input(candidate),
                side,
                &index.controller,
            )
        })();
        let sample = match inspected {
            Ok((output, elapsed)) => RunSample {
                run: record.run,
                output: Some(output),
                external_ns: Some(elapsed),
                error: None,
            },
            Err(error) => RunSample {
                run: record.run,
                output: None,
                external_ns: None,
                error: Some(error.to_string()),
            },
        };
        samples.push(sample);
    }
    let comparison = analyze(&recipe, &samples);
    Ok(Report {
        schema_version: 1,
        directory: directory.into(),
        recipe_sha256: expected_recipe.into(),
        index_sha256: sha256(&index_bytes),
        original_controller: index.controller,
        original_controller_sha256: index
            .files
            .get("controller")
            .ok_or("missing controller identity")?
            .clone(),
        analyzer_build: build(),
        analyzer_sha256: file_sha256(&std::env::current_exe()?)?,
        comparison,
    })
}
fn copy_tree(source: &Path, destination: &Path, depth: usize) -> Result<()> {
    if depth > 8 {
        return Err("original bundle depth limit".into());
    }
    fs::create_dir(destination)?;
    let mut count = 0;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        count += 1;
        if count > 128 {
            return Err("original bundle directory entry limit".into());
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err("symlink in original bundle".into());
        }
        if metadata.is_dir() {
            copy_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                depth + 1,
            )?;
        } else if metadata.is_file() && metadata.len() <= ARTIFACT_BYTES as u64 {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        } else {
            return Err("unsupported original bundle file".into());
        }
    }
    Ok(())
}

/// Replay a retained campaign from its own binaries/receipts with new process/run IDs.
/// The complete original remains in the new bundle; promotion is always INCOMPLETE.
pub fn replay(options: &ReplayOptions) -> Result<Report> {
    let _original = verify(&options.directory, &options.recipe_sha256)?;
    let mut recipe = Recipe::parse(&bounded(
        &options.directory.join("recipe.json"),
        super::RECIPE_BYTES,
    )?)?;
    if recipe.kind == CampaignKind::ReplayAudit {
        return Err("replay the original campaign, not a replay".into());
    }
    let template = Recipe::instrumentation(
        options.campaign_id,
        recipe.order_seed,
        recipe.baseline.clone(),
        test_protocol::performance::DiagnosticsMode::Always,
    )?;
    recipe.kind = CampaignKind::ReplayAudit;
    recipe.campaign_id = options.campaign_id;
    recipe.provenance.previous_confirmation_runs.extend(
        recipe
            .pairs
            .iter()
            .flat_map(|p| [p.baseline_run, p.candidate_run]),
    );
    recipe.provenance.original_recipe_sha256 = Some(options.recipe_sha256.clone());
    recipe.pairs = template.pairs;
    recipe.validate()?;
    let temporary = tempfile::tempdir()?;
    let file = temporary.path().join("replay-recipe.json");
    let bytes = serde_json::to_vec_pretty(&recipe)?;
    save(&file, &bytes)?;
    run_internal(
        &Options {
            recipe: file,
            recipe_sha256: sha256(&bytes),
            baseline_worker: options.directory.join("baseline-worker"),
            candidate_worker: options.directory.join("candidate-worker"),
            baseline_receipt: options.directory.join("baseline-receipt.json"),
            candidate_receipt: options.directory.join("candidate-receipt.json"),
            baseline_config: options.directory.join("baseline-config.json"),
            candidate_config: options.directory.join("candidate-config.json"),
            artifacts: options.artifacts.clone(),
        },
        Some(&options.directory),
    )
}
