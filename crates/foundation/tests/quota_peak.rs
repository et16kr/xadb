use foundation::runtime::Quota;
use std::sync::{Arc, Barrier};

#[test]
fn quota_peak_retains_released_reservations_and_excludes_rejections() {
    let quota = Quota::new(100);
    let first = quota.reserve(37).unwrap();
    let second = quota.reserve(12).unwrap();
    assert!(quota.reserve(52).is_err());
    drop(first);
    drop(second);
    assert_eq!(quota.used(), 0);
    assert_eq!(quota.clone().peak(), 49);
    drop(quota.reserve(42).unwrap());
    assert_eq!(quota.peak(), 49);
    drop(quota.reserve(100).unwrap());
    assert_eq!(quota.peak(), 100);
}

#[test]
fn quota_peak_keeps_the_concurrent_reservation_high_water_after_join() {
    let quota = Quota::new(64);
    let barrier = Arc::new(Barrier::new(65));
    let threads = (0..64)
        .map(|_| {
            let quota = quota.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let lease = quota.reserve(1).unwrap();
                barrier.wait();
                drop(lease);
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(quota.used(), 0);
    assert_eq!(quota.peak(), 64);
}
