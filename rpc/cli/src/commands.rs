//! Per-method RPC command modules.
//!
//! Each method lives in its own module exposing a `clap::Args` struct that
//! implements [`RpcCommand`], whose associated `Output` is the method's typed
//! response. See [`get_info`] for the canonical template.

use kaspa_rpc_core::api::rpc::RpcApi;
use std::sync::Arc;

/// A runnable RPC command: builds and issues one request, yielding a
/// serializable response that the engine renders to stdout.
pub trait RpcCommand {
    /// The typed response produced by [`run`](RpcCommand::run).
    type Output: serde::Serialize;

    fn run(&self, client: &Arc<dyn RpcApi>) -> impl Future<Output = crate::error::Result<Self::Output>>;
}

pub mod add_peer;
pub mod ban;
pub mod call;
pub mod estimate_network_hashes_per_second;
pub mod get_balance_by_address;
pub mod get_balances_by_addresses;
pub mod get_block;
pub mod get_block_count;
pub mod get_block_dag_info;
pub mod get_block_reward_info;
pub mod get_block_template;
pub mod get_blocks;
pub mod get_coin_supply;
pub mod get_connected_peer_info;
pub mod get_connections;
pub mod get_current_block_color;
pub mod get_current_network;
pub mod get_daa_score_timestamp_estimate;
pub mod get_fee_estimate;
pub mod get_fee_estimate_experimental;
pub mod get_headers;
pub mod get_info;
pub mod get_mempool_entries;
pub mod get_mempool_entries_by_addresses;
pub mod get_mempool_entry;
pub mod get_metrics;
pub mod get_peer_addresses;
pub mod get_seq_commit_lane_proof;
pub mod get_server_info;
pub mod get_sink;
pub mod get_sink_blue_score;
pub mod get_subnetwork;
pub mod get_sync_status;
pub mod get_system_info;
pub mod get_utxo_return_address;
pub mod get_utxos_by_addresses;
pub mod get_virtual_chain_from_block;
pub mod get_virtual_chain_from_block_v2;
pub mod ping;
pub mod resolve_finality_conflict;
pub mod shutdown;
pub mod submit_block;
pub mod submit_transaction;
pub mod submit_transaction_replacement;
pub mod subscribe;
pub mod unban;
