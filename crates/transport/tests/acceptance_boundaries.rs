mod support;
use foundation::network::{Ready, Registration};
use std::sync::{Arc, Mutex};
use support::{Action, Script, Stream, TestClock, advance, fixture, frame, outcome};
use transport::{
    ConnectionPhase, ErrorKind, MAX_WIRE_BYTES, PROCESS_PAYLOAD_BYTES, SendOutcome, Transport,
};

#[test]
fn global_payload_exact_limit_and_one_byte_over_are_distinguished() {
    let transport = Transport::new(Arc::new(TestClock::default()));
    let connections = (0..17)
        .map(|slot| {
            transport
                .accept(Box::new(Stream(
                    Arc::new(Mutex::new(Script::default())),
                    Registration {
                        slot,
                        generation: 1,
                    },
                )))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let payload = vec![0x39; MAX_WIRE_BYTES - 42];
    let full_frames = PROCESS_PAYLOAD_BYTES as usize / MAX_WIRE_BYTES;
    let mut tickets = Vec::new();
    for i in 0..full_frames {
        tickets.push(connections[i / 4].sender().try_send(&payload).unwrap());
    }
    let remaining = PROCESS_PAYLOAD_BYTES as usize - full_frames * MAX_WIRE_BYTES;
    assert!(remaining >= 42);
    let last = connections[full_frames / 4].sender();
    let before = transport.payload_bytes();
    assert_eq!(
        last.try_send(&vec![0x56; remaining - 42 + 1])
            .err()
            .expect("COMM_MUTANT quota-plus-one: global limit plus one was admitted")
            .kind,
        ErrorKind::Backpressured
    );
    assert_eq!(transport.payload_bytes(), before);
    tickets.push(last.try_send(&vec![0x56; remaining - 42]).unwrap());
    assert_eq!(transport.payload_bytes(), PROCESS_PAYLOAD_BYTES);
    assert_eq!(
        transport.diagnostics().peak_payload_bytes,
        PROCESS_PAYLOAD_BYTES
    );
    assert_eq!(
        connections[16].sender().try_send(&[]).err().unwrap().kind,
        ErrorKind::Backpressured
    );
    drop(last);
    drop(tickets);
    drop(connections);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn trickle_progress_never_extends_the_first_byte_deadline() {
    let (_, clock, script, mut connection) = fixture(frame(&[1, 2]));
    script
        .lock()
        .unwrap()
        .reads
        .extend([Action::Limit(1), Action::Block]);
    advance(&mut connection);
    clock.set(29_900_000_000);
    script
        .lock()
        .unwrap()
        .reads
        .extend([Action::Limit(1), Action::Block]);
    advance(&mut connection);
    assert_eq!(connection.stats().read_bytes, 2);
    clock.set(30_000_000_000);
    advance(&mut connection);
    assert_eq!(
        connection
            .terminal_error()
            .expect("COMM_MUTANT sliding-deadline: trickle extended absolute deadline")
            .kind,
        ErrorKind::FrameTimedOut
    );
    assert_eq!(connection.stats().read_bytes, 2);
    assert!(connection.take_received().is_none());
}

#[test]
fn paused_receive_retains_hangup_until_leases_return_and_buffered_frame_is_delivered() {
    let (_, _, script, mut connection) =
        fixture([frame(&[]), frame(&[0]), frame(&[0, 1])].concat());
    advance(&mut connection);
    let first = connection.take_received().unwrap();
    let second = connection.take_received().unwrap();
    script.lock().unwrap().eof = true;
    let calls = script.lock().unwrap().read_calls;
    connection.advance(
        connection.registration(),
        Ready {
            hangup: true,
            ..Ready::default()
        },
    );
    assert_eq!(
        connection.phase(),
        ConnectionPhase::Open,
        "COMM_MUTANT hup-is-eof: paused HUP closed buffered input"
    );
    assert!(!connection.peer_write_closed());
    assert_eq!(script.lock().unwrap().read_calls, calls);
    drop(first);
    connection.advance(connection.registration(), Ready::default());
    let third = connection.take_received().unwrap();
    assert_eq!(third.payload(), [0, 1]);
    assert!(!connection.peer_write_closed());
    drop(second);
    drop(third);
    connection.advance(connection.registration(), Ready::default());
    assert!(connection.peer_write_closed());
    assert_eq!(connection.phase(), ConnectionPhase::Closed);
    assert_eq!(connection.terminal_error().unwrap().kind, ErrorKind::Closed);
}

#[test]
fn cancellation_after_partial_tx_never_sends_the_next_accepted_frame() {
    let (transport, clock, script, mut connection) = fixture(vec![]);
    let first = connection.sender().try_send(&[0]).unwrap();
    let second = connection.sender().try_send(&[0, 1]).unwrap();
    script
        .lock()
        .unwrap()
        .writes
        .extend([Action::Limit(3), Action::Block]);
    advance(&mut connection);
    connection.cancellation().cancel();
    clock.set(20_000_000);
    advance(&mut connection);
    assert_eq!(
        script.lock().unwrap().output,
        frame(&[0])[..3],
        "COMM_MUTANT cancel-next-frame: writes continued after cancellation"
    );
    assert!(
        matches!(outcome(first), SendOutcome::Failed { error, written_bytes: 3 } if error.kind == ErrorKind::Cancelled)
    );
    assert!(
        matches!(outcome(second), SendOutcome::Failed { error, written_bytes: 0 } if error.kind == ErrorKind::Cancelled)
    );
    assert_eq!(transport.payload_bytes(), 0);
}

#[test]
fn poisoned_fixture_cleanup_preserves_the_original_failure_without_resuming_io() {
    use foundation::network::ByteStream;
    let script = Arc::new(Mutex::new(Script::default()));
    let mut stream = Stream(
        script.clone(),
        Registration {
            slot: 0,
            generation: 1,
        },
    );
    let poisoned = std::panic::catch_unwind(|| {
        let _guard = script.lock().unwrap();
        panic!("intentional isolated fixture poison");
    });
    assert!(poisoned.is_err());
    stream.close();
    let ledger = match script.lock() {
        Err(poisoned) => poisoned.into_inner(),
        Ok(_) => panic!("fixture should remain poisoned"),
    };
    assert_eq!(ledger.closed, 1);
    assert_eq!((ledger.read_calls, ledger.write_calls), (0, 0));
}
