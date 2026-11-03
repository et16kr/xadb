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
    world.observe(
        "D-531",
        blob("D-531"),
        Change::Forward {
            ordinal: 20,
            statement: boundary(4, None),
        },
        row_pages(15, Some(11)),
    );

    // Statement 5: delete row 4 (freed by D-520) over the effect-0 H.
    let plan = delete_plan(5, 16, 4);
    adopted_delete(&world, "D-532", &plan, reserve_pages(16));
    let reserved = world.produce("D-532", Change::ReserveDelete(&plan), reserve_pages(16));
    assert_eq!(reserved.job_ordinal, Some(21));
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

    // Live cleanup from the head: DeletePrune (h1), Prune over the effect-1
    // H, DeletePrune (h0).
    world.produce(
        "D-535",
        Change::DeletePrune { ordinal: 21 },
        row_pages(16, Some(15)),
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

/// `leaf` with committed filler rows (keys from 1000, row 9's version, text
/// values) added until `TREE_ENTRY_CAPACITY - entry bytes == free`. Each
/// entry costs its record, the 12-byte cell head and the 4-byte slot.
fn padded_leaf(leaf: &Page, free: usize) -> Box<Page> {
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
        let reserved: usize = world
            .bank
            .iter()
            .filter_map(|cell| row::decode_job(cell, namespace()).ok())
            .filter(|job| {
                job.state() == row::JobState::RootRetained && job.effect() == row::RowEffect::Delete
            })
            .map(|job| job.cell_bytes() as usize - 180)
            .sum();
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
