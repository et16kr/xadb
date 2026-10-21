//! Actual Linux effects and SIGKILL images, separately checked from durable models.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use xadb_engine::testing::storage_session::FreshStorage;
use xadb_foundation::{
    codec::content_digest,
    error::Effect,
    namespace::{DirectoryPort, NamespaceErrorKind},
};
use xadb_platform::namespace::{LinuxDirectory, LinuxWriterLease};
#[path = "support/storage_session.rs"]
mod support;
use support::*;

static NATIVE_PROCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
const DATA: &str = "d3-data-0000000100000001";
const BANK0: &str = "d3-filemap-0";
const BANK1: &str = "d3-filemap-1";

fn literal(id: u32, name: &str) -> Vec<u8> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../engine/tests/fixtures/dev3-storage-session/literal/profile{id}-{name}.bin"
    )))
    .unwrap()
}
fn expected_objects(db: [u8; 16], id: u32) -> BTreeMap<String, Vec<u8>> {
    // Independent literal offset substitution. No engine control/page encoder.
    let (profile, wal, writer) = expected_controls(db, id);
    let mut allocation = literal(id, "allocation");
    allocation[8..24].copy_from_slice(&db);
    seal(&mut allocation);
    let mut chunk = literal(id, "chunk");
    chunk[16..32].copy_from_slice(&db);
    seal(&mut chunk);
    let mut root = literal(id, "root");
    root[16..32].copy_from_slice(&db);
    root[224..256].copy_from_slice(&content_digest(&chunk));
    seal(&mut root);
    let mut selector = literal(id, "FILEMAP");
    selector[8..24].copy_from_slice(&db);
    selector[48..80].copy_from_slice(&content_digest(&profile));
    selector[112..144].copy_from_slice(&content_digest(&root));
    seal(&mut selector);
    let mut descriptor = literal(id, "d3-bootstrap");
    descriptor[16..32].copy_from_slice(&db);
    descriptor[48..80].copy_from_slice(&content_digest(&profile));
    descriptor[176..208].copy_from_slice(&content_digest(&root));
    descriptor[208..240].copy_from_slice(&content_digest(&allocation));
    seal(&mut descriptor);
    let mut data = vec![0; 1048576];
    data[..16384].copy_from_slice(&allocation);
    let mut bank0 = vec![0; 2113536];
    bank0[..320].copy_from_slice(&root);
    bank0[16384..16640].copy_from_slice(&chunk);
    BTreeMap::from([
        ("LOCK".into(), Vec::new()),
        ("d3-profile".into(), profile),
        (WAL.into(), wal),
        ("WRITER".into(), writer),
        ("d3-bootstrap".into(), descriptor),
        (DATA.into(), data),
        (BANK0.into(), bank0),
        (BANK1.into(), vec![0; 2113536]),
        ("FILEMAP".into(), selector),
    ])
}
fn plan() -> Vec<(&'static str, &'static str, u64, usize)> {
    let mut out = Vec::new();
    for (name, length, stride) in [
        ("d3-profile", 4096, 4096),
        (WAL, 4096, 4096),
        ("d3-writer-stage", 4096, 4096),
        ("d3-bootstrap", 4096, 4096),
        (DATA, 1048576, 16384),
        (BANK0, 2113536, 16384),
        (BANK1, 2113536, 16384),
        ("d3-filemap-select-stage", 4096, 4096),
    ] {
        out.push(("create", name, 0, 0));
        for offset in (0..length).step_by(stride) {
            out.push(("write", name, offset as u64, stride));
        }
        out.push(("sync", name, 0, 0));
        if matches!(name, "d3-writer-stage" | "d3-filemap-select-stage") {
            out.push(("publish", name, 0, 0));
        }
        if matches!(
            name,
            "d3-wal-00000001"
                | "d3-writer-stage"
                | "d3-bootstrap"
                | "d3-filemap-1"
                | "d3-filemap-select-stage"
        ) {
            out.push(("dirsync", "", 0, 0));
        }
    }
    assert_eq!(out.len(), 350);
    out
}
fn expected_prefix(db: [u8; 16], completed: usize) -> BTreeMap<String, Vec<u8>> {
    let objects = expected_objects(db, 1);
    let mut out = BTreeMap::from([("LOCK".into(), Vec::new())]);
    for (kind, name, offset, length) in plan().into_iter().take(completed) {
        let selected = match name {
            "d3-writer-stage" => "WRITER",
            "d3-filemap-select-stage" => "FILEMAP",
            _ => name,
        };
        match kind {
            "create" => {
                assert!(out.insert(name.into(), Vec::new()).is_none());
            }
            "write" => {
                let bytes = out.get_mut(name).unwrap();
                assert_eq!(bytes.len(), offset as usize);
                bytes.extend_from_slice(
                    &objects[selected][offset as usize..offset as usize + length],
                );
            }
            "publish" => {
                let bytes = out.remove(name).unwrap();
                assert!(out.insert(selected.into(), bytes).is_none());
            }
            "sync" | "dirsync" => {}
            _ => unreachable!(),
        }
    }
    out
}
fn phase(index: usize) -> &'static str {
    match index {
        13 => "descriptor_create",
        14 => "descriptor_write",
        15 => "descriptor_sync",
        16 => "descriptor_directory",
        17 => "data_create",
        18..=81 => "data_write",
        82 => "data_sync",
        83 => "bank0_create",
        84..=212 => "bank0_write",
        213 => "bank0_sync",
        214 => "bank1_create",
        215..=343 => "bank1_write",
        344 => "bank1_sync",
        345 => "files_directory",
        346 => "selector_create",
        347 => "selector_write",
        348 => "selector_sync",
        349 => "selector_publish",
        350 => "selector_directory",
        _ => panic!("unfrozen mutation"),
    }
}
fn assert_events(events: &[Value], count: usize, after: bool) {
    assert_eq!(events.len(), count);
    let expected = plan();
    for (i, event) in events.iter().enumerate() {
        let (kind, name, offset, length) = expected[i];
        assert_eq!(event["at"], i);
        assert_eq!(event["kind"], kind);
        assert_eq!(event["name"], name);
        assert_eq!(event["offset"], offset);
        assert_eq!(event["length"], length);
        assert_eq!(event["completed"], i + 1 < count || after);
        if kind == "write" && (i + 1 < count || after) {
            assert_eq!(event["completed_bytes"], length);
        }
    }
}
fn lock_receipt(path: &Path) -> Value {
    let meta = fs::symlink_metadata(path.join("LOCK")).unwrap();
    assert!(meta.file_type().is_file());
    assert_eq!(meta.len(), 0);
    json!({"name":"LOCK","regular":true,"length":0,"device":meta.dev(),"inode":meta.ino(),"links":meta.nlink()})
}
fn canonical_worker(root: &Path) -> (PathBuf, String) {
    let current = std::env::current_exe().unwrap();
    let bytes = fs::read(&current).unwrap();
    let hash = hex(&content_digest(&bytes));
    let destination = root
        .parent()
        .unwrap()
        .join(format!("storage-session-worker-{hash}"));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
    {
        Ok(mut file) => {
            file.write_all(&bytes).unwrap();
            file.set_permissions(fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            assert!(fs::read(&destination).unwrap() == bytes);
        }
        Err(e) => panic!("cannot retain worker: {e}"),
    }
    (destination, hash)
}
fn filesystem(path: &Path) -> String {
    let output = Command::new("findmnt")
        .args(["-n", "-T"])
        .arg(path)
        .args(["-o", "FSTYPE"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let kind = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert_eq!(kind, "ext4");
    kind
}
fn retry_refuses(path: &Path) {
    let before = inventory(path);
    let (writer, trace) = Writer::acquire(path);
    let mut slot = memory();
    let e = FreshStorage::create(writer, 1, &mut slot, &mut [[0; 16384]; 3])
        .err()
        .unwrap();
    assert_eq!(e.effect, Effect::Unchanged);
    assert!(trace.lock().unwrap().events.is_empty());
    assert!(inventory(path) == before);
}

#[test]
fn bootstrap_native_profiles_and_lease() {
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("profiles");
    let (binary, binary_sha256) = canonical_worker(&root);
    let filesystem = filesystem(&root);
    let mut rows = Vec::new();
    assert!(FreshStorage::<LinuxWriterLease>::metadata_bytes() <= 16384);
    assert!(FreshStorage::<Writer>::metadata_bytes() <= 16384);
    for id in 1..=3 {
        let path = root.join(format!("profile-{id}"));
        fs::create_dir(&path).unwrap();
        let (writer, trace) = Writer::acquire(&path);
        let mut slot = memory();
        let session = FreshStorage::create(writer, id, &mut slot, &mut [[0; 16384]; 3]).unwrap();
        let prefix = session.prefix().unwrap();
        assert_ne!(prefix.database, [0; 16]);
        assert_eq!(trace.lock().unwrap().entropy, Some(prefix.database));
        let mut expected = expected_objects(prefix.database, id);
        assert!(inventory(&path) == expected);
        let before = file_manifest(&path);
        let port = LinuxDirectory::open_existing(&path).unwrap();
        assert!(matches!(port.acquire_writer(),Err(e)if e.kind==NamespaceErrorKind::Busy));
        assert_events(&trace.lock().unwrap().events, 350, true);
        let bytes = frame(prefix.database, 0, 1, &[]);
        let (digest, count, synced) = session.read_and_append(&bytes).unwrap();
        assert_eq!(
            (digest, count),
            (content_digest(&expected[DATA][..16384]), 1)
        );
        assert_eq!((synced.synced_lsn, synced.synced_offset), (1, 4288));
        expected.get_mut(WAL).unwrap().extend(bytes);
        let after = file_manifest(&path);
        for (a, b) in before
            .as_array()
            .unwrap()
            .iter()
            .zip(after.as_array().unwrap())
        {
            assert_eq!(
                (
                    a["name"].clone(),
                    a["device"].clone(),
                    a["inode"].clone(),
                    a["links"].clone()
                ),
                (
                    b["name"].clone(),
                    b["device"].clone(),
                    b["inode"].clone(),
                    b["links"].clone()
                )
            );
        }
        assert!(inventory(&path) == expected);
        {
            let t = trace.lock().unwrap();
            assert_eq!((t.handles, t.peak, t.events.len()), (1, 3, 352));
        }
        drop(session);
        assert_eq!(trace.lock().unwrap().handles, 0);
        retry_refuses(&path);
        rows.push(json!({"profile":id,"database":path,"entropy":prefix.database,"files_before_append":before,"files":file_manifest(&path),"lock":lock_receipt(&path),
            "events":trace.lock().unwrap().events,"shared_read_buffer_and_wal":true,"fresh_retry_mutations":0,"status":"PASS"}));
    }
    write_json(
        &root.join("result.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"PASS","scope":"native_profiles","filesystem":filesystem,"binary":binary,"binary_sha256":binary_sha256,"cases":rows}),
    );
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let end = Instant::now() + Duration::from_secs(10);
            while self.0.try_wait().ok().flatten().is_none() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
fn child(mode: &str) {
    let path = PathBuf::from(std::env::var_os("XADB_STORAGE_CHILD_DATABASE").unwrap());
    let receipt = PathBuf::from(std::env::var_os("XADB_STORAGE_CHILD_RECEIPT").unwrap());
    let (writer, trace) = Writer::acquire(&path);
    let mut slot = memory();
    if mode == "kill" {
        {
            let mut t = trace.lock().unwrap();
            t.cut = Some(std::env::var("XADB_STORAGE_CHILD_CUT").unwrap());
            t.hook_path = Some(receipt);
        }
        let _session = FreshStorage::create(writer, 1, &mut slot, &mut [[0; 16384]; 3]).unwrap();
        panic!("requested native hook missing");
    }
    let at: usize = std::env::var("XADB_STORAGE_CHILD_AT")
        .unwrap()
        .parse()
        .unwrap();
    let after = std::env::var("XADB_STORAGE_CHILD_AFTER").unwrap() == "true";
    trace.lock().unwrap().fault = Some(if after {
        Fault::After(at)
    } else {
        Fault::Before(at)
    });
    let e = FreshStorage::create(writer, 1, &mut slot, &mut [[0; 16384]; 3])
        .err()
        .expect("fault did not stop installation");
    let t = trace.lock().unwrap();
    assert_eq!(t.handles, 0);
    write_json(
        &receipt,
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"ERROR_OBSERVED","phase":e.phase,"cause":e.cause,"effect":format!("{:?}",e.effect),"events":t.events,"entropy":t.entropy,"hit":t.hit}),
    );
    println!("XADB_STORAGE_DONE");
    std::io::stdout().flush().unwrap();
}
fn run_child(binary: &Path, case: &Path, mode: &str, at: usize, after: bool, cut: &str) -> Value {
    let database = case.join("database");
    fs::create_dir(&database).unwrap();
    let receipt = case.join("worker.json");
    let test = if mode == "kill" {
        "bootstrap_native_kill_images"
    } else {
        "bootstrap_native_fault_images"
    };
    let mut child = KillOnDrop(
        Command::new(binary)
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env("XADB_STORAGE_CHILD_DATABASE", &database)
            .env("XADB_STORAGE_CHILD_RECEIPT", &receipt)
            .env("XADB_STORAGE_CHILD_CUT", cut)
            .env("XADB_STORAGE_CHILD_AT", at.to_string())
            .env("XADB_STORAGE_CHILD_AFTER", after.to_string())
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
            .write(true)
            .create_new(true)
            .open(out_path)
            .unwrap();
        for line in BufReader::new(stdout).lines() {
            let line = line.unwrap();
            writeln!(retained, "{line}").unwrap();
            let _ = lines.send(line);
        }
    });
    let errors = std::thread::spawn(move || {
        let mut stderr = stderr;
        let mut retained = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(err_path)
            .unwrap();
        std::io::copy(&mut stderr, &mut retained).unwrap();
    });
    let marker = if mode == "kill" {
        format!("XADB_STORAGE_CUT|{cut}")
    } else {
        "XADB_STORAGE_DONE".into()
    };
    let hook_start = Instant::now();
    let end = hook_start + Duration::from_secs(60);
    let mut count = 0;
    loop {
        let line = receiver
            .recv_timeout(end.saturating_duration_since(Instant::now()))
            .expect("native worker hook missing/timeout");
        if let Some(start) = line.find("XADB_STORAGE_") {
            assert_eq!(&line[start..], marker);
            count += 1;
            break;
        }
    }
    let hook_elapsed_ms = hook_start.elapsed().as_millis();
    let exit_start = Instant::now();
    if mode == "kill" {
        child.0.kill().unwrap();
    }
    let end = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < end, "worker did not exit within deadline");
        std::thread::sleep(Duration::from_millis(10));
    };
    let exit_elapsed_ms = exit_start.elapsed().as_millis();
    if mode == "kill" {
        assert_eq!(status.signal(), Some(9));
    } else {
        assert!(status.success());
    }
    output.join().unwrap();
    errors.join().unwrap();
    for line in receiver.try_iter() {
        if line.contains(&marker) {
            count += 1;
        }
    }
    assert_eq!(count, 1);
    let value: Value = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
    write_json(
        &case.join("process.json"),
        &json!({"binary":binary,"hook_count":count,"signal":status.signal(),"exit_code":status.code(),"hook_timeout_seconds":60,"exit_timeout_seconds":10,"hook_elapsed_ms":hook_elapsed_ms,"exit_elapsed_ms":exit_elapsed_ms,"timed_out":false}),
    );
    value
}

#[test]
fn bootstrap_native_fault_images() {
    if std::env::var_os("XADB_STORAGE_CHILD_DATABASE").is_some() {
        child("fault");
        return;
    }
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("faults");
    let (binary, binary_sha256) = canonical_worker(&root);
    let filesystem = filesystem(&root);
    let mut rows = Vec::new();
    for at in 12..350 {
        for after in [false, true] {
            let case = root.join(format!(
                "{}-{}",
                at + 1,
                if after { "after" } else { "before" }
            ));
            fs::create_dir(&case).unwrap();
            let value = run_child(&binary, &case, "fault", at, after, "");
            assert_eq!(value["hit"], 1);
            assert_eq!(value["effect"], "CallerMustResolve");
            assert_eq!(value["phase"], phase(at + 1));
            assert_eq!(
                value["cause"],
                if matches!(plan()[at].0, "write" | "sync") {
                    "io"
                } else {
                    "namespace"
                }
            );
            let events = value["events"].as_array().unwrap();
            assert_events(events, at + 1, after);
            for (i, event) in events.iter().enumerate() {
                assert_eq!(
                    event["injected"],
                    if i == at {
                        json!(if after { "after" } else { "before" })
                    } else {
                        Value::Null
                    }
                );
            }
            let db: [u8; 16] = serde_json::from_value(value["entropy"].clone()).unwrap();
            assert_ne!(db, [0; 16]);
            let database = case.join("database");
            assert!(inventory(&database) == expected_prefix(db, at + usize::from(after)));
            retry_refuses(&database);
            rows.push(json!({"at":at,"after":after,"case":case,"worker_receipt":"worker.json","files":file_manifest(&database),"lock":lock_receipt(&database),"fresh_retry_mutations":0,"status":"PASS"}));
        }
    }
    assert_eq!(rows.len(), 676);
    write_json(
        &root.join("result.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"PASS","scope":"native_faults","filesystem":filesystem,"binary":binary,"binary_sha256":binary_sha256,"cases":rows}),
    );
}

#[test]
fn bootstrap_native_kill_images() {
    if std::env::var_os("XADB_STORAGE_CHILD_DATABASE").is_some() {
        child("kill");
        return;
    }
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("kills");
    let (binary, binary_sha256) = canonical_worker(&root);
    let filesystem = filesystem(&root);
    let mut rows = Vec::new();
    for (cut, at) in CUTS {
        for repetition in 0..3 {
            let case = root.join(format!("{cut}-{repetition}"));
            fs::create_dir(&case).unwrap();
            let value = run_child(&binary, &case, "kill", at, true, cut);
            assert_eq!(value["cut"], cut);
            assert_eq!(value["after"], at);
            let events = value["events"].as_array().unwrap();
            assert_events(events, at + 1, true);
            for event in events {
                assert_eq!(event["injected"], Value::Null);
            }
            let db: [u8; 16] = serde_json::from_value(value["entropy"].clone()).unwrap();
            assert_ne!(db, [0; 16]);
            let database = case.join("database");
            assert!(inventory(&database) == expected_prefix(db, at + 1));
            retry_refuses(&database);
            rows.push(json!({"cut":cut,"at":at,"repetition":repetition,"case":case,"worker_receipt":"worker.json","files":file_manifest(&database),"lock":lock_receipt(&database),"fresh_retry_mutations":0,"status":"PASS"}));
        }
    }
    assert_eq!(rows.len(), 39);
    write_json(
        &root.join("result.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"PASS","scope":"native_kills","filesystem":filesystem,"binary":binary,"binary_sha256":binary_sha256,"cases":rows}),
    );
}
