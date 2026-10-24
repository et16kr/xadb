//! New G6 local representation inputs; no completed genesis test is invoked.
use super::*;
use crate::storage::{
    control::{Profile, decode_bootstrap_profile},
    page::PageIdentity,
};
use crate::{api::DatabaseId, transaction::TxnId};
use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
};
use xadb_foundation::{codec::content_digest, identity::ObjectId};

pub(in crate::storage) fn ids(prefix: &str, count: usize) -> Vec<String> {
    (1..=count).map(|i| format!("G6-{prefix}{i:02}")).collect()
}
pub(in crate::storage) struct Harness {
    owned: BTreeSet<String>,
    selected: BTreeSet<String>,
    seen: BTreeSet<String>,
    identity: String,
}
impl Harness {
    pub(in crate::storage) fn new(ids: Vec<String>) -> Self {
        let count = ids.len();
        let owned: BTreeSet<_> = ids.into_iter().collect();
        assert_eq!(count, owned.len());
        assert!(count > 0);
        let parse = |key: &str| {
            std::env::var(key).ok().map(|text| {
                assert!(!text.is_empty());
                assert!(text.len() <= 16384, "bounded exact selection input");
                let mut result = BTreeSet::new();
                for id in text.split(',') {
                    assert!(owned.contains(id), "unknown/foreign ID {id}");
                    assert!(result.insert(id.to_owned()), "duplicate ID {id}");
                }
                result
            })
        };
        let requested = parse("XADB_G6_INCLUDE");
        let explicit = requested.is_some();
        let include = requested.unwrap_or_else(|| owned.clone());
        let exclude = parse("XADB_G6_EXCLUDE").unwrap_or_default();
        assert!(!explicit || include.is_disjoint(&exclude));
        let selected: BTreeSet<_> = include.difference(&exclude).cloned().collect();
        assert!(!selected.is_empty());
        let identity = [
            "XADB_G6_SOURCE_SHA256",
            "XADB_G6_ELF_SHA256",
            "XADB_G6_INPUT_SHA256",
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
        assert!(self.owned.contains(id));
        assert!(self.seen.insert(id.to_owned()));
        if !self.selected.contains(id) {
            println!("G6_SKIP\t{id}");
            return;
        }
        println!("G6_CONTEXT\t{id}\t{}", self.identity);
        println!("G6_BEGIN\t{id}");
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(()) => println!("G6_PASS\t{id}"),
            Err(error) => {
                println!("G6_FAIL\t{id}");
                resume_unwind(error);
            }
        }
    }
    pub(in crate::storage) fn finish(self) {
        assert_eq!(self.seen, self.owned);
    }
}
pub(in crate::storage) fn cases(prefix: &str, count: usize, mut f: impl FnMut(usize)) {
    let owned = ids(prefix, count);
    let mut h = Harness::new(owned.clone());
    for (i, id) in owned.iter().enumerate() {
        h.case(id, || f(i + 1));
    }
    h.finish();
}
pub(in crate::storage) fn namespace() -> Namespace {
    Namespace {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 1,
    }
}
pub(in crate::storage) fn gen_value(value: u64) -> Generation {
    Generation::new(value).unwrap()
}
pub(in crate::storage) fn actor(local: u64, generation: u64) -> TxnRef {
    TxnRef::new(
        TxnId {
            database: namespace().database,
            timeline: 1,
            writer_epoch: 1,
            local,
        },
        gen_value(generation),
    )
    .unwrap()
}
pub(in crate::storage) fn page_ref(number: u64, incarnation: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x100000001).unwrap(),
        page_no: number,
        incarnation: gen_value(incarnation),
    }
}
pub(in crate::storage) fn status_ref() -> StatusSlotRef {
    StatusSlotRef::new(page_ref(9, 2), 0, gen_value(2)).unwrap()
}
pub(in crate::storage) fn status_identity() -> PageIdentity {
    PageIdentity {
        database: namespace().database,
        page: page_ref(9, 2),
        owner: ObjectId::new(2).unwrap(),
        owner_generation: gen_value(1),
    }
}
pub(in crate::storage) fn put16(raw: &mut [u8], at: usize, value: u16) {
    raw[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
pub(in crate::storage) fn put32(raw: &mut [u8], at: usize, value: u32) {
    raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
pub(in crate::storage) fn put64(raw: &mut [u8], at: usize, value: u64) {
    raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
pub(in crate::storage) fn read64(raw: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
}
pub(in crate::storage) fn resign(raw: &mut [u8]) {
    let end = raw.len() - 32;
    let sha = content_digest(&raw[..end]);
    raw[end..].copy_from_slice(&sha);
}
pub(in crate::storage) fn frame_resign(raw: &mut [u8]) {
    let sha = content_digest(&raw[..128]);
    raw[128..160].copy_from_slice(&sha);
    resign(raw);
}
pub(in crate::storage) fn boxed<const N: usize>() -> Box<[u8; N]> {
    vec![0; N].into_boxed_slice().try_into().unwrap()
}
pub(in crate::storage) fn cell(raw: &[u8], slot: usize) -> &[u8] {
    let at = 128 + slot * 4;
    let start = u16::from_le_bytes(raw[at..at + 2].try_into().unwrap()) as usize;
    let len = u16::from_le_bytes(raw[at + 2..at + 4].try_into().unwrap()) as usize;
    &raw[start..start + len]
}
fn hex(raw: &[u8]) -> String {
    content_digest(raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Runtime reads only immutable generated literals. The executor binds each
/// vector index SHA separately; no source encoder is an expected-byte oracle.
pub(in crate::storage) struct Fixture {
    pub(in crate::storage) files: BTreeMap<String, Vec<u8>>,
    pub(in crate::storage) profile: Profile,
    pub(in crate::storage) ordinal: usize,
}
impl Fixture {
    pub(in crate::storage) fn new(profile: u32, ordinal: usize) -> Self {
        let vector = format!("p{profile}-o{ordinal:02}");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/dev3-plan4-empty-retention/vectors")
            .join(&vector);
        let text = std::env::var("XADB_G6_VECTOR_INDEXES").expect("exact vector index bindings");
        assert!(text.len() <= 16384, "bounded vector binding input");
        let mut bindings = BTreeMap::new();
        for pair in text.split(',') {
            let (k, v) = pair.split_once('=').expect("vector=sha");
            assert!(v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(bindings.insert(k, v).is_none());
        }
        let index_path = root.join("raw-index.tsv");
        assert!(std::fs::metadata(&index_path).unwrap().len() <= 16384);
        let index = std::fs::read(index_path).unwrap();
        assert_eq!(
            hex(&index),
            *bindings
                .get(vector.as_str())
                .expect("missing selected vector binding")
        );
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for row in std::str::from_utf8(&index).unwrap().lines().skip(1) {
            let fields: Vec<_> = row.split('\t').collect();
            assert_eq!(fields.len(), 3);
            let path = fields[0];
            assert!(!path.starts_with('/') && !path.split('/').any(|p| p == ".."));
            assert!(path.len() <= 128 && files.len() < 34);
            let length = fields[1].parse::<usize>().unwrap();
            assert!(length <= 56712);
            total = total.checked_add(length).unwrap();
            assert!(total <= 550160);
            let file_path = root.join(path);
            assert_eq!(std::fs::metadata(&file_path).unwrap().len(), length as u64);
            let bytes = std::fs::read(file_path).unwrap();
            assert_eq!(bytes.len(), length);
            assert_eq!(hex(&bytes), fields[2]);
            assert!(files.insert(path.to_owned(), bytes).is_none());
        }
        assert_eq!(files.len(), 34);
        assert_eq!(files.values().map(Vec::len).sum::<usize>(), 550160);
        let p = decode_bootstrap_profile(
            &files[&format!("references/REF-P{profile}-d3-profile.bin")],
            Some(namespace()),
        )
        .unwrap();
        Self {
            files,
            profile: p,
            ordinal,
        }
    }
    pub(in crate::storage) fn standard() -> Self {
        Self::new(1, 0)
    }
    pub(in crate::storage) fn raw(&self, name: &str) -> &[u8] {
        &self.files[&format!("raw/{name}")]
    }
    pub(in crate::storage) fn page(&self, name: &str) -> &[u8; PAGE_BYTES] {
        self.raw(name).try_into().unwrap()
    }
    pub(in crate::storage) fn descriptor(&self, name: &str) -> &[u8; 1536] {
        self.raw(name).try_into().unwrap()
    }
    pub(in crate::storage) fn reference(&self, number: usize) -> &[u8; PAGE_BYTES] {
        self.files[&format!("references/P{number:02}.bin")]
            .as_slice()
            .try_into()
            .unwrap()
    }
    pub(in crate::storage) fn job_ref(&self) -> JobSlotRef {
        JobSlotRef::new(
            page_ref(2 + (self.ordinal / 15) as u64, 1),
            (self.ordinal % 15) as u16,
            gen_value(2),
        )
        .unwrap()
    }
    pub(in crate::storage) fn cells(&self, phase: &str) -> Box<[[u8; JOB_BYTES]; JOB_COUNT]> {
        let mut cells: Box<[[u8; JOB_BYTES]; JOB_COUNT]> = vec![[0; JOB_BYTES]; JOB_COUNT]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        for (i, value) in cells.iter_mut().enumerate() {
            let page = 2 + i / 15;
            value.copy_from_slice(cell(self.reference(page), i % 15));
        }
        if !matches!(phase, "initial" | "start") {
            let raw = self.page(&format!("jobs-{phase}.page"));
            let first = self.ordinal / 15 * 15;
            let count = if self.ordinal / 15 == 4 { 4 } else { 15 };
            for (i, value) in cells.iter_mut().skip(first).take(count).enumerate() {
                value.copy_from_slice(cell(raw, i));
            }
        }
        cells
    }
    pub(in crate::storage) fn replace(&mut self, name: &str, bytes: Vec<u8>) {
        let previous = self.files.insert(format!("raw/{name}"), bytes);
        assert!(previous.is_some());
    }
    /// Independent hash binding after a context fault. State/counters/debt do
    /// not change here; only selected-page bytes and ordered census digest do.
    pub(in crate::storage) fn bind_cells(
        &mut self,
        phase: &str,
        cells: &[[u8; JOB_BYTES]; JOB_COUNT],
    ) {
        let root_name = format!("root-{phase}.page");
        let mut root = self.raw(&root_name).to_vec();
        let mut hash = ContentHasher::new();
        for c in cells {
            hash.update(c);
        }
        root[15968..16000].copy_from_slice(&hash.digest());
        resign(&mut root);
        let a_name = format!("aging-{phase}.descriptor");
        let mut aging = self.raw(&a_name).to_vec();
        aging[416..448].copy_from_slice(&content_digest(&root));
        aging[448..480].copy_from_slice(&content_digest(&root[15840..16352]));
        resign(&mut aging);
        self.replace(&root_name, root);
        self.replace(&a_name, aging);
        if phase != "start" {
            let j_name = format!("jobs-{phase}.page");
            let mut jobs = self.raw(&j_name).to_vec();
            let first = self.ordinal / 15 * 15;
            let count = if self.ordinal / 15 == 4 { 4 } else { 15 };
            for i in 0..count {
                let at = 16352 - 1024 * (i + 1);
                jobs[at..at + 1024].copy_from_slice(&cells[first + i]);
            }
            resign(&mut jobs);
            self.replace(&j_name, jobs);
        }
    }
}
pub(in crate::storage) fn root_spec(raw: &[u8]) -> RootSpec {
    RootSpec {
        aging: if raw[6] == 0 {
            None
        } else {
            Some(SystemWorkRef::new(namespace(), SystemWorkKind::Aging, 1, gen_value(1)).unwrap())
        },
        revision: read64(raw, 64),
        cursor: read64(raw, 72),
        apply: read64(raw, 80),
        issued: read64(raw, 88),
        free: read64(raw, 96),
        leased: read64(raw, 104),
        ready: read64(raw, 112),
        job_digest: raw[128..160].try_into().unwrap(),
        completed: read64(raw, 376),
    }
}
pub(in crate::storage) fn leased() -> JobSpec {
    JobSpec {
        generation: gen_value(2),
        state: JobState::Leased {
            source: actor(1, 2),
            status: status_ref(),
            lease_lsn: 4,
        },
    }
}

#[test]
fn retention_canonical_inputs() {
    cases("Q", 12, |n| {
        let f = Fixture::standard();
        match n {
            1 => {
                let v = decode_root(cell(f.reference(1), 0), namespace()).unwrap();
                assert!(v.spec().aging.is_none());
                assert_eq!(v.spec().revision, 1);
                assert_eq!(v.spec().free, u64::MAX);
            }
            2 => {
                let raw = cell(f.page("root-start.page"), 0);
                let mut out = [0; 512];
                encode_root(root_spec(raw), namespace(), &mut out).unwrap();
                assert_eq!(out.as_slice(), raw);
            }
            3 => {
                let (h, v) = decode_root_page(f.page("root-finish.page"), namespace()).unwrap();
                assert_eq!(
                    (
                        h.lsn.0,
                        v.spec().revision,
                        v.spec().cursor,
                        v.spec().completed
                    ),
                    (7, 5, 1, 1)
                );
                assert_eq!(v.spec().free, u64::MAX);
                assert_ne!(
                    v.spec().job_digest,
                    root_spec(cell(f.reference(1), 0)).job_digest
                );
            }
            4 => {
                let cells = f.cells("begin");
                let borrowed: Vec<_> = cells[..15].iter().map(|v| v.as_slice()).collect();
                let mut out = boxed::<PAGE_BYTES>();
                encode_jobs_page(namespace(), 0, 4, &borrowed, &mut *out).unwrap();
                assert_eq!(&*out, f.page("jobs-begin.page"));
            }
            5 => assert_eq!(
                decode_jobs_page(f.reference(6), namespace(), 4)
                    .unwrap()
                    .len(),
                4
            ),
            6 => assert_eq!(
                decode_job(cell(f.reference(2), 0), namespace())
                    .unwrap()
                    .spec(),
                JobSpec {
                    generation: gen_value(1),
                    state: JobState::Free
                }
            ),
            7 => {
                let v = decode_job(cell(f.page("jobs-begin.page"), 0), namespace()).unwrap();
                assert_eq!(v.spec(), leased());
                assert_eq!(v.last_step(), 4);
            }
            8 => {
                let v = decode_job(cell(f.page("jobs-abort.page"), 0), namespace()).unwrap();
                assert_eq!(
                    v.spec(),
                    JobSpec {
                        generation: gen_value(2),
                        state: JobState::Ready {
                            source: actor(1, 2),
                            status: status_ref(),
                            lease_lsn: 4,
                            handoff_lsn: 6,
                            sender_digest: content_digest(f.descriptor("tx-rollback.descriptor"))
                        }
                    }
                );
                assert_eq!(v.last_step(), 6);
            }
            9 => assert_eq!(
                decode_job(cell(f.page("jobs-finish.page"), 0), namespace())
                    .unwrap()
                    .spec(),
                JobSpec {
                    generation: gen_value(2),
                    state: JobState::Free
                }
            ),
            10 => {
                let r = JobSlotRef::new(page_ref(6, 1), 3, gen_value(2)).unwrap();
                let mut out = [0; 40];
                r.encode(&mut out).unwrap();
                let mut expected = [0; 40];
                for (at, v) in [(0, 0x100000001), (8, 6), (16, 1), (32, 2)] {
                    put64(&mut expected, at, v);
                }
                put16(&mut expected, 24, 3);
                assert_eq!(out, expected);
                assert_eq!(JobSlotRef::decode(&out).unwrap(), r);
                assert_eq!(r.ordinal().unwrap(), 63);
            }
            11 => {
                let (h, v) = decode_root_page(f.page("root-start.page"), namespace()).unwrap();
                assert_eq!(h.lsn.0, 3);
                assert_eq!(v.spec().aging.unwrap().kind(), SystemWorkKind::Aging);
                assert_eq!(v.spec().revision, 2);
            }
            12 => {
                let p = decode_jobs_page(f.page("jobs-begin.page"), namespace(), 0).unwrap();
                assert_eq!(p.len(), 15);
                for i in 1..15 {
                    assert_eq!(p.cell(i), Some(cell(f.reference(2), i)));
                }
            }
            _ => unreachable!(),
        }
    });
}

#[test]
fn retention_root_rejections() {
    cases("R", 14, |n| {
        let f = Fixture::standard();
        let mut raw = cell(f.page("root-begin.page"), 0).to_vec();
        let error = match n {
            1 => {
                raw[7] = 1;
                RetentionError::Corrupt
            }
            2 => {
                raw[511] = 1;
                RetentionError::Corrupt
            }
            3 => {
                put16(&mut raw, 4, 2);
                RetentionError::UnsupportedFormat
            }
            4 => {
                raw[6] = 2;
                RetentionError::UnsupportedState
            }
            5 => {
                put64(&mut raw, 72, 64);
                RetentionError::InvalidInput
            }
            6 => {
                put64(&mut raw, 96, u64::MAX);
                RetentionError::Corrupt
            }
            7 => {
                put64(&mut raw, 96, (u64::MAX ^ 1) & !(1 << 63));
                RetentionError::Corrupt
            }
            8 => {
                put64(&mut raw, 48, 2);
                RetentionError::IdentityMismatch
            }
            9 => {
                put64(&mut raw, 376, 1);
                RetentionError::OrderMismatch
            }
            10 => {
                put64(&mut raw, 64, 4);
                RetentionError::OrderMismatch
            }
            11 => {
                raw[160] ^= 1;
                RetentionError::Corrupt
            }
            12 => {
                let mut page = f.page("root-begin.page").to_vec();
                put64(&mut page, 64, 5);
                resign(&mut page);
                let before = page.clone();
                assert_eq!(
                    decode_root_page(&page, namespace()).err(),
                    Some(RetentionError::OrderMismatch)
                );
                assert_eq!(page, before);
                return;
            }
            13 | 14 => {
                let count = if n == 13 { 33 } else { 56 };
                let mask = (1u64 << count) - 1;
                put64(&mut raw, 96, !mask);
                put64(&mut raw, 104, if n == 13 { mask } else { 0 });
                put64(&mut raw, 112, if n == 14 { mask } else { 0 });
                put64(&mut raw, 88, count);
                put64(&mut raw, 64, if n == 13 { 35 } else { 114 });
                RetentionError::Corrupt
            }
            _ => unreachable!(),
        };
        let before = raw.clone();
        assert_eq!(decode_root(&raw, namespace()).err(), Some(error));
        assert_eq!(raw, before);
    });
}

#[test]
fn retention_job_rejections() {
    cases("J", 18, |n| {
        let f = Fixture::standard();
        if n == 17 {
            assert_eq!(
                JobSlotRef::new(page_ref(6, 1), 4, gen_value(2)).err(),
                Some(RetentionError::IdentityMismatch)
            );
            return;
        }
        if n == 18 {
            let mut out = [0xa5; 1024];
            let mut ns = namespace();
            ns.timeline = 0;
            assert_eq!(
                encode_job(leased(), ns, &mut out),
                Err(RetentionError::InvalidInput)
            );
            assert_eq!(out, [0xa5; 1024]);
            return;
        }
        let name = if matches!(n, 5 | 6 | 7 | 16) {
            "jobs-begin.page"
        } else if matches!(n, 14 | 15) {
            "jobs-finish.page"
        } else {
            "jobs-abort.page"
        };
        let mut raw = cell(f.page(name), 0).to_vec();
        let error = match n {
            1 => {
                put16(&mut raw, 4, 2);
                RetentionError::UnsupportedFormat
            }
            2 => {
                raw[6] = 3;
                RetentionError::UnsupportedState
            }
            3 => {
                raw[122] = 0;
                RetentionError::Corrupt
            }
            4 => {
                raw[7] = 2;
                RetentionError::Corrupt
            }
            5 => {
                put64(&mut raw, 88, 8);
                RetentionError::IdentityMismatch
            }
            6 => {
                put64(&mut raw, 56, 3);
                RetentionError::IdentityMismatch
            }
            7 => {
                put64(&mut raw, 72, 1);
                RetentionError::Corrupt
            }
            8 => {
                raw[127] = 1;
                RetentionError::Corrupt
            }
            9 => {
                raw[136] = 1;
                RetentionError::Corrupt
            }
            10 => {
                raw[120] = 1;
                RetentionError::Corrupt
            }
            11 => {
                raw[121] = 4;
                RetentionError::Corrupt
            }
            12 => {
                put64(&mut raw, 72, 4);
                RetentionError::OrderMismatch
            }
            13 => {
                put64(&mut raw, 776, 0);
                RetentionError::Corrupt
            }
            14 => {
                put64(&mut raw, 8, 0);
                RetentionError::Corrupt
            }
            15 => {
                raw[16] = 1;
                RetentionError::Corrupt
            }
            16 => {
                put64(&mut raw, 8, 1);
                RetentionError::OrderMismatch
            }
            _ => unreachable!(),
        };
        let before = raw.clone();
        assert_eq!(decode_job(&raw, namespace()).err(), Some(error));
        assert_eq!(raw, before);
    });
}

#[test]
fn retention_census_inputs() {
    cases("C", 8, |n| {
        let f = Fixture::standard();
        let mut cells = f.cells("begin");
        let mut root = cell(f.page("root-begin.page"), 0).to_vec();
        if matches!(n, 2..=4) {
            cells[1] = cells[0];
            if n == 2 || n == 3 {
                put64(&mut cells[1], 88, 10);
            }
            if n == 3 {
                put64(&mut cells[1], 56, 3);
                put64(&mut cells[1], 112, 3);
            }
            if n == 4 {
                put64(&mut cells[1], 48, 2);
                put64(&mut cells[1], 96, 3);
            }
            put64(&mut root, 64, 4);
            put64(&mut root, 88, 2);
            put64(&mut root, 96, !3);
            put64(&mut root, 104, 3);
            let mut budget: Vec<_> = cells[0][384..].to_vec();
            put64(&mut budget, 392, 2);
            put64(&mut budget, 400, 110224);
            put64(&mut budget, 496, 110224);
            root[160..192].copy_from_slice(&content_digest(&budget));
            let mut hash = ContentHasher::new();
            for c in cells.iter() {
                hash.update(c);
            }
            root[128..160].copy_from_slice(&hash.digest());
        }
        match n {
            5 => cells.swap(0, 1),
            6 => cells[0].copy_from_slice(cell(f.reference(2), 0)),
            7 => put64(&mut cells[63], 8, 2),
            8 => {
                put64(&mut cells[0], 64, 5);
                put64(&mut cells[0], 376, 5);
                let mut hash = ContentHasher::new();
                for c in cells.iter() {
                    hash.update(c);
                }
                root[128..160].copy_from_slice(&hash.digest());
            }
            _ => {}
        }
        let view = decode_root(&root, namespace()).unwrap();
        let result = verify_census(view, namespace(), &cells);
        if n == 1 {
            assert_eq!(result, Ok(()));
        } else {
            assert_eq!(
                result,
                Err(if matches!(n, 2..=4) {
                    RetentionError::IdentityMismatch
                } else if n == 8 {
                    RetentionError::OrderMismatch
                } else {
                    RetentionError::Corrupt
                })
            );
        }
    });
}

#[test]
#[ignore]
fn retention_s5_generations() {
    let owned: Vec<_> = ["LEASED", "READY", "FREE"]
        .into_iter()
        .flat_map(|s| ["MAXM1", "MAX"].map(move |v| format!("G6-SG-{s}-{v}")))
        .collect();
    let mut h = Harness::new(owned.clone());
    for id in owned {
        h.case(&id, || {
            let f = Fixture::standard();
            let name = if id.contains("LEASED") {
                "jobs-begin.page"
            } else if id.contains("READY") {
                "jobs-abort.page"
            } else {
                "jobs-finish.page"
            };
            let mut raw = cell(f.page(name), 0).to_vec();
            let generation = if id.ends_with("MAXM1") {
                u64::MAX - 1
            } else {
                u64::MAX
            };
            put64(&mut raw, 8, generation);
            assert_eq!(
                decode_job(&raw, namespace())
                    .unwrap()
                    .spec()
                    .generation
                    .get(),
                generation
            );
        });
    }
    h.finish();
}
#[test]
#[ignore]
fn retention_s5_root_reserved() {
    let owned: Vec<_> = (384..=510).map(|i| format!("G6-SR-{i}")).collect();
    let mut h = Harness::new(owned.clone());
    for (i, id) in owned.iter().enumerate() {
        h.case(id, || {
            let f = Fixture::standard();
            let mut raw = cell(f.page("root-begin.page"), 0).to_vec();
            raw[384 + i] = 1;
            assert_eq!(
                decode_root(&raw, namespace()).err(),
                Some(RetentionError::Corrupt)
            );
        });
    }
    h.finish();
}
#[test]
#[ignore]
fn retention_s5_job_reserved() {
    let owned: Vec<_> = (123..=126).map(|i| format!("G6-SJ-{i}")).collect();
    let mut h = Harness::new(owned.clone());
    for (i, id) in owned.iter().enumerate() {
        h.case(id, || {
            let f = Fixture::standard();
            let mut raw = cell(f.page("jobs-abort.page"), 0).to_vec();
            raw[123 + i] = 1;
            assert_eq!(
                decode_job(&raw, namespace()).err(),
                Some(RetentionError::Corrupt)
            );
        });
    }
    h.finish();
}
