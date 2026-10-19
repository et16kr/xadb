mod support;
use std::{
    io,
    sync::{Arc, Mutex},
};
use support::*;
use xadb_foundation::network::{ByteStream, Connector, Ready, Registration};
use xadb_transport::{ConnectionPhase, ErrorKind, MAX_WIRE_BYTES, SendOutcome, Transport};

#[test]
fn short_io_interrupt_block_and_zero_keep_offsets_and_exact_failure_bytes() {
    let (t, clock, s, mut c) = fixture(vec![]);
    s.lock()
        .unwrap()
        .writes
        .extend([Action::Interrupt, Action::Limit(3), Action::Block]);
    let ticket = c.sender().try_send(&[1, 2]).unwrap();
    advance(&mut c);
    assert_eq!(s.lock().unwrap().output, frame(&[1, 2])[..3]);
    advance(&mut c);
    assert_eq!(s.lock().unwrap().write_calls, 3);
    clock.set(20_000_000);
    advance(&mut c);
    let result = ticket
        .try_take()
        .unwrap_or_else(|_| panic!("COMM_MUTANT offset-reset: partial send did not complete"));
    assert_eq!(
        result,
        SendOutcome::FlushedLocally,
        "COMM_MUTANT offset-reset: wrong terminal outcome"
    );
    assert_eq!(s.lock().unwrap().output, frame(&[1, 2]));
    assert_eq!(t.payload_bytes(), 0);
    for action in [Action::Zero, Action::Overcount, Action::Error] {
        let (_, _, s, mut c) = fixture(vec![]);
        s.lock().unwrap().writes.extend([Action::Limit(5), action]);
        let ticket = c.sender().try_send(&[]).unwrap();
        advance(&mut c);
        match outcome(ticket) {
            SendOutcome::Failed { written_bytes, .. } => assert_eq!(written_bytes, 5),
            _ => panic!(),
        }
    }
    for action in [Action::Overcount, Action::Error] {
        let (_, _, s, mut c) = fixture(vec![]);
        s.lock().unwrap().reads.push_back(action);
        advance(&mut c);
        assert_eq!(c.phase(), ConnectionPhase::Closed);
    }
}
#[test]
fn quotas_hold_external_rx_and_terminal_tickets_across_owner_drop() {
    let (t, _, _, mut c) = fixture([frame(&[1]), frame(&[2]), frame(&[3])].concat());
    advance(&mut c);
    let first = c.take_received().unwrap();
    let second = c.take_received().unwrap();
    assert!(c.interest().is_none());
    assert_eq!(t.payload_bytes(), 86);
    drop(first);
    advance(&mut c);
    let third = c.take_received().unwrap();
    assert_eq!(third.payload(), [3]);
    let sender = c.sender();
    let tickets = (0..4)
        .map(|_| sender.try_send(&[]).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        sender.try_send(&[]).err().unwrap().kind,
        ErrorKind::Backpressured
    );
    drop(c);
    assert_eq!(t.connection_count(), 1);
    for ticket in tickets {
        assert!(
            matches!(outcome(ticket),SendOutcome::Failed{error,..} if error.kind==ErrorKind::Closed)
        );
    }
    drop(second);
    drop(third);
    drop(sender);
    assert_eq!(t.payload_bytes(), 0);
    assert_eq!(t.connection_count(), 0);
    let (t, _, _, mut c) = fixture(vec![]);
    let sender = c.sender();
    let mut tickets = (0..4)
        .map(|_| sender.try_send(&[]).unwrap())
        .collect::<Vec<_>>();
    advance(&mut c);
    assert_eq!(t.payload_bytes(), 0);
    assert_eq!(
        sender.try_send(&[]).err().unwrap().kind,
        ErrorKind::Backpressured
    );
    assert_eq!(outcome(tickets.pop().unwrap()), SendOutcome::FlushedLocally);
    assert!(sender.try_send(&[]).is_ok());
}
#[test]
fn connection_limit_and_global_payload_competition_release_without_leaks() {
    let t = Transport::new(Arc::new(TestClock::default()));
    let mut connections = vec![];
    for slot in 0..128 {
        connections.push(
            t.accept(Box::new(Stream(
                Arc::new(Mutex::new(Script::default())),
                Registration {
                    slot,
                    generation: 1,
                },
            )))
            .unwrap(),
        );
    }
    assert_eq!(t.connection_count(), 128);
    assert_eq!(
        t.accept(Box::new(Stream(
            Arc::new(Mutex::new(Script::default())),
            Registration {
                slot: 129,
                generation: 1
            }
        )))
        .err()
        .unwrap()
        .kind,
        ErrorKind::ResourceExhausted
    );
    let payload = vec![0; 1_048_576];
    let mut tickets = vec![];
    for c in &connections {
        for _ in 0..4 {
            match c.sender().try_send(&payload) {
                Ok(ticket) => tickets.push(ticket),
                Err(e) => assert_eq!(e.kind, ErrorKind::Backpressured),
            }
        }
    }
    assert_eq!(tickets.len(), (64 * 1024 * 1024) / MAX_WIRE_BYTES);
    assert!(t.payload_bytes() <= 64 * 1024 * 1024);
    drop(connections);
    assert_eq!(t.payload_bytes(), 0);
    assert!(t.connection_count() > 0);
    drop(tickets);
    assert_eq!(t.connection_count(), 0);
}
#[test]
fn last_io_crossing_deadline_never_reports_success_but_keeps_full_progress() {
    let (_, clock, s, mut c) = fixture(vec![]);
    let ticket = c.sender().try_send(&[]).unwrap();
    s.lock().unwrap().clock_after_write = Some((clock, 30_000_000_000));
    advance(&mut c);
    assert_eq!(
        outcome(ticket),
        SendOutcome::Failed {
            error: ErrorKind::SendTimedOut.into(),
            written_bytes: 42
        },
        "COMM_MUTANT stale-clock: last write crossed deadline but reported success"
    );
    let (_, clock, s, mut c) = fixture(frame(&[]));
    s.lock().unwrap().reads.push_back(Action::Limit(1));
    s.lock().unwrap().reads.push_back(Action::Block);
    advance(&mut c);
    clock.set(29_999_999_999);
    s.lock().unwrap().clock_after_read = Some((clock.clone(), 30_000_000_000));
    advance(&mut c);
    assert!(c.take_received().is_none());
    assert_eq!(c.terminal_error().unwrap().kind, ErrorKind::FrameTimedOut);
}
#[test]
fn drain_discards_partial_rx_timer_but_preserves_verified_frames_and_tx() {
    let (t, clock, s, mut c) = fixture([frame(&[7]), frame(&[8])[..20].to_vec()].concat());
    advance(&mut c);
    clock.set(29_900_000_000);
    let ticket = c.sender().try_send(&[3]).unwrap();
    c.begin_drain();
    c.begin_drain();
    assert_eq!(t.payload_bytes(), 43 + 43);
    clock.set(31_000_000_000);
    advance(&mut c);
    assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
    assert_eq!(c.take_received().unwrap().payload(), [7]);
    assert_eq!(c.phase(), ConnectionPhase::Closed);
    assert_eq!(s.lock().unwrap().output, frame(&[3]));
    assert_eq!(t.payload_bytes(), 0);
}
#[test]
fn drop_partial_tx_cancel_after_last_write_and_stale_event_are_terminal() {
    let (_, _, s, mut c) = fixture(vec![]);
    s.lock()
        .unwrap()
        .writes
        .extend([Action::Limit(3), Action::Block]);
    let ticket = c.sender().try_send(&[]).unwrap();
    advance(&mut c);
    drop(c);
    assert_eq!(
        ticket
            .try_take()
            .unwrap_or_else(|_| panic!("COMM_MUTANT drop-pending: owner Drop left ticket Pending")),
        SendOutcome::Failed {
            error: ErrorKind::Closed.into(),
            written_bytes: 3
        },
        "COMM_MUTANT drop-pending: owner Drop lost written prefix"
    );
    let (_, _, s, mut c) = fixture(vec![]);
    let ticket = c.sender().try_send(&[]).unwrap();
    s.lock().unwrap().cancel_after_write = Some(c.cancellation());
    advance(&mut c);
    assert_eq!(
        outcome(ticket),
        SendOutcome::Failed {
            error: ErrorKind::Cancelled.into(),
            written_bytes: 42
        }
    );
    let (_, _, s, mut c) = fixture(vec![]);
    let ticket = c.sender().try_send(&[]).unwrap();
    c.advance(
        Registration {
            slot: 1,
            generation: 2,
        },
        Ready {
            writable: true,
            ..Ready::default()
        },
    );
    assert_eq!(
        s.lock().unwrap().write_calls,
        0,
        "COMM_MUTANT stale-event: old generation performed a write"
    );
    c.cancel();
    assert!(matches!(
        outcome(ticket),
        SendOutcome::Failed {
            written_bytes: 0,
            ..
        }
    ));
    c.cancel();
    assert_eq!(s.lock().unwrap().closed, 1);
}
struct TestConnector(Arc<Mutex<Script>>);
impl Connector for TestConnector {
    fn connect(&mut self) -> io::Result<Box<dyn ByteStream>> {
        Ok(Box::new(Stream(
            self.0.clone(),
            Registration {
                slot: 1,
                generation: 1,
            },
        )))
    }
}
#[test]
fn connecting_has_write_interest_without_tx_and_deadlines_are_absolute() {
    let clock = Arc::new(TestClock::default());
    let t = Transport::new(clock.clone());
    let s = Arc::new(Mutex::new(Script::default()));
    let mut c = t.connect(&mut TestConnector(s)).unwrap();
    assert!(
        c.interest()
            .expect("COMM_MUTANT connect-interest: pending connect has no interest")
            .writable,
        "COMM_MUTANT connect-interest: pending connect lacks writable interest"
    );
    assert_eq!(
        c.sender().try_send(&[]).err().unwrap().kind,
        ErrorKind::Closed
    );
    advance(&mut c);
    assert_eq!(c.phase(), ConnectionPhase::Open);
    clock.set(60_000_000_000);
    c.advance(c.registration(), Ready::default());
    assert_eq!(c.terminal_error().unwrap().kind, ErrorKind::IdleTimedOut);
    let clock = Arc::new(TestClock::default());
    let t = Transport::new(clock.clone());
    let s = Arc::new(Mutex::new(Script {
        connecting: true,
        ..Script::default()
    }));
    let mut c = t.connect(&mut TestConnector(s)).unwrap();
    clock.set(5_000_000_000);
    advance(&mut c);
    assert_eq!(c.terminal_error().unwrap().kind, ErrorKind::ConnectTimedOut);
    let (_, clock, s, mut c) = fixture(vec![]);
    s.lock().unwrap().writes.push_back(Action::Block);
    let ticket = c.sender().try_send(&[]).unwrap();
    c.begin_drain();
    clock.set(5_000_000_000);
    advance(&mut c);
    assert!(
        matches!(outcome(ticket),SendOutcome::Failed{error,..} if error.kind==ErrorKind::DrainTimedOut)
    );
}
#[test]
fn clock_regression_closes_gate_permanently_and_zero_progress_is_bounded() {
    let (_, clock, _, mut c) = fixture(vec![]);
    clock.set(100);
    drop(c.sender().try_send(&[]).unwrap());
    clock.set(99);
    assert_eq!(
        c.sender().try_send(&[]).err().unwrap().kind,
        ErrorKind::AdapterViolation
    );
    clock.set(100);
    assert_eq!(
        c.sender().try_send(&[]).err().unwrap().kind,
        ErrorKind::AdapterViolation
    );
    advance(&mut c);
    assert_eq!(c.phase(), ConnectionPhase::Closed);
    let (_, _, s, mut c) = fixture(vec![]);
    let _ticket = c.sender().try_send(&[]).unwrap();
    s.lock()
        .unwrap()
        .writes
        .extend(std::iter::repeat_n(Action::Interrupt, 100));
    advance(&mut c);
    assert_eq!(s.lock().unwrap().write_calls, 32);
}
#[test]
fn concurrent_producers_and_cancellation_return_every_ticket_and_charge() {
    for _ in 0..64 {
        let (t, _, _, mut c) = fixture(vec![]);
        let sender = c.sender();
        let threads = (0..8)
            .map(|i| {
                let sender = sender.clone();
                std::thread::spawn(move || sender.try_send(&[i; 128]))
            })
            .collect::<Vec<_>>();
        c.cancel();
        drop(c);
        for thread in threads {
            if let Ok(ticket) = thread.join().unwrap() {
                assert!(matches!(outcome(ticket), SendOutcome::Failed { .. }));
            }
        }
        drop(sender);
        assert_eq!(t.payload_bytes(), 0);
        assert_eq!(t.connection_count(), 0);
    }
}

#[test]
fn interest_clock_regression_is_sticky_even_after_time_recovers() {
    let (_, clock, _, mut c) = fixture(vec![]);
    clock.set(100);
    assert!(c.interest().is_some());
    clock.set(99);
    assert!(c.interest().is_none());
    clock.set(100);
    assert_eq!(
        c.sender().try_send(&[]).err().unwrap().kind,
        ErrorKind::AdapterViolation
    );
    advance(&mut c);
    assert_eq!(c.phase(), ConnectionPhase::Closed);
}

#[test]
fn error_only_hint_without_socket_error_is_backed_off() {
    let (_, clock, s, mut c) = fixture(vec![]);
    c.advance(
        c.registration(),
        Ready {
            error: true,
            ..Ready::default()
        },
    );
    assert!(c.interest().is_none());
    advance(&mut c);
    assert_eq!(s.lock().unwrap().read_calls, 0);
    clock.set(20_000_000);
    assert!(c.interest().is_some());
}
