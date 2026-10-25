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
fn move_while_borrowed<W: WriterNamespace>(owner: Plan4GenesisSelection<W>) {
    let observation = owner.observation();
    drop(owner); // expected diagnostic
    std::hint::black_box(observation.namespace());
}
