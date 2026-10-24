#![allow(dead_code)]
use super::status_init_operation::{fixed::FixedPrepared, PreparedInit};
fn old(_: PreparedInit<'_, '_>) {}
fn probe(prepared: FixedPrepared<'_, '_>) {
    old(prepared); // expected diagnostic
}
