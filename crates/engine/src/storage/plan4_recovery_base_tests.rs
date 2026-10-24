//! Separate XRB1 source-only inventory. No old test wrapper is called and no
//! CURRENT, INIT provenance, source selection or restored authority is claimed.
use super::*;
use crate::{
    api::{DatabaseId, Namespace},
    storage::control::decode_bootstrap_profile,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
};

#[path = "plan4_recovery_base_bridge_tests.rs"]
mod bridges;

const NAMES: [&str; 15] = [
    "reserved",
    "cancelled",
    "init",
    "start",
    "active",
    "rollback",
    "ready",
    "finish",
    "active2",
    "active32",
    "ready55",
    "mixed55",
    "ordinal63",
    "epoch2",
    "reserved32",
];
const RANGES: [(usize, usize); 6] = [
    (4096, 55296),
    (65536, 16384),
    (81920, 98304),
    (180224, 22528),
    (202752, 6144),
    (208896, 16384),
];
fn hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
fn scratch() -> Box<[[u8; PAGE_BYTES]; 4]> {
    vec![[0; PAGE_BYTES]; 4]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}
fn number(raw: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
}
fn set(raw: &mut [u8], at: usize, value: u64) {
    raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn set32(raw: &mut [u8], at: usize, value: u32) {
    raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn set16(raw: &mut [u8], at: usize, value: u16) {
    raw[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn seal(raw: &mut [u8]) {
    let end = raw.len() - 32;
    let d = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&d);
}
fn base_seal(raw: &mut [u8]) {
    for (i, (offset, length)) in RANGES.into_iter().enumerate() {
        let d = content_digest(&raw[offset..offset + length]);
        raw[1312 + i * 64..1344 + i * 64].copy_from_slice(&d);
    }
    seal(raw);
}
fn cell_range(page: &[u8], slot: usize) -> std::ops::Range<usize> {
    let at = 128 + slot * 4;
    let offset = u16::from_le_bytes(page[at..at + 2].try_into().unwrap()) as usize;
    let length = u16::from_le_bytes(page[at + 2..at + 4].try_into().unwrap()) as usize;
    offset..offset + length
}

/// Independent one-cell common-page construction from the documented header
/// offsets; no product encoder supplies expected pages or mutation resealing.
fn status_image(record: &[u8], initial: bool) -> Vec<u8> {
    let mut out = vec![0; PAGE_BYTES];
    out[..4].copy_from_slice(b"XDP1");
    set16(&mut out, 4, 1);
    out[6] = 9;
    out[8..24].copy_from_slice(&namespace().database.bytes());
    out[24..72].copy_from_slice(&record[8..56]);
    if initial {
        set(&mut out, 64, number(record, 56));
    }
    set16(&mut out, 72, 1);
    set16(&mut out, 74, 132);
    set16(&mut out, 76, 16128);
    set16(&mut out, 128, 16128);
    set16(&mut out, 130, 224);
    if initial {
        out[16128..16132].copy_from_slice(b"XSH1");
        set16(&mut out, 16132, 1);
        set16(&mut out, 16134, 32);
        out[16160..16164].copy_from_slice(b"XTS1");
        set16(&mut out, 16164, 1);
        set(&mut out, 16208, 1);
    } else {
        out[16128..16352].copy_from_slice(&record[128..352]);
    }
    seal(&mut out);
    out
}
fn bind_status(raw: &mut [u8], ordinal: usize) {
    let at = 180224 + ordinal * 352;
    let image = status_image(&raw[at..at + 352], false);
    let d = content_digest(&image);
    raw[at + 64..at + 96].copy_from_slice(&d);
    let cert = 202752 + ordinal * 96;
    raw.copy_within(at + 8..at + 56, cert + 8);
    raw[cert + 56..cert + 88].copy_from_slice(&d);
}
fn bind_queue(raw: &mut [u8]) {
    let mut hash = ContentHasher::new();
    for i in 0..5 {
        let start = 81920 + (i + 1) * PAGE_BYTES;
        seal(&mut raw[start..start + PAGE_BYTES]);
        for j in 0..if i == 4 { 4 } else { 15 } {
            let range = cell_range(&raw[start..start + PAGE_BYTES], j);
            hash.update(&raw[start + range.start..start + range.end]);
        }
        let d = content_digest(&raw[start..start + PAGE_BYTES]);
        let cert = 202752 + (i + 2) * 96;
        raw[cert + 56..cert + 88].copy_from_slice(&d);
    }
    let start = 81920;
    let range = cell_range(&raw[start..start + PAGE_BYTES], 0);
    raw[start + range.start + 128..start + range.start + 160].copy_from_slice(&hash.digest());
    seal(&mut raw[start..start + PAGE_BYTES]);
    let d = content_digest(&raw[start..start + PAGE_BYTES]);
    raw[202904..202936].copy_from_slice(&d);
    let aging = 4096 + 34 * 1536;
    if raw[aging] != 0 {
        raw[aging + 416..aging + 448].copy_from_slice(&d);
        let d = content_digest(&raw[start + range.start..start + range.end]);
        raw[aging + 448..aging + 480].copy_from_slice(&d);
        seal(&mut raw[aging..aging + 1536]);
    }
}
fn job_at(raw: &[u8], ordinal: usize) -> usize {
    let page = 81920 + (ordinal / 15 + 1) * PAGE_BYTES;
    page + cell_range(&raw[page..page + PAGE_BYTES], ordinal % 15).start
}

struct Fixture {
    root: PathBuf,
    index: BTreeMap<String, (usize, String)>,
    expected: Expected,
}
impl Fixture {
    fn new() -> Self {
        let root =
            PathBuf::from(std::env::var("XADB_XRB_FIXTURE_DIR").expect("frozen fixture directory"));
        let index_raw = std::fs::read(root.join("raw-index.tsv")).unwrap();
        assert!(index_raw.len() <= 16384);
        assert_eq!(
            hex(&index_raw),
            std::env::var("XADB_XRB_INDEX_SHA256").unwrap()
        );
        let mut index = BTreeMap::new();
        let mut total = 0;
        for line in std::str::from_utf8(&index_raw).unwrap().lines().skip(1) {
            let f: Vec<_> = line.split('\t').collect();
            assert_eq!(f.len(), 3);
            assert!(
                (f[0].starts_with("raw/") || f[0].starts_with("references/"))
                    && !f[0].split('/').any(|x| x == "..")
            );
            let length = f[1].parse::<usize>().unwrap();
            assert!(length <= BASE_BYTES);
            total += length;
            assert!(total < 16 << 20);
            assert!(
                index
                    .insert(f[0].to_owned(), (length, f[2].to_owned()))
                    .is_none()
            );
        }
        assert_eq!(index.len(), 29);
        let read = |name: &str| {
            let path = format!("references/{name}.bin");
            let v = std::fs::read(root.join(&path)).unwrap();
            let (len, sha) = &index[&path];
            assert_eq!(v.len(), *len);
            assert_eq!(hex(&v), *sha);
            v
        };
        let profile = decode_bootstrap_profile(&read("profile"), Some(namespace())).unwrap();
        let fm = read("filemap");
        let expected = Expected {
            profile,
            filemap: SelectedControl {
                generation: number(&fm, 32),
                reference: ControlRef::decode(&fm[80..144]).unwrap(),
            },
            bootstrap_digest: content_digest(&read("bootstrap")),
        };
        Self {
            root,
            index,
            expected,
        }
    }
    fn raw(&self, name: &str) -> Vec<u8> {
        let path = format!("raw/{name}.xrb");
        self.read(&path)
    }
    fn read(&self, path: &str) -> Vec<u8> {
        let v = std::fs::read(self.root.join(path)).unwrap();
        let (len, sha) = &self.index[path];
        assert_eq!(v.len(), *len);
        assert_eq!(hex(&v), *sha);
        v
    }
}

fn cases(prefix: &str, count: usize, mut run: impl FnMut(usize)) {
    let owned: BTreeSet<_> = (1..=count).map(|i| format!("XB-{prefix}{i:03}")).collect();
    let parse = |key: &str, required: bool| {
        let text = std::env::var(key).unwrap_or_else(|_| {
            assert!(!required, "missing {key}");
            String::new()
        });
        assert!(text.len() <= 16384);
        let mut set = BTreeSet::new();
        for id in text.split(',').filter(|s| !s.is_empty()) {
            assert!(owned.contains(id), "foreign ID {id}");
            assert!(set.insert(id.to_owned()), "duplicate ID");
        }
        set
    };
    let include = parse("XADB_XRB_INCLUDE", true);
    let exclude = parse("XADB_XRB_EXCLUDE", false);
    assert!(include.is_disjoint(&exclude));
    let completed = parse("XADB_XRB_COMPLETED", false);
    assert!(
        include.is_disjoint(&completed),
        "completed ID replay forbidden"
    );
    let selected: BTreeSet<_> = include.difference(&exclude).cloned().collect();
    assert!(!selected.is_empty());
    let context = [
        "XADB_XRB_SOURCE_SHA256",
        "XADB_XRB_ELF_SHA256",
        "XADB_XRB_INPUT_SHA256",
    ]
    .map(|k| {
        let v = std::env::var(k).unwrap();
        assert!(v.len() == 64 && v.bytes().all(|x| x.is_ascii_hexdigit()));
        v
    })
    .join("\t");
    for i in 1..=count {
        let id = format!("XB-{prefix}{i:03}");
        if !selected.contains(&id) {
            println!("XB_SKIP\t{id}");
            continue;
        }
        println!("XB_CONTEXT\t{id}\t{context}");
        println!("XB_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(|| run(i))) {
            Ok(()) => println!("XB_PASS\t{id}"),
            Err(e) => {
                println!("XB_FAIL\t{id}");
                resume_unwind(e)
            }
        }
    }
}
fn rejected(raw: &[u8], expected: Expected, want: Error) {
    let before = content_digest(raw);
    let mut s = scratch();
    match decode(raw, expected, &mut s) {
        Err(e) => assert_eq!(e, want),
        Ok(_) => panic!("invalid base accepted"),
    };
    assert_eq!(content_digest(raw), before);
}

#[test]
fn xrb_canonical_snapshots() {
    cases("C", 15, |i| {
        let f = Fixture::new();
        let raw = f.raw(NAMES[i - 1]);
        let mut s = scratch();
        let before = content_digest(&raw);
        let view = decode(&raw, f.expected, &mut s).unwrap();
        assert_eq!(view.digest(), before);
        assert_eq!(view.bytes().as_slice(), raw);
        let tx = match i {
            5 | 6 | 13 => 1,
            9 => 2,
            10 | 12 | 15 => 32,
            _ => 0,
        };
        let ready = match i {
            7 | 14 => 1,
            11 => 55,
            12 => 23,
            _ => 0,
        };
        assert_eq!(view.census().transactions, tx);
        assert_eq!(view.census().leased, tx);
        assert_eq!(view.census().ready, ready);
        assert_eq!(view.census().reserved.is_some(), i == 1 || i == 15);
        if i == 15 {
            assert_eq!(view.census().systems, 2);
            assert_eq!(view.census().inventory.owners, 34);
            assert_eq!(view.census().inventory.items, 1);
            assert_eq!(view.census().reserved.unwrap().page_no, 41);
        }
        assert_eq!(view.header().generation, if i == 14 { 2 } else { 1 });
        let mut image = boxed::<PAGE_BYTES>();
        for ordinal in 0..64 {
            let cert = view.certificate(ordinal).unwrap();
            assert_eq!(cert.is_some(), raw[65536 + 10208 + ordinal * 96] == 2);
            assert_eq!(view.image(ordinal, &mut image).unwrap(), cert.is_some());
            if let Some(c) = cert {
                assert_eq!(content_digest(image.as_slice()), c.full_digest);
                assert_eq!(
                    c.full_digest.as_slice(),
                    &raw[202752 + ordinal * 96 + 56..202752 + ordinal * 96 + 88]
                );
                if ordinal <= 6 {
                    let at = if ordinal == 0 {
                        65536
                    } else {
                        81920 + (ordinal - 1) * PAGE_BYTES
                    };
                    assert_eq!(image.as_slice(), &raw[at..at + PAGE_BYTES]);
                }
            }
        }
        assert_eq!(content_digest(&raw), before);
    });
}

#[test]
fn xrb_header_rejections() {
    cases("H", 16, |i| {
        let f = Fixture::new();
        let mut raw = f.raw("active");
        let want = match i {
            1 => {
                set16(&mut raw, 4, 2);
                Error::Unsupported
            }
            2 => {
                set16(&mut raw, 6, 2);
                Error::Unsupported
            }
            3 => {
                set32(&mut raw, 8, 1048575);
                Error::IdentityMismatch
            }
            4 => {
                set(&mut raw, 96, 0);
                set(&mut raw, 248, 0);
                Error::InvalidInput
            }
            5 => {
                set(&mut raw, 88, 0);
                Error::InvalidInput
            }
            6 => {
                set(&mut raw, 104, 1);
                raw[112] = 1;
                Error::OrderMismatch
            }
            7 => {
                set32(&mut raw, 144, 0);
                Error::InvalidInput
            }
            8 => {
                set32(&mut raw, 1184, 0);
                Error::InvalidInput
            }
            9 => {
                set32(&mut raw, 256, 33);
                Error::InvalidInput
            }
            10 => {
                let m = number(&raw, 264);
                set(&mut raw, 264, m | 1);
                Error::InvalidInput
            }
            11 => {
                set(&mut raw, 288, 0);
                Error::InvalidInput
            }
            12 => {
                set(&mut raw, 200, 1);
                Error::IdentityMismatch
            }
            13 => {
                raw[1188] = 1;
                Error::IdentityMismatch
            }
            14 => {
                raw[48] ^= 1;
                Error::IdentityMismatch
            }
            15 => {
                raw[464] ^= 1;
                Error::IdentityMismatch
            }
            16 => {
                set(&mut raw, 192, 0);
                Error::IdentityMismatch
            }
            _ => unreachable!(),
        };
        base_seal(&mut raw);
        rejected(&raw, f.expected, want);
    });
}

#[test]
fn xrb_section_rejections() {
    cases("S", 16, |i| {
        let f = Fixture::new();
        let mut raw = f.raw("active");
        let want = if i <= 6 {
            let at = 1280 + (i - 1) * 64 + 8;
            set32(&mut raw, at, RANGES[i - 1].1 as u32 + 1);
            Error::Corrupt
        } else if i <= 12 {
            raw[1312 + (i - 7) * 64] ^= 1;
            Error::Corrupt
        } else {
            match i {
                13 => {
                    raw[59392] = 1;
                    Error::Corrupt
                }
                14 => {
                    raw[225280] = 1;
                    Error::Corrupt
                }
                15 => {
                    raw[180224 + 9 * 352 + 4] = 1;
                    Error::Corrupt
                }
                16 => {
                    raw[202752 + 9 * 96 + 3] = 1;
                    Error::IdentityMismatch
                }
                _ => unreachable!(),
            }
        };
        if i <= 12 {
            seal(&mut raw)
        } else {
            base_seal(&mut raw)
        };
        rejected(&raw, f.expected, want);
    });
}

#[test]
fn xrb_global_census_rejections() {
    cases("G", 24, |i| {
        let f = Fixture::new();
        let name = if (16..=20).contains(&i) {
            "reserved"
        } else if (21..=23).contains(&i) {
            "active2"
        } else if i == 24 {
            "ready"
        } else {
            "active"
        };
        let mut raw = f.raw(name);
        let status = 180224 + 9 * 352;
        let j = job_at(&raw, 0);
        let desc = 4096;
        let want = match i {
            1 => {
                set(&mut raw, desc + 64, 3);
                set(&mut raw, desc + 208, 3);
                seal(&mut raw[desc..desc + 1536]);
                Error::IdentityMismatch
            }
            2 | 24 => {
                set(&mut raw, j + 48, 2);
                bind_queue(&mut raw);
                Error::IdentityMismatch
            }
            3 => {
                set(&mut raw, status + 200, 2);
                bind_status(&mut raw, 9);
                Error::IdentityMismatch
            }
            4 => {
                set(&mut raw, j + 88, 10);
                bind_queue(&mut raw);
                Error::IdentityMismatch
            }
            5 => {
                set(&mut raw, status + 208, 3);
                bind_status(&mut raw, 9);
                Error::IdentityMismatch
            }
            6 => {
                set(&mut raw, status + 136, 1);
                bind_status(&mut raw, 9);
                Error::Unsupported
            }
            7 => {
                raw[status + 166] = 3;
                set(&mut raw, status + 216, 1);
                bind_status(&mut raw, 9);
                Error::Unsupported
            }
            8 => {
                raw[status + 166] = 2;
                raw[status + 167] = 1;
                bind_status(&mut raw, 9);
                Error::Unsupported
            }
            9 => {
                raw[status + 224] = 1;
                raw[status + 232..status + 256].copy_from_slice(
                    &[FILE.to_le_bytes(), 10u64.to_le_bytes(), 2u64.to_le_bytes()].concat(),
                );
                set(&mut raw, status + 264, 2);
                bind_status(&mut raw, 9);
                Error::Unsupported
            }
            10 => {
                set(&mut raw, status + 48, 5);
                bind_status(&mut raw, 9);
                Error::IdentityMismatch
            }
            11 => {
                set(&mut raw, status + 56, 0);
                Error::Corrupt
            }
            12 => {
                set(&mut raw, status + 56, 5);
                Error::Corrupt
            }
            13 => {
                raw[status..status + 352].fill(0);
                Error::Corrupt
            }
            14 => {
                raw[202752 + 9 * 96..202752 + 10 * 96].fill(0);
                Error::IdentityMismatch
            }
            15 => {
                raw[202752 + 7 * 96 + 56] ^= 1;
                Error::IdentityMismatch
            }
            16 => {
                let at = 208896 + 2176 + 128;
                set(&mut raw, at + 16, 10);
                let normalized = content_digest(&raw[at..at + 96]);
                let descriptor = 4096 + 32 * 1536;
                raw[descriptor + 560..descriptor + 592].copy_from_slice(&normalized);
                seal(&mut raw[descriptor..descriptor + 1536]);
                seal(&mut raw[211072..211328]);
                let d = content_digest(&raw[211072..211328]);
                raw[208984..209016].copy_from_slice(&d);
                seal(&mut raw[208896..209112]);
                Error::IdentityMismatch
            }
            17 => {
                set16(&mut raw, 211202, 1);
                seal_inventory(&mut raw);
                Error::Inventory(InventoryError::Corrupt)
            }
            18 => {
                raw[211200] = 2;
                seal_inventory(&mut raw);
                Error::Inventory(InventoryError::Corrupt)
            }
            19 => {
                set(&mut raw, 208968, 0);
                seal(&mut raw[208896..209112]);
                Error::Corrupt
            }
            20 => {
                let entry = 65536 + 10208 + 9 * 96;
                raw[entry..entry + 96].fill(0);
                set(&mut raw, entry + 8, 2);
                seal(&mut raw[65536..81920]);
                let d = content_digest(&raw[65536..81920]);
                raw[202808..202840].copy_from_slice(&d);
                Error::IdentityMismatch
            }
            21 | 22 => {
                let at = 4096 + 1536;
                set(&mut raw, at + 56, 1);
                set(&mut raw, at + 200, 1);
                if i == 22 {
                    set(&mut raw, at + 64, 3);
                    set(&mut raw, at + 208, 3)
                }
                seal(&mut raw[at..at + 1536]);
                Error::IdentityMismatch
            }
            23 => {
                let at = job_at(&raw, 1);
                set(&mut raw, at + 88, 9);
                bind_queue(&mut raw);
                Error::IdentityMismatch
            }
            _ => unreachable!(),
        };
        base_seal(&mut raw);
        rejected(&raw, f.expected, want);
    });
}
const FILE: u64 = 0x100000001;
fn seal_inventory(raw: &mut [u8]) {
    seal(&mut raw[211072..211328]);
    let d = content_digest(&raw[211072..211328]);
    raw[208984..209016].copy_from_slice(&d);
    seal(&mut raw[208896..209112]);
}

#[test]
fn xrb_provenance_and_bindings() {
    cases("I", 8, |i| {
        let f = Fixture::new();
        let mut raw = f.raw(if i == 1 { "active2" } else { "ready" });
        let mut s = scratch();
        if i == 3 {
            set(&mut raw, 180224 + 9 * 352 + 56, 1);
            base_seal(&mut raw)
        }
        if i >= 6 {
            let mut expected = f.expected;
            match i {
                6 => expected.filemap.reference.digest[0] ^= 1,
                7 => {
                    expected.profile = decode_bootstrap_profile(
                        &f.read("references/other-profile.bin"),
                        Some(namespace()),
                    )
                    .unwrap()
                }
                8 => expected.bootstrap_digest[0] ^= 1,
                _ => unreachable!(),
            };
            rejected(&raw, expected, Error::IdentityMismatch);
            return;
        }
        let view = decode(&raw, f.expected, &mut s).unwrap();
        match i {
            1 => {
                let mut n = 0;
                let mut seen = 0u64;
                view.check_init_provenance(&mut s[0], |claim| {
                    let ordinal = claim.header.identity.page.page_no as usize;
                    assert!((9..=10).contains(&ordinal));
                    assert_eq!(seen & (1 << ordinal), 0);
                    seen |= 1 << ordinal;
                    let at = 180224 + ordinal * 352;
                    let image = status_image(&raw[at..at + 352], true);
                    assert_eq!(claim.full_digest, content_digest(&image));
                    assert_eq!(claim.header.lsn.0, 2 * (ordinal - 9) as u64 + 2);
                    n += 1;
                    Ok(())
                })
                .unwrap();
                assert_eq!(n, 2);
                assert_eq!(seen, (1 << 9) | (1 << 10));
            }
            2 | 3 => {
                assert_eq!(
                    view.check_init_provenance(&mut s[0], |claim| {
                        if i == 3 {
                            assert_eq!(claim.header.lsn.0, 1);
                        }
                        Err(Error::ProvenanceMismatch)
                    }),
                    Err(Error::ProvenanceMismatch)
                );
            }
            4 => {
                let copy = raw.clone();
                let second = decode(&copy, f.expected, &mut s).unwrap();
                assert_eq!(view.digest(), second.digest());
                assert_eq!(view.bytes(), second.bytes());
            }
            5 => {
                let other = f.raw("epoch2");
                let second = decode(&other, f.expected, &mut s).unwrap();
                assert_ne!(view.digest(), second.digest());
                assert_eq!(view.header().capture_lsn, second.header().capture_lsn);
                assert_eq!(second.header().previous_digest, view.digest());
            }
            _ => unreachable!(),
        }
    });
}

fn input_header(raw: &[u8]) -> Header {
    let digest = |at: usize| raw[at..at + 32].try_into().unwrap();
    let r: [RemainingBudget; 5] = std::array::from_fn(|i| {
        let at = 512 + i * 128;
        RemainingBudget {
            steps: number(raw, at + 8),
            wal: number(raw, at + 16),
            control: number(raw, at + 24),
            growth: number(raw, at + 32),
            pages: number(raw, at + 40),
            undo_slots: number(raw, at + 48),
            status_slots: number(raw, at + 56),
            peak: ResourcePeak {
                frames: number(raw, at + 64),
                memory: number(raw, at + 72),
                locks: number(raw, at + 80),
                registry: number(raw, at + 88),
                files: number(raw, at + 96),
                doublewrite: number(raw, at + 104),
            },
            protected: number(raw, at + 112),
        }
    });
    Header {
        generation: number(raw, 88),
        capture_lsn: number(raw, 96),
        previous_generation: number(raw, 104),
        previous_digest: digest(112),
        source_epoch: u32::from_le_bytes(raw[144..148].try_into().unwrap()),
        source_offset: number(raw, 152),
        source_prefix_digest: digest(160),
        source_local: number(raw, 192),
        system_high_water: std::array::from_fn(|i| number(raw, 208 + 8 * i)),
        active_transactions: u32::from_le_bytes(raw[256..260].try_into().unwrap()),
        active_systems: u32::from_le_bytes(raw[260..264].try_into().unwrap()),
        status_mask: number(raw, 264),
        leased: number(raw, 272),
        ready: number(raw, 280),
        root_revision: number(raw, 288),
        aging_groups: number(raw, 296),
        allocation_terminal: match raw[304] {
            0 => AllocationTerminal::None,
            1 => AllocationTerminal::Cancelled,
            2 => AllocationTerminal::InitializedStatus,
            _ => panic!(),
        },
        retired_batch: number(raw, 496),
        doublewrite_generation: number(raw, 504),
        remaining: r,
        physical_bytes: number(raw, 1152),
        issued_epoch: u32::from_le_bytes(raw[1184..1188].try_into().unwrap()),
        source_header_digest: digest(1192),
    }
}
#[test]
fn xrb_encoder_literals() {
    cases("E", 8, |i| {
        let f = Fixture::new();
        let raw = f.raw(if i <= 6 {
            ["init", "active", "rollback", "ready", "finish", "reserved"][i - 1]
        } else {
            "ready"
        });
        let mut s = scratch();
        if i <= 6 {
            let queue: Box<[[u8; PAGE_BYTES]; 6]> = raw[81920..180224]
                .chunks_exact(PAGE_BYTES)
                .map(|p| p.try_into().unwrap())
                .collect::<Vec<_>>()
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let statuses: Box<[[u8; 352]; 64]> = raw[180224..202752]
                .chunks_exact(352)
                .map(|p| p.try_into().unwrap())
                .collect::<Vec<_>>()
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let certificates: Box<[[u8; 96]; 64]> = raw[202752..208896]
                .chunks_exact(96)
                .map(|p| p.try_into().unwrap())
                .collect::<Vec<_>>()
                .into_boxed_slice()
                .try_into()
                .unwrap();
            let sections = Sections {
                descriptors: raw[4096..59392].try_into().unwrap(),
                allocator: raw[65536..81920].try_into().unwrap(),
                queue: &queue,
                statuses: &statuses,
                certificates: &certificates,
                inventory: raw[208896..225280].try_into().unwrap(),
            };
            let mut output = boxed::<BASE_BYTES>();
            let view = encode(
                input_header(&raw),
                f.expected,
                sections,
                &mut output,
                &mut s,
            )
            .unwrap();
            assert_eq!(view.bytes().as_slice(), raw);
        } else {
            let at = 180224 + 9 * 352;
            let image = status_image(&raw[at..at + 352], false);
            let header = PageHeader {
                identity: PageIdentity {
                    database: namespace().database,
                    page: PageRef::decode(&raw[at + 8..at + 32]).unwrap(),
                    owner: ObjectId::new(2).unwrap(),
                    owner_generation: Generation::new(1).unwrap(),
                },
                kind: PageKind::TransactionStatus,
                level: 0,
                lsn: PageLsn(6),
            };
            if i == 7 {
                let mut out = [0xa5; 352];
                encode_status_record(
                    f.expected,
                    header,
                    2,
                    image.as_slice().try_into().unwrap(),
                    &mut out,
                )
                .unwrap();
                assert_eq!(out.as_slice(), &raw[at..at + 352]);
            } else {
                let mut out = [0xa5; 96];
                encode_certificate(
                    f.expected,
                    header,
                    image.as_slice().try_into().unwrap(),
                    &mut out,
                )
                .unwrap();
                assert_eq!(out.as_slice(), &raw[202752 + 9 * 96..202752 + 10 * 96]);
            }
        }
    });
}

#[test]
fn xrb_partial_source_layout() {
    cases("ST", 1, |_| {
        use std::mem::size_of;
        let local = source_bytes();
        println!(
            "XB_PARTIAL_SOURCE\tlocal={local}\tview={}\theader={}\tresult={}\tdescriptor={}\tcaller_base={}\tcaller_scratch={}\tCALLER_AND_LIVE_OWNER_NOT_INCLUDED",
            size_of::<BaseView<'_>>(),
            size_of::<Header>(),
            size_of::<Result<BaseView<'_>>>(),
            size_of::<DescriptorSpec>(),
            BASE_BYTES,
            4 * PAGE_BYTES
        );
        assert!(size_of::<BaseView<'_>>() < 8192);
        assert!(size_of::<Header>() < 4096);
        assert!(local >= size_of::<BaseView<'_>>());
        // Measurement of new source layout only. No F05 capacity acceptance here.
    });
}

#[test]
#[ignore]
fn xrb_s5_reserved_header_bytes() {
    cases("RH", 256, |i| {
        let offsets: Vec<_> = (305..320)
            .chain(456..464)
            .chain(1160..1184)
            .chain(1188..1192)
            .chain(1224..1280)
            .chain(1664..1813)
            .collect();
        assert_eq!(offsets.len(), 256);
        let f = Fixture::new();
        let mut raw = f.raw("active");
        raw[offsets[i - 1]] = 1;
        base_seal(&mut raw);
        rejected(&raw, f.expected, Error::IdentityMismatch);
    });
}
