use crate::imports::*;
use convert_case::{Case, Casing};
use kaspa_rpc_core::{api::ops::RpcApiOps, *};

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
                let result = rpc.get_metrics(true, true, true, true).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetServerInfo => {
                let result = rpc.get_server_info_call(GetServerInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSyncStatus => {
                let result = rpc.get_sync_status_call(GetSyncStatusRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetCurrentNetwork => {
                let result = rpc.get_current_network_call(GetCurrentNetworkRequest {}).await?;
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
                let result = rpc.get_peer_addresses_call(GetPeerAddressesRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSink => {
                let result = rpc.get_sink_call(GetSinkRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::GetMempoolEntry => {
            //     let result = rpc.get_mempool_entry_call(GetMempoolEntryRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetMempoolEntries => {
                // TODO
                let result = rpc
                    .get_mempool_entries_call(GetMempoolEntriesRequest { include_orphan_pool: true, filter_transaction_pool: true })
                    .await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetConnectedPeerInfo => {
                let result = rpc.get_connected_peer_info_call(GetConnectedPeerInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::AddPeer => {
                if argv.is_empty() {
                    return Err(Error::custom("Usage: rpc addpeer <ip:port> [true|false for 'is_permanent']"));
                }
                let peer_address = argv.remove(0).parse::<RpcContextualPeerAddress>()?;
                let is_permanent = argv.remove(0).parse::<bool>().unwrap_or(false);
                let result = rpc.add_peer_call(AddPeerRequest { peer_address, is_permanent }).await?;
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
                let result = rpc.get_block_call(GetBlockRequest { hash, include_transactions: true }).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::GetSubnetwork => {
            //     let result = rpc.get_subnetwork_call(GetSubnetworkRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            // RpcApiOps::GetVirtualChainFromBlock => {
            //     let result = rpc.get_virtual_chain_from_block_call(GetVirtualChainFromBlockRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            // RpcApiOps::GetBlocks => {
            //     let result = rpc.get_blocks_call(GetBlocksRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::GetBlockCount => {
                let result = rpc.get_block_count_call(GetBlockCountRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetBlockDagInfo => {
                let result = rpc.get_block_dag_info_call(GetBlockDagInfoRequest {}).await?;
                self.println(&ctx, result);
            }
            // RpcApiOps::ResolveFinalityConflict => {
            //     let result = rpc.resolve_finality_conflict_call(ResolveFinalityConflictRequest {  }).await?;
            //     self.println(&ctx, result);
            // }
            RpcApiOps::Shutdown => {
                let result = rpc.shutdown_call(ShutdownRequest {}).await?;
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
                let result = rpc.get_utxos_by_addresses_call(GetUtxosByAddressesRequest { addresses }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetBalanceByAddress => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify at least one address"));
                }
                let addresses = argv.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                for address in addresses {
                    let result = rpc.get_balance_by_address_call(GetBalanceByAddressRequest { address }).await?;
                    self.println(&ctx, sompi_to_kaspa(result.balance));
                }
            }
            RpcApiOps::GetBalancesByAddresses => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify at least one address"));
                }
                let addresses = argv.iter().map(|s| Address::try_from(s.as_str())).collect::<std::result::Result<Vec<_>, _>>()?;
                let result = rpc.get_balances_by_addresses_call(GetBalancesByAddressesRequest { addresses }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetSinkBlueScore => {
                let result = rpc.get_sink_blue_score_call(GetSinkBlueScoreRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::Ban => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify peer IP address"));
                }
                let ip: RpcIpAddress = argv.remove(0).parse()?;
                let result = rpc.ban_call(BanRequest { ip }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::Unban => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify peer IP address"));
                }
                let ip: RpcIpAddress = argv.remove(0).parse()?;
                let result = rpc.unban_call(UnbanRequest { ip }).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetInfo => {
                let result = rpc.get_info_call(GetInfoRequest {}).await?;
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
                    .get_mempool_entries_by_addresses_call(GetMempoolEntriesByAddressesRequest {
                        addresses,
                        include_orphan_pool,
                        filter_transaction_pool,
                    })
                    .await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetCoinSupply => {
                let result = rpc.get_coin_supply_call(GetCoinSupplyRequest {}).await?;
                self.println(&ctx, result);
            }
            RpcApiOps::GetDaaScoreTimestampEstimate => {
                if argv.is_empty() {
                    return Err(Error::custom("Please specify a daa_score"));
                }
                let daa_score_result = argv.iter().map(|s| s.parse::<u64>()).collect::<std::result::Result<Vec<_>, _>>();

                match daa_score_result {
                    Ok(daa_scores) => {
                        let result =
                            rpc.get_daa_score_timestamp_estimate_call(GetDaaScoreTimestampEstimateRequest { daa_scores }).await?;
                        self.println(&ctx, result);
                    }
                    Err(_err) => {
                        return Err(Error::custom("Could not parse daa_scores to u64"));
                    }
                }
            }
            RpcApiOps::GetTransactionData => {
                if argv.is_empty() {
                    return Err(Error::custom("Missing transaction Ids to query"));
                }
                let transaction_ids = argv
                    .iter()
                    .map(|transaction_id| RpcHash::from_hex(transaction_id.as_str()))
                    .take_while(|res| res.is_ok())
                    .map(|res| res.unwrap()) // safe to unwrap because of `take_while` condition. 
                    .collect::<Vec<_>>();
                if argv.len() - transaction_ids.len() > 4 {
                    return Err(Error::custom("Too many arguments"));
                }
                let argv_index_offset = transaction_ids.len();
                let result = rpc
                    .get_transaction_data_call(GetTransactionDataRequest {
                        transaction_ids,
                        include_transactions: try_extract_bool(&argv, argv_index_offset, "include_transactions")?,
                        include_acceptance_data: try_extract_bool(&argv, argv_index_offset + 1, "include_acceptance_data")?,
                        include_inclusion_data: try_extract_bool(&argv, argv_index_offset + 2, "include_inclusion_data")?,
                        include_verbose_data: try_extract_bool(&argv, argv_index_offset + 3, "include_verbose_data")?,
                    })
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
        let help = RpcApiOps::list()
            .iter()
            .filter_map(|op| op.doc().is_not_empty().then_some((op.as_str().to_case(Case::Kebab).to_string(), op.doc())))
            .collect::<Vec<(_, _)>>();

        ctx.term().help(&help, None)?;

        tprintln!(ctx);
        tprintln!(ctx, "Please note that not all listed RPC methods are currently implemented");
        tprintln!(ctx);

        Ok(())
    }
}

// argv extractors:

/// will try and extract a bool from the argv at the given index,
/// if the index is out of bounds it will return the default bool value (i.e. `false`),
/// if parsing at the index returns an error, it will return an error, with indication of the field name.
#[inline]
fn try_extract_bool(argv: &[String], index: usize, field_name: &str) -> Result<bool> {
    argv.get(index)
        .unwrap_or(&bool::default().to_string())
        .parse::<bool>()
        .map_err(|err| Error::custom(format!("Could not parse `{0}` to bool: {1}", field_name, err)))
}
