//! New durability tests reuse the existing deterministic I/O port, not its tests.
use super::{test_support::*, *};
use std::{
    sync::{Arc, atomic::Ordering, mpsc},
    time::Instant,
};

pub(super) const STEP: Duration = Duration::from_secs(5);
pub(super) fn session(
    memory: &mut [u8; MAX_FRAME_BYTES],
) -> (WalRuntime<'_, Writer>, Arc<Mutex<State>>) {
    let (writer, state) = fresh();
    let session = GenesisWalSession::create(writer, 1, memory, &mut [0; 16384]).unwrap();
    (session.runtime, state)
}
pub(super) fn issue<'s>(
    session: &'s WalRuntime<'_, Writer>,
    previous: u64,
    lsn: u64,
) -> (WalRequirement<'s>, Vec<u8>) {
    let bytes = raw_frame(previous, lsn, &[0x21, 0x43, 0x65]);
    let prefix = session.prefix().unwrap();
    let header = wal_frame::decode_frame(
        &bytes,
        FrameExpectation {
            stream: prefix.stream,
            previous_lsn: prefix.complete_lsn,
        },
    )
    .unwrap()
    .header();
    let requirement = session
        .stage_verified_required(prefix, header, &bytes)
        .unwrap();
    (requirement, bytes)
}
pub(super) fn calls(state: &Arc<Mutex<State>>) -> usize {
    state.lock().unwrap().events.len()
}
pub(super) fn pause_at(
    state: &Arc<Mutex<State>>,
    at: usize,
    after: bool,
) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (entered, arrival) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    state.lock().unwrap().pause = Some(Pause {
        at,
        after,
        entered,
        release,
        unwind: false,
    });
    (arrival, resume)
}
pub(super) fn waited(session: &WalRuntime<'_, Writer>, minimum: usize) {
    let deadline = Instant::now() + STEP;
    while session.durability_waits.load(Ordering::SeqCst) < minimum {
        assert!(Instant::now() < deadline, "durability wait not reached");
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn retained(session: &WalRuntime<'_, Writer>, expected: WalFrameLocation) {
    let state = session
        .append
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert_eq!(state.state, SlotState::IoOwned(expected));
    assert!(state.slot.is_none());
    assert_eq!(state.prefix.synced_lsn, 0);
    assert_eq!(state.prefix.synced_offset, 4096);
    assert_eq!(state.prefix.complete_lsn, 1);
    assert_eq!(state.prefix.complete_offset, 4291);
}
pub(super) fn fatal(error: &WalIoError) {
    assert_eq!(error.effect, Effect::CallerMustResolve);
    assert!(matches!(error.cause, Cause::Fenced | Cause::Poisoned));
}

// Diagnostic seam has no product state guard or I/O guard at either ACK.
pub(super) struct BeforeWait {
    arrived: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}
pub(super) fn pause_before_wait<W: WriterNamespace>(
    session: &WalRuntime<'_, W>,
) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let (arrived, arrival) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    *session.durability_before_wait.lock().unwrap() = Some(BeforeWait { arrived, release });
    (arrival, resume)
}
pub(super) fn before_wait<W: WriterNamespace>(session: &WalRuntime<'_, W>) {
    let pause = session.durability_before_wait.lock().unwrap().take();
    if let Some(pause) = pause {
        pause.arrived.try_send(()).unwrap();
        pause.release.recv_timeout(STEP).unwrap();
    }
}

// One thread's next physical stage exercises the original Capacity branch.
// This cannot construct a requirement, change the slot/prefix, or supply an
// UnstagedCommit. It is armed only after the S3 fixture finishes its setup.
thread_local! {
    static STAGE_CAPACITY_ONCE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}
pub(super) struct StageCapacityOnce(std::marker::PhantomData<std::rc::Rc<()>>);
pub(super) fn stage_capacity_once() -> StageCapacityOnce {
    STAGE_CAPACITY_ONCE.with(|state| {
        assert_eq!(state.replace(1), 0);
    });
    StageCapacityOnce(std::marker::PhantomData)
}
impl StageCapacityOnce {
    pub(super) fn hits(&self) -> u8 {
        STAGE_CAPACITY_ONCE.with(|state| u8::from(state.get() == 2))
    }
}
impl Drop for StageCapacityOnce {
    fn drop(&mut self) {
        STAGE_CAPACITY_ONCE.with(|state| state.set(0));
    }
}
// One thread's WAL file has only `share` bytes left from `start` (the
// fixture's WAL end) while the guard lives: C's Row WAL admission
// (`check_progress_room`, the frame and every future step's WAL) and the
// physical stage (`stage_verified`) compare with their original `>` against
// min(WAL_FILE_CAP, start + share) instead of WAL_FILE_CAP. Armed without a
// share it only observes. It records the first admission (offset, frame,
// future WAL, refused by the WAL term), every admission and stage, and the
// highest staged end (S11, MCA05-V01..V03, review R1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WalAdmission {
    pub(super) offset: u64,
    pub(super) frame: u64,
    pub(super) future: u64,
    pub(super) refused: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WalShareRecord {
    pub(super) cap: Option<u64>,
    pub(super) first: Option<WalAdmission>,
    pub(super) admissions: u32,
    pub(super) refused_admissions: u32,
    pub(super) stages: u32,
    pub(super) refused_stages: u32,
    pub(super) staged_end: u64,
}
thread_local! {
    static WAL_SHARE: std::cell::Cell<Option<WalShareRecord>> = const { std::cell::Cell::new(None) };
}
pub(super) struct WalShare(std::marker::PhantomData<std::rc::Rc<()>>);
/// `None` observes only; `Some(cap)` is the absolute WAL end the file may reach.
pub(super) fn wal_share(cap: Option<u64>) -> WalShare {
    WAL_SHARE.with(|state| {
        let empty = WalShareRecord {
            cap,
            first: None,
            admissions: 0,
            refused_admissions: 0,
            stages: 0,
            refused_stages: 0,
            staged_end: 0,
        };
        assert_eq!(state.replace(Some(empty)), None);
    });
    WalShare(std::marker::PhantomData)
}
impl WalShare {
    pub(super) fn record(&self) -> WalShareRecord {
        WAL_SHARE.with(|state| state.get().expect("armed"))
    }
}
impl Drop for WalShare {
    fn drop(&mut self) {
        WAL_SHARE.with(|state| state.set(None));
    }
}
/// The cap `check_progress_room` compares `offset + frame + future` with.
pub(super) fn wal_admission_cap(offset: u64, frame: u64, future: u64, cap: u64) -> u64 {
    WAL_SHARE.with(|state| match state.get() {
        None => cap,
        Some(mut record) => {
            let limited = record.cap.map_or(cap, |share| share.min(cap));
            let refused = offset
                .checked_add(frame)
                .and_then(|value| value.checked_add(future))
                .is_none_or(|value| value > limited);
            record.admissions += 1;
            record.refused_admissions += u32::from(refused);
            record.first.get_or_insert(WalAdmission {
                offset,
                frame,
                future,
                refused,
            });
            state.set(Some(record));
            limited
        }
    })
}
/// The cap `stage_verified` compares the staged frame's `end` with.
pub(super) fn wal_stage_cap(end: u64, cap: u64) -> u64 {
    WAL_SHARE.with(|state| match state.get() {
        None => cap,
        Some(mut record) => {
            let limited = record.cap.map_or(cap, |share| share.min(cap));
            record.stages += 1;
            record.refused_stages += u32::from(end > limited);
            record.staged_end = record.staged_end.max(end);
            state.set(Some(record));
            limited
        }
    })
}

pub(super) fn take_stage_capacity_once() -> bool {
    STAGE_CAPACITY_ONCE.with(|state| {
        if state.get() == 1 {
            state.set(2);
            true
        } else {
            false
        }
    })
}
