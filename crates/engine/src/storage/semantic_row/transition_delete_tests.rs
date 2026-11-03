//! RDL-C (dev3-row-delete.md#transitions): the C-owned DELETE groups and the
//! whole-group observer against the independent
//! `module-row-format/transition-delete/author.py` literals. History D starts
//! from the end of N1's history H and runs a second actor T2 through every
//! DELETE transition C produces or observes: the delete RowReserve and its
//! Cancel, A's DeleteForward (h0, effect-1 H, effect-0 H) and DeleteClr
//! (effect-1 H), A's INSERT Forward and C's Prune over an effect-1 H, both
//! DeletePrunes, and the ReleaseRole/RootFree of a committed and of a
//! compensated delete root. Every C-produced group is byte-equal to the
//! literal and every literal is accepted by the observer; the `R-D-*`
//! variants are refused. Literal observations confer no actual source,
//! reservation, pin, staging, installation or recovery authority.

use super::super::DeletePlan;
use super::*;
use crate::storage::retention_page::RetentionError;

/// The Job census refusal of one RowId held by two non-Free roots.
const HELD: Error = Error::Queue(RetentionError::IdentityMismatch);

/// Every `R-D-*` variant and the refusal its base's observer must give.
pub(super) const REJECTS: [(&str, Error); 11] = [
    ("R-D-521-effect", Error::Corrupt),
    ("R-D-524-undo", Error::Corrupt),
    ("R-D-524-leaf", Error::Corrupt),
    ("R-D-526-item", Error::Corrupt),
    ("R-D-528-restore", Error::Corrupt),
    ("R-D-528-count", Error::Corrupt),
    ("R-D-531-edge", Error::Corrupt),
    ("R-D-533-descriptor", Error::Corrupt),
    ("R-D-535-digest", Error::Corrupt),
    ("R-D-537-undo", Error::Corrupt),
    ("R-D-538-stamp", Error::Corrupt),
];

/// History D's groups, C's and A's.
const D_GROUPS: usize = 30;
/// Of them A's, observed only: DeleteForward ×3, DeleteClr, INSERT Forward.
const D_OBSERVED: usize = 5;

/// T2's terminal Job, created by D-516 (Python history D).
const TERMINAL: usize = 18;

fn delete_plan(statement: u64, container: u64, row: u64) -> DeletePlan {
    DeletePlan {
        terminal: TERMINAL,
        container: page_ref(container, 2),
        table: table(),
        statement,
        row: RowId::new(row).unwrap(),
    }
}

/// Reserve-time pages: Jobs page 1, T2's Status page 20, the Free container.
fn reserve_pages(container: u64) -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(20),
        undo: Some(container),
        table: true,
        ..pages()
    }
}

/// Row-operation pages: the root's U and, with h, its predecessor's.
fn row_pages(undo: u64, previous: Option<u64>) -> Pages {
    Pages {
        jobs: Some(1),
        status: Some(20),
        undo: Some(undo),
        previous,
        table: true,
    }
}

/// The RowIds in the current RowLeaf, in key order.
fn leaf_rows(world: &World) -> Vec<u64> {
    let leaf = world.leaf.as_ref().unwrap();
    let count = usize::from(u16::from_le_bytes([leaf[72], leaf[73]]));
    (1..count)
        .map(|slot| u64::from_be_bytes(cell(leaf, slot)[4..12].try_into().unwrap()))
        .collect()
}

/// RDL-C (#api): a delete RowReserve replayed as `ReserveAdopted` adopts the
/// effect byte and RowId of the candidate's Job after and recomputes every
/// other byte: the live observation and its after pages, through
/// `observe_borrowed` over a lent tail. Its effect-flipped variant adopts an
/// INSERT and is refused.
fn adopted_delete(world: &World, name: &str, plan: &DeletePlan, pages: Pages) {
    let bytes = blob(name);
    let live = world.inputs(Change::ReserveDelete(plan), pages);
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
    if name == "D-521" {
        let mut lane = new_lane();
        let mut replayed = loans(0x5a);
        assert_eq!(
            observe_borrowed(
                &replay,
                blob("R-D-521-effect"),
                &mut replayed,
                lent(&mut lane)
            )
            .map(|_| ()),
            Err(Error::Corrupt),
            "R-D-521-effect adopted"
        );
        restored(name, &lane);
    }
    println!("RDL_C_ADOPTED\t{name}\trow={}", plan.row.get());
}

/// History D (see the module comment) with the refusals that need its
/// intermediate states; `rejects` also checks the `R-D-*` variants.
pub(super) fn history_d(rejects: bool) -> usize {
    history_d_with(rejects, |_, _| {})
}

/// History D calling `probe` with the World after each named group.
fn history_d_with(rejects: bool, mut probe: impl FnMut(&str, &World)) -> usize {
    let mut world = history_h_world(false);
    world.rejects = rejects.then(|| Cell::new(0));
    World::replace(&mut world.status, 20, &page_of(blob("D-status20-0")));
    World::replace(&mut world.undo, 15, &page_of(blob("D-undo15-0")));
    World::replace(&mut world.undo, 16, &page_of(blob("D-undo16-0")));
    let begin = world.produce(
        "D-516",
        Change::Begin {
            actor: actor(6, 2),
            status: slot(20, 2),
            scope: JobScope::Row,
        },
        Pages {
            jobs: Some(1),
            status: Some(20),
            ..pages()
        },
    );
    assert_eq!(begin.job_ordinal, Some(TERMINAL));

    // T's committed, pruned INSERT roots 15 and 17 still hold rows 4 and 11:
    // the after bank of a delete Reserve would hold one RowId twice
    // (#classification; C's preflight classifies this Busy in 2b-2).
    for row in [4, 11] {
        let plan = delete_plan(1, 11, row);
        refuses(
            &format!("delete Reserve of row {row} held by T's root"),
            &world.inputs(Change::ReserveDelete(&plan), reserve_pages(11)),
            FRAME_BYTES,
            HELD,
        );
    }
    let undo10 = Pages {
        jobs: Some(1),
        undo: Some(10),
        ..pages()
    };
    for name in ["D-517", "D-518", "D-519"] {
        world.produce(name, Change::ReleaseRole { ordinal: 15 }, undo10);
    }
    world.produce("D-520", Change::RootFree { ordinal: 15 }, undo10);

    // Statement 1: a delete Reserve and its Cancel. No RowId is issued.
    let row_high = u64_at(cell(&world.root, 0), 432);
    let plan = delete_plan(1, 11, 2);
    adopted_delete(&world, "D-521", &plan, reserve_pages(11));
    let reserved = world.produce("D-521", Change::ReserveDelete(&plan), reserve_pages(11));
    assert_eq!(
        reserved.issued,
        Some(Issued {
            row: RowId::new(2).unwrap(),
            undo: undo_ref(11, 4),
            sequence: 5,
            mutation: 1,
        })
    );
    assert_eq!(reserved.job_ordinal, Some(16));
    assert_eq!(u64_at(cell(&world.root, 0), 432), row_high, "row_high");
    assert_eq!(world.bank[16][284], 1, "effect byte");
    world.produce(
        "D-522",
        Change::Cancel { ordinal: 16 },
        Pages {
            jobs: Some(1),
            status: Some(20),
            undo: Some(11),
            ..pages()
        },
    );

    // Statement 2: delete row 2 (h0).
    let plan = delete_plan(2, 11, 2);
    adopted_delete(&world, "D-523", &plan, reserve_pages(11));
    world.produce("D-523", Change::ReserveDelete(&plan), reserve_pages(11));
    probe("D-523", &world);
    world.observe(
        "D-524",
        blob("D-524"),
        Change::DeleteForward {
            ordinal: 16,
            statement: boundary(2, None),
        },
        row_pages(11, None),
    );
    probe("D-524", &world);

    // Statement 3: delete row 9 over the effect-1 H, then roll it back.
    let plan = delete_plan(3, 10, 9);
    adopted_delete(&world, "D-525", &plan, reserve_pages(10));
    let reserved = world.produce("D-525", Change::ReserveDelete(&plan), reserve_pages(10));
    assert_eq!(reserved.job_ordinal, Some(19));
    probe("D-525", &world);
    world.observe(
        "D-526",
        blob("D-526"),
        Change::DeleteForward {
            ordinal: 19,
            statement: boundary(3, None),
        },
        row_pages(10, Some(11)),
    );
    probe("D-526", &world);
    let mark = boundary(3, Some(undo_ref(11, 5)));
    world.produce(
        "D-527",
        Change::RollbackStart {
            terminal: TERMINAL,
            statement: Some(mark),
        },
        Pages {
            status: Some(20),
            undo: Some(10),
            ..pages()
        },
    );
    probe("D-527", &world);
    world.observe(
        "D-528",
        blob("D-528"),
        Change::DeleteClr {
            ordinal: 19,
            statement: Some(mark),
        },
        row_pages(10, Some(11)),
    );
    world.produce(
        "D-529",
        Change::StatementFinish {
            terminal: TERMINAL,
            statement: mark,
        },
        Pages {
            status: Some(20),
            ..pages()
        },
    );
    probe("D-529", &world);
    // The compensated root keeps row 9 until T2 ends.
    let plan = delete_plan(4, 15, 9);
    refuses(
        "delete Reserve of row 9 held by T2's compensated root",
        &world.inputs(Change::ReserveDelete(&plan), reserve_pages(15)),
        FRAME_BYTES,
        HELD,
    );

    // Statement 4: an INSERT over the effect-1 H.
    let plan = ReservePlan {
        terminal: TERMINAL,
        container: page_ref(15, 2),
        table: table(),
        statement: 4,
        canonical: blob("D-530-canonical"),
    };
    let reserved = world.produce("D-530", Change::Reserve(&plan), reserve_pages(15));
    assert_eq!(reserved.job_ordinal, Some(20));
    assert_eq!(
        reserved.issued.map(|issued| issued.row),
        Some(RowId::new(row_high + 1).unwrap())
    );
    probe("D-530", &world);
    world.observe(
        "D-531",
        blob("D-531"),
        Change::Forward {
            ordinal: 20,
            statement: boundary(4, None),
        },
        row_pages(15, Some(11)),
    );
    // C's delete Reserve takes only a committed, pruned, inline row: T2's own
    // unpruned INSERT row and an absent RowId are not the planned target and
    // are refused before output (the preflight's Unsupported/0-row answers
    // for them, #classification, are stage 2b-2's).
    for (label, row) in [
        ("T2's own INSERT row", row_high + 1),
        ("an absent RowId", 1000),
    ] {
        let plan = delete_plan(5, 16, row);
        refuses(
            &format!("delete Reserve of {label}"),
            &world.inputs(Change::ReserveDelete(&plan), reserve_pages(16)),
            FRAME_BYTES,
            Error::IdentityMismatch,
        );
    }

    // Statement 5: delete row 4 (freed by D-520) over the effect-0 H.
    let plan = delete_plan(5, 16, 4);
    adopted_delete(&world, "D-532", &plan, reserve_pages(16));
    let reserved = world.produce("D-532", Change::ReserveDelete(&plan), reserve_pages(16));
    assert_eq!(reserved.job_ordinal, Some(21));
    probe("D-532", &world);
    world.observe(
        "D-533",
        blob("D-533"),
        Change::DeleteForward {
            ordinal: 21,
            statement: boundary(5, None),
        },
        row_pages(16, Some(15)),
    );
    world.produce(
        "D-534",
        Change::Commit { terminal: TERMINAL },
        Pages {
            jobs: Some(1),
            status: Some(20),
            ..pages()
        },
    );
    probe("D-534", &world);
    // The root's effect binds the opcode (`prepare`'s role rule): INSERT's
    // Prune of a delete root is refused before any output.
    refuses(
        "INSERT Prune of the delete root 21",
        &world.inputs(Change::Prune { ordinal: 21 }, row_pages(16, Some(15))),
        FRAME_BYTES,
        Error::IdentityMismatch,
    );

    // Live cleanup from the head: DeletePrune (h1), Prune over the effect-1
    // H, DeletePrune (h0).
    world.produce(
        "D-535",
        Change::DeletePrune { ordinal: 21 },
        row_pages(16, Some(15)),
    );
    refuses(
        "DeletePrune of the INSERT root 20",
        &world.inputs(Change::DeletePrune { ordinal: 20 }, row_pages(15, Some(11))),
        FRAME_BYTES,
        Error::IdentityMismatch,
    );
    world.produce(
        "D-536",
        Change::Prune { ordinal: 20 },
        row_pages(15, Some(11)),
    );
    world.produce(
        "D-537",
        Change::DeletePrune { ordinal: 16 },
        row_pages(11, None),
    );
    assert_eq!(leaf_rows(&world), [9, 11, 12], "rows after cleanup");
    probe("D-537", &world);

    // The committed and the compensated delete roots release their U.
    for (ordinal, page_no, names) in [
        (16, 11, ["D-538", "D-539", "D-540", "D-541"]),
        (19, 10, ["D-542", "D-543", "D-544", "D-545"]),
    ] {
        let undo = Pages {
            jobs: Some(1),
            undo: Some(page_no),
            ..pages()
        };
        for name in &names[..3] {
            world.produce(name, Change::ReleaseRole { ordinal }, undo);
        }
        world.produce(names[3], Change::RootFree { ordinal }, undo);
    }
    world.rejects.map_or(0, Cell::into_inner)
}

/// A's five history D groups and the group after which their before state
/// stands: DeleteForward (h0, effect-1 H, effect-0 H), DeleteClr and the
/// INSERT Forward over an effect-1 H.
const A_GROUPS: [(&str, &str); 5] = [
    ("D-524", "D-523"),
    ("D-526", "D-525"),
    ("D-528", "D-527"),
    ("D-531", "D-530"),
    ("D-533", "D-532"),
];

/// The stabilized state before one of A's history D groups, for A's
/// producer tests (RDL-A): every page and descriptor a producer lane lends,
/// and the literal A must reproduce. Test-only bookkeeping, no authority.
pub(crate) struct ABefore {
    pub(crate) name: &'static str,
    pub(crate) previous_lsn: u64,
    pub(crate) root: Box<Page>,
    /// Jobs page 1, which holds every Job of T2.
    pub(crate) jobs: Box<Page>,
    pub(crate) bank: Box<[[u8; JOB_BYTES]; JOB_COUNT]>,
    /// T2's Status page 20.
    pub(crate) status: Box<Page>,
    pub(crate) undo: Vec<(u64, Box<Page>)>,
    pub(crate) leaf: Box<Page>,
    pub(crate) transaction: Box<Descriptor>,
    pub(crate) aging: Box<Descriptor>,
    pub(crate) frame: &'static [u8],
}

/// T2's actor and Status location in history D.
pub(crate) fn a_actor() -> (TxnRef, StatusSlotRef) {
    (actor(6, 2), slot(20, 2))
}

/// The before states of A's history D groups, in history order.
pub(crate) fn a_befores() -> Vec<ABefore> {
    let mut out = Vec::new();
    history_d_with(false, |name, world| {
        let Some((group, _)) = A_GROUPS.iter().find(|(_, before)| *before == name) else {
            return;
        };
        out.push(ABefore {
            name: group,
            previous_lsn: world.lsn,
            root: Box::new(world.root),
            jobs: Box::new(world.jobs[1].expect("Jobs page 1")),
            bank: Box::new(world.bank),
            status: Box::new(*World::find(&world.status, 20)),
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

/// The RowLeaf and Jobs bank after each named history D group, for the C
/// preflight's classification tests (RDL-C, dev3-row-delete.md#classification).
pub(crate) fn d_leaves(names: &[&str]) -> Vec<DLeaf> {
    let mut out = Vec::new();
    history_d_with(false, |name, world| {
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

/// The T2 Terminal Job's ordinal in history D's Jobs bank.
pub(crate) const D_TERMINAL: usize = TERMINAL;

/// A history D state: its group's name, its RowLeaf and its Jobs bank.
pub(crate) type DLeaf = (String, Box<Page>, Box<[[u8; JOB_BYTES]; JOB_COUNT]>);

#[test]
fn rdl_c_delete_history_matches_literals() {
    assert_eq!(history_d(false), 0);
}

#[test]
fn rdl_c_delete_history_rejects_variants() {
    let checked = history_d(true);
    println!("RDL_C_REJECTS checked={checked}");
    assert_eq!(checked, REJECTS.len());
}

/// S1-a over history D: every observation repeated through
/// `observe_borrowed` (H's own observations subtracted).
#[test]
fn rdl_c_delete_history_borrowed_observer() {
    let (_, h_accepted, _) = with_borrowed(|| history_h(false));
    let (checked, accepted, refused) = with_borrowed(|| history_d(true));
    println!("RDL_C_BORROWED accepted={accepted} refused={refused} h={h_accepted}");
    assert_eq!(
        (checked, accepted - h_accepted, refused),
        (REJECTS.len(), D_GROUPS, REJECTS.len())
    );
}

/// S1-d over history D: every C production repeated through
/// `produce_borrowed` over a dirty lent scratch.
#[test]
fn rdl_c_delete_history_borrowed_producer() {
    PRODUCED.set(Some(0));
    history_h(false);
    let h = PRODUCED.take().expect("produced count");
    PRODUCED.set(Some(0));
    history_d(false);
    let all = PRODUCED.take().expect("produced count");
    println!("RDL_C_PRODUCED groups={}", all - h);
    assert_eq!(all - h, D_GROUPS - D_OBSERVED);
}

// ---- leaf reservation (dev3-row-delete.md#leaf-reservation) ----------------

/// D: Σ(C − 180) over the table's Retained delete roots in the World's bank.
fn retained_d(world: &World) -> usize {
    world
        .bank
        .iter()
        .filter_map(|cell| row::decode_job(cell, namespace()).ok())
        .filter(|job| {
            job.state() == row::JobState::RootRetained && job.effect() == row::RowEffect::Delete
        })
        .map(|job| job.cell_bytes() as usize - 180)
        .sum()
}

/// `leaf` with committed filler rows (keys from 1000, row 9's version, text
/// values) added until `TREE_ENTRY_CAPACITY - entry bytes == free`. Each
/// entry costs its record, the 12-byte cell head and the 4-byte slot.
pub(crate) fn padded_leaf(leaf: &Page, free: usize) -> Box<Page> {
    let expected = TreeExpectation {
        identity: PageIdentity {
            database: namespace().database,
            page: leaf_ref(leaf),
            owner: table().object,
            owner_generation: table().generation,
        },
        kind: PageKind::RowLeaf,
        level: 0,
        arity: 0,
    };
    let columns = columns();
    let schema = RowSchemaInput::Columns(&columns);
    let lsn = PageLsn(u64_at(leaf, 64));
    let entries = |page: &Page| {
        let view = tree_page::decode_tree_page(page, expected).unwrap();
        (0..view.len())
            .map(|index| match view.entry(index).unwrap().payload {
                EntryPayload::Value(value) => value.len() + 16,
                _ => panic!("value"),
            })
            .sum::<usize>()
    };
    let version = {
        let view = tree_page::decode_tree_page(leaf, expected).unwrap();
        let at = view.lower_bound(&9u64.to_be_bytes()).unwrap();
        assert!(at.equal);
        let EntryPayload::Value(value) = view.entry(at.index).unwrap().payload else {
            panic!("row 9");
        };
        row_record::decode_row_record_with_schema(
            value,
            RecordExpectation {
                namespace: namespace(),
                row: RowId::new(9).unwrap(),
            },
            schema,
        )
        .unwrap()
        .version()
    };
    let mut page = Box::new(*leaf);
    let mut key = 1000u64;
    loop {
        let used = entries(&page);
        let left = tree_page::TREE_ENTRY_CAPACITY - free - used;
        if left == 0 {
            return page;
        }
        // A text row costs 168 + 18 + n record bytes + 16: 202..=2232.
        let entry = if left <= 2232 {
            left
        } else {
            2232.min(left - 202)
        };
        assert!(entry >= 202, "filler entry {entry}");
        let text = vec![b'x'; entry - 202];
        let value = canonical(key as i64, Some(&text));
        let row = RowId::new(key).unwrap();
        let mut record = vec![0; 4096];
        let length = row_record::encode_row_record_with_schema(
            RecordExpectation {
                namespace: namespace(),
                row,
            },
            schema,
            RecordVersion { row, ..version },
            row_record::RowPayload::Inline(
                row_record::decode_canonical_row_with_schema(&value, schema).unwrap(),
            ),
            &mut record,
        )
        .unwrap();
        let mut cell_bytes = vec![0; 4096];
        let cell = tree_page::encode_leaf_cell(
            PageKind::RowLeaf,
            0,
            &key.to_be_bytes(),
            &record[..length],
            &mut cell_bytes,
        )
        .unwrap();
        let view = tree_page::decode_tree_page(&page[..], expected).unwrap();
        let mut refs: Vec<&[u8]> = vec![&[]; MAX_LEAF_EDIT_CELLS];
        let mut next: Box<Page> = vec![0; PAGE_BYTES].into_boxed_slice().try_into().unwrap();
        view.prepare_leaf_edit(LeafEdit::Insert(cell), &mut refs[..])
            .unwrap()
            .encode(lsn, &mut next[..])
            .unwrap();
        page = next;
        key += 1;
    }
}

/// RDL-C: the leaf reservation D (#leaf-reservation) at three states of
/// history D. With the delete roots Retained, an INSERT Reserve takes its
/// cell only while entries + cell + D stay within the capacity (refused one
/// byte short, before output, though the cell alone would fit), and a leaf
/// whose entries already exceed capacity - D is refused as Corrupt by every
/// leaf step, the delete Reserve included. DeleteClr gives its D back, so
/// after D-529 only the first root's D remains, and after the last
/// DeletePrune (D-537) no delete root is Retained.
#[test]
fn rdl_c_leaf_reservation_bounds_every_leaf_step() {
    let canonical_row = blob("D-530-canonical");
    let c = 180 + canonical_row.len();
    let mut seen = Vec::new();
    history_d_with(false, |name, world| {
        if !["D-524", "D-526", "D-529", "D-537"].contains(&name) {
            return;
        }
        let reserved = retained_d(world);
        let expected = match name {
            "D-524" | "D-529" => 199 - 180,
            "D-526" => (199 - 180) + (194 - 180),
            _ => 0,
        };
        assert_eq!(reserved, expected, "{name} D");
        seen.push((name.to_owned(), reserved));
        let plan = ReservePlan {
            terminal: TERMINAL,
            container: page_ref(15, 2),
            table: table(),
            statement: 4,
            canonical: canonical_row,
        };
        let leaf = world.leaf.as_ref().unwrap();
        let insert = |free: usize| {
            let padded = padded_leaf(leaf, free);
            let mut inputs = world.inputs(Change::Reserve(&plan), reserve_pages(15));
            inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
            let mut after = loans(0xa5);
            let mut frame = vec![0xa5; FRAME_BYTES];
            produce(&inputs, &mut after, &mut frame).map(|_| ())
        };
        if name == "D-537" {
            // T2 has committed; a new Reserve is refused by its Status.
            return;
        }
        assert_eq!(insert(c + 4 + reserved), Ok(()), "{name}: exact fit");
        let label = format!("{name}: INSERT Reserve one byte short of D");
        let padded = padded_leaf(leaf, c + 4 + reserved - 1);
        let mut inputs = world.inputs(Change::Reserve(&plan), reserve_pages(15));
        inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
        refuses(&label, &inputs, FRAME_BYTES, Error::OrderMismatch);
        if reserved > 0 {
            let label = format!("{name}: INSERT Reserve over a leaf inside D");
            let padded = padded_leaf(leaf, reserved - 1);
            let mut inputs = world.inputs(Change::Reserve(&plan), reserve_pages(15));
            inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
            refuses(&label, &inputs, FRAME_BYTES, Error::Corrupt);
            let plan = delete_plan(4, 15, 11);
            let label = format!("{name}: delete Reserve over a leaf inside D");
            let mut inputs = world.inputs(Change::ReserveDelete(&plan), reserve_pages(15));
            inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
            refuses(&label, &inputs, FRAME_BYTES, Error::Corrupt);
        }
    });
    let names: Vec<&str> = seen.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["D-524", "D-526", "D-529", "D-537"]);
}

/// C's whole-group observer and leaf.rs over `frame`, an INSERT Forward of
/// row r (D-531's change) whose producer wrote the U, Status and RowLeaf
/// afters `after`, at the D-530 state of history D with the RowLeaf `leaf`
/// in place of the World's (#leaf-reservation). A group C accepts then goes
/// through the World's observation over its own afters, so status.rs,
/// undo.rs, leaf.rs and jobs.rs see it over the same leaf too.
pub(crate) fn d531_over(
    leaf: &Page,
    frame: &[u8],
    after: RowAfters<'_>,
) -> (Result<(), Error>, Result<(), Error>) {
    let mut result = None;
    history_d_with(false, |name, world| {
        if name != "D-530" {
            return;
        }
        let change = Change::Forward {
            ordinal: 20,
            statement: boundary(4, None),
        };
        let pages = row_pages(15, Some(11));
        let mut world = Box::new(world.clone());
        world.leaf = Some(*leaf);
        let local = world
            .observe_leaf(frame, change, pages, after)
            .expect("D-530 produced r's RowReserve")
            .map(|_| ());
        let mut computed = loans(0xa5);
        let observed = observe(&world.inputs(change, pages), frame, &mut computed);
        if observed.is_ok() {
            world.observe("D-531-padded", frame, change, pages);
        } else {
            assert!(computed.iter().flatten().all(|byte| *byte == 0xa5));
        }
        result = Some((observed.map(|_| ()), local));
    });
    result.expect("history D reaches D-530")
}

/// RDL-C (#delete-states): a DeleteForward candidate that tombstones another
/// RowId than its Job's. A's D-524 group (Job row 2) with the tombstone's key
/// and record RowId changed to 3, which the D-523 leaf does not hold (row 4
/// follows), and its native after, redo page image and digests rebound, so
/// the frame is framed and bound and its leaf after is a valid page. C's
/// whole-group observer refuses it (its recomputed group differs), and
/// leaf.rs, which no longer finds the Job's row 2 in the after leaf, refuses
/// it too.
/// A refuses the same change of RowId before output (RDL-A,
/// `group_delete_tests.rs`).
#[test]
fn rdl_c_delete_forward_of_another_row_is_refused() {
    let mut seen = false;
    history_d_with(false, |name, world| {
        if name != "D-523" {
            return;
        }
        let rows = leaf_rows(world);
        assert!(rows.contains(&2) && !rows.contains(&3) && rows.contains(&4));
        let change = Change::DeleteForward {
            ordinal: 16,
            statement: boundary(2, None),
        };
        let pages = row_pages(11, None);
        let inputs = world.inputs(change, pages);
        let mut after = loans(0);
        observe(&inputs, blob("D-524"), &mut after).unwrap();
        let (mutant, leaf_after) = rebind(blob("D-524"), world.lsn, 1, &after[LEAF], |cell| {
            assert_eq!(cell.len(), 180, "the tombstone cell");
            cell[4..12].copy_from_slice(&3u64.to_be_bytes());
            cell[20..28].copy_from_slice(&3u64.to_le_bytes());
        });
        let mut computed = loans(0xa5);
        assert_eq!(
            observe(&inputs, &mutant, &mut computed).map(|_| ()),
            Err(Error::Corrupt),
            "C's observer"
        );
        let local = world
            .observe_leaf(
                &mutant,
                change,
                pages,
                RowAfters {
                    undo: &after[UNDO],
                    status: &after[STATUS],
                    leaf: &leaf_after,
                },
            )
            .unwrap();
        assert_eq!(local.map(|_| ()), Err(Error::IdentityMismatch), "leaf.rs");
        seen = true;
    });
    assert!(seen);
}

/// RDL-C: the leaf reservation in the leaf steps after a Reserve. A's INSERT
/// Forward (D-531 at the D-530 state, D = 19) re-checks entries + cell + D:
/// over a leaf where entries + D fit but the cell would break the
/// reservation, the observer refuses it (#leaf-reservation calls this
/// corruption, not Capacity: Reserve proved the fit under the same gate; the
/// refusal is OrderMismatch, the split check it extends). C's DeletePrune
/// (D-535 at the D-534 state, two Retained delete roots) takes a leaf at
/// exactly capacity − D and refuses one whose entries already exceed it as
/// Corrupt, like every leaf step.
#[test]
fn rdl_c_leaf_reservation_forward_and_delete_prune() {
    let c = 180 + blob("D-530-canonical").len();
    let mut seen = Vec::new();
    history_d_with(false, |name, world| {
        let reserved = retained_d(world);
        let leaf = world.leaf.as_ref().unwrap();
        match name {
            "D-530" => {
                assert_eq!(reserved, 199 - 180, "{name} D");
                let padded = padded_leaf(leaf, c + 4 + reserved - 1);
                let mut inputs = world.inputs(
                    Change::Forward {
                        ordinal: 20,
                        statement: boundary(4, None),
                    },
                    row_pages(15, Some(11)),
                );
                inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
                let mut after = loans(0xa5);
                assert_eq!(
                    observe(&inputs, blob("D-531"), &mut after).map(|_| ()),
                    Err(Error::OrderMismatch),
                    "{name}: INSERT Forward one byte short of D"
                );
                assert!(after.iter().flatten().all(|byte| *byte == 0xa5));
            }
            "D-534" => {
                assert_eq!(reserved, (199 - 180) + (194 - 180), "{name} D");
                // Entries exactly capacity − D is a legal leaf.
                let padded = padded_leaf(leaf, reserved);
                let mut inputs =
                    world.inputs(Change::DeletePrune { ordinal: 21 }, row_pages(16, Some(15)));
                inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
                let mut after = loans(0xa5);
                let mut frame = vec![0xa5; FRAME_BYTES];
                assert_eq!(
                    produce(&inputs, &mut after, &mut frame).map(|_| ()),
                    Ok(()),
                    "{name}: DeletePrune at exactly capacity - D"
                );
                let padded = padded_leaf(leaf, reserved - 1);
                let mut inputs =
                    world.inputs(Change::DeletePrune { ordinal: 21 }, row_pages(16, Some(15)));
                inputs.table.as_mut().unwrap().leaf_image = Some(&*padded);
                refuses(
                    &format!("{name}: DeletePrune over a leaf inside D"),
                    &inputs,
                    FRAME_BYTES,
                    Error::Corrupt,
                );
            }
            _ => return,
        }
        seen.push((name.to_owned(), reserved));
    });
    println!("RDL_C_LEAF_STEPS\t{seen:?}");
    assert_eq!(seen.len(), 2);
}
