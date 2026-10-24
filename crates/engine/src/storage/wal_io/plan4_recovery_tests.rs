//! New effect-owning recovery consumers. This module never invokes completed
//! genesis, live, pure-codec or wrapper test functions. Static cases do no I/O.
use super::*;
use crate::storage::wal_io::plan4_test_support as port;
use std::{
    collections::BTreeSet,
    mem::size_of,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

struct Harness {
    owned: BTreeSet<String>,
    selected: BTreeSet<String>,
    seen: BTreeSet<String>,
    identity: String,
}
impl Harness {
    fn new(ids: &[&str]) -> Self {
        let owned: BTreeSet<_> = ids.iter().map(|id| (*id).to_owned()).collect();
        assert!(!owned.is_empty() && owned.len() == ids.len());
        let parse = |name: &str| {
            std::env::var(name).ok().map(|text| {
                assert!(!text.is_empty() && text.len() <= 16_384);
                let mut result = BTreeSet::new();
                for id in text.split(',') {
                    assert!(owned.contains(id), "unknown or foreign recovery ID {id}");
                    assert!(result.insert(id.to_owned()), "duplicate recovery ID {id}");
                }
                result
            })
        };
        let requested = parse("XADB_PLAN4_RECOVERY_INCLUDE");
        let explicit = requested.is_some();
        let include = requested.unwrap_or_else(|| owned.clone());
        let exclude = parse("XADB_PLAN4_RECOVERY_EXCLUDE").unwrap_or_default();
        assert!(!explicit || include.is_disjoint(&exclude));
        let selected: BTreeSet<_> = include.difference(&exclude).cloned().collect();
        assert!(!selected.is_empty(), "empty recovery selection");
        let identity = [
            "XADB_PLAN4_RECOVERY_SOURCE_SHA256",
            "XADB_PLAN4_RECOVERY_ELF_SHA256",
            "XADB_PLAN4_RECOVERY_INPUT_SHA256",
        ]
        .map(|name| {
            let value = std::env::var(name).unwrap_or_else(|_| panic!("missing {name}"));
            assert!(value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()));
            value
        })
        .join("\t");
        Self {
            owned,
            selected,
            seen: BTreeSet::new(),
            identity,
        }
    }
    fn case(&mut self, id: &str, run: impl FnOnce()) {
        assert!(self.owned.contains(id) && self.seen.insert(id.to_owned()));
        if !self.selected.contains(id) {
            println!("P4_RECOVERY_SKIP\t{id}");
            return;
        }
        println!("P4_RECOVERY_CONTEXT\t{id}\t{}", self.identity);
        println!("P4_RECOVERY_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(run)) {
            Ok(()) => println!("P4_RECOVERY_PASS\t{id}"),
            Err(error) => {
                println!("P4_RECOVERY_FAIL\t{id}");
                resume_unwind(error);
            }
        }
    }
    fn finish(self) {
        assert_eq!(self.owned, self.seen);
    }
}

fn static_source<W: WriterNamespace>(label: &str) {
    type Runtime<'a, W> = Plan4RecoveryRuntime<'a, 'a, 'a, W>;
    type Closed<'a, W> = Plan4Recovered<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    type Live<'a, W> = super::super::plan4_pages::Plan4Retention<'a, 'a, 'a, 'a, 'a, 'a, 'a, 'a, W>;
    let c = source_census::<W>();
    let live = super::super::plan4_pages::retention_recovery_source_census::<W>();
    let bind = super::super::plan4_pages::plan4_recovery_clean_source_census::<W>();
    println!("P4_RECOVERY_CENSUS\t{label}\t{c:?}");
    println!("P4_RECOVERY_LIVE_CENSUS\t{label}\t{live:?}");
    println!("P4_RECOVERY_BIND_CENSUS\t{label}\t{bind:?}");
    let codec = base::recovery_source_phases();
    println!(
        "P4_RECOVERY_CODEC_PHASES\t{label}\tdeep={}\tother={}",
        codec.deep, codec.other
    );
    for (name, bytes) in [
        ("Writer", size_of::<W>()),
        ("File", size_of::<W::File>()),
        ("Runtime", size_of::<Runtime<'_, W>>()),
        ("ClosedFacade", size_of::<Closed<'_, W>>()),
        ("LiveOwner", size_of::<Live<'_, W>>()),
        ("Phase", size_of::<Plan4RecoveryPhase>()),
        ("Observation", size_of::<Plan4RecoveryObservation>()),
        ("Binding", size_of::<RecoveryRetentionBinding<'_, '_>>()),
        ("VerifiedBase", size_of::<VerifiedPlan4Base<'_>>()),
        ("SelectedEpoch", size_of::<SelectedNewEpoch>()),
        (
            "RuntimeResult",
            size_of::<std::result::Result<Runtime<'_, W>, Plan4RecoveryFailure<'_, '_, '_, W>>>(),
        ),
        (
            "LiveHandoffResult",
            size_of::<std::result::Result<Live<'_, W>, Closed<'_, W>>>(),
        ),
    ] {
        println!("P4_RECOVERY_TYPE\t{label}\t{name}\t{bytes}");
    }
    println!(
        "P4_RECOVERY_COMPONENT\t{label}\tbase={}\tpool={}\thome={}\tflush={}",
        base::source_bytes(),
        crate::storage::dirty_frame::retention_core_census(),
        crate::storage::page_io::retention_home_source_bytes::<W>(),
        super::super::flush_io::retention_source_bytes::<W>()
    );
    println!(
        "P4_RECOVERY_EFFECTS\t{label}\tadapter_construct=0\tacquire=0\tread=0\twrite=0\tsync=0\tspawn=0"
    );
    // All scalar values survive a first cap failure. These assertions are
    // admission source bounds, never measured compiler stack or live F05.
    assert!(resources::fits::<W>());
    assert!(c.peak <= 65_536 && c.metadata <= 196_608 && c.whole <= 262_144);
    assert!(live.peak <= 65_536 && live.metadata <= 196_608 && live.whole <= 262_144);
    assert_eq!(
        (c.caller_maintenance, c.caller_wal, c.caller_dwb),
        (12_582_912, 1_048_576, 4_194_304)
    );
    assert!(size_of::<W::File>() <= 16_384);
}

#[test]
fn recovery_source_memory() {
    let mut h = Harness::new(&["P4-REC-ST01"]);
    h.case("P4-REC-ST01", || {
        static_source::<port::Writer>("actual-memory")
    });
    h.finish();
}
#[cfg(target_os = "linux")]
#[test]
fn recovery_source_native() {
    let mut h = Harness::new(&["P4-REC-ST02"]);
    h.case("P4-REC-ST02", || {
        static_source::<super::super::plan4_pages::native::Writer>("actual-native")
    });
    h.finish();
}

#[path = "plan4_recovery_tests_cases.rs"]
mod cases;
#[path = "plan4_recovery_tests_literals.rs"]
mod literals;
#[path = "plan4_recovery_tests_support.rs"]
mod support;
