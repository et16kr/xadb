//! S2-b2 review A6: `check_candidate`'s RowLeaf shortage branch. No live
//! fixture reaches it without first filling the leaf (about eighty rows of
//! the producer's header), so the rule is checked at its boundary here: a
//! candidate that needs exactly the reported free bytes is accepted, one
//! byte more is Unchanged Capacity in Prepare.
use super::*;
use crate::storage::page::{FileId, PageIdentity, PageRef};
use foundation::identity::{Generation, ObjectHandle, ObjectId, SchemaEpoch};

fn table() -> RowTableSource {
    let one = Generation::new(1).expect("generation");
    RowTableSource {
        handle: ObjectHandle {
            object: ObjectId::new(9).expect("object"),
            generation: one,
            schema_epoch: SchemaEpoch::new(1).expect("epoch"),
        },
        slot: 0,
        anchor: PageIdentity {
            database: crate::api::DatabaseId::new([7; 16]).expect("database"),
            page: PageRef {
                file: FileId::new((2 << 32) | 1).expect("file"),
                page_no: 11,
                incarnation: one,
            },
            owner: ObjectId::new(9).expect("owner"),
            owner_generation: one,
        },
    }
}

fn cut(leaf_free: u16) -> RowPreflightCut {
    RowPreflightCut {
        source: [0; 32],
        complete_lsn: 1,
        roles: 0,
        leaf_free,
        root_job: 0,
    }
}

#[test]
fn s2b2_a6_check_candidate_leaf_shortage_is_capacity() {
    let length = 64;
    let mut candidate = [0u8; PAGE_BYTES];
    candidate[..length].fill(0x5a);
    let plan = RowInputPlan::new(
        length,
        SchemaEpoch::new(1).expect("epoch"),
        content_digest(&candidate[..length]),
    );
    // A's rule: four cell-header bytes, eight key bytes, the row header and
    // a slot over the canonical row.
    let need = u16::try_from(length + ROW_HEADER_BYTES + 16).expect("need");
    check_candidate(&plan, &candidate, table(), &cut(need)).expect("fits exactly");
    let error =
        check_candidate(&plan, &candidate, table(), &cut(need - 1)).expect_err("one byte short");
    assert_eq!(error.effect, Effect::Unchanged);
    assert_eq!(error.phase, RetentionPhase::Prepare);
    assert!(
        matches!(error.cause, RetentionCause::Capacity),
        "{:?}",
        error.cause
    );
    // The plan check runs first: a shortage never masks a forged candidate.
    candidate[0] ^= 1;
    let error =
        check_candidate(&plan, &candidate, table(), &cut(need - 1)).expect_err("forged candidate");
    assert!(
        matches!(
            error.cause,
            RetentionCause::RowSemantic(row_semantic::Error::InvalidInput)
        ),
        "{:?}",
        error.cause
    );
}
