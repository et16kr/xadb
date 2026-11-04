//! RUP-C (dev3-row-update.md#classification, #tests): the preflight's
//! classification of an UPDATE or DELETE target (`row_class`) over the
//! RowLeaf and Jobs bank of history U (`transition_update_tests.rs`,
//! independent `transition-update/author.py` literals). Another
//! transaction's Inline row is an INSERT or an UPDATE row by the effect of
//! the one root Job that holds it, and a bank whose holder fails the pair
//! binding is the sources' contradiction, never zero rows, Busy or
//! SerializationConflict. Literal observations confer no actual source,
//! reservation, pin, staging, installation or recovery authority.

use super::{
    RetentionCause, TargetClass,
    delete_tests::{columns, committed_q, expected, namespace, stranger, version},
    row_class,
};
use crate::{
    storage::{
        page::{FileId, PAGE_BYTES, PageRef},
        retention_page::{
            JOB_BYTES, JOB_COUNT,
            row::{self as job3, JobState, RowEffect},
        },
        row_record::RowSchemaInput,
        semantic_row::transition::tests::update::{U_TERMINAL, a_actor, u_leaves},
        transaction_record::{TxnRef, VersionStamp},
        undo_ref::UndoRef,
    },
    transaction::TxnId,
};
use foundation::identity::{Generation, RowId};

type Page = [u8; PAGE_BYTES];
type Bank = [[u8; JOB_BYTES]; JOB_COUNT];

/// T3, history U's actor.
fn t3() -> TxnRef {
    a_actor().0
}

fn class(
    (leaf, bank): (&Page, &Bank),
    actor: TxnRef,
    row: u64,
    (snapshot, serializable): (u64, bool),
) -> Result<TargetClass, RetentionCause> {
    let columns = columns();
    row_class(
        leaf,
        expected(),
        RowSchemaInput::Columns(&columns),
        (bank, namespace()),
        (actor, RowId::new(row).unwrap()),
        (snapshot, serializable),
    )
}

fn corrupt(result: Result<TargetClass, RetentionCause>) -> bool {
    matches!(
        result,
        Err(RetentionCause::RowSemantic(
            crate::storage::semantic_row::Error::Corrupt
        ))
    )
}

fn job(bank: &Bank, ordinal: usize) -> job3::JobView<'_> {
    job3::decode_job(&bank[ordinal], namespace()).unwrap()
}

/// The RowId root `ordinal` of `bank` holds.
fn held(bank: &Bank, ordinal: usize) -> u64 {
    job(bank, ordinal).row_id().unwrap().get()
}

const ALL: (u64, bool) = (u64::MAX, true);
fn rc(q: u64) -> (u64, bool) {
    (q, false)
}
fn sr(q: u64) -> (u64, bool) {
    (q, true)
}

/// RUP-C: the contract's table over history U, for an UPDATE row, an
/// INSERT row and a compensated root of T3, before and after T3's commit,
/// its cleanup and its RootFrees.
#[test]
fn rup_c_update_class_follows_the_contract_table() {
    let states = u_leaves(&[
        "U-558", "U-562", "U-565", "U-567", "U-568", "U-571", "U-579",
    ]);
    let state = |name: &str| {
        let (_, leaf, bank) = states.iter().find(|(at, ..)| at == name).unwrap();
        (&**leaf, &**bank)
    };

    // U-558: T3 (Leased) has updated row 9 (root 23, effect 2). T's INSERT
    // row 11 is committed, pruned and free of roots since U-549.
    let u558 = state("U-558");
    assert_eq!(held(u558.1, 23), 9);
    assert_eq!(job(u558.1, 23).effect(), RowEffect::Update);
    assert_eq!(job(u558.1, 23).state(), JobState::RootRetained);
    assert!(matches!(
        version(u558.0, 9).0.stamp,
        VersionStamp::Transaction(actor) if actor == t3()
    ));
    for (actor, expected, label) in [
        (t3(), TargetClass::Unsupported, "own UPDATE row"),
        (stranger(), TargetClass::Busy, "T3 Leased"),
    ] {
        for snapshot in [ALL, rc(u64::MAX)] {
            assert_eq!(
                class(u558, actor, 9, snapshot).unwrap(),
                expected,
                "{label}"
            );
        }
    }
    let q11 = committed_q(u558.0, 11);
    assert_eq!(
        class(u558, t3(), 11, sr(q11)).unwrap(),
        TargetClass::Proceed
    );
    assert_eq!(
        class(u558, t3(), 11, sr(q11 - 1)).unwrap(),
        TargetClass::NoRow
    );
    assert_eq!(class(u558, t3(), 1000, ALL).unwrap(), TargetClass::NoRow);

    // U-562: row 12's UpdateClr restored its committed row, which T3's
    // compensated root 24 holds until T3 ends.
    let u562 = state("U-562");
    assert_eq!(held(u562.1, 24), 12);
    assert!(matches!(
        version(u562.0, 12).0.stamp,
        VersionStamp::Committed(_)
    ));
    for actor in [t3(), stranger()] {
        assert_eq!(class(u562, actor, 12, ALL).unwrap(), TargetClass::Busy);
    }

    // U-565: T3's INSERT row (root 25, effect 0) beside its UPDATE row.
    let u565 = state("U-565");
    let inserted = held(u565.1, 25);
    assert_eq!(job(u565.1, 25).effect(), RowEffect::Insert);
    assert_eq!(
        class(u565, t3(), inserted, ALL).unwrap(),
        TargetClass::Unsupported,
        "own INSERT row"
    );
    assert_eq!(
        class(u565, stranger(), inserted, ALL).unwrap(),
        TargetClass::NoRow,
        "T3 Leased INSERT row"
    );
    assert_eq!(class(u565, stranger(), 9, ALL).unwrap(), TargetClass::Busy);

    // U-567: row 11 at the same size (root 26, effect 2).
    let u567 = state("U-567");
    assert_eq!(held(u567.1, 26), 11);
    assert_eq!(
        class(u567, t3(), 11, ALL).unwrap(),
        TargetClass::Unsupported
    );
    assert_eq!(class(u567, stranger(), 11, ALL).unwrap(), TargetClass::Busy);

    // U-568: T3 committed at q3, nothing pruned yet.
    let u568 = state("U-568");
    let terminal = job(u568.1, U_TERMINAL);
    assert_eq!(terminal.state(), JobState::TerminalReady);
    assert_eq!(terminal.source(), Some(t3()));
    let q3 = terminal.commit_q();
    for row in [9, 11] {
        assert_eq!(
            class(u568, stranger(), row, sr(q3)).unwrap(),
            TargetClass::Busy,
            "row {row}: q' <= S, before its UpdatePrune"
        );
        assert_eq!(
            class(u568, stranger(), row, sr(q3 - 1)).unwrap(),
            TargetClass::SerializationConflict,
            "row {row}: q' > S, Serializable"
        );
        assert_eq!(
            class(u568, stranger(), row, rc(q3 - 1)).unwrap(),
            TargetClass::Busy,
            "row {row}: q' > S, ReadCommitted"
        );
    }
    assert_eq!(
        class(u568, stranger(), inserted, sr(q3)).unwrap(),
        TargetClass::Busy,
        "INSERT row, unpruned"
    );
    for snapshot in [sr(q3 - 1), rc(q3 - 1)] {
        assert_eq!(
            class(u568, stranger(), inserted, snapshot).unwrap(),
            TargetClass::NoRow,
            "INSERT row q' > S"
        );
    }
    assert_eq!(class(u568, stranger(), 12, ALL).unwrap(), TargetClass::Busy);

    // U-571: UpdatePrune restamped rows 9 and 11 Committed(q3) (Prune the
    // INSERT row); their Releasing roots still hold them.
    let u571 = state("U-571");
    for row in [9, 11, inserted] {
        assert_eq!(committed_q(u571.0, row), q3, "row {row}");
        assert_eq!(
            class(u571, stranger(), row, sr(q3)).unwrap(),
            TargetClass::Busy,
            "row {row}"
        );
        assert_eq!(
            class(u571, stranger(), row, sr(q3 - 1)).unwrap(),
            TargetClass::NoRow,
            "row {row}"
        );
    }

    // U-579: roots 23 and 24 are free; 25 and 26 still hold theirs.
    let u579 = state("U-579");
    assert_eq!(
        class(u579, stranger(), 9, sr(q3)).unwrap(),
        TargetClass::Proceed
    );
    assert_eq!(
        class(u579, stranger(), 9, sr(q3 - 1)).unwrap(),
        TargetClass::NoRow
    );
    assert_eq!(
        class(u579, stranger(), 12, ALL).unwrap(),
        TargetClass::Proceed
    );
    for row in [11, inserted] {
        assert_eq!(
            class(u579, stranger(), row, ALL).unwrap(),
            TargetClass::Busy
        );
    }
}

/// A Free Job cell of `bank`.
fn free(bank: &Bank) -> usize {
    bank.iter()
        .position(|cell| {
            job3::decode_job(cell, namespace()).is_ok_and(|job| job.state() == JobState::Free)
        })
        .expect("a Free Job")
}

/// RUP-C: a holder root that fails a field of the pair binding (effect,
/// state, source, table, undo) or a RowId held by no root or by two is
/// Corrupt for every snapshot, Leased or committed. Every mutated bank
/// decodes Job by Job, so the binding is what refuses it.
#[test]
fn rup_c_update_class_never_lowers_a_holder_contradiction() {
    let states = u_leaves(&["U-557", "U-558", "U-568"]);
    let (_, _, u557_bank) = &states[0];
    let (_, u558_leaf, u558_bank) = &states[1];
    let (_, u568_leaf, u568_bank) = &states[2];
    let q3 = job(u568_bank, U_TERMINAL).commit_q();
    let mut checked = 0;
    for (label, leaf, bank, snapshots) in [
        ("Leased", u558_leaf, u558_bank, vec![ALL, rc(u64::MAX)]),
        (
            "committed",
            u568_leaf,
            u568_bank,
            vec![sr(q3), sr(q3 - 1), rc(q3 - 1)],
        ),
    ] {
        let root = job(bank, 23);
        assert_eq!(
            (root.row_id().unwrap().get(), root.effect()),
            (9, RowEffect::Update)
        );
        let mut mutants: Vec<(&str, Box<Bank>)> = Vec::new();

        // A DELETE root: effect 1, C' cleared and its cleanup debt recomputed.
        let mut effect = bank.clone();
        effect[23][284] = 1;
        effect[23][286..288].fill(0);
        let wal = job3::row_cleanup_wal(
            RowEffect::Delete,
            root.cell_bytes(),
            0,
            root.has_previous_transaction().unwrap(),
        );
        for at in [784, 880] {
            effect[23][at..at + 8].copy_from_slice(&wal.to_le_bytes());
        }
        mutants.push(("effect 1", effect));

        // Another source, same Status generation.
        let mut source = bank.clone();
        let other = TxnRef::new(
            TxnId {
                local: 8,
                ..t3().id()
            },
            t3().generation(),
        )
        .unwrap();
        other.encode(&mut source[23][16..64]).unwrap();
        mutants.push(("another source", source));

        // Another table (object 7).
        let mut object = bank.clone();
        object[23][176..184].copy_from_slice(&7_u64.to_le_bytes());
        mutants.push(("another table", object));

        // Another table generation is refused by the decode
        // (`validate_root`); another undo is not.
        let mut undo = bank.clone();
        UndoRef::new(
            PageRef {
                file: FileId::new(0x1_0000_0001).unwrap(),
                page_no: 30,
                incarnation: Generation::new(2).unwrap(),
            },
            0,
            Generation::new(2).unwrap(),
        )
        .unwrap()
        .encode(&mut undo[23][128..168])
        .unwrap();
        mutants.push(("another undo", undo));

        // No holder, and two.
        let at = free(bank);
        let mut none = bank.clone();
        none[23] = bank[at];
        mutants.push(("no holder", none));
        let mut two = bank.clone();
        two[at] = bank[23];
        mutants.push(("two holders", two));

        for (mutant, mutated) in &mutants {
            for cell in mutated.iter() {
                job3::decode_job(cell, namespace())
                    .unwrap_or_else(|error| panic!("{label} {mutant}: {error:?}"));
            }
            for actor in [stranger(), t3()] {
                for &snapshot in &snapshots {
                    let result = class((leaf, mutated), actor, 9, snapshot);
                    if actor == t3() {
                        // T3's own row needs no holder: Unsupported.
                        assert_eq!(result.unwrap(), TargetClass::Unsupported);
                        continue;
                    }
                    assert!(corrupt(result), "{label} {mutant} {snapshot:?}");
                    checked += 1;
                }
            }
        }
    }

    // The holder of the Leased UPDATE row still Reserved (U-557's bank
    // under U-558's leaf).
    assert_eq!(job(u557_bank, 23).state(), JobState::RootReserved);
    assert!(corrupt(class((u558_leaf, u557_bank), stranger(), 9, ALL)));
    checked += 1;
    println!("RUP_C_HOLDER_MUTANTS checked={checked}");
    assert_eq!(checked, 6 * 2 + 6 * 3 + 1);
}
