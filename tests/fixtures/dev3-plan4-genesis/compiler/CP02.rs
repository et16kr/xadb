#![allow(dead_code, unused_imports)]
use super::{BootstrapPlan, BootstrapPlanKind, FreshStorageSession};
use crate::storage::{
    control::Profile,
    page::PAGE_BYTES,
    recovery_select::{Plan4GenesisObservation, Plan4GenesisSelection, RecoverySelection},
    wal_frame::MAX_FRAME_BYTES,
    wal_io::Plan4GenesisSeed,
};
use foundation::namespace::{DirectoryPort, WriterNamespace};
fn legal_selection<D: DirectoryPort>(directory: &D, maintenance: &mut [u8; 12_582_912]) {
    let owner = Plan4GenesisSelection::acquire(directory, None, maintenance).unwrap();
    { let observation = owner.observation(); std::hint::black_box(observation.namespace()); }
    std::hint::black_box(&mut *maintenance);
    drop(owner);
}
