mod support;

use foundation::network::{Ready, Registration};
use std::{
    sync::{Arc, Barrier, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use support::{Action, Script, Stream, TestClock, frame};
use transport::{
    ConnectionPhase, ErrorKind, PROCESS_PAYLOAD_BYTES, ReceivedFrame, SendOutcome, SendTicket,
    Transport,
};

const PRODUCERS: usize = 4;
const SENDS_PER_PRODUCER: usize = 128;

#[derive(Default)]
struct RoundCounts {
    attempts: u64,
    accepted: u64,
    backpressured: u64,
    terminal_rejected: u64,
    race_accepted: u64,
    race_rejected: u64,
    postclose_rejected: u64,
    leases: u64,
    flushed: u64,
    failed: u64,
}

fn stress_round(seed: u64, terminal_mode: usize) -> RoundCounts {
    assert!(terminal_mode < 3);
    let clock = Arc::new(TestClock::default());
    let transport = Transport::new(clock.clone());
    let incoming_payloads = [
        vec![seed as u8],
        vec![0, seed.wrapping_add(1) as u8],
        vec![],
    ];
    let incoming = incoming_payloads
        .iter()
        .flat_map(|payload| frame(payload))
        .collect();
    let script = Arc::new(Mutex::new(Script {
        input: incoming,
        reads: Default::default(),
        writes: [Action::Limit(1), Action::Block].into(),
        eof: false,
        ..Script::default()
    }));
    let mut connection = transport
        .accept(Box::new(Stream(
            script.clone(),
            Registration {
                slot: 0,
                generation: 1,
            },
        )))
        .unwrap();
    let registration = connection.registration();
    let cancellation = connection.cancellation();
    let sender = connection.sender();
    // Two complete frames are read and verified, then held by the lease worker
    // until owner termination races with their release.
    let (lease_tx, lease_rx) = mpsc::channel::<(ReceivedFrame, Vec<u8>)>();
    let release_leases = Arc::new(Barrier::new(2));
    let lease_release = release_leases.clone();
    let lease_expectations = incoming_payloads[..2].to_vec();
    let lease_worker = thread::spawn(move || {
        let mut held = Vec::new();
        for expected in lease_expectations {
            let (frame, delivered_expected) = lease_rx
                .recv()
                .expect("expected RX lease was not delivered");
            assert_eq!(delivered_expected, expected);
            assert_eq!(frame.payload(), expected);
            held.push(frame);
        }
        assert!(lease_rx.try_recv().is_err(), "unexpected extra RX lease");
        lease_release.wait();
        let count = held.len() as u64;
        drop(held);
        count
    });
    connection.advance(
        registration,
        Ready {
            readable: true,
            ..Ready::default()
        },
    );
    let mut received = 0;
    while let Some(frame) = connection.take_received() {
        assert!(received < 2, "owner delivered more than two RX slots");
        lease_tx
            .send((frame, incoming_payloads[received].clone()))
            .unwrap();
        received += 1;
    }
    assert_eq!(received, 2, "owner did not deliver both expected RX frames");
    drop(lease_tx);

    let start = Arc::new(Barrier::new(PRODUCERS + 1));
    let pressure = Arc::new(Barrier::new(PRODUCERS + 1));
    let race_start = Arc::new(Barrier::new(PRODUCERS + 1));
    let race_tail = Arc::new(Barrier::new(PRODUCERS));
    let race_done = Arc::new(Barrier::new(PRODUCERS + 1));
    let postclose = Arc::new(Barrier::new(PRODUCERS + 1));
    let (ticket_tx, ticket_rx) = mpsc::channel::<(SendTicket, Vec<u8>)>();
    let (race_first_tx, race_first_rx) = mpsc::channel::<bool>();
    let mut producers = Vec::new();
    for producer in 0..PRODUCERS {
        let sender = sender.clone();
        let start = start.clone();
        let pressure = pressure.clone();
        let race_start = race_start.clone();
        let race_tail = race_tail.clone();
        let race_done = race_done.clone();
        let postclose = postclose.clone();
        let ticket_tx = ticket_tx.clone();
        let race_first_tx = race_first_tx.clone();
        producers.push(thread::spawn(move || {
            start.wait();
            let mut attempts = 0;
            let mut backpressured_count = 0;
            let mut terminal_rejected = 0;
            let mut race_rejected = 0;
            let mut postclose_rejected = 0;
            let mut item = 0;
            loop {
                attempts += 1;
                let mut payload = Vec::with_capacity(16);
                payload.extend_from_slice(&seed.to_le_bytes());
                payload.extend_from_slice(&(producer as u32).to_le_bytes());
                payload.extend_from_slice(&(item as u32).to_le_bytes());
                match sender.try_send(&payload) {
                    Ok(ticket) => ticket_tx.send((ticket, payload)).unwrap(),
                    Err(error) if error.kind == ErrorKind::Backpressured => {
                        backpressured_count += 1;
                        pressure.wait();
                        break;
                    }
                    Err(error) => panic!("producer stopped before pressure barrier: {error:?}"),
                }
                item += 1;
                assert!(
                    item <= SENDS_PER_PRODUCER,
                    "bounded queue did not backpressure"
                );
            }

            // Four slots have been freed by consuming their terminal tickets.
            // This encoding/admission attempt starts in the same phase as the
            // owner's cancel, drain, or Drop transition.
            let mut payload = vec![producer as u8; 16 * 1024];
            payload[..8].copy_from_slice(&seed.to_le_bytes());
            race_start.wait();
            if producer != 0 {
                race_tail.wait();
            }
            attempts += 1;
            match sender.try_send(&payload) {
                Ok(ticket) => {
                    ticket_tx.send((ticket, payload)).unwrap();
                    if producer == 0 {
                        race_first_tx.send(true).unwrap();
                    }
                }
                Err(error) if matches!(error.kind, ErrorKind::Cancelled | ErrorKind::Closed) => {
                    terminal_rejected += 1;
                    race_rejected += 1;
                    if producer == 0 {
                        race_first_tx.send(false).unwrap();
                    }
                }
                Err(error) => {
                    panic!("producer admission race returned unexpected error: {error:?}")
                }
            }
            race_done.wait();

            // The owner is terminal and every raced ticket has been collected
            // before this post-close probe reaches the admission gate.
            postclose.wait();
            attempts += 1;
            match sender.try_send(&[producer as u8, 0x5a]) {
                Err(error) if matches!(error.kind, ErrorKind::Cancelled | ErrorKind::Closed) => {
                    terminal_rejected += 1;
                    postclose_rejected += 1;
                }
                Err(error) => {
                    panic!("terminal producer race returned nonterminal error: {error:?}")
                }
                Ok(_) => panic!("terminal producer race was admitted"),
            }
            (
                attempts,
                backpressured_count,
                terminal_rejected,
                race_rejected,
                postclose_rejected,
            )
        }));
    }
    drop(ticket_tx);
    start.wait();
    let first = ticket_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("concurrent producers admitted no ticket");
    let mut tickets = vec![first];
    pressure.wait();

    // All producers have observed a full four-slot queue. Keep one TX partial
    // write as a witness, then finish and collect all four tickets to release
    // the admission slots for a real producer/shutdown race.
    connection.advance(
        registration,
        Ready {
            writable: true,
            ..Ready::default()
        },
    );
    assert_eq!(
        script.lock().unwrap().output.len(),
        1,
        "partial write witness missing"
    );
    clock.set(20_000_000);
    script.lock().unwrap().writes.clear();
    for _ in 0..8 {
        connection.advance(
            registration,
            Ready {
                writable: true,
                ..Ready::default()
            },
        );
    }
    tickets.extend(ticket_rx.try_iter());
    tickets.sort_by_key(|(ticket, _)| ticket.sequence());
    assert_eq!(
        tickets.len(),
        PRODUCERS,
        "initial four-slot pressure was not exact"
    );
    let mut accepted_payloads = Vec::new();
    let mut counts = RoundCounts {
        accepted: tickets.len() as u64,
        ..RoundCounts::default()
    };
    let mut initial_wire = Vec::new();
    for (index, (ticket, payload)) in tickets.into_iter().enumerate() {
        assert_eq!(
            ticket.sequence(),
            index as u64 + 1,
            "admission sequence gap"
        );
        accepted_payloads.push((ticket.sequence(), payload.clone()));
        initial_wire.extend(frame(&payload));
        assert_eq!(
            ticket
                .try_take()
                .ok()
                .expect("initial ticket did not flush"),
            SendOutcome::FlushedLocally
        );
        counts.flushed += 1;
    }
    assert_eq!(script.lock().unwrap().output, initial_wire);

    let mut cancel_join = None;
    race_start.wait();
    let first_race_admitted = race_first_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("first producer did not finish its admission attempt");
    race_tail.wait();
    if terminal_mode == 0 {
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>();
        let (ack_tx, ack_rx) = mpsc::channel::<()>();
        let cancel = cancellation.clone();
        cancel_join = Some(thread::spawn(move || {
            if cancel_rx.recv().is_ok() {
                cancel.cancel();
                ack_tx.send(()).unwrap();
            }
        }));
        cancel_tx.send(()).unwrap();
        release_leases.wait();
        ack_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("cancellation thread did not acknowledge its request");
        connection.advance(registration, Ready::default());
        assert_eq!(connection.phase(), ConnectionPhase::Closed);
        drop(connection);
    } else if terminal_mode == 1 {
        connection.begin_drain();
        release_leases.wait();
        clock.set(40_000_000);
        for _ in 0..4 {
            if connection.phase() == ConnectionPhase::Closed {
                break;
            }
            connection.advance(
                registration,
                Ready {
                    writable: true,
                    ..Ready::default()
                },
            );
        }
        assert_eq!(connection.phase(), ConnectionPhase::Closed);
        drop(connection);
    } else {
        release_leases.wait();
        drop(connection);
    }
    drop(sender);
    if let Some(join) = cancel_join {
        join.join().expect("cancellation thread panicked");
    }
    race_done.wait();
    assert!(
        first_race_admitted,
        "available TX slot did not reach admission"
    );
    let mut raced_tickets: Vec<_> = ticket_rx.try_iter().collect();
    raced_tickets.sort_by_key(|(ticket, _)| ticket.sequence());
    for (index, (ticket, payload)) in raced_tickets.into_iter().enumerate() {
        let expected_sequence = PRODUCERS as u64 + index as u64 + 1;
        assert_eq!(
            ticket.sequence(),
            expected_sequence,
            "raced admission sequence gap"
        );
        accepted_payloads.push((ticket.sequence(), payload.clone()));
        counts.accepted += 1;
        counts.race_accepted += 1;
        // Results are terminal after the owner transition, and consuming each
        // ticket returns its slot before post-close producer probes are released.
        match ticket
            .try_take()
            .ok()
            .expect("raced ticket was not terminal")
        {
            SendOutcome::FlushedLocally => {
                assert_eq!(terminal_mode, 1, "non-drain race unexpectedly flushed");
                counts.flushed += 1;
            }
            SendOutcome::Failed {
                error,
                written_bytes,
            } => {
                assert_eq!(written_bytes, 0, "owner closed after partial raced write");
                assert_eq!(
                    error.kind,
                    match terminal_mode {
                        0 => ErrorKind::Cancelled,
                        1 | 2 => ErrorKind::Closed,
                        _ => unreachable!(),
                    }
                );
                counts.failed += 1;
            }
        }
    }
    let output = script.lock().unwrap().output.clone();
    accepted_payloads.sort_by_key(|(sequence, _)| *sequence);
    let mut expected_wire = Vec::new();
    for (_, payload) in &accepted_payloads {
        expected_wire.extend(frame(payload));
    }
    assert!(output.len() <= expected_wire.len());
    assert_eq!(
        output,
        expected_wire[..output.len()],
        "wire output left FIFO prefix"
    );

    if terminal_mode == 1 {
        assert_eq!(counts.failed, 0, "accepted TX did not drain locally");
        assert_eq!(output, expected_wire, "drain did not flush all accepted TX");
    } else {
        assert_eq!(
            output, initial_wire,
            "terminal race wrote bytes after close began"
        );
    }
    postclose.wait();
    for producer in producers {
        let (attempts, rejected_backpressure, terminal_rejected, race_rejected, postclose_rejected) =
            producer.join().expect("producer thread panicked");
        counts.attempts += attempts;
        counts.backpressured += rejected_backpressure;
        counts.terminal_rejected += terminal_rejected;
        counts.race_rejected += race_rejected;
        counts.postclose_rejected += postclose_rejected;
    }
    assert_eq!(
        counts.attempts,
        counts.accepted + counts.backpressured + counts.terminal_rejected,
        "producer attempt accounting mismatch"
    );
    assert!(
        counts.backpressured > 0,
        "synchronized producers missed pressure"
    );
    assert_eq!(counts.postclose_rejected, PRODUCERS as u64);
    assert!(counts.terminal_rejected >= counts.postclose_rejected);
    assert!(
        ticket_rx.try_recv().is_err(),
        "closed producer admitted an extra ticket"
    );
    counts.leases = lease_worker.join().expect("lease worker panicked");
    assert_eq!(counts.leases, 2, "RX lease count/order was incomplete");
    assert!(transport.payload_bytes() <= PROCESS_PAYLOAD_BYTES);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
    counts
}

#[test]
fn producer_lease_cancel_drain_and_drop_thread_races() {
    let mut totals = RoundCounts::default();
    for round in 0_u64..256 {
        let counts = stress_round(round.wrapping_mul(0x9e37_79b9), round as usize % 3);
        totals.attempts += counts.attempts;
        totals.accepted += counts.accepted;
        totals.backpressured += counts.backpressured;
        totals.terminal_rejected += counts.terminal_rejected;
        totals.race_accepted += counts.race_accepted;
        totals.race_rejected += counts.race_rejected;
        totals.postclose_rejected += counts.postclose_rejected;
        totals.leases += counts.leases;
        totals.flushed += counts.flushed;
        totals.failed += counts.failed;
    }
    assert!(totals.accepted >= 256);
    assert!(totals.backpressured > 0 && totals.leases > 0);
    assert_eq!(totals.postclose_rejected, 256 * PRODUCERS as u64);
    assert_eq!(
        totals.race_accepted + totals.race_rejected,
        256 * PRODUCERS as u64
    );
    assert!(totals.race_accepted >= 256);
    assert_eq!(
        totals.attempts,
        totals.accepted + totals.backpressured + totals.terminal_rejected
    );
    assert_eq!(totals.accepted, totals.flushed + totals.failed);
}

#[test]
#[ignore = "V3 actual thirty-minute producer/lease/cancel/drain/drop stress"]
fn producer_lease_thread_stress_thirty_minutes() {
    let corpus = [
        0,
        1,
        2,
        255,
        65_535,
        u64::MAX,
        20_261_019,
        0x9e37_79b9_7f4a_7c15,
    ];
    for (index, seed) in corpus.into_iter().enumerate() {
        stress_round(seed, index % 3);
    }
    let started = Instant::now();
    let mut seed = 20_261_019_u64;
    let mut totals = RoundCounts::default();
    let mut rounds = 0_u64;
    while started.elapsed() < Duration::from_secs(1800) {
        let mode = (rounds % 3) as usize;
        let current = seed;
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let result = std::panic::catch_unwind(|| stress_round(current, mode));
        let counts = match result {
            Ok(counts) => counts,
            Err(error) => {
                if let Some(directory) = std::env::var_os("XADB_COMM_CRASH_DIRECTORY") {
                    let directory = std::path::Path::new(&directory);
                    std::fs::create_dir_all(directory).unwrap();
                    std::fs::write(
                        directory.join(format!("threads-{current}.txt")),
                        format!("seed={current}\nmode={mode}\nround={rounds}\n"),
                    )
                    .unwrap();
                }
                eprintln!("COMM_THREAD_COUNTEREXAMPLE seed={current} mode={mode} round={rounds}");
                std::panic::resume_unwind(error);
            }
        };
        totals.attempts += counts.attempts;
        totals.accepted += counts.accepted;
        totals.backpressured += counts.backpressured;
        totals.terminal_rejected += counts.terminal_rejected;
        totals.race_accepted += counts.race_accepted;
        totals.race_rejected += counts.race_rejected;
        totals.postclose_rejected += counts.postclose_rejected;
        totals.leases += counts.leases;
        totals.flushed += counts.flushed;
        totals.failed += counts.failed;
        rounds += 1;
    }
    println!(
        "COMM_THREAD_COUNTS target=threads rounds={rounds} attempts={} accepted={} backpressured={} rejected={} race_accepted={} race_rejected={} postclose_rejected={} leases={} flushed={} failed={}",
        totals.attempts,
        totals.accepted,
        totals.backpressured,
        totals.terminal_rejected,
        totals.race_accepted,
        totals.race_rejected,
        totals.postclose_rejected,
        totals.leases,
        totals.flushed,
        totals.failed
    );
    println!(
        "COMM_SOAK_COMPLETE target=threads elapsed_ms={} iterations={} seed=20261019 corpus=8 crashes=0",
        started.elapsed().as_millis(),
        totals.attempts
    );
}
