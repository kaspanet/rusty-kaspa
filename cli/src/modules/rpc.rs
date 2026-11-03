use crate::imports::*;
use convert_case::{Case, Casing};
use kaspa_rpc_core::api::ops::RpcApiOps;

#[derive(Default, Handler)]
#[help("Execute RPC commands against the connected Kaspa node")]
pub struct Rpc;

impl Rpc {
    fn println<T>(&self, ctx: &Arc<KaspaCli>, v: T)
    where
        T: core::fmt::Debug,
    {
        ctx.term().writeln(format!("{v:#?}").crlf());
    }

    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let rpc = ctx.wallet().rpc_api().clone();
        // tprintln!(ctx, "{response}");

        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        let op_str = argv.remove(0);

        let sanitize = regex::Regex::new(r"\s*rpc\s+\S+\s+").unwrap();
        let _args = sanitize.replace(cmd, "").trim().to_string();
        let op_str_uc = op_str.to_case(Case::UpperCamel).to_string();
        // tprintln!(ctx, "uc: '{op_str_uc}'");

        let op = RpcApiOps::from_str(op_str_uc.as_str()).ok_or(Error::custom(format!("No such rpc method: '{op_str}'")))?;

        match op {
            RpcApiOps::Ping => {
                rpc.ping().await?;
                tprintln!(ctx, "ok");
            }
            RpcApiOps::GetMetrics => {
                let result = rpc.get_metrics(true, true, true, true, true, true).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSystemInfo => {
                let result = rpc.get_system_info().await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetConnections => {
                let result = rpc.get_connections(true).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetServerInfo => {
                let result = rpc.get_server_info_call(None, GetServerInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSyncStatus => {
                let result = rpc.get_sync_status_call(None, GetSyncStatusRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetCurrentNetwork => {
                let result = rpc.get_current_network_call(None, GetCurrentNetworkRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::SubmitBlock => {
            //     let result = rpc.submit_block_call(SubmitBlockRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            // RpcApiOps::GetBlockTemplate => {
            //     let result = rpc.get_block_template_call(GetBlockTemplateRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetPeerAddresses => {
                let result = rpc.get_peer_addresses_call(None, GetPeerAddressesRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSink => {
                let result = rpc.get_sink_call(None, GetSinkRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::GetMempoolEntry => {
            //     let result = rpc.get_mempool_entry_call(GetMempoolEntryRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetMempoolEntries => {
                // TODO
                let result = rpc
                    .get_mempool_entries_call(
                        None,
                        GetMempoolEntriesRequest { include_orphan_pool: true, filter_transaction_pool: true },
                    )
                    .await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetConnectedPeerInfo => {
                let result = rpc.get_connected_peer_info_call(None, GetConnectedPeerInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::AddPeer => {
                if argv.is_empty() {
                    return Err(Error::custom("Usage: rpc addpeer <ip:port> [true|false for 'is_permanent']"));
                }
                let peer_address = argv.remove(0).parse::<RpcContextualPeerAddress>()?;
                let is_permanent = argv.remove(0).parse::<bool>().unwrap_or(false);
                let result = rpc.add_peer_call(None, AddPeerRequest { peer_address, is_permanent }).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::SubmitTransaction => {
            //     let result = rpc.submit_transaction_call(SubmitTransactionRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetBlock => {
                if argv.is_empty() {
                    return Err(Error::custom("Missing block hash argument"));
                }
                let hash = argv.remove(0);
                let hash = RpcHash::from_hex(hash.as_str())?;
                let include_transactions = argv.first().and_then(|x| x.parse::<bool>().ok()).unwrap_or(true);
                let result = rpc.get_block_call(None, GetBlockRequest { hash, include_transactions }).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::GetSubnetwork => {
            //     let result = rpc.get_subnetwork_call(GetSubnetworkRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetVirtualChainFromBlock => {
                if argv.is_empty() {
                    return Err(Error::custom("Missing startHash argument"));
                };
                let start_hash = RpcHash::from_hex(argv.remove(0).as_str())?;
                let include_accepted_transaction_ids = argv.first().and_then(|x| x.parse::<bool>().ok()).unwrap_or_default();
                let min_confirmation_count = argv.get(1).and_then(|x| x.parse::<u64>().ok()).unwrap_or_default();

                let result = rpc
                    .get_virtual_chain_from_block_call(
                        None,
                        GetVirtualChainFromBlockRequest {
                            start_hash,
                            include_accepted_transaction_ids,
                            min_confirmation_count: Some(min_confirmation_count),
                        },
                    )
                    .await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::GetBlocks => {
            //     let result = rpc.get_blocks_call(GetBlocksRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetBlockCount => {
                let result = rpc.get_block_count_call(None, GetBlockCountRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetBlockDagInfo => {
                let result = rpc.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::ResolveFinalityConflict => {
            //     let result = rpc.resolve_finality_conflict_call(ResolveFinalityConflictRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::Shutdown => {
                let result = rpc.shutdown_call(None, ShutdownRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::GetHeaders => {
            //     let result = rpc.get_headers_call(GetHeadersRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetUtxosByAddresses => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify at least one address"));
                }
                let addresses = argv.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                let result = rpc.get_utxos_by_addresses_call(None, GetUtxosByAddressesRequest { addresses }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetBalanceByAddress => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify at least one address"));
                }
                let addresses = argv.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                for address in addresses {
                    let result = rpc.get_balance_by_address_call(None, GetBalanceByAddressRequest { address }).await?;
                    self.println(&ctx, sompi_to_kaspa(result.balance));
                }
            }
            RpcApiOps::GetBalancesByAddresses => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify at least one address"));
                }
                let addresses = argv.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                let result = rpc.get_balances_by_addresses_call(None, GetBalancesByAddressesRequest { addresses }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSinkBlueScore => {
                let result = rpc.get_sink_blue_score_call(None, GetSinkBlueScoreRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::Ban => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify peer IP address"));
                }
                let ip: RpcIpAddress = argv.remove(0).parse()?;
                let result = rpc.ban_call(None, BanRequest { ip }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::Unban => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify peer IP address"));
                }
                let ip: RpcIpAddress = argv.remove(0).parse()?;
                let result = rpc.unban_call(None, UnbanRequest { ip }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetInfo => {
                let result = rpc.get_info_call(None, GetInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::EstimateNetworkHashesPerSecond => {
            //     let result = rpc.estimate_network_hashes_per_second_call(EstimateNetworkHashesPerSecondRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetMempoolEntriesByAddresses => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify at least one address"));
                }
                let addresses = argv.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                let include_orphan_pool = true;
                let filter_transaction_pool = true;
                let result = rpc
                    .get_mempool_entries_by_addresses_call(
                        None,
                        GetMempoolEntriesByAddressesRequest { addresses, include_orphan_pool, filter_transaction_pool },
                    )
                    .await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetCoinSupply => {
                let result = rpc.get_coin_supply_call(None, GetCoinSupplyRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetDaaScoreTimestampEstimate => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify a daa_score"));
                }
                let daa_score_result = argv.iter().map(|s| s.parse::<u64>()).collect::<std::result::Result<Vec<_>, _>>();

                match daa_score_result {
                    Ok(daa_scores) => {
                        let result = rpc
                            .get_daa_score_timestamp_estimate_call(None, GetDaaScoreTimestampEstimateRequest { daa_scores })
                            .await?;
                        self.println(&ctx, result);
                    }
                    Err(_err) => {
                        return Err(Error::custom("Could not parse daa_scores to u64"));
                    }
                }
            }
            RpcApiOps::GetFeeEstimate => {
                let result = rpc.get_fee_estimate_call(None, GetFeeEstimateRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetFeeEstimateExperimental => {
                let verbose = if argv.is_empty() { false } else { argv.remove(0).parse().unwrap_or(false) };
                let result = rpc.get_fee_estimate_experimental_call(None, GetFeeEstimateExperimentalRequest { verbose }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetCurrentBlockColor => {
                if argv.is_empty() {
                    return Err(Error::custom("Missing block hash argument"));
                }
                let hash = argv.remove(0);
                let hash = RpcHash::from_hex(hash.as_str())?;
                let result = rpc.get_current_block_color_call(None, GetCurrentBlockColorRequest { hash }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetBlockRewardInfo => {
                if argv.is_empty() {
                    return Err(Error::custom("Missing block hash argument"));
                }
                let hash = argv.remove(0);
                let hash = RpcHash::from_hex(hash.as_str())?;
                let result = rpc.get_block_reward_info_call(None, GetBlockRewardInfoRequest::new(hash)).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetUtxoReturnAddress => {
                if argv.is_empty() || argv.len() != 2 {
                    return Err(Error::custom("Please specify a txid and a accepting_block_daa_score"));
                }

                let txid = argv.remove(0);
                let txid = RpcHash::from_hex(txid.as_str())?;

                let accepting_block_daa_score = argv.remove(0).parse::<u64>()?;

                let result =
                    rpc.get_utxo_return_address_call(None, GetUtxoReturnAddressRequest { txid, accepting_block_daa_score }).await?;

                self.println(&ctx, result);
            }
            RpcApiOps::GetVirtualChainFromBlockV2 => {
                if argv.is_empty() {
                    return Err(Error::custom("Missing startHash argument"));
                };

                let start_hash = RpcHash::from_hex(argv.remove(0).as_str())?;

                let verbosity_level_i32 = argv.pop().and_then(|arg| arg.parse::<i32>().ok()).unwrap_or_default();
                let verbosity_level = RpcDataVerbosityLevel::try_from(verbosity_level_i32)?;

                let result = rpc
                    .get_virtual_chain_from_block_v2_call(
                        None,
                        GetVirtualChainFromBlockV2Request {
                            start_hash,
                            data_verbosity_level: Some(verbosity_level),
                            min_confirmation_count: None,
                        },
                    )
                    .await;

                self.println(&ctx, result);
            }
            RpcApiOps::GetUtxosByAddressesV2 => {
                let mut args = argv;

                if args.len() < 8 {
                    return Err(Error::custom(
                        "Usage: rpc get-utxos-by-addresses-v2 <addr...> <from_daa_score|None> <to_daa_score|None> <start_address|None(defaults-to-first-address)> <start_daa_score|None> <start_outpoint_transaction_id|None> <start_outpoint_index|None> <limit|None>",
                    ));
                }

                let parse_optional_usize = |value: String| -> Result<Option<usize>> {
                    if value.eq_ignore_ascii_case("none") { Ok(None) } else { Ok(Some(value.parse::<usize>()?)) }
                };

                let parse_optional_u32 = |value: String| -> Result<Option<u32>> {
                    if value.eq_ignore_ascii_case("none") { Ok(None) } else { Ok(Some(value.parse::<u32>()?)) }
                };

                let parse_optional_u64 = |value: String| -> Result<Option<u64>> {
                    if value.eq_ignore_ascii_case("none") { Ok(None) } else { Ok(Some(value.parse::<u64>()?)) }
                };

                let parse_optional_rpc_hash = |value: String| -> Result<Option<RpcHash>> {
                    if value.eq_ignore_ascii_case("none") { Ok(None) } else { Ok(Some(RpcHash::from_hex(value.as_str())?)) }
                };

                let parse_optional_address = |value: String| -> Result<Option<Address>> {
                    if value.eq_ignore_ascii_case("none") { Ok(None) } else { Ok(Some(Address::try_from(value.as_str())?)) }
                };

                let limit = parse_optional_usize(args.pop().unwrap())?;
                let start_outpoint_index = parse_optional_u32(args.pop().unwrap())?;
                let start_outpoint_transaction_id = parse_optional_rpc_hash(args.pop().unwrap())?;
                let start_daa_score = parse_optional_u64(args.pop().unwrap())?;
                let start_address = parse_optional_address(args.pop().unwrap())?;
                let to_daa_score = parse_optional_u64(args.pop().unwrap())?;
                let from_daa_score = parse_optional_u64(args.pop().unwrap())?;

                let cursor = if start_outpoint_index.is_none()
                    && start_outpoint_transaction_id.is_none()
                    && start_address.is_none()
                    && start_daa_score.is_none()
                {
                    None
                } else if start_outpoint_index.is_some() && start_outpoint_transaction_id.is_some() && start_daa_score.is_some() {
                    Some(RpcGetUtxosByAddressesCursor::new(
                        start_address,
                        start_daa_score.ok_or(RpcError::General("no start_daa_score specified for cursor".to_string()))?,
                        Some(RpcTransactionOutpoint {
                            transaction_id: start_outpoint_transaction_id
                                .ok_or(RpcError::General("no start_outpoint_transaction_id specified for cursor".to_string()))?,
                            index: start_outpoint_index
                                .ok_or(RpcError::General("no start_outpoint_index specified for cursor".to_string()))?,
                        }),
                    ))
                } else {
                    return Err(Error::custom(
                        "incomplete cursor parameters; start_address may be None, but start_daa_score and start_outpoint must be provided together".to_string(),
                    ));
                };

                let addresses = args.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                let result = rpc
                    .get_utxos_by_addresses_v2_call(
                        None,
                        GetUtxosByAddressesV2Request::new(addresses, from_daa_score, to_daa_score, cursor, limit),
                    )
                    .await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetUtxosByCovenantIds => {
                use std::str::FromStr as _;

                let args = argv;

                const USAGE: &str = "Usage: rpc get-utxos-by-covenant-ids <covenant_id>... [--from-daa=<from_daa_score>] [--to-daa=<to_daa_score>] [--spks=<script_public_key_csv>] [--start-covenant-id=<covenant_id>] [--start-daa-score=<daa_score>] [--start-spk=<script_public_key>] [--start-tx-id=<transaction_id>] [--start-outpoint-index=<index>] [--budget=<budget>]";

                let mut covenant_ids = Vec::new();
                let mut from_daa_score = None;
                let mut to_daa_score = None;
                let mut script_public_keys = Vec::new();
                let mut start_covenant_id = None;
                let mut start_daa_score = None;
                let mut start_script_public_key = None;
                let mut start_outpoint_transaction_id = None;
                let mut start_outpoint_index = None;
                let mut budget = None;

                for arg in args {
                    let Some((flag, value)) = arg.strip_prefix("--").and_then(|flag| flag.split_once('=')) else {
                        covenant_ids.push(RpcHash::from_hex(arg.as_str())?);
                        continue;
                    };
                    match flag {
                        "from-daa" => from_daa_score = Some(value.parse::<u64>()?),
                        "to-daa" => to_daa_score = Some(value.parse::<u64>()?),
                        "spks" => {
                            script_public_keys = value
                                .split(',')
                                .map(|s| RpcScriptPublicKey::from_str(s.trim()))
                                .collect::<std::result::Result<Vec<_>, _>>()?
                        }
                        "start-covenant-id" => start_covenant_id = Some(RpcHash::from_hex(value)?),
                        "start-daa-score" => start_daa_score = Some(value.parse::<u64>()?),
                        "start-spk" => start_script_public_key = Some(RpcScriptPublicKey::from_str(value)?),
                        "start-tx-id" => start_outpoint_transaction_id = Some(RpcHash::from_hex(value)?),
                        "start-outpoint-index" => start_outpoint_index = Some(value.parse::<u32>()?),
                        "budget" => budget = Some(value.parse::<usize>()?),
                        _ => return Err(Error::custom(USAGE)),
                    }
                }
                if covenant_ids.is_empty() {
                    return Err(Error::custom(USAGE));
                }

                let start_outpoint = match (start_outpoint_transaction_id, start_outpoint_index) {
                    (Some(transaction_id), Some(index)) => Some(RpcTransactionOutpoint { transaction_id, index }),
                    (None, None) => None,
                    _ => {
                        return Err(Error::custom(
                            "incomplete cursor parameters; start-tx-id and start-outpoint-index must be provided together".to_string(),
                        ));
                    }
                };
                // a request carries either a full cursor or none at all
                let cursor = match (start_covenant_id, start_daa_score, start_script_public_key, start_outpoint) {
                    (None, None, None, None) => None,
                    (Some(start_covenant_id), Some(start_daa_score), Some(start_script_public_key), Some(start_outpoint)) => {
                        Some(RpcGetUtxosByCovenantIdsCursor::new(
                            start_covenant_id,
                            start_daa_score,
                            start_script_public_key,
                            start_outpoint,
                        ))
                    }
                    _ => return Err(Error::custom("incomplete cursor parameters; provide all cursor fields or none".to_string())),
                };

                let result = rpc
                    .get_utxos_by_covenant_ids_call(
                        None,
                        GetUtxosByCovenantIdsRequest::new(
                            covenant_ids,
                            script_public_keys,
                            from_daa_score,
                            to_daa_score,
                            cursor,
                            budget,
                        ),
                    )
                    .await?;
                self.println(&ctx, result);
            }
            _ => {
                tprintln!(ctx, "rpc method exists but is not supported by the cli: '{op_str}'\r\n");
                return Ok(());
            }
        }

        let prefix = Regex::new(r"(?i)^\s*rpc\s+\S+\s+").unwrap();
        let _req = prefix.replace(cmd, "").trim().to_string();

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        // RpcApiOps that do not contain docs are not displayed
        let help = RpcApiOps::into_iter()
            .filter_map(|op| op.rustdoc().is_not_empty().then_some((op.as_str().to_case(Case::Kebab).to_string(), op.rustdoc())))
            .collect::<Vec<(_, _)>>();

        ctx.term().help(&help, None)?;

        tprintln!(ctx);
        tprintln!(ctx, "Please note that not all listed RPC methods are currently implemented");
        tprintln!(ctx);

        Ok(())
    }
}
