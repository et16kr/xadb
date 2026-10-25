//! Current source accounting for the changed shared core/WAL fields. No old
//! completed functional case is called or replayed by this static selection.
use super::*;

#[test]
fn commit_common_original_owner_source_admission() {
    let common = crate::transaction::concurrency::commit_common_source_bytes();
    println!("COMMIT_COMMON_CORE source_bytes={common} semantic_consumer=NOT_RUN");
    assert!(common <= 65_536);
    super::static_source::<port::Writer>("commit-common-memory");
    #[cfg(target_os = "linux")]
    super::static_source::<super::super::super::plan4_pages::native::Writer>(
        "commit-common-native",
    );
}
