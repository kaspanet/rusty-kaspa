//! Local transport prototypes: relay limits and a stream after direct upgrade.
//! Loopback tests do not establish successful traversal of a real NAT.
use futures::StreamExt;
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder, dcutr, identify, noise, relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    yamux,
};
use std::time::Duration;

const DURATION: Duration = Duration::from_secs(10);
const BYTES: u64 = 1024 * 1024;

#[derive(NetworkBehaviour)]
struct Behaviour {
    relay_client: relay::client::Behaviour,
    relay_server: relay::Behaviour,
    identify: identify::Behaviour,
    dcutr: libp2p::swarm::behaviour::toggle::Toggle<dcutr::Behaviour>,
    streams: libp2p_stream::Behaviour,
}

fn node(punch: bool) -> Swarm<Behaviour> {
    SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_relay_client(noise::Config::new, yamux::Config::default)
        .unwrap()
        .with_behaviour(|key, relay_client| Behaviour {
            relay_client,
            relay_server: relay::Behaviour::new(
                key.public().to_peer_id(),
                relay::Config { max_circuit_duration: DURATION, max_circuit_bytes: BYTES, ..Default::default() },
            ),
            identify: identify::Behaviour::new(identify::Config::new("kaspa/prototype/1".into(), key.public())),
            dcutr: if punch { Some(dcutr::Behaviour::new(key.public().to_peer_id())) } else { None }.into(),
            streams: libp2p_stream::Behaviour::new(),
        })
        .unwrap()
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(120)))
        .build()
}

async fn next(nodes: &mut [Swarm<Behaviour>; 3]) -> (usize, SwarmEvent<BehaviourEvent>) {
    let [r, a, b] = nodes;
    tokio::select! {e=r.select_next_some()=>(0,e),e=a.select_next_some()=>(1,e),e=b.select_next_some()=>(2,e)}
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{AsyncReadExt, AsyncWriteExt};
    use libp2p::{multiaddr::Protocol, swarm::StreamProtocol};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relay_reservation_hole_punch_and_direct_stream() {
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut nodes = [node(true), node(true), node(true)];
            let ids: Vec<PeerId> = nodes.iter().map(|n| *n.local_peer_id()).collect();
            for n in &mut nodes {
                n.listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()).unwrap();
            }
            let mut addrs: Vec<Option<Multiaddr>> = vec![None; 3];
            while addrs.iter().any(Option::is_none) {
                let (i, e) = next(&mut nodes).await;
                if let SwarmEvent::NewListenAddr { address, .. } = e {
                    addrs[i] = Some(address);
                }
            }
            nodes[0].add_external_address(addrs[0].clone().unwrap());
            let relay = addrs[0].clone().unwrap().with(Protocol::P2p(ids[0]));
            nodes[1].dial(relay.clone()).unwrap();
            nodes[2].dial(relay.clone()).unwrap();
            let mut identified = [false; 3];
            while !identified[1] || !identified[2] {
                let (i, e) = next(&mut nodes).await;
                if matches!(e, SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { .. }))) {
                    identified[i] = true;
                }
            }
            nodes[2].listen_on(relay.clone().with(Protocol::P2pCircuit)).unwrap();
            loop {
                let (i, e) = next(&mut nodes).await;
                if i == 2
                    && matches!(
                        e,
                        SwarmEvent::Behaviour(BehaviourEvent::RelayClient(relay::client::Event::ReservationReqAccepted { .. }))
                    )
                {
                    break;
                }
            }
            nodes[1].dial(relay.with(Protocol::P2pCircuit).with(Protocol::P2p(ids[2]))).unwrap();
            let mut upgraded = [false; 3];
            let mut relayed = Vec::new();
            while !upgraded[1] || !upgraded[2] {
                let (i, e) = next(&mut nodes).await;
                match e {
                    SwarmEvent::ConnectionEstablished { peer_id, connection_id, endpoint, .. }
                        if (i == 1 && peer_id == ids[2]) || (i == 2 && peer_id == ids[1]) =>
                    {
                        if endpoint.is_relayed() {
                            relayed.push((i, connection_id));
                        } else {
                            upgraded[i] = true;
                        }
                    }
                    _ => {}
                }
            }
            assert!(!relayed.is_empty(), "initial connection must use circuit relay");
            let mut closing = relayed.len();
            for (i, id) in relayed {
                assert!(nodes[i].close_connection(id));
            }
            while closing > 0 {
                let (_, e) = next(&mut nodes).await;
                if let SwarmEvent::ConnectionClosed { endpoint, .. } = e
                    && endpoint.is_relayed()
                {
                    closing -= 1;
                }
            }
            let protocol = StreamProtocol::new("/kaspa/prototype/1");
            let mut incoming = nodes[2].behaviour().streams.new_control().accept(protocol.clone()).unwrap();
            let mut control = nodes[1].behaviour().streams.new_control();
            let io = async move {
                let mut outbound = control.open_stream(ids[2], protocol).await.unwrap();
                outbound.write_all(b"direct Kaspa stream probe").await.unwrap();
                outbound.flush().await.unwrap();
                let (peer, mut inbound) = incoming.next().await.unwrap();
                assert_eq!(peer, ids[1]);
                let mut data = [0; 25];
                inbound.read_exact(&mut data).await.unwrap();
                assert_eq!(&data, b"direct Kaspa stream probe");
            };
            tokio::pin!(io);
            loop {
                tokio::select! {_=&mut io=>break,(_, _)=next(&mut nodes)=>{}}
            }
        })
        .await
        .unwrap();
    }

    async fn circuit_limit(bytes: bool) {
        tokio::time::timeout(Duration::from_secs(25), async {
            let mut nodes = [node(false), node(false), node(false)];
            let ids: Vec<PeerId> = nodes.iter().map(|node| *node.local_peer_id()).collect();
            nodes[0].listen_on("/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()).unwrap();
            let relay = loop {
                let (index, event) = next(&mut nodes).await;
                if index == 0
                    && let SwarmEvent::NewListenAddr { address, .. } = event {
                        break address.with(Protocol::P2p(ids[0]));
                    }
            };
            nodes[0].add_external_address(relay.clone());
            nodes[2].listen_on(relay.clone().with(Protocol::P2pCircuit)).unwrap();
            loop {
                let (index, event) = next(&mut nodes).await;
                if index == 2 && matches!(event, SwarmEvent::Behaviour(BehaviourEvent::RelayClient(
                    relay::client::Event::ReservationReqAccepted { .. }
                ))) {
                    break;
                }
            }
            let started = std::time::Instant::now();
            nodes[1].dial(relay.with(Protocol::P2pCircuit).with(Protocol::P2p(ids[2]))).unwrap();
            loop {
                let (index, event) = next(&mut nodes).await;
                if index == 1 && matches!(event, SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == ids[2]) {
                    break;
                }
            }
            let protocol = StreamProtocol::new("/kaspa/limit-probe/1");
            let mut incoming = nodes[2].behaviour().streams.new_control().accept(protocol.clone()).unwrap();
            let mut control = nodes[1].behaviour().streams.new_control();
            let writer = async move {
                let mut stream = control.open_stream(ids[2], protocol).await.unwrap();
                if bytes {
                    let _ = stream.write_all(&vec![9; 2 * 1024 * 1024]).await;
                } else {
                    stream.write_all(b"start").await.unwrap();
                    stream.flush().await.unwrap();
                }
                futures::future::pending::<()>().await;
            };
            let reader = async move {
                let (_, mut stream) = incoming.next().await.unwrap();
                let _ = stream.read_to_end(&mut Vec::new()).await;
                futures::future::pending::<()>().await;
            };
            tokio::pin!(writer, reader);
            let closed = loop {
                tokio::select! {
                    _ = &mut writer => unreachable!(),
                    _ = &mut reader => unreachable!(),
                    (index, event) = next(&mut nodes) => {
                        if index == 0
                            && let SwarmEvent::Behaviour(BehaviourEvent::RelayServer(relay::Event::CircuitClosed { error, .. })) = event {
                                break error.unwrap();
                            }
                    }
                }
            };
            if bytes {
                assert!(closed.to_string().contains("Max circuit bytes"), "{closed}");
                assert!(started.elapsed() < DURATION);
            } else {
                assert_eq!(closed.kind(), std::io::ErrorKind::TimedOut);
                assert!(started.elapsed() >= DURATION);
            }
        }).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upstream_ten_second_circuit_limit() {
        circuit_limit(false).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn upstream_one_mib_circuit_limit() {
        circuit_limit(true).await;
    }
}
