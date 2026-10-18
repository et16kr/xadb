use crate::{
    Connection, ConnectionPhase, Error, ErrorKind, MAX_CONNECTIONS, MAX_LISTENERS, Result,
    Transport, WAIT_NANOS,
};
use std::{io, time::Duration};
use xadb_foundation::{
    network::{Interest, Listener, ReadinessEvent, ReadinessWait, Ready, Registration},
    runtime::Cancellation,
};

/// Driver slot incarnation, distinct from a native registration or fd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionId {
    slot: usize,
    generation: u64,
}

/// Single-thread owner with bounded round-robin I/O and listener admission.
/// Call step repeatedly from an application-owned thread; the assembly owns join.
/// No internal thread, unbounded mailbox, or application callback is created.
pub struct Driver<W: ReadinessWait> {
    transport: Transport,
    wait: W,
    connections: Vec<Option<Connection>>,
    generations: [u64; MAX_CONNECTIONS],
    listeners: [Option<Box<dyn Listener>>; MAX_LISTENERS],
    next_connection: usize,
    next_listener: usize,
    listener_retry: [u64; MAX_LISTENERS],
    cancellation: Cancellation,
    draining: bool,
}
impl<W: ReadinessWait> Driver<W> {
    /// Allocate bounded control storage. Every adopted connection must come from
    /// this same transport clock/resource domain.
    ///
    /// # Errors
    /// Returns ResourceExhausted if control storage cannot be allocated.
    pub fn new(transport: Transport, wait: W) -> Result<Self> {
        let mut connections = Vec::new();
        connections
            .try_reserve_exact(MAX_CONNECTIONS)
            .map_err(|_| ErrorKind::ResourceExhausted)?;
        connections.resize_with(MAX_CONNECTIONS, || None);
        Ok(Self {
            transport,
            wait,
            connections,
            generations: [0; MAX_CONNECTIONS],
            listeners: std::array::from_fn(|_| None),
            next_connection: 0,
            next_listener: 0,
            listener_retry: [0; MAX_LISTENERS],
            cancellation: Cancellation::default(),
            draining: false,
        })
    }
    /// Driver-wide cancellation, inherited by adopted connections and their
    /// existing senders. Observed at admission, each I/O check, and wait/accept.
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
    /// Add an owned listener; does not perform accept or bind.
    ///
    /// # Errors
    /// Rejects duplicate registrations, more than three listeners, or shutdown.
    pub fn add_listener(&mut self, listener: Box<dyn Listener>) -> Result<()> {
        if self.draining || self.cancellation.check().is_err() {
            return Err(ErrorKind::Closed.into());
        }
        let reg = listener.registration();
        if reg.generation == 0 || self.registration_in_use(reg) {
            return Err(ErrorKind::AdapterViolation.into());
        }
        let slot = self
            .listeners
            .iter_mut()
            .find(|s| s.is_none())
            .ok_or(ErrorKind::ResourceExhausted)?;
        *slot = Some(listener);
        Ok(())
    }
    fn registration_in_use(&self, registration: Registration) -> bool {
        self.connections
            .iter()
            .flatten()
            .any(|c| c.registration() == registration)
            || self
                .listeners
                .iter()
                .flatten()
                .any(|l| l.registration() == registration)
    }
    /// Adopt a connection without changing its clock, admission, or frame state.
    ///
    /// # Errors
    /// A different clock domain, duplicate registration, shutdown, or no slot is
    /// rejected; the moved connection closes through Drop.
    pub fn add(&mut self, connection: Connection) -> Result<ConnectionId> {
        if self.draining || self.cancellation.check().is_err() {
            return Err(ErrorKind::Closed.into());
        }
        if !std::sync::Arc::ptr_eq(&self.transport.clock, &connection.shared.clock)
            || self.registration_in_use(connection.registration())
        {
            return Err(ErrorKind::AdapterViolation.into());
        }
        let slot = self
            .connections
            .iter()
            .enumerate()
            .position(|(i, c)| c.is_none() && self.generations[i] != u64::MAX)
            .ok_or(ErrorKind::ResourceExhausted)?;
        connection
            .shared
            .admission
            .lock()
            .map_err(|_| ErrorKind::Poisoned)?
            .driver_cancellation = Some(self.cancellation.clone());
        self.generations[slot] += 1;
        self.connections[slot] = Some(connection);
        Ok(ConnectionId {
            slot,
            generation: self.generations[slot],
        })
    }
    /// Bounded iterator including closed owners whose external resources remain.
    pub fn connections(&self) -> impl Iterator<Item = ConnectionId> + '_ {
        self.connections.iter().enumerate().filter_map(|(slot, c)| {
            c.as_ref().map(|_| ConnectionId {
                slot,
                generation: self.generations[slot],
            })
        })
    }
    /// Borrow only if the driver slot generation still matches.
    pub fn connection(&self, id: ConnectionId) -> Option<&Connection> {
        (self.generations.get(id.slot) == Some(&id.generation))
            .then(|| self.connections[id.slot].as_ref())
            .flatten()
    }
    /// Mutably borrow the one owner, e.g. to collect a frame or begin drain.
    pub fn connection_mut(&mut self, id: ConnectionId) -> Option<&mut Connection> {
        if self.generations.get(id.slot) != Some(&id.generation) {
            return None;
        }
        self.connections[id.slot].as_mut()
    }
    /// Stop listeners first, then drain all accepted TX. Call step until all
    /// connections are Closed, then join the assembly's driver thread explicitly.
    pub fn begin_drain(&mut self) {
        self.draining = true;
        for listener in &mut self.listeners {
            if let Some(mut l) = listener.take() {
                l.close();
            }
        }
        for connection in self.connections.iter_mut().flatten() {
            connection.begin_drain();
        }
    }
    fn cancel(&mut self) {
        self.draining = true;
        for listener in &mut self.listeners {
            if let Some(mut l) = listener.take() {
                l.close();
            }
        }
        for connection in self.connections.iter_mut().flatten() {
            connection.cancel();
        }
    }
    /// One bounded wait and round-robin sweep. max_wait is capped at 20 ms and
    /// further bounded by connection deadlines. Empty interest sets still wait.
    ///
    /// # Errors
    /// Wait/clock/adapter failures close the driver; it cannot be reused.
    pub fn step(&mut self, max_wait: Duration) -> Result<()> {
        let result = self.step_inner(max_wait);
        if let Err(error) = result {
            self.cancellation.cancel();
            self.draining = true;
            for listener in &mut self.listeners {
                if let Some(mut l) = listener.take() {
                    l.close();
                }
            }
            for connection in self.connections.iter_mut().flatten() {
                connection.close(error);
            }
        }
        result
    }
    fn step_inner(&mut self, max_wait: Duration) -> Result<()> {
        if self.cancellation.check().is_err() {
            self.cancel();
            return Ok(());
        }
        // Cancellation, deadlines and TX admission are checked even without events.
        for connection in self.connections.iter_mut().flatten() {
            connection.poll_control();
        }
        for entry in &mut self.connections {
            if entry.as_ref().is_some_and(Connection::reclaimable) {
                *entry = None;
            }
        }
        let now = self.transport.clock.now()?;
        let mut timeout = max_wait.min(Duration::from_nanos(WAIT_NANOS));
        const EMPTY: Interest = Interest {
            registration: Registration {
                slot: 0,
                generation: 0,
            },
            readable: false,
            writable: false,
        };
        const EVENT: ReadinessEvent = ReadinessEvent {
            registration: Registration {
                slot: 0,
                generation: 0,
            },
            ready: Ready {
                readable: false,
                writable: false,
                hangup: false,
                error: false,
                invalid: false,
            },
        };
        let mut interests = [EMPTY; MAX_CONNECTIONS + MAX_LISTENERS];
        let mut len = 0;
        for connection in self
            .connections
            .iter()
            .flatten()
            .filter(|c| c.phase() != ConnectionPhase::Closed)
        {
            timeout = timeout.min(Duration::from_nanos(
                connection.next_deadline()?.saturating_sub(now),
            ));
            if let Some(interest) = connection.interest() {
                interests[len] = interest;
                len += 1;
            }
        }
        if self.transport.connection_count() < MAX_CONNECTIONS as u64 && !self.draining {
            for (index, listener) in self
                .listeners
                .iter()
                .enumerate()
                .filter_map(|(i, l)| l.as_ref().map(|l| (i, l)))
            {
                if now < self.listener_retry[index] {
                    timeout = timeout.min(Duration::from_nanos(self.listener_retry[index] - now));
                    continue;
                }
                interests[len] = Interest {
                    registration: listener.registration(),
                    readable: true,
                    writable: false,
                };
                len += 1;
            }
        }
        let mut events = [EVENT; MAX_CONNECTIONS + MAX_LISTENERS];
        let fresh = self.transport.clock.now()?;
        timeout = timeout.saturating_sub(Duration::from_nanos(fresh.saturating_sub(now)));
        if self.cancellation.check().is_err() {
            self.cancel();
            return Ok(());
        }
        let count = match self.wait.wait(&interests[..len], timeout, &mut events) {
            Ok(n) if n <= len && n <= events.len() => n,
            Ok(_) => return Err(ErrorKind::AdapterViolation.into()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => 0,
            Err(e) => return Err(Error::io(e)),
        };
        if self.cancellation.check().is_err() {
            self.cancel();
            return Ok(());
        }
        for offset in 0..MAX_CONNECTIONS {
            let index = (self.next_connection + offset) % MAX_CONNECTIONS;
            if let Some(connection) = &mut self.connections[index] {
                let ready = events[..count]
                    .iter()
                    .filter(|e| e.registration == connection.registration())
                    .fold(Ready::default(), |mut r, e| {
                        r.readable |= e.ready.readable;
                        r.writable |= e.ready.writable;
                        r.hangup |= e.ready.hangup;
                        r.error |= e.ready.error;
                        r.invalid |= e.ready.invalid;
                        r
                    });
                connection.advance(connection.registration(), ready);
            }
            if self.cancellation.check().is_err() {
                self.cancel();
                return Ok(());
            }
        }
        self.next_connection = (self.next_connection + 1) % MAX_CONNECTIONS;
        for listener in self.listeners.iter().flatten() {
            if events[..count]
                .iter()
                .any(|e| e.registration == listener.registration() && e.ready.invalid)
            {
                return Err(ErrorKind::AdapterViolation.into());
            }
            if events[..count].iter().any(|e| {
                e.registration == listener.registration() && (e.ready.error || e.ready.hangup)
            }) {
                return Err(ErrorKind::Io.into());
            }
        }
        let mut ready = std::array::from_fn::<_, MAX_LISTENERS, _>(|i| {
            self.listeners[i].as_ref().is_some_and(|l| {
                events[..count].iter().any(|e| {
                    e.registration == l.registration()
                        && (e.ready.readable || e.ready.error || e.ready.invalid)
                })
            })
        });
        for _ in 0..8 {
            if self.draining || self.transport.connection_count() >= MAX_CONNECTIONS as u64 {
                break;
            }
            if self.cancellation.check().is_err() {
                self.cancel();
                break;
            }
            let Some(index) = (0..MAX_LISTENERS)
                .map(|o| (self.next_listener + o) % MAX_LISTENERS)
                .find(|i| ready[*i])
            else {
                break;
            };
            self.next_listener = (index + 1) % MAX_LISTENERS;
            let family = match self.transport.reserve_connection() {
                Ok(f) => f,
                Err(_) => break,
            };
            let now = self.transport.clock.now()?;
            let listener = self.listeners[index]
                .as_mut()
                .ok_or(ErrorKind::AdapterViolation)?;
            match listener.try_accept() {
                Ok(stream) => {
                    if self.cancellation.check().is_err() {
                        drop(stream);
                        self.cancel();
                        break;
                    }
                    let connection = Connection::new(&self.transport, stream, family, false, now)?;
                    self.add(connection)?;
                }
                // A pending peer can disappear before accept. Back off only
                // this listener; existing sessions and other listeners remain
                // usable. Persistent listener/adapter errors still fail below.
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::ConnectionReset
                    ) =>
                {
                    ready[index] = false;
                    self.listener_retry[index] =
                        crate::deadline(self.transport.clock.now()?, WAIT_NANOS)?;
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => return Err(Error::io(e)),
            }
        }
        Ok(())
    }
}
impl<W: ReadinessWait> Drop for Driver<W> {
    fn drop(&mut self) {
        for listener in &mut self.listeners {
            if let Some(mut l) = listener.take() {
                l.close();
            }
        }
        // Connection::drop resolves pending tickets as Closed, not Cancelled.
        for entry in &mut self.connections {
            *entry = None;
        }
    }
}
