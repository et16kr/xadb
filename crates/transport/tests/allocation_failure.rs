#![cfg(feature = "test-support")]
mod support;
use support::{advance, fixture, frame, outcome};
use transport::{ConnectionPhase, ErrorKind, SendOutcome, test_support::FailurePoint};

#[test]
fn every_send_preparation_failure_returns_all_charges_without_admission() {
    for point in [
        FailurePoint::SendSlot,
        FailurePoint::LocalBytes,
        FailurePoint::GlobalBytes,
        FailurePoint::Allocation,
        FailurePoint::Encode,
        FailurePoint::Admission,
    ] {
        let (transport, clock, script, mut connection) = fixture(vec![]);
        let sender = connection.sender();
        transport.fail_next(point);
        let error = sender.try_send(b"prepared but rejected").err().unwrap();
        assert_eq!(
            error.kind,
            match point {
                FailurePoint::Encode => ErrorKind::InvalidFrame,
                FailurePoint::Admission => ErrorKind::Closed,
                _ => ErrorKind::Backpressured,
            }
        );
        assert_eq!(transport.payload_bytes(), 0);
        assert_eq!(connection.stats().queue_slots, [0, 0]);
        advance(&mut connection);
        assert!(script.lock().unwrap().output.is_empty());
        let ticket = sender.try_send(b"next").unwrap();
        assert_eq!(ticket.sequence(), 1);
        clock.set(20_000_000);
        advance(&mut connection);
        assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
        assert_eq!(script.lock().unwrap().output, frame(b"next"));
        drop(sender);
        drop(connection);
        assert_eq!(transport.connection_count(), 0);
        assert_eq!(transport.payload_bytes(), 0);
    }
}

#[test]
fn receive_reservation_and_allocation_failures_are_terminal_and_return_charges() {
    for point in [
        FailurePoint::ReceiveSlot,
        FailurePoint::LocalBytes,
        FailurePoint::GlobalBytes,
        FailurePoint::Allocation,
    ] {
        let (transport, _, _, mut connection) = fixture(frame(b"never delivered"));
        transport.fail_next(point);
        advance(&mut connection);
        assert_eq!(connection.phase(), ConnectionPhase::Closed);
        assert_eq!(
            connection.terminal_error().unwrap().kind,
            ErrorKind::ResourceExhausted
        );
        assert!(connection.take_received().is_none());
        assert_eq!(transport.payload_bytes(), 0);
        assert_eq!(connection.stats().queue_slots, [0, 0]);
        drop(connection);
        assert_eq!(transport.connection_count(), 0);
    }
}

#[test]
fn connection_reservation_failure_precedes_native_creation_and_is_domain_local() {
    use foundation::network::{ByteStream, Connector};
    struct NeverConnect;
    impl Connector for NeverConnect {
        fn connect(&mut self) -> std::io::Result<Box<dyn ByteStream>> {
            panic!("connect effect after failed reservation")
        }
    }
    let (transport, _, _, connection) = fixture(vec![]);
    let (other, _, _, other_connection) = fixture(vec![]);
    transport.fail_next(FailurePoint::ConnectionSlot);
    let ticket = other_connection.sender().try_send(b"independent").unwrap();
    assert_eq!(
        transport.connect(&mut NeverConnect).err().unwrap().kind,
        ErrorKind::ResourceExhausted
    );
    assert_eq!(transport.connection_count(), 1);
    drop(connection);
    drop(ticket);
    drop(other_connection);
    assert_eq!(transport.connection_count(), 0);
    assert_eq!(other.payload_bytes(), 0);
}
