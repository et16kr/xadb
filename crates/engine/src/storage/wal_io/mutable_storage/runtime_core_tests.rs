//! New extraction boundaries only; never invokes a completed legacy test.
use super::*;
use crate::storage::{
    page::{FileId, PageRef},
    retention_genesis::tests::{self as input, Harness},
    wal_io::{bootstrap as boot, plan4_test_support as port},
};
use foundation::{
    codec::content_digest,
    identity::ObjectId,
    namespace::{DirectoryPort, FileIdentity},
};
use std::{
    collections::BTreeMap,
    mem::{align_of, size_of},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

fn selected(id: &str, operation: impl FnOnce()) {
    let mut harness = Harness::new(vec![id.to_owned()]);
    harness.case(id, operation);
    harness.finish();
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}

// These are additional simultaneous source values introduced by explicit
// forwarding. Count both argument scopes and both Result carriers, even though
// a tail return may reuse machine storage. Distinct API calls do not coexist.
fn forward_phase<A, R>() -> usize {
    2 * size_of::<A>() + 2 * size_of::<R>()
}
fn static_source<W: WriterNamespace>(label: &str, historical_owner: usize) {
    type G<'a, W> = wal::GenesisWalSession<'a, W>;
    type R<'a, W> = wal::WalRuntime<'a, W>;
    type C<'a, W> = boot::InstalledStorageCore<'a, W>;
    type F<'a, W> = boot::FreshStorageSession<'a, W>;
    type M<'a, W> = MutableStorageOwner<'a, 'a, W>;
    let runtime = size_of::<R<'_, W>>();
    let genesis = size_of::<G<'_, W>>();
    let core = size_of::<C<'_, W>>();
    let fresh = size_of::<F<'_, W>>();
    let dwb = size_of::<DoublewriteStorageSession<'_, '_, W>>();
    let owner = size_of::<M<'_, W>>();

    let prefix = forward_phase::<&G<'_, W>, wal::Result<wal::WalPrefix>>();
    let stage = forward_phase::<(&G<'_, W>, &[u8]), wal::Result<wal::WalFrameLocation>>();
    let sync = forward_phase::<&G<'_, W>, wal::Result<wal::WalSyncObservation>>();
    let durable = forward_phase::<
        (&G<'_, W>, &wal::WalRequirement<'_>),
        wal::Result<wal::EnsureDurable<'_>>,
    >();
    let health = forward_phase::<&G<'_, W>, bool>();
    let wal_forward = prefix.max(stage).max(sync).max(durable).max(health);
    let status = forward_phase::<&F<'_, W>, boot::Result<boot::StatusRole<'_>>>();
    let undo = forward_phase::<&F<'_, W>, boot::Result<wal::UndoRole<'_>>>();
    let reader = forward_phase::<
        (&F<'_, W>, &mut [u8; PAGE_BYTES], &mut [u8; PAGE_BYTES]),
        std::result::Result<PageFileReader<'_, W>, PageIoError>,
    >();
    // DWB->Fresh->Core has two new/retained wrapper frames before the reader;
    // conservatively retain both pairs of forwarded args/results at this point.
    let dwb_reader = 2 * reader;
    let old = boot::source_budget::<W>();
    // Actual construction return carriers: wrapping does not imply zero layout
    // cost. This type-only bound deliberately counts inner and outer values.
    let genesis_return = runtime + genesis + size_of::<wal::Result<G<'_, W>>>();
    let fresh_return = core + fresh + size_of::<boot::Result<F<'_, W>>>();

    let (conversion, retained, handoff, old_bound, new_bound, error, result) =
        EmptyTransactions::<W>::runtime_core_layout();
    let g5 = super::plan4_pages::source_census::<W>();
    println!(
        "P4_CENSUS\t{label}\twriter={}\tfile={}\truntime={runtime}\tgenesis={genesis}\tcore={core}\tfresh={fresh}\tdwb={dwb}\tmutable={owner}\thistorical_mutable={historical_owner}\tgenesis_return={genesis_return}\tfresh_return={fresh_return}",
        size_of::<W>(),
        size_of::<W::File>()
    );
    println!(
        "P4_CENSUS\t{label}-forwarding\tprefix={prefix}\tstage={stage}\tsync={sync}\tdurable={durable}\thealth={health}\tstatus={status}\tundo={undo}\treader={reader}\tdwb_reader={dwb_reader}\told_prepare={}\told_readback={}\told_status={}\told_undo={}",
        old.prepare_nested, old.readback_nested, old.fresh_role, old.undo_role
    );
    println!(
        "P4_CENSUS\t{label}-empty\tconversion={conversion}\tretained={retained}\thandoff_fits={handoff}\told={old_bound}\tnew={new_bound}\terror={error}\tresult={result}\tg5_peak={}\tg5_whole={}",
        g5.peak, g5.whole
    );
    // Emit every actual value before the first admission assertion.
    assert_eq!(runtime, genesis);
    assert_eq!(align_of::<R<'_, W>>(), align_of::<G<'_, W>>());
    assert_eq!(core, fresh);
    assert_eq!(align_of::<C<'_, W>>(), align_of::<F<'_, W>>());
    assert_eq!(owner, historical_owner);
    assert!(genesis <= 16384 && fresh <= 16384 && dwb <= 16384);
    assert!(size_of::<super::super::InstallPlan>() <= 4096);
    assert!(owner <= 32768 && size_of::<W::File>() <= 16384);
    assert!(boot::source_budget_fits::<W>());
    assert!(boot::plan4_genesis_capacity::<W>());
    assert!(super::fits::<W>());
    assert!(wal::durability_types_fit());
    assert!(wal_forward + wal::DURABILITY_CALL_BYTES <= 65536);
    assert!(old.fresh_role + status <= 1024);
    assert!(old.undo_role + undo <= 1024);
    assert!(old.mutable_role + status <= 2048);
    assert!(old.mutable_undo_role + undo <= 2048);
    assert!(old.prepare_nested <= 16384 && old.readback_nested <= 16384);
    assert!(fresh + 2 * size_of::<W::File>() + old.readback_nested + dwb_reader <= 65536);
    assert!(genesis_return <= 65536 && fresh_return <= 65536);
    assert!(conversion <= 65536 && retained + conversion <= 196608 && handoff);
    assert_eq!(g5.owner, owner);
    assert_eq!(g5.old_common_delta, 0);
    assert!(g5.peak <= 65536 && g5.whole <= 262144);
    assert_eq!(g5.caller_operation, 114688);
    assert_eq!(g5.caller_flush, 4 << 20);
}
#[test]
fn runtime_core_source_memory() {
    selected("RC-ST01", || {
        static_source::<port::Writer>("runtime-core-memory", 848)
    });
}
#[cfg(target_os = "linux")]
#[test]
fn runtime_core_source_native() {
    selected("RC-ST02", || {
        // 984 before the native adapter File stopped holding its 128-byte
        // trace name inline; the owner retains one File.
        static_source::<super::plan4_pages::native::Writer>("runtime-core-native", 856);
    });
}

#[derive(Debug, PartialEq, Eq)]
struct Trace {
    files: BTreeMap<String, (u64, FileIdentity, [u8; 32])>,
    events: usize,
    mutations: usize,
    entropy: usize,
    acquired: usize,
    lock_creates: usize,
    writers: usize,
    handles: usize,
}
fn trace(state: &Arc<Mutex<port::State>>) -> Trace {
    let state = state.lock().unwrap();
    Trace {
        files: state
            .files
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    (value.length, value.identity, content_digest(&value.bytes)),
                )
            })
            .collect(),
        events: state.events.len(),
        mutations: state.mutations(),
        entropy: state.entropy_calls,
        acquired: state.acquire_calls,
        lock_creates: state.lock_creates,
        writers: state.writers,
        handles: state.handles,
    }
}
fn slot_pointer<W: WriterNamespace>(runtime: &wal::WalRuntime<'_, W>) -> usize {
    runtime
        .append
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .slot
        .as_ref()
        .unwrap()
        .as_ptr() as usize
}
fn allocator_identity() -> PageIdentity {
    PageIdentity {
        database: input::namespace().database,
        page: PageRef {
            file: FileId::new(0x1_0000_0001).unwrap(),
            page_no: 0,
            incarnation: Generation::new(1).unwrap(),
        },
        owner: ObjectId::new(1).unwrap(),
        owner_generation: Generation::new(1).unwrap(),
    }
}
fn read_core(core: &boot::InstalledStorageCore<'_, port::Writer>) {
    let mut root = boxed::<PAGE_BYTES>();
    let mut scratch = boxed::<PAGE_BYTES>();
    let mut page = boxed::<PAGE_BYTES>();
    let expected = input::raw("P00");
    let mut reader = core.open_reader(&mut root, &mut scratch).unwrap();
    assert!(core.reader_active.load(Ordering::Acquire));
    reader
        .read_page(allocator_identity(), PageKind::Allocation, 0, &mut page)
        .unwrap();
    assert_eq!(page.as_slice(), expected);
    drop(reader);
    assert!(!core.reader_active.load(Ordering::Acquire));
}
// Each case creates its own genuine owner; no raw Fresh/Core construction.
macro_rules! installed {
    (|$dwb:ident, $state:ident, $slot_pointer:ident, $flush_pointer:ident| $body:block) => {{
        let directory = port::Directory::fresh();
        let $state = directory.state();
        let mut slot = boxed::<{ wal::MAX_FRAME_BYTES }>();
        let $slot_pointer = slot.as_ptr() as usize;
        let mut maintenance = boxed::<12_582_912>();
        let mut flush = boxed::<{ 4 << 20 }>();
        let $flush_pointer = flush.as_ptr() as usize;
        let fresh = boot::FreshStorageSession::create_retention_catalog(
            directory.acquire_writer().unwrap(), 1, &mut slot, &mut maintenance,
        ).unwrap();
        let $dwb = fresh.install_doublewrite(&mut flush).unwrap();
        $body
        let after = trace(&$state);
        assert_eq!((after.writers, after.handles), (0, 0));
        assert_eq!(after.acquired, 1);
        assert_eq!(after.entropy, 1);
    }};
}
fn no_drop_effect(before: &Trace, after: &Trace) {
    assert_eq!(after.files, before.files);
    assert_eq!(after.events, before.events);
    assert_eq!(after.mutations, before.mutations);
    assert_eq!(after.acquired, before.acquired);
    assert_eq!(after.entropy, before.entropy);
    assert_eq!((after.writers, after.handles), (0, 0));
}

#[test]
fn runtime_core_healthy_handoff() {
    selected("RC-M01", || {
        installed!(|dwb, state, slot_addr, flush_addr| {
            let mut root = boxed::<PAGE_BYTES>();
            let mut scratch = boxed::<PAGE_BYTES>();
            let mut page = boxed::<PAGE_BYTES>();
            {
                let mut reader = dwb.open_reader(&mut root, &mut scratch).unwrap();
                assert!(dwb.fresh.core.reader_active.load(Ordering::Acquire));
                reader
                    .read_page(allocator_identity(), PageKind::Allocation, 0, &mut page)
                    .unwrap();
                assert_eq!(page.as_slice(), input::raw("P00"));
            }
            assert!(!dwb.fresh.core.reader_active.load(Ordering::Acquire));
            let old_prefix = dwb.fresh.prefix().unwrap();
            let old_plan = (
                dwb.fresh.core.plan.kind,
                dwb.fresh.core.plan.profile,
                dwb.fresh.core.plan.selected,
                dwb.fresh.core.plan.allocation_digest,
            );
            assert_eq!(slot_pointer(&dwb.fresh.core.wal), slot_addr);
            assert_eq!(dwb._workspace.as_ptr() as usize, flush_addr);
            let before = trace(&state);
            let storage = dwb.into_mutable_storage();
            assert_eq!(trace(&state), before);
            assert_eq!(storage.wal().prefix().unwrap(), old_prefix);
            assert_eq!(
                (
                    storage.core.plan.kind,
                    storage.core.plan.profile,
                    storage.core.plan.selected,
                    storage.core.plan.allocation_digest
                ),
                old_plan
            );
            assert_eq!(slot_pointer(storage.wal()), slot_addr);
            assert_eq!(
                storage.workspace.lock().unwrap().as_ref().unwrap().as_ptr() as usize,
                flush_addr
            );
            assert_eq!(storage.binding.load(Ordering::Acquire), AVAILABLE);
            assert!(!storage.session_fence().is_fenced());
            read_core(&storage.core);
            let before_drop = trace(&state);
            assert_eq!(before_drop.files, before.files);
            assert_eq!(before_drop.mutations, before.mutations);
            drop(storage);
            no_drop_effect(&before_drop, &trace(&state));
        });
    });
}

#[test]
fn runtime_core_genesis_forwarding() {
    selected("RC-M02", || {
        let directory = port::Directory::fresh();
        let state = directory.state();
        let mut slot = boxed::<{ wal::MAX_FRAME_BYTES }>();
        let slot_addr = slot.as_ptr() as usize;
        let session = wal::GenesisWalSession::create(
            directory.acquire_writer().unwrap(),
            1,
            &mut slot,
            &mut [0; PAGE_BYTES],
        )
        .unwrap();
        let before = trace(&state);
        let prefix = session.prefix().unwrap();
        let bytes = wal::test_support::raw_frame(0, 1, &[0x17, 0x38, 0x59]);
        let location = session.try_stage(&bytes).unwrap();
        assert_eq!(trace(&state), before);
        assert_eq!(location.stream, prefix.stream);
        assert_eq!(location.file, prefix.file);
        assert_eq!(
            (
                location.previous_lsn,
                location.lsn,
                location.start,
                location.length,
                location.end
            ),
            (0, 1, 4096, 195, 4291)
        );
        assert_eq!(slot_pointer(&session.runtime), slot_addr);
        {
            let append = session.runtime.append.lock().unwrap();
            assert_eq!(append.state, wal::SlotState::Complete(location));
            assert_eq!(&append.slot.as_ref().unwrap()[..bytes.len()], bytes);
        }
        let complete = session.prefix().unwrap();
        assert_eq!(
            (
                complete.complete_lsn,
                complete.complete_offset,
                complete.synced_lsn,
                complete.synced_offset
            ),
            (1, 4291, 0, 4096)
        );
        let synced = session.sync_pending().unwrap();
        assert_eq!(synced.frame, location);
        assert_eq!(synced.prefix, session.prefix().unwrap());
        assert_eq!(
            (
                synced.prefix.complete_lsn,
                synced.prefix.complete_offset,
                synced.prefix.synced_lsn,
                synced.prefix.synced_offset
            ),
            (1, 4291, 1, 4291)
        );
        assert!(!session.is_fenced());
        let actual = {
            let state = state.lock().unwrap();
            state.files[wal::WAL_NAME].bytes.clone()
        };
        let mut expected = input::reference(1, wal::WAL_NAME);
        expected.extend_from_slice(&bytes);
        assert_eq!(actual, expected);
        let before_drop = trace(&state);
        assert_eq!(before_drop.acquired, 1);
        drop(session);
        no_drop_effect(&before_drop, &trace(&state));
    });
}

#[test]
fn runtime_core_fenced_handoff() {
    selected("RC-M03", || {
        installed!(|dwb, state, slot_addr, flush_addr| {
            let prefix = dwb.fresh.prefix().unwrap();
            dwb.fresh.core.wal.fenced.fence();
            let before = trace(&state);
            let storage = dwb.into_mutable_storage();
            assert_eq!(trace(&state), before);
            assert!(storage.session_fence().is_fenced());
            let error = storage.health().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, Cause::Fenced));
            assert_eq!(storage.wal().append.lock().unwrap().prefix, prefix);
            assert_eq!(slot_pointer(storage.wal()), slot_addr);
            assert_eq!(
                storage.workspace.lock().unwrap().as_ref().unwrap().as_ptr() as usize,
                flush_addr
            );
            assert!(!storage.core.reader_active.load(Ordering::Acquire));
            assert_eq!(trace(&state), before);
            drop(storage);
            no_drop_effect(&before, &trace(&state));
        });
    });
}

#[test]
fn runtime_core_poisoned_handoff() {
    selected("RC-M04", || {
        installed!(|dwb, state, slot_addr, flush_addr| {
            let prefix = dwb.fresh.prefix().unwrap();
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _held = dwb.fresh.core.wal.append.lock().unwrap();
                    panic!("runtime/core original append poison");
                }))
                .is_err()
            );
            let before = trace(&state);
            let storage = dwb.into_mutable_storage();
            assert_eq!(trace(&state), before);
            assert!(storage.wal().append.is_poisoned());
            let error = storage.health().unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(matches!(error.cause, Cause::Poisoned));
            assert!(storage.session_fence().is_fenced());
            let observed_prefix = storage
                .wal()
                .append
                .lock()
                .err()
                .unwrap()
                .into_inner()
                .prefix;
            assert_eq!(observed_prefix, prefix);
            assert_eq!(slot_pointer(storage.wal()), slot_addr);
            assert_eq!(
                storage.workspace.lock().unwrap().as_ref().unwrap().as_ptr() as usize,
                flush_addr
            );
            assert!(!storage.core.reader_active.load(Ordering::Acquire));
            assert_eq!(trace(&state), before);
            drop(storage);
            no_drop_effect(&before, &trace(&state));
        });
    });
}

#[test]
fn runtime_core_reader_claim_handoff() {
    selected("RC-M05", || {
        installed!(|dwb, state, slot_addr, flush_addr| {
            read_core(&dwb.fresh.core);
            let prefix = dwb.fresh.prefix().unwrap();
            // Declared private claim fault. No borrowing reader is moved or leaked.
            dwb.fresh.core.reader_active.store(true, Ordering::Release);
            let before = trace(&state);
            let storage = dwb.into_mutable_storage();
            assert_eq!(trace(&state), before);
            assert!(storage.core.reader_active.load(Ordering::Acquire));
            let mut root = boxed::<PAGE_BYTES>();
            let mut scratch = boxed::<PAGE_BYTES>();
            let error = storage
                .core
                .open_reader(&mut root, &mut scratch)
                .err()
                .unwrap();
            assert!(matches!(error, PageIoError::Busy));
            assert!(storage.core.reader_active.load(Ordering::Acquire));
            assert_eq!(storage.wal().prefix().unwrap(), prefix);
            assert_eq!(slot_pointer(storage.wal()), slot_addr);
            assert_eq!(
                storage.workspace.lock().unwrap().as_ref().unwrap().as_ptr() as usize,
                flush_addr
            );
            assert!(!storage.session_fence().is_fenced());
            assert_eq!(trace(&state), before);
            // Explicit cleanup of the injected test fault, not product reset.
            storage.core.reader_active.store(false, Ordering::Release);
            read_core(&storage.core);
            let before_drop = trace(&state);
            assert_eq!(before_drop.files, before.files);
            assert_eq!(before_drop.mutations, before.mutations);
            drop(storage);
            no_drop_effect(&before_drop, &trace(&state));
        });
    });
}
