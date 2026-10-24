#![allow(dead_code)]
use crate::storage::wal_io::{StatusRole, UndoRole};

fn probe<'s>(status: StatusRole<'s>) -> UndoRole<'s> {
    status // expected diagnostic
}
