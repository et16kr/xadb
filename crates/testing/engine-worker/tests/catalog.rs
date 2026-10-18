//! Actual native recipe execution checked by the independent catalog interpreter.
#[path = "../../reference/tests/support/catalog_cases.rs"]
mod cases;
use std::{
    fs,
    path::{Path, PathBuf},
};
use xadbtest::{
    catalog::{self, Options, Recipe},
    file_sha256, sha256,
};
use xadbtest_protocol::engine::{Observation, Request, Response};

struct Root {
    path: PathBuf,
    _temporary: Option<tempfile::TempDir>,
}
fn root(name: &str) -> Root {
    if let Some(parent) = std::env::var_os("XADBTEST_CATALOG_ARTIFACTS") {
        let path = PathBuf::from(parent).join(name);
        fs::create_dir_all(&path).unwrap();
        Root {
            path,
            _temporary: None,
        }
    } else {
        let temporary = tempfile::tempdir().unwrap();
        Root {
            path: temporary.path().to_path_buf(),
            _temporary: Some(temporary),
        }
    }
}
fn fixture(root: &Path, pairs: &[(Request, Response)]) -> Options {
    let worker = std::env::var_os("XADBTEST_ENGINE_WORKER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_xadbtest-engine-worker").into());
    let recipe = Recipe {
        version: 1,
        run: [1; 16],
        actions: pairs.iter().map(|p| p.0.action.clone()).collect(),
    };
    let bytes = serde_json::to_vec(&recipe).unwrap();
    let path = root.join("input.json");
    fs::write(&path, &bytes).unwrap();
    Options {
        recipe: path,
        recipe_sha256: sha256(&bytes),
        worker_sha256: file_sha256(&worker).unwrap(),
        worker,
        artifacts: root.join("runs"),
    }
}
fn generated(seed: u64, requests: usize) {
    let root = root(&format!("seed-{seed}-{requests}"));
    let expected = cases::generated(seed, requests);
    let options = fixture(&root.path, &expected);
    let report = catalog::run(&options).unwrap();
    assert_eq!(report.verdict, "ACCEPTED");
    assert_eq!(report.checked_pairs, requests);
    assert_eq!(report.recipe_sha256, options.recipe_sha256);
    assert_eq!(report.worker_sha256, options.worker_sha256);
    assert!(report.directory.join("catalog-result.json").is_file());
}

#[test]
fn saved_recipe_replays_into_distinct_owned_runs_and_verifies_without_live_source() {
    let root = root("literal-replay");
    let trace = cases::literal_catalog();
    let options = fixture(&root.path, &trace.pairs);
    let first = catalog::run(&options).unwrap();
    assert_eq!(first.verdict, "ACCEPTED");
    let replay = Options {
        recipe: first.directory.join("recipe.json"),
        recipe_sha256: options.recipe_sha256.clone(),
        worker: first.directory.join("worker"),
        worker_sha256: options.worker_sha256.clone(),
        artifacts: root.path.join("replay"),
    };
    let second = catalog::run(&replay).unwrap();
    assert_eq!(second.verdict, "ACCEPTED");
    assert_ne!(first.directory, second.directory);
    let checked = catalog::verify(
        &first.directory,
        &options.recipe_sha256,
        &options.worker_sha256,
    )
    .unwrap();
    assert_eq!(checked.verdict, "ACCEPTED");
    assert_eq!(checked.history_sha256, first.history_sha256);
}

#[test]
fn wrong_input_identity_unknown_duplicate_version_and_byte_limits_never_start_worker() {
    let root = root("bad-inputs");
    let mut options = fixture(&root.path, &cases::generated(0, 16));
    options.recipe_sha256 = "0".repeat(64);
    assert!(catalog::run(&options).is_err());
    assert!(!options.artifacts.exists());
    let original = fs::read(&options.recipe).unwrap();
    let base: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let mut wrong_version = base.clone();
    wrong_version["version"] = 2.into();
    let mut unknown = base;
    unknown["unknown"] = true.into();
    let duplicate = String::from_utf8(original).unwrap().replacen(
        "\"version\":1",
        "\"version\":1,\"version\":1",
        1,
    );
    for (bytes, message) in [
        (
            serde_json::to_vec(&wrong_version).unwrap(),
            "invalid recipe envelope",
        ),
        (duplicate.into_bytes(), "duplicate field"),
        (serde_json::to_vec(&unknown).unwrap(), "unknown field"),
        (vec![b' '; 16 * 1024 * 1024 + 1], "artifact byte limit"),
    ] {
        fs::write(&options.recipe, &bytes).unwrap();
        options.recipe_sha256 = sha256(&bytes);
        let error = catalog::run(&options).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
        assert!(!options.artifacts.exists());
    }
}
fn copy_directory(from: &Path, to: &Path) {
    fs::create_dir(to).unwrap();
    for item in fs::read_dir(from).unwrap() {
        let path = item.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_directory(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}
fn rehash_receipt(directory: &Path) {
    let path = directory.join("completion.json");
    let mut receipt: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    receipt["history_sha256"] = file_sha256(&directory.join("history.bin")).unwrap().into();
    receipt["stdout_sha256"] = file_sha256(&directory.join("stdout.bin")).unwrap().into();
    fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
}

#[test]
fn tampered_complete_history_is_rejected_even_after_stream_hashes_are_refreshed() {
    let root = root("history-mutants");
    let options = fixture(&root.path, &cases::generated(9, 16));
    let baseline = catalog::run(&options).unwrap();
    assert_eq!(baseline.verdict, "ACCEPTED");
    for mode in 0..5 {
        let directory = root.path.join(format!("mutant-{mode}"));
        copy_directory(&baseline.directory, &directory);
        let mut history = fs::read(directory.join("history.bin")).unwrap();
        match mode {
            0 => {
                // Correct envelope with an incorrect independently observable row value.
                let mut offset = 0;
                let mut raw = Vec::new();
                let mut rebuilt = Vec::new();
                let mut mutated = false;
                while offset < history.len() {
                    let header = &history[offset..offset + 29];
                    let count = u32::from_be_bytes(header[25..29].try_into().unwrap()) as usize;
                    let payload = &history[offset + 29..offset + 29 + count];
                    let mut replacement = payload.to_vec();
                    if header[0] == 3 {
                        let mut response: Response =
                            xadbtest_protocol::frame::read(&mut &payload[..])
                                .unwrap()
                                .unwrap();
                        if !mutated
                            && let Observation::Row { value: Some(row) } = &mut response.observation
                        {
                            row.values = vec![xadbtest_protocol::engine::Cell::Int64(99)];
                            mutated = true;
                        }
                        replacement = xadbtest_protocol::frame::encode(&response).unwrap();
                        raw.extend_from_slice(&replacement);
                    }
                    rebuilt.extend_from_slice(&header[..25]);
                    rebuilt.extend_from_slice(&(replacement.len() as u32).to_be_bytes());
                    rebuilt.extend(replacement);
                    offset += 29 + count;
                }
                assert!(mutated);
                history = rebuilt;
                fs::write(directory.join("stdout.bin"), raw).unwrap();
            }
            1 => history[8] = 1, // First sequence must be zero.
            2 => {
                history.pop();
            } // Incomplete normal-close event.
            3 => history.extend_from_slice(&[0; 29]),
            _ => fs::write(directory.join("stdout.bin"), b"").unwrap(),
        }
        fs::write(directory.join("history.bin"), history).unwrap();
        rehash_receipt(&directory);
        let result = catalog::verify(&directory, &options.recipe_sha256, &options.worker_sha256);
        if mode == 0 {
            assert_eq!(result.unwrap().verdict, "REJECTED");
        } else {
            assert!(result.is_err());
        }
    }
    assert_eq!(
        catalog::verify(
            &baseline.directory,
            &options.recipe_sha256,
            &options.worker_sha256
        )
        .unwrap()
        .verdict,
        "ACCEPTED"
    );
}

#[test]
fn actual_catalog_v2_64_seeds_execute_128_requests() {
    for seed in 0..64 {
        generated(seed, 128);
    }
}
#[test]
fn actual_catalog_maximum_1024_requests() {
    generated(u64::MAX, 1024);
}
#[test]
#[ignore = "explicit V3 actual native catalog recipe profile"]
fn actual_catalog_v3_256_seeds_execute_1024_requests() {
    for seed in 0..256 {
        generated(seed, 1024);
    }
}

#[test]
fn exact_sixteen_mib_recipe_with_json_whitespace_is_valid() {
    let root = root("exact-recipe-size");
    let mut options = fixture(&root.path, &cases::generated(2, 16));
    let mut bytes = fs::read(&options.recipe).unwrap();
    bytes.resize(16 * 1024 * 1024, b' ');
    options.recipe_sha256 = sha256(&bytes);
    fs::write(&options.recipe, bytes).unwrap();
    assert_eq!(catalog::run(&options).unwrap().verdict, "ACCEPTED");
}
