//! Independent D3-M04 fixed-history recovery model. No production imports.
//! Candidate applies events; the oracle derives complete images algebraically
//! from immutable fixture history, never from the candidate reservation ledger.
use std::collections::{BTreeMap, HashMap, hash_map::Entry};
use std::time::{Duration, Instant};

const NONE: u8 = 4;
const NULL: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fixture {
    ClrChain,
    PartialClr,
    CutQueue,
    CommitCapture,
    BeginRemoved,
}
impl Fixture {
    fn name(self) -> &'static str {
        match self {
            Self::ClrChain => "clr_chain",
            Self::PartialClr => "partial_clr",
            Self::CutQueue => "cut_queue",
            Self::CommitCapture => "commit_capture",
            Self::BeginRemoved => "begin_removed",
        }
    }
    fn initial_prefix(self) -> u8 {
        u8::from(self == Self::PartialClr)
    }
    fn end(self) -> u8 {
        if self == Self::CommitCapture { 4 } else { 3 }
    }
    fn group(self, at: u8) -> Group {
        if self == Self::CutQueue {
            return [Group::Cut, Group::Release0, Group::Release1][usize::from(at - 1)];
        }
        if self == Self::CommitCapture && at == 1 {
            return Group::Commit;
        }
        let offset = u8::from(self == Self::CommitCapture);
        [Group::ClrLast, Group::ClrFirst, Group::Abort][usize::from(at - offset - 1)]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Commit,
    ClrLast,
    ClrFirst,
    Abort,
    Cut,
    Release0,
    Release1,
}
impl Group {
    fn cost(self) -> u8 {
        if matches!(self, Self::ClrLast | Self::ClrFirst) {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutant {
    None,
    ProgressWithoutEffect,
    CountWithoutCut,
    ReleaseWithoutCursor,
    OmitCommitStatus,
    CaptureMissingDpt,
    ZeroRestoredLedger,
    ReuseConsumedClr,
    StaleHomeOwnership,
    RedoDoubleCharge,
    ClrBeforeWriter,
}
impl Mutant {
    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ProgressWithoutEffect => "progress_without_effect",
            Self::CountWithoutCut => "count_without_cut",
            Self::ReleaseWithoutCursor => "release_without_cursor",
            Self::OmitCommitStatus => "omit_commit_status",
            Self::CaptureMissingDpt => "capture_missing_dpt",
            Self::ZeroRestoredLedger => "zero_restored_ledger",
            Self::ReuseConsumedClr => "reuse_consumed_clr",
            Self::StaleHomeOwnership => "stale_home_ownership",
            Self::RedoDoubleCharge => "redo_double_charge",
            Self::ClrBeforeWriter => "clr_before_writer",
        }
    }
    fn fixture(self) -> Fixture {
        match self {
            Self::CountWithoutCut | Self::ReleaseWithoutCursor => Fixture::CutQueue,
            Self::OmitCommitStatus => Fixture::CommitCapture,
            Self::ReuseConsumedClr => Fixture::PartialClr,
            Self::ZeroRestoredLedger
            | Self::StaleHomeOwnership
            | Self::RedoDoubleCharge
            | Self::ClrBeforeWriter => Fixture::BeginRemoved,
            _ => Fixture::ClrChain,
        }
    }
    fn error(self) -> &'static str {
        match self {
            Self::None => "",
            Self::ProgressWithoutEffect => "clr_effect_progress",
            Self::CountWithoutCut => "edge_count",
            Self::ReleaseWithoutCursor => "queue_release_once",
            Self::OmitCommitStatus => "commit_status",
            Self::CaptureMissingDpt => "capture_lsn_closure",
            Self::ZeroRestoredLedger | Self::StaleHomeOwnership => "reservation_restore",
            Self::ReuseConsumedClr | Self::RedoDoubleCharge => "remaining_obligation",
            Self::ClrBeforeWriter => "writer_not_selected",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Image {
    row: [u8; 2],
    index: [u8; 2],
    root_a: u8,
    head_b: u8,
    next: [u8; 4],
    counts: [u8; 4],
    generation: [u8; 4],
    status: [u8; 4],
    status_generation: [u8; 4],
    queue: u8,
    cursor: u8,
    progress: u8,
    remaining: u8,
    tokens: u8,
    ownership: u8,
    growth: u8,
    att: u8,
    frontier: u8,
    last: u8,
}
impl Image {
    fn candidate_initial(fixture: Fixture) -> Self {
        Self {
            row: [1, 1],
            index: [1, 1],
            root_a: 0,
            head_b: 3,
            next: [1, NONE, NONE, 2],
            counts: [1; 4],
            generation: [0; 4],
            status: [1, u8::from(fixture != Fixture::CommitCapture), 1, 2],
            status_generation: [0, 1, 0, 1],
            queue: NONE,
            cursor: 0,
            progress: 0,
            remaining: 5,
            tokens: 12,
            ownership: 15,
            growth: 0,
            att: if fixture == Fixture::CommitCapture {
                3
            } else {
                2
            },
            frontier: u8::from(fixture != Fixture::CommitCapture),
            last: 0,
        }
    }
    fn payload(self, page: usize) -> u64 {
        fn pack(values: &[u8]) -> u64 {
            values
                .iter()
                .enumerate()
                .fold(0, |n, (i, v)| n | (u64::from(*v) << (i * 4)))
        }
        match page {
            0 => u64::from(self.row[0]),
            1 => u64::from(self.row[1]),
            2 => pack(&self.index),
            3 => pack(&[
                self.root_a,
                self.head_b,
                self.next[0],
                self.next[3],
                self.counts[0],
                self.counts[1],
                self.counts[2],
                self.counts[3],
                self.generation[0],
                self.generation[1],
                self.generation[2],
                self.generation[3],
            ]),
            4 => pack(&[
                self.status[0],
                self.status[1],
                self.status[2],
                self.status[3],
                self.status_generation[0],
                self.status_generation[1],
                self.status_generation[2],
                self.status_generation[3],
                self.att,
            ]),
            5 => pack(&[
                self.ownership,
                self.tokens,
                self.remaining,
                self.growth,
                self.progress,
            ]),
            6 => pack(&[self.queue, self.cursor]),
            7 => pack(&[self.last, self.frontier]),
            _ => unreachable!(),
        }
    }
    fn descriptor(self) -> Descriptor {
        Descriptor {
            remaining: self.remaining,
            tokens: self.tokens,
            ownership: self.ownership,
            growth: self.growth,
        }
    }
}

// This oracle is closed-form history algebra. It does not call candidate_apply,
// group(), restore(), or candidate_initial() and does not consult runtime state.
fn expected_image(fixture: Fixture, prefix: u8) -> Image {
    let commit = fixture == Fixture::CommitCapture;
    let cut = fixture == Fixture::CutQueue;
    let done = if cut {
        0
    } else {
        prefix.saturating_sub(u8::from(commit)).min(2)
    };
    let terminal = !cut && prefix == if commit { 4 } else { 3 };
    let b = [1, 0, NULL][usize::from(done)];
    let released = if cut { prefix.saturating_sub(1) } else { 0 };
    let a_terminal = !commit || prefix != 0;
    Image {
        row: [1, b],
        index: [1, b],
        root_a: if cut && prefix > 0 { NONE } else { 0 },
        head_b: [3, 2, NONE][usize::from(done)],
        next: [
            if released > 0 { NONE } else { 1 },
            NONE,
            NONE,
            if done > 0 { NONE } else { 2 },
        ],
        counts: [
            u8::from(released == 0),
            u8::from(released < 2),
            u8::from(done < 2),
            u8::from(done == 0),
        ],
        generation: [u8::from(released > 0), u8::from(released > 1), 0, 0],
        status: [1, u8::from(a_terminal), 1, if terminal { 3 } else { 2 }],
        status_generation: [0, 1, 0, 1],
        queue: if cut && prefix == 1 {
            0
        } else if cut && prefix == 2 {
            1
        } else {
            NONE
        },
        cursor: released,
        progress: done,
        remaining: if terminal { 0 } else { 5 - 2 * done },
        tokens: [12, 4, 0][usize::from(done)],
        ownership: 15
            & !(if released == 0 {
                0
            } else if released == 1 {
                1
            } else {
                3
            }),
        growth: 0,
        att: u8::from(!a_terminal) | if terminal { 0 } else { 2 },
        frontier: u8::from(a_terminal),
        last: prefix,
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Page {
    payload: u64,
    lsn: u8,
}
fn candidate_pages(fixture: Fixture, prefix: u8) -> [Page; 8] {
    let mut image = Image::candidate_initial(fixture);
    let mut pages = std::array::from_fn(|i| Page {
        payload: image.payload(i),
        lsn: 10,
    });
    for at in 1..=prefix {
        candidate_apply(&mut image, fixture.group(at), at, Mutant::None);
        for (i, page) in pages.iter_mut().enumerate() {
            let payload = image.payload(i);
            if page.payload != payload {
                *page = Page {
                    payload,
                    lsn: 10 + at,
                };
            }
        }
    }
    pages
}
fn expected_page(fixture: Fixture, prefix: u8, page: usize) -> Page {
    let payload = expected_image(fixture, prefix).payload(page);
    let mut lsn = 10;
    for at in 1..=prefix {
        if expected_image(fixture, at).payload(page)
            != expected_image(fixture, at - 1).payload(page)
        {
            lsn = 10 + at;
        }
    }
    Page { payload, lsn }
}
fn candidate_apply(image: &mut Image, group: Group, at: u8, mutant: Mutant) {
    match group {
        Group::Commit => {
            if mutant != Mutant::OmitCommitStatus {
                image.status[1] = 1;
            }
            image.frontier = 1;
            image.att &= !1;
        }
        Group::ClrLast | Group::ClrFirst => {
            let first = group == Group::ClrFirst;
            if mutant != Mutant::ProgressWithoutEffect {
                image.row[1] = if first { NULL } else { 0 };
                image.index[1] = image.row[1];
            }
            image.progress += 1;
            image.head_b = if first { NONE } else { 2 };
            image.next[3] = NONE;
            image.counts[if first { 2 } else { 3 }] = 0;
            image.remaining -= 2;
            image.tokens &= !(1 << if first { 2 } else { 3 });
        }
        Group::Abort => {
            image.status[3] = 3;
            image.att &= !2;
            image.remaining = 0;
        }
        Group::Cut => {
            image.root_a = NONE;
            if mutant == Mutant::CountWithoutCut {
                image.counts[0] = 0;
            }
            image.queue = 0;
        }
        Group::Release0 | Group::Release1 => {
            let slot = usize::from(group == Group::Release1);
            image.queue = if slot == 0 { 1 } else { NONE };
            image.next[slot] = NONE;
            image.counts[slot] = 0;
            image.generation[slot] += 1;
            image.ownership &= !(1 << slot);
            if mutant != Mutant::ReleaseWithoutCursor {
                image.cursor += 1;
            }
        }
    }
    image.last = at;
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Descriptor {
    remaining: u8,
    tokens: u8,
    ownership: u8,
    growth: u8,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct Checkpoint {
    at: u8,
    descriptor: Descriptor,
    head_b: u8,
    progress: u8,
    att: u8,
    dpt: u8,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Phase {
    Run,
    Restart,
    Booted,
    Analyzed,
    Redo,
    Rebase,
    Writer,
    Ready,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct State {
    phase: Phase,
    image: Image,
    pages: [Page; 8],
    home: [Page; 8],
    complete: u8,
    applied: u8,
    synced: u8,
    selected_prefix: u8,
    captured: Option<Checkpoint>,
    checkpoint: Checkpoint,
    capture_used: bool,
    analysis: Option<Descriptor>,
    ledger: Option<Descriptor>,
    actual_wal: u8,
    actual_file: u8,
    header_bytes: u8,
    bootstrap: u8,
    frames: u8,
    frame_peak: u8,
    redo_cursor: u8,
    recovery: bool,
    writer_selected: bool,
    crashes: u8,
    observed: Option<[u8; 2]>,
    restore_count: u8,
    illegal_clr: bool,
}
impl State {
    fn initial(fixture: Fixture) -> Self {
        let at = fixture.initial_prefix();
        let mut image = Image::candidate_initial(fixture);
        if at == 1 {
            candidate_apply(&mut image, Group::ClrLast, 1, Mutant::None);
        }
        let pages = candidate_pages(fixture, at);
        let mut home = candidate_pages(fixture, 0).map(|mut p| {
            p.lsn = 9;
            p
        });
        if fixture == Fixture::PartialClr {
            home[1] = pages[1];
        }
        if fixture == Fixture::BeginRemoved {
            home[5].payload = 0;
        }
        let checkpoint = Checkpoint {
            at,
            descriptor: image.descriptor(),
            head_b: image.head_b,
            progress: image.progress,
            att: image.att,
            dpt: (0..8).fold(0, |mask, i| {
                mask | (u8::from(home[i].lsn < pages[i].lsn) << i)
            }),
        };
        let restart = fixture == Fixture::BeginRemoved;
        Self {
            phase: if restart { Phase::Restart } else { Phase::Run },
            image,
            pages,
            home,
            complete: if fixture == Fixture::CommitCapture {
                1
            } else {
                at
            },
            applied: at,
            synced: at,
            selected_prefix: at,
            captured: None,
            checkpoint,
            capture_used: false,
            analysis: None,
            ledger: if restart {
                None
            } else {
                Some(image.descriptor())
            },
            actual_wal: if at == 1 {
                6
            } else if fixture == Fixture::CommitCapture {
                5
            } else {
                4
            },
            actual_file: 8,
            header_bytes: u8::from(fixture == Fixture::PartialClr),
            bootstrap: 0,
            frames: 0,
            frame_peak: 0,
            redo_cursor: 0,
            recovery: matches!(fixture, Fixture::PartialClr | Fixture::BeginRemoved),
            writer_selected: fixture == Fixture::PartialClr,
            crashes: u8::from(fixture == Fixture::PartialClr),
            observed: None,
            restore_count: 0,
            illegal_clr: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Append,
    PrepareFrames,
    Apply,
    WalSync,
    HomeFlush(usize),
    Capture,
    Select,
    Crash(u8),
    BootReserve,
    Analysis,
    Restore,
    Redo,
    Rebase,
    WriterSelect,
    VerifyReady,
    EarlyClr,
}
impl Action {
    fn name(self) -> String {
        match self {
            Self::Append => "append_group".into(),
            Self::PrepareFrames => "prepare_group_frames".into(),
            Self::Apply => "apply_group".into(),
            Self::WalSync => "wal_sync".into(),
            Self::HomeFlush(i) => format!("home_flush_{i}"),
            Self::Capture => "checkpoint_capture".into(),
            Self::Select => "checkpoint_select".into(),
            Self::Crash(p) => format!("crash_prefix_{p}"),
            Self::BootReserve => "boot_reserve".into(),
            Self::Analysis => "analysis".into(),
            Self::Restore => "reserve_restore".into(),
            Self::Redo => "redo_page".into(),
            Self::Rebase => "rebase".into(),
            Self::WriterSelect => "writer_select".into(),
            Self::VerifyReady => "verify_ready".into(),
            Self::EarlyClr => "early_clr".into(),
        }
    }
}
fn actions() -> Vec<Action> {
    let mut result = vec![
        Action::Append,
        Action::PrepareFrames,
        Action::Apply,
        Action::WalSync,
        Action::Capture,
        Action::Select,
        Action::BootReserve,
        Action::Analysis,
        Action::Restore,
        Action::Redo,
        Action::Rebase,
        Action::WriterSelect,
        Action::VerifyReady,
        Action::EarlyClr,
    ];
    result.extend((0..8).map(Action::HomeFlush));
    result.extend((0..=4).map(Action::Crash));
    result
}
fn step(mut s: State, fixture: Fixture, mutant: Mutant, action: Action) -> Option<State> {
    match action {
        Action::Append
            if s.phase == Phase::Run && s.complete == s.applied && s.complete < fixture.end() =>
        {
            if s.recovery && !s.writer_selected {
                return None;
            }
            let cost = fixture.group(s.complete + 1).cost();
            if s.actual_wal + s.header_bytes + cost > 16 {
                return None;
            }
            s.complete += 1;
            s.actual_wal += cost;
        }
        Action::PrepareFrames
            if s.phase == Phase::Run && s.complete > s.applied && s.frames == 0 =>
        {
            s.frames = 3;
            s.frame_peak = 3;
        }
        Action::Apply if s.phase == Phase::Run && s.complete > s.applied && s.frames == 3 => {
            let next = s.applied + 1;
            let group = fixture.group(next);
            if group == Group::Commit && s.synced < next {
                return None;
            }
            candidate_apply(&mut s.image, group, next, mutant);
            for (i, page) in s.pages.iter_mut().enumerate() {
                let payload = s.image.payload(i);
                if page.payload != payload {
                    *page = Page {
                        payload,
                        lsn: 10 + next,
                    };
                }
            }
            s.applied = next;
            s.frames = 0;
            let mut descriptor = s.image.descriptor();
            if mutant == Mutant::ReuseConsumedClr {
                descriptor.remaining = 5;
                descriptor.tokens = 12;
            }
            s.ledger = Some(descriptor);
        }
        Action::WalSync if s.phase == Phase::Run && s.synced < s.complete => {
            s.synced = s.complete;
        }
        Action::HomeFlush(i)
            if s.phase == Phase::Run
                && s.frames == 0
                && s.pages[i] != s.home[i]
                && s.pages[i].lsn <= 10 + s.synced =>
        {
            s.home[i] = s.pages[i];
            s.frame_peak = s.frame_peak.max(1);
        }
        Action::Capture if s.phase == Phase::Run && s.complete == s.applied && !s.capture_used => {
            let dpt = (0..8).fold(0, |mask, i| {
                mask | (u8::from(s.home[i].lsn < s.pages[i].lsn) << i)
            });
            s.capture_used = true;
            s.captured = Some(Checkpoint {
                at: s.applied,
                descriptor: s.image.descriptor(),
                head_b: s.image.head_b,
                progress: s.image.progress,
                att: s.image.att,
                dpt: if mutant == Mutant::CaptureMissingDpt {
                    0
                } else {
                    dpt
                },
            });
        }
        Action::Select
            if s.phase == Phase::Run
                && s.captured
                    .is_some_and(|cp| cp.at <= s.synced && cp != s.checkpoint) =>
        {
            s.checkpoint = s.captured.expect("guarded capture");
        }
        Action::Crash(prefix)
            if s.crashes < 2
                && prefix >= s.synced
                && prefix <= s.complete
                && prefix >= s.checkpoint.at =>
        {
            s.phase = Phase::Restart;
            s.complete = prefix;
            s.synced = prefix;
            s.selected_prefix = prefix;
            s.applied = s.checkpoint.at;
            s.captured = None;
            s.image = Image::candidate_initial(fixture);
            s.pages = [Page { payload: 0, lsn: 0 }; 8];
            s.ledger = None;
            s.analysis = None;
            s.bootstrap = 0;
            s.frames = 0;
            s.redo_cursor = 0;
            s.recovery = true;
            s.writer_selected = false;
            s.observed = None;
            s.restore_count = 0;
            s.crashes += 1;
        }
        Action::BootReserve if s.phase == Phase::Restart => {
            s.bootstrap = 2;
            s.phase = Phase::Booted;
        }
        Action::Analysis if s.phase == Phase::Booted => {
            // Descriptor reconstruction uses the persisted checkpoint plus suffix groups.
            let mut image = Image::candidate_initial(fixture);
            for at in 1..=s.checkpoint.at {
                candidate_apply(&mut image, fixture.group(at), at, Mutant::None);
            }
            image.head_b = s.checkpoint.head_b;
            image.progress = s.checkpoint.progress;
            image.remaining = s.checkpoint.descriptor.remaining;
            image.tokens = s.checkpoint.descriptor.tokens;
            image.ownership = s.checkpoint.descriptor.ownership;
            image.growth = s.checkpoint.descriptor.growth;
            for at in (s.checkpoint.at + 1)..=s.selected_prefix {
                candidate_apply(&mut image, fixture.group(at), at, Mutant::None);
            }
            s.image = image;
            s.analysis = Some(image.descriptor());
            s.phase = Phase::Analyzed;
        }
        Action::Restore if s.phase == Phase::Analyzed => {
            let mut descriptor = s.analysis.expect("analysis precedes restore");
            if mutant == Mutant::ZeroRestoredLedger {
                descriptor.remaining = 0;
                descriptor.tokens = 0;
            }
            if mutant == Mutant::StaleHomeOwnership {
                descriptor.ownership = (s.home[5].payload & 15) as u8;
            }
            s.ledger = Some(descriptor);
            s.analysis = None;
            s.restore_count += 1;
            s.bootstrap = 0;
            s.phase = Phase::Redo;
        }
        Action::Redo if s.phase == Phase::Redo && s.redo_cursor < 8 => {
            let page = usize::from(s.redo_cursor);
            let replay = candidate_pages(fixture, s.selected_prefix);
            if s.home[page].lsn < replay[page].lsn {
                s.home[page] = replay[page];
            }
            s.pages[page] = s.home[page];
            s.redo_cursor += 1;
            s.frame_peak = s.frame_peak.max(1);
            if mutant == Mutant::RedoDoubleCharge {
                let ledger = s.ledger.as_mut().expect("restored ledger");
                ledger.remaining = ledger.remaining.saturating_sub(1);
            }
            if s.redo_cursor == 8 {
                s.applied = s.selected_prefix;
                s.phase = Phase::Rebase;
            }
        }
        Action::Rebase if s.phase == Phase::Rebase => {
            s.actual_wal = 4;
            s.header_bytes = 0;
            s.phase = Phase::Writer;
        }
        Action::WriterSelect if s.phase == Phase::Writer => {
            if s.actual_wal + s.header_bytes + 1 > 16 {
                return None;
            }
            s.header_bytes += 1;
            s.writer_selected = true;
            s.phase = Phase::Run;
        }
        Action::VerifyReady
            if s.phase == Phase::Run
                && s.applied == fixture.end()
                && s.synced == s.applied
                && s.frames == 0
                && fixture != Fixture::CutQueue =>
        {
            s.observed = Some(s.image.row);
            s.phase = Phase::Ready;
        }
        Action::EarlyClr if mutant == Mutant::ClrBeforeWriter && s.phase == Phase::Analyzed => {
            s.actual_wal += 2;
            s.illegal_clr = true;
        }
        _ => return None,
    }
    Some(s)
}

fn invariant(s: State, fixture: Fixture) -> Result<(), &'static str> {
    if s.illegal_clr {
        return Err("writer_not_selected");
    }
    if s.frames > 3 || s.frame_peak > 3 {
        return Err("frame_ownership");
    }
    if s.actual_file != 8 || s.actual_wal + s.header_bytes + s.bootstrap > 16 {
        return Err("physical_byte_accounting");
    }
    let prefix = if matches!(
        s.phase,
        Phase::Analyzed | Phase::Redo | Phase::Rebase | Phase::Writer
    ) {
        s.selected_prefix
    } else {
        s.applied
    };
    let expected = expected_image(fixture, prefix);
    if !matches!(s.phase, Phase::Restart | Phase::Booted) {
        let mut counts = [0; 4];
        for pointer in [s.image.root_a, s.image.head_b, s.image.queue]
            .into_iter()
            .chain(s.image.next)
        {
            if pointer != NONE {
                counts[usize::from(pointer)] += 1;
            }
        }
        if counts != s.image.counts {
            return Err("edge_count");
        }
        if s.image.cursor != expected.cursor || s.image.generation != expected.generation {
            return Err("queue_release_once");
        }
        if s.image.status != expected.status
            || s.image.frontier != expected.frontier
            || s.image.att != expected.att
        {
            return Err("commit_status");
        }
        if s.image.row != expected.row
            || s.image.index != expected.index
            || s.image.progress != expected.progress
            || s.image.head_b != expected.head_b
        {
            return Err("clr_effect_progress");
        }
        if s.image.descriptor() != expected.descriptor() {
            return Err("remaining_obligation");
        }
    }
    if let Some(ledger) = s.ledger {
        if ledger != expected.descriptor() {
            return Err(if s.phase == Phase::Redo && s.redo_cursor == 0 {
                "reservation_restore"
            } else {
                "remaining_obligation"
            });
        }
        if ledger.remaining + s.bootstrap > 8
            || s.actual_file + ledger.growth > 16
            || s.actual_wal + s.header_bytes + ledger.remaining + s.bootstrap > 16
        {
            return Err("reservation_capacity");
        }
        // Existing owned page tokens are identities within already charged file bytes.
        if ledger.tokens & !ledger.ownership != 0 {
            return Err("page_token_ownership");
        }
    }
    for cp in s.captured.into_iter().chain([s.checkpoint]) {
        let at = expected_image(fixture, cp.at);
        if cp.head_b != at.head_b
            || cp.progress != at.progress
            || cp.descriptor != at.descriptor()
            || cp.att != at.att
        {
            return Err("capture_lsn_closure");
        }
    }
    for i in 0..8 {
        if s.home[i].lsn > 10 + s.synced {
            return Err("wal_before_home");
        }
        if s.home[i].lsn >= 10
            && !(0..=s.synced).any(|at| expected_page(fixture, at, i) == s.home[i])
        {
            return Err("home_image");
        }
    }
    if matches!(s.phase, Phase::Rebase | Phase::Writer) {
        for i in 0..8 {
            if s.pages[i] != expected_page(fixture, s.selected_prefix, i) {
                return Err("redo_convergence");
            }
        }
    }
    if s.phase == Phase::Ready && s.observed != Some([1, NULL]) {
        return Err("winner_loser_reader");
    }
    Ok(())
}

// Checkpoint capture-time home LSNs are verifier inputs, never persisted CP
// fields. Once independently checked they have no future transition effects.
fn transition_invariant(
    before: State,
    after: State,
    action: Action,
    fixture: Fixture,
) -> Result<(), &'static str> {
    invariant(after, fixture)?;
    if matches!(action, Action::Capture) {
        let cp = after.captured.ok_or("capture_lsn_closure")?;
        let dpt = (0..8).fold(0, |mask, i| {
            mask | (u8::from(before.home[i].lsn < expected_page(fixture, cp.at, i).lsn) << i)
        });
        if cp.dpt != dpt {
            return Err("capture_lsn_closure");
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Budget {
    states: usize,
    transitions: usize,
    time: Duration,
}
const BUDGET: Budget = Budget {
    states: 1_000_000,
    transitions: 8_000_000,
    time: Duration::from_secs(60),
};

struct Node {
    state: State,
    parent: Option<(usize, Action)>,
    depth: u8,
}
struct Report {
    states: usize,
    transitions: usize,
    depth: u8,
    elapsed_ms: u128,
    incomplete: bool,
    unexplored: usize,
    error: Option<&'static str>,
    trace: Vec<Action>,
    replayed: bool,
    witnesses_replayed: bool,
    witnesses: BTreeMap<&'static str, Vec<Action>>,
}

fn trace(nodes: &[Node], mut at: usize, last: Action) -> Vec<Action> {
    let mut result = vec![last];
    while let Some((parent, action)) = nodes[at].parent {
        result.push(action);
        at = parent;
    }
    result.reverse();
    result
}

fn witness_predicates(
    state: State,
    next: State,
    action: Action,
    fixture: Fixture,
    mutant: Mutant,
) -> Vec<(&'static str, bool)> {
    [
        (
            "append_apply_gap",
            next.phase == Phase::Run && next.complete > next.applied,
        ),
        (
            "clr_atomic",
            next.phase == Phase::Run && next.image.progress > 0,
        ),
        ("home_flush", matches!(action, Action::HomeFlush(_))),
        ("checkpoint_capture", next.captured.is_some()),
        ("reader_winner_loser", next.phase == Phase::Ready),
        ("two_crashes", next.crashes == 2),
        ("group_frames_three", next.frames == 3),
        (
            "recovery_writer_clr",
            next.phase == Phase::Run
                && next.recovery
                && next.writer_selected
                && matches!(action, Action::Apply)
                && next.image.progress > state.image.progress,
        ),
        (
            "partial_clr_recrash",
            matches!(action, Action::Crash(_)) && state.image.progress == 1 && state.crashes == 1,
        ),
        (
            "remaining_three",
            next.ledger.is_some_and(|r| r.remaining == 3),
        ),
        (
            "cut_count_queue",
            next.phase == Phase::Run && next.image.queue == 0 && next.image.root_a == NONE,
        ),
        (
            "cursor_one_generation",
            next.image.cursor == 1 && next.image.generation[0] == 1,
        ),
        (
            "cursor_two_generation",
            next.image.cursor == 2 && next.image.generation[1] == 1,
        ),
        (
            "commit_capture_blocked",
            fixture == Fixture::CommitCapture
                && next.complete > next.applied
                && step(next, fixture, mutant, Action::Capture).is_none(),
        ),
        (
            "committed_capture",
            fixture == Fixture::CommitCapture
                && next.captured.is_some_and(|cp| cp.at > 0 && cp.att & 1 == 0),
        ),
        (
            "analysis_without_begin",
            fixture == Fixture::BeginRemoved && next.phase == Phase::Analyzed,
        ),
        ("boot_reserve", next.bootstrap == 2),
        (
            "reservation_restored",
            next.restore_count == 1 && next.ledger.is_some(),
        ),
        ("redo_all_pages", next.redo_cursor == 8),
        (
            "restore_recrash",
            matches!(action, Action::Crash(_)) && state.restore_count > 0,
        ),
        (
            "boot_recrash",
            matches!(action, Action::Crash(_)) && state.bootstrap > 0,
        ),
        (
            "writer_recrash",
            matches!(action, Action::Crash(_)) && state.writer_selected,
        ),
    ]
    .to_vec()
}

// Exact future-behaviour quotient approved separately after the initial cap.
// Nodes retain their original concrete state and replayable predecessor trace.
fn visited_key(mut state: State) -> State {
    if matches!(state.phase, Phase::Run | Phase::Ready) {
        state.selected_prefix = 0;
    }
    // DPT is independently checked at capture. This model redoes every page;
    // selected/captured DPT only affect an otherwise inert Select stutter.
    state.checkpoint.dpt = 0;
    if let Some(captured) = state.captured.as_mut() {
        captured.dpt = 0;
    }
    if state.captured == Some(state.checkpoint) {
        state.captured = None;
    }
    state.frame_peak = 0;
    state
}

fn explore(fixture: Fixture, mutant: Mutant, budget: Budget) -> Report {
    let started = Instant::now();
    let initial = State::initial(fixture);
    let mut nodes = vec![Node {
        state: initial,
        parent: None,
        depth: 0,
    }];
    let mut seen = HashMap::from([(visited_key(initial), 0)]);
    let mut report = Report {
        states: 1,
        transitions: 0,
        depth: 0,
        elapsed_ms: 0,
        incomplete: false,
        unexplored: 0,
        error: None,
        trace: Vec::new(),
        replayed: false,
        witnesses_replayed: false,
        witnesses: BTreeMap::new(),
    };
    if let Err(error) = invariant(initial, fixture) {
        report.error = Some(error);
        return report;
    }
    let mut cursor = 0;
    let actions = actions();
    'search: while cursor < nodes.len() {
        if started.elapsed() >= budget.time || report.states > budget.states {
            report.incomplete = true;
            break;
        }
        let state = nodes[cursor].state;
        let depth = nodes[cursor].depth;
        if depth < 16 {
            for action in actions.iter().copied() {
                let Some(next) = step(state, fixture, mutant, action) else {
                    continue;
                };
                if report.transitions >= budget.transitions || started.elapsed() >= budget.time {
                    report.incomplete = true;
                    break 'search;
                }
                report.transitions += 1;
                report.depth = report.depth.max(depth + 1);
                if let Err(error) = transition_invariant(state, next, action, fixture) {
                    report.error = Some(error);
                    report.trace = trace(&nodes, cursor, action);
                    break 'search;
                }
                let witnesses = witness_predicates(state, next, action, fixture, mutant);
                for (name, reached) in witnesses {
                    if reached {
                        report
                            .witnesses
                            .entry(name)
                            .or_insert_with(|| trace(&nodes, cursor, action));
                    }
                }
                if let Entry::Vacant(entry) = seen.entry(visited_key(next)) {
                    if report.states >= budget.states {
                        report.incomplete = true;
                        break 'search;
                    }
                    entry.insert(nodes.len());
                    nodes.push(Node {
                        state: next,
                        parent: Some((cursor, action)),
                        depth: depth + 1,
                    });
                    report.states += 1;
                }
            }
        }
        cursor += 1;
    }
    report.unexplored = if report.incomplete || report.error.is_some() {
        nodes.len() - cursor
    } else {
        0
    };
    report.elapsed_ms = started.elapsed().as_millis();
    report
}

fn emit(fixture: Fixture, mutant: Mutant, report: &Report) {
    let missing: Vec<_> = if mutant == Mutant::None {
        required_witnesses(fixture)
            .iter()
            .filter(|name| !report.witnesses.contains_key(**name))
            .copied()
            .collect()
    } else {
        Vec::new()
    };
    let status = if report.incomplete {
        "INCONCLUSIVE"
    } else if report.error.is_some() {
        if mutant != Mutant::None && report.error == Some(mutant.error()) && report.replayed {
            "EXPECTED_COUNTEREXAMPLE"
        } else {
            "FAIL"
        }
    } else if !missing.is_empty() || !report.witnesses_replayed {
        "FAIL"
    } else {
        "PASS"
    };
    let trace: Vec<_> = report.trace.iter().map(|a| a.name()).collect();
    let witnesses = report
        .witnesses
        .iter()
        .map(|(name, actions)| {
            let names: Vec<_> = actions.iter().map(|a| a.name()).collect();
            format!("{name:?}:{names:?}")
        })
        .collect::<Vec<_>>()
        .join(",");
    let error = report.error.map_or("null".to_owned(), |e| format!("{e:?}"));
    println!(
        "XADB_DEV3_RECOVERY_MODEL {{\"scope\":\"D3-M04-F01\",\"fixture\":{:?},\"mutant\":{:?},\"status\":{status:?},\"states\":{},\"transitions\":{},\"max_depth\":{},\"elapsed_ms\":{},\"unexplored\":{},\"violation\":{error},\"missing_witnesses\":{missing:?},\"witnesses_replayed\":{},\"counterexample_replayed\":{},\"counterexample\":{trace:?},\"witnesses\":{{{witnesses}}}}}",
        fixture.name(),
        mutant.name(),
        report.states,
        report.transitions,
        report.depth,
        report.elapsed_ms,
        report.unexplored,
        report.witnesses_replayed,
        report.replayed,
    );
}

fn required_witnesses(fixture: Fixture) -> &'static [&'static str] {
    match fixture {
        Fixture::ClrChain => &[
            "append_apply_gap",
            "clr_atomic",
            "home_flush",
            "checkpoint_capture",
            "reader_winner_loser",
            "two_crashes",
            "group_frames_three",
        ],
        Fixture::PartialClr => &[
            "recovery_writer_clr",
            "partial_clr_recrash",
            "remaining_three",
            "reader_winner_loser",
            "writer_recrash",
        ],
        Fixture::CutQueue => &[
            "cut_count_queue",
            "cursor_one_generation",
            "cursor_two_generation",
            "home_flush",
            "two_crashes",
        ],
        Fixture::CommitCapture => &[
            "commit_capture_blocked",
            "committed_capture",
            "reader_winner_loser",
            "two_crashes",
        ],
        Fixture::BeginRemoved => &[
            "analysis_without_begin",
            "boot_reserve",
            "reservation_restored",
            "redo_all_pages",
            "restore_recrash",
            "boot_recrash",
            "writer_recrash",
            "recovery_writer_clr",
        ],
    }
}

fn validate_witnesses(fixture: Fixture, report: &mut Report) {
    if report.incomplete || report.error.is_some() {
        return;
    }
    if required_witnesses(fixture)
        .iter()
        .any(|name| !report.witnesses.contains_key(name))
    {
        report.error = Some("required_witness_missing");
        return;
    }
    for (name, actions) in &report.witnesses {
        let mut state = State::initial(fixture);
        if invariant(state, fixture).is_err() || actions.is_empty() {
            report.error = Some("witness_replay_failed");
            return;
        }
        for (i, action) in actions.iter().enumerate() {
            let Some(next) = step(state, fixture, Mutant::None, *action) else {
                report.error = Some("witness_replay_failed");
                return;
            };
            if transition_invariant(state, next, *action, fixture).is_err() {
                report.error = Some("witness_replay_failed");
                return;
            }
            if i + 1 == actions.len()
                && !witness_predicates(state, next, *action, fixture, Mutant::None)
                    .into_iter()
                    .any(|(key, reached)| key == *name && reached)
            {
                report.error = Some("witness_replay_failed");
                return;
            }
            state = next;
        }
    }
    report.witnesses_replayed = true;
}
fn replay_counterexample(fixture: Fixture, mutant: Mutant, actions: &[Action]) -> bool {
    let mut state = State::initial(fixture);
    if invariant(state, fixture).is_err() || actions.is_empty() {
        return false;
    }
    for (i, action) in actions.iter().enumerate() {
        let Some(next) = step(state, fixture, mutant, *action) else {
            return false;
        };
        let error = transition_invariant(state, next, *action, fixture).err();
        if i + 1 == actions.len() {
            return error == Some(mutant.error());
        }
        if error.is_some() {
            return false;
        }
        state = next;
    }
    false
}

#[test]
fn recovery_atomic_groups_exhaustive() {
    let mut failures = Vec::new();
    for fixture in [
        Fixture::ClrChain,
        Fixture::PartialClr,
        Fixture::CutQueue,
        Fixture::CommitCapture,
        Fixture::BeginRemoved,
    ] {
        let mut report = explore(fixture, Mutant::None, BUDGET);
        validate_witnesses(fixture, &mut report);
        emit(fixture, Mutant::None, &report);
        if report.incomplete || report.error.is_some() {
            failures.push(format!(
                "{} incomplete={} error={:?}",
                fixture.name(),
                report.incomplete,
                report.error
            ));
            continue;
        }
        let required = required_witnesses(fixture);
        for witness in required {
            if !report.witnesses.contains_key(witness) {
                failures.push(format!("{} missing {witness}", fixture.name()));
            }
        }
    }
    for mutant in [
        Mutant::ProgressWithoutEffect,
        Mutant::CountWithoutCut,
        Mutant::ReleaseWithoutCursor,
        Mutant::OmitCommitStatus,
        Mutant::CaptureMissingDpt,
        Mutant::ZeroRestoredLedger,
        Mutant::ReuseConsumedClr,
        Mutant::StaleHomeOwnership,
        Mutant::RedoDoubleCharge,
        Mutant::ClrBeforeWriter,
    ] {
        let fixture = mutant.fixture();
        let mut report = explore(fixture, mutant, BUDGET);
        if report.incomplete || report.error != Some(mutant.error()) {
            emit(fixture, mutant, &report);
            failures.push(format!(
                "{} incomplete={} error={:?}",
                mutant.name(),
                report.incomplete,
                report.error
            ));
            continue;
        }
        if replay_counterexample(fixture, mutant, &report.trace) {
            report.replayed = true;
        } else {
            report.error = Some("counterexample_replay_failed");
            failures.push(format!("{} replay failed", mutant.name()));
        }
        emit(fixture, mutant, &report);
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn recovery_model_budget_exhaustion_is_inconclusive() {
    for budget in [
        Budget {
            states: 1,
            ..BUDGET
        },
        Budget {
            transitions: 0,
            ..BUDGET
        },
        Budget {
            time: Duration::ZERO,
            ..BUDGET
        },
    ] {
        assert!(explore(Fixture::ClrChain, Mutant::None, budget).incomplete);
    }
}
