//! New Catalog integration source-value admission; no adapter or effects.
use super::*;
use crate::storage::wal_io::plan4_test_support as port;

fn inspect<W: WriterNamespace>(kind: &str) {
    let identity = [
        "XADB_CATALOG_SOURCE_SHA256",
        "XADB_CATALOG_ELF_SHA256",
        "XADB_CATALOG_INPUT_SHA256",
    ]
    .map(|key| {
        let value = std::env::var(key).unwrap_or_else(|_| panic!("missing {key}"));
        assert_eq!(value.len(), 64);
        assert!(value.bytes().all(|b| b.is_ascii_hexdigit()));
        value
    });
    println!("CATALOG_SOURCE_CONTEXT\t{kind}\t{}", identity.join("\t"));
    let live = resources::source_census::<W>();
    let pages = super::super::resources::source_census::<W>();
    let recovery = super::super::super::plan4_recovery::source_census::<W>();
    let semantic = crate::storage::semantic_catalog::source_census();
    let visibility = visibility::source_census();
    println!(
        "CATALOG_SOURCE_PRODUCER\t{kind}\t{:?}",
        create_pair::catalog_live::catalog_live_source_bytes::<W>()
    );
    println!("CATALOG_SOURCE_LIVE\t{kind}\t{live:?}");
    println!("CATALOG_SOURCE_PAGES\t{kind}\t{pages:?}");
    println!(
        "CATALOG_SOURCE_FRESH_PREREQUISITES\t{kind}\tdirty={}\tdurability={}\tgenesis={}",
        dirty_frame::fits(),
        crate::storage::wal_io::durability_types_fit(),
        super::super::genesis::fits::<W>()
    );
    println!("CATALOG_SOURCE_RECOVERY\t{kind}\t{recovery:?}");
    println!(
        "CATALOG_SOURCE_BASE\t{kind}\t{:?}",
        crate::storage::plan4_recovery_base::catalog::source_census()
    );
    println!("CATALOG_SOURCE_SEMANTIC\t{kind}\t{semantic:?}");
    println!("CATALOG_SOURCE_VISIBILITY\t{kind}\t{visibility:?}");
    println!(
        "CATALOG_SOURCE_EFFECTS\t{kind}\tadapter=0\tacquire=0\tread=0\twrite=0\tsync=0\tspawn=0"
    );
    assert!(
        live.peak <= 65_536 && live.metadata <= 196_608 && live.whole <= 262_144,
        "live source cap"
    );
    assert!(
        recovery.peak <= 65_536 && recovery.metadata <= 196_608 && recovery.whole <= 262_144,
        "recovery source cap"
    );
    assert!(resources::fits::<W>());
    assert!(
        super::super::resources::fits::<W>(),
        "fresh page binding admission"
    );
    assert!(visibility::fits());
    assert_eq!(recovery.caller_maintenance, 12_582_912);
    assert_eq!(visibility.backing, 8192);
    assert_eq!(visibility.reader_private, 425_984);
    println!("CATALOG_SOURCE_PASS\t{kind}");
}

#[test]
fn catalog_source_memory() {
    inspect::<port::Writer>("memory");
}

#[test]
#[cfg(target_os = "linux")]
fn catalog_source_native() {
    inspect::<super::super::native::Writer>("native");
}
