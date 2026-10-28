//! New local leaf semantics and same-frame integration; no prior tests invoked.
use super::{leaf::*, *};
use crate::storage::{
    catalog_record::{self, CatalogDefinitionView, CatalogExpectation},
    control,
    page::{PAGE_BYTES, PageRef},
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        row::{self, JobCells},
    },
    row_record::{self, RecordExpectation, RowSchemaInput},
    transaction_record::{StatusSlotRef, TxnRef},
    undo_ref::UndoRef,
    wal_frame::{FrameExpectation, WalStreamIdentity},
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation, ObjectHandle, ObjectId, RowId, SchemaEpoch},
};
const INPUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/leaf/cases.bin"
));
const INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/leaf/index.tsv"
));
const PROFILES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
struct Case {
    frame: Vec<u8>,
    jobs: Box<[[u8; JOB_BYTES]; JOB_COUNT]>,
    pages: Box<[[u8; PAGE_BYTES]; 8]>,
    catalog: Vec<u8>,
    canonical: Vec<u8>,
    context: undo::UndoContext,
    table: ObjectHandle,
    anchor: PageRef,
    has_h: bool,
    sealed: Option<SealedValue>,
    commit: Option<CommitSeq>,
}
fn put(b: &mut [u8], at: usize, v: u64, width: usize) {
    b[at..at + width].copy_from_slice(&v.to_le_bytes()[..width]);
}
fn reference(n: u64) -> UndoRef {
    let mut b = [0; 40];
    for (at, v) in [(0, 0x100000001), (8, n), (16, 2), (32, 2)] {
        put(&mut b, at, v, 8);
    }
    UndoRef::decode(&b).unwrap()
}
fn case(name: &str) -> Case {
    let f: Vec<_> = INDEX
        .lines()
        .find(|l| l.split('\t').next() == Some(name))
        .unwrap()
        .split('\t')
        .collect();
    let at: usize = f[1].parse().unwrap();
    let fl: usize = f[2].parse().unwrap();
    let cl: usize = f[3].parse().unwrap();
    let vl: usize = f[4].parse().unwrap();
    let op: u16 = f[5].parse().unwrap();
    let b = &INPUT[at..at + fl + 65536 + 8 * PAGE_BYTES + cl + vl];
    let profile = control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap();
    let mut jobs = Box::new([[0; JOB_BYTES]; JOB_COUNT]);
    for (i, j) in jobs.iter_mut().enumerate() {
        j.copy_from_slice(&b[fl + i * 1024..fl + (i + 1) * 1024]);
    }
    let actor = TxnRef::decode(&jobs[1][16..64]).unwrap();
    let mut loc = [0; 40];
    reference(9).encode(&mut loc).unwrap();
    let mut pages = Box::new([[0; PAGE_BYTES]; 8]);
    for (i, p) in pages.iter_mut().enumerate() {
        p.copy_from_slice(&b[fl + 65536 + i * PAGE_BYTES..fl + 65536 + (i + 1) * PAGE_BYTES]);
    }
    let cp = fl + 65536 + 8 * PAGE_BYTES;
    let canonical = b[cp + cl..].to_vec();
    Case {
        frame: b[..fl].to_vec(),
        jobs,
        pages,
        catalog: b[cp..cp + cl].to_vec(),
        sealed: (op == 2).then(|| SealedValue {
            length: canonical.len(),
            digest: content_digest(&canonical),
        }),
        canonical,
        context: undo::UndoContext {
            profile,
            actor,
            location: StatusSlotRef::decode(&loc).unwrap(),
        },
        table: ObjectHandle {
            object: ObjectId::new(6).unwrap(),
            generation: Generation::new(1).unwrap(),
            schema_epoch: SchemaEpoch::new(1).unwrap(),
        },
        anchor: reference(10).page(),
        has_h: f[6] == "1",
        commit: (op == 20).then(|| CommitSeq::new(9).unwrap()),
    }
}
impl Case {
    fn layout(&self) -> LayoutView<'_> {
        decode_layout(
            &self.frame,
            FrameExpectation {
                stream: WalStreamIdentity {
                    database: self.context.profile.spec().namespace.database,
                    timeline: 1,
                    writer_epoch: 1,
                },
                previous_lsn: 100,
            },
            self.context.profile,
        )
        .expect("mutant passes wire")
    }
    fn inputs(&self) -> Inputs<'_> {
        Inputs {
            context: self.context,
            jobs: JobCells::original(&self.jobs),
            ordinal: 1,
            undo: undo::UndoImages {
                before: &self.pages[0],
                after: &self.pages[1],
                predecessor: self.has_h.then_some(&self.pages[2]),
            },
            status: undo::StatusProof {
                statement: (self.layout().operation() == Operation::Forward).then(|| {
                    status::StatementBoundary {
                        sequence: 3,
                        mark: self.has_h.then(|| reference(13)),
                    }
                }),
                commit: self.commit,
                images: status::StatusImages {
                    before: &self.pages[3],
                    after: &self.pages[4],
                },
            },
            table: TableInput {
                handle: self.table,
                anchor: self.anchor,
                catalog: &self.catalog,
                anchor_image: &self.pages[5],
            },
            leaf: LeafImages {
                before: &self.pages[6],
                after: &self.pages[7],
            },
            sealed: self.sealed,
        }
    }
    fn observe(&self) -> Result<LeafObservation> {
        leaf::observe(self.layout(), self.inputs())
    }
    fn core_at(&self) -> usize {
        if self.frame[7] == 3 { 3488 } else { 1888 }
    }
    fn offsets(&self, kind: u8) -> (usize, usize) {
        let layout = self.layout();
        let native = layout.native_records().find(|n| n.kind() == kind).unwrap();
        let mut na = self.core_at() + 256;
        let mut ra = self.frame.len() - 32 - layout.redo().len();
        for _ in 0..native.redo_index() {
            na += u32_at(&self.frame, na + 20) as usize;
            ra += u32_at(&self.frame, ra + 4) as usize;
        }
        (na, ra)
    }
    fn seal_frame(&mut self) {
        let h = content_digest(&self.frame[..128]);
        self.frame[128..160].copy_from_slice(&h);
        let end = self.frame.len() - 32;
        let h = content_digest(&self.frame[..end]);
        self.frame[end..].copy_from_slice(&h);
    }
    fn cell_range(&self, p: usize, s: usize) -> std::ops::Range<usize> {
        let a = u16_at(&self.pages[p], 128 + 4 * s) as usize;
        let len = u16_at(&self.pages[p], 130 + 4 * s) as usize;
        a..a + len
    }
    fn selected_slot(&self, p: usize) -> Option<usize> {
        (1..u16_at(&self.pages[p], 72) as usize).find(|&s| {
            let r = self.cell_range(p, s);
            self.pages[p][r.start + 4..r.start + 12] == 3u64.to_be_bytes()
        })
    }
    fn record_at(&self, p: usize) -> usize {
        self.cell_range(p, self.selected_slot(p).unwrap()).start + 12
    }
    fn row_projection(&self, p: usize) -> [u8; 32] {
        let mut b = vec![0; 48];
        b[..8].copy_from_slice(b"XRP1\x01\0\x01\0");
        b[8] = 1;
        for (at, v) in [(16, 6), (24, 1), (32, 3)] {
            put(&mut b, at, v, 8);
        }
        if let Some(s) = self.selected_slot(p) {
            b[40] = 1;
            let r = self.cell_range(p, s);
            b.extend_from_slice(&self.pages[p][r.start + 12..r.end]);
        }
        content_digest(&b)
    }
    // Rebind raw native/redo bytes so semantic negatives reach the new guards.
    // Does not alter expected sealed canonical input; projection opt-in below.
    fn bind(&mut self, projections: bool) {
        let operation = self.layout().operation();
        if projections {
            let ca = self.core_at();
            let b = self.row_projection(6);
            let a = self.row_projection(7);
            self.frame[ca + 176..ca + 208].copy_from_slice(&b);
            self.frame[ca + 208..ca + 240].copy_from_slice(&a);
            let original = if operation == Operation::Forward {
                a
            } else {
                b
            };
            self.jobs[1][288..320].copy_from_slice(&original);
            self.seal_frame();
        }
        for kind in [1, 3, 5, 9] {
            let (na, ra) = self.offsets(kind);
            let (bi, ai) = match kind {
                1 => (6, 7),
                3 => (0, 1),
                5 => (3, 4),
                9 => (0, 0),
                _ => unreachable!(),
            };
            let mut cursor = na + 128;
            for (p, sl, of, len_at, dig) in [(bi, 4, 8, 12, 24), (ai, 6, 10, 16, 56)] {
                let len = u32_at(&self.frame, na + len_at) as usize;
                if len == 0 {
                    continue;
                }
                if kind == 9 {
                    if sl == 4 {
                        self.frame[cursor..cursor + len].copy_from_slice(&self.jobs[1]);
                        self.frame[na + dig..na + dig + 32]
                            .copy_from_slice(&content_digest(&self.jobs[1]));
                    }
                } else {
                    let slot = if kind == 1 {
                        self.selected_slot(p).unwrap()
                    } else {
                        0
                    };
                    let range = self.cell_range(p, slot);
                    assert_eq!(range.len(), len);
                    put(&mut self.frame, na + sl, slot as u64, 2);
                    put(&mut self.frame, na + of, range.start as u64, 2);
                    self.frame[cursor..cursor + len].copy_from_slice(&self.pages[p][range.clone()]);
                    self.frame[na + dig..na + dig + 32]
                        .copy_from_slice(&content_digest(&self.pages[p][range]));
                }
                cursor += len;
            }
            if kind != 9 {
                let len = u32_at(&self.frame, ra + 4) as usize;
                self.frame[ra + 64..ra + len].copy_from_slice(&self.pages[ai][72..16352]);
            }
            self.seal_frame();
        }
        for page in self.pages.iter_mut() {
            if page[..4] == *b"XDP1" {
                let h = content_digest(&page[..16352]);
                page[16352..].copy_from_slice(&h);
            }
        }
        self.seal_frame();
    }
    fn canonical_selected(&self, p: usize) {
        let ns = self.context.profile.spec().namespace;
        let cat = catalog_record::decode_catalog_record(
            &self.catalog,
            CatalogExpectation {
                namespace: ns,
                object: self.table.object,
            },
        )
        .unwrap();
        let CatalogDefinitionView::Table(schema) = cat.definition() else {
            panic!("table")
        };
        let range = self.cell_range(p, self.selected_slot(p).unwrap());
        row_record::decode_row_record_with_schema(
            &self.pages[p][range.start + 12..range.end],
            RecordExpectation {
                namespace: ns,
                row: RowId::new(3).unwrap(),
            },
            RowSchemaInput::Catalog(schema),
        )
        .expect("semantic mutant passes canonical row decoder");
    }
}
fn reject(name: &str, edit: impl FnOnce(&mut Case)) {
    let mut c = case(name);
    edit(&mut c);
    assert!(c.observe().is_err(), "accepted {name} mutant");
}
#[test]
fn leaf_fifteen_raw_pairs_and_current_anchor() {
    assert_eq!(INDEX.lines().count(), 15);
    for line in INDEX.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let at: usize = f[1].parse().unwrap();
        let fl: usize = f[2].parse().unwrap();
        let cl: usize = f[3].parse().unwrap();
        let vl: usize = f[4].parse().unwrap();
        assert_eq!(
            content_digest(&INPUT[at..at + fl + 65536 + 8 * PAGE_BYTES + cl + vl])
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            f[7]
        );
        let c = case(f[0]);
        let result = c.observe().unwrap_or_else(|e| panic!("{} {e:?}", f[0]));
        assert_eq!(result.row().get(), 3);
        assert_eq!(result.page().page_no, 20);
        assert_eq!(result.undo().reference(), reference(14));
        assert_eq!(
            result.present(),
            match f[5] {
                "2" => (false, true),
                "4" => (true, false),
                _ => (true, true),
            }
        );
        assert_eq!(
            row::decode_job(&c.jobs[1], c.context.profile.spec().namespace)
                .unwrap()
                .pair()
                .unwrap()[1]
                .page_no,
            11
        );
        assert_eq!(c.canonical.len(), vl);
    }
}
#[test]
fn leaf_table_identity_anchor_and_physical_alias() {
    for (at, value) in [(8, 7), (16, 2), (24, 2)] {
        reject("forward-middle", |c| {
            put(&mut c.catalog, at, value, 8);
        });
    }
    reject("forward-middle", |c| c.catalog[7] = 1);
    reject("forward-middle", |c| {
        c.table.schema_epoch = SchemaEpoch::new(2).unwrap()
    });
    reject("forward-middle", |c| {
        put(&mut c.catalog, 184, 16, 8);
    });
    for number in [9, 13, 14] {
        reject("forward-middle", |c| {
            c.anchor = PageRef {
                page_no: number,
                ..c.anchor
            };
            put(&mut c.catalog, 184, number, 8);
            put(&mut c.jobs[1], 208, number, 8);
            put(&mut c.pages[5], 32, number, 8);
            c.bind(false);
        });
        reject("forward-middle", |c| {
            let a = c.cell_range(5, 0).start;
            put(&mut c.pages[5], a + 16, number, 8);
            put(&mut c.pages[5], a + 24, 3, 8);
            c.bind(false);
        });
    }
    for (p, at, value) in [
        (5, 64, 0),
        (5, 64, 101),
        (6, 64, 0),
        (6, 64, 101),
        (7, 64, 100),
        (6, 48, 7),
        (7, 56, 2),
    ] {
        reject("forward-middle", |c| {
            put(&mut c.pages[p], at, value, 8);
            c.bind(false);
        });
    }
    reject("forward-middle", |c| {
        let a = c.cell_range(5, 0).start;
        c.pages[5][a + 6] = 3;
        c.bind(false);
    });
}
#[test]
fn leaf_native_slots_offsets_bytes_redo_and_lsn() {
    for name in ["forward-middle", "clr-middle", "prune-middle"] {
        let probe = case(name);
        let (na, ra) = probe.offsets(1);
        for (at, value, width) in [
            (na + if name.starts_with("forward") { 6 } else { 4 }, 1, 2),
            (
                na + if name.starts_with("forward") { 10 } else { 8 },
                16000,
                2,
            ),
            (ra + 48, 89, 8),
        ] {
            reject(name, |c| {
                put(&mut c.frame, at, value, width);
                c.seal_frame();
            });
        }
        reject(name, |c| {
            c.frame[ra + 64 + 100] ^= 1;
            c.seal_frame();
        });
        reject(name, |c| {
            let pos = na + 128;
            c.frame[pos + 180] ^= 1;
            let len = u32_at(&c.frame, na + 12) as usize;
            let (off, n) = if len == 0 {
                (56, u32_at(&c.frame, na + 16) as usize)
            } else {
                (24, len)
            };
            let digest = content_digest(&c.frame[pos..pos + n]);
            c.frame[na + off..na + off + 32].copy_from_slice(&digest);
            c.seal_frame();
        });
    }
}
#[test]
fn leaf_preserves_every_neighbor_and_validates_all_rows() {
    for name in ["forward-middle", "clr-middle", "prune-middle"] {
        reject(name, |c| {
            let r = c.cell_range(7, 1);
            c.pages[7][r.start + 12 + 173] ^= 1;
            c.bind(false);
        });
        reject(name, |c| {
            let r = c.cell_range(6, 1);
            c.pages[6][r.start + 12 + 8] = 2;
            c.bind(false);
        });
        reject(name, |c| {
            let r = c.cell_range(7, 1);
            c.pages[7][r.start + 12 + 172] = 4;
            c.bind(false);
        });
        reject(name, |c| {
            let r = c.cell_range(7, 0);
            c.pages[7][r.start + 7] = 1;
            c.bind(false);
        });
    }
}
#[test]
fn leaf_forward_original_seal_job_size_and_projection() {
    reject("forward-empty", |c| c.sealed = None);
    reject("forward-empty", |c| c.sealed.as_mut().unwrap().length += 1);
    reject("forward-empty", |c| {
        c.sealed.as_mut().unwrap().digest[0] ^= 1
    });
    reject("forward-empty", |c| {
        let at = c.record_at(7);
        c.pages[7][at + 173] ^= 1;
        c.bind(true);
        c.canonical_selected(7);
    });
    reject("forward-empty", |c| {
        c.jobs[1][288] ^= 1;
        c.bind(false);
    });
    reject("forward-empty", |c| {
        put(&mut c.jobs[1], 280, 194, 4);
        put(&mut c.jobs[1], 784, 311608 + 388, 8);
        put(&mut c.jobs[1], 880, 311608 + 388, 8);
        c.bind(false);
        row::decode_job(&c.jobs[1], c.context.profile.spec().namespace).unwrap();
    });
    for at in [176, 208] {
        reject("forward-empty", |c| {
            let ca = c.core_at();
            c.frame[ca + at] ^= 1;
            c.seal_frame();
        });
    }
}
#[test]
fn leaf_current_version_and_prune_payload_are_exact() {
    for name in ["forward-empty", "clr-middle", "prune-middle"] {
        let p = if name.starts_with("forward") { 7 } else { 6 };
        for (at, v) in [(64, 3), (112, 4), (128, 15), (152, 3)] {
            reject(name, |c| {
                let r = c.record_at(p);
                put(&mut c.pages[p], r + at, v, 8);
                c.bind(true);
                c.canonical_selected(p);
            });
        }
    }
    for (at, v) in [(24, 8), (112, 4)] {
        reject("prune-middle", |c| {
            let r = c.record_at(7);
            put(&mut c.pages[7], r + at, v, 8);
            c.bind(true);
            c.canonical_selected(7);
        });
    }
    reject("prune-middle", |c| {
        let r = c.record_at(7);
        c.pages[7][r + 173] ^= 1;
        c.bind(true);
        c.canonical_selected(7);
    });
    reject("prune-middle", |c| {
        let r = c.record_at(7);
        c.pages[7][r + 6] = 2;
        reference(14)
            .encode(&mut c.pages[7][r + 120..r + 160])
            .unwrap();
        c.bind(true);
        c.canonical_selected(7);
    });
    for name in ["clr-middle", "prune-middle"] {
        reject(name, |c| {
            c.sealed = Some(SealedValue {
                length: c.canonical.len(),
                digest: content_digest(&c.canonical),
            })
        });
    }
}
#[test]
fn leaf_schema_and_inline_payload_validation() {
    reject("forward-null", |c| c.catalog[240] = 0);
    reject("forward-empty", |c| c.catalog[239] = 1);
    reject("forward-empty", |c| {
        let r = c.record_at(7);
        c.pages[7][r + 7] = 2;
        c.bind(true);
    });
    reject("forward-mixed", |c| {
        let r = c.record_at(7);
        c.pages[7][r + 168] = 31;
        c.bind(true);
    });
    reject("forward-empty", |c| {
        let r = c.record_at(7);
        put(&mut c.pages[7], r + 8, 4, 8);
        c.bind(true);
    });
}
#[test]
fn leaf_requires_same_frame_undo_status_and_supported_operation() {
    reject("forward-middle", |c| {
        c.pages[1][15920 + 80] = 1;
        c.bind(false);
    });
    reject("clr-middle", |c| {
        c.pages[4][16128 + 8] = 9;
        c.bind(false);
    });
    reject("prune-middle", |c| {
        c.commit = Some(CommitSeq::new(8).unwrap())
    });
    reject("forward-middle", |c| {
        c.pages[2][15920 + 96] = 8;
        c.bind(false);
    });
    let c = case("forward-empty");
    let mut input = c.inputs();
    input.ordinal = 64;
    assert!(leaf::observe(c.layout(), input).is_err());
    const WIRE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/frames.bin"
    ));
    const WI: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/index.tsv"
    ));
    for op in [1, 3, 5, 6, 7, 11, 12, 16, 17, 21, 22] {
        let name = format!("op-{op}");
        let f: Vec<_> = WI
            .lines()
            .find(|l| l.starts_with(&(name.clone() + "\t")))
            .unwrap()
            .split('\t')
            .collect();
        let at: usize = f[1].parse().unwrap();
        let len: usize = f[2].parse().unwrap();
        let v = decode_layout(
            &WIRE[at..at + len],
            FrameExpectation {
                stream: WalStreamIdentity {
                    database: c.context.profile.spec().namespace.database,
                    timeline: 1,
                    writer_epoch: 1,
                },
                previous_lsn: 100,
            },
            c.context.profile,
        )
        .unwrap();
        assert_eq!(leaf::observe(v, c.inputs()), Err(Error::Unsupported));
    }
}
