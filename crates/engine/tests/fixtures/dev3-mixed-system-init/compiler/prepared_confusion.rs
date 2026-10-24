#![allow(dead_code)]
use super::status_init_operation::{PreparedInit, mixed};

fn probe<'out, 's>(prepared: mixed::Prepared<'out, 's>) -> PreparedInit<'out, 's> {
    prepared // expected diagnostic
}
