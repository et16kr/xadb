//! G6 source-only semantic tests with independent saved literal expectations.
use super::*;
use crate::storage::{
    retention_page::tests::{
        self as input, Fixture, Harness, actor, boxed, cases, cell, frame_resign, namespace, put16,
        put32, put64, read64, resign, status_identity, status_ref,
    },
    retention_page::{JobView, RootView},
    wal_frame::{MAX_FRAME_BYTES, WalStreamIdentity},
};
use std::mem::size_of;

const OPS: [RetentionOperation; 4] = [
    RetentionOperation::AgingStart,
    RetentionOperation::Begin,
    RetentionOperation::AbortEnd,
    RetentionOperation::Finish,
];
fn phase(op: RetentionOperation) -> &'static str {
    match op {
        RetentionOperation::AgingStart => "start",
        RetentionOperation::Begin => "begin",
        RetentionOperation::AbortEnd => "abort",
        RetentionOperation::Finish => "finish",
    }
}
fn before_phase(op: RetentionOperation) -> &'static str {
    match op {
        RetentionOperation::AgingStart => "initial",
        RetentionOperation::Begin => "start",
        RetentionOperation::AbortEnd => "begin",
        RetentionOperation::Finish => "abort",
    }
}
fn wal_name(op: RetentionOperation) -> &'static str {
    if op == RetentionOperation::AgingStart {
        "aging-start.wal"
    } else {
        match op {
            RetentionOperation::Begin => "begin.wal",
            RetentionOperation::AbortEnd => "abort.wal",
            RetentionOperation::Finish => "finish.wal",
            _ => unreachable!(),
        }
    }
}
fn expected(f: &Fixture, op: RetentionOperation) -> RetentionExpected {
    let root_only = op == RetentionOperation::AgingStart;
    RetentionExpected {
        frame: FrameExpectation {
            stream: WalStreamIdentity {
                database: namespace().database,
                timeline: 1,
                writer_epoch: 1,
            },
            previous_lsn: match op {
                RetentionOperation::AgingStart => 2,
                RetentionOperation::Begin => 3,
                RetentionOperation::AbortEnd => 5,
                RetentionOperation::Finish => 6,
            },
        },
        profile: f.profile,
        actor: (!root_only).then(|| actor(1, 2)),
        status: (!root_only).then(status_ref),
        status_page: (!root_only).then(status_identity),
        job: (!root_only).then(|| f.job_ref()),
        previous_local: 0,
    }
}
fn before<'a>(
    f: &'a Fixture,
    op: RetentionOperation,
    cells: &'a [[u8; 1024]; 64],
) -> RetentionBefore<'a> {
    match op {
        RetentionOperation::AgingStart => RetentionBefore {
            root: f.reference(1),
            jobs: None,
            status: None,
            sender: None,
            aging: None,
            job_cells: cells,
        },
        RetentionOperation::Begin => RetentionBefore {
            root: f.page("root-start.page"),
            jobs: Some(f.reference(2 + f.ordinal / 15)),
            status: Some(f.page("status-free.page")),
            sender: None,
            aging: Some(f.descriptor("aging-start.descriptor")),
            job_cells: cells,
        },
        RetentionOperation::AbortEnd => RetentionBefore {
            root: f.page("root-begin.page"),
            jobs: Some(f.page("jobs-begin.page")),
            status: Some(f.page("status-rollback.page")),
            sender: Some(f.descriptor("tx-rollback.descriptor")),
            aging: Some(f.descriptor("aging-begin.descriptor")),
            job_cells: cells,
        },
        RetentionOperation::Finish => RetentionBefore {
            root: f.page("root-abort.page"),
            jobs: Some(f.page("jobs-abort.page")),
            status: Some(f.page("status-abort.page")),
            sender: None,
            aging: Some(f.descriptor("aging-abort.descriptor")),
            job_cells: cells,
        },
    }
}
struct Call {
    producer: bool,
    omit_status: bool,
    check: bool,
}
fn attempt(
    f: &Fixture,
    op: RetentionOperation,
    e: RetentionExpected,
    b: RetentionBefore<'_>,
    call: Call,
    frame_input: Option<&[u8]>,
) -> Result<()> {
    let mut root = boxed::<PAGE_BYTES>();
    let mut jobs = boxed::<PAGE_BYTES>();
    let mut status = boxed::<PAGE_BYTES>();
    root.fill(0xa5);
    jobs.fill(0xa5);
    status.fill(0xa5);
    let after = RetentionAfter {
        root: &mut root,
        jobs: if op == RetentionOperation::AgingStart {
            None
        } else {
            Some(&mut jobs)
        },
        status: if op == RetentionOperation::AgingStart || call.omit_status {
            None
        } else {
            Some(&mut status)
        },
    };
    let mut output = vec![0xa5; op.length()];
    let raw = frame_input.unwrap_or_else(|| f.raw(wal_name(op)));
    let result = if call.producer {
        match op {
            RetentionOperation::AgingStart => produce_aging_start(e, b, after, &mut output),
            RetentionOperation::Begin => produce_begin(e, b, after, &mut output),
            RetentionOperation::AbortEnd => produce_abort_end(e, b, after, &mut output),
            RetentionOperation::Finish => produce_finish(e, b, after, &mut output),
        }
    } else {
        match op {
            RetentionOperation::AgingStart => observe_aging_start(raw, e, b, after),
            RetentionOperation::Begin => observe_begin(raw, e, b, after),
            RetentionOperation::AbortEnd => observe_abort_end(raw, e, b, after),
            RetentionOperation::Finish => observe_finish(raw, e, b, after),
        }
    };
    let observed = result?;
    if call.check {
        assert_eq!(observed.operation(), op);
        assert_eq!(observed.bytes(), f.raw(wal_name(op)));
        assert_eq!(observed.header().lsn, e.frame.previous_lsn + 1);
        assert_eq!(observed.header().previous_lsn, e.frame.previous_lsn);
        assert_eq!(observed.header().stream, e.frame.stream);
        assert_eq!(
            observed.aging_after(),
            f.descriptor(&format!("aging-{}.descriptor", phase(op)))
        );
        if op.handoff() {
            assert_eq!(
                observed.sender_after(),
                Some(f.descriptor(&format!("tx-{}.descriptor", phase(op))))
            );
        } else {
            assert!(observed.sender_after().is_none());
        }
        assert_eq!(&*root, f.page(&format!("root-{}.page", phase(op))));
        if op != RetentionOperation::AgingStart {
            assert_eq!(&*jobs, f.page(&format!("jobs-{}.page", phase(op))));
            assert_eq!(&*status, f.page(&format!("status-{}.page", phase(op))));
        } else {
            assert!(jobs.iter().chain(status.iter()).all(|v| *v == 0xa5));
        }
    }
    Ok(())
}
fn exercise(f: &Fixture, op: RetentionOperation, producer: bool) {
    let cells = f.cells(before_phase(op));
    let saved = f.raw(wal_name(op)).to_vec();
    attempt(
        f,
        op,
        expected(f, op),
        before(f, op, &cells),
        Call {
            producer,
            omit_status: false,
            check: true,
        },
        None,
    )
    .unwrap();
    assert_eq!(f.raw(wal_name(op)), saved);
}

#[test]
fn retention_producer_literals() {
    cases("P", 4, |n| exercise(&Fixture::standard(), OPS[n - 1], true));
}
#[test]
fn retention_observer_literals() {
    cases("O", 4, |n| {
        exercise(&Fixture::standard(), OPS[n - 1], false)
    });
}

#[test]
fn retention_semantic_rejections() {
    cases("N", 27, |n| {
        let mut f = Fixture::standard();
        let op = match n {
            1..=7 | 9 | 10 | 17 | 19 | 22 => RetentionOperation::Begin,
            8 | 11..=13 | 16 | 20 | 23 | 25 | 26 => RetentionOperation::AbortEnd,
            _ => RetentionOperation::Finish,
        };
        let phase_before = before_phase(op);
        let mut cells = f.cells(phase_before);
        match n {
            6 => {
                let mut raw = f.raw("status-free.page").to_vec();
                put64(&mut raw, 16128 + 80, 2);
                resign(&mut raw);
                f.replace("status-free.page", raw);
            }
            7 => {
                put64(&mut cells[0], 8, 2);
                f.bind_cells("start", &cells);
                let mut page = f.reference(2).to_vec();
                put64(&mut page, 64, 3);
                page[15328..16352].copy_from_slice(&cells[0]);
                resign(&mut page);
                f.files.insert("references/P02.bin".to_owned(), page);
            }
            8 => {
                let mut raw = f.raw("status-rollback.page").to_vec();
                put64(&mut raw, 16128 + 8, 1);
                resign(&mut raw);
                f.replace("status-rollback.page", raw);
            }
            9 => {
                let mut raw = f.raw("root-start.page").to_vec();
                put64(&mut raw, 64, 5);
                put64(&mut raw, 15840 + 80, 5);
                resign(&mut raw);
                f.replace("root-start.page", raw);
            }
            10 => put64(&mut cells[63], 8, 2),
            11 => {
                let status = f.raw("status-begin.page").to_vec();
                let sender = f.raw("tx-begin.descriptor").to_vec();
                f.replace("status-rollback.page", status);
                f.replace("tx-rollback.descriptor", sender);
            }
            12 => {
                let mut sender = f.raw("tx-rollback.descriptor").to_vec();
                put64(&mut sender, 608, 59753);
                resign(&mut sender);
                f.replace("tx-rollback.descriptor", sender);
            }
            13 => {
                put64(&mut cells[0], 48, 2);
                f.bind_cells("begin", &cells);
            }
            14 => {
                for stem in ["root", "jobs"] {
                    let raw = f.raw(&format!("{stem}-begin.page")).to_vec();
                    f.replace(&format!("{stem}-abort.page"), raw);
                }
                let aging = f.raw("aging-begin.descriptor").to_vec();
                f.replace("aging-abort.descriptor", aging);
                cells = f.cells("abort");
            }
            15 => {
                let mut raw = f.raw("status-begin.page").to_vec();
                put64(&mut raw, 64, 6);
                resign(&mut raw);
                f.replace("status-abort.page", raw);
            }
            25 => {
                // The generic descriptor remains valid; only the closed
                // Begin -> RollbackStart predecessor binding is violated.
                let mut sender = f.raw("tx-rollback.descriptor").to_vec();
                put64(&mut sender, 72, 6);
                put64(&mut sender, 80, 5);
                resign(&mut sender);
                f.replace("tx-rollback.descriptor", sender);
                let mut status = f.raw("status-rollback.page").to_vec();
                put64(&mut status, 64, 6);
                resign(&mut status);
                f.replace("status-rollback.page", status);
            }
            26 => {
                let mut status = f.raw("status-rollback.page").to_vec();
                put64(&mut status, 64, 4);
                resign(&mut status);
                f.replace("status-rollback.page", status);
            }
            27 => {
                let mut status = f.raw("status-abort.page").to_vec();
                put64(&mut status, 64, 5);
                resign(&mut status);
                f.replace("status-abort.page", status);
            }
            _ => {}
        }
        if n == 7 {
            let page = retention_page::decode_jobs_page(f.reference(2), namespace(), 0).unwrap();
            assert_eq!(page.header().lsn.0, 3);
            assert_eq!(
                retention_page::decode_job(page.cell(0).unwrap(), namespace())
                    .unwrap()
                    .spec()
                    .generation
                    .get(),
                2
            );
        }
        if n == 8 {
            let page = status_home::decode_status_home_page(
                f.page("status-rollback.page"),
                status_identity(),
                namespace(),
            )
            .unwrap();
            let home = page.cell(0).unwrap();
            assert_eq!(home.incoming(), 1);
            assert!(
                matches!(home.core(), TransactionStatusCell::Occupied(status)
                if matches!(status.state(), TransactionState::Aborting(_)))
            );
        }
        let mut e = expected(&f, op);
        if n == 3 {
            e.previous_local = 1;
        }
        if n == 5 {
            e.status_page.as_mut().unwrap().owner = ObjectId::new(3).unwrap();
        }
        if n == 25 {
            e.frame.previous_lsn = 6;
        }
        if matches!(n, 25 | 26) {
            let sender = obligation::decode_descriptor(
                f.descriptor("tx-rollback.descriptor"),
                ReservationOwner::Transaction(actor(1, 2)),
                retention_obligation::profile_reference(f.profile),
            )
            .unwrap();
            assert_eq!(sender.chain.birth, 4);
            assert_eq!(sender.chain.groups, 2);
            assert_eq!(sender.chain.apply, if n == 25 { 6 } else { 5 });
            assert_eq!(sender.chain.previous, if n == 25 { 5 } else { 4 });
        }
        if n >= 25 {
            let page_name = if n == 27 {
                "status-abort.page"
            } else {
                "status-rollback.page"
            };
            let home = status_home::decode_status_home_page(
                f.page(page_name),
                status_identity(),
                namespace(),
            )
            .unwrap();
            assert_eq!(
                home.lsn(),
                match n {
                    25 => 6,
                    26 => 4,
                    _ => 5,
                }
            );
        }
        let mut b = before(&f, op, &cells);
        if n == 1 {
            b.aging = None;
        }
        if n == 2 {
            b.sender = Some(f.descriptor("tx-begin.descriptor"));
        }
        let mut raw = f.raw(wal_name(op)).to_vec();
        if (16..=24).contains(&n) {
            match n {
                16 => raw[3488 + 208..3488 + 240].fill(0),
                17 => {
                    let mut zero = [0; 640];
                    for i in 0..5 {
                        zero[128 * i] = i as u8 + 1;
                    }
                    raw[3488 + 208..3488 + 240].copy_from_slice(&content_digest(&zero));
                }
                18 => raw[1888 + 255] = 1,
                19 => raw[3488 + 255] = 1,
                20 => {
                    let at = 3744 + 1152 + 2176;
                    let body = at + 128 + 224;
                    raw[body..body + 224].copy_from_slice(cell(f.page("status-finish.page"), 0));
                    let sha = content_digest(&raw[body..body + 224]);
                    raw[at + 56..at + 88].copy_from_slice(&sha);
                }
                21 => {
                    let at = 2144 + 1152;
                    let body = at + 128 + 1024;
                    put64(&mut raw, body + 8, 3);
                    let sha = content_digest(&raw[body..body + 1024]);
                    raw[at + 56..at + 88].copy_from_slice(&sha);
                }
                22 => raw[160 + 152] ^= 1,
                23 => raw[160 + 184..160 + 216].fill(0),
                24 => put64(&mut raw, 6048 + 48, 5),
                _ => unreachable!(),
            };
            resign(&mut raw);
        }
        let saved = raw.clone();
        let actual = attempt(
            &f,
            op,
            e,
            b,
            Call {
                producer: !(16..25).contains(&n),
                omit_status: n == 4,
                check: false,
            },
            Some(&raw),
        )
        .err();
        let error = match n {
            1 | 2 | 4 => RetentionSemanticError::InvalidInput,
            3 | 6 | 7 | 9 | 25..=27 => RetentionSemanticError::OrderMismatch,
            5 | 13 | 15 => RetentionSemanticError::IdentityMismatch,
            8 | 11 | 14 => RetentionSemanticError::UnsupportedOperation,
            10 => RetentionSemanticError::Retention(RetentionError::Corrupt),
            12 => RetentionSemanticError::BudgetMismatch,
            _ => RetentionSemanticError::Corrupt,
        };
        assert_eq!(actual, Some(error));
        assert_eq!(raw, saved);
    });
}

fn header_from(raw: &[u8]) -> WalFrameHeader {
    WalFrameHeader {
        stream: WalStreamIdentity {
            database: namespace().database,
            timeline: 1,
            writer_epoch: 1,
        },
        kind: match raw[6] {
            1 => FrameKind::Begin,
            2 => FrameKind::Update,
            3 => FrameKind::System,
            6 => FrameKind::AbortEnd,
            _ => panic!("literal kind"),
        },
        lsn: read64(raw, 56),
        previous_lsn: read64(raw, 64),
        txn: if matches!(raw[6], 1 | 2 | 6) {
            Some(actor(1, 2).id())
        } else {
            None
        },
        previous_txn_lsn: read64(raw, 112),
    }
}
#[test]
fn retention_envelope_inputs() {
    cases("E", 32, |n| {
        let f = Fixture::standard();
        let mut raw = f
            .raw(if n == 3 {
                "abort.wal"
            } else if n == 10 {
                "rollback.wal"
            } else {
                "begin.wal"
            })
            .to_vec();
        let mut h = header_from(&raw);
        let mut e = FrameExpectation {
            stream: h.stream,
            previous_lsn: h.previous_lsn,
        };
        if matches!(n, 1 | 4 | 5 | 6 | 31 | 32) {
            let len = match n {
                4 => 191,
                5 => MAX_FRAME_BYTES + 1,
                31 => 192,
                _ => raw.len(),
            };
            let mut out = vec![0xa5; len];
            if n == 1 {
                out[160..len - 32].copy_from_slice(&raw[160..len - 32]);
            }
            if n == 6 {
                h.stream.timeline = 0;
            }
            if n == 31 {
                h.kind = FrameKind::System;
                h.txn = None;
                h.previous_txn_lsn = 0;
            }
            if n == 32 {
                h.txn = None;
            }
            let result =
                wal_frame::finish_retention_envelope(h, if n == 31 { 0 } else { 3 }, &mut out);
            match n {
                1 => {
                    result.unwrap();
                    assert_eq!(out, raw);
                    assert_eq!(&out[160..len - 32], &raw[160..len - 32]);
                }
                31 => {
                    result.unwrap();
                    let v = wal_frame::decode_retention_envelope(&out, e).unwrap();
                    assert_eq!(v.header, h);
                    assert!(v.payload.is_empty());
                    assert_eq!(v.record_count, 0);
                }
                _ => {
                    assert_eq!(
                        result,
                        Err(if n == 4 {
                            WalError::NoSpace
                        } else if n == 5 {
                            WalError::TooLarge
                        } else {
                            WalError::InvalidInput
                        })
                    );
                    assert!(out.iter().all(|byte| *byte == 0xa5));
                }
            }
            return;
        }
        let error = match n {
            2 | 3 => None,
            7 => {
                e.stream.timeline = 0;
                Some(WalError::InvalidInput)
            }
            8 => {
                assert_eq!(
                    wal_frame::decode_frame(&raw, e).err(),
                    Some(WalError::UnsupportedFormat)
                );
                return;
            }
            9 => {
                assert_eq!(
                    wal_frame::decode_semantic_envelope(&raw, e).err(),
                    Some(WalError::UnsupportedFormat)
                );
                return;
            }
            10 => Some(WalError::UnsupportedFormat),
            11 => {
                raw[7] = 4;
                Some(WalError::UnsupportedFormat)
            }
            12 => {
                raw[0] ^= 1;
                Some(WalError::Corrupt)
            }
            13 => {
                put16(&mut raw, 4, 2);
                Some(WalError::UnsupportedFormat)
            }
            14 => {
                raw[10] = 1;
                Some(WalError::Corrupt)
            }
            15 => {
                e.stream.database = crate::api::DatabaseId::new([9; 16]).unwrap();
                Some(WalError::IdentityMismatch)
            }
            16 => {
                e.stream.timeline = 2;
                Some(WalError::IdentityMismatch)
            }
            17 => {
                e.stream.writer_epoch = 2;
                Some(WalError::IdentityMismatch)
            }
            18 => {
                e.previous_lsn = 2;
                Some(WalError::OrderMismatch)
            }
            19 => {
                put64(&mut raw, 56, 3);
                Some(WalError::OrderMismatch)
            }
            20 | 21 => Some(WalError::Corrupt),
            22 => {
                put32(&mut raw, 12, 191);
                Some(WalError::Corrupt)
            }
            23 => {
                let length = raw.len() - 193;
                put32(&mut raw, 16, length as u32);
                Some(WalError::Corrupt)
            }
            24..=26 => Some(WalError::Truncated),
            27 => Some(WalError::Corrupt),
            28 => {
                put16(&mut raw, 8, 159);
                Some(WalError::Corrupt)
            }
            29 => {
                raw[6] = 255;
                Some(WalError::UnsupportedFormat)
            }
            30 => {
                put16(&mut raw, 20, 0);
                None
            }
            _ => unreachable!(),
        };
        if matches!(n,11..=14|19|22|23|28..=30) {
            frame_resign(&mut raw);
        }
        if n == 20 {
            raw[128] ^= 1;
            resign(&mut raw);
        }
        if n == 21 {
            let last = raw.len() - 1;
            raw[last] ^= 1;
        }
        if n == 24 {
            raw.clear();
        }
        if n == 25 {
            raw.truncate(159);
        }
        if n == 26 {
            raw.truncate(raw.len() - 1);
        }
        if n == 27 {
            raw.push(0);
        }
        let saved = raw.clone();
        let result = wal_frame::decode_retention_envelope(&raw, e);
        if let Some(error) = error {
            assert_eq!(result.err(), Some(error));
        } else {
            let v = result.unwrap();
            assert_eq!(v.header, h);
            assert_eq!(v.payload, &raw[160..raw.len() - 32]);
            assert_eq!(v.record_count, if n == 30 { 0 } else { 3 });
        }
        assert_eq!(raw, saved);
    });
}

#[test]
fn retention_error_output_preservation() {
    cases("V", 8, |n| {
        let f = Fixture::standard();
        let mut guarded = vec![0x3c; 114688 + 64];
        let length = match n {
            1 => 511,
            2 => 512,
            3 => 1023,
            4 => 16384,
            5 => 16383,
            6 => 1535,
            7 => 56711,
            8 => 55113,
            _ => unreachable!(),
        };
        guarded[32..32 + length].fill(0xa5);
        let saved = guarded.clone();
        let out = &mut guarded[32..32 + length];
        match n {
            1 | 2 => {
                let mut spec = input::root_spec(cell(f.page("root-start.page"), 0));
                if n == 2 {
                    spec.cursor = 64;
                }
                assert_eq!(
                    retention_page::encode_root(spec, namespace(), out),
                    Err(RetentionError::InvalidInput)
                );
            }
            3 => assert_eq!(
                retention_page::encode_job(input::leased(), namespace(), out),
                Err(RetentionError::InvalidInput)
            ),
            4 | 5 => {
                let cells = f.cells("begin");
                let count = if n == 4 { 14 } else { 15 };
                let values: Vec<_> = cells[..count].iter().map(|c| c.as_slice()).collect();
                assert_eq!(
                    retention_page::encode_jobs_page(namespace(), 0, 4, &values, out),
                    Err(RetentionError::InvalidInput)
                );
            }
            6 => {
                let spec = retention_obligation::decode(
                    f.descriptor("aging-start.descriptor"),
                    f.profile,
                    f.page("root-start.page"),
                )
                .unwrap()
                .descriptor();
                assert_eq!(
                    retention_obligation::encode(&spec, f.profile, f.page("root-start.page"), out),
                    Err(AgingObligationError::Descriptor(
                        ObligationError::InvalidInput
                    ))
                );
            }
            7 | 8 => {
                let op = if n == 7 {
                    RetentionOperation::Begin
                } else {
                    RetentionOperation::Finish
                };
                let cells = f.cells(before_phase(op));
                let mut root = boxed::<PAGE_BYTES>();
                let mut jobs = boxed::<PAGE_BYTES>();
                let mut status = boxed::<PAGE_BYTES>();
                root.fill(0x91);
                jobs.fill(0x92);
                status.fill(0x93);
                let after = RetentionAfter {
                    root: &mut root,
                    jobs: Some(&mut jobs),
                    status: Some(&mut status),
                };
                let e = expected(&f, op);
                let b = before(&f, op, &cells);
                let result = if n == 7 {
                    produce_begin(e, b, after, out)
                } else {
                    produce_finish(e, b, after, out)
                };
                assert_eq!(result.err(), Some(RetentionSemanticError::InvalidInput));
                assert!(
                    root.iter().all(|v| *v == 0x91)
                        && jobs.iter().all(|v| *v == 0x92)
                        && status.iter().all(|v| *v == 0x93)
                );
            }
            _ => unreachable!(),
        }
        assert_eq!(guarded, saved);
    });
}

#[test]
#[ignore]
fn retention_s5_profiles() {
    let mut owned = Vec::new();
    for p in 2..=3 {
        for kind in ["P", "O"] {
            for n in 1..=4 {
                owned.push(format!("G6-SP-P{p}-{kind}{n:02}"));
            }
        }
    }
    let mut h = Harness::new(owned);
    for p in 2..=3 {
        for kind in ["P", "O"] {
            for (n, op) in OPS.into_iter().enumerate() {
                h.case(&format!("G6-SP-P{p}-{kind}{:02}", n + 1), || {
                    exercise(&Fixture::new(p, 0), op, kind == "P")
                });
            }
        }
    }
    h.finish();
}
#[test]
#[ignore]
fn retention_s5_ordinals() {
    let owned: Vec<_> = (1..=63)
        .flat_map(|n| ["BEGIN", "FINISH"].map(move |kind| format!("G6-SO-{n:02}-{kind}")))
        .collect();
    let mut h = Harness::new(owned);
    for n in 1..=63 {
        for (kind, op) in [
            ("BEGIN", RetentionOperation::Begin),
            ("FINISH", RetentionOperation::Finish),
        ] {
            h.case(&format!("G6-SO-{n:02}-{kind}"), || {
                exercise(&Fixture::new(1, n), op, false)
            });
        }
    }
    h.finish();
}
fn reserved(prefix: &str, start: usize, last: usize, op: RetentionOperation) {
    let owned: Vec<_> = (start..=last).map(|i| format!("G6-{prefix}-{i}")).collect();
    let mut h = Harness::new(owned.clone());
    for (i, id) in owned.iter().enumerate() {
        h.case(id, || {
            let f = Fixture::standard();
            let cells = f.cells(before_phase(op));
            let mut raw = f.raw(wal_name(op)).to_vec();
            raw[op.operation_at() + start + i] = 1;
            resign(&mut raw);
            assert_eq!(
                attempt(
                    &f,
                    op,
                    expected(&f, op),
                    before(&f, op, &cells),
                    Call {
                        producer: false,
                        omit_status: false,
                        check: false
                    },
                    Some(&raw),
                ),
                Err(RetentionSemanticError::Corrupt)
            );
        });
    }
    h.finish();
}
#[test]
#[ignore]
fn retention_s5_handoff_reserved() {
    reserved("SH", 240, 254, RetentionOperation::Begin);
}
#[test]
#[ignore]
fn retention_s5_system_reserved() {
    reserved("SX", 232, 254, RetentionOperation::Finish);
}
#[test]
#[ignore]
fn retention_s5_envelope_prefixes() {
    let owned: Vec<_> = (1..=158).map(|n| format!("G6-SE-{n:03}")).collect();
    let mut h = Harness::new(owned.clone());
    for (n, id) in owned.iter().enumerate() {
        h.case(id, || {
            let f = Fixture::standard();
            assert_eq!(
                wal_frame::decode_retention_envelope(
                    &f.raw("begin.wal")[..n + 1],
                    expected(&f, RetentionOperation::Begin).frame
                )
                .err(),
                Some(WalError::Truncated)
            );
        });
    }
    h.finish();
}

#[test]
#[allow(
    clippy::redundant_comparisons,
    reason = "Each protocol frame has its own documented caller loan bound"
)]
fn retention_source_census() {
    let mut h = Harness::new(vec!["G6-ST01".to_owned()]);
    h.case("G6-ST01", || {
        let semantic = super::source_bytes();
        let aging = retention_obligation::source_bytes();
        // Added conservatively to the product semantic closure. Local page
        // codecs can coexist with canonical buffers and two prior cells.
        let page_component = 4 * 1024 + 3 * 512 + 640
            + 6 * size_of::<RootSpec>() + 6 * size_of::<JobSpec>()
            + 4 * size_of::<std::result::Result<RootView<'_>, RetentionError>>()
            + 6 * size_of::<std::result::Result<JobView<'_>, RetentionError>>()
            + 4 * size_of::<std::result::Result<page::PageView<'_>, RetentionError>>()
            + 4 * size_of::<ContentHasher>() + 1024;
        let borrowed = 4 * size_of::<RetentionExpected>()
            + 4 * size_of::<RetentionBefore<'_>>()
            + 3 * size_of::<RetentionAfter<'_>>()
            + 4 * size_of::<Result<RetentionObservation<'_>>>();
        let core = semantic + page_component;
        let after_loan = 3 * PAGE_BYTES;
        let frame_loan = 65536usize;
        let output_loan = after_loan + frame_loan;
        let readonly_loan = 3 * PAGE_BYTES + 2 * 1536 + 4096 + 64 * 1024;
        println!(
            "G6_CENSUS\tG6-ST01\tsemantic={semantic}\taging_included={aging}\tpage_additional={page_component}\tborrowed_included={borrowed}\tcore={core}\tdescriptor={}\tdescriptor_result={}\tsemantic_result={}\troot_result={}\tjob_result={}\tcomputed={}\tpair={}",
            size_of::<DescriptorSpec>(),
            size_of::<std::result::Result<DescriptorSpec, ObligationError>>(),
            size_of::<Result<RetentionObservation<'_>>>(),
            size_of::<std::result::Result<RootView<'_>, RetentionError>>(),
            size_of::<std::result::Result<JobView<'_>, RetentionError>>(),
            size_of::<Computed>(),
            size_of::<Pair<'_>>(),
        );
        println!(
            "G6_CENSUS\tG6-ST01-loans\tafter={after_loan}\tframe={frame_loan}\toutput={output_loan}\treadonly={readonly_loan}\tfixture_payload=550160\tfixture_source_backing_cap=2097152\tgeneration_output_cap=2097152\tlive_resource_peak=NOT_ACCEPTED"
        );
        assert!(core <= 65536, "actual pure source values exceed64KiB");
        assert!(aging <= semantic);
        assert_eq!(output_loan, 114688);
        assert_eq!(readonly_loan, 121856);
        assert!(HANDOFF_BYTES <= frame_loan && FINISH_BYTES <= frame_loan && AGING_START_BYTES <= frame_loan);
    });
    h.finish();
}
