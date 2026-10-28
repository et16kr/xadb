//! STATUS_LOCAL_ONLY: independent raw pages, explicit state pairs. The frame's
//! other native roles/descriptors remain invalid sentinels, never authorities.
use super::{status::*, *};
use crate::storage::{
    control,
    page::{PAGE_BYTES, PageRef},
    transaction_record::{StatusSlotRef, TransactionState, TransactionStatusCell, TxnRef},
    undo_ref::UndoRef,
    wal_frame::{FrameExpectation, WalStreamIdentity},
};
use foundation::{
    codec::content_digest,
    identity::{CommitSeq, Generation},
};
const INPUT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/status/cases.bin"
));
const INDEX: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/status/index.tsv"
));
const PROFILES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/module-row-format/envelopes/profiles.bin"
));

struct Case {
    frame: Vec<u8>,
    before: [u8; PAGE_BYTES],
    after: [u8; PAGE_BYTES],
    context: StatusContext,
}
fn put(b: &mut [u8], at: usize, v: u64, width: usize) {
    b[at..at + width].copy_from_slice(&v.to_le_bytes()[..width]);
}
fn undo(n: u64) -> UndoRef {
    let mut b = [0; 40];
    put(&mut b, 0, 0x100000001, 8);
    put(&mut b, 8, n, 8);
    put(&mut b, 16, 2, 8);
    put(&mut b, 32, 2, 8);
    UndoRef::decode(&b).unwrap()
}
fn case(name: &str) -> Case {
    let line = INDEX
        .lines()
        .find(|l| l.split('\t').next() == Some(name))
        .unwrap();
    let f: Vec<_> = line.split('\t').collect();
    let at: usize = f[1].parse().unwrap();
    let len: usize = f[2].parse().unwrap();
    let b = &INPUT[at..at + len + 2 * PAGE_BYTES];
    let profile = control::row_profile::decode_profile(&PROFILES[..4096], None).unwrap();
    let mut actor = [0; 48];
    actor[..16].copy_from_slice(&profile.spec().namespace.database.bytes());
    put(&mut actor, 16, 1, 8);
    put(&mut actor, 24, 1, 4);
    put(&mut actor, 32, 1, 8);
    put(&mut actor, 40, 2, 8);
    let mut location = [0; 40];
    location[..24].copy_from_slice(&b[len + 24..len + 48]);
    put(&mut location, 32, 2, 8);
    let stmt: u64 = f[3].parse().unwrap();
    let mark: u64 = f[4].parse().unwrap();
    let q: u64 = f[5].parse().unwrap();
    Case {
        frame: b[..len].to_vec(),
        before: b[len..len + PAGE_BYTES].try_into().unwrap(),
        after: b[len + PAGE_BYTES..].try_into().unwrap(),
        context: StatusContext {
            profile,
            actor: TxnRef::decode(&actor).unwrap(),
            location: StatusSlotRef::decode(&location).unwrap(),
            statement: (stmt != 0).then_some(StatementBoundary {
                sequence: stmt,
                mark: (mark != 0).then(|| undo(mark)),
            }),
            commit: (q != 0).then(|| CommitSeq::new(q).unwrap()),
        },
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
        .expect("mutant must still pass framing")
    }
    fn observe(&self) -> Result<StatusObservation> {
        status::observe(
            self.layout(),
            self.context,
            StatusImages {
                before: &self.before,
                after: &self.after,
            },
        )
    }
    fn reseal_frame(&mut self) {
        let h = content_digest(&self.frame[..128]);
        self.frame[128..160].copy_from_slice(&h);
        let end = self.frame.len() - 32;
        let h = content_digest(&self.frame[..end]);
        self.frame[end..].copy_from_slice(&h);
    }
    fn seal_pages(&mut self) {
        for b in [&mut self.before, &mut self.after] {
            let h = content_digest(&b[..16352]);
            b[16352..].copy_from_slice(&h);
        }
    }
    fn offsets(&self) -> (usize, usize) {
        let layout = self.layout();
        let n = layout.native_records().find(|n| n.kind() == 5).unwrap();
        let mut na = self.core_at() + 256;
        let mut ra = self.frame.len() - 32 - layout.redo().len();
        for _ in 0..n.redo_index() {
            na += u32_at(&self.frame, na + 20) as usize;
            ra += u32_at(&self.frame, ra + 4) as usize;
        }
        (na, ra)
    }
    fn core_at(&self) -> usize {
        if self.frame[7] == 3 { 3488 } else { 1888 }
    }
    /// Rebind mutated pages to frame too, so semantic negatives reach the new
    /// observer beyond byte equality. No product encoder creates their states.
    fn bind_pages(&mut self) {
        let (na, ra) = self.offsets();
        for (cell, body, digest) in [
            (&self.before[16128..16352], na + 128, na + 24),
            (&self.after[16128..16352], na + 352, na + 56),
        ] {
            self.frame[body..body + 224].copy_from_slice(cell);
            self.frame[digest..digest + 32].copy_from_slice(&content_digest(cell));
        }
        let off = usize::from(u16_at(&self.frame, ra + 56));
        let len = u32_at(&self.frame, ra + 4) as usize;
        self.frame[ra + 64..ra + len].copy_from_slice(&self.after[off..16352]);
        self.seal_pages();
        self.reseal_frame();
    }
}
fn reject(name: &str, edit: impl FnOnce(&mut Case)) {
    let mut c = case(name);
    edit(&mut c);
    assert!(c.observe().is_err(), "accepted mutant {name}");
}
fn state(home: crate::storage::status_home::StatusHome) -> TransactionState {
    let TransactionStatusCell::Occupied(s) = home.core() else {
        panic!("occupied")
    };
    s.state()
}

#[test]
fn status_independent_twenty_four_local_pairs() {
    assert_eq!(INDEX.lines().count(), 24);
    for line in INDEX.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let at: usize = f[1].parse().unwrap();
        let len: usize = f[2].parse().unwrap();
        let hash: String = content_digest(&INPUT[at..at + len + 2 * PAGE_BYTES])
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hash, f[6]);
        let c = case(f[0]);
        let before = c.before;
        let after = c.after;
        let observation = c.observe().unwrap_or_else(|e| panic!("{}: {e:?}", f[0]));
        assert_eq!(
            observation.before(),
            crate::storage::status_home::StatusHome::decode(&before[16128..16352]).unwrap()
        );
        assert_eq!(
            observation.after(),
            crate::storage::status_home::StatusHome::decode(&after[16128..16352]).unwrap()
        );
        assert_eq!(c.before, before);
        assert_eq!(c.after, after);
    }
}
#[test]
fn status_statement_marks_and_full_recovery_are_distinct() {
    for name in [
        "forward-retained",
        "rollback-statement-mark",
        "clr-statement-last-mark",
        "finish-retained",
    ] {
        reject(name, |c| {
            c.context.statement = None;
        });
        reject(name, |c| {
            c.context.statement.as_mut().unwrap().sequence += 1;
        });
    }
    for name in [
        "rollback-statement-mark",
        "clr-statement-last-mark",
        "finish-retained",
    ] {
        reject(name, |c| {
            c.context.statement.as_mut().unwrap().mark = Some(undo(11));
        });
    }
    for name in [
        "begin",
        "escalate-statement-full",
        "clr-full-retained",
        "commit-retained",
        "abort-end",
        "prune-retained",
        "terminal-abort",
    ] {
        reject(name, |c| {
            c.context.statement = Some(StatementBoundary {
                sequence: 3,
                mark: None,
            });
        });
    }
    let full = case("clr-full-retained");
    assert_eq!(full.layout().core().statement(), 2);
    assert!(full.context.statement.is_none());
    assert_eq!(full.observe().unwrap().after().incoming(), 2);
    let finish = case("finish-retained").observe().unwrap();
    assert_eq!(finish.after().incoming(), 3);
    assert_eq!(state(finish.after()), TransactionState::Active);
    // A successful Active statement has no StatementFinish WAL.
    reject("finish-retained", |c| {
        let a = c.after;
        c.before[16128..16352].copy_from_slice(&a[16128..16352]);
        c.bind_pages();
    });
    // Full rollback cannot be restarted by a statement rollback or full Start.
    reject("rollback-statement-none", |c| {
        c.before[16166] = 2;
        c.before[16167] = 2;
        put(&mut c.before, 16192, 5, 1);
        let u = undo(14);
        u.encode(&mut c.before[16280..16320]).unwrap();
        c.bind_pages();
    });
    reject("rollback-full-head", |c| {
        let a = c.after;
        c.before[16128..16352].copy_from_slice(&a[16128..16352]);
        c.bind_pages();
    });
}
#[test]
fn status_rejects_local_state_and_head_mutants() {
    for (name, at, value, width) in [
        ("forward-retained", 16166, 3, 1), // committed before needs q too below
        ("clr-full-retained", 16166, 1, 1),
        ("prune-retained", 16224, 0, 1),
        ("abort-end", 16167, 1, 1),
        ("terminal-abort", 16166, 1, 1),
    ] {
        reject(name, |c| {
            put(&mut c.before, at, value, width);
            c.bind_pages();
        });
    }
    // Well-formed actor state, wrong head for Forward/CLR/Prune.
    for name in ["forward-retained", "clr-full-retained", "prune-retained"] {
        reject(name, |c| {
            undo(11).encode(&mut c.before[16232..16272]).unwrap();
            if name == "clr-full-retained" {
                undo(11).encode(&mut c.before[16312..16352]).unwrap();
            }
            c.bind_pages();
        });
    }
    // q cannot silently change on Prune; Commit must use the given q.
    reject("prune-retained", |c| {
        put(&mut c.after, 16216, 10, 8);
        c.bind_pages();
    });
    reject("commit-retained", |c| {
        c.context.commit = Some(CommitSeq::new(10).unwrap());
    });
    reject("commit-retained", |c| {
        c.context.commit = None;
    });
    reject("forward-empty", |c| {
        c.context.commit = Some(CommitSeq::new(9).unwrap());
    });
}
#[test]
fn status_counts_never_wrap_or_discard_retained_heads() {
    reject("forward-retained", |c| {
        put(&mut c.before, 16136, u64::MAX, 8);
        c.bind_pages();
    });
    for name in ["clr-full-retained", "prune-retained"] {
        reject(name, |c| {
            put(&mut c.before, 16136, 0, 8);
            c.bind_pages();
        });
        reject(name, |c| {
            put(&mut c.after, 16136, 0, 8);
            c.bind_pages();
        });
        reject(name, |c| {
            put(&mut c.after, 16136, 1, 8);
            c.bind_pages();
        });
    }
    for name in ["clr-full-last", "prune-last"] {
        reject(name, |c| {
            put(&mut c.before, 16136, 2, 8);
            c.bind_pages();
        });
    }
    reject("finish-retained", |c| {
        put(&mut c.after, 16136, 1, 8);
        c.bind_pages();
    });
    reject("commit-retained", |c| {
        put(&mut c.after, 16136, 2, 8);
        c.bind_pages();
    });
    reject("forward-empty", |c| {
        put(&mut c.before, 16136, 1, 8);
        c.bind_pages();
    });
}
#[test]
fn status_generation_and_original_context_binding() {
    reject("begin", |c| {
        put(&mut c.before, 16208, 2, 8);
        c.bind_pages();
    });
    reject("terminal-abort", |c| {
        put(&mut c.after, 16208, 1, 8);
        c.bind_pages();
    });
    reject("forward-empty", |c| {
        c.context.actor = TxnRef::new(c.context.actor.id(), Generation::new(3).unwrap()).unwrap();
    });
    reject("forward-empty", |c| {
        let mut b = [0; 40];
        c.context.location.encode(&mut b).unwrap();
        put(&mut b, 32, 3, 8);
        c.context.location = StatusSlotRef::decode(&b).unwrap();
    });
    reject("forward-empty", |c| {
        let mut b = [0; 40];
        c.context.location.encode(&mut b).unwrap();
        put(&mut b, 24, 1, 2);
        c.context.location = StatusSlotRef::decode(&b).unwrap();
    });
    reject("forward-empty", |c| {
        let mut b = [0; 40];
        c.context.location.encode(&mut b).unwrap();
        put(&mut b, 8, 10, 8);
        c.context.location = StatusSlotRef::decode(&b).unwrap();
    });
    // Same TxnId, different group owner generation still passes wire grammar.
    reject("forward-empty", |c| {
        put(&mut c.frame, 248, 3, 8);
        c.reseal_frame();
    });
    reject("forward-empty", |c| {
        put(&mut c.after, 16208, 3, 8);
        c.bind_pages();
    });
}
#[test]
fn status_native_and_redo_are_bound_to_both_pages() {
    reject("forward-retained", |c| {
        put(&mut c.before, 16136, 1, 8);
        c.seal_pages();
    });
    reject("forward-retained", |c| {
        put(&mut c.after, 16136, 4, 8);
        c.seal_pages();
    });
    reject("rollback-statement-mark", |c| {
        let (_, ra) = c.offsets();
        c.frame[ra + 64 + 8] ^= 1;
        c.reseal_frame();
    });
    reject("forward-empty", |c| {
        let (_, ra) = c.offsets();
        put(&mut c.frame, ra + 48, 99, 8);
        c.reseal_frame();
    });
    reject("forward-empty", |c| {
        let (na, _) = c.offsets();
        put(&mut c.frame, na + 8, 16127, 2);
        c.reseal_frame();
    });
    reject("forward-empty", |c| {
        let (_, ra) = c.offsets();
        put(&mut c.frame, ra + 32, 3, 8);
        c.reseal_frame();
    });
    reject("forward-empty", |c| {
        let (_, ra) = c.offsets();
        put(&mut c.frame, ra + 40, 2, 8);
        c.reseal_frame();
    });
}
#[test]
fn status_page_identity_lsn_and_canonical_single_cell() {
    for (at, value, width) in [
        (32, 10, 8),
        (40, 3, 8),
        (48, 3, 8),
        (56, 2, 8),
        (6, 8, 1),
        (7, 1, 1),
        (64, 0, 8),
        (64, 101, 8),
        (72, 2, 2),
        (74, 136, 2),
    ] {
        reject("forward-empty", |c| {
            put(&mut c.before, at, value, width);
            c.seal_pages();
        });
    }
    reject("forward-empty", |c| {
        put(&mut c.after, 64, 100, 8);
        c.seal_pages();
    });
    // Profile and actor namespace remain external, not inferred from pages.
    let c = case("forward-empty");
    let layout = c.layout();
    let mut context = c.context;
    let mut raw = PROFILES[..4096].to_vec();
    raw[32] ^= 1; // malformed profile is not constructed
    assert!(control::row_profile::decode_profile(&raw, None).is_err());
    let mut actor = [0; 48];
    context.actor.encode(&mut actor).unwrap();
    actor[0] ^= 1;
    context.actor = TxnRef::decode(&actor).unwrap();
    assert!(
        status::observe(
            layout,
            context,
            StatusImages {
                before: &c.before,
                after: &c.after
            }
        )
        .is_err()
    );
}
#[test]
fn status_undo_reference_shape_and_unsupported_operations() {
    for (at, value, width) in [
        (0, 0x100000002, 8),
        (8, 9, 8),
        (8, 64, 8),
        (24, 1, 2),
        (32, 1, 8),
    ] {
        reject("forward-retained", |c| {
            let ca = c.core_at();
            put(&mut c.frame, ca + 32 + at, value, width);
            c.reseal_frame();
        });
    }
    reject("forward-retained", |c| {
        c.context.statement.as_mut().unwrap().sequence = 0;
    });
    reject("forward-retained", |c| {
        c.context.statement.as_mut().unwrap().mark = Some(undo(9));
    });
    // The observed status location remains exactly the external PageRef.
    let c = case("begin");
    let mut raw = [0; 24];
    c.context.location.page().encode(&mut raw).unwrap();
    assert_eq!(PageRef::decode(&raw).unwrap().page_no, 9);
}

/// Change only the source state, with all state-specific fields canonical.
/// The independent table supplies state tags; no product status encoder or
/// transition function computes a before/after oracle.
fn canonical_source_state(c: &mut Case, tag: u8, rollback: u8) {
    let core = &mut c.before[16160..16352];
    let head: [u8; 40] = core[72..112].try_into().unwrap();
    let has_head = core[64] & 1;
    core[6] = tag;
    core[7] = rollback;
    core[56..64].fill(0);
    core[64] = has_head;
    core[112..].fill(0);
    if tag == 2 && has_head != 0 {
        core[64] |= 4;
        core[152..192].copy_from_slice(&head);
    }
    if tag == 3 {
        put(core, 56, 9, 8);
    }
    c.bind_pages();
    let decoded = crate::storage::status_home::StatusHome::decode(&c.before[16128..16352]);
    assert!(
        decoded.is_ok(),
        "negative must pass the lower Status decoder"
    );
}

#[test]
fn status_rejects_canonical_wrong_source_states() {
    // name, canonical source tag, rollback kind; all other source head/count,
    // external options, core and destination match the original operation.
    for (name, tag, rollback) in [
        ("begin", 1, 0),
        ("forward-retained", 3, 0),
        ("rollback-statement-mark", 3, 0),
        ("rollback-full-head", 3, 0),
        ("clr-full-retained", 1, 0),
        ("finish-retained", 1, 0),
        ("commit-retained", 2, 2),
        ("abort-end", 1, 0),
        ("prune-retained", 1, 0),
        ("terminal-abort", 1, 0),
    ] {
        let mut c = case(name);
        if name == "begin" {
            // Reused actor at the requested generation, already Active.
            let donor = case("forward-empty");
            c.before[16128..16352].copy_from_slice(&donor.before[16128..16352]);
        }
        canonical_source_state(&mut c, tag, rollback);
        assert_eq!(c.observe(), Err(Error::OrderMismatch), "{name}");
    }
}

#[test]
fn status_rejects_non_status_operations_and_older_profiles() {
    const WIRE: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/frames.bin"
    ));
    const WIRE_INDEX: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/module-row-format/wire/index.tsv"
    ));
    for op in [11, 12, 16, 21, 22] {
        let name = format!("op-{op}");
        let line = WIRE_INDEX
            .lines()
            .find(|l| l.split('\t').next() == Some(&name))
            .unwrap();
        let f: Vec<_> = line.split('\t').collect();
        let at: usize = f[1].parse().unwrap();
        let len: usize = f[2].parse().unwrap();
        let mut c = case("begin");
        c.frame = WIRE[at..at + len].to_vec();
        assert_eq!(c.observe(), Err(Error::Unsupported), "{name}");
    }
    // Layout remains an already-validated scope3 view while the external
    // owner profile is deliberately different. No fallback observer is used.
    let c = case("begin");
    for legacy in [1, 2] {
        let mut raw = [0; 4096];
        if legacy == 1 {
            control::encode_profile(c.context.profile.spec(), &mut raw).unwrap();
        } else {
            control::encode_catalog_profile(c.context.profile.spec(), &mut raw).unwrap();
        }
        let mut context = c.context;
        context.profile = control::decode_bootstrap_profile(&raw, None).unwrap();
        assert_eq!(context.profile.format(), legacy);
        assert_eq!(
            status::observe(
                c.layout(),
                context,
                StatusImages {
                    before: &c.before,
                    after: &c.after
                }
            ),
            Err(Error::Unsupported)
        );
    }
}
