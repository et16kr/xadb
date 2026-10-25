mod support;
use foundation::network::{
    ByteStream, Interest, Listener, ReadinessEvent, ReadinessWait, Ready, Registration,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use support::*;
use transport::{Driver, ErrorKind, SendOutcome, Transport};
#[derive(Default)]
struct WaitLog {
    calls: usize,
    interests: Vec<Interest>,
    timeout: Duration,
    invalid: bool,
    fail: bool,
}
struct Wait(Arc<Mutex<WaitLog>>);
impl ReadinessWait for Wait {
    fn wait(
        &mut self,
        input: &[Interest],
        timeout: Duration,
        out: &mut [ReadinessEvent],
    ) -> io::Result<usize> {
        let mut log = self.0.lock().unwrap();
        log.calls += 1;
        log.interests = input.to_vec();
        log.timeout = timeout;
        if log.fail {
            return Err(io::ErrorKind::ConnectionReset.into());
        }
        for (i, interest) in input.iter().enumerate() {
            out[i] = ReadinessEvent {
                registration: interest.registration,
                ready: Ready {
                    readable: interest.readable,
                    writable: interest.writable,
                    invalid: log.invalid,
                    ..Ready::default()
                },
            };
        }
        Ok(input.len())
    }
}
struct FloodListener {
    index: u32,
    log: Arc<Mutex<Vec<u32>>>,
    serial: u64,
    block: bool,
}
impl Listener for FloodListener {
    fn registration(&self) -> Registration {
        Registration {
            slot: self.index,
            generation: 1,
        }
    }
    fn try_accept(&mut self) -> io::Result<Box<dyn ByteStream>> {
        self.log.lock().unwrap().push(self.index);
        if self.block {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.serial += 1;
        Ok(Box::new(Stream(
            Arc::new(Mutex::new(Script::default())),
            Registration {
                slot: self.index + 3,
                generation: self.serial,
            },
        )))
    }
    fn close(&mut self) {}
}
#[test]
fn listener_fairness_eight_attempt_budget_full_pause_and_generation_rejection() {
    let clock = Arc::new(TestClock::default());
    let t = Transport::new(clock);
    let log = Arc::new(Mutex::new(WaitLog::default()));
    let accepts = Arc::new(Mutex::new(vec![]));
    let mut d = Driver::new(t.clone(), Wait(log.clone())).unwrap();
    for index in 0..3 {
        d.add_listener(Box::new(FloodListener {
            index,
            log: accepts.clone(),
            serial: 0,
            block: false,
        }))
        .unwrap();
    }
    for _ in 0..16 {
        d.step(Duration::ZERO).unwrap();
    }
    let observed = accepts.lock().unwrap().clone();
    assert_eq!(observed.len(), 128);
    for (i, id) in observed.iter().enumerate() {
        assert_eq!(*id, i as u32 % 3);
    }
    d.step(Duration::ZERO).unwrap();
    assert_eq!(accepts.lock().unwrap().len(), 128);
    assert!(
        log.lock()
            .unwrap()
            .interests
            .iter()
            .all(|i| i.registration.slot >= 3)
    );
    let id = d.connections().next().unwrap();
    d.connection_mut(id).unwrap().cancel();
    d.step(Duration::ZERO).unwrap();
    assert!(d.connection(id).is_none());
    assert_eq!(t.connection_count(), 128);
    drop(d);
    assert_eq!(t.connection_count(), 0);
}
#[test]
fn listener_wouldblock_backoff_empty_wait_and_invalid_registration_close() {
    let clock = Arc::new(TestClock::default());
    let t = Transport::new(clock.clone());
    let log = Arc::new(Mutex::new(WaitLog::default()));
    let accepts = Arc::new(Mutex::new(vec![]));
    let mut d = Driver::new(t, Wait(log.clone())).unwrap();
    d.add_listener(Box::new(FloodListener {
        index: 0,
        log: accepts.clone(),
        serial: 0,
        block: true,
    }))
    .unwrap();
    d.step(Duration::from_secs(1)).unwrap();
    d.step(Duration::from_secs(1)).unwrap();
    assert_eq!(accepts.lock().unwrap().len(), 1);
    assert!(log.lock().unwrap().interests.is_empty());
    assert_eq!(log.lock().unwrap().timeout, Duration::from_millis(20));
    clock.set(20_000_000);
    d.step(Duration::ZERO).unwrap();
    assert_eq!(accepts.lock().unwrap().len(), 2);
    clock.set(40_000_000);
    log.lock().unwrap().invalid = true;
    assert_eq!(
        d.step(Duration::ZERO).unwrap_err().kind,
        ErrorKind::AdapterViolation
    );
    assert_eq!(accepts.lock().unwrap().len(), 2);
}
#[test]
fn wait_error_keeps_original_cause_and_clock_domains_cannot_mix() {
    let (t, _, _, c) = fixture(vec![]);
    let ticket = c.sender().try_send(&[]).unwrap();
    let log = Arc::new(Mutex::new(WaitLog {
        fail: true,
        ..Default::default()
    }));
    let mut d = Driver::new(t, Wait(log)).unwrap();
    d.add(c).unwrap();
    assert_eq!(
        d.step(Duration::ZERO).unwrap_err().io_kind,
        Some(io::ErrorKind::ConnectionReset)
    );
    assert!(
        matches!(outcome(ticket),SendOutcome::Failed{error,written_bytes:0} if error.kind==ErrorKind::Io&&error.io_kind==Some(io::ErrorKind::ConnectionReset))
    );
    let (_, _, _, c) = fixture(vec![]);
    assert_eq!(d.add(c).unwrap_err().kind, ErrorKind::Closed);
    let (t, _, _, _) = fixture(vec![]);
    let (_, _, _, foreign) = fixture(vec![]);
    let mut d = Driver::new(t, Wait(Arc::new(Mutex::new(WaitLog::default())))).unwrap();
    assert_eq!(
        d.add(foreign).unwrap_err().kind,
        ErrorKind::AdapterViolation
    );
}
#[test]
fn permanently_ready_connections_each_receive_bounded_turn() {
    let clock = Arc::new(TestClock::default());
    let t = Transport::new(clock);
    let log = Arc::new(Mutex::new(WaitLog::default()));
    let mut d = Driver::new(t.clone(), Wait(log)).unwrap();
    let mut scripts = vec![];
    let mut tickets = vec![];
    for slot in 0..16 {
        let s = Arc::new(Mutex::new(Script::default()));
        let c = t
            .accept(Box::new(Stream(
                s.clone(),
                Registration {
                    slot,
                    generation: 1,
                },
            )))
            .unwrap();
        tickets.push(c.sender().try_send(&vec![0; 1_048_576]).unwrap());
        d.add(c).unwrap();
        scripts.push(s);
    }
    d.step(Duration::ZERO).unwrap();
    for s in scripts {
        assert_eq!(s.lock().unwrap().output.len(), 65_536);
        assert_eq!(s.lock().unwrap().write_calls, 1);
    }
    drop(d);
    for ticket in tickets {
        assert!(matches!(
            outcome(ticket),
            SendOutcome::Failed {
                written_bytes: 65536,
                ..
            }
        ));
    }
    assert_eq!(t.payload_bytes(), 0);
}

#[test]
fn retained_hup_never_doubles_per_step_read_budget() {
    let (t, clock, s, mut c) = fixture(frame(&vec![7; 1_048_576]));
    // Save HUP while input is temporarily unavailable, then allow buffered bytes.
    s.lock().unwrap().reads.push_back(Action::Block);
    c.advance(
        c.registration(),
        Ready {
            hangup: true,
            ..Ready::default()
        },
    );
    clock.set(20_000_000);
    let mut d = Driver::new(t, Wait(Arc::new(Mutex::new(WaitLog::default())))).unwrap();
    let id = d.add(c).unwrap();
    d.step(Duration::ZERO).unwrap();
    assert_eq!(d.connection(id).unwrap().stats().read_bytes, 65_536);
    d.step(Duration::ZERO).unwrap();
    assert_eq!(d.connection(id).unwrap().stats().read_bytes, 131_072);
}
