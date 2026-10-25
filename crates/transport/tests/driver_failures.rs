mod support;

use foundation::{
    network::{ByteStream, Interest, Listener, ReadinessEvent, ReadinessWait, Ready, Registration},
    runtime::Cancellation,
};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use support::{Action, Script, Stream, TestClock, fixture, frame, outcome};
use transport::{ConnectionPhase, Driver, ErrorKind, SendOutcome, Transport};

struct ReadyWait;
impl ReadinessWait for ReadyWait {
    fn wait(
        &mut self,
        interests: &[Interest],
        _: Duration,
        output: &mut [ReadinessEvent],
    ) -> io::Result<usize> {
        for (interest, event) in interests.iter().zip(output) {
            *event = ReadinessEvent {
                registration: interest.registration,
                ready: Ready {
                    readable: interest.readable,
                    writable: interest.writable,
                    ..Ready::default()
                },
            };
        }
        Ok(interests.len())
    }
}

#[test]
fn driver_cancel_during_partial_write_stops_before_next_write() {
    let (transport, _, script, connection) = fixture(vec![]);
    let first = connection.sender().try_send(&[]).unwrap();
    let queued = connection.sender().try_send(&[1]).unwrap();
    let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
    let id = driver.add(connection).unwrap();
    {
        let mut script = script.lock().unwrap();
        script.writes.push_back(Action::Limit(3));
        script.cancel_after_write = Some(driver.cancellation());
    }
    driver.step(Duration::ZERO).unwrap();
    assert_eq!(script.lock().unwrap().write_calls, 1);
    assert_eq!(script.lock().unwrap().output, frame(&[])[..3]);
    assert_eq!(
        driver.connection(id).unwrap().phase(),
        ConnectionPhase::Closed
    );
    assert!(
        matches!(outcome(first), SendOutcome::Failed { error, written_bytes: 3 }
        if error.kind == ErrorKind::Cancelled)
    );
    assert!(
        matches!(outcome(queued), SendOutcome::Failed { error, written_bytes: 0 }
        if error.kind == ErrorKind::Cancelled)
    );
    driver.step(Duration::ZERO).unwrap();
    assert_eq!(script.lock().unwrap().write_calls, 1);
    drop(driver);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn driver_cancel_rejects_preexisting_sender_before_next_step() {
    let (transport, _, script, connection) = fixture(vec![]);
    let sender = connection.sender();
    let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
    driver.add(connection).unwrap();
    driver.cancellation().cancel();
    let error = match sender.try_send(&[]) {
        Err(error) => error,
        Ok(_) => panic!("driver cancellation admitted another frame"),
    };
    assert_eq!(error.kind, ErrorKind::Cancelled);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(script.lock().unwrap().write_calls, 0);
    driver.step(Duration::ZERO).unwrap();
    drop(driver);
    drop(sender);
    assert_eq!(transport.connection_count(), 0);
}

struct CancelAfterRead {
    stream: Stream,
    cancellation: Cancellation,
}
impl ByteStream for CancelAfterRead {
    fn registration(&self) -> Registration {
        self.stream.registration()
    }
    fn try_read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let result = self.stream.try_read(output);
        self.cancellation.cancel();
        result
    }
    fn try_write(&mut self, input: &[u8]) -> io::Result<usize> {
        self.stream.try_write(input)
    }
    fn finish_connect(&mut self) -> io::Result<()> {
        self.stream.finish_connect()
    }
    fn take_error(&mut self) -> io::Result<Option<io::Error>> {
        self.stream.take_error()
    }
    fn close(&mut self) {
        self.stream.close();
    }
}

#[test]
fn driver_cancel_during_read_prevents_delivery_and_write() {
    let transport = Transport::new(Arc::new(TestClock::default()));
    let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
    let script = Arc::new(Mutex::new(Script {
        input: frame(&[1, 2]).into(),
        ..Script::default()
    }));
    let connection = transport
        .accept(Box::new(CancelAfterRead {
            stream: Stream(
                script.clone(),
                Registration {
                    slot: 1,
                    generation: 1,
                },
            ),
            cancellation: driver.cancellation(),
        }))
        .unwrap();
    let ticket = connection.sender().try_send(&[3]).unwrap();
    let id = driver.add(connection).unwrap();
    driver.step(Duration::ZERO).unwrap();
    assert_eq!(script.lock().unwrap().read_calls, 1);
    assert_eq!(script.lock().unwrap().write_calls, 0);
    let connection = driver.connection_mut(id).unwrap();
    assert!(connection.take_received().is_none());
    assert_eq!(connection.stats().read_bytes, 10);
    assert!(
        matches!(outcome(ticket), SendOutcome::Failed { error, written_bytes: 0 }
        if error.kind == ErrorKind::Cancelled)
    );
    drop(driver);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

struct FailingListener {
    error: io::ErrorKind,
    calls: Arc<Mutex<usize>>,
    closed: Arc<Mutex<usize>>,
}
impl Listener for FailingListener {
    fn registration(&self) -> Registration {
        Registration {
            slot: 2,
            generation: 1,
        }
    }
    fn try_accept(&mut self) -> io::Result<Box<dyn ByteStream>> {
        *self.calls.lock().unwrap() += 1;
        Err(self.error.into())
    }
    fn close(&mut self) {
        *self.closed.lock().unwrap() += 1;
    }
}

#[test]
fn aborted_accept_preserves_healthy_connection_and_backs_off_listener() {
    for error in [
        io::ErrorKind::ConnectionAborted,
        io::ErrorKind::ConnectionReset,
    ] {
        let (transport, clock, script, connection) = fixture(vec![]);
        let sender = connection.sender();
        let ticket = sender.try_send(&[7]).unwrap();
        script
            .lock()
            .unwrap()
            .writes
            .extend([Action::Limit(3), Action::Block]);
        let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
        let id = driver.add(connection).unwrap();
        let calls = Arc::new(Mutex::new(0));
        let closed = Arc::new(Mutex::new(0));
        driver
            .add_listener(Box::new(FailingListener {
                error,
                calls: calls.clone(),
                closed: closed.clone(),
            }))
            .unwrap();
        driver.step(Duration::ZERO).unwrap();
        let ticket = ticket
            .try_take()
            .expect_err("healthy partial TX must remain pending");
        assert_eq!(
            driver.connection(id).unwrap().phase(),
            ConnectionPhase::Open
        );
        assert_eq!(*closed.lock().unwrap(), 0);
        assert_eq!(*calls.lock().unwrap(), 1);
        driver.step(Duration::ZERO).unwrap();
        assert_eq!(*calls.lock().unwrap(), 1);
        clock.set(20_000_000);
        let second = sender.try_send(&[8]).unwrap();
        driver.step(Duration::ZERO).unwrap();
        assert_eq!(*calls.lock().unwrap(), 2);
        assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
        assert_eq!(outcome(second), SendOutcome::FlushedLocally);
        assert_eq!(
            script.lock().unwrap().output,
            [frame(&[7]), frame(&[8])].concat()
        );
        drop(driver);
        drop(sender);
        assert_eq!(*closed.lock().unwrap(), 1);
        assert_eq!(transport.payload_bytes(), 0);
        assert_eq!(transport.connection_count(), 0);
    }
}

#[test]
fn connection_cancel_keeps_driver_and_sibling_usable() {
    let (transport, _, _, connection) = fixture(vec![]);
    let cancel = connection.cancellation();
    let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
    driver.add(connection).unwrap();
    let sibling = transport
        .accept(Box::new(Stream(
            Arc::new(Mutex::new(Script::default())),
            Registration {
                slot: 3,
                generation: 1,
            },
        )))
        .unwrap();
    let ticket = sibling.sender().try_send(&[1]).unwrap();
    let id = driver.add(sibling).unwrap();
    cancel.cancel();
    driver.step(Duration::ZERO).unwrap();
    assert!(driver.cancellation().check().is_ok());
    assert_eq!(
        driver.connection(id).unwrap().phase(),
        ConnectionPhase::Open
    );
    assert_eq!(outcome(ticket), SendOutcome::FlushedLocally);
    drop(driver);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

struct PausedWrite {
    stream: Stream,
    reached: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<()>,
}
impl ByteStream for PausedWrite {
    fn registration(&self) -> Registration {
        self.stream.registration()
    }
    fn try_read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.stream.try_read(output)
    }
    fn try_write(&mut self, input: &[u8]) -> io::Result<usize> {
        let result = self.stream.try_write(input);
        // Test-only scheduling gate: cancellation must precede the return from
        // the first successful partial I/O. Production ports never wait here.
        self.reached.send(()).unwrap();
        self.resume.recv_timeout(Duration::from_secs(5)).unwrap();
        result
    }
    fn finish_connect(&mut self) -> io::Result<()> {
        self.stream.finish_connect()
    }
    fn take_error(&mut self) -> io::Result<Option<io::Error>> {
        self.stream.take_error()
    }
    fn close(&mut self) {
        self.stream.close();
    }
}

#[test]
fn concurrent_driver_cancel_preserves_only_the_completed_write_prefix() {
    let transport = Transport::new(Arc::new(TestClock::default()));
    let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
    let script = Arc::new(Mutex::new(Script {
        writes: [Action::Limit(3)].into(),
        ..Script::default()
    }));
    let (reached, progress) = std::sync::mpsc::channel();
    let (resume, release) = std::sync::mpsc::channel();
    let connection = transport
        .accept(Box::new(PausedWrite {
            stream: Stream(
                script.clone(),
                Registration {
                    slot: 1,
                    generation: 1,
                },
            ),
            reached,
            resume: release,
        }))
        .unwrap();
    let ticket = connection.sender().try_send(&[1]).unwrap();
    driver.add(connection).unwrap();
    let cancellation = driver.cancellation();
    let canceller = std::thread::spawn(move || {
        progress.recv_timeout(Duration::from_secs(5)).unwrap();
        cancellation.cancel();
        resume.send(()).unwrap();
    });
    driver.step(Duration::ZERO).unwrap();
    canceller.join().unwrap();
    assert_eq!(script.lock().unwrap().write_calls, 1);
    assert!(
        matches!(outcome(ticket), SendOutcome::Failed { error, written_bytes: 3 }
        if error.kind == ErrorKind::Cancelled)
    );
    drop(driver);
    assert_eq!(transport.payload_bytes(), 0);
    assert_eq!(transport.connection_count(), 0);
}

#[test]
fn fatal_accept_error_still_closes_driver_with_original_cause() {
    let (transport, _, _, connection) = fixture(vec![]);
    let sender = connection.sender();
    let mut driver = Driver::new(transport.clone(), ReadyWait).unwrap();
    let id = driver.add(connection).unwrap();
    let closed = Arc::new(Mutex::new(0));
    driver
        .add_listener(Box::new(FailingListener {
            error: io::ErrorKind::PermissionDenied,
            calls: Arc::new(Mutex::new(0)),
            closed: closed.clone(),
        }))
        .unwrap();
    let error = driver.step(Duration::ZERO).unwrap_err();
    assert_eq!(error.io_kind, Some(io::ErrorKind::PermissionDenied));
    assert_eq!(driver.connection(id).unwrap().terminal_error(), Some(error));
    assert_eq!(*closed.lock().unwrap(), 1);
    drop(driver);
    drop(sender);
    assert_eq!(transport.connection_count(), 0);
}
