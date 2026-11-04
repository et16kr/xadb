//! RUP-C (dev3-row-update.md#transitions): the C-owned UPDATE groups and the
//! whole-group observer against the independent
//! `module-row-format/transition-update/author.py` literals. History U starts
//! from the end of history D and runs a third actor T3 through every UPDATE
//! transition C produces or observes: the update RowReserve and its Cancel,
//! A's UpdateForward (growing h0, shrinking over an effect-2 H, same size over
//! an effect-0 H) and UpdateClr (effect-2 H), A's INSERT Forward over an
//! effect-2 H, both UpdatePrunes and the Prune over an effect-2 H, and the
//! ReleaseRole/RootFree of a committed and of a compensated update root.
//! Every C-produced group is byte-equal to the literal and every literal is
//! accepted by the observer; the `R-U-*` variants are refused. Literal
//! observations confer no actual source, reservation, pin, staging,
//! installation or recovery authority.

use super::super::UpdatePlan;
use super::delete::padded_leaf;
use super::*;
use crate::storage::retention_page::RetentionError;

/// The Job census refusal of one RowId held by two non-Free roots.
const HELD: Error = Error::Queue(RetentionError::IdentityMismatch);

/// Every `R-U-*` variant that frames and the refusal its base's observer
/// must give.
pub(super) const REJECTS: [(&str, Error); 12] = [
    ("R-U-555-effect", Error::Corrupt),
    ("R-U-555-cell", Error::Corrupt),
    ("R-U-558-value", Error::IdentityMismatch),
    ("R-U-558-length", Error::IdentityMismatch),
    ("R-U-558-undo", Error::Corrupt),
    ("R-U-560-edge", Error::Corrupt),
    ("R-U-562-restore", Error::Corrupt),
    ("R-U-562-count", Error::Corrupt),
    ("R-U-565-edge", Error::Corrupt),
    ("R-U-569-q", Error::Corrupt),
    ("R-U-569-undo", Error::Corrupt),
    ("R-U-572-stamp", Error::Corrupt),
];
/// The `R-U-*` variant wire decode refuses before any semantic check: a
/// Releasing U that keeps its payload breaks UpdatePrune's U native size
/// (432+L → 432, dev3-row-update.md#update-shape).
pub(super) const FRAMING: [(&str, Error); 1] = [("R-U-571-payload", Error::Corrupt)];

/// History U's groups, C's and A's.
const U_GROUPS: usize = 34;
/// Of them A's, observed only: UpdateForward ×3, UpdateClr, INSERT Forward.
const U_OBSERVED: usize = 5;

/// T3's terminal Job, created by U-554 (Python history U).
const TERMINAL: usize = 22;

fn update_plan(statement: u64, container: u64, row: u64, canonical: &[u8]) -> UpdatePlan<'_> {
    UpdatePlan {
        terminal: TERMINAL,
        container: page_ref(container, 2),
        table: table(),
        statement,
        row: RowId::new(row).unwrap(),
        canonical,
    }
}

/// Reserve-time pages: Jobs page 1, T3's Status page 21, the Free container.
fn reserve_pages(container: u64) -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(21),
        undo: Some(container),
        table: true,
        ..pages()
    }
}

/// Row-operation pages: the root's U and, with h, its predecessor's.
fn row_pages(undo: u64, previous: Option<u64>) -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(21),
        undo: Some(undo),
        previous,
        table: true,
    }
}

/// Release-step pages: Jobs page 1 and the root's U page.
fn release_pages(undo: u64) -> Pages {
    Pages {
        jobs: Some(1),
        undo: Some(undo),
        ..pages()
    }
}

/// D (dev3-row-update.md#leaf-reservation): Σ over the table's Retained Row
/// roots of DELETE's `C−180` and UPDATE's `max(0, C−C')`, from the contract's
/// bytes (`@280`, `@284`, `[286,288)`), not from `restore_bytes`.
fn widened_d(world: &World) -> usize {
    world
        .bank
        .iter()
        .filter(|cell| {
            row::decode_job(&cell[..], namespace())
                .is_ok_and(|job| job.state() == row::JobState::RootRetained)
        })
        .map(|cell| {
            let c = u32::from_le_bytes(cell[280..284].try_into().unwrap()) as usize;
            let n = usize::from(u16::from_le_bytes([cell[286], cell[287]]));
            match cell[284] {
                1 => c - 180,
                2 => c.saturating_sub(n),
                _ => 0,
            }
        })
        .sum()
}

/// RUP-C (#api): an update RowReserve replayed as `ReserveAdopted` adopts the
/// effect byte, RowId, C' and projection of the candidate's Job after and
/// recomputes every other byte, C from the leaf's old row; its
/// effect-flipped variant is refused.
fn adopted_update(world: &World, name: &str, plan: &UpdatePlan<'_>, pages: Pages) {
    let bytes = blob(name);
    let live = world.inputs(Change::ReserveUpdate(plan), pages);
    let mut owned = loans(0);
    let observed =
        observe(&live, bytes, &mut owned).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    assert_eq!(observed.issued.map(|issued| issued.row), Some(plan.row));
    let replay = world.inputs(
        Change::ReserveAdopted {
            terminal: plan.terminal,
            container: plan.container,
            table: plan.table,
            statement: plan.statement,
        },
        pages,
    );
    let mut lane = new_lane();
    let mut replayed = loans(0x5a);
    let adopted = observe_borrowed(&replay, bytes, &mut replayed, lent(&mut lane));
    restored(name, &lane);
    assert_eq!(adopted, Ok(observed), "{name}: adopted");
    for &role in &observed.order[..usize::from(observed.count)] {
        let role = usize::from(role);
        same(
            &format!("{name} adopted role{role}"),
            &replayed[role],
            &owned[role],
        );
    }
    if name == "U-555" {
        for variant in ["R-U-555-effect", "R-U-555-cell"] {
            let mut lane = new_lane();
            let mut replayed = loans(0x5a);
            assert_eq!(
                observe_borrowed(&replay, blob(variant), &mut replayed, lent(&mut lane))
                    .map(|_| ()),
                Err(Error::Corrupt),
                "{variant} adopted"
            );
            restored(name, &lane);
        }
    }
    println!("RUP_C_ADOPTED\t{name}\trow={}", plan.row.get());
}

/// History U (see the module comment) with the refusals that need its
/// intermediate states; `rejects` also checks the `R-U-*` variants.
pub(super) fn history_u(rejects: bool) -> usize {
    history_u_with(rejects, |_, _| {})
}

/// History U calling `probe` with the World after each named group.
fn history_u_with(rejects: bool, mut probe: impl FnMut(&str, &World)) -> usize {
    let mut world = delete::history_d_world(false, |_, _| {});
    world.rejects = rejects.then(|| Cell::new(0));
    // T's root 17 (row 11) and T2's root 20 (row 12) release their U.
    for (ordinal, page_no, names) in [
        (17, 14, ["U-546", "U-547", "U-548", "U-549"]),
        (20, 15, ["U-550", "U-551", "U-552", "U-553"]),
    ] {
        for name in &names[..3] {
            world.produce(
                name,
                Change::ReleaseRole { ordinal },
                release_pages(page_no),
            );
        }
        world.produce(
            names[3],
            Change::RootFree { ordinal },
            release_pages(page_no),
        );
    }
    World::replace(&mut world.status, 21, &page_of(blob("U-status21-0")));
    let begin = world.produce(
        "U-554",
        Change::Begin {
            actor: actor(7, 2),
            status: slot(21, 2),
            scope: JobScope::Row,
        },
        Pages {
            jobs: Some(1),
            status: Some(21),
            ..pages()
        },
    );
    assert_eq!(begin.job_ordinal, Some(TERMINAL));
    probe("U-554", &world);

    // C's update Reserve takes only a committed, pruned, inline row: an
    // absent RowId is refused before output.
    let absent = blob("U-555-canonical");
    refuses(
        "update Reserve of an absent RowId",
        &world.inputs(
            Change::ReserveUpdate(&update_plan(1, 10, 1000, absent)),
            reserve_pages(10),
        ),
        FRAME_BYTES,
        Error::IdentityMismatch,
    );

    // Statement 1: an update Reserve and its Cancel. No RowId is issued.
    let row_high = u64_at(cell(&world.root, 0), 432);
    let plan = update_plan(1, 10, 9, blob("U-555-canonical"));
    adopted_update(&world, "U-555", &plan, reserve_pages(10));
    let reserved = world.produce("U-555", Change::ReserveUpdate(&plan), reserve_pages(10));
    assert_eq!(reserved.job_ordinal, Some(23));
    assert_eq!(reserved.issued.map(|issued| issued.row), Some(plan.row));
    assert_eq!(u64_at(cell(&world.root, 0), 432), row_high, "row_high");
    assert_eq!(world.bank[23][284], 2, "effect byte");
    assert_eq!(
        u16::from_le_bytes([world.bank[23][286], world.bank[23][287]]),
        202,
        "C'"
    );
    world.produce(
        "U-556",
        Change::Cancel { ordinal: 23 },
        Pages {
            jobs: Some(1),
            status: Some(21),
            undo: Some(10),
            ..pages()
        },
    );
    probe("U-556", &world);

    // Statement 2: row 9 grows (C 194 → C' 224, h0).
    let plan = update_plan(2, 10, 9, blob("U-557-canonical"));
    adopted_update(&world, "U-557", &plan, reserve_pages(10));
    world.produce("U-557", Change::ReserveUpdate(&plan), reserve_pages(10));
    probe("U-557", &world);
    world.observe(
        "U-558",
        blob("U-558"),
        Change::UpdateForward {
            ordinal: 23,
            statement: boundary(2, None),
        },
        row_pages(10, None),
    );
    probe("U-558", &world);
    // T3's own UPDATE row is not committed and pruned (the old row is
    // unpruned): not the planned target, refused before output (the
    // preflight's Unsupported for it, #classification, is stage 3's).
    refuses(
        "update Reserve of T3's own unpruned UPDATE row 9",
        &world.inputs(
            Change::ReserveUpdate(&update_plan(3, 11, 9, blob("U-555-canonical"))),
            reserve_pages(11),
        ),
        FRAME_BYTES,
        Error::IdentityMismatch,
    );

    // Statement 3: row 12 shrinks (C 204 → C' 194) over the effect-2 H, then
    // is rolled back.
    let plan = update_plan(3, 11, 12, blob("U-559-canonical"));
    adopted_update(&world, "U-559", &plan, reserve_pages(11));
    let reserved = world.produce("U-559", Change::ReserveUpdate(&plan), reserve_pages(11));
    assert_eq!(reserved.job_ordinal, Some(24));
    probe("U-559", &world);
    world.observe(
        "U-560",
        blob("U-560"),
        Change::UpdateForward {
            ordinal: 24,
            statement: boundary(3, None),
        },
        row_pages(11, Some(10)),
    );
    probe("U-560", &world);
    let mark = boundary(3, Some(undo_ref(10, 5)));
    world.produce(
        "U-561",
        Change::RollbackStart {
            terminal: TERMINAL,
            statement: Some(mark),
        },
        Pages {
            status: Some(21),
            undo: Some(11),
            ..pages()
        },
    );
    probe("U-561", &world);
    world.observe(
        "U-562",
        blob("U-562"),
        Change::UpdateClr {
            ordinal: 24,
            statement: Some(mark),
        },
        row_pages(11, Some(10)),
    );
    probe("U-562", &world);
    world.produce(
        "U-563",
        Change::StatementFinish {
            terminal: TERMINAL,
            statement: mark,
        },
        Pages {
            status: Some(21),
            ..pages()
        },
    );
    probe("U-563", &world);
    // The compensated root keeps row 12 until T3 ends.
    refuses(
        "update Reserve of row 12 held by T3's compensated root",
        &world.inputs(
            Change::ReserveUpdate(&update_plan(4, 14, 12, blob("U-559-canonical"))),
            reserve_pages(14),
        ),
        FRAME_BYTES,
        HELD,
    );

    // Statement 4: an INSERT over the effect-2 H.
    let plan = ReservePlan {
        terminal: TERMINAL,
        container: page_ref(14, 2),
        table: table(),
        statement: 4,
        canonical: blob("U-564-canonical"),
    };
    let reserved = world.produce("U-564", Change::Reserve(&plan), reserve_pages(14));
    assert_eq!(reserved.job_ordinal, Some(25));
    let inserted = reserved.issued.map(|issued| issued.row.get()).unwrap();
    probe("U-564", &world);
    world.observe(
        "U-565",
        blob("U-565"),
        Change::Forward {
            ordinal: 25,
            statement: boundary(4, None),
        },
        row_pages(14, Some(10)),
    );
    probe("U-565", &world);
    // T3's own unpruned INSERT row is not the planned target.
    refuses(
        "update Reserve of T3's own INSERT row",
        &world.inputs(
            Change::ReserveUpdate(&update_plan(5, 15, inserted, blob("U-566-canonical"))),
            reserve_pages(15),
        ),
        FRAME_BYTES,
        Error::IdentityMismatch,
    );

    // Statement 5: row 11 at the same size (C = C' = 194) over the effect-0 H.
    let plan = update_plan(5, 15, 11, blob("U-566-canonical"));
    adopted_update(&world, "U-566", &plan, reserve_pages(15));
    let reserved = world.produce("U-566", Change::ReserveUpdate(&plan), reserve_pages(15));
    assert_eq!(reserved.job_ordinal, Some(26));
    probe("U-566", &world);
    world.observe(
        "U-567",
        blob("U-567"),
        Change::UpdateForward {
            ordinal: 26,
            statement: boundary(5, None),
        },
        row_pages(15, Some(14)),
    );
    probe("U-567", &world);
    world.produce(
        "U-568",
        Change::Commit { terminal: TERMINAL },
        Pages {
            jobs: Some(1),
            status: Some(21),
            ..pages()
        },
    );
    probe("U-568", &world);
    // The root's effect binds the opcode: INSERT's and DELETE's Prune of an
    // update root, and UpdatePrune of the INSERT root, are refused before
    // any output.
    for (label, change, pages) in [
        (
            "INSERT Prune of the update root 26",
            Change::Prune { ordinal: 26 },
            row_pages(15, Some(14)),
        ),
        (
            "DeletePrune of the update root 26",
            Change::DeletePrune { ordinal: 26 },
            row_pages(15, Some(14)),
        ),
        (
            "UpdatePrune of the INSERT root 25",
            Change::UpdatePrune { ordinal: 25 },
            row_pages(14, Some(10)),
        ),
    ] {
        refuses(
            label,
            &world.inputs(change, pages),
            FRAME_BYTES,
            Error::IdentityMismatch,
        );
    }

    // Live cleanup from the head: UpdatePrune (h1 over the effect-0 H),
    // Prune over the effect-2 H, UpdatePrune (h0).
    world.produce(
        "U-569",
        Change::UpdatePrune { ordinal: 26 },
        row_pages(15, Some(14)),
    );
    world.produce(
        "U-570",
        Change::Prune { ordinal: 25 },
        row_pages(14, Some(10)),
    );
    world.produce(
        "U-571",
        Change::UpdatePrune { ordinal: 23 },
        row_pages(10, None),
    );
    probe("U-571", &world);

    // The committed and the compensated update roots release their U.
    for (ordinal, page_no, names) in [
        (23, 10, ["U-572", "U-573", "U-574", "U-575"]),
        (24, 11, ["U-576", "U-577", "U-578", "U-579"]),
    ] {
        for name in &names[..3] {
            world.produce(
                name,
                Change::ReleaseRole { ordinal },
                release_pages(page_no),
            );
        }
        world.produce(
            names[3],
            Change::RootFree { ordinal },
            release_pages(page_no),
        );
    }
    probe("U-579", &world);
    world.rejects.as_ref().map_or(0, Cell::get)
}

/// A's groups of history U and the group before each: UpdateForward h0
/// (row 9 grows), UpdateForward over an effect-2 H (row 12 shrinks), its
/// UpdateClr, the INSERT Forward over an effect-2 H and the same-size
/// UpdateForward over an effect-0 H (row 11).
const A_GROUPS: [(&str, &str); 5] = [
    ("U-558", "U-557"),
    ("U-560", "U-559"),
    ("U-562", "U-561"),
    ("U-565", "U-564"),
    ("U-567", "U-566"),
];

/// T3's actor and Status location in history U.
pub(crate) fn a_actor() -> (TxnRef, StatusSlotRef) {
    (actor(7, 2), slot(21, 2))
}

/// T3's Terminal Job's ordinal in history U's Jobs bank.
pub(crate) const U_TERMINAL: usize = TERMINAL;

/// One literal of the UPDATE author's set (history U, `R-U-*`).
pub(crate) fn u_blob(name: &str) -> &'static [u8] {
    assert!(name.starts_with("U-") || name.starts_with("R-U-"), "{name}");
    blob(name)
}

/// The before states of A's history U groups, in history order (RUP-A),
/// with the same bookkeeping as history D's (`delete::ABefore`).
pub(crate) fn a_befores() -> Vec<delete::ABefore> {
    let mut out = Vec::new();
    history_u_with(false, |name, world| {
        let Some((group, _)) = A_GROUPS.iter().find(|(_, before)| *before == name) else {
            return;
        };
        out.push(delete::ABefore {
            name: group,
            previous_lsn: world.lsn,
            root: Box::new(world.root),
            jobs: Box::new(world.jobs[1].expect("Jobs page 1")),
            bank: Box::new(world.bank),
            status: Box::new(*World::find(&world.status, 21)),
            undo: world
                .undo
                .iter()
                .map(|(page_no, page)| (*page_no, Box::new(*page)))
                .collect(),
            leaf: Box::new(world.leaf.expect("RowLeaf")),
            transaction: Box::new(world.transaction.expect("Tx descriptor")),
            aging: Box::new(world.aging.expect("Aging descriptor")),
            frame: blob(group),
        });
    });
    assert_eq!(
        out.iter().map(|before| before.name).collect::<Vec<_>>(),
        A_GROUPS.map(|(group, _)| group)
    );
    out
}

/// History U's RowLeaf and Jobs bank after each named group, in history
/// order (the preflight classification of RUP-C,
/// `plan4_retention/row_update_tests.rs`).
pub(crate) fn u_leaves(names: &[&str]) -> Vec<delete::DLeaf> {
    let mut out = Vec::new();
    history_u_with(false, |name, world| {
        if names.contains(&name) {
            out.push((
                name.to_owned(),
                Box::new(world.leaf.expect("RowLeaf")),
                Box::new(world.bank),
            ));
        }
    });
    assert_eq!(
        out.iter()
            .map(|(name, ..)| name.as_str())
            .collect::<Vec<_>>(),
        names
    );
    out
}

/// C's whole-group observer over `frame`, one of A's history U groups
/// `group` (U-558 growing UpdateForward, U-562 UpdateClr of the shrinking
/// root), at the state before it with the RowLeaf `leaf` in place of the
/// World's (#leaf-reservation). A refusal precedes output.
pub(crate) fn a_update_over(group: &str, leaf: &Page, frame: &[u8]) -> Result<(), Error> {
    let (before, change, pages) = match group {
        "U-558" => (
            "U-557",
            Change::UpdateForward {
                ordinal: 23,
                statement: boundary(2, None),
            },
            row_pages(10, None),
        ),
        "U-562" => (
            "U-561",
            Change::UpdateClr {
                ordinal: 24,
                statement: Some(boundary(3, Some(undo_ref(10, 5)))),
            },
            row_pages(11, Some(10)),
        ),
        _ => panic!("{group}"),
    };
    let mut result = None;
    history_u_with(false, |name, world| {
        if name != before {
            return;
        }
        let mut after = loans(0xa5);
        let observed = observe(&over_leaf(world, change, pages, leaf), frame, &mut after);
        if observed.is_err() {
            assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
        }
        result = Some(observed.map(|_| ()));
    });
    result.unwrap_or_else(|| panic!("history U reaches {before}"))
}

/// The new value of row 11 that grows beside a reservation (review r1 #1):
/// 14 B longer than row 11's U-566 value, so C' − C = `grown().len()` − 14.
pub(crate) fn grown() -> Vec<u8> {
    canonical(11, Some(b"eleven, now twenty bytes"))
}

/// The ordinal of the growing update root `grown_world` reserves.
pub(crate) const GROWN_ORDINAL: usize = 25;

/// A state history U does not reach (review r1 #1): the U-560 state, where
/// the Retained shrinking root 24 holds D = 10, with C's growing update
/// Reserve of row 11 (statement 4, container 14, `grown`) produced over it
/// and installed. It has no literal: C's producer is RUP-C's
/// (`rup_c_leaf_reservation_bounds_update_reserve` takes the same Reserve
/// at its exact fit), and A's UpdateForward over it is checked by C's
/// observer at the leaf boundary (`grown_forward_over`).
fn grown_world() -> Box<World> {
    let mut out = None;
    let value = grown();
    history_u_with(false, |name, world| {
        if name != "U-560" {
            return;
        }
        let mut world = Box::new(world.clone());
        let plan = update_plan(4, 14, 11, &value);
        let mut after = loans(0xa5);
        let mut frame = vec![0xa5; FRAME_BYTES];
        let observation = produce(
            &world.inputs(Change::ReserveUpdate(&plan), reserve_pages(14)),
            &mut after,
            &mut frame,
        )
        .expect("the growing update Reserve beside D");
        assert_eq!(observation.job_ordinal, Some(GROWN_ORDINAL));
        world.install(
            "U-560-grow",
            &observation,
            &after,
            &frame[..observation.frame_bytes],
        );
        out = Some(world);
    });
    out.expect("history U reaches U-560")
}

/// `grown_world` as A's before state (`delete::ABefore`, no literal frame).
pub(crate) fn grown_before() -> delete::ABefore {
    let world = grown_world();
    delete::ABefore {
        name: "U-560-grow",
        previous_lsn: world.lsn,
        root: Box::new(world.root),
        jobs: Box::new(world.jobs[1].expect("Jobs page 1")),
        bank: Box::new(world.bank),
        status: Box::new(*World::find(&world.status, 21)),
        undo: world
            .undo
            .iter()
            .map(|(page_no, page)| (*page_no, Box::new(*page)))
            .collect(),
        leaf: Box::new(world.leaf.expect("RowLeaf")),
        transaction: Box::new(world.transaction.expect("Tx descriptor")),
        aging: Box::new(world.aging.expect("Aging descriptor")),
        frame: &[],
    }
}

/// C's whole-group observer over A's growing UpdateForward `frame` of row
/// 11 at `grown_world`'s state with the RowLeaf `leaf` in place of the
/// World's: `entries + (C' − C) + D` is re-checked with D = 10.
pub(crate) fn grown_forward_over(leaf: &Page, frame: &[u8]) -> Result<(), Error> {
    let mut world = grown_world();
    world.leaf = Some(*leaf);
    let change = Change::UpdateForward {
        ordinal: GROWN_ORDINAL,
        statement: boundary(4, None),
    };
    let mut after = loans(0xa5);
    let result = observe(
        &world.inputs(change, row_pages(14, Some(11))),
        frame,
        &mut after,
    )
    .map(|_| ());
    if result.is_err() {
        // A reservation refusal precedes output.
        assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
    }
    result
}

#[test]
fn rup_c_update_history_matches_literals() {
    assert_eq!(history_u(false), 0);
}

#[test]
fn rup_c_update_history_rejects_variants() {
    let checked = history_u(true);
    println!("RUP_C_REJECTS checked={checked}");
    assert_eq!(checked, REJECTS.len() + FRAMING.len());
}

/// S1-a over history U: every observation repeated through
/// `observe_borrowed` (H's and D's own observations subtracted).
#[test]
fn rup_c_update_history_borrowed_observer() {
    let (_, d_accepted, d_refused) = with_borrowed(|| delete::history_d(false));
    let (checked, accepted, refused) = with_borrowed(|| history_u(true));
    println!("RUP_C_BORROWED accepted={accepted} refused={refused} d={d_accepted}");
    assert_eq!(
        (checked, accepted - d_accepted, refused - d_refused),
        (REJECTS.len() + FRAMING.len(), U_GROUPS, REJECTS.len())
    );
}

/// S1-d over history U: every C production repeated through
/// `produce_borrowed` over a dirty lent scratch.
#[test]
fn rup_c_update_history_borrowed_producer() {
    PRODUCED.set(Some(0));
    delete::history_d(false);
    let d = PRODUCED.take().expect("produced count");
    PRODUCED.set(Some(0));
    history_u(false);
    let all = PRODUCED.take().expect("produced count");
    println!("RUP_C_PRODUCED groups={}", all - d);
    assert_eq!(all - d, U_GROUPS - U_OBSERVED);
}

// ---- leaf space (dev3-row-update.md#leaf-reservation) ----------------------

/// `world`'s inputs for `change` with the RowLeaf `leaf` in place of its own.
fn over_leaf<'a>(world: &'a World, change: Change<'a>, pages: Pages, leaf: &'a Page) -> Inputs<'a> {
    let mut inputs = world.inputs(change, pages);
    inputs.table.as_mut().unwrap().leaf_image = Some(leaf);
    inputs
}

/// C's production of `change` over `leaf`, output discarded.
fn produced_over(
    world: &World,
    change: Change<'_>,
    pages: Pages,
    leaf: &Page,
) -> Result<(), Error> {
    let inputs = over_leaf(world, change, pages, leaf);
    let mut after = loans(0xa5);
    let mut frame = vec![0xa5; FRAME_BYTES];
    produce(&inputs, &mut after, &mut frame).map(|_| ())
}

/// RUP-C: the widened D at the states of history U, and the update Reserve's
/// room check `entries + max(0, C'−C) + D ≤ 16188`. Before U-557 (D 0) the
/// growing update Reserve of row 9 (+30 B) takes a leaf with exactly 30 B
/// free and is refused one byte short (before output, OrderMismatch as for
/// INSERT); the shrinking Reserve of row 12 needs no room (a full leaf
/// fits). After U-560 the Retained shrinking root holds D = 10: an INSERT
/// Reserve and a growing update Reserve of row 11 take their own growth plus
/// D and are refused one byte short, and a leaf already inside D is Corrupt
/// for both. A Retained growing or same-size root holds no D, and UpdateClr
/// gives the shrinking root's D back.
#[test]
fn rup_c_leaf_reservation_bounds_update_reserve() {
    let grown = canonical(11, Some(b"eleven, now twenty bytes"));
    let growth = grown.len() - 14;
    let insert_cell = 180 + blob("U-564-canonical").len();
    let mut seen = Vec::new();
    history_u_with(false, |name, world| {
        let d = widened_d(world);
        let expected = match name {
            "U-560" | "U-561" => 10,
            _ => 0,
        };
        assert_eq!(d, expected, "{name} D");
        let leaf = world.leaf.as_ref().unwrap();
        match name {
            "U-556" => {
                let plan = update_plan(2, 10, 9, blob("U-557-canonical"));
                let change = Change::ReserveUpdate(&plan);
                assert_eq!(
                    produced_over(world, change, reserve_pages(10), &padded_leaf(leaf, 30)),
                    Ok(()),
                    "{name}: growing update Reserve, exact fit"
                );
                refuses(
                    &format!("{name}: growing update Reserve one byte short"),
                    &over_leaf(world, change, reserve_pages(10), &padded_leaf(leaf, 29)),
                    FRAME_BYTES,
                    Error::OrderMismatch,
                );
                let plan = update_plan(2, 10, 12, blob("U-559-canonical"));
                assert_eq!(
                    produced_over(
                        world,
                        Change::ReserveUpdate(&plan),
                        reserve_pages(10),
                        &padded_leaf(leaf, 0)
                    ),
                    Ok(()),
                    "{name}: shrinking update Reserve over a full leaf"
                );
            }
            "U-560" => {
                let insert = ReservePlan {
                    terminal: TERMINAL,
                    container: page_ref(14, 2),
                    table: table(),
                    statement: 4,
                    canonical: blob("U-564-canonical"),
                };
                let update = update_plan(4, 14, 11, &grown);
                for (label, change, room) in [
                    ("INSERT Reserve", Change::Reserve(&insert), insert_cell + 4),
                    (
                        "growing update Reserve",
                        Change::ReserveUpdate(&update),
                        growth,
                    ),
                ] {
                    assert_eq!(
                        produced_over(
                            world,
                            change,
                            reserve_pages(14),
                            &padded_leaf(leaf, room + d)
                        ),
                        Ok(()),
                        "{name}: {label}, exact fit beside D"
                    );
                    refuses(
                        &format!("{name}: {label} one byte short of D"),
                        &over_leaf(
                            world,
                            change,
                            reserve_pages(14),
                            &padded_leaf(leaf, room + d - 1),
                        ),
                        FRAME_BYTES,
                        Error::OrderMismatch,
                    );
                    refuses(
                        &format!("{name}: {label} over a leaf inside D"),
                        &over_leaf(world, change, reserve_pages(14), &padded_leaf(leaf, d - 1)),
                        FRAME_BYTES,
                        Error::Corrupt,
                    );
                }
            }
            _ => return,
        }
        seen.push((name.to_owned(), d));
    });
    println!("RUP_C_LEAF_RESERVE\t{seen:?}");
    assert_eq!(seen.len(), 2);
}

/// RUP-C: the leaf steps after an update Reserve. A's growing UpdateForward
/// (U-558 at the U-557 state) re-checks `entries + (C'−C) + D`: over a leaf
/// one byte short the observer refuses it before output (OrderMismatch,
/// #leaf-reservation: Reserve proved the fit under the same gate). C's
/// UpdatePrune (U-569 at the U-568 state, D 0) takes a full leaf (it keeps
/// C'), and A's UpdateClr of the shrinking root (U-562 at the U-561 state,
/// D 10) is refused over a leaf already inside D. Their acceptance at
/// exactly capacity needs A's frame over that leaf, not the literal: it is
/// RUP-A's (`rup_a_update_groups_at_the_leaf_reservation_boundary`), as is
/// a growing UpdateForward beside D > 0
/// (`rup_a_growing_update_forward_beside_d`, over `grown_world`).
#[test]
fn rup_c_leaf_reservation_forward_prune_and_clr() {
    let mut seen = Vec::new();
    history_u_with(false, |name, world| {
        let d = widened_d(world);
        let leaf = world.leaf.as_ref().unwrap();
        match name {
            "U-557" => {
                let change = Change::UpdateForward {
                    ordinal: 23,
                    statement: boundary(2, None),
                };
                let padded = padded_leaf(leaf, 30 - 1);
                let inputs = over_leaf(world, change, row_pages(10, None), &padded);
                let mut after = loans(0xa5);
                assert_eq!(
                    observe(&inputs, blob("U-558"), &mut after).map(|_| ()),
                    Err(Error::OrderMismatch),
                    "{name}: growing UpdateForward one byte short"
                );
                assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
            }
            "U-561" => {
                assert_eq!(d, 10, "{name} D");
                let change = Change::UpdateClr {
                    ordinal: 24,
                    statement: Some(boundary(3, Some(undo_ref(10, 5)))),
                };
                let padded = padded_leaf(leaf, d - 1);
                let inputs = over_leaf(world, change, row_pages(11, Some(10)), &padded);
                let mut after = loans(0xa5);
                assert_eq!(
                    observe(&inputs, blob("U-562"), &mut after).map(|_| ()),
                    Err(Error::Corrupt),
                    "{name}: UpdateClr over a leaf inside D"
                );
                assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
            }
            "U-568" => {
                assert_eq!(d, 0, "{name} D");
                assert_eq!(
                    produced_over(
                        world,
                        Change::UpdatePrune { ordinal: 26 },
                        row_pages(15, Some(14)),
                        &padded_leaf(leaf, 0)
                    ),
                    Ok(()),
                    "{name}: UpdatePrune over a full leaf"
                );
            }
            _ => return,
        }
        seen.push((name.to_owned(), d));
    });
    println!("RUP_C_LEAF_STEPS\t{seen:?}");
    assert_eq!(seen.len(), 3);
}

// ---- mutants that need a World state ---------------------------------------

/// RUP-C (#update-states): an UpdateForward candidate whose new row carries
/// another RowId than its Job's. A's U-558 (Job row 9) with the new row's key
/// and record RowId changed to 10, which the U-557 leaf does not hold (row 11
/// follows), its native after, redo page image and digests rebound: the
/// frame is framed and bound, and C's whole-group observer refuses it as
/// IdentityMismatch (the new row is not the Job's RowId).
#[test]
fn rup_c_update_forward_of_another_row_is_refused() {
    let mut seen = false;
    history_u_with(false, |name, world| {
        if name != "U-557" {
            return;
        }
        let change = Change::UpdateForward {
            ordinal: 23,
            statement: boundary(2, None),
        };
        let inputs = world.inputs(change, row_pages(10, None));
        let mut after = loans(0);
        observe(&inputs, blob("U-558"), &mut after).unwrap();
        let (mutant, _) = rebind(blob("U-558"), world.lsn, 1, &after[LEAF], |cell| {
            assert_eq!(cell.len(), 224, "the new row cell C'");
            assert_eq!(cell[4..12], 9u64.to_be_bytes());
            cell[4..12].copy_from_slice(&10u64.to_be_bytes());
            cell[20..28].copy_from_slice(&10u64.to_le_bytes());
        });
        let mut computed = loans(0xa5);
        assert_eq!(
            observe(&inputs, &mutant, &mut computed).map(|_| ()),
            Err(Error::IdentityMismatch),
            "C's observer"
        );
        assert!(computed.iter().flatten().all(|byte| *byte == 0xa5));
        seen = true;
    });
    assert!(seen);
}

/// RUP-C (#previous-proof): the effect-2 H proof. UpdateForward U-560 (over
/// the growing update root 23) and A's INSERT Forward U-565 (over the same
/// root after the rollback) are accepted in history U; with the H page's U
/// incoming count +1 (page digest resealed), C's observer refuses both.
#[test]
fn rup_c_effect2_previous_proof_binds_the_h_page() {
    let mut seen = Vec::new();
    history_u_with(false, |name, world| {
        let (group, change, pages) = match name {
            "U-559" => (
                "U-560",
                Change::UpdateForward {
                    ordinal: 24,
                    statement: boundary(3, None),
                },
                row_pages(11, Some(10)),
            ),
            "U-564" => (
                "U-565",
                Change::Forward {
                    ordinal: 25,
                    statement: boundary(4, None),
                },
                row_pages(14, Some(10)),
            ),
            _ => return,
        };
        let mut world = Box::new(world.clone());
        let mut page = Box::new(*World::find(&world.undo, 10));
        let start = usize::from(u16::from_le_bytes([page[128], page[129]]));
        let incoming = u64_at(&page[..], start + 80) + 1;
        page[start + 80..start + 88].copy_from_slice(&incoming.to_le_bytes());
        let digest = content_digest(&page[..DIGEST_OFFSET]);
        page[DIGEST_OFFSET..].copy_from_slice(&digest);
        World::replace(&mut world.undo, 10, &page);
        let mut after = loans(0xa5);
        let refused = observe(&world.inputs(change, pages), blob(group), &mut after).map(|_| ());
        assert!(refused.is_err(), "{group}: H incoming +1 accepted");
        assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
        seen.push((group.to_owned(), refused));
    });
    println!("RUP_C_H_PROOF\t{seen:?}");
    assert_eq!(seen.len(), 2);
}
