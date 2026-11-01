//! Independent recovery expectations. Existing committed literals are pinned;
//! new epoch/second-use values use explicit wire fields and SHA only. No
//! product encoder, decoder, transition, or completed test is an oracle.
use super::*;
use std::{collections::BTreeMap, path::PathBuf};

#[allow(
    clippy::duplicate_mod,
    reason = "literal recipe binds to this module's distinct recovery Oracle type"
)]
#[path = "../../plan4_retention/literal_tests.rs"]
mod population;

pub(super) struct Oracle {
    files: BTreeMap<String, Vec<u8>>,
}
impl Oracle {
    pub(super) fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/dev3-plan4-empty-retention/current3");
        let index = std::fs::read(root.join("raw-index.tsv")).unwrap();
        assert_eq!(
            hex(&index),
            "3b9e3ac52864e5d1e6c46071a16936c2a1a8cd44386691c96022494c65851740"
        );
        let mut files = BTreeMap::new();
        let mut total = 0;
        for line in std::str::from_utf8(&index).unwrap().lines().skip(1) {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(fields.len(), 3);
            assert!(!fields[0].starts_with('/') && !fields[0].split('/').any(|s| s == ".."));
            let bytes = std::fs::read(root.join(fields[0])).unwrap();
            assert_eq!(bytes.len(), fields[1].parse::<usize>().unwrap());
            assert_eq!(hex(&bytes), fields[2]);
            total += bytes.len();
            assert!(total <= 1_028_952);
            assert!(files.insert(fields[0].to_owned(), bytes).is_none());
        }
        assert_eq!((files.len(), total), (63, 1_028_952));
        Self { files }
    }
    pub(super) fn profile(&self) -> &[u8] {
        &self.files["references/REF-P1-d3-profile.bin"]
    }
    pub(super) fn raw(&self, name: &str) -> &[u8] {
        &self.files[&format!("raw/{name}")]
    }
    pub(super) fn page(&self, name: &str) -> &[u8; PAGE_BYTES] {
        self.raw(name).try_into().unwrap()
    }
    pub(super) fn genesis(&self, number: usize) -> &[u8; PAGE_BYTES] {
        self.files[&format!("references/P{number:02}.bin")]
            .as_slice()
            .try_into()
            .unwrap()
    }
    pub(super) fn allocation(&self, terminal: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let reserve = population::population(self, self.genesis(0), 1, 1, 0, &[]);
        let result = if terminal == 0 {
            reserve
        } else {
            population::population(
                self,
                &reserve.allocator,
                1,
                1,
                terminal,
                &reserve.descriptor,
            )
        };
        (result.frame, result.allocator, result.descriptor)
    }
    pub(super) fn cleanup(&self, name: &str, epoch: u32) -> Vec<u8> {
        let mut out = self.raw(name).to_vec();
        put(&mut out, 48, u64::from(epoch), 4);
        let header = content_digest(&out[..128]);
        out[128..160].copy_from_slice(&header);
        seal(&mut out);
        out
    }
}
pub(super) fn hex(bytes: &[u8]) -> String {
    content_digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub(super) fn put(out: &mut [u8], at: usize, value: u64, width: usize) {
    out[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}
pub(super) fn seal(out: &mut [u8]) {
    let at = out.len() - 32;
    let digest = content_digest(&out[..at]);
    out[at..].copy_from_slice(&digest);
}
pub(super) struct SecondCycle {
    pub(super) frames: Vec<Vec<u8>>,
    pub(super) roots: Vec<Vec<u8>>,
    pub(super) jobs: Vec<Vec<u8>>,
    pub(super) statuses: Vec<Vec<u8>>,
    pub(super) tx: Vec<Vec<u8>>,
    pub(super) aging: Vec<Vec<u8>>,
}
impl SecondCycle {
    /// Independently generated current-format epoch2/local1 history.
    pub(super) fn new(o: &Oracle) -> Self {
        Self {
            frames: ["begin", "rollback", "abort", "finish"]
                .map(|p| o.raw(&format!("second-{p}.wal")).to_vec())
                .to_vec(),
            roots: ["begin", "abort", "finish"]
                .map(|p| o.raw(&format!("second-root-{p}.page")).to_vec())
                .to_vec(),
            jobs: ["begin", "abort", "finish"]
                .map(|p| o.raw(&format!("second-jobs-{p}.page")).to_vec())
                .to_vec(),
            statuses: ["begin", "rollback", "abort", "finish"]
                .map(|p| o.raw(&format!("second-status-{p}.page")).to_vec())
                .to_vec(),
            tx: ["begin", "rollback", "abort"]
                .map(|p| o.raw(&format!("second-tx-{p}.descriptor")).to_vec())
                .to_vec(),
            aging: ["begin", "abort", "finish"]
                .map(|p| o.raw(&format!("second-aging-{p}.descriptor")).to_vec())
                .to_vec(),
        }
    }
}
