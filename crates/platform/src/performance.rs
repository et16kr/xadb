//! Bounded Linux observations for a single-thread measurement driver.
//!
//! Missing, malformed or unavailable observations stay `None`. These observers
//! do not change scheduling or machine configuration and make no RSS limit claim.
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

fn parse_cpu(value: &str) -> Option<u64> {
    let fields = value.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 {
        return None;
    }
    let cpu = fields[0].parse().ok()?;
    fields[1].parse::<u64>().ok()?;
    fields[2].parse::<u64>().ok()?;
    Some(cpu)
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

/// Main-thread scheduler execution nanoseconds from Linux `/proc/self/schedstat`.
/// This is appropriate only for the declared single-thread worker. Callers use a
/// checked before/after delta and must preserve missing or decreasing samples.
pub fn thread_cpu_ns() -> Option<u64> {
    parse_cpu(&bounded("/proc/self/schedstat", 256)?)
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
    fn cpu_vectors_reject_partial_invalid_and_extra_fields() {
        assert_eq!(parse_cpu("123 456 7\n"), Some(123));
        assert_eq!(parse_cpu("0 0 0"), Some(0));
        for value in ["", "123", "1 2", "1 x 3", "-1 2 3", "1 2 3 4"] {
            assert_eq!(parse_cpu(value), None);
        }
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
