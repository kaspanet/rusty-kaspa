use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::RpcError;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::ConnectionsProfileData, protowire::ConnectionsProfileData, {
    Self {
        cpu_usage: item.cpu_usage as f64,
        memory_usage: item.memory_usage,

    }
});

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

from!(item: &kaspa_rpc_core::BandwidthMetrics, protowire::BandwidthMetrics, {
    Self {
        borsh_bytes_tx: item.borsh_bytes_tx,
        borsh_bytes_rx: item.borsh_bytes_rx,
        json_bytes_tx: item.json_bytes_tx,
        json_bytes_rx: item.json_bytes_rx,
        grpc_p2p_bytes_tx: item.p2p_bytes_tx,
        grpc_p2p_bytes_rx: item.p2p_bytes_rx,
        grpc_user_bytes_tx: item.grpc_bytes_tx,
        grpc_user_bytes_rx: item.grpc_bytes_rx,
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
        mempool_size: item.network_mempool_size,
        tip_hashes_count: item.network_tip_hashes_count,
        difficulty: item.network_difficulty,
        past_median_time: item.network_past_median_time,
        virtual_parent_hashes_count: item.network_virtual_parent_hashes_count,
        virtual_daa_score: item.network_virtual_daa_score,
    }
});

from!(item: &kaspa_rpc_core::StorageMetrics, protowire::StorageMetrics, {
    Self {
        storage_size_bytes: item.storage_size_bytes,
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::ConnectionsProfileData, kaspa_rpc_core::ConnectionsProfileData, {
    Self { cpu_usage : item.cpu_usage as f32, memory_usage : item.memory_usage }
});

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

try_from!(item: &protowire::BandwidthMetrics, kaspa_rpc_core::BandwidthMetrics, {
    Self {
        borsh_bytes_tx: item.borsh_bytes_tx,
        borsh_bytes_rx: item.borsh_bytes_rx,
        json_bytes_tx: item.json_bytes_tx,
        json_bytes_rx: item.json_bytes_rx,
        p2p_bytes_tx: item.grpc_p2p_bytes_tx,
        p2p_bytes_rx: item.grpc_p2p_bytes_rx,
        grpc_bytes_tx: item.grpc_user_bytes_tx,
        grpc_bytes_rx: item.grpc_user_bytes_rx,
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
        network_mempool_size: item.mempool_size,
        network_tip_hashes_count: item.tip_hashes_count,
        network_difficulty: item.difficulty,
        network_past_median_time: item.past_median_time,
        network_virtual_parent_hashes_count: item.virtual_parent_hashes_count,
        network_virtual_daa_score: item.virtual_daa_score,
    }
});

try_from!(item: &protowire::StorageMetrics, kaspa_rpc_core::StorageMetrics, {
    Self {
        storage_size_bytes: item.storage_size_bytes,
    }
});
