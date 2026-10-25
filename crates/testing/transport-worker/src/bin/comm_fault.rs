//! Engine-free RX-prefix fixture. The independent controller owns SIGKILL.
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
use transport::{ConnectionPhase, Driver, MAX_WIRE_BYTES, Transport};

fn main() {
    if let Err(error) = run() {
        eprintln!("COMM RX-prefix fixture: {error}");
        std::process::exit(1);
    }
}

fn emit(receipt: serde_json::Value) -> io::Result<()> {
    println!("{receipt}");
    io::stdout().flush()
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err("usage: comm_fault FAMILY PREFIX_BYTES NONCE [NEW_UNIX_DIRECTORY]".into());
    }
    let prefix: u64 = args[1].parse()?;
    if prefix == 0 || prefix > MAX_WIRE_BYTES as u64 || args[2].len() != 32 {
        return Err("invalid RX prefix or case nonce".into());
    }
    let mut directory = None;
    let endpoint = match args[0].as_str() {
        "tcp4" if args.len() == 3 => LocalEndpoint::tcp("127.0.0.1:0".parse()?)?,
        "tcp6" if args.len() == 3 => LocalEndpoint::tcp("[::1]:0".parse()?)?,
        "unix" if args.len() == 4 => {
            let dir = UnixDirectory::create(Path::new(&args[3]))?;
            let endpoint = LocalEndpoint::unix(&dir, "socket")?;
            directory = Some(dir);
            endpoint
        }
        _ => return Err("invalid fixture endpoint arguments".into()),
    };
    let network = LocalNetwork::default();
    let listener = network.listen(endpoint)?;
    let endpoint = listener.endpoint().clone();
    let transport = Transport::new(Arc::new(MonotonicClock::default()));
    let mut driver = Driver::new(transport, network)?;
    driver.add_listener(Box::new(listener))?;
    let identity = json!({
        "fixture": "comm-rx-prefix-v1", "family": args[0], "prefix_bytes": prefix,
        "nonce": args[2], "pid": std::process::id(),
        "source_sha256": env!("XADBTEST_SOURCE_SHA256"),
        "profile": env!("XADBTEST_PROFILE"), "target": env!("XADBTEST_TARGET"),
    });
    let mut ready = identity.clone();
    ready["event"] = json!("ready");
    ready["tcp"] = json!(endpoint.socket_addr().map(|a| a.to_string()));
    ready["unix"] = json!(endpoint.unix_path().map(|p| p.to_string_lossy()));
    emit(ready)?;

    let end = Instant::now() + Duration::from_secs(12);
    let mut reported = false;
    while Instant::now() < end {
        if reported {
            // Freeze the observed prefix while the controller records and kills
            // this exact process. No terminal or cleanup success is fabricated.
            std::thread::sleep(Duration::from_millis(5));
            continue;
        }
        driver.step(Duration::from_millis(1))?;
        let ids = driver.connections().collect::<Vec<_>>();
        if ids.len() > 1 {
            return Err("RX-prefix fixture accepts exactly one peer".into());
        }
        let Some(id) = ids.first() else { continue };
        let owner = driver.connection_mut(*id).ok_or("missing RX owner")?;
        if owner.phase() != ConnectionPhase::Open {
            return Err("connection terminated before RX-prefix observation".into());
        }
        let stats = owner.stats();
        if stats.read_bytes < prefix {
            continue;
        }
        if stats.read_bytes != prefix || stats.overflowed {
            return Err("RX prefix overshot or counters overflowed".into());
        }
        let payload = owner.take_received().map(|frame| frame.payload().to_vec());
        let mut receipt = identity.clone();
        receipt["event"] = json!("rx_prefix");
        receipt["phase"] = json!("Open");
        receipt["read_bytes"] = json!(stats.read_bytes);
        receipt["written_bytes"] = json!(stats.written_bytes);
        receipt["received_frames"] = json!(stats.received_frames);
        receipt["payload"] = json!(payload);
        receipt["overflowed"] = json!(stats.overflowed);
        receipt["registration"] = json!({
            "slot": owner.registration().slot, "generation": owner.registration().generation,
        });
        emit(receipt)?;
        reported = true;
    }
    drop(directory);
    Err("controller did not kill the fixture before its deadline".into())
}
