//! Test-only bounded cuts. Captures are diagnostics, never stage authority.
use super::{Batch, InstallObligation, InstallPhase};
use crate::storage::wal_io::WalFrameLocation;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;

pub(super) const STEP: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Point {
    Snapshot,
    AfterStageOwned,
    AfterPageInstalled,
    AfterDescriptorInstalled,
    AfterDptInstalled,
    BeforeSync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Resume {
    Continue,
    Panic,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Installation {
    pub location: WalFrameLocation,
    pub generation: u64,
    pub first_rec_lsn: u64,
    pub before_counter: u64,
    pub after_counter: u64,
    pub cancel: bool,
    pub phase: InstallPhase,
}

#[derive(Debug)]
pub(super) struct Capture {
    pub descriptor: Vec<u8>,
    pub batch: Option<Batch>,
    pub work_high_water: u64,
    pub terminal_pending: bool,
    pub installation: Option<Installation>,
}

pub(super) fn capture(
    descriptor: &[u8; 1536],
    batch: Option<Batch>,
    work_high_water: u64,
    terminal_pending: bool,
    installation: Option<&InstallObligation<'_>>,
) -> Capture {
    Capture {
        descriptor: descriptor.to_vec(),
        batch,
        work_high_water,
        terminal_pending,
        installation: installation.map(|value| Installation {
            location: value.location,
            generation: value.generation,
            first_rec_lsn: value.first_rec_lsn.0,
            before_counter: value.before_counter,
            after_counter: value.after_counter,
            cancel: value.cancel,
            phase: value.phase,
        }),
    }
}

pub(super) struct Hook {
    point: Point,
    arrived: mpsc::SyncSender<(Point, Capture)>,
    release: Mutex<mpsc::Receiver<Resume>>,
    hits: AtomicUsize,
}
pub(super) struct Controller {
    point: Point,
    arrived: mpsc::Receiver<(Point, Capture)>,
    release: mpsc::SyncSender<Resume>,
}
impl Hook {
    pub(super) fn at(point: Point) -> (Arc<Self>, Controller) {
        let (arrived, receive) = mpsc::sync_channel(1);
        let (release, resume) = mpsc::sync_channel(1);
        (
            Arc::new(Self {
                point,
                arrived,
                release: Mutex::new(resume),
                hits: AtomicUsize::new(0),
            }),
            Controller {
                point,
                arrived: receive,
                release,
            },
        )
    }
    pub(super) fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}
impl Controller {
    pub(super) fn arrived(&self) -> Capture {
        let (point, capture) = self
            .arrived
            .recv_timeout(STEP)
            .expect("dirty cut not reached in 5s");
        assert_eq!(point, self.point);
        capture
    }
    pub(super) fn resume(&self, action: Resume) {
        self.release
            .try_send(action)
            .expect("duplicate/missing dirty resume");
    }
}
pub(super) fn hit(hook: &Option<Arc<Hook>>, point: Point, capture: Capture) {
    let Some(hook) = hook.as_ref().filter(|hook| hook.point == point) else {
        return;
    };
    assert_eq!(
        hook.hits.fetch_add(1, Ordering::SeqCst),
        0,
        "duplicate dirty cut"
    );
    hook.arrived
        .try_send((point, capture))
        .expect("missing dirty controller");
    let resume = hook
        .release
        .lock()
        .unwrap()
        .recv_timeout(STEP)
        .expect("dirty cut controller did not resume in 5s");
    assert_eq!(resume, Resume::Continue, "requested dirty cut unwind");
}
