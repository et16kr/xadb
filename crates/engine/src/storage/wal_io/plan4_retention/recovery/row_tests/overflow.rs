//! OVF-C04 uses frames from C's group producer (the test A encoder for
//! Forward/CLR) and the ordinary recovery dispatcher. Expected pages come
//! from applying WAL redo; claim bytes are constructed from the test history.
use super::*;
use crate::storage::{
    overflow::{MAX_OVERFLOW_PAYLOAD, OverflowHead},
    row_record::{self, RecordExpectation, RecordVersion, ValueRef},
    semantic_row::{
        FRAME_BYTES,
        transition::{ChainImages, ChunkPlan, OverflowReservePlan},
    },
};

const ROOT_JOB: usize = 17;
fn selected(r: &Replay) -> job3::JobView<'_> {
    job3::decode_job(&r.cells[ROOT_JOB], namespace()).unwrap()
}

fn produced(r: &Replay, change: Change<'_>, pages: Pages) -> Vec<u8> {
    let mut after = Box::new([[0; PAGE_BYTES]; 5]);
    let mut frame = vec![0; FRAME_BYTES];
    let o = transition::produce(&r.inputs(change, pages), &mut after, &mut frame).unwrap();
    frame.truncate(o.frame_bytes);
    frame
}

/// Compare every arena page, control row and Job with independent redo and
/// control rules. Exactly one claim may change in a ChunkWrite.
fn replay(r: &mut Replay, frame: &[u8], init: Option<(u64, [u8; 352])>) {
    let before = r.saved();
    let afters = r.redo_afters(frame);
    let (bank, cells) = r.expected_control(frame, None, &afters);
    let result = r.apply(frame).unwrap();
    let mut expected = before.arena.clone();
    for (i, page) in &afters {
        expected[*i as usize * PAGE_BYTES..(*i as usize + 1) * PAGE_BYTES].copy_from_slice(page);
    }
    assert!(r.arena == expected);
    assert!(r.bank == bank && r.cells.as_flattened() == cells);
    let mut claims = before.claims.clone();
    if let Some((n, claim)) = init {
        claims[n as usize * 352..(n as usize + 1) * 352].copy_from_slice(&claim);
        assert_eq!(result.initialized[0].unwrap().identity.page, page_ref(n, 2));
    } else {
        assert!(result.initialized[0].is_none());
    }
    assert!(result.initialized[1].is_none() && result.retired.iter().all(Option::is_none));
    assert!(r.claims.as_flattened() == claims);
    assert_eq!(r.cursor.previous_lsn, before.cursor.0 + 1);
    assert!(r.tail.iter().all(|b| *b == 0));
    r.track(frame, &before);
    r.closure()
        .expect("accepted Row frame must close over current replay");
}

fn reserved() -> (Replay, Vec<u8>, u64) {
    let mut r = h_504(Mode::default());
    let text = "x".repeat(24000);
    let mut value = vec![0; text.len() + 18];
    let used = row_record::encode_canonical_row_borrowed(
        RowSchemaInput::Columns(&r.columns),
        &[ValueRef::Int64(7), ValueRef::Utf8(&text)],
        &mut value,
    )
    .unwrap();
    assert_eq!(used, value.len());
    let plan = OverflowReservePlan {
        terminal: 14,
        container: page_ref(14, 2),
        table: table(),
        statement: 3,
        total: used,
        digest: content_digest(&value),
    };
    let frame = produced(&r, Change::ReserveOverflow(&plan), reserve_pages());
    // H-505 selects the next free Job in the same history as O2a.
    replay(&mut r, &frame, None);
    assert_eq!(selected(&r).pending().unwrap().written, 0);
    let lease = r.cursor.previous_lsn;
    (r, value, lease)
}

fn write(r: &mut Replay, value: &[u8], index: usize, lease: u64, n: u64) {
    let start = index * MAX_OVERFLOW_PAYLOAD;
    let plan = ChunkPlan {
        ordinal: ROOT_JOB,
        allocator: r.page(0),
        digest: content_digest(value),
        payload: &value[start..value.len().min(start + MAX_OVERFLOW_PAYLOAD)],
    };
    let frame = produced(
        r,
        Change::ChunkWrite(&plan),
        Pages {
            jobs: Some(1),
            status: Some(9),
            ..pages()
        },
    );
    let lsn = r.cursor.previous_lsn + 1;
    let mut claim = [0; 352];
    claim[..5].copy_from_slice(&[1, 7, 7, 0, 2]);
    page_ref(n, 2).encode(&mut claim[8..32]).unwrap();
    for (at, v) in [
        (32, 6),
        (40, 1),
        (48, lsn),
        (176, lease),
        (184, 6),
        (192, 1),
        (200, 1),
    ] {
        claim[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    claim[56..60].copy_from_slice(&1u32.to_le_bytes());
    claim[64..96].copy_from_slice(&content_digest(&frame));
    let image = r
        .redo_afters(&frame)
        .into_iter()
        .find(|(id, _)| *id == n)
        .unwrap()
        .1;
    claim[96..128].copy_from_slice(&content_digest(&image));
    r.leased().0.encode(&mut claim[128..176]).unwrap();
    page_ref(12, 2).encode(&mut claim[208..232]).unwrap();
    claim[232..234].copy_from_slice(&(index as u16).to_le_bytes());
    claim[234..236].copy_from_slice(&2u16.to_le_bytes());
    claim[236..240].copy_from_slice(&(value.len() as u32).to_le_bytes());
    if index == 1 {
        let jobs_page = r
            .redo_afters(&frame)
            .into_iter()
            .find(|(id, _)| *id == 3)
            .unwrap()
            .1;
        let (wrong_front, _) = transition::tests::rebind(&frame, lsn - 1, 9, &jobs_page, |cell| {
            page_ref(n + 1, 2).encode(&mut cell[328..352]).unwrap();
        });
        decode_layout(&wrong_front, r.expectation(), profile())
            .expect("framing-valid conflicting Job and INIT");
        r.refused("rebound Job front differs from INIT", &wrong_front);
        r.arena[n as usize * PAGE_BYTES] = 1;
        r.refused("occupied INIT slot", &frame);
        r.arena[n as usize * PAGE_BYTES] = 0;
        r.claims[n as usize][351] = 1;
        r.refused("occupied INIT claim", &frame);
        r.claims[n as usize][351] = 0;
    }
    // ChunkWrite advances the Root revision, apply LSN and Job digest.
    // The budget depends on cleanup, unchanged by ChunkWrite.
    let before_root = cell(r.page(1), 0).to_vec();
    replay(r, &frame, Some((n, claim)));
    let root = cell(r.page(1), 0);
    for range in [0..64, 72..80, 88..128, 160..512] {
        assert_eq!(&root[range.clone()], &before_root[range]);
    }
}

#[test]
fn ovf_c04_replay_partial_cancel() {
    let (mut r, value, lease) = reserved();
    write(&mut r, &value, 1, lease, 15);
    let chunks = [r.page(15)];
    let images = ChainImages {
        allocator: r.page(0),
        pages: &chunks,
    };
    let cancel = Change::OverflowCancel {
        ordinal: ROOT_JOB,
        chain: &images,
        anchor: r.page(12),
    };
    let frame = produced(
        &r,
        cancel,
        Pages {
            jobs: Some(1),
            status: Some(9),
            undo: Some(14),
            ..pages()
        },
    );
    r.arena[16 * PAGE_BYTES - 1] ^= 1;
    r.refused("damaged pending image", &frame);
    r.arena[16 * PAGE_BYTES - 1] ^= 1;
    replay(&mut r, &frame, None);
    assert_eq!(selected(&r).state(), job3::JobState::Free);
}

fn forwarded() -> (Replay, Pages) {
    let (mut r, value, lease) = reserved();
    write(&mut r, &value, 1, lease, 15);
    write(&mut r, &value, 0, lease, 16);
    let chunks = [r.page(16), r.page(15)];
    let images = ChainImages {
        allocator: r.page(0),
        pages: &chunks,
    };
    let job = selected(&r);
    let row = job.row_id().unwrap();
    let actor = job.source().unwrap();
    let head = OverflowHead::new(page_ref(16, 2), value.len(), content_digest(&value)).unwrap();
    let mut record = [0; 232];
    row_record::encode_overflow_record(
        RecordExpectation {
            namespace: namespace(),
            row,
        },
        RecordVersion {
            row,
            stamp: VersionStamp::Transaction(actor),
            logical: LogicalRowVersion::new(actor.id(), job.mutation()).unwrap(),
            undo: job.undo(),
        },
        head,
        &mut record,
    )
    .unwrap();
    let mut cell = [0; 244];
    tree_page::encode_leaf_cell(
        PageKind::RowLeaf,
        0,
        &row.get().to_be_bytes(),
        &record,
        &mut cell,
    )
    .unwrap();
    let row_pages = Pages {
        previous: Some(10),
        ..reserve_pages()
    };
    let forward = Change::ForwardOverflow {
        ordinal: ROOT_JOB,
        statement: boundary(3, None),
        chain: &images,
    };
    let frame = transition::tests::a_group(&r.inputs(forward, row_pages), Some(&cell)).unwrap();
    r.arena[16 * PAGE_BYTES - 1] ^= 1;
    r.refused("damaged Forward suffix", &frame);
    r.arena[16 * PAGE_BYTES - 1] ^= 1;
    replay(&mut r, &frame, None);
    (r, row_pages)
}

#[test]
fn ovf_c04_replay_commit_prune() {
    let (mut r, row_pages) = forwarded();
    let frame = produced(
        &r,
        Change::Commit { terminal: 14 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    replay(&mut r, &frame, None);
    let prune = Change::Prune { ordinal: ROOT_JOB };
    let frame = transition::tests::a_group(&r.inputs(prune, row_pages), None).unwrap();
    replay(&mut r, &frame, None);
    assert_eq!(selected(&r).state(), job3::JobState::RootReleasingCommitted);
    assert!(!selected(&r).overflow());
}

#[test]
fn ovf_c04_replay_forward_compensate_retire() {
    let (mut r, row_pages) = forwarded();
    let frame = produced(
        &r,
        Change::RollbackStart {
            terminal: 14,
            statement: None,
        },
        Pages {
            status: Some(9),
            undo: Some(14),
            ..pages()
        },
    );
    replay(&mut r, &frame, None);
    for (ordinal, p) in [
        (ROOT_JOB, row_pages),
        (
            15,
            Pages {
                undo: Some(10),
                previous: None,
                ..row_pages
            },
        ),
    ] {
        let frame = transition::tests::a_group(
            &r.inputs(
                Change::Clr {
                    ordinal,
                    statement: None,
                },
                p,
            ),
            None,
        )
        .unwrap();
        replay(&mut r, &frame, None);
    }
    let frame = produced(
        &r,
        Change::AbortEnd { terminal: 14 },
        Pages {
            jobs: Some(0),
            status: Some(9),
            ..pages()
        },
    );
    replay(&mut r, &frame, None);
    let chunks = [r.page(16), r.page(15)];
    let images = ChainImages {
        allocator: r.page(0),
        pages: &chunks,
    };
    let retire = Change::ChainRetire {
        ordinal: ROOT_JOB,
        chain: &images,
        anchor: r.page(12),
    };
    let frame = produced(
        &r,
        retire,
        Pages {
            jobs: Some(1),
            ..Pages::default()
        },
    );
    replay(&mut r, &frame, None);
    assert!(selected(&r).chain().is_none());
}
