//! Independent plan4 inputs. Raw generation and execution require the frozen gate.
use super::*;
use crate::api::DatabaseId;
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
};

pub(in crate::storage) fn ids(prefix: &str, first: usize, last: usize) -> Vec<String> {
    (first..=last)
        .map(|n| format!("P4-{prefix}{n:02}"))
        .collect()
}
pub(in crate::storage) struct Harness {
    owned: BTreeSet<String>,
    selected: BTreeSet<String>,
    seen: BTreeSet<String>,
    identity: String,
}
impl Harness {
    pub(in crate::storage) fn new(owned: Vec<String>) -> Self {
        let count = owned.len();
        let owned: BTreeSet<_> = owned.into_iter().collect();
        assert_eq!(owned.len(), count, "duplicate owned ID");
        assert!(!owned.is_empty(), "empty harness ownership");
        let parse = |name: &str| -> Option<BTreeSet<String>> {
            std::env::var(name).ok().map(|value| {
                assert!(!value.is_empty(), "empty {name}");
                let mut result = BTreeSet::new();
                for id in value.split(',') {
                    assert!(
                        owned.contains(id),
                        "unknown/foreign exact ID {id} in {name}"
                    );
                    assert!(result.insert(id.to_owned()), "duplicate ID {id}");
                }
                result
            })
        };
        let requested = parse("XADB_PLAN4_INCLUDE");
        let explicit = requested.is_some();
        let include = requested.unwrap_or_else(|| owned.clone());
        let exclude = parse("XADB_PLAN4_EXCLUDE").unwrap_or_default();
        assert!(
            !explicit || include.is_disjoint(&exclude),
            "include/exclude overlap"
        );
        let selected: BTreeSet<_> = include.difference(&exclude).cloned().collect();
        assert!(!selected.is_empty(), "empty actual selection");
        let identity = [
            "XADB_PLAN4_GENESIS_SOURCE_SHA256",
            "XADB_PLAN4_GENESIS_ELF_SHA256",
            "XADB_PLAN4_GENESIS_INPUT_SHA256",
        ]
        .map(|key| {
            let value = std::env::var(key).unwrap_or_else(|_| panic!("missing {key}"));
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
    pub(in crate::storage) fn case(&mut self, id: &str, operation: impl FnOnce()) {
        assert!(self.owned.contains(id), "foreign harness ID {id}");
        assert!(self.seen.insert(id.to_owned()), "duplicate execution {id}");
        if !self.selected.contains(id) {
            println!("P4_SKIP\t{id}");
            return;
        }
        println!("P4_CONTEXT\t{id}\t{}", self.identity);
        println!("P4_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(()) => println!("P4_PASS\t{id}"),
            Err(error) => {
                println!("P4_FAIL\t{id}");
                resume_unwind(error);
            }
        }
    }
    pub(in crate::storage) fn finish(self) {
        assert_eq!(self.seen, self.owned, "unvisited harness ID");
    }
}
pub(in crate::storage) fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
pub(in crate::storage) fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dev3-plan4-genesis")
}
pub(in crate::storage) fn hash_hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub(in crate::storage) fn raw(id: &str) -> Vec<u8> {
    let root = fixture_root();
    let index = std::fs::read_to_string(root.join("raw-index.tsv")).unwrap();
    let rows: Vec<_> = index
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 4, "raw index shape");
            (fields[0] == id).then_some(fields)
        })
        .collect();
    assert_eq!(rows.len(), 1, "raw index ID {id}");
    let row = &rows[0];
    assert!(!row[1].starts_with('/') && !row[1].split('/').any(|p| p == ".."));
    let bytes = std::fs::read(root.join(row[1])).unwrap();
    assert_eq!(bytes.len(), row[2].parse::<usize>().unwrap());
    assert_eq!(hash_hex(&bytes), row[3], "raw SHA {id}");
    bytes
}
pub(in crate::storage) fn reference(profile: u32, name: &str) -> Vec<u8> {
    raw(&format!("REF-P{profile}-{name}"))
}
pub(in crate::storage) fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}
pub(in crate::storage) fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}
pub(in crate::storage) fn resign(bytes: &mut [u8]) {
    let end = bytes.len() - 32;
    let digest = content_digest(&bytes[..end]);
    bytes[end..].copy_from_slice(&digest);
}
pub(in crate::storage) fn files(profile: u32) -> BTreeMap<String, Vec<u8>> {
    files_for(profile, namespace().database.bytes())
}
/// Published-offset namespace substitution, independent of product encoders.
pub(in crate::storage) fn files_for(profile: u32, database: [u8; 16]) -> BTreeMap<String, Vec<u8>> {
    assert!((1..=3).contains(&profile) && database.iter().any(|b| *b != 0));
    let mut profile_raw = reference(profile, "d3-profile");
    profile_raw[16..32].copy_from_slice(&database);
    resign(&mut profile_raw);
    let profile_hash = content_digest(&profile_raw);
    let mut epoch = reference(profile, "d3-wal-00000001");
    epoch[16..32].copy_from_slice(&database);
    epoch[56..88].copy_from_slice(&profile_hash);
    resign(&mut epoch);
    let mut writer = reference(profile, "WRITER");
    writer[16..32].copy_from_slice(&database);
    writer[64..96].copy_from_slice(&profile_hash);
    writer[96..128].copy_from_slice(&content_digest(&epoch));
    resign(&mut writer);
    let mut chunk = reference(profile, "chunk");
    chunk[16..32].copy_from_slice(&database);
    resign(&mut chunk);
    let mut root = reference(profile, "root");
    root[16..32].copy_from_slice(&database);
    root[224..256].copy_from_slice(&content_digest(&chunk));
    resign(&mut root);
    let mut filemap = reference(profile, "FILEMAP");
    filemap[8..24].copy_from_slice(&database);
    filemap[48..80].copy_from_slice(&profile_hash);
    filemap[112..144].copy_from_slice(&content_digest(&root));
    resign(&mut filemap);
    let mut data = vec![0; 1_048_576];
    for i in 0..9 {
        let mut page = raw(&format!("P{i:02}"));
        page[8..24].copy_from_slice(&database);
        if i == 1 {
            page[15_840 + 192..15_840 + 208].copy_from_slice(&database);
        }
        resign(&mut page);
        data[i * PAGE_BYTES..(i + 1) * PAGE_BYTES].copy_from_slice(&page);
    }
    let mut bootstrap = raw(&format!("XBS-P{profile}"));
    bootstrap[16..32].copy_from_slice(&database);
    bootstrap[48..80].copy_from_slice(&profile_hash);
    bootstrap[176..208].copy_from_slice(&content_digest(&root));
    bootstrap[208..240].copy_from_slice(&content_digest(&data[..PAGE_BYTES]));
    resign(&mut bootstrap);
    let mut bank0 = vec![0; 2_113_536];
    bank0[..320].copy_from_slice(&root);
    bank0[PAGE_BYTES..PAGE_BYTES + 256].copy_from_slice(&chunk);
    let mut empty = reference(profile, "empty");
    empty[8..24].copy_from_slice(&database);
    resign(&mut empty);
    let empty_hash = content_digest(&empty);
    let mut dwb = reference(profile, "DWB");
    dwb[8..24].copy_from_slice(&database);
    dwb[48..80].copy_from_slice(&profile_hash);
    dwb[112..144].copy_from_slice(&empty_hash);
    resign(&mut dwb);
    let mut stage = reference(profile, "d3-dwb-stage");
    stage[16..32].copy_from_slice(&database);
    stage[48..80].copy_from_slice(&profile_hash);
    stage[160..192].copy_from_slice(&content_digest(&root));
    stage[232..264].copy_from_slice(&empty_hash);
    resign(&mut stage);
    let mut dwb0 = vec![0; PAGE_BYTES];
    dwb0[..160].copy_from_slice(&empty);
    let values = [
        ("LOCK", vec![]),
        ("d3-profile", profile_raw),
        ("d3-wal-00000001", epoch),
        ("WRITER", writer),
        ("d3-bootstrap", bootstrap),
        ("FILEMAP", filemap),
        ("d3-filemap-0", bank0),
        ("d3-filemap-1", vec![0; 2_113_536]),
        ("d3-data-0000000100000001", data),
        ("DWB", dwb),
        ("d3-dwb-stage", stage),
        ("d3-dwb-0", dwb0),
        ("d3-dwb-1", vec![0; PAGE_BYTES]),
        ("d3-dwb-images", vec![0; 16_711_680]),
        ("d3-dwb-reserved", vec![0; 8192]),
    ];
    let files: BTreeMap<_, _> = values.into_iter().map(|(n, b)| (n.to_owned(), b)).collect();
    assert_eq!(files.len(), 15);
    assert_eq!(files.values().map(Vec::len).sum::<usize>(), 22_069_248);
    files
}

#[test]
fn plan4_codec_invalid_inputs() {
    let mut h = Harness::new(ids("E", 1, 12));
    for case in 1..=12 {
        h.case(&format!("P4-E{case:02}"), || {
            let mut ns = namespace();
            let mut page = RetentionGenesisPage::Root;
            if case <= 8 {
                let len = match case {
                    1 => 0,
                    2 | 4 => PAGE_BYTES - 1,
                    3 => PAGE_BYTES + 1,
                    _ => PAGE_BYTES,
                };
                match case {
                    4 => page = RetentionGenesisPage::Jobs { index: 4 },
                    5 => ns.timeline = 0,
                    6 => ns.timeline = 2,
                    7 => page = RetentionGenesisPage::Jobs { index: 5 },
                    8 => page = RetentionGenesisPage::Jobs { index: 255 },
                    _ => {}
                }
                let mut output = vec![0xa7; len];
                assert_eq!(
                    encode(ns, page, &mut output),
                    Err(RetentionGenesisError::InvalidInput)
                );
                assert!(output.iter().all(|byte| *byte == 0xa7));
            } else {
                let mut input = raw(if case == 12 { "P02" } else { "P01" });
                match case {
                    9 => {
                        input.pop();
                    }
                    10 => input.push(0),
                    11 => ns.timeline = 0,
                    12 => page = RetentionGenesisPage::Jobs { index: 5 },
                    _ => unreachable!(),
                }
                let before = input.clone();
                let error = if case < 11 {
                    RetentionGenesisError::Page(PageError::Corrupt)
                } else {
                    RetentionGenesisError::InvalidInput
                };
                assert_eq!(validate(&input, ns, page), Err(error));
                assert_eq!(input, before);
            }
        });
    }
    h.finish();
}

pub(in crate::storage) fn job_field_id(ordinal: usize, field: &str) -> String {
    format!("P4-SJ{ordinal:02}-{field}")
}
#[test]
#[ignore = "DEV3 final S5 expansion"]
fn plan4_s5_job_fields() {
    let fields = [
        "G0", "G2", "STATE1", "R7", "SRC16", "B384", "B511", "B1023", "R320",
    ];
    let omitted = [(0, "G0"), (63, "G2"), (31, "STATE1"), (17, "B1023")];
    let cases: Vec<_> = (0..64)
        .flat_map(|n| fields.into_iter().map(move |f| (n, f)))
        .filter(|item| !omitted.contains(item))
        .collect();
    assert_eq!(cases.len(), 572);
    let mut h = Harness::new(cases.iter().map(|(n, f)| job_field_id(*n, f)).collect());
    for (ordinal, field) in cases {
        h.case(&job_field_id(ordinal, field), || {
            let index = ordinal / 15;
            let slot = ordinal % 15;
            let mut bytes = raw(&format!("P{:02}", index + 2));
            let at = 16352 - 1024 * (slot + 1);
            match field {
                "G0" => put64(&mut bytes, at + 8, 0),
                "G2" => put64(&mut bytes, at + 8, 2),
                "STATE1" => bytes[at + 6] = 1,
                "R7" => bytes[at + 7] = 1,
                "SRC16" => bytes[at + 16] = 1,
                "B384" => bytes[at + 384] = 1,
                "B511" => bytes[at + 511] = 1,
                "B1023" => bytes[at + 1023] = 1,
                "R320" => bytes[at + 320] = 1,
                _ => unreachable!(),
            }
            resign(&mut bytes);
            assert_eq!(
                validate(
                    &bytes,
                    namespace(),
                    RetentionGenesisPage::Jobs { index: index as u8 }
                ),
                Err(RetentionGenesisError::Corrupt)
            );
        });
    }
    h.finish();
}

#[test]
#[ignore = "DEV3 final S5 expansion"]
fn plan4_s5_root_reserved() {
    let owned = (384..511)
        .map(|offset| format!("P4-SR{offset:03}"))
        .collect();
    let mut h = Harness::new(owned);
    for offset in 384..511 {
        h.case(&format!("P4-SR{offset:03}"), || {
            let mut bytes = raw("P01");
            bytes[15840 + offset] = 1;
            resign(&mut bytes);
            assert_eq!(
                validate(&bytes, namespace(), RetentionGenesisPage::Root),
                Err(RetentionGenesisError::Corrupt)
            );
        });
    }
    h.finish();
}
