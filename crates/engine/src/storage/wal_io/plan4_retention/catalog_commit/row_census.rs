//! Historical S3 selector retained across trusted export policy adoption.
//! Actual A inventory is checked in A; this selector now composes fixed budgets.
#[test]
fn s3_c01_row_commit_windows_and_actual_a_caller_fit_fixed_charges() {
    super::row_budget::inspect::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
    #[cfg(target_os = "linux")]
    super::row_budget::inspect::<super::super::super::super::native::Writer>("native-type");
}
