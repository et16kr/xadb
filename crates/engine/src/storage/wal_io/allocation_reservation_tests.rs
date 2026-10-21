//! Frozen AR01–AR08 integration inputs. Product codecs never generate the oracle.
use super::super::super::test_support::{self as port, Action, Plan as Fault, State, Writer};
use super::*;
use crate::storage::wal_frame::WalError;
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use xadb_foundation::codec::content_digest;

const INPUT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocation-reservation"
);
fn input(name: &str) -> Vec<u8> {
    std::fs::read(format!("{INPUT}/{name}")).unwrap()
}
fn literal(p: u32, n: usize, part: &str) -> Vec<u8> {
    input(&format!("p{p}-n{n}-{part}"))
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn reset(s: &Arc<Mutex<State>>) {
    let mut s = s.lock().unwrap();
    s.events.clear();
    s.read_events.clear();
    s.reads = 0;
    s.hit = 0;
    s.peak = s.handles;
}
fn files(s: &Arc<Mutex<State>>) -> BTreeMap<String, (u64, [u8; 32])> {
    s.lock()
        .unwrap()
        .files
        .iter()
        .map(|(n, e)| {
            assert!(e.sparse_tail.is_empty());
            (n.clone(), (e.length, content_digest(&e.bytes)))
        })
        .collect()
}
fn fixed_files(s: &Arc<Mutex<State>>) -> BTreeMap<String, (u64, [u8; 32])> {
    let mut all = files(s);
    all.retain(|n, _| !n.starts_with("d3-wal-"));
    all
}
fn wal_bytes(s: &Arc<Mutex<State>>) -> Vec<u8> {
    let s = s.lock().unwrap();
    let entries: Vec<_> = s
        .files
        .iter()
        .filter(|(n, _)| n.starts_with("d3-wal-"))
        .collect();
    assert_eq!(entries.len(), 1);
    entries[0].1.bytes.clone()
}
fn with_owner(
    p: u32,
    f: impl FnOnce(&mut AllocationSession<'_, '_, '_, '_, Writer>, &Arc<Mutex<State>>),
) {
    let mut slot = port::slot();
    let mut flush = boxed::<{ 4 << 20 }>();
    let mut frame = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<OPERATION_BYTES>();
    let (writer, state) = port::fresh();
    let fresh = super::super::super::FreshStorageSession::create(
        writer,
        p,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    reset(&state);
    let mut owner = installed
        .into_allocation_session(&mut frame, &mut operation)
        .unwrap();
    f(&mut owner, &state);
    drop(owner);
    let s = state.lock().unwrap();
    assert_eq!(s.handles, 0);
    assert_eq!(s.writers, 0);
}
fn seal_page(page: &mut [u8]) {
    let sha = content_digest(&page[..16352]);
    page[16352..].copy_from_slice(&sha);
}
fn seal_descriptor(d: &mut [u8]) {
    let sha = content_digest(&d[..1504]);
    d[1504..].copy_from_slice(&sha);
}
fn put64(b: &mut [u8], at: usize, n: u64) {
    b[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn get64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}
fn assert_fenced(owner: &mut AllocationSession<'_, '_, '_, '_, Writer>) {
    assert!(owner.is_fenced());
    assert_eq!(
        owner.observe().unwrap_err().effect,
        Effect::CallerMustResolve
    );
    assert_eq!(
        owner.reserved_page(0).unwrap_err().effect,
        Effect::CallerMustResolve
    );
    assert_eq!(
        owner.reserve_pages(1).unwrap_err().effect,
        Effect::CallerMustResolve
    );
    assert_eq!(
        owner.cancel_reserved().unwrap_err().effect,
        Effect::CallerMustResolve
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Fault boundaries immediately after the named phase.
pub(super) enum Point {
    Snapshot,
    Stage,
    Apply,
    Sync,
}
#[derive(Clone, Copy)]
pub(super) struct TerminalCapture {
    reserved_page_bytes: u64,
    protected_bytes: u64,
    complete_offset: u64,
    synced_offset: u64,
}
type CaptureSlot = Arc<Mutex<Option<TerminalCapture>>>;

#[derive(Clone)]
pub(super) enum Hook {
    Busy,
    Offset,
    Lsn,
    Missing,
    Poison,
    Panic(Point),
    Fence,
    CorruptAfterSync,
    Capture(CaptureSlot),
}
pub(super) fn stage_cut<W: WriterNamespace>(
    hook: Option<&Hook>,
    _: &wal::GenesisWalSession<'_, W>,
) {
    assert!(
        !matches!(hook, Some(Hook::Panic(Point::Stage))),
        "allocation AfterStage unwind"
    );
}
pub(super) fn cut<W: WriterNamespace>(
    state: &mut AllocationState<'_, '_>,
    point: Point,
    wal: &wal::GenesisWalSession<'_, W>,
) {
    let Some(hook) = state.hook.clone() else {
        return;
    };
    match (point, hook) {
        (p, Hook::Panic(wanted)) if p == wanted => panic!("allocation After{point:?} unwind"),
        (Point::Snapshot, Hook::Busy) => {
            let mut a = wal.append.lock().unwrap();
            a.state = wal::SlotState::Complete(wal::WalFrameLocation {
                stream: a.prefix.stream,
                file: a.prefix.file,
                previous_lsn: a.prefix.complete_lsn,
                lsn: a.prefix.complete_lsn + 1,
                start: a.prefix.complete_offset,
                length: 1,
                end: a.prefix.complete_offset + 1,
            });
        }
        (Point::Snapshot, Hook::Offset) => wal.append.lock().unwrap().prefix.complete_offset += 1,
        (Point::Snapshot, Hook::Lsn) => wal.append.lock().unwrap().prefix.complete_lsn += 1,
        (Point::Snapshot, Hook::Missing) => {
            wal.append.lock().unwrap().slot.take();
        }
        (Point::Snapshot, Hook::Poison) => {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                let _guard = wal.append.lock().unwrap();
                panic!("allocation append poison");
            }));
        }
        (Point::Sync, Hook::Fence) => wal.fenced.fence(),
        (Point::Sync, Hook::CorruptAfterSync) => state.descriptor[0] ^= 1,
        (Point::Apply, Hook::Capture(record)) => {
            assert!(state.terminal_pending);
            assert_eq!(state.rec_lsn, Some(PageLsn(1)));
            assert_eq!(state.descriptor[353], 4); // Finished
            for row in 0..5 {
                assert!(
                    state.descriptor[648 + 128 * row..760 + 128 * row]
                        .iter()
                        .all(|v| *v == 0)
                );
            }
            let prefix = wal.prefix().unwrap();
            let p = state.batch.unwrap().count() as u64 * PAGE_BYTES as u64;
            *record.lock().unwrap() = Some(TerminalCapture {
                reserved_page_bytes: p,
                protected_bytes: state.protection(prefix).unwrap(),
                complete_offset: prefix.complete_offset,
                synced_offset: prefix.synced_offset,
            });
        }
        _ => (),
    }
}

#[test]
fn allocation_ar01_constructor_resources_and_read_only() {
    for p in 1..=3 {
        let mut slot = port::slot();
        let mut flush = boxed::<{ 4 << 20 }>();
        let mut frame = boxed::<PAGE_BYTES>();
        let mut op = boxed::<OPERATION_BYTES>();
        let addresses = [
            slot.as_ptr() as usize,
            flush.as_ptr() as usize,
            frame.as_ptr() as usize,
            op.as_ptr() as usize,
        ];
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(addresses[i], addresses[j]);
            }
        }
        let (writer, state) = port::fresh();
        let fresh = super::super::super::FreshStorageSession::create(
            writer,
            p,
            &mut slot,
            &mut [[0; PAGE_BYTES]; 3],
        )
        .unwrap();
        let installed = fresh.install_doublewrite(&mut flush).unwrap();
        let before = files(&state);
        reset(&state);
        let owner = installed
            .into_allocation_session(&mut frame, &mut op)
            .unwrap();
        let view = owner.observe().unwrap();
        assert_eq!(
            (
                view.reserved,
                view.page_lsn.0,
                view.work_high_water,
                view.modification_counter
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(view.rec_lsn, None);
        assert_eq!(view.protected_bytes, 21024768);
        assert_eq!(
            (view.wal.complete_offset, view.wal.synced_offset),
            (4096, 4096)
        );
        assert_eq!(owner.reserved_page(0).unwrap(), None);
        assert_eq!(before, files(&state));
        let s = state.lock().unwrap();
        assert!(s.events.is_empty());
        assert!(
            s.read_events
                .iter()
                .any(|e| e.kind == "read" && e.length == PAGE_BYTES)
        );
        assert_eq!(s.peak, 3);
        assert!(s.peak <= 5);
        assert_eq!(s.handles, 1);
        drop(s);
        let owner_size = std::mem::size_of_val(&owner);
        assert!(owner_size <= 16384);
        assert!(std::mem::size_of::<operation::Plan>() <= 4096);
        eprintln!(
            "AR01 profile={p} owner={owner_size} file={} plan={} component_bound=65536",
            std::mem::size_of::<port::File>(),
            std::mem::size_of::<operation::Plan>()
        );
        drop(owner);
        assert_eq!(state.lock().unwrap().handles, 0);
    }
}

#[test]
fn allocation_ar02_exact_independent_sequences() {
    for p in 1..=3 {
        with_owner(p, |owner, state| {
            let fixed = fixed_files(state);
            let mut expected = wal_bytes(state);
            let mut lsn = 0;
            for (work, n) in [1, 8, 63].into_iter().enumerate() {
                for cancel in [false, true] {
                    let part = if cancel { "cancel" } else { "reserve" };
                    let view = if cancel {
                        owner.cancel_reserved()
                    } else {
                        owner.reserve_pages(n)
                    }
                    .unwrap();
                    lsn += 1;
                    expected.extend(literal(p, n, &format!("{part}.frame")));
                    assert_eq!(wal_bytes(state), expected, "profile{p} n{n} {part} WAL");
                    assert_eq!(
                        &owner.state.frame[..],
                        literal(p, n, &format!("{part}.page"))
                    );
                    assert_eq!(
                        &owner.state.descriptor[..],
                        literal(p, n, &format!("{part}.descriptor"))
                    );
                    assert_eq!(view.page_lsn, PageLsn(lsn));
                    assert_eq!(view.rec_lsn, Some(PageLsn(1)));
                    assert_eq!(view.modification_counter, lsn);
                    assert_eq!(view.work_high_water, work as u64 + 1);
                    assert_eq!(view.reserved, if cancel { 0 } else { n });
                    assert_eq!(view.wal.complete_offset, expected.len() as u64);
                    assert_eq!(view.wal.synced_offset, expected.len() as u64);
                    let tokens = literal(p, n, "items.bin");
                    let raw = literal(p, n, &format!("{part}.frame"));
                    for ordinal in 0..n {
                        let start = 14688 + 208 * ordinal + if cancel { 16 } else { 112 };
                        assert_eq!(
                            &raw[start..start + 96],
                            &tokens[96 * ordinal..96 * (ordinal + 1)]
                        );
                        if !cancel {
                            let reference = owner.reserved_page(ordinal).unwrap().unwrap();
                            let mut encoded = [0; 24];
                            reference.encode(&mut encoded).unwrap();
                            assert_eq!(&encoded, &tokens[96 * ordinal + 8..96 * ordinal + 32]);
                        }
                    }
                    assert_eq!(owner.reserved_page(n).unwrap(), None);
                    assert_eq!(fixed_files(state), fixed);
                }
            }
            assert_eq!(expected.len(), 160000);
            let page = allocation_page::decode_allocation_page(
                owner.state.frame,
                owner.state.identity,
                owner.installed.fresh.plan.profile.spec().namespace,
            )
            .unwrap();
            for i in 1..64 {
                let expected = if i == 1 {
                    4
                } else if i <= 8 {
                    3
                } else {
                    2
                };
                assert_eq!(
                    page.entry(i).unwrap(),
                    AllocationEntry::Free(Generation::new(expected).unwrap())
                );
            }
        });
    }
}

// Synthetic boundary state, never reported as a reachable native history.
fn finished_boundary(owner: &mut AllocationSession<'_, '_, '_, '_, Writer>, lsn: u64, high: u64) {
    assert!(owner.state.batch.is_none());
    let d = &mut owner.state.descriptor;
    put64(d, 56, high);
    put64(d, 72, lsn);
    put64(d, 80, lsn - 1);
    put64(d, 88, lsn - 1);
    put64(d, 544, lsn);
    seal_descriptor(d);
    put64(owner.state.frame, 64, lsn);
    seal_page(owner.state.frame);
    owner.state.work_high_water = high;
    let mut a = owner.installed.fresh.wal.append.lock().unwrap();
    a.prefix.complete_lsn = lsn;
    a.prefix.synced_lsn = lsn;
}
fn unchanged(
    owner: &mut AllocationSession<'_, '_, '_, '_, Writer>,
    state: &Arc<Mutex<State>>,
    count: Option<usize>,
    cause: &str,
) {
    let before = owner.observe().unwrap();
    let page = *owner.state.frame;
    let desc = owner.state.descriptor;
    let old_files = files(state);
    let e = if let Some(n) = count {
        owner.reserve_pages(n)
    } else {
        owner.cancel_reserved()
    }
    .unwrap_err();
    assert_eq!((e.phase, e.effect), (Phase::Preflight, Effect::Unchanged));
    assert_eq!(format!("{:?}", e.cause), cause);
    assert_eq!(owner.observe().unwrap(), before);
    assert_eq!(*owner.state.frame, page);
    assert_eq!(owner.state.descriptor, desc);
    assert_eq!(files(state), old_files);
}
#[test]
fn allocation_ar03_preflight_boundaries_preserve_authority() {
    with_owner(1, |o, s| {
        unchanged(o, s, Some(0), "InvalidInput");
        unchanged(o, s, Some(64), "InvalidInput");
        unchanged(o, s, None, "NoReservation");
        o.reserve_pages(1).unwrap();
        unchanged(o, s, Some(0), "InvalidInput");
        unchanged(o, s, Some(1), "ActiveReservation");
        o.cancel_reserved().unwrap();
        for high in [u64::MAX - 1, u64::MAX] {
            finished_boundary(o, 2, high);
            if high == u64::MAX {
                unchanged(o, s, Some(1), "CounterExhausted");
            } else {
                o.reserve_pages(1).unwrap();
                o.cancel_reserved().unwrap();
                assert_eq!(o.state.work_high_water, u64::MAX);
            }
        }
    });
    for available in [0, 7, 8] {
        with_owner(1, |o, s| {
            for i in available + 1..64 {
                put64(o.state.frame, 10208 + 96 * i + 8, u64::MAX);
            }
            seal_page(o.state.frame);
            if available < 8 {
                unchanged(o, s, Some(8), "Exhausted");
            } else {
                o.reserve_pages(8).unwrap();
                o.cancel_reserved().unwrap();
            }
        });
    }
    for lsn in [u64::MAX - 2, u64::MAX - 1, u64::MAX] {
        with_owner(1, |o, s| {
            o.reserve_pages(1).unwrap();
            o.cancel_reserved().unwrap();
            finished_boundary(o, lsn, 1);
            if lsn == u64::MAX - 2 {
                o.reserve_pages(1).unwrap();
                o.cancel_reserved().unwrap();
                assert_eq!(o.observe().unwrap().page_lsn.0, u64::MAX);
            } else {
                unchanged(o, s, Some(1), "CounterExhausted");
            }
        });
    }
    for counter in [u64::MAX - 2, u64::MAX - 1, u64::MAX] {
        with_owner(1, |o, s| {
            o.state.modification_counter = counter;
            if counter == u64::MAX - 2 {
                o.reserve_pages(1).unwrap();
                o.cancel_reserved().unwrap();
                assert_eq!(o.observe().unwrap().modification_counter, u64::MAX);
            } else {
                unchanged(o, s, Some(1), "CounterExhausted");
            }
        });
    }
    for extra in [-1i64, 0, 1] {
        with_owner(1, |o, s| {
            let offset = (wal::WAL_FILE_CAP as i64 - 42400 - extra) as u64;
            {
                let mut a = o.installed.fresh.wal.append.lock().unwrap();
                a.prefix.complete_offset = offset;
                a.prefix.synced_offset = offset;
            }
            if extra < 0 {
                unchanged(o, s, Some(1), "Capacity");
            } else {
                o.reserve_pages(1).unwrap();
                o.cancel_reserved().unwrap();
                assert_eq!(o.observe().unwrap().wal.complete_offset, offset + 42400);
            }
        });
    }
}

#[test]
fn allocation_ar04_frozen_semantic_negatives() {
    with_owner(1, |o, _| {
        o.reserve_pages(1).unwrap();
        o.cancel_reserved().unwrap();
        let prefix = o.observe().unwrap().wal;
        let profile = o.installed.fresh.plan.profile;
        let batch = Batch {
            owner: SystemWorkRef::new(
                profile.spec().namespace,
                SystemWorkKind::AllocationMaintenance,
                2,
                Generation::new(1).unwrap(),
            )
            .unwrap(),
            mask: 510,
            birth: 3,
        };
        let reserve_before = *o.state.frame;
        let reserve_after: [u8; PAGE_BYTES] = literal(1, 8, "reserve.page").try_into().unwrap();
        let before_desc = literal(1, 8, "reserve.descriptor");
        let rows = String::from_utf8(input("semantic-negatives.tsv")).unwrap();
        let mut seen = 0;
        for row in rows.lines() {
            let columns: Vec<_> = row.split('\t').collect();
            assert_eq!(columns.len(), 5);
            let focused = std::env::var_os("XADB_ALLOC_OWNER_TAG_REVIEW").is_some();
            if focused
                && columns[0] != "group_owner_tag"
                && !(225..232).any(|n| columns[0] == format!("reserved_{n}"))
            {
                continue;
            }
            let cancel = columns[1].contains("cancel");
            let mut bytes = input(columns[1]);
            for action in columns[4].split(';') {
                let v: Vec<_> = action.split(':').collect();
                let x: usize = v[1].parse().unwrap();
                let y: usize = v[2].parse().unwrap();
                let z: u64 = v[3].parse().unwrap();
                match v[0] {
                    "w" => bytes[x..x + y].copy_from_slice(&z.to_le_bytes()[..y]),
                    "c" => bytes.copy_within(x..x + z as usize, y),
                    _ => panic!("unknown recipe"),
                }
            }
            for seal in columns[3].split(',') {
                match seal {
                    "descriptor" => {
                        let sha = content_digest(&bytes[352..1856]);
                        bytes[1856..1888].copy_from_slice(&sha);
                    }
                    "native" => {
                        let old = content_digest(&bytes[2272..8480]);
                        let new = content_digest(&bytes[8480..14688]);
                        bytes[2168..2200].copy_from_slice(&old);
                        bytes[2200..2232].copy_from_slice(&new);
                    }
                    "outer" => port::seal_frame(&mut bytes),
                    _ => panic!("unknown reseal"),
                }
            }
            let mut after: [u8; PAGE_BYTES] = literal(
                1,
                8,
                if cancel {
                    "cancel.page"
                } else {
                    "reserve.page"
                },
            )
            .try_into()
            .unwrap();
            // Derive actual after-state from mutated Patch, including coherent native+Patch attacks.
            after[10144..16352].copy_from_slice(&bytes[16416..22624]);
            seal_page(&mut after);
            let mut operation_prefix = prefix;
            if cancel {
                operation_prefix.complete_lsn = 3;
                operation_prefix.synced_lsn = 3;
                operation_prefix.complete_offset += 22656;
                operation_prefix.synced_offset += 22656;
            }
            let plan = operation::Plan {
                profile,
                identity: o.state.identity,
                prefix: operation_prefix,
                batch,
                cancel,
                before_descriptor: if cancel { Some(&before_desc) } else { None },
            };
            let result = operation::validate(
                plan,
                if cancel {
                    &reserve_after
                } else {
                    &reserve_before
                },
                &bytes,
                &after,
            );
            let error = result.expect_err(columns[0]);
            assert_eq!(format!("{error:?}"), columns[2], "{}", columns[0]);
            seen += 1;
        }
        // D1 counterexamples: preserve shape precedence and typed OwnerRef errors.
        for variant in 0..5 {
            if std::env::var_os("XADB_ALLOC_OWNER_TAG_REVIEW").is_some() && variant != 4 {
                continue;
            }
            let mut bytes = literal(1, 8, "reserve.frame");
            match variant {
                0 => {
                    bytes[170] = 1;
                    bytes[2144] = 1;
                }
                1 => bytes[192] = 0,
                2 => bytes[192] = 3,
                3 => {
                    bytes[192] = 1;
                    bytes[216..224].fill(0);
                }
                4 => {
                    bytes[192] = 1;
                    bytes[224..228].copy_from_slice(&256u32.to_le_bytes());
                }
                _ => unreachable!(),
            }
            port::seal_frame(&mut bytes);
            let plan = operation::Plan {
                profile,
                identity: o.state.identity,
                prefix,
                batch,
                cancel: false,
                before_descriptor: None,
            };
            assert_eq!(
                operation::validate(plan, &reserve_before, &bytes, &reserve_after),
                Err(if variant == 4 {
                    SemanticError::IdentityMismatch
                } else {
                    SemanticError::Corrupt
                }),
                "D1 variant{variant}"
            );
        }
        assert_eq!(
            seen,
            if std::env::var_os("XADB_ALLOC_OWNER_TAG_REVIEW").is_some() {
                8
            } else {
                541
            }
        );
        eprintln!("AR04 exact semantic rejections={seen}");
    });
}

#[test]
fn allocation_ar05_stage_cuts_and_fencing() {
    for hook in [
        Hook::Busy,
        Hook::Offset,
        Hook::Lsn,
        Hook::Missing,
        Hook::Poison,
    ] {
        with_owner(1, |o, s| {
            let old_page = *o.state.frame;
            let old_desc = o.state.descriptor;
            let old_files = files(s);
            o.state.hook = Some(hook.clone());
            let e = o.reserve_pages(8).unwrap_err();
            assert_eq!(e.phase, Phase::Stage);
            match hook {
                Hook::Busy => {
                    assert_eq!(e.effect, Effect::Unchanged);
                    assert!(matches!(
                        e.cause,
                        Cause::Wal(wal::WalIoError {
                            cause: wal::Cause::Busy,
                            ..
                        })
                    ));
                }
                Hook::Offset | Hook::Lsn => {
                    assert_eq!(e.effect, Effect::Unchanged);
                    assert!(matches!(
                        e.cause,
                        Cause::Wal(wal::WalIoError {
                            cause: wal::Cause::Frame(WalError::OrderMismatch),
                            ..
                        })
                    ));
                }
                _ => {
                    assert_eq!(e.effect, Effect::CallerMustResolve);
                    let Cause::Wal(lower) = e.cause else {
                        panic!("missing original stage error")
                    };
                    if matches!(hook, Hook::Poison) {
                        // The existing WAL state() observes poison through is_fenced().
                        assert!(o.installed.fresh.wal.append.is_poisoned());
                        assert!(matches!(lower.cause, wal::Cause::Fenced));
                    } else {
                        assert!(matches!(lower.cause, wal::Cause::Poisoned));
                    }
                    assert_fenced(o);
                }
            }
            assert_eq!(*o.state.frame, old_page);
            assert_eq!(o.state.descriptor, old_desc);
            assert_eq!(files(s), old_files);
        });
    }
    for point in [Point::Stage, Point::Apply] {
        for cancel in [false, true] {
            with_owner(1, |o, s| {
                if cancel {
                    o.reserve_pages(8).unwrap();
                }
                let before = o.observe().unwrap();
                let old_files = files(s);
                let old_page = *o.state.frame;
                let old_descriptor = o.state.descriptor;
                o.state.hook = Some(Hook::Panic(point));
                assert!(
                    catch_unwind(AssertUnwindSafe(|| if cancel {
                        o.cancel_reserved()
                    } else {
                        o.reserve_pages(8)
                    }))
                    .is_err()
                );
                assert_fenced(o);
                assert_eq!(files(s), old_files);
                let prefix = o.installed.fresh.wal.append.lock().unwrap().prefix;
                assert_eq!(prefix.complete_lsn, before.page_lsn.0 + 1);
                assert_eq!(prefix.synced_lsn, before.page_lsn.0);
                assert_eq!(o.state.batch.unwrap().count(), 8);
                assert_eq!(o.state.terminal_pending, cancel);
                assert_eq!(o.state.work_high_water, 1);
                assert_eq!(o.state.rec_lsn, Some(PageLsn(1)));
                assert_eq!(
                    o.state.protection(prefix),
                    Some(21024768 + 2 * 22656 + 9 * 16384)
                );
                if point == Point::Stage {
                    assert_eq!(*o.state.frame, old_page);
                    assert_eq!(o.state.descriptor, old_descriptor);
                }
                assert!(std::mem::size_of_val(o) <= 16384);
            });
        }
    }
    for hook in [Hook::Fence, Hook::CorruptAfterSync] {
        with_owner(1, |o, _| {
            o.reserve_pages(8).unwrap();
            o.state.hook = Some(hook.clone());
            let e = o.cancel_reserved().unwrap_err();
            if matches!(hook, Hook::Fence) {
                assert_eq!(e.phase, Phase::Sync);
                assert!(matches!(e.cause, Cause::Fenced));
            } else {
                assert_eq!(e.phase, Phase::Observe);
                assert!(matches!(e.cause, Cause::Semantic(SemanticError::Corrupt)));
            }
            assert_eq!(e.effect, Effect::CallerMustResolve);
            assert!(o.state.terminal_pending);
            assert_eq!(o.state.batch.unwrap().count(), 8);
            assert_fenced(o);
        });
    }
}

#[test]
fn allocation_ar06_write_sync_faults() {
    for cancel in [false, true] {
        for at in [0, 1] {
            let mut actions = vec![
                Action::BeforeError,
                Action::AfterError,
                Action::BeforePanic,
                Action::AfterPanic,
            ];
            if at == 0 {
                actions.extend([
                    Action::PrefixError,
                    Action::Zero,
                    Action::InvalidCount,
                    Action::Short(1),
                    Action::Interrupted,
                ]);
            }
            for action in actions {
                if std::env::var_os("XADB_ALLOC_ERROR_REVIEW").is_some()
                    && matches!(
                        action,
                        Action::Short(_)
                            | Action::Interrupted
                            | Action::BeforePanic
                            | Action::AfterPanic
                    )
                {
                    continue;
                }
                with_owner(1, |o, s| {
                    // N8 literal follows one completed N1 cycle.
                    o.reserve_pages(1).unwrap();
                    o.cancel_reserved().unwrap();
                    if cancel {
                        o.reserve_pages(8).unwrap();
                    }
                    let old = wal_bytes(s);
                    let fixed = fixed_files(s);
                    let before = o.observe().unwrap();
                    reset(s);
                    s.lock().unwrap().plan = Some(Fault { at, action });
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        if cancel {
                            o.cancel_reserved()
                        } else {
                            o.reserve_pages(8)
                        }
                    }));
                    let success = matches!(action, Action::Short(_) | Action::Interrupted);
                    if success {
                        assert!(outcome.unwrap().is_ok());
                        assert!(!o.is_fenced());
                    } else if matches!(action, Action::BeforePanic | Action::AfterPanic) {
                        assert!(outcome.is_err());
                        assert_fenced(o);
                    } else {
                        let e = outcome.unwrap().unwrap_err();
                        assert_eq!(
                            (e.phase, e.effect),
                            (Phase::Sync, Effect::CallerMustResolve)
                        );
                        let Cause::Wal(lower) = e.cause else {
                            panic!("missing lower WAL error")
                        };
                        assert_eq!(
                            lower.phase,
                            if at == 0 {
                                wal::Phase::Write
                            } else {
                                wal::Phase::Sync
                            }
                        );
                        assert_eq!(lower.effect, Effect::CallerMustResolve);
                        let wal::Cause::Io(io) = lower.cause else {
                            panic!("missing original I/O")
                        };
                        assert_eq!(io.code, xadb_foundation::error::ErrorCode::IoWriteUnknown);
                        assert_eq!(io.effect(), Effect::CallerMustResolve);
                        assert_eq!(
                            io.io_kind,
                            match action {
                                Action::Zero => Some(std::io::ErrorKind::WriteZero),
                                Action::InvalidCount => None,
                                _ => Some(std::io::ErrorKind::Other),
                            }
                        );
                        assert_eq!(
                            io.context,
                            if at == 1 {
                                "sync outcome unknown"
                            } else {
                                match action {
                                    Action::Zero => "zero-length write",
                                    Action::InvalidCount => "invalid write count from adapter",
                                    _ => "write outcome requires caller resolution",
                                }
                            }
                        );
                        assert_fenced(o);
                    }
                    assert_eq!(s.lock().unwrap().hit, 1);
                    let bytes = literal(
                        1,
                        8,
                        if cancel {
                            "cancel.frame"
                        } else {
                            "reserve.frame"
                        },
                    );
                    let written = if at == 1
                        || success
                        || matches!(action, Action::AfterError | Action::AfterPanic)
                    {
                        22656
                    } else if action == Action::PrefixError {
                        11328
                    } else {
                        0
                    };
                    let mut expected = old;
                    expected.extend_from_slice(&bytes[..written]);
                    assert_eq!(
                        wal_bytes(s),
                        expected,
                        "cancel={cancel} at={at} action={action:?}"
                    );
                    assert_eq!(fixed_files(s), fixed);
                    if !success {
                        assert_eq!(o.state.batch.unwrap().count(), 8);
                        assert_eq!(o.state.terminal_pending, cancel);
                        assert_eq!(o.state.rec_lsn, Some(PageLsn(1)));
                        let a = o
                            .installed
                            .fresh
                            .wal
                            .append
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        assert_eq!(a.prefix.synced_lsn, before.page_lsn.0);
                        assert_eq!(a.prefix.complete_lsn, before.page_lsn.0 + 1);
                        assert!(matches!(a.state, wal::SlotState::IoOwned(_)));
                        assert!(a.slot.is_none());
                    }
                });
            }
        }
    }
}

#[test]
fn allocation_ar07_terminal_barrier_and_retained_protection() {
    with_owner(1, |o, s| {
        let first = o.reserve_pages(8).unwrap();
        assert_eq!(first.protected_bytes, 21024768 + 2 * 22656 + 9 * 16384);
        reset(s);
        let record = Arc::new(Mutex::new(None));
        o.state.hook = Some(Hook::Capture(record.clone()));
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        s.lock().unwrap().pause = Some(port::Pause {
            at: 1,
            after: false,
            entered: entered_tx,
            release: release_rx,
            unwind: false,
        });
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let result = o.cancel_reserved();
                done_tx.send(()).unwrap();
                result
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let captured = record.lock().unwrap().unwrap();
            assert_eq!(captured.reserved_page_bytes, 8 * 16384);
            assert_eq!(captured.protected_bytes, first.protected_bytes);
            assert_eq!(
                (captured.complete_offset, captured.synced_offset),
                (49408, 26752)
            );
            assert!(done_rx.try_recv().is_err());
            release_tx.send(()).unwrap();
            let view = handle.join().unwrap().unwrap();
            assert_eq!(view.protected_bytes, 21024768 + 2 * 22656 + 16384);
            assert_eq!(view.rec_lsn, Some(PageLsn(1)));
            assert_eq!(view.work_high_water, 1);
        });
        o.state.hook = None;
        let next = o.reserve_pages(1).unwrap();
        assert_eq!(next.work_high_water, 2);
        assert_eq!(next.rec_lsn, Some(PageLsn(1)));
        assert_eq!(o.reserved_page(0).unwrap().unwrap().incarnation.get(), 3);
    });
}

#[test]
fn allocation_ar08_constructor_rejections() {
    for variant in 0..9 {
        let mut slot = port::slot();
        let mut flush = boxed::<{ 4 << 20 }>();
        let mut frame = boxed::<PAGE_BYTES>();
        let mut op = boxed::<OPERATION_BYTES>();
        let (writer, s) = port::fresh();
        let fresh = super::super::super::FreshStorageSession::create(
            writer,
            1,
            &mut slot,
            &mut [[0; PAGE_BYTES]; 3],
        )
        .unwrap();
        let installed = fresh.install_doublewrite(&mut flush).unwrap();
        reset(&s);
        match variant {
            0 => installed.fresh.wal.fenced.fence(),
            1 => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    let _lock = installed.fresh.wal.append.lock().unwrap();
                    panic!("constructor poison");
                }));
            }
            2 => {
                installed
                    .fresh
                    .wal
                    .append
                    .lock()
                    .unwrap()
                    .prefix
                    .complete_lsn = 1
            }
            3 => installed.fresh.reader_active.store(true, Ordering::Release),
            4..=7 => {
                let mut state = s.lock().unwrap();
                let name = state
                    .files
                    .iter()
                    .find(|(_, e)| e.length == 1048576)
                    .unwrap()
                    .0
                    .clone();
                let e = state.files.get_mut(&name).unwrap();
                match variant {
                    4 => put64(&mut e.bytes[..PAGE_BYTES], 48, 99),
                    5 => put64(&mut e.bytes[..PAGE_BYTES], 64, 1),
                    6 => put64(&mut e.bytes[..PAGE_BYTES], 10312, 2),
                    7 => e.length -= 1,
                    _ => unreachable!(),
                }
                if variant != 7 {
                    seal_page(&mut e.bytes[..PAGE_BYTES]);
                }
            }
            8 => {
                s.lock().unwrap().read_plan = Some(Fault {
                    at: 0,
                    action: Action::BeforeError,
                })
            }
            _ => unreachable!(),
        }
        let before = files(&s);
        let error = match installed.into_allocation_session(&mut frame, &mut op) {
            Ok(_) => panic!("accepted constructor variant{variant}"),
            Err(e) => e,
        };
        assert_eq!(
            error.effect,
            Effect::CallerMustResolve,
            "variant{variant}: {error:?}"
        );
        assert_eq!(
            error.phase,
            if matches!(variant, 4 | 7 | 8) {
                Phase::Read
            } else {
                Phase::Constructor
            }
        );
        assert!(
            match (&error.cause, variant) {
                (Cause::Fenced, 0) | (Cause::Poisoned, 1) | (Cause::Invariant, 2 | 3 | 5 | 6) =>
                    true,
                (
                    Cause::PageIo(PageIoError::Page(
                        crate::storage::page::PageError::IdentityMismatch,
                    )),
                    4,
                ) => true,
                (Cause::PageIo(PageIoError::LengthMismatch), 7) => true,
                (Cause::PageIo(PageIoError::Namespace(e)), 8) => e.effect == Effect::Unchanged,
                _ => false,
            },
            "variant{variant}: {error:?}"
        );
        if variant <= 3 {
            assert_eq!(s.lock().unwrap().reads, 0);
        }
        assert_eq!(files(&s), before);
        let state = s.lock().unwrap();
        assert!(state.events.is_empty());
        assert_eq!(state.handles, 0);
        assert_eq!(state.writers, 0);
    }
}

#[test]
fn allocation_constructor_new_owner_size_boundary() {
    use super::super::super::FreshStorageSession;
    const PAD: usize =
        16384 - std::mem::size_of::<DoublewriteStorageSession<'static, 'static, Writer>>() - 64;
    type Large = port::SizedWriter<PAD, 0>;
    assert!(std::mem::size_of::<DoublewriteStorageSession<'_, '_, Large>>() <= 16384);
    assert!(std::mem::size_of::<AllocationSession<'_, '_, '_, '_, Large>>() > 16384);
    // A File this large already exceeds the retained upstream owner's cap.
    // Do not execute unchanged upstream constructor tests to repeat that rejection.
    assert!(std::mem::size_of::<FreshStorageSession<'_, port::SizedWriter<0, 16384>>>() > 16384);
    let (writer, s) = port::fresh();
    let mut slot = port::slot();
    let mut flush = boxed::<{ 4 << 20 }>();
    let mut frame = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<OPERATION_BYTES>();
    let fresh = FreshStorageSession::create(
        Large {
            inner: writer,
            padding: [0; PAD],
        },
        1,
        &mut slot,
        &mut [[0; PAGE_BYTES]; 3],
    )
    .unwrap();
    let installed = fresh.install_doublewrite(&mut flush).unwrap();
    let before = files(&s);
    reset(&s);
    let e = installed
        .into_allocation_session(&mut frame, &mut operation)
        .err()
        .unwrap();
    assert_eq!(
        (e.phase, e.effect),
        (Phase::Constructor, Effect::CallerMustResolve)
    );
    assert!(matches!(e.cause, Cause::Capacity));
    assert_eq!(files(&s), before);
    let state = s.lock().unwrap();
    assert!(state.events.is_empty());
    assert!(state.read_events.is_empty());
    assert_eq!(state.handles, 0);
    assert_eq!(state.writers, 0);
}
