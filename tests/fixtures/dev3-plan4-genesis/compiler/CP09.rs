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
fn assert_clone<T: Clone>() {}
fn duplicate_lease<W: WriterNamespace>() {
    assert_clone::<Plan4GenesisSelection<W>>(); // expected diagnostic
}
