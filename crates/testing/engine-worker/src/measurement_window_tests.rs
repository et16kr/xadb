use super::*;
use std::io::BufRead;

#[test]
fn disabled_and_invalid_configuration_are_distinct() {
    assert!(config(None, None).unwrap().is_none());
    for (path, token) in [
        (Some("/tmp/socket"), None),
        (None, Some("0")),
        (Some("relative"), Some("00000000000000000000000000000000")),
        (
            Some("/tmp/socket"),
            Some("0000000000000000000000000000000G"),
        ),
    ] {
        assert!(config(path.map(str::to_owned), token.map(str::to_owned)).is_err());
    }
    let mut disabled = Window(None);
    disabled.before_warmup().unwrap();
    disabled.begin().unwrap();
    disabled.end(1).unwrap();
    assert_eq!(disabled.path("original"), "original");
}

#[test]
fn ordered_gate_brackets_actual_work_and_marks_diagnostic_path() {
    let (client, server) = UnixStream::pair().unwrap();
    client.set_nonblocking(true).unwrap();
    let peer = std::thread::spawn(move || {
        let mut peer = std::io::BufReader::new(server);
        let mut line = String::new();
        peer.read_line(&mut line).unwrap();
        assert_eq!(line, "START test\n");
        peer.get_mut().write_all(b"GO test RUSAGE_ONLY\n").unwrap();
        line.clear();
        peer.read_line(&mut line).unwrap();
        let stop: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(stop["phase"], "STOP");
        assert_eq!(stop["elapsed_ns"], 1);
        assert!(stop["end_ns"].as_u64().unwrap() >= stop["start_ns"].as_u64().unwrap());
        assert_eq!(stop["before"].as_array().unwrap().len(), 6);
        peer.get_mut()
            .write_all(b"DONE test RUSAGE_ONLY\n")
            .unwrap();
    });
    let mut window = Window(Some(Gate {
        stream: client,
        token: "test".into(),
        mode: "RUSAGE_ONLY",
        boundary: Boundary::Measurement,
        armed_ns: None,
        start: None,
        finished: false,
    }));
    assert_eq!(window.path("original"), "original.diagnostic_window.v1");
    assert!(window.end(1).is_err());
    window.begin().unwrap();
    assert!(window.begin().is_err());
    window.end(1).unwrap();
    assert!(window.begin().is_err());
    peer.join().unwrap();
}

#[test]
fn bad_ack_token_eof_and_oversize_fail_closed() {
    for reply in [
        b"GO wrong PMU\n".to_vec(),
        b"READY test PMU\n".to_vec(),
        vec![],
        vec![b'x'; 513],
    ] {
        let (mut client, mut peer) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        peer.write_all(&reply).unwrap();
        peer.shutdown(std::net::Shutdown::Write).unwrap();
        assert!(
            exchange(
                &mut client,
                "START test",
                &["GO test PMU".into()],
                Duration::from_millis(100)
            )
            .is_err()
        );
    }
}

#[test]
fn partial_bytes_do_not_extend_absolute_exchange_deadline() {
    let (mut client, mut peer) = UnixStream::pair().unwrap();
    client.set_nonblocking(true).unwrap();
    let writer = std::thread::spawn(move || {
        for _ in 0..20 {
            if peer.write_all(b"x").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let start = Instant::now();
    assert!(
        exchange(
            &mut client,
            "START test",
            &["GO test PMU".into()],
            Duration::from_millis(40)
        )
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_millis(180));
    drop(client);
    writer.join().unwrap();
}

#[test]
fn prewarmup_requires_explicit_complete_k0_configuration() {
    assert!(Boundary::parse(None, "durable", false).is_ok());
    for name in ["prewarmup-continuous-v1", "prewarmup-wait-v1"] {
        assert!(Boundary::parse(Some(name), "k0", true).is_ok());
        assert!(Boundary::parse(Some(name), "k0", false).is_err());
        assert!(Boundary::parse(Some(name), "durable", true).is_err());
    }
    for name in ["", "measurement", "unknown"] {
        assert!(Boundary::parse(Some(name), "k0", true).is_err());
    }
}

#[test]
fn prewarmup_modes_arm_once_and_only_control_waits_after_warmup() {
    for boundary in [Boundary::Continuous, Boundary::Wait] {
        let (client, server) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        let peer = std::thread::spawn(move || {
            let mut peer = std::io::BufReader::new(server);
            let mut line = String::new();
            peer.read_line(&mut line).unwrap();
            assert_eq!(line, "ARM test\n");
            peer.get_mut()
                .write_all(b"ARMED test RUSAGE_ONLY\n")
                .unwrap();
            if boundary == Boundary::Wait {
                line.clear();
                peer.read_line(&mut line).unwrap();
                assert_eq!(line, "START test\n");
                peer.get_mut().write_all(b"GO test RUSAGE_ONLY\n").unwrap();
            }
            // Continuous must send STOP next, with no post-warmup peer reply.
            line.clear();
            peer.read_line(&mut line).unwrap();
            let stop: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(stop["phase"], "STOP");
            assert!(stop["armed_ns"].as_u64().unwrap() <= stop["start_ns"].as_u64().unwrap());
            assert!(stop["start_ns"].as_u64().unwrap() <= stop["end_ns"].as_u64().unwrap());
            peer.get_mut()
                .write_all(b"DONE test RUSAGE_ONLY\n")
                .unwrap();
        });
        let mut window = Window(Some(Gate {
            stream: client,
            token: "test".into(),
            mode: "RUSAGE_ONLY",
            boundary,
            armed_ns: None,
            start: None,
            finished: false,
        }));
        assert!(window.begin().is_err());
        window.before_warmup().unwrap();
        assert!(window.before_warmup().is_err());
        window.begin().unwrap();
        assert!(window.begin().is_err());
        window.end(1).unwrap();
        assert!(window.before_warmup().is_err());
        assert!(window.begin().is_err());
        assert_eq!(
            window.path("original"),
            format!("original{}", boundary.suffix())
        );
        peer.join().unwrap();
    }
}

#[test]
fn rejected_arm_cannot_enter_measurement() {
    let (client, mut peer) = UnixStream::pair().unwrap();
    client.set_nonblocking(true).unwrap();
    peer.write_all(b"ARMED wrong RUSAGE_ONLY\n").unwrap();
    let mut window = Window(Some(Gate {
        stream: client,
        token: "test".into(),
        mode: "RUSAGE_ONLY",
        boundary: Boundary::Continuous,
        armed_ns: None,
        start: None,
        finished: false,
    }));
    assert!(window.before_warmup().is_err());
    assert!(window.0.as_ref().unwrap().armed_ns.is_none());
    assert!(window.begin().is_err());
}
