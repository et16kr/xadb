#![allow(dead_code)]
use crate::storage::wal_io::StatusLoadPermit; // expected diagnostic
type Raw<'a,'c,'s> = StatusLoadPermit<'a,'c,'s>;
