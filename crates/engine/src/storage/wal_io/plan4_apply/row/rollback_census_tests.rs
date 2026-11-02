//! Historical S4 selectors retained across trusted export policy adoption.
//! The body now composes fixed budgets and contains no A implementation reference.
#[test]
fn s4_c01_memory_rollback_census() {
    super::budget_census::inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
}
#[test]
fn s4_c01_native_rollback_census() {
    super::budget_census::inspect::<super::super::super::super::native::Writer>("native");
}
