use super::*;
use crate::storage::wal_io::bootstrap::test_support as port;

#[test]
fn empty_live_resource_gate() {
    type W = port::Writer;
    let retained = super::super::retained_bytes::<W>();
    println!(
        "live_call={} retained={} metadata={} whole={} conversion={} pending={}",
        call_bytes::<W>(),
        retained,
        retained + call_bytes::<W>(),
        retained + call_bytes::<W>() + 65536,
        super::super::conversion_bytes::<W>(),
        size_of::<Option<WalRequirement<'_>>>()
    );
    assert!(fits::<W>());
    assert!(fits::<xadb_platform::namespace::LinuxWriterLease>());
}

use crate::{
    storage::{
        dirty_frame::{DirtyFrame, init_test_support as probe},
        wal_io::{self as wal, FreshStorageSession, IntoFlushError},
    },
    transaction::concurrency::OwnerRole,
};
use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
};
fn boxed<const N: usize>(n: u8) -> Box<[u8; N]> {
    vec![n; N].into_boxed_slice().try_into().unwrap()
}
macro_rules! fixture {
    ($count:expr, $profile:expr, |$live:ident,$core:ident,$state:ident,$storage:ident,$pages:ident,$owner:ident $(,$foreign:ident)?| $body:block $(, prepare |$provision:ident| $setup:block)?) => {{
        let mut wal_slot = port::slot();
        let mut workspace = boxed::<{ 4 << 20 }>(0);
        let (namespace, $state) = port::fresh();
        let fresh = FreshStorageSession::create_status(
            namespace,
            $profile,
            &mut wal_slot,
            &mut [[0; PAGE_BYTES]; 3],
        )
        .unwrap();
        let installed = fresh.install_doublewrite(&mut workspace).unwrap();
        let $storage = installed.into_mutable_storage();
        let $core = PageConcurrency::new($storage.session_fence()).unwrap();
        $(let $foreign = PageConcurrency::new($storage.session_fence()).unwrap();)?
        let mut bytes = vec![[0; PAGE_BYTES]; $count + 1];
        let mut frames: Vec<_> = bytes.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114688>(0);
        let binding = $storage
            .bind_resident_status(&$core, &mut frames, &mut operation)
            .unwrap();
        let mut bound = match binding.activate() {
            Ok(v) => v,
            Err(IntoFlushError::Before { error, .. }) | Err(IntoFlushError::Fatal(error)) => {
                panic!("activate {error:?}")
            }
        };
        let mut $owner = $core.try_owner(OwnerRole::Maintenance).unwrap();
        let mut $pages = Vec::new();
        for index in 1..=$count {
            if index > 1 {
                bound.prepare_slot(index).unwrap();
            }
            let (mut writer, _, _) = bound.split_with_flush();
            let page = match writer.reserve_one(&mut $owner).unwrap().initialize() {
                Ok(v) => v.page,
                Err(_) => panic!("INIT"),
            };
            $pages.push(page);
        }
        $(let mut $provision = bound; $setup let bound = $provision;)?
        let mut bank = boxed::<{ super::super::super::EMPTY_TRANSACTION_BANK_BYTES }>(0);
        #[allow(unused_mut)] // Admission-only cases observe the same fixture without a live mutation.
        let mut $live = match bound.into_empty_transactions(&mut $owner, &mut bank) {
            Ok(v) => v,
            Err(_) => panic!("handoff"),
        };
        {
            let mut state = $state.lock().unwrap();
            state.events.clear();
            state.read_events.clear();
            state.hit = 0;
            state.reads = 0;
        }
        $body
    }};
}
thread_local! {
    static CUT: Cell<Option<(u8,bool)>> = const { Cell::new(None) };
    static FORGE_TERMINAL: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    static FINAL_DAMAGE: Cell<Option<u8>> = const { Cell::new(None) };
    static CENSUS_MISMATCH: Cell<Option<(u32,u32)>> = const { Cell::new(None) };
}
pub(super) fn census_mismatch(seen: u32, occupied: u32) {
    CENSUS_MISMATCH.with(|value| value.set(Some((seen, occupied))));
}
pub(super) fn hit<W: WriterNamespace>(phase: u8, storage: &MutableStorageOwner<'_, '_, W>) {
    CUT.with(|cut| {
        if let Some((selected, panic)) = cut.get()
            && selected == phase
        {
            cut.set(None);
            if panic {
                panic!("selected staged installation cut {phase}");
            }
            storage.session_fence().fence();
        }
    });
}
pub(super) fn after_sync<W: WriterNamespace>(
    live: &mut EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>,
) {
    FINAL_DAMAGE.with(|damage| {
        if let Some(kind) = damage.take() {
            match kind {
                0 => {
                    static FOREIGN: crate::storage::session_fence::SessionFence =
                        crate::storage::session_fence::SessionFence::new();
                    live.pending.as_mut().unwrap().origin = &FOREIGN;
                }
                1 => live.pending.as_mut().unwrap().exact.end += 1,
                2 => dirty_frame::test_support::seed_counter(&live.inner.inner.inner.pool, 1, 9),
                _ => unreachable!(),
            }
        }
    });
    FORGE_TERMINAL.with(|forge| {
        if let Some(mut old) = forge.borrow_mut().take() {
            let lsn = live.pending.unwrap().location().lsn;
            old[72..80].copy_from_slice(&lsn.to_le_bytes());
            seal(&mut old);
            row_mut(live.bank, 0)[..1536].copy_from_slice(&old);
            dirty_frame::status_load_test_support::edit_content(
                &live.inner.inner.inner.pool,
                1,
                |page| {
                    page[16160..16352].copy_from_slice(&old[160..352]);
                    seal(page);
                },
            );
        }
    });
}
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
fn page_bytes<W: WriterNamespace>(
    live: &EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>,
    index: usize,
) -> Box<[u8; PAGE_BYTES]> {
    let mut bytes = boxed::<PAGE_BYTES>(0);
    probe::snapshot(&live.inner.inner.inner.pool, index, &mut bytes);
    bytes
}
fn image_stamp<W: WriterNamespace>(
    live: &EmptyTransactions<'_, '_, '_, '_, '_, '_, '_, '_, W>,
    index: usize,
) -> FrameStamp {
    let mut bytes = boxed::<PAGE_BYTES>(0);
    dirty_frame::test_support::diagnose(&live.inner.inner.inner.pool, index, &mut bytes).frame
}
fn literal(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dev3-empty-transactions/live")
            .join(name),
    )
    .unwrap()
}

#[test]
fn empty_live_actual_lifecycle_matches_raw_literals_and_preserves_system() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let allocator = page_bytes(&live, 0);
        let descriptor = live.inner.inner.inner.control.base.descriptor;
        assert_eq!(
            &*page_bytes(&live, 1),
            literal("begin.before.page").as_slice()
        );
        let begin = live.begin(&mut owner, pages[0]).unwrap();
        assert_eq!(
            (
                begin.lsn.0,
                begin.actor.id().local,
                begin.actor.generation().get(),
                begin.remaining_wal,
                begin.retained_transactions
            ),
            (3, 1, 2, 6080, 1)
        );
        assert_eq!(
            &*page_bytes(&live, 1),
            literal("begin.after.page").as_slice()
        );
        assert_eq!(&row(live.bank, 0)[..1536], literal("begin.descriptor"));
        let rollback = live.rollback_start(&mut owner, begin.actor).unwrap();
        assert_eq!((rollback.lsn.0, rollback.remaining_wal), (4, 3040));
        assert_eq!(
            &*page_bytes(&live, 1),
            literal("rollback.after.page").as_slice()
        );
        let abort = live.abort_end(&mut owner, begin.actor).unwrap();
        assert_eq!(
            (
                abort.lsn.0,
                abort.remaining_wal,
                abort.retained_transactions
            ),
            (5, 0, 0)
        );
        assert_eq!(abort.state, TransactionState::Aborted);
        assert_eq!(
            &*page_bytes(&live, 1),
            literal("abort.after.page").as_slice()
        );
        assert!(live.bank.iter().all(|b| *b == 0));
        assert!(live.pending.is_none());
        assert_eq!(page_bytes(&live, 0), allocator);
        assert_eq!(live.inner.inner.inner.control.base.descriptor, descriptor);
        let stamp = image_stamp(&live, 1);
        assert_eq!(
            (stamp.page_lsn.0, stamp.modification_counter, stamp.rec_lsn),
            (5, 4, Some(PageLsn(2)))
        );
        let guard = state.lock().unwrap();
        let events = &guard.events;
        assert_eq!(
            events.iter().map(|e| e.kind).collect::<Vec<_>>(),
            vec!["write", "sync", "write", "sync", "write", "sync"]
        );
        for (i, name) in ["begin.wal", "rollback.wal", "abort.wal"]
            .iter()
            .enumerate()
        {
            assert_eq!(events[i * 2].data, literal(name));
        }
        assert_eq!(
            guard.files[wal::WAL_NAME].bytes,
            literal("wal-through-abort.bin")
        );
        assert_eq!(storage.wal().prefix().unwrap().synced_lsn, 5);
        assert_eq!(guard.reads, 0); // dirty resident homes win over stale disk
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn empty_live_interleaves_two_owners_and_preserves_unselected_home() {
    fixture!(2, 1, |live, core, state, storage, pages, owner| {
        let a = live.begin(&mut owner, pages[0]).unwrap();
        let old_a = page_bytes(&live, 1);
        let b = live.begin(&mut owner, pages[1]).unwrap();
        assert_eq!(page_bytes(&live, 1), old_a);
        assert_eq!((a.lsn.0, b.lsn.0, b.remaining_wal), (5, 6, 12160));
        live.rollback_start(&mut owner, a.actor).unwrap();
        live.rollback_start(&mut owner, b.actor).unwrap();
        assert_eq!(
            live.abort_end(&mut owner, a.actor).unwrap().remaining_wal,
            3040
        );
        assert_eq!(
            live.abort_end(&mut owner, b.actor).unwrap().remaining_wal,
            0
        );
        assert_eq!(storage.wal().prefix().unwrap().synced_lsn, 10);
        assert_eq!(state.lock().unwrap().reads, 0);
    });
}
#[test]
fn empty_live_32_retained_slots_release_for_33rd_different_home() {
    fixture!(33, 1, |live, core, state, storage, pages, owner| {
        let mut actors = Vec::new();
        for &page in &pages[..32] {
            actors.push(live.begin(&mut owner, page).unwrap().actor);
        }
        let prefix = storage.wal().prefix().unwrap();
        let error = live.begin(&mut owner, pages[32]).unwrap_err();
        assert!(matches!(error.cause, Cause::Capacity));
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert_eq!((live.issued_local, live.adopted_local), (33, 32));
        live.rollback_start(&mut owner, actors[0]).unwrap();
        live.abort_end(&mut owner, actors[0]).unwrap();
        let next = live.begin(&mut owner, pages[32]).unwrap();
        assert_eq!(
            (next.actor.id().local, next.retained_transactions),
            (34, 32)
        );
        assert_eq!(next.remaining_wal, 32 * 6080);
        assert_eq!(state.lock().unwrap().reads, 0);
        assert_eq!(
            decode_status_home_page(
                &*page_bytes(&live, 1),
                image_stamp(&live, 1).identity,
                storage.profile().spec().namespace
            )
            .unwrap()
            .cell(0)
            .unwrap()
            .core(),
            TransactionStatusCell::Occupied(
                crate::storage::transaction_record::TransactionStatus::new(
                    actors[0],
                    TransactionState::Aborted,
                    None
                )
                .unwrap()
            )
        );
    });
}
#[test]
fn empty_live_candidate_failure_burns_identity_and_gap_is_adopted() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let mut invalid = pages[0];
        invalid.page_no = 63;
        let prefix = storage.wal().prefix().unwrap();
        assert!(matches!(
            live.begin(&mut owner, invalid).unwrap_err().cause,
            Cause::Membership
        ));
        assert_eq!((live.issued_local, live.adopted_local), (1, 0));
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        let value = live.begin(&mut owner, pages[0]).unwrap();
        assert_eq!(value.actor.id().local, 2);
        assert_eq!(value.adopted_local, 2);
        assert_eq!(state.lock().unwrap().events.len(), 2);
    });
}
#[test]
fn empty_live_issuer_exhaustion_does_not_block_retained_completion() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let txn = live.begin(&mut owner, pages[0]).unwrap();
        live.issued_local = u64::MAX;
        assert!(matches!(
            live.begin(&mut owner, pages[0]).unwrap_err().cause,
            Cause::Exhausted
        ));
        live.rollback_start(&mut owner, txn.actor).unwrap();
        assert_eq!(
            live.abort_end(&mut owner, txn.actor).unwrap().remaining_wal,
            0
        );
        assert!(!storage.session_fence().is_fenced());
        assert_eq!(state.lock().unwrap().events.len(), 6);
    });
}
#[test]
fn empty_live_wrong_actor_and_state_have_no_wal_or_bank_effect() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let txn = live.begin(&mut owner, pages[0]).unwrap();
        let prefix = storage.wal().prefix().unwrap();
        let bank = live.bank.to_vec();
        let other = TxnRef::new(txn.actor.id(), Generation::new(3).unwrap()).unwrap();
        assert!(matches!(
            live.rollback_start(&mut owner, other).unwrap_err().cause,
            Cause::MissingTransaction
        ));
        assert!(matches!(
            live.abort_end(&mut owner, txn.actor).unwrap_err().cause,
            Cause::WrongState
        ));
        assert!(matches!(
            live.begin(&mut owner, pages[0]).unwrap_err().cause,
            Cause::WrongState
        ));
        assert_eq!(storage.wal().prefix().unwrap(), prefix);
        assert_eq!(live.bank.as_slice(), bank);
        assert_eq!(state.lock().unwrap().events.len(), 2);
        live.rollback_start(&mut owner, txn.actor).unwrap();
        live.abort_end(&mut owner, txn.actor).unwrap();
        assert!(matches!(
            live.abort_end(&mut owner, txn.actor).unwrap_err().cause,
            Cause::MissingTransaction
        ));
    });
}
#[test]
fn empty_live_foreign_owner_and_flusher_cannot_stage() {
    fixture!(1, 1, |live,
                    core,
                    state,
                    storage,
                    pages,
                    owner,
                    other_core| {
        let mut other = other_core.try_owner(OwnerRole::Maintenance).unwrap();
        assert!(matches!(
            live.begin(&mut other, pages[0]).unwrap_err().cause,
            Cause::BindingMismatch
        ));
        assert_eq!(live.issued_local, 0);
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let error = live.begin(&mut flusher, pages[0]).unwrap_err();
        assert_eq!(error.effect, Effect::Unchanged);
        assert_eq!(live.issued_local, 1);
        assert!(state.lock().unwrap().events.is_empty());
        assert_eq!(
            live.begin(&mut owner, pages[0]).unwrap().actor.id().local,
            2
        );
    });
}
#[test]
fn empty_live_admission_reserves_all_steps_without_double_charging_completion() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let mut prefix = storage.wal().prefix().unwrap();
        let mut stamp = image_stamp(&live, 1);
        let old = Ledger {
            occupied: 1,
            free: Some(1),
            steps: 2,
            wal: 6080,
        };
        prefix.complete_lsn = u64::MAX - 4;
        assert!(matches!(
            admit(prefix, old, stamp, Change::Begin, 3)
                .unwrap_err()
                .cause,
            Cause::Exhausted
        ));
        prefix.complete_lsn = u64::MAX - 5;
        admit(prefix, old, stamp, Change::Begin, 3).unwrap();
        stamp.modification_counter = u64::MAX - 2;
        assert!(admit(prefix, old, stamp, Change::Begin, 3).is_err());
        stamp.modification_counter = u64::MAX - 3;
        prefix.complete_offset = wal::WAL_FILE_CAP - 6080;
        admit(prefix, old, stamp, Change::Rollback, 2).unwrap();
        prefix.complete_offset += 1;
        assert!(admit(prefix, old, stamp, Change::Rollback, 2).is_err());
        assert!(state.lock().unwrap().events.is_empty());
        assert_eq!(live.issued_local, 0);
    });
}
#[test]
fn empty_live_corrupt_or_noncanonical_bank_is_fatal_before_stage() {
    for damage in 0..3 {
        fixture!(2, 1, |live, core, state, storage, pages, owner| {
            let a = live.begin(&mut owner, pages[0]).unwrap();
            let prefix = storage.wal().prefix().unwrap();
            let r = row_mut(live.bank, 0);
            match damage {
                0 => r[1504] ^= 1,
                1 => {
                    r[640 + 2 * 128 + 8 + 8 * 8..640 + 2 * 128 + 8 + 8 * 8 + 8]
                        .copy_from_slice(&1u64.to_le_bytes());
                    seal(&mut r[..1536]);
                }
                _ => {
                    r[LOCATION.start + 8..LOCATION.start + 16]
                        .copy_from_slice(&pages[1].page_no.to_le_bytes());
                }
            }
            assert_eq!(
                live.rollback_start(&mut owner, a.actor).unwrap_err().effect,
                Effect::CallerMustResolve
            );
            assert!(storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().events.len(), 2);
            assert_eq!(prefix.complete_lsn, 5);
        });
    }
}
#[test]
fn empty_live_duplicate_directory_actor_is_fatal_even_with_valid_checksums() {
    fixture!(2, 1, |live, core, state, storage, pages, owner| {
        live.begin(&mut owner, pages[0]).unwrap();
        let a = page_bytes(&live, 1);
        dirty_frame::status_load_test_support::edit_content(&live.inner.inner.inner.pool, 2, |b| {
            b[16128..16352].copy_from_slice(&a[16128..16352]);
            seal(b);
        });
        assert_eq!(
            live.begin(&mut owner, pages[1]).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert_eq!(state.lock().unwrap().events.len(), 2);
        assert!(storage.session_fence().is_fenced());
    });
}
#[test]
fn empty_live_bank_owner_without_matching_home_is_fatal() {
    fixture!(2, 1, |live, core, state, storage, pages, owner| {
        let b = page_bytes(&live, 1);
        let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
        dirty_frame::status_load_test_support::edit_content(&live.inner.inner.inner.pool, 1, |a| {
            a[16128..16352].copy_from_slice(&b[16128..16352]);
            seal(a);
        });
        assert_eq!(
            live.rollback_start(&mut owner, actor).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert_eq!(state.lock().unwrap().events.len(), 2);
        assert!(storage.session_fence().is_fenced());
    });
}
#[test]
fn empty_live_valid_bank_orphan_reaches_bidirectional_seen_guard() {
    fixture!(2, 1, |live, core, state, storage, pages, owner| {
        let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
        let mut orphan = row(live.bank, 0).to_vec();
        // Independent wire mutations: outer TxnRef, core TxnRef, bookkeeping.
        for offset in [24 + 32, 160 + 8 + 32, ACTOR.start + 32] {
            orphan[offset..offset + 8].copy_from_slice(&2u64.to_le_bytes());
        }
        orphan[LOCATION.start + 8..LOCATION.start + 16]
            .copy_from_slice(&pages[1].page_no.to_le_bytes());
        seal(&mut orphan[..1536]);
        row_mut(live.bank, 1).copy_from_slice(&orphan);
        live.issued_local = 2;
        live.adopted_local = 2;
        let prefix = storage.wal().prefix().unwrap();
        let valid = ledger(live.bank, storage.profile(), prefix, live.adopted_local).unwrap();
        assert_eq!((valid.occupied, valid.steps, valid.wal), (3, 4, 12160));
        CENSUS_MISMATCH.with(|value| value.set(None));
        assert_eq!(
            live.rollback_start(&mut owner, actor).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        CENSUS_MISMATCH.with(|value| assert_eq!(value.take(), Some((1, 3))));
        assert_eq!(state.lock().unwrap().events.len(), 2);
        assert!(storage.session_fence().is_fenced());
    });
}
#[test]
fn empty_live_all_staged_failure_cuts_retain_owner_and_fence() {
    for phase in [STAGED, PAGE_INSTALLED, DESCRIPTOR_INSTALLED, DPT_INSTALLED] {
        for panic in [false, true] {
            fixture!(1, 1, |live, core, state, storage, pages, owner| {
                CUT.with(|cut| cut.set(Some((phase, panic))));
                let result = catch_unwind(AssertUnwindSafe(|| live.begin(&mut owner, pages[0])));
                if panic {
                    assert!(result.is_err());
                } else {
                    assert_eq!(
                        result.unwrap().unwrap_err().effect,
                        Effect::CallerMustResolve
                    );
                }
                assert!(storage.session_fence().is_fenced());
                assert!(live.pending.is_some());
                assert_eq!(row(live.bank, 0)[PHASE], phase);
                assert_eq!(live.adopted_local, 1);
                assert!(state.lock().unwrap().events.is_empty());
            });
        }
    }
}
#[test]
fn empty_live_actual_wal_write_and_sync_failures_cannot_return_success() {
    for at in [0, 1] {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            state.lock().unwrap().plan = Some(port::Plan {
                at,
                action: port::Action::BeforeError,
            });
            let error = live.begin(&mut owner, pages[0]).unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(live.pending.is_some());
            assert_eq!(row(live.bank, 0)[PHASE], DPT_INSTALLED);
            assert!(storage.session_fence().is_fenced());
            let events = state.lock().unwrap().events.clone();
            assert_eq!(events.len(), at + 1);
            assert_eq!(events[at].kind, if at == 0 { "write" } else { "sync" });
            assert_eq!(image_stamp(&live, 1).page_lsn.0, 3);
        });
    }
}
#[test]
fn empty_live_short_wal_write_completes_exact_suffix_before_sync() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        state.lock().unwrap().plan = Some(port::Plan {
            at: 0,
            action: port::Action::Short(17),
        });
        live.begin(&mut owner, pages[0]).unwrap();
        let s = state.lock().unwrap();
        assert_eq!(s.events.len(), 3);
        assert_eq!(s.events[0].returned, Some(17));
        assert_eq!(s.events[1].offset, s.events[0].offset + 17);
        assert_eq!(s.events[1].data, &s.events[0].data[17..]);
        assert_eq!(s.events[2].kind, "sync");
        assert!(live.pending.is_none());
        assert!(!storage.session_fence().is_fenced());
    });
}
#[test]
fn empty_live_abort_sync_failure_keeps_terminal_row_reserved() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let a = live.begin(&mut owner, pages[0]).unwrap();
        live.rollback_start(&mut owner, a.actor).unwrap();
        {
            let mut s = state.lock().unwrap();
            s.hit = 0;
            s.events.clear();
            s.plan = Some(port::Plan {
                at: 1,
                action: port::Action::BeforeError,
            });
        }
        assert_eq!(
            live.abort_end(&mut owner, a.actor).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert_eq!(row(live.bank, 0)[PHASE], DPT_INSTALLED);
        assert!(live.pending.is_some());
        assert!(storage.session_fence().is_fenced());
        assert_eq!(row(live.bank, 0)[6], 4); // terminal descriptor is not yet retired
    });
}
#[test]
fn empty_live_final_observation_rejects_mutually_consistent_wrong_terminal() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let a = live.begin(&mut owner, pages[0]).unwrap();
        live.rollback_start(&mut owner, a.actor).unwrap();
        FORGE_TERMINAL.with(|f| *f.borrow_mut() = Some(row(live.bank, 0)[..1536].to_vec()));
        assert_eq!(
            live.abort_end(&mut owner, a.actor).unwrap_err().effect,
            Effect::CallerMustResolve
        );
        assert!(live.pending.is_some());
        assert_eq!(row(live.bank, 0)[PHASE], DPT_INSTALLED);
        assert!(storage.session_fence().is_fenced());
        assert_eq!(state.lock().unwrap().events.len(), 6);
    });
}
#[test]
fn empty_live_final_observation_rejects_pending_origin_interval_and_frame_counter() {
    for damage in 0..3 {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            FINAL_DAMAGE.with(|value| value.set(Some(damage)));
            assert_eq!(
                live.begin(&mut owner, pages[0]).unwrap_err().effect,
                Effect::CallerMustResolve
            );
            assert!(live.pending.is_some());
            assert_eq!(row(live.bank, 0)[PHASE], DPT_INSTALLED);
            assert!(storage.session_fence().is_fenced());
            assert_eq!(state.lock().unwrap().events.len(), 2);
        });
    }
}
#[test]
fn empty_live_old_allocator_observer_remains_global_tip_strict() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        live.begin(&mut owner, pages[0]).unwrap();
        let prefix = storage.wal().prefix().unwrap();
        let stamp = live.allocator_snapshot(&mut owner, prefix).unwrap();
        assert!(
            live.inner
                .inner
                .inner
                .control
                .base
                .observation(storage.profile(), prefix, &stamp, false)
                .is_err()
        );
        assert_eq!(stamp.page_lsn.0, 2);
        assert_eq!(state.lock().unwrap().events.len(), 2);
    });
}
#[test]
#[ignore = "DEV3 S5: authored profile/depth matrix; intermediate executes mandatory cases only"]
fn empty_live_s5_profiles_and_transaction_depths() {
    for profile in 1..=3 {
        for depth in [1, 2, 8, 32] {
            fixture!(
                depth,
                profile,
                |live, core, state, storage, pages, owner| {
                    let actors: Vec<_> = pages
                        .iter()
                        .map(|&page| live.begin(&mut owner, page).unwrap().actor)
                        .collect();
                    for actor in actors.into_iter().rev() {
                        live.rollback_start(&mut owner, actor).unwrap();
                        live.abort_end(&mut owner, actor).unwrap();
                    }
                    assert!(live.bank.iter().all(|b| *b == 0));
                    assert!(!storage.session_fence().is_fenced());
                    assert_eq!(state.lock().unwrap().events.len(), depth * 6);
                }
            );
        }
    }
}

#[test]
fn empty_live_census_reads_nonresident_member_without_losing_dirty_selected_home() {
    fixture!(
        2,
        1,
        |live, core, state, storage, pages, owner| {
            let a = live.begin(&mut owner, pages[0]).unwrap();
            let before = page_bytes(&live, 1);
            let reads = state.lock().unwrap().reads;
            live.rollback_start(&mut owner, a.actor).unwrap();
            assert!(state.lock().unwrap().reads > reads);
            assert_ne!(page_bytes(&live, 1), before);
            assert_eq!(storage.wal().prefix().unwrap().synced_lsn, 6);
            assert!(matches!(
                live.begin(&mut owner, pages[1]).unwrap_err().cause,
                Cause::NotResident
            ));
        },
        prepare | provision | {
            let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            {
                let (_, _, mut flush) = provision.split_with_flush();
                flush.try_flush_page(&mut flusher, pages[1]).unwrap();
            }
            provision.prepare_slot(2).unwrap();
        }
    );
}
#[test]
fn empty_live_nonresident_read_failure_is_retryable_but_corruption_fences() {
    for corrupt in [false, true] {
        fixture!(
            2,
            1,
            |live, core, state, storage, pages, owner| {
                let prefix = storage.wal().prefix().unwrap();
                {
                    let mut s = state.lock().unwrap();
                    if corrupt {
                        let file = s
                            .files
                            .values_mut()
                            .find(|file| {
                                file.bytes.len() >= 3 * PAGE_BYTES
                                    && file.bytes.get(2 * PAGE_BYTES..2 * PAGE_BYTES + 4)
                                        == Some(b"XDP1")
                            })
                            .unwrap();
                        file.bytes[2 * PAGE_BYTES + 100] ^= 1;
                    } else {
                        s.read_plan = Some(port::Plan {
                            at: 0,
                            action: port::Action::BeforeError,
                        });
                    }
                }
                let error = live.begin(&mut owner, pages[0]).unwrap_err();
                assert_eq!(
                    error.effect,
                    if corrupt {
                        Effect::CallerMustResolve
                    } else {
                        Effect::Unchanged
                    }
                );
                assert!(state.lock().unwrap().events.is_empty());
                assert!(live.pending.is_none());
                assert_eq!((live.issued_local, live.adopted_local), (1, 0));
                if corrupt {
                    assert!(storage.session_fence().is_fenced());
                } else {
                    state.lock().unwrap().read_plan = None;
                    assert_eq!(storage.wal().prefix().unwrap(), prefix);
                    assert_eq!(
                        live.begin(&mut owner, pages[0]).unwrap().actor.id().local,
                        2
                    );
                }
            },
            prepare | provision | {
                let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                {
                    let (_, _, mut flush) = provision.split_with_flush();
                    flush.try_flush_page(&mut flusher, pages[1]).unwrap();
                }
                provision.prepare_slot(2).unwrap();
            }
        );
    }
}
#[test]
fn empty_live_preexisting_poison_or_fence_prevents_candidate_issuance() {
    for poison in [false, true] {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            if poison {
                dirty_frame::test_support::poison_dpt(&live.inner.inner.inner.pool);
            } else {
                storage.session_fence().fence();
            }
            assert_eq!(
                live.begin(&mut owner, pages[0]).unwrap_err().effect,
                Effect::CallerMustResolve
            );
            assert_eq!(live.issued_local, 0);
            assert!(state.lock().unwrap().events.is_empty());
        });
    }
}

#[test]
fn empty_history_exact_lifecycle_and_historical_images() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let (_, free) = live.historical_images(&mut owner, pages[0]).unwrap();
        assert!(free.contains(&literal("begin.before.page")));
        assert!(!free.contains(&literal("begin.after.page")));
        let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
        let (_, active) = live.historical_images(&mut owner, pages[0]).unwrap();
        assert!(active.contains(&literal("begin.after.page")));
        assert!(!active.contains(&literal("rollback.after.page")));
        live.rollback_start(&mut owner, actor).unwrap();
        let (_, aborting) = live.historical_images(&mut owner, pages[0]).unwrap();
        assert!(aborting.contains(&literal("rollback.after.page")));
        live.abort_end(&mut owner, actor).unwrap();
        let events = state.lock().unwrap().events.len();
        let (_, ended) = live.historical_images(&mut owner, pages[0]).unwrap();
        for name in [
            "begin.before.page",
            "begin.after.page",
            "rollback.after.page",
            "abort.after.page",
        ] {
            assert!(ended.contains(&literal(name)), "{name}");
        }
        assert_eq!(
            *ended.current(),
            content_digest(&literal("abort.after.page"))
        );
        assert_eq!(state.lock().unwrap().events.len(), events);
        assert!(!storage.session_fence().is_fenced());
    });
}

#[test]
fn empty_history_resource_gate() {
    type W = port::Writer;
    println!(
        "history={} retained={} conversion={} whole={}",
        history_call_bytes::<W>(),
        super::super::retained_bytes::<W>(),
        super::super::conversion_bytes::<W>(),
        super::super::retained_bytes::<W>() + history_call_bytes::<W>() + 65536
    );
    assert!(history_fits::<W>());
    assert!(history_fits::<xadb_platform::namespace::LinuxWriterLease>());
}

#[test]
fn empty_history_flushes_each_actual_state_and_preserves_ledger() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let mut flusher_owner = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut actor = None;
        for step in 0..4 {
            match step {
                0 => (),
                1 => actor = Some(live.begin(&mut owner, pages[0]).unwrap().actor),
                2 => {
                    live.rollback_start(&mut owner, actor.unwrap()).unwrap();
                }
                3 => {
                    live.abort_end(&mut owner, actor.unwrap()).unwrap();
                }
                _ => unreachable!(),
            }
            let bank = live.bank.to_vec();
            let expected = page_bytes(&live, 1);
            assert!(matches!(
                live.flush_status(&mut flusher_owner, pages[0]).unwrap(),
                wal::FlushProgress::Complete(_)
            ));
            assert_eq!(live.bank.as_slice(), bank);
            assert_eq!(image_stamp(&live, 1).rec_lsn, None);
            let guard = state.lock().unwrap();
            let data = &guard.files["d3-data-0000000100000001"].bytes;
            assert_eq!(&data[PAGE_BYTES..2 * PAGE_BYTES], expected.as_slice());
        }
        assert!(matches!(
            live.flush_status(&mut flusher_owner, pages[0]).unwrap(),
            wal::FlushProgress::NoDirty
        ));
        assert!(!storage.session_fence().is_fenced());
    });
}

fn history_reseal_page(bytes: &mut [u8]) {
    let end = PAGE_BYTES - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}

#[test]
fn empty_history_interleaved_actors_gap_and_late_selected_init() {
    fixture!(3, 1, |live, core, state, storage, pages, owner| {
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let a = live.begin(&mut owner, pages[0]).unwrap();
        assert!(live.begin(&mut owner, pages[0]).is_err()); // burns local2
        let b = live.begin(&mut owner, pages[2]).unwrap();
        assert_eq!(b.actor.id().local, 3);
        live.rollback_start(&mut owner, a.actor).unwrap();
        live.rollback_start(&mut owner, b.actor).unwrap();
        live.abort_end(&mut owner, b.actor).unwrap();
        live.abort_end(&mut owner, a.actor).unwrap();
        for page in [pages[0], pages[2], pages[1]] {
            assert!(matches!(
                live.flush_status(&mut dedicated, page).unwrap(),
                wal::FlushProgress::Complete(_)
            ));
        }
        assert!(live.bank.iter().all(|b| *b == 0));
        assert!(!storage.session_fence().is_fenced());
        let _ = &state;
    });
}

#[test]
fn empty_history_rejects_checksum_valid_nonhistorical_home() {
    for variant in 0..4 {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
            let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
            live.rollback_start(&mut owner, actor).unwrap();
            live.abort_end(&mut owner, actor).unwrap();
            let mut old = literal("begin.after.page");
            match variant {
                0 => old[64..72].copy_from_slice(&2u64.to_le_bytes()), // real page state at wrong prior LSN
                1 => old[64..72].copy_from_slice(&5u64.to_le_bytes()), // equal-LSN different state
                2 => old[64..72].copy_from_slice(&6u64.to_le_bytes()), // future
                3 => old[16128 + 32 + 40..16128 + 32 + 48].copy_from_slice(&99u64.to_le_bytes()), // other local id
                _ => unreachable!(),
            }
            history_reseal_page(&mut old);
            let name = "d3-data-0000000100000001";
            state.lock().unwrap().files.get_mut(name).unwrap().bytes[PAGE_BYTES..2 * PAGE_BYTES]
                .copy_from_slice(&old);
            let error = live.flush_status(&mut dedicated, pages[0]).unwrap_err();
            assert_eq!(error.effect, Effect::CallerMustResolve, "variant{variant}");
            assert!(storage.session_fence().is_fenced());
            assert_eq!(
                &state.lock().unwrap().files[name].bytes[PAGE_BYTES..2 * PAGE_BYTES],
                old
            );
        });
    }
}

#[test]
fn empty_history_selected_init_and_transaction_semantic_damage_rejects() {
    // selected INIT magic/version/opcode/count/native/descriptor/target and
    // Begin/Rollback/Abort op/native/prior/budget damage, with valid envelope SHA.
    let cases = [
        (1, 160),
        (1, 164),
        (1, 166),
        (1, 20),
        (1, 14816),
        (1, 352 + 128),
        (1, 21520 + 8),
        (2, 1888),
        (2, 2272),
        (2, 2720 + 48),
        (3, 352 + 128),
        (3, 1888),
        (4, 2272),
        (4, 2720 + 48),
    ];
    for (frame_index, byte) in cases {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
            live.rollback_start(&mut owner, actor).unwrap();
            live.abort_end(&mut owner, actor).unwrap();
            let mut guard = state.lock().unwrap();
            let bytes = &mut guard.files.get_mut(wal::WAL_NAME).unwrap().bytes;
            let mut at = 4096;
            for _ in 0..frame_index {
                at += u32::from_le_bytes(bytes[at + 12..at + 16].try_into().unwrap()) as usize;
            }
            let len = u32::from_le_bytes(bytes[at + 12..at + 16].try_into().unwrap()) as usize;
            bytes[at + byte] ^= 1;
            port::seal_frame(&mut bytes[at..at + len]);
            let writes = guard.events.len();
            drop(guard);
            let error = match live.historical_images(&mut owner, pages[0]) {
                Err(e) => e,
                Ok(_) => panic!("accepted {frame_index}/{byte}"),
            };
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(storage.session_fence().is_fenced());
            assert!(storage.wal().file.try_lock().is_ok());
            assert_eq!(state.lock().unwrap().events.len(), writes);
        });
    }
}

#[test]
fn empty_history_wal_read_retry_busy_and_poison() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
        state.lock().unwrap().read_plan = Some(port::Plan {
            at: 0,
            action: port::Action::BeforeError,
        });
        let error = match live.historical_images(&mut owner, pages[0]) {
            Err(e) => e,
            Ok(_) => panic!("read error"),
        };
        assert_eq!(error.effect, Effect::Unchanged);
        assert!(!storage.session_fence().is_fenced());
        state.lock().unwrap().read_plan = None;
        let held = storage.wal().file.lock().unwrap();
        assert!(matches!(
            live.historical_images(&mut owner, pages[0]),
            Err(EmptyTransactionError {
                cause: Cause::Busy,
                effect: Effect::Unchanged
            })
        ));
        drop(held);
        live.historical_images(&mut owner, pages[0]).unwrap();
        live.rollback_start(&mut owner, actor).unwrap();
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _held = storage.wal().file.lock().unwrap();
            panic!("poison WAL read lock");
        }));
        assert!(matches!(
            live.historical_images(&mut owner, pages[0]),
            Err(EmptyTransactionError {
                effect: Effect::CallerMustResolve,
                ..
            })
        ));
        assert!(storage.session_fence().is_fenced());
    });
}

#[test]
fn empty_history_complete_prefix_truncation_and_checksum_damage_rejects() {
    for variant in 0..5 {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            live.begin(&mut owner, pages[0]).unwrap();
            let mut guard = state.lock().unwrap();
            let entry = guard.files.get_mut(wal::WAL_NAME).unwrap();
            let bytes = &mut entry.bytes;
            match variant {
                0 => {
                    bytes.truncate(4096 + 159);
                }
                1 => {
                    bytes.truncate(bytes.len() - 1);
                }
                2 => {
                    bytes[4096 + 128] ^= 1;
                }
                3 => {
                    bytes[4096 + 170] ^= 1;
                }
                4 => {
                    let len = u32::from_le_bytes(bytes[4108..4112].try_into().unwrap()) as usize;
                    bytes[4096 + 56..4096 + 64].copy_from_slice(&9u64.to_le_bytes());
                    port::seal_frame(&mut bytes[4096..4096 + len]);
                }
                _ => unreachable!(),
            }
            entry.length = bytes.len() as u64;
            let writes = guard.events.len();
            drop(guard);
            let result = live.historical_images(&mut owner, pages[0]);
            let error = match result {
                Err(error) => error,
                Ok(_) => panic!("accepted variant{variant}"),
            };
            assert_eq!(error.effect, Effect::CallerMustResolve);
            assert!(storage.session_fence().is_fenced());
            let reads = state.lock().unwrap().reads;
            assert!(live.begin(&mut owner, pages[0]).is_err());
            let guard = state.lock().unwrap();
            assert_eq!((guard.events.len(), guard.reads), (writes, reads));
            println!("HISTORY_PREFIX_VARIANT_PASS {variant}");
            let _ = &core;
        });
    }
}

#[test]
fn empty_history_rejects_self_consistent_but_nonhistorical_bank() {
    fixture!(2, 1, |live, core, state, storage, pages, owner| {
        let a = live.begin(&mut owner, pages[0]).unwrap().actor; // LSN5
        live.begin(&mut owner, pages[1]).unwrap(); // LSN6
        live.rollback_start(&mut owner, a).unwrap(); // LSN7
        let prefix = storage.wal().prefix().unwrap();
        let (_, _, mut d, _) =
            descriptor(row(live.bank, 0), storage.profile(), prefix, false).unwrap();
        d.chain.birth = 6;
        d.chain.previous = 6;
        d.minimum_wal_lsn = 6;
        d.inventory.change_lsn = 6;
        crate::storage::obligation::encode_descriptor(&d, &mut row_mut(live.bank, 0)[..1536])
            .unwrap();
        let ledger = ledger(live.bank, storage.profile(), prefix, live.adopted_local).unwrap();
        assert!(
            live.census(&mut owner, prefix, pages[0], ledger.occupied)
                .is_ok()
        );
        let writes = state.lock().unwrap().events.len();
        assert!(matches!(
            live.historical_images(&mut owner, pages[0]),
            Err(EmptyTransactionError {
                effect: Effect::CallerMustResolve,
                ..
            })
        ));
        assert!(storage.session_fence().is_fenced());
        assert_eq!(state.lock().unwrap().events.len(), writes);
        let _ = &core;
    });
}

#[test]
fn empty_history_invalid_read_count_fences_and_short_reads_complete() {
    for action in [
        port::Action::InvalidCount,
        port::Action::Short(1),
        port::Action::Interrupted,
    ] {
        fixture!(1, 1, |live, core, state, storage, pages, owner| {
            state.lock().unwrap().read_plan = Some(port::Plan { at: 0, action });
            let result = live.historical_images(&mut owner, pages[0]);
            if action == port::Action::InvalidCount {
                assert!(matches!(
                    result,
                    Err(EmptyTransactionError {
                        effect: Effect::CallerMustResolve,
                        ..
                    })
                ));
                assert!(storage.session_fence().is_fenced());
            } else {
                assert!(result.is_ok());
                assert!(!storage.session_fence().is_fenced());
            }
            let guard = state.lock().unwrap();
            assert_eq!(guard.hit, 1);
            assert!(guard.events.is_empty());
            let _ = &core;
        });
    }
}

#[test]
fn empty_history_flush_faults_retain_original_authority() {
    // Six selected mutation ordinals of the existing 31-effect DWB protocol;
    // both before/after errors run with a new live occupied Status authority.
    for cut in [1, 7, 15, 21, 27, 31] {
        for action in [port::Action::BeforeError, port::Action::AfterError] {
            fixture!(1, 1, |live, core, state, storage, pages, owner| {
                live.begin(&mut owner, pages[0]).unwrap();
                let bank = live.bank.to_vec();
                let stamp = image_stamp(&live, 1);
                let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                {
                    let mut guard = state.lock().unwrap();
                    guard.events.clear();
                    guard.plan = Some(port::Plan {
                        at: cut - 1,
                        action,
                    });
                }
                let error = live.flush_status(&mut dedicated, pages[0]).unwrap_err();
                assert_eq!(error.effect, Effect::CallerMustResolve);
                assert!(storage.session_fence().is_fenced());
                assert_eq!(live.bank.as_slice(), bank);
                assert_eq!(image_stamp(&live, 1), stamp);
                let writes = state.lock().unwrap().events.len();
                assert_eq!(writes, cut);
                assert_eq!(state.lock().unwrap().hit, 1);
                assert!(live.continue_status_flush(&mut dedicated).is_err());
                assert!(live.begin(&mut owner, pages[0]).is_err());
                assert_eq!(state.lock().unwrap().events.len(), writes);
                println!("HISTORY_FLUSH_FAULT_PASS {cut} {action:?}");
            });
        }
    }
}

#[test]
#[ignore = "DEV3 S5 expanded live history profiles and selected states"]
fn empty_history_s5_profiles_and_selected_states() {
    for profile in 1..=3 {
        for selected_state in 0..4 {
            fixture!(3, profile, |live, core, state, storage, pages, owner| {
                let actor =
                    (selected_state > 0).then(|| live.begin(&mut owner, pages[2]).unwrap().actor);
                if selected_state > 1 {
                    live.rollback_start(&mut owner, actor.unwrap()).unwrap();
                }
                if selected_state > 2 {
                    live.abort_end(&mut owner, actor.unwrap()).unwrap();
                }
                let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                let before = page_bytes(&live, 3);
                assert!(matches!(
                    live.flush_status(&mut dedicated, pages[2]).unwrap(),
                    wal::FlushProgress::Complete(_)
                ));
                assert_eq!(image_stamp(&live, 3).rec_lsn, None);
                assert_eq!(
                    &state.lock().unwrap().files["d3-data-0000000100000001"].bytes
                        [3 * PAGE_BYTES..4 * PAGE_BYTES],
                    before.as_slice()
                );
                assert!(!storage.session_fence().is_fenced());
                println!("HISTORY_S5_PASS {profile} {selected_state}");
            });
        }
    }
}

#[test]
fn empty_history_retired_authority_continues_without_rewriting() {
    fixture!(1, 1, |live, core, state, storage, pages, owner| {
        let actor = live.begin(&mut owner, pages[0]).unwrap().actor;
        let bank = live.bank.to_vec();
        let (selection, history) = live.historical_images(&mut owner, pages[0]).unwrap();
        let (arrived, ack) = std::sync::mpsc::sync_channel(1);
        let (resume, release) = std::sync::mpsc::sync_channel(1);
        {
            let mut guard = state.lock().unwrap();
            guard.events.clear();
            guard.pause = Some(port::Pause {
                at: 30,
                after: true,
                entered: arrived,
                release,
                unwind: false,
            });
        }
        {
            let (_, reader, mut flush) = live.inner.inner.split_with_flush();
            let mut read_owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let held = std::thread::scope(|scope| {
                let worker = scope.spawn(|| {
                    let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
                    flush
                        .try_flush_history(&mut dedicated, pages[0], history)
                        .unwrap()
                });
                ack.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
                // The old provisioning StatusReader validates INIT/Free. Hold
                // the actual pool read guard for this live occupied target.
                let held = reader
                    .pool
                    .reserve_target_set(&mut read_owner, &[selection.request])
                    .unwrap()
                    .try_read()
                    .unwrap();
                resume.send(()).unwrap();
                assert_eq!(
                    worker.join().unwrap(),
                    wal::FlushProgress::RetiredPendingFinalize
                );
                held
            });
            held.release_checked().unwrap();
        }
        let writes = state.lock().unwrap().events.len();
        assert_eq!(writes, 31);
        let mut dedicated = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        for error in [
            live.begin(&mut owner, pages[0]).unwrap_err(),
            live.rollback_start(&mut owner, actor).unwrap_err(),
            live.flush_status(&mut dedicated, pages[0]).unwrap_err(),
        ] {
            assert!(matches!(error.cause, Cause::Busy));
            assert_eq!(error.effect, Effect::Unchanged);
        }
        assert_eq!(live.bank.as_slice(), bank);
        assert!(matches!(
            live.continue_status_flush(&mut dedicated).unwrap(),
            wal::FlushProgress::Complete(_)
        ));
        assert_eq!(state.lock().unwrap().events.len(), writes);
        assert_eq!(image_stamp(&live, 1).rec_lsn, None);
        assert!(!storage.session_fence().is_fenced());
    });
}
