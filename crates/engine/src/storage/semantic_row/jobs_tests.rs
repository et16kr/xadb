//! New Job-local semantics only; raw fixtures never grant descriptor authority.
use super::{jobs::*, *};
use crate::storage::{
    control,
    page::PAGE_BYTES,
    retention_page::{
        JOB_BYTES, JOB_COUNT,
        row::{self, JobCells, JobState},
    },
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
    "/../../tests/fixtures/module-row-format/jobs/cases.bin"
));
const INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/jobs/index.tsv"
));
const PROFILES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));
struct Case {
    frame: Vec<u8>,
    jobs: Box<[[u8; JOB_BYTES]; JOB_COUNT]>,
    pages: Box<[[u8; PAGE_BYTES]; 2]>,
    sender: [u8; 1536],
    context: undo::UndoContext,
    ordinal: usize,
    op: Operation,
    reserve: Option<ReservePlan>,
    commit: Option<CommitSeq>,
}
fn put(b: &mut [u8], at: usize, value: u64, width: usize) {
    b[at..at + width].copy_from_slice(&value.to_le_bytes()[..width]);
}
fn reference(n: u64) -> UndoRef {
    let mut b = [0; 40];
    for (a, v) in [(0, 0x100000001), (8, n), (16, 2), (32, 2)] {
        put(&mut b, a, v, 8);
    }
    UndoRef::decode(&b).unwrap()
}
fn plan(c: u32, h: bool) -> ReservePlan {
    ReservePlan {
        undo: reference(14),
        sequence: 3,
        table: ObjectHandle {
            object: ObjectId::new(6).unwrap(),
            generation: Generation::new(1).unwrap(),
            schema_epoch: SchemaEpoch::new(1).unwrap(),
        },
        pair: [reference(10).page(), reference(11).page()],
        statement: 3,
        mutation: 3,
        row: RowId::new(3).unwrap(),
        cell_bytes: c,
        projection: [0x44; 32],
        predecessor: h,
    }
}
fn free(g: u64) -> [u8; 1024] {
    let mut b = [0; 1024];
    b[..6].copy_from_slice(b"XQJ1\x03\0");
    put(&mut b, 8, g, 8);
    b
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
    let op = Operation::decode(f[3].parse().unwrap()).unwrap();
    let b = &INPUT[at..at + fl + 99840];
    let mut jobs = Box::new([[0; 1024]; 64]);
    for (i, j) in jobs.iter_mut().enumerate() {
        j.copy_from_slice(&b[fl + i * 1024..fl + (i + 1) * 1024]);
    }
    let mut pages = Box::new([[0; PAGE_BYTES]; 2]);
    for (i, p) in pages.iter_mut().enumerate() {
        p.copy_from_slice(&b[fl + 65536 + i * PAGE_BYTES..fl + 65536 + (i + 1) * PAGE_BYTES]);
    }
    let profile = control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap();
    // Actor/plan are independently specified, never decoded from candidate after.
    let mut actor = [0; 48];
    for (i, v) in actor[..16].iter_mut().enumerate() {
        *v = i as u8 + 1;
    }
    for (a, v, w) in [(16, 1, 8), (24, 1, 4), (32, 1, 8), (40, 2, 8)] {
        put(&mut actor, a, v, w);
    }
    let mut status = [0; 40];
    reference(9).encode(&mut status).unwrap();
    Case {
        frame: b[..fl].to_vec(),
        jobs,
        pages,
        sender: b[fl + 98304..].try_into().unwrap(),
        context: undo::UndoContext {
            profile,
            actor: TxnRef::decode(&actor).unwrap(),
            location: StatusSlotRef::decode(&status).unwrap(),
        },
        ordinal: f[4].parse().unwrap(),
        op,
        reserve: (op == Operation::Reserve).then(|| plan(f[9].parse().unwrap(), f[7] == "1")),
        commit: matches!(op, Operation::Commit | Operation::Prune)
            .then(|| CommitSeq::new(9).unwrap()),
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
            ordinal: self.ordinal,
            images: JobImages {
                before: &self.pages[0],
                after: &self.pages[1],
            },
            reserve: self.reserve,
            sender: matches!(
                self.op,
                Operation::Forward | Operation::Commit | Operation::AbortEnd
            )
            .then_some(&self.sender),
            commit: self.commit,
        }
    }
    fn observe(&self) -> Result<JobObservation> {
        jobs::observe(self.layout(), self.inputs())
    }
    fn core(&self) -> usize {
        if self.frame[7] == 3 { 3488 } else { 1888 }
    }
    fn offsets(&self) -> (usize, usize) {
        // Structural offsets only: callers may be editing an unsealed frame.
        let mut na = self.core() + 256;
        let mut ra = self.frame.len()
            - 32
            - u32_at(&self.frame, if self.frame[7] == 3 { 400 } else { 312 }) as usize;
        for _ in 0..u16_at(&self.frame, 20) {
            if self.frame[na] == 9 {
                return (na, ra);
            }
            na += u32_at(&self.frame, na + 20) as usize;
            ra += u32_at(&self.frame, ra + 4) as usize;
        }
        panic!("Job native missing")
    }
    fn start(&self, p: usize, s: usize) -> usize {
        u16_at(&self.pages[p], 128 + 4 * s) as usize
    }
    fn selected(&self, p: usize) -> &[u8] {
        let a = self.start(p, self.ordinal % 15);
        &self.pages[p][a..a + 1024]
    }
    fn edit_after(&mut self, edit: impl FnOnce(&mut [u8])) {
        let a = self.start(1, self.ordinal % 15);
        edit(&mut self.pages[1][a..a + 1024]);
        self.bind();
    }
    fn original(&mut self, ordinal: usize, cell: [u8; 1024]) {
        self.jobs[ordinal] = cell;
        if ordinal / 15 == self.ordinal / 15 {
            let a = self.start(0, ordinal % 15);
            self.pages[0][a..a + 1024].copy_from_slice(&cell);
            if ordinal != self.ordinal {
                let a = self.start(1, ordinal % 15);
                self.pages[1][a..a + 1024].copy_from_slice(&cell);
            }
        }
        self.bind();
    }
    fn seal(&mut self) {
        let h = content_digest(&self.frame[..128]);
        self.frame[128..160].copy_from_slice(&h);
        let end = self.frame.len() - 32;
        let h = content_digest(&self.frame[..end]);
        self.frame[end..].copy_from_slice(&h);
    }
    fn bind(&mut self) {
        let (na, ra) = self.offsets();
        let slot = self.ordinal % 15;
        for (p, sl, of, dig, raw) in [(0, 4, 8, 24, 128), (1, 6, 10, 56, 1152)] {
            let a = self.start(p, slot);
            let cell = &self.pages[p][a..a + 1024];
            self.frame[na + raw..na + raw + 1024].copy_from_slice(cell);
            self.frame[na + dig..na + dig + 32].copy_from_slice(&content_digest(cell));
            put(&mut self.frame, na + sl, slot as u64, 2);
            put(&mut self.frame, na + of, a as u64, 2);
        }
        self.frame[ra + 8..ra + 48].copy_from_slice(&self.pages[0][24..64]);
        self.frame[ra + 48..ra + 56].copy_from_slice(&self.pages[0][64..72]);
        self.frame[ra + 64..ra + 16344].copy_from_slice(&self.pages[1][72..16352]);
        for p in self.pages.iter_mut() {
            let h = content_digest(&p[..16352]);
            p[16352..].copy_from_slice(&h);
        }
        self.seal();
    }
    fn canonical(&self, p: usize) {
        row::decode_job(self.selected(p), self.context.profile.spec().namespace)
            .expect("canonical-valid semantic mutant");
    }
}
fn reject(name: &str, edit: impl FnOnce(&mut Case)) {
    let mut c = case(name);
    edit(&mut c);
    assert!(c.observe().is_err(), "accepted {name} mutant");
}
fn state(s: &str) -> JobState {
    match s {
        "free" => JobState::Free,
        "terminal" | "terminal-leased" => JobState::TerminalLeased,
        "terminal-committed" | "terminal-aborted" => JobState::TerminalReady,
        "reserved" => JobState::RootReserved,
        "retained" => JobState::RootRetained,
        "committed" => JobState::RootReleasingCommitted,
        "compensated" => JobState::RootReleasingCompensated,
        _ => panic!("state"),
    }
}
#[test]
fn jobs_twenty_six_raw_pairs_and_page_boundaries() {
    assert_eq!(INDEX.lines().count(), 26);
    for line in INDEX.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let a: usize = f[1].parse().unwrap();
        let n: usize = f[2].parse().unwrap();
        assert_eq!(
            content_digest(&INPUT[a..a + n + 99840])
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            f[12]
        );
        let c = case(f[0]);
        let got = c.observe().unwrap_or_else(|e| panic!("{} {e:?}", f[0]));
        assert_eq!(got.ordinal(), c.ordinal);
        assert_eq!(got.states(), (state(f[5]), state(f[6])));
        let g: u64 = f[10].parse().unwrap();
        assert_eq!(
            got.generations(),
            (
                Generation::new(g).unwrap(),
                Generation::new(
                    g + u64::from(matches!(c.op, Operation::Begin | Operation::Reserve))
                )
                .unwrap()
            )
        );
    }
}
#[test]
fn jobs_generation_lease_free_and_exhaustion() {
    for name in ["begin-max", "reserve-max-slot63"] {
        let mut c = case(name);
        let ordinal = c.ordinal;
        c.original(ordinal, free(u64::MAX));
        c.canonical(0);
        assert_eq!(c.observe(), Err(Error::Exhausted));
        reject(name, |c| {
            c.edit_after(|b| put(b, 8, 2, 8));
            c.canonical(1);
        });
        reject(name, |c| {
            c.edit_after(|b| {
                put(b, 64, 100, 8);
                put(b, 376, 100, 8)
            });
            c.canonical(1);
        });
    }
    for name in ["cancel", "rootfree-committed", "terminalfinish-aborted"] {
        reject(name, |c| {
            c.edit_after(|b| put(b, 8, 3, 8));
            c.canonical(1);
        });
    }
    reject("begin-genesis", |c| {
        c.original(c.ordinal, free(2));
    }); // LSN0 only all Free generation1.
}
#[test]
fn jobs_reserve_plan_is_independent_and_exact() {
    for field in 0..12 {
        reject("reserve", |c| {
            let p = c.reserve.as_mut().unwrap();
            match field {
                0 => p.undo = reference(15),
                1 => p.sequence += 1,
                2 => p.table.object = ObjectId::new(7).unwrap(),
                3 => p.table.generation = Generation::new(2).unwrap(),
                4 => p.table.schema_epoch = SchemaEpoch::new(2).unwrap(),
                5 => p.pair[0] = reference(12).page(),
                6 => p.pair[1] = reference(12).page(),
                7 => p.statement += 1,
                8 => p.mutation += 1,
                9 => p.row = RowId::new(4).unwrap(),
                10 => p.cell_bytes += 1,
                _ => p.projection[0] ^= 1,
            }
        });
    }
    reject("reserve", |c| {
        c.reserve.as_mut().unwrap().predecessor = true
    });
    // Rebind all candidate projections, retaining the original expected plan.
    reject("reserve", |c| {
        c.edit_after(|b| put(b, 320, 4, 8));
        c.canonical(1);
    });
    reject("reserve", |c| {
        c.edit_after(|b| {
            put(b, 784, 312106, 8);
            put(b, 880, 312106, 8)
        });
        c.canonical(1);
    });
    for (at, v) in [(8, 4), (16, 4), (24, 4), (40, 15)] {
        reject("reserve", |c| {
            let a = c.core();
            put(&mut c.frame, a + at, v, 8);
            c.seal();
        });
    }
}
#[test]
fn jobs_canonical_wrong_transitions_and_step_times() {
    for (name, other) in [
        ("forward", "clr"),
        ("cancel", "forward"),
        ("clr", "prune"),
        ("prune", "clr"),
        ("commit-mixed", "abort-compensated"),
        ("abort-compensated", "commit-mixed"),
        ("terminalfinish-committed", "commit-mixed"),
    ] {
        let replacement: [u8; 1024] = case(other).selected(1).try_into().unwrap();
        reject(name, |c| {
            c.edit_after(|b| b.copy_from_slice(&replacement));
            c.canonical(1);
        });
    }
    for name in ["forward", "commit-mixed", "abort-compensated"] {
        reject(name, |c| {
            c.edit_after(|b| {
                put(b, 72, 100, 8);
                put(b, 376, 100, 8)
            });
            c.canonical(1);
        });
    }
    for name in ["clr", "prune"] {
        reject(name, |c| {
            c.edit_after(|b| {
                put(b, 272, 100, 8);
                put(b, 376, 100, 8)
            });
            c.canonical(1);
        });
    }
    // Wrong before ReleaseRole state is still a well-formed Retained Job.
    reject("release-committed-0", |c| {
        let b = case("clr").jobs[1];
        c.original(1, b);
        c.canonical(0);
    });
}
#[test]
fn jobs_preserves_root_material_rowid_sender_and_budget() {
    for name in [
        "forward",
        "clr",
        "prune",
        "release-committed-0",
        "release-compensated-1",
    ] {
        for (at, v) in [
            (64, 29),
            (136, 15),
            (168, 4),
            (176, 7),
            (248, 4),
            (256, 4),
            (320, 4),
        ] {
            reject(name, |c| {
                c.edit_after(|b| put(b, at, v, 8));
                c.canonical(1);
            });
        }
    }
    for name in [
        "clr",
        "prune",
        "release-committed-0",
        "release-compensated-1",
    ] {
        reject(name, |c| {
            c.edit_after(|b| b[328] ^= 1);
            c.canonical(1);
        });
    }
    reject("forward", |c| {
        c.edit_after(|b| {
            put(b, 784, 312106, 8);
            put(b, 880, 312106, 8)
        });
        c.canonical(1);
    });
    for name in ["release-committed-0", "release-compensated-0"] {
        reject(name, |c| {
            c.edit_after(|b| {
                b[121] = 2;
                put(b, 776, 2, 8);
                put(b, 784, 111056, 8);
                put(b, 880, 111056, 8)
            });
            c.canonical(1);
        });
    }
    reject("rootfree-committed", |c| {
        let b = case("release-committed-2").jobs[1];
        c.original(1, b);
        c.canonical(0);
    });
}
#[test]
fn jobs_sender_binds_original_prior_and_after() {
    for name in ["forward", "commit-mixed", "abort-compensated"] {
        reject(name, |c| c.sender[0] ^= 1);
        reject(name, |c| {
            c.frame[312] ^= 1;
            c.seal();
        });
        reject(name, |c| {
            c.edit_after(|b| b[328] ^= 1);
            c.canonical(1);
        });
        let c = case(name);
        let mut input = c.inputs();
        input.sender = None;
        assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
    }
    let c = case("clr");
    let mut input = c.inputs();
    input.sender = Some(&c.sender);
    assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
}
#[test]
fn jobs_actual_page_bank_native_redo_and_lsn() {
    for name in ["begin-slot14", "forward-max-slot15", "reserve-max-slot63"] {
        let (na, ra) = case(name).offsets();
        for (at, v, w) in [
            (na + 4, 0, 2),
            (na + 6, 0, 2),
            (na + 8, 14000, 2),
            (na + 10, 14000, 2),
            (ra + 48, 99, 8),
        ] {
            // slot15 is slot0; its wrong slot is1.
            let v = if (at == na + 4 || at == na + 6) && name == "forward-max-slot15" {
                1
            } else {
                v
            };
            reject(name, |c| {
                put(&mut c.frame, at, v, w);
                c.seal();
            });
        }
        reject(name, |c| {
            c.frame[ra + 64 + 100] ^= 1;
            c.seal();
        });
        reject(name, |c| {
            c.frame[na + 128 + 8] ^= 1;
            let d = content_digest(&c.frame[na + 128..na + 1152]);
            c.frame[na + 24..na + 56].copy_from_slice(&d);
            c.seal();
        });
    }
    reject("forward", |c| {
        c.jobs[1][320] ^= 1;
    });
    reject("forward", |c| {
        let at = c.start(1, 2);
        put(&mut c.pages[1], at + 8, 2, 8);
        c.bind();
    });
    for (p, at, v) in [
        (0, 64, 101),
        (1, 64, 100),
        (0, 32, 3),
        (1, 48, 5),
        (1, 56, 2),
    ] {
        reject("forward", |c| {
            put(&mut c.pages[p], at, v, 8);
            c.bind();
            if p == 0 && at == 64 {
                // Keep redo canonical so the actual page LSN reaches this observer.
                let (_, ra) = c.offsets();
                put(&mut c.frame, ra + 48, 100, 8);
                c.seal();
            }
        });
    }
    reject("forward", |c| {
        let at = c.start(0, 2);
        c.pages[0][at + 16] = 1;
        c.bind();
    });
}
#[test]
fn jobs_terminal_census_uses_all_original_cells() {
    for name in [
        "commit-mixed",
        "abort-compensated",
        "terminalfinish-committed",
    ] {
        reject(name, |c| {
            let mut b = case("commit-mixed").jobs[0];
            put(&mut b, 56, 3, 8);
            put(&mut b, 112, 3, 8);
            row::decode_job(&b, c.context.profile.spec().namespace).unwrap();
            c.original(63, b);
        });
        reject(name, |c| {
            let mut b = case("commit-mixed").jobs[0];
            put(&mut b, 88, 60, 8);
            row::decode_job(&b, c.context.profile.spec().namespace).unwrap();
            c.original(63, b);
        });
        reject(name, |c| {
            let b = case("commit-mixed").jobs[0];
            c.original(63, b);
        });
    }
    for (name, other) in [
        ("commit-mixed", "forward"),
        ("abort-compensated", "clr"),
        ("terminalfinish-committed", "release-compensated-2"),
        ("terminalfinish-aborted", "release-compensated-0"),
        ("reserve", "forward"),
        ("release-committed-0", "forward"),
    ] {
        reject(name, |c| {
            let b = case(other).jobs[1];
            c.original(63, b);
        });
    }
    reject("prune", |c| {
        let b = case("terminalfinish-aborted").jobs[0];
        c.original(0, b);
    });
    reject("release-committed-0", |c| {
        let mut b = c.jobs[1];
        put(&mut b, 264, 8, 8);
        c.original(1, b);
        c.edit_after(|b| put(b, 264, 8, 8));
        c.canonical(0);
        c.canonical(1);
    });
    reject("release-compensated-0", |c| {
        let b = case("clr").jobs[1];
        c.original(63, b);
    });
    reject("forward", |c| {
        c.original(0, free(2));
    });
    reject("begin-max", |c| {
        let b = case("release-compensated-0").jobs[1];
        c.original(63, b);
    });
    reject("forward", |c| {
        let mut b = case("release-compensated-1").jobs[1];
        put(&mut b, 376, 101, 8);
        c.original(63, b);
    });
}
#[test]
fn jobs_options_core_and_unsupported_boundary() {
    let c = case("reserve");
    let mut input = c.inputs();
    input.reserve = None;
    assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
    let c = case("forward");
    let mut input = c.inputs();
    input.reserve = Some(plan(185, false));
    assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
    for name in ["commit-mixed", "prune"] {
        let c = case(name);
        let mut input = c.inputs();
        input.commit = None;
        assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
        reject(name, |c| c.commit = Some(CommitSeq::new(8).unwrap()));
    }
    let c = case("cancel");
    let mut input = c.inputs();
    input.commit = Some(CommitSeq::new(9).unwrap());
    assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
    let mut input = c.inputs();
    input.ordinal = 64;
    assert_eq!(jobs::observe(c.layout(), input), Err(Error::InvalidInput));
    let mut input = c.inputs();
    input.context.actor = TxnRef::new(c.context.actor.id(), Generation::new(3).unwrap()).unwrap();
    assert!(jobs::observe(c.layout(), input).is_err());
    for name in [
        "prune",
        "release-committed-0",
        "rootfree-compensated",
        "terminalfinish-aborted",
    ] {
        reject(name, |c| {
            let a = c.core();
            put(&mut c.frame, a + 168, 63, 8);
            c.seal();
        });
    }
    for name in [
        "forward",
        "cancel",
        "clr",
        "prune",
        "release-committed-0",
        "rootfree-compensated",
    ] {
        reject(name, |c| {
            let a = c.core();
            put(&mut c.frame, a + 24, 4, 8);
            c.seal();
        });
    }
    const WI: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/index.tsv"
    ));
    const WB: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/frames.bin"
    ));
    for op in [3u16, 5, 16] {
        let fields: Vec<_> = WI
            .lines()
            .find(|l| l.split('\t').nth(3).unwrap().parse::<u16>().unwrap() == op)
            .unwrap()
            .split('\t')
            .collect();
        let at: usize = fields[1].parse().unwrap();
        let len: usize = fields[2].parse().unwrap();
        let mut c = case("forward");
        c.frame = WB[at..at + len].to_vec();
        assert_eq!(c.observe(), Err(Error::Unsupported));
    }
}

#[test]
fn jobs_cell_size_and_predecessor_match_same_frame() {
    for name in ["forward", "clr", "prune"] {
        for h in [false, true] {
            reject(name, |c| {
                let mut b = c.jobs[c.ordinal];
                let size = if h { 185 } else { 186 };
                let wal = 311608 + 2 * size + 128 * u64::from(h);
                put(&mut b, 280, size, 4);
                put(&mut b, 784, wal, 8);
                put(&mut b, 880, wal, 8);
                c.original(c.ordinal, b);
                if c.op == Operation::Forward {
                    c.edit_after(|b| {
                        put(b, 280, size, 4);
                        put(b, 784, wal, 8);
                        put(b, 880, wal, 8);
                    });
                }
                c.canonical(0);
                c.canonical(1);
            });
        }
    }
    let c = case("begin-genesis");
    for legacy in [1, 2] {
        let mut raw = [0; 4096];
        if legacy == 1 {
            control::encode_profile(c.context.profile.spec(), &mut raw).unwrap();
        } else {
            control::encode_catalog_profile(c.context.profile.spec(), &mut raw).unwrap();
        }
        let mut input = c.inputs();
        input.context.profile = control::decode_bootstrap_profile(&raw, None).unwrap();
        assert_eq!(jobs::observe(c.layout(), input), Err(Error::Unsupported));
    }
}
