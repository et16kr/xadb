#![allow(dead_code)]
use crate::storage::disk::RowControlObservation;
fn needs_static(_: &'static [[u8; 1024]; 64]) {}
fn probe(control: RowControlObservation<'_>) {
    needs_static(control.jobs()); // expected diagnostic
}
