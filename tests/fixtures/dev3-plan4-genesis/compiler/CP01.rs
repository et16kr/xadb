#![allow(dead_code, unused_imports)]
use super::{BootstrapPlan, BootstrapPlanKind, FreshStorageSession};
use crate::storage::{
    control::Profile,
    page::PAGE_BYTES,
    recovery_select::{Plan4GenesisObservation, Plan4GenesisSelection, RecoverySelection},
    wal_frame::MAX_FRAME_BYTES,
    wal_io::Plan4GenesisSeed,
};
use xadb_foundation::namespace::{DirectoryPort, WriterNamespace};
fn legal_fresh<W: WriterNamespace>(writer: W, slot: &mut [u8; MAX_FRAME_BYTES], maintenance: &mut [u8; 12_582_912], flush: &mut [u8; 4_194_304]) {
    let fresh = FreshStorageSession::create_retention_catalog(writer, 1, slot, maintenance).unwrap();
    std::hint::black_box(&mut *maintenance);
    let owner = fresh.install_doublewrite(flush).unwrap();
    drop(owner);
}
fn legal_seed_scaffold(profile: Profile, scratch: &mut [[u8; PAGE_BYTES]; 3]) {
    let plan = BootstrapPlan::prepare_for_kind(profile, scratch, BootstrapPlanKind::RetentionCatalogBootstrap).unwrap();
    std::hint::black_box(plan);
    let seed: Option<Plan4GenesisSeed> = None;
    std::hint::black_box(seed);
}
