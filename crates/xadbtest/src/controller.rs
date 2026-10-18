//! Controller-owned execution, observations, evidence, and cleanup.
use crate::verdict::{CaseValidity, CleanupState, ExecutionState, ProductVerdict, Verdict};
use crate::{oracle, process, spec};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use xadbtest_protocol::{MAX_INPUT_BYTES, PROTOCOL_VERSION, Request};

/// One run's explicit input identities and destination. Paths are caller-owned;
/// a new private directory is used for execution, never as a security sandbox.
#[derive(Debug)]
pub struct RunOptions {
    pub case: PathBuf,
    pub expected_case_sha256: String,
    pub worker: PathBuf,
    pub expected_worker_sha256: String,
    pub artifacts: PathBuf,
    #[cfg(feature = "self-test")]
    pub worker_args: Vec<String>,
    #[cfg(feature = "self-test")]
    pub inject_cleanup_failure: bool,
}

/// Completed local evidence bundle and its verdict; not release qualification.
#[derive(Debug)]
pub struct RunReport {
    pub directory: PathBuf,
    pub verdict: Verdict,
}

#[derive(Serialize)]
struct Manifest {
    manifest_schema_version: u32,
    run_id: String,
    scope: &'static str,
    source_sha256: &'static str,
    controller_sha256: String,
    toolchain: &'static str,
    target: &'static str,
    profile: &'static str,
    self_test_feature: bool,
    os_release: String,
    kernel: String,
    case_sha256: Option<String>,
    expected_case_sha256: String,
    worker_sha256: Option<String>,
    expected_worker_sha256: String,
    oracle_sha256: Option<String>,
    oracle_implementation_sha256: String,
    work_directory: PathBuf,
    limits: Option<spec::Limits>,
    worker_args: Vec<String>,
    cleanup_failure_injected: bool,
    release_qualification: &'static str,
}

struct Journal {
    file: File,
    sequence: usize,
}

impl Journal {
    fn record(&mut self, event: &str, detail: serde_json::Value) -> io::Result<()> {
        serde_json::to_writer(
            &mut self.file,
            &serde_json::json!({
                "receipt_schema_version": 1, "sequence": self.sequence,
                "event": event, "detail": detail
            }),
        )?;
        writeln!(self.file)?;
        self.file.sync_all()?;
        self.sequence += 1;
        Ok(())
    }
}

/// Hash bytes for identity, not authenticity or approval of a baseline.
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Hash an opened file through a bounded read buffer.
///
/// # Errors
/// Returns open/read errors; callers must separately protect the source from mutation.
pub fn file_sha256(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn save(directory: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(name))?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn save_json(directory: &Path, name: &str, value: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    save(directory, name, &bytes)
}

/// A new bundle is allocated for every attempt. Failed attempts are never overwritten.
/// A return error means the harness could not complete its evidence, never PASS.
/// This call blocks through execution and cleanup. It validates case/worker bytes
/// but does not inspect the current checkout. `scripts/check_dev0.py` and
/// `scripts/check_dev1.py` additionally bind pre/post source to the compiled identity.
/// See `tests/controller.rs` for independent verdict and failure-path cases.
///
/// # Errors
/// Returns an I/O error if evidence cannot be completed. Partial bundles remain for
/// diagnosis; Drop only attempts cleanup and cannot promote an incomplete run to PASS.
pub fn run(options: &RunOptions) -> io::Result<RunReport> {
    fs::create_dir_all(&options.artifacts)?;
    let root = options.artifacts.canonicalize()?;
    let directory = tempfile::Builder::new()
        .prefix("run-")
        .tempdir_in(&root)?
        .keep();
    let run_id = directory
        .file_name()
        .ok_or_else(|| io::Error::other("missing run ID"))?
        .to_string_lossy()
        .into_owned();
    let work = tempfile::Builder::new()
        .prefix("xadbtest-work-")
        .tempdir()?;
    let mut journal = Journal {
        file: OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("receipts.jsonl"))?,
        sequence: 0,
    };
    File::open(&root)?.sync_all()?;
    journal.record(
        "run.prepared",
        serde_json::json!({"run_id":run_id, "work_directory":work.path()}),
    )?;
    let mut verdict = Verdict::prepared(run_id.clone());
    #[cfg(feature = "self-test")]
    let (worker_args, cleanup_failure_injected) =
        (options.worker_args.clone(), options.inject_cleanup_failure);
    #[cfg(not(feature = "self-test"))]
    let (worker_args, cleanup_failure_injected) = (Vec::new(), false);
    let mut manifest = Manifest {
        manifest_schema_version: 1,
        run_id: run_id.clone(),
        scope: "LOCAL_TYPED_WORKER_NO_RELEASE_QUALIFICATION",
        source_sha256: env!("XADBTEST_SOURCE_SHA256"),
        controller_sha256: file_sha256(&std::env::current_exe()?)?,
        toolchain: env!("XADBTEST_TOOLCHAIN"),
        target: env!("XADBTEST_TARGET"),
        profile: env!("XADBTEST_PROFILE"),
        self_test_feature: cfg!(feature = "self-test"),
        os_release: fs::read_to_string("/etc/os-release").unwrap_or_else(|_| "UNKNOWN".into()),
        kernel: fs::read_to_string("/proc/sys/kernel/osrelease")
            .unwrap_or_else(|_| "UNKNOWN".into()),
        case_sha256: None,
        expected_case_sha256: options.expected_case_sha256.clone(),
        worker_sha256: None,
        expected_worker_sha256: options.expected_worker_sha256.clone(),
        oracle_sha256: None,
        oracle_implementation_sha256: sha256(include_bytes!("oracle.rs")),
        work_directory: work.path().to_path_buf(),
        limits: None,
        worker_args,
        cleanup_failure_injected,
        release_qualification: "NOT_APPLICABLE_UNPUBLISHED_DEV0",
    };
    let prepared = prepare(options, &directory, work.path(), &mut manifest);
    save_json(&directory, "manifest.json", &manifest)?;
    File::open(&directory)?.sync_all()?;
    match prepared {
        Err(error) => {
            match error {
                PrepareError::Invalid(reason) => {
                    verdict.case_validity = CaseValidity::Invalid;
                    verdict.reasons.push(reason);
                }
                PrepareError::Infrastructure(reason) => {
                    verdict.execution_state = ExecutionState::Error;
                    verdict.reasons.push(reason);
                }
            }
            journal.record(
                "preflight.rejected",
                serde_json::json!({"reasons":verdict.reasons}),
            )?;
        }
        Ok(case) => {
            verdict.case_validity = CaseValidity::Valid;
            journal.record(
                "worker.spawn.intent",
                serde_json::json!({"binary_sha256":manifest.worker_sha256}),
            )?;
            match process::OwnedChild::spawn(
                &work.path().join("worker"),
                work.path(),
                &manifest.worker_args,
            ) {
                Err(error) => {
                    verdict.execution_state = ExecutionState::Error;
                    verdict.reasons.push(format!("XADBTEST-SPAWN: {error}"));
                }
                Ok((mut child, stdout, stderr)) => {
                    verdict.execution_state = ExecutionState::Running;
                    journal.record(
                        "worker.spawned",
                        serde_json::json!({"pid":child.id(), "process_group":child.id()}),
                    )?;
                    let capture = child.capture(
                        stdout,
                        stderr,
                        case.limits.timeout_ms,
                        case.limits.output_bytes_per_stream,
                    );
                    // Journal intent while the handle still owns the unreaped process.
                    journal.record(
                        "worker.cleanup.intent",
                        serde_json::json!({"pid":child.id()}),
                    )?;
                    let status = child.finish();
                    save(&directory, "observations.jsonl", &capture.stdout)?;
                    save(&directory, "stderr.txt", &capture.stderr)?;
                    if capture.timed_out {
                        verdict.execution_state = ExecutionState::Timeout;
                        verdict
                            .reasons
                            .push("XADBTEST-TIMEOUT: worker exceeded deadline".into());
                    } else if let Some(error) = capture.error {
                        verdict.execution_state = ExecutionState::Error;
                        verdict.reasons.push(error);
                    } else {
                        verdict.execution_state = ExecutionState::Completed;
                    }
                    // Preserve any valid observed failure even if later output or cleanup fails.
                    oracle::evaluate(
                        &case,
                        &run_id,
                        &manifest.expected_case_sha256,
                        &capture.stdout,
                        &mut verdict,
                    );
                    if verdict.execution_state != ExecutionState::Completed {
                        if verdict.product_verdict != ProductVerdict::Fail {
                            verdict.product_verdict = ProductVerdict::Unknown;
                        }
                        // A timeout/output failure is an execution failure, not a bad input case.
                        if verdict.assertions_checked == 0 {
                            verdict.case_validity = CaseValidity::Valid;
                        }
                    }
                    match status {
                        Ok(status) => {
                            verdict.cleanup_state = CleanupState::Pass;
                            journal.record("worker.reaped", serde_json::json!({"pid":child.id(), "exit_code":status.code(), "success":status.success()}))?;
                            if !status.success()
                                && verdict.execution_state == ExecutionState::Completed
                            {
                                verdict.execution_state = ExecutionState::Error;
                                verdict.reasons.push(
                                    "XADBTEST-WORKER-EXIT: worker did not exit successfully".into(),
                                );
                            }
                        }
                        Err(error) => {
                            verdict.cleanup_state = CleanupState::Fail;
                            verdict.reasons.push(error.to_string());
                        }
                    }
                }
            }
        }
    }
    journal.record(
        "directory.cleanup.intent",
        serde_json::json!({"path":work.path()}),
    )?;
    // Test-only fault affects the same fallible cleanup boundary; real resources are still reclaimed.
    let cleanup = work.close().and_then(|()| {
        if cleanup_failure_injected {
            Err(io::Error::other("injected cleanup failure"))
        } else {
            Ok(())
        }
    });
    match cleanup {
        Ok(()) if verdict.cleanup_state != CleanupState::Fail => {
            verdict.cleanup_state = CleanupState::Pass
        }
        Ok(()) => (),
        Err(error) => {
            verdict.cleanup_state = CleanupState::Fail;
            verdict
                .reasons
                .push(format!("XADBTEST-CLEANUP-DIRECTORY: {error}"));
        }
    }
    journal.record(
        "directory.cleanup.completed",
        serde_json::json!({"state":verdict.cleanup_state}),
    )?;
    verdict.aggregate();
    journal.record(
        "run.completed",
        serde_json::json!({"case_verdict":verdict.case_verdict, "exit_code":verdict.exit_code()}),
    )?;
    save_json(&directory, "verdict.json", &verdict)?;
    // Artifact fingerprints bind the bytes retained by the controller; they are not signatures.
    let mut artifacts = std::collections::BTreeMap::new();
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.is_file() {
            artifacts.insert(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                file_sha256(&path)?,
            );
        }
    }
    save_json(&directory, "artifacts.json", &artifacts)?;
    File::open(&directory)?.sync_all()?;
    Ok(RunReport { directory, verdict })
}

enum PrepareError {
    Invalid(String),
    Infrastructure(String),
}
impl From<String> for PrepareError {
    fn from(reason: String) -> Self {
        Self::Invalid(reason)
    }
}
impl From<&str> for PrepareError {
    fn from(reason: &str) -> Self {
        Self::Invalid(reason.into())
    }
}

fn prepare(
    options: &RunOptions,
    directory: &Path,
    work: &Path,
    manifest: &mut Manifest,
) -> Result<spec::Scenario, PrepareError> {
    if !valid_hash(&options.expected_case_sha256) || !valid_hash(&options.expected_worker_sha256) {
        return Err("XADBTEST-IDENTITY: explicit lowercase SHA-256 fingerprints required".into());
    }
    let mut bytes = Vec::new();
    File::open(&options.case)
        .and_then(|file| {
            file.take((MAX_INPUT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
        })
        .map_err(|e| PrepareError::Infrastructure(format!("XADBTEST-CASE-READ: {e}")))?;
    save(directory, "case.json", &bytes)
        .map_err(|e| PrepareError::Infrastructure(format!("XADBTEST-EVIDENCE: {e}")))?;
    let digest = sha256(&bytes);
    manifest.case_sha256 = Some(digest.clone());
    if digest != options.expected_case_sha256 {
        return Err("XADBTEST-CASE-IDENTITY: case digest mismatch".into());
    }
    let case = spec::parse(&bytes)?;
    manifest.oracle_sha256 = Some(sha256(
        &serde_json::to_vec(&case.oracle)
            .map_err(|e| PrepareError::Infrastructure(e.to_string()))?,
    ));
    manifest.limits = Some(case.limits.clone());
    // Execute a private copy of precisely the bytes hashed, not a mutable input path.
    fs::copy(&options.worker, work.join("worker"))
        .map_err(|e| PrepareError::Infrastructure(format!("XADBTEST-WORKER-COPY: {e}")))?;
    let worker_hash = file_sha256(&work.join("worker"))
        .map_err(|e| PrepareError::Infrastructure(format!("XADBTEST-WORKER-HASH: {e}")))?;
    manifest.worker_sha256 = Some(worker_hash.clone());
    if worker_hash != options.expected_worker_sha256 {
        return Err("XADBTEST-WORKER-IDENTITY: worker digest mismatch".into());
    }
    let request = Request {
        protocol_version: PROTOCOL_VERSION,
        run_id: manifest.run_id.clone(),
        case_sha256: digest,
        steps: case.steps.clone(),
    };
    let request_bytes =
        serde_json::to_vec(&request).map_err(|e| PrepareError::Infrastructure(e.to_string()))?;
    if request_bytes.len() > MAX_INPUT_BYTES {
        return Err("XADBTEST-REQUEST-LIMIT: request exceeds budget".into());
    }
    save(work, "request.json", &request_bytes)
        .map_err(|e| PrepareError::Infrastructure(format!("XADBTEST-REQUEST: {e}")))?;
    save(directory, "request.json", &request_bytes)
        .map_err(|e| PrepareError::Infrastructure(format!("XADBTEST-EVIDENCE: {e}")))?;
    Ok(case)
}
