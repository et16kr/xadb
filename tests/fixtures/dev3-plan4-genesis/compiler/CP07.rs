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
fn escape<'a, D: DirectoryPort>(directory: &D, maintenance: &mut [u8; 12_582_912]) -> Plan4GenesisObservation<'a> where D::Writer: 'a {
    let owner = Plan4GenesisSelection::acquire(directory, None, maintenance).unwrap();
    owner.observation() // expected diagnostic
}
