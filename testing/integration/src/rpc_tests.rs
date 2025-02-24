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
use kaspa_rpc_core::{api::rpc::RpcApi, model::*, Notification};
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
                letrpc_client = client.clone();
                tst!(op,{let(sender,event_receiver)=async_channel::unbounded();rpc_client.start(Some(Arc::new(ChannelNotify::new(sender)))).await;rpc_client.start_notify(Default::default(),Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope{})).await.unwrap();letresponse=rpc_client.get_sink_call(None,GetSinkRequest{}).await.unwrap();assert_eq!(response.sink,SIMNET_GENESIS.hash);letresponse=rpc_client.get_sink_blue_score_call(None,GetSinkBlueScoreRequest{}).await.unwrap();assert_eq!(response.blue_score,0);letresponse=rpc_client.get_block_count_call(None,GetBlockCountRequest{}).await.unwrap();assert_eq!(response.block_count,0);letresponse=rpc_client.get_virtual_chain_from_block_call(None,GetVirtualChainFromBlockRequest{start_hash:SIMNET_GENESIS.hash,include_accepted_transaction_ids:false,},).await.unwrap();assert!(response.added_chain_block_hashes.is_empty());assert!(response.removed_chain_block_hashes.is_empty());letGetBlockTemplateResponse{block,is_synced}=rpc_client.get_block_template_call(None,GetBlockTemplateRequest{pay_address:Address::new(Prefix::Simnet,Version::PubKey, &[0u8;32]),extra_data:Vec::new(),},).await.unwrap();assert!(!is_synced);letheader:Header=(&block.header).into();letblock_hash=header.hash;letresponse=rpc_client.submit_block(block.clone(),false).await.unwrap();assert_eq!(response.report,SubmitBlockReport::Success);whileletOk(notification)=matchtokio::time::timeout(Duration::from_secs(1),event_receiver.recv()).await{Ok(res)=>res,Err(elapsed)=>panic!("expected virtual event before {}",elapsed),}{matchnotification{Notification::VirtualDaaScoreChanged(msg)ifmsg.virtual_daa_score==1=>{break;}Notification::VirtualDaaScoreChanged(msg)ifmsg.virtual_daa_score>1=>{panic!("DAA score too high for number of submitted blocks")}Notification::VirtualDaaScoreChanged(_)=>{}_=>{}}}letresponse=rpc_client.get_sink_call(None,GetSinkRequest{}).await.unwrap();assert_eq!(response.sink,block_hash);letresponse=rpc_client.get_block_count_call(None,GetBlockCountRequest{}).await.unwrap();assert_eq!(response.block_count,1);letresponse=rpc_client.get_virtual_chain_from_block_call(None,GetVirtualChainFromBlockRequest{start_hash:SIMNET_GENESIS.hash,include_accepted_transaction_ids:false,},).await.unwrap();assert!(response.added_chain_block_hashes.contains(&block_hash));assert!(response.removed_chain_block_hashes.is_empty());letresult=rpc_client.get_current_block_color_call(None,GetCurrentBlockColorRequest{hash:SIMNET_GENESIS.hash}).await;assert_match!(result,Ok(GetCurrentBlockColorResponse{blue:true}));letresult=rpc_client.get_current_block_color_call(None,GetCurrentBlockColorRequest{hash:block_hash}).await;assert!(result.is_err());letresult=rpc_client.get_current_block_color_call(None,GetCurrentBlockColorRequest{hash:999.into()}).await;assert!(result.is_err());})
            }
            KaspadPayloadOps::GetBlockTemplate => {
                tst!(op, "see SubmitBlock")
            }
            KaspadPayloadOps::GetCurrentBlockColor => {
                tst!(op, "see SubmitBlock")
            }
            KaspadPayloadOps::GetCurrentNetwork => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_current_network_call(None, GetCurrentNetworkRequest {}).await.unwrap();
                    assert_eq!(response.network, network_id.network_type);
                })
            }
            KaspadPayloadOps::GetBlock => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresult = rpc_client.get_block_call(None, GetBlockRequest { hash: 0.into(), include_transactions: false }).await;
                    assert!(result.is_err());
                    letresponse = rpc_client
                        .get_block_call(None, GetBlockRequest { hash: SIMNET_GENESIS.hash, include_transactions: false })
                        .await
                        .unwrap();
                    assert_eq!(response.block.header.hash, SIMNET_GENESIS.hash);
                })
            }
            KaspadPayloadOps::GetBlocks => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client
                        .get_blocks_call(None, GetBlocksRequest { include_blocks: true, include_transactions: false, low_hash: None })
                        .await
                        .unwrap();
                    assert_eq!(response.blocks.len(), 1, "genesis block should be returned");
                    assert_eq!(response.blocks[0].header.hash, SIMNET_GENESIS.hash);
                    assert_eq!(response.block_hashes[0], SIMNET_GENESIS.hash);
                })
            }
            KaspadPayloadOps::GetInfo => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_info_call(None, GetInfoRequest {}).await.unwrap();
                    assert_eq!(response.server_version, kaspa_core::kaspad_env::version().to_string());
                    assert_eq!(response.mempool_size, 0);
                    assert!(response.is_utxo_indexed);
                    assert!(response.has_message_id);
                    assert!(response.has_notify_command);
                })
            }
            KaspadPayloadOps::Shutdown => {
                tst!(op, "must be run in the end")
            }
            KaspadPayloadOps::GetPeerAddresses => {
                tst!(op, "see AddPeer, Ban")
            }
            KaspadPayloadOps::GetSink => {
                tst!(op, "see SubmitBlock")
            }
            KaspadPayloadOps::GetMempoolEntry => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse_result = rpc_client
                        .get_mempool_entry_call(
                            None,
                            GetMempoolEntryRequest {
                                transaction_id: 0.into(),
                                include_orphan_pool: true,
                                filter_transaction_pool: false,
                            },
                        )
                        .await;
                    assert!(response_result.is_err());
                })
            }
            KaspadPayloadOps::GetMempoolEntries => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client
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
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_connected_peer_info_call(None, GetConnectedPeerInfoRequest {}).await.unwrap();
                    assert!(response.peer_info.is_empty());
                })
            }
            KaspadPayloadOps::AddPeer => {
                letrpc_client = client.clone();
                tst!(op, {
                    letpeer_address = ContextualNetAddress::from_str("1.2.3.4").unwrap();
                    let_ = rpc_client.add_peer_call(None, AddPeerRequest { peer_address, is_permanent: true }).await.unwrap();
                    letresponse = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(response.known_addresses.is_empty());
                })
            }
            KaspadPayloadOps::Ban => {
                letrpc_client = client.clone();
                tst!(op, {
                    letpeer_address = ContextualNetAddress::from_str("5.6.7.8").unwrap();
                    letip = peer_address.normalize(1).ip;
                    let_ = rpc_client.add_peer_call(None, AddPeerRequest { peer_address, is_permanent: false }).await.unwrap();
                    let_ = rpc_client.ban_call(None, BanRequest { ip }).await.unwrap();
                    letresponse = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(response.banned_addresses.contains(&ip));
                    let_ = rpc_client.unban_call(None, UnbanRequest { ip }).await.unwrap();
                    letresponse = rpc_client.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await.unwrap();
                    assert!(!response.banned_addresses.contains(&ip));
                })
            }
            KaspadPayloadOps::Unban => {
                tst!(op, "see Ban")
            }
            KaspadPayloadOps::SubmitTransaction => {
                letrpc_client = client.clone();
                tst!(op, {
                    lettransaction = Transaction::new(0, vec![], vec![], 0, SubnetworkId::default(), 0, vec![]);
                    letresult = rpc_client.submit_transaction((&transaction).into(), false).await;
                    assert!(result.is_err());
                })
            }
            KaspadPayloadOps::SubmitTransactionReplacement => {
                letrpc_client = client.clone();
                tst!(op, {
                    lettransaction = Transaction::new(0, vec![], vec![], 0, SubnetworkId::default(), 0, vec![]);
                    letresult = rpc_client.submit_transaction_replacement((&transaction).into()).await;
                    assert!(result.is_err());
                })
            }
            KaspadPayloadOps::GetSubnetwork => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresult =
                        rpc_client.get_subnetwork_call(None, GetSubnetworkRequest { subnetwork_id: SubnetworkId::from_byte(0) }).await;
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
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await.unwrap();
                    assert_eq!(response.network, network_id);
                })
            }
            KaspadPayloadOps::ResolveFinalityConflict => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse_result = rpc_client
                        .resolve_finality_conflict_call(
                            None,
                            ResolveFinalityConflictRequest { finality_block_hash: Hash::from_bytes([0; 32]) },
                        )
                        .await;
                    assert!(response_result.is_err());
                })
            }
            KaspadPayloadOps::GetHeaders => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse_result = rpc_client
                        .get_headers_call(None, GetHeadersRequest { start_hash: SIMNET_GENESIS.hash, limit: 1, is_ascending: true })
                        .await;
                    assert!(response_result.is_err());
                })
            }
            KaspadPayloadOps::GetUtxosByAddresses => {
                letrpc_client = client.clone();
                tst!(op, {
                    letaddresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
                    letresponse =
                        rpc_client.get_utxos_by_addresses_call(None, GetUtxosByAddressesRequest { addresses }).await.unwrap();
                    assert!(response.entries.is_empty());
                })
            }
            KaspadPayloadOps::GetBalanceByAddress => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client
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
                letrpc_client = client.clone();
                tst!(op, {
                    letaddresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[1u8; 32])];
                    letresponse = rpc_client
                        .get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(addresses.clone()))
                        .await
                        .unwrap();
                    assert_eq!(response.entries.len(), 1);
                    assert_eq!(response.entries[0].address, addresses[0]);
                    assert_eq!(response.entries[0].balance, Some(0));
                    letresponse =
                        rpc_client.get_balances_by_addresses_call(None, GetBalancesByAddressesRequest::new(vec![])).await.unwrap();
                    assert!(response.entries.is_empty());
                })
            }
            KaspadPayloadOps::GetSinkBlueScore => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await.unwrap();
                    assert!(response.blue_score < 2);
                })
            }
            KaspadPayloadOps::EstimateNetworkHashesPerSecond => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse_result = rpc_client
                        .estimate_network_hashes_per_second_call(
                            None,
                            EstimateNetworkHashesPerSecondRequest { window_size: 1000, start_hash: None },
                        )
                        .await;
                    assert!(response_result.is_err());
                })
            }
            KaspadPayloadOps::GetMempoolEntriesByAddresses => {
                letrpc_client = client.clone();
                tst!(op, {
                    letaddresses = vec![Address::new(Prefix::Simnet, Version::PubKey, &[0u8; 32])];
                    letresponse = rpc_client
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
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_coin_supply_call(None, GetCoinSupplyRequest {}).await.unwrap();
                    assert_eq!(response.circulating_sompi, 0);
                    assert_eq!(response.max_sompi, MAX_SOMPI);
                })
            }
            KaspadPayloadOps::Ping => {
                letrpc_client = client.clone();
                tst!(op, {
                    let_ = rpc_client.ping_call(None, PingRequest {}).await.unwrap();
                })
            }
            KaspadPayloadOps::GetConnections => {
                letrpc_client = client.clone();
                tst!(op, {
                    let_ = rpc_client.get_connections_call(None, GetConnectionsRequest { include_profile_data: true }).await.unwrap();
                })
            }
            KaspadPayloadOps::GetMetrics => {
                letrpc_client = client.clone();
                tst!(op, {
                    letget_metrics_call_response = rpc_client
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
                    letget_metrics_call_response = rpc_client
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
                    letget_metrics_call_response = rpc_client
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
                    letget_metrics_call_response = rpc_client
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
                letrpc_client = client.clone();
                tst!(op, {
                    let_response = rpc_client.get_system_info_call(None, GetSystemInfoRequest {}).await.unwrap();
                })
            }
            KaspadPayloadOps::GetServerInfo => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresponse = rpc_client.get_server_info_call(None, GetServerInfoRequest {}).await.unwrap();
                    assert!(response.has_utxo_index);
                    assert_eq!(response.network_id, network_id);
                })
            }
            KaspadPayloadOps::GetSyncStatus => {
                letrpc_client = client.clone();
                tst!(op, {
                    let_ = rpc_client.get_sync_status_call(None, GetSyncStatusRequest {}).await.unwrap();
                })
            }
            KaspadPayloadOps::GetDaaScoreTimestampEstimate => {
                letrpc_client = client.clone();
                tst!(op,{letresults=rpc_client.get_daa_score_timestamp_estimate_call(None,GetDaaScoreTimestampEstimateRequest{daa_scores:vec![0,500,2000,u64::MAX]},).await.unwrap();fortimestampinresults.timestamps.iter(){info!("Timestamp estimate is {}",timestamp);}letresults=rpc_client.get_daa_score_timestamp_estimate_call(None,GetDaaScoreTimestampEstimateRequest{daa_scores:vec![]}).await.unwrap();fortimestampinresults.timestamps.iter(){info!("Timestamp estimate is {}",timestamp);}})
            }
            KaspadPayloadOps::GetFeeEstimate => {
                letrpc_client = client.clone();
                tst!(op,{letresponse=rpc_client.get_fee_estimate().await.unwrap();info!("{:?}",response.priority_bucket);assert!(!response.normal_buckets.is_empty());assert!(!response.low_buckets.is_empty());forbucketinresponse.ordered_buckets(){info!("{:?}",bucket);}})
            }
            KaspadPayloadOps::GetFeeEstimateExperimental => {
                letrpc_client = client.clone();
                tst!(op,{letresponse=rpc_client.get_fee_estimate_experimental(true).await.unwrap();assert!(!response.estimate.normal_buckets.is_empty());assert!(!response.estimate.low_buckets.is_empty());forbucketinresponse.estimate.ordered_buckets(){info!("{:?}",bucket);}assert!(response.verbose.is_some());info!("{:?}",response.verbose);})
            }
            KaspadPayloadOps::GetUtxoReturnAddress => {
                letrpc_client = client.clone();
                tst!(op, {
                    letresults = rpc_client.get_utxo_return_address(RpcHash::from_bytes([0; 32]), 1000).await;
                    assert!(results.is_err_and(|err|{matcherr{kaspa_rpc_core::RpcError::General(msg)=>{info!("Expected error message: {}",msg);true}_=>false,}}));
                })
            }
            KaspadPayloadOps::NotifyBlockAdded => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, BlockAddedScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyNewBlockTemplate => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, NewBlockTemplateScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyFinalityConflict => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, FinalityConflictScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyUtxosChanged => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, UtxosChangedScope::new(vec![]).into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifySinkBlueScoreChanged => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, SinkBlueScoreChangedScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyPruningPointUtxoSetOverride => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, PruningPointUtxoSetOverrideScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyVirtualDaaScoreChanged => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.start_notify(id, VirtualDaaScoreChangedScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::NotifyVirtualChainChanged => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client
                        .start_notify(id, VirtualChainChangedScope { include_accepted_transaction_ids: false }.into())
                        .await
                        .unwrap();
                })
            }
            KaspadPayloadOps::StopNotifyingUtxosChanged => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.stop_notify(id, UtxosChangedScope::new(vec![]).into()).await.unwrap();
                })
            }
            KaspadPayloadOps::StopNotifyingPruningPointUtxoSetOverride => {
                letrpc_client = client.clone();
                letid = listener_id;
                tst!(op, {
                    rpc_client.stop_notify(id, PruningPointUtxoSetOverrideScope {}.into()).await.unwrap();
                })
            }
            KaspadPayloadOps::GetPruningWindowRoots => todo!(),
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
