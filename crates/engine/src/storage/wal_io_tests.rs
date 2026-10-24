use super::{test_support::*, *};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, mpsc::sync_channel},
    time::Duration,
};

fn session(
    memory: &mut [u8; MAX_FRAME_BYTES],
) -> (GenesisWalSession<'_, Writer>, Arc<Mutex<State>>) {
    let (writer, state) = fresh();
    let session = GenesisWalSession::create(writer, 1, memory, &mut [0; 16384]).unwrap();
    (session, state)
}
fn profile() -> Profile {
    control::decode_bootstrap_profile(PROFILE, Some(namespace())).unwrap()
}
fn bytes(state: &Arc<Mutex<State>>, name: &str) -> Vec<u8> {
    state.lock().unwrap().files[name].bytes.clone()
}
fn inventory(state: &Arc<Mutex<State>>) -> Vec<(String, u64, FileIdentity, Vec<u8>)> {
    state
        .lock()
        .unwrap()
        .files
        .iter()
        .map(|(n, e)| (n.clone(), e.length, e.identity, e.bytes.clone()))
        .collect()
}
fn reject(
    session: &GenesisWalSession<'_, Writer>,
    state: &Arc<Mutex<State>>,
    frame: &[u8],
    expected: impl FnOnce(&Cause) -> bool,
) {
    let prefix = session.prefix().unwrap();
    let before = inventory(state);
    let events = state.lock().unwrap().events.len();
    let slot = session
        .runtime
        .append
        .lock()
        .unwrap()
        .slot
        .as_ref()
        .unwrap()
        .to_vec();
    let error = session.try_stage(frame).unwrap_err();
    assert_eq!(error.effect, Effect::Unchanged);
    assert!(
        expected(&error.cause),
        "unexpected cause: {:?}",
        error.cause
    );
    assert_eq!(session.prefix().unwrap(), prefix);
    assert_eq!(inventory(state), before);
    assert_eq!(state.lock().unwrap().events.len(), events);
    assert_eq!(
        session
            .runtime
            .append
            .lock()
            .unwrap()
            .slot
            .as_ref()
            .unwrap()
            .as_slice(),
        slot
    );
}
fn assert_fenced(session: &GenesisWalSession<'_, Writer>, state: &Arc<Mutex<State>>, synced: u64) {
    assert!(session.is_fenced());
    let before = inventory(state);
    let calls = state.lock().unwrap().events.len();
    for error in [
        session.prefix().unwrap_err(),
        session.try_stage(&raw_frame(1, 2, &[])).unwrap_err(),
        session.sync_pending().unwrap_err(),
    ] {
        assert_eq!(error.effect, Effect::CallerMustResolve);
        assert!(matches!(error.cause, Cause::Fenced));
    }
    assert_eq!(calls, state.lock().unwrap().events.len());
    assert_eq!(before, inventory(state));
    let guard = session
        .runtime
        .append
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    assert_eq!(guard.prefix.synced_offset, synced);
}

#[test]
fn genesis_codec_literals_and_damage() {
    let profile = profile();
    let digest = content_digest(EPOCH);
    let mut output = [0xa5; 4101];
    assert_eq!(encode_epoch(profile, &mut output).unwrap(), EPOCH);
    assert_eq!(&output[4096..], &[0xa5; 5]);
    output.fill(0xa5);
    assert_eq!(
        encode_writer(profile, EPOCH, &mut output).unwrap(),
        SELECTED
    );
    assert_eq!(&output[4096..], &[0xa5; 5]);
    let decode = |writer: bool, value: &[u8]| {
        if writer {
            decode_writer(value, profile, digest)
        } else {
            decode_epoch(value, profile).map(|_| ())
        }
    };
    let mut prefixes = 0;
    let mut damaged = 0;
    let mut semantic = 0;
    let mut reserved = 0;
    for (writer, literal) in [(false, EPOCH), (true, SELECTED)] {
        for end in 0..=4096 {
            assert_eq!(decode(writer, &literal[..end]).is_ok(), end == 4096);
            prefixes += 1;
        }
        let mut trailing = literal.to_vec();
        trailing.push(0);
        assert_eq!(decode(writer, &trailing), Err(ControlError::Corrupt));
        for offset in 0..4096 {
            let mut value = literal.to_vec();
            value[offset] ^= 1;
            assert!(decode(writer, &value).is_err(), "bitflip {writer}/{offset}");
            damaged += 1;
        }
        use ControlError::{Corrupt as C, IdentityMismatch as I, UnsupportedFormat as U};
        let mut cases = vec![
            (0, vec![literal[0] ^ 1], C),
            (4, 2u16.to_le_bytes().to_vec(), U),
            (6, vec![2], U),
            (8, 4095u32.to_le_bytes().to_vec(), C),
            (16, vec![literal[16] ^ 1], I),
            (32, 2u64.to_le_bytes().to_vec(), I),
        ];
        cases.extend(if writer {
            vec![
                (40, 2u64.to_le_bytes().to_vec(), I),
                (48, 2u32.to_le_bytes().to_vec(), I),
                (56, 2u64.to_le_bytes().to_vec(), I),
                (64, vec![literal[64] ^ 1], I),
                (96, vec![literal[96] ^ 1], I),
                (128, 1u64.to_le_bytes().to_vec(), C),
                (136, 4095u64.to_le_bytes().to_vec(), C),
            ]
        } else {
            vec![
                (40, 2u32.to_le_bytes().to_vec(), I),
                (48, 2u64.to_le_bytes().to_vec(), I),
                (56, vec![literal[56] ^ 1], I),
                (88, 1u64.to_le_bytes().to_vec(), C),
                (96, 4095u64.to_le_bytes().to_vec(), C),
                (104, 1048575u32.to_le_bytes().to_vec(), C),
                (112, 268435455u64.to_le_bytes().to_vec(), C),
            ]
        });
        for (offset, replacement, expected) in cases {
            let mut value = literal.to_vec();
            value[offset..offset + replacement.len()].copy_from_slice(&replacement);
            seal_control(&mut value);
            assert_eq!(
                decode(writer, &value),
                Err(expected),
                "semantic {writer}/{offset}"
            );
            semantic += 1;
        }
        let ranges = if writer {
            vec![7..8, 12..16, 52..56, 144..4064]
        } else {
            vec![7..8, 12..16, 44..48, 108..112, 120..4064]
        };
        for offset in ranges.into_iter().flatten() {
            let mut value = literal.to_vec();
            value[offset] = 1;
            seal_control(&mut value);
            assert_eq!(decode(writer, &value), Err(C), "reserved {writer}/{offset}");
            reserved += 1;
        }
    }
    assert_eq!(
        (prefixes, damaged, semantic, reserved),
        (8194, 8192, 26, 7886)
    );
    for which in 0..4 {
        let mut spec = profile.spec();
        match which {
            0 => spec.namespace.database = DatabaseId::new([0x21; 16]).unwrap(),
            1 => spec.namespace.timeline = 2,
            2 => spec.generation = 2,
            3 => spec.id = 2,
            _ => unreachable!(),
        }
        let mut encoded = [0; 4096];
        control::encode_profile(spec, &mut encoded).unwrap();
        let other = control::decode_bootstrap_profile(&encoded, Some(spec.namespace)).unwrap();
        let expected = if matches!(which, 1 | 2) {
            ControlError::InvalidInput
        } else {
            ControlError::IdentityMismatch
        };
        assert_eq!(decode_epoch(EPOCH, other), Err(expected));
        assert_eq!(decode_writer(SELECTED, other, digest), Err(expected));
        if matches!(which, 1 | 2) {
            output.fill(0xa5);
            assert_eq!(
                encode_epoch(other, &mut output),
                Err(ControlError::InvalidInput)
            );
            assert_eq!(output, [0xa5; 4101]);
            assert_eq!(
                encode_writer(other, EPOCH, &mut output),
                Err(ControlError::InvalidInput)
            );
            assert_eq!(output, [0xa5; 4101]);
        }
    }
    output.fill(0xa5);
    assert_eq!(
        encode_epoch(profile, &mut output[..4095]),
        Err(ControlError::NoSpace)
    );
    assert_eq!(output, [0xa5; 4101]);
    assert_eq!(
        encode_writer(profile, EPOCH, &mut output[..4095]),
        Err(ControlError::NoSpace)
    );
    assert_eq!(output, [0xa5; 4101]);
    let mut corrupt_epoch = EPOCH.to_vec();
    corrupt_epoch[88] = 1;
    seal_control(&mut corrupt_epoch);
    assert_eq!(
        encode_writer(profile, &corrupt_epoch, &mut output),
        Err(ControlError::Corrupt)
    );
    assert_eq!(output, [0xa5; 4101]);
}

#[test]
fn genesis_requires_exact_empty_namespace() {
    for case in 0..14 {
        let (writer, state) = fresh();
        let id = match case {
            12 => 0,
            13 => 4,
            _ => 1,
        };
        {
            let mut s = state.lock().unwrap();
            match case {
                0 => {
                    s.files.clear();
                }
                1 => {
                    let lock = s.files.get_mut("LOCK").unwrap();
                    lock.bytes.push(9);
                    lock.length = 1;
                }
                2..=9 => {
                    let name = [
                        PROFILE_NAME,
                        WAL_NAME,
                        STAGE_NAME,
                        "WRITER",
                        "CURRENT",
                        "FILEMAP",
                        "DWB",
                        "other",
                    ][case - 2];
                    let mut entry = Entry::new(42);
                    entry.bytes = b"preserve this artifact".to_vec();
                    entry.length = entry.bytes.len() as u64;
                    s.files.insert(name.into(), entry);
                }
                10 => s.entropy_zero = true,
                11 => s.entropy_error = true,
                _ => (),
            }
        }
        let before = inventory(&state);
        let mut memory = slot();
        memory.fill(0xa5);
        let error = GenesisWalSession::create(writer, id, &mut memory, &mut [0; 16384])
            .err()
            .unwrap();
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(inventory(&state), before);
        assert!(state.lock().unwrap().events.is_empty());
        assert!(memory.iter().all(|&b| b == 0xa5));
        assert_eq!(state.lock().unwrap().writers, 0);
    }
}

#[test]
fn genesis_installs_in_durable_order() {
    let expected = [
        ("create", PROFILE_NAME),
        ("write", PROFILE_NAME),
        ("sync", PROFILE_NAME),
        ("create", WAL_NAME),
        ("write", WAL_NAME),
        ("sync", WAL_NAME),
        ("dirsync", ""),
        ("create", STAGE_NAME),
        ("write", STAGE_NAME),
        ("sync", STAGE_NAME),
        ("publish", STAGE_NAME),
        ("dirsync", ""),
    ];
    for id in 1..=3 {
        let (writer, state) = fresh();
        let mut memory = slot();
        let session = GenesisWalSession::create(writer, id, &mut memory, &mut [0; 16384]).unwrap();
        let pbytes = bytes(&state, PROFILE_NAME);
        let p = control::decode_bootstrap_profile(&pbytes, Some(namespace())).unwrap();
        assert_eq!(p.spec().id, id);
        let epoch = bytes(&state, WAL_NAME);
        let selector = bytes(&state, "WRITER");
        let digest = decode_epoch(&epoch, p).unwrap();
        decode_writer(&selector, p, digest).unwrap();
        if id == 1 {
            assert_eq!(pbytes, PROFILE);
            assert_eq!(epoch, EPOCH);
            assert_eq!(selector, SELECTED);
        }
        let prefix = session.prefix().unwrap();
        assert_eq!(
            (
                prefix.complete_lsn,
                prefix.synced_lsn,
                prefix.complete_offset,
                prefix.synced_offset
            ),
            (0, 0, 4096, 4096)
        );
        {
            let s = state.lock().unwrap();
            assert_eq!(s.events.len(), 12);
            assert_eq!(
                s.events
                    .iter()
                    .map(|e| (e.kind, e.name.as_str()))
                    .collect::<Vec<_>>(),
                expected
            );
            for e in &s.events {
                assert_eq!(e.offset, 0);
                assert_eq!(e.length, if e.kind == "write" { 4096 } else { 0 });
            }
            assert_eq!(s.reads, 9);
            assert_eq!((s.writers, s.handles, s.peak), (1, 1, 2));
            assert_eq!(prefix.file, s.files[WAL_NAME].identity);
            assert_eq!(s.files.len(), 4);
        }
        assert!(acquire(&state).is_err());
        if id == 1 {
            session.try_stage(&raw_frame(0, 1, &[])).unwrap();
            session.sync_pending().unwrap();
            // Actual product port calls/bytes feed the separate bounded durable
            // image checker. The checker has its own frozen reference corpus.
            for (at, event) in state.lock().unwrap().events.iter().enumerate() {
                let hex: String = event.data.iter().map(|b| format!("{b:02x}")).collect();
                println!(
                    "WAL_IO_TRACE|{at}|{}|{}|{}|{}|{hex}",
                    event.kind, event.name, event.offset, event.length
                );
            }
        }
        drop(session);
        {
            let s = state.lock().unwrap();
            assert_eq!((s.handles, s.writers), (0, 0));
        }
        let writer = acquire(&state).unwrap();
        let before = inventory(&state);
        assert!(GenesisWalSession::create(writer, id, &mut memory, &mut [0; 16384]).is_err());
        assert_eq!(inventory(&state), before);
        assert_eq!(
            state.lock().unwrap().events.len(),
            if id == 1 { 14 } else { 12 }
        );
    }
}

fn retry_preserved(state: &Arc<Mutex<State>>, memory: &mut [u8; MAX_FRAME_BYTES]) {
    let before = inventory(state);
    let eligible = before.len() == 1 && before[0].0 == "LOCK" && before[0].1 == 0;
    let count = state.lock().unwrap().events.len();
    {
        let mut s = state.lock().unwrap();
        s.plan = None;
        s.read_fail = None;
        s.metadata_change = None;
    }
    let result = GenesisWalSession::create(acquire(state).unwrap(), 1, memory, &mut [0; 16384]);
    assert_eq!(result.is_ok(), eligible);
    if !eligible {
        assert_eq!(inventory(state), before);
        assert_eq!(state.lock().unwrap().events.len(), count);
        assert_eq!(result.err().unwrap().effect, Effect::Unchanged);
    }
}

#[test]
fn genesis_install_faults_preserve_partial_namespace() {
    for at in 0..12 {
        for action in [Action::BeforeError, Action::AfterError, Action::AfterPanic] {
            let (writer, state) = fresh();
            let mut memory = slot();
            state.lock().unwrap().plan = Some(Plan { at, action });
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                GenesisWalSession::create(writer, 1, &mut memory, &mut [0; 16384]).map(|_| ())
            }));
            if action == Action::AfterPanic {
                assert!(outcome.is_err());
            } else {
                let error = outcome.unwrap().unwrap_err();
                assert_eq!(error.effect, Effect::CallerMustResolve);
                let phase = [
                    Phase::ProfileCreate,
                    Phase::ProfileWrite,
                    Phase::ProfileSync,
                    Phase::EpochCreate,
                    Phase::EpochWrite,
                    Phase::EpochSync,
                    Phase::InitialDirectorySync,
                    Phase::SelectorCreate,
                    Phase::SelectorWrite,
                    Phase::SelectorSync,
                    Phase::SelectorPublish,
                    Phase::SelectedDirectorySync,
                ][at];
                assert_eq!(error.phase, phase);
                if matches!(at, 1 | 2 | 4 | 5 | 8 | 9) {
                    let Cause::Io(cause) = error.cause else {
                        panic!("lost file cause");
                    };
                    assert_eq!(cause.code, xadb_foundation::ErrorCode::IoWriteUnknown);
                    assert_eq!(cause.io_kind, Some(std::io::ErrorKind::Other));
                } else {
                    let Cause::Namespace(cause) = error.cause else {
                        panic!("lost namespace cause");
                    };
                    assert_eq!(
                        cause.kind,
                        xadb_foundation::namespace::NamespaceErrorKind::Io
                    );
                    assert_eq!(
                        cause.effect,
                        if action == Action::AfterError || matches!(at, 6 | 11) {
                            Effect::CallerMustResolve
                        } else {
                            Effect::Unchanged
                        }
                    );
                }
            }
            {
                let s = state.lock().unwrap();
                assert_eq!(s.hit, 1);
                assert_eq!(s.events.len(), at + 1);
                assert_eq!((s.writers, s.handles), (0, 0));
            }
            retry_preserved(&state, &mut memory);
        }
    }
    for at in 0..9 {
        let (writer, state) = fresh();
        let mut memory = slot();
        state.lock().unwrap().read_fail = Some(at);
        let error = GenesisWalSession::create(writer, 1, &mut memory, &mut [0; 16384])
            .err()
            .unwrap();
        assert_eq!(
            (error.phase, error.effect),
            (Phase::Readback, Effect::CallerMustResolve)
        );
        match error.cause {
            Cause::Namespace(e) => assert_eq!(e.effect, Effect::Unchanged),
            Cause::Io(e) => assert_eq!(e.effect(), Effect::Unchanged),
            other => panic!("lost cause {other:?}"),
        }
        assert_eq!(state.lock().unwrap().hit, 1);
        assert_eq!(state.lock().unwrap().events.len(), 12);
        retry_preserved(&state, &mut memory);
    }
    for at in [3, 6] {
        for identity in [false, true] {
            let (writer, state) = fresh();
            let mut memory = slot();
            state.lock().unwrap().metadata_change = Some((at, identity));
            let error = GenesisWalSession::create(writer, 1, &mut memory, &mut [0; 16384])
                .err()
                .unwrap();
            assert_eq!(
                (error.phase, error.effect),
                (Phase::Readback, Effect::CallerMustResolve)
            );
            assert!(if identity {
                matches!(error.cause, Cause::IdentityMismatch)
            } else {
                matches!(error.cause, Cause::LengthMismatch)
            });
            retry_preserved(&state, &mut memory);
        }
    }
}

#[test]
fn wal_slot_writes_exact_logical_and_physical_prefixes() {
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    let mut expected = EPOCH.to_vec();
    let mut previous = 0;
    for (lsn, payload) in [(1, vec![5; 3]), (7, vec![7; 32]), (20, vec![])] {
        let mut frame = raw_frame(previous, lsn, &payload);
        let original = frame.clone();
        let before = session.prefix().unwrap();
        let location = session.try_stage(&frame).unwrap();
        assert_eq!(
            (
                location.previous_lsn,
                location.lsn,
                location.start,
                location.end,
                location.length
            ),
            (
                previous,
                lsn,
                expected.len() as u64,
                (expected.len() + frame.len()) as u64,
                frame.len()
            )
        );
        assert_eq!(location.file, before.file);
        assert_eq!(location.stream, before.stream);
        frame.fill(0xdd); // A successful stage does not borrow the prepared input.
        assert_eq!(
            session
                .runtime
                .append
                .lock()
                .unwrap()
                .slot
                .as_ref()
                .unwrap()[..original.len()],
            original
        );
        let complete = session.prefix().unwrap();
        assert_eq!(
            (
                complete.complete_lsn,
                complete.complete_offset,
                complete.synced_lsn,
                complete.synced_offset
            ),
            (lsn, location.end, previous, expected.len() as u64)
        );
        reject(&session, &state, &raw_frame(lsn, lsn + 1, &[]), |c| {
            matches!(c, Cause::Busy)
        });
        assert_eq!(bytes(&state, WAL_NAME), expected);
        let observation = session.sync_pending().unwrap();
        expected.extend_from_slice(&original);
        assert_eq!(observation.frame, location);
        assert_eq!(
            (
                observation.prefix.synced_lsn,
                observation.prefix.synced_offset
            ),
            (lsn, expected.len() as u64)
        );
        assert_eq!(bytes(&state, WAL_NAME), expected);
        reject(&session, &state, &original, |c| {
            matches!(c, Cause::Frame(WalError::OrderMismatch))
        });
        let mut stale = raw_frame(lsn, lsn + 1, &[]);
        put32(&mut stale, 48, 2);
        seal_frame(&mut stale);
        reject(&session, &state, &stale, |c| {
            matches!(c, Cause::Frame(WalError::IdentityMismatch))
        });
        previous = lsn;
    }
    let calls = state.lock().unwrap().events.len();
    assert!(matches!(
        session.sync_pending().unwrap_err().cause,
        Cause::NoPending
    ));
    assert_eq!(state.lock().unwrap().events.len(), calls);
    // Independent one-record PageRedo shape: the writer consumes legacy tag1
    // without applying this after-image to any page or granting commit authority.
    let mut record = vec![0; 65];
    record[0] = 1;
    record[1] = 1;
    put32(&mut record, 4, 65);
    put64(&mut record, 8, (1u64 << 32) | 1);
    for offset in [24, 32, 40] {
        put64(&mut record, offset, 1);
    }
    put64(&mut record, 48, 20);
    record[56..58].copy_from_slice(&72u16.to_le_bytes());
    record[58..60].copy_from_slice(&1u16.to_le_bytes());
    record[64] = 0x3a;
    let mut redo = raw_frame(20, 21, &record);
    redo[7] = 1;
    redo[20] = 1;
    seal_frame(&mut redo);
    let mut malformed = redo.clone();
    malformed[163] = 1;
    seal_frame(&mut malformed);
    reject(&session, &state, &malformed, |c| {
        matches!(c, Cause::Frame(WalError::Corrupt))
    });
    session.try_stage(&redo).unwrap();
    session.sync_pending().unwrap();
    expected.extend(redo);
    assert_eq!(bytes(&state, WAL_NAME), expected);
}

#[test]
fn wal_slot_rejects_boundaries_before_effect() {
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    for offset in [24, 40, 48, 64] {
        let mut frame = raw_frame(0, 1, &[]);
        frame[offset] ^= 2;
        seal_frame(&mut frame);
        let expected = if offset == 64 {
            WalError::OrderMismatch
        } else {
            WalError::IdentityMismatch
        };
        reject(
            &session,
            &state,
            &frame,
            |c| matches!(c, Cause::Frame(e) if *e == expected),
        );
    }
    let valid = raw_frame(0, 1, &[]);
    for kind in 0..4 {
        let mut frame = valid.clone();
        match kind {
            0 => frame[0] ^= 1,
            1 => frame.push(0),
            2 => {
                frame.pop();
            }
            3 => frame[191] ^= 1,
            _ => unreachable!(),
        }
        reject(&session, &state, &frame, |c| matches!(c, Cause::Frame(_)));
    }
    let mut semantic = valid;
    semantic[7] = 2;
    seal_frame(&mut semantic);
    reject(&session, &state, &semantic, |c| {
        matches!(c, Cause::Frame(WalError::UnsupportedFormat))
    });
    let oversized = raw_frame(0, 1, &vec![0xab; MAX_FRAME_BYTES - 191]);
    reject(&session, &state, &oversized, |c| {
        matches!(c, Cause::Frame(_))
    });
    let maximum = raw_frame(0, 1, &vec![0xab; MAX_FRAME_BYTES - 192]);
    assert_eq!(maximum.len(), MAX_FRAME_BYTES);
    session.try_stage(&maximum).unwrap();
    session.sync_pending().unwrap();
    assert_eq!(&bytes(&state, WAL_NAME)[4096..], maximum);
    drop(session);
    for gap in [193u64, 192, 191] {
        let mut memory = slot();
        let (session, state) = self::session(&mut memory);
        // Typed synthetic Idle state: not an assertion about a recovered prior chain.
        let start = WAL_FILE_CAP - gap;
        {
            let mut a = session.runtime.append.lock().unwrap();
            a.prefix.complete_offset = start;
            a.prefix.synced_offset = start;
            a.prefix.complete_lsn = 11;
            a.prefix.synced_lsn = 11;
        }
        state
            .lock()
            .unwrap()
            .files
            .get_mut(WAL_NAME)
            .unwrap()
            .length = start;
        let frame = raw_frame(11, 12, &[]);
        if gap == 191 {
            reject(&session, &state, &frame, |c| matches!(c, Cause::Capacity));
        } else {
            assert_eq!(session.try_stage(&frame).unwrap().end, start + 192);
            // One 96-byte short write does not charge the interval a second time.
            state.lock().unwrap().plan = Some(Plan {
                at: 12,
                action: Action::Short(96),
            });
            assert_eq!(
                session.sync_pending().unwrap().prefix.synced_offset,
                start + 192
            );
            let s = state.lock().unwrap();
            let e = &s.files[WAL_NAME];
            assert_eq!(e.length, start + 192);
            assert_eq!(e.sparse_tail[&start], frame[..96]);
            assert_eq!(e.sparse_tail[&(start + 96)], frame[96..]);
            assert_eq!(s.events[12].offset, start);
            assert_eq!(s.events[13].offset, start + 96);
        }
    }
    assert_eq!(
        interval_end(i64::MAX as u64 - 192, 192).unwrap(),
        i64::MAX as u64
    );
    for start in [i64::MAX as u64 - 191, u64::MAX - 191] {
        assert!(matches!(
            interval_end(start, 192),
            Err(Cause::ArithmeticOverflow)
        ));
    }
    let mut memory = slot();
    let (session, state) = self::session(&mut memory);
    session.try_stage(&raw_frame(0, u64::MAX, &[])).unwrap();
    session.sync_pending().unwrap();
    for next in [0, 1, u64::MAX] {
        reject(&session, &state, &raw_frame(u64::MAX, next, &[]), |c| {
            matches!(c, Cause::Frame(WalError::OrderMismatch))
        });
    }
}

#[test]
fn wal_io_faults_fence_without_replay() {
    for action in [Action::Short(96), Action::Interrupted] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let frame = raw_frame(0, 1, &[]);
        session.try_stage(&frame).unwrap();
        state.lock().unwrap().plan = Some(Plan { at: 12, action });
        session.sync_pending().unwrap();
        assert!(!session.is_fenced());
        assert_eq!(&bytes(&state, WAL_NAME)[4096..], frame);
        assert_eq!(state.lock().unwrap().hit, 1);
    }
    for (at, action, expected_length) in [
        (12, Action::Zero, 4096),
        (12, Action::InvalidCount, 4096),
        (12, Action::BeforeError, 4096),
        (12, Action::PrefixError, 4192),
        (13, Action::BeforeError, 4288),
        (13, Action::AfterError, 4288),
        (12, Action::AfterPanic, 4288),
        (13, Action::AfterPanic, 4288),
    ] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let frame = raw_frame(0, 1, &[]);
        let location = session.try_stage(&frame).unwrap();
        state.lock().unwrap().plan = Some(Plan { at, action });
        let outcome = catch_unwind(AssertUnwindSafe(|| session.sync_pending()));
        if action == Action::AfterPanic {
            assert!(outcome.is_err());
        } else {
            let error = outcome.unwrap().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert_eq!(
                error.phase,
                if at == 13 { Phase::Sync } else { Phase::Write }
            );
            let Cause::Io(cause) = error.cause else {
                panic!("lost file cause");
            };
            assert_eq!(cause.code, xadb_foundation::ErrorCode::IoWriteUnknown);
            assert_eq!(
                cause.io_kind,
                match action {
                    Action::Zero => Some(std::io::ErrorKind::WriteZero),
                    Action::InvalidCount => None,
                    _ => Some(std::io::ErrorKind::Other),
                }
            );
        }
        {
            let s = state.lock().unwrap();
            let partial = action == Action::PrefixError;
            assert_eq!(s.hit, if partial { 2 } else { 1 });
            assert_eq!(s.events.len(), if partial { 14 } else { at + 1 });
            assert_eq!(s.events[12].kind, "write");
            assert_eq!((s.events[12].offset, s.events[12].length), (4096, 192));
            let last = s.events.last().unwrap();
            assert_eq!(last.name, WAL_NAME);
            assert_eq!(last.kind, if at == 13 { "sync" } else { "write" });
            assert_eq!(
                (last.offset, last.length),
                if at == 13 {
                    (0, 0)
                } else if partial {
                    (4192, 96)
                } else {
                    (4096, 192)
                }
            );
        }
        assert_fenced(&session, &state, 4096);
        let a = session
            .runtime
            .append
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert_eq!(a.state, SlotState::IoOwned(location));
        assert!(a.slot.is_none());
        assert_eq!(a.prefix.complete_offset, 4288);
        assert_eq!(
            state.lock().unwrap().files[WAL_NAME].length,
            expected_length
        );
        assert_eq!(
            &bytes(&state, WAL_NAME)[4096..],
            &frame[..(expected_length - 4096) as usize]
        );
    }
    for file in [false, true] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        session.try_stage(&raw_frame(0, 1, &[])).unwrap();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                if file {
                    let _g = session.runtime.file.lock().unwrap();
                    panic!("file poison");
                } else {
                    let _g = session.runtime.append.lock().unwrap();
                    panic!("gate poison");
                }
            }))
            .is_err()
        );
        assert_fenced(&session, &state, 4096);
        assert_eq!(state.lock().unwrap().events.len(), 12);
    }
    // The sync itself succeeds; publication is independently denied by poison.
    let mut memory = slot();
    let (session, state) = session(&mut memory);
    session.try_stage(&raw_frame(0, 1, &[])).unwrap();
    let (entered, reached) = sync_channel(0);
    let (release, resume) = sync_channel(0);
    state.lock().unwrap().pause = Some(Pause {
        at: 13,
        after: true,
        entered,
        release: resume,
        unwind: false,
    });
    std::thread::scope(|scope| {
        let io = scope.spawn(|| session.sync_pending());
        reached.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _g = session.runtime.append.lock().unwrap();
                panic!("publication poison");
            }))
            .is_err()
        );
        release.send(()).unwrap();
        let error = io.join().unwrap().unwrap_err();
        assert_eq!(error.phase, Phase::Publish);
    });
    assert_fenced(&session, &state, 4096);
    assert_eq!(bytes(&state, WAL_NAME).len(), 4288);
    let a = session.runtime.append.lock().err().unwrap().into_inner();
    assert!(matches!(a.state, SlotState::IoOwned(_)));
    assert!(a.slot.is_none());
}

#[test]
fn wal_io_releases_gate_and_protects_slot() {
    for unwind in [false, true] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let frame = raw_frame(0, 1, &[0x4c; 32]);
        let location = session.try_stage(&frame).unwrap();
        let (entered, reached) = sync_channel(0);
        let (release, resume) = sync_channel(0);
        state.lock().unwrap().pause = Some(Pause {
            at: 12,
            after: false,
            entered,
            release: resume,
            unwind,
        });
        std::thread::scope(|scope| {
            let io = scope.spawn(|| catch_unwind(AssertUnwindSafe(|| session.sync_pending())));
            reached.recv_timeout(Duration::from_secs(5)).unwrap();
            let (finished, observed) = sync_channel(0);
            let session = &session;
            scope.spawn(move || {
                let prefix = session.prefix().unwrap();
                assert!(matches!(
                    session
                        .try_stage(&raw_frame(1, 2, &[0xa5]))
                        .unwrap_err()
                        .cause,
                    Cause::Busy
                ));
                assert!(matches!(
                    session.sync_pending().unwrap_err().cause,
                    Cause::Busy
                ));
                finished.send(prefix).unwrap();
            });
            let prefix = observed.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                (prefix.complete_offset, prefix.synced_offset),
                (location.end, 4096)
            );
            let a = session.runtime.append.lock().unwrap();
            assert_eq!(a.state, SlotState::IoOwned(location));
            assert!(a.slot.is_none());
            drop(a);
            assert_eq!(bytes(&state, WAL_NAME), EPOCH);
            assert_eq!(state.lock().unwrap().events.len(), 13);
            release.send(()).unwrap();
            let outcome = io.join().unwrap();
            if unwind {
                assert!(outcome.is_err());
            } else {
                assert_eq!(outcome.unwrap().unwrap().frame, location);
            }
        });
        if unwind {
            assert_fenced(&session, &state, 4096);
            assert_eq!(bytes(&state, WAL_NAME), EPOCH);
        } else {
            assert_eq!(&bytes(&state, WAL_NAME)[4096..], frame);
        }
    }
    for sync_b in [false, true] {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let a = raw_frame(0, 1, &[0xa1]);
        let b = raw_frame(0, 7, &[0xb7; 32]);
        let (entered, reached) = sync_channel(0);
        let (release, resume) = sync_channel(0);
        std::thread::scope(|scope| {
            let session_ref = &session;
            let a_ref = &a;
            let worker = scope.spawn(move || {
                session_ref.runtime.stage_inner(a_ref, None, || {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                })
            });
            reached.recv_timeout(Duration::from_secs(5)).unwrap();
            let (finished, observed) = sync_channel(0);
            let session = &session;
            let b = &b;
            scope.spawn(move || {
                session.try_stage(b).unwrap();
                if sync_b {
                    session.sync_pending().unwrap();
                }
                finished.send(()).unwrap();
            });
            observed.recv_timeout(Duration::from_secs(5)).unwrap();
            let expected = session.prefix().unwrap();
            let files = inventory(&state);
            let calls = state.lock().unwrap().events.len();
            release.send(()).unwrap();
            let error = worker.join().unwrap().unwrap_err();
            assert_eq!(error.effect, Effect::Unchanged);
            assert!(if sync_b {
                matches!(error.cause, Cause::Frame(WalError::OrderMismatch))
            } else {
                matches!(error.cause, Cause::Busy)
            });
            assert_eq!(session.prefix().unwrap(), expected);
            assert_eq!(inventory(&state), files);
            assert_eq!(state.lock().unwrap().events.len(), calls);
            assert_eq!(
                &session
                    .runtime
                    .append
                    .lock()
                    .unwrap()
                    .slot
                    .as_ref()
                    .unwrap()[..b.len()],
                b.as_slice()
            );
        });
    }
}

#[test]
fn wal_generated_prefixes_match_independent_bytes() {
    let mut census = [0u64; 8];
    let mut staged_bytes = 0;
    let mut max_lsn = 0;
    let mut max_offset = 0;
    for seed in 0..64u64 {
        let mut memory = slot();
        let (session, state) = session(&mut memory);
        let mut rng = seed + 1;
        let mut current = 0;
        let mut synced = 0;
        let mut pending: Option<Vec<u8>> = None;
        let mut expected = EPOCH.to_vec();
        for step in 0..128u64 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            match rng >> 61 {
                0..=3 => {
                    let lsn = current + 1 + ((rng >> 8) & 7);
                    let payload: Vec<_> = (0..((rng >> 16) & 255))
                        .map(|i| (i + seed + step) as u8)
                        .collect();
                    let frame = raw_frame(current, lsn, &payload);
                    if pending.is_some() {
                        reject(&session, &state, &frame, |c| matches!(c, Cause::Busy));
                        census[1] += 1;
                    } else {
                        let location = session.try_stage(&frame).unwrap();
                        assert_eq!(location.start, expected.len() as u64);
                        current = lsn;
                        staged_bytes += frame.len() as u64;
                        pending = Some(frame);
                        census[0] += 1;
                    }
                }
                4..=5 => {
                    if let Some(frame) = pending.take() {
                        session.sync_pending().unwrap();
                        expected.extend(frame);
                        synced = current;
                        census[2] += 1;
                    } else {
                        assert!(matches!(
                            session.sync_pending().unwrap_err().cause,
                            Cause::NoPending
                        ));
                        census[3] += 1;
                    }
                }
                6 => {
                    let mut frame = raw_frame(current, current + 1, &[]);
                    put32(&mut frame, 48, 2);
                    seal_frame(&mut frame);
                    if pending.is_some() {
                        reject(&session, &state, &frame, |c| matches!(c, Cause::Busy));
                        census[4] += 1;
                    } else {
                        reject(&session, &state, &frame, |c| {
                            matches!(c, Cause::Frame(WalError::IdentityMismatch))
                        });
                        census[5] += 1;
                    }
                }
                7 => census[6] += 1,
                _ => unreachable!(),
            }
            let prefix = session.prefix().unwrap();
            assert_eq!((prefix.complete_lsn, prefix.synced_lsn), (current, synced));
            assert_eq!(
                (prefix.complete_offset, prefix.synced_offset),
                (
                    (expected.len() + pending.as_ref().map_or(0, Vec::len)) as u64,
                    expected.len() as u64
                )
            );
            let actual = bytes(&state, WAL_NAME);
            assert_eq!(actual, expected);
            assert_eq!(content_digest(&actual), content_digest(&expected));
            if let Some(frame) = &pending {
                assert_eq!(
                    session
                        .runtime
                        .append
                        .lock()
                        .unwrap()
                        .slot
                        .as_ref()
                        .unwrap()[..frame.len()],
                    **frame
                );
            }
        }
        if let Some(frame) = pending {
            session.sync_pending().unwrap();
            expected.extend(frame);
            census[7] += 1;
        }
        assert_eq!(bytes(&state, WAL_NAME), expected);
        assert_eq!(
            (
                session.prefix().unwrap().synced_lsn,
                session.prefix().unwrap().synced_offset
            ),
            (current, expected.len() as u64)
        );
        max_lsn = max_lsn.max(current);
        max_offset = max_offset.max(expected.len());
    }
    assert_eq!(census, [1378, 2718, 1336, 727, 705, 325, 1003, 42]);
    assert_eq!((staged_bytes, max_lsn, max_offset), (443779, 135, 15315));
}

#[test]
fn wal_workspace_and_handles_remain_bounded() {
    assert!(std::mem::size_of::<GenesisWalSession<'_, Writer>>() <= 16384);
    assert_eq!(MAX_FRAME_BYTES, 1 << 20);
    let mut memory = slot();
    let pointer = memory.as_ptr();
    let (session, state) = session(&mut memory);
    assert_eq!(
        session
            .runtime
            .append
            .lock()
            .unwrap()
            .slot
            .as_ref()
            .unwrap()
            .as_ptr(),
        pointer
    );
    let input = raw_frame(0, 1, &[]);
    assert_ne!(input.as_ptr(), pointer);
    session.try_stage(&input).unwrap();
    drop(input);
    {
        let s = state.lock().unwrap();
        assert_eq!((s.handles, s.peak, s.files.len()), (1, 2, 4));
    }
    let before = inventory(&state);
    let calls = state.lock().unwrap().events.len();
    drop(session);
    assert_eq!(inventory(&state), before);
    assert_eq!(state.lock().unwrap().events.len(), calls);
    {
        let s = state.lock().unwrap();
        assert_eq!((s.writers, s.handles), (0, 0));
    }
    memory[0] = 0x11;
    let mut memory = slot();
    let (session, state) = self::session(&mut memory);
    session.try_stage(&raw_frame(0, 1, &[])).unwrap();
    state.lock().unwrap().plan = Some(Plan {
        at: 12,
        action: Action::BeforeError,
    });
    assert!(session.sync_pending().is_err());
    let before = inventory(&state);
    let calls = state.lock().unwrap().events.len();
    drop(session);
    assert_eq!(inventory(&state), before);
    assert_eq!(state.lock().unwrap().events.len(), calls);
}
