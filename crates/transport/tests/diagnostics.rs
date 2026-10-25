mod support;
use foundation::network::Registration;
use std::sync::Arc;
use support::{Action, Script, Stream, TestClock, advance, frame, outcome};
use transport::{EventKind, Instrumentation, SendOutcome, Transport};

#[test]
fn diagnostics_are_bounded_ordered_and_modes_preserve_bytes_and_results() {
    for mode in [Instrumentation::Always, Instrumentation::Detailed] {
        let clock = Arc::new(TestClock::default());
        let transport = Transport::with_instrumentation(clock.clone(), mode);
        let script = Arc::new(std::sync::Mutex::new(Script::default()));
        let mut connection = transport
            .accept(Box::new(Stream(
                script.clone(),
                Registration {
                    slot: 7,
                    generation: 9,
                },
            )))
            .unwrap();
        let sender = connection.sender();
        let mut expected = Vec::new();
        for i in 0..300 {
            clock.set(i * 100_000_000);
            let bytes = [i as u8, (i / 256) as u8];
            let ticket = sender.try_send(&bytes).unwrap();
            script.lock().unwrap().input.extend(frame(&bytes));
            advance(&mut connection);
            assert_eq!(connection.take_received().unwrap().payload(), bytes);
            assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
            expected.extend(frame(&bytes));
        }
        assert_eq!(script.lock().unwrap().output, expected);
        connection.cancel();
        let stats = connection.stats();
        assert_eq!(stats.send_latency.buckets.iter().sum::<u64>(), 300);
        assert_eq!(stats.receive_latency.buckets.iter().sum::<u64>(), 300);
        assert_eq!(stats.copied_payload_bytes, 600);
        assert_eq!(stats.queue_bytes, [0, 0]);
        assert_eq!(stats.peak_queue_bytes, [44, 44]);
        assert_eq!(stats.peak_queue_slots, [1, 1]);
        let snapshot = transport.diagnostics();
        assert_eq!(snapshot.peak_payload_bytes, 88);
        if mode == Instrumentation::Detailed {
            assert_eq!(snapshot.events.iter().flatten().count(), 256);
            assert_eq!(snapshot.lost_events, 901 - 256);
            let events = snapshot.events.into_iter().flatten().collect::<Vec<_>>();
            assert!(events.windows(2).all(|e| e[0].at_nanos <= e[1].at_nanos));
            assert_eq!(events.last().unwrap().kind, EventKind::Closed);
        } else {
            assert!(snapshot.events.iter().all(Option::is_none));
            assert_eq!(snapshot.lost_events, 0);
        }
        drop(sender);
        drop(connection);
        assert_eq!(transport.payload_bytes(), 0);
        assert_eq!(transport.connection_count(), 0);
    }
}

#[test]
fn rx_pause_and_backpressure_are_observed_without_unbounded_queues() {
    let (transport, clock, script, mut connection) =
        support::fixture([frame(b"a"), frame(b"b"), frame(b"c")].concat());
    script.lock().unwrap().reads.push_back(Action::Limit(10));
    advance(&mut connection);
    // Two leases pin both receive slots; a further owner control sample starts pause.
    clock.set(20_000_000);
    advance(&mut connection);
    clock.set(40_000_000);
    advance(&mut connection);
    assert!(connection.stats().rx_paused_nanos >= 20_000_000);
    drop(connection.take_received());
    clock.set(60_000_000);
    advance(&mut connection);
    let sender = connection.sender();
    let tickets = (0..4)
        .map(|_| sender.try_send(b"").unwrap())
        .collect::<Vec<_>>();
    assert!(sender.try_send(b"").is_err());
    assert_eq!(transport.diagnostics().backpressured, 1);
    drop(tickets);
    drop(sender);
    drop(connection);
    assert_eq!(transport.payload_bytes(), 0);
}
