//! Interrupted publication and pre-effect content drift use named exact cuts.
use super::faults::{rejected, unchanged_error};
use super::literals::{Oracle, put, seal};
use super::observations::get;
use super::support::*;
use super::*;
use xadb_foundation::namespace::NamespaceErrorKind;

fn canonical_active() -> Images {
    let mut files = fresh_memory(4);
    let o = Oracle::new();
    let allocator = o.allocation(2).1;
    for (n, bytes) in [
        (0, allocator.as_slice()),
        (1, o.page("root-begin.page")),
        (2, o.page("jobs-begin.page")),
        (9, o.page("status-begin.page")),
    ] {
        files.get_mut(HOME_NAME).unwrap()[n * PAGE_BYTES..(n + 1) * PAGE_BYTES]
            .copy_from_slice(bytes);
    }
    files
}

fn sealed_genesis(files: &mut Images, wrong_digest: bool) {
    let mut current = vec![0; 4096];
    current[..4].copy_from_slice(b"XRC1");
    current[4..8].copy_from_slice(&[1, 0, 1, 0]);
    current[16..32].copy_from_slice(&files["d3-profile"][16..32]);
    for (at, value, width) in [
        (8, 4096, 4),
        (12, 256, 4),
        (32, 1, 8),
        (40, 1, 8),
        (48, 1, 8),
        (88, 1, 4),
        (224, 1, 8),
    ] {
        put(&mut current, at, value, width);
    }
    current[56..88].copy_from_slice(&content_digest(&files["d3-profile"]));
    let source = &files["d3-wal-00000001"];
    let mut closure = vec![0; 256];
    closure[..4].copy_from_slice(&[1, 2, 0, 1]);
    for (at, value, width) in [
        (4, 1, 4),
        (8, 1, 8),
        (24, 4096, 4),
        (96, 4096, 8),
        (104, source.len() as u64, 8),
        (112, if wrong_digest { 4 } else { 5 }, 8),
        (184, 1, 8),
    ] {
        put(&mut closure, at, value, width);
    }
    closure[32..64].copy_from_slice(&content_digest(&files["d3-bootstrap"]));
    closure[64..96].copy_from_slice(&content_digest(&source[..4096]));
    closure[120..152].copy_from_slice(&content_digest(source));
    closure[152..184].copy_from_slice(&content_digest(&files["WRITER"]));
    if wrong_digest {
        closure[120] ^= 1;
    }
    current[256..512].copy_from_slice(&closure);
    current[512..768].copy_from_slice(&closure);
    seal(&mut current);
    files.insert("CURRENT".into(), current);
}

#[test]
fn recovery_publication() {
    let ids: Vec<_> = (1..=10).map(|i| format!("P4-REC-E{i:02}")).collect();
    let names: Vec<_> = ids.iter().map(String::as_str).collect();
    let mut h = Harness::new(&names);
    for case in 1..=10 {
        h.case(&format!("P4-REC-E{case:02}"), || {
            if case == 10 {
                for wrong_digest in [true,false] {
                    let mut files=canonical_active(); sealed_genesis(&mut files,wrong_digest);
                    let d=clone_directory(&files); let state=d.state(); clear(&state);
                    rejected(&d,&state,|error,observation| {
                        unchanged_error(error,observation,Plan4RecoveryPhase::Analysis);
                        assert!(if wrong_digest {matches!(error.cause,Plan4RecoveryCause::ContentMismatch)} else {matches!(error.cause,Plan4RecoveryCause::OrderMismatch)},"wrong sealed primary cause: {error:?}");
                    });
                    assert_eq!(images(&state),files);
                    assert_eq!(state.lock().unwrap().mutations(),0);
                    println!("P4_RECOVERY_VARIANT\tP4-REC-E10\t{}",if wrong_digest {"digest"} else {"lsn"});
                }
                return;
            }
            let files = canonical_active();
            let d = clone_directory(&files); let state = d.state(); clear(&state);
            if case <= 7 {
                let (op, path, occurrence, action) = match case {
                    1 => ("write", "d3-rebase-0", 1, port::Action::BeforeError),
                    2 => ("publish", "d3-current-stage", 1, port::Action::AfterError),
                    3 => ("remove", "d3-wal-00000001", 1, port::Action::BeforeError),
                    4 => ("create", "d3-wal-00000002", 1, port::Action::BeforeError),
                    5 => ("write", "d3-wal-00000002", 1, port::Action::PrefixErrorBytes(17)),
                    6 => ("publish", "d3-writer-stage", 1, port::Action::AfterError),
                    // No home repair: source sync + seven CURRENT + two base
                    // syncs + retirement + epoch + WRITER = thirteen dirsyncs.
                    7 => ("dirsync", "", 13, port::Action::AfterError),
                    _ => unreachable!(),
                };
                state.lock().unwrap().fault = Some(port::Fault { key: port::Key::new(op, path, 0, occurrence), action });
            } else {
                let mut changed = files["d3-wal-00000001"].clone(); changed[4096 + 4000] ^= 1;
                state.lock().unwrap().hook = Some(port::Hook {
                    // E08 changes bytes just before the second complete hash.
                    // E09 changes an already consumed earlier frame while the
                    // final frame has yet to be consumed; the two later hashes
                    // would otherwise agree on the changed bytes.
                    key: port::Key::new("read", "d3-wal-00000001", if case == 8 { 0 } else { 82_968 }, if case == 8 { 4 } else { 1 }),
                    edit: port::Edit::Bytes("d3-wal-00000001".into(), changed),
                });
            }
            rejected(&d, &state, |error, observation| {
                let phase = match case { 1 | 2 => Plan4RecoveryPhase::Rebase, 3 => Plan4RecoveryPhase::Reclaim, 4..=7 => Plan4RecoveryPhase::LogOpen, _ => Plan4RecoveryPhase::Analysis };
                if case >= 8 {
                    unchanged_error(error, observation, phase);
                    assert!(matches!(error.cause, Plan4RecoveryCause::ContentMismatch));
                } else {
                    assert_eq!((error.phase, error.database_effect), (phase, Effect::CallerMustResolve));
                    assert_eq!(observation.mutations_attempted, observation.mutations_completed + 1);
                    assert!(match case { 1 | 5 => matches!(error.cause, Plan4RecoveryCause::Io(_)), _ => matches!(&error.cause, Plan4RecoveryCause::Namespace(e) if e.kind == NamespaceErrorKind::Io) });
                }
            });
            let actual = images(&state);
            let events = state.lock().unwrap().events.clone();
            if case >= 8 {
                assert_eq!(state.lock().unwrap().hook_hits, 1);
                assert_eq!(state.lock().unwrap().mutations(), 0);
                assert_eq!(actual.len(), files.len());
                for (name, bytes) in &files { if name != "d3-wal-00000001" { assert_eq!(&actual[name], bytes); } }
                assert_eq!(actual["d3-wal-00000001"].len(), files["d3-wal-00000001"].len());
                return;
            }
            assert_eq!(state.lock().unwrap().fault_hits, 1);
            assert!(!events.iter().any(|e| e.op == "write" && e.name == HOME_NAME));
            if case <= 3 { assert_eq!(actual["d3-wal-00000001"], files["d3-wal-00000001"]); }
            if case == 1 { assert!(actual["d3-rebase-0"].is_empty()); }
            if case <= 2 { assert_eq!(actual["CURRENT"][256..260], [1, 2, 0, 1]); }
            if case == 3 {
                assert_eq!(actual[BASE_NAMES[0]], actual[BASE_NAMES[1]]);
                assert_eq!((actual["CURRENT"][257], actual["CURRENT"][513]), (0, 0));
            }
            if matches!(case, 4..=6) {
                let current = &actual["CURRENT"];
                assert_eq!((get(current, 88, 4), current[92], get(current, 96, 4)), (2, 1, 2));
                assert_eq!((current[257], current[513]), (0, 0));
            }
            if case == 4 {
                assert!(!actual.contains_key("d3-wal-00000002"));
                // A distinct explicit retry consumes the durable burned intent
                // and must choose epoch3, never reuse epoch2.
                let next = clone_directory(&actual); let next_state = next.state(); clear(&next_state);
                let mut maintenance = boxed::<12_582_912>(); let mut dwb = boxed::<4_194_304>(); let mut slot = boxed::<1_048_576>();
                { let runtime = actual_runtime(&next, &mut maintenance, &mut dwb, &mut slot); assert_eq!(runtime.selected.header.epoch, 3); }
                assert!(!images(&next_state).contains_key("d3-wal-00000002"));
                assert_eq!(file(&next_state, "d3-wal-00000003").len(), 4096); assert_closed(&next_state);
            }
            if case == 5 {
                let event = events.iter().find(|e| e.op == "write" && e.name == "d3-wal-00000002").unwrap();
                assert_eq!((event.requested, event.error_prefix, event.returned), (4096, 17, None));
                assert_eq!(actual["d3-wal-00000002"].len(), 17);
                assert_eq!(actual["WRITER"], files["WRITER"]);
            }
            if case == 6 { assert_eq!(&actual["WRITER"][..4], b"XWS2"); }
            if case == 7 {
                assert_eq!((actual["CURRENT"][257], actual["CURRENT"][513], actual["CURRENT"][92]), (1, 1, 0));
                assert_eq!(events.iter().filter(|e| e.op == "dirsync").count(), 13);
            }
        });
    }
    h.finish();
}
