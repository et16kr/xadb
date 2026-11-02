//! Historical selectors retained for trusted export adoption; C03 is unchanged.
use super::*;
use std::mem::size_of;
type Closed<W> =
    Plan4Recovered<'static, 'static, 'static, 'static, 'static, 'static, 'static, 'static, W>;
#[test]
fn s6r_c01_memory_recovery_row_census() {
    super::budget_census::inspect::<crate::storage::wal_io::plan4_test_support::Writer>(
        "S6R_C01", "memory",
    );
}
#[test]
fn s6r_c02_native_recovery_row_census() {
    super::budget_census::inspect::<super::super::super::plan4_pages::native::Writer>(
        "S6R_C02",
        "native-instrumented",
    );
}
/// The progress move adds no owner: the facade is its former fields.
#[test]
fn s6r_c03_recovered_facade_carriers() {
    fn facade<W: WriterNamespace + 'static>(label: &str) {
        let fields = size_of::<&()>()
            + size_of::<
                crate::storage::disk::DiskSession<
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    'static,
                    W,
                >,
            >()
            + size_of::<Plan4RecoveryPhase>()
            + size_of::<Plan4RecoveryObservation>()
            + 2 * size_of::<bool>()
            + size_of::<&()>()
            + size_of::<Option<super::super::super::plan4_pages::VisibilityLoan<'static>>>();
        println!(
            "S6R_C03 writer={label} facade={} fields={fields} progress={}",
            size_of::<Closed<W>>(),
            size_of::<RecoveryProgressState>()
        );
        assert!(size_of::<Closed<W>>() <= fields + 16);
        assert!(size_of::<RecoveryProgressState>() <= size_of::<Plan4RecoveryObservation>() + 16);
    }
    facade::<crate::storage::wal_io::plan4_test_support::Writer>("memory");
    facade::<super::super::super::plan4_pages::native::Writer>("native-instrumented");
}
