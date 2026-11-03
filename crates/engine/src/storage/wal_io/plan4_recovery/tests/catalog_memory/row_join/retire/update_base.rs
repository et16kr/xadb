//! RUP-R05..R07 (dev3-row-update.md#recovery, #tests): C's base4 of the
//! UPDATE crash states over actual captured bases, with RDL-R's harness
//! (`delete_base.rs`): each case restarts `crash_images` to READY and takes
//! the base its Rebase captured; each refusal edits the base's RowLeaf or
//! Undo page by raw offsets (no product encoder) and reseals it. The
//! accepted census is a state-machine fact.
use super::super::super::super::super::row_recovery::{Drive, PageRead, restart_with};
use super::super::delete::base::{
    ARENA, FLAGS, LOGICAL, MUTATION, ROW_RECORD, STAMP, UNDO, census, committed_stamp, find,
    leaf_cell, page_cells, rebased, record, renumbered, rewrite,
};
use super::*;
use crate::storage::plan4_recovery_base::row::{RowBaseError, actor::Error as ActorError};

/// A NULL row's record over the nullable T: 168 + the 5 B canonical.
const NULL_RECORD: usize = 173;
/// The Live update U of the replaced row: 432 + L, L = C - 180 = 13 (the
/// old Int64 value).
const LIVE_U: usize = 432 + 13;
/// D of the shrinking UPDATE: C - C' = 193 - 185.
const SHRINK_D: usize = 8;

/// RUP-R05: C's base4 takes every effect-2 crash state. Reserved (the old
/// committed row, U Free), Retained beside a Ready or a Leased Terminal (the
/// new row `Transaction(T)` with its Live RowValue U), ReleasingCompensated
/// (the restored old row, U Releasing), and ReleasingCommitted, taken by
/// stopping a restart of the committed files after its first Row cell
/// change (the recovered UpdatePrune: the new row restamped, U Releasing).
/// The UPDATE keeps the row's key in every state, so each base holds two
/// rows. The shrinking UPDATE's Active base (C' = C - 8, D = 8) is taken
/// too.
#[test]
fn rup_r05_base4_takes_each_update_state() {
    case_within("RUP-R05", std::time::Duration::from_secs(600), || {
        let committed = crash_images(Crash::Committed).0;
        let pruned = restart_with(
            &committed,
            Some(RowRecoveryPolicy::WithRowClr),
            Drive::CrashAfterCells(1),
            0,
            &PageRead,
        );
        let mut seen = Vec::new();
        for (name, images) in [
            ("reserved", crash_images(Crash::Reserved).0),
            ("committed", committed.clone()),
            ("active", crash_images(Crash::Active).0),
            ("rolled-back", crash_images(Crash::RolledBack).0),
            ("pruned", pruned.images),
            ("active-shrink", crash_images_of(Crash::Active, true).0),
        ] {
            let (bytes, expected) = rebased(&images);
            let census = census(&bytes, expected).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            println!("RUP_R05 {name} {census:?}");
            seen.push((
                name,
                census.actors,
                census.retained,
                census.releasing,
                census.rows,
            ));
        }
        assert_eq!(
            seen,
            [
                ("reserved", 1, 0, 0, 2),
                ("committed", 1, 1, 0, 2),
                ("active", 1, 1, 0, 2),
                ("rolled-back", 1, 0, 1, 2),
                ("pruned", 1, 0, 1, 2),
                ("active-shrink", 1, 1, 0, 2),
            ]
        );
    });
}

/// RUP-R06: refusals over the shrinking UPDATE's Active base (Retained
/// root, Leased Terminal; the new NULL row of C' = 185 at RowId 1, the kept
/// row 101 at RowId 2), each one edit of its RowLeaf or update U page.
/// - The new row restamped `Committed(3)`: its Retained root no longer
///   finds its own row (actor, Identity).
/// - An unpaired Inline row: the actor's new row at the kept row's RowId
///   with a new mutation, which no Retained root names (actor reverse
///   scan, Identity).
/// - The new row's cell not C': the new row's version over an Int64 body,
///   the record 8 B longer (actor, Identity; the planned projection
///   differs with it, the same conjunction binds both).
/// - A non-canonical update U: the first byte of its RowValue changed;
///   entry (a) with the table schema refuses it (actor, Undo).
/// - The widened leaf reservation: the leaf padded with copies of the kept
///   row and of a foreign transaction's Inline row so that entries + D
///   (D = C - C' = 8) is above the capacity (16,188 B) is Corrupt; exactly
///   at the capacity the reservation holds and the padded RowIds above
///   row_high are refused after it (Order).
#[test]
fn rup_r06_base4_refuses_broken_update_states() {
    case_within("RUP-R06", std::time::Duration::from_secs(600), || {
        let (bytes, expected) = rebased(&crash_images_of(Crash::Active, true).0);
        assert!(census(&bytes, expected).is_ok());
        let n = find(&bytes, 1, |_| true);
        let cells = page_cells(&bytes[ARENA + n * PAGE_BYTES..ARENA + (n + 1) * PAGE_BYTES]);
        assert_eq!(cells.len(), 3, "metadata, the new row, the kept row");
        let (metadata, new, kept) = (
            cells[0].clone(),
            record(&cells[1]).to_vec(),
            record(&cells[2]).to_vec(),
        );
        assert_eq!((new.len(), kept.len()), (NULL_RECORD, ROW_RECORD));
        assert_eq!(
            (new[FLAGS], kept[FLAGS]),
            (0b10, 0),
            "the new row has its U"
        );
        let base_cells = || vec![metadata.clone(), leaf_cell(1, &new), leaf_cell(2, &kept)];
        let edited = |cells: Vec<Vec<u8>>| {
            let mut copy = bytes.clone();
            rewrite(&mut copy, n, &cells);
            census(&copy, expected)
        };
        // The unedited relayout is the same base.
        assert!(edited(base_cells()).is_ok());

        let mut restamped = new.clone();
        restamped[STAMP].copy_from_slice(&committed_stamp(3));
        let mut cells = base_cells();
        cells[1] = leaf_cell(1, &restamped);
        let stamp = edited(cells);
        println!("RUP_R06 restamped {stamp:?}");
        assert_eq!(stamp, Err(RowBaseError::Actor(ActorError::Identity)));

        let mut unpaired = renumbered(&new, 2);
        unpaired[MUTATION].copy_from_slice(&2u64.to_le_bytes());
        let mut cells = base_cells();
        cells[2] = leaf_cell(2, &unpaired);
        let orphan = edited(cells);
        println!("RUP_R06 unpaired {orphan:?}");
        assert_eq!(orphan, Err(RowBaseError::Actor(ActorError::Identity)));

        // The kept Int64 row's record carrying the new row's RowId, flags,
        // stamp, logical and U.
        let mut grown = renumbered(&kept, 1);
        grown[FLAGS] = new[FLAGS];
        for field in [STAMP, LOGICAL, UNDO] {
            grown[field.clone()].copy_from_slice(&new[field]);
        }
        let mut cells = base_cells();
        cells[1] = leaf_cell(1, &grown);
        let cell = edited(cells);
        println!("RUP_R06 cell {cell:?}");
        assert_eq!(cell, Err(RowBaseError::Actor(ActorError::Identity)));

        let u = find(&bytes, 8, |cells| {
            cells.len() == 1 && cells[0].len() == LIVE_U
        });
        let mut value = page_cells(&bytes[ARENA + u * PAGE_BYTES..ARENA + (u + 1) * PAGE_BYTES]);
        value[0][432] ^= 0xff;
        let mut copy = bytes.clone();
        rewrite(&mut copy, u, &value);
        let undo = census(&copy, expected);
        println!("RUP_R06 undo {undo:?}");
        assert!(
            matches!(undo, Err(RowBaseError::Actor(ActorError::Undo(_)))),
            "{undo:?}"
        );

        // entries = 189 (new) + 197 (kept) + 197a + 189b, D = 8. A foreign
        // transaction's Inline row: the kept row's writer stamps it
        // (`Transaction` names the logical's TxnId).
        let mut foreign = new.clone();
        foreign[LOGICAL].copy_from_slice(&kept[LOGICAL]);
        foreign[STAMP.start + 8..STAMP.start + 48]
            .copy_from_slice(&kept[LOGICAL.start..LOGICAL.start + 40]);
        let padded = |rows: u64, inline: u64| {
            let mut cells = base_cells();
            for row in 3..3 + rows {
                cells.push(leaf_cell(row, &renumbered(&kept, row)));
            }
            for row in 3 + rows..3 + rows + inline {
                cells.push(leaf_cell(row, &renumbered(&foreign, row)));
            }
            let used: usize = cells[1..].iter().map(|cell| cell.len() + 4).sum();
            (used + SHRINK_D, edited(cells))
        };
        let (over, widened) = padded(38, 44);
        let (at, held) = padded(37, 45);
        println!("RUP_R06 over={over} {widened:?} at={at} {held:?}");
        assert_eq!((over, at), (16_196, 16_188));
        assert_eq!(widened, Err(RowBaseError::Corrupt));
        assert_eq!(held, Err(RowBaseError::Order));
    });
}

/// RUP-R07 (the release cursor 3 branch, as RDL-R07): a ReleasingCompensated
/// update root's restored old row is checked against U's before. At
/// release cursor 0 that before holds the stamp and the logical; at cursor
/// 3 the codec has dropped the stamp, and only the logical is left. The
/// rolled-back files' restart is stopped once its cleanup has moved the
/// root to cursor 3, and the base the next restart captures is taken.
/// - Both bases are accepted as captured (two rows: the restored and the
///   kept).
/// - The restored row's logical changed is refused at either cursor
///   (actor, Identity).
/// - The restored row restamped `Committed(1)`: at cursor 0 the actor's
///   comparison with U's stamp refuses it (Identity); at cursor 3 the
///   graph's committed-row order does (Order).
#[test]
fn rup_r07_base4_takes_the_cursor_three_compensated_update_by_its_logical() {
    case_within("RUP-R07", std::time::Duration::from_secs(600), || {
        let rolled = crash_images(Crash::RolledBack).0;
        let mut cursor_three = None;
        for cells in 1..8 {
            let stopped = restart_with(
                &rolled,
                Some(RowRecoveryPolicy::WithRowClr),
                Drive::CrashAfterCells(cells),
                0,
                &PageRead,
            );
            let last = stopped.trace.last().unwrap().clone();
            println!("RUP_R07 cells={cells} trace={last:?}");
            if last.iter().any(|(_, state, cursor)| {
                *state == JobState::RootReleasingCompensated && *cursor == 3
            }) {
                cursor_three = Some(stopped.images);
                break;
            }
        }
        let cursor_three = cursor_three.expect("the cleanup reaches cursor 3");
        for (cursor, images, with_stamp) in [(0, rolled, true), (3, cursor_three, false)] {
            let (bytes, expected) = rebased(&images);
            let accepted = census(&bytes, expected).unwrap_or_else(|e| panic!("{cursor}: {e:?}"));
            assert_eq!(
                (accepted.actors, accepted.releasing, accepted.rows),
                (1, 1, 2),
                "cursor {cursor}"
            );
            let n = find(&bytes, 1, |_| true);
            let cells = page_cells(&bytes[ARENA + n * PAGE_BYTES..ARENA + (n + 1) * PAGE_BYTES]);
            assert_eq!(cells.len(), 3, "metadata, the restored row, the kept row");
            let (metadata, restored, kept) = (
                cells[0].clone(),
                record(&cells[1]).to_vec(),
                record(&cells[2]).to_vec(),
            );
            assert_eq!((restored.len(), restored[FLAGS]), (ROW_RECORD, 0));
            let edited = |row: Vec<u8>| {
                let mut copy = bytes.clone();
                rewrite(
                    &mut copy,
                    n,
                    &[metadata.clone(), leaf_cell(1, &row), leaf_cell(2, &kept)],
                );
                census(&copy, expected)
            };
            assert!(
                edited(restored.clone()).is_ok(),
                "cursor {cursor}: relayout"
            );
            let mut relogged = restored.clone();
            relogged[MUTATION].copy_from_slice(&77u64.to_le_bytes());
            let logical = edited(relogged);
            assert_eq!(
                logical,
                Err(RowBaseError::Actor(ActorError::Identity)),
                "cursor {cursor}: logical"
            );
            let mut restamped = restored.clone();
            restamped[STAMP].copy_from_slice(&committed_stamp(1));
            let stamp = edited(restamped);
            assert_eq!(
                stamp,
                Err(if with_stamp {
                    RowBaseError::Actor(ActorError::Identity)
                } else {
                    RowBaseError::Order
                }),
                "cursor {cursor}: stamp"
            );
            println!("RUP_R07 cursor={cursor} logical={logical:?} stamp={stamp:?}");
        }
    });
}
