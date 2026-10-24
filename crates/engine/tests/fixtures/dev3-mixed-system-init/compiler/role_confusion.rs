#![allow(dead_code)]
use super::status_init_operation::mixed;
use crate::storage::wal_io::StatusRole;

fn probe<'p, 's>(role: &'p StatusRole<'s>, mut plan: mixed::Plan<'p, 's>) {
    plan.role = role; // expected diagnostic
    std::hint::black_box(plan);
}
