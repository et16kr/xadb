#![allow(dead_code)]
use crate::storage::disk::CallerCredit;
fn probe(credit: CallerCredit<'_, '_, '_, '_>) {
    let _copy = credit.clone(); // expected diagnostic
}
