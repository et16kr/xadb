//! Bounded Linux observations for a single-thread measurement driver.
//!
//! Missing, malformed or unavailable observations stay `None`. These observers
//! do not change scheduling or machine configuration and make no RSS limit claim.
use nix::time::{ClockId, clock_gettime};
use std::{fs::File, io::Read};

fn bounded(path: &str, limit: u64) -> Option<String> {
    let mut value = String::new();
    File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_string(&mut value)
        .ok()?;
    (value.len() as u64 <= limit).then_some(value)
}

fn cpu_nanos(seconds: i64, nanoseconds: i64) -> Option<u64> {
    if !(0..1_000_000_000).contains(&nanoseconds) {
        return None;
    }
    u64::try_from(seconds)
        .ok()?
        .checked_mul(1_000_000_000)?
        .checked_add(nanoseconds as u64)
}

fn parse_peak(value: &str) -> Option<u64> {
    let mut result = None;
    for line in value.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("VmHWM:") {
            continue;
        }
        if result.is_some() {
            return None;
        }
        let kib = fields.next()?.parse::<u64>().ok()?;
        if fields.next() != Some("kB") || fields.next().is_some() {
            return None;
        }
        result = Some(kib.checked_mul(1024)?);
    }
    result
}

/// Calling-thread CPU nanoseconds from Linux `CLOCK_THREAD_CPUTIME_ID`.
/// Unlike the cached `/proc/self/schedstat` scheduler total, this samples CPU
/// time at the call boundary and identifies the calling thread. Callers use a
/// checked before/after delta and preserve unsupported or decreasing samples.
pub fn thread_cpu_ns() -> Option<u64> {
    let time = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).ok()?;
    cpu_nanos(time.tv_sec(), time.tv_nsec())
}

/// Process peak resident bytes up to this sampling point, including setup and
/// warmup. It is not the peak for only the measured interval.
pub fn process_peak_rss_bytes() -> Option<u64> {
    parse_peak(&bounded("/proc/self/status", 65536)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_vectors_reject_negative_invalid_and_overflowing_times() {
        assert_eq!(cpu_nanos(0, 0), Some(0));
        assert_eq!(cpu_nanos(2, 123), Some(2_000_000_123));
        assert_eq!(cpu_nanos(18_446_744_073, 709_551_615), Some(u64::MAX));
        assert_eq!(cpu_nanos(18_446_744_073, 709_551_616), None);
        assert_eq!(cpu_nanos(i64::MAX, 0), None);
        assert_eq!(cpu_nanos(-1, 0), None);
        assert_eq!(cpu_nanos(0, -1), None);
        assert_eq!(cpu_nanos(0, 1_000_000_000), None);
    }

    #[test]
    fn cpu_observation_tracks_the_calling_thread_while_its_parent_waits() {
        std::thread::spawn(|| {
            let reference = || {
                let time = clock_gettime(ClockId::CLOCK_THREAD_CPUTIME_ID).unwrap();
                cpu_nanos(time.tv_sec(), time.tv_nsec()).unwrap()
            };
            let before = thread_cpu_ns().unwrap();
            let started = reference();
            while reference() - started < 20_000_000 {
                std::hint::black_box((0_u64..128).sum::<u64>());
            }
            let after = thread_cpu_ns().unwrap();
            let ended = reference();
            // Independent bracketing clock samples bound the entire interval.
            // A cached main-thread total misses this child thread's CPU work.
            assert!(after - before >= 20_000_000);
            assert!(before <= started && after <= ended);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn rss_vectors_preserve_units_missing_duplicates_and_overflow() {
        assert_eq!(parse_peak("Name:\ttest\nVmHWM:\t123 kB\n"), Some(125952));
        for value in [
            "Name: test",
            "VmHWM: 12 MB",
            "VmHWM: 12 kB extra",
            "VmHWM: 1 kB\nVmHWM: 2 kB",
            "VmHWM: 18446744073709551615 kB",
        ] {
            assert_eq!(parse_peak(value), None);
        }
    }

    #[test]
    fn linux_observations_are_optional_and_never_fabricated() {
        let before = thread_cpu_ns();
        let after = thread_cpu_ns();
        if let (Some(before), Some(after)) = (before, after) {
            assert!(after >= before);
        }
        if let Some(peak) = process_peak_rss_bytes() {
            assert!(peak > 0);
        }
        assert!(bounded("/proc/self/status", 1).is_none());
    }
}
