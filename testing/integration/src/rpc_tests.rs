use std::str::FromStr;

use crate::common::daemon::Daemon;
use futures_util::future::try_join_all;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_hashes::Hash;
use kaspa_rpc_core::{api::rpc::RpcApi, model::*};
use kaspa_utils::{fd_budget, networking::ContextualNetAddress};
use kaspad_lib::args::Args;
use tokio::task::JoinHandle;

#[macro_export]
macro_rules! rpc_function_test {
    ($tasks:ident, $rpc_client:ident, $test_body:block) => {
        let task: JoinHandle<()> = tokio::spawn(async move { $test_body });
        $tasks.push(task);
    };
}

/// `cargo test --release --package kaspa-testing-integration --lib -- rpc_tests::sanity_test`
#[tokio::test]
async fn sanity_test() {
    kaspa_core::panic::configure_panic();
    kaspa_core::log::try_init_logger(
        "info,kaspa_rpc_core=debug,kaspa_rpc_service=debug,kaspa_grpc_client=debug,kaspa_grpc_server=debug",
    );

    let args = Args {
        simnet: true,
        disable_upnp: true, // UPnP registration might take some time and is not needed for this test
        enable_unsynced_mining: true,
        block_template_cache_lifetime: Some(0),
        utxoindex: true,
        unsafe_rpc: true,
        ..Default::default()
    };

    let fd_total_budget = fd_budget::limit();
    let mut daemon = Daemon::new_random_with_args(args, fd_total_budget);
    let client = daemon.start().await;
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    // Test Ping:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let _ = rpc_client.ping_call(PingRequest {}).await.unwrap();
    });

    // Test Get Info:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let get_info_call_response = rpc_client.get_info_call(GetInfoRequest {}).await.unwrap();
        assert_eq!("0.1.7", get_info_call_response.server_version);
    });

    // Test Get Metrics:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let get_metrics_call_response =
            rpc_client.get_metrics_call(GetMetricsRequest { consensus_metrics: true, process_metrics: true }).await.unwrap();

        assert!(get_metrics_call_response.process_metrics.is_some());
        assert!(get_metrics_call_response.consensus_metrics.is_some());

        let get_metrics_call_response =
            rpc_client.get_metrics_call(GetMetricsRequest { consensus_metrics: false, process_metrics: true }).await.unwrap();

        assert!(get_metrics_call_response.process_metrics.is_some());
        assert!(get_metrics_call_response.consensus_metrics.is_none());

        let get_metrics_call_response =
            rpc_client.get_metrics_call(GetMetricsRequest { consensus_metrics: true, process_metrics: false }).await.unwrap();

        assert!(get_metrics_call_response.process_metrics.is_none());
        assert!(get_metrics_call_response.consensus_metrics.is_some());

        let get_metrics_call_response =
            rpc_client.get_metrics_call(GetMetricsRequest { consensus_metrics: false, process_metrics: false }).await.unwrap();

        assert!(get_metrics_call_response.process_metrics.is_none());
        assert!(get_metrics_call_response.consensus_metrics.is_none());
    });

    // Test Get Coin Supply:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response = rpc_client.get_coin_supply_call(GetCoinSupplyRequest {}).await.unwrap();

        // Nothing mined, so there should be nothing circulating
        assert_eq!(0, response.circulating_sompi);
        // Max sompi should always be higher than 0
        assert!(response.max_sompi > 0);
    });

    // Test Get Server Info: get_server_info_call
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response = rpc_client.get_server_info_call(GetServerInfoRequest {}).await.unwrap();

        assert!(response.has_utxo_index); // we set utxoindex above
        assert_eq!(response.network_id, daemon.network);
    });

    // Test Get Sync Status:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response_result = rpc_client.get_sync_status_call(GetSyncStatusRequest {}).await;

        assert!(response_result.is_ok());
    });

    // Test Get Current Network:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response = rpc_client.get_current_network_call(GetCurrentNetworkRequest {}).await.unwrap();

        assert_eq!(response.network, daemon.network.network_type);
    });

    // Test Get Block Template:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let _ = rpc_client
            .get_block_template_call(GetBlockTemplateRequest {
                pay_address: Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32]),
                extra_data: Vec::new(),
            })
            .await
            .unwrap();
    });

    // Test Add Peer, Ban Peer, Unban, Get Peer Addresses:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let peer_to_add = ContextualNetAddress::from_str("1.2.3.4:16110").unwrap();
        let peer_to_ban = ContextualNetAddress::from_str("5.6.7.8:16110").unwrap();

        let response = rpc_client.get_peer_addresses_call(GetPeerAddressesRequest {}).await.unwrap();

        assert!(response.known_addresses.is_empty());
        assert!(response.banned_addresses.is_empty());

        // Add peer only adds the IP to a connection request. It will only be added to known_addresses if it
        // actually can be connected to. So in CI we can't expect it to be added unless we set up an actual peer
        let _ = rpc_client.add_peer_call(AddPeerRequest { peer_address: peer_to_add, is_permanent: true }).await.unwrap();

        let _ = rpc_client.add_peer_call(AddPeerRequest { peer_address: peer_to_ban, is_permanent: false }).await.unwrap();
        let _ = rpc_client.ban_call(BanRequest { ip: peer_to_ban.normalize(12345).ip }).await.unwrap();
        let response = rpc_client.get_peer_addresses_call(GetPeerAddressesRequest {}).await.unwrap();

        assert!(response.banned_addresses.contains(&peer_to_ban.normalize(12345).ip));

        let _ = rpc_client.unban_call(UnbanRequest { ip: peer_to_ban.normalize(12345).ip }).await.unwrap();
    });

    // Test Get Mempool Entries:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response = rpc_client
            .get_mempool_entries_call(GetMempoolEntriesRequest { filter_transaction_pool: false, include_orphan_pool: false })
            .await
            .unwrap();

        assert!(response.mempool_entries.is_empty());
    });

    // Test Get Connected Peer Info:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response = rpc_client.get_connected_peer_info_call(GetConnectedPeerInfoRequest {}).await.unwrap();

        assert!(response.peer_info.is_empty());
    });

    // Test Get Block Count:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let _ = rpc_client.get_block_count_call(GetBlockCountRequest {}).await.unwrap();
    });

    // Test Block Dag Info:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let _ = rpc_client.get_block_dag_info_call(GetBlockDagInfoRequest {}).await.unwrap();
    });

    // Test get Balance By Address:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response = rpc_client
            .get_balance_by_address_call(GetBalanceByAddressRequest {
                address: Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32]),
            })
            .await
            .unwrap();

        assert_eq!(0, response.balance);
    });

    // Test Get Balances By Addresses:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let addresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
        let _ = rpc_client.get_balances_by_addresses_call(GetBalancesByAddressesRequest { addresses }).await.unwrap();
    });

    // Test Utxos By Addresses:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let addresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
        let _ = rpc_client.get_utxos_by_addresses_call(GetUtxosByAddressesRequest { addresses }).await.unwrap();
    });

    // Test Get Sink Blue Score:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let _ = rpc_client.get_sink_blue_score_call(GetSinkBlueScoreRequest {}).await.unwrap();
    });

    // Test Get Mempool Entries By Addresses
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let addresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
        let _ = rpc_client
            .get_mempool_entries_by_addresses_call(GetMempoolEntriesByAddressesRequest {
                addresses,
                include_orphan_pool: false,
                filter_transaction_pool: false,
            })
            .await
            .unwrap();
    });

    // Test Resolve Finality Conflict:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response_result = rpc_client
            .resolve_finality_conflict_call(ResolveFinalityConflictRequest { finality_block_hash: Hash::from_bytes([0; 32]) })
            .await;

        // Err because it's currently unimplemented
        assert!(response_result.is_err());
    });

    // Test Get Headers:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response_result = rpc_client
            .get_headers_call(GetHeadersRequest { start_hash: Hash::from_bytes([255; 32]), limit: 1, is_ascending: true })
            .await;

        // Err because it's currently unimplemented
        assert!(response_result.is_err());
    });

    // Test Subnetwork:
    let rpc_client = daemon.new_client().await;
    rpc_function_test!(tasks, rpc_client, {
        let response_result = rpc_client.get_subnetwork_call(GetSubnetworkRequest { subnetwork_id: SubnetworkId::from_byte(0) }).await;

        // Err because it's currently unimplemented
        assert!(response_result.is_err());
    });

    // Test Get Virtual Chain From Block
    // TODO: requires some setup
    // let rpc_client = daemon.new_client().await;
    // rpc_function_test!(tasks, rpc_client, {
    //     let _ = rpc_client
    //         .get_virtual_chain_from_block_call(GetVirtualChainFromBlockRequest {
    //             start_hash: Hash::from_bytes([255; 32]),
    //             include_accepted_transaction_ids: false,
    //         })
    //         .await
    //         .unwrap();
    // });

    // Test Get Blocks:
    // TODO: requires some setup
    // let rpc_client = daemon.new_client().await;
    // rpc_function_test!(tasks, rpc_client, {
    //     let _ = rpc_client
    //         .get_blocks_call(GetBlocksRequest { include_blocks: false, include_transactions: false, low_hash: None })
    //         .await
    //         .unwrap();
    // });

    // TODO: Fix by increasing the actual window_size until this works
    // Current error: difficulty error: under min allowed window size (0 < 1000)
    // let rpc_client = daemon.new_client().await;
    // rpc_function_test!(tasks, rpc_client, {
    //     let _ = rpc_client
    //         .estimate_network_hashes_per_second_call(EstimateNetworkHashesPerSecondRequest {
    //             window_size: 1000,
    //             start_hash: None,
    //         })
    //         .await
    //         .unwrap();
    // });

    // Test Get Mempool Entry:
    // TODO: Fix by adding actual mempool entries this can get because otherwise it errors out
    // let rpc_client = daemon.new_client().await;
    // rpc_function_test!(tasks, rpc_client, {
    //     let _ = rpc_client
    //         .get_mempool_entry_call(GetMempoolEntryRequest {
    //             transaction_id: Hash::from_bytes([255; 32]),
    //             include_orphan_pool: false,
    //             filter_transaction_pool: false,
    //         })
    //         .await
    //         .unwrap();
    // });

    // Test Block:
    // TODO: Fix by adding actual mempool entries this can pool because otherwise it errors out
    // let rpc_client = daemon.new_client().await;
    // rpc_function_test!(tasks, rpc_client, {
    //     let _ = rpc_client
    //         .get_block_call(GetBlockRequest {
    //             hash: Hash::from_bytes([255; 32]),
    //             include_transactions: false,
    //         })
    //         .await
    //         .unwrap();
    // });

    // These are covered by other tests:
    // submit_transaction_call
    // submit_block_call

    // shutdown_call

    let _results = try_join_all(tasks).await;

    // Shutdown should only be tested after everything
    let rpc_client = daemon.new_client().await;
    let _ = rpc_client.shutdown_call(ShutdownRequest {}).await.unwrap();

    //
    // Fold-up
    //
    client.disconnect().await.unwrap();
    drop(client);
    daemon.shutdown();
}
