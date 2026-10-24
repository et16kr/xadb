//! New live-retention callers only. Static cases construct no owner or port.
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
                    assert!(owned.contains(id), "unknown or foreign ID {id}");
                    assert!(result.insert(id.to_owned()), "duplicate ID {id}");
                }
                result
            })
        };
        let requested = parse("XADB_PLAN4_LIVE_INCLUDE");
        let explicit = requested.is_some();
        let include = requested.unwrap_or_else(|| owned.clone());
        let exclude = parse("XADB_PLAN4_LIVE_EXCLUDE").unwrap_or_default();
        assert!(!explicit || include.is_disjoint(&exclude));
        let selected: BTreeSet<_> = include.difference(&exclude).cloned().collect();
        assert!(!selected.is_empty(), "empty live-retention selection");
        let identity = [
            "XADB_PLAN4_LIVE_SOURCE_SHA256",
            "XADB_PLAN4_LIVE_ELF_SHA256",
            "XADB_PLAN4_LIVE_INPUT_SHA256",
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
    fn case(&mut self, id: &str, operation: impl FnOnce()) {
        assert!(self.owned.contains(id) && self.seen.insert(id.to_owned()));
        if !self.selected.contains(id) {
            println!("P4_LIVE_SKIP\t{id}");
            return;
        }
        println!("P4_LIVE_CONTEXT\t{id}\t{}", self.identity);
        println!("P4_LIVE_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(()) => println!("P4_LIVE_PASS\t{id}"),
            Err(error) => {
                println!("P4_LIVE_FAIL\t{id}");
                resume_unwind(error);
            }
        }
    }
    fn finish(self) {
        assert_eq!(self.seen, self.owned);
    }
}

fn static_source<W: WriterNamespace>(
    label: &str,
    old_writer: usize,
    old_file: usize,
    old_owner: usize,
) {
    let c = source_census::<W>();
    let old = super::super::resources::source_census::<W>();
    let core = dirty_frame::plan4_core_census();
    // These are observations of actual types, not copies of a fabricated old
    // authority. Only the recorded G5 values below have a historical baseline.
    let baseline = [
        ("Writer", size_of::<W>(), old_writer),
        ("File", size_of::<W::File>(), old_file),
        ("MutableStorageOwner", c.original_owner, old_owner),
        ("Plan4Pages", old.bound, 17_240),
        ("SharedPool", core.pool, 17_216),
        ("LiveDirectory", core.directory, 5_160),
        ("Mode", core.mode, 1),
    ];
    for (name, actual, prior) in baseline {
        println!(
            "P4_LIVE_LAYOUT\t{label}\t{name}\tactual={actual}\tbaseline={prior}\tdelta={}",
            actual as i128 - prior as i128
        );
    }
    println!("P4_LIVE_CENSUS\t{label}\t{c:?}");
    println!(
        "P4_LIVE_COMPONENT\t{label}\tretention_core={}\tselected_home={}\tflush={}\tsemantic={}\tpopulation={}\told_common_delta={}",
        dirty_frame::retention_core_census(),
        crate::storage::page_io::retention_home_source_bytes::<W>(),
        super::super::super::flush_io::retention_source_bytes::<W>(),
        crate::storage::semantic_retention::source_bytes(),
        population::source_bytes(),
        old.old_common_delta,
    );
    // All observations, including the private scalar layouts supplied by the
    // owner modules, precede every acceptance assertion.
    let mut flush_layout = super::super::super::flush_io::test_support::layout();
    flush_layout.extend(super::super::super::flush_io::test_support::status_layout());
    for (name, actual, cap) in &flush_layout {
        if matches!(
            *name,
            "FlushIo" | "FlushIoError" | "PolicyRef" | "TargetClass"
        ) {
            println!(
                "P4_LIVE_LAYOUT\t{label}\t{name}\tactual={actual}\tcap={cap}\tbaseline=UNKNOWN\tdelta=UNKNOWN"
            );
        }
    }
    for (name, actual) in dirty_frame::init_test_support::init_layout() {
        if matches!(name, "claim_record" | "claim_slot") {
            println!(
                "P4_LIVE_LAYOUT\t{label}\t{name}\tactual={actual}\tbaseline=UNKNOWN\tdelta=UNKNOWN"
            );
        }
    }
    println!(
        "P4_LIVE_EFFECTS\t{label}\tport_construct=0\tacquire=0\tread=0\twrite=0\tsync=0\tspawn=0"
    );
    assert!(resources::fits::<W>());
    assert!(c.original_owner <= 32_768 && size_of::<W::File>() <= 16_384);
    assert!(c.peak <= 65_536 && c.metadata <= 196_608 && c.whole <= 262_144);
    assert_eq!(c.metadata, c.peak + 36 * control::SLOT_BYTES);
    assert_eq!(c.whole, c.metadata + 65_536);
    assert_eq!(
        (
            c.caller_control,
            c.caller_writer_replay,
            c.caller_operation,
            c.caller_dwb,
            c.caller_dwb_replay
        ),
        (196_608, 1_572_864, 114_688, 4_194_304, 1_572_864),
    );
    assert_eq!(old.old_common_delta, 0);
    for (name, actual, cap) in flush_layout {
        if matches!(
            name,
            "FlushIo" | "FlushIoError" | "PolicyRef" | "TargetClass"
        ) {
            assert!(actual <= cap, "shared carrier {name}");
        }
    }
    for (name, actual, prior) in baseline {
        assert_eq!(actual, prior, "changed common carrier {name}");
    }
}

#[test]
fn retention_source_memory() {
    let mut h = Harness::new(&["P4-LIVE-ST01"]);
    h.case("P4-LIVE-ST01", || {
        static_source::<port::Writer>("actual-memory", 8, 32, 848)
    });
    h.finish();
}

#[cfg(target_os = "linux")]
#[test]
fn retention_source_native() {
    let mut h = Harness::new(&["P4-LIVE-ST02"]);
    h.case("P4-LIVE-ST02", || {
        static_source::<super::super::native::Writer>("actual-native", 16, 160, 984)
    });
    h.finish();
}

// This seam runs only after capture_flush has released every target pin/latch.
// Its address key prevents an unrelated pool from consuming a scheduled pause.
struct AfterCapturePause {
    pool: usize,
    arrived: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<crate::storage::dirty_frame::test_support::Resume>,
}
static AFTER_CAPTURE: std::sync::Mutex<Option<AfterCapturePause>> = std::sync::Mutex::new(None);
fn install_after_capture(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    arrived: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<crate::storage::dirty_frame::test_support::Resume>,
) {
    let mut slot = AFTER_CAPTURE.lock().unwrap();
    assert!(slot.is_none());
    *slot = Some(AfterCapturePause {
        pool: std::ptr::from_ref(pool) as usize,
        arrived,
        resume,
    });
}
pub(super) fn after_capture(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>) {
    let pause = {
        let mut slot = AFTER_CAPTURE.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|p| p.pool == std::ptr::from_ref(pool) as usize)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        pause
            .arrived
            .try_send(())
            .expect("capture released arrival");
        match pause
            .resume
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("capture released resume")
        {
            crate::storage::dirty_frame::test_support::Resume::Continue => {}
            crate::storage::dirty_frame::test_support::Resume::Panic => {
                panic!("requested released capture unwind")
            }
        }
    }
}

static AFTER_INSTALL: std::sync::Mutex<Option<AfterCapturePause>> = std::sync::Mutex::new(None);
fn install_after_install(
    pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>,
    arrived: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<crate::storage::dirty_frame::test_support::Resume>,
) {
    let mut slot = AFTER_INSTALL.lock().unwrap();
    assert!(slot.is_none());
    *slot = Some(AfterCapturePause {
        pool: std::ptr::from_ref(pool) as usize,
        arrived,
        resume,
    });
}
pub(super) fn after_install(pool: &crate::storage::dirty_frame::SharedPool<'_, '_, '_, '_>) {
    let pause = {
        let mut slot = AFTER_INSTALL.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|p| p.pool == std::ptr::from_ref(pool) as usize)
        {
            slot.take()
        } else {
            None
        }
    };
    if let Some(pause) = pause {
        pause
            .arrived
            .try_send(())
            .expect("install released arrival");
        match pause
            .resume
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("install released resume")
        {
            crate::storage::dirty_frame::test_support::Resume::Continue => {}
            crate::storage::dirty_frame::test_support::Resume::Panic => {
                panic!("requested released capture unwind")
            }
        }
    }
}

#[path = "plan4_retention_lifecycle_tests.rs"]
mod lifecycle;
