//! RUP-A (dev3-row-update.md#tests): A's UPDATE groups, UpdateForward (h0
//! growing, over an effect-2 H shrinking, over an effect-0 H at the same
//! size) and UpdateClr over an effect-2 H, and its INSERT Forward over an
//! effect-2 H, produced from the before states of history U and compared
//! byte for byte with the independent `transition-update/author.py`
//! literals that C's observer accepts (`transition_update_tests.rs`).
//! Literal observations confer no actual source, reservation, pin, staging,
//! installation or recovery authority.

use super::{
    InsertError,
    group::Change,
    group_delete_tests::{Case, History, namespace, produce, same},
};
use crate::storage::{
    retention_page::row,
    semantic_row::transition::tests::{
        delete::{ABefore, padded_leaf},
        update::{
            GROWN_ORDINAL, U_TERMINAL, a_actor, a_befores, a_update_over, grown, grown_before,
            grown_forward_over, u_blob,
        },
    },
};
use foundation::identity::RowId;

/// The RootJob of each A group of history U.
const ORDINALS: [(&str, usize); 5] = [
    ("U-558", 23),
    ("U-560", 24),
    ("U-562", 24),
    ("U-565", 25),
    ("U-567", 26),
];

fn history_u() -> History {
    History {
        ordinals: &ORDINALS,
        terminal: U_TERMINAL,
        status_page: 21,
        actor: a_actor(),
    }
}

/// The value each Forward's lane carries: its Reserve's sealed canonical.
fn canonical(group: &str) -> &'static [u8] {
    u_blob(match group {
        "U-558" => "U-557-canonical",
        "U-560" => "U-559-canonical",
        "U-565" => "U-564-canonical",
        "U-567" => "U-566-canonical",
        _ => panic!("{group} has no Forward value"),
    })
}

/// The change A's lane would carry for this group (C's
/// `row_producer_context`, dev3-row-update.md#api): an UPDATE Forward's
/// sealed new value with the RootJob's old cell C and planned projection,
/// an UPDATE CLR's current new row C'.
fn change(case: &Case, state: &ABefore) -> Change<'static> {
    let job = row::decode_job(&state.bank[case.ordinal], namespace()).unwrap();
    let update = job.effect() == row::RowEffect::Update;
    match case.clr {
        Some((target, next)) => Change::Clr {
            cell_bytes: if update {
                job.new_cell() as usize
            } else {
                job.cell_bytes() as usize
            },
            projection: *job.projection_digest(),
            target,
            next,
        },
        None if update => Change::UpdateForward {
            canonical: canonical(state.name),
            old_cell: job.cell_bytes() as usize,
            projection: *job.projection_digest(),
        },
        None => Change::Forward {
            canonical: canonical(state.name),
        },
    }
}

/// RUP-A: A produces each of its history U groups byte for byte.
#[test]
fn rup_a_update_groups_match_literals() {
    for state in &a_befores() {
        let case = Case::over(state, &history_u());
        let (result, frame, after) = produce(&case, state, change(&case, state));
        let produced = result.unwrap_or_else(|error| panic!("{}: {error:?}", state.name));
        assert_eq!(produced.after_count, 5, "{}", state.name);
        assert_eq!(produced.roles, case.after_order, "{}", state.name);
        assert_eq!(produced.frame_len, state.frame.len(), "{}", state.name);
        same(state.name, &frame[..produced.frame_len], state.frame);
        assert!(
            frame[produced.frame_len..].iter().all(|byte| *byte == 0xa5),
            "{}: frame tail",
            state.name
        );
        assert!(
            after
                .iter()
                .all(|page| page.iter().any(|byte| *byte != 0xa5))
        );
        println!(
            "RUP_A_GROUP\t{}\tbytes={}\troles={:?}",
            state.name, produced.frame_len, produced.roles
        );
    }
}

/// RUP-A: A refuses an UPDATE group before its first output byte when the
/// lane's input disagrees with the RootJob or the pages. The effect binds
/// the change (an INSERT value on an update root, a new value on an INSERT
/// root: Unsupported). The sealed new value binds the RootJob (another
/// length than C', another value of the same length, another planned
/// projection or old cell C: IdentityMismatch). The RootJob binds the RowId
/// (IdentityMismatch). An UPDATE CLR's current row is the new row C', not
/// the old C (IdentityMismatch). A new value that needs a split is Capacity.
#[test]
fn rup_a_update_producer_refuses_before_output() {
    let states = a_befores();
    let by_name = |name: &str| states.iter().find(|state| state.name == name).unwrap();
    let refused = |label: &str, case: &Case, state: &ABefore, change: Change<'_>| {
        let (result, frame, after) = produce(case, state, change);
        let error = result.map(|_| ()).expect_err(label);
        assert!(frame.iter().all(|byte| *byte == 0xa5), "{label}: frame");
        assert!(
            after.iter().flatten().all(|byte| *byte == 0xa5),
            "{label}: after"
        );
        println!("RUP_A_REFUSED\t{label}\t{error:?}");
        error
    };
    let job_of = |case: &Case, state: &ABefore| {
        let job = row::decode_job(&state.bank[case.ordinal], namespace()).unwrap();
        (job.cell_bytes() as usize, *job.projection_digest())
    };

    // Row 9 grows (C 194 → C' 224, h0).
    let state = by_name("U-558");
    let case = Case::over(state, &history_u());
    let (old_cell, planned) = job_of(&case, state);
    let value = canonical("U-558");
    let cases: [(&str, Change<'_>, InsertError); 5] = [
        (
            "INSERT value on an update root",
            Change::Forward { canonical: value },
            InsertError::Unsupported,
        ),
        (
            "a new value of another length than C'",
            Change::UpdateForward {
                canonical: u_blob("U-559-canonical"),
                old_cell,
                projection: planned,
            },
            InsertError::IdentityMismatch,
        ),
        (
            "another planned projection",
            Change::UpdateForward {
                canonical: value,
                old_cell,
                projection: {
                    let mut other = planned;
                    other[0] ^= 1;
                    other
                },
            },
            InsertError::IdentityMismatch,
        ),
        (
            "another old cell",
            Change::UpdateForward {
                canonical: value,
                old_cell: old_cell + 1,
                projection: planned,
            },
            InsertError::IdentityMismatch,
        ),
        (
            "an old cell equal to C'",
            Change::UpdateForward {
                canonical: value,
                old_cell: 180 + value.len(),
                projection: planned,
            },
            InsertError::IdentityMismatch,
        ),
    ];
    for (label, change, error) in cases {
        assert_eq!(refused(label, &case, state, change), error, "{label}");
    }

    // Another RowId than the RootJob's: row 11, committed and pruned in the
    // same leaf.
    let mut other = Case::over(state, &history_u());
    other.row = RowId::new(11).unwrap();
    assert_eq!(
        refused(
            "another RowId",
            &other,
            state,
            change(&Case::over(state, &history_u()), state),
        ),
        InsertError::IdentityMismatch
    );

    // A growing value that needs a split, before any output: the leaf
    // padded to 29 free bytes for the 30 the row grows by.
    let mut full = Case::over(state, &history_u());
    let leaf_at = full.sources[5].unwrap().index;
    full.before[leaf_at] = *padded_leaf(&state.leaf, 224 - 194 - 1);
    assert_eq!(
        refused(
            "a growing value without room",
            &full,
            state,
            change(&full, state)
        ),
        InsertError::Capacity
    );

    // Row 12 shrinks (C 204 → C' 194) over the effect-2 H: another value of
    // the same length (row 11's U-566 value) is another planned row.
    let state = by_name("U-560");
    let case = Case::over(state, &history_u());
    let (old_cell, planned) = job_of(&case, state);
    let other_value = u_blob("U-566-canonical");
    assert_eq!(other_value.len(), canonical("U-560").len());
    assert_ne!(other_value, canonical("U-560"));
    assert_eq!(
        refused(
            "another value of the same length",
            &case,
            state,
            Change::UpdateForward {
                canonical: other_value,
                old_cell,
                projection: planned,
            },
        ),
        InsertError::IdentityMismatch
    );

    // Its UpdateClr names the current new row C', not the old C.
    let state = by_name("U-562");
    let case = Case::over(state, &history_u());
    let (old_cell, planned) = job_of(&case, state);
    let Some((target, next)) = case.clr else {
        panic!("U-562 is a CLR");
    };
    assert_eq!(
        refused(
            "UpdateClr naming the old cell C",
            &case,
            state,
            Change::Clr {
                cell_bytes: old_cell,
                projection: planned,
                target,
                next,
            },
        ),
        InsertError::IdentityMismatch
    );

    // The INSERT root of statement 4 takes no new value of an UPDATE.
    let state = by_name("U-565");
    let case = Case::over(state, &history_u());
    let (cell, planned) = job_of(&case, state);
    assert_eq!(
        refused(
            "a new value on an INSERT root",
            &case,
            state,
            Change::UpdateForward {
                canonical: canonical("U-565"),
                old_cell: cell,
                projection: planned,
            },
        ),
        InsertError::Unsupported
    );
}

/// RUP-A (dev3-row-update.md#leaf-reservation): A's growing UpdateForward
/// (U-558, C 194 → C' 224) is written as soon as the 30 bytes it grows by
/// are free (one byte fewer is Capacity above); A checks only that the
/// replacement needs no split, and C's observer checks the reservation D
/// beside it. The same-size UpdateForward (U-567) needs no free byte.
#[test]
fn rup_a_update_forward_needs_only_its_growth() {
    let states = a_befores();
    for (group, growth) in [("U-558", 224 - 194), ("U-567", 0)] {
        let state = states.iter().find(|state| state.name == group).unwrap();
        let mut case = Case::over(state, &history_u());
        let leaf_at = case.sources[5].unwrap().index;
        case.before[leaf_at] = *padded_leaf(&state.leaf, growth);
        let (result, ..) = produce(&case, state, change(&case, state));
        result.unwrap_or_else(|error| panic!("{group} free {growth}: {error:?}"));
        println!("RUP_A_LEAF\t{group}\tfree={growth}\tfits");
    }
}

/// RUP-A and RUP-C (review r1 #1, dev3-row-update.md#leaf-reservation): A's
/// growing UpdateForward of row 11 (C 194 → C' 194 + 14 B) beside the
/// reservation D = 10 of the Retained shrinking root 24 (`grown_before`, a
/// state history U does not reach). A writes it as soon as its growth is
/// free; C's observer re-checks `entries + (C' − C) + D` and accepts it at
/// exactly capacity, and refuses it one byte fuller (OrderMismatch) before
/// any output, where only D decides: the cell itself still fits.
#[test]
fn rup_a_growing_update_forward_beside_d() {
    let state = grown_before();
    let history = History {
        ordinals: &[("U-560-grow", GROWN_ORDINAL)],
        ..history_u()
    };
    let value = grown();
    let job = row::decode_job(&state.bank[GROWN_ORDINAL], namespace()).unwrap();
    assert_eq!(job.effect(), row::RowEffect::Update);
    let growth = (job.new_cell() - job.cell_bytes()) as usize;
    assert_eq!(growth, value.len() - 14);
    let d = 204 - 194;
    for (free, expected) in [
        (growth + d, Ok(())),
        (
            growth + d - 1,
            Err(crate::storage::semantic_row::Error::OrderMismatch),
        ),
    ] {
        let mut case = Case::over(&state, &history);
        let leaf_at = case.sources[5].unwrap().index;
        let padded = padded_leaf(&state.leaf, free);
        case.before[leaf_at] = *padded;
        let change = Change::UpdateForward {
            canonical: &value,
            old_cell: job.cell_bytes() as usize,
            projection: *job.projection_digest(),
        };
        let (result, frame, _) = produce(&case, &state, change);
        let produced = result.unwrap_or_else(|error| panic!("free {free}: {error:?}"));
        assert_eq!(
            grown_forward_over(&padded, &frame[..produced.frame_len]),
            expected,
            "free {free}"
        );
        println!("RUP_A_GROWN_BESIDE_D\tfree={free}\t{expected:?}");
    }
}

/// RUP-A (dev3-row-update.md#leaf-reservation, review r1 #2): A's frames
/// over a leaf at the reservation boundary, through C's observer. The
/// growing UpdateForward of row 9 (U-558, D 0) is accepted with exactly its
/// 30 B of growth free (one byte fewer is A's own Capacity, above). The
/// UpdateClr of the shrinking root 24 (U-562, D 10, its own share 10) is
/// accepted at exactly entries + D = capacity, since the 10 B it restores
/// are its own reservation (the room it needs is D less its share, 0). One
/// byte fewer is A's own Capacity (its restored cell does not fit); C's
/// refusal of a leaf already inside D is RUP-C's, over the literal.
#[test]
fn rup_a_update_groups_at_the_leaf_reservation_boundary() {
    let states = a_befores();
    for (group, free, expected) in [("U-558", 224 - 194, Ok(())), ("U-562", 204 - 194, Ok(()))] {
        let state = states.iter().find(|state| state.name == group).unwrap();
        let mut case = Case::over(state, &history_u());
        let leaf_at = case.sources[5].unwrap().index;
        let padded = padded_leaf(&state.leaf, free);
        case.before[leaf_at] = *padded;
        let (result, frame, _) = produce(&case, state, change(&case, state));
        let produced = result.unwrap_or_else(|error| panic!("{group} free {free}: {error:?}"));
        assert_eq!(
            a_update_over(group, &padded, &frame[..produced.frame_len]),
            expected,
            "{group} free {free}"
        );
        println!("RUP_A_LEAF_BOUNDARY\t{group}\tfree={free}\t{expected:?}");
    }
}
