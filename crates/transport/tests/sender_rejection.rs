mod support;

use foundation::network::{Interest, ReadinessEvent, ReadinessWait};
use std::{io, time::Duration};
use support::{Action, advance, fixture, outcome};
use transport::{
    ConnectionPhase, ConnectionSender, Driver, Error, ErrorKind, SendOutcome, SendTicket,
};

fn full_queue(sender: &ConnectionSender) -> Vec<SendTicket> {
    (0..4).map(|_| sender.try_send(b"held").unwrap()).collect()
}

fn rejection(sender: &ConnectionSender) -> Error {
    sender
        .try_send(b"rejected")
        .err()
        .expect("terminal/full sender admitted a frame")
}

fn assert_failed(tickets: Vec<SendTicket>, expected: Error) {
    for ticket in tickets {
        assert_eq!(
            outcome(ticket),
            SendOutcome::Failed {
                error: expected,
                written_bytes: 0
            }
        );
    }
}

#[test]
fn closed_sender_with_uncollected_tickets_reports_closed() {
    let (transport, _, script, connection) = fixture(vec![]);
    let sender = connection.sender();
    let tickets = full_queue(&sender);
    drop(connection);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 1);
    assert_eq!(rejection(&sender), ErrorKind::Closed.into());
    assert_eq!(transport.diagnostics().backpressured, 0);
    assert!(script.lock().unwrap().output.is_empty());
    assert_failed(tickets, ErrorKind::Closed.into());
    drop(sender);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn cancelled_sender_with_full_slots_reports_cancelled_before_progress() {
    let (transport, _, script, mut connection) = fixture(vec![]);
    let sender = connection.sender();
    let tickets = full_queue(&sender);
    sender.cancel();
    assert_eq!(connection.phase(), ConnectionPhase::Open);
    assert_eq!(rejection(&sender), ErrorKind::Cancelled.into());
    assert_eq!(connection.stats().queue_slots[1], 4);
    assert_eq!(transport.diagnostics().backpressured, 0);
    assert_eq!(script.lock().unwrap().write_calls, 0);
    advance(&mut connection);
    assert_failed(tickets, ErrorKind::Cancelled.into());
    drop(connection);
    drop(sender);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

struct NoEvents;
impl ReadinessWait for NoEvents {
    fn wait(&mut self, _: &[Interest], _: Duration, _: &mut [ReadinessEvent]) -> io::Result<usize> {
        Ok(0)
    }
}

#[test]
fn driver_cancelled_sender_with_full_slots_reports_cancelled_before_step() {
    let (transport, _, script, connection) = fixture(vec![]);
    let sender = connection.sender();
    let tickets = full_queue(&sender);
    let mut driver = Driver::new(transport.clone(), NoEvents).unwrap();
    let id = driver.add(connection).unwrap();
    driver.cancellation().cancel();
    assert_eq!(
        driver.connection(id).unwrap().phase(),
        ConnectionPhase::Open
    );
    assert_eq!(rejection(&sender), ErrorKind::Cancelled.into());
    assert_eq!(transport.diagnostics().backpressured, 0);
    assert_eq!(script.lock().unwrap().write_calls, 0);
    driver.step(Duration::ZERO).unwrap();
    assert_failed(tickets, ErrorKind::Cancelled.into());
    drop(driver);
    drop(sender);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn draining_sender_with_full_slots_reports_closed_and_preserves_accepted_tx() {
    let (transport, _, _, mut connection) = fixture(vec![]);
    let sender = connection.sender();
    let tickets = full_queue(&sender);
    connection.begin_drain();
    assert_eq!(connection.phase(), ConnectionPhase::Draining);
    assert_eq!(rejection(&sender), ErrorKind::Closed.into());
    assert_eq!(transport.diagnostics().backpressured, 0);
    assert_eq!(connection.stats().queue_slots[1], 4);
    advance(&mut connection);
    for ticket in tickets {
        assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
    }
    assert_eq!(connection.stats().flushed_frames, 4);
    drop(connection);
    drop(sender);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn failed_sender_with_terminal_tickets_preserves_io_cause() {
    let (transport, _, script, mut connection) = fixture(vec![]);
    let sender = connection.sender();
    let tickets = full_queue(&sender);
    script.lock().unwrap().writes.push_back(Action::Error);
    advance(&mut connection);
    let expected = Error {
        kind: ErrorKind::Io,
        io_kind: Some(io::ErrorKind::ConnectionReset),
    };
    assert_eq!(connection.terminal_error(), Some(expected));
    assert_eq!(rejection(&sender), expected);
    assert_eq!(transport.diagnostics().backpressured, 0);
    assert_failed(tickets, expected);
    drop(connection);
    drop(sender);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn open_sender_with_full_slots_remains_backpressured_and_recovers() {
    let (transport, clock, _, mut connection) = fixture(vec![]);
    let sender = connection.sender();
    let mut tickets = full_queue(&sender);
    assert_eq!(rejection(&sender), ErrorKind::Backpressured.into());
    assert_eq!(transport.diagnostics().backpressured, 1);
    advance(&mut connection);
    assert_eq!(outcome(tickets.pop().unwrap()), SendOutcome::FlushedLocally);
    tickets.push(sender.try_send(b"next").unwrap());
    clock.set(20_000_000);
    advance(&mut connection);
    for ticket in tickets {
        assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
    }
    drop(connection);
    drop(sender);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[cfg(feature = "test-support")]
#[test]
fn preparation_failures_preserve_terminal_errors_and_return_every_charge() {
    use transport::test_support::FailurePoint;

    for point in [
        FailurePoint::SendSlot,
        FailurePoint::LocalBytes,
        FailurePoint::GlobalBytes,
        FailurePoint::Allocation,
    ] {
        for terminal in [ErrorKind::Closed, ErrorKind::Cancelled, ErrorKind::Io] {
            let (transport, _, script, connection) = fixture(vec![]);
            let sender = connection.sender();
            let ticket = sender.try_send(b"held").unwrap();
            let mut owner = Some(connection);
            let expected = match terminal {
                ErrorKind::Cancelled => {
                    sender.cancel();
                    advance(owner.as_mut().unwrap());
                    terminal.into()
                }
                ErrorKind::Io => {
                    script.lock().unwrap().writes.push_back(Action::Error);
                    advance(owner.as_mut().unwrap());
                    Error {
                        kind: terminal,
                        io_kind: Some(io::ErrorKind::ConnectionReset),
                    }
                }
                _ => {
                    drop(owner.take());
                    terminal.into()
                }
            };
            transport.fail_next(point);
            assert_eq!(rejection(&sender), expected, "{point:?}/{terminal:?}");
            assert_eq!(transport.payload_bytes(), 0);
            assert_eq!(transport.diagnostics().backpressured, 0);
            assert_failed(vec![ticket], expected);
            drop(owner);
            drop(sender);
            assert_eq!(transport.connection_count(), 0);
        }
    }
}
