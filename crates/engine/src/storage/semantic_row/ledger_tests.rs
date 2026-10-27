use super::*;
use crate::storage::control;
use foundation::codec::content_digest;
const COUNT: usize = 19;
const JOBS: &[u8; COUNT * 65536] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/ledger/jobs.bin"
));
const HOMES: &[u8; COUNT * 224] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/ledger/homes.bin"
));
const DESCRIPTORS: &[u8; COUNT * 1536] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/ledger/descriptors.bin"
));
const BUDGETS: &[u8; COUNT * 640] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/ledger/budgets.bin"
));
const PROFILES: &[u8; 12288] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
fn profile() -> Profile {
    control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap()
}
fn jobs(n: usize) -> &'static [[u8; 1024]; 64] {
    JOBS[n * 65536..(n + 1) * 65536]
        .as_chunks::<1024>()
        .0
        .try_into()
        .unwrap()
}
fn home(n: usize) -> StatusHome {
    StatusHome::decode(&HOMES[n * 224..(n + 1) * 224]).unwrap()
}
fn location() -> StatusSlotRef {
    StatusSlotRef::decode(&jobs(0)[0][80..120]).unwrap()
}
fn descriptor(n: usize) -> &'static [u8; 1536] {
    DESCRIPTORS[n * 1536..(n + 1) * 1536].try_into().unwrap()
}
fn credit(n: usize) -> bool {
    matches!(n, 1..=5)
}
fn get(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn put(bytes: &mut [u8], at: usize, v: u64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn residual(n: usize, bank: &[[u8; 1024]; 64]) -> Result<[RemainingBudget; 5]> {
    transaction_remaining(
        profile(),
        home(n),
        location(),
        JobCells::original(bank),
        credit(n),
    )
}
fn validate(n: usize, bytes: &[u8; 1536]) -> Result<()> {
    validate_transaction(
        bytes,
        profile(),
        home(n),
        location(),
        JobCells::original(jobs(n)),
    )
}
fn changed_descriptor(n: usize, f: impl FnOnce(&mut obligation::DescriptorSpec)) -> [u8; 1536] {
    let TransactionStatusCell::Occupied(status) = home(n).core() else {
        unreachable!()
    };
    let mut spec = obligation::decode_descriptor(
        descriptor(n),
        ReservationOwner::Transaction(status.reference()),
        profile_reference(profile()),
    )
    .unwrap();
    f(&mut spec);
    let mut out = [0; 1536];
    obligation::encode_descriptor(&spec, &mut out).unwrap();
    out
}
fn changed_home(n: usize, f: impl FnOnce(&mut [u8; 224])) -> StatusHome {
    let mut bytes: [u8; 224] = HOMES[n * 224..(n + 1) * 224].try_into().unwrap();
    f(&mut bytes);
    StatusHome::decode(&bytes).unwrap()
}
fn assert_rows(n: usize, actual: [RemainingBudget; 5]) {
    for (i, r) in actual.iter().enumerate() {
        let values = [
            r.steps,
            r.wal,
            r.control,
            r.growth,
            r.pages,
            r.undo_slots,
            r.status_slots,
            r.peak.frames,
            r.peak.memory,
            r.peak.locks,
            r.peak.registry,
            r.peak.files,
            r.peak.doublewrite,
            r.protected,
        ];
        for (j, v) in values.iter().enumerate() {
            assert_eq!(
                *v,
                get(BUDGETS, n * 640 + i * 128 + 8 + j * 8),
                "case {n} row {i} field {j}"
            );
        }
    }
}
#[test]
fn row_ledger_literal_states_and_residuals() {
    for n in 0..COUNT {
        assert_rows(n, residual(n, jobs(n)).unwrap());
        if matches!(n, 12 | 13 | 15) {
            let terminal = row::decode_job(&jobs(n)[0], profile().spec().namespace).unwrap();
            for cell in &jobs(n)[1..] {
                let job = row::decode_job(cell, profile().spec().namespace).unwrap();
                if job.state().is_releasing() && job.release_cursor() > 0 {
                    assert!(
                        job.last_step() > terminal.adoption_lsn(),
                        "postterminal case {n}"
                    );
                }
            }
        }
        if !matches!(n, 12 | 13 | 15) {
            assert_eq!(validate(n, descriptor(n)), Ok(()), "case {n}");
        }
    }
    assert_eq!(residual(4, jobs(4)).unwrap()[0].steps, 2);
    assert_eq!(residual(5, jobs(5)).unwrap()[1].undo_slots, 1);
    assert_eq!(
        residual(11, jobs(11)).unwrap(),
        [RemainingBudget::default(); 5]
    );
}
#[test]
fn row_ledger_actor_and_terminal_identity() {
    let free = StatusHome::new(
        TransactionStatusCell::Free(foundation::identity::Generation::new(2).unwrap()),
        0,
    )
    .unwrap();
    assert_eq!(
        transaction_remaining(
            profile(),
            free,
            location(),
            JobCells::original(jobs(0)),
            false
        ),
        Err(Error::IdentityMismatch)
    );
    let mut old = PROFILES[..4096].to_vec();
    old[4] = 2;
    let h = content_digest(&old[..4064]);
    old[4064..].copy_from_slice(&h);
    let old = control::decode_bootstrap_profile(&old, None).unwrap();
    assert_eq!(
        transaction_remaining(old, home(0), location(), JobCells::original(jobs(0)), false),
        Err(Error::IdentityMismatch)
    );
    let mut loc = jobs(0)[0][80..120].to_vec();
    put(&mut loc, 8, 10);
    assert_eq!(
        transaction_remaining(
            profile(),
            home(0),
            StatusSlotRef::decode(&loc).unwrap(),
            JobCells::original(jobs(0)),
            false
        ),
        Err(Error::IdentityMismatch)
    );
    let mut bank = *jobs(4);
    put(&mut bank[1], 56, 3);
    put(&mut bank[1], 112, 3);
    assert_eq!(residual(4, &bank), Err(Error::IdentityMismatch));
    let mut bank = *jobs(0);
    bank[1] = bank[0];
    assert_eq!(residual(0, &bank), Err(Error::IdentityMismatch));
    let mut bank = *jobs(0);
    bank[0] = bank[63];
    assert_eq!(residual(0, &bank), Err(Error::IdentityMismatch));
    let mut bank = *jobs(0);
    put(&mut bank[0], 856, 9);
    assert_eq!(residual(0, &bank), Err(Error::IdentityMismatch));
    let mut bank = *jobs(11);
    put(&mut bank[0], 264, 8);
    assert_eq!(residual(11, &bank), Err(Error::IdentityMismatch));
    let mut bank = *jobs(14);
    bank[0] = jobs(11)[0];
    assert_eq!(residual(14, &bank), Err(Error::IdentityMismatch));
}
#[test]
fn row_ledger_root_order_head_and_counts() {
    for field in [168, 320] {
        let mut bank = *jobs(4);
        put(&mut bank[2], field, 1);
        assert_eq!(
            residual(4, &bank),
            Err(Error::IdentityMismatch),
            "field {field}"
        );
    }
    let mut bank = *jobs(4);
    put(&mut bank[2], 136, 12);
    assert_eq!(residual(4, &bank), Err(Error::IdentityMismatch));
    for (which, field, value) in [(2, 256, 1), (1, 248, 3)] {
        let mut bank = *jobs(4);
        put(&mut bank[which], field, value);
        assert_eq!(residual(4, &bank), Err(Error::OrderMismatch));
    }
    let mut bank = *jobs(4);
    for at in [784, 880] {
        let v = get(&bank[2], at);
        put(&mut bank[2], at, v - 128);
    }
    assert_eq!(residual(4, &bank), Err(Error::OrderMismatch));
    let mut bank = *jobs(5);
    put(&mut bank[1], 168, 3);
    put(&mut bank[1], 256, 3);
    put(&mut bank[1], 248, 2);
    assert_eq!(residual(5, &bank), Err(Error::OrderMismatch));
    for changed in [
        changed_home(4, |b| put(b, 8, 1)),
        changed_home(4, |b| put(b, 112, 12)),
    ] {
        assert_eq!(
            transaction_remaining(
                profile(),
                changed,
                location(),
                JobCells::original(jobs(4)),
                true
            ),
            Err(Error::IdentityMismatch)
        );
    }
    let mut bank = *jobs(5);
    for at in [784, 880] {
        let v = get(&bank[2], at);
        put(&mut bank[2], at, v - 128);
    }
    assert_eq!(residual(5, &bank), Err(Error::OrderMismatch));
}
#[test]
fn row_ledger_rollback_and_cleanup_states() {
    let changed = changed_home(7, |b| put(b, 152, 14));
    assert_eq!(
        transaction_remaining(
            profile(),
            changed,
            location(),
            JobCells::original(jobs(7)),
            false
        ),
        Err(Error::OrderMismatch)
    );
    let changed = changed_home(7, |b| put(b, 192, 12));
    assert_eq!(
        transaction_remaining(
            profile(),
            changed,
            location(),
            JobCells::original(jobs(7)),
            false
        ),
        Err(Error::OrderMismatch)
    );
    let mut bank = *jobs(6);
    bank[2] = jobs(13)[2];
    assert_eq!(residual(6, &bank), Err(Error::OrderMismatch));
    let mut bank = *jobs(6);
    bank[2] = jobs(12)[2];
    assert_eq!(residual(6, &bank), Err(Error::OrderMismatch));
    let mut bank = *jobs(12);
    put(&mut bank[2], 264, 8);
    assert_eq!(residual(12, &bank), Err(Error::OrderMismatch));
    let mut bank = *jobs(14);
    bank[1] = jobs(3)[1];
    assert_eq!(residual(14, &bank), Err(Error::OrderMismatch));
    let mut bank = *jobs(9);
    bank[2] = jobs(5)[2];
    assert_eq!(residual(9, &bank), Err(Error::OrderMismatch));
}
#[test]
fn row_ledger_exact_budget_and_statement_credit() {
    assert_eq!(
        transaction_remaining(
            profile(),
            home(2),
            location(),
            JobCells::original(jobs(2)),
            false
        ),
        Err(Error::BudgetMismatch)
    );
    assert_eq!(
        transaction_remaining(
            profile(),
            home(7),
            location(),
            JobCells::original(jobs(7)),
            true
        ),
        Err(Error::BudgetMismatch)
    );
    for row in 0..5 {
        let bad = changed_descriptor(5, |s| {
            if s.remaining[row].steps == 0 {
                s.remaining[row].steps = 1;
            }
            s.remaining[row].wal += 1;
            s.remaining[row].protected += 1;
        });
        assert_eq!(validate(5, &bad), Err(Error::BudgetMismatch), "row {row}");
    }
    let bad = changed_descriptor(5, |s| {
        s.remaining[1].undo_slots = 2;
        s.inventory.items = 2;
    });
    assert_eq!(validate(5, &bad), Err(Error::BudgetMismatch));
    let bad = changed_descriptor(5, |s| {
        s.remaining[3].steps = 1;
        s.remaining[3].wal = 3040;
        s.remaining[3].protected = 3040;
    });
    assert_eq!(validate(5, &bad), Err(Error::BudgetMismatch));
    let bad = changed_descriptor(4, |s| s.remaining[0].peak.registry = 15);
    assert_eq!(validate(4, &bad), Err(Error::BudgetMismatch));
    let bad = changed_descriptor(11, |s| {
        s.remaining[0] = RemainingBudget {
            steps: 1,
            wal: 91281,
            protected: 91281,
            peak: row::ROW_PEAK,
            ..RemainingBudget::default()
        }
    });
    assert_eq!(validate(11, &bad), Err(Error::BudgetMismatch));
}
#[test]
fn row_ledger_descriptor_identity_and_order() {
    for field in [96, 128, 160] {
        let mut bad = *descriptor(4);
        bad[field] ^= 1;
        let hash = content_digest(&bad[..1504]);
        bad[1504..].copy_from_slice(&hash);
        assert!(validate(4, &bad).is_err(), "field {field}");
    }
    for bad in [
        changed_descriptor(4, |s| {
            s.chain.birth = 2;
            s.chain.previous = 2;
            s.minimum_wal_lsn = 2;
            s.inventory.change_lsn = 2;
        }),
        changed_descriptor(4, |s| s.minimum_wal_lsn = 2),
        changed_descriptor(4, |s| s.consumed_control = 1),
        changed_descriptor(4, |s| s.inventory.change_lsn = 0),
    ] {
        assert_eq!(validate(4, &bad), Err(Error::IdentityMismatch));
    }
    for bad in [
        changed_descriptor(4, |s| s.chain.apply = 20),
        changed_descriptor(4, |s| s.last_mutation_sequence = 1),
    ] {
        assert_eq!(validate(4, &bad), Err(Error::OrderMismatch));
    }
    for bad in [
        changed_descriptor(5, |s| s.chain.apply = 21),
        changed_descriptor(5, |s| s.inventory.change_lsn = 19),
        changed_descriptor(5, |s| s.last_mutation_sequence = 3),
    ] {
        assert_eq!(validate(5, &bad), Err(Error::OrderMismatch));
    }
    assert_eq!(
        validate(11, &changed_descriptor(11, |s| s.chain.apply = 51)),
        Err(Error::OrderMismatch)
    );
    let mut bad = *descriptor(4);
    bad[608] ^= 1;
    assert!(validate(4, &bad).is_err());
}
#[test]
fn row_ledger_terminal_descriptor_lifetime_and_gaps() {
    for n in [12, 13, 15] {
        assert_eq!(
            residual(n, jobs(n)).unwrap(),
            [RemainingBudget::default(); 5]
        );
    }
    assert_eq!(
        validate_transaction(
            descriptor(11),
            profile(),
            home(12),
            location(),
            JobCells::original(jobs(12))
        ),
        Err(Error::IdentityMismatch)
    );
    assert_eq!(
        validate_transaction(
            descriptor(14),
            profile(),
            home(15),
            location(),
            JobCells::original(jobs(15))
        ),
        Err(Error::OrderMismatch)
    );
    assert_eq!(validate(16, descriptor(16)), Ok(()));
    assert_eq!(
        validate(4, &changed_descriptor(4, |s| s.last_mutation_sequence = 9)),
        Ok(())
    );
    let mut bank = *jobs(4);
    put(&mut bank[2], 168, 9);
    put(&mut bank[2], 256, 9);
    put(&mut bank[2], 248, 9);
    assert_rows(4, residual(4, &bank).unwrap());
}
#[test]
fn row_ledger_observation_scope_and_borrowed_replacement() {
    // Inventory digest identity requires the physical inventory owner later.
    let bad = changed_descriptor(2, |s| s.inventory.digest = [0x91; 32]);
    assert_eq!(validate(2, &bad), Ok(()));
    // Other actors are locally decoded, but this actor observer does not prove
    // their Terminal census or whole-bank physical uniqueness.
    let mut bank = *jobs(0);
    bank[1] = jobs(3)[1];
    put(&mut bank[1], 48, 2);
    put(&mut bank[1], 88, 16);
    assert_rows(0, residual(0, &bank).unwrap());
    bank[1][4] = 2;
    assert!(residual(0, &bank).is_err());
    let view = JobCells::original(&bank).replace(1, &jobs(0)[1]).unwrap();
    assert_eq!(
        transaction_remaining(profile(), home(0), location(), view, false).unwrap(),
        residual(0, jobs(0)).unwrap()
    );
}

#[test]
fn row_ledger_full_abort_next_must_match_head() {
    for changed in [
        changed_home(9, |bytes| put(bytes, 192, 12)),
        changed_home(9, |bytes| {
            bytes[96] &= !4;
            bytes[184..224].fill(0);
        }),
    ] {
        assert_eq!(
            transaction_remaining(
                profile(),
                changed,
                location(),
                JobCells::original(jobs(9)),
                false
            ),
            Err(Error::OrderMismatch)
        );
    }
}
