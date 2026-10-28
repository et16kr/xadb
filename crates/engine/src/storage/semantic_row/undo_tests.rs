//! Independent UNDO_LOCAL_ONLY pairs; other frame roles remain opaque.
use super::{undo::*, *};
use crate::storage::{
    control,
    page::PAGE_BYTES,
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        row::{self, JobCells},
    },
    transaction_record::{StatusSlotRef, TxnRef},
    undo_record::UndoRecordState,
    undo_ref::UndoRef,
    wal_frame::{FrameExpectation, WalStreamIdentity},
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation},
};
const INPUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/undo/cases.bin"
));
const INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/undo/index.tsv"
));
const PROFILES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
struct Case {
    frame: Vec<u8>,
    jobs: Box<[[u8; JOB_BYTES]; JOB_COUNT]>,
    before: [u8; PAGE_BYTES],
    after: [u8; PAGE_BYTES],
    h: [u8; PAGE_BYTES],
    sb: [u8; PAGE_BYTES],
    sa: [u8; PAGE_BYTES],
    context: UndoContext,
    source: UndoSource,
    has_h: bool,
    status: bool,
    statement: Option<status::StatementBoundary>,
    commit: Option<CommitSeq>,
}
fn put(b: &mut [u8], at: usize, v: u64, width: usize) {
    b[at..at + width].copy_from_slice(&v.to_le_bytes()[..width]);
}
fn uref(n: u64) -> UndoRef {
    let mut b = [0; 40];
    for (at, v) in [(0, 0x100000001), (8, n), (16, 2), (32, 2)] {
        put(&mut b, at, v, 8);
    }
    UndoRef::decode(&b).unwrap()
}
fn case(name: &str) -> Case {
    let line = INDEX
        .lines()
        .find(|l| l.split('\t').next() == Some(name))
        .unwrap();
    let f: Vec<_> = line.split('\t').collect();
    let at: usize = f[1].parse().unwrap();
    let flen: usize = f[2].parse().unwrap();
    let op: u16 = f[3].parse().unwrap();
    let h: u64 = f[4].parse().unwrap();
    let stmt = f[5] == "1";
    let generation: u64 = f[6].parse().unwrap();
    let b = &INPUT[at..at + flen + 65536 + 5 * PAGE_BYTES];
    let profile = control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap();
    let mut a = [0; 48];
    a[..16].copy_from_slice(&profile.spec().namespace.database.bytes());
    for (at, v, w) in [(16, 1, 8), (24, 1, 4), (32, 1, 8), (40, 2, 8)] {
        put(&mut a, at, v, w);
    }
    let mut loc = [0; 40];
    uref(9).encode(&mut loc).unwrap();
    let mut ur = [0; 40];
    uref(14).encode(&mut ur).unwrap();
    put(&mut ur, 32, generation, 8);
    let p = flen + 65536;
    let mut jobs = Box::new([[0; JOB_BYTES]; JOB_COUNT]);
    for (i, j) in jobs.iter_mut().enumerate() {
        j.copy_from_slice(&b[flen + i * 1024..flen + (i + 1) * 1024]);
    }
    let status = matches!(op, 2 | 4 | 20);
    Case {
        frame: b[..flen].to_vec(),
        jobs,
        before: b[p..p + PAGE_BYTES].try_into().unwrap(),
        after: b[p + PAGE_BYTES..p + 2 * PAGE_BYTES].try_into().unwrap(),
        h: b[p + 2 * PAGE_BYTES..p + 3 * PAGE_BYTES]
            .try_into()
            .unwrap(),
        sb: b[p + 3 * PAGE_BYTES..p + 4 * PAGE_BYTES]
            .try_into()
            .unwrap(),
        sa: b[p + 4 * PAGE_BYTES..].try_into().unwrap(),
        context: UndoContext {
            profile,
            actor: TxnRef::decode(&a).unwrap(),
            location: StatusSlotRef::decode(&loc).unwrap(),
        },
        source: if op == 11 {
            UndoSource::Reserve {
                ordinal: 1,
                planned: UndoReservation {
                    reference: UndoRef::decode(&ur).unwrap(),
                    sequence: 3,
                    statement: 3,
                    mutation: 3,
                },
            }
        } else {
            UndoSource::Existing { ordinal: 1 }
        },
        has_h: status && h != 0,
        status,
        statement: if op == 2 || stmt {
            Some(status::StatementBoundary {
                sequence: 3,
                mark: (h != 0).then(|| uref(13)),
            })
        } else {
            None
        },
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
        .expect("mutant must pass wire")
    }
    fn observe(&self) -> Result<UndoObservation> {
        undo::observe(
            self.layout(),
            self.context,
            JobCells::original(&self.jobs),
            self.source,
            UndoImages {
                before: &self.before,
                after: &self.after,
                predecessor: self.has_h.then_some(&self.h),
            },
            self.status.then_some(StatusProof {
                statement: self.statement,
                commit: self.commit,
                images: status::StatusImages {
                    before: &self.sb,
                    after: &self.sa,
                },
            }),
        )
    }
    fn core_at(&self) -> usize {
        if self.frame[7] == 3 { 3488 } else { 1888 }
    }
    fn offsets(&self, kind: u8) -> (usize, usize) {
        let v = self.layout();
        let native = v.native_records().find(|n| n.kind() == kind).unwrap();
        let mut na = self.core_at() + 256;
        let mut ra = self.frame.len() - 32 - v.redo().len();
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
    fn seal_pages(&mut self) {
        for b in [
            &mut self.before,
            &mut self.after,
            &mut self.h,
            &mut self.sb,
            &mut self.sa,
        ] {
            if b[..4] == *b"XDP1" {
                let h = content_digest(&b[..16352]);
                b[16352..].copy_from_slice(&h);
            }
        }
    }
    fn bind(&mut self) {
        for kind in if self.status {
            &[3, 5, 9][..]
        } else {
            &[3, 9][..]
        } {
            let (na, ra) = self.offsets(*kind);
            let (b, a) = match kind {
                3 => (&self.before[15920..16352], Some(&self.after[15920..16352])),
                5 => (&self.sb[16128..16352], Some(&self.sa[16128..16352])),
                9 => (self.jobs[1].as_slice(), None),
                _ => unreachable!(),
            };
            self.frame[na + 128..na + 128 + b.len()].copy_from_slice(b);
            self.frame[na + 24..na + 56].copy_from_slice(&content_digest(b));
            if let Some(a) = a {
                self.frame[na + 128 + b.len()..na + 128 + b.len() + a.len()].copy_from_slice(a);
                self.frame[na + 56..na + 88].copy_from_slice(&content_digest(a));
                let rlen = u32_at(&self.frame, ra + 4) as usize;
                self.frame[ra + 64..ra + rlen].copy_from_slice(if *kind == 3 {
                    &self.after[72..16352]
                } else {
                    &self.sa[72..16352]
                });
            }
            self.seal_frame();
        }
        self.seal_pages();
        self.seal_frame();
    }
}
fn reject(name: &str, edit: impl FnOnce(&mut Case)) {
    let mut c = case(name);
    edit(&mut c);
    assert!(c.observe().is_err(), "accepted {name} mutant");
}
fn canonical_job(c: &Case) {
    row::decode_job(&c.jobs[1], c.context.profile.spec().namespace)
        .expect("wrong Job is still canonical");
}

#[test]
fn undo_independent_seventeen_pairs_and_same_frame_status() {
    assert_eq!(INDEX.lines().count(), 17);
    for line in INDEX.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let at: usize = f[1].parse().unwrap();
        let len: usize = f[2].parse().unwrap();
        let hash: String = content_digest(&INPUT[at..at + len + 65536 + 5 * PAGE_BYTES])
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hash, f[7]);
        let c = case(f[0]);
        let original = content_digest(&c.before);
        let o = c.observe().unwrap_or_else(|e| panic!("{}: {e:?}", f[0]));
        assert_eq!(o.reference().page().page_no, 14);
        assert_eq!(content_digest(&c.before), original);
        let states = match c.layout().operation() {
            Operation::Reserve => (UndoRecordState::Free, UndoRecordState::Free),
            Operation::Forward => (UndoRecordState::Free, UndoRecordState::Live),
            Operation::Clr | Operation::Prune => {
                (UndoRecordState::Live, UndoRecordState::Releasing)
            }
            Operation::RootFree => (UndoRecordState::Releasing, UndoRecordState::Free),
            _ => (UndoRecordState::Releasing, UndoRecordState::Releasing),
        };
        assert_eq!(o.states(), states);
        assert_eq!(o.predecessor().is_some(), c.has_h);
    }
}
#[test]
fn undo_reserve_uses_external_plan_and_generation_without_wrap() {
    for name in ["reserve", "reserve-max"] {
        let c = case(name);
        assert!(c.observe().is_ok());
        reject(name, |c| {
            c.source = UndoSource::Existing { ordinal: 1 };
        });
        reject(name, |c| {
            if let UndoSource::Reserve {
                ref mut planned, ..
            } = c.source
            {
                planned.sequence += 1;
            }
        });
        reject(name, |c| {
            if let UndoSource::Reserve {
                ref mut planned, ..
            } = c.source
            {
                planned.statement = 2;
            }
        });
        reject(name, |c| {
            if let UndoSource::Reserve {
                ref mut planned, ..
            } = c.source
            {
                planned.mutation = 4;
            }
        });
        reject(name, |c| {
            put(&mut c.before, 15928, 2, 8);
            c.bind();
        });
        reject(name, |c| {
            put(&mut c.after, 15928, 1, 8);
            c.bind();
        });
    }
    reject("reserve", |c| {
        c.jobs[1] = case("forward-none").jobs[1];
        c.bind();
        canonical_job(c);
    });
    reject("forward-none", |c| {
        c.source = UndoSource::Reserve {
            ordinal: 1,
            planned: UndoReservation {
                reference: uref(14),
                sequence: 3,
                statement: 3,
                mutation: 3,
            },
        };
    });
}
#[test]
fn undo_original_job_and_scalar_binding() {
    for (target, donor) in [
        ("forward-none", "clr-full-none"),
        ("clr-full-none", "forward-none"),
        ("prune-none", "rootfree-committed"),
        ("release0-never", "clr-full-none"),
        ("rootfree-compensated", "release2"),
    ] {
        reject(target, |c| {
            c.jobs[1] = case(donor).jobs[1];
            c.bind();
            canonical_job(c);
        });
    }
    for at in [24, 8, 16] {
        reject("forward-none", |c| {
            let ca = c.core_at();
            put(&mut c.frame, ca + at, 4, 8);
            c.seal_frame();
        });
    }
    reject("forward-none", |c| {
        put(&mut c.jobs[1], 48, 2, 8);
        c.bind();
        canonical_job(c);
    });
    reject("forward-none", |c| {
        put(&mut c.jobs[1], 88, 15, 8);
        c.bind();
        canonical_job(c);
    });
    reject("forward-none", |c| {
        put(&mut c.jobs[1], 136, 15, 8);
        c.bind();
        canonical_job(c);
    });
    reject("forward-none", |c| {
        c.source = UndoSource::Existing { ordinal: 64 };
    });
    reject("forward-none", |c| {
        let (na, _) = c.offsets(9);
        put(&mut c.frame, na + 4, 2, 2);
        c.seal_frame();
    });
    reject("forward-none", |c| {
        let (_, ra) = c.offsets(9);
        put(&mut c.frame, ra + 16, 3, 8);
        c.seal_frame();
    });
}
#[test]
fn undo_actual_predecessor_uses_its_own_target_and_nearest_job() {
    for name in [
        "forward-other-table",
        "clr-full-other-table",
        "prune-other-table",
        "forward-two-predecessors",
    ] {
        let c = case(name);
        assert_eq!(u64_at(&c.h, 16016), 7);
        assert!(c.observe().is_ok());
        reject(name, |c| {
            c.has_h = false;
        });
        for (at, v) in [(16016, 6), (15992, 4), (15936, 1), (16000, 3), (15976, 2)] {
            reject(name, |c| {
                put(&mut c.h, at, v, 8);
                c.seal_pages();
            });
        }
        reject(name, |c| {
            put(&mut c.h, 64, 101, 8);
            c.seal_pages();
        });
    }
    reject("forward-none", |c| {
        c.has_h = true;
        c.h = case("forward-other-table").h;
    });
    reject("forward-other-table", |c| {
        put(&mut c.jobs[2], 168, 4, 8);
        c.bind();
    });
    reject("forward-other-table", |c| {
        put(&mut c.jobs[2], 256, 3, 8);
        c.bind();
    });
    reject("forward-other-table", |c| {
        put(&mut c.jobs[2], 248, 4, 8);
        c.bind();
    });
    reject("forward-other-table", |c| {
        c.jobs[3] = c.jobs[2];
        c.bind();
    });
    reject("forward-other-table", |c| {
        c.jobs[3] = c.jobs[1];
        c.bind();
    });
    reject("forward-two-predecessors", |c| {
        put(&mut c.h, 16080, 2, 8);
        c.seal_pages();
    });
}
#[test]
fn undo_material_cut_and_incoming_are_exact() {
    for name in [
        "forward-none",
        "clr-full-none",
        "clr-full-other-table",
        "prune-none",
        "prune-other-table",
    ] {
        for (at, v, w) in [
            (15992, 4, 8),
            (15936, 4, 8),
            (16016, 7, 8),
            (16032, 8, 8),
            (16244, 1, 1),
        ] {
            reject(name, |c| {
                put(&mut c.after, at, v, w);
                c.bind();
            });
        }
    }
    for name in ["clr-full-none", "prune-none"] {
        reject(name, |c| {
            c.after[16246] = 2;
            c.bind();
        });
    }
    for name in ["clr-full-other-table", "prune-other-table"] {
        reject(name, |c| {
            c.after[16246] = 0;
            c.bind();
        });
    }
    for name in ["clr-full-none", "prune-none"] {
        reject(name, |c| {
            put(&mut c.before, 16000, 3, 8);
            c.bind();
        });
    }
    reject("forward-none", |c| {
        put(&mut c.after, 16000, 3, 8);
        c.bind();
    });
    // A Live cell with wrong material still decodes canonically; expected
    // creator/sequence/target/edge comparison must reject it independently.
    reject("clr-full-other-table", |c| {
        put(&mut c.before, 16080, 1, 8);
        c.bind();
    });
}
#[test]
fn undo_release_cursor_material_and_same_generation_free() {
    for name in [
        "release0-never",
        "release0-cut",
        "release1-never",
        "release1-cut",
        "release2",
    ] {
        reject(name, |c| {
            c.after[16248] = c.before[16248];
            c.bind();
        });
        reject(name, |c| {
            put(&mut c.after, 15992, 4, 8);
            c.bind();
        });
        reject(name, |c| {
            put(&mut c.after, 15928, 3, 8);
            c.bind();
        });
    }
    reject("release0-cut", |c| {
        c.after[16246] = 0;
        c.bind();
    });
    reject("release0-never", |c| {
        c.after[16246] = 2;
        c.bind();
    });
    reject("release1-cut", |c| {
        c.after[16246] = 2;
        c.bind();
    });
    reject("release2", |c| {
        c.after[16247] = 0;
        c.bind();
    });
    for name in ["rootfree-compensated", "rootfree-committed"] {
        reject(name, |c| {
            put(&mut c.after, 15928, 1, 8);
            c.bind();
        });
    }
    // Work cursor 62->63 is not Undo role cursor; the literals already differ.
    let c = case("release0-cut");
    assert_eq!(c.layout().core().cursors(), (62, 63));
    assert!(c.observe().is_ok());
}
#[test]
fn undo_same_frame_status_proof_is_required_and_checked() {
    for name in ["forward-none", "clr-full-none", "prune-none"] {
        reject(name, |c| {
            c.status = false;
        });
    }
    reject("clr-statement-last-mark", |c| {
        c.statement = None;
    });
    reject("forward-other-table", |c| {
        uref(12).encode(&mut c.sb[16232..16272]).unwrap();
        c.bind();
    });
    reject("clr-full-other-table", |c| {
        put(&mut c.sa, 16136, 2, 8);
        c.bind();
    });
    reject("prune-other-table", |c| {
        c.commit = Some(CommitSeq::new(10).unwrap());
    });
    reject("release0-cut", |c| {
        c.status = true;
        c.sb = case("prune-none").sb;
        c.sa = case("prune-none").sa;
    });
}
#[test]
fn undo_native_redo_page_identity_and_lsn_binding() {
    for (at, v, w) in [
        (32, 15, 8),
        (40, 3, 8),
        (48, 2, 8),
        (56, 2, 8),
        (6, 9, 1),
        (7, 1, 1),
        (64, 0, 8),
        (64, 101, 8),
        (72, 2, 2),
    ] {
        reject("forward-none", |c| {
            put(&mut c.before, at, v, w);
            c.seal_pages();
        });
    }
    reject("forward-none", |c| {
        put(&mut c.after, 64, 100, 8);
        c.seal_pages();
    });
    reject("forward-none", |c| {
        let (_, ra) = c.offsets(3);
        put(&mut c.frame, ra + 48, 29, 8);
        c.seal_frame();
    });
    reject("forward-none", |c| {
        let (_, ra) = c.offsets(3);
        c.frame[ra + 64 + 400] ^= 1;
        c.seal_frame();
    });
    reject("forward-none", |c| {
        let (na, _) = c.offsets(3);
        put(&mut c.frame, na + 8, 15919, 2);
        c.seal_frame();
    });
    reject("forward-none", |c| {
        put(&mut c.after, 15992, 4, 8);
        c.seal_pages();
    });
    reject("forward-none", |c| {
        c.context.actor = TxnRef::new(c.context.actor.id(), Generation::new(3).unwrap()).unwrap();
    });
    // A generation change in group owner is not merely a same-TxnId match.
    reject("forward-none", |c| {
        put(&mut c.frame, 248, 3, 8);
        c.seal_frame();
    });
}

#[test]
fn undo_rejects_operations_without_undo_native() {
    const WIRE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/frames.bin"
    ));
    const WIRE_INDEX: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/index.tsv"
    ));
    for op in [1, 3, 5, 6, 7, 12, 16, 17] {
        let name = format!("op-{op}");
        let line = WIRE_INDEX
            .lines()
            .find(|l| l.split('\t').next() == Some(&name))
            .unwrap();
        let fields: Vec<_> = line.split('\t').collect();
        let at: usize = fields[1].parse().unwrap();
        let len: usize = fields[2].parse().unwrap();
        let mut c = case("forward-none");
        c.frame = WIRE[at..at + len].to_vec();
        assert_eq!(c.observe(), Err(Error::Unsupported), "{name}");
    }
}
