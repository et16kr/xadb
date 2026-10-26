#![allow(dead_code)]
use crate::storage::disk::RowProducerLane;
fn probe(lane: RowProducerLane<'_>) {
    let parts = lane.parts();
    drop(lane); // expected diagnostic
    std::hint::black_box(parts);
}
