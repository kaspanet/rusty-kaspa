use std::{
    net::{TcpListener, UdpSocket},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn p2p_startup_failures_exit_unsuccessfully_after_cleanup() {
    for failure in ["quic", "p2p"] {
        let directory = tempfile::tempdir().unwrap();
        let legacy = TcpListener::bind("127.0.0.1:0").unwrap();
        let quic_udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = legacy.local_addr().unwrap().port();
        let relay_port = quic_udp.local_addr().unwrap().port();
        let rpc = TcpListener::bind("127.0.0.1:0").unwrap();
        let rpc_port = rpc.local_addr().unwrap().port();
        let occupied_p2p = (failure == "p2p").then_some(legacy);
        let occupied_udp = (failure == "quic").then_some(quic_udp);
        drop(rpc);
        let output = directory.path().join("daemon.log");
        let log = std::fs::File::create(&output).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_kaspad"))
            .args(["--simnet", "--yes", "--disable-upnp", "--nodnsseed", "--outpeers=0", "--async-threads=2", "--ram-scale=0.1"])
            .arg(format!("--appdir={}", directory.path().display()))
            .arg(format!("--listen=127.0.0.1:{port}"))
            .arg(format!("--relay-port={relay_port}"))
            .arg(format!("--rpclisten=127.0.0.1:{rpc_port}"))
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{failure} startup failure did not stop the daemon: {}", std::fs::read_to_string(&output).unwrap());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = std::fs::read_to_string(output).unwrap();
        assert_eq!(status.code(), Some(1), "{failure}: {output}");
        assert!(output.contains("P2P startup failed:"), "{failure}: {output}");
        assert!(output.contains("Kaspad has stopped"), "{failure}: {output}");
        drop((occupied_p2p, occupied_udp));
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
        assert!(TcpListener::bind(("127.0.0.1", relay_port)).is_ok());
        assert!(UdpSocket::bind(("127.0.0.1", relay_port)).is_ok());
    }
}
