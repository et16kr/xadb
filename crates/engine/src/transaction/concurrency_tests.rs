//! Independent fixed-array admission oracle and controlled real-thread schedules.
//! Commit transitions in this file are SIMULATED, with no WAL/status/DPT effect.

use super::test_support::{self as support, Installation, PausePoint, Resume};
use super::*;
use std::mem::size_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::Duration;

const STEP: Duration = Duration::from_secs(5);
const NORMAL: &str = include_str!("../../../../tests/fixtures/dev3-concurrency/normal-cases.tsv");
const EXPANDED: &str =
    include_str!("../../../../tests/fixtures/dev3-concurrency/expanded-schedules.tsv");
const FREE: u8 = 0;
const IDLE: u8 = 1;
const APPLY: u8 = 2;
const CONTENT: u8 = 3;
const RETIRED: u8 = 4;
const STAGE: u8 = 5;
const PENDING: u8 = 6;
const INSTALLING: u8 = 7;

/// Expected state is advanced from fixture commands, never copied from observation.
struct Model {
    modes: [u8; 34],
    generations: [u64; 34],
    installation: (u8, usize, u64, u64, Option<u64>),
    installation_generation: u64,
    fenced: bool,
}

impl Model {
    fn new() -> Self {
        Self {
            modes: [FREE; 34],
            generations: [0; 34],
            installation: (0, 0, 0, 0, None),
            installation_generation: 0,
            fenced: false,
        }
    }

    fn issue(&mut self, role: OwnerRole) -> (usize, u64) {
        let mut range = match role {
            OwnerRole::Foreground => 0..32,
            OwnerRole::Maintenance => 32..33,
            OwnerRole::DedicatedFlusher => 33..34,
        };
        let slot = range.find(|slot| self.modes[*slot] == FREE).unwrap();
        self.generations[slot] = self.generations[slot].checked_add(1).unwrap();
        self.modes[slot] = IDLE;
        (slot, self.generations[slot])
    }

    fn release(&mut self, slot: usize) {
        assert_eq!(self.modes[slot], IDLE);
        self.modes[slot] = if self.generations[slot] == u64::MAX {
            RETIRED
        } else {
            FREE
        };
    }

    fn commit(&mut self, kind: u8, slot: usize, lsn: Option<u64>) {
        self.installation = (
            kind,
            slot,
            self.generations[slot],
            self.installation_generation,
            lsn,
        );
    }

    fn open(&mut self) {
        self.installation = (0, 0, 0, 0, None);
    }

    fn check(&self, core: &PageConcurrency<'_>) {
        self.matches(&support::observe(core)).unwrap();
    }

    fn matches(&self, observed: &support::Observation) -> std::result::Result<(), &'static str> {
        let mut apply = 0;
        let mut content = 0;
        for index in 0..34 {
            let actual = match observed.slots[index].mode {
                Mode::Free => FREE,
                Mode::Idle => IDLE,
                Mode::Apply => APPLY,
                Mode::Content => CONTENT,
                Mode::Retired => RETIRED,
                Mode::CommitStage => STAGE,
                Mode::CommitPending => PENDING,
                Mode::CommitDraining => return Err("production drain is outside simulated model"),
                Mode::CommitInstalling => INSTALLING,
            };
            if actual != self.modes[index]
                || observed.slots[index].generation != self.generations[index]
            {
                return Err("slot identity/mode");
            }
            // Literal contract table; no product counter/census helper is called.
            match self.modes[index] {
                APPLY | STAGE | INSTALLING => apply += 1,
                CONTENT => content += 1,
                FREE | IDLE | RETIRED | PENDING => (),
                _ => return Err("unknown oracle mode"),
            }
        }
        if observed.active_apply != apply
            || observed.active_content != content
            || apply + content > 34
        {
            return Err("counter census");
        }
        let installation = match observed.installation {
            Installation::Open => (0, 0, 0, 0, None),
            Installation::Intent(k) => (
                1,
                k.slot,
                k.owner_generation,
                k.installation_generation,
                None,
            ),
            Installation::Pending(k, lsn) => (
                2,
                k.slot,
                k.owner_generation,
                k.installation_generation,
                Some(lsn),
            ),
            Installation::Draining(_, _) => {
                return Err("production drain is outside simulated model");
            }
            Installation::Installing(k, lsn) => (
                3,
                k.slot,
                k.owner_generation,
                k.installation_generation,
                Some(lsn),
            ),
            Installation::Abandoned(k, lsn) => (
                4,
                k.slot,
                k.owner_generation,
                k.installation_generation,
                lsn,
            ),
        };
        if installation != self.installation
            || observed.installation_generation != self.installation_generation
        {
            return Err("installation identity");
        }
        if observed.fenced != self.fenced {
            return Err("fence state");
        }
        Ok(())
    }
}

fn issue<'c, 'f>(
    core: &'c PageConcurrency<'f>,
    model: &mut Model,
    role: OwnerRole,
) -> Owner<'c, 'f> {
    let expected = model.issue(role);
    let owner = core.try_owner(role).unwrap();
    assert_eq!(support::owner_identity(&owner), expected);
    model.check(core);
    owner
}

fn rejected<T>(result: Result<T>, cause: Cause, effect: Effect) {
    match result {
        Ok(_) => panic!("expected {cause:?}/{effect:?}, got success"),
        Err(error) => assert_eq!(error, ConcurrencyError { cause, effect }),
    }
}

fn channel<T>() -> (SyncSender<T>, Receiver<T>) {
    mpsc::sync_channel(1)
}

fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(STEP)
        .expect("required schedule step missing or disconnected")
}

fn send<T>(sender: &SyncSender<T>, value: T) {
    sender
        .try_send(value)
        .unwrap_or_else(|_| panic!("duplicate/out-of-order channel step"));
}

fn expect(receiver: &Receiver<u8>, value: u8) {
    assert_eq!(receive(receiver), value);
}

struct Pause {
    arrived: Receiver<()>,
    release: SyncSender<Resume>,
}

impl Pause {
    fn install(core: &PageConcurrency<'_>, point: PausePoint) -> Self {
        let (arrived_tx, arrived) = channel();
        let (release, release_rx) = channel();
        support::install_pause(core, point, arrived_tx, release_rx);
        Self { arrived, release }
    }

    fn reached(&self) {
        receive(&self.arrived);
    }
    fn resume(&self) {
        send(&self.release, Resume::Continue);
    }
    fn panic(&self) {
        send(&self.release, Resume::Panic);
    }
}

impl Drop for Pause {
    fn drop(&mut self) {
        // Release an instrumented thread on an assertion path; a full queue already
        // contains its release. A product mutex hang still requires root's watchdog.
        let _ = self.release.try_send(Resume::Continue);
    }
}

fn case(name: &str) {
    let matching: Vec<_> = NORMAL
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter(|line| line.split('\t').nth(1) == Some(name))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "missing/duplicate normal fixture case {name}"
    );
    println!("GUARD_CASE {}", matching[0]);
}

#[derive(Clone, Copy)]
struct Schedule {
    id: &'static str,
    family: &'static str,
    enter: usize,
    leave: usize,
    a: OwnerRole,
    b: OwnerRole,
    seed: u64,
    post_check: bool,
    physical_idle: bool,
    foreground: usize,
    path: &'static str,
    physical_stage: &'static str,
    actions: [&'static str; 10],
}

fn role(text: &str) -> OwnerRole {
    match text {
        "Foreground" => OwnerRole::Foreground,
        "Maintenance" => OwnerRole::Maintenance,
        "DedicatedFlusher" => OwnerRole::DedicatedFlusher,
        _ => panic!("unknown fixture role"),
    }
}

fn parse_schedule(line: &'static str) -> Schedule {
    let mut fields = line.split('\t');
    let columns: [&str; 14] = std::array::from_fn(|_| fields.next().expect("short schedule row"));
    assert!(fields.next().is_none(), "long schedule row");
    let mut actions = columns[13].split(',');
    let schedule = Schedule {
        id: columns[0],
        family: columns[1],
        enter: columns[3].parse().unwrap(),
        leave: columns[4].parse().unwrap(),
        a: role(columns[5]),
        b: role(columns[6]),
        seed: columns[7].parse().unwrap(),
        post_check: columns[8] == "after_first_open_check_before_version_lock",
        physical_idle: columns[9] == "1",
        foreground: columns[10].parse().unwrap(),
        path: columns[11],
        physical_stage: columns[12],
        actions: std::array::from_fn(|_| actions.next().expect("short action census")),
    };
    assert!(actions.next().is_none(), "long action census");
    assert!(schedule.enter < schedule.leave && schedule.leave <= 7);
    schedule
}

fn first_schedule(family: &str) -> Schedule {
    EXPANDED
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(parse_schedule)
        .find(|row| row.family == family)
        .unwrap()
}

fn with_content_interval(
    core: &PageConcurrency<'_>,
    flusher: &mut Owner<'_, '_>,
    model: &mut Model,
    schedule: Schedule,
    mut drive: impl FnMut(usize, &mut Model),
) {
    let base = base_actions(schedule.family);
    let mut trace = Vec::with_capacity(10);
    for (step, action) in base.iter().enumerate().take(schedule.enter) {
        drive(step, model);
        trace.push(*action);
    }
    if schedule.family == "GA04" {
        rejected(flusher.try_apply(), Cause::InvalidRole, Effect::Unchanged);
        model.check(core);
    }
    let content = flusher.try_content().unwrap();
    model.modes[33] = CONTENT;
    model.check(core);
    trace.push("C_CONTENT_ENTER");
    for (step, action) in base
        .iter()
        .enumerate()
        .take(schedule.leave)
        .skip(schedule.enter)
    {
        drive(step, model);
        trace.push(*action);
    }
    drop(content);
    model.modes[33] = IDLE;
    model.check(core);
    trace.push("C_CONTENT_DROP");
    for (step, action) in base.iter().enumerate().skip(schedule.leave) {
        drive(step, model);
        trace.push(*action);
    }
    check_trace(
        schedule,
        &trace,
        if schedule.path.starts_with("sim_") {
            "SIMULATED"
        } else {
            schedule.physical_stage
        },
    )
    .unwrap();
}

#[test]
fn concurrency_ga01_capacity_and_generations() {
    case("full_capacity");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut owners: Vec<_> = (0..32)
        .map(|_| Some(issue(&core, &mut model, OwnerRole::Foreground)))
        .collect();
    let maintenance = issue(&core, &mut model, OwnerRole::Maintenance);
    let flusher = issue(&core, &mut model, OwnerRole::DedicatedFlusher);
    for role in [
        OwnerRole::Foreground,
        OwnerRole::Maintenance,
        OwnerRole::DedicatedFlusher,
    ] {
        rejected(core.try_owner(role), Cause::Capacity, Effect::Unchanged);
        model.check(&core);
    }
    case("lowest_slot_reuse");
    drop(owners[7].take());
    model.release(7);
    model.check(&core);
    owners[7] = Some(issue(&core, &mut model, OwnerRole::Foreground));
    assert_eq!(model.generations[7], 2);
    case("reserved_slot_reuse");
    drop(maintenance);
    model.release(32);
    let maintenance = issue(&core, &mut model, OwnerRole::Maintenance);
    drop(flusher);
    model.release(33);
    let flusher = issue(&core, &mut model, OwnerRole::DedicatedFlusher);
    assert_eq!((model.generations[32], model.generations[33]), (2, 2));
    drop(owners);
    for index in 0..32 {
        model.release(index);
    }
    drop(maintenance);
    model.release(32);
    drop(flusher);
    model.release(33);
    model.check(&core);

    case("role_partitions");
    for role in [
        OwnerRole::Foreground,
        OwnerRole::Maintenance,
        OwnerRole::DedicatedFlusher,
    ] {
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut model = Model::new();
        let count = if role == OwnerRole::Foreground { 32 } else { 1 };
        let held: Vec<_> = (0..count).map(|_| issue(&core, &mut model, role)).collect();
        rejected(core.try_owner(role), Cause::Capacity, Effect::Unchanged);
        model.check(&core);
        drop(held);
    }
    for (name, role, slot) in [
        ("maximum_generation_maintenance", OwnerRole::Maintenance, 32),
        (
            "maximum_generation_flusher",
            OwnerRole::DedicatedFlusher,
            33,
        ),
    ] {
        case(name);
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut model = Model::new();
        support::set_slot(&core, slot, u64::MAX - 1, Mode::Free);
        model.generations[slot] = u64::MAX - 1;
        let owner = issue(&core, &mut model, role);
        drop(owner);
        model.release(slot);
        model.check(&core);
        rejected(
            core.try_owner(role),
            Cause::GenerationExhausted,
            Effect::Unchanged,
        );
        model.check(&core);
    }
    case("all_retired_foreground");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    for index in 0..32 {
        support::set_slot(&core, index, u64::MAX, Mode::Retired);
        model.modes[index] = RETIRED;
        model.generations[index] = u64::MAX;
    }
    rejected(
        core.try_owner(OwnerRole::Foreground),
        Cause::GenerationExhausted,
        Effect::Unchanged,
    );
    model.check(&core);
    case("mixed_retired_occupied");
    support::set_slot(&core, 0, 4, Mode::Idle);
    model.modes[0] = IDLE;
    model.generations[0] = 4;
    rejected(
        core.try_owner(OwnerRole::Foreground),
        Cause::Capacity,
        Effect::Unchanged,
    );
    model.check(&core);
    case("corrupt_free_max");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    support::set_slot(&core, 32, u64::MAX, Mode::Free);
    let before = support::observe(&core);
    rejected(
        core.try_owner(OwnerRole::Maintenance),
        Cause::Invariant,
        Effect::CallerMustResolve,
    );
    let after = support::observe(&core);
    assert!(after.fenced);
    assert_eq!(before.slots, after.slots);
    case("simulated_install_generation_max");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut owner = issue(&core, &mut model, OwnerRole::Foreground);
    support::set_installation_generation(&core, u64::MAX);
    model.installation_generation = u64::MAX;
    rejected(
        support::simulated_commit(owner.try_apply().unwrap().try_version().unwrap()),
        Cause::GenerationExhausted,
        Effect::Unchanged,
    );
    model.check(&core);
    complete("GA01");
}

fn installed<'o, 'c, 'f>(
    pending: support::SimulatedInstallation<'o, 'c, 'f>,
) -> support::SimulatedInstallGuard<'o, 'c, 'f> {
    pending
        .try_install()
        .unwrap_or_else(|(error, _)| panic!("SIMULATED install: {error:?}"))
}

fn run_ga02(schedule: Schedule) {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    for role in [schedule.a, schedule.b] {
        let index = if role == OwnerRole::Foreground { 0 } else { 32 };
        support::set_slot(&core, index, schedule.seed, Mode::Free);
        model.generations[index] = schedule.seed;
    }
    let mut flusher = issue(&core, &mut model, OwnerRole::DedicatedFlusher);
    std::thread::scope(|scope| {
        let (at, ar) = channel();
        let (aa, ax) = channel();
        let (ai, aix) = channel();
        let core = &core;
        scope.spawn(move || {
            let mut owner = core.try_owner(schedule.a).unwrap();
            send(&ai, support::owner_identity(&owner));
            expect(&ar, 0);
            let apply = owner.try_apply().unwrap();
            send(&aa, 0);
            expect(&ar, 2);
            let version = apply.try_version().unwrap();
            send(&aa, 2);
            expect(&ar, 4);
            drop(version);
            send(&aa, 4);
            expect(&ar, 255);
            drop(owner);
            send(&aa, 255);
        });
        let (a, ag) = model.issue(schedule.a);
        assert_eq!(receive(&aix), (a, ag));
        model.check(core);
        let (bt, br) = channel();
        let (ba, bx) = channel();
        let (bi, bix) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(schedule.b).unwrap();
            send(&bi, support::owner_identity(&owner));
            expect(&br, 1);
            let apply = owner.try_apply().unwrap();
            send(&ba, 1);
            expect(&br, 3);
            rejected(apply.try_version(), Cause::Busy, Effect::Unchanged);
            send(&ba, 3);
            expect(&br, 5);
            let apply = owner.try_apply().unwrap();
            send(&ba, 5);
            expect(&br, 6);
            let version = apply.try_version().unwrap();
            send(&ba, 6);
            expect(&br, 7);
            drop(version);
            send(&ba, 7);
            expect(&br, 255);
            drop(owner);
            send(&ba, 255);
        });
        let (b, bg) = model.issue(schedule.b);
        assert_eq!(receive(&bix), (b, bg));
        model.check(core);
        with_content_interval(core, &mut flusher, &mut model, schedule, |step, model| {
            let (tx, rx) = if matches!(step, 0 | 2 | 4) {
                (&at, &ax)
            } else {
                (&bt, &bx)
            };
            send(tx, step as u8);
            expect(rx, step as u8);
            match step {
                0 => model.modes[a] = APPLY,
                1 | 5 => model.modes[b] = APPLY,
                3 | 7 => model.modes[b] = IDLE,
                4 => model.modes[a] = IDLE,
                _ => (),
            }
            if matches!(step, 2 | 3 | 6) {
                assert!(support::version_busy(core));
            }
            if matches!(step, 4 | 7) {
                assert!(!support::version_busy(core));
            }
            model.check(core);
        });
        send(&at, 255);
        expect(&ax, 255);
        model.release(a);
        send(&bt, 255);
        expect(&bx, 255);
        model.release(b);
        model.check(core);
    });
    drop(flusher);
    model.release(33);
    model.check(&core);
}

#[test]
fn concurrency_ga02_real_version_exclusion() {
    let base = Schedule {
        enter: 3,
        leave: 4,
        ..first_schedule("GA02")
    };
    case("foreground_first");
    run_ga02(base);
    case("maintenance_first");
    run_ga02(Schedule {
        a: OwnerRole::Maintenance,
        b: OwnerRole::Foreground,
        ..base
    });
    complete("GA02");
}

fn run_ga03(schedule: Schedule) {
    println!(
        "SIMULATED admission recheck physical_idle={}",
        schedule.physical_idle
    );
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut flusher = issue(&core, &mut model, OwnerRole::DedicatedFlusher);
    std::thread::scope(|scope| {
        let core = &core;
        let pre = schedule
            .post_check
            .then(|| Pause::install(core, PausePoint::AfterVersionPrecheck));
        let (at, ar) = channel();
        let (aa, ax) = channel();
        let (ai, aix) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(schedule.a).unwrap();
            send(&ai, support::owner_identity(&owner));
            expect(&ar, 0);
            let apply = owner.try_apply().unwrap();
            send(&aa, 0);
            if !schedule.post_check {
                expect(&ar, 2);
            }
            rejected(
                apply.try_version(),
                Cause::InstallationPending,
                Effect::Unchanged,
            );
            send(&aa, 2);
            expect(&ar, 3);
            let content = owner.try_content().unwrap();
            send(&aa, 3);
            expect(&ar, 4);
            drop(content);
            send(&aa, 4);
            expect(&ar, 7);
            drop(owner.try_apply().unwrap());
            send(&aa, 7);
            expect(&ar, 255);
            drop(owner);
            send(&aa, 255);
        });
        let (a, ag) = model.issue(schedule.a);
        assert_eq!(receive(&aix), (a, ag));
        model.check(core);
        let (bt, br) = channel();
        let (ba, bx) = channel();
        let (bi, bix) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            send(&bi, support::owner_identity(&owner));
            expect(&br, 1);
            let mut stage =
                support::simulated_commit(owner.try_apply().unwrap().try_version().unwrap())
                    .unwrap();
            stage.mark_attempted();
            let pending = stage.complete(101).unwrap();
            send(&ba, 1);
            expect(&br, 5);
            let install = installed(pending);
            send(&ba, 5);
            expect(&br, 6);
            install.complete().unwrap();
            send(&ba, 6);
            expect(&br, 255);
            drop(owner);
            send(&ba, 255);
        });
        let (b, bg) = model.issue(OwnerRole::Foreground);
        assert_eq!(receive(&bix), (b, bg));
        model.check(core);
        let mut witness = Vec::with_capacity(6);
        with_content_interval(core, &mut flusher, &mut model, schedule, |step, model| {
            match step {
                0 => {
                    send(&at, 0);
                    expect(&ax, 0);
                    if let Some(pre) = &pre {
                        pre.reached();
                        witness.push(Witness::PrecheckOpen);
                        drop(support::hold_state(core));
                        witness.push(Witness::StateAvailable);
                    }
                    model.modes[a] = APPLY;
                }
                1 => {
                    send(&bt, 1);
                    expect(&bx, 1);
                    model.modes[b] = PENDING;
                    model.installation_generation = 1;
                    model.commit(2, b, Some(101));
                }
                2 => {
                    let gate = Pause::install(core, PausePoint::AfterVersionLock);
                    if let Some(pre) = &pre {
                        pre.resume();
                        gate.reached();
                        assert!(support::version_busy(core));
                        witness.push(Witness::VersionHeld);
                        model.check(core);
                        gate.resume();
                    } else {
                        send(&at, 2);
                    }
                    expect(&ax, 2);
                    if schedule.post_check {
                        witness.push(Witness::PendingRejected);
                    }
                    if !schedule.post_check {
                        assert!(matches!(
                            gate.arrived.try_recv(),
                            Err(mpsc::TryRecvError::Empty)
                        ));
                    }
                    model.modes[a] = IDLE;
                    assert!(!support::version_busy(core));
                    if schedule.post_check {
                        witness.push(Witness::VersionReleased);
                        model.check(core);
                        witness.push(Witness::AdmissionReleased);
                    }
                }
                3 => {
                    send(&at, 3);
                    expect(&ax, 3);
                    model.modes[a] = CONTENT;
                }
                4 => {
                    send(&at, 4);
                    expect(&ax, 4);
                    model.modes[a] = IDLE;
                }
                5 => {
                    send(&bt, 5);
                    expect(&bx, 5);
                    model.modes[b] = INSTALLING;
                    model.commit(3, b, Some(101));
                }
                6 => {
                    send(&bt, 6);
                    expect(&bx, 6);
                    model.modes[b] = IDLE;
                    model.open();
                }
                7 => {
                    send(&at, 7);
                    expect(&ax, 7);
                }
                _ => unreachable!(),
            }
            model.check(core);
        });
        if schedule.post_check {
            check_witness("postcheck", &witness).unwrap();
        }
        send(&at, 255);
        expect(&ax, 255);
        model.release(a);
        send(&bt, 255);
        expect(&bx, 255);
        model.release(b);
        model.check(core);
    });
    drop(flusher);
    model.release(33);
    model.check(&core);
}

#[test]
fn concurrency_ga03_simulated_admission_recheck() {
    let base = first_schedule("GA03");
    for (name, post_check, physical_idle) in [
        ("pre_idle0", false, false),
        ("pre_idle1", false, true),
        ("post_idle0", true, false),
        ("post_idle1", true, true),
    ] {
        case(name);
        run_ga03(Schedule {
            post_check,
            physical_idle,
            ..base
        });
    }
    complete("GA03");
}

fn run_ga04(schedule: Schedule) {
    println!(
        "SIMULATED pending exclusion physical_idle={}",
        schedule.physical_idle
    );
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut flusher = issue(&core, &mut model, OwnerRole::DedicatedFlusher);
    std::thread::scope(|scope| {
        let core = &core;
        let (bt, br) = channel();
        let (ba, bx) = channel();
        let (bi, bix) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            send(&bi, support::owner_identity(&owner));
            expect(&br, 0);
            let mut stage =
                support::simulated_commit(owner.try_apply().unwrap().try_version().unwrap())
                    .unwrap();
            stage.mark_attempted();
            let pending = stage.complete(101).unwrap();
            send(&ba, 0);
            expect(&br, 5);
            let install = installed(pending);
            send(&ba, 5);
            expect(&br, 6);
            install.complete().unwrap();
            send(&ba, 6);
            expect(&br, 255);
            drop(owner);
            send(&ba, 255);
        });
        let (b, bg) = model.issue(OwnerRole::Foreground);
        assert_eq!(receive(&bix), (b, bg));
        model.check(core);
        let (at, ar) = channel();
        let (aa, ax) = channel();
        let (ai, aix) = channel();
        scope.spawn(move || {
            let mut foreground: Vec<_> = (1..schedule.foreground)
                .map(|_| core.try_owner(OwnerRole::Foreground).unwrap())
                .collect();
            let mut maintenance = core.try_owner(OwnerRole::Maintenance).unwrap();
            send(
                &ai,
                (
                    support::owner_identity(foreground.last().unwrap()),
                    support::owner_identity(&maintenance),
                ),
            );
            expect(&ar, 1);
            rejected(
                foreground[0].try_apply(),
                Cause::InstallationPending,
                Effect::Unchanged,
            );
            let content = foreground[0].try_content().unwrap();
            send(&aa, 1);
            expect(&ar, 11);
            drop(content);
            send(&aa, 11);
            expect(&ar, 2);
            rejected(
                maintenance.try_apply(),
                Cause::InstallationPending,
                Effect::Unchanged,
            );
            let content = maintenance.try_content().unwrap();
            send(&aa, 2);
            expect(&ar, 12);
            drop(content);
            send(&aa, 12);
            expect(&ar, 3);
            drop(foreground.pop());
            send(&aa, 3);
            expect(&ar, 4);
            let mut replacement = core.try_owner(OwnerRole::Foreground).unwrap();
            drop(replacement.try_content().unwrap());
            rejected(
                replacement.try_apply(),
                Cause::InstallationPending,
                Effect::Unchanged,
            );
            foreground.push(replacement);
            send(&aa, 4);
            expect(&ar, 7);
            drop(foreground[0].try_apply().unwrap());
            drop(maintenance.try_apply().unwrap());
            send(&aa, 7);
            expect(&ar, 255);
            drop(foreground);
            drop(maintenance);
            send(&aa, 255);
        });
        let mut last = (0, 0);
        for _ in 1..schedule.foreground {
            last = model.issue(OwnerRole::Foreground);
        }
        let maintenance = model.issue(OwnerRole::Maintenance);
        assert_eq!(receive(&aix), (last, maintenance));
        model.check(core);
        with_content_interval(core, &mut flusher, &mut model, schedule, |step, model| {
            let (tx, rx) = if matches!(step, 0 | 5 | 6) {
                (&bt, &bx)
            } else {
                (&at, &ax)
            };
            send(tx, step as u8);
            expect(rx, step as u8);
            if matches!(step, 1 | 2) {
                let index = if step == 1 { 1 } else { 32 };
                model.modes[index] = CONTENT;
                model.check(core);
                send(&at, (step + 10) as u8);
                expect(&ax, (step + 10) as u8);
                model.modes[index] = IDLE;
            }
            match step {
                0 => {
                    model.installation_generation = 1;
                    model.modes[b] = PENDING;
                    model.commit(2, b, Some(101));
                }
                3 => model.release(last.0),
                4 => {
                    assert_eq!(model.issue(OwnerRole::Foreground), (last.0, last.1 + 1));
                }
                5 => {
                    model.modes[b] = INSTALLING;
                    model.commit(3, b, Some(101));
                }
                6 => {
                    model.modes[b] = IDLE;
                    model.open();
                }
                _ => (),
            }
            model.check(core);
        });
        send(&at, 255);
        expect(&ax, 255);
        for index in 1..schedule.foreground {
            model.release(index);
        }
        model.release(32);
        send(&bt, 255);
        expect(&bx, 255);
        model.release(b);
        model.check(core);
    });
    drop(flusher);
    model.release(33);
    model.check(&core);
}

#[test]
fn concurrency_ga04_simulated_pending_owner() {
    let base = Schedule {
        enter: 1,
        leave: 5,
        ..first_schedule("GA04")
    };
    case("saturated_pending");
    run_ga04(Schedule {
        foreground: 32,
        ..base
    });
    case("small_pending");
    run_ga04(Schedule {
        foreground: 2,
        physical_idle: true,
        ..base
    });
    for wrong_core in [true, false] {
        case(if wrong_core {
            "wrong_core_origin_fenced"
        } else {
            "pending_retry_keeps_token"
        });
        let fence = SessionFence::new();
        let core = PageConcurrency::new(&fence).unwrap();
        let mut model = Model::new();
        let other_fence = SessionFence::new();
        let other = PageConcurrency::new(&other_fence).unwrap();
        let untouched = support::observe(&other);
        let mut owner = issue(&core, &mut model, OwnerRole::Foreground);
        let mut stage =
            support::simulated_commit(owner.try_apply().unwrap().try_version().unwrap()).unwrap();
        stage.mark_attempted();
        let pending = stage.complete(101).unwrap();
        let key = pending.key();
        model.modes[0] = PENDING;
        model.installation_generation = 1;
        model.commit(2, 0, Some(101));
        model.check(&core);
        let held = (!wrong_core).then(|| core.version.lock().unwrap());
        let (error, pending) = {
            let result = if wrong_core {
                pending.try_install_on(&other)
            } else {
                pending.try_install()
            };
            match result {
                Err(pair) => pair,
                Ok(_) => panic!("expected retained SIMULATED pending"),
            }
        };
        assert_eq!(pending.key(), key);
        assert_eq!(
            error,
            ConcurrencyError {
                cause: if wrong_core {
                    Cause::Invariant
                } else {
                    Cause::Busy
                },
                effect: if wrong_core {
                    Effect::CallerMustResolve
                } else {
                    Effect::Unchanged
                }
            }
        );
        assert_eq!(support::observe(&other), untouched);
        drop(held);
        if wrong_core {
            model.fenced = true;
            model.check(&core);
            drop(pending);
            model.modes[0] = IDLE;
            model.commit(4, 0, Some(101));
        } else {
            model.check(&core);
            installed(pending).complete().unwrap();
            model.modes[0] = IDLE;
            model.open();
        }
        model.check(&core);
        drop(owner);
        model.release(0);
        model.check(&core);
    }
    complete("GA04");
}

enum Held<'o, 'c, 'f> {
    Apply(ApplyAdmission<'o, 'c, 'f>),
    Content(ContentAdmission<'o, 'c, 'f>),
    Version(VersionEditGuard<'o, 'c, 'f>),
    Stage(support::SimulatedCommitStage<'o, 'c, 'f>),
    Pending(support::SimulatedInstallation<'o, 'c, 'f>),
    Installing(support::SimulatedInstallGuard<'o, 'c, 'f>),
}

impl Held<'_, '_, '_> {
    fn discard(self) {
        match self {
            Self::Apply(value) => drop(value),
            Self::Content(value) => drop(value),
            Self::Version(value) => drop(value),
            Self::Stage(value) => drop(value),
            Self::Pending(value) => drop(value),
            Self::Installing(value) => drop(value),
        }
    }
}

fn fault_steps(
    owner: &mut Owner<'_, '_>,
    path: &str,
    commands: &Receiver<u8>,
    ack: &SyncSender<u8>,
) {
    let core = owner.core;
    let held = match path {
        "apply_unwind" | "state_poison" => Held::Apply(owner.try_apply().unwrap()),
        "content_unwind" => Held::Content(owner.try_content().unwrap()),
        "version_unwind" => Held::Version(owner.try_apply().unwrap().try_version().unwrap()),
        _ => Held::Stage(
            support::simulated_commit(owner.try_apply().unwrap().try_version().unwrap()).unwrap(),
        ),
    };
    send(ack, 1);
    expect(commands, 2);
    let held = match held {
        Held::Stage(mut stage) => {
            if path != "sim_not_attempted_cancel" {
                stage.mark_attempted();
            }
            if path == "sim_pending_drop" {
                Held::Pending(stage.complete(101).unwrap())
            } else if path == "sim_install_failure" {
                Held::Installing(installed(stage.complete(101).unwrap()))
            } else {
                Held::Stage(stage)
            }
        }
        other => other,
    };
    send(ack, 2);
    expect(commands, 3);
    match path {
        "sim_not_attempted_cancel" => {
            if let Held::Stage(stage) = held {
                stage.cancel_unattempted().unwrap();
            } else {
                panic!("wrong held mode");
            }
        }
        "sim_unchanged_reject" => {
            if let Held::Stage(stage) = held {
                stage.reject_unchanged();
            } else {
                panic!("wrong held mode");
            }
        }
        "state_poison" => {
            support::poison_state(core);
            held.discard();
        }
        "sim_pending_drop" | "sim_install_failure" => held.discard(),
        _ => assert!(
            catch_unwind(AssertUnwindSafe(move || {
                let _held = held;
                panic!("controlled guard unwind");
            }))
            .is_err()
        ),
    }
}

fn run_ga05(schedule: Schedule) {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut flusher = issue(&core, &mut model, OwnerRole::DedicatedFlusher);
    let simulated = schedule.path.starts_with("sim_");
    let healthy = matches!(
        schedule.path,
        "sim_not_attempted_cancel" | "sim_unchanged_reject"
    );
    println!(
        "GUARD_CLEANUP path={} physical_stage={}",
        schedule.path,
        if simulated { "SIMULATED" } else { "NONE" }
    );
    std::thread::scope(|scope| {
        let core = &core;
        let (tx, rx) = channel();
        let (ack, done) = channel();
        let (id, ids) = channel();
        scope.spawn(move || {
            expect(&rx, 0);
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            send(&id, support::owner_identity(&owner));
            send(&ack, 0);
            expect(&rx, 1);
            fault_steps(&mut owner, schedule.path, &rx, &ack);
            send(&ack, 3);
            expect(&rx, 4);
            send(&ack, 4);
            expect(&rx, 5);
            if healthy {
                drop(owner.try_apply().unwrap());
            } else {
                rejected(owner.try_apply(), Cause::Fenced, Effect::CallerMustResolve);
            }
            send(&ack, 5);
            expect(&rx, 6);
            drop(owner);
            send(&ack, 6);
            expect(&rx, 7);
            send(&ack, 7);
        });
        let mut witness = Vec::with_capacity(5);
        let version_path = matches!(schedule.path, "version_unwind" | "sim_attempted_unwind");
        with_content_interval(core, &mut flusher, &mut model, schedule, |step, model| {
            let pause = (step == 3 && schedule.path.ends_with("unwind"))
                .then(|| Pause::install(core, PausePoint::FencedBeforeRelease));
            send(&tx, step as u8);
            if let Some(pause) = &pause {
                pause.reached();
                assert!(core.fence.is_fenced());
                if version_path {
                    witness.push(Witness::FenceSet);
                }
                model.fenced = true;
                model.check(core);
                if matches!(schedule.path, "version_unwind" | "sim_attempted_unwind") {
                    assert!(support::version_busy(core));
                    witness.push(Witness::VersionHeld);
                }
                pause.resume();
            }
            expect(&done, step as u8);
            match step {
                0 => {
                    assert_eq!(receive(&ids), model.issue(OwnerRole::Foreground));
                }
                1 => {
                    model.modes[0] = if simulated {
                        STAGE
                    } else if schedule.path == "content_unwind" {
                        CONTENT
                    } else {
                        APPLY
                    };
                    if simulated {
                        model.installation_generation = 1;
                        model.commit(1, 0, None);
                    }
                }
                2 => match schedule.path {
                    "sim_pending_drop" => {
                        model.modes[0] = PENDING;
                        model.commit(2, 0, Some(101));
                    }
                    "sim_install_failure" => {
                        model.modes[0] = INSTALLING;
                        model.commit(3, 0, Some(101));
                    }
                    _ => (),
                },
                3 => {
                    model.modes[0] = IDLE;
                    model.fenced = !healthy;
                    if simulated {
                        if healthy {
                            model.open();
                        } else {
                            model.commit(
                                4,
                                0,
                                if schedule.path == "sim_attempted_unwind" {
                                    None
                                } else {
                                    Some(101)
                                },
                            );
                        }
                    }
                }
                6 => model.release(0),
                _ => (),
            }
            model.check(core);
            if version_path && step == 1 {
                assert!(support::version_busy(core));
                witness.push(Witness::VersionHeld);
            }
            if version_path && step == 3 {
                assert!(!support::version_busy(core));
                witness.push(Witness::VersionReleased);
                witness.push(Witness::AdmissionReleased);
            }
        });
        if version_path {
            check_witness("unwind", &witness).unwrap();
        }
    });
    drop(flusher);
    model.release(33);
    model.check(&core);
}

fn after_version_lock_unwind() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let locked = Pause::install(&core, PausePoint::AfterVersionLock);
    let fenced = Pause::install(&core, PausePoint::FencedBeforeRelease);
    std::thread::scope(|scope| {
        let core = &core;
        let (done, rx) = channel();
        let (release, wait) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _version = owner.try_apply().unwrap().try_version().unwrap();
                }))
                .is_err()
            );
            send(&done, 1);
            expect(&wait, 1);
            drop(owner);
            send(&done, 2);
        });
        locked.reached();
        model.issue(OwnerRole::Foreground);
        model.modes[0] = APPLY;
        model.check(core);
        assert!(support::version_busy(core));
        locked.panic();
        fenced.reached();
        model.fenced = true;
        model.check(core);
        assert!(support::version_busy(core));
        fenced.resume();
        expect(&rx, 1);
        model.modes[0] = IDLE;
        model.check(core);
        assert!(!support::version_busy(core));
        // The actual mutex is poisoned by unwinding, and its poison is retained.
        assert!(core.version.is_poisoned());
        send(&release, 1);
        expect(&rx, 2);
        model.release(0);
        model.check(core);
    });
}

fn owner_unwind() {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let pause = Pause::install(&core, PausePoint::FencedBeforeRelease);
    std::thread::scope(|scope| {
        let core = &core;
        let (ready, ready_rx) = channel();
        let (go, go_rx) = channel();
        scope.spawn(move || {
            let owner = core.try_owner(OwnerRole::Foreground).unwrap();
            send(&ready, 1);
            expect(&go_rx, 1);
            assert!(
                catch_unwind(AssertUnwindSafe(move || {
                    let _owner = owner;
                    panic!("controlled owner unwind");
                }))
                .is_err()
            );
            send(&ready, 2);
        });
        let mut witness = Vec::with_capacity(4);
        expect(&ready_rx, 1);
        model.issue(OwnerRole::Foreground);
        model.check(core);
        witness.push(Witness::OwnerHeld);
        send(&go, 1);
        pause.reached();
        model.fenced = true;
        model.check(core);
        witness.push(Witness::FenceSet);
        witness.push(Witness::OwnerHeld);
        pause.resume();
        expect(&ready_rx, 2);
        model.release(0);
        model.check(core);
        witness.push(Witness::OwnerReleased);
        check_witness("owner", &witness).unwrap();
    });
}

#[test]
fn concurrency_ga05_cleanup_fences_before_release() {
    case("owner_unwind");
    owner_unwind();
    let base = first_schedule("GA05");
    for path in [
        "apply_unwind",
        "content_unwind",
        "version_unwind",
        "sim_not_attempted_cancel",
        "sim_unchanged_reject",
        "sim_attempted_unwind",
        "sim_pending_drop",
        "sim_install_failure",
    ] {
        case(path);
        run_ga05(Schedule { path, ..base });
    }
    case("after_version_lock_unwind");
    after_version_lock_unwind();
    complete("GA05");
}

fn corrupted_counts_survive_drop(count: u16) {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let apply = owner.try_apply().unwrap();
    support::set_counts(&core, count, 0);
    let before = support::observe(&core);
    drop(apply);
    let after = support::observe(&core);
    assert!(after.fenced);
    assert_eq!(after.slots, before.slots);
    assert_eq!(after.active_apply, count);
    assert_eq!(after.active_content, 0);
    rejected(
        core.try_owner(OwnerRole::Maintenance),
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    drop(owner);
    assert_eq!(support::observe(&core), after);
}

/// Expected Busy/Pending is fixed before the cleanup pause. Corruption injected
/// after that acknowledgement must upgrade the returned effect, not be discarded.
fn cleanup_failure(pending_mode: bool, poison: bool, first_fatal: bool) {
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    std::thread::scope(|scope| {
        let core = &core;
        let (ready, ready_rx) = channel();
        let (go, go_rx) = channel();
        let (done, done_rx) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let apply = owner.try_apply().unwrap();
            send(&ready, ());
            expect(&go_rx, 1);
            let error = match apply.try_version() {
                Err(error) => error,
                Ok(_) => panic!("expected failure"),
            };
            send(&done, error);
            expect(&go_rx, 2);
            drop(owner);
            send(&ready, ());
        });
        receive(&ready_rx);
        let mut commit_owner = pending_mode.then(|| core.try_owner(OwnerRole::Foreground).unwrap());
        let pending = commit_owner.as_mut().map(|owner| {
            let mut stage =
                support::simulated_commit(owner.try_apply().unwrap().try_version().unwrap())
                    .unwrap();
            stage.mark_attempted();
            stage.complete(101).unwrap()
        });
        let gate = (!pending_mode && !first_fatal).then(|| core.version.lock().unwrap());
        if first_fatal {
            support::poison_version(core);
        }
        let cleanup = Pause::install(core, PausePoint::BeforeCleanupLock);
        send(&go, 1);
        cleanup.reached();
        if poison {
            support::poison_state(core);
        } else {
            support::set_counts(core, 2, 0);
        }
        let before = support::observe(core);
        cleanup.resume();
        let error = receive(&done_rx);
        assert_eq!(
            error,
            ConcurrencyError {
                cause: if first_fatal || poison {
                    Cause::Poisoned
                } else {
                    Cause::Invariant
                },
                effect: Effect::CallerMustResolve
            }
        );
        let after = support::observe(core);
        assert!(after.fenced);
        if poison {
            assert!(core.state.is_poisoned());
            assert_eq!(after.slots[0].mode, Mode::Idle);
            assert_eq!(after.active_apply, 0);
            assert_eq!(after.installation, before.installation);
        } else {
            assert_eq!(after.slots, before.slots);
            assert_eq!(after.active_apply, before.active_apply);
            assert_eq!(after.installation, before.installation);
        }
        send(&go, 2);
        receive(&ready_rx);
        drop(gate);
        drop(pending);
        drop(commit_owner);
    });
}

#[test]
fn concurrency_ga06_poison_and_blocked_drop() {
    case("state_poison");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    support::poison_state(&core);
    rejected(
        core.try_owner(OwnerRole::Foreground),
        Cause::Poisoned,
        Effect::CallerMustResolve,
    );
    rejected(
        core.try_owner(OwnerRole::Foreground),
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    assert!(core.state.is_poisoned());
    case("version_poison");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    support::poison_version(&core);
    rejected(
        owner.try_apply().unwrap().try_version(),
        Cause::Poisoned,
        Effect::CallerMustResolve,
    );
    assert!(core.version.is_poisoned());
    let observed = support::observe(&core);
    assert_eq!(observed.active_apply, 0);
    assert_eq!(observed.slots[0].mode, Mode::Idle);
    case("blocked_drop");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    std::thread::scope(|scope| {
        let core = &core;
        let (ready, ready_rx) = channel();
        let (go, go_rx) = channel();
        let (done, done_rx) = channel();
        scope.spawn(move || {
            let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
            let apply = owner.try_apply().unwrap();
            send(&ready, ());
            expect(&go_rx, 1);
            drop(apply);
            send(&done, 1);
            expect(&go_rx, 2);
            drop(owner);
            send(&done, 2);
        });
        receive(&ready_rx);
        let cleanup = Pause::install(core, PausePoint::BeforeCleanupLock);
        let state = support::hold_state(core);
        send(&go, 1);
        cleanup.reached();
        assert_eq!(state.active_apply, 1);
        assert_eq!(state.slots[0].mode, Mode::Apply);
        cleanup.resume();
        assert!(
            matches!(
                done_rx.recv_timeout(STEP),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "Drop completed while bookkeeping remained locked"
        );
        drop(state);
        expect(&done_rx, 1);
        let observed = support::observe(core);
        assert_eq!(observed.active_apply, 0);
        assert_eq!(observed.slots[0].mode, Mode::Idle);
        assert!(!observed.fenced);
        send(&go, 2);
        expect(&done_rx, 2);
        assert_eq!(support::observe(core).slots[0].mode, Mode::Free);
    });
    case("overcount_drop");
    corrupted_counts_survive_drop(2);
    case("undercount_drop");
    corrupted_counts_survive_drop(0);
    case("fenced_all_entries");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut a = core.try_owner(OwnerRole::Foreground).unwrap();
    let mut b = core.try_owner(OwnerRole::Maintenance).unwrap();
    let apply = a.try_apply().unwrap();
    fence.fence();
    rejected(
        PageConcurrency::new(&fence),
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    rejected(
        core.try_owner(OwnerRole::Foreground),
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    rejected(b.try_apply(), Cause::Fenced, Effect::CallerMustResolve);
    rejected(b.try_content(), Cause::Fenced, Effect::CallerMustResolve);
    rejected(
        apply.try_version(),
        Cause::Fenced,
        Effect::CallerMustResolve,
    );
    for (name, pending, poison) in [
        ("busy_cleanup_poison", false, true),
        ("busy_cleanup_overcount", false, false),
        ("pending_cleanup_poison", true, true),
        ("pending_cleanup_overcount", true, false),
    ] {
        case(name);
        cleanup_failure(pending, poison, false);
    }
    case("first_fatal_cause_preserved");
    cleanup_failure(false, false, true);
    case("fence_after_completed_cleanup");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut owner = core.try_owner(OwnerRole::Foreground).unwrap();
    let gate = core.version.lock().unwrap();
    rejected(
        owner.try_apply().unwrap().try_version(),
        Cause::Busy,
        Effect::Unchanged,
    );
    assert_eq!(support::observe(&core).active_apply, 0);
    fence.fence();
    drop(gate);
    rejected(owner.try_apply(), Cause::Fenced, Effect::CallerMustResolve);
    complete("GA06");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Witness {
    PrecheckOpen,
    StateAvailable,
    StateHeld,
    VersionHeld,
    PendingRejected,
    VersionReleased,
    FenceSet,
    AdmissionReleased,
    OwnerHeld,
    OwnerReleased,
}

fn check_witness(kind: &str, events: &[Witness]) -> std::result::Result<(), &'static str> {
    use Witness::*;
    let expected: &[Witness] = match kind {
        "postcheck" => &[
            PrecheckOpen,
            StateAvailable,
            VersionHeld,
            PendingRejected,
            VersionReleased,
            AdmissionReleased,
        ],
        "unwind" => &[
            VersionHeld,
            FenceSet,
            VersionHeld,
            VersionReleased,
            AdmissionReleased,
        ],
        "owner" => &[OwnerHeld, FenceSet, OwnerHeld, OwnerReleased],
        "lock_order" => &[StateAvailable, VersionHeld],
        _ => return Err("unknown witness contract"),
    };
    if events != expected {
        return Err("missing or reordered observed cut/lock/fence witness");
    }
    Ok(())
}

fn base_actions(family: &str) -> [&'static str; 8] {
    match family {
        "GA02" => [
            "A_APPLY",
            "B_APPLY",
            "A_VERSION_OK",
            "B_VERSION_BUSY",
            "A_VERSION_DROP",
            "B_REAPPLY",
            "B_VERSION_OK",
            "B_VERSION_DROP",
        ],
        "GA03" => [
            "A_APPLY_PAUSE",
            "B_SIM_PENDING101",
            "A_VERSION_INSTALLATION_PENDING",
            "A_CONTENT_ENTER",
            "A_CONTENT_DROP",
            "B_SIM_INSTALL_BEGIN",
            "B_SIM_INSTALL_COMPLETE",
            "A_APPLY_ROUNDTRIP",
        ],
        "GA04" => [
            "B_SIM_PENDING101",
            "A_FOREGROUND_APPLY_PENDING",
            "A_MAINTENANCE_APPLY_PENDING",
            "A_DROP_FOREGROUND",
            "A_REISSUE_CONTENT_AND_REJECT_APPLY",
            "B_SIM_INSTALL_BEGIN",
            "B_SIM_INSTALL_COMPLETE",
            "A_APPLY_ROUNDTRIP",
        ],
        "GA05" => [
            "A_OWNER",
            "A_GUARD",
            "A_PREFAULT_MARKER",
            "A_FAULT_OR_CANCEL",
            "A_SETTLED",
            "A_REENTRY_EXPECTATION",
            "A_OWNER_DROP",
            "FINAL",
        ],
        _ => panic!("unknown schedule family"),
    }
}

fn expected_actions(schedule: Schedule) -> [&'static str; 10] {
    let base = base_actions(schedule.family);
    let mut expected = [""; 10];
    let mut cursor = 0;
    for (index, action) in base.into_iter().enumerate() {
        if index == schedule.enter {
            expected[cursor] = "C_CONTENT_ENTER";
            cursor += 1;
        }
        if index == schedule.leave {
            expected[cursor] = "C_CONTENT_DROP";
            cursor += 1;
        }
        expected[cursor] = action;
        cursor += 1;
    }
    assert_eq!(cursor, 10);
    expected
}

fn check_trace(
    schedule: Schedule,
    actual: &[&str],
    physical_stage: &str,
) -> std::result::Result<(), &'static str> {
    if actual != expected_actions(schedule) {
        return Err("missing/duplicate/out-of-order actor action");
    }
    if matches!(schedule.family, "GA03" | "GA04") && physical_stage != "SIMULATED" {
        return Err("unlabelled simulated transition");
    }
    if schedule.family == "GA05"
        && schedule.path.starts_with("sim_")
        && physical_stage != "SIMULATED"
    {
        return Err("unlabelled simulated cleanup");
    }
    Ok(())
}

fn validate_inventory(
    rows: impl Iterator<Item = Schedule> + Clone,
) -> std::result::Result<(), &'static str> {
    if rows.clone().count() != 512 {
        return Err("schedule census");
    }
    for (index, row) in rows.clone().enumerate() {
        if rows.clone().take(index).any(|other| other.id == row.id) {
            return Err("duplicate schedule id");
        }
        if !["GA02", "GA03", "GA04", "GA05"].contains(&row.family) {
            return Err("unknown family");
        }
        if !row.id.starts_with(row.family) || row.enter >= row.leave || row.leave > 7 {
            return Err("schedule bounds");
        }
        check_trace(row, &row.actions, row.physical_stage)?;
    }
    for family in ["GA02", "GA03", "GA04", "GA05"] {
        if rows.clone().filter(|row| row.family == family).count() != 128 {
            return Err("family census");
        }
    }
    Ok(())
}

fn selected_count(
    rows: impl Iterator<Item = Schedule>,
    family: &str,
    id: Option<&str>,
) -> std::result::Result<usize, &'static str> {
    let count = rows
        .filter(|row| row.family == family && id.is_none_or(|id| row.id == id))
        .count();
    if count != if id.is_some() { 1 } else { 128 } {
        return Err("missing/duplicate selection");
    }
    Ok(count)
}

fn complete(group: &str) {
    let count = NORMAL
        .lines()
        .filter(|line| line.starts_with(group))
        .count();
    println!("GUARD_COMPLETE {group} {count}");
}

#[path = "concurrency_resource_tests.rs"]
mod resource;

#[test]
fn concurrency_ga07_resource_and_oracle_negatives() {
    case("type_ceilings");
    resource::type_ceilings();
    case("all_live_control");
    resource::all_live_control();
    case("independent_negative_oracles");
    let fence = SessionFence::new();
    let core = PageConcurrency::new(&fence).unwrap();
    let mut model = Model::new();
    let mut owner = issue(&core, &mut model, OwnerRole::Foreground);
    let apply = owner.try_apply().unwrap();
    model.modes[0] = APPLY;
    let correct = support::observe(&core);
    model.matches(&correct).unwrap();
    let mut bad = correct;
    bad.active_apply = 0;
    assert_eq!(model.matches(&bad), Err("counter census"));
    bad = correct;
    bad.active_apply = 2;
    assert_eq!(model.matches(&bad), Err("counter census"));
    bad = correct;
    bad.slots[0].mode = Mode::Idle;
    assert_eq!(model.matches(&bad), Err("slot identity/mode"));
    bad = correct;
    bad.slots[0].generation += 1;
    assert_eq!(model.matches(&bad), Err("slot identity/mode"));
    bad = correct;
    bad.fenced = true;
    assert_eq!(model.matches(&bad), Err("fence state"));
    bad = correct;
    bad.installation_generation = 1;
    assert_eq!(model.matches(&bad), Err("installation identity"));
    bad = correct;
    bad.installation = Installation::Abandoned(
        support::InstallationKey {
            slot: 0,
            owner_generation: 1,
            installation_generation: 1,
        },
        Some(101),
    );
    assert_eq!(model.matches(&bad), Err("installation identity"));
    drop(apply);
    assert!(size_of::<Schedule>() <= 384);
    let rows = EXPANDED
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(parse_schedule);
    validate_inventory(rows.clone()).unwrap();
    let schedule = first_schedule("GA03");
    let valid = expected_actions(schedule);
    check_trace(schedule, &valid, "SIMULATED").unwrap();
    assert!(check_trace(schedule, &valid[..9], "SIMULATED").is_err());
    let mut invalid = valid;
    invalid[3] = invalid[2];
    assert!(check_trace(schedule, &invalid, "SIMULATED").is_err());
    invalid = valid;
    invalid[2] = "B_APPLY_PAUSE";
    assert!(check_trace(schedule, &invalid, "SIMULATED").is_err());
    invalid = valid;
    invalid.swap(3, 4);
    assert!(check_trace(schedule, &invalid, "SIMULATED").is_err());
    assert!(check_trace(schedule, &valid, "NONE").is_err());
    assert!(selected_count(rows.clone(), "GA03", Some("unknown")).is_err());
    assert!(selected_count(std::iter::empty(), "GA03", None).is_err());
    let first = rows.clone().next().unwrap();
    let duplicate = rows
        .enumerate()
        .map(|(index, row)| if index == 1 { first } else { row });
    assert_eq!(
        validate_inventory(duplicate.clone()),
        Err("duplicate schedule id")
    );
    assert!(selected_count(duplicate, first.family, Some(first.id)).is_err());
    use Witness::*;
    let post = [
        PrecheckOpen,
        StateAvailable,
        VersionHeld,
        PendingRejected,
        VersionReleased,
        AdmissionReleased,
    ];
    check_witness("postcheck", &post).unwrap();
    assert!(
        check_witness("postcheck", &post[1..]).is_err(),
        "precheck omission"
    );
    assert!(
        check_witness(
            "postcheck",
            &[
                PrecheckOpen,
                StateAvailable,
                VersionHeld,
                VersionReleased,
                AdmissionReleased
            ]
        )
        .is_err(),
        "postcheck omission"
    );
    assert!(
        check_witness(
            "unwind",
            &[
                VersionHeld,
                VersionReleased,
                FenceSet,
                VersionHeld,
                AdmissionReleased
            ]
        )
        .is_err(),
        "unlock before fence"
    );
    check_witness("lock_order", &[StateAvailable, VersionHeld]).unwrap();
    assert!(
        check_witness("lock_order", &[StateHeld, VersionHeld]).is_err(),
        "state to version edge"
    );
    println!("GUARD_NEGATIVE_ORACLES 20");
    complete("GA07");
}

fn expanded(family: &str, run: fn(Schedule)) {
    let rows = EXPANDED
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(parse_schedule);
    validate_inventory(rows.clone()).unwrap();
    let selector = std::env::var("XADB_CONCURRENCY_SCHEDULE").ok();
    let count = selected_count(rows.clone(), family, selector.as_deref()).unwrap();
    for schedule in
        rows.filter(|row| row.family == family && selector.as_deref().is_none_or(|id| row.id == id))
    {
        println!("GUARD_SCHEDULE {} {}", schedule.id, schedule.physical_stage);
        run(schedule);
        println!("GUARD_SCHEDULE_COMPLETE {}", schedule.id);
    }
    println!("GUARD_EXPANDED_COMPLETE {family} {count}");
}

#[test]
#[ignore = "DEV3 S5 expanded schedules; select explicitly"]
fn concurrency_s5_ga02_schedules() {
    expanded("GA02", run_ga02);
}
#[test]
#[ignore = "DEV3 S5 expanded SIMULATED schedules; select explicitly"]
fn concurrency_s5_ga03_simulated_schedules() {
    expanded("GA03", run_ga03);
}
#[test]
#[ignore = "DEV3 S5 expanded SIMULATED schedules; select explicitly"]
fn concurrency_s5_ga04_simulated_schedules() {
    expanded("GA04", run_ga04);
}
#[test]
#[ignore = "DEV3 S5 expanded cleanup schedules; select explicitly"]
fn concurrency_s5_ga05_cleanup_schedules() {
    expanded("GA05", run_ga05);
}
