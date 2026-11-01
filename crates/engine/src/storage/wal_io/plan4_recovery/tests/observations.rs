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
    if let Some(at) = actual.iter().zip(expected).position(|(a, b)| a != b) {
        eprintln!(
            "{label} first differing bytes @{at}: {:?} / {:?}",
            &actual[at..(at + 32).min(actual.len())],
            &expected[at..(at + 32).min(expected.len())]
        );
    }
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
        (104, 2_097_152, 4),
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
            (24, 2_097_152, 4),
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

// Base4 is assembled from independently serialized input images/descriptors.
// Only source identity, physical size and DWB counters come from the input DB.
fn base(original: &Images, stop: u64, second: bool) -> Vec<u8> {
    let oracle = Oracle::new();
    let cycle = second.then(|| SecondCycle::new(&oracle));
    let source_epoch = if second { 2 } else { 1 };
    let generation = source_epoch as u64;
    let source = &original[&format!("d3-wal-{source_epoch:08x}")];
    let profile = oracle.profile();
    let mut pages = (0..9)
        .map(|n| oracle.genesis(n).to_vec())
        .collect::<Vec<_>>();
    pages[0] = oracle.allocation(if stop == 1 { 0 } else { 2 }).1;
    let mut descriptors: Vec<(usize, &[u8])> = Vec::new();
    if stop != 1 {
        let phase = match stop {
            4 => "begin",
            5 => "rollback",
            6 => "abort",
            7 | 11 => "finish",
            _ => panic!("unfrozen base"),
        };
        let queue = if stop == 5 { "begin" } else { phase };
        if let Some(s) = &cycle {
            pages[1] = s.roots[2].clone();
            pages[2] = s.jobs[2].clone();
            pages.push(s.statuses[3].clone());
            descriptors.push((34, &s.aging[2]));
        } else {
            pages[1] = oracle.page(&format!("root-{queue}.page")).to_vec();
            pages[2] = oracle.page(&format!("jobs-{queue}.page")).to_vec();
            pages.push(oracle.page(&format!("status-{phase}.page")).to_vec());
            if matches!(stop, 4 | 5) {
                descriptors.push((
                    0,
                    oracle.raw(if stop == 4 {
                        "tx-begin.descriptor"
                    } else {
                        "tx-rollback.descriptor"
                    }),
                ));
            }
            descriptors.push((
                34,
                oracle.raw(&format!(
                    "aging-{}.descriptor",
                    if stop == 5 { "begin" } else { phase }
                )),
            ));
        }
    }
    let (_, _, allocation) = oracle.allocation(if stop == 1 { 0 } else { 2 });
    descriptors.push((32, &allocation));
    descriptors.sort_by_key(|(slot, _)| *slot);
    let root_at = get(&pages[1], 128, 2) as usize;
    let root = &pages[1][root_at..root_at + 512];
    let mut expected = vec![0; 2_097_152];
    expected[..4].copy_from_slice(b"XRB1");
    expected[16..32].copy_from_slice(&profile[16..32]);
    expected[48..80].copy_from_slice(&content_digest(profile));
    for (at, value, width) in [
        (4, 4, 2),
        (6, 1, 2),
        (8, 2_097_152, 4),
        (12, 4096, 4),
        (32, 1, 8),
        (40, 1, 8),
        (80, 4, 8),
        (88, generation, 8),
        (96, stop, 8),
        (104, generation - 1, 8),
        (144, source_epoch as u64, 4),
        (148, 1, 4),
        (152, source.len() as u64, 8),
        (192, u64::from(stop != 1), 8),
        (208, 1, 8),
        (224, u64::from(stop != 1), 8),
        (240, 5, 8),
        (248, stop, 8),
        (256, u64::from(matches!(stop, 4 | 5)), 4),
        (260, 1, 4),
        (264, if stop == 1 { 0 } else { 512 }, 8),
        (272, get(root, 104, 8), 8),
        (280, get(root, 112, 8), 8),
        (288, get(root, 64, 8), 8),
        (296, get(root, 64, 8) - 1, 8),
        (304, if stop == 1 { 0 } else { 2 }, 1),
        (320, 1, 1),
        (328, 1, 8),
        (336, 4096, 4),
        (448, 1, 8),
        (1152, original.values().map(|b| b.len() as u64).sum(), 8),
        (1184, source_epoch as u64, 4),
    ] {
        put(&mut expected, at, value, width);
    }
    if second {
        expected[112..144].copy_from_slice(&content_digest(&original["d3-rebase-0"]));
    }
    expected[160..192].copy_from_slice(&content_digest(source));
    expected[352..384].copy_from_slice(&content_digest(profile));
    expected[384..448].copy_from_slice(&original["FILEMAP"][80..144]);
    expected[464..496].copy_from_slice(&content_digest(&original["d3-bootstrap"]));
    expected[1192..1224].copy_from_slice(&content_digest(&source[..4096]));
    let home = &original[HOME_NAME];
    let repairs = pages
        .iter()
        .enumerate()
        .filter(|(n, p)| p.as_slice() != &home[n * PAGE_BYTES..(n + 1) * PAGE_BYTES])
        .count() as u64;
    let dwb = &original[&format!("d3-dwb-{}", original["DWB"][81])];
    let batch = get(dwb, 40, 8) + repairs;
    put(&mut expected, 496, batch, 8);
    put(&mut expected, 504, batch * 3 + 1, 8);
    let mut owners = Vec::new();
    for (slot, descriptor) in &descriptors {
        let at = 4096 + slot * 1536;
        expected[at..at + 1536].copy_from_slice(descriptor);
        if *slot == 32 && stop != 1 {
            continue;
        }
        owners.extend_from_slice(&descriptor[16..72]);
        for row in 0..5 {
            let from = 640 + row * 128;
            let to = 512 + row * 128;
            expected[to] = (row + 1) as u8;
            for offset in [8, 16, 24, 32, 40, 48, 56, 112] {
                let value = get(&expected, to + offset, 8) + get(descriptor, from + offset, 8);
                put(&mut expected, to + offset, value, 8);
            }
            for offset in [64, 72, 80, 88, 96, 104] {
                let value = get(&expected, to + offset, 8).max(get(descriptor, from + offset, 8));
                put(&mut expected, to + offset, value, 8);
            }
        }
    }
    for row in 0..5 {
        expected[512 + row * 128] = (row + 1) as u8;
    }
    for (number, image) in pages.iter().enumerate() {
        let at = 262144 + number * PAGE_BYTES;
        expected[at..at + PAGE_BYTES].copy_from_slice(image);
        let at = 65536 + number * 96;
        expected[at..at + 3].copy_from_slice(&[1, image[6], 0]);
        expected[at + 8..at + 56].copy_from_slice(&image[24..72]);
        expected[at + 56..at + 88].copy_from_slice(&content_digest(image));
    }
    if stop != 1 {
        let at = 73728 + 9 * 352;
        let init = oracle.page("status-free.page");
        expected[at..at + 5].copy_from_slice(&[1, 1, 9, 0, 1]);
        expected[at + 8..at + 48].copy_from_slice(&init[24..64]);
        put(&mut expected, at + 48, 2, 8);
        put(&mut expected, at + 56, 1, 4);
        expected[at + 64..at + 96].copy_from_slice(&content_digest(oracle.raw("status.wal")));
        expected[at + 96..at + 128].copy_from_slice(&content_digest(init));
        expected[at + 128..at + 176].copy_from_slice(&allocation[24..72]);
        put(&mut expected, at + 176, 1, 8);
    }
    let mut inventory = vec![0; 160 + owners.len()];
    inventory[..4].copy_from_slice(b"XOI1");
    inventory[6..8].copy_from_slice(&[1, 2]);
    inventory[16..32].copy_from_slice(&profile[16..32]);
    let inventory_len = inventory.len() as u64;
    for (at, value, width) in [
        (4, 1, 2),
        (8, 128, 2),
        (10, 96, 2),
        (12, inventory_len, 4),
        (32, 1, 8),
        (40, generation, 8),
        (48, stop, 8),
        (56, (owners.len() / 56) as u64, 2),
    ] {
        put(&mut inventory, at, value, width);
    }
    inventory[128..128 + owners.len()].copy_from_slice(&owners);
    inventory[88..120].copy_from_slice(&content_digest(&[]));
    if stop == 1 {
        let mut chunk = vec![0; 256];
        chunk[..128].copy_from_slice(&inventory[..128]);
        chunk[6] = 2;
        chunk[56..64].fill(0);
        put(&mut chunk, 12, 256, 4);
        put(&mut chunk, 64, 1, 8);
        put(&mut chunk, 80, 1, 4);
        chunk[88..120].copy_from_slice(&content_digest(&owners));
        chunk[128] = 1;
        chunk[129] = 3;
        for (at, value) in [(136, 0x1_0000_0001), (144, 9), (152, 2), (192, 1), (200, 1)] {
            put(&mut chunk, at, value, 8);
        }
        seal(&mut chunk);
        put(&mut inventory, 64, 1, 4);
        put(&mut inventory, 72, 1, 8);
        put(&mut inventory, 80, 256, 8);
        inventory[88..120].copy_from_slice(&content_digest(&chunk));
        expected[98304 + 2176..98304 + 2176 + 256].copy_from_slice(&chunk);
    }
    seal(&mut inventory);
    expected[98304..98304 + inventory.len()].copy_from_slice(&inventory);
    for (i, (at, len, count, width)) in [
        (4096, 55296, 36, 1536),
        (65536, 6144, 64, 96),
        (73728, 22528, 64, 352),
        (98304, 16384, 1, 16384),
        (262144, 1048576, 64, 16384),
    ]
    .into_iter()
    .enumerate()
    {
        let directory = 1280 + i * 64;
        put(&mut expected, directory, (i + 1) as u64, 2);
        for (delta, value) in [(4, at), (8, len), (12, count), (16, width)] {
            put(&mut expected, directory + delta, value as u64, 4);
        }
        let digest = content_digest(&expected[at..at + len]);
        expected[directory + 32..directory + 64].copy_from_slice(&digest);
    }
    seal(&mut expected);
    expected
}
pub(super) fn first_base(original: &Images, stop: u64) -> Vec<u8> {
    base(original, stop, false)
}
pub(super) fn second_base(original: &Images) -> Vec<u8> {
    base(original, 11, true)
}
