//! `call` command: a generic escape hatch that invokes any covered RPC method
//! by name, deserializing the request parameters from JSON.
//!
//! Parameters are provided as inline JSON, or `@file` / `@-` (stdin) via
//! [`crate::args::json_arg`]. When omitted, an empty object is used, which is
//! sufficient for parameter-less methods. The method name accepts either the
//! kebab-case (`get-block-dag-info`) or snake-case (`get_block_dag_info`) form.

use crate::args::json_arg;
use crate::commands::RpcCommand;
use crate::error::{CliError, Result};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::*;
use serde::de::DeserializeOwned;
use std::sync::Arc;

/// Invoke any covered RPC method by name with JSON parameters.
#[derive(clap::Args, Debug)]
pub struct Call {
    /// RPC method name (kebab- or snake-case), e.g. `get-block-dag-info`.
    pub method: String,

    /// Request parameters as inline JSON, or `@file` / `@-` for a file/stdin.
    /// Omit for parameter-less methods.
    pub params: Option<String>,
}

/// Deserialize the request parameters, defaulting to an empty object.
fn parse_params<T: DeserializeOwned>(params: &Option<String>) -> Result<T> {
    let value = match params {
        Some(s) => json_arg::<serde_json::Value>(s)?,
        None => serde_json::Value::Object(Default::default()),
    };
    serde_json::from_value(value).map_err(|e| CliError::Usage(format!("invalid params: {e}")))
}

/// Expand to: parse params into `$req`, invoke `$method`, serialize the response.
macro_rules! call {
    ($client:expr, $params:expr, $req:ty, $method:ident) => {{
        let request: $req = parse_params($params)?;
        let response = $client.$method(None, request).await?;
        serde_json::to_value(response)?
    }};
}

impl RpcCommand for Call {
    type Output = serde_json::Value;

    async fn run(&self, client: &Arc<dyn RpcApi>) -> Result<Self::Output> {
        let method = self.method.replace('_', "-");
        let params = &self.params;
        let value = match method.as_str() {
            "get-info" => call!(client, params, GetInfoRequest, get_info_call),
            "get-server-info" => call!(client, params, GetServerInfoRequest, get_server_info_call),
            "get-sync-status" => call!(client, params, GetSyncStatusRequest, get_sync_status_call),
            "get-current-network" => call!(client, params, GetCurrentNetworkRequest, get_current_network_call),
            "get-system-info" => call!(client, params, GetSystemInfoRequest, get_system_info_call),
            "get-connections" => call!(client, params, GetConnectionsRequest, get_connections_call),
            "get-metrics" => call!(client, params, GetMetricsRequest, get_metrics_call),
            "get-block-dag-info" => call!(client, params, GetBlockDagInfoRequest, get_block_dag_info_call),
            "get-block-count" => call!(client, params, GetBlockCountRequest, get_block_count_call),
            "get-coin-supply" => call!(client, params, GetCoinSupplyRequest, get_coin_supply_call),
            "get-sink" => call!(client, params, GetSinkRequest, get_sink_call),
            "get-sink-blue-score" => call!(client, params, GetSinkBlueScoreRequest, get_sink_blue_score_call),
            "get-connected-peer-info" => call!(client, params, GetConnectedPeerInfoRequest, get_connected_peer_info_call),
            "get-peer-addresses" => call!(client, params, GetPeerAddressesRequest, get_peer_addresses_call),
            "get-fee-estimate" => call!(client, params, GetFeeEstimateRequest, get_fee_estimate_call),
            "get-fee-estimate-experimental" => {
                call!(client, params, GetFeeEstimateExperimentalRequest, get_fee_estimate_experimental_call)
            }
            "ping" => call!(client, params, PingRequest, ping_call),
            "get-block" => call!(client, params, GetBlockRequest, get_block_call),
            "get-blocks" => call!(client, params, GetBlocksRequest, get_blocks_call),
            "get-headers" => call!(client, params, GetHeadersRequest, get_headers_call),
            "get-current-block-color" => call!(client, params, GetCurrentBlockColorRequest, get_current_block_color_call),
            "get-virtual-chain-from-block" => {
                call!(client, params, GetVirtualChainFromBlockRequest, get_virtual_chain_from_block_call)
            }
            "get-subnetwork" => call!(client, params, GetSubnetworkRequest, get_subnetwork_call),
            "get-daa-score-timestamp-estimate" => {
                call!(client, params, GetDaaScoreTimestampEstimateRequest, get_daa_score_timestamp_estimate_call)
            }
            "estimate-network-hashes-per-second" => {
                call!(client, params, EstimateNetworkHashesPerSecondRequest, estimate_network_hashes_per_second_call)
            }
            "get-mempool-entry" => call!(client, params, GetMempoolEntryRequest, get_mempool_entry_call),
            "get-mempool-entries" => call!(client, params, GetMempoolEntriesRequest, get_mempool_entries_call),
            "get-mempool-entries-by-addresses" => {
                call!(client, params, GetMempoolEntriesByAddressesRequest, get_mempool_entries_by_addresses_call)
            }
            "get-utxos-by-addresses" => call!(client, params, GetUtxosByAddressesRequest, get_utxos_by_addresses_call),
            "get-balance-by-address" => call!(client, params, GetBalanceByAddressRequest, get_balance_by_address_call),
            "get-balances-by-addresses" => {
                call!(client, params, GetBalancesByAddressesRequest, get_balances_by_addresses_call)
            }
            "get-utxo-return-address" => call!(client, params, GetUtxoReturnAddressRequest, get_utxo_return_address_call),
            "get-block-reward-info" => call!(client, params, GetBlockRewardInfoRequest, get_block_reward_info_call),
            other => return Err(CliError::Usage(format!("unknown method '{other}'"))),
        };
        Ok(value)
    }
}
