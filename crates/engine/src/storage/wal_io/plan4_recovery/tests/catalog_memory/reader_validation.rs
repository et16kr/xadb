//! Read the actual v2 owner; frozen bytes are observations or corruption input.
use super::super::super::super::plan4_pages::{RetentionCause, RetentionPhase};
use super::*;
use crate::storage::{
    dirty_frame::{init_test_support, status_load_test_support},
    retention_page,
};

fn exercise(pages: &[Plan4Page], corruption: Option<bool>) {
    let oracle = Oracle::new();
    with_fresh!(oracle, |live,
                         owner,
                         flusher,
                         status,
                         undo,
                         private,
                         reader_private,
                         core,
                         fence,
                         state| {
        let _ = (status, undo, private, reader_private, core);
        let expected = map(&oracle.gap.checkpoints["MC03-L05"][3]);
        for page in pages {
            let target = live.target(*page).unwrap();
            let mut cursor = Plan4ResidencyCursor::new();
            let mut resident = false;
            for _ in 0..32 {
                match live.residency_step(&target, &mut cursor).unwrap() {
                    RetentionResidencyStep::Resident { .. } => {
                        resident = true;
                        break;
                    }
                    RetentionResidencyStep::FlushRequired { target, .. } => {
                        drive_flush(live, flusher, Some(&target));
                    }
                }
            }
            assert!(resident);
            let number = match page {
                Plan4Page::Root => 1,
                Plan4Page::Jobs { index } => 2 + u64::from(*index),
                _ => unreachable!(),
            };
            let index = resident_index(live, number).unwrap();
            if let Some(old_format) = corruption {
                if old_format {
                    let root = number == 1;
                    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("../../tests/fixtures/dev3-plan4-empty-retention/vectors/p1-o00/references")
                            .join(if root { "P01.bin" } else { "P02.bin" });
                    let bytes = read(&path, PAGE_BYTES);
                    assert_eq!(
                        hex(&bytes),
                        if root {
                            "eba8bb02bacd91999b00f9891322b8cfebfb222004aa7b0f9332142834858f9c"
                        } else {
                            "49dafbbf9fdd3e8c8d93e073170114602d50a86e5f82697f4de7ba5232f4f0b3"
                        }
                    );
                    let bytes: &[u8; PAGE_BYTES] = bytes.as_slice().try_into().unwrap();
                    let namespace = super::super::support::namespace();
                    // The input is a valid checksummed v1 page, not random damage.
                    if root {
                        retention_page::decode_root_page(bytes, namespace).unwrap();
                    } else {
                        retention_page::decode_jobs_page(bytes, namespace, 0).unwrap();
                    }
                    status_load_test_support::edit_content(
                        live.catalog_test_pool(),
                        index,
                        |actual| actual.copy_from_slice(bytes),
                    );
                } else {
                    status_load_test_support::edit_content(
                        live.catalog_test_pool(),
                        index,
                        |actual| actual[200] ^= 1,
                    );
                }
            }
            let files = state.lock().unwrap().images();
            let wal = live.catalog_test_wal_state();
            let mut bytes_before = boxed::<PAGE_BYTES>();
            let before = probe::diagnose(live.catalog_test_pool(), index, &mut bytes_before);
            {
                let (_, reader, _) = live.split();
                match (reader.read(owner, &target), corruption) {
                    (Ok(guard), None) => {
                        assert_eq!(
                            guard.bytes().unwrap().as_slice(),
                            oracle.gap.raw(&expected[&(number as usize)])
                        );
                        guard.release_checked().unwrap();
                    }
                    (Err(error), Some(old_format)) => {
                        assert_eq!(error.phase, RetentionPhase::Census);
                        assert_eq!(error.effect, Effect::CallerMustResolve);
                        if old_format {
                            assert!(matches!(
                                error.cause,
                                RetentionCause::Retention(
                                    retention_page::RetentionError::UnsupportedFormat
                                )
                            ));
                        } else {
                            assert!(matches!(error.cause, RetentionCause::Retention(_)));
                        }
                    }
                    (Err(error), None) => panic!("v2 read failed: {error:?}"),
                    (Ok(_), Some(_)) => panic!("invalid page accepted"),
                }
            }
            let mut bytes_after = boxed::<PAGE_BYTES>();
            let after = probe::diagnose(live.catalog_test_pool(), index, &mut bytes_after);
            assert_eq!(bytes_before, bytes_after);
            assert_eq!(
                (
                    before.frame,
                    before.dirty,
                    before.entry,
                    before.pins,
                    before.total
                ),
                (
                    after.frame,
                    after.dirty,
                    after.entry,
                    after.pins,
                    after.total
                )
            );
            assert_eq!(
                init_test_support::claim_state(live.catalog_test_pool()),
                (false, 0)
            );
            assert_eq!(state.lock().unwrap().images(), files);
            assert_eq!(live.catalog_test_wal_state(), wal);
            assert_eq!(fence.is_fenced(), corruption.is_some());
            if corruption.is_none() {
                owner.check_idle().unwrap();
            }
        }
    });
}

#[test]
fn catalog_cr01_v2_root_read() {
    case("CR01", || exercise(&[Plan4Page::Root], None));
}

#[test]
fn catalog_cr02_v2_jobs_read() {
    case("CR02", || {
        exercise(
            &(0..5)
                .map(|index| Plan4Page::Jobs { index })
                .collect::<Vec<_>>(),
            None,
        )
    });
}

#[test]
fn catalog_cr03_mixed_version_rejected() {
    case("CR03", || {
        // Each fatal input uses a separate fresh owner. No retry after fencing.
        for page in [Plan4Page::Root, Plan4Page::Jobs { index: 0 }] {
            exercise(&[page], Some(true));
        }
    });
}

#[test]
fn catalog_cr04_corruption_rejected() {
    case("CR04", || {
        for page in [Plan4Page::Root, Plan4Page::Jobs { index: 0 }] {
            exercise(&[page], Some(false));
        }
    });
}
