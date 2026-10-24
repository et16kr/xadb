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
fn private_inner<W: WriterNamespace>(owner: Plan4GenesisSelection<W>) {
    let _old = owner.inner; // expected diagnostic
}
