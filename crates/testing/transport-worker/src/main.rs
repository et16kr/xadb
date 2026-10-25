//! Default engine-free native echo fixture. AXF1 expectations live in the peer.
use platform::{
    network::{LocalEndpoint, LocalNetwork, UnixDirectory},
    runtime::MonotonicClock,
};
use serde_json::json;
use std::{
    io::{self, Write},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use transport::{ConnectionPhase, Driver, SendOutcome, SendTicket, Transport};
fn main() {
    if let Err(error) = run() {
        eprintln!("transport fixture: {error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut directory = None;
    let endpoint = match args.first().map(String::as_str) {
        Some("tcp4") if args.len() == 1 => LocalEndpoint::tcp("127.0.0.1:0".parse()?)?,
        Some("tcp6") if args.len() == 1 => LocalEndpoint::tcp("[::1]:0".parse()?)?,
        Some("unix") if args.len() == 2 => {
            let dir = UnixDirectory::create(Path::new(&args[1]))?;
            let endpoint = LocalEndpoint::unix(&dir, "socket")?;
            directory = Some(dir);
            endpoint
        }
        _ => {
            return Err(
                "usage: xadbtest-transport-worker tcp4|tcp6|unix NEW_PRIVATE_DIRECTORY".into(),
            );
        }
    };
    let network = LocalNetwork::default();
    let listener = network.listen(endpoint)?;
    let endpoint = listener.endpoint().clone();
    let transport = Transport::new(Arc::new(MonotonicClock::default()));
    let mut driver = Driver::new(transport.clone(), network)?;
    driver.add_listener(Box::new(listener))?;
    println!(
        "{}",
        json!({"event":"ready","source_sha256":env!("XADBTEST_SOURCE_SHA256"),"profile":env!("XADBTEST_PROFILE"),"target":env!("XADBTEST_TARGET"),"pid":std::process::id(),"tcp":endpoint.socket_addr().map(|a|a.to_string()),"unix":endpoint.unix_path().map(|p|p.to_string_lossy())})
    );
    io::stdout().flush()?;
    let end = Instant::now() + Duration::from_secs(15);
    let mut owner = None;
    let mut sender = None;
    let mut tickets: [Option<SendTicket>; 4] = std::array::from_fn(|_| None);
    let mut frames = 0;
    let mut flushed = 0;
    let mut failed = 0;
    let terminal;
    let final_stats;
    loop {
        if Instant::now() >= end {
            return Err("fixture deadline exceeded".into());
        }
        driver.step(Duration::from_millis(2))?;
        if owner.is_none() {
            owner = driver.connections().next();
            if let Some(id) = owner {
                sender = Some(driver.connection(id).ok_or("missing connection")?.sender());
            }
        }
        for slot in &mut tickets {
            if let Some(ticket) = slot.take() {
                match ticket.try_take() {
                    Ok(SendOutcome::FlushedLocally) => flushed += 1,
                    Ok(SendOutcome::Failed { .. }) => failed += 1,
                    Err(ticket) => *slot = Some(ticket),
                }
            }
        }
        if let Some(id) = owner {
            let c = driver
                .connection_mut(id)
                .ok_or("owner reclaimed while sender alive")?;
            if c.phase() == ConnectionPhase::Closed {
                terminal = c.terminal_error().ok_or("missing terminal")?;
                final_stats = c.stats();
                break;
            }
            for slot in tickets.iter_mut().filter(|s| s.is_none()) {
                if let Some(frame) = c.take_received() {
                    frames += 1;
                    *slot = Some(
                        sender
                            .as_ref()
                            .ok_or("missing sender")?
                            .try_send(frame.payload())?,
                    );
                } else {
                    break;
                }
            }
        }
    }
    drop(driver);
    drop(tickets);
    drop(sender);
    drop(endpoint);
    drop(directory);
    println!(
        "{}",
        json!({"event":"closed","frames":frames,"flushed":flushed,"failed":failed,"reason":format!("{:?}",terminal.kind),"io_kind":terminal.io_kind.map(|k|format!("{k:?}")),"read_bytes":final_stats.read_bytes,"written_bytes":final_stats.written_bytes,"short_io":final_stats.short_io,"would_block":final_stats.would_block,"payload_bytes":transport.payload_bytes(),"connections":transport.connection_count()})
    );
    Ok(())
}
