use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::ProcessMetrics, protowire::ProcessMetrics, {
    Self {
        resident_set_size: item.resident_set_size,
        virtual_memory_size: item.virtual_memory_size,
        core_num: item.core_num,
        cpu_usage: item.cpu_usage,
        fd_num: item.fd_num,
        disk_io_read_bytes: item.disk_io_read_bytes,
        disk_io_write_bytes: item.disk_io_write_bytes,
        disk_io_read_per_sec: item.disk_io_read_per_sec,
        disk_io_write_per_sec: item.disk_io_write_per_sec,
    }
});

from!(item: &kaspa_rpc_core::ConnectionMetrics, protowire::ConnectionMetrics, {
    Self {
        borsh_live_connections: item.borsh_live_connections,
        borsh_connection_attempts: item.borsh_connection_attempts,
        borsh_handshake_failures: item.borsh_handshake_failures,
        json_live_connections: item.json_live_connections,
        json_connection_attempts: item.json_connection_attempts,
        json_handshake_failures: item.json_handshake_failures,
        active_peers: item.active_peers,
    }
});

from!(item: &kaspa_rpc_core::ConsensusMetrics, protowire::ConsensusMetrics, {
    Self {
        blocks_submitted: item.node_blocks_submitted_count,
        header_counts: item.node_headers_processed_count,
        dep_counts: item.node_dependencies_processed_count,
        body_counts: item.node_bodies_processed_count,
        txs_counts: item.node_transactions_processed_count,
        chain_block_counts: item.node_chain_blocks_processed_count,
        mass_counts: item.node_mass_processed_count,

        block_count: item.node_database_blocks_count,
        header_count: item.node_database_headers_count,
        tip_hashes_count: item.network_tip_hashes_count,
        difficulty: item.network_difficulty,
        past_median_time: item.network_past_median_time,
        virtual_parent_hashes_count: item.network_virtual_parent_hashes_count,
        virtual_daa_score: item.network_virtual_daa_score,
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::ProcessMetrics, kaspa_rpc_core::ProcessMetrics, {
    Self {
        resident_set_size: item.resident_set_size,
        virtual_memory_size: item.virtual_memory_size,
        core_num: item.core_num,
        cpu_usage: item.cpu_usage,
        fd_num: item.fd_num,
        disk_io_read_bytes: item.disk_io_read_bytes,
        disk_io_write_bytes: item.disk_io_write_bytes,
        disk_io_read_per_sec: item.disk_io_read_per_sec,
        disk_io_write_per_sec: item.disk_io_write_per_sec,
    }
});

try_from!(item: &protowire::ConnectionMetrics, kaspa_rpc_core::ConnectionMetrics, {
    Self {
        borsh_live_connections: item.borsh_live_connections,
        borsh_connection_attempts: item.borsh_connection_attempts,
        borsh_handshake_failures: item.borsh_handshake_failures,
        json_live_connections: item.json_live_connections,
        json_connection_attempts: item.json_connection_attempts,
        json_handshake_failures: item.json_handshake_failures,
        active_peers: item.active_peers,
    }
});

try_from!(item: &protowire::ConsensusMetrics, kaspa_rpc_core::ConsensusMetrics, {
    Self {
        node_blocks_submitted_count: item.blocks_submitted,
        node_headers_processed_count: item.header_counts,
        node_dependencies_processed_count: item.dep_counts,
        node_bodies_processed_count: item.body_counts,
        node_transactions_processed_count: item.txs_counts,
        node_chain_blocks_processed_count: item.chain_block_counts,
        node_mass_processed_count: item.mass_counts,

        node_database_blocks_count: item.block_count,
        node_database_headers_count: item.header_count,
        network_tip_hashes_count: item.tip_hashes_count,
        network_difficulty: item.difficulty,
        network_past_median_time: item.past_median_time,
        network_virtual_parent_hashes_count: item.virtual_parent_hashes_count,
        network_virtual_daa_score: item.virtual_daa_score,
    }
});
