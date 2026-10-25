//! Independent value-state oracle. It imports no production state transitions.
//! The peer stream is the fixed ordered empty/00/0001 AXF1 sequence. Consumed
//! output/counter history is checked then discarded: it cannot affect a future
//! transition. Connection identity, timers, queued leases/tickets and sequence
//! allocation remain in the canonical state; connections are never permuted.
#![allow(dead_code)]
use crate::support::{self, Action as IoAction, Script, Stream, TestClock};
use foundation::network::{Ready, Registration};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use transport::{
    Connection, ConnectionPhase, ErrorKind, ReceivedFrame, SendOutcome, SendTicket, Transport,
};

pub const TICK_NANOS: u64 = 20_000_000;
const FRAME_TICKS: u64 = 1500;
const IDLE_TICKS: u64 = 3000;
const DRAIN_TICKS: u64 = 250;
pub fn payload(code: u8) -> &'static [u8] {
    match code {
        0 => &[],
        1 => &[0],
        2 => &[0, 1],
        _ => panic!("invalid model payload"),
    }
}
fn wire(code: u8) -> Vec<u8> {
    support::frame(payload(code))
}
fn length(code: u8) -> usize {
    42 + payload(code).len()
}
fn boundary(offset: usize, code: u8) -> usize {
    [10, 10 + payload(code).len(), length(code)]
        .into_iter()
        .find(|&n| n > offset)
        .unwrap()
        - offset
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Operation {
    Enqueue(u8),
    ReadByte,
    ReadBoundary,
    WriteByte,
    WriteBoundary,
    ReadBlock,
    WriteBlock,
    Eof,
    Cancel,
    Drain,
    TakeReceive,
    ReleaseReceive(u8),
    CollectTicket(u8),
    DropTicket(u8),
    Deadline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Event {
    pub connection: u8,
    pub operation: Operation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Reason {
    Closed,
    Cancelled,
    Truncated,
    Frame,
    Send,
    Idle,
    Drain,
}
impl Reason {
    fn error(self) -> ErrorKind {
        match self {
            Self::Closed => ErrorKind::Closed,
            Self::Cancelled => ErrorKind::Cancelled,
            Self::Truncated => ErrorKind::TruncatedFrame,
            Self::Frame => ErrorKind::FrameTimedOut,
            Self::Send => ErrorKind::SendTimedOut,
            Self::Idle => ErrorKind::IdleTimedOut,
            Self::Drain => ErrorKind::DrainTimedOut,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ResultCell {
    Pending,
    Flushed,
    Failed(Reason),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Ticket {
    sequence: u64,
    code: u8,
    offset: usize,
    expires: u64,
    live: bool,
    result: ResultCell,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Peer {
    // 0=Open, 1=Draining, 2=Closed. No native Connecting in this reduced model.
    phase: u8,
    reason: Option<Reason>,
    cancelled: bool,
    eof_sent: bool,
    eof_observed: bool,
    rx_number: u8,
    rx_offset: usize,
    rx_expires: Option<u64>,
    queued: VecDeque<u8>,
    leases: Vec<u8>,
    tickets: [Option<Ticket>; 2],
    next_sequence: u64,
    idle_expires: u64,
    drain_expires: Option<u64>,
    retry_at: u64,
}
impl Default for Peer {
    fn default() -> Self {
        Self {
            phase: 0,
            reason: None,
            cancelled: false,
            eof_sent: false,
            eof_observed: false,
            rx_number: 0,
            rx_offset: 0,
            rx_expires: None,
            queued: VecDeque::new(),
            leases: Vec::new(),
            tickets: [None, None],
            next_sequence: 1,
            idle_expires: IDLE_TICKS,
            drain_expires: None,
            retry_at: 0,
        }
    }
}
impl Peer {
    fn rx_slots(&self) -> usize {
        self.queued.len() + self.leases.len() + usize::from(self.rx_offset >= 10)
    }
    fn can_read(&self) -> bool {
        self.phase == 0 && (self.rx_offset > 0 || self.rx_slots() < 2)
    }
    fn next_tx(&self) -> Option<usize> {
        self.tickets
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                slot.as_ref()
                    .filter(|t| t.result == ResultCell::Pending)
                    .map(|t| (i, t.sequence))
            })
            .min_by_key(|(_, sequence)| *sequence)
            .map(|(i, _)| i)
    }
    fn close(&mut self, reason: Reason) {
        if self.phase == 2 {
            return;
        }
        self.phase = 2;
        self.reason = Some(reason);
        self.rx_offset = 0;
        self.rx_expires = None;
        for entry in &mut self.tickets {
            if let Some(t) = entry {
                if t.result == ResultCell::Pending {
                    t.result = ResultCell::Failed(reason);
                }
                if !t.live {
                    *entry = None;
                }
            }
        }
    }
    fn drain(&mut self, now: u64) {
        if self.phase != 0 {
            return;
        }
        self.phase = 1;
        self.rx_offset = 0;
        self.rx_expires = None;
        self.drain_expires = Some(now + DRAIN_TICKS);
        if self.next_tx().is_none() {
            self.close(Reason::Closed);
        }
    }
    fn control(&mut self, now: u64) {
        if self.phase == 2 {
            return;
        }
        let reason = if self.cancelled {
            Some(Reason::Cancelled)
        } else if self.drain_expires.is_some_and(|t| now >= t) {
            Some(Reason::Drain)
        } else if self.rx_expires.is_some_and(|t| now >= t) {
            Some(Reason::Frame)
        } else if self
            .tickets
            .iter()
            .flatten()
            .any(|t| t.result == ResultCell::Pending && now >= t.expires)
        {
            Some(Reason::Send)
        } else if self.phase == 0 && now >= self.idle_expires {
            Some(Reason::Idle)
        } else {
            None
        };
        if let Some(reason) = reason {
            self.close(reason);
        }
    }
    fn next_deadline(&self) -> Option<u64> {
        if self.phase == 2 {
            return None;
        }
        self.drain_expires
            .into_iter()
            .chain(self.rx_expires)
            .chain((self.phase == 0).then_some(self.idle_expires))
            .chain(
                self.tickets
                    .iter()
                    .flatten()
                    .filter(|t| t.result == ResultCell::Pending)
                    .map(|t| t.expires),
            )
            .min()
    }
    fn charges(&self) -> ([u64; 2], [u64; 2]) {
        let rx_bytes = self
            .queued
            .iter()
            .chain(&self.leases)
            .map(|&c| length(c) as u64)
            .sum::<u64>()
            + if self.rx_offset >= 10 {
                length(self.rx_number) as u64
            } else {
                0
            };
        let tx_bytes = self
            .tickets
            .iter()
            .flatten()
            .filter(|t| t.result == ResultCell::Pending)
            .map(|t| length(t.code) as u64)
            .sum();
        (
            [rx_bytes, tx_bytes],
            [
                self.rx_slots() as u64,
                self.tickets.iter().flatten().count() as u64,
            ],
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Model {
    now: u64,
    peers: [Peer; 2],
}
#[derive(Debug, PartialEq, Eq)]
pub struct Observation {
    written: Vec<u8>,
    read: usize,
    send: Option<std::result::Result<u64, ErrorKind>>,
    received: Option<Option<u8>>,
    ticket: Option<SendOutcome>,
}
impl Model {
    /// Canonical future state. Time has no externally supplied epoch in this
    /// model: only remaining timer/retry delays affect subsequent operations.
    /// Closed sockets cannot consume peer input or use old timers/next sequence.
    /// Destructive endpoint observations still use the unmodified representative
    /// state and trace. No actor permutation or partial-order reduction is used.
    pub fn state_key(&self) -> Self {
        let mut key = self.clone();
        key.now = 0;
        for peer in &mut key.peers {
            if peer.phase == 2 {
                // Retain historical progress predicates used by witnesses.
                peer.rx_number = peer.rx_number.min(2);
                peer.eof_sent = false;
                peer.idle_expires = 0;
                peer.drain_expires = None;
                peer.retry_at = 0;
                peer.next_sequence = 0;
            } else {
                peer.idle_expires = if peer.phase == 0 {
                    peer.idle_expires.saturating_sub(self.now)
                } else {
                    0
                };
                peer.drain_expires = peer.drain_expires.map(|t| t.saturating_sub(self.now));
                peer.retry_at = peer.retry_at.saturating_sub(self.now);
            }
            peer.rx_expires = peer.rx_expires.map(|t| t.saturating_sub(self.now));
            for ticket in peer.tickets.iter_mut().flatten() {
                ticket.expires = if ticket.result == ResultCell::Pending {
                    ticket.expires.saturating_sub(self.now)
                } else {
                    0
                };
            }
        }
        key
    }
    /// Lossless encoding of the canonical depth-eight value state. Fixed-width
    /// keys avoid hundreds of millions of allocator-owned queue objects. Bounds
    /// are checked; this is not a digest or a probabilistic collision filter.
    pub fn packed_key(&self) -> [u8; 80] {
        let key = self.state_key();
        let mut bytes = [0; 80];
        let mut cursor = 0;
        fn byte(bytes: &mut [u8], cursor: &mut usize, n: u64) {
            bytes[*cursor] = u8::try_from(n).expect("depth-eight byte bound");
            *cursor += 1;
        }
        fn time(bytes: &mut [u8], cursor: &mut usize, n: Option<u64>) {
            let n = n
                .map(|n| u16::try_from(n).expect("depth-eight timer bound"))
                .unwrap_or(u16::MAX);
            bytes[*cursor..*cursor + 2].copy_from_slice(&n.to_le_bytes());
            *cursor += 2;
        }
        fn reason(reason: Option<Reason>) -> u64 {
            match reason {
                None => 0,
                Some(Reason::Closed) => 1,
                Some(Reason::Cancelled) => 2,
                Some(Reason::Truncated) => 3,
                Some(Reason::Frame) => 4,
                Some(Reason::Send) => 5,
                Some(Reason::Idle) => 6,
                Some(Reason::Drain) => 7,
            }
        }
        for peer in &key.peers {
            for n in [
                peer.phase as u64,
                reason(peer.reason),
                u64::from(peer.cancelled),
                u64::from(peer.eof_sent) | (u64::from(peer.eof_observed) << 1),
                peer.rx_number as u64,
                peer.rx_offset as u64,
                peer.next_sequence,
            ] {
                byte(&mut bytes, &mut cursor, n);
            }
            for n in [
                peer.rx_expires,
                Some(peer.idle_expires),
                peer.drain_expires,
                Some(peer.retry_at),
            ] {
                time(&mut bytes, &mut cursor, n);
            }
            for values in [
                peer.queued.iter().copied().collect::<Vec<_>>(),
                peer.leases.clone(),
            ] {
                byte(&mut bytes, &mut cursor, values.len() as u64);
                let packed = values
                    .iter()
                    .enumerate()
                    .fold(0u64, |n, (i, code)| n | ((*code as u64) << (i * 2)));
                byte(&mut bytes, &mut cursor, packed);
            }
            for slot in &peer.tickets {
                if let Some(t) = slot {
                    for n in [
                        t.code as u64 + 1,
                        t.sequence,
                        t.offset as u64,
                        u64::from(t.live),
                        match t.result {
                            ResultCell::Pending => 0,
                            ResultCell::Flushed => 1,
                            ResultCell::Failed(r) => 1 + reason(Some(r)),
                        },
                    ] {
                        byte(&mut bytes, &mut cursor, n);
                    }
                    time(&mut bytes, &mut cursor, Some(t.expires));
                } else {
                    cursor += 7;
                }
            }
        }
        assert_eq!(cursor, 66);
        bytes
    }
    pub fn enabled(&self) -> Vec<Event> {
        let mut events = Vec::new();
        for (i, peer) in self.peers.iter().enumerate() {
            let mut operations = vec![
                Operation::Enqueue(0),
                Operation::Enqueue(1),
                Operation::Enqueue(2),
                Operation::Cancel,
                Operation::Drain,
                Operation::TakeReceive,
            ];
            if peer.phase == 0 && !peer.eof_sent {
                operations.push(Operation::Eof);
            }
            if peer.can_read() {
                operations.push(Operation::ReadBlock);
                if peer.eof_sent || peer.rx_number < 3 {
                    operations.extend([Operation::ReadByte, Operation::ReadBoundary]);
                }
            }
            if peer.phase != 2 && peer.next_tx().is_some() {
                operations.extend([
                    Operation::WriteByte,
                    Operation::WriteBoundary,
                    Operation::WriteBlock,
                ]);
            }
            for index in 0..peer.leases.len() {
                operations.push(Operation::ReleaseReceive(index as u8));
            }
            for (index, ticket) in peer.tickets.iter().enumerate() {
                if let Some(t) = ticket
                    && t.live
                {
                    operations.push(Operation::DropTicket(index as u8));
                    if t.result != ResultCell::Pending {
                        operations.push(Operation::CollectTicket(index as u8));
                    }
                }
            }
            events.extend(operations.into_iter().map(|operation| Event {
                connection: i as u8,
                operation,
            }));
        }
        if self.peers.iter().any(|p| p.phase != 2) {
            events.push(Event {
                connection: 0,
                operation: Operation::Deadline,
            });
        }
        events
    }
    pub fn step(&mut self, event: Event) -> Observation {
        let mut seen = Observation {
            written: Vec::new(),
            read: 0,
            send: None,
            received: None,
            ticket: None,
        };
        if event.operation == Operation::Deadline {
            self.now = self.now.max(
                self.peers
                    .iter()
                    .filter_map(Peer::next_deadline)
                    .min()
                    .unwrap(),
            );
            for peer in &mut self.peers {
                peer.control(self.now);
            }
            return seen;
        }
        let io = matches!(
            event.operation,
            Operation::ReadByte
                | Operation::ReadBoundary
                | Operation::ReadBlock
                | Operation::WriteByte
                | Operation::WriteBoundary
                | Operation::WriteBlock
                | Operation::Eof
        );
        if io {
            self.now += 1;
        }
        let peer = &mut self.peers[event.connection as usize];
        if io {
            peer.control(self.now);
        }
        match event.operation {
            Operation::Enqueue(code) => {
                seen.send = Some(if peer.cancelled {
                    Err(ErrorKind::Cancelled)
                } else if peer.phase == 2 {
                    Err(peer.reason.unwrap().error())
                } else if peer.phase != 0 {
                    Err(ErrorKind::Closed)
                } else if peer.tickets.iter().all(Option::is_some) {
                    Err(ErrorKind::Backpressured)
                } else {
                    let index = peer.tickets.iter().position(Option::is_none).unwrap();
                    let sequence = peer.next_sequence;
                    peer.next_sequence += 1;
                    peer.tickets[index] = Some(Ticket {
                        sequence,
                        code,
                        offset: 0,
                        expires: self.now + FRAME_TICKS,
                        live: true,
                        result: ResultCell::Pending,
                    });
                    Ok(sequence)
                });
            }
            Operation::Cancel => {
                peer.cancelled = true;
                peer.close(Reason::Cancelled);
            }
            Operation::Drain => peer.drain(self.now),
            Operation::TakeReceive => {
                let code = peer.queued.pop_front();
                if let Some(code) = code {
                    peer.leases.push(code);
                }
                seen.received = Some(code);
            }
            Operation::ReleaseReceive(index) => {
                peer.leases.remove(index as usize);
            }
            Operation::CollectTicket(index) => {
                let t = peer.tickets[index as usize].take().unwrap();
                seen.ticket = Some(match t.result {
                    ResultCell::Flushed => SendOutcome::FlushedLocally,
                    ResultCell::Failed(reason) => SendOutcome::Failed {
                        error: reason.error().into(),
                        written_bytes: t.offset,
                    },
                    ResultCell::Pending => panic!("collect requires terminal result"),
                });
            }
            Operation::DropTicket(index) => {
                let slot = &mut peer.tickets[index as usize];
                let t = slot.as_mut().unwrap();
                t.live = false;
                if t.result != ResultCell::Pending {
                    *slot = None;
                }
            }
            Operation::Eof => {
                peer.eof_sent = true;
                if peer.can_read() {
                    if peer.rx_offset > 0 {
                        peer.close(Reason::Truncated);
                    } else {
                        peer.eof_observed = true;
                        peer.drain(self.now);
                    }
                }
            }
            Operation::ReadBlock => {
                if peer.can_read() {
                    peer.retry_at = self.now + 1;
                }
            }
            Operation::WriteBlock => {
                if peer.phase != 2 && peer.next_tx().is_some() {
                    peer.retry_at = self.now + 1;
                }
            }
            Operation::ReadByte | Operation::ReadBoundary => {
                if peer.can_read() {
                    if peer.eof_sent {
                        if peer.rx_offset > 0 {
                            peer.close(Reason::Truncated);
                        } else {
                            peer.eof_observed = true;
                            peer.drain(self.now);
                        }
                    } else {
                        let n = if event.operation == Operation::ReadByte {
                            1
                        } else {
                            boundary(peer.rx_offset, peer.rx_number)
                        };
                        if peer.rx_offset == 0 {
                            peer.rx_expires = Some(self.now + FRAME_TICKS);
                        }
                        peer.rx_offset += n;
                        seen.read = n;
                        if peer.rx_offset == length(peer.rx_number) {
                            peer.queued.push_back(peer.rx_number);
                            peer.rx_number += 1;
                            peer.rx_offset = 0;
                            peer.rx_expires = None;
                            peer.idle_expires = self.now + IDLE_TICKS;
                        }
                        if peer.can_read() {
                            peer.retry_at = self.now + 1;
                        }
                    }
                }
            }
            Operation::WriteByte | Operation::WriteBoundary => {
                if peer.phase != 2
                    && let Some(index) = peer.next_tx()
                {
                    let t = peer.tickets[index].as_mut().unwrap();
                    let n = if event.operation == Operation::WriteByte {
                        1
                    } else {
                        boundary(t.offset, t.code)
                    };
                    seen.written = wire(t.code)[t.offset..t.offset + n].to_vec();
                    t.offset += n;
                    if t.offset == length(t.code) {
                        t.result = ResultCell::Flushed;
                        if !t.live {
                            peer.tickets[index] = None;
                        }
                        peer.idle_expires = self.now + IDLE_TICKS;
                    }
                    if peer.next_tx().is_some() {
                        peer.retry_at = self.now + 1;
                    } else if peer.phase == 1 {
                        peer.close(Reason::Closed);
                    }
                }
            }
            Operation::Deadline => unreachable!(),
        }
        seen
    }
    pub fn witness(&self, before: &Self, event: Event) -> Vec<&'static str> {
        let mut names = Vec::new();
        let peer = &before.peers[event.connection as usize];
        match event.operation {
            Operation::ReleaseReceive(_) => names.push("rx_complete_take_release"),
            Operation::CollectTicket(i)
                if peer.tickets[i as usize].as_ref().unwrap().result == ResultCell::Flushed =>
            {
                names.push("tx_flush_collect")
            }
            Operation::Cancel => {
                if peer.rx_offset > 0 {
                    names.push("partial_rx_cancel");
                }
                if peer
                    .tickets
                    .iter()
                    .flatten()
                    .any(|t| t.offset > 0 && t.result == ResultCell::Pending)
                {
                    names.push("partial_tx_cancel");
                }
            }
            Operation::Enqueue(_)
                if peer.phase == 0
                    && !peer.cancelled
                    && peer.tickets.iter().all(Option::is_some) =>
            {
                names.push("full_queue_rejected")
            }
            _ => (),
        }
        if self.peers.iter().all(|p| p.rx_number > 0) {
            names.push("both_connections_progress");
        }
        // Initial stream is empty then 00; both must be delivered and taken in
        // their exact order. At depth eight this witness may retain both leases.
        if self
            .peers
            .iter()
            .any(|p| p.rx_number >= 2 && p.leases.as_slice() == [0, 1])
        {
            names.push("two_frame_order");
        }
        names
    }
    pub fn terminal(&self) -> bool {
        self.peers.iter().all(|p| p.phase == 2)
    }
}

pub struct Sut {
    transport: Transport,
    clock: Arc<TestClock>,
    connections: [Connection; 2],
    scripts: [Arc<Mutex<Script>>; 2],
    tickets: [[Option<SendTicket>; 2]; 2],
    leases: [Vec<ReceivedFrame>; 2],
}
impl Default for Sut {
    fn default() -> Self {
        let clock = Arc::new(TestClock::default());
        let transport = Transport::for_two_slot_model(clock.clone());
        let input = [wire(0), wire(1), wire(2)].concat();
        let scripts = std::array::from_fn(|_| {
            Arc::new(Mutex::new(Script {
                input: input.clone().into(),
                ..Script::default()
            }))
        });
        let connections = std::array::from_fn(|i| {
            transport
                .accept(Box::new(Stream(
                    scripts[i].clone(),
                    Registration {
                        slot: i as u32,
                        generation: 1,
                    },
                )))
                .unwrap()
        });
        Self {
            transport,
            clock,
            connections,
            scripts,
            tickets: std::array::from_fn(|_| [None, None]),
            leases: std::array::from_fn(|_| Vec::new()),
        }
    }
}
impl Sut {
    pub fn step(&mut self, event: Event, before: &Model, after: &Model, expected: Observation) {
        self.clock.set(after.now * TICK_NANOS);
        if event.operation == Operation::Deadline {
            for connection in &mut self.connections {
                connection.advance(connection.registration(), Ready::default());
            }
        } else {
            let i = event.connection as usize;
            let connection = &mut self.connections[i];
            let read_before = connection.stats().read_bytes;
            let mut ready = Ready::default();
            let mut advance = false;
            match event.operation {
                Operation::Enqueue(code) => match (
                    connection.sender().try_send(payload(code)),
                    expected.send.unwrap(),
                ) {
                    (Ok(ticket), Ok(sequence)) => {
                        assert_eq!(ticket.sequence(), sequence);
                        let index = after.peers[i]
                            .tickets
                            .iter()
                            .position(|t| t.as_ref().is_some_and(|t| t.sequence == sequence))
                            .unwrap();
                        assert!(self.tickets[i][index].is_none());
                        self.tickets[i][index] = Some(ticket);
                    }
                    (Err(actual), Err(error)) => assert_eq!(actual.kind, error),
                    (Ok(_), Err(error)) => panic!("admitted rejected enqueue {error:?}"),
                    (Err(actual), Ok(_)) => panic!("rejected valid enqueue: {actual:?}"),
                },
                Operation::Cancel => connection.cancel(),
                Operation::Drain => connection.begin_drain(),
                Operation::TakeReceive => {
                    match (connection.take_received(), expected.received.unwrap()) {
                        (Some(frame), Some(code)) => {
                            assert_eq!(frame.payload(), payload(code));
                            self.leases[i].push(frame);
                        }
                        (None, None) => (),
                        _ => panic!("RX delivery mismatch"),
                    }
                }
                Operation::ReleaseReceive(index) => {
                    self.leases[i].remove(index as usize);
                }
                Operation::DropTicket(index) => {
                    self.tickets[i][index as usize].take().unwrap();
                }
                Operation::CollectTicket(index) => {
                    let actual = self.tickets[i][index as usize]
                        .take()
                        .unwrap()
                        .try_take()
                        .unwrap_or_else(|_| panic!("expected terminal ticket remains pending"));
                    assert_eq!(actual, expected.ticket.unwrap());
                }
                Operation::Eof => {
                    let mut script = self.scripts[i].lock().unwrap();
                    script.input.clear();
                    script.eof = true;
                    script.reads.clear();
                    ready.readable = true;
                    advance = true;
                }
                Operation::ReadByte | Operation::ReadBoundary | Operation::ReadBlock => {
                    let mut script = self.scripts[i].lock().unwrap();
                    script.reads.clear();
                    if event.operation == Operation::ReadBlock {
                        script.reads.push_back(IoAction::Block);
                    } else {
                        let p = &before.peers[i];
                        let n = if p.eof_sent || event.operation == Operation::ReadByte {
                            1
                        } else {
                            boundary(p.rx_offset, p.rx_number)
                        };
                        script.reads.extend([IoAction::Limit(n), IoAction::Block]);
                    }
                    ready.readable = true;
                    advance = true;
                }
                Operation::WriteByte | Operation::WriteBoundary | Operation::WriteBlock => {
                    let mut script = self.scripts[i].lock().unwrap();
                    script.writes.clear();
                    if event.operation == Operation::WriteBlock {
                        script.writes.push_back(IoAction::Block);
                    } else {
                        let p = &before.peers[i];
                        let t = p.tickets[p.next_tx().unwrap()].as_ref().unwrap();
                        let n = if event.operation == Operation::WriteByte {
                            1
                        } else {
                            boundary(t.offset, t.code)
                        };
                        script.writes.extend([IoAction::Limit(n), IoAction::Block]);
                    }
                    ready.writable = true;
                    advance = true;
                }
                Operation::Deadline => unreachable!(),
            }
            if advance {
                connection.advance(connection.registration(), ready);
            }
            assert_eq!(
                connection.stats().read_bytes - read_before,
                expected.read as u64
            );
            let mut script = self.scripts[i].lock().unwrap();
            assert_eq!(script.output, expected.written);
            script.output.clear();
        }
        let mut bytes = 0;
        for (i, peer) in after.peers.iter().enumerate() {
            let connection = &self.connections[i];
            assert_eq!(
                connection.phase(),
                match peer.phase {
                    0 => ConnectionPhase::Open,
                    1 => ConnectionPhase::Draining,
                    2 => ConnectionPhase::Closed,
                    _ => unreachable!(),
                }
            );
            assert_eq!(
                connection.terminal_error().map(|e| e.kind),
                peer.reason.map(Reason::error)
            );
            assert_eq!(connection.peer_write_closed(), peer.eof_observed);
            let (expected_bytes, slots) = peer.charges();
            let stats = connection.stats();
            assert_eq!(stats.queue_bytes, expected_bytes);
            assert_eq!(stats.queue_slots, slots);
            bytes += expected_bytes.iter().sum::<u64>();
        }
        assert_eq!(self.transport.payload_bytes(), bytes);
        assert_eq!(self.transport.connection_count(), 2);
    }
    pub fn replay(trace: &[Event]) {
        let mut model = Model::default();
        let mut sut = Self::default();
        for &event in trace {
            let before = model.clone();
            let expected = model.step(event);
            sut.step(event, &before, &model, expected);
        }
        // Destructive endpoint observation happens on this fresh replay only.
        // It checks even a delivery/result produced by the eighth event; no
        // ninth model event is needed to expose a bad final queue/result cell.
        for (i, peer) in model.peers.iter().enumerate() {
            for &code in &peer.queued {
                let frame = sut.connections[i]
                    .take_received()
                    .expect("missing final queued frame");
                assert_eq!(frame.payload(), payload(code));
            }
            assert!(
                sut.connections[i].take_received().is_none(),
                "unexpected final queued frame"
            );
            for (j, slot) in peer.tickets.iter().enumerate() {
                if let Some(t) = slot
                    && t.live
                {
                    let actual = sut.tickets[i][j].take().unwrap().try_take();
                    match (actual, t.result) {
                        (Err(ticket), ResultCell::Pending) => sut.tickets[i][j] = Some(ticket),
                        (Ok(SendOutcome::FlushedLocally), ResultCell::Flushed) => (),
                        (
                            Ok(SendOutcome::Failed {
                                error,
                                written_bytes,
                            }),
                            ResultCell::Failed(reason),
                        ) => {
                            assert_eq!(error.kind, reason.error());
                            assert_eq!(written_bytes, t.offset);
                        }
                        _ => panic!("final ticket result differs from independent model"),
                    }
                }
            }
        }
        sut.finish();
    }
    pub fn finish(mut self) {
        for connection in &mut self.connections {
            connection.cancel();
        }
        self.tickets = std::array::from_fn(|_| [None, None]);
        self.leases = std::array::from_fn(|_| Vec::new());
        let transport = self.transport.clone();
        drop(self);
        assert_eq!(transport.payload_bytes(), 0);
        assert_eq!(transport.connection_count(), 0);
    }
}

pub const REQUIRED_WITNESSES: [&str; 7] = [
    "rx_complete_take_release",
    "tx_flush_collect",
    "partial_rx_cancel",
    "partial_tx_cancel",
    "full_queue_rejected",
    "both_connections_progress",
    "two_frame_order",
];

#[cfg(test)]
mod oracle_regression {
    use super::*;

    fn full(code: u8) -> Vec<Operation> {
        vec![Operation::Enqueue(code), Operation::Enqueue((code + 1) % 3)]
    }

    fn flushed(code: u8) -> Vec<Operation> {
        let mut operations = full(code);
        for payload in [code, (code + 1) % 3] {
            operations.extend(std::iter::repeat_n(
                Operation::WriteBoundary,
                if payload == 0 { 2 } else { 3 },
            ));
        }
        operations
    }

    fn rejected(
        connection: u8,
        code: u8,
        operations: Vec<Operation>,
        expected: ErrorKind,
        capacity_witness: bool,
    ) {
        let mut trace = operations
            .into_iter()
            .map(|operation| Event {
                connection,
                operation,
            })
            .collect::<Vec<_>>();
        let mut model = Model::default();
        for &event in &trace {
            model.step(event);
        }
        let before = model.clone();
        let final_event = Event {
            connection,
            operation: Operation::Enqueue(code),
        };
        assert_eq!(model.step(final_event).send, Some(Err(expected)));
        assert_eq!(
            model
                .witness(&before, final_event)
                .contains(&"full_queue_rejected"),
            capacity_witness,
        );
        assert_eq!(model, before, "rejected enqueue must have no effect");
        trace.push(final_event);
        Sut::replay(&trace);
    }

    #[test]
    fn open_full_ticket_rejection_retains_capacity_witness() {
        for connection in 0..2 {
            for code in 0..3 {
                rejected(connection, code, full(code), ErrorKind::Backpressured, true);
            }
        }
    }

    #[test]
    fn cancelled_and_draining_full_tickets_preserve_gate_errors() {
        // Preserve the exact four-event counterexample from the failed campaign.
        rejected(
            0,
            0,
            vec![
                Operation::Enqueue(0),
                Operation::Enqueue(0),
                Operation::Cancel,
            ],
            ErrorKind::Cancelled,
            false,
        );
        for connection in 0..2 {
            for code in 0..3 {
                for (operation, expected) in [
                    (Operation::Cancel, ErrorKind::Cancelled),
                    (Operation::Drain, ErrorKind::Closed),
                ] {
                    let mut operations = full(code);
                    operations.push(operation);
                    rejected(connection, code, operations, expected, false);
                }
                let mut operations = flushed(code);
                operations.push(Operation::Drain);
                rejected(connection, code, operations, ErrorKind::Closed, false);
            }
        }
    }

    #[test]
    fn terminal_full_tickets_keep_causes_and_later_cancel_precedence() {
        for connection in 0..2 {
            for code in 0..3 {
                let mut send = full(code);
                send.push(Operation::Deadline);
                let mut frame = vec![Operation::ReadByte];
                frame.extend(full(code));
                frame.push(Operation::Deadline);
                let mut drain = full(code);
                drain.extend([Operation::Drain, Operation::Deadline]);
                let mut idle = flushed(code);
                // The untouched connection reaches its idle deadline first.
                // A second global advance reaches the written connection's.
                idle.extend([Operation::Deadline, Operation::Deadline]);
                for (operations, expected) in [
                    (send, ErrorKind::SendTimedOut),
                    (frame, ErrorKind::FrameTimedOut),
                    (drain, ErrorKind::DrainTimedOut),
                    (idle, ErrorKind::IdleTimedOut),
                ] {
                    rejected(connection, code, operations.clone(), expected, false);
                    let mut cancelled = operations;
                    cancelled.push(Operation::Cancel);
                    rejected(connection, code, cancelled, ErrorKind::Cancelled, false);
                }
            }
        }
    }
}
