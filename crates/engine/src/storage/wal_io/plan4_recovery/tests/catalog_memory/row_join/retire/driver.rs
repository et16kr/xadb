//! The live Row cleanup caller (reclamation `#live-row-cleanup-caller`): A's
//! cleanup drain `retire_finished_live`, called after A's Commit or
//! `finish_aborted_live` returned, over the actual C supply. Expectations
//! are state-machine facts (groups installed, held steps, freed Jobs, a
//! reused Status, the WAL left as it was by a held step), as in LRR-01..06,
//! never values read back from the same producer.
use super::*;
use crate::storage::{
    disk::RowMutationObservation,
    row::insert::driver::{
        Retired,
        live::{LiveCall, commit_live, finish_aborted_live, insert_live, retire_finished_live},
    },
};

/// A's cleanup drain through the driver; it must not stop.
macro_rules! drained {
    ($writer:expr, $owner:expr, $flush:ident, $flusher:ident, $private:expr) => {{
        let mut call = LiveCall::new($writer, $owner, &mut $flush, $flusher);
        match retire_finished_live(&mut call, $private) {
            Ok(retired) => retired,
            Err(stop) => panic!("A's cleanup drain stopped {stop:?}"),
        }
    }};
}

/// One drain that installed `groups` and held `held`, with nothing else left
/// and no unlink (these tables' leaves are their roots).
fn done(groups: usize, held: usize) -> Retired {
    Retired {
        groups,
        held,
        complete: true,
        unlinked: 0,
        unlink_held: 0,
    }
}

/// LRC-J01: on one Row Status, a one-row transaction through A's driver is
/// committed, a second is fully rolled back and finished, and a third is
/// committed. After each, A's drain installs that actor's groups (six after
/// a Commit, five with no Prune after the abort), every Row Job is Free and
/// the next Begin on the same Status succeeds. A drain with nothing left
/// installs nothing. A new statement reads the two committed rows and not
/// the rolled back one.
#[test]
fn lrc_j01_drain_after_commit_and_abort_frees_every_row_job() {
    case_within("LRC-J01", std::time::Duration::from_secs(120), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let _ = state;
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let mut rows = Vec::new();
                let mut aborted = None;
                for (round, value) in [7_i64, 8, 9].into_iter().enumerate() {
                    let tx = writer
                        .begin_row_transaction(
                            owner,
                            IsolationLevel::Serializable,
                            &row_status,
                            private,
                            &mut flush,
                        )
                        .unwrap_or_else(|error| panic!("Begin {round}: {error:?}"));
                    let view = writer.begin_statement(owner, &tx).unwrap();
                    let table = issue_table!(reader, owner, view, reader_private, flush, flusher);
                    let applied = a_insert!(
                        &mut writer,
                        owner,
                        flush,
                        flusher,
                        view,
                        &table,
                        private,
                        [value]
                    );
                    let row = applied[0].row();
                    if round == 1 {
                        aborted = Some(row);
                        {
                            let mut rollback = writer
                                .start_row_rollback(
                                    owner,
                                    &tx,
                                    None,
                                    RollbackScope::Transaction,
                                    private,
                                    &mut flush,
                                    flusher,
                                )
                                .unwrap();
                            loop {
                                let target = match rollback.next().unwrap() {
                                    RollbackStep::Candidate(mut task) => {
                                        let proposal =
                                            produce_clr(task.producer_lane().unwrap()).unwrap();
                                        let prepared = applied!(task.prepare(proposal));
                                        applied!(prepared.apply());
                                        None
                                    }
                                    RollbackStep::FlushRequired { target } => Some(target),
                                    RollbackStep::Complete => break,
                                };
                                if let Some(target) = target {
                                    rollback.flush(&target).unwrap();
                                }
                            }
                            applied!(rollback.finish());
                        }
                        {
                            let mut call = LiveCall::new(&mut writer, owner, &mut flush, flusher);
                            if let Err(error) = finish_aborted_live(&mut call, tx) {
                                panic!("A's finish_aborted refused {:?}", error.failure);
                            }
                        }
                        assert_eq!(
                            drained!(&mut writer, owner, flush, flusher, private),
                            done(5, 0),
                            "aborted round {round}"
                        );
                    } else {
                        let q = a_commit!(&mut writer, owner, flush, flusher, tx, private);
                        assert_eq!(q, 2 + rows.len() as u64);
                        rows.push((row, value));
                        assert_eq!(
                            drained!(&mut writer, owner, flush, flusher, private),
                            done(6, 0),
                            "committed round {round}"
                        );
                    }
                    assert_row_jobs_free(writer.catalog_test_jobs());
                    assert!(!fence.is_fenced());
                }
                assert_eq!(
                    drained!(&mut writer, owner, flush, flusher, private),
                    done(0, 0)
                );
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
                let read = writer
                    .begin_transaction(
                        b,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let view = writer.begin_statement(b, &read).unwrap();
                assert_eq!(view.q().unwrap(), 3);
                let table = issue_table!(reader, b, view, reader_private, flush, flusher);
                for (row, value) in &rows {
                    assert_eq!(
                        point_at!(reader, b, reader_private, &view, &table, *row),
                        Some(*value)
                    );
                }
                let aborted = aborted.unwrap();
                assert_eq!(
                    point_at!(reader, b, reader_private, &view, &table, aborted),
                    None
                );
                assert!(writer.finish_statement(b, view).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert!(!fence.is_fenced());
                println!(
                    "LRC_J01 aborted={aborted:?} rounds=commit,abort,commit drains=6,5,6,0 rows={rows:?}"
                );
            }
        );
    });
}

/// LRC-J02: a statement snapshot older than the Commit holds the head Prune
/// (`Visibility(Busy)`, unchanged). A's drain skips it, finds no other
/// step (the actor's later groups wait on the Prune), and leaves the WAL
/// and the Jobs as they were. After the old statement ends, the next drain
/// installs all six groups.
#[test]
fn lrc_j02_drain_leaves_a_held_prune_for_a_later_drain() {
    case_within("LRC-J02", std::time::Duration::from_secs(120), || {
        let oracle = Oracle::new();
        with_fresh!(
            oracle,
            frames = 16,
            |live, owner, flusher, status, undo, private, reader_private, core, fence, state| {
                let (row_status, _) =
                    create_table(live, owner, flusher, status, undo, private, true);
                let (mut writer, reader, mut flush) = live.split();
                let mut b_owner = core.try_owner(OwnerRole::Foreground).unwrap();
                let b = &mut b_owner;
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
                let row = a_insert!(
                    &mut writer,
                    owner,
                    flush,
                    flusher,
                    view,
                    &table,
                    private,
                    [7]
                )[0]
                .row();
                let read = writer
                    .begin_transaction(
                        b,
                        TransactionOptions {
                            read_only: true,
                            isolation: IsolationLevel::ReadCommitted,
                        },
                        reader_private,
                    )
                    .unwrap();
                let old = writer.begin_statement(b, &read).unwrap();
                let q = a_commit!(&mut writer, owner, flush, flusher, tx, private);
                assert!(old.q().unwrap() < q);
                let jobs = writer.catalog_test_jobs().to_vec();
                let (wal, _) = files(state);
                for _ in 0..2 {
                    assert_eq!(
                        drained!(&mut writer, owner, flush, flusher, private),
                        done(0, 1)
                    );
                    assert_eq!(files(state).0, wal, "a held drain stages nothing");
                    assert_eq!(writer.catalog_test_jobs().to_vec(), jobs);
                    assert!(!fence.is_fenced());
                    owner.check_idle().unwrap();
                }
                assert!(writer.finish_statement(b, old).is_ok());
                assert_eq!(
                    drained!(&mut writer, owner, flush, flusher, private),
                    done(6, 0)
                );
                assert_row_jobs_free(writer.catalog_test_jobs());
                let new = writer.begin_statement(b, &read).unwrap();
                let new_table = issue_table!(reader, b, new, reader_private, flush, flusher);
                assert_eq!(
                    point_at!(reader, b, reader_private, &new, &new_table, row),
                    Some(7)
                );
                assert!(writer.finish_statement(b, new).is_ok());
                assert!(writer.finish_read_only(b, read).is_ok());
                assert!(!fence.is_fenced());
                println!("LRC_J02 q={q} held_drains=2 then drained=6");
            }
        );
    });
}

#[path = "structural_effect.rs"]
mod structural_effect;
#[path = "structural_prepare.rs"]
mod structural_prepare;
