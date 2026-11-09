//! Daemon-level smoke: ordinary TCP bootstrap, automatic address gossip discovery,
//! then block synchronization and a signed transaction over the Libp2p session.
use crate::common::daemon::Daemon;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspad_lib::args::Args;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn libp2p_startup_failures_stop_the_daemon_and_release_listeners() {
    use std::sync::atomic::Ordering;

    for failure in ["identity", "quic"] {
        let args = Args { simnet: true, disable_upnp: true, disable_dns_seeding: true, outbound_target: 0, ..Default::default() };
        let mut server = Daemon::new_random_with_args(args, 10);
        let port = server.p2p_port;
        let relay_port = server.args.read().relay_port.unwrap();
        let occupied_udp =
            (failure == "quic").then(|| std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, relay_port)).unwrap());
        if failure == "identity" {
            let args = server.args.read();
            let path = std::path::Path::new(args.appdir.as_ref().unwrap()).join(args.network().to_prefixed()).join("libp2p.secret");
            std::fs::write(path, [1; 31]).unwrap();
        }
        server.run();
        let stopped = tokio::time::timeout(Duration::from_secs(10), async {
            while server.core.keep_running.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        server.shutdown();
        stopped.unwrap_or_else(|_| panic!("{failure} startup failure did not stop the daemon"));
        assert!(std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).is_ok());
        drop(occupied_udp);
        assert!(std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, relay_port)).is_ok());
        assert!(std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, relay_port)).is_ok());
    }
}

#[test]
fn legacy_address_database_migrates_during_daemon_initialization() {
    use kaspa_addressmanager::DbAddressesStore;
    use kaspa_consensus::consensus::factory::{LATEST_DB_VERSION, MultiConsensusManagementStore};
    use kaspa_database::{
        prelude::{CachePolicy, ConnBuilder, DbKey},
        registry::DatabaseStorePrefixes,
        utils::get_kaspa_tempdir,
    };
    use kaspa_utils::networking::NetAddress;

    let directory = get_kaspa_tempdir();
    let mut args = Args {
        appdir: Some(directory.path().to_str().unwrap().to_owned()),
        simnet: true,
        yes: true,
        disable_upnp: true,
        disable_dns_seeding: true,
        outbound_target: 0,
        ..Default::default()
    };
    Daemon::fill_args_with_random_ports(&mut args);
    let path = directory.path().join(args.network().to_prefixed()).join("datadir").join("meta");
    std::fs::create_dir_all(&path).unwrap();
    let tcp: NetAddress = "8.8.8.8:16111".parse().unwrap();
    {
        let db = ConnBuilder::default().with_db_path(path.clone()).with_files_limit(10).build().unwrap();
        MultiConsensusManagementStore::new(db.clone()).set_version(7).unwrap();
        let mut key = std::net::Ipv4Addr::new(8, 8, 8, 8).to_ipv6_mapped().octets().to_vec();
        key.extend(tcp.port.to_le_bytes());
        // Bincode tuples and structs have the same field layout. This fixture
        // is the released LegacyEntry, before Libp2p metadata was introduced.
        db.put(DbKey::new(DatabaseStorePrefixes::AddressesLegacy.as_ref(), key), bincode::serialize(&(2u64, tcp)).unwrap()).unwrap();
    }
    let (core, rpc) = kaspad_lib::daemon::create_core_with_runtime(&Default::default(), &args, 10);
    // Database upgrades run during construction, before services start.
    drop(rpc);
    core.join(Vec::new());
    drop(core);
    let db = ConnBuilder::default().with_db_path(path).with_files_limit(10).build().unwrap();
    assert_eq!(LATEST_DB_VERSION, 8);
    assert_eq!(MultiConsensusManagementStore::new_readonly(db.clone()).version().unwrap(), 8);
    assert!(
        db.iterator(rocksdb::IteratorMode::Start)
            .all(|row| !row.unwrap().0.starts_with(DatabaseStorePrefixes::AddressesLegacy.as_ref()))
    );
    let rows = DbAddressesStore::new(db, CachePolicy::Empty).iterator().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, (tcp, None));
    assert_eq!(rows[0].1.connection_failed_count, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn protocol_violations_disconnect_without_banning_and_rpc_bans_both_transports() {
    use kaspa_p2p_lib::{
        Adaptor, ConnectionInitializer, Hub, KaspadHandshake, KaspadMessagePayloadType, Router,
        common::ProtocolError,
        convert::model::version::Version,
        libp2p_transport::{Libp2pTransport, load_identity},
        make_message,
        pb::{self, kaspad_message::Payload},
    };
    use kaspa_utils::{
        connection::{EndpointAddress, Libp2pMetadata, PeerAddress},
        networking::PeerId,
    };
    use std::sync::Arc;

    struct Initializer {
        advertised: parking_lot::RwLock<Option<PeerAddress>>,
    }

    #[async_trait::async_trait]
    impl ConnectionInitializer for Initializer {
        async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
            let mut handshake = KaspadHandshake::new(&router);
            router.start();
            let version = Version::new(
                self.advertised.read().clone(),
                PeerId::from_slice(&rand::random::<[u8; 16]>()).unwrap(),
                "kaspa-simnet".into(),
                None,
                11,
            );
            let peer: Version = handshake.handshake(version.into()).await?.try_into()?;
            router.set_identity(peer.id);
            let mut incoming = router.subscribe(vec![
                KaspadMessagePayloadType::RequestAddresses,
                KaspadMessagePayloadType::Addresses,
                KaspadMessagePayloadType::Ping,
                KaspadMessagePayloadType::Pong,
            ]);
            handshake.exchange_ready_messages().await?;
            tokio::spawn(async move {
                while let Some(message) = incoming.recv().await {
                    let response = match message.payload {
                        Some(Payload::RequestAddresses(_)) => Some(make_message!(Payload::Addresses, pb::AddressesMessage::default())),
                        Some(Payload::Ping(ping)) => Some(make_message!(Payload::Pong, pb::PongMessage { nonce: ping.nonce })),
                        _ => None,
                    };
                    if let Some(response) = response {
                        if router.enqueue(response).await.is_err() {
                            break;
                        }
                    }
                }
            });
            Ok(())
        }
    }

    for use_libp2p in [false, true] {
        let args = Args {
            simnet: true,
            unsafe_rpc: true,
            disable_upnp: true,
            disable_dns_seeding: true,
            outbound_target: 0,
            ..Default::default()
        };
        let mut server = Daemon::new_random_with_args(args.clone(), 10);
        let rpc = server.start().await;
        let identity_path = {
            let args = server.args.read();
            std::path::Path::new(args.appdir.as_ref().unwrap()).join(args.network().to_prefixed()).join("libp2p.secret")
        };
        let (_, id) = load_identity(identity_path).await.unwrap();
        // These explicit routes are only for injecting a protocol violation.
        // Automatic discovery has its own separate daemon/NAT tests.
        let mut endpoint = EndpointAddress { id, relay: None };
        let mut relay_node = None;
        let mut relay_client = None;
        if use_libp2p {
            let mut relay_args = args.clone();
            Daemon::fill_args_with_random_ports(&mut relay_args);
            let relay_port = relay_args.listen.unwrap().normalize(0).port;
            relay_args.externalip = Some(format!("127.0.0.2:{relay_port}").parse().unwrap());
            let mut relay = Daemon::with_manager(Arc::new(crate::common::daemon::ClientManager::new(relay_args)), 10);
            let relay_rpc = relay.start().await;
            rpc.add_peer(format!("127.0.0.2:{relay_port}").parse().unwrap(), false).await.unwrap();
            wait(
                async || {
                    let session = AddressSession::connect(server.p2p_port, None).await;
                    if let Some(route) =
                        session.advertised.and_then(|address| address.endpoint()).filter(|route| route.relay.is_some())
                    {
                        endpoint = route;
                        true
                    } else {
                        false
                    }
                },
                20,
            )
            .await;
            // Keep bootstrap on a different IP so banning the libp2p peer does not remove its relay route.
            relay_client = Some(relay_rpc);
            relay_node = Some(relay);
        }
        let directory = tempfile::tempdir().unwrap();
        let mut ports = args;
        Daemon::fill_args_with_random_ports(&mut ports);
        let (key, _) = load_identity(directory.path().join("client.key")).await.unwrap();
        let transport = Arc::new(Libp2pTransport::new(key, ports.relay_port.unwrap(), true).unwrap());
        let initializer = Arc::new(Initializer { advertised: Default::default() });
        let client = Adaptor::client_only(Hub::new(), initializer.clone(), Default::default(), transport.clone());
        client.start_libp2p(std::net::Ipv4Addr::LOCALHOST.into()).unwrap();
        let tcp = format!("127.0.0.1:{}", server.p2p_port);
        let mut advertised = PeerAddress {
            ip: std::net::Ipv4Addr::LOCALHOST.into(),
            tcp_port: 30000,
            libp2p: Some(Libp2pMetadata {
                endpoint_id: transport.id(),
                port: transport.relay_port(),
                relay_port: transport.relay_port(),
                nonce: 1,
                signature: Vec::new(),
            }),
            relay_address: None,
        };
        advertised.libp2p.as_mut().unwrap().signature = transport.sign_address(&advertised, "kaspa-simnet").unwrap();
        if use_libp2p {
            let (other_key, other_id) = load_identity(directory.path().join("other.key")).await.unwrap();
            let mut mismatched = advertised.clone();
            mismatched.libp2p.as_mut().unwrap().endpoint_id = other_id;
            mismatched.libp2p.as_mut().unwrap().signature = other_key.sign(&mismatched.signing_bytes("kaspa-simnet")).unwrap();
            assert!(mismatched.verify("kaspa-simnet"));
            *initializer.advertised.write() = Some(mismatched);
            assert!(
                client.connect_libp2p_peer(&endpoint).await.is_err(),
                "another owner's valid signature must not bypass transport identity"
            );
            assert_eq!(client.active_peers_len(), 0);
            // Missing metadata is allowed on the next handshake.
            *initializer.advertised.write() = None;
        }
        let peer = if use_libp2p {
            client.connect_libp2p_peer(&endpoint).await.unwrap()
        } else {
            client.connect_peer(tcp.clone()).await.unwrap()
        };
        wait(
            async || {
                rpc.get_connected_peer_info()
                    .await
                    .unwrap()
                    .peer_info
                    .iter()
                    .filter(|peer| peer.libp2p_id.is_some() == use_libp2p)
                    .count()
                    == 1
            },
            10,
        )
        .await;
        let peers = rpc.get_connected_peer_info().await.unwrap().peer_info;
        let reported = peers.iter().find(|peer| peer.libp2p_id.is_some() == use_libp2p).unwrap();
        assert_eq!(reported.libp2p_id, use_libp2p.then(|| transport.id()));
        assert!(reported.address.ip.is_loopback());
        if use_libp2p {
            assert_eq!(reported.address.port, transport.relay_port(), "RPC must report the connected UDP socket");
        }
        // Match master's policy: a repeated Ready disconnects without banning.
        client.send(peer, make_message!(Payload::Ready, pb::ReadyMessage {})).await.unwrap();
        let ip: kaspa_rpc_core::RpcIpAddress = std::net::Ipv4Addr::LOCALHOST.into();
        wait(
            async || {
                rpc.get_connected_peer_info()
                    .await
                    .unwrap()
                    .peer_info
                    .iter()
                    .filter(|peer| peer.libp2p_id.is_some() == use_libp2p)
                    .count()
                    == 0
            },
            10,
        )
        .await;
        // The server has disconnected. Release the test client's old router
        // before testing a fresh dial; remote TCP EOF need not arrive immediately.
        client.terminate(peer).await;
        wait(async || client.active_peers_len() == 0, 10).await;
        assert!(rpc.get_peer_addresses().await.unwrap().banned_addresses.is_empty());
        // Matching metadata is accepted as well; TCP does not require transport authentication.
        *initializer.advertised.write() = Some(advertised);
        let peer = if use_libp2p {
            client.connect_libp2p_peer(&endpoint).await.unwrap()
        } else {
            client.connect_peer(tcp.clone()).await.unwrap()
        };
        wait(
            async || {
                rpc.get_connected_peer_info()
                    .await
                    .unwrap()
                    .peer_info
                    .iter()
                    .filter(|peer| peer.libp2p_id.is_some() == use_libp2p)
                    .count()
                    == 1
            },
            10,
        )
        .await;

        rpc.ban(ip).await.unwrap();
        assert!(rpc.get_peer_addresses().await.unwrap().banned_addresses.contains(&ip));
        wait(
            async || {
                rpc.get_connected_peer_info()
                    .await
                    .unwrap()
                    .peer_info
                    .iter()
                    .filter(|peer| peer.libp2p_id.is_some() == use_libp2p)
                    .count()
                    == 0
            },
            10,
        )
        .await;
        client.terminate(peer).await;
        wait(async || client.active_peers_len() == 0, 10).await;
        assert!(client.connect_peer(tcp).await.is_err(), "the shared ban must reject TCP reconnection");
        assert!(client.connect_libp2p_peer(&endpoint).await.is_err(), "the shared ban must reject libp2p reconnection");
        client.close().await;
        rpc.disconnect().await.unwrap();
        server.shutdown();
        if let Some(relay_rpc) = relay_client {
            relay_rpc.disconnect().await.unwrap();
        }
        if let Some(mut relay) = relay_node {
            relay.shutdown();
        }
    }
}

async fn wait(mut predicate: impl AsyncFnMut() -> bool, seconds: u64) {
    tokio::time::timeout(Duration::from_secs(seconds), async {
        while !predicate().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Libp2p daemon smoke condition timed out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn automatic_libp2p_discovery_and_sync_smoke() {
    kaspa_core::log::try_init_logger("info,libp2p=warn,noq=warn,libp2p_relay=error");
    // Use a real local interface because the address manager intentionally excludes loopback.
    let ip = local_ip_address::list_afinet_netifas()
        .unwrap()
        .into_iter()
        .map(|(_, ip)| ip)
        .find(|ip| ip.is_ipv4() && !ip.is_loopback())
        .expect("local IPv4 interface for TCP bootstrap");
    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true,
        disable_dns_seeding: true,
        utxoindex: true,
        outbound_target: 2,
        ..Default::default()
    };
    let mut relay = Daemon::new_random_with_args(Args { outbound_target: 0, ..args.clone() }, 10);
    // The advertised interface IP cannot reach these loopback-bound listeners.
    // TCP must fail, and libp2p dialing must use the gossiped relay route to
    // discover the actual local UDP candidate. This is local coordination,
    // not a claim that this host test proves Internet NAT traversal.
    let private_node = |mut args: Args| {
        Daemon::fill_args_with_random_ports(&mut args);
        let port = args.listen.unwrap().normalize(0).port;
        args.listen = Some(format!("127.0.0.1:{port}").parse().unwrap());
        args.externalip = Some(format!("{ip}:{port}").parse().unwrap());
        Daemon::with_manager(std::sync::Arc::new(crate::common::daemon::ClientManager::new(args)), 10)
    };
    let mut a = private_node(args.clone());
    let mut b = private_node(args);
    let r_client = relay.start().await;
    let a_client = a.start().await;
    let b_client = b.start().await;
    let bootstrap = format!("{ip}:{}", relay.p2p_port);
    a_client.add_peer(bootstrap.parse().unwrap(), false).await.unwrap();
    // Respect the existing per-IP pending handshake budget while starting the topology.
    wait(async || a_client.get_connected_peer_info().await.unwrap().peer_info.len() == 1, 15).await;
    b_client.add_peer(bootstrap.parse().unwrap(), false).await.unwrap();
    wait(async || a_client.get_connected_peer_info().await.unwrap().peer_info.iter().any(|peer| peer.libp2p_id.is_some()), 200).await;
    wait(async || b_client.get_connected_peer_info().await.unwrap().peer_info.iter().any(|peer| peer.libp2p_id.is_some()), 30).await;
    // A successful outbound TCP socket is retained independently of its signed advertisement.
    let mut observer = AddressSession::connect(a.p2p_port, None).await;
    let tcp_record = kaspa_utils::connection::PeerAddress::from(bootstrap.parse::<kaspa_utils::networking::NetAddress>().unwrap());
    assert!(observer.addresses().await.contains(&tcp_record));
    drop(observer);
    // Stop the bootstrap node, leaving only the automatically discovered A/B Libp2p session.
    r_client.disconnect().await.unwrap();
    relay.shutdown();
    wait(async || a_client.get_connected_peer_info().await.unwrap().peer_info.iter().all(|peer| peer.libp2p_id.is_some()), 10).await;
    crate::common::libp2p::exchange(&a_client, &b_client, Duration::from_secs(30)).await;
    // Ban the authenticated connection's real direct source IP. The connected
    // peer is reported by identity, so this also checks the transport/IP boundary.
    b_client.ban(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST).into()).await.unwrap();
    wait(async || b_client.get_connected_peer_info().await.unwrap().peer_info.is_empty(), 10).await;
    // Wake automatic dialing through an ordinary request, then cover a complete
    // 30-second manager tick. Outgoing libp2p sessions must honor the same ban.
    b_client.add_peer(bootstrap.parse().unwrap(), false).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    while tokio::time::Instant::now() < deadline {
        assert!(b_client.get_connected_peer_info().await.unwrap().peer_info.is_empty(), "automatic dialing reconnected a banned IP");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    a_client.disconnect().await.unwrap();
    b_client.disconnect().await.unwrap();
    a.shutdown();
    b.shutdown();
}

struct AddressSession {
    sender: tokio::sync::mpsc::Sender<kaspa_p2p_lib::pb::KaspadMessage>,
    incoming: tonic::Streaming<kaspa_p2p_lib::pb::KaspadMessage>,
    advertised: Option<kaspa_utils::connection::PeerAddress>,
}

impl AddressSession {
    async fn connect(port: u16, address: Option<kaspa_utils::connection::PeerAddress>) -> Self {
        use kaspa_p2p_lib::{
            convert::model::version::Version,
            make_message,
            pb::{self, kaspad_message::Payload, p2p_client::P2pClient},
        };
        let mut client = P2pClient::connect(format!("http://127.0.0.1:{port}")).await.unwrap();
        let (sender, receiver) = tokio::sync::mpsc::channel(16);
        let incoming = client.message_stream(tokio_stream::wrappers::ReceiverStream::new(receiver)).await.unwrap().into_inner();
        let version = Version::new(
            address,
            kaspa_utils::networking::PeerId::from_slice(&rand::random::<[u8; 16]>()).unwrap(),
            "kaspa-simnet".into(),
            None,
            11,
        );
        sender.send(make_message!(Payload::Version, version.into())).await.unwrap();
        let mut session = Self { sender, incoming, advertised: None };
        loop {
            let payload = tokio::time::timeout(Duration::from_secs(10), session.incoming.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .payload
                .unwrap();
            match payload {
                Payload::Version(version) => {
                    session.advertised = Version::try_from(version).unwrap().address;
                    session.sender.send(make_message!(Payload::Verack, pb::VerackMessage {})).await.unwrap();
                }
                Payload::Verack(_) => session.sender.send(make_message!(Payload::Ready, pb::ReadyMessage {})).await.unwrap(),
                Payload::Ready(_) => break,
                _ => {}
            }
        }
        session
    }

    async fn addresses(&mut self) -> Vec<kaspa_utils::connection::PeerAddress> {
        use kaspa_p2p_lib::{
            make_message,
            pb::{self, kaspad_message::Payload},
        };
        self.sender.send(make_message!(Payload::RequestAddresses, pb::RequestAddressesMessage::default())).await.unwrap();
        loop {
            let payload = tokio::time::timeout(Duration::from_secs(15), self.incoming.message())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .payload
                .unwrap();
            match payload {
                Payload::Addresses(message) => {
                    return message.address_list.into_iter().map(TryInto::try_into).collect::<Result<_, _>>().unwrap();
                }
                Payload::RequestAddresses(_) => {
                    self.sender.send(make_message!(Payload::Addresses, pb::AddressesMessage::default())).await.unwrap()
                }
                Payload::Ping(ping) => {
                    self.sender.send(make_message!(Payload::Pong, pb::PongMessage { nonce: ping.nonce })).await.unwrap()
                }
                _ => {}
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gossip_accepts_fresh_unverified_addresses_and_repairs_invalid_records() {
    use kaspa_p2p_lib::{
        libp2p_transport::load_identity,
        make_message,
        pb::{self, kaspad_message::Payload},
    };
    use kaspa_utils::{
        connection::{Libp2pMetadata, PeerAddress},
        networking::NetAddress,
    };

    async fn gossip_and_expect(port: u16, address: PeerAddress) {
        let mut publisher = AddressSession::connect(port, None).await;
        loop {
            let message = tokio::time::timeout(Duration::from_secs(10), publisher.incoming.message()).await.unwrap().unwrap().unwrap();
            if matches!(message.payload, Some(Payload::RequestAddresses(_))) {
                publisher
                    .sender
                    .send(make_message!(Payload::Addresses, pb::AddressesMessage { address_list: vec![address.clone().into()] }))
                    .await
                    .unwrap();
                break;
            }
        }
        wait(
            async || {
                let mut observer = AddressSession::connect(port, None).await;
                observer.addresses().await.contains(&address)
            },
            10,
        )
        .await;
    }

    let args = Args { simnet: true, disable_upnp: true, disable_dns_seeding: true, outbound_target: 0, ..Default::default() };
    let mut server = Daemon::new_random_with_args(args, 10);
    let rpc = server.start().await;
    let directory = tempfile::tempdir().unwrap();
    let (key, id) = load_identity(directory.path().join("owner.key")).await.unwrap();
    let mut valid = PeerAddress {
        libp2p: Some(Libp2pMetadata { endpoint_id: id, port: 30000, relay_port: 0, nonce: 1, signature: Vec::new() }),
        ..PeerAddress::from("8.8.8.8:16111".parse::<NetAddress>().unwrap())
    };
    let signature = key.sign(&valid.signing_bytes("kaspa-simnet")).unwrap();
    valid.libp2p.as_mut().unwrap().signature = signature;
    let mut invalid = valid.clone();
    invalid.libp2p.as_mut().unwrap().port = 30001;
    invalid.libp2p.as_mut().unwrap().nonce = u64::MAX;
    assert!(!invalid.verify("kaspa-simnet"));
    gossip_and_expect(server.p2p_port, invalid).await;
    gossip_and_expect(server.p2p_port, valid).await;
    rpc.disconnect().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_handshake_records_update_by_nonce_and_gossip_unchanged() {
    use kaspa_p2p_lib::libp2p_transport::load_identity;
    use kaspa_utils::{
        connection::{Libp2pMetadata, PeerAddress},
        networking::NetAddress,
    };
    let args = Args {
        simnet: true,
        externalip: Some("8.8.4.4:16111".parse().unwrap()),
        disable_upnp: true,
        disable_dns_seeding: true,
        outbound_target: 0,
        ..Default::default()
    };
    let mut server = Daemon::new_random_with_args(args, 10);
    let rpc = server.start().await;
    let directory = tempfile::tempdir().unwrap();
    let (key, id) = load_identity(directory.path().join("owner.key")).await.unwrap();
    let mut original = PeerAddress {
        libp2p: Some(Libp2pMetadata { endpoint_id: id, port: 30000, relay_port: 0, nonce: 1, signature: Vec::new() }),
        ..PeerAddress::from("8.8.8.8:16111".parse::<NetAddress>().unwrap())
    };
    let signature = key.sign(&original.signing_bytes("kaspa-simnet")).unwrap();
    original.libp2p.as_mut().unwrap().signature = signature;
    // Fresh handshake addresses follow the same optimistic policy as fresh gossip.
    let mut unverified = original.clone();
    unverified.libp2p.as_mut().unwrap().port = 30009;
    unverified.libp2p.as_mut().unwrap().nonce = u64::MAX;
    assert!(!unverified.verify("kaspa-simnet"));
    let mut first = AddressSession::connect(server.p2p_port, Some(unverified.clone())).await;
    let own = first.advertised.as_ref().unwrap();
    assert_eq!(own.ip.to_string(), "8.8.4.4", "the explicit advertised IP must not be replaced by a local or observed socket");
    assert_eq!(own.tcp_port, 16111);
    assert!(own.verify("kaspa-simnet"));
    let received = first.addresses().await;
    assert!(received.contains(&unverified));
    // A valid owner record repairs an invalid entry even with a lower nonce.
    let mut repair = AddressSession::connect(server.p2p_port, Some(original.clone())).await;
    let received = repair.addresses().await;
    assert!(received.contains(&original));
    assert!(!received.contains(&unverified));
    assert!(received.iter().all(|record| record.verify("kaspa-simnet")));
    let mut fresh = original.clone();
    fresh.libp2p.as_mut().unwrap().nonce = 2;
    fresh.libp2p.as_mut().unwrap().port = 30001;
    let signature = key.sign(&fresh.signing_bytes("kaspa-simnet")).unwrap();
    fresh.libp2p.as_mut().unwrap().signature = signature;
    let mut second = AddressSession::connect(server.p2p_port, Some(fresh.clone())).await;
    let received = second.addresses().await;
    assert!(received.contains(&fresh));
    assert!(!received.contains(&original));
    // Replaying a valid older advertisement cannot roll the address back.
    let mut replay = AddressSession::connect(server.p2p_port, Some(original)).await;
    assert!(replay.addresses().await.contains(&fresh));
    // Signed metadata is sent even when the peer advertises no libp2p service flag.
    let mut observer = AddressSession::connect(server.p2p_port, None).await;
    assert!(observer.addresses().await.contains(&fresh));
    // An invalid competing update is ignored by the address manager without rejecting the session.
    let mut forged = fresh.clone();
    forged.libp2p.as_mut().unwrap().port = 30002;
    forged.libp2p.as_mut().unwrap().nonce = 3;
    let mut invalid = AddressSession::connect(server.p2p_port, Some(forged.clone())).await;
    let received = invalid.addresses().await;
    assert!(received.contains(&fresh));
    assert!(!received.contains(&forged));
    drop((first, repair, second, replay, observer, invalid));
    rpc.disconnect().await.unwrap();
    server.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signed_relay_is_learned_through_handshake_and_reservation_is_gossiped() {
    let args = Args { simnet: true, disable_upnp: true, disable_dns_seeding: true, outbound_target: 0, ..Default::default() };
    let mut relay = Daemon::new_random_with_args(args.clone(), 10);
    let mut client = Daemon::new_random_with_args(args, 10);
    let relay_rpc = relay.start().await;
    let client_rpc = client.start().await;
    let mut relay_session = AddressSession::connect(relay.p2p_port, None).await;
    let relay_record = relay_session.addresses().await.into_iter().find(|record| record.libp2p.is_some()).unwrap();
    assert!(relay_record.verify("kaspa-simnet"));
    let relay_socket = std::net::SocketAddr::new(relay_record.ip.into(), relay_record.relay_port().unwrap());
    let mut session = AddressSession::connect(client.p2p_port, Some(relay_record.clone())).await;
    let before = session.advertised.clone().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let reserved = loop {
        let addresses = session.addresses().await;
        assert!(addresses.contains(&relay_record));
        assert!(addresses.iter().all(|address| address.verify("kaspa-simnet")));
        if let Some(record) = addresses.into_iter().find(|record| {
            record.key() == before.key()
                && record.relay_address
                    == Some(kaspa_utils::connection::LibP2pAddress { address: relay_socket, id: relay_record.endpoint_id().unwrap() })
        }) {
            break record;
        }
        assert!(tokio::time::Instant::now() < deadline, "automatic relay registration did not become advertised");
        tokio::time::sleep(Duration::from_secs(11)).await;
    };
    assert_eq!(reserved.nonce(), before.nonce().map(|nonce| nonce + 1));
    drop((session, relay_session));
    // New handshakes and address replies read the exact same cached signature and nonce.
    for _ in 0..3 {
        let mut observer = AddressSession::connect(client.p2p_port, None).await;
        assert_eq!(observer.advertised.as_ref(), Some(&reserved));
        assert!(observer.addresses().await.contains(&reserved));
    }
    relay_rpc.disconnect().await.unwrap();
    relay.shutdown();
    wait(
        async || {
            let observer = AddressSession::connect(client.p2p_port, None).await;
            let record = observer.advertised.unwrap();
            if record.relay_address.is_some() {
                return false;
            }
            assert_eq!(record.key(), reserved.key());
            assert_eq!(record.nonce(), reserved.nonce().map(|nonce| nonce + 1));
            assert!(record.verify("kaspa-simnet"));
            true
        },
        30,
    )
    .await;
    client_rpc.disconnect().await.unwrap();
    client.shutdown();
}
