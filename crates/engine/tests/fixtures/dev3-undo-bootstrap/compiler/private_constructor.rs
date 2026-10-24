#![allow(dead_code)]
use crate::storage::wal_io::UndoRole;

fn probe<'s>(role: UndoRole<'s>) -> UndoRole<'s> {
    UndoRole { ..role } // expected diagnostic
}
