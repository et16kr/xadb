mod soak_support;
mod support;
use foundation::network::{Ready, Registration};
use soak_support::random;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use support::{Action, Script, Stream, TestClock, frame};
use transport::{
    Connection, ConnectionPhase, ErrorKind, MAX_WIRE_BYTES, PROCESS_PAYLOAD_BYTES, ReceivedFrame,
    SendOutcome, SendTicket, Transport,
};

struct Accepted {
    start: usize,
    length: usize,
    ticket: Option<SendTicket>,
}
struct Lease {
    frame: ReceivedFrame,
    expected: Vec<u8>,
}
struct Fixture {
    transport: Transport,
    clock: Arc<TestClock>,
    script: Arc<Mutex<Script>>,
    connection: Connection,
    expected_rx: Vec<Vec<u8>>,
    next_rx: usize,
    leases: Vec<Lease>,
    expected_tx: Vec<u8>,
    accepted: Vec<Accepted>,
    now: u64,
    next_sequence: u64,
    terminal_output: Option<usize>,
}
impl Fixture {
    fn new(seed: u64) -> Self {
        let expected = [vec![], vec![0xaa], vec![0, 1], vec![seed as u8; 63]].to_vec();
        let bad = (seed % 5) as usize;
        let mut input = Vec::new();
        for (i, payload) in expected.iter().enumerate() {
            let mut wire = frame(payload);
            if i == bad {
                let last = wire.len() - 1;
                wire[last] ^= 1;
            }
            input.extend(wire);
        }
        let clock = Arc::new(TestClock::default());
        let transport = Transport::new(clock.clone()); // Real production TX4.
        let script = Arc::new(Mutex::new(Script {
            input: input.into(),
            eof: true,
            ..Script::default()
        }));
        let connection = transport
            .accept(Box::new(Stream(
                script.clone(),
                Registration {
                    slot: 0,
                    generation: 1,
                },
            )))
            .unwrap();
        Self {
            transport,
            clock,
            script,
            connection,
            expected_rx: expected.into_iter().take(bad).collect(),
            next_rx: 0,
            leases: Vec::new(),
            expected_tx: Vec::new(),
            accepted: Vec::new(),
            now: 0,
            next_sequence: 1,
            terminal_output: None,
        }
    }
    fn collect(&mut self) {
        let written = self.script.lock().unwrap().output.len();
        for item in &mut self.accepted {
            if let Some(ticket) = item.ticket.take() {
                let progress = written.saturating_sub(item.start).min(item.length);
                match ticket.try_take() {
                    Err(ticket) => {
                        assert!(progress < item.length, "fully written frame stayed pending");
                        item.ticket = Some(ticket);
                    }
                    Ok(result) => match result {
                        SendOutcome::FlushedLocally => assert_eq!(progress, item.length),
                        SendOutcome::Failed {
                            error,
                            written_bytes,
                        } => {
                            assert!(progress < item.length, "fully written frame failed");
                            assert_eq!(written_bytes, progress);
                            assert_eq!(
                                self.connection.terminal_error().map(|e| e.kind),
                                Some(error.kind)
                            );
                        }
                    },
                }
            }
        }
    }
    fn drain_received(&mut self) {
        self.leases.clear();
        while let Some(frame) = self.connection.take_received() {
            assert!(self.next_rx < self.expected_rx.len());
            assert_eq!(frame.payload(), self.expected_rx[self.next_rx]);
            self.next_rx += 1;
        }
    }
    fn verify(&mut self) {
        {
            let script = self.script.lock().unwrap();
            assert!(script.output.len() <= self.expected_tx.len());
            assert_eq!(script.output, self.expected_tx[..script.output.len()]);
            if let Some(last) = self.terminal_output {
                assert_eq!(script.output.len(), last, "I/O after terminal close");
            }
            if self.connection.phase() == ConnectionPhase::Closed {
                self.terminal_output = Some(script.output.len());
            }
        }
        for lease in &self.leases {
            assert_eq!(lease.frame.payload(), lease.expected);
        }
        let stats = self.connection.stats();
        assert!(stats.queue_slots[0] <= 2 && stats.queue_slots[1] <= 4);
        assert!(stats.queue_bytes[0] <= (2 * MAX_WIRE_BYTES) as u64);
        assert!(stats.queue_bytes[1] <= (4 * MAX_WIRE_BYTES) as u64);
        assert!(self.transport.payload_bytes() <= PROCESS_PAYLOAD_BYTES);
        assert_eq!(self.transport.connection_count(), 1);
        self.collect();
    }
    fn queue_payload(&mut self, payload: &[u8]) {
        match self.connection.sender().try_send(payload) {
            Ok(ticket) => {
                assert_eq!(self.connection.phase(), ConnectionPhase::Open);
                assert_eq!(ticket.sequence(), self.next_sequence);
                self.next_sequence += 1;
                let wire = frame(payload);
                self.accepted.push(Accepted {
                    start: self.expected_tx.len(),
                    length: wire.len(),
                    ticket: Some(ticket),
                });
                self.expected_tx.extend(wire);
            }
            Err(error) => {
                if self.connection.phase() == ConnectionPhase::Open {
                    assert_eq!(error.kind, ErrorKind::Backpressured);
                }
            }
        }
    }
    fn operation(&mut self, rng: &mut u64, operation: usize, kind: usize) {
        if kind >= 16 {
            self.now += 20_000_000;
            self.clock.set(self.now);
        }
        match kind {
            0..=3 => {
                let n = random(rng) as usize % 257;
                let payload = (0..n)
                    .map(|i| (i as u8).wrapping_add(operation as u8))
                    .collect::<Vec<_>>();
                self.queue_payload(&payload);
            }
            choice @ (4..=7 | 15) => {
                self.now += 20_000_000;
                self.clock.set(self.now);
                let before = self.connection.stats();
                let ready = Ready {
                    readable: choice <= 5 || choice == 15,
                    writable: choice >= 6,
                    ..Ready::default()
                };
                {
                    let mut script = self.script.lock().unwrap();
                    script.reads.clear();
                    script.writes.clear();
                    if choice == 15 {
                        script.reads.push_back(Action::Interrupt);
                        script.writes.push_back(Action::Interrupt);
                    }
                    script
                        .reads
                        .extend([Action::Limit(1 + random(rng) as usize % 97), Action::Block]);
                    script
                        .writes
                        .extend([Action::Limit(1 + random(rng) as usize % 79), Action::Block]);
                }
                self.connection
                    .advance(self.connection.registration(), ready);
                let after = self.connection.stats();
                assert!(after.read_bytes - before.read_bytes <= 65_536);
                assert!(after.written_bytes - before.written_bytes <= 65_536);
                assert!(after.attempts - before.attempts <= 64);
            }
            8 => {
                if let Some(frame) = self.connection.take_received() {
                    assert!(
                        self.next_rx < self.expected_rx.len(),
                        "invalid or duplicate receive after corruption"
                    );
                    let expected = self.expected_rx[self.next_rx].clone();
                    assert_eq!(frame.payload(), expected);
                    self.next_rx += 1;
                    self.leases.push(Lease { frame, expected });
                }
            }
            9 => {
                if !self.leases.is_empty() {
                    self.leases.remove(random(rng) as usize % self.leases.len());
                }
            }
            10 => self.collect(),
            11 => {
                if let Some(item) = self.accepted.iter_mut().find(|i| i.ticket.is_some()) {
                    item.ticket.take();
                }
            }
            12 => {
                self.now +=
                    [0, 20_000_000, 29_900_000_000, 30_000_000_000][random(rng) as usize % 4];
                self.clock.set(self.now);
                self.connection
                    .advance(self.connection.registration(), Ready::default());
            }
            13 => {
                self.connection.cancellation().cancel();
                self.connection
                    .advance(self.connection.registration(), Ready::default());
            }
            14 => self.connection.begin_drain(),
            17 | 19 | 20 => {
                // These faults must reach a real pending write. Reap completed
                // tickets first, then queue a probe if the existing prefix is
                // already flushed. The independent wire ledger still includes
                // every accepted byte.
                self.collect();
                if !self.accepted.iter().any(|item| item.ticket.is_some()) {
                    self.queue_payload(&[operation as u8]);
                }
                let mut script = self.script.lock().unwrap();
                script.writes.clear();
                script.writes.push_back(match kind {
                    17 => Action::Error,
                    19 => Action::Overcount,
                    20 => Action::Zero,
                    _ => unreachable!(),
                });
                drop(script);
                let before = self.script.lock().unwrap().write_calls;
                self.connection.advance(
                    self.connection.registration(),
                    Ready {
                        writable: true,
                        ..Ready::default()
                    },
                );
                assert!(self.script.lock().unwrap().write_calls > before);
                match kind {
                    17 => {
                        let error = self.connection.terminal_error().expect("write reset");
                        assert_eq!(error.kind, ErrorKind::Io);
                        assert_eq!(error.io_kind, Some(std::io::ErrorKind::ConnectionReset));
                    }
                    19 => assert_eq!(
                        self.connection.terminal_error().map(|error| error.kind),
                        Some(ErrorKind::AdapterViolation)
                    ),
                    20 => assert_eq!(
                        self.connection.terminal_error().map(|error| error.kind),
                        Some(ErrorKind::WriteZero)
                    ),
                    _ => unreachable!(),
                }
            }
            16 | 18 | 21 => {
                self.drain_received();
                let mut script = self.script.lock().unwrap();
                script.reads.clear();
                script.reads.push_back(match kind {
                    16 => Action::Error,
                    18 => Action::Overcount,
                    21 => Action::Zero,
                    _ => unreachable!(),
                });
                drop(script);
                self.connection.advance(
                    self.connection.registration(),
                    Ready {
                        readable: true,
                        ..Ready::default()
                    },
                );
                if kind == 16 {
                    let error = self.connection.terminal_error().expect("read reset");
                    assert_eq!(error.kind, ErrorKind::Io);
                    assert_eq!(error.io_kind, Some(std::io::ErrorKind::ConnectionReset));
                } else if kind == 18 {
                    assert_eq!(
                        self.connection.terminal_error().map(|error| error.kind),
                        Some(ErrorKind::AdapterViolation)
                    );
                } else {
                    assert!(self.connection.terminal_error().is_some());
                }
            }
            22 => {
                let before = self.connection.stats();
                let registration = self.connection.registration();
                let wrong = Registration {
                    slot: registration.slot,
                    generation: registration.generation.wrapping_add(1).max(1),
                };
                self.connection.advance(
                    wrong,
                    Ready {
                        readable: true,
                        writable: true,
                        ..Ready::default()
                    },
                );
                let after = self.connection.stats();
                assert_eq!(after.attempts, before.attempts, "stale event reached I/O");
                assert_eq!(self.connection.phase(), ConnectionPhase::Open);
            }
            23 => {
                self.connection.advance(
                    self.connection.registration(),
                    Ready {
                        invalid: true,
                        ..Ready::default()
                    },
                );
                assert_eq!(
                    self.connection.terminal_error().map(|error| error.kind),
                    Some(ErrorKind::AdapterViolation)
                );
            }
            24 => self.connection.advance(
                self.connection.registration(),
                Ready {
                    readable: true,
                    writable: true,
                    hangup: true,
                    ..Ready::default()
                },
            ),
            25 => {
                let before = self.connection.stats();
                self.connection.advance(
                    self.connection.registration(),
                    Ready {
                        error: true,
                        ..Ready::default()
                    },
                );
                assert_eq!(self.connection.stats().attempts, before.attempts);
                assert_eq!(self.connection.phase(), ConnectionPhase::Open);
            }
            _ => unreachable!(),
        }
        self.verify();
    }
    fn finish(mut self) {
        // Observe every frame the terminal trace can still deliver before
        // cleanup destroys the owner's internal receive queue.
        self.drain_received();
        self.connection.cancel();
        self.verify();
        assert!(self.accepted.iter().all(|i| i.ticket.is_none()));
        self.leases.clear();
        let transport = self.transport.clone();
        drop(self);
        assert_eq!(transport.payload_bytes(), 0);
        assert_eq!(transport.connection_count(), 0);
    }
}

const EVENT_KINDS: usize = 26;
fn events(seed: u64, operations: usize) -> [u64; EVENT_KINDS] {
    let mut rng = seed;
    let mut fixture = Fixture::new(seed);
    let mut counts = [0; EVENT_KINDS];
    for operation in 0..operations {
        let selector = random(&mut rng) % 122;
        let kind = if selector < 112 {
            (selector % 16) as usize
        } else {
            16 + (selector - 112) as usize
        };
        if fixture.connection.phase() == ConnectionPhase::Closed || kind >= 16 {
            fixture.finish();
            fixture = Fixture::new(random(&mut rng));
        }
        fixture.operation(&mut rng, operation, kind);
        counts[kind] += 1;
    }
    fixture.finish();
    counts
}
const CORPUS: [u64; 8] = [
    0,
    1,
    2,
    255,
    65_535,
    u64::MAX,
    20_261_019,
    0x9e3779b97f4a7c15,
];

#[test]
fn production_four_slot_events_v3_256_seeds_1024_operations_and_corpus() {
    let mut counts = [0; EVENT_KINDS];
    for seed in 0..256 {
        for (total, count) in counts.iter_mut().zip(events(seed, 1024)) {
            *total += count;
        }
    }
    for seed in CORPUS {
        for (total, count) in counts.iter_mut().zip(events(seed, 1024)) {
            *total += count;
        }
    }
    assert!(
        counts.iter().all(|&count| count > 0),
        "event kind not covered: {counts:?}"
    );
}

#[test]
#[ignore = "V3 actual thirty-minute production event-sequence mutation campaign"]
fn event_sequence_fuzz_thirty_minutes() {
    for seed in CORPUS {
        events(seed, 1024);
    }
    let started = Instant::now();
    let mut state = soak_support::SEED;
    let mut iterations = 0_u64;
    let mut counts = [0; EVENT_KINDS];
    while started.elapsed() < Duration::from_secs(1800) {
        let seed = random(&mut state);
        let event_counts = match std::panic::catch_unwind(|| events(seed, 1024)) {
            Ok(event_counts) => event_counts,
            Err(error) => {
                let prefix = (1..=1024)
                    .find(|&n| std::panic::catch_unwind(|| events(seed, n)).is_err())
                    .unwrap();
                if let Some(directory) = std::env::var_os("XADB_COMM_CRASH_DIRECTORY") {
                    let directory = std::path::Path::new(&directory);
                    std::fs::create_dir_all(directory).unwrap();
                    std::fs::write(
                        directory.join(format!("events-{seed}.txt")),
                        format!("seed={seed}\nfirst_failing_prefix={prefix}\n"),
                    )
                    .unwrap();
                }
                eprintln!("COMM_EVENT_COUNTEREXAMPLE seed={seed} first_failing_prefix={prefix}");
                std::panic::resume_unwind(error);
            }
        };
        for (total, count) in counts.iter_mut().zip(event_counts) {
            *total += count;
        }
        iterations += 1024;
    }
    assert!(
        counts.iter().all(|&count| count > 0),
        "event kind not covered: {counts:?}"
    );
    println!(
        "COMM_EVENT_COUNTS target=events counts={}",
        counts
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    );
    println!(
        "COMM_SOAK_COMPLETE target=events elapsed_ms={} iterations={iterations} seed={} corpus=8 crashes=0",
        started.elapsed().as_millis(),
        soak_support::SEED
    );
}
