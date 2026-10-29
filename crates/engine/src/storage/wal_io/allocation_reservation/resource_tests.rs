//! Concrete stack observation complements the separately reviewed live-type ledger.
//! Caller workspaces and the in-memory port's file contents are outside component64KiB.
use super::super::super::test_support as port;
use super::*;
use crate::storage::{
    obligation::{DescriptorSpec, RemainingBudget},
    page_io::PageFileReader,
};
use foundation::codec::ContentHasher;
use std::{mem::size_of, thread};

fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}

fn resident_high_water_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

fn containing_mapping(address: usize) -> (usize, usize) {
    std::fs::read_to_string("/proc/self/maps")
        .unwrap()
        .lines()
        .find_map(|line| {
            let range = line.split_whitespace().next()?.split_once('-')?;
            let start = usize::from_str_radix(range.0, 16).ok()?;
            let end = usize::from_str_radix(range.1, 16).ok()?;
            (start <= address && address < end).then_some((start, end))
        })
        .expect("local address has a mapped stack")
}

#[test]
fn allocation_component_layout_inventory() {
    let owner = size_of::<AllocationSession<'_, '_, '_, '_, port::Writer>>();
    let file = size_of::<port::File>();
    let reader = size_of::<PageFileReader<'_, port::Writer>>();
    let plan = size_of::<operation::Plan<'_>>();
    assert!(owner <= 16384 && file <= 16384 && plan <= 4096);
    assert!(budget::fits::<port::Writer>());
    assert!(budget::fits::<port::SizedWriter<0, 12000>>());
    assert!(!budget::fits::<port::SizedWriter<16384, 0>>());
    assert!(!budget::fits::<port::SizedWriter<0, 16384>>());
    println!(
        "RESOURCE_LAYOUT owner={owner} file={file} reader={reader} plan={plan} entries64={} descriptor={} remaining5={} hasher={} allocation_state={}",
        size_of::<[AllocationEntry; 64]>(),
        size_of::<DescriptorSpec>(),
        size_of::<[RemainingBudget; 5]>(),
        size_of::<ContentHasher>(),
        size_of::<AllocationState<'_, '_>>()
    );
    println!(
        "RESOURCE_BUDGET common={} hash={} entries={} encode={} descriptors={} constructor={} plans={} constructor_total={} operation_total={}",
        budget::COMMON,
        budget::HASH,
        budget::ENTRIES,
        budget::ENCODE,
        budget::DESCRIPTORS,
        budget::CONSTRUCTOR,
        budget::PLANS,
        owner + reader + file + budget::CONSTRUCTOR,
        owner + budget::PLANS + budget::TEMPORARIES,
    );
}

#[test]
#[ignore = "requires read-only pthread stack observer"]
fn allocation_component_runs_with_observed_64k_stack() {
    assert_eq!(
        std::env::var("XADB_ALLOCATION_STACK_OBSERVER").as_deref(),
        Ok("1")
    );
    // The setup is deliberately outside the bounded thread: Fresh/DWB install
    // are prior capabilities with independent workspaces and evidence.
    let mut slot = port::slot();
    let mut flush = boxed::<{ 4 << 20 }>();
    let mut frame = boxed::<PAGE_BYTES>();
    let mut operation = boxed::<OPERATION_BYTES>();
    let mut scratch: Box<[[u8; PAGE_BYTES]; 3]> = vec![[0; PAGE_BYTES]; 3]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let (writer, state) = port::fresh();
    let installed =
        super::super::super::FreshStorageSession::create(writer, 1, &mut slot, &mut scratch)
            .unwrap()
            .install_doublewrite(&mut flush)
            .unwrap();
    // Keep the test thread closure small; the component still consumes the
    // original owner by value and owns its usual inline state.
    let installed = Box::new(installed);
    let rss_before = resident_high_water_kib();
    let stack_bytes = thread::scope(|scope| {
        thread::Builder::new()
            .name("allocation-64k".into())
            .stack_size(65536)
            .spawn_scoped(scope, move || {
                let marker = 0u8;
                let (start, end) = containing_mapping(&marker as *const u8 as usize);
                let thread_path = std::fs::read_link("/proc/thread-self").unwrap();
                let tid: u64 = thread_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .parse()
                    .unwrap();
                // A RW VMA can include an adjacent region or a MADV_GUARD page.
                // The external read-only observer checks pthread's actual
                // stack attributes and this marker; the VMA is diagnostic only.
                println!(
                    "RESOURCE_STACK_THREAD tid={tid} name=allocation-64k marker={} mapped_bytes={} compiled_source={}",
                    &marker as *const u8 as usize,
                    end - start,
                    option_env!("XADB_ALLOCATION_SOURCE_SHA256").unwrap_or("MISSING")
                );
                let mut owner = (*installed)
                    .into_allocation_session(&mut frame, &mut operation)
                    .unwrap();
                let reserved = owner.reserve_pages(63).unwrap();
                assert_eq!(reserved.reserved, 63);
                assert!(owner.reserved_page(62).unwrap().is_some());
                let cancelled = owner.cancel_reserved().unwrap();
                assert_eq!(cancelled.reserved, 0);
                assert_eq!(cancelled.page_lsn, PageLsn(2));
                assert_eq!(cancelled.rec_lsn, Some(PageLsn(1)));
                assert_eq!(owner.observe().unwrap(), cancelled);
                end - start
            })
            .unwrap()
            .join()
            .unwrap()
    });
    let rss_after = resident_high_water_kib();
    let state = state.lock().unwrap();
    assert_eq!(state.handles, 0);
    assert_eq!(state.writers, 0);
    println!(
        "RESOURCE_PROCESS mapped_bytes={stack_bytes} process_hwm_before_kib={rss_before} process_hwm_after_kib={rss_after}"
    );
}
