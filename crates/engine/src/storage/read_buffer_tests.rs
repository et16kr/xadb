use super::super::{
    page::{FileId, PageError},
    page_io::test_support::{
        self as port, Action, DATA, MAP, Pause, Plan, Writer, expected, fixture, ns, put32, put64,
        seal,
    },
};
use super::*;
use crate::api::DatabaseId;
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};
use xadb_foundation::{
    codec::content_digest,
    identity::{Generation, ObjectId},
};

fn request(page: u64) -> PageIdentity {
    let mut id = expected();
    id.page.page_no = page;
    id
}
fn image(page: u64, internal: bool) -> [u8; PAGE_BYTES] {
    let mut bytes: [u8; PAGE_BYTES] = port::GOLDEN.try_into().unwrap();
    put64(&mut bytes, 32, page);
    if internal {
        bytes[6] = 2;
        bytes[7] = 1;
    }
    seal(&mut bytes);
    bytes
}
fn source() -> Writer {
    let writer = fixture(1, false);
    {
        let mut state = writer.0.lock().unwrap();
        let data = state.files.get_mut(DATA).unwrap();
        for page in 1..=9 {
            data[page * PAGE_BYTES..(page + 1) * PAGE_BYTES]
                .copy_from_slice(&image(page as u64, false));
        }
    }
    writer
}
fn with_buffer<T>(
    writer: &mut Writer,
    n: usize,
    run: impl FnOnce(&ReadBuffer<'_, '_, Writer>) -> T,
) -> T {
    let mut root = [0; PAGE_BYTES];
    let mut scratch = [0; PAGE_BYTES];
    let mut load = [0; PAGE_BYTES];
    let mut frames: Vec<_> = (0..n).map(|_| ReadFrame::new()).collect();
    let reader = PageFileReader::open(writer, Some(ns()), &mut root, &mut scratch).unwrap();
    let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
    run(&buffer)
}
fn pin<'p, 's, 'm>(
    buffer: &'p ReadBuffer<'s, 'm, Writer>,
    page: u64,
) -> PagePin<'p, 's, 'm, Writer> {
    buffer.pin(request(page), PageKind::RowLeaf, 0).unwrap()
}
fn full(pin: &PagePin<'_, '_, '_, Writer>, page: u64) {
    let guard = pin.read().unwrap();
    assert_eq!(guard.bytes(), &image(page, false));
    assert_eq!(guard.view().unwrap().header().identity, request(page));
}
fn data_reads(state: &Arc<Mutex<port::State>>) -> usize {
    state
        .lock()
        .unwrap()
        .reads
        .iter()
        .filter(|r| r.name == DATA)
        .count()
}
fn clean(writer: &Writer) {
    let state = writer.0.lock().unwrap();
    assert_eq!((state.handles, state.mutations), (0, 0));
    assert!(state.peak <= 2);
}
fn headers(buffer: &ReadBuffer<'_, '_, Writer>) -> Vec<Header> {
    buffer
        .frames
        .iter()
        .map(|f| *f.header.lock().unwrap())
        .collect()
}
fn busy(buffer: &ReadBuffer<'_, '_, Writer>, page: u64) {
    assert!(matches!(
        buffer.pin(request(page), PageKind::RowLeaf, 0),
        Err(BufferError::BufferBusy)
    ));
}
fn channel() -> (mpsc::SyncSender<()>, mpsc::Receiver<()>) {
    mpsc::sync_channel(0)
}
fn receive(rx: &mpsc::Receiver<()>) {
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
fn read_buffer_pins_selected_bytes_and_hits() {
    let mut writer = source();
    let state = Arc::clone(&writer.0);
    with_buffer(&mut writer, 2, |buffer| {
        assert_eq!(buffer.selector_generation, 29);
        let a = pin(buffer, 1);
        let again = pin(buffer, 1);
        let b = pin(buffer, 2);
        full(&a, 1);
        full(&again, 1);
        full(&b, 2);
        assert_eq!((a.index, a.generation), (again.index, again.generation));
        assert_eq!(headers(buffer)[a.index].pins, 2);
        assert_eq!(data_reads(&state), 2);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .reads
                .iter()
                .filter(|r| r.name == MAP)
                .count(),
            4
        ); // root, initial chunk, two lookups
        drop(again);
        drop(a);
        let a = pin(buffer, 1);
        full(&a, 1);
        assert_eq!(a.generation, 1);
        drop(a);
        drop(b);
        assert!(headers(buffer).iter().all(|h| h.pins == 0));
        assert_eq!(data_reads(&state), 2);
    });
    clean(&writer);
}

#[test]
fn read_buffer_capacity_and_profile_bounds() {
    assert!(std::mem::size_of::<ReadFrame>() - PAGE_BYTES <= 256);
    assert!(std::mem::size_of::<ReadBuffer<'_, '_, Writer>>() <= 4096);
    for (profile, max) in [(1, 256), (2, 1024), (3, 4096)] {
        for n in [0, 1, max, max + 1] {
            let mut writer = source();
            {
                let mut state = writer.0.lock().unwrap();
                let profile_bytes = state.files.get_mut("d3-profile").unwrap();
                put32(profile_bytes, 128, profile);
                put64(profile_bytes, 168, (max * PAGE_BYTES) as u64);
                seal(profile_bytes);
                let hash = content_digest(profile_bytes);
                let selector = state.files.get_mut("FILEMAP").unwrap();
                selector[48..80].copy_from_slice(&hash);
                seal(selector);
            }
            let mut frames: Vec<_> = (0..n).map(|_| ReadFrame::new()).collect();
            for frame in &mut frames {
                frame.header.get_mut().unwrap().generation = 7;
            }
            let mut root = [0; PAGE_BYTES];
            let mut scratch = [0; PAGE_BYTES];
            let mut load = [0; PAGE_BYTES];
            let reader =
                PageFileReader::open(&mut writer, Some(ns()), &mut root, &mut scratch).unwrap();
            assert_eq!(reader.profile().spec().id, profile);
            {
                let result = ReadBuffer::new(reader, &mut frames, &mut load);
                if n == 0 || n > max {
                    assert!(matches!(result, Err(BufferError::InvalidCapacity)));
                } else {
                    let buffer = result.unwrap();
                    assert_eq!(buffer.frames.len(), n);
                    let a = pin(&buffer, 1);
                    full(&a, 1);
                    assert_eq!(a.generation, 8);
                }
            }
            if n > max {
                assert!(frames.iter().all(|f| *f.header.lock().unwrap()
                    == Header {
                        generation: 7,
                        ..Header::new()
                    }));
            }
            clean(&writer);
        }
    }
}

#[test]
fn read_buffer_clock_and_pins_bound_replacement() {
    for n in 1..=3 {
        let mut writer = source();
        let state = Arc::clone(&writer.0);
        with_buffer(&mut writer, n, |buffer| {
            let mut pins: Vec<_> = (1..=n).map(|p| Some(pin(buffer, p as u64))).collect();
            let duplicate = pin(buffer, 1);
            let generation = duplicate.generation;
            busy(buffer, 9);
            assert_eq!(data_reads(&state), n);
            drop(pins[0].take());
            busy(buffer, 9);
            full(&duplicate, 1);
            drop(duplicate);
            let replacement = pin(buffer, 9);
            assert_eq!(
                (replacement.index, replacement.generation),
                (0, generation + 1)
            );
            full(&replacement, 9);
            for (i, p) in pins.iter().enumerate().skip(1) {
                full(p.as_ref().unwrap(), (i + 1) as u64);
            }
            drop(replacement);
            drop(pins);
            // All reference bits true; exact two-pass clock chooses current hand.
            let hand = *buffer.hand.lock().unwrap();
            let next = pin(buffer, 8);
            assert_eq!(next.index, hand);
            full(&next, 8);
            assert_eq!(*buffer.hand.lock().unwrap(), (hand + 1) % n);
        });
        clean(&writer);
    }
    // A failed replacement consumes only the selected victim's generation.
    let mut writer = source();
    let state = Arc::clone(&writer.0);
    with_buffer(&mut writer, 2, |buffer| {
        let a = pin(buffer, 1);
        drop(a);
        let b = pin(buffer, 2);
        let phase = state.lock().unwrap().reads.len();
        state.lock().unwrap().plan = Some(Plan {
            phase,
            action: Action::BeforeError,
        });
        assert!(matches!(
            buffer.pin(request(3), PageKind::RowLeaf, 0),
            Err(BufferError::Read(_))
        ));
        assert_eq!(
            headers(buffer)[0],
            Header {
                generation: 2,
                ..Header::new()
            }
        );
        full(&b, 2);
        state.lock().unwrap().plan = None;
        let c = pin(buffer, 3);
        assert_eq!(c.generation, 3);
        full(&c, 3);
    });
    clean(&writer);
}

fn aliases(base: Request) -> Vec<Request> {
    let mut result = Vec::new();
    for field in 0..4 {
        let mut other = base;
        match field {
            0 => other.identity.page.incarnation = Generation::new(99).unwrap(),
            1 => other.identity.owner = ObjectId::new(99).unwrap(),
            2 => other.identity.owner_generation = Generation::new(99).unwrap(),
            _ => {
                other.kind = if base.level == 0 {
                    PageKind::IndexLeaf
                } else {
                    PageKind::IndexInternal
                }
            }
        }
        result.push(other);
    }
    if base.level == 1 {
        result.push(Request { level: 2, ..base });
    }
    result
}
#[test]
fn read_buffer_identity_never_aliases_cached_pages() {
    let mut checked = 0;
    for internal in [false, true] {
        for loading in [false, true] {
            let mut writer = source();
            let state = Arc::clone(&writer.0);
            state.lock().unwrap().files.get_mut(DATA).unwrap()[PAGE_BYTES..2 * PAGE_BYTES]
                .copy_from_slice(&image(1, internal));
            with_buffer(&mut writer, 1, |buffer| {
                let base = Request {
                    identity: request(1),
                    kind: if internal {
                        PageKind::RowInternal
                    } else {
                        PageKind::RowLeaf
                    },
                    level: u8::from(internal),
                };
                let actual = buffer.pin(base.identity, base.kind, base.level).unwrap();
                if loading {
                    // Header-only controlled state; live Loading concurrency is also exercised below.
                    buffer.frames[actual.index].header.lock().unwrap().state = State::Loading(base);
                }
                let before = headers(buffer);
                let reads = state.lock().unwrap().reads.len();
                for alias in aliases(base) {
                    assert!(matches!(
                        buffer.pin(alias.identity, alias.kind, alias.level),
                        Err(BufferError::IdentityMismatch)
                    ));
                    assert_eq!(headers(buffer), before);
                    assert_eq!(state.lock().unwrap().reads.len(), reads);
                    checked += 1;
                }
                buffer.frames[actual.index].header.lock().unwrap().state = State::Clean(base);
                let guard = actual.read().unwrap();
                assert_eq!(guard.bytes(), &image(1, internal));
            });
            clean(&writer);
        }
    }
    assert_eq!(checked, 18);
    let mut writer = source();
    let state = Arc::clone(&writer.0);
    with_buffer(&mut writer, 2, |buffer| {
        let mut wrong_db = request(1);
        wrong_db.database = DatabaseId::new([0x22; 16]).unwrap();
        let cases = [
            (wrong_db, PageKind::RowLeaf, 0),
            (request(1), PageKind::RowLeaf, 1),
            (request(1), PageKind::RowInternal, 0),
            (request(1), PageKind::RowInternal, 16),
            (request(1), PageKind::Allocation, 0),
            (request(0), PageKind::RowLeaf, 0),
        ];
        let before = headers(buffer);
        let reads = state.lock().unwrap().reads.len();
        for (i, (id, kind, level)) in cases.into_iter().enumerate() {
            let error = buffer.pin(id, kind, level);
            if i == 0 {
                assert!(matches!(
                    error,
                    Err(BufferError::Read(PageIoError::IdentityMismatch))
                ));
            } else {
                assert!(matches!(
                    error,
                    Err(BufferError::Read(PageIoError::InvalidRequest))
                ));
            }
        }
        assert_eq!(headers(buffer), before);
        assert_eq!(state.lock().unwrap().reads.len(), reads);
        let mut missing = request(1);
        missing.page.file = FileId::new(0x100000003).unwrap();
        assert!(matches!(
            buffer.pin(missing, PageKind::RowLeaf, 0),
            Err(BufferError::Read(PageIoError::MissingMapping))
        ));
        let a = pin(buffer, 1);
        let b = pin(buffer, 2);
        assert_ne!(a.index, b.index);
        full(&a, 1);
        full(&b, 2);
    });
    clean(&writer);
    // Two mapped files share the same page number without sharing a cache key.
    let mut writer = fixture(2, false);
    let mut second = image(1, false);
    put64(&mut second, 24, 0x100000003);
    seal(&mut second);
    {
        let mut state = writer.0.lock().unwrap();
        state.files.get_mut(DATA).unwrap()[PAGE_BYTES..2 * PAGE_BYTES]
            .copy_from_slice(&image(1, false));
        let mut bytes = vec![0; 1 << 20];
        bytes[PAGE_BYTES..2 * PAGE_BYTES].copy_from_slice(&second);
        state.files.insert("d3-data-0000000100000003".into(), bytes);
    }
    with_buffer(&mut writer, 2, |buffer| {
        let a = pin(buffer, 1);
        let mut b_id = request(1);
        b_id.page.file = FileId::new(0x100000003).unwrap();
        let b = buffer.pin(b_id, PageKind::RowLeaf, 0).unwrap();
        assert_ne!(a.index, b.index);
        let guard = b.read().unwrap();
        assert_eq!(guard.bytes(), &second);
        assert_eq!(guard.view().unwrap().header().identity, b_id);
        full(&a, 1);
        let mut absent = request(1);
        absent.page.file = FileId::new(0x100000004).unwrap();
        assert!(matches!(
            buffer.pin(absent, PageKind::RowLeaf, 0),
            Err(BufferError::BufferBusy)
        ));
        drop(guard);
        drop(b);
        assert!(matches!(
            buffer.pin(absent, PageKind::RowLeaf, 0),
            Err(BufferError::Read(PageIoError::MissingMapping))
        ));
        full(&a, 1);
    });
    clean(&writer);
}

#[test]
fn read_buffer_load_failure_releases_claim() {
    let actions = [
        Action::BeforeError,
        Action::Short(1),
        Action::Short(17),
        Action::Short(4096),
        Action::Interrupted,
        Action::PrefixEof,
        Action::PrefixError,
        Action::InvalidCount,
    ];
    let mut successes = 0;
    let mut failures = 0;
    for phase_offset in 0..2 {
        for action in actions {
            let mut writer = source();
            let state = Arc::clone(&writer.0);
            with_buffer(&mut writer, 2, |buffer| {
                let stable = pin(buffer, 1);
                let phase = state.lock().unwrap().reads.len() + phase_offset;
                state.lock().unwrap().plan = Some(Plan { phase, action });
                let result = buffer.pin(request(2), PageKind::RowLeaf, 0);
                let success = matches!(action, Action::Short(_) | Action::Interrupted);
                if success {
                    let p = result.unwrap();
                    full(&p, 2);
                    successes += 1;
                } else {
                    assert!(matches!(result, Err(BufferError::Read(_))));
                    failures += 1;
                }
                full(&stable, 1);
                assert!(
                    headers(buffer)
                        .iter()
                        .all(|h| !matches!(h.state, State::Loading(_)))
                );
                assert!(state.lock().unwrap().hit);
                if matches!(action, Action::Short(_)) {
                    assert!(state.lock().unwrap().actual_short);
                }
                state.lock().unwrap().plan = None;
                let retried = pin(buffer, 2);
                full(&retried, 2);
                assert_eq!(retried.generation, if success { 1 } else { 2 });
            });
            clean(&writer);
        }
    }
    assert_eq!((successes, failures), (8, 8));
    for damage in 0..6 {
        let mut writer = if damage == 4 {
            fixture(0, false)
        } else {
            source()
        };
        {
            let mut s = writer.0.lock().unwrap();
            match damage {
                0 => s.files.get_mut(DATA).unwrap()[PAGE_BYTES + 256] ^= 1,
                1 => {
                    let bytes = &mut s.files.get_mut(DATA).unwrap()[PAGE_BYTES..2 * PAGE_BYTES];
                    put64(bytes, 40, 99);
                    seal(bytes);
                }
                2 => {
                    s.files.get_mut(DATA).unwrap().pop();
                }
                3 => s.files.get_mut(DATA).unwrap().push(0),
                4 => {}
                _ => s.files.get_mut(MAP).unwrap()[PAGE_BYTES + 128] ^= 1,
            }
        }
        if damage == 5 {
            let mut root = [0; PAGE_BYTES];
            let mut scratch = [0; PAGE_BYTES];
            assert!(matches!(
                PageFileReader::open(&mut writer, Some(ns()), &mut root, &mut scratch),
                Err(PageIoError::Control(_))
            ));
        } else {
            with_buffer(&mut writer, 2, |buffer| {
                let result = buffer.pin(request(1), PageKind::RowLeaf, 0);
                match damage {
                    0 => assert!(matches!(
                        result,
                        Err(BufferError::Read(PageIoError::Page(PageError::Corrupt)))
                    )),
                    1 => assert!(matches!(
                        result,
                        Err(BufferError::Read(PageIoError::Page(
                            PageError::IdentityMismatch
                        )))
                    )),
                    2 | 3 => assert!(matches!(
                        result,
                        Err(BufferError::Read(PageIoError::LengthMismatch))
                    )),
                    _ => assert!(matches!(
                        result,
                        Err(BufferError::Read(PageIoError::MissingMapping))
                    )),
                }
                assert_eq!(
                    headers(buffer)[0],
                    Header {
                        generation: 1,
                        ..Header::new()
                    }
                );
            });
        }
        clean(&writer);
        // Teardown precedes fixture repair; no external mutation of a selected live source.
        let mut repaired = source();
        with_buffer(&mut repaired, 2, |buffer| full(&pin(buffer, 1), 1));
        clean(&repaired);
    }
}

#[test]
fn read_buffer_loading_deduplicates_and_keeps_hits_live() {
    let mut writer = source();
    let state = Arc::clone(&writer.0);
    with_buffer(&mut writer, 2, |buffer| {
        drop(pin(buffer, 1));
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        state.lock().unwrap().pause = Some(Pause {
            offset: 2 * PAGE_BYTES as u64,
            entered: entered_tx,
            release: release_rx,
        });
        std::thread::scope(|scope| {
            let loader = scope.spawn(|| {
                let p = pin(buffer, 2);
                full(&p, 2);
                p.generation
            });
            receive(&entered_rx);
            let (done_tx, done_rx) = channel();
            let peer = scope.spawn(move || {
                busy(buffer, 2);
                let mut alias = request(2);
                alias.owner = ObjectId::new(99).unwrap();
                assert!(matches!(
                    buffer.pin(alias, PageKind::RowLeaf, 0),
                    Err(BufferError::IdentityMismatch)
                ));
                let a = pin(buffer, 1);
                full(&a, 1);
                busy(buffer, 3);
                drop(a);
                done_tx.send(()).unwrap();
            });
            // Always release I/O before reporting failure, so an error cannot hang scoped join.
            let completion = done_rx.recv_timeout(Duration::from_secs(5));
            release_tx.send(()).unwrap();
            completion.unwrap();
            peer.join().unwrap();
            let generation = loader.join().unwrap();
            let b = pin(buffer, 2);
            let again = pin(buffer, 2);
            full(&b, 2);
            full(&again, 2);
            assert_eq!((b.generation, again.generation), (generation, generation));
        });
        assert_eq!(data_reads(&state), 2);
    });
    clean(&writer);
}

#[test]
fn read_buffer_concurrent_pins_prevent_aba() {
    let mut cycles = 0;
    for seed in 0..64 {
        let mut writer = source();
        let state = Arc::clone(&writer.0);
        with_buffer(&mut writer, 1, |buffer| {
            let a = 1 + seed % 4;
            let b = 5 + seed % 4;
            let (a_tx, b_rx) = channel();
            let (b_tx, a_rx) = channel();
            std::thread::scope(|scope| {
                let first = scope.spawn(move || {
                    let mut last = 0;
                    for _ in 0..128 {
                        let one = pin(buffer, a);
                        let two = pin(buffer, a);
                        let guard = two.read().unwrap();
                        assert!(one.generation > last);
                        assert_eq!(one.generation, two.generation);
                        last = one.generation;
                        assert_eq!(guard.bytes(), &image(a, false));
                        a_tx.send(()).unwrap();
                        receive(&a_rx);
                        drop(one);
                        assert_eq!(guard.bytes(), &image(a, false));
                        a_tx.send(()).unwrap();
                        receive(&a_rx);
                        drop(guard);
                        drop(two);
                        a_tx.send(()).unwrap();
                        receive(&a_rx);
                        let again = pin(buffer, a);
                        full(&again, a);
                        assert_eq!(again.generation, last + 2);
                        drop(again);
                    }
                });
                let second = scope.spawn(move || {
                    for _ in 0..128 {
                        receive(&b_rx);
                        busy(buffer, b);
                        b_tx.send(()).unwrap();
                        receive(&b_rx);
                        busy(buffer, b);
                        b_tx.send(()).unwrap();
                        receive(&b_rx);
                        let other = pin(buffer, b);
                        full(&other, b);
                        drop(other);
                        b_tx.send(()).unwrap();
                    }
                });
                first.join().unwrap();
                second.join().unwrap();
            });
            assert_eq!(data_reads(&state), 257);
            assert_eq!(headers(buffer)[0].pins, 0);
        });
        clean(&writer);
        cycles += 128;
    }
    assert_eq!(cycles, 8192);
}

#[test]
fn read_buffer_limits_and_poison_fence() {
    let mut writer = source();
    with_buffer(&mut writer, 2, |buffer| {
        let a = pin(buffer, 1);
        buffer.frames[a.index].header.lock().unwrap().pins = u32::MAX;
        let before = headers(buffer);
        assert!(matches!(
            buffer.pin(request(1), PageKind::RowLeaf, 0),
            Err(BufferError::PinLimit)
        ));
        assert_eq!(headers(buffer), before);
        buffer.frames[a.index].header.lock().unwrap().pins = 1;
        drop(a);
        for frame in buffer.frames {
            frame.header.lock().unwrap().generation = u64::MAX;
        }
        let a = pin(buffer, 1);
        full(&a, 1);
        assert_eq!(a.generation, u64::MAX);
        drop(a);
        assert!(matches!(
            buffer.pin(request(2), PageKind::RowLeaf, 0),
            Err(BufferError::GenerationExhausted)
        ));
        assert!(headers(buffer).iter().all(|h| h.generation == u64::MAX));
        buffer.frames[1].header.lock().unwrap().generation = 3;
        let b = pin(buffer, 2);
        assert_eq!((b.index, b.generation), (1, 4));
        full(&b, 2);
    });
    clean(&writer);
    for target in 0..4 {
        let mut writer = source();
        with_buffer(&mut writer, 2, |buffer| {
            let existing = pin(buffer, 1);
            std::thread::scope(|scope| {
                assert!(
                    scope
                        .spawn(|| match target {
                            0 => {
                                let _held = buffer.loader.lock().unwrap();
                                panic!("test-only loader poison");
                            }
                            1 => {
                                let _held = buffer.frames[existing.index].header.lock().unwrap();
                                panic!("test-only header poison");
                            }
                            2 => {
                                let _held = buffer.frames[existing.index].content.write().unwrap();
                                panic!("test-only content poison");
                            }
                            _ => {
                                let _held = buffer.hand.lock().unwrap();
                                panic!("test-only selection poison");
                            }
                        })
                        .join()
                        .is_err()
                );
            });
            if target == 2 {
                assert!(matches!(existing.read(), Err(BufferError::Poisoned)));
            } else {
                assert!(matches!(
                    buffer.pin(request(2), PageKind::RowLeaf, 0),
                    Err(BufferError::Poisoned)
                ));
            }
            assert!(buffer.fenced.load(Ordering::Acquire));
            drop(existing);
            assert!(matches!(
                buffer.pin(request(1), PageKind::RowLeaf, 0),
                Err(BufferError::Poisoned)
            ));
        });
        clean(&writer);
    }
    // Actual read unwinds after Loading claim, with the fake State lock released.
    // Inspect fence before any other operation can discover loader poison.
    let mut writer = source();
    let state = Arc::clone(&writer.0);
    with_buffer(&mut writer, 2, |buffer| {
        let existing = pin(buffer, 1);
        let (entered_tx, entered_rx) = channel();
        let (release_tx, release_rx) = channel();
        state.lock().unwrap().pause = Some(Pause {
            offset: 2 * PAGE_BYTES as u64,
            entered: entered_tx,
            release: release_rx,
        });
        std::thread::scope(|scope| {
            let loader = scope.spawn(|| {
                let _pin = pin(buffer, 2);
            });
            receive(&entered_rx);
            drop(release_tx);
            assert!(loader.join().is_err());
        });
        assert!(buffer.fenced.load(Ordering::Acquire));
        assert!(!state.is_poisoned());
        assert!(buffer.loader.is_poisoned());
        drop(existing);
        assert!(matches!(
            buffer.pin(request(1), PageKind::RowLeaf, 0),
            Err(BufferError::Poisoned)
        ));
    });
    clean(&writer);
}

// Independent value model: no production Request/selection helper participates in decisions.
#[derive(Clone, Copy, Default)]
struct ModelSlot {
    page: Option<u64>,
    count: u32,
    recent: bool,
    generation: u64,
}
#[derive(Default, Debug, PartialEq)]
struct Counts {
    success: usize,
    busy: usize,
    releases: usize,
    reads: usize,
    noop: usize,
    loads: usize,
    hits: usize,
    visits: usize,
}
fn model_pin(
    slots: &mut [ModelSlot],
    hand: &mut usize,
    page: u64,
    counts: &mut Counts,
) -> Option<usize> {
    if let Some(i) = slots.iter().position(|s| s.page == Some(page)) {
        slots[i].count += 1;
        slots[i].recent = true;
        counts.success += 1;
        counts.hits += 1;
        return Some(i);
    }
    for _ in 0..slots.len() * 2 {
        let i = *hand;
        *hand = (*hand + 1) % slots.len();
        counts.visits += 1;
        let slot = &mut slots[i];
        if slot.count > 0 {
            continue;
        }
        if slot.page.is_some() && slot.recent {
            slot.recent = false;
            continue;
        }
        *slot = ModelSlot {
            page: Some(page),
            count: 1,
            recent: true,
            generation: slot.generation + 1,
        };
        counts.success += 1;
        counts.loads += 1;
        return Some(i);
    }
    counts.busy += 1;
    None
}
#[test]
fn read_buffer_generated_sequences_match_reference_clock() {
    let mut counts = Counts::default();
    for seed in 0..64 {
        let n = [1, 2, 3, 8][seed % 4];
        let mut writer = source();
        let state = Arc::clone(&writer.0);
        let initial_loads = counts.loads;
        with_buffer(&mut writer, n, |buffer| {
            let mut slots = vec![ModelSlot::default(); n];
            let mut hand = 0;
            let mut handles: [Option<PagePin<'_, '_, '_, Writer>>; 8] =
                std::array::from_fn(|_| None);
            let mut model_handles: [Option<usize>; 8] = [None; 8];
            let mut rng = seed as u64 + 1;
            for _ in 0..128 {
                rng = rng
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let h = ((rng >> 32) % 8) as usize;
                let action = (rng >> 40) % 4;
                let page = 1 + (rng >> 48) % 9;
                if action == 2 && handles[h].is_none() {
                    counts.noop += 1;
                }
                if action < 2 || action == 2 && handles[h].is_some() {
                    if let Some(index) = model_handles[h].take() {
                        slots[index].count -= 1;
                        counts.releases += 1;
                    }
                    drop(handles[h].take());
                }
                match action {
                    0 | 1 => {
                        let expected = model_pin(&mut slots, &mut hand, page, &mut counts);
                        let result = buffer.pin(request(page), PageKind::RowLeaf, 0);
                        if let Some(i) = expected {
                            let actual = result.unwrap();
                            assert_eq!((actual.index, actual.generation), (i, slots[i].generation));
                            full(&actual, page);
                            handles[h] = Some(actual);
                            model_handles[h] = Some(i);
                        } else {
                            assert!(matches!(result, Err(BufferError::BufferBusy)));
                        }
                    }
                    2 => {}
                    _ => {
                        if let Some(p) = handles[h].as_ref() {
                            full(p, slots[model_handles[h].unwrap()].page.unwrap());
                            counts.reads += 1;
                        } else {
                            counts.noop += 1;
                        }
                    }
                }
                assert_eq!(*buffer.hand.lock().unwrap(), hand);
                for (actual, model) in headers(buffer).into_iter().zip(&slots) {
                    let page = match actual.state {
                        State::Empty => None,
                        State::Clean(r) => Some(r.identity.page.page_no),
                        State::Loading(_) => panic!("synchronous operation retained Loading"),
                    };
                    assert_eq!(
                        (page, actual.pins, actual.referenced, actual.generation),
                        (model.page, model.count, model.recent, model.generation)
                    );
                }
                assert_eq!(data_reads(&state), counts.loads - initial_loads);
            }
            for (actual, expected) in handles.into_iter().zip(model_handles) {
                if let Some(i) = expected {
                    slots[i].count -= 1;
                    counts.releases += 1;
                }
                drop(actual);
            }
            assert!(headers(buffer).iter().all(|h| h.pins == 0));
        });
        clean(&writer);
    }
    assert_eq!(
        counts,
        Counts {
            success: 2487,
            busy: 1560,
            releases: 2487,
            reads: 787,
            noop: 2610,
            loads: 961,
            hits: 1526,
            visits: 8467
        }
    );
}

#[test]
fn read_buffer_workspace_can_only_be_reused_after_release() {
    let mut writer = source();
    let state = Arc::clone(&writer.0);
    let mut frames = vec![ReadFrame::new()];
    let mut root = [0; PAGE_BYTES];
    let mut scratch = [0; PAGE_BYTES];
    let mut load = [0; PAGE_BYTES];
    for generation in 1..=3 {
        let reader =
            PageFileReader::open(&mut writer, Some(ns()), &mut root, &mut scratch).unwrap();
        let buffer = ReadBuffer::new(reader, &mut frames, &mut load).unwrap();
        assert_eq!(headers(&buffer)[0].state, State::Empty);
        let a = pin(&buffer, 1);
        assert_eq!(a.generation, generation);
        full(&a, 1);
        drop(a);
        drop(buffer);
        clean(&writer);
    }
    assert_eq!(data_reads(&state), 3);
    for invalid in 0..3 {
        let mut frames = vec![ReadFrame::new()];
        match invalid {
            0 => frames[0].header.get_mut().unwrap().pins = 1,
            1 => {
                frames[0].header.get_mut().unwrap().state = State::Loading(Request {
                    identity: request(1),
                    kind: PageKind::RowLeaf,
                    level: 0,
                })
            }
            _ => std::thread::scope(|scope| {
                assert!(
                    scope
                        .spawn(|| {
                            let _held = frames[0].content.write().unwrap();
                            panic!("test-only constructor poison");
                        })
                        .join()
                        .is_err()
                );
            }),
        }
        let reader =
            PageFileReader::open(&mut writer, Some(ns()), &mut root, &mut scratch).unwrap();
        assert!(matches!(
            ReadBuffer::new(reader, &mut frames, &mut load),
            Err(BufferError::Poisoned)
        ));
        clean(&writer);
    }
}
