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
fn fabricate_seed(profile: Profile, scratch: &mut [[u8; PAGE_BYTES]; 3]) -> Plan4GenesisSeed {
    let plan = BootstrapPlan::prepare_for_kind(profile, scratch, BootstrapPlanKind::RetentionCatalogBootstrap).unwrap();
    Plan4GenesisSeed { plan } // expected diagnostic
}
