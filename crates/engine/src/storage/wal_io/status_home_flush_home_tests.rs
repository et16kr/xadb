//! New Status home predicates and actual DWB fault consumers. All expected
//! persistent bytes come from the independent Status-flush literal inventory.
use super::*;
use crate::storage::control::{ControlError, ControlKind, ControlRef, SelectedControl};
use xadb_foundation::error::{Certainty, ErrorCode};

const WRITES: [usize; 11] = [1, 3, 6, 10, 12, 14, 17, 21, 23, 25, 28];
const FAULTS: &str =
    include_str!("../../../../../tests/fixtures/dev3-status-home-flush/faults.tsv");

fn selection(bytes: &[u8], kind: ControlKind) -> SelectedControl {
    assert_eq!(bytes.len(), 4096);
    assert_eq!(bytes[80], kind as u8);
    let u64_at = |at| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    SelectedControl {
        generation: u64_at(32),
        reference: ControlRef {
            kind,
            slot: bytes[81],
            index: u32_at(84),
            generation: u64_at(88),
            encoded_len: u32_at(96),
            digest: bytes[112..144].try_into().unwrap(),
        },
    }
}
fn dwb_selection(key: &str, phase: &str) -> SelectedControl {
    selection(
        &literal(&format!("{key}/{phase}")),
        ControlKind::Doublewrite,
    )
}
fn same_ram(
    actual: &(
        Box<[u8; PAGE_BYTES]>,
        Box<[u8; PAGE_BYTES]>,
        init_test::InitSnapshot,
    ),
    before: &(
        Box<[u8; PAGE_BYTES]>,
        Box<[u8; PAGE_BYTES]>,
        init_test::InitSnapshot,
    ),
) {
    assert_eq!(actual.0, before.0);
    assert_eq!(actual.1, before.1);
    let (a, b) = (&actual.2, &before.2);
    assert_eq!(a.published, b.published);
    assert_eq!(a.claim_payload, b.claim_payload);
    assert_eq!(a.claim_birth, b.claim_birth);
    assert_eq!(a.initializer_pins_held, b.initializer_pins_held);
    assert_eq!(a.pins, b.pins);
    assert_eq!(a.total, b.total);
    assert_eq!(a.page_lsns, b.page_lsns);
    assert_eq!(a.counters, b.counters);
    assert_eq!(a.rec_lsns, b.rec_lsns);
    assert_eq!(a.dirty, b.dirty);
    assert_eq!(a.entries, b.entries);
    assert_eq!(a.requirements, b.requirements);
}
fn original_io(error: StatusFlushError) -> flush_io::FlushIoError {
    assert_eq!(error.effect, Effect::CallerMustResolve);
    let Cause::Io(inner) = error.cause else {
        panic!("expected original I/O cause: {error:?}")
    };
    assert_eq!(inner.effect, Effect::CallerMustResolve);
    inner
}
fn assert_prepared_failure(
    flusher: &StatusFlusher<'_, '_, '_, '_, '_, '_, '_, port::Writer>,
    state: &State,
    original: &(
        Box<[u8; PAGE_BYTES]>,
        Box<[u8; PAGE_BYTES]>,
        init_test::InitSnapshot,
    ),
    files: &Inventory,
) {
    assert!(flusher.snapshot().fenced);
    assert_eq!(
        (
            flusher.snapshot().io.attempted,
            flusher.snapshot().io.completed
        ),
        (0, 0)
    );
    assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
    assert!(flusher.control.capture.is_some() && flusher.control.authority.is_some());
    assert!(flusher.control.retired.is_none());
    same_ram(&diagnose(flusher.pool), original);
    assert_eq!(state.lock().unwrap().events.len(), 0);
    assert!(state.lock().unwrap().reads > 0);
    assert_eq!(inventory(state), *files);
}
fn home_case(logical: &str, accepted: bool, mismatch: bool) {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        let home = literal(&format!("homes/{logical}"));
        assert_eq!(home.len(), PAGE_BYTES);
        state.lock().unwrap().files.get_mut(HOME).unwrap().bytes[PAGE_BYTES..2 * PAGE_BYTES]
            .copy_from_slice(&home);
        let before = diagnose(flusher.pool);
        let files = inventory(&state);
        reset(&state);
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        if accepted {
            let result = done(ok(flusher.try_flush_status(&mut dedicated)));
            assert!(result.finalized.cleaned);
            assert_eq!(
                (
                    result.io.attempted,
                    result.io.completed,
                    result.io.last_issued
                ),
                (31, 31, 1)
            );
            assert!(result.io.homes_synced && result.io.retirement_selected);
            let after = diagnose(flusher.pool);
            assert_eq!(after.0, before.0);
            assert_eq!(after.1, before.1);
            assert_eq!(after.2.dirty, [true, false]);
            assert_eq!(after.2.counters, [2, 1]);
            assert_eq!(after.2.page_lsns, [PageLsn(2); 2]);
            assert_eq!(after.2.rec_lsns, [Some(PageLsn(1)), None]);
            assert!(after.2.entries[1].is_none() && after.2.requirements[1].is_none());
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_none());
            full_state(&state, "p1-c0-SA-b1/after", &files);
            assert_eq!(state.lock().unwrap().events.len(), 31);
        } else {
            let error = original_io(err(flusher.try_flush_status(&mut dedicated)));
            assert_eq!(error.phase, flush_io::Phase::Prepare);
            if mismatch {
                assert!(matches!(error.cause, flush_io::Cause::ContentMismatch));
            } else {
                assert!(matches!(
                    error.cause,
                    flush_io::Cause::Status(SemanticError::PageMismatch)
                ));
            }
            assert_prepared_failure(&flusher, &state, &before, &files);
        }
    });
}

// This is deliberately a mechanical corrupted-trusted-plan input. The fresh
// reader's original plan/disk produce the real bound and INIT. Only thereafter
// the actual FILEMAP files become the independently encoded synthetic tuple.
// No mutable access to a borrowed owner or production raw setter is used.
fn mapping_case(which: &str) {
    let mut slot = port::slot();
    let mut workspace = boxed::<{ 4 << 20 }>(0);
    let (namespace, state) = port::fresh();
    let fresh =
        FreshStorageSession::create_status(namespace, 1, &mut slot, &mut [[0; PAGE_BYTES]; 3])
            .unwrap();
    let installed = fresh.install_doublewrite(&mut workspace).unwrap();
    let mut storage = installed.into_mutable_storage();
    let selector = literal(&format!("filemap/{which}/selector"));
    storage.plan.filemap = selection(&selector, ControlKind::FileMapRoot);
    {
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut allocator = boxed::<PAGE_BYTES>(0x2d);
        let mut payload = boxed::<PAGE_BYTES>(0xa5);
        let mut operation = boxed::<INIT_OPERATION_BYTES>(0x73);
        let mut frames = [
            DirtyFrame::new(&mut allocator),
            DirtyFrame::new(&mut payload),
        ];
        let original = ok(storage.bind_status_bootstrap(&core, &mut frames, &mut operation));
        let mut bound = converted(original.into_with_flush());
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        {
            let mut s = state.lock().unwrap();
            s.files.get_mut("FILEMAP").unwrap().bytes = selector;
            let container = literal(&format!("filemap/{which}/container"));
            let entry = s.files.get_mut("d3-filemap-0").unwrap();
            assert_eq!(entry.length as usize, container.len());
            entry.bytes = container;
        }
        let before = diagnose(flusher.pool);
        let files = inventory(&state);
        reset(&state);
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        let error = original_io(err(flusher.try_flush_status(&mut dedicated)));
        assert_eq!(error.phase, flush_io::Phase::Prepare);
        match which {
            "not-live" => assert!(matches!(error.cause, flush_io::Cause::IdentityMismatch)),
            "wrong-extent" | "short-mapped" => assert!(matches!(
                error.cause,
                flush_io::Cause::Control(ControlError::Corrupt)
            )),
            "missing" => assert!(matches!(error.cause, flush_io::Cause::MissingMapping)),
            _ => panic!("unknown FILEMAP input"),
        }
        assert_prepared_failure(&flusher, &state, &before, &files);
    }
    drop(storage);
    let s = state.lock().unwrap();
    assert_eq!((s.handles, s.writers), (0, 0));
}
fn selection_mismatch() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        state
            .lock()
            .unwrap()
            .files
            .get_mut("FILEMAP")
            .unwrap()
            .bytes = literal("filemap/missing/selector");
        let original = diagnose(flusher.pool);
        let files = inventory(&state);
        reset(&state);
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        let error = original_io(err(flusher.try_flush_status(&mut dedicated)));
        assert_eq!(error.phase, flush_io::Phase::Prepare);
        assert!(matches!(error.cause, flush_io::Cause::SelectionMismatch));
        assert_prepared_failure(&flusher, &state, &original, &files);
        assert!(
            !state
                .lock()
                .unwrap()
                .read_events
                .iter()
                .any(|event| event.name == "d3-filemap-0")
        );
    });
}
fn pause(state: &State, at: usize, after: bool) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (entered, ack) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    state.lock().unwrap().pause = Some(port::Pause {
        at,
        after,
        entered,
        release,
        unwind: false,
    });
    (ack, resume)
}
fn replaced_home() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        let before = diagnose(flusher.pool);
        let files = inventory(&state);
        let original_identity = state.lock().unwrap().files[HOME].identity;
        reset(&state);
        let (ack, resume) = pause(&state, 19, true);
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        let injection_state = Arc::clone(&state);
        std::thread::scope(|scope| {
            let injector = scope.spawn(move || {
                ack.recv_timeout(STEP).unwrap();
                let mut replacement = injection_state.lock().unwrap().files[HOME].clone();
                replacement.identity.inode += 100;
                injection_state
                    .lock()
                    .unwrap()
                    .files
                    .insert(HOME.into(), replacement);
                resume.send(()).unwrap();
            });
            let error = original_io(err(flusher.try_flush_status(&mut dedicated)));
            assert!(matches!(error.cause, flush_io::Cause::IdentityMismatch));
            assert_eq!(error.phase, phase(20));
            injector.join().unwrap();
        });
        let observed = flusher.snapshot();
        assert!(observed.fenced);
        assert_eq!((observed.io.attempted, observed.io.completed), (20, 20));
        assert_eq!(
            observed.io.known,
            dwb_selection("p1-c0-SA-b1", "pending.selector")
        );
        assert_eq!(observed.io.possible, [Some(observed.io.known), None]);
        assert_eq!(observed.io.last_issued, 1);
        assert!(!observed.io.homes_synced && !observed.io.retirement_selected);
        assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
        same_ram(&diagnose(flusher.pool), &before);
        full_state(&state, "SF-P1-SA-BEFORE-21", &files);
        let s = state.lock().unwrap();
        assert_ne!(s.files[HOME].identity, original_identity);
        assert_eq!(s.events.len(), 20);
        assert_eq!(
            s.read_events
                .iter()
                .filter(|event| event.kind == "open" && event.name == HOME)
                .count(),
            2
        );
        assert!(!s.events.iter().any(|event| event.name == HOME));
    });
}

pub(super) fn run_bh() {
    let mut count = 0;
    for (id, logical, accepted, mismatch) in [
        ("BH01", "BH01-lower-lsn", false, true),
        ("BH02", "BH02-exact", true, false),
        ("BH03", "BH03-free-generation2", false, true),
        ("BH04", "BH04-higher-lsn", false, true),
    ] {
        if selected(id) {
            home_case(logical, accepted, mismatch);
            passed(id, &mut count);
        }
    }
    for (id, variants) in [
        ("BH05", &["lower-incarnation", "higher-incarnation"][..]),
        ("BH06", &["object", "owner-generation", "kind", "level"][..]),
        ("BH07", &["half", "nonzero", "bad-checksum"][..]),
    ] {
        if selected(id) {
            for name in variants {
                home_case(&format!("{id}-{name}"), false, false);
                variant(id, name);
            }
            passed(id, &mut count);
        }
    }
    if selected("BH08") {
        for name in ["not-live", "wrong-extent", "short-mapped"] {
            mapping_case(name);
            variant("BH08", name);
        }
        passed("BH08", &mut count);
    }
    if selected("BH09") {
        selection_mismatch();
        passed("BH09", &mut count);
    }
    if selected("BH10") {
        replaced_home();
        passed("BH10", &mut count);
    }
    complete("BH", count);
}

fn phase(cut: usize) -> flush_io::Phase {
    use flush_io::{Phase as P, SelectionStep as S};
    match cut {
        10 => P::ImageWrite,
        11 => P::ImageSync,
        21 => P::HomeWrite,
        22 => P::HomeSync,
        _ => {
            let (index, local) = if cut <= 9 {
                (0, cut - 1)
            } else if cut <= 20 {
                (1, cut - 12)
            } else {
                (2, cut - 23)
            };
            P::Selection {
                index,
                step: [
                    S::StageWrite,
                    S::StageSync,
                    S::BankWrite,
                    S::BankSync,
                    S::SelectorCreate,
                    S::SelectorWrite,
                    S::SelectorSync,
                    S::Publish,
                    S::DirectorySync,
                ][local],
            }
        }
    }
}
fn lower_fault(error: StatusFlushError, cut: usize, mode: &str) {
    let inner = original_io(error);
    assert_eq!(inner.phase, phase(cut));
    match inner.cause {
        flush_io::Cause::Io(e) => {
            assert_eq!(e.code, ErrorCode::IoWriteUnknown);
            assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
            assert_eq!(e.effect(), Effect::CallerMustResolve);
            assert_eq!(e.certainty(), Certainty::Unknown);
            assert_eq!(
                e.context,
                if WRITES.contains(&cut) {
                    "write outcome requires caller resolution"
                } else {
                    "sync outcome unknown"
                }
            );
        }
        flush_io::Cause::Namespace(e) => {
            assert_eq!(e.kind, xadb_foundation::namespace::NamespaceErrorKind::Io);
            assert_eq!(e.context, "fixed WAL namespace fault");
            assert_eq!(e.io_kind, Some(std::io::ErrorKind::Other));
            let unknown = mode == "AFTER" || matches!(cut, 9 | 20 | 31);
            assert_eq!(
                e.certainty(),
                if unknown {
                    Certainty::Unknown
                } else {
                    Certainty::Known
                }
            );
            assert_eq!(
                e.effect,
                if unknown {
                    Effect::CallerMustResolve
                } else {
                    Effect::Unchanged
                }
            );
        }
        other => panic!("lost original lower cause {other:?}"),
    }
}

fn effect_trace(state: &State, input: &str, cut: usize, mode: &str) {
    let success = matches!(mode, "SHORT" | "INTERRUPTED");
    let state = state.lock().unwrap();
    let mut cursor = 0;
    for ordinal in 1..=if success { 31 } else { cut } {
        let selection = if ordinal <= 9 {
            "issuance"
        } else if ordinal <= 20 {
            "pending"
        } else {
            "retirement"
        };
        let selected = dwb_selection(input, &format!("{selection}.selector"));
        let (kind, name, offset, data) = match ordinal {
            1 | 12 | 23 => (
                "write",
                "d3-dwb-stage".into(),
                0,
                literal(&format!("{input}/{selection}.bank")),
            ),
            2 | 13 | 24 => ("sync", "d3-dwb-stage".into(), 0, Vec::new()),
            3 | 14 | 25 => (
                "write",
                format!("d3-dwb-{}", selected.reference.slot),
                0,
                literal(&format!("{input}/{selection}.bank")),
            ),
            4 | 15 | 26 => (
                "sync",
                format!("d3-dwb-{}", selected.reference.slot),
                0,
                Vec::new(),
            ),
            5 | 16 | 27 => ("create", "d3-dwb-select-stage".into(), 0, Vec::new()),
            6 | 17 | 28 => (
                "write",
                "d3-dwb-select-stage".into(),
                0,
                literal(&format!("{input}/{selection}.selector")),
            ),
            7 | 18 | 29 => ("sync", "d3-dwb-select-stage".into(), 0, Vec::new()),
            8 | 19 | 30 => ("publish", "d3-dwb-select-stage".into(), 0, Vec::new()),
            9 | 20 | 31 => ("dirsync", String::new(), 0, Vec::new()),
            10 => (
                "write",
                "d3-dwb-images".into(),
                0,
                literal(&format!("{input}/slot")),
            ),
            11 => ("sync", "d3-dwb-images".into(), 0, Vec::new()),
            21 => (
                "write",
                HOME.into(),
                PAGE_BYTES as u64,
                literal(&format!("{input}/capture")),
            ),
            22 => ("sync", HOME.into(), 0, Vec::new()),
            _ => unreachable!(),
        };
        let event = &state.events[cursor];
        assert_eq!(
            (event.kind, &event.name, event.offset, event.length),
            (kind, &name, offset, data.len()),
            "effect {ordinal}"
        );
        let is_injected = ordinal == cut;
        let before_failure = is_injected && mode == "BEFORE";
        assert_eq!(event.data, if before_failure { &[][..] } else { &data[..] });
        if kind == "write" {
            let returned = if is_injected {
                match mode {
                    "SHORT" => Some(1),
                    "BEFORE" | "AFTER" | "PREFIX" | "INTERRUPTED" => None,
                    _ => unreachable!(),
                }
            } else {
                Some(data.len())
            };
            assert_eq!(event.returned, returned);
        } else {
            assert_eq!(event.returned, None);
        }
        cursor += 1;
        if is_injected && success {
            let consumed = usize::from(mode == "SHORT");
            let retry = &state.events[cursor];
            assert_eq!(
                (retry.kind, &retry.name, retry.offset, retry.length),
                (
                    "write",
                    &name,
                    offset + consumed as u64,
                    data.len() - consumed
                )
            );
            assert_eq!(retry.data, data[consumed..]);
            assert_eq!(retry.returned, Some(data.len() - consumed));
            cursor += 1;
        }
    }
    assert_eq!(cursor, state.events.len());
}

fn fault_case(profile: u32, order: &str, mode: &str, cut: usize, key: &str) {
    assert!((1..=31).contains(&cut));
    let succeeds = matches!(mode, "SHORT" | "INTERRUPTED");
    fixture!(profile, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        let untouched = inventory(&state);
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        if order == "AS" {
            let done = done(ok(flusher.try_flush_allocator(&mut dedicated)));
            assert!(done.finalized.cleaned);
            full_state(&state, &format!("p{profile}-c0-AS-b1/after"), &untouched);
        } else {
            assert_eq!(order, "SA");
        }
        let before = diagnose(flusher.pool);
        reset(&state);
        let action = match mode {
            "BEFORE" => port::Action::BeforeError,
            "AFTER" => port::Action::AfterError,
            "PREFIX" => port::Action::PrefixError,
            "SHORT" => port::Action::Short(1),
            "INTERRUPTED" => port::Action::Interrupted,
            _ => panic!("unknown fault"),
        };
        if matches!(mode, "PREFIX" | "SHORT" | "INTERRUPTED") {
            assert!(WRITES.contains(&cut));
        }
        state.lock().unwrap().plan = Some(port::Plan {
            at: cut - 1,
            action,
        });
        let batch = if order == "AS" { 2 } else { 1 };
        let input = format!("p{profile}-c0-{order}-b{batch}");
        if succeeds {
            let result = done(ok(flusher.try_flush_status(&mut dedicated)));
            assert!(result.finalized.cleaned);
            assert_eq!((result.io.attempted, result.io.completed), (31, 31));
            assert_eq!(result.io.last_issued, batch);
            assert_eq!(
                result.io.known,
                dwb_selection(&input, "retirement.selector")
            );
            assert_eq!(result.io.possible, [Some(result.io.known), None]);
            assert!(result.io.homes_synced && result.io.retirement_selected);
            {
                let s = state.lock().unwrap();
                assert_eq!((s.hit, s.events.len()), (1, 32));
                let first = &s.events[cut - 1];
                let retry = &s.events[cut];
                assert_eq!((first.kind, retry.kind), ("write", "write"));
                assert_eq!(first.name, retry.name);
                if mode == "SHORT" {
                    assert_eq!(first.returned, Some(1));
                    assert_eq!(retry.offset, first.offset + 1);
                    assert_eq!(retry.data, first.data[1..]);
                } else {
                    assert_eq!(first.returned, None);
                    assert_eq!(retry.offset, first.offset);
                    assert_eq!(retry.data, first.data);
                }
            }
            effect_trace(&state, &input, cut, mode);
            full_state(&state, &format!("{input}/after"), &untouched);
            if order == "SA" {
                reset(&state);
                let result = done(ok(flusher.try_flush_allocator(&mut dedicated)));
                assert!(result.finalized.cleaned);
                assert_eq!((result.io.last_issued, result.io.known.generation), (2, 7));
                assert_eq!(state.lock().unwrap().events.len(), 31);
            }
            let after = diagnose(flusher.pool);
            assert_eq!(after.0, before.0);
            assert_eq!(after.1, before.1);
            assert_eq!(after.2.dirty, [false; 2]);
            assert_eq!(after.2.entries, [None; 2]);
            assert_eq!(after.2.requirements, [None; 2]);
            assert_eq!(after.2.rec_lsns, [None; 2]);
            assert_eq!(after.2.page_lsns, [PageLsn(2); 2]);
            assert_eq!(after.2.counters, [2, 1]);
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_none());
        } else {
            lower_fault(err(flusher.try_flush_status(&mut dedicated)), cut, mode);
            let observed = flusher.snapshot();
            assert!(observed.fenced);
            assert_eq!(
                (
                    observed.io.attempted as usize,
                    observed.io.completed as usize
                ),
                (cut, cut - 1)
            );
            let phase_name = if cut <= 9 {
                "before/DWB"
            } else if cut <= 20 {
                "issuance.selector"
            } else {
                "pending.selector"
            };
            let known = dwb_selection(&input, phase_name);
            assert_eq!(observed.io.known, known);
            let next = if cut <= 9 {
                "issuance.selector"
            } else if cut <= 20 {
                "pending.selector"
            } else {
                "retirement.selector"
            };
            let ambiguous = matches!(cut, 8 | 9 | 19 | 20 | 30 | 31);
            assert_eq!(
                observed.io.possible,
                [Some(known), ambiguous.then(|| dwb_selection(&input, next))]
            );
            assert_eq!(observed.io.last_issued, batch - 1 + u64::from(cut > 9));
            assert_eq!(observed.io.homes_synced, cut > 22);
            assert!(!observed.io.retirement_selected);
            assert!(frame_test::flush_diagnose(flusher.pool).claim.is_some());
            assert!(flusher.control.capture.is_some() && flusher.control.authority.is_some());
            assert!(flusher.control.retired.is_none());
            same_ram(&diagnose(flusher.pool), &before);
            effect_trace(&state, &input, cut, mode);
            let s = state.lock().unwrap();
            assert_eq!((s.hit, s.events.len()), (1, cut));
            drop(s);
        }
        full_state(&state, key, &untouched);
        if !succeeds {
            let files = inventory(&state);
            let counts = {
                let s = state.lock().unwrap();
                (s.events.len(), s.reads)
            };
            assert_eq!(
                err(flusher.continue_flush(&mut dedicated)).effect,
                Effect::CallerMustResolve
            );
            assert_eq!(inventory(&state), files);
            let s = state.lock().unwrap();
            assert_eq!((s.events.len(), s.reads), counts);
        }
    });
}
fn poison_after_retirement() {
    fixture!(1, |bound, core, state| {
        let (mut writer, _, mut flusher) = bound.split_with_flush();
        let mut owner = ok(core.try_owner(OwnerRole::Maintenance));
        initialize(&mut writer, &mut owner, 0);
        let before = diagnose(flusher.pool);
        let files = inventory(&state);
        reset(&state);
        let (ack, resume) = pause(&state, 30, true);
        let pool = writer.pool;
        let mut dedicated = ok(core.try_owner(OwnerRole::DedicatedFlusher));
        std::thread::scope(|scope| {
            let injector = scope.spawn(move || {
                ack.recv_timeout(STEP).unwrap();
                frame_test::poison_dpt(pool);
                resume.send(()).unwrap();
            });
            let error = err(flusher.try_flush_status(&mut dedicated));
            assert_eq!(error.effect, Effect::CallerMustResolve);
            let Cause::Frame(inner) = error.cause else {
                panic!("expected registry poison {error:?}")
            };
            assert_eq!(
                (inner.cause, inner.effect),
                (dirty_frame::Cause::Poisoned, Effect::CallerMustResolve)
            );
            injector.join().unwrap();
        });
        let observed = flusher.snapshot();
        assert!(observed.fenced && observed.io.homes_synced && observed.io.retirement_selected);
        assert_eq!((observed.io.attempted, observed.io.completed), (31, 31));
        assert_eq!(
            observed.io.known,
            dwb_selection("p1-c0-SA-b1", "retirement.selector")
        );
        assert_eq!(observed.io.possible, [Some(observed.io.known), None]);
        assert_eq!(observed.io.last_issued, 1);
        assert!(frame_test::flush_diagnose(pool).claim.is_some());
        assert!(flusher.control.retired.is_some());
        same_ram(&diagnose(pool), &before);
        full_state(&state, "p1-c0-SA-b1/after", &files);
        assert_eq!(state.lock().unwrap().events.len(), 31);
        let after = inventory(&state);
        reset(&state);
        assert_eq!(
            err(flusher.continue_flush(&mut dedicated)).effect,
            Effect::CallerMustResolve
        );
        no_io(&state);
        assert_eq!(inventory(&state), after);
    });
}

pub(super) fn run_pf() {
    let mut count = 0;
    for (id, order, mode, cut) in [
        ("PF01", "SA", "PREFIX", 10),
        ("PF02", "SA", "AFTER", 19),
        ("PF03", "SA", "PREFIX", 21),
        ("PF04", "SA", "AFTER", 22),
        ("PF05", "SA", "AFTER", 31),
    ] {
        if selected(id) {
            fault_case(
                1,
                order,
                mode,
                cut,
                &format!("SF-P1-{order}-{mode}-{cut:02}"),
            );
            passed(id, &mut count);
        }
    }
    if selected("PF06") {
        poison_after_retirement();
        passed("PF06", &mut count);
    }
    if selected("PF07") {
        mapping_case("missing");
        passed("PF07", &mut count);
    }
    if selected("PF08") {
        fault_case(1, "AS", "PREFIX", 21, "SF-P1-AS-PREFIX-21");
        passed("PF08", &mut count);
    }
    complete("PF", count);
}

pub(super) fn run_s5_faults() {
    let mut count = 0;
    let mut inventory_count = 0;
    let mut aliases = 0;
    for row in FAULTS.lines().skip(1) {
        let f: Vec<_> = row.split('\t').collect();
        assert_eq!(f.len(), 11);
        inventory_count += 1;
        if f[9] == "S5_ALIAS" {
            aliases += 1;
            assert_ne!(f[8], "-");
            continue;
        }
        assert_eq!(f[9], "S5_EXTRA");
        assert_eq!(f[8], "-");
        if selected(f[0]) {
            fault_case(
                f[1].parse().unwrap(),
                f[2],
                f[4],
                f[5].parse().unwrap(),
                f[7],
            );
            passed(f[0], &mut count);
        }
    }
    assert_eq!((inventory_count, aliases), (570, 6));
    complete("SF", count);
}
