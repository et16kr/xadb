//! Cursor credits on the actual original isolation slot. Each credit is a
//! 512-byte RAM charge only; returning one subtracts just its own charge from
//! the slot and the global total, and leaves other credits, the statement view,
//! the slot's phase/sequence and every snapshot as they were.
use super::super::super::super::plan4_pages::RetentionCause;
use super::*;
use crate::storage::disk::{IsolationLevel, TransactionOptions};

const READ_ONLY: TransactionOptions = TransactionOptions {
    isolation: IsolationLevel::ReadCommitted,
    read_only: true,
};

#[test]
fn cursor_credits_return_only_their_own_charge() {
    case("TX_CURSOR_CREDIT", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = (status, undo, core);
            let before = state.lock().unwrap().images();
            let (mut writer, _, _) = live.split();
            let first = writer.begin_transaction(owner, READ_ONLY, private).unwrap();
            let second = writer
                .begin_transaction(owner, READ_ONLY, reader_private)
                .unwrap();
            let one = writer.begin_statement(owner, &first).unwrap();
            let two = writer.begin_statement(owner, &second).unwrap();
            assert_eq!(one.test_charges(), (2048, 1024));
            assert_eq!(two.test_charges(), (2048, 1024));

            // Two cursors of the same view each hold their own charge.
            let a = one.reserve_cursor_credit().unwrap();
            let b = one.reserve_cursor_credit().unwrap();
            let c = two.reserve_cursor_credit().unwrap();
            assert_eq!(one.test_charges(), (3584, 2048));
            assert_eq!(two.test_charges(), (3584, 1536));
            a.release_checked().unwrap();
            assert_eq!(one.test_charges(), (3072, 1536));
            assert_eq!(two.test_charges(), (3072, 1536));
            drop(b);
            assert_eq!(one.test_charges(), (2560, 1024));
            c.release_checked().unwrap();
            assert_eq!(two.test_charges(), (2048, 1024));
            assert_eq!(one.test_sequence(), 1);
            assert!(one.q().is_ok() && two.q().is_ok());

            assert!(writer.finish_statement(owner, two).is_ok());
            assert!(writer.finish_read_only(owner, second).is_ok());
            assert_eq!(one.test_charges(), (1024, 1024));
            assert!(writer.finish_statement(owner, one).is_ok());
            assert!(writer.finish_read_only(owner, first).is_ok());
            assert_eq!(live.catalog_test_isolation(), (3, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}

#[test]
fn cursor_credit_capacity_is_unchanged_before_any_charge() {
    case("TX_CURSOR_CAP", || {
        let oracle = Oracle::new();
        with_fresh!(oracle, |live,
                             owner,
                             flusher,
                             status,
                             undo,
                             private,
                             reader_private,
                             core,
                             fence,
                             state| {
            let _ = (status, undo, reader_private, core);
            let before = state.lock().unwrap().images();
            let (mut writer, _, _) = live.split();
            let transaction = writer.begin_transaction(owner, READ_ONLY, private).unwrap();
            let view = writer.begin_statement(owner, &transaction).unwrap();
            // 49152B / 512B = 96 charges; the lease and the view hold two.
            let mut cursors = Vec::new();
            for _ in 0..94 {
                cursors.push(view.reserve_cursor_credit().unwrap());
            }
            assert_eq!(view.test_charges(), (49_152, 49_152));
            let failure = match view.reserve_cursor_credit() {
                Err(e) => e,
                Ok(_) => panic!("credit beyond the 49152-byte envelope"),
            };
            assert!(matches!(failure.cause, RetentionCause::Capacity));
            assert_eq!(failure.effect, Effect::Unchanged);
            assert_eq!(view.test_charges(), (49_152, 49_152));
            assert!(!fence.is_fenced());
            let last = cursors.pop().unwrap();
            last.release_checked().unwrap();
            cursors.push(view.reserve_cursor_credit().unwrap());
            assert_eq!(view.test_charges(), (49_152, 49_152));
            for credit in cursors {
                credit.release_checked().unwrap();
            }
            assert_eq!(view.test_charges(), (1024, 1024));
            assert!(writer.finish_statement(owner, view).is_ok());
            assert!(writer.finish_read_only(owner, transaction).is_ok());
            assert_eq!(live.catalog_test_isolation(), (2, 0, 0));
            assert_eq!(state.lock().unwrap().images(), before);
            assert!(!fence.is_fenced());
            owner.check_idle().unwrap();
            flusher.check_idle().unwrap();
        });
    });
}
