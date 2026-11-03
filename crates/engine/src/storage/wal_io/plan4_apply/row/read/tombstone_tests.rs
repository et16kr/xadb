//! RDL-B: the tombstone's Live U as B's `value_before` reads it
//! (dev3-row-delete.md#read). The one accepted shape is the U the
//! tombstone's own transaction wrote for this mutation, whose before is the
//! committed Inline row; every other U under a tombstone is corruption.
use super::tombstone_before;
use crate::{
    api::DatabaseId,
    storage::{
        row_record::RecordVersion,
        transaction_record::{LogicalRowVersion, TxnRef, VersionStamp},
        undo_record::{
            UndoBeforeKind, UndoCreator, UndoEdge, UndoPayload, UndoRecord, UndoRelease,
            UndoTargetKind, UndoTargetView,
        },
    },
    transaction::TxnId,
};
use foundation::identity::{CommitSeq, Generation, ObjectId, RowId};

const VALUE: &[u8] = &[7, 0, 0, 0, 0, 0, 0, 0, 0x65];

fn id(local: u64) -> TxnId {
    TxnId {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
        writer_epoch: 2,
        local,
    }
}
fn g(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn q(n: u64) -> CommitSeq {
    CommitSeq::new(n).unwrap()
}
/// The committed row the delete replaced: created by txn 3 at mutation 1.
fn old() -> LogicalRowVersion {
    LogicalRowVersion::new(id(3), 1).unwrap()
}
/// The tombstone of txn 9 (Status generation 4), mutation 2.
fn tombstone() -> RecordVersion {
    RecordVersion {
        row: RowId::new(1).unwrap(),
        stamp: VersionStamp::Transaction(TxnRef::new(id(9), g(4)).unwrap()),
        logical: LogicalRowVersion::new(id(9), 2).unwrap(),
        undo: None,
    }
}
fn live() -> UndoRecord<'static> {
    UndoRecord {
        creator: UndoCreator {
            id: id(9),
            status_generation: g(4),
        },
        sequence: 1,
        mutation_sequence: 2,
        incoming: 1,
        target: UndoTargetView {
            kind: UndoTargetKind::Row,
            owner: ObjectId::new(3).unwrap(),
            generation: g(1),
            entity: 1,
            arity: 0,
            key: &[],
        },
        edges: [UndoEdge::Never, UndoEdge::Never],
        before_kind: UndoBeforeKind::RowValue,
        before_stamp: Some(VersionStamp::Committed(q(2))),
        before_logical: Some(old()),
        payload: UndoPayload::Inline(VALUE),
        release: UndoRelease {
            cursor: 0,
            chunks_released: 0,
            next: None,
        },
    }
}

#[test]
fn rdl_b_tombstone_before_takes_the_committed_inline_row() {
    // A Committed tombstone never reaches here: C's capture takes a
    // tombstone only as `Transaction(T)` with its U.
    assert_eq!(
        tombstone_before(&live(), &tombstone()),
        Some((q(2), old(), VALUE))
    );
}

#[test]
fn rdl_b_tombstone_before_refuses_every_other_u() {
    type Mutant = fn(&mut UndoRecord<'static>);
    let mutants: [(&str, Mutant); 10] = [
        ("Missing before", |r| {
            r.before_kind = UndoBeforeKind::Missing;
        }),
        ("RowDeleted before", |r| {
            r.before_kind = UndoBeforeKind::RowDeleted;
        }),
        ("Transaction before stamp", |r| {
            r.before_stamp = Some(VersionStamp::Transaction(TxnRef::new(id(3), g(1)).unwrap()));
        }),
        ("no before stamp", |r| r.before_stamp = None),
        ("no before logical", |r| r.before_logical = None),
        ("no value", |r| r.payload = UndoPayload::None),
        ("other creator", |r| r.creator.id = id(8)),
        ("other Status generation", |r| {
            r.creator.status_generation = g(5);
        }),
        ("other mutation", |r| r.mutation_sequence = 1),
        ("sequence 0", |r| r.sequence = 0),
    ];
    for (name, mutate) in mutants {
        let mut record = live();
        mutate(&mut record);
        assert_eq!(tombstone_before(&record, &tombstone()), None, "{name}");
    }
    // The tombstone side: a Transaction stamp of another transaction or
    // generation than the logical's creator.
    let mut other = tombstone();
    other.stamp = VersionStamp::Transaction(TxnRef::new(id(8), g(4)).unwrap());
    assert_eq!(tombstone_before(&live(), &other), None);
    other.stamp = VersionStamp::Transaction(TxnRef::new(id(9), g(5)).unwrap());
    assert_eq!(tombstone_before(&live(), &other), None);
}
