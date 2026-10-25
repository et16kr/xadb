#![allow(dead_code)]
use foundation::{
    network::{ByteStream, Ready, Registration},
    runtime::{Cancellation, Clock},
};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use transport::{Connection, SendOutcome, SendTicket, Transport};

#[derive(Default)]
pub struct TestClock(pub AtomicU64);
impl Clock for TestClock {
    fn now_nanos(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
impl TestClock {
    pub fn set(&self, nanos: u64) {
        self.0.store(nanos, Ordering::SeqCst);
    }
}
#[derive(Clone, Copy)]
pub enum Action {
    Limit(usize),
    Block,
    Interrupt,
    Zero,
    Overcount,
    Error,
}
#[derive(Default)]
pub struct Script {
    pub input: VecDeque<u8>,
    pub output: Vec<u8>,
    pub reads: VecDeque<Action>,
    pub writes: VecDeque<Action>,
    pub read_calls: usize,
    pub write_calls: usize,
    pub closed: usize,
    pub eof: bool,
    pub clock_after_read: Option<(Arc<TestClock>, u64)>,
    pub clock_after_write: Option<(Arc<TestClock>, u64)>,
    pub cancel_after_write: Option<Cancellation>,
    pub connecting: bool,
}
pub struct Stream(pub Arc<Mutex<Script>>, pub Registration);
impl ByteStream for Stream {
    fn registration(&self) -> Registration {
        self.1
    }
    fn try_read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let mut s = self.0.lock().unwrap();
        s.read_calls += 1;
        if let Some((clock, time)) = &s.clock_after_read {
            clock.set(*time);
        }
        let limit = match s.reads.pop_front().unwrap_or(Action::Limit(usize::MAX)) {
            Action::Limit(n) => n,
            Action::Block => return Err(io::ErrorKind::WouldBlock.into()),
            Action::Interrupt => return Err(io::ErrorKind::Interrupted.into()),
            Action::Zero => return Ok(0),
            Action::Overcount => return Ok(output.len() + 1),
            Action::Error => return Err(io::ErrorKind::ConnectionReset.into()),
        };
        let n = output.len().min(limit).min(s.input.len());
        if n == 0 && !s.eof {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        for b in &mut output[..n] {
            *b = s.input.pop_front().unwrap();
        }
        Ok(n)
    }
    fn try_write(&mut self, input: &[u8]) -> io::Result<usize> {
        let mut s = self.0.lock().unwrap();
        s.write_calls += 1;
        if let Some((clock, time)) = &s.clock_after_write {
            clock.set(*time);
        }
        if let Some(cancel) = &s.cancel_after_write {
            cancel.cancel();
        }
        let n = match s.writes.pop_front().unwrap_or(Action::Limit(usize::MAX)) {
            Action::Limit(n) => n.min(input.len()),
            Action::Block => return Err(io::ErrorKind::WouldBlock.into()),
            Action::Interrupt => return Err(io::ErrorKind::Interrupted.into()),
            Action::Zero => return Ok(0),
            Action::Overcount => return Ok(input.len() + 1),
            Action::Error => return Err(io::ErrorKind::ConnectionReset.into()),
        };
        s.output.extend_from_slice(&input[..n]);
        Ok(n)
    }
    fn finish_connect(&mut self) -> io::Result<()> {
        if self.0.lock().unwrap().connecting {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            Ok(())
        }
    }
    fn take_error(&mut self) -> io::Result<Option<io::Error>> {
        Ok(None)
    }
    fn close(&mut self) {
        // A failing assertion can poison the fixture while borrowing its I/O
        // ledger. Cleanup must preserve the original counterexample rather than
        // cause a second panic during Connection::drop. No I/O resumes here.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .closed += 1;
    }
}
pub fn frame(payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0x41, 0x58, 0x46, 0x31, 1, 0];
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    let digest = Sha256::digest(&bytes);
    bytes.extend(digest);
    bytes
}
pub fn fixture(input: Vec<u8>) -> (Transport, Arc<TestClock>, Arc<Mutex<Script>>, Connection) {
    let clock = Arc::new(TestClock::default());
    let transport = Transport::new(clock.clone());
    let script = Arc::new(Mutex::new(Script {
        input: input.into(),
        ..Script::default()
    }));
    let connection = transport
        .accept(Box::new(Stream(
            script.clone(),
            Registration {
                slot: 1,
                generation: 1,
            },
        )))
        .unwrap();
    (transport, clock, script, connection)
}
pub fn advance(connection: &mut Connection) {
    connection.advance(
        connection.registration(),
        Ready {
            readable: true,
            writable: true,
            ..Ready::default()
        },
    );
}
pub fn outcome(ticket: SendTicket) -> SendOutcome {
    match ticket.try_take() {
        Ok(outcome) => outcome,
        Err(_) => panic!("ticket remains pending"),
    }
}
