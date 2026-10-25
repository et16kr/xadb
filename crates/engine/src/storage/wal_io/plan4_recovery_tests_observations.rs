//! Independent byte expectations for the new owner. No product decoder is
//! called to decide what the selected controls or retained ledger should mean.
use super::literals::{Oracle, SecondCycle, hex, put, seal};
use super::support::*;
use super::*;

pub(super) fn get(raw: &[u8], at: usize, width: usize) -> u64 {
    let mut out = [0; 8];
    out[..width].copy_from_slice(&raw[at..at + width]);
    u64::from_le_bytes(out)
}
fn exact(label: &str, actual: &[u8], expected: &[u8]) {
    assert!(
        actual == expected,
        "{label}: lengths {} / {}, first difference {:?}, SHA {} / {}",
        actual.len(),
        expected.len(),
        actual.iter().zip(expected).position(|(a, b)| a != b),
        hex(actual),
        hex(expected)
    );
}

pub(super) fn bank(bank: &[u8], oracle: &Oracle, lsn: u64, second: Option<&SecondCycle>) {
    const ROW: usize = 1664;
    assert_eq!(bank.len(), 65_536);
    let mut expected = vec![0; 65_536];
    let (tx, aging) = match (lsn, second) {
        (1 | 2, _) => (None, None),
        (4, _) => (
            Some(oracle.raw("tx-begin.descriptor")),
            Some(oracle.raw("aging-begin.descriptor")),
        ),
        (5, _) => (
            Some(oracle.raw("tx-rollback.descriptor")),
            Some(oracle.raw("aging-begin.descriptor")),
        ),
        (6, _) => (None, Some(oracle.raw("aging-abort.descriptor"))),
        (7, _) => (None, Some(oracle.raw("aging-finish.descriptor"))),
        (8 | 9, Some(s)) => (
            Some(s.tx[(lsn - 8) as usize].as_slice()),
            Some(s.aging[0].as_slice()),
        ),
        (10, Some(s)) => (None, Some(s.aging[1].as_slice())),
        (11, Some(s)) => (None, Some(s.aging[2].as_slice())),
        _ => panic!("unfrozen ledger input {lsn}"),
    };
    if let Some(tx) = tx {
        expected[..1536].copy_from_slice(tx);
        for (offset, value) in [
            (1536, 0x1_0000_0001),
            (1544, 9),
            (1552, 2),
            (1568, if lsn < 8 { 2 } else { 3 }),
        ] {
            put(&mut expected, offset, value, 8);
        }
        expected[1576..1624].copy_from_slice(&tx[24..72]);
        expected[1624] = 1;
    }
    if let Some(aging) = aging {
        expected[32 * ROW..32 * ROW + 1536].copy_from_slice(aging);
        expected[32 * ROW + 1624] = 1;
    }
    let (_, _, allocation) = oracle.allocation(if lsn == 1 {
        0
    } else if lsn == 2 {
        1
    } else {
        2
    });
    expected[33 * ROW..33 * ROW + 1536].copy_from_slice(&allocation);
    put(&mut expected, 33 * ROW + 1536, 1, 8);
    expected[33 * ROW + 1544] = if lsn == 1 {
        0
    } else if lsn == 2 {
        1
    } else {
        2
    };
    expected[33 * ROW + 1624] = 1;
    exact(
        &format!("retained ATT/Aging/allocation bank at L{lsn}"),
        bank,
        &expected,
    );
}

pub(super) fn remaining(oracle: &Oracle, stop: u64) -> (u64, u64) {
    let tx = match stop {
        4 => Some(oracle.raw("tx-begin.descriptor")),
        5 => Some(oracle.raw("tx-rollback.descriptor")),
        _ => None,
    };
    let mut total = (0, 0);
    if let Some(bytes) = tx {
        for row in 0..5 {
            total.0 += get(bytes, 656 + row * 128, 8);
            total.1 += get(bytes, 648 + row * 128, 8);
        }
    }
    if matches!(stop, 4..=6) {
        total.0 += 55_112;
        total.1 += 1;
    }
    if stop == 1 {
        let (_, _, bytes) = oracle.allocation(0);
        for row in 0..5 {
            total.0 += get(&bytes, 656 + row * 128, 8);
            total.1 += get(&bytes, 648 + row * 128, 8);
        }
    }
    total
}

fn prepared(magic: &[u8; 4], profile: &[u8]) -> Vec<u8> {
    let mut out = vec![0; 4096];
    out[..4].copy_from_slice(magic);
    out[4..8].copy_from_slice(&[1, 0, 1, 0]);
    put(&mut out, 8, 4096, 4);
    put(&mut out, 12, 256, 4);
    out[16..32].copy_from_slice(&profile[16..32]);
    put(&mut out, 32, 1, 8);
    out
}
fn profile_at(out: &mut [u8], at: usize, profile: &[u8]) {
    put(out, at, 1, 8);
    out[at + 8..at + 40].copy_from_slice(&content_digest(profile));
}

pub(super) fn controls(
    read: &impl Fn(&str) -> Vec<u8>,
    base: &[u8],
    epoch: u32,
    generation: u64,
    current_generation: u64,
    source_header: &[u8],
) {
    let profile = read("d3-profile");
    let lsn = get(base, 96, 8);
    let digest = content_digest(base);
    let mut header = prepared(b"XWE2", &profile);
    profile_at(&mut header, 48, &profile);
    for (at, value, width) in [
        (40, u64::from(epoch), 4),
        (88, lsn, 8),
        (96, 4096, 8),
        (104, 1_048_576, 4),
        (112, 268_435_456, 8),
        (120, generation, 8),
        (160, u64::from(epoch - 1), 4),
        (168, lsn, 8),
        (208, 1, 4),
    ] {
        put(&mut header, at, value, width);
    }
    header[128..160].copy_from_slice(&digest);
    header[176..208].copy_from_slice(&content_digest(source_header));
    seal(&mut header);
    let epoch_digest = content_digest(&header);
    exact(
        "new epoch",
        &read(&format!("d3-wal-{epoch:08x}"))[..4096],
        &header,
    );
    let mut writer = prepared(b"XWS2", &profile);
    profile_at(&mut writer, 56, &profile);
    for (at, value, width) in [
        (40, u64::from(epoch), 8),
        (48, u64::from(epoch), 4),
        (52, 1, 4),
        (128, generation, 8),
        (136, lsn, 8),
        (176, 4096, 8),
        (184, current_generation, 8),
    ] {
        put(&mut writer, at, value, width);
    }
    writer[96..128].copy_from_slice(&epoch_digest);
    writer[144..176].copy_from_slice(&digest);
    seal(&mut writer);
    exact("WRITER", &read("WRITER"), &writer);
    let mut current = prepared(b"XRC1", &profile);
    profile_at(&mut current, 48, &profile);
    for (at, value, width) in [
        (40, current_generation, 8),
        (88, u64::from(epoch), 4),
        (100, u64::from(epoch - 1), 4),
        (224, u64::from(epoch), 8),
        (232, generation, 8),
    ] {
        put(&mut current, at, value, width);
    }
    for (at, slot) in [
        (256, ((generation - 1) % 2) as u8),
        (512, (generation % 2) as u8),
    ] {
        current[at..at + 4].copy_from_slice(&[2, 1, slot, 1]);
        for (delta, value, width) in [
            (4, u64::from(epoch), 4),
            (8, generation, 8),
            (16, lsn, 8),
            (24, 1_048_576, 4),
            (96, 4096, 8),
            (184, u64::from(epoch), 8),
        ] {
            put(&mut current, at + delta, value, width);
        }
        current[at + 32..at + 64].copy_from_slice(&digest);
        current[at + 64..at + 96].copy_from_slice(&epoch_digest);
        current[at + 152..at + 184].copy_from_slice(&content_digest(&writer));
    }
    seal(&mut current);
    exact("CURRENT", &read("CURRENT"), &current);
    for name in BASE_NAMES {
        exact(name, &read(name), base);
    }
}

pub(super) fn first_base(original: &Images, stop: u64) -> Vec<u8> {
    let (label, digest) = match stop {
        1 => (
            "reserved",
            "51e8c85a58a668890868b0eec3e4f1e610761bd8e8871c8ec9c01e84ee0538a9",
        ),
        4 => (
            "active",
            "1d66e83db0dbcffa88cfdb9940ca9e942483bee654efc1e1b115767fb2806507",
        ),
        5 => (
            "rollback",
            "064d81eccf6f139973398b5cbe17f743293a2e89190db698150fcdb391b189c2",
        ),
        6 => (
            "ready",
            "caa762e7f9c2043c4f810aa04384631b7ad997c42a0d137b9203c4bdcd582438",
        ),
        7 => (
            "finish",
            "cb2ba8a9cf004201111e8353c97020602df2917a1f6dc77933bdac39910f4587",
        ),
        _ => panic!("unfrozen base {stop}"),
    };
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../tests/fixtures/dev3-plan4-recovery-base/vectors/p1/raw/{label}.xrb"
    ));
    let mut expected = std::fs::read(path).unwrap();
    assert_eq!(hex(&expected), digest);
    let oracle = Oracle::new();
    let mut pages = (0..9)
        .map(|n| oracle.genesis(n).to_vec())
        .collect::<Vec<_>>();
    pages[0] = oracle.allocation(if stop == 1 { 0 } else { 2 }).1;
    if stop != 1 {
        let phase = match stop {
            4 => "begin",
            5 => "rollback",
            6 => "abort",
            7 => "finish",
            _ => unreachable!(),
        };
        let queue = if stop == 5 { "begin" } else { phase };
        pages[1] = oracle.page(&format!("root-{queue}.page")).to_vec();
        pages[2] = oracle.page(&format!("jobs-{queue}.page")).to_vec();
        pages.push(oracle.page(&format!("status-{phase}.page")).to_vec());
    }
    let home = &original[HOME_NAME];
    let repairs = pages
        .iter()
        .enumerate()
        .filter(|(n, p)| p.as_slice() != &home[n * PAGE_BYTES..(n + 1) * PAGE_BYTES])
        .count() as u64;
    let selector = &original["DWB"];
    let dwb = &original[&format!("d3-dwb-{}", selector[81])];
    let batch = get(dwb, 40, 8) + repairs;
    put(&mut expected, 496, batch, 8);
    put(&mut expected, 504, batch * 3 + 1, 8);
    put(
        &mut expected,
        1152,
        original.values().map(|b| b.len() as u64).sum(),
        8,
    );
    expected[160..192].copy_from_slice(&content_digest(&original["d3-wal-00000001"]));
    expected[1192..1224].copy_from_slice(&content_digest(&original["d3-wal-00000001"][..4096]));
    seal(&mut expected);
    expected
}

pub(super) fn second_base(original: &Images) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/dev3-plan4-recovery-base/vectors/p1/raw/finish.xrb");
    let mut expected = std::fs::read(path).unwrap();
    assert_eq!(
        hex(&expected),
        "cb2ba8a9cf004201111e8353c97020602df2917a1f6dc77933bdac39910f4587"
    );
    let cycle = SecondCycle::new(&Oracle::new());
    let source = &original["d3-wal-00000002"];
    for (at, value, width) in [
        (88, 2, 8),
        (96, 11, 8),
        (104, 1, 8),
        (144, 2, 4),
        (152, source.len() as u64, 8),
        (192, 1, 8),
        (248, 11, 8),
        (288, 8, 8),
        (296, 7, 8),
        (1184, 2, 4),
    ] {
        put(&mut expected, at, value, width);
    }
    expected[112..144].copy_from_slice(&content_digest(&original["d3-rebase-0"]));
    expected[160..192].copy_from_slice(&content_digest(source));
    expected[1192..1224].copy_from_slice(&content_digest(&source[..4096]));
    let selector = &original["DWB"];
    let dwb = &original[&format!("d3-dwb-{}", selector[81])];
    put(&mut expected, 496, get(dwb, 40, 8), 8);
    put(&mut expected, 504, get(dwb, 32, 8), 8);
    put(
        &mut expected,
        1152,
        original.values().map(|b| b.len() as u64).sum(),
        8,
    );
    expected[4096 + 34 * 1536..4096 + 35 * 1536].copy_from_slice(&cycle.aging[2]);
    expected[81920..81920 + PAGE_BYTES].copy_from_slice(&cycle.roots[2]);
    expected[98304..98304 + PAGE_BYTES].copy_from_slice(&cycle.jobs[2]);
    let status = 180224 + 9 * 352;
    expected[status + 8..status + 56].copy_from_slice(&cycle.statuses[3][24..72]);
    expected[status + 64..status + 96].copy_from_slice(&content_digest(&cycle.statuses[3]));
    expected[status + 128..status + 352].copy_from_slice(&cycle.statuses[3][16128..16352]);
    for (n, image) in [
        (1, &cycle.roots[2]),
        (2, &cycle.jobs[2]),
        (9, &cycle.statuses[3]),
    ] {
        let at = 202752 + n * 96;
        expected[at + 8..at + 56].copy_from_slice(&image[24..72]);
        expected[at + 56..at + 88].copy_from_slice(&content_digest(image));
    }
    // The sole active owner remains Aging1; only the inventory cut changes.
    let inventory = 208896;
    put(&mut expected, inventory + 40, 2, 8);
    put(&mut expected, inventory + 48, 11, 8);
    let root_len = get(&expected, inventory + 12, 4) as usize;
    seal(&mut expected[inventory..inventory + root_len]);
    for (i, (at, len)) in [
        (4096, 55296),
        (65536, 16384),
        (81920, 98304),
        (180224, 22528),
        (202752, 6144),
        (208896, 16384),
    ]
    .into_iter()
    .enumerate()
    {
        let digest = content_digest(&expected[at..at + len]);
        expected[1280 + i * 64 + 32..1280 + (i + 1) * 64].copy_from_slice(&digest);
    }
    seal(&mut expected);
    expected
}
