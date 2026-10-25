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
            .join("../../tests/fixtures/dev3-plan4-empty-retention/vectors/p1-o00");
        let index = std::fs::read(root.join("raw-index.tsv")).unwrap();
        assert_eq!(
            hex(&index),
            "ce3fd367007a6d798dbb6c2f18367a714d4f105611fb190fe57a1aa72d6d99a8"
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
            assert!(total <= 550_160);
            assert!(files.insert(fields[0].to_owned(), bytes).is_none());
        }
        assert_eq!((files.len(), total), (34, 550_160));
        Self { files }
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
fn actor(epoch: u32, generation: u64) -> [u8; 48] {
    let mut bytes = [0; 48];
    for (i, byte) in bytes[..16].iter_mut().enumerate() {
        *byte = i as u8 + 1;
    }
    put(&mut bytes, 16, 1, 8);
    put(&mut bytes, 24, u64::from(epoch), 4);
    put(&mut bytes, 32, 1, 8);
    put(&mut bytes, 40, generation, 8);
    bytes
}
fn page_with(template: &[u8], lsn: u64, at: usize, cell: &[u8]) -> Vec<u8> {
    let mut out = template.to_vec();
    put(&mut out, 64, lsn, 8);
    out[at..at + cell.len()].copy_from_slice(cell);
    seal(&mut out);
    out
}
fn native(kind: u8, index: u16, before: &[u8], after: &[u8]) -> Vec<u8> {
    assert_eq!(before.len(), after.len());
    let mut out = vec![0; 128 + before.len() + after.len()];
    out[..2].copy_from_slice(&[kind, 3]);
    put(&mut out, 2, u64::from(index), 2);
    for at in [8, 10] {
        put(&mut out, at, (16_352 - before.len()) as u64, 2);
    }
    for at in [12, 16] {
        put(&mut out, at, before.len() as u64, 4);
    }
    let len = out.len();
    put(&mut out, 20, len as u64, 4);
    out[24..56].copy_from_slice(&content_digest(before));
    out[56..88].copy_from_slice(&content_digest(after));
    out[128..128 + before.len()].copy_from_slice(before);
    out[128 + before.len()..].copy_from_slice(after);
    out
}
fn patch(before: &[u8], after: &[u8], narrow: bool) -> Vec<u8> {
    let (offset, len) = if narrow { (16_128, 224) } else { (72, 16_280) };
    let mut out = vec![0; 64 + len];
    out[..2].copy_from_slice(&[1, before[6]]);
    put(&mut out, 4, (64 + len) as u64, 4);
    out[8..32].copy_from_slice(&before[24..48]);
    out[32..56].copy_from_slice(&before[48..72]);
    put(&mut out, 56, offset as u64, 2);
    put(&mut out, 58, len as u64, 2);
    out[64..].copy_from_slice(&after[offset..offset + len]);
    out
}
fn envelope(
    mut header: Vec<u8>,
    payload: &[u8],
    lsn: u64,
    owner_previous: u64,
    transaction: bool,
    actor: &[u8; 48],
) -> Vec<u8> {
    assert_eq!(header.len(), 160);
    put(&mut header, 48, 2, 4);
    put(&mut header, 56, lsn, 8);
    put(&mut header, 64, lsn - 1, 8);
    if transaction {
        header[72..112].copy_from_slice(&actor[..40]);
        put(&mut header, 112, owner_previous, 8);
    }
    let header_sha = content_digest(&header[..128]);
    header[128..160].copy_from_slice(&header_sha);
    header.extend_from_slice(payload);
    header.extend_from_slice(&[0; 32]);
    seal(&mut header);
    header
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
    /// Exact epoch2/local1, reused Status/Job generation3, birth8. The
    /// independent first-cycle templates supply only fixed grammar/layout.
    pub(super) fn new(o: &Oracle) -> Self {
        let actor = actor(2, 3);
        let phases = ["begin", "rollback", "abort", "finish"];
        let mut homes = Vec::new();
        let mut statuses = Vec::new();
        let mut tx = Vec::new();
        for (i, phase) in phases.iter().enumerate() {
            let mut home = o.page(&format!("status-{phase}.page"))[16_128..16_352].to_vec();
            if i == 3 {
                put(&mut home, 80, 3, 8);
            } else {
                home[40..88].copy_from_slice(&actor);
            }
            let status = page_with(
                o.page(&format!("status-{phase}.page")),
                8 + i as u64,
                16_128,
                &home,
            );
            if i < 3 {
                let mut descriptor = o.raw(&format!("tx-{phase}.descriptor")).to_vec();
                descriptor[24..72].copy_from_slice(&actor);
                descriptor[160..352].copy_from_slice(&home[32..]);
                for (at, value) in [
                    (72, 8 + i as u64),
                    (80, if i == 0 { 0 } else { 7 + i as u64 }),
                    (88, 8),
                    (544, 8),
                    (1336, 8),
                ] {
                    put(&mut descriptor, at, value, 8);
                }
                seal(&mut descriptor);
                tx.push(descriptor);
            }
            homes.push(home);
            statuses.push(status);
        }
        let mut cells = vec![o.genesis(2)[15_328..16_352].to_vec(); 64];
        let mut roots = Vec::new();
        let mut jobs = Vec::new();
        let mut aging = Vec::new();
        let mut job_cells = Vec::new();
        for (i, phase) in ["begin", "abort", "finish"].iter().enumerate() {
            let mut job = o.page(&format!("jobs-{phase}.page"))[15_328..16_352].to_vec();
            put(&mut job, 8, 3, 8);
            if i < 2 {
                job[16..64].copy_from_slice(&actor);
                put(&mut job, 64, 8, 8);
                put(&mut job, 112, 3, 8);
                put(&mut job, 376, if i == 0 { 8 } else { 10 }, 8);
                if i == 1 {
                    put(&mut job, 72, 10, 8);
                    job[328..360].copy_from_slice(&content_digest(&tx[1]));
                }
            }
            cells[0] = job.clone();
            let mut root = o.page(&format!("root-{phase}.page"))[15_840..16_352].to_vec();
            let lsn = [8, 10, 11][i];
            put(&mut root, 64, 6 + i as u64, 8);
            put(&mut root, 72, 1, 8);
            put(&mut root, 80, lsn, 8);
            put(&mut root, 88, 2, 8);
            put(&mut root, 376, if i == 2 { 2 } else { 1 }, 8);
            root[128..160].copy_from_slice(&content_digest(&cells.concat()));
            let root_page = page_with(o.page(&format!("root-{phase}.page")), lsn, 15_840, &root);
            let jobs_page = page_with(o.page(&format!("jobs-{phase}.page")), lsn, 15_328, &job);
            let mut descriptor = o.raw(&format!("aging-{phase}.descriptor")).to_vec();
            for (at, value) in [
                (72, lsn),
                (80, [7, 8, 10][i]),
                (360, 6 + i as u64),
                (368, 1),
                (392, 6 + i as u64),
                (608, 74_784 + if i == 2 { 55_112 } else { 0 }),
                (624, 5 + i as u64),
            ] {
                put(&mut descriptor, at, value, 8);
            }
            descriptor[416..448].copy_from_slice(&content_digest(&root_page));
            descriptor[448..480].copy_from_slice(&content_digest(&root));
            seal(&mut descriptor);
            roots.push(root_page);
            jobs.push(jobs_page);
            aging.push(descriptor);
            job_cells.push(job);
        }
        let root_before = [
            o.page("root-finish.page").as_slice(),
            roots[0].as_slice(),
            roots[1].as_slice(),
        ];
        let jobs_before = [
            o.page("jobs-finish.page").as_slice(),
            jobs[0].as_slice(),
            jobs[1].as_slice(),
        ];
        let status_before = [
            o.page("status-finish.page").as_slice(),
            statuses[1].as_slice(),
            statuses[2].as_slice(),
        ];
        let aging_before = [
            o.raw("aging-finish.descriptor"),
            aging[0].as_slice(),
            aging[1].as_slice(),
        ];
        let mut frames = Vec::new();
        for i in 0..4 {
            if i == 1 {
                let template = o.raw("rollback.wal");
                let mut group = template[160..352].to_vec();
                put(&mut group, 16, 9, 8);
                put(&mut group, 24, 8, 8);
                group[40..88].copy_from_slice(&actor);
                group[88..120].copy_from_slice(&content_digest(&tx[0]));
                group.extend_from_slice(&tx[1]);
                group.extend_from_slice(&template[1888..2144]);
                group.extend_from_slice(&native(5, 0, &homes[0], &homes[1]));
                group.extend_from_slice(&patch(&statuses[0], &statuses[1], true));
                frames.push(envelope(
                    template[..160].to_vec(),
                    &group,
                    9,
                    8,
                    true,
                    &actor,
                ));
                continue;
            }
            let j = if i == 0 { 0 } else { i - 1 };
            let phase = phases[i];
            let template = o.raw(&format!("{phase}.wal"));
            let op_at = if i == 3 { 1888 } else { 3488 };
            let mut operation = template[op_at..op_at + 256].to_vec();
            if i == 3 {
                put(&mut operation, 16, 7, 8);
                put(&mut operation, 24, 8, 8);
                put(&mut operation, 64, 3, 8);
                put(&mut operation, 104, 3, 8);
                operation[112..160].copy_from_slice(&actor);
                operation[160..192]
                    .copy_from_slice(&content_digest(&root_before[j][15_840..16_352]));
                let mut before_cells = vec![o.genesis(2)[15_328..16_352].to_vec(); 64];
                before_cells[0] = job_cells[1].clone();
                operation[192..224].copy_from_slice(&content_digest(&before_cells.concat()));
            } else {
                put(&mut operation, 56, 3, 8);
                put(&mut operation, 96, 3, 8);
            }
            let mut group = template[160..if i == 3 { 352 } else { 416 }].to_vec();
            if i == 3 {
                put(&mut group, 16, 11, 8);
                put(&mut group, 24, 10, 8);
                group[88..120].copy_from_slice(&content_digest(aging_before[j]));
                group.extend_from_slice(&aging[j]);
            } else {
                put(&mut group, 16, 8 + i as u64, 8);
                put(&mut group, 24, if i == 0 { 0 } else { 9 }, 8);
                put(&mut group, 32, if i == 0 { 7 } else { 8 }, 8);
                group[48..96].copy_from_slice(&actor);
                if i == 2 {
                    group[152..184].copy_from_slice(&content_digest(&tx[1]));
                }
                group[184..216].copy_from_slice(&content_digest(aging_before[j]));
                group.extend_from_slice(&tx[i]);
                group.extend_from_slice(&aging[j]);
            }
            group.extend_from_slice(&operation);
            group.extend_from_slice(&native(
                8,
                0,
                &root_before[j][15_840..16_352],
                &roots[j][15_840..16_352],
            ));
            group.extend_from_slice(&native(
                9,
                1,
                &jobs_before[j][15_328..16_352],
                &job_cells[j],
            ));
            group.extend_from_slice(&native(5, 2, &status_before[j][16_128..16_352], &homes[i]));
            group.extend_from_slice(&patch(root_before[j], &roots[j], false));
            group.extend_from_slice(&patch(jobs_before[j], &jobs[j], false));
            group.extend_from_slice(&patch(status_before[j], &statuses[i], false));
            frames.push(envelope(
                template[..160].to_vec(),
                &group,
                8 + i as u64,
                if i == 0 {
                    0
                } else if i == 2 {
                    9
                } else {
                    0
                },
                i != 3,
                &actor,
            ));
        }
        assert_eq!(
            frames.iter().map(Vec::len).collect::<Vec<_>>(),
            [56_712, 3_040, 56_712, 55_112]
        );
        Self {
            frames,
            roots,
            jobs,
            statuses,
            tx,
            aging,
        }
    }
}
