#![allow(dead_code)]
use super::status_init_operation::{fixed::FixedPrepared, PreparedInit};
fn probe(inner: PreparedInit<'_, '_>) {
    std::hint::black_box(FixedPrepared { inner }); // expected diagnostic
}
