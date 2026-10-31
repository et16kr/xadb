//! S9 (`#row-mi03-s9`): B's point read after the S6 restarts to READY
//! (C+A+B). `row_recovery` checks READY with C's page reader so it stays in
//! the C+A export; this module keeps B's original read under a new Row
//! snapshot for the same restarts and compares it with the page reader and
//! the history's expected values. The read below is S6's `restart_in` tail,
//! moved unchanged.
use super::super::super::RowRecoveryPolicy;
use super::row_forward::VALUE;
use super::row_recovery::{Drive, Ending, ReadyRead, image, leaf_values, restart_with};
use super::*;
use crate::storage::{
    disk::{IsolationLevel, TableStep},
    row_record::ValueRef,
};

/// B's read of RowId 1..=`reads` after READY: a Serializable Row Begin, one
/// statement, `read_point_live` per RowId, then the empty abort.
pub(super) struct BRead;

impl ReadyRead for BRead {
    fn read<'s, 'c, W: WriterNamespace>(
        &self,
        live: &mut Live<'_, 's, 'c, '_, '_, '_, '_, '_, W>,
        owner: &mut Owner<'c, 's>,
        flusher: &mut Owner<'c, 's>,
        private: &mut [u8; super::super::super::CATALOG_PRIVATE_BYTES],
        _: &Images,
        reads: u64,
    ) -> Vec<Option<i64>> {
        use crate::storage::disk::{Plan4ResidencyCursor, ReadStep};
        use crate::storage::row::read::driver::{
            PointRead,
            live::{LiveCall as ReadCall, read_point_live},
        };
        use foundation::{identity::RowId, runtime::Cancellation};
        let row_status = live.recovery_test_expected_target(11).unwrap();
        // B's Row Begin takes its Root, first Jobs page and Status resident
        // through the original empty-transaction preparation.
        let first_jobs = crate::storage::retention_page::JobSlotRef::new(
            page_ref(2, 1),
            0,
            foundation::identity::Generation::new(1).unwrap(),
        )
        .unwrap();
        let mut cursor = Plan4ResidencyCursor::new();
        for _ in 0..16 {
            match live
                .prepare_empty_targets(owner, &row_status, first_jobs, &mut cursor)
                .unwrap()
            {
                None => break,
                Some(RetentionResidencyStep::FlushRequired { target, .. }) => {
                    drive_flush(live, flusher, Some(&target))
                }
                Some(_) => panic!("unexpected Begin preparation"),
            }
        }
        // B's table issue reads the Catalog anchor and leaf.
        prepare_catalog(live, owner, flusher);
        let mut reader_private = boxed::<PRIVATE_BYTES>();
        let reader_private = &mut *reader_private;
        let mut values = Vec::new();
        {
            let (mut writer, reader, mut flush) = live.split();
            let tx = writer
                .begin_row_transaction(
                    owner,
                    IsolationLevel::Serializable,
                    &row_status,
                    private,
                    &mut flush,
                )
                .unwrap();
            let view = writer.begin_statement(owner, &tx).unwrap();
            let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
            let mut residency = Plan4ResidencyCursor::new();
            let cancel = Cancellation::default();
            let mut output = [0xA5; 64];
            for row in 1..=reads {
                let read = loop {
                    match read_point_live(
                        ReadCall {
                            port: &reader,
                            owner,
                            private: reader_private,
                            residency: &mut residency,
                        },
                        &view,
                        &table,
                        RowId::new(row).unwrap(),
                        &mut output,
                        &cancel,
                    )
                    .unwrap()
                    {
                        ReadStep::Ready(read) => break read,
                        ReadStep::FlushRequired { target } => {
                            flush_target!(flush, flusher, target)
                        }
                    }
                };
                values.push(match read {
                    PointRead::Found(copied) => {
                        assert_eq!(copied.version().row.get(), row);
                        let decoded = crate::storage::row_record::decode_canonical_row(
                            &output[..copied.len()],
                            &[crate::api::Column {
                                name: "i".into(),
                                data_type: crate::api::DataType::Int64,
                                nullable: false,
                            }],
                        )
                        .unwrap();
                        match decoded.value(0).unwrap() {
                            ValueRef::Int64(value) => Some(value),
                            other => panic!("row {row}: {other:?}"),
                        }
                    }
                    PointRead::Missing => None,
                });
            }
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(
                writer
                    .abort_empty_transaction(owner, tx, private, &mut flush)
                    .is_ok()
            );
            owner.check_idle().unwrap();
        }
        values
    }
}

/// One restart read by B and by the page reader; both equal `expected`.
fn both(
    images: &Images,
    policy: Option<RowRecoveryPolicy>,
    drive: Drive,
    expected: &[Option<i64>],
) {
    let reads = expected.len() as u64;
    let restarted = restart_with(images, policy, drive, reads, &BRead);
    assert_eq!(restarted.reads, expected, "B after READY");
    assert_eq!(
        leaf_values(&restarted.images, reads),
        expected,
        "page reader after READY"
    );
}

/// S6R-M01 by B: `rows` committed rows under both policies.
fn committed(id: &str, rows: usize) {
    case(id, || {
        let images = image(rows, Ending::Committed);
        for policy in [
            RowRecoveryPolicy::WithRowClr,
            RowRecoveryPolicy::RejectInitialLosers,
        ] {
            both(&images, Some(policy), Drive::Step, &vec![Some(VALUE); rows]);
        }
    });
}

#[test]
fn s9r_b01_one_committed_row_read_by_b() {
    committed("S9R-B01-1", 1);
}

#[test]
fn s9r_b01_three_committed_rows_read_by_b() {
    committed("S9R-B01-3", 3);
}

/// S6R-M02 by B: the reserved row is cancelled by the recovered cleanup; B
/// finds RowId 1 Missing.
#[test]
fn s9r_b02_reserved_row_is_missing_for_b() {
    case("S9R-B02", || {
        let images = image(1, Ending::Reserved);
        both(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::Step,
            &[None],
        );
    });
}

/// S6R-M04 by B: A's recovered CLR rolls the Active chain back; B finds the
/// three RowIds Missing.
#[test]
fn s9r_b03_active_chain_is_missing_for_b() {
    case("S9R-B03", || {
        let images = image(3, Ending::Active);
        both(
            &images,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::Action,
            &[None; 3],
        );
    });
}

/// The S6 native cases keep B's read after READY (C+A+B), so they live here.
#[cfg(target_os = "linux")]
#[path = "row_recovery_read/native.rs"]
mod native;
