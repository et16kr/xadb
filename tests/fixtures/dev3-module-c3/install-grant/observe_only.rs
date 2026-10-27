#![allow(dead_code)]
use crate::storage::disk::InstallGrant;
fn probe(grant: InstallGrant<'_, '_, '_>) {
    std::hint::black_box((grant.identity(), grant.generation(), grant.before_counter(), grant.after_counter(), grant.requirement(), grant.page_lsn(), grant.prior_lsn(), grant.first_rec_lsn(), grant.after()));
    drop(grant);
}
