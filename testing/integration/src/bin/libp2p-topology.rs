//! RPC assertions for scripts/libp2p-nat-test.sh. Endpoint IDs are assertion inputs
//! only; this program never dials, publishes, or supplies Libp2p addressing.
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::{api::rpc::RpcApi, notify::mode::NotificationMode};
use std::time::Duration;

async fn client(url: &str) -> GrpcClient {
    // The harness starts daemons asynchronously; wait for their RPC listeners.
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(client) = GrpcClient::connect_with_args(
                NotificationMode::Direct,
                url.to_owned(),
                None,
                false,
                None,
                false,
                None,
                Default::default(),
            )
            .await
            {
                break client;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("RPC listener at {url} was not ready within 30 seconds"))
}

async fn wait(mut condition: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(300), async {
        while !condition().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("topology assertion timed out");
}

#[tokio::main]
async fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert!(args.len() >= 3, "usage: libp2p-topology add|peer|exchange|blocked|ban|unban RPC ARG");
    let a = client(&args[1]).await;
    match args[0].as_str() {
        "add" => {
            a.add_peer(args[2].parse().unwrap(), false).await.unwrap();
        }
        "ban" => a.ban(args[2].parse().unwrap()).await.unwrap(),
        "unban" => a.unban(args[2].parse().unwrap()).await.unwrap(),
        "peer" => {
            let expected_id = args[2].strip_prefix("libp2p://").map(|id| {
                let mut bytes = kaspa_utils::connection::Libp2pId::default();
                faster_hex::hex_decode(id.as_bytes(), &mut bytes).unwrap();
                bytes
            });
            wait(async || {
                a.get_connected_peer_info().await.unwrap().peer_info.iter().any(|peer| match &expected_id {
                    Some(id) => peer.libp2p_id == Some(*id),
                    None => peer.libp2p_id.is_none() && peer.address.to_string() == args[2],
                })
            })
            .await;
        }
        "exchange" => {
            let b = client(&args[2]).await;
            kaspa_testing_integration::common::libp2p::exchange(&a, &b, Duration::from_secs(300)).await;
            b.disconnect().await.unwrap();
            println!("block synchronization and signed transaction exchange passed");
        }
        "blocked" => {
            let b = client(&args[2]).await;
            let sink = b.get_block_dag_info().await.unwrap().sink;
            let miner = kaspa_addresses::Address::new(kaspa_addresses::Prefix::Simnet, kaspa_addresses::Version::PubKey, &[1; 32]);
            let template = a.get_block_template(miner, vec![]).await.unwrap();
            a.submit_block(template.block, false).await.unwrap();
            wait(async || a.get_block_dag_info().await.unwrap().sink != sink).await;
            // The caller has isolated other Kaspa peers and blocked TCP and direct
            // UDP. libp2p relay access remains available, but must not carry blocks.
            #[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(TIMESTAMP)")]
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            while tokio::time::Instant::now() < deadline {
                assert_eq!(b.get_block_dag_info().await.unwrap().sink, sink, "block propagated with all direct Kaspa paths blocked");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            b.disconnect().await.unwrap();
            println!("no block propagation with TCP and direct UDP blocked");
        }
        _ => panic!("unknown command"),
    }
    a.disconnect().await.unwrap();
}
