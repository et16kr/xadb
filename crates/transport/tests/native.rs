mod support;
use foundation::network::{Connector, Listener};
use platform::{
    network::{LocalEndpoint, LocalNetwork, UnixDirectory},
    runtime::MonotonicClock,
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    os::unix::net::UnixStream,
    sync::Arc,
    time::{Duration, Instant},
};
use support::frame;
use transport::{ConnectionPhase, Driver, SendOutcome, Transport};

fn exercise(endpoint: LocalEndpoint, network: LocalNetwork) {
    let listener = network.listen(endpoint).unwrap();
    let endpoint = listener.endpoint().clone();
    let t = Transport::new(Arc::new(MonotonicClock::default()));
    let mut driver = Driver::new(t.clone(), network).unwrap();
    driver.add_listener(Box::new(listener)).unwrap();
    let peers = (0..4)
        .map(|id| {
            let endpoint = endpoint.clone();
            std::thread::spawn(move || {
                let mut peer: Box<dyn ReadWrite> = if let Some(a) = endpoint.socket_addr() {
                    let s = TcpStream::connect(a).unwrap();
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    Box::new(s)
                } else {
                    let s = UnixStream::connect(endpoint.unix_path().unwrap()).unwrap();
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    Box::new(s)
                };
                for size in [0, 1, 2, 4096, 65536, 1_048_576] {
                    let payload = vec![id; size];
                    let wire = frame(&payload);
                    for chunk in wire.chunks(337) {
                        peer.write_all(chunk).unwrap();
                    }
                    let mut observed = vec![0; wire.len()];
                    peer.read_exact(&mut observed).unwrap();
                    assert_eq!(observed, wire);
                }
            })
        })
        .collect::<Vec<_>>();
    let end = Instant::now() + Duration::from_secs(15);
    let mut tickets = vec![];
    let mut received = 0;
    while peers.iter().any(|p| !p.is_finished()) || received < 24 {
        assert!(Instant::now() < end, "native peers did not finish");
        driver.step(Duration::from_millis(1)).unwrap();
        for id in driver.connections().collect::<Vec<_>>() {
            let c = driver.connection_mut(id).unwrap();
            while let Some(rx) = c.take_received() {
                received += 1;
                tickets.push(c.sender().try_send(rx.payload()).unwrap());
            }
        }
        tickets = tickets
            .into_iter()
            .filter_map(|ticket| match ticket.try_take() {
                Ok(result) => {
                    assert_eq!(result, SendOutcome::FlushedLocally);
                    None
                }
                Err(ticket) => Some(ticket),
            })
            .collect();
    }
    for p in peers {
        p.join().unwrap();
    }
    for _ in 0..5 {
        driver.step(Duration::from_millis(1)).unwrap();
    }
    drop(tickets);
    drop(driver);
    assert_eq!(t.payload_bytes(), 0);
    assert_eq!(t.connection_count(), 0);
}
trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}
#[test]
fn native_ipv4_ipv6_and_owned_unix_independent_peer_vectors() {
    exercise(
        LocalEndpoint::tcp("127.0.0.1:0".parse().unwrap()).unwrap(),
        LocalNetwork::default(),
    );
    exercise(
        LocalEndpoint::tcp("[::1]:0".parse().unwrap()).unwrap(),
        LocalNetwork::default(),
    );
    let tmp = tempfile::tempdir().unwrap();
    let dir = UnixDirectory::create(&tmp.path().join("private")).unwrap();
    let endpoint = LocalEndpoint::unix(&dir, "socket").unwrap();
    exercise(endpoint, LocalNetwork::default());
    assert!(!dir.path().join("socket").exists());
}
#[test]
fn native_pending_connect_without_tx_and_refusal() {
    let network = LocalNetwork::default();
    let mut listener = network
        .listen(LocalEndpoint::tcp("127.0.0.1:0".parse().unwrap()).unwrap())
        .unwrap();
    let endpoint = listener.endpoint().clone();
    let t = Transport::new(Arc::new(MonotonicClock::default()));
    let mut driver = Driver::new(t.clone(), network.clone()).unwrap();
    let c = t.connect(&mut network.connector(endpoint.clone())).unwrap();
    let id = driver.add(c).unwrap();
    let end = Instant::now() + Duration::from_secs(2);
    while driver.connection(id).unwrap().phase() != ConnectionPhase::Open {
        assert!(Instant::now() < end);
        driver.step(Duration::from_millis(1)).unwrap();
    }
    let mut accepted = listener.try_accept().unwrap();
    accepted.close();
    listener.close();
    drop(driver);
    let c = t.connect(&mut network.connector(endpoint));
    match c {
        Err(e) => assert_eq!(e.kind, transport::ErrorKind::Io),
        Ok(c) => {
            let sender = c.sender();
            let mut d = Driver::new(t.clone(), network).unwrap();
            let id = d.add(c).unwrap();
            for _ in 0..10 {
                d.step(Duration::from_millis(1)).unwrap();
            }
            assert_eq!(d.connection(id).unwrap().phase(), ConnectionPhase::Closed);
            drop(sender);
        }
    }
    assert_eq!(t.connection_count(), 0);
}
#[test]
fn owned_unix_preserves_files_symlinks_and_replaced_socket_paths() {
    use std::os::unix::fs::symlink;
    let tmp = tempfile::tempdir().unwrap();
    let dir = UnixDirectory::create(&tmp.path().join("private")).unwrap();
    let network = LocalNetwork::default();
    assert!(UnixDirectory::create(dir.path()).is_err());
    let file = dir.path().join("file");
    std::fs::write(&file, b"keep").unwrap();
    let endpoint = LocalEndpoint::unix(&dir, "file").unwrap();
    assert!(network.listen(endpoint.clone()).is_err());
    assert!(network.connector(endpoint).connect().is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"keep");
    symlink(&file, dir.path().join("link")).unwrap();
    assert!(
        network
            .listen(LocalEndpoint::unix(&dir, "link").unwrap())
            .is_err()
    );
    let endpoint = LocalEndpoint::unix(&dir, "socket").unwrap();
    let mut listener = network.listen(endpoint.clone()).unwrap();
    assert!(network.listen(endpoint).is_err());
    std::fs::remove_file(dir.path().join("socket")).unwrap();
    std::fs::write(dir.path().join("socket"), b"replacement").unwrap();
    listener.close();
    listener.close();
    assert_eq!(
        std::fs::read(dir.path().join("socket")).unwrap(),
        b"replacement"
    );
    for address in ["0.0.0.0:0", "[::]:0", "192.0.2.1:1", "[::ffff:192.0.2.1]:1"] {
        assert!(LocalEndpoint::tcp(address.parse::<SocketAddr>().unwrap()).is_err());
    }
}
#[test]
fn native_buffered_frame_then_fin_is_delivered_before_eof() {
    let network = LocalNetwork::default();
    let mut listener = network
        .listen(LocalEndpoint::tcp("127.0.0.1:0".parse().unwrap()).unwrap())
        .unwrap();
    let mut peer = TcpStream::connect(listener.endpoint().socket_addr().unwrap()).unwrap();
    let stream = listener.try_accept().unwrap();
    let t = Transport::new(Arc::new(MonotonicClock::default()));
    let mut driver = Driver::new(t.clone(), network).unwrap();
    let c = t.accept(stream).unwrap();
    let sender = c.sender();
    let id = driver.add(c).unwrap();
    peer.write_all(&frame(&[1, 2])).unwrap();
    peer.shutdown(std::net::Shutdown::Write).unwrap();
    let mut observed = None;
    for _ in 0..20 {
        driver.step(Duration::from_millis(1)).unwrap();
        if let Some(rx) = driver.connection_mut(id).unwrap().take_received() {
            assert!(observed.is_none());
            observed = Some(rx);
        }
    }
    assert_eq!(observed.as_ref().unwrap().payload(), [1, 2]);
    assert!(driver.connection(id).unwrap().peer_write_closed());
    drop(driver);
    assert_eq!(t.payload_bytes(), 44);
    drop(observed);
    drop(sender);
    assert_eq!(t.payload_bytes(), 0);
    assert_eq!(t.connection_count(), 0);
}

#[test]
fn empty_native_wait_preserves_submillisecond_timeout() {
    use foundation::network::ReadinessWait;
    let mut network = LocalNetwork::default();
    let start = Instant::now();
    for _ in 0..10 {
        assert_eq!(
            network
                .wait(&[], Duration::from_micros(500), &mut [])
                .unwrap(),
            0
        );
    }
    assert!(start.elapsed() >= Duration::from_millis(5));
}
