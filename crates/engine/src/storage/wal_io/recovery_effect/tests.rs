//! Frozen producer bytes and authored operation vectors; no producer rerun.
use super::bootstrap::test_support as port;
use super::*;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
const GENESIS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/dev3-storage-session/literal"
);
const DWB: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-doublewrite-install/literal"
);
const STATUS_BOOT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-bootstrap"
);
const EXCLUSIVE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocation-reservation"
);
const DIRTY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-dirty-allocation"
);
const INIT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-status-init"
);
const FLUSH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/dev3-allocator-flush-one"
);
type State = Arc<Mutex<port::State>>;
struct Directory(State);
impl DirectoryPort for Directory {
    type Writer = port::Writer;
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        port::acquire(&self.0)
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn read(root: &str, path: &str) -> Vec<u8> {
    std::fs::read(format!("{root}/{path}")).unwrap()
}
fn literal(root: &str, profile: u32, path: &str) -> Vec<u8> {
    read(root, &format!("profile{profile}-{path}.bin"))
}
fn vector(root: &str, profile: u32, count: usize, part: &str) -> Vec<u8> {
    read(root, &format!("p{profile}-n{count}-{part}"))
}
fn status_vector(profile: u32, cycles: u64, terminal: &str, work: u64, part: &str) -> Vec<u8> {
    read(
        INIT,
        &format!("p{profile}-cancel{cycles}-{terminal}/work{work}-{part}"),
    )
}
fn reset(state: &State) {
    let mut s = state.lock().unwrap();
    s.events.clear();
    s.read_events.clear();
    s.read_effect_counts.clear();
    s.reads = 0;
    s.hit = 0;
    s.peak = s.handles;
    s.open_modes.clear();
}
fn fixture(profile: u32, plan: u64, frames: &[u8]) -> Directory {
    let (writer, state) = port::fresh();
    drop(writer);
    {
        let mut s = state.lock().unwrap();
        let mut put = |path: &str, bytes: Vec<u8>, length: u64| {
            let mut entry = port::Entry::new(100 + s.files.len() as u64);
            entry.bytes = bytes;
            entry.length = length;
            s.files.insert(path.into(), entry);
        };
        for path in ["d3-profile", "WRITER", "FILEMAP"] {
            put(path, literal(GENESIS, profile, path), 4096);
        }
        let mut wal = literal(GENESIS, profile, "d3-wal-00000001");
        wal.extend_from_slice(frames);
        let length = wal.len() as u64;
        put(wal::WAL_NAME, wal, length);
        put(
            "d3-bootstrap",
            literal(
                if plan == 1 { GENESIS } else { STATUS_BOOT },
                profile,
                "d3-bootstrap",
            ),
            4096,
        );
        let mut map = vec![0; PAGE_BYTES + 256];
        map[..320].copy_from_slice(&literal(GENESIS, profile, "root"));
        map[PAGE_BYTES..].copy_from_slice(&literal(GENESIS, profile, "chunk"));
        put("d3-filemap-0", map, control::CONTAINER_BYTES);
        put("d3-filemap-1", vec![0xa5; 17], control::CONTAINER_BYTES);
        put(
            "d3-data-0000000100000001",
            {
                let mut home = literal(GENESIS, profile, "allocation");
                home.resize(2 * PAGE_BYTES, 0);
                home
            },
            control::EXTENT_BYTES,
        );
        put("DWB", literal(DWB, profile, "DWB"), 4096);
        put(
            "d3-dwb-0",
            {
                let mut bank = literal(DWB, profile, "empty");
                bank.resize(PAGE_BYTES, 0);
                bank
            },
            PAGE_BYTES as u64,
        );
        put("d3-dwb-1", vec![0xa5; 17], PAGE_BYTES as u64);
        put("d3-dwb-stage", vec![0xa5; 17], PAGE_BYTES as u64);
        put("d3-dwb-images", vec![0xa5; 17], 16_711_680);
        put("d3-dwb-reserved", vec![0xa5; 17], 8192);
    }
    reset(&state);
    Directory(state)
}
fn pair(profile: u32, mode: u64, count: usize, cancel: bool) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let root = if mode == 81920 { EXCLUSIVE } else { DIRTY };
    let mut frames = vector(root, profile, count, "reserve.frame");
    if count > 1 {
        // These literal families were captured from one actual producer run:
        // count1 work1@1/2, count8 work2@3/4, count63 work3@5/6. Preserve
        // every predecessor byte instead of rebasing isolated identities.
        let mut predecessors = Vec::new();
        for prior in [1, 8] {
            if prior < count {
                predecessors.extend_from_slice(&vector(root, profile, prior, "reserve.frame"));
                predecessors.extend_from_slice(&vector(root, profile, prior, "cancel.frame"));
            }
        }
        predecessors.extend_from_slice(&frames);
        frames = predecessors;
    }
    if cancel {
        frames.extend_from_slice(&vector(root, profile, count, "cancel.frame"));
    }
    let terminal = if cancel { "cancel" } else { "reserve" };
    (
        frames,
        vector(root, profile, count, &format!("{terminal}.page")),
        vector(root, profile, count, &format!("{terminal}.descriptor")),
    )
}
fn status_case(
    profile: u32,
    cycles: u64,
    terminal: &str,
) -> (Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>) {
    let work = cycles + 1;
    let mut frames = read(
        INIT,
        &format!("p{profile}-cancel{cycles}-{terminal}/wal.frames-only"),
    );
    let (allocator, descriptor, status) = if terminal == "init" {
        (
            status_vector(profile, cycles, terminal, work, "allocator.after"),
            status_vector(profile, cycles, terminal, work, "init.descriptor"),
            Some(status_vector(
                profile,
                cycles,
                terminal,
                work,
                "status.after",
            )),
        )
    } else {
        (
            status_vector(profile, cycles, terminal, work, "allocator.after"),
            status_vector(profile, cycles, terminal, work, "cancel.descriptor"),
            None,
        )
    };
    frames.shrink_to_fit();
    (frames, allocator, descriptor, status)
}
fn mark(prefix: &str, count: usize, mut run: impl FnMut(usize)) {
    let ids: Vec<_> = (1..=count).map(|n| format!("{prefix}{n:03}")).collect();
    let filter = match std::env::var("XADB_RECOVERY_EFFECTS_CASES") {
        Ok(raw) => {
            assert!(!raw.is_empty());
            let mut set = BTreeSet::new();
            for id in raw.split(',') {
                assert!(ids.iter().any(|v| v == id), "unknown case {id}");
                assert!(set.insert(id.to_owned()), "duplicate case {id}");
            }
            Some(set)
        }
        Err(std::env::VarError::NotPresent) => None,
        Err(e) => panic!("invalid selection {e}"),
    };
    let mut done = 0;
    for (n, id) in ids.iter().enumerate() {
        if filter.as_ref().is_some_and(|f| !f.contains(id)) {
            continue;
        }
        println!("RECOVERY_EFFECTS_CASE_BEGIN\t{id}");
        run(n);
        println!("RECOVERY_EFFECTS_CASE\t{id}\tPASS");
        done += 1;
    }
    assert!(done > 0);
    println!("RECOVERY_EFFECTS_COMPLETE count={done}");
}
fn install(directory: &Directory, path: &str, bytes: Vec<u8>, length: u64) {
    let mut state = directory.0.lock().unwrap();
    let inode = 200 + state.files.len() as u64;
    let mut e = port::Entry::new(inode);
    e.bytes = bytes;
    e.length = length;
    state.files.insert(path.into(), e);
}
fn set_home(directory: &Directory, bytes: &[u8], index: usize) {
    let mut s = directory.0.lock().unwrap();
    let home = &mut s.files.get_mut(HOME_NAME).unwrap().bytes;
    home[index * PAGE_BYTES..(index + 1) * PAGE_BYTES].copy_from_slice(bytes);
}
fn success(
    directory: &Directory,
    allocator: &[u8],
    descriptor: Option<&[u8]>,
    status: Option<&[u8]>,
    dirty: u64,
) {
    let mut m = boxed();
    let mut f = boxed();
    let owner =
        ClosedRecovery::acquire(directory, Some(port::namespace()), &mut m, &mut f).unwrap();
    assert!(!owner.observation().source_stabilized);
    assert_eq!(owner.effects().mutation_attempted, 0);
    let result = owner.recover().unwrap();
    let o = result.observation();
    assert_eq!(result.allocator_image().as_slice(), allocator);
    assert_eq!(result.status_image().map(|v| v.as_slice()), status);
    assert_eq!(result.descriptor_bytes().map(|v| v.as_slice()), descriptor);
    assert!(o.source_stabilized && o.boot.slots_provisioned && o.ledger.restored_once);
    assert_eq!(o.physical.dirty_mask, dirty);
    assert_eq!(o.physical.flushed_mask, dirty);
    assert_eq!(o.physical.canonical_passes, 1);
    assert_eq!(o.effects.slot_chunks_done, [16, 16]);
    assert_eq!(o.effects.slot_synced, 3);
    assert!(o.effects.boot_directory_synced);
    assert_eq!(
        o.ledger.replay_topup,
        result.prefix().replay_page_topup_bytes()
    );
    assert_eq!(
        o.ledger.actual_owned + o.ledger.outstanding,
        o.ledger.owned_peak
    );
    assert_eq!(
        o.ledger.owned_peak,
        24_182_784 + result.prefix().retained_wal_bytes() + result.prefix().remaining_wal()
    );
    assert_eq!(
        o.ledger.protected,
        23_134_208
            + result.prefix().retained_wal_bytes()
            + result.prefix().remaining_protected()
            + result.prefix().replay_page_topup_bytes()
    );
    assert_eq!(o.ledger.slot_growth, 0);
    assert_eq!(o.fd_peak, 5);
    assert!(o.source_census.peak <= 65536);
    assert_eq!(o.possible, [Some(o.known), None]);
    assert_eq!(o.state, ControlState::Empty);
    drop(result);
    let s = directory.0.lock().unwrap();
    assert_eq!(s.handles, 0);
    assert_eq!(s.writers, 0);
}
fn acquire_error(directory: &Directory) -> RecoveryAcquireError {
    let mut m = boxed();
    let mut f = boxed();
    match ClosedRecovery::acquire(directory, Some(port::namespace()), &mut m, &mut f) {
        Err(e) => e,
        Ok(_) => panic!("expected acquisition failure"),
    }
}
fn recovered_error(
    directory: &Directory,
    check: impl FnOnce(&RecoveryFailure<'_, '_, port::Writer>),
) {
    let mut m = boxed();
    let mut f = boxed();
    let owner =
        ClosedRecovery::acquire(directory, Some(port::namespace()), &mut m, &mut f).unwrap();
    let failure = match owner.recover() {
        Err(e) => e,
        Ok(_) => panic!("expected effect failure"),
    };
    assert_eq!(failure.error().effect, Effect::CallerMustResolve);
    assert_eq!(failure.error().lease_effect, LeaseEffect::MayHaveChanged);
    check(&failure);
    assert_eq!(directory.0.lock().unwrap().writers, 1);
    drop(failure);
    assert_eq!(directory.0.lock().unwrap().writers, 0);
}
fn install_pending(
    directory: &Directory,
    profile: u32,
    image: &[u8],
    kind: PageKind,
    lsn: u64,
    batch: u64,
    state: ControlState,
) {
    let p = control::decode_bootstrap_profile(
        &literal(GENESIS, profile, "d3-profile"),
        Some(port::namespace()),
    )
    .unwrap();
    let identity =
        super::physical::identity_of(image.try_into().unwrap(), p.spec().namespace.database)
            .unwrap();
    let generation = 3 * batch + if state == ControlState::Pending { 0 } else { 1 };
    let entry = PendingImage {
        slot: 0,
        page: identity.page,
        owner: identity.owner,
        owner_generation: identity.owner_generation,
        kind,
        level: 0,
        lsn: crate::storage::page::PageLsn(lsn),
        image_digest: content_digest(image),
    };
    let entries = if state == ControlState::Pending {
        Some(entry)
    } else {
        None
    };
    let mut bank = vec![0; PAGE_BYTES];
    let len = dwb::encode_control(
        ControlHeader {
            namespace: p.spec().namespace,
            generation,
            batch_id: batch,
            state,
            max_lsn: if entries.is_some() {
                entry.lsn
            } else {
                crate::storage::page::PageLsn(0)
            },
        },
        entries.as_slice(),
        &mut bank,
    )
    .unwrap()
    .len();
    if state == ControlState::Pending {
        let view = dwb::decode_control(
            &bank[..len],
            dwb::ControlExpectation {
                namespace: p.spec().namespace,
                generation,
            },
        )
        .unwrap();
        let mut slot = vec![0; dwb::SLOT_BYTES];
        dwb::encode_slot(&view, 0, image, &mut slot).unwrap();
        install(directory, "d3-dwb-images", slot, 16_711_680);
    }
    let selected = SelectedControl {
        generation,
        reference: ControlRef {
            kind: ControlKind::Doublewrite,
            slot: ((generation - 1) % 2) as u8,
            index: 0,
            generation,
            encoded_len: len as u32,
            digest: content_digest(&bank[..len]),
        },
    };
    let path = if selected.reference.slot == 0 {
        "d3-dwb-0"
    } else {
        "d3-dwb-1"
    };
    install(directory, path, bank, PAGE_BYTES as u64);
    let mut raw = vec![0; 4096];
    control::encode_selector(SelectorRole::Doublewrite, selected, p, &mut raw).unwrap();
    install(directory, "DWB", raw, 4096);
}
#[test]
fn producer_histories_and_closed_ledger() {
    mark("ES", 24, |n| {
        let profile = (n / 8 + 1) as u32;
        match n % 8 {
            0 => {
                let d = fixture(profile, 1, &[]);
                success(&d, &literal(GENESIS, profile, "allocation"), None, None, 0);
            }
            k @ 1..=4 => {
                let mode = if k < 3 { 81920 } else { 98304 };
                let cancel = k % 2 == 0;
                let (frames, page, desc) =
                    pair(profile, mode, if mode == 98304 { 63 } else { 1 }, cancel);
                let d = fixture(profile, 1, &frames);
                success(&d, &page, Some(&desc), None, 1);
            }
            5 => {
                let frames = status_vector(profile, 0, "init", 1, "reserve.wal");
                let page = status_vector(profile, 0, "init", 1, "allocator.reserved");
                let desc = status_vector(profile, 0, "init", 1, "reserve.descriptor");
                let d = fixture(profile, 2, &frames);
                success(&d, &page, Some(&desc), None, 1);
            }
            6 | 7 => {
                let (frames, page, desc, status) =
                    status_case(profile, 0, if n % 8 == 6 { "cancel" } else { "init" });
                let d = fixture(profile, 2, &frames);
                success(
                    &d,
                    &page,
                    Some(&desc),
                    status.as_deref(),
                    if status.is_some() { 3 } else { 1 },
                );
            }
            _ => unreachable!(),
        }
    });
}
#[test]
fn independent_page_skip_cursors() {
    mark("EK", 16, |n| {
        if n < 12 {
            let profile = (n / 4 + 1) as u32;
            let mode = if n % 4 < 2 { 81920 } else { 98304 };
            let (frames, page, desc) = pair(profile, mode, 1, n % 2 == 1);
            let d = fixture(profile, 1, &frames);
            set_home(&d, &page, 0);
            success(&d, &page, Some(&desc), None, 0);
        } else {
            let (frames, page, desc, status) = status_case(1, 0, "init");
            let status = status.unwrap();
            let d = fixture(1, 2, &frames);
            match n - 12 {
                0 => {
                    set_home(&d, &page, 0);
                    set_home(&d, &status, 1);
                }
                1 => set_home(&d, &page, 0),
                2 => set_home(&d, &status, 1),
                3 => {
                    let mut bad = status.clone();
                    bad[100] ^= 1;
                    set_home(&d, &bad, 1);
                }
                _ => unreachable!(),
            }
            success(&d, &page, Some(&desc), Some(&status), [0, 2, 1, 3][n - 12]);
        }
    });
}
fn seal_page(raw: &mut [u8]) {
    let digest = content_digest(&raw[..page::DIGEST_OFFSET]);
    raw[page::DIGEST_OFFSET..PAGE_BYTES].copy_from_slice(&digest);
}
#[test]
fn pending_and_healthy_corruption_boundaries() {
    mark("EH", 11, |n| {
        if n <= 4 {
            let (frames, page, desc) = pair(1, 98304, 1, true);
            let d = fixture(1, 1, &frames);
            let historical = vector(DIRTY, 1, 1, "reserve.page");
            install_pending(
                &d,
                1,
                &historical,
                PageKind::Allocation,
                1,
                1,
                ControlState::Pending,
            );
            match n {
                0 => (),
                1 => set_home(&d, &historical, 0),
                2 => {
                    let mut torn = boxed::<PAGE_BYTES>();
                    torn.fill(0x45);
                    set_home(&d, torn.as_slice(), 0);
                }
                3 => set_home(&d, &page, 0),
                4 => {
                    let mut bad = historical.clone();
                    // Keep a healthy allocator envelope and cell: change one
                    // still-Free entry's valid generation, preserving LSN1.
                    let identity = super::physical::identity_of(
                        bad.as_slice().try_into().unwrap(),
                        port::namespace().database,
                    )
                    .unwrap();
                    let namespace = port::namespace();
                    assert_eq!(
                        allocation_page::decode_allocation_page(&bad, identity, namespace)
                            .unwrap()
                            .entry(2)
                            .unwrap(),
                        AllocationEntry::Free(Generation::new(1).unwrap())
                    );
                    let at = 10144
                        + allocation_page::ALLOCATION_HEADER_BYTES
                        + 2 * allocation_page::ALLOCATION_ENTRY_BYTES;
                    AllocationEntry::Free(Generation::new(2).unwrap())
                        .encode(
                            namespace,
                            &mut bad[at..at + allocation_page::ALLOCATION_ENTRY_BYTES],
                        )
                        .unwrap();
                    seal_page(&mut bad);
                    assert_eq!(
                        allocation_page::decode_allocation_page(&bad, identity, namespace)
                            .unwrap()
                            .entry(2)
                            .unwrap(),
                        AllocationEntry::Free(Generation::new(2).unwrap())
                    );
                    assert_eq!(
                        page::decode_page(&bad, identity, PageKind::Allocation)
                            .unwrap()
                            .header()
                            .lsn
                            .0,
                        1
                    );
                    assert_ne!(bad, historical);
                    set_home(&d, &bad, 0);
                }
                _ => unreachable!(),
            }
            if n >= 3 {
                recovered_error(&d, |e| {
                    assert_eq!(e.error().phase, RecoveryPhase::PendingValidate);
                    if n == 3 {
                        assert!(matches!(e.error().cause, RecoveryCause::PriorMismatch));
                    } else {
                        assert!(matches!(e.error().cause, RecoveryCause::Invariant));
                    }
                    assert!(e.effects().boot_directory_synced);
                    assert_eq!(e.effects().home_write_attempted, 0);
                });
            } else {
                success(&d, &page, Some(&desc), None, 1);
            }
        } else if n <= 7 {
            let (frames, _, _) = pair(1, 81920, 1, false);
            let d = fixture(1, 1, &frames);
            let mut raw = literal(GENESIS, 1, "allocation");
            if n == 5 {
                raw[110] ^= 1;
            } else if n == 6 {
                let identity = super::physical::identity_of(
                    raw.as_slice().try_into().unwrap(),
                    port::namespace().database,
                )
                .unwrap();
                let header = page::decode_page(&raw, identity, PageKind::Allocation)
                    .unwrap()
                    .header();
                let cell = raw[10144..16352].to_vec();
                page::encode_page(header, &[&cell, &[0u8; 32]], &mut raw).unwrap();
            } else {
                raw[120] ^= 1;
                seal_page(&mut raw);
            }
            set_home(&d, &raw, 0);
            recovered_error(&d, |e| {
                assert_eq!(e.error().phase, RecoveryPhase::Redo);
                match n {
                    5 => assert!(matches!(e.error().cause, RecoveryCause::PriorMismatch)),
                    7 => assert!(matches!(
                        e.error().cause,
                        RecoveryCause::PageIo(PageIoError::Page(page::PageError::Corrupt))
                    )),
                    6 => assert!(matches!(e.error().cause, RecoveryCause::Allocation(_))),
                    _ => unreachable!(),
                };
                assert_eq!(e.effects().home_write_attempted, 0);
            });
        } else {
            let (frames, _, _, status) = status_case(1, 0, "init");
            let d = fixture(1, 2, &frames);
            let mut raw = status.unwrap();
            raw[64..72].copy_from_slice(&0u64.to_le_bytes());
            if n == 8 {
                // Valid StatusHome codec, outside the supported Free-generation1 set.
                crate::storage::status_home::StatusHome::new(
                    crate::storage::transaction_record::TransactionStatusCell::Free(
                        Generation::new(2).unwrap(),
                    ),
                    0,
                )
                .unwrap()
                .encode(&mut raw[16128..16352])
                .unwrap();
            } else if n == 9 {
                assert_eq!(u64::from_le_bytes(raw[40..48].try_into().unwrap()), 2);
                raw[40..48].copy_from_slice(&3u64.to_le_bytes());
            } else {
                raw[64..72].copy_from_slice(&1u64.to_le_bytes());
            }
            seal_page(&mut raw);
            set_home(&d, &raw, 1);
            recovered_error(&d, |e| {
                assert_eq!(e.error().phase, RecoveryPhase::Redo);
                if n == 8 {
                    assert!(matches!(e.error().cause, RecoveryCause::Semantic(_)));
                } else if n == 9 {
                    assert!(matches!(
                        e.error().cause,
                        RecoveryCause::PageIo(PageIoError::Page(page::PageError::IdentityMismatch))
                    ));
                } else {
                    assert!(matches!(e.error().cause, RecoveryCause::PriorMismatch));
                }
                assert_eq!(e.effects().home_write_attempted, 0);
            });
        }
    });
}
#[test]
fn reserved_only_lengths_and_rejections() {
    mark("EB", 24, |n| {
        let profile = if n < 21 { (n / 7 + 1) as u32 } else { 1 };
        let d = fixture(profile, 1, &[]);
        if n < 21 {
            let length = [
                None,
                Some(0),
                Some(1),
                Some(65535),
                Some(65536),
                Some(1048575),
                Some(1048576),
            ][n % 7];
            if let Some(len) = length {
                for path in SLOT_NAMES {
                    install(&d, path, vec![0; len as usize], len);
                }
            }
            success(&d, &literal(GENESIS, profile, "allocation"), None, None, 0);
        } else if n == 21 {
            install(&d, SLOT_NAMES[1], vec![1], 1);
            let e = acquire_error(&d);
            assert_eq!(e.error.phase, RecoveryPhase::Inventory);
            assert!(matches!(e.error.cause, RecoveryCause::Unsupported));
            assert_eq!(e.effects.mutation_attempted, 0);
        } else if n == 22 {
            install(&d, SLOT_NAMES[0], vec![0; 1048577], 1048577);
            let e = acquire_error(&d);
            assert!(matches!(e.error.cause, RecoveryCause::LengthMismatch));
            assert_eq!(e.effects.mutation_attempted, 0);
        } else {
            install(&d, SLOT_NAMES[0], vec![], 0);
            let mut work = boxed();
            let e = match RecoveryPrefix::acquire(&d, Some(port::namespace()), &mut work) {
                Err(e) => e,
                Ok(_) => panic!("readonly policy widened"),
            };
            assert!(matches!(e.cause, super::super::Cause::UnsupportedClosure));
        }
    });
}
struct LockCreatingDirectory(State);
impl DirectoryPort for LockCreatingDirectory {
    type Writer = port::Writer;
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        {
            let mut s = self.0.lock().unwrap();
            s.files
                .entry("LOCK".into())
                .or_insert_with(|| port::Entry::new(1));
        }
        port::acquire(&self.0)
    }
}
#[test]
fn successful_lease_missing_lock_is_uncertain() {
    mark("EL", 1, |_| {
        let d = fixture(1, 1, &[]);
        d.0.lock().unwrap().files.remove("LOCK");
        install(&d, "CURRENT", vec![0; 4096], 4096);
        let before = d.0.lock().unwrap().files.clone();
        let directory = LockCreatingDirectory(Arc::clone(&d.0));
        let mut m = boxed();
        let mut f = boxed();
        let e = match ClosedRecovery::acquire(&directory, Some(port::namespace()), &mut m, &mut f) {
            Err(e) => e,
            Ok(_) => panic!("CURRENT must reject"),
        };
        assert_eq!(e.error.phase, RecoveryPhase::Prefix);
        assert_eq!(e.error.effect, Effect::CallerMustResolve);
        assert_eq!(e.error.lease_effect, LeaseEffect::MayHaveChanged);
        assert!(matches!(
            e.error.cause,
            RecoveryCause::Prefix(RecoveryPrefixError {
                cause: super::super::Cause::UnsupportedClosure,
                ..
            })
        ));
        assert_eq!(e.effects.mutation_attempted, 0);
        let state = d.0.lock().unwrap();
        assert_eq!(state.files["LOCK"].length, 0);
        assert_eq!(state.files.len(), before.len() + 1);
        for (path, old) in before {
            let new = &state.files[&path];
            assert_eq!(new.bytes, old.bytes);
            assert_eq!(new.length, old.length);
            assert_eq!(new.identity, old.identity);
        }
        assert!(state.events.is_empty());
        assert_eq!(state.handles, 0);
        assert_eq!(state.writers, 0);
    });
}
#[test]
fn guard_input_generation_and_reentry() {
    mark("EG", 7, |n| {
        let d = fixture(1, 1, &[]);
        match n {
            0 => {
                let mut m = boxed();
                let mut f = boxed();
                let mut bad = port::namespace();
                bad.timeline = 0;
                let e = match ClosedRecovery::acquire(&d, Some(bad), &mut m, &mut f) {
                    Err(e) => e,
                    Ok(_) => panic!("invalid namespace"),
                };
                assert_eq!(e.error.phase, RecoveryPhase::Preflight);
                assert_eq!(e.error.effect, Effect::Unchanged);
                assert_eq!(e.error.lease_effect, LeaseEffect::NotAcquired);
                assert!(matches!(
                    e.error.cause,
                    RecoveryCause::Control(ControlError::InvalidInput)
                ));
                assert_eq!(d.0.lock().unwrap().writers, 0);
            }
            1 => {
                let (frames, page, desc, status) = status_case(1, 0, "init");
                let d = fixture(1, 2, &frames);
                // Existing issuance Empty g2/b1 keeps batch1 consumed. New A/S batches2/3
                // finish g10 rather than reusing batch1 or adding a flat seven to g2.
                install_pending(
                    &d,
                    1,
                    &page,
                    PageKind::Allocation,
                    2,
                    1,
                    ControlState::Empty,
                );
                {
                    let mut s = d.0.lock().unwrap();
                    let raw = &mut s.files.get_mut("d3-dwb-1").unwrap().bytes;
                    raw[32..40].copy_from_slice(&2u64.to_le_bytes());
                    let digest = content_digest(&raw[..128]);
                    raw[128..160].copy_from_slice(&digest);
                    let hash = content_digest(&raw[..160]);
                    let sel = &mut s.files.get_mut("DWB").unwrap().bytes;
                    sel[32..40].copy_from_slice(&2u64.to_le_bytes());
                    sel[88..96].copy_from_slice(&2u64.to_le_bytes());
                    sel[112..144].copy_from_slice(&hash);
                    let digest = content_digest(&sel[..4064]);
                    sel[4064..].copy_from_slice(&digest);
                }
                let mut m = boxed();
                let mut f = boxed();
                let out = ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f)
                    .unwrap()
                    .recover()
                    .unwrap();
                assert_eq!(out.observation().known.generation, 10);
                assert_eq!(out.observation().batch, 3);
                assert_eq!(out.allocator_image().as_slice(), page);
                assert_eq!(out.status_image().unwrap().as_slice(), status.unwrap());
                assert_eq!(out.descriptor_bytes().unwrap().as_slice(), desc);
            }
            2 => {
                let (frames, page, _) = pair(1, 81920, 1, false);
                let d = fixture(1, 1, &frames);
                install_pending(
                    &d,
                    1,
                    &page,
                    PageKind::Allocation,
                    1,
                    (u64::MAX - 1) / 3,
                    ControlState::Empty,
                );
                let e = acquire_error(&d);
                assert_eq!(e.error.phase, RecoveryPhase::Inventory);
                assert!(matches!(e.error.cause, RecoveryCause::ArithmeticOverflow));
                assert_eq!(e.effects.mutation_attempted, 0);
            }
            3 => {
                install(&d, "unknown-file", vec![], 0);
                let e = acquire_error(&d);
                assert_eq!(e.error.phase, RecoveryPhase::Inventory);
                assert!(matches!(e.error.cause, RecoveryCause::Unsupported));
            }
            4 => {
                d.0.lock().unwrap().files.get_mut("LOCK").unwrap().length = 1;
                let e = acquire_error(&d);
                assert!(matches!(e.error.cause, RecoveryCause::Unsupported));
            }
            5 => {
                let mut m = boxed();
                let mut f = boxed();
                let owner =
                    ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f).unwrap();
                let e = acquire_error(&d);
                assert_eq!(e.error.phase, RecoveryPhase::Lease);
                assert_eq!(e.error.effect, Effect::Unchanged);
                drop(owner);
                assert_eq!(d.0.lock().unwrap().writers, 0);
            }
            6 => {
                let (frames, page, desc) = pair(1, 98304, 1, true);
                let d = fixture(1, 1, &frames);
                let mut m = boxed();
                let mut f = boxed();
                let first = ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f)
                    .unwrap()
                    .recover()
                    .unwrap();
                let generation = first.observation().known.generation;
                drop(first);
                m.fill(0xa5);
                f.fill(0x5a);
                reset(&d.0);
                let second = ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f)
                    .unwrap()
                    .recover()
                    .unwrap();
                assert_eq!(second.observation().known.generation, generation);
                assert_eq!(second.observation().physical.dirty_mask, 0);
                assert_eq!(second.effects().home_write_attempted, 0);
                assert_eq!(second.effects().dwb_publish_attempted, 0);
                assert_eq!(second.effects().slot_chunks_done, [16, 16]);
                assert_eq!(second.allocator_image().as_slice(), page);
                assert_eq!(second.descriptor_bytes().unwrap().as_slice(), desc);
                drop(second);
                assert_eq!(d.0.lock().unwrap().writers, 0);
            }
            _ => unreachable!(),
        }
    });
}

#[test]
fn healthy_status_prior_and_format_classification() {
    mark("ET", 3, |n| {
        let (frames, page, desc, status) = status_case(1, 0, "init");
        let status = status.unwrap();
        let d = fixture(1, 2, &frames);
        let mut home = status.clone();
        home[64..72].copy_from_slice(&0u64.to_le_bytes());
        if n > 0 {
            home[4..6].copy_from_slice(&2u16.to_le_bytes());
        }
        seal_page(&mut home);
        if n == 2 {
            home[110] ^= 1;
        }
        set_home(&d, &home, 1);
        if n == 1 {
            recovered_error(&d, |e| {
                assert_eq!(e.error().phase, RecoveryPhase::Redo);
                assert!(matches!(
                    e.error().cause,
                    RecoveryCause::PageIo(PageIoError::Page(page::PageError::UnsupportedFormat))
                ));
                assert_eq!(e.effects().home_write_attempted, 0);
            });
        } else {
            success(&d, &page, Some(&desc), Some(&status), 3);
        }
    });
}
// list_owned has an enumeration FD, not a FileIo handle. This adapter injects
// its typed namespace failure without changing the lower completed port helper.
struct InventoryDirectory(State);
struct InventoryWriter(port::Writer);
impl DirectoryPort for InventoryDirectory {
    type Writer = InventoryWriter;
    fn acquire_writer(&self) -> NamespaceResult<InventoryWriter> {
        port::acquire(&self.0).map(InventoryWriter)
    }
}
impl WriterNamespace for InventoryWriter {
    type File = port::File;
    fn random_incarnation(&mut self) -> NamespaceResult<[u8; 16]> {
        self.0.random_incarnation()
    }
    fn create_new(&mut self, n: &FileName) -> NamespaceResult<Self::File> {
        self.0.create_new(n)
    }
    fn open_existing(&mut self, n: &FileName, m: OpenMode) -> NamespaceResult<Self::File> {
        self.0.open_existing(n, m)
    }
    fn open_existing_with_metadata(
        &mut self,
        n: &FileName,
        m: OpenMode,
    ) -> NamespaceResult<(Self::File, FileMetadata)> {
        self.0.open_existing_with_metadata(n, m)
    }
    fn metadata(&self, n: &FileName) -> NamespaceResult<Option<FileMetadata>> {
        self.0.metadata(n)
    }
    fn list_owned(&self, limit: usize) -> NamespaceResult<Vec<DirectoryEntry>> {
        assert_eq!(limit, 18);
        Err(NamespaceError::io(
            NamespaceErrorKind::Io,
            Effect::Unchanged,
            "inventory before-effect fault",
            std::io::Error::other("fixed inventory fault"),
        ))
    }
    fn sync_directory(&mut self) -> NamespaceResult<()> {
        self.0.sync_directory()
    }
    fn publish_selector(&mut self, n: &FileName, s: Selector) -> NamespaceResult<()> {
        self.0.publish_selector(n, s)
    }
    fn remove_retired(&mut self, n: &FileName) -> NamespaceResult<()> {
        self.0.remove_retired(n)
    }
}
#[test]
fn inventory_enumeration_failure_is_postlease() {
    mark("EI", 1, |_| {
        let d = fixture(1, 1, &[]);
        let dir = InventoryDirectory(Arc::clone(&d.0));
        let mut m = boxed();
        let mut f = boxed();
        let e = match ClosedRecovery::acquire(&dir, Some(port::namespace()), &mut m, &mut f) {
            Err(e) => e,
            Ok(_) => panic!("inventory fault must reject"),
        };
        assert_eq!(e.error.phase, RecoveryPhase::Inventory);
        assert!(matches!(e.error.cause, RecoveryCause::Namespace(_)));
        assert_eq!(e.error.effect, Effect::CallerMustResolve);
        assert_eq!(e.error.lease_effect, LeaseEffect::MayHaveChanged);
        assert_eq!(e.effects.mutation_attempted, 0);
        let s = d.0.lock().unwrap();
        assert!(s.events.is_empty());
        assert_eq!(s.writers, 0);
        assert_eq!(s.handles, 0);
    });
}
#[test]
fn partial_eof_last_byte_boot_boundaries() {
    mark("EZ", 3, |n| {
        let d = fixture(1, 1, &[]);
        match n {
            0 => {
                for path in SLOT_NAMES {
                    install(&d, path, vec![0; 17], 17);
                }
                success(&d, &literal(GENESIS, 1, "allocation"), None, None, 0);
            }
            1 => {
                let mut raw = vec![0; 1048576];
                raw[1048575] = 1;
                install(&d, SLOT_NAMES[1], raw, 1048576);
                let e = acquire_error(&d);
                assert_eq!(e.error.phase, RecoveryPhase::Inventory);
                assert!(matches!(e.error.cause, RecoveryCause::Unsupported));
                assert_eq!(e.effects.mutation_attempted, 0);
            }
            2 => {
                install(&d, SLOT_NAMES[0], vec![0; 17], 17);
                d.0.lock().unwrap().read_plan = Some(port::Plan {
                    at: 57,
                    action: port::Action::Zero,
                });
                let e = acquire_error(&d);
                assert_eq!(e.error.phase, RecoveryPhase::Inventory);
                assert!(matches!(e.error.cause, RecoveryCause::Io(_)));
                assert_eq!(e.effects.mutation_attempted, 0);
                assert_eq!(d.0.lock().unwrap().hit, 1);
            }
            _ => unreachable!(),
        }
    });
}
struct SizedDirectory<const O: usize, const F: usize>(std::cell::Cell<usize>);
impl<const O: usize, const F: usize> DirectoryPort for SizedDirectory<O, F> {
    type Writer = port::SizedWriter<O, F>;
    fn acquire_writer(&self) -> NamespaceResult<Self::Writer> {
        self.0.set(self.0.get() + 1);
        Err(NamespaceError::new(
            NamespaceErrorKind::Busy,
            Effect::Unchanged,
            "unexpected oversized acquire",
        ))
    }
}
fn prelease_capacity<const O: usize, const F: usize>() {
    let d = SizedDirectory::<O, F>(std::cell::Cell::new(0));
    let mut m = boxed();
    let mut f = boxed();
    let e = match ClosedRecovery::acquire(&d, None, &mut m, &mut f) {
        Err(e) => e,
        Ok(_) => panic!("oversized source owner"),
    };
    assert_eq!(e.error.phase, RecoveryPhase::Preflight);
    assert!(matches!(e.error.cause, RecoveryCause::Capacity));
    assert_eq!(e.error.effect, Effect::Unchanged);
    assert_eq!(e.error.lease_effect, LeaseEffect::NotAcquired);
    assert_eq!(d.0.get(), 0);
    assert_eq!(e.effects.calls_attempted, 0);
    m.fill(1);
    f.fill(2);
}
#[test]
fn source_types_and_whole_owner_prelease_census() {
    mark("EC", 3, |n| match n {
        0 => {
            assert!(size_of::<port::SizedWriter<16384, 0>>() > 16384);
            prelease_capacity::<16384, 0>();
        }
        1 => {
            assert!(size_of::<port::SizedFile<16384>>() > 16384);
            prelease_capacity::<0, 16384>();
        }
        2 => {
            type W = port::SizedWriter<12000, 0>;
            assert!(size_of::<W>() <= 16384);
            assert!(size_of::<<W as WriterNamespace>::File>() <= 16384);
            assert!(super::super::fits::<W>());
            assert!(resources::source_census::<W>().peak > 65536);
            prelease_capacity::<12000, 0>();
        }
        _ => unreachable!(),
    });
}

#[path = "test_faults.rs"]
mod faults;
#[test]
fn new_caller_short_interrupt_invalid_and_unwind() {
    mark("EN", 12, |n| {
        let d = fixture(1, 1, &[]);
        if n < 4 {
            let action = [
                port::Action::Short(17),
                port::Action::Interrupted,
                port::Action::Zero,
                port::Action::InvalidCount,
            ][n];
            d.0.lock().unwrap().plan = Some(port::Plan { at: 1, action });
            if n < 2 {
                success(&d, &literal(GENESIS, 1, "allocation"), None, None, 0);
            } else {
                recovered_error(&d, |e| {
                    assert_eq!(e.error().phase, RecoveryPhase::BootstrapProvision);
                    assert!(matches!(e.error().cause, RecoveryCause::Io(_)));
                    assert_eq!(e.effects().slot_chunks_done, [0, 0]);
                    assert_eq!(e.effects().home_write_attempted, 0);
                });
            }
        } else if n < 7 {
            install(&d, SLOT_NAMES[0], vec![0; 17], 17);
            let action = [
                port::Action::Short(7),
                port::Action::Interrupted,
                port::Action::PrefixEof,
            ][n - 4];
            d.0.lock().unwrap().read_plan = Some(port::Plan { at: 57, action });
            if n < 6 {
                success(&d, &literal(GENESIS, 1, "allocation"), None, None, 0);
            } else {
                let e = acquire_error(&d);
                assert_eq!(e.error.phase, RecoveryPhase::Inventory);
                assert!(matches!(e.error.cause, RecoveryCause::Io(_)));
                assert_eq!(e.effects.mutation_attempted, 0);
                assert_eq!(d.0.lock().unwrap().configured_read_hits, 1);
            }
        } else if n == 7 {
            d.0.lock().unwrap().plan = Some(port::Plan {
                at: 1,
                action: port::Action::AfterPanic,
            });
            let mut m = boxed();
            let mut f = boxed();
            let owner =
                ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f).unwrap();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = owner.recover();
            }));
            assert!(result.is_err());
            {
                let s = d.0.lock().unwrap();
                assert_eq!(s.events.len(), 2);
                assert_eq!(s.events.last().unwrap().kind, "write");
                assert_eq!(s.writers, 0);
                assert_eq!(s.handles, 0);
                assert_eq!(s.files[SLOT_NAMES[0]].length, 65536);
            }
            // The first attempt consumed its intentional panic. Clear the
            // fixture fault before resetting ordinals for the clean reentry.
            d.0.lock().unwrap().plan = None;
            m.fill(0xa5);
            f.fill(0x5a);
            reset(&d.0);
            let result = ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f)
                .unwrap()
                .recover()
                .unwrap();
            assert_eq!(result.effects().slot_chunks_done, [16, 16]);
            assert_eq!(result.effects().home_write_attempted, 0);
            drop(result);
        } else if n == 8 {
            install(&d, SLOT_NAMES[0], vec![0; 17], 17);
            d.0.lock().unwrap().read_plan = Some(port::Plan {
                at: 57,
                action: port::Action::BeforePanic,
            });
            let mut m = boxed();
            let mut f = boxed();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f);
            }));
            assert!(result.is_err());
            let s = d.0.lock().unwrap();
            assert_eq!(s.writers, 0);
            assert_eq!(s.handles, 0);
            assert!(s.events.is_empty());
            drop(s);
            m.fill(1);
            f.fill(2);
        } else {
            let (frames, page, desc) = pair(1, 98304, 1, true);
            let d = fixture(1, 1, &frames);
            let historical = vector(DIRTY, 1, 1, "reserve.page");
            install_pending(
                &d,
                1,
                &historical,
                PageKind::Allocation,
                1,
                1,
                ControlState::Pending,
            );
            let action = [
                port::Action::Short(17),
                port::Action::Interrupted,
                port::Action::BeforePanic,
            ][n - 9];
            d.0.lock().unwrap().plan = Some(port::Plan { at: 39, action });
            if n < 11 {
                success(&d, &page, Some(&desc), None, 1);
            } else {
                let mut m = boxed();
                let mut f = boxed();
                let owner =
                    ClosedRecovery::acquire(&d, Some(port::namespace()), &mut m, &mut f).unwrap();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = owner.recover();
                }));
                assert!(result.is_err());
                let s = d.0.lock().unwrap();
                assert_eq!(s.events.len(), 40);
                assert_eq!(s.events.last().unwrap().kind, "write");
                assert_eq!(s.events.last().unwrap().name, HOME_NAME);
                assert_eq!(s.writers, 0);
                assert_eq!(s.handles, 0);
                drop(s);
                m.fill(1);
                f.fill(2);
            }
        }
    });
}

#[path = "operational_tests.rs"]
mod operational;

#[path = "empty_transaction_tests.rs"]
mod empty;

#[path = "../mixed_system_recovery_tests.rs"]
mod mixed;
