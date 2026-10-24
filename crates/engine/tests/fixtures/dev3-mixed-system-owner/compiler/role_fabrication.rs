#![allow(dead_code)]
use crate::storage::wal_io::bootstrap::{FixedSystemRole, StatusRole};
fn probe(status: StatusRole<'_>) {
    std::hint::black_box(FixedSystemRole { status }); // expected diagnostic
}
