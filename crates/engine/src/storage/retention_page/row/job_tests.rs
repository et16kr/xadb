//! Literal cell/census validation only: no persistent pages or actual grants.
use super::*;
use crate::api::DatabaseId;

macro_rules! literal {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/module-row-format/job3/",
            $name
        ))
    };
}
const CELLS: &[u8; 31 * JOB_BYTES] = literal!("cells.bin");
const JOBS: &[u8; 4 * JOB_COUNT * JOB_BYTES] = literal!("jobs.bin");
const ROOTS: &[u8; 4 * ROOT_BYTES] = literal!("roots.bin");
const AGGREGATES: &[u8; 4 * 640] = literal!("aggregates.bin");

fn ns() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn cell(i: usize) -> [u8; JOB_BYTES] {
    CELLS[i * JOB_BYTES..(i + 1) * JOB_BYTES]
        .try_into()
        .unwrap()
}
fn group(i: usize) -> ([[u8; JOB_BYTES]; JOB_COUNT], [u8; ROOT_BYTES]) {
    let cells = std::array::from_fn(|j| {
        JOBS[(i * 64 + j) * JOB_BYTES..(i * 64 + j + 1) * JOB_BYTES]
            .try_into()
            .unwrap()
    });
    (
        cells,
        ROOTS[i * ROOT_BYTES..(i + 1) * ROOT_BYTES]
            .try_into()
            .unwrap(),
    )
}
fn rejected(input: &[u8], error: RetentionError) {
    assert_eq!(decode_job(input, ns()).err(), Some(error));
}
fn verify(root: &[u8; ROOT_BYTES], jobs: &[[u8; JOB_BYTES]; JOB_COUNT]) -> Result<()> {
    let mut scratch = [0; 640];
    verify_census(decode_root(root, ns())?, ns(), jobs, &mut scratch)
}
// Independent commitment repair for negative mutations. No Job decoder or
// product summarize_jobs is used to manufacture the expected masks/aggregate.
fn seal(root: &mut [u8; ROOT_BYTES], jobs: &[[u8; JOB_BYTES]; JOB_COUNT]) {
    let mut hash = ContentHasher::new();
    let mut masks = [0u64; 3];
    let mut rows = [0u8; 640];
    for r in 0..5 {
        rows[r * 128] = r as u8 + 1;
    }
    for (ordinal, j) in jobs.iter().enumerate() {
        masks[if j[6] == 0 {
            0
        } else if j[6] == 1 {
            1
        } else {
            2
        }] |= 1 << ordinal;
        hash.update(j);
        for r in 0..5 {
            for at in (8..120).step_by(8) {
                let offset = r * 128 + at;
                let old = get(&rows, offset);
                let value = get(j, 384 + offset);
                put(
                    &mut rows,
                    offset,
                    if (64..112).contains(&at) {
                        old.max(value)
                    } else {
                        old + value
                    },
                );
            }
        }
    }
    for (i, mask) in masks.into_iter().enumerate() {
        put(root, 96 + 8 * i, mask);
    }
    put(root, 88, u64::from((masks[1] | masks[2]).count_ones()));
    root[128..160].copy_from_slice(&hash.digest());
    root[160..192].copy_from_slice(&content_digest(&rows));
}
fn census_rejected(
    jobs: [[u8; JOB_BYTES]; JOB_COUNT],
    mut root: [u8; ROOT_BYTES],
    error: RetentionError,
) {
    seal(&mut root, &jobs);
    assert_eq!(verify(&root, &jobs), Err(error));
}

#[test]
fn job3_literal_views_and_exact_recipes() {
    for i in 0..31 {
        let bytes = cell(i);
        let view = decode_job(&bytes, ns()).unwrap();
        assert_eq!(view.bytes().as_ptr(), bytes.as_ptr());
        assert_eq!(
            view.state().mask_index(),
            if bytes[6] == 0 {
                0
            } else if bytes[6] == 1 {
                1
            } else {
                2
            }
        );
        assert_eq!(view.generation().get(), if i == 0 { 1 } else { 2 });
        let mut output = [0xa5; JOB_BYTES];
        encode_job(&bytes, ns(), &mut output).unwrap();
        assert_eq!(output, bytes);
        if i == 0 {
            assert_eq!(view.scope(), None);
            assert!(view.source().is_none() && view.status().is_none());
            assert_eq!(view.cleanup(), RemainingBudget::default());
            continue;
        }
        let row = (4..=6).contains(&i) || i >= 19;
        assert_eq!(
            view.scope(),
            Some(if row {
                JobScope::Row
            } else {
                JobScope::Catalog
            })
        );
        let debt = view.cleanup();
        assert_eq!(debt.peak, if row { ROW_PEAK } else { CATALOG_PEAK });
        let (steps, wal) = if i <= 6 {
            (1, 55112)
        } else if bytes[6] < 3 {
            let expected_c = match i {
                7 | 9 => 254,
                8 | 10 => 4600,
                19 | 21 => 185,
                20 | 22 => 2228,
                _ => unreachable!(),
            };
            assert_eq!(view.cell_bytes(), expected_c);
            assert_eq!(
                view.has_previous_transaction(),
                if row { Some(i == 20 || i == 22) } else { None }
            );
            (
                5,
                311608 + 2 * u64::from(expected_c) + if i == 20 || i == 22 { 128 } else { 0 },
            )
        } else {
            assert!(view.pair().is_none());
            assert_eq!(view.cell_bytes(), 0);
            assert_eq!(view.projection_digest(), &[0; 32]);
            assert_eq!(view.has_previous_transaction(), None);
            (
                4 - u64::from(bytes[121]),
                (4 - u64::from(bytes[121])) * 55528,
            )
        };
        assert_eq!((debt.steps, debt.wal, debt.protected), (steps, wal, wal));
        assert_eq!(
            view.row_id().map(|r| r.get()),
            if i >= 19 { Some(1) } else { None }
        );
        if i >= 7 {
            assert_eq!(
                (view.undo_sequence(), view.statement(), view.mutation()),
                (1, 1, 1)
            );
            assert_eq!(view.object().unwrap().object.get(), 6);
            assert_eq!(view.undo().unwrap().page().page_no, 12);
        }
    }
}

#[test]
fn job3_closed_grammar_and_identity_rejections() {
    for length in 0..JOB_BYTES {
        rejected(&CELLS[..length], RetentionError::Corrupt);
    }
    rejected(&[0; JOB_BYTES + 1], RetentionError::Corrupt);
    for version in [0u16, 1, 2, 4, u16::MAX] {
        let mut bad = cell(19);
        bad[4..6].copy_from_slice(&version.to_le_bytes());
        rejected(&bad, RetentionError::UnsupportedFormat);
    }
    for (state, kind) in [
        (0, 1),
        (1, 0),
        (2, 0),
        (0, 2),
        (0, 3),
        (3, 1),
        (4, 1),
        (5, 3),
        (1, 4),
    ] {
        let mut bad = cell(19);
        bad[6] = state;
        bad[7] = kind;
        rejected(&bad, RetentionError::UnsupportedState);
    }
    for at in [0, 122, 127, 284, 287, 360, 375] {
        let mut bad = cell(19);
        bad[at] ^= 1;
        rejected(&bad, RetentionError::Corrupt);
    }
    for at in [16, 79, 320, 1023] {
        let mut bad = cell(0);
        bad[at] = 1;
        rejected(&bad, RetentionError::Corrupt);
    }
    let mut wrong_ns = ns();
    wrong_ns.timeline = 2;
    assert_eq!(
        decode_job(&cell(19), wrong_ns).err(),
        Some(RetentionError::InvalidInput)
    );
    for (at, value) in [
        (32, 2),
        (88, 8),
        (88, 64),
        (112, 3),
        (136, 9),
        (160, 1),
        (176, 5),
        (184, 2),
        (192, 2),
        (208, 12),
        (232, 10),
        (320, 0),
    ] {
        let mut bad = cell(19);
        put(&mut bad, at, value);
        rejected(&bad, RetentionError::IdentityMismatch);
    }
    for (at, value) in [
        (8, 1),
        (56, 1),
        (64, 0),
        (168, 0),
        (248, 0),
        (256, 0),
        (376, 0),
    ] {
        let mut bad = cell(19);
        put(&mut bad, at, value);
        // source generation must still match Status before the ordering check.
        if at == 56 {
            put(&mut bad, 112, value);
        }
        rejected(&bad, RetentionError::OrderMismatch);
    }
    for (i, at, value) in [
        (19, 72, 3),
        (19, 264, 1),
        (19, 272, 3),
        (19, 280, 184),
        (19, 280, 2229),
        (7, 248, 2),
        (7, 256, 2),
        (21, 72, 2),
        (23, 272, 3),
        (23, 264, 0),
        (27, 264, 1),
    ] {
        let mut bad = cell(i);
        put(&mut bad, at, value);
        let error = if (i == 7 && (at == 248 || at == 256)) || i == 21 || i >= 23 {
            RetentionError::OrderMismatch
        } else {
            RetentionError::Corrupt
        };
        rejected(&bad, error);
    }
    for i in [23, 27] {
        let mut bad = cell(i);
        bad[121] = 4;
        rejected(&bad, RetentionError::OrderMismatch);
        for at in [200, 224, 280, 288] {
            let mut bad = cell(i);
            bad[at] = 1;
            rejected(&bad, RetentionError::Corrupt);
        }
    }
    for i in [1, 2, 3, 4, 5, 6, 7, 11] {
        let mut bad = cell(i);
        put(&mut bad, 320, 1);
        rejected(&bad, RetentionError::Corrupt);
    }
    for (i, at, value, error) in [
        (4, 128, 1, RetentionError::Corrupt),
        (4, 72, 2, RetentionError::Corrupt),
        (4, 264, 1, RetentionError::Corrupt),
        (4, 328, 1, RetentionError::Corrupt),
        (4, 376, 2, RetentionError::OrderMismatch),
        (5, 264, 0, RetentionError::OrderMismatch),
        (6, 264, 1, RetentionError::OrderMismatch),
        (5, 72, 1, RetentionError::OrderMismatch),
    ] {
        let mut bad = cell(i);
        put(&mut bad, at, value);
        rejected(&bad, error);
    }
    let mut bad = cell(5);
    bad[120] = 3;
    rejected(&bad, RetentionError::UnsupportedState);
}

#[test]
fn job3_budget_every_byte_is_checked() {
    // XOR128 cannot switch between the two valid registry recipes (9/16).
    for i in 1..31 {
        for at in 384..1024 {
            let mut bad = cell(i);
            bad[at] ^= 128;
            rejected(&bad, RetentionError::BudgetMismatch);
        }
    }
    // h is exactly 0 or 1, with protected equal to WAL; no inferred rounding.
    for extra in [1, 127, 129, 256] {
        let mut bad = cell(19);
        put(&mut bad, 784, 311978 + extra);
        put(&mut bad, 880, 311978 + extra);
        rejected(&bad, RetentionError::BudgetMismatch);
    }
}

#[test]
fn job3_encode_errors_preserve_output() {
    for i in [0, 4, 19, 22, 27] {
        let mut bad = cell(i);
        bad[122] = 1;
        let mut out = [0xa5; JOB_BYTES];
        assert!(encode_job(&bad, ns(), &mut out).is_err());
        assert_eq!(out, [0xa5; JOB_BYTES]);
        for len in [0, 1, JOB_BYTES - 1, JOB_BYTES + 1] {
            let mut out = vec![0xa5; len];
            assert_eq!(
                encode_job(&cell(i), ns(), &mut out),
                Err(RetentionError::InvalidInput)
            );
            assert!(out.iter().all(|v| *v == 0xa5));
        }
    }
}

#[test]
fn job3_independent_multi_root_censuses() {
    for i in 0..4 {
        let (jobs, root) = group(i);
        let mut aggregate = [0; 640];
        let view = decode_root(&root, ns()).unwrap();
        verify_census(view, ns(), &jobs, &mut aggregate).unwrap();
        assert_eq!(aggregate.as_slice(), &AGGREGATES[i * 640..(i + 1) * 640]);
        let (masks, digest) =
            summarize_jobs(JobCells::original(&jobs), ns(), &mut aggregate).unwrap();
        assert_eq!(masks, view.masks());
        assert_eq!(&digest, view.job_digest());
        assert_eq!(aggregate.as_slice(), &AGGREGATES[i * 640..(i + 1) * 640]);
        assert_eq!(get(&aggregate, 472), 16); // sequential peak=max, never summed
    }
}

#[test]
fn job3_census_rejects_identity_and_high_water_conflicts() {
    let (jobs, root) = group(0);
    for (ordinal, at, value) in [
        (2, 320, 1),
        (2, 168, 1),
        (2, 136, 12),
        (2, 136, 16),
        (5, 88, 9),
        (2, 256, 1),
        (2, 208, 18),
        (2, 232, 10),
        (2, 216, 2),
        (2, 176, 7),
    ] {
        let mut bad = jobs;
        put(&mut bad[ordinal], at, value);
        let mut expected_root = root;
        if at == 176 {
            put(&mut expected_root, 400, value);
        }
        census_rejected(bad, expected_root, RetentionError::IdentityMismatch);
    }
    for at in [392, 400, 432, 80] {
        let mut bad = root;
        put(&mut bad, at, if at == 400 { 5 } else { 1 });
        assert_eq!(verify(&bad, &jobs), Err(RetentionError::OrderMismatch));
    }
    for at in [128, 160] {
        let mut bad = root;
        bad[at] ^= 1;
        assert_eq!(verify(&bad, &jobs), Err(RetentionError::Corrupt));
    }
    let (committed, mut committed_root) = group(1);
    put(&mut committed_root, 408, 0);
    assert_eq!(
        verify(&committed_root, &committed),
        Err(RetentionError::OrderMismatch)
    );
    // A valid but different mask partition still fails exact queue agreement.
    let mut bad = root;
    put(&mut bad, 104, 117);
    put(&mut bad, 112, 10);
    assert_eq!(verify(&bad, &jobs), Err(RetentionError::Corrupt));
}

#[test]
fn job3_census_binds_root_and_job_namespace() {
    let (mut jobs, mut root) = group(0);
    let foreign = Namespace {
        database: DatabaseId::new([0x33; 16]).unwrap(),
        timeline: 1,
    };
    for j in &mut jobs {
        if j[6] != 0 {
            j[16..32].fill(0x33);
        }
        decode_job(j, foreign).unwrap();
    }
    seal(&mut root, &jobs);
    let root = decode_root(&root, ns()).unwrap();
    let mut scratch = [0; 640];
    assert_eq!(
        verify_census(root, foreign, &jobs, &mut scratch),
        Err(RetentionError::IdentityMismatch)
    );
}

#[test]
fn job3_census_checks_terminal_causal_boundaries() {
    let (jobs, root) = group(1);
    // Each mutation is individually valid as a Job; only the relationship
    // with the original terminal at LSN50 is false.
    for (ordinal, at, value, also) in [
        (1, 72, 50, Some((376, 50))), // Retained adoption must precede Commit.
        (2, 272, 50, None),           // Committed Prune must follow Commit.
        (3, 272, 50, None),           // Compensating CLR must precede terminal.
        (3, 376, 50, None),           // Cursor>0 cleanup must follow terminal.
    ] {
        let mut bad = jobs;
        put(&mut bad[ordinal], at, value);
        if let Some((at, value)) = also {
            put(&mut bad[ordinal], at, value);
        }
        decode_job(&bad[ordinal], ns()).unwrap();
        census_rejected(bad, root, RetentionError::OrderMismatch);
    }
    // Same CLR-before-terminal requirement holds for full abort.
    let (mut jobs, root) = group(2);
    put(&mut jobs[1], 272, 50);
    put(&mut jobs[1], 376, 50);
    decode_job(&jobs[1], ns()).unwrap();
    census_rejected(jobs, root, RetentionError::OrderMismatch);
}

#[test]
fn job3_census_active_actor_limit() {
    let (_, mut root) = group(0);
    let mut jobs = [cell(0); JOB_COUNT];
    for (i, j) in jobs.iter_mut().enumerate().take(32) {
        *j = cell(4);
        put(j, 48, i as u64 + 1);
        put(j, 88, i as u64 + 9);
    }
    seal(&mut root, &jobs);
    verify(&root, &jobs).unwrap();
    jobs[32] = cell(4);
    put(&mut jobs[32], 48, 33);
    put(&mut jobs[32], 88, 41);
    census_rejected(jobs, root, RetentionError::Corrupt);
}

#[test]
fn job3_census_terminal_scope_and_create_visibility() {
    let (jobs, root) = group(0);
    let mut bad = jobs;
    bad[0] = cell(0);
    census_rejected(bad, root, RetentionError::IdentityMismatch);
    let mut bad = jobs;
    bad[7] = bad[0];
    census_rejected(bad, root, RetentionError::IdentityMismatch);
    let mut bad = jobs;
    bad[0] = cell(1);
    census_rejected(bad, root, RetentionError::IdentityMismatch);
    // Second reserved root for the same actor, otherwise distinct identity.
    let mut bad = jobs;
    bad[2][6] = 1;
    put(&mut bad[2], 72, 0);
    put(&mut bad[2], 376, 2);
    bad[2][328..360].fill(0);
    census_rejected(bad, root, RetentionError::IdentityMismatch);
    let mut bad = jobs;
    bad[0] = cell(6);
    census_rejected(bad, root, RetentionError::OrderMismatch);
    // Cleanup cannot advance while its source actor is still nonterminal.
    let mut bad = jobs;
    bad[3] = cell(28);
    put(&mut bad[3], 136, 14);
    put(&mut bad[3], 168, 3);
    put(&mut bad[3], 320, 3);
    put(&mut bad[3], 256, 3);
    census_rejected(bad, root, RetentionError::OrderMismatch);
    let (mixed, mixed_root) = group(3);
    let mut bad = mixed;
    bad[0] = cell(1);
    bad[0][16..64].copy_from_slice(&mixed[0][16..64]);
    bad[0][80..120].copy_from_slice(&mixed[0][80..120]);
    census_rejected(bad, mixed_root, RetentionError::OrderMismatch);
    for lease in [49, 50] {
        let mut bad = mixed;
        put(&mut bad[3], 64, lease);
        census_rejected(bad, mixed_root, RetentionError::OrderMismatch);
    }
    // A CREATE that is still Reserved or was compensated cannot supply a
    // table to another actor, even with internally valid terminal/root cells.
    for (terminal_index, root_index) in [(1, 7), (3, 15)] {
        let mut bad = mixed;
        bad[0] = cell(terminal_index);
        bad[1] = cell(root_index);
        for i in [0, 1] {
            bad[i][16..64].copy_from_slice(&mixed[i][16..64]);
            bad[i][80..120].copy_from_slice(&mixed[i][80..120]);
        }
        put(&mut bad[1], 136, 17);
        put(&mut bad[1], 168, 8);
        census_rejected(bad, mixed_root, RetentionError::OrderMismatch);
    }
    // Catalog actor root<=1 and global duplicate Catalog table stay closed.
    let mut bad = mixed;
    bad[4] = bad[1];
    put(&mut bad[4], 136, 18);
    put(&mut bad[4], 168, 10);
    census_rejected(bad, mixed_root, RetentionError::IdentityMismatch);
    // Duplicate Catalog object across two different actors is also closed.
    let mut bad = mixed;
    bad[4] = cell(2);
    bad[5] = cell(9);
    for i in [4, 5] {
        put(&mut bad[i], 48, 3);
        put(&mut bad[i], 88, 18);
    }
    put(&mut bad[5], 136, 19);
    put(&mut bad[5], 168, 10);
    census_rejected(bad, mixed_root, RetentionError::IdentityMismatch);
}

#[test]
fn job3_selected_cell_borrows_original_census() {
    let (jobs, mut root) = group(0);
    let mut after = jobs[4];
    after[6] = 2;
    put(&mut after, 72, 5);
    put(&mut after, 376, 5);
    after[328..360].fill(0x55);
    let cells = JobCells::original(&jobs);
    assert!(cells.replace(64, &after).is_err());
    let selected = cells.replace(4, &after).unwrap();
    assert!(selected.replace(3, &after).is_err());
    assert_eq!(selected.cell(4).as_ptr(), after.as_ptr());
    assert_eq!(selected.cell(3).as_ptr(), jobs[3].as_ptr());
    let mut expected = jobs;
    expected[4] = after;
    seal(&mut root, &expected);
    let mut aggregate = [0; 640];
    verify_selected_census(
        decode_root(&root, ns()).unwrap(),
        ns(),
        selected,
        &mut aggregate,
    )
    .unwrap();
    assert_eq!(jobs[4][6], 1);
}
