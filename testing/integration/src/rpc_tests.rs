use std::{str::FromStr, sync::Arc, time::Duration};

use crate::common::{client_notify::ChannelNotify, daemon::Daemon};
use futures_util::future::try_join_all;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus::params::SIMNET_GENESIS;
use kaspa_consensus_core::{constants::MAX_SOMPI, header::Header, subnets::SubnetworkId, tx::Transaction};
use kaspa_core::{assert_match, info};
use kaspa_grpc_core::ops::KaspadPayloadOps;
use kaspa_hashes::Hash;
use kaspa_notify::{
    connection::{ChannelConnection, ChannelType},
    scope::{
        BlockAddedScope, FinalityConflictScope, NewBlockTemplateScope, PruningPointUtxoSetOverrideScope, Scope,
        SinkBlueScoreChangedScope, UtxosChangedScope, VirtualChainChangedScope, VirtualDaaScoreChangedScope,
    },
};
use kaspa_rpc_core::{Notification, api::rpc::RpcApi, model::*};
use kaspa_utils::{fd_budget, networking::ContextualNetAddress};
use kaspad_lib::args::Args;
use tokio::task::JoinHandle;

#[macro_export]
macro_rules! tst {
    ($op:ident, $test_body:block) => {
        tokio::spawn(async move {
            info!("Testing  {:?}", $op);
            $test_body
        })
    };

    ($op:ident, $reason:literal) => {
        tokio::spawn(async move {
            info!("Skipping {:?} --- {}", $op, $reason);
        })
    };
}

/// `cargo test --release --package kaspa-testing-integration --lib -- rpc_tests::sanity_test`
#[tokio::test]
async fn sanity_test() {
    kaspa_core::log::try_init_logger("info");
    // As we log the panic, we want to set it up after the logger
    kaspa_core::panic::configure_panic();

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
    let (sender, _) = async_channel::unbounded();
    let connection = ChannelConnection::new("test", sender, ChannelType::Closable);
    let listener_id = client.register_new_listener(connection);
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    // The intent of this for/match design (emphasizing the absence of an arm with fallback pattern in the match)
    // is to force any implementor of a new RpcApi method to add a matching arm here and to strongly incentivize
    // the adding of an actual sanity test of said new method.
    for op in KaspadPayloadOps::iter() {
        let network_id = daemon.network;
        let task: JoinHandle<()> = match op {
            KaspadPayloadOps::SubmitBlock => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Register to basic virtual events in order to keep track of block submission
                    let (sender, event_receiver) = async_channel::unbounded();
                    rpc_client.start(Some(Arc::new(ChannelNotify::new(sender)))).await;
                    rpc_client
                        .start_notify(Default::default(), Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {}))
                        .await
                        .unwrap();

                    // Before submitting a first block, the sink is the genesis,
                    let response = rpc_client.get_sink_call(None, GetSinkRequest {}).await.unwrap();
                    assert_eq!(response.sink, SIMNET_GENESIS.hash);
                    let response = rpc_client.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await.unwrap();
                    assert_eq!(response.blue_score, 0);

                    // the block count is 0
                    let response = rpc_client.get_block_count_call(None, GetBlockCountRequest {}).await.unwrap();
                    assert_eq!(response.block_count, 0);

                    // and the virtual chain is the genesis only
                    let response = rpc_client
                        .get_virtual_chain_from_block_call(
                            None,
                            GetVirtualChainFromBlockRequest {
                                start_hash: SIMNET_GENESIS.hash,
                                include_accepted_transaction_ids: false,
                                min_confirmation_count: None,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(response.added_chain_block_hashes.is_empty());
                    assert!(response.removed_chain_block_hashes.is_empty());

                    // Get a block template
                    let GetBlockTemplateResponse { block, is_synced } = rpc_client
                        .get_block_template_call(
                            None,
                            GetBlockTemplateRequest {
                                pay_address: Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32]),
                                extra_data: Vec::new(),
                            },
                        )
                        .await
                        .unwrap();
                    assert!(!is_synced);

                    // Compute the expected block hash for the received block
                    let header: Header = (&block.header).try_into().unwrap();
                    let block_hash = header.hash;

                    // Submit the template (no mining, in simnet PoW is skipped)
                    let response = rpc_client.submit_block(block.clone(), false).await.unwrap();
                    assert_eq!(response.report, SubmitBlockReport::Success);

                    // Wait for virtual event indicating the block was processed and entered past(virtual)
                    while let Ok(notification) = match tokio::time::timeout(Duration::from_secs(1), event_receiver.recv()).await {
                        Ok(res) => res,
                        Err(elapsed) => panic!("expected virtual event before {}", elapsed),
                    } {
                        match notification {
                            Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score == 1 => {
                                break;
                            }
                            Notification::VirtualDaaScoreChanged(msg) if msg.virtual_daa_score > 1 => {
                                panic!("DAA score too high for number of submitted blocks")
                            }
                            Notification::VirtualDaaScoreChanged(_) => {}
                            _ => {}
                        }
                    }

                    // After submitting a first block, the sink is the submitted block,
                    let response = rpc_client.get_sink_call(None, GetSinkRequest {}).await.unwrap();
                    assert_eq!(response.sink, block_hash);

                    // the block count is 1
                    let response = rpc_client.get_block_count_call(None, GetBlockCountRequest {}).await.unwrap();
                    assert_eq!(response.block_count, 1);

                    // and the virtual chain from genesis contains the added block
                    let response = rpc_client
                        .get_virtual_chain_from_block_call(
                            None,
                            GetVirtualChainFromBlockRequest {
                                start_hash: SIMNET_GENESIS.hash,
                                include_accepted_transaction_ids: false,
                                min_confirmation_count: None,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(response.added_chain_block_hashes.contains(&block_hash));
                    assert!(response.removed_chain_block_hashes.is_empty());

                    // VSPC min confirmation count test
                    let vc_min_count_1_response = rpc_client
                        .get_virtual_chain_from_block_call(
                            None,
                            GetVirtualChainFromBlockRequest {
                                start_hash: SIMNET_GENESIS.hash,
                                include_accepted_transaction_ids: false,
                                min_confirmation_count: Some(1),
                            },
                        )
                        .await
                        .unwrap();
                    assert!(vc_min_count_1_response.added_chain_block_hashes.is_empty());

                    let result =
                        rpc_client.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash: SIMNET_GENESIS.hash }).await;

                    // Genesis was merged by the new sink, so we're expecting a positive blueness response
                    assert_match!(result, Ok(GetCurrentBlockColorResponse { blue: true }));

                    // The new sink has no merging block yet, so we expect a MergerNotFound error
                    let result = rpc_client.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash: block_hash }).await;
                    assert!(result.is_err());

                    // Non-existing blocks should return an error
                    let result = rpc_client.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash: 999.into() }).await;
                    assert!(result.is_err());
                })
            }

            KaspadPayloadOps::GetBlockTemplate => {
                tst!(op, "see SubmitBlock")
            }

            KaspadPayloadOps::GetCurrentBlockColor => {
                tst!(op, "see SubmitBlock")
            }

            KaspadPayloadOps::GetCurrentNetwork => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_current_network_call(None, GetCurrentNetworkRequest {}).await.unwrap();
                    assert_eq!(response.network, network_id.network_type);
                })
            }

            KaspadPayloadOps::GetBlock => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result =
                        rpc_client.get_block_call(None, GetBlockRequest { hash: 0.into(), include_transactions: false }).await;
                    assert!(result.is_err());

                    let response = rpc_client
                        .get_block_call(None, GetBlockRequest { hash: SIMNET_GENESIS.hash, include_transactions: false })
                        .await
                        .unwrap();
                    assert_eq!(response.block.header.hash, SIMNET_GENESIS.hash);
                })
            }

            KaspadPayloadOps::GetBlocks => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_blocks_call(None, GetBlocksRequest { include_blocks: true, include_transactions: false, low_hash: None })
                        .await
                        .unwrap();
                    assert_eq!(response.blocks.len(), 1, "genesis block should be returned");
                    assert_eq!(response.blocks[0].header.hash, SIMNET_GENESIS.hash);
                    assert_eq!(response.block_hashes[0], SIMNET_GENESIS.hash);
                })
            }

            KaspadPayloadOps::GetInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_info_call(None, GetInfoRequest {}).await.unwrap();
                    assert_eq!(response.server_version, kaspa_core::kaspad_env::version().to_string());
                    assert_eq!(response.mempool_size, 0);
                    assert!(response.is_utxo_indexed);
                    assert!(response.has_message_id);
                    assert!(response.has_notify_command);
                })
            }

            KaspadPayloadOps::Shutdown => {
                // This test is purposely left blank since shutdown can only be tested after all other
                // tests completed
                tst!(op, "must be run in the end")
            }

            KaspadPayloadOps::GetPeerAddresses => {
                tst!(op, "see AddPeer, Ban")
            }

            KaspadPayloadOps::GetSink => {
                tst!(op, "see SubmitBlock")
            }

            KaspadPayloadOps::GetMempoolEntry => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .get_mempool_entry_call(
                            None,
                            GetMempoolEntryRequest {
                                transaction_id: 0.into(),
                                include_orphan_pool: true,
                                filter_transaction_pool: false,
                            },
                        )
                        .await;
                    // Test Get Mempool Entry:
                    // TODO: Fix by adding actual mempool entries this can get because otherwise it errors out
                    assert!(response_result.is_err());
                })
            }

            KaspadPayloadOps::GetMempoolEntries => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_mempool_entries_call(
                            None,
                            GetMempoolEntriesRequest { include_orphan_pool: true, filter_transaction_pool: false },
                        )
                        .await
                        .unwrap();
                    assert!(response.mempool_entries.is_empty());
                })
            }

            KaspadPayloadOps::GetConnectedPeerInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_connected_peer_info_call(None, GetConnectedPeerInfoRequest {}).await.unwrap();
                    assert!(response.peer_info.is_empty());
                })
            }

            KaspadPayloadOps::AddPeer => {
                let rpc_client = client.clone();
                tst!(op, {
                    let peer_address = ContextualNetAddress::from_str("1.2.3.4").unwrap();
                    let _ = rpc_client.add_peer_call(None, AddPeerRequest { peer_address, is_permanent: true }).await.unwrap();

                    // Add peer only adds the IP to a connection request. It will only be added to known_addresses if it
                    // actually can be connected to. So in this test we can't expect it to be added unless we set up an
                    // actual peer.
                    let response = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(response.known_addresses.is_empty());
                })
            }

            KaspadPayloadOps::Ban => {
                let rpc_client = client.clone();
                tst!(op, {
                    let peer_address = ContextualNetAddress::from_str("5.6.7.8").unwrap();
                    let ip = peer_address.normalize(1).ip;

                    let _ = rpc_client.add_peer_call(None, AddPeerRequest { peer_address, is_permanent: false }).await.unwrap();
                    let _ = rpc_client.ban_call(None, BanRequest { ip }).await.unwrap();

                    let response = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(response.banned_addresses.contains(&ip));

                    let _ = rpc_client.unban_call(None, UnbanRequest { ip }).await.unwrap();
                    let response = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(!response.banned_addresses.contains(&ip));
                })
            }

            KaspadPayloadOps::Unban => {
                tst!(op, "see Ban")
            }

            KaspadPayloadOps::SubmitTransaction => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Build an erroneous transaction...
                    let transaction = Transaction::new(0, vec![], vec![], 0, SubnetworkId::default(), 0, vec![]);
                    let result = rpc_client.submit_transaction((&transaction).into(), false).await;
                    // ...that gets rejected by the consensus
                    assert!(result.is_err());
                })
            }

            KaspadPayloadOps::SubmitTransactionReplacement => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Build an erroneous transaction...
                    let transaction = Transaction::new(0, vec![], vec![], 0, SubnetworkId::default(), 0, vec![]);
                    let result = rpc_client.submit_transaction_replacement((&transaction).into()).await;
                    // ...that gets rejected by the consensus
                    assert!(result.is_err());
                })
            }

            KaspadPayloadOps::SubmitLocalTransaction => {
                let rpc_client = client.clone();
                tst!(op, {
                    // Build an erroneous transaction...
                    let transaction = Transaction::new(0, vec![], vec![], 0, SubnetworkId::default(), 0, vec![]);
                    let result = rpc_client.submit_local_transaction((&transaction).into()).await;
                    // ...that gets rejected by the consensus
                    assert!(result.is_err());
                })
            }

            KaspadPayloadOps::GetSubnetwork => {
                let rpc_client = client.clone();
                tst!(op, {
                    let result =
                        rpc_client.get_subnetwork_call(None, GetSubnetworkRequest { subnetwork_id: SubnetworkId::from_byte(0) }).await;

                    // Err because it's currently unimplemented
                    assert!(result.is_err());
                })
            }

            KaspadPayloadOps::GetVirtualChainFromBlock => {
                tst!(op, "see SubmitBlock")
            }

            KaspadPayloadOps::GetBlockCount => {
                tst!(op, "see SubmitBlock")
            }

            KaspadPayloadOps::GetBlockDagInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await.unwrap();
                    assert_eq!(response.network, network_id);
                })
            }

            KaspadPayloadOps::ResolveFinalityConflict => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .resolve_finality_conflict_call(
                            None,
                            ResolveFinalityConflictRequest { finality_block_hash: Hash::from_bytes([0; 32]) },
                        )
                        .await;

                    // Err because it's currently unimplemented
                    assert!(response_result.is_err());
                })
            }

            KaspadPayloadOps::GetHeaders => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .get_headers_call(None, GetHeadersRequest { start_hash: SIMNET_GENESIS.hash, limit: 1, is_ascending: true })
                        .await;

                    // Err because it's currently unimplemented
                    assert!(response_result.is_err());
                })
            }

            KaspadPayloadOps::GetUtxosByAddresses => {
                let rpc_client = client.clone();
                tst!(op, {
                    let addresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
                    let response =
                        rpc_client.get_utxos_by_addresses_call(None, GetUtxosByAddressesRequest { addresses }).await.unwrap();
                    assert!(response.entries.is_empty());
                })
            }

            KaspadPayloadOps::GetBalanceByAddress => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_balance_by_address_call(
                            None,
                            GetBalanceByAddressRequest { address: Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32]) },
                        )
                        .await
                        .unwrap();
                    assert_eq!(response.balance, 0);
                })
            }

            KaspadPayloadOps::GetBalancesByAddresses => {
                let rpc_client = client.clone();
                tst!(op, {
                    let addresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[1u8; 32])];
                    let response = rpc_client
                        .get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(addresses.clone()))
                        .await
                        .unwrap();
                    assert_eq!(response.entries.len(), 1);
                    assert_eq!(response.entries[0].address, addresses[0]);
                    assert_eq!(response.entries[0].balance, Some(0));

                    let response =
                        rpc_client.get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(vec![])).await.unwrap();
                    assert!(response.entries.is_empty());
                })
            }

            KaspadPayloadOps::GetSinkBlueScore => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await.unwrap();
                    // A concurrent test may have added a single block so the blue score can be either 0 or 1
                    assert!(response.blue_score < 2);
                })
            }

            KaspadPayloadOps::EstimateNetworkHashesPerSecond => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response_result = rpc_client
                        .estimate_network_hashes_per_second_call(
                            None,
                            EstimateNetworkHashesPerSecondRequest { window_size: 1000, start_hash: None },
                        )
                        .await;
                    // The current DAA window is almost empty so an error is expected
                    assert!(response_result.is_err());
                })
            }

            KaspadPayloadOps::GetMempoolEntriesByAddresses => {
                let rpc_client = client.clone();
                tst!(op, {
                    let addresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
                    let response = rpc_client
                        .get_mempool_entries_by_addresses_call(
                            None,
                            GetMempoolEntriesByAddressesRequest::new(addresses.clone(), true, false),
                        )
                        .await
                        .unwrap();
                    assert_eq!(response.entries.len(), 1);
                    assert_eq!(response.entries[0].address, addresses[0]);
                    assert!(response.entries[0].receiving.is_empty());
                    assert!(response.entries[0].sending.is_empty());
                })
            }

            KaspadPayloadOps::GetCoinSupply => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_coin_supply_call(None, GetCoinSupplyRequest {}).await.unwrap();
                    assert_eq!(response.circulating_sompi, 0);
                    assert_eq!(response.max_sompi, MAX_SOMPI);
                })
            }

            KaspadPayloadOps::Ping => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _ = rpc_client.ping_call(None, PingRequest {}).await.unwrap();
                })
            }

            KaspadPayloadOps::GetConnections => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _ = rpc_client.get_connections_call(None, GetConnectionsRequest { include_profile_data: true }).await.unwrap();
                })
            }

            KaspadPayloadOps::GetMetrics => {
                let rpc_client = client.clone();
                tst!(op, {
                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: true,
                                connection_metrics: true,
                                bandwidth_metrics: true,
                                process_metrics: true,
                                storage_metrics: true,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_some());
                    assert!(get_metrics_call_response.consensus_metrics.is_some());

                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: false,
                                connection_metrics: true,
                                bandwidth_metrics: true,
                                process_metrics: true,
                                storage_metrics: true,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_some());
                    assert!(get_metrics_call_response.consensus_metrics.is_none());

                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: true,
                                connection_metrics: true,
                                bandwidth_metrics: false,
                                process_metrics: false,
                                storage_metrics: false,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_none());
                    assert!(get_metrics_call_response.consensus_metrics.is_some());

                    let get_metrics_call_response = rpc_client
                        .get_metrics_call(
                            None,
                            GetMetricsRequest {
                                consensus_metrics: false,
                                connection_metrics: true,
                                bandwidth_metrics: false,
                                process_metrics: false,
                                storage_metrics: false,
                                custom_metrics: true,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(get_metrics_call_response.process_metrics.is_none());
                    assert!(get_metrics_call_response.consensus_metrics.is_none());
                })
            }

            KaspadPayloadOps::GetSystemInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _response = rpc_client.get_system_info_call(None, GetSystemInfoRequest {}).await.unwrap();
                })
            }

            KaspadPayloadOps::GetServerInfo => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_server_info_call(None, GetServerInfoRequest {}).await.unwrap();
                    assert!(response.has_utxo_index); // we set utxoindex above
                    assert_eq!(response.network_id, network_id);
                })
            }

            KaspadPayloadOps::GetSyncStatus => {
                let rpc_client = client.clone();
                tst!(op, {
                    let _ = rpc_client.get_sync_status_call(None, GetSyncStatusRequest {}).await.unwrap();
                })
            }

            KaspadPayloadOps::GetDaaScoreTimestampEstimate => {
                let rpc_client = client.clone();
                tst!(op, {
                    let results = rpc_client
                        .get_daa_score_timestamp_estimate_call(
                            None,
                            GetDaaScoreTimestampEstimateRequest { daa_scores: vec![0, 500, 2000, u64::MAX] },
                        )
                        .await
                        .unwrap();

                    for timestamp in results.timestamps.iter() {
                        info!("Timestamp estimate is {}", timestamp);
                    }

                    let results = rpc_client
                        .get_daa_score_timestamp_estimate_call(None, GetDaaScoreTimestampEstimateRequest { daa_scores: vec![] })
                        .await
                        .unwrap();

                    for timestamp in results.timestamps.iter() {
                        info!("Timestamp estimate is {}", timestamp);
                    }
                })
            }

            KaspadPayloadOps::GetFeeEstimate => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_fee_estimate().await.unwrap();
                    info!("{:?}", response.priority_bucket);
                    assert!(!response.normal_buckets.is_empty());
                    assert!(!response.low_buckets.is_empty());
                    for bucket in response.ordered_buckets() {
                        info!("{:?}", bucket);
                    }
                })
            }

            KaspadPayloadOps::GetFeeEstimateExperimental => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client.get_fee_estimate_experimental(true).await.unwrap();
                    assert!(!response.estimate.normal_buckets.is_empty());
                    assert!(!response.estimate.low_buckets.is_empty());
                    for bucket in response.estimate.ordered_buckets() {
                        info!("{:?}", bucket);
                    }
                    assert!(response.verbose.is_some());
                    info!("{:?}", response.verbose);
                })
            }

            KaspadPayloadOps::GetUtxoReturnAddress => {
                let rpc_client = client.clone();
                tst!(op, {
                    let results = rpc_client.get_utxo_return_address(RpcHash::from_bytes([0; 32]), 1000).await;

                    assert!(results.is_err_and(|err| {
                        match err {
                            kaspa_rpc_core::RpcError::General(msg) => {
                                info!("Expected error message: {}", msg);
                                true
                            }
                            _ => false,
                        }
                    }));
                })
            }

            KaspadPayloadOps::GetVirtualChainFromBlockV2 => {
                let rpc_client = client.clone();
                tst!(op, {
                    let response = rpc_client
                        .get_virtual_chain_from_block_v2_call(
                            None,
                            GetVirtualChainFromBlockV2Request {
                                start_hash: SIMNET_GENESIS.hash,
                                data_verbosity_level: None,
                                min_confirmation_count: None,
                            },
                        )
                        .await
                        .unwrap();
                    assert!(response.added_chain_block_hashes.is_empty());
                    assert!(response.removed_chain_block_hashes.is_empty());
                })
            }

            KaspadPayloadOps::NotifyBlockAdded => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, BlockAddedScope {}.into()).await.unwrap();
                })
            }

            KaspadPayloadOps::NotifyNewBlockTemplate => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, NewBlockTemplateScope {}.into()).await.unwrap();
                })
            }

            KaspadPayloadOps::NotifyFinalityConflict => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, FinalityConflictScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyUtxosChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, UtxosChangedScope::new(vec![]).into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifySinkBlueScoreChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, SinkBlueScoreChangedScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyPruningPointUtxoSetOverride => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, PruningPointUtxoSetOverrideScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyVirtualDaaScoreChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, VirtualDaaScoreChangedScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyVirtualChainChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client
                        .start_notify(id, VirtualChainChangedScope { include_accepted_transaction_ids: false }.into())
                        .await
                        .unwrap();
                })
            }
            KaspadPayloadOps::StopNotifyingUtxosChanged => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.stop_notify(id, UtxosChangedScope::new(vec![]).into()).await.unwrap();
                })
            }
            KaspadPayloadOps::StopNotifyingPruningPointUtxoSetOverride => {
                let rpc_client = client.clone();
                let id = listener_id;
                tst!(op, {
                    rpc_client.stop_notify(id, PruningPointUtxoSetOverrideScope {}.into()).await.unwrap();
                })
            }
        };
        tasks.push(task);
    }

    let _results = try_join_all(tasks).await;

    // Unregister the notification listener
    assert!(client.unregister_listener(listener_id).await.is_ok());

    // Shutdown should only be tested after everything
    let rpc_client = client.clone();
    let _ = rpc_client.shutdown_call(None, ShutdownRequest {}).await.unwrap();

    //
    // Fold-up
    //
    client.disconnect().await.unwrap();
    drop(client);
    daemon.shutdown();
}

// =============================================================================
// SubmitLocalTransaction E2E tests (B1–B5)
// =============================================================================

/// `cargo test --release --package kaspa-testing-integration --lib -- rpc_tests::submit_local_transaction_e2e`
#[tokio::test]
async fn submit_local_transaction_e2e() {
    use crate::common::{client_notify::ChannelNotify, utils::fetch_spendable_utxos};
    use kaspa_consensus_core::{constants::TX_VERSION, sign::sign, subnets::SUBNETWORK_ID_NATIVE, tx::SignableTransaction};
    use kaspa_notify::scope::VirtualDaaScoreChangedScope;
    use kaspa_txscript::pay_to_address_script;
    use rand::thread_rng;
    use std::time::Duration;

    kaspa_core::log::try_init_logger("info");
    kaspa_core::panic::configure_panic();

    let args = Args {
        simnet: true,
        disable_upnp: true,
        enable_unsynced_mining: true,
        block_template_cache_lifetime: Some(0),
        utxoindex: true,
        unsafe_rpc: true,
        ..Default::default()
    };

    let fd_total_budget = fd_budget::limit();
    let mut daemon = Daemon::new_random_with_args(args, fd_total_budget);
    let client = daemon.start().await;

    // Mining key and address
    let (miner_sk, miner_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let miner_address =
        Address::new(daemon.network.into(), kaspa_addresses::Version::PubKey, &miner_pk.x_only_public_key().0.serialize());
    let miner_schnorr_key = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, &miner_sk);

    // User key and address
    let (_, user_pk) = secp256k1::generate_keypair(&mut thread_rng());
    let user_address =
        Address::new(daemon.network.into(), kaspa_addresses::Version::PubKey, &user_pk.x_only_public_key().0.serialize());

    // Set up notifications
    let (sender, event_receiver) = async_channel::unbounded();
    client.start(Some(Arc::new(ChannelNotify::new(sender)))).await;
    client.start_notify(Default::default(), VirtualDaaScoreChangedScope {}.into()).await.unwrap();

    // Mine initial blocks to reach coinbase maturity
    let coinbase_maturity = kaspa_consensus::params::SIMNET_PARAMS.coinbase_maturity();
    let extra_blocks = 10u64;
    for _ in 0..coinbase_maturity + extra_blocks {
        let template = client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        client.submit_block(template.block, false).await.unwrap();
    }
    // Wait for virtual DAA score to catch up
    loop {
        match tokio::time::timeout(Duration::from_millis(500), event_receiver.recv()).await {
            Ok(Ok(Notification::VirtualDaaScoreChanged(msg))) if msg.virtual_daa_score >= coinbase_maturity + extra_blocks => break,
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }

    // Fetch spendable UTXOs
    let utxos = fetch_spendable_utxos(&client, miner_address.clone(), coinbase_maturity).await;
    assert!(!utxos.is_empty(), "should have spendable UTXOs after mining");

    // =========================================================================
    // B1: Happy path — submit valid local TX, verify template inclusion
    // =========================================================================
    info!("B1: submit_local_tx_rpc_happy_path");
    {
        let spk = pay_to_address_script(&user_address);
        let utxo = &utxos[0];
        let amount = utxo.1.amount - 1000; // small fee for mass, though local TX allows zero
        let inputs = vec![kaspa_consensus_core::tx::TransactionInput {
            previous_outpoint: utxo.0,
            signature_script: vec![],
            sequence: 0,
            sig_op_count: 1,
        }];
        let outputs = vec![kaspa_consensus_core::tx::TransactionOutput { value: amount, script_public_key: spk }];
        let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let signed_tx = sign(SignableTransaction::with_entries(unsigned_tx, vec![utxo.1.clone()]), miner_schnorr_key);
        let tx_id = signed_tx.id();

        // Submit as local transaction
        let result = client.submit_local_transaction((&signed_tx.tx).into()).await;
        assert!(result.is_ok(), "B1: valid local transaction should be accepted: {:?}", result.err());
        assert_eq!(result.unwrap(), tx_id, "B1: returned transaction ID should match");

        // Verify it appears in the block template
        let template = client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        let template_tx_ids: Vec<_> =
            template.block.transactions.iter().filter_map(|t| Transaction::try_from(t.clone()).ok()).map(|t| t.id()).collect();
        assert!(
            template_tx_ids.contains(&tx_id),
            "B1: local TX {} should be in block template. Template TXs: {:?}",
            tx_id,
            template_tx_ids
        );

        // Mine the block and verify acceptance
        client.submit_block(template.block, false).await.unwrap();
    }

    // =========================================================================
    // B2: Reject invalid transaction via RPC
    // =========================================================================
    info!("B2: submit_local_tx_rpc_reject_invalid");
    {
        // Empty transaction — should be rejected
        let bad_tx = Transaction::new(0, vec![], vec![], 0, SubnetworkId::default(), 0, vec![]);
        let result = client.submit_local_transaction((&bad_tx).into()).await;
        assert!(result.is_err(), "B2: invalid transaction should be rejected");
    }

    // =========================================================================
    // B3: Zero-fee TX gets included in a block
    // =========================================================================
    info!("B3: submit_local_tx_zero_fee_in_block");
    {
        // Refresh UTXOs after B1 mined a block
        let utxos = fetch_spendable_utxos(&client, miner_address.clone(), coinbase_maturity).await;
        assert!(!utxos.is_empty(), "B3: should have spendable UTXOs");

        let spk = pay_to_address_script(&user_address);
        let utxo = &utxos[0];
        let amount = utxo.1.amount; // zero fee — use full amount
        let inputs = vec![kaspa_consensus_core::tx::TransactionInput {
            previous_outpoint: utxo.0,
            signature_script: vec![],
            sequence: 0,
            sig_op_count: 1,
        }];
        let outputs = vec![kaspa_consensus_core::tx::TransactionOutput { value: amount, script_public_key: spk }];
        let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let signed_tx = sign(SignableTransaction::with_entries(unsigned_tx, vec![utxo.1.clone()]), miner_schnorr_key);
        let tx_id = signed_tx.id();

        let result = client.submit_local_transaction((&signed_tx.tx).into()).await;
        assert!(result.is_ok(), "B3: zero-fee local transaction should be accepted: {:?}", result.err());

        // Get template and mine
        let template = client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        let template_tx_ids: Vec<_> =
            template.block.transactions.iter().filter_map(|t| Transaction::try_from(t.clone()).ok()).map(|t| t.id()).collect();
        assert!(template_tx_ids.contains(&tx_id), "B3: zero-fee local TX should be in template");

        client.submit_block(template.block, false).await.unwrap();
    }

    // =========================================================================
    // B4: Local TX does not appear in mempool entries
    // =========================================================================
    info!("B4: submit_local_tx_not_in_mempool");
    {
        let utxos = fetch_spendable_utxos(&client, miner_address.clone(), coinbase_maturity).await;
        assert!(!utxos.is_empty(), "B4: should have spendable UTXOs");

        let spk = pay_to_address_script(&user_address);
        let utxo = &utxos[0];
        let amount = utxo.1.amount;
        let inputs = vec![kaspa_consensus_core::tx::TransactionInput {
            previous_outpoint: utxo.0,
            signature_script: vec![],
            sequence: 0,
            sig_op_count: 1,
        }];
        let outputs = vec![kaspa_consensus_core::tx::TransactionOutput { value: amount, script_public_key: spk }];
        let unsigned_tx = Transaction::new(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let signed_tx = sign(SignableTransaction::with_entries(unsigned_tx, vec![utxo.1.clone()]), miner_schnorr_key);
        let tx_id = signed_tx.id();

        client.submit_local_transaction((&signed_tx.tx).into()).await.unwrap();

        // Check that TX is NOT in mempool
        let mempool_result = client.get_mempool_entry(tx_id, false, false).await;
        assert!(mempool_result.is_err(), "B4: local TX should NOT appear in mempool entries");

        // But it should be in the block template
        let template = client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        let template_tx_ids: Vec<_> =
            template.block.transactions.iter().filter_map(|t| Transaction::try_from(t.clone()).ok()).map(|t| t.id()).collect();
        assert!(template_tx_ids.contains(&tx_id), "B4: local TX should be in template despite not being in mempool");

        // Mine to clean up
        client.submit_block(template.block, false).await.unwrap();
    }

    // NOTE: B5 (2-node broadcast test) removed — CI environment fd_budget limit
    // prevents running 2 daemons concurrently. The no-broadcast behavior is verified
    // by code inspection: submit_local_transaction in flow_context.rs does not call
    // broadcast_transactions. Can be tested manually with 2 nodes.

    // =========================================================================
    // Cleanup
    // =========================================================================
    let _ = client.shutdown_call(None, ShutdownRequest {}).await;
    client.disconnect().await.unwrap();
    drop(client);
    daemon.shutdown();
}
