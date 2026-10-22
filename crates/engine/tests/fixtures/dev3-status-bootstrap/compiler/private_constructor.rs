#![allow(dead_code)]
use crate::storage::wal_io::StatusRole;

fn probe<'s>(role: StatusRole<'s>) -> StatusRole<'s> {
    StatusRole { ..role } // expected diagnostic
}
