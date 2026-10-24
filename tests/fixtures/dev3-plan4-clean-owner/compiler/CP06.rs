#![allow(dead_code, unused_imports)]
use super::plan4_pages::Plan4Target;
use crate::storage::dirty_frame::TargetRequest;
fn fabricate(request: TargetRequest) -> Plan4Target {
    Plan4Target { request } // expected diagnostic
}
