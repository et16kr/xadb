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
