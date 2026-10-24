//! Bounded, engine-free native fixture for the COMM C-V18 acceptance profile.
use serde_json::json;
use std::{
    collections::VecDeque,
    io::{self, BufRead, Write},
    path::Path,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use xadb_platform::{
    network::{LocalEndpoint, LocalNetwork, UnixDirectory},
    runtime::MonotonicClock,
};
use xadb_transport::{
    ConnectionId, ConnectionPhase, ConnectionSender, Driver, Instrumentation, SendOutcome,
    SendTicket, Transport,
};

const MAX_PENDING_ECHO_FRAMES: usize = 2;

#[derive(Default)]
struct TxPressure {
    since: Option<(Instant, u64)>,
    peak_stalled_ns: u64,
}

impl TxPressure {
    // A full producer queue alone can be ordinary progress. Only a continuous
    // open, full TX queue with backlog and unchanged native written bytes is a
    // pressure witness. Readiness need not permit a syscall returning WouldBlock.
    fn observe(&mut self, now: Instant, saturated: bool, written_bytes: u64) {
        if !saturated {
            self.since = None;
            return;
        }
        match self.since {
            Some((since, previous_bytes)) if previous_bytes == written_bytes => {
                self.peak_stalled_ns = self
                    .peak_stalled_ns
                    .max(u64::try_from(now.duration_since(since).as_nanos()).unwrap_or(u64::MAX));
            }
            _ => self.since = Some((now, written_bytes)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Workload {
    Echo,
    Bulk,
}

struct Owner {
    id: ConnectionId,
    sender: ConnectionSender,
    tickets: Vec<SendTicket>,
    pending_rx: VecDeque<xadb_transport::ReceivedFrame>,
    received_frames: u64,
    received_bytes: u64,
    measured_received_frames: [u64; 30],
    measured_received_payload_bytes: [u64; 30],
    sent_frames: u64,
    flushed_frames: u64,
    failed_tickets: u64,
    stats: xadb_transport::ConnectionStats,
    expected_payload_bytes: usize,
    expected_sequence: u64,
    index: usize,
    expected_disconnect: bool,
    expected_peer_reset: bool,
    backpressured_admissions: u64,
    pending_echo_backlog_peak: u64,
    ticket_cap_saturations: u64,
    tx_pressure: TxPressure,
    dropped_echo_responses: u64,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("COMM performance worker: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 6 {
        return Err("usage: comm_performance tcp4|tcp6|unix ALWAYS|DETAILED ECHO|BULK CONNECTIONS PAYLOAD_BYTES NORMAL|SLOW [PRIVATE_DIR]".into());
    }
    let family = args[0].as_str();
    let mode = match args[1].as_str() {
        "ALWAYS" => Instrumentation::Always,
        "DETAILED" => Instrumentation::Detailed,
        _ => return Err("invalid instrumentation mode".into()),
    };
    let workload = match args[2].as_str() {
        "ECHO" => Workload::Echo,
        "BULK" => Workload::Bulk,
        _ => return Err("invalid workload".into()),
    };
    let expected_connections = args[3].parse::<usize>()?;
    if !(1..=128).contains(&expected_connections) {
        return Err("connections must be 1..=128".into());
    }
    let expected_payload_bytes = args[4].parse::<usize>()?;
    if expected_payload_bytes > 1_048_576 {
        return Err("payload bytes must be 0..=1048576".into());
    }
    let slow_peer = match args[5].as_str() {
        "NORMAL" => false,
        "SLOW" if workload == Workload::Echo => true,
        _ => return Err("invalid slow-peer profile".into()),
    };
    let mut directory = None;
    let endpoint = match family {
        "tcp4" if args.len() == 6 => LocalEndpoint::tcp("127.0.0.1:0".parse()?)?,
        "tcp6" if args.len() == 6 => LocalEndpoint::tcp("[::1]:0".parse()?)?,
        "unix" if args.len() == 7 => {
            let dir = UnixDirectory::create(Path::new(&args[6]))?;
            let endpoint = LocalEndpoint::unix(&dir, "socket")?;
            directory = Some(dir);
            endpoint
        }
        "unix" => return Err("Unix workload requires a private directory".into()),
        _ => return Err("invalid address family or argument count".into()),
    };

    let network = LocalNetwork::default();
    let listener = network.listen(endpoint)?;
    let endpoint = listener.endpoint().clone();
    let transport = Transport::with_instrumentation(Arc::new(MonotonicClock::default()), mode);
    let mut driver = Driver::new(transport.clone(), network)?;
    driver.add_listener(Box::new(listener))?;
    println!(
        "{}",
        json!({
            "event": "ready",
            "source_sha256": env!("XADBTEST_SOURCE_SHA256"),
            "profile": env!("XADBTEST_PROFILE"),
            "target": env!("XADBTEST_TARGET"),
            "pid": std::process::id(),
            "family": family,
            "instrumentation": args[1],
            "workload": args[2],
            "expected_connections": expected_connections,
            "payload_bytes": expected_payload_bytes,
            "slow_peer": slow_peer,
            "tcp": endpoint.socket_addr().map(|a| a.to_string()),
            "unix": endpoint.unix_path().map(|p| p.to_string_lossy())
        })
    );
    io::stdout().flush()?;

    let mut owners: Vec<Owner> = Vec::with_capacity(expected_connections);
    let (run_deadline, command_receiver) = loop {
        driver.step(Duration::from_millis(2))?;
        discover(&driver, &mut owners, expected_payload_bytes);
        pump(&mut driver, &mut owners, workload, None)?;
        if owners.len() >= expected_connections {
            if slow_peer {
                owners[0].expected_disconnect = true;
                owners[0].expected_peer_reset = true;
            }
            owners[expected_connections - 1].expected_disconnect = true;
            println!("{}", json!({"event": "armed"}));
            io::stdout().flush()?;
            let mut command = String::new();
            io::stdin().lock().read_line(&mut command)?;
            if command.trim() != "start" {
                return Err("invalid performance start handshake".into());
            }
            let started_at = Instant::now();
            let (command_sender, command_receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let stdin = io::stdin();
                for line in stdin.lock().lines() {
                    if command_sender.send(line).is_err() {
                        break;
                    }
                }
            });
            println!(
                "{}",
                json!({"event": "started", "at_monotonic_ns": monotonic_nanos()})
            );
            io::stdout().flush()?;
            break (
                (
                    started_at + Duration::from_secs(5),
                    started_at + Duration::from_secs(35),
                ),
                command_receiver,
            );
        }
        if owners.iter().any(|owner| {
            driver
                .connection(owner.id)
                .is_some_and(|connection| connection.phase() == ConnectionPhase::Closed)
        }) {
            return Err("a peer closed before the expected connection count was reached".into());
        }
    };

    let (measure_start, run_deadline) = run_deadline;
    let measurement_window = Some((measure_start, run_deadline));
    while Instant::now() < run_deadline {
        driver.step(Duration::from_millis(2))?;
        discover(&driver, &mut owners, expected_payload_bytes);
        pump(&mut driver, &mut owners, workload, measurement_window)?;
    }

    let cancelled_index = owners.len() - 1;
    let requested_received_frames = loop {
        driver.step(Duration::from_millis(2))?;
        discover(&driver, &mut owners, expected_payload_bytes);
        pump(&mut driver, &mut owners, workload, measurement_window)?;
        match command_receiver.try_recv() {
            Ok(Ok(command)) => {
                let mut fields = command.split_whitespace();
                if fields.next() != Some("quiesce") {
                    return Err("expected performance quiesce command".into());
                }
                let frames = fields
                    .next()
                    .ok_or("quiesce command omitted offered frame count")?
                    .parse::<u64>()?;
                if fields.next().is_some() {
                    return Err("invalid performance quiesce command".into());
                }
                break frames;
            }
            Ok(Err(error)) => return Err(error.into()),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("performance command channel disconnected before quiesce".into());
            }
        }
    };
    while owners[cancelled_index].received_frames < requested_received_frames {
        let target = &owners[cancelled_index];
        let connection = driver
            .connection(target.id)
            .ok_or("cancellation target disappeared before quiesce")?;
        if connection.phase() != ConnectionPhase::Open || connection.terminal_error().is_some() {
            return Err(
                "cancellation target closed before all offered frames were received".into(),
            );
        }
        driver.step(Duration::from_millis(2))?;
        discover(&driver, &mut owners, expected_payload_bytes);
        pump(&mut driver, &mut owners, workload, measurement_window)?;
    }
    let target = owners
        .get(cancelled_index)
        .ok_or("cancellation target disappeared")?;
    let target_connection = driver
        .connection(target.id)
        .ok_or("cancellation target disappeared")?;
    if target_connection.phase() != ConnectionPhase::Open
        || target_connection.terminal_error().is_some()
    {
        return Err("cancellation target was not live before cancellation".into());
    }
    println!(
        "{}",
        json!({
            "event": "cancel-ready",
            "connection_index": cancelled_index,
            "phase_before": format!("{:?}", target_connection.phase()),
            "terminal_error_before": null,
            "received_frames": target.received_frames
        })
    );
    io::stdout().flush()?;
    loop {
        driver.step(Duration::from_millis(2))?;
        discover(&driver, &mut owners, expected_payload_bytes);
        pump(&mut driver, &mut owners, workload, measurement_window)?;
        match command_receiver.try_recv() {
            Ok(Ok(command)) if command.trim() == "cancel" => break,
            Ok(Ok(_)) => return Err("invalid performance cancellation command".into()),
            Ok(Err(error)) => return Err(error.into()),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("performance cancellation command channel disconnected".into());
            }
        }
    }
    let target_id = owners[cancelled_index].id;
    let connection = driver
        .connection_mut(target_id)
        .ok_or("cancellation target disappeared before cancellation")?;
    if connection.phase() != ConnectionPhase::Open || connection.terminal_error().is_some() {
        return Err("cancellation target closed before the cancellation request".into());
    }
    connection.cancel();
    let terminal_error = connection.terminal_error();
    if connection.phase() != ConnectionPhase::Closed
        || terminal_error.is_none_or(|error| error.kind != xadb_transport::ErrorKind::Cancelled)
    {
        return Err("cancellation did not produce the Cancelled terminal reason".into());
    }
    println!(
        "{}",
        json!({
            "event": "cancelled",
            "connection_index": cancelled_index,
            "phase_before": "Open",
            "phase_after": "Closed",
            "terminal_error_kind": "Cancelled",
            "at_monotonic_ns": monotonic_nanos()
        })
    );
    io::stdout().flush()?;

    let peer_close_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < peer_close_deadline
        && owners.iter().any(|owner| {
            driver.connection(owner.id).is_some_and(|connection| {
                connection.phase() != ConnectionPhase::Closed && !connection.peer_write_closed()
            })
        })
    {
        driver.step(Duration::from_millis(2))?;
        discover(&driver, &mut owners, expected_payload_bytes);
        pump(&mut driver, &mut owners, workload, measurement_window)?;
    }

    driver.begin_drain();
    let drain_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < drain_deadline {
        driver.step(Duration::from_millis(2))?;
        pump(&mut driver, &mut owners, Workload::Bulk, measurement_window)?;
        if owners.iter().all(|owner| {
            driver
                .connection(owner.id)
                .is_none_or(|connection| connection.phase() == ConnectionPhase::Closed)
        }) {
            break;
        }
    }
    for owner in &mut owners {
        if !owner.pending_rx.is_empty() {
            if !owner.expected_disconnect {
                return Err(format!(
                    "unexpected pending echo responses for connection {}",
                    owner.index
                )
                .into());
            }
            owner.dropped_echo_responses = owner
                .dropped_echo_responses
                .saturating_add(owner.pending_rx.len() as u64);
        }
        owner.pending_rx.clear();
        collect_tickets(owner);
        if let Some(connection) = driver.connection(owner.id) {
            owner.stats = connection.stats();
        }
    }
    let totals = aggregate(&owners);
    let connection_details = owners
        .iter()
        .map(|owner| {
            json!({
                "connection_index": owner.index,
                "expected_disconnect": owner.expected_disconnect,
                "expected_peer_reset": owner.expected_peer_reset,
                "received_frames": owner.received_frames,
                "sent_frames": owner.sent_frames,
                "flushed_frames": owner.flushed_frames,
                "failed_tickets": owner.failed_tickets,
                "dropped_echo_responses": owner.dropped_echo_responses,
                "backpressured_admissions": owner.backpressured_admissions,
                "pending_echo_backlog_peak": owner.pending_echo_backlog_peak,
                "ticket_cap_saturations": owner.ticket_cap_saturations,
                "peak_stalled_tx_ns": owner.tx_pressure.peak_stalled_ns,
                "would_block": owner.stats.would_block,
                "peak_tx_queue_bytes": owner.stats.peak_queue_bytes[1],
                "peak_tx_queue_slots": owner.stats.peak_queue_slots[1]
            })
        })
        .collect::<Vec<_>>();
    let diagnostics = transport.diagnostics();
    let diagnostic_events = diagnostics
        .events
        .iter()
        .flatten()
        .map(|event| {
            json!({
                "at_nanos": event.at_nanos,
                "registration": {
                    "slot": event.registration.slot,
                    "generation": event.registration.generation
                },
                "kind": match event.kind {
                    xadb_transport::EventKind::Enqueued => "ENQUEUED",
                    xadb_transport::EventKind::Received => "RECEIVED",
                    xadb_transport::EventKind::Flushed => "FLUSHED",
                    xadb_transport::EventKind::Closed => "CLOSED"
                },
                "value": event.value,
                "error": event.error.map(|error| format!("{error:?}"))
            })
        })
        .collect::<Vec<_>>();
    let closed_count = owners
        .iter()
        .filter(|owner| {
            driver
                .connection(owner.id)
                .is_none_or(|connection| connection.phase() == ConnectionPhase::Closed)
        })
        .count();
    let source_sha256 = env!("XADBTEST_SOURCE_SHA256");
    drop(owners);
    drop(driver);
    drop(endpoint);
    drop(directory);
    let payload_bytes = transport.payload_bytes();
    let connection_count = transport.connection_count();
    if closed_count != expected_connections || payload_bytes != 0 || connection_count != 0 {
        return Err(format!(
            "cleanup failed: closed={closed_count}/{expected_connections}, payload={payload_bytes}, connections={connection_count}"
        )
        .into());
    }
    println!(
        "{}",
        json!({
            "event": "closed",
            "source_sha256": source_sha256,
            "instrumentation": args[1],
            "workload": args[2],
            "duration_seconds": 35,
            "connections": closed_count,
            "totals": totals,
            "connection_details": connection_details,
            "diagnostics": {
                "events_retained": diagnostics.events.iter().flatten().count(),
                "events_lost": diagnostics.lost_events,
                "backpressured": diagnostics.backpressured,
                "peak_payload_bytes": diagnostics.peak_payload_bytes,
                "events": diagnostic_events
            },
            "cleanup": {"payload_bytes": payload_bytes, "connection_count": connection_count}
        })
    );
    Ok(())
}

fn discover(driver: &Driver<LocalNetwork>, owners: &mut Vec<Owner>, payload_bytes: usize) {
    for id in driver.connections() {
        if owners.iter().all(|owner| owner.id != id)
            && let Some(connection) = driver.connection(id)
        {
            owners.push(Owner {
                id,
                sender: connection.sender(),
                tickets: Vec::with_capacity(4),
                pending_rx: VecDeque::with_capacity(2),
                received_frames: 0,
                received_bytes: 0,
                measured_received_frames: [0; 30],
                measured_received_payload_bytes: [0; 30],
                sent_frames: 0,
                flushed_frames: 0,
                failed_tickets: 0,
                stats: connection.stats(),
                expected_payload_bytes: payload_bytes,
                expected_sequence: 0,
                index: owners.len(),
                expected_disconnect: false,
                expected_peer_reset: false,
                backpressured_admissions: 0,
                pending_echo_backlog_peak: 0,
                ticket_cap_saturations: 0,
                tx_pressure: TxPressure::default(),
                dropped_echo_responses: 0,
            });
        }
    }
}

fn collect_tickets(owner: &mut Owner) {
    let mut pending = Vec::with_capacity(4);
    for ticket in owner.tickets.drain(..) {
        match ticket.try_take() {
            Ok(SendOutcome::FlushedLocally) => owner.flushed_frames += 1,
            Ok(SendOutcome::Failed { .. }) => owner.failed_tickets += 1,
            Err(ticket) => pending.push(ticket),
        }
    }
    owner.tickets = pending;
}

fn pump(
    driver: &mut Driver<LocalNetwork>,
    owners: &mut [Owner],
    workload: Workload,
    measurement_window: Option<(Instant, Instant)>,
) -> Result<(), Box<dyn std::error::Error>> {
    for owner in owners {
        collect_tickets(owner);
        if let Some(connection) = driver.connection_mut(owner.id) {
            while workload == Workload::Bulk || owner.pending_rx.len() < MAX_PENDING_ECHO_FRAMES {
                let Some(frame) = connection.take_received() else {
                    break;
                };
                let frame_len = frame.payload().len() as u64;
                if frame.payload().len() != owner.expected_payload_bytes
                    || !frame
                        .payload()
                        .iter()
                        .enumerate()
                        .all(|(index, byte)| expected_byte(owner.expected_sequence, index) == *byte)
                {
                    return Err("independent bulk payload oracle mismatch".into());
                }
                owner.expected_sequence = owner
                    .expected_sequence
                    .checked_add(1)
                    .ok_or("payload sequence overflow")?;
                owner.received_frames = owner.received_frames.saturating_add(1);
                owner.received_bytes = owner.received_bytes.saturating_add(frame_len);
                if let Some((start, end)) = measurement_window {
                    let observed = Instant::now();
                    if start <= observed && observed < end {
                        let second = observed.duration_since(start).as_secs() as usize;
                        if second < 30 {
                            owner.measured_received_frames[second] =
                                owner.measured_received_frames[second].saturating_add(1);
                            owner.measured_received_payload_bytes[second] = owner
                                .measured_received_payload_bytes[second]
                                .saturating_add(frame_len);
                        }
                    }
                }
                if workload == Workload::Echo {
                    owner.pending_rx.push_back(frame);
                }
            }
            owner.stats = connection.stats();
        }
        if workload == Workload::Echo {
            owner.pending_echo_backlog_peak = owner
                .pending_echo_backlog_peak
                .max(owner.pending_rx.len() as u64);
            if owner.tickets.len() >= 4 && !owner.pending_rx.is_empty() {
                owner.ticket_cap_saturations = owner.ticket_cap_saturations.saturating_add(1);
            }
            while owner.tickets.len() < 4 {
                let Some(frame) = owner.pending_rx.pop_front() else {
                    break;
                };
                match owner.sender.try_send(frame.payload()) {
                    Ok(ticket) => {
                        owner.sent_frames = owner.sent_frames.saturating_add(1);
                        owner.tickets.push(ticket);
                    }
                    Err(error) if error.kind == xadb_transport::ErrorKind::Backpressured => {
                        owner.backpressured_admissions =
                            owner.backpressured_admissions.saturating_add(1);
                        owner.pending_rx.push_front(frame);
                        break;
                    }
                    Err(error)
                        if owner.expected_disconnect
                            && matches!(
                                error.kind,
                                xadb_transport::ErrorKind::Closed
                                    | xadb_transport::ErrorKind::Cancelled
                            ) =>
                    {
                        owner.dropped_echo_responses =
                            owner.dropped_echo_responses.saturating_add(1);
                    }
                    Err(error)
                        if owner.expected_peer_reset
                            && error.kind == xadb_transport::ErrorKind::Io
                            && error.io_kind == Some(std::io::ErrorKind::ConnectionReset) =>
                    {
                        owner.dropped_echo_responses =
                            owner.dropped_echo_responses.saturating_add(1);
                    }
                    Err(error) => return Err(format!("echo admission failed: {error}").into()),
                }
            }
            if owner.expected_peer_reset {
                let open = driver
                    .connection(owner.id)
                    .is_some_and(|connection| connection.phase() == ConnectionPhase::Open);
                owner.tx_pressure.observe(
                    Instant::now(),
                    open && owner.tickets.len() == 4 && !owner.pending_rx.is_empty(),
                    owner.stats.written_bytes,
                );
            }
        }
    }
    Ok(())
}

fn aggregate(owners: &[Owner]) -> serde_json::Value {
    let mut received_frames = 0_u64;
    let mut received_bytes = 0_u64;
    let mut sent_frames = 0_u64;
    let mut flushed_frames = 0_u64;
    let mut failed_tickets = 0_u64;
    let mut read_bytes = 0_u64;
    let mut written_bytes = 0_u64;
    let mut attempts = 0_u64;
    let mut short_io = 0_u64;
    let mut would_block = 0_u64;
    let mut copied_payload_bytes = 0_u64;
    let mut peak_queue_bytes = [0_u64; 2];
    let mut peak_queue_slots = [0_u64; 2];
    let mut send_latency = [0_u64; 64];
    let mut receive_latency = [0_u64; 64];
    let mut measured_received_frames = [0_u64; 30];
    let mut measured_received_payload_bytes = [0_u64; 30];
    let mut overflowed = false;
    for owner in owners {
        received_frames = received_frames.saturating_add(owner.received_frames);
        received_bytes = received_bytes.saturating_add(owner.received_bytes);
        sent_frames = sent_frames.saturating_add(owner.sent_frames);
        flushed_frames = flushed_frames.saturating_add(owner.flushed_frames);
        failed_tickets = failed_tickets.saturating_add(owner.failed_tickets);
        for index in 0..30 {
            measured_received_frames[index] = measured_received_frames[index]
                .saturating_add(owner.measured_received_frames[index]);
            measured_received_payload_bytes[index] = measured_received_payload_bytes[index]
                .saturating_add(owner.measured_received_payload_bytes[index]);
        }
        read_bytes = read_bytes.saturating_add(owner.stats.read_bytes);
        written_bytes = written_bytes.saturating_add(owner.stats.written_bytes);
        attempts = attempts.saturating_add(owner.stats.attempts);
        short_io = short_io.saturating_add(owner.stats.short_io);
        would_block = would_block.saturating_add(owner.stats.would_block);
        copied_payload_bytes =
            copied_payload_bytes.saturating_add(owner.stats.copied_payload_bytes);
        for index in 0..2 {
            peak_queue_bytes[index] =
                peak_queue_bytes[index].saturating_add(owner.stats.peak_queue_bytes[index]);
            peak_queue_slots[index] =
                peak_queue_slots[index].saturating_add(owner.stats.peak_queue_slots[index]);
        }
        for index in 0..64 {
            send_latency[index] =
                send_latency[index].saturating_add(owner.stats.send_latency.buckets[index]);
            receive_latency[index] =
                receive_latency[index].saturating_add(owner.stats.receive_latency.buckets[index]);
        }
        overflowed |= owner.stats.overflowed
            || owner.stats.send_latency.overflowed
            || owner.stats.receive_latency.overflowed;
    }
    json!({
        "received_frames": received_frames,
        "received_payload_bytes": received_bytes,
        "measured_received_frames_per_second": measured_received_frames.to_vec(),
        "measured_received_payload_bytes_per_second": measured_received_payload_bytes.to_vec(),
        "sent_frames": sent_frames,
        "flushed_frames": flushed_frames,
        "failed_tickets": failed_tickets,
        "read_bytes": read_bytes,
        "written_bytes": written_bytes,
        "attempts": attempts,
        "short_io": short_io,
        "would_block": would_block,
        "copied_payload_bytes": copied_payload_bytes,
        "peak_queue_bytes_sum_by_direction": peak_queue_bytes,
        "peak_queue_slots_sum_by_direction": peak_queue_slots,
        "send_latency_histogram": send_latency.to_vec(),
        "receive_latency_histogram": receive_latency.to_vec(),
        "overflowed": overflowed
    })
}

fn monotonic_nanos() -> u128 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_nanos()
}

fn expected_byte(sequence: u64, index: usize) -> u8 {
    sequence
        .wrapping_mul(37)
        .wrapping_add((index as u64).wrapping_mul(31))
        .wrapping_add(7) as u8
}

#[cfg(test)]
mod pressure_tests {
    use super::TxPressure;
    use std::time::{Duration, Instant};

    #[test]
    fn only_continuous_saturation_without_byte_progress_builds_pressure() {
        let now = Instant::now();
        let mut pressure = TxPressure::default();
        pressure.observe(now, true, 42);
        pressure.observe(now + Duration::from_millis(499), true, 42);
        assert_eq!(pressure.peak_stalled_ns, 499_000_000);
        pressure.observe(now + Duration::from_millis(500), true, 42);
        assert_eq!(pressure.peak_stalled_ns, 500_000_000);
        // One native byte of progress restarts the interval, even at a full queue.
        pressure.observe(now + Duration::from_secs(1), true, 43);
        pressure.observe(now + Duration::from_millis(1100), true, 43);
        assert_eq!(pressure.peak_stalled_ns, 500_000_000);
        pressure.observe(now + Duration::from_millis(1600), true, 43);
        assert_eq!(pressure.peak_stalled_ns, 600_000_000);
    }

    #[test]
    fn closed_or_unsaturated_intervals_never_accumulate_pressure() {
        let now = Instant::now();
        let mut pressure = TxPressure::default();
        pressure.observe(now, true, 0);
        pressure.observe(now + Duration::from_millis(400), true, 0);
        pressure.observe(now + Duration::from_millis(450), false, 0);
        pressure.observe(now + Duration::from_secs(30), false, 0);
        assert_eq!(pressure.peak_stalled_ns, 400_000_000);
        pressure.observe(now + Duration::from_secs(31), true, 0);
        pressure.observe(now + Duration::from_millis(31050), true, 0);
        assert_eq!(pressure.peak_stalled_ns, 400_000_000);
    }

    #[test]
    fn continuing_native_writes_do_not_qualify_even_with_a_full_queue() {
        let now = Instant::now();
        let mut pressure = TxPressure::default();
        for index in 0..100 {
            pressure.observe(now + Duration::from_millis(index * 100), true, index);
        }
        assert_eq!(pressure.peak_stalled_ns, 0);
    }
}
