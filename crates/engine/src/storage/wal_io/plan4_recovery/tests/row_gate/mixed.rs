//! S1-c: live Catalog mixed-bank tolerance of the admission check that every
//! Catalog operation runs first (`check_ledger`), at function level. Before
//! S1-d nothing puts a Row Job cell into a live owner (no Row Begin, and the
//! interim gate refuses Row state at restart), so the check runs over the
//! seeded frozen base4 literals with exactly the ATT/Aging rows and Job cells
//! a live owner holds after replay (`history::validate_live`). Expected debt
//! is summed here from each ATT descriptor and each Job cell of either scope.
use super::super::super::super::plan4_pages::{
    RetentionCause, RetentionPhase, retention_ledger_check,
};
use super::*;
use crate::storage::{
    allocation_page::ReservationOwner, obligation, retention_obligation::profile_reference,
    wal_io::WalPrefix,
};
use foundation::{error::Effect, namespace::FileIdentity};

fn prefix(writer_epoch: u32) -> WalPrefix {
    WalPrefix {
        stream: WalStreamIdentity {
            database: profile().spec().namespace.database,
            timeline: 1,
            writer_epoch,
        },
        file: FileIdentity {
            device: 0,
            inode: 0,
        },
        complete_lsn: u64::MAX,
        complete_offset: u64::MAX,
        synced_lsn: u64::MAX,
        synced_offset: u64::MAX,
    }
}

/// Every occupied ATT row's descriptor remaining plus every Job cell's
/// cleanup, as `(wal, steps)`, and the Row-scope share of that sum.
fn expected(replay: &[u8; RETENTION_REPLAY_BYTES]) -> ((u64, u64), (u64, u64)) {
    let p = profile();
    let namespace = p.spec().namespace;
    let mut row_actors = Vec::new();
    let mut total = (0, 0);
    let mut row = (0, 0);
    for cell in retention_replay_cells(replay) {
        let job = job3::decode_job(cell, namespace).unwrap();
        let debt = job.cleanup();
        total = (total.0 + debt.wal, total.1 + debt.steps);
        if job.scope() == Some(JobScope::Row) {
            row = (row.0 + debt.wal, row.1 + debt.steps);
            row_actors.push(job.source().unwrap());
        }
    }
    for slot in 0..32 {
        let bytes = bank_row(replay, slot);
        if bytes.iter().all(|b| *b == 0) {
            continue;
        }
        let actor = TxnRef::decode(&bytes[ACTOR]).unwrap();
        let descriptor = obligation::decode_descriptor(
            &bytes[..1536],
            ReservationOwner::Transaction(actor),
            profile_reference(p),
        )
        .unwrap();
        for debt in descriptor.remaining {
            total = (total.0 + debt.wal, total.1 + debt.steps);
            if row_actors.contains(&actor) {
                row = (row.0 + debt.wal, row.1 + debt.steps);
            }
        }
    }
    (total, row)
}

/// The live ATT identity rule needs the actor's epoch and adoption; a bank
/// without ATT rows needs neither.
fn admission(replay: &[u8; RETENTION_REPLAY_BYTES]) -> (u32, u64) {
    (0..32)
        .map(|slot| bank_row(replay, slot))
        .filter(|row| row.iter().any(|b| *b != 0))
        .map(|row| TxnRef::decode(&row[ACTOR]).unwrap().id())
        .fold((1, 0), |(_, local), id| {
            (id.writer_epoch, local.max(id.local))
        })
}

/// R09-style: Catalog-only banks keep their admission totals, and the mixed
/// live bank (one Active Row actor, two Retained Row roots) is admitted
/// instead of fencing every Catalog writer. Its totals include the Row
/// actor's ATT remaining and every Row cell's cleanup through the Row codec.
#[test]
fn s1c_ledger_check_admits_mixed_bank_with_row_debt() {
    let genesis = fixture(GENESIS);
    let p = profile();
    for (name, pin, row) in [
        ("genesis", GENESIS, false),
        ("aging", AGING, false),
        ("live", LIVE, true),
    ] {
        let replay = seeded(&fixture(pin), &genesis);
        let (epoch, adopted) = admission(&replay);
        let (total, share) = expected(&replay);
        let result = retention_ledger_check(p, &replay, adopted, prefix(epoch));
        println!("S1C_LEDGER\t{name}\tresult={result:?}\texpected={total:?}\trow_share={share:?}");
        assert_eq!(result.ok(), Some(total), "{name}");
        assert_eq!(share != (0, 0), row, "{name}");
    }
}

/// The Row actor's ATT row is matched to its Row TerminalLeased through the
/// Row codec in both directions, and ATT damage on the Row side is refused
/// with no effect, exactly like Catalog ATT damage.
#[test]
fn s1c_ledger_check_matches_row_terminals_to_att() {
    let genesis = fixture(GENESIS);
    let p = profile();
    let live = seeded(&fixture(LIVE), &genesis);
    let (actor, status, slot) = row_actor(&live);
    let (epoch, adopted) = (actor.id().writer_epoch, actor.id().local);
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
    let refused = |name: &str, replay: &[u8; RETENTION_REPLAY_BYTES]| {
        let result = retention_ledger_check(p, replay, adopted, prefix(epoch));
        println!("S1C_LEDGER_REJECT\t{name}\t{result:?}");
        let error = result.expect_err(name);
        assert!(
            error.phase == RetentionPhase::Census
                && matches!(error.cause, RetentionCause::Invariant)
                && error.effect == Effect::Unchanged,
            "{name}"
        );
    };
    let mutations: [(&str, Box<dyn Fn(&mut [u8; RETENTION_REPLAY_BYTES])>); 5] = [
        // A Row TerminalLeased whose ATT row is gone (reverse direction).
        (
            "att-cleared",
            Box::new(move |r| r[bank_at(slot)..bank_at(slot + 1)].fill(0)),
        ),
        (
            "att-duplicated",
            Box::new(move |r| r.copy_within(bank_at(slot)..bank_at(slot + 1), bank_at(empty))),
        ),
        // The ATT Status no longer names the Row Terminal's Status.
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
        refused(name, &replay);
    }
    // A Row ATT row without any Job of its actor (the AgingStart bank has no
    // Terminal): Catalog selection finds nothing and no Row Terminal matches.
    let mut replay = seeded(&fixture(AGING), &genesis);
    assert!(bank_row(&replay, 0).iter().all(|b| *b == 0));
    replay[bank_at(0)..bank_at(1)].copy_from_slice(bank_row(&live, slot));
    refused("att-without-terminal", &replay);
}

/// Review A1: `check_ledger` runs on admission-only paths (empty-actor and
/// Catalog terminal entries) that never run the history closure, so a Row
/// ATT descriptor must pass the Row ledger's Status-free subset there: the
/// terminal lease birth, the reserved-root items, the open terminal row and
/// the Row recipe of every debt row. Each re-encoded (still well-formed)
/// descriptor change is an Unchanged Census Invariant.
#[test]
fn s1d_ledger_check_applies_row_admission_subset() {
    let genesis = fixture(GENESIS);
    let p = profile();
    let live = seeded(&fixture(LIVE), &genesis);
    let (actor, _, slot) = row_actor(&live);
    let (epoch, adopted) = (actor.id().writer_epoch, actor.id().local);
    assert!(retention_ledger_check(p, &live, adopted, prefix(epoch)).is_ok());
    let spec = obligation::decode_descriptor(
        bank_row(&live, slot)[..1536].try_into().unwrap(),
        ReservationOwner::Transaction(actor),
        profile_reference(p),
    )
    .unwrap();
    assert!(spec.remaining[0].steps > 0, "live actor has CLR debt");
    // The subset's frame is dominated by the Catalog branch it excludes.
    assert!(
        crate::storage::semantic_row::ledger::admission_source_bytes()
            <= crate::storage::semantic_catalog::profile3_transaction_source_bytes()
    );
    // An items count without its inventory digest is not encodable, so the
    // items clause is covered only through well-formed descriptors here.
    let changes: [(&str, Box<dyn Fn(&mut obligation::DescriptorSpec)>); 4] = [
        (
            "terminal-row",
            Box::new(|s| s.remaining[2] = obligation::RemainingBudget::default()),
        ),
        (
            "catalog-recipe",
            Box::new(|s| {
                s.remaining[0].peak = crate::storage::retention_page::catalog::CREATE_PEAK
            }),
        ),
        (
            "birth",
            Box::new(|s| {
                s.chain.birth -= 1;
                s.minimum_wal_lsn -= 1;
            }),
        ),
        ("minimum", Box::new(|s| s.minimum_wal_lsn -= 1)),
    ];
    for (name, change) in changes {
        let mut changed = spec;
        change(&mut changed);
        let mut bytes = [0; 1536];
        obligation::encode_descriptor(&changed, &mut bytes)
            .unwrap_or_else(|e| panic!("{name}: encode {e:?}"));
        let mut replay = live.clone();
        replay[bank_at(slot)..bank_at(slot) + 1536].copy_from_slice(&bytes);
        let result = retention_ledger_check(p, &replay, adopted, prefix(epoch));
        println!("S1D_LEDGER_SUBSET_REJECT\t{name}\t{result:?}");
        let error = result.expect_err(name);
        assert!(
            error.phase == RetentionPhase::Census
                && matches!(error.cause, RetentionCause::Invariant)
                && error.effect == Effect::Unchanged,
            "{name}"
        );
    }
}
