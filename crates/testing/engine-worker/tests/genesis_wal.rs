//! Native files, API faults and process kills are separate from power-loss claims.
use engine::testing::genesis_wal::GenesisWal;
use foundation::{
    error::Effect,
    namespace::{DirectoryPort, NamespaceErrorKind},
};
use platform::namespace::{LinuxDirectory, LinuxWriterLease};
use serde_json::json;
use std::{
    fs,
    io::{BufRead, BufReader, Read},
    os::unix::{fs::MetadataExt, process::ExitStatusExt},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[path = "support/genesis_wal.rs"]
mod support;
use support::*;

// These independent native cases share a process FD table. A concurrent spawn
// can transiently inherit another case's lease until exec, even with CLOEXEC.
// Serialize fixture lifetimes, keeping the product's Busy result unchanged.
// Actual append-gate/thread schedules remain in the engine runtime tests.
static NATIVE_PROCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn native_genesis_installs_and_appends_exact_bytes() {
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("install");
    let mut rows = Vec::new();
    assert!(GenesisWal::<LinuxWriterLease>::metadata_bytes() <= 16384);
    assert!(GenesisWal::<Writer>::metadata_bytes() <= 16384);
    for id in 1..=3 {
        let path = root.join(format!("profile-{id}"));
        fs::create_dir(&path).unwrap();
        let (writer, trace) = Writer::acquire(&path);
        let mut slot = memory();
        let session = GenesisWal::create(writer, id, &mut slot, &mut [0; 16384]).unwrap();
        let first = session.prefix().unwrap();
        assert_eq!(trace.lock().unwrap().entropy, Some(first.database));
        let (profile, mut expected, selected) = expected_controls(first.database, id);
        assert_ne!(first.database, [0; 16]);
        assert_eq!(fs::read(path.join("d3-profile")).unwrap(), profile);
        assert_eq!(fs::read(path.join(WAL)).unwrap(), expected);
        assert_eq!(fs::read(path.join("WRITER")).unwrap(), selected);
        let metadata = fs::metadata(path.join(WAL)).unwrap();
        assert_eq!(first.file, (metadata.dev(), metadata.ino()));
        assert_eq!(
            (
                first.complete_lsn,
                first.complete_offset,
                first.synced_lsn,
                first.synced_offset
            ),
            (0, 4096, 0, 4096)
        );
        let port = LinuxDirectory::open_existing(&path).unwrap();
        assert!(matches!(port.acquire_writer(), Err(e) if e.kind == NamespaceErrorKind::Busy));
        {
            let t = trace.lock().unwrap();
            assert_install_events(&t.events, 12);
            assert_eq!((t.handles, t.peak), (1, 2));
        }
        let mut previous = 0;
        for (lsn, payload) in [(1, vec![0x31; 3]), (7, vec![0x37; 32]), (20, vec![])] {
            let frame = frame(first.database, previous, lsn, &payload);
            let staged = session.stage(&frame).unwrap();
            assert_eq!(
                (
                    staged.complete_lsn,
                    staged.complete_offset,
                    staged.synced_lsn,
                    staged.synced_offset
                ),
                (
                    lsn,
                    (expected.len() + frame.len()) as u64,
                    previous,
                    expected.len() as u64
                )
            );
            assert_eq!(fs::read(path.join(WAL)).unwrap(), expected);
            let synced = session.sync().unwrap();
            expected.extend(frame);
            assert_eq!(
                (synced.synced_lsn, synced.synced_offset),
                (lsn, expected.len() as u64)
            );
            assert_eq!(fs::read(path.join(WAL)).unwrap(), expected);
            previous = lsn;
        }
        assert!(!session.is_fenced());
        drop(session);
        let before = inventory(&path);
        let (writer, retry_trace) = Writer::acquire(&path);
        let error = GenesisWal::create(writer, id, &mut slot, &mut [0; 16384])
            .err()
            .unwrap();
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(error.phase, "preflight");
        assert!(retry_trace.lock().unwrap().events.is_empty());
        assert_eq!(inventory(&path), before);
        assert_eq!(trace.lock().unwrap().handles, 0);
        drop(port.acquire_writer().unwrap());
        rows.push(json!({"profile":id,"database":hex(&first.database),"files":file_manifest(&path),"events":trace.lock().unwrap().events,"retry_effect":"Unchanged","status":"PASS"}));
    }
    write_json(
        &root.join("result.json"),
        &json!({"status":"PASS","scope":"native genesis installation and physical append only","cases":rows}),
    );
}

#[test]
fn native_genesis_faults_preserve_partial_files() {
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("faults");
    let mut rows = Vec::new();
    let phases = [
        "profile_create",
        "profile_write",
        "profile_sync",
        "epoch_create",
        "epoch_write",
        "epoch_sync",
        "initial_directory_sync",
        "selector_create",
        "selector_write",
        "selector_sync",
        "selector_publish",
        "selected_directory_sync",
    ];
    for (at, phase) in phases.into_iter().enumerate() {
        for after in [false, true] {
            let path = root.join(format!(
                "install-{at}-{}",
                if after { "after" } else { "before" }
            ));
            fs::create_dir(&path).unwrap();
            let (writer, trace) = Writer::acquire(&path);
            trace.lock().unwrap().fault = Some(if after {
                Fault::After(at)
            } else {
                Fault::Before(at)
            });
            let mut slot = memory();
            let error = GenesisWal::create(writer, 1, &mut slot, &mut [0; 16384])
                .err()
                .unwrap();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert_eq!(error.phase, phase);
            assert_eq!(
                error.cause,
                if matches!(at, 1 | 2 | 4 | 5 | 8 | 9) {
                    "io"
                } else {
                    "namespace"
                }
            );
            {
                let t = trace.lock().unwrap();
                assert_eq!(t.hit, 1);
                assert_install_events(&t.events, at + 1);
                assert_eq!(t.events[at]["completed"], after);
                assert_eq!(t.handles, 0);
            }
            let before = inventory(&path);
            let before_manifest = file_manifest(&path);
            let eligible = at == 0 && !after;
            let (writer, retry_trace) = Writer::acquire(&path);
            let retry = GenesisWal::create(writer, 1, &mut slot, &mut [0; 16384]);
            assert_eq!(retry.is_ok(), eligible);
            if !eligible {
                assert_eq!(retry.err().unwrap().effect, Effect::Unchanged);
                assert!(retry_trace.lock().unwrap().events.is_empty());
                assert_eq!(inventory(&path), before);
            }
            rows.push(json!({"at":at,"after":after,"effect":"CallerMustResolve","events":trace.lock().unwrap().events,"files_at_fault":before_manifest,"fresh_retry_eligible":eligible,"status":"PASS"}));
        }
    }
    // Real write prefixes and real sync-after-success errors preserve uncertainty.
    for (label, fault, length, count) in [
        ("prefix", Fault::Prefix(12, 96), 4192, 13),
        ("write_after", Fault::After(12), 4288, 13),
        ("sync_after", Fault::After(13), 4288, 14),
    ] {
        let path = root.join(label);
        fs::create_dir(&path).unwrap();
        let (writer, trace) = Writer::acquire(&path);
        let mut slot = memory();
        let session = GenesisWal::create(writer, 1, &mut slot, &mut [0; 16384]).unwrap();
        let first = session.prefix().unwrap();
        let frame = frame(first.database, 0, 1, &[]);
        session.stage(&frame).unwrap();
        trace.lock().unwrap().fault = Some(fault);
        let error = session.sync().unwrap_err();
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert_eq!(error.cause, "io");
        assert_eq!(
            error.phase,
            if label == "sync_after" {
                "sync"
            } else {
                "write"
            }
        );
        assert!(session.is_fenced());
        {
            let t = trace.lock().unwrap();
            assert_eq!(t.hit, 1);
            assert_install_events(&t.events, count);
        }
        let actual = fs::read(path.join(WAL)).unwrap();
        assert_eq!(actual.len(), length);
        assert_eq!(&actual[4096..], &frame[..length - 4096]);
        let before = inventory(&path);
        assert_eq!(session.stage(&frame).unwrap_err().cause, "fenced");
        assert_eq!(session.sync().unwrap_err().cause, "fenced");
        assert_eq!(session.prefix().unwrap_err().cause, "fenced");
        assert_eq!(trace.lock().unwrap().events.len(), count);
        assert_eq!(inventory(&path), before);
        drop(session);
        assert_eq!(inventory(&path), before);
        assert_eq!(trace.lock().unwrap().handles, 0);
        rows.push(json!({"append_fault":label,"files":file_manifest(&path),"events":trace.lock().unwrap().events,"status":"PASS"}));
    }
    assert_eq!(rows.len(), 27);
    write_json(
        &root.join("result.json"),
        &json!({"status":"PASS","installation_faults":24,"append_faults":3,"cases":rows}),
    );
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn child_at_cut(path: &Path, hook: &str, receipt: &Path) {
    assert!(CUTS.iter().any(|(name, _)| *name == hook));
    let (writer, trace) = Writer::acquire(path);
    {
        let mut t = trace.lock().unwrap();
        t.cut = Some(hook.to_owned());
        t.hook_path = Some(receipt.to_owned());
    }
    let mut slot = memory();
    let session = GenesisWal::create(writer, 1, &mut slot, &mut [0; 16384]).unwrap();
    let db = session.prefix().unwrap().database;
    session.stage(&frame(db, 0, 1, &[])).unwrap();
    session.sync().unwrap();
    panic!("requested native cut was not reached");
}

#[test]
fn native_genesis_process_kills_preserve_observed_prefixes() {
    if let Some(path) = std::env::var_os("XADB_WAL_KILL_CHILD_PATH") {
        child_at_cut(
            Path::new(&path),
            &std::env::var("XADB_WAL_KILL_CHILD_CUT").unwrap(),
            Path::new(&std::env::var_os("XADB_WAL_KILL_CHILD_RECEIPT").unwrap()),
        );
        unreachable!();
    }
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("kills");
    let mut rows = Vec::new();
    for (cut, at) in CUTS {
        for repetition in 0..3 {
            let case = root.join(format!("{cut}-{repetition}"));
            fs::create_dir(&case).unwrap();
            let path = case.join("database");
            fs::create_dir(&path).unwrap();
            let receipt = case.join("hook.json");
            let mut child = KillOnDrop(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "native_genesis_process_kills_preserve_observed_prefixes",
                        "--nocapture",
                        "--test-threads=1",
                    ])
                    .env("XADB_WAL_KILL_CHILD_PATH", &path)
                    .env("XADB_WAL_KILL_CHILD_CUT", cut)
                    .env("XADB_WAL_KILL_CHILD_RECEIPT", &receipt)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            );
            let stdout = child.0.stdout.take().unwrap();
            let stderr = child.0.stderr.take().unwrap();
            let (lines, receiver) = mpsc::channel();
            let out_path = case.join("stdout.log");
            let err_path = case.join("stderr.log");
            let output = std::thread::spawn(move || {
                let mut retained = fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(out_path)
                    .unwrap();
                for line in BufReader::new(stdout).lines() {
                    let line = line.unwrap();
                    use std::io::Write;
                    writeln!(retained, "{line}").unwrap();
                    lines.send(line).unwrap();
                }
            });
            let errors = std::thread::spawn(move || {
                let mut reader = stderr;
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes).unwrap();
                fs::write(err_path, bytes).unwrap();
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            let marker = format!("XADB_WAL_CUT|{cut}");
            let mut count = 0;
            loop {
                let line = receiver
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .expect("native hook missing/timeout");
                if let Some(start) = line.find("XADB_WAL_CUT|") {
                    assert_eq!(&line[start..], marker);
                    count += 1;
                    break;
                }
            }
            child.0.kill().unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let status = loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                assert!(Instant::now() < deadline, "killed worker not reaped");
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(status.signal(), Some(9));
            output.join().unwrap();
            errors.join().unwrap();
            for line in receiver.try_iter() {
                if line.contains("XADB_WAL_CUT|") {
                    count += 1;
                }
            }
            assert_eq!(count, 1);
            let observed: serde_json::Value =
                serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
            assert_eq!(observed["cut"], cut);
            assert_eq!(observed["after"], at);
            assert_install_events(observed["events"].as_array().unwrap(), at + 1);
            for event in observed["events"].as_array().unwrap() {
                assert_eq!(event["completed"], true);
                assert_eq!(event["injected"], serde_json::Value::Null);
            }
            let files = inventory(&path);
            let profile = &files["d3-profile"];
            let db: [u8; 16] = profile[16..32].try_into().unwrap();
            assert_eq!(observed["entropy"], json!(db));
            let (expected_profile, mut expected_epoch, expected_writer) = expected_controls(db, 1);
            assert_eq!(profile, &expected_profile);
            assert_eq!(files["LOCK"], Vec::<u8>::new());
            let mut names = vec!["LOCK", "d3-profile"];
            if at >= 5 {
                if at >= 12 {
                    expected_epoch.extend(frame(db, 0, 1, &[]));
                }
                assert_eq!(files[WAL], expected_epoch);
                names.push(WAL);
            }
            if at == 9 {
                assert_eq!(files["d3-writer-stage"], expected_writer);
                names.push("d3-writer-stage");
            }
            if at >= 10 {
                assert_eq!(files["WRITER"], expected_writer);
                names.push("WRITER");
            }
            names.sort();
            assert_eq!(files.keys().map(String::as_str).collect::<Vec<_>>(), names);
            let (writer, retry_trace) = Writer::acquire(&path);
            let mut slot = memory();
            let error = GenesisWal::create(writer, 1, &mut slot, &mut [0; 16384])
                .err()
                .unwrap();
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(retry_trace.lock().unwrap().events.is_empty());
            assert_eq!(inventory(&path), files);
            rows.push(json!({"cut":cut,"after":at,"repetition":repetition,"signal":9,"hook_count":count,"files":file_manifest(&path),"receipt":receipt,"status":"PASS"}));
        }
    }
    assert_eq!(rows.len(), 24);
    write_json(
        &root.join("result.json"),
        &json!({"status":"PASS","scope":"SIGKILL pagecache observation, not real power loss or recovery","cases":rows}),
    );
}
