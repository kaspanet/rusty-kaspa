use kaspa_addresses::Address;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspad_lib::args::Args;

use crate::common::daemon::Daemon;
use kaspa_utils::fd_budget::get_limit;
use std::time::Duration;

#[tokio::test]
async fn daemon_sanity_test() {
    let total_fd_limit = get_limit() / 2 - 128;
    let mut kaspad1 = Daemon::new_random(total_fd_limit);
    let rpc_client1 = kaspad1.start().await;

    let mut kaspad2 = Daemon::new_random(total_fd_limit);
    let rpc_client2 = kaspad2.start().await;

    tokio::time::sleep(Duration::from_secs(1)).await;
    rpc_client1.disconnect().await.unwrap();
    drop(rpc_client1);
    kaspad1.shutdown();

    rpc_client2.disconnect().await.unwrap();
    drop(rpc_client2);
    kaspad2.shutdown();
}

#[tokio::test]
async fn daemon_mining_test() {
    kaspa_core::log::try_init_logger("INFO");

    let args = Args {
        simnet: true,
        unsafe_rpc: true,
        enable_unsynced_mining: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        ..Default::default()
    };
    let total_fd_limit = get_limit() / 2 - 128;

    let mut kaspad1 = Daemon::new_random_with_args(args.clone(), total_fd_limit);
    let mut kaspad2 = Daemon::new_random_with_args(args, total_fd_limit);
    let rpc_client1 = kaspad1.start().await;
    let rpc_client2 = kaspad2.start().await;

    rpc_client2.add_peer(format!("127.0.0.1:{}", kaspad1.p2p_port).try_into().unwrap(), true).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await; // Let it connect
    assert_eq!(rpc_client2.get_connected_peer_info().await.unwrap().peer_info.len(), 1);

    // Mine 10 blocks to daemon #1
    for _ in 0..10 {
        let template = rpc_client1
            .get_block_template(Address::new(kaspad1.network.into(), kaspa_addresses::Version::PubKey, &[0; 32]), vec![])
            .await
            .unwrap();
        rpc_client1.submit_block(template.block, false).await.unwrap();
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    // Expect the blocks to be relayed to daemon #2
    assert_eq!(rpc_client2.get_block_dag_info().await.unwrap().block_count, 10);

    // Check that acceptance data contains the expected coinbase tx ids
    let vc = rpc_client2.get_virtual_chain_from_block(kaspa_consensus::params::SIMNET_GENESIS.hash, true).await.unwrap();
    assert_eq!(vc.removed_chain_block_hashes.len(), 0);
    assert_eq!(vc.added_chain_block_hashes.len(), 10);
    assert_eq!(vc.accepted_transaction_ids.len(), 10);
    for accepted_txs_pair in vc.accepted_transaction_ids {
        assert_eq!(accepted_txs_pair.accepted_transaction_ids.len(), 1);
    }
}
