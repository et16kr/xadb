//! Native DWB installation cuts, with original Fresh files and worker identity.
//! Reuses the real Linux fault port; its explicit cut index starts after Fresh.
use engine::testing::storage_session::{DoublewriteStorage, FreshStorage};
use foundation::codec::content_digest;
use platform::namespace::LinuxWriterLease;
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
#[path = "support/storage_session.rs"]
mod support;
use support::*;

static NATIVE_PROCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/fixtures/dev3-doublewrite-install"
);
const DWB_CUTS: [usize; 13] = [4, 6, 7, 8, 9, 10, 13, 16, 19, 20, 23, 24, 25];
fn workspace() -> Box<[u8; 4194304]> {
    vec![0; 4194304].into_boxed_slice().try_into().unwrap()
}
fn literal(id: u32, name: &str) -> Vec<u8> {
    fs::read(format!("{FIXTURES}/literal/profile{id}-{name}.bin")).unwrap()
}
fn plan() -> Vec<Value> {
    serde_json::from_slice(&fs::read(format!("{FIXTURES}/effects.json")).unwrap()).unwrap()
}
fn objects(db: [u8; 16], id: u32) -> BTreeMap<String, Vec<u8>> {
    let mut result = expected_storage_objects(db, id);
    let profile_hash = content_digest(&result["d3-profile"]);
    let mut empty = literal(id, "empty");
    empty[8..24].copy_from_slice(&db);
    seal(&mut empty);
    let empty_hash = content_digest(&empty);
    let mut selector = literal(id, "DWB");
    selector[8..24].copy_from_slice(&db);
    selector[48..80].copy_from_slice(&profile_hash);
    selector[112..144].copy_from_slice(&empty_hash);
    seal(&mut selector);
    let mut descriptor = literal(id, "d3-dwb-stage");
    descriptor[16..32].copy_from_slice(&db);
    descriptor[48..80].copy_from_slice(&profile_hash);
    descriptor[160..192].copy_from_slice(&result["FILEMAP"][112..144]);
    descriptor[232..264].copy_from_slice(&empty_hash);
    seal(&mut descriptor);
    let mut bank0 = vec![0; 16384];
    bank0[..160].copy_from_slice(&empty);
    result.insert("d3-dwb-stage".into(), descriptor);
    result.insert("DWB".into(), selector);
    result.insert("d3-dwb-0".into(), bank0);
    result.insert("d3-dwb-1".into(), vec![0; 16384]);
    result.insert("d3-dwb-images".into(), vec![0; 16711680]);
    result.insert("d3-dwb-reserved".into(), vec![0; 8192]);
    result
}
fn expected_prefix(db: [u8; 16], count: usize) -> BTreeMap<String, Vec<u8>> {
    let objects = objects(db, 1);
    let mut result = expected_storage_objects(db, 1);
    for row in plan().iter().take(count) {
        let name = row["name"].as_str().unwrap();
        let offset = row["offset"].as_u64().unwrap() as usize;
        let length = row["length"].as_u64().unwrap() as usize;
        let selected = if name == "d3-dwb-select-stage" {
            "DWB"
        } else {
            name
        };
        match row["op"].as_str().unwrap() {
            "create" => {
                assert!(result.insert(name.into(), Vec::new()).is_none());
            }
            "write" => {
                let bytes = result.get_mut(name).unwrap();
                assert_eq!(bytes.len(), offset);
                bytes.extend_from_slice(&objects[selected][offset..offset + length]);
            }
            "publish" => {
                let bytes = result.remove(name).unwrap();
                assert!(result.insert("DWB".into(), bytes).is_none());
            }
            "sync" | "directory_sync" => {}
            _ => panic!("unknown frozen effect"),
        }
    }
    result
}
fn phase(index: usize) -> &'static str {
    [
        "descriptor_create",
        "descriptor_write",
        "descriptor_sync",
        "descriptor_directory",
        "images_create",
        "images_write",
        "images_write",
        "images_write",
        "images_write",
        "images_sync",
        "bank0_create",
        "bank0_write",
        "bank0_sync",
        "bank1_create",
        "bank1_write",
        "bank1_sync",
        "reserved_create",
        "reserved_write",
        "reserved_sync",
        "files_directory",
        "selector_create",
        "selector_write",
        "selector_sync",
        "selector_publish",
        "selector_directory",
    ][index]
}
fn assert_events(events: &[Value], count: usize, after: bool) {
    assert_eq!(events.len(), count);
    let expected = plan();
    for (i, e) in events.iter().enumerate() {
        let row = &expected[i];
        assert_eq!(e["at"], i);
        assert_eq!(
            e["kind"],
            if row["op"] == "directory_sync" {
                json!("dirsync")
            } else {
                row["op"].clone()
            }
        );
        for key in ["name", "offset", "length"] {
            assert_eq!(e[key], row[key]);
        }
        assert_eq!(e["completed"], i + 1 < count || after);
        if row["op"] == "write" && (i + 1 < count || after) {
            assert_eq!(e["completed_bytes"], row["length"]);
        }
    }
}
fn manifest(path: &Path) -> Value {
    json!(inventory(path).iter().map(|(name,bytes)|{
        let meta=fs::symlink_metadata(path.join(name)).unwrap();assert!(meta.file_type().is_file());assert_eq!(meta.nlink(),1);
        json!({"name":name,"length":meta.len(),"sha256":hex(&content_digest(bytes)),"device":meta.dev(),"inode":meta.ino(),"links":meta.nlink()})
    }).collect::<Vec<_>>())
}
fn assert_old(before: &Value, after: &Value) {
    assert_eq!(before.as_array().unwrap().len(), 9);
    for row in before.as_array().unwrap() {
        let actual = after
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == row["name"])
            .unwrap();
        assert_eq!(row, actual);
    }
}
fn clear_fresh_trace(trace: &std::sync::Arc<std::sync::Mutex<Trace>>) {
    let mut trace = trace.lock().unwrap();
    assert_eq!(trace.events.len(), 350);
    assert_eq!(trace.handles, 1);
    trace.events.clear();
    trace.peak = 1;
    assert_eq!(trace.hit, 0);
}

#[test]
fn doublewrite_install_native_profiles() {
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("dwb-profiles");
    let (binary, binary_sha256) = canonical_worker(&root);
    let filesystem = filesystem(&root);
    let mut cases = Vec::new();
    assert!(DoublewriteStorage::<LinuxWriterLease>::metadata_bytes() <= 16384);
    assert!(DoublewriteStorage::<Writer>::metadata_bytes() <= 16384);
    for id in 1..=3 {
        let path = root.join(format!("profile-{id}"));
        fs::create_dir(&path).unwrap();
        let (writer, trace) = Writer::acquire(&path);
        let mut slot = memory();
        let mut workspace = workspace();
        let fresh = FreshStorage::create(writer, id, &mut slot, &mut [[0; 16384]; 3]).unwrap();
        let db = fresh.prefix().unwrap().database;
        assert_eq!(trace.lock().unwrap().entropy, Some(db));
        assert!(inventory(&path) == expected_storage_objects(db, id));
        let before = manifest(&path);
        clear_fresh_trace(&trace);
        let owner = fresh.install_doublewrite(&mut workspace).unwrap();
        let expected = objects(db, id);
        assert!(inventory(&path) == expected);
        assert_events(&trace.lock().unwrap().events, 25, true);
        assert_eq!(trace.lock().unwrap().peak, 2);
        let (hash, count) = owner.read_allocation(db).unwrap();
        assert_eq!(
            (hash, count),
            (
                content_digest(&expected["d3-data-0000000100000001"][..16384]),
                1
            )
        );
        drop(owner);
        assert_eq!(trace.lock().unwrap().handles, 0);
        let after = manifest(&path);
        assert_old(&before, &after);
        cases.push(json!({"profile":id,"database":path,"entropy":db,"files_before":before,"files":after,"events":trace.lock().unwrap().events,"clean_allocation_read":true,"status":"PASS"}));
    }
    write_json(
        &root.join("result.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"PASS","scope":"doublewrite_native_profiles","filesystem":filesystem,"binary":binary,"binary_sha256":binary_sha256,"cases":cases}),
    );
}

fn child(mode: &str) {
    let path = PathBuf::from(std::env::var_os("XADB_STORAGE_CHILD_DATABASE").unwrap());
    let receipt = PathBuf::from(std::env::var_os("XADB_STORAGE_CHILD_RECEIPT").unwrap());
    let at: usize = std::env::var("XADB_STORAGE_CHILD_AT")
        .unwrap()
        .parse()
        .unwrap();
    let after = std::env::var("XADB_STORAGE_CHILD_AFTER").unwrap() == "true";
    let (writer, trace) = Writer::acquire(&path);
    let mut slot = memory();
    let mut workspace = workspace();
    let fresh = FreshStorage::create(writer, 1, &mut slot, &mut [[0; 16384]; 3]).unwrap();
    let db = fresh.prefix().unwrap().database;
    assert!(inventory(&path) == expected_storage_objects(db, 1));
    write_json(
        &receipt.with_file_name("before.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"entropy":db,"files":manifest(&path)}),
    );
    clear_fresh_trace(&trace);
    if mode == "kill" {
        {
            let mut t = trace.lock().unwrap();
            t.cut = Some(std::env::var("XADB_STORAGE_CHILD_CUT").unwrap());
            t.cut_index = Some(at);
            t.hook_path = Some(receipt);
        }
        let _owner = fresh.install_doublewrite(&mut workspace).unwrap();
        panic!("requested DWB hook missing");
    }
    trace.lock().unwrap().fault = Some(if after {
        Fault::After(at)
    } else {
        Fault::Before(at)
    });
    let error = fresh
        .install_doublewrite(&mut workspace)
        .err()
        .expect("fault did not stop DWB installation");
    let lower = if let Some(e) = &error.lower_io {
        json!({"code":format!("{:?}",e.code), "kind":Value::Null,
               "effect":format!("{:?}",e.effect()), "io_kind":e.io_kind.map(|v|format!("{v:?}"))})
    } else if let Some(e) = &error.lower_namespace {
        json!({"code":Value::Null, "kind":format!("{:?}",e.kind),
               "effect":format!("{:?}",e.effect), "io_kind":e.io_kind.map(|v|format!("{v:?}"))})
    } else {
        panic!("unexpected non-I/O native fault");
    };
    let t = trace.lock().unwrap();
    assert_eq!(t.handles, 0);
    write_json(
        &receipt,
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"ERROR_OBSERVED","phase":error.phase,"cause":error.cause,"detail":error.detail,"lower":lower,"effect":format!("{:?}",error.effect),"attempted":error.attempted,"completed":error.completed,"events":t.events,"entropy":t.entropy,"hit":t.hit}),
    );
    println!("XADB_STORAGE_DONE");
    std::io::stdout().flush().unwrap();
}

#[test]
fn doublewrite_install_native_faults() {
    if std::env::var_os("XADB_STORAGE_CHILD_DATABASE").is_some() {
        child("fault");
        return;
    }
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("dwb-faults");
    let (binary, binary_sha256) = canonical_worker(&root);
    let filesystem = filesystem(&root);
    let mut cases = Vec::new();
    for at in 0..25 {
        for after in [false, true] {
            let case = root.join(format!(
                "{}-{}",
                at + 1,
                if after { "after" } else { "before" }
            ));
            fs::create_dir(&case).unwrap();
            let worker = run_child(&binary, &case, "fault", at, after, "");
            assert_eq!(worker["source_sha256"], env!("XADBTEST_SOURCE_SHA256"));
            assert_eq!(worker["hit"], 1);
            assert_eq!(worker["attempted"], at + 1);
            assert_eq!(worker["completed"], at);
            assert_eq!(worker["effect"], "CallerMustResolve");
            assert_eq!(worker["phase"], phase(at));
            assert_eq!(
                worker["cause"],
                if matches!(plan()[at]["op"].as_str().unwrap(), "write" | "sync") {
                    "io"
                } else {
                    "namespace"
                }
            );
            let io = matches!(plan()[at]["op"].as_str().unwrap(), "write" | "sync");
            assert_eq!(
                worker["lower"],
                json!({
                "code":if io {json!("IoWriteUnknown")} else {Value::Null},
                "kind":if io {Value::Null} else {json!("Io")},
                "effect":if io || after {"CallerMustResolve"} else {"Unchanged"},
                "io_kind":"Other"})
            );
            assert_events(worker["events"].as_array().unwrap(), at + 1, after);
            for (i, event) in worker["events"].as_array().unwrap().iter().enumerate() {
                assert_eq!(
                    event["injected"],
                    if i == at {
                        json!(if after { "after" } else { "before" })
                    } else {
                        Value::Null
                    }
                );
            }
            let db: [u8; 16] = serde_json::from_value(worker["entropy"].clone()).unwrap();
            assert_ne!(db, [0; 16]);
            let database = case.join("database");
            assert!(inventory(&database) == expected_prefix(db, at + usize::from(after)));
            let files = manifest(&database);
            let before: Value =
                serde_json::from_slice(&fs::read(case.join("before.json")).unwrap()).unwrap();
            assert_eq!(before["source_sha256"], env!("XADBTEST_SOURCE_SHA256"));
            assert_eq!(before["entropy"], worker["entropy"]);
            assert_old(&before["files"], &files);
            cases.push(json!({"at":at,"after":after,"case":case,"worker_receipt":"worker.json","before_receipt":"before.json","files":files,"status":"PASS"}));
        }
    }
    assert_eq!(cases.len(), 50);
    write_json(
        &root.join("result.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"PASS","scope":"doublewrite_native_faults","filesystem":filesystem,"binary":binary,"binary_sha256":binary_sha256,"cases":cases}),
    );
}

#[test]
fn doublewrite_install_native_sigkill() {
    if std::env::var_os("XADB_STORAGE_CHILD_DATABASE").is_some() {
        child("kill");
        return;
    }
    let _process = NATIVE_PROCESS.lock().unwrap();
    let root = artifact_root("dwb-kills");
    let (binary, binary_sha256) = canonical_worker(&root);
    let filesystem = filesystem(&root);
    let mut cases = Vec::new();
    for index in DWB_CUTS {
        for repetition in 0..3 {
            let at = index - 1;
            let cut = format!("effect-{index}");
            let case = root.join(format!("{cut}-{repetition}"));
            fs::create_dir(&case).unwrap();
            let worker = run_child(&binary, &case, "kill", at, true, &cut);
            assert_eq!(worker["source_sha256"], env!("XADBTEST_SOURCE_SHA256"));
            assert_eq!(worker["cut"], cut);
            assert_eq!(worker["after"], at);
            assert_events(worker["events"].as_array().unwrap(), index, true);
            for e in worker["events"].as_array().unwrap() {
                assert_eq!(e["injected"], Value::Null);
            }
            let db: [u8; 16] = serde_json::from_value(worker["entropy"].clone()).unwrap();
            assert_ne!(db, [0; 16]);
            let database = case.join("database");
            assert!(inventory(&database) == expected_prefix(db, index));
            let files = manifest(&database);
            let before: Value =
                serde_json::from_slice(&fs::read(case.join("before.json")).unwrap()).unwrap();
            assert_eq!(before["source_sha256"], env!("XADBTEST_SOURCE_SHA256"));
            assert_eq!(before["entropy"], worker["entropy"]);
            assert_old(&before["files"], &files);
            cases.push(json!({"at":at,"cut":cut,"repetition":repetition,"case":case,"worker_receipt":"worker.json","before_receipt":"before.json","files":files,"status":"PASS"}));
        }
    }
    assert_eq!(cases.len(), 39);
    write_json(
        &root.join("result.json"),
        &json!({"source_sha256":env!("XADBTEST_SOURCE_SHA256"),"status":"PASS","scope":"doublewrite_native_kills","filesystem":filesystem,"binary":binary,"binary_sha256":binary_sha256,"cases":cases}),
    );
}

// Private process wiring retains the prior bounded hook/exit protocol. Test names
// and binary identity belong to this distinct fixed DWB fault domain.
fn canonical_worker(root: &Path) -> (PathBuf, String) {
    let current = std::env::current_exe().unwrap();
    let bytes = fs::read(&current).unwrap();
    let hash = hex(&content_digest(&bytes));
    let destination = root
        .parent()
        .unwrap()
        .join(format!("doublewrite-install-worker-{hash}"));
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

fn run_child(binary: &Path, case: &Path, mode: &str, at: usize, after: bool, cut: &str) -> Value {
    let database = case.join("database");
    fs::create_dir(&database).unwrap();
    let receipt = case.join("worker.json");
    let test = if mode == "kill" {
        "doublewrite_install_native_sigkill"
    } else {
        "doublewrite_install_native_faults"
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
