//! `decode_catalog_cell` against the independent Job3 literals: a valid Row
//! cell is skipped (Ok(None)), Free/Catalog keep the exact view, and every
//! decode error stays the Job3 decoder's own error.
use super::*;
use crate::api::DatabaseId;
use crate::storage::retention_page::{JOB_COUNT, put};

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
/// Literal scope from raw bytes only: kind3 roots and registry16 Terminals are
/// Row; state0 is Free. No product decoder decides the expectation.
fn raw_row(bytes: &[u8]) -> bool {
    bytes[6] != 0 && (bytes[7] == 3 || (bytes[7] == 1 && get(bytes, 384 + 3 * 128 + 88) == 16))
}
fn same_error(input: &[u8], namespace: Namespace) {
    let expected = row::decode_job(input, namespace).err();
    assert!(expected.is_some());
    assert_eq!(decode_catalog_cell(input, namespace).err(), expected);
}

#[test]
fn catalog_cell_skips_row_and_keeps_catalog_views() {
    let mut seen = [0usize; 3];
    for i in 0..31 {
        let bytes = cell(i);
        let row = (4..=6).contains(&i) || i >= 19;
        assert_eq!(raw_row(&bytes), row);
        match decode_catalog_cell(&bytes, ns()).unwrap() {
            None => {
                assert!(row);
                assert_eq!(
                    decode_job(&bytes, ns()).err(),
                    Some(RetentionError::UnsupportedState)
                );
                let job3 = row::decode_job(&bytes, ns()).unwrap();
                assert_eq!(job3.scope(), Some(row::JobScope::Row));
                seen[2] += 1;
            }
            Some(view) => {
                assert!(!row);
                let exact = decode_job(&bytes, ns()).unwrap();
                assert_eq!(view.bytes().as_ptr(), bytes.as_ptr());
                assert_eq!(view.state(), exact.state());
                assert_eq!(view.job3().scope(), exact.job3().scope());
                seen[usize::from(i != 0)] += 1;
            }
        }
    }
    assert_eq!(seen, [1, 15, 15]);
}

#[test]
fn catalog_cell_keeps_every_job3_error() {
    for i in [0, 7, 19, 23] {
        for length in [0, 1, 7, 383, JOB_BYTES - 1] {
            same_error(&cell(i)[..length], ns());
        }
        let mut long = [0; JOB_BYTES + 1];
        long[..JOB_BYTES].copy_from_slice(&cell(i));
        same_error(&long, ns());
        for version in [0u16, 2, 4, u16::MAX] {
            let mut bad = cell(i);
            bad[4..6].copy_from_slice(&version.to_le_bytes());
            same_error(&bad, ns());
        }
        let mut bad = cell(i);
        bad[0] ^= 1;
        same_error(&bad, ns());
        let mut wrong = ns();
        wrong.timeline = 2;
        if i != 0 {
            same_error(&cell(i), wrong);
        }
    }
    for i in [7, 19, 23] {
        for (state, kind) in [(0, 1), (1, 0), (2, 0), (0, 3), (5, 3), (1, 4)] {
            let mut bad = cell(i);
            bad[6] = state;
            bad[7] = kind;
            same_error(&bad, ns());
        }
        // A registry other than 9/16, and any budget byte, is never a skip.
        for registry in [0u64, 8, 10, 15, 17] {
            let mut bad = cell(i);
            put(&mut bad, 384 + 3 * 128 + 88, registry);
            same_error(&bad, ns());
        }
        for at in [384, 640, 1023] {
            let mut bad = cell(i);
            bad[at] ^= 128;
            same_error(&bad, ns());
        }
    }
    // Switching a Catalog Terminal's registry to Row (or back) without its
    // recipe is a budget error, not a silent scope change.
    for (i, registry) in [(7, 16u64), (19, 9)] {
        let mut bad = cell(i);
        put(&mut bad, 384 + 3 * 128 + 88, registry);
        same_error(&bad, ns());
    }
}

#[test]
fn catalog_cell_scans_mixed_banks() {
    for group in 0..4 {
        let mut rows = 0;
        let mut catalog = 0;
        for ordinal in 0..JOB_COUNT {
            let at = (group * JOB_COUNT + ordinal) * JOB_BYTES;
            let bytes = &JOBS[at..at + JOB_BYTES];
            match decode_catalog_cell(bytes, ns()).unwrap() {
                None => {
                    assert!(raw_row(bytes));
                    rows += 1;
                }
                Some(view) => {
                    assert!(!raw_row(bytes));
                    catalog += usize::from(view.state() != JobState::Free);
                }
            }
        }
        assert_eq!(
            (rows, catalog),
            [(7, 0), (4, 0), (5, 0), (2, 2)][group],
            "group {group}"
        );
    }
}
