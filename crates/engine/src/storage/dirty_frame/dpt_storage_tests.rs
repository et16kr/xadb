//! Private representation checks use no WAL or install authority. Actual
//! producer/flush/recovery cases exercise the changed representation separately.
use super::*;
use crate::storage::{page::PAGE_BYTES, page_io::test_support::expected};

fn with_pool(run: impl FnOnce(&SharedPool<'_, '_, '_, '_>)) {
    let mut pages = [[0; PAGE_BYTES]; 2];
    let mut frames: Vec<_> = pages.iter_mut().map(DirtyFrame::new).collect();
    frames[0]
        .register(TargetRequest {
            identity: expected(),
            kind: PageKind::RowLeaf,
            level: 0,
        })
        .unwrap();
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let binding = AtomicU8::new(2);
    let pool = SharedPool::new(&core, &fence, &binding, &frames).unwrap();
    run(&pool);
}

#[test]
fn compact_dpt_preserves_full_identity_and_generation_comparisons() {
    with_pool(|pool| {
        let mut state = pool.registry.lock().unwrap();
        let entry = DirtyEntry {
            identity: expected(),
            generation: 1,
            rec_lsn: PageLsn(7),
            modification_counter: 9,
        };
        state.entries[0] = Some(DptEntry::from_checked(&entry));
        assert!(pool.dpt_matches(&state, 0, Some(&entry)));
        assert!(!pool.dpt_matches(&state, 0, None));
        for field in 0..4 {
            let mut wrong = entry;
            match field {
                0 => wrong.identity.page.page_no += 1,
                1 => wrong.generation += 1,
                2 => wrong.rec_lsn.0 += 1,
                _ => wrong.modification_counter += 1,
            }
            assert!(!pool.dpt_matches(&state, 0, Some(&wrong)));
        }
        // Matching stored/expected scalars cannot authorize a different
        // publication generation, or an unpublished/empty slot.
        let stale = DirtyEntry {
            generation: 2,
            ..entry
        };
        state.entries[0] = Some(DptEntry::from_checked(&stale));
        assert!(!pool.dpt_matches(&state, 0, Some(&stale)));
        state.entries[1] = Some(DptEntry::from_checked(&entry));
        assert!(!pool.dpt_matches(&state, 1, Some(&entry)));
        assert!(!pool.dpt_matches(&state, 1, None));
        // No authentic requirement was made by these representation inputs.
        assert!(!pool.dpt_valid(&state));
        state.entries = [None; MAX_FRAMES];
        assert!(pool.dpt_valid(&state));
        assert!(!pool.fence.is_fenced());
    });
}

#[test]
fn compact_dpt_diagnostics_reject_unresolved_generation() {
    with_pool(|pool| {
        let mut state = pool.registry.lock().unwrap();
        let entry = DirtyEntry {
            identity: expected(),
            generation: 2,
            rec_lsn: PageLsn(7),
            modification_counter: 9,
        };
        state.entries[0] = Some(DptEntry::from_checked(&entry));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool.diagnose_dpt(&state, 0)
            }))
            .is_err()
        );
        assert_eq!(state.entries[0].unwrap().generation, 2);
        state.entries[0] = None;
    });
}

#[test]
fn compact_dpt_layout_and_borrowed_match_fit_original_physical_allowance() {
    use std::mem::size_of;
    // Original broad registration-call bound, preserved independently from
    // the new private row layout. resolve/Slot::registration/map are the only
    // nested calls in dpt_matches; expected and stored rows are borrowed.
    let original_generic = 4 * 192
        + 4 * 112
        + 2 * 96
        + 6 * size_of::<Result<Registration>>()
        + 2 * 32
        + 4 * 32
        + 32 * 8
        + 64;
    let closure = dpt_call_bytes();
    let target_charge = target_set_call_bytes::<2>();
    let prior_rows = size_of::<[Option<DirtyEntry>; MAX_FRAMES]>();
    let current_rows = size_of::<[Option<DptEntry>; MAX_FRAMES]>();
    println!(
        "COMPACT_DPT prior_rows={prior_rows} current_rows={current_rows} saved={} original_generic={original_generic} closure={closure} target_charge={target_charge} whole={} ceiling={CALL_BYTES}",
        prior_rows - current_rows,
        original_generic + closure
    );
    assert!(current_rows < prior_rows);
    assert!(original_generic + closure <= CALL_BYTES);
    assert!(target_charge >= closure && target_charge <= CALL_BYTES);
}
