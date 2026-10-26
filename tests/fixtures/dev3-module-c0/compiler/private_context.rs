#![allow(dead_code)]
use crate::storage::disk::RowPreflightContext;
fn probe(context: RowPreflightContext) {
    std::hint::black_box(context.table); // expected diagnostic
}
