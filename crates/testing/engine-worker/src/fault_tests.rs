//! Native selector port evidence only; these tests neither run DEV3 recovery nor
//! simulate cache loss. The controller validates observations before releasing.
use super::*;
use std::{fs, thread, time::Duration};
use xadb_foundation::io::{read_exact_at, write_all_at};

const RUN: [u8; 16] = [11; 16];
const TOKEN: [u8; 16] = [22; 16];
const ACTOR: u8 = 2;
const DIRECTORY: u8 = 1;
const PAYLOAD: &[u8] = b"arbitrary selector bytes\0v2";
const SELECTORS: [(Selector, T, &str); 2] = [
    (Selector::FileMap, T::FileMap, "d3-filemap-select-stage"),
    (Selector::Doublewrite, T::Doublewrite, "d3-dwb-select-stage"),
];

#[test]
fn dev3_open_metadata_preserves_selector_roles() {
    use std::os::unix::fs::MetadataExt;
    use xadb_foundation::namespace::FileIdentity;

    for (_, target, stage) in SELECTORS {
        for mode in [OpenMode::ReadOnly, OpenMode::ReadWrite] {
            for phase in [wire::Phase::Open, wire::Phase::Other] {
                let temporary = tempfile::tempdir().unwrap();
                fs::write(temporary.path().join(stage), PAYLOAD).unwrap();
                let native = fs::metadata(temporary.path().join(stage)).unwrap();
                let controller = Controller::new(target, B::SyncBefore, wire::Decision::Continue);
                let mut writer = controller.writer(temporary.path());
                let _scope = scope(31, phase);
                let (mut file, metadata) = writer
                    .open_existing_with_metadata(&FileName::new(stage).unwrap(), mode)
                    .unwrap();
                assert_eq!(metadata.length, PAYLOAD.len() as u64);
                assert_eq!(
                    metadata.identity,
                    FileIdentity {
                        device: native.dev(),
                        inode: native.ino()
                    }
                );
                file.sync_all().unwrap();
                let mut actual = vec![0; PAYLOAD.len()];
                read_exact_at(&mut file, &mut actual, 0).unwrap();
                assert_eq!(actual, PAYLOAD);
                check_witness(&controller.finish(), 31, phase, target, Some(stage));
            }
        }
    }
}

fn scope(request: u64, phase: wire::Phase) -> Scope {
    Scope(CONTEXT.replace(Some(Context {
        run: RUN,
        actor: ACTOR,
        request,
        phase,
    })))
}

struct Controller {
    hub: Arc<Hub>,
    thread: thread::JoinHandle<wire::Reached>,
}

impl Controller {
    /// Private state setup replaces only environment/socket discovery. Actual
    /// adapter hit/partial, framed messages, release and file effects all run.
    fn new(target: T, boundary: B, decision: wire::Decision) -> Self {
        let plan = wire::Plan {
            cut: wire::Cut { target, boundary },
            occurrence: 1,
            actor: Some(ACTOR),
            directory: Some(DIRECTORY),
        };
        plan.validate().unwrap();
        let (worker, mut controller) = UnixStream::pair().unwrap();
        for socket in [&worker, &controller] {
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
        }
        let thread = thread::spawn(move || {
            let connected: wire::Observation = frame::read(&mut controller).unwrap().unwrap();
            assert_eq!((connected.run, connected.token), (RUN, TOKEN));
            assert!(
                matches!(connected.event, wire::Event::Connected { process_id } if process_id != 0)
            );
            let observation: wire::Observation = frame::read(&mut controller).unwrap().unwrap();
            assert_eq!((observation.run, observation.token), (RUN, TOKEN));
            let wire::Event::Reached { witness } = observation.event else {
                panic!("expected reached boundary");
            };
            assert_eq!(witness.plan, plan);
            assert_eq!(
                (witness.matched, witness.actor, witness.directory),
                (1, ACTOR, DIRECTORY)
            );
            let release = frame::encode(&wire::Release {
                version: wire::VERSION,
                run: RUN,
                token: TOKEN,
                decision,
            })
            .unwrap();
            controller.write_all(&release).unwrap();
            controller.flush().unwrap();
            let released: wire::Observation = frame::read(&mut controller).unwrap().unwrap();
            assert_eq!((released.run, released.token), (RUN, TOKEN));
            assert_eq!(released.event, wire::Event::Released { decision });
            witness
        });
        let mut control = Control {
            stream: worker,
            run: RUN,
            token: TOKEN,
            plan,
            matched: 0,
            fired: false,
            bytes: 0,
        };
        control
            .send(wire::Event::Connected {
                process_id: std::process::id(),
            })
            .unwrap();
        let hub = Arc::new(Hub(Mutex::new(State {
            configured: true,
            control: Some(control),
        })));
        Self { hub, thread }
    }

    fn writer(&self, path: &std::path::Path) -> Writer {
        Directory::new(
            LinuxDirectory::open_existing(path).unwrap(),
            Arc::clone(&self.hub),
            DIRECTORY,
        )
        .acquire_writer()
        .unwrap()
    }

    fn finish(self) -> wire::Reached {
        let witness = self.thread.join().unwrap();
        let state = self.hub.0.lock().unwrap();
        let control = state.control.as_ref().unwrap();
        assert!(control.fired);
        assert_eq!(control.matched, 1);
        assert!(control.bytes > 0 && control.bytes <= wire::MAX_CONTROL_BYTES);
        witness
    }
}

fn check_witness(
    witness: &wire::Reached,
    request: u64,
    phase: wire::Phase,
    target: T,
    file: Option<&str>,
) {
    assert_eq!(witness.request, request);
    assert_eq!(witness.phase, phase);
    assert_eq!(witness.observed_target, target);
    assert_eq!(witness.file.as_deref(), file);
    assert_eq!(witness.frame_start, None);
    assert_eq!(witness.frame_end, None);
    assert_eq!(witness.frame_written_bytes, None);
}

fn seed_selected(path: &std::path::Path) {
    fs::write(path.join("FILEMAP"), b"old-map").unwrap();
    fs::write(path.join("DWB"), b"old-dwb").unwrap();
}

fn other_selected(selected: Selector) -> (&'static str, &'static [u8]) {
    match selected {
        Selector::FileMap => ("DWB", b"old-dwb"),
        Selector::Doublewrite => ("FILEMAP", b"old-map"),
        _ => panic!("new selectors only"),
    }
}

fn prepared(writer: &mut Writer, name: &str, bytes: &[u8]) -> File {
    let mut file = writer.create_new(&FileName::new(name).unwrap()).unwrap();
    write_all_at(&mut file, bytes, 0).unwrap();
    file.sync_all().unwrap();
    file
}

fn receipt(
    suite: &str,
    case: usize,
    witness: &wire::Reached,
    decision: wire::Decision,
    bytes: &[u8],
) {
    println!(
        "XADB_DEV3_SELECTOR_CASE {}",
        serde_json::json!({
            "suite":suite, "case":case, "decision":decision, "witness":witness,
            "observed_bytes":bytes, "status":"PASS", "scope":"native_port_not_recovery"
        })
    );
}

#[test]
fn dev3_selector_faults_preserve_target_lineage_and_certainty() {
    let mut cases = 0;
    for (selected, target, stage) in SELECTORS {
        for boundary in [
            B::RenameBefore,
            B::RenameAfter,
            B::DirectorySyncBefore,
            B::DirectorySyncAfter,
        ] {
            for decision in [wire::Decision::Continue, wire::Decision::ReturnError] {
                let directory = tempfile::tempdir().unwrap();
                seed_selected(directory.path());
                let old = fs::read(directory.path().join(selected.as_str())).unwrap();
                let request = 1 + cases as u64;
                let _scope = scope(request, wire::Phase::Other);
                let controller = Controller::new(target, boundary, decision);
                let mut writer = controller.writer(directory.path());
                let file = prepared(&mut writer, stage, PAYLOAD);
                let rename = writer.publish_selector(&FileName::new(stage).unwrap(), selected);
                let result = if matches!(boundary, B::DirectorySyncBefore | B::DirectorySyncAfter) {
                    rename.unwrap();
                    writer.sync_directory()
                } else {
                    if rename.is_ok() {
                        writer.sync_directory().unwrap();
                    }
                    rename
                };
                match decision {
                    wire::Decision::Continue => result.unwrap(),
                    wire::Decision::ReturnError => {
                        assert_eq!(
                            result.unwrap_err().effect,
                            if boundary == B::RenameBefore {
                                Effect::Unchanged
                            } else {
                                Effect::CallerMustResolve
                            }
                        );
                    }
                }
                let before_rename =
                    boundary == B::RenameBefore && decision == wire::Decision::ReturnError;
                let actual = fs::read(directory.path().join(selected.as_str())).unwrap();
                assert_eq!(
                    actual,
                    if before_rename {
                        old.as_slice()
                    } else {
                        PAYLOAD
                    }
                );
                assert_eq!(directory.path().join(stage).exists(), before_rename);
                let (other, expected_other) = other_selected(selected);
                assert_eq!(
                    fs::read(directory.path().join(other)).unwrap(),
                    expected_other
                );
                let mut reopened = writer
                    .open_existing(
                        &FileName::new(selected.as_str()).unwrap(),
                        OpenMode::ReadOnly,
                    )
                    .unwrap();
                let mut read = vec![0; actual.len()];
                read_exact_at(&mut reopened, &mut read, 0).unwrap();
                assert_eq!(read, actual);
                let witness = controller.finish();
                check_witness(&witness, request, wire::Phase::Other, target, Some(stage));
                assert_eq!(
                    (
                        witness.offset,
                        witness.requested_bytes,
                        witness.completed_bytes
                    ),
                    (None, None, None)
                );
                receipt("namespace", cases, &witness, decision, &actual);
                drop((reopened, file, writer));
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 16);
}

fn resembling_dev2_frame() -> Vec<u8> {
    let mut bytes = vec![0; 67];
    bytes[..6].copy_from_slice(b"AXF1\x01\x00");
    bytes[6..10].copy_from_slice(&25_u32.to_le_bytes());
    bytes[10..16].copy_from_slice(b"XDK1\x01\x00");
    bytes[16..24].copy_from_slice(&1_u64.to_le_bytes());
    bytes[24] = 6;
    bytes
}

#[test]
fn dev3_selector_write_sync_faults_use_exact_names() {
    let mut cases = 0;
    for (_, target, stage) in SELECTORS {
        for boundary in [
            B::WriteBefore,
            B::WritePartial,
            B::WriteAfter,
            B::SyncBefore,
            B::SyncAfter,
        ] {
            for decision in [wire::Decision::Continue, wire::Decision::ReturnError] {
                let directory = tempfile::tempdir().unwrap();
                let request = 30 + cases as u64;
                let _scope = scope(request, wire::Phase::Other);
                let controller = Controller::new(target, boundary, decision);
                let mut writer = controller.writer(directory.path());
                let mut file = writer.create_new(&FileName::new(stage).unwrap()).unwrap();
                let sync_cut = matches!(boundary, B::SyncBefore | B::SyncAfter);
                if sync_cut {
                    write_all_at(&mut file, PAYLOAD, 0).unwrap();
                    let result = file.sync_all();
                    assert_eq!(result.is_ok(), decision == wire::Decision::Continue);
                } else {
                    let result = file.write_at(PAYLOAD, 0);
                    match decision {
                        wire::Decision::Continue => assert_eq!(
                            result.unwrap(),
                            if boundary == B::WritePartial {
                                1
                            } else {
                                PAYLOAD.len()
                            }
                        ),
                        wire::Decision::ReturnError => assert!(result.is_err()),
                    }
                }
                let actual = fs::read(directory.path().join(stage)).unwrap();
                let expected =
                    if boundary == B::WriteBefore && decision == wire::Decision::ReturnError {
                        &[][..]
                    } else if boundary == B::WritePartial {
                        &PAYLOAD[..1]
                    } else {
                        PAYLOAD
                    };
                assert_eq!(actual, expected, "{target:?}/{boundary:?}/{decision:?}");
                let witness = controller.finish();
                check_witness(&witness, request, wire::Phase::Other, target, Some(stage));
                if sync_cut {
                    assert_eq!(
                        (
                            witness.offset,
                            witness.requested_bytes,
                            witness.completed_bytes
                        ),
                        (None, None, None)
                    );
                } else {
                    assert_eq!(
                        (witness.offset, witness.requested_bytes),
                        (Some(0), Some(PAYLOAD.len() as u64))
                    );
                    assert_eq!(
                        witness.completed_bytes,
                        Some(match boundary {
                            B::WriteBefore => 0,
                            B::WritePartial => 1,
                            _ => PAYLOAD.len() as u64,
                        })
                    );
                }
                receipt("write_sync", cases, &witness, decision, &actual);
                cases += 1;
            }
        }
        // A separate Continue cut exercises write_all_at's actual short-write
        // retry from byte one, rather than merely expecting full bytes.
        let directory = tempfile::tempdir().unwrap();
        let _scope = scope(60, wire::Phase::Other);
        let controller = Controller::new(target, B::WritePartial, wire::Decision::Continue);
        let mut writer = controller.writer(directory.path());
        let mut file = writer.create_new(&FileName::new(stage).unwrap()).unwrap();
        write_all_at(&mut file, PAYLOAD, 0).unwrap();
        let actual = fs::read(directory.path().join(stage)).unwrap();
        assert_eq!(actual, PAYLOAD);
        let witness = controller.finish();
        check_witness(&witness, 60, wire::Phase::Other, target, Some(stage));
        assert_eq!(witness.completed_bytes, Some(1));
        receipt(
            "partial_retry",
            usize::from(target == T::Doublewrite),
            &witness,
            wire::Decision::Continue,
            &actual,
        );
    }
    assert_eq!(cases, 20);
    let exact = [
        ("d3-filemap-select-stage", T::FileMap),
        ("FILEMAP", T::FileMap),
        ("d3-dwb-select-stage", T::Doublewrite),
        ("DWB", T::Doublewrite),
    ];
    let similar = [
        "d3-filemap-0",
        "d3-dwb-0",
        "d3-filemap-select-stage-extra",
        "DWB-extra",
    ];
    // Open-phase sync classification is observed through the real adapter for
    // all four exact names and all four similar names.
    for (case, (name, target)) in exact
        .into_iter()
        .chain(similar.map(|name| (name, T::RecoveryOldFile)))
        .enumerate()
    {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(name), PAYLOAD).unwrap();
        let _scope = scope(70 + case as u64, wire::Phase::Open);
        let controller = Controller::new(T::Any, B::SyncBefore, wire::Decision::Continue);
        let mut writer = controller.writer(directory.path());
        let mut file = writer
            .open_existing(&FileName::new(name).unwrap(), OpenMode::ReadWrite)
            .unwrap();
        file.sync_all().unwrap();
        let witness = controller.finish();
        check_witness(
            &witness,
            70 + case as u64,
            wire::Phase::Open,
            target,
            Some(name),
        );
        assert_eq!(fs::read(directory.path().join(name)).unwrap(), PAYLOAD);
        receipt(
            "open_sync_names",
            case,
            &witness,
            wire::Decision::Continue,
            PAYLOAD,
        );
    }
    for (case, (name, target)) in exact.into_iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(name), []).unwrap();
        let _scope = scope(80 + case as u64, wire::Phase::Open);
        let controller = Controller::new(T::Any, B::WriteAfter, wire::Decision::Continue);
        let mut writer = controller.writer(directory.path());
        let mut file = writer
            .open_existing(&FileName::new(name).unwrap(), OpenMode::ReadWrite)
            .unwrap();
        let payload = resembling_dev2_frame();
        write_all_at(&mut file, &payload, 0).unwrap();
        // Exact role still wins after a recognizable but unvalidated AXF1 body.
        assert_eq!(file.sync_target(), target);
        file.sync_all().unwrap();
        let witness = controller.finish();
        check_witness(
            &witness,
            80 + case as u64,
            wire::Phase::Open,
            target,
            Some(name),
        );
        assert_eq!(fs::read(directory.path().join(name)).unwrap(), payload);
        receipt(
            "exact_write_names",
            case,
            &witness,
            wire::Decision::Continue,
            &payload,
        );
    }
    for (case, name) in similar.into_iter().enumerate() {
        let directory = tempfile::tempdir().unwrap();
        let _scope = scope(90 + case as u64, wire::Phase::Open);
        let controller = Controller::new(T::Any, B::SyncBefore, wire::Decision::Continue);
        let mut writer = controller.writer(directory.path());
        let mut file = writer.create_new(&FileName::new(name).unwrap()).unwrap();
        write_all_at(&mut file, PAYLOAD, 0).unwrap();
        file.sync_all().unwrap();
        let witness = controller.finish();
        check_witness(
            &witness,
            90 + case as u64,
            wire::Phase::Open,
            T::Any,
            Some(name),
        );
        assert_eq!(fs::read(directory.path().join(name)).unwrap(), PAYLOAD);
        receipt(
            "new_similar_names",
            case,
            &witness,
            wire::Decision::Continue,
            PAYLOAD,
        );
    }
}

#[test]
fn dev3_selector_repeated_unknown_reopens_do_not_cross_targets() {
    let mut cases = 0;
    for (selected, target, stage) in SELECTORS {
        let directory = tempfile::tempdir().unwrap();
        seed_selected(directory.path());
        let (other, expected_other) = other_selected(selected);
        for (generation, boundary) in [B::RenameAfter, B::DirectorySyncBefore]
            .into_iter()
            .enumerate()
        {
            let payload =
                format!("{}-generation-{}", selected.as_str(), generation + 1).into_bytes();
            let request = 100 + cases * 10 + generation as u64;
            let controller = Controller::new(target, boundary, wire::Decision::ReturnError);
            {
                let _scope = scope(request, wire::Phase::Open);
                let mut writer = controller.writer(directory.path());
                let file = prepared(&mut writer, stage, &payload);
                let result = writer.publish_selector(&FileName::new(stage).unwrap(), selected);
                let error = if boundary == B::RenameAfter {
                    result.unwrap_err()
                } else {
                    result.unwrap();
                    writer.sync_directory().unwrap_err()
                };
                assert_eq!(error.effect, Effect::CallerMustResolve);
                drop((file, writer));
            }
            let witness = controller.finish();
            check_witness(&witness, request, wire::Phase::Open, target, Some(stage));
            // Release all old writer/file leases, acquire a new lease and read
            // the selected name, without claiming engine recovery ran.
            let port = LinuxDirectory::open_existing(directory.path()).unwrap();
            let mut reopened = port.acquire_writer().unwrap();
            let mut file = reopened
                .open_existing(
                    &FileName::new(selected.as_str()).unwrap(),
                    OpenMode::ReadOnly,
                )
                .unwrap();
            let mut actual = vec![0; payload.len()];
            read_exact_at(&mut file, &mut actual, 0).unwrap();
            assert_eq!(actual, payload);
            assert_eq!(
                fs::read(directory.path().join(other)).unwrap(),
                expected_other
            );
            assert!(!directory.path().join(stage).exists());
            receipt(
                "reopen",
                (cases * 2 + generation as u64) as usize,
                &witness,
                wire::Decision::ReturnError,
                &actual,
            );
            drop((file, reopened));
        }
        // A successfully renamed selector cannot lend its target to a later
        // request. Target::Any observes the next request's unclassified sync.
        let controller = Controller::new(T::Any, B::DirectorySyncBefore, wire::Decision::Continue);
        let mut writer = controller.writer(directory.path());
        {
            let _scope = scope(130 + cases * 2, wire::Phase::Other);
            let file = prepared(&mut writer, stage, b"third");
            writer
                .publish_selector(&FileName::new(stage).unwrap(), selected)
                .unwrap();
            drop(file);
        }
        {
            let _scope = scope(131 + cases * 2, wire::Phase::Other);
            writer.sync_directory().unwrap();
        }
        assert!(writer.lineage.is_none());
        let witness = controller.finish();
        check_witness(&witness, 131 + cases * 2, wire::Phase::Other, T::Any, None);
        assert_eq!(
            fs::read(directory.path().join(selected.as_str())).unwrap(),
            b"third"
        );
        assert_eq!(
            fs::read(directory.path().join(other)).unwrap(),
            expected_other
        );
        receipt(
            "cross_request",
            cases as usize,
            &witness,
            wire::Decision::Continue,
            b"third",
        );
        cases += 1;
    }
    assert_eq!(cases, 2);
}
