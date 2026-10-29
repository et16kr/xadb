//! S1-b: the combined Profile3 closure over a mixed Catalog+Row bank, the
//! interim Row analysis gate (XADB-MODULE-ROW-BEGIN-001, "replay와 분석
//! gate") through the actual recovery owner, and the Row arm's source census.
//! Base inputs are the frozen base4-closure literals (sha256-pinned, read at
//! run time). Row groups appended to an actual fresh WAL are produced by the
//! scope3 producer from the actual replayed before state; nothing a product
//! function returns is a golden.
use super::super::super::plan4_pages::{
    EmptyRetentionControlLoan, Plan4Page, Plan4ResidencyCursor, RETENTION_CONTROL_BYTES,
    RetentionResidencyStep, VisibilityLoan,
};
use super::super::super::plan4_pages::{
    RETENTION_REPLAY_BYTES, retention_recovery_source_census, retention_replay_bank,
    retention_replay_cells, retention_replay_closure, retention_replay_image,
    retention_replay_row_scope, seed_recovery_replay,
};
use super::faults::{rejected, unchanged_error};
use super::support::*;
use super::*;
use crate::storage::{
    control::{ControlRef, SelectedControl, row_profile},
    dirty_frame::DirtyFrame,
    retention_page::row::{self as job3, JobScope, JobState},
    semantic_row::{
        decode_layout,
        transition::{self, Change, Inputs, JOB, ROOT, STATUS},
    },
    transaction_record::{StatusSlotRef, TransactionStatusCell, TxnRef},
    wal_frame::{FrameExpectation, WalStreamIdentity},
    wal_io::FreshStorageSession,
};
use crate::transaction::concurrency::{OwnerRole, PageConcurrency};
use foundation::identity::{Generation, ObjectId};
use std::path::Path;

const FIXTURES: &str = "tests/fixtures/module-row-format";
const GENESIS: (&str, &str) = (
    "base4-closure/genesis.xrb",
    "9b7c330e6af39c640dfc55b9836d4c7a42c7846c0139af6111b7b30be05e8c32",
);
const AGING: (&str, &str) = (
    "base4-closure/aging.xrb",
    "38e96da47fddb73821b9844f6ca36e41df7ab649442727c52df386ab8459e25f",
);
/// Committed Catalog CREATE, a finished Row INSERT and one Active Row actor
/// with two Retained roots (base4-closure manifest `live`).
const LIVE: (&str, &str) = (
    "base4-closure/live.xrb",
    "5a446fbf0fab629b7bfc10b5040e0a54b77ea6e2a993ca58db70482f3f5c94d9",
);
const PROFILES: (&str, &str) = (
    "envelopes/profiles.bin",
    "e8f5b232f8583c50d697db8ba29d920c10e5cc3c81a939f2cd4289edc0d5767a",
);
/// The Catalog writer's private loan (`catalog_memory::PRIVATE_BYTES`).
const PRIVATE_BYTES: usize = 425_984;
/// Bank row geometry of the replay control ledger (`plan4_retention::control`).
const ROW: usize = 1664;
const LOCATION: std::ops::Range<usize> = 1536..1576;
const ACTOR: std::ops::Range<usize> = 1576..1624;
const PHASE: usize = 1624;
const AGING_ROW: usize = 32;

fn fixture((name, sha): (&str, &str)) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(FIXTURES)
        .join(name);
    let raw = std::fs::read(&path).expect("frozen input");
    assert!(raw.len() <= base::row::BASE_BYTES, "{}", path.display());
    let digest: String = content_digest(&raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(digest, sha, "pin {}", path.display());
    raw
}
fn profile() -> control::Profile {
    row_profile::decode_profile(&fixture(PROFILES)[..4096], None).unwrap()
}
fn boxed_replay() -> Box<[u8; RETENTION_REPLAY_BYTES]> {
    vec![0xa5; RETENTION_REPLAY_BYTES]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
/// The actual seed (images, Job refresh, ledger rows, combined closure) of a
/// verified base witness, as selection hands it to analysis.
fn seeded(bytes: &[u8], genesis: &[u8]) -> Box<[u8; RETENTION_REPLAY_BYTES]> {
    let profile = profile();
    let expected = base::Expected {
        profile,
        filemap: SelectedControl {
            generation: 1,
            reference: ControlRef::decode(&genesis[384..448]).unwrap(),
        },
        bootstrap_digest: genesis[464..496].try_into().unwrap(),
    };
    let mut scratch = Box::new([[0; PAGE_BYTES]; 4]);
    let view = base::row::decode(bytes, expected, &mut scratch).unwrap();
    let base = VerifiedPlan4Base {
        view,
        digest: view.digest(),
    };
    let mut replay = boxed_replay();
    seed_recovery_replay(profile, Some(&base), &mut replay)
        .unwrap_or_else(|error| panic!("seed {error:?}"));
    replay
}
fn bank_row(replay: &[u8; RETENTION_REPLAY_BYTES], slot: usize) -> &[u8] {
    &retention_replay_bank(replay)[slot * ROW..(slot + 1) * ROW]
}
/// The bank's byte offset inside the replay (the bank follows the 64 Jobs).
fn bank_at(slot: usize) -> usize {
    1_048_576 + 65_536 + slot * ROW
}
/// The unique Row TerminalLeased actor and its stable ATT row slot.
fn row_actor(replay: &[u8; RETENTION_REPLAY_BYTES]) -> (TxnRef, StatusSlotRef, usize) {
    let profile = profile();
    let mut found = None;
    for cell in retention_replay_cells(replay) {
        let job = job3::decode_job(cell, profile.spec().namespace).unwrap();
        if job.scope() == Some(JobScope::Row) && job.state() == JobState::TerminalLeased {
            assert!(found.is_none(), "one Row actor");
            found = Some((job.source().unwrap(), job.status().unwrap()));
        }
    }
    let (actor, status) = found.expect("Row actor");
    let slots: Vec<_> = (0..32)
        .filter(|slot| {
            let row = bank_row(replay, *slot);
            row[PHASE] == 1 && TxnRef::decode(&row[ACTOR]).ok() == Some(actor)
        })
        .collect();
    assert_eq!(slots.len(), 1);
    (actor, status, slots[0])
}

/// H06: the mixed live base (Catalog CREATE committed, Row INSERT finished,
/// one Active Row actor) is seeded through the combined closure and its Row
/// state is observed by the gate; Catalog-only genesis and AgingStart are
/// not. Ledger damage on the Row side is refused by the same closure.
#[test]
fn s1b_mixed_bank_passes_combined_closure_and_gate_observation() {
    let genesis = fixture(GENESIS);
    let p = profile();
    for (name, pin, row) in [
        ("genesis", GENESIS, false),
        ("aging", AGING, false),
        ("live", LIVE, true),
    ] {
        let replay = seeded(&fixture(pin), &genesis);
        retention_replay_closure(p, &replay).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let present = retention_replay_row_scope(p, &replay).unwrap();
        let mut census = [0usize; 3];
        for cell in retention_replay_cells(&replay) {
            let job = job3::decode_job(cell, p.spec().namespace).unwrap();
            match (job.state(), job.scope()) {
                (JobState::Free, _) => census[0] += 1,
                (_, Some(JobScope::Catalog)) => census[1] += 1,
                (_, Some(JobScope::Row)) => census[2] += 1,
                (_, None) => panic!("{name}: scopeless non-Free Job"),
            }
        }
        println!(
            "S1B_H06\t{name}\tfree={}\tcatalog={}\trow={}\tgate={present}",
            census[0], census[1], census[2]
        );
        assert_eq!(present, row, "{name}");
        assert_eq!(census[2] > 0, row, "{name}");
    }

    let live = seeded(&fixture(LIVE), &genesis);
    let (actor, status, slot) = row_actor(&live);
    let empty = (0..32)
        .find(|i| bank_row(&live, *i).iter().all(|b| *b == 0))
        .unwrap();
    let mut other = [0; 40];
    StatusSlotRef::new(
        status.page(),
        status.slot(),
        Generation::new(status.generation().get() + 1).unwrap(),
    )
    .unwrap()
    .encode(&mut other)
    .unwrap();
    let mutations: [(&str, Box<dyn Fn(&mut [u8; RETENTION_REPLAY_BYTES])>); 5] = [
        (
            "att-cleared",
            Box::new(move |r| r[bank_at(slot)..bank_at(slot + 1)].fill(0)),
        ),
        (
            "att-duplicated",
            Box::new(move |r| r.copy_within(bank_at(slot)..bank_at(slot + 1), bank_at(empty))),
        ),
        (
            "att-location",
            Box::new(move |r| {
                r[bank_at(slot) + LOCATION.start..bank_at(slot) + LOCATION.end]
                    .copy_from_slice(&other)
            }),
        ),
        (
            "att-descriptor",
            Box::new(move |r| r[bank_at(slot) + 700] ^= 1),
        ),
        (
            "att-unstable",
            Box::new(move |r| r[bank_at(slot) + PHASE] = 0),
        ),
    ];
    for (name, mutate) in mutations {
        let mut replay = live.clone();
        mutate(&mut replay);
        let result = retention_replay_closure(p, &replay);
        println!("S1B_H06_REJECT\t{name}\t{result:?}");
        assert!(result.is_err(), "{name}");
        // Damage never hides Row state from the gate's observation.
        assert!(
            !matches!(retention_replay_row_scope(p, &replay), Ok(false)),
            "{name}"
        );
    }
    // An ATT row carrying the Row peak recipe alone is Row state for the gate,
    // and without its Terminal the closure refuses it.
    let mut replay = seeded(&fixture(GENESIS), &genesis);
    assert!(bank_row(&replay, 0).iter().all(|b| *b == 0));
    replay[bank_at(0)..bank_at(1)].copy_from_slice(bank_row(&live, slot));
    assert_eq!(retention_replay_row_scope(p, &replay).ok(), Some(true));
    assert!(retention_replay_closure(p, &replay).is_err());
    assert_eq!(TxnRef::decode(&bank_row(&replay, 0)[ACTOR]).unwrap(), actor);
}

/// A minimal stabilized scope3 state between groups, taken from the actual
/// replay and advanced only by installing each produced group's afters.
struct RowWorld {
    profile: control::Profile,
    lsn: u64,
    root: [u8; PAGE_BYTES],
    jobs: [u8; PAGE_BYTES],
    status: [u8; PAGE_BYTES],
    cells: Box<[[u8; 1024]; 64]>,
    transaction: Option<[u8; 1536]>,
    aging: [u8; 1536],
}
impl RowWorld {
    fn from_replay(
        replay: &[u8; RETENTION_REPLAY_BYTES],
        lsn: u64,
        profile: control::Profile,
    ) -> Self {
        let aging: [u8; 1536] = bank_row(replay, AGING_ROW)[..1536].try_into().unwrap();
        assert!(aging.iter().any(|b| *b != 0), "Aging descriptor");
        Self {
            profile,
            lsn,
            root: *retention_replay_image(replay, 1),
            jobs: *retention_replay_image(replay, 2),
            status: *retention_replay_image(replay, 9),
            cells: Box::new(*retention_replay_cells(replay)),
            transaction: None,
            aging,
        }
    }
    fn produce(&mut self, change: Change<'_>, jobs: bool, status: bool) -> Vec<u8> {
        let profile = self.profile;
        let inputs = Inputs {
            profile,
            frame: FrameExpectation {
                stream: WalStreamIdentity {
                    database: namespace().database,
                    timeline: 1,
                    writer_epoch: 1,
                },
                previous_lsn: self.lsn,
            },
            change,
            jobs: &self.cells,
            root: &self.root,
            jobs_page: jobs.then_some(&self.jobs),
            status: status.then_some(&self.status),
            undo: None,
            previous: None,
            table: None,
            transaction: match change {
                Change::Begin { .. } => None,
                _ => self.transaction.as_ref(),
            },
            aging: Some(&self.aging),
        };
        let mut after = Box::new([[0; PAGE_BYTES]; 5]);
        let mut frame = vec![0; 98_304];
        let observation = transition::produce(&inputs, &mut after, &mut frame)
            .unwrap_or_else(|e| panic!("produce {:?}: {e:?}", change.operation()));
        frame.truncate(observation.frame_bytes);
        let layout = decode_layout(&frame, inputs.frame, profile).unwrap();
        for &role in &observation.order[..usize::from(observation.count)] {
            let image = after[usize::from(role)];
            match usize::from(role) {
                ROOT => self.root = image,
                JOB => {
                    self.jobs = image;
                    let page = job3::decode_jobs_page(&image, namespace(), 0).unwrap();
                    for slot in 0..15 {
                        self.cells[slot].copy_from_slice(page.cell(slot).unwrap());
                    }
                }
                STATUS => self.status = image,
                other => panic!("unexpected role {other}"),
            }
        }
        let op = layout.operation();
        if op.handoff() {
            self.transaction = Some(*layout.descriptor(0).unwrap());
            self.aging = *layout.descriptor(1).unwrap();
        } else if op.status_patch() {
            self.transaction = Some(*layout.descriptor(0).unwrap());
        } else {
            self.aging = *layout.descriptor(0).unwrap();
        }
        assert_eq!(layout.header().lsn, self.lsn + 1);
        self.lsn += 1;
        println!(
            "S1B_H05_GROUP\t{:?}\tlsn={}\tbytes={}\tjob={:?}",
            observation.operation, self.lsn, observation.frame_bytes, observation.job_ordinal
        );
        frame
    }
}

/// An actual fresh Profile3 database: the current Catalog writer's AgingStart
/// (LSN1) and Status INIT (LSN2..3), with the allocator, Root, Jobs0 and
/// Status flushed. Only actual owner methods run (the loan layout is
/// `catalog_memory::with_fresh`'s); no frozen oracle is compared here.
fn fresh_aging() -> Images {
    let d = port::Directory::fresh();
    {
        let state = d.state();
        let mut state = state.lock().unwrap();
        state.live_writes = true;
        state.recovery_writes = true;
    }
    let state = d.state();
    {
        let mut slot = boxed::<1_048_576>();
        let mut maintenance = boxed::<12_582_912>();
        let mut dwb = boxed::<4_194_304>();
        let storage = FreshStorageSession::create_retention_catalog(
            d.acquire_writer().unwrap(),
            1,
            &mut slot,
            &mut maintenance,
        )
        .unwrap()
        .install_doublewrite(&mut dwb)
        .unwrap()
        .into_mutable_storage();
        let core = PageConcurrency::new(storage.session_fence()).unwrap();
        let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
        let mut flusher = core.try_owner(OwnerRole::DedicatedFlusher).unwrap();
        let mut backing = vec![[0xa7; PAGE_BYTES]; 9];
        let mut frames: Vec<_> = backing.iter_mut().map(DirtyFrame::new).collect();
        let mut operation = boxed::<114_688>();
        let (prefix, visibility) = maintenance.split_at_mut(12_574_720);
        let (prefix, private) = prefix.split_at_mut(12_148_736);
        let (prefix, isolation) = prefix.split_at_mut(12_144_576);
        let (prefix, epochs) = prefix.split_at_mut(12_142_976);
        let isolation: &mut [u8; 4_160] = isolation.try_into().unwrap();
        let (prefix, control) = prefix.split_at_mut(6_815_744);
        let replay: &mut [u8; RETENTION_REPLAY_BYTES] = (&mut prefix
            [4_194_304 - PRIVATE_BYTES..5_767_168 - PRIVATE_BYTES])
            .try_into()
            .unwrap();
        let control: &mut [u8; RETENTION_CONTROL_BYTES] = (&mut control[..RETENTION_CONTROL_BYTES])
            .try_into()
            .unwrap();
        let private: &mut [u8; PRIVATE_BYTES] = private.try_into().unwrap();
        let pages = storage
            .bind_plan4_pages(&core, &mut frames, &mut operation)
            .unwrap();
        let Ok(mut live) = pages.into_empty_retention(
            &mut owner,
            EmptyRetentionControlLoan {
                control,
                replay,
                isolation,
            },
        ) else {
            panic!("actual fresh activation");
        };
        live.enable_catalog_visibility(
            &mut owner,
            VisibilityLoan {
                bytes: visibility.try_into().unwrap(),
                epochs: epochs.try_into().unwrap(),
            },
        )
        .unwrap();
        let aging = live.catalog_aging_start(&mut owner, private).unwrap();
        assert_eq!(aging.header.lsn, 1);
        let mut cursor = Plan4ResidencyCursor::new();
        let mut prepared = false;
        for _ in 0..32 {
            match live.prepare_status_slot(&owner, &mut cursor).unwrap() {
                None => {
                    prepared = true;
                    break;
                }
                Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                    drive_flush(&mut live, &mut flusher, Some(&target))
                }
                Some(_) => panic!("unexpected Status slot preparation"),
            }
        }
        assert!(prepared);
        let reserved = live.reserve_status(&mut owner).unwrap();
        assert_eq!(reserved.page(), page_ref(9, 2));
        let Ok(status) = reserved.initialize() else {
            panic!("actual INIT");
        };
        drive_flush(&mut live, &mut flusher, None);
        for target in [
            live.target(Plan4Page::Root).unwrap(),
            live.target(Plan4Page::Jobs { index: 0 }).unwrap(),
            status,
        ] {
            drive_flush(&mut live, &mut flusher, Some(&target));
        }
    }
    assert_closed(&state);
    images(&state)
}

/// The actual owner's preinspection (analysis) of `files`, before any later
/// phase: its result and the replay it leaves in the original maintenance
/// loan. No mutation is attempted and no file changes.
#[allow(clippy::type_complexity, reason = "one test observation tuple")]
fn preinspected(
    files: &Images,
) -> (
    std::result::Result<u64, Plan4RecoveryError>,
    Box<[u8; RETENTION_REPLAY_BYTES]>,
    control::Profile,
) {
    let d = clone_directory(files);
    let state = d.state();
    let mut maintenance = boxed::<12_582_912>();
    let mut dwb = boxed::<4_194_304>();
    let mut slot = boxed::<1_048_576>();
    let result = {
        let mut owner =
            Plan4Recovery::acquire(&d, Some(namespace()), &mut maintenance, &mut dwb, &mut slot)
                .ok()
                .unwrap();
        let result = owner
            .state
            .preinspect()
            .map(|()| owner.observation().source_lsn);
        assert_eq!(owner.observation().mutations_attempted, 0);
        let replay: Box<[u8; RETENTION_REPLAY_BYTES]> = owner.state.maintenance
            [REPLAY_START..REPLAY_END]
            .to_vec()
            .into_boxed_slice()
            .try_into()
            .unwrap();
        (result, replay, owner.state.profile())
    };
    assert_closed(&state);
    assert_eq!(images(&state), *files);
    result
}

/// H05: Row groups appended after the actual fresh Catalog history replay
/// through the Row arm and stop at the interim gate: Analysis/Unsupported,
/// database Unchanged, no mutation, the original owner kept (fenced) and no
/// READY; every file is unchanged. A normally aborted Row actor (Terminal
/// Ready, no ATT row) is gated the same way.
#[test]
fn s1b_row_state_stops_at_interim_analysis_gate() {
    let files = fresh_aging();
    let (lsn, replay, profile) = preinspected(&files);
    let lsn = lsn.unwrap();
    assert_eq!(lsn, 3);
    assert_eq!(
        retention_replay_row_scope(profile, &replay).ok(),
        Some(false)
    );
    let status_page = crate::storage::status_home::decode_status_home_page(
        retention_replay_image(&replay, 9),
        crate::storage::page::PageIdentity {
            database: namespace().database,
            page: page_ref(9, 2),
            owner: ObjectId::new(2).unwrap(),
            owner_generation: Generation::new(1).unwrap(),
        },
        namespace(),
    )
    .unwrap();
    let TransactionStatusCell::Free(previous) = status_page.cell(0).unwrap().core() else {
        panic!("fresh Status 9 is Free after the Catalog history");
    };
    let generation = Generation::new(previous.get() + 1).unwrap();
    let location = StatusSlotRef::new(page_ref(9, 2), 0, generation).unwrap();
    let actor = TxnRef::new(
        crate::transaction::TxnId {
            database: namespace().database,
            timeline: 1,
            writer_epoch: 1,
            local: 2,
        },
        generation,
    )
    .unwrap();
    let begin = Change::Begin {
        actor,
        status: location,
        scope: JobScope::Row,
    };
    for (variant, terminal_ready) in [("begin", false), ("aborted", true)] {
        let mut world = RowWorld::from_replay(&replay, lsn, profile);
        let mut wal = files[wal::WAL_NAME].clone();
        wal.extend(world.produce(begin, true, true));
        if terminal_ready {
            let ordinal = (0..64)
                .find(|i| {
                    job3::decode_job(&world.cells[*i], namespace())
                        .is_ok_and(|j| j.source() == Some(actor))
                })
                .unwrap();
            wal.extend(world.produce(
                Change::RollbackStart {
                    terminal: ordinal,
                    statement: None,
                },
                false,
                true,
            ));
            wal.extend(world.produce(Change::AbortEnd { terminal: ordinal }, true, true));
            let job = job3::decode_job(&world.cells[ordinal], namespace()).unwrap();
            assert_eq!(
                (job.state(), job.scope()),
                (JobState::TerminalReady, Some(JobScope::Row))
            );
        }
        let mut input = files.clone();
        input.insert(wal::WAL_NAME.to_owned(), wal);
        // The Unsupported is the gate's: every Row group was replayed through
        // the Row arm and the combined closure, and the replay holds exactly
        // the produced Row state at the last LSN when analysis stops.
        let (result, gated, _) = preinspected(&input);
        let error = result.expect_err("gated");
        assert!(matches!(error.cause, Plan4RecoveryCause::Unsupported));
        assert_eq!(*retention_replay_cells(&gated), *world.cells, "{variant}");
        assert_eq!(retention_replay_image(&gated, 1), &world.root, "{variant}");
        assert_eq!(
            retention_replay_image(&gated, 9),
            &world.status,
            "{variant}"
        );
        assert_eq!(retention_replay_row_scope(profile, &gated).ok(), Some(true));
        retention_replay_closure(profile, &gated).unwrap();
        let d = clone_directory(&input);
        let state = d.state();
        rejected(&d, &state, |error, observation| {
            println!(
                "S1B_H05\t{variant}\tphase={:?}\tcause={:?}\teffect={:?}\tdatabase={:?}\tmutations={}/{}",
                error.phase,
                error.cause,
                error.effect,
                error.database_effect,
                observation.mutations_attempted,
                observation.mutations_completed
            );
            unchanged_error(error, observation, Plan4RecoveryPhase::Analysis);
            assert!(
                matches!(error.cause, Plan4RecoveryCause::Unsupported),
                "{variant}"
            );
        });
        assert_eq!(images(&state), input, "{variant}: every file unchanged");
    }
}

fn census<W: WriterNamespace>(label: &str) {
    let live = retention_recovery_source_census::<W>();
    let recovered = source_census::<W>();
    let phases = live
        .history_retention
        .max(live.history_population)
        .max(live.history_rollback)
        .max(live.history_frame_return)
        .max(live.history_closure);
    println!(
        "S1B_ROW_CENSUS\t{label}\trow={}\tphase_max={phases}\tframe={}\tclosure={}\tframe_return={}\tpopulation={}\tlive_peak={}\tcatalog_lookup={}\tcatalog_commit={}\trecovered_analysis={}\trecovered_peak={}",
        live.history_row,
        live.recovery_frame,
        live.history_closure,
        live.history_frame_return,
        live.history_population,
        live.peak,
        live.catalog_lookup,
        live.catalog_commit,
        recovered.analysis,
        recovered.peak,
    );
    // The Row arm is exclusive with the Catalog/System arms of one frame: it
    // fits under the existing frame-phase maximum, so the history frame and
    // every consumer of it keep their values.
    assert!(
        live.history_row <= phases,
        "{label}: Row arm above frame max"
    );
    assert_eq!(live.recovery_frame, phases, "{label}: frame changed");
    assert!(live.history_closure <= live.recovery_frame);
    assert!(live.peak <= 65_536 && recovered.peak <= 65_536);
}

/// C01: memory writer census (S1-b Row arm and combined closure).
#[test]
fn s1b_row_arm_census_memory() {
    census::<port::Writer>("actual-memory");
}

/// C02: native writer census.
#[cfg(target_os = "linux")]
#[test]
fn s1b_row_arm_census_native() {
    census::<super::super::super::plan4_pages::native::Writer>("actual-native");
}

/// C03: the Row arm and the gate use only the original caller loans: the
/// replay tail lent to either observer is the same WORK_END tail, and no new
/// file handle, caller buffer or temporary file is charged.
#[test]
fn s1b_row_arm_keeps_original_loans_and_handles() {
    for (label, c, live) in [
        (
            "memory",
            source_census::<port::Writer>(),
            retention_recovery_source_census::<port::Writer>(),
        ),
        #[cfg(target_os = "linux")]
        (
            "native",
            source_census::<super::super::super::plan4_pages::native::Writer>(),
            retention_recovery_source_census::<super::super::super::plan4_pages::native::Writer>(),
        ),
    ] {
        println!(
            "S1B_ROW_LOANS\t{label}\ttemporary_files={}\tmaintenance={}\twal={}\tdwb={}\twriter_replay={}\tcontrol={}",
            c.temporary_files,
            c.caller_maintenance,
            c.caller_wal,
            c.caller_dwb,
            live.caller_writer_replay,
            live.caller_control
        );
        assert_eq!(c.temporary_files, 2, "{label}");
        assert_eq!(
            (c.caller_maintenance, c.caller_wal, c.caller_dwb),
            (12_582_912, 1_048_576, 4_194_304),
            "{label}"
        );
        assert_eq!(
            (live.caller_writer_replay, live.caller_control),
            (RETENTION_REPLAY_BYTES, 196_608),
            "{label}"
        );
    }
}

#[path = "row_gate/mixed.rs"]
mod mixed;
