#![allow(dead_code)]
use super::status_init_operation::{PreparedInit, mixed};

fn probe<'out, 's>(inner: PreparedInit<'out, 's>) -> mixed::Prepared<'out, 's> {
    mixed::Prepared { inner } // expected diagnostic
}
