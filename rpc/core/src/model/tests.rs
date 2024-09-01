#[cfg(test)]
mod mockery {

    use crate::{model::*, RpcScriptClass};
    use kaspa_addresses::{Prefix, Version};
    use kaspa_consensus_core::api::BlockCount;
    use kaspa_consensus_core::network::NetworkType;
    use kaspa_consensus_core::subnets::SubnetworkId;
    use kaspa_consensus_core::tx::ScriptPublicKey;
    use kaspa_hashes::Hash;
    use kaspa_math::Uint192;
    use kaspa_notify::subscription::Command;
    use kaspa_rpc_macros::test_wrpc_serializer as test;
    use kaspa_utils::networking::{ContextualNetAddress, IpAddress, NetAddress};
    use rand::Rng;
    use std::net::{IpAddr, Ipv4Addr};
    use std::sync::Arc;
    use uuid::Uuid;
    use workflow_serializer::prelude::*;

    // this trait is used to generate random
    // values for testing on various data types
    trait Mock {
        fn mock() -> Self;
    }

    impl<T> Mock for Option<T>
    where
        T: Mock,
    {
        fn mock() -> Self {
            Some(T::mock())
        }
    }

    impl<T> Mock for Vec<T>
    where
        T: Mock,
    {
        fn mock() -> Self {
            vec![T::mock()]
        }
    }

    impl<T> Mock for Arc<T>
    where
        T: Mock,
    {
        fn mock() -> Self {
            Arc::new(T::mock())
        }
    }

    fn mock<T>() -> T
    where
        T: Mock,
    {
        Mock::mock()
    }

    // this function tests serialization and deserialization of a type
    // by serializing it (A), deserializing it, serializing it again (B)
    // and comparing A and B buffers.
    fn test<T>(kind: &str)
    where
        T: Serializer + Deserializer + Mock,
    {
        let data = T::mock();

        const PREFIX: u32 = 0x12345678;
        const SUFFIX: u32 = 0x90abcdef;

        let mut buffer1 = Vec::new();
        let writer = &mut buffer1;
        store!(u32, &PREFIX, writer).unwrap();
        serialize!(T, &data, writer).unwrap();
        store!(u32, &SUFFIX, writer).unwrap();

        let reader = &mut buffer1.as_slice();
        let prefix: u32 = load!(u32, reader).unwrap();
        // this will never occur, but it's a good practice to check in case
        // the serialization/deserialization logic changes in the future
        assert_eq!(prefix, PREFIX, "misalignment when consuming serialized buffer in `{kind}`");
        let tmp = deserialize!(T, reader).unwrap();
        let suffix: u32 = load!(u32, reader).unwrap();
        assert_eq!(suffix, SUFFIX, "misalignment when consuming serialized buffer in `{kind}`");

        let mut buffer2 = Vec::new();
        let writer = &mut buffer2;
        store!(u32, &PREFIX, writer).unwrap();
        serialize!(T, &tmp, writer).unwrap();
        store!(u32, &SUFFIX, writer).unwrap();

        assert!(buffer1 == buffer2, "serialization/deserialization failure while testing `{kind}`");
    }

    #[macro_export]
    macro_rules! impl_mock {
        ($($type:ty),*) => {
            $(impl Mock for $type {
                fn mock() -> Self {
                    rand::thread_rng().gen()
                }
            })*
        };
    }

    impl_mock!(bool, u8, u16, u32, f32, u64, i64, f64);

    impl Mock for Uint192 {
        fn mock() -> Self {
            Uint192([mock(), mock(), mock()])
        }
    }

    impl Mock for SubnetworkId {
        fn mock() -> Self {
            let mut bytes: [u8; 20] = [0; 20];
            rand::thread_rng().fill(&mut bytes);
            SubnetworkId::from_bytes(bytes)
        }
    }

    impl Mock for Hash {
        fn mock() -> Self {
            let mut bytes: [u8; 32] = [0; 32];
            rand::thread_rng().fill(&mut bytes);
            Hash::from_bytes(bytes)
        }
    }

    impl Mock for RpcAddress {
        fn mock() -> Self {
            RpcAddress::new(Prefix::Mainnet, Version::PubKey, Hash::mock().as_bytes().as_slice())
        }
    }

    impl Mock for RpcHeader {
        fn mock() -> Self {
            RpcHeader {
                version: mock(),
                timestamp: mock(),
                bits: mock(),
                nonce: mock(),
                hash_merkle_root: mock(),
                accepted_id_merkle_root: mock(),
                utxo_commitment: mock(),
                hash: mock(),
                parents_by_level: vec![mock()],
                daa_score: mock(),
                blue_score: mock(),
                blue_work: mock(),
                pruning_point: mock(),
            }
        }
    }

    impl Mock for RpcRawHeader {
        fn mock() -> Self {
            RpcRawHeader {
                version: mock(),
                timestamp: mock(),
                bits: mock(),
                nonce: mock(),
                hash_merkle_root: mock(),
                accepted_id_merkle_root: mock(),
                utxo_commitment: mock(),
                parents_by_level: vec![mock()],
                daa_score: mock(),
                blue_score: mock(),
                blue_work: mock(),
                pruning_point: mock(),
            }
        }
    }

    impl Mock for RpcBlockVerboseData {
        fn mock() -> Self {
            RpcBlockVerboseData {
                hash: mock(),
                difficulty: mock(),
                selected_parent_hash: mock(),
                transaction_ids: mock(),
                is_header_only: mock(),
                blue_score: mock(),
                children_hashes: mock(),
                merge_set_blues_hashes: mock(),
                merge_set_reds_hashes: mock(),
                is_chain_block: mock(),
            }
        }
    }

    impl Mock for RpcBlock {
        fn mock() -> Self {
            RpcBlock { header: mock(), transactions: mock(), verbose_data: mock() }
        }
    }

    impl Mock for RpcRawBlock {
        fn mock() -> Self {
            RpcRawBlock { header: mock(), transactions: mock() }
        }
    }

    impl Mock for RpcTransactionInputVerboseData {
        fn mock() -> Self {
            RpcTransactionInputVerboseData {}
        }
    }

    impl Mock for RpcTransactionInput {
        fn mock() -> Self {
            RpcTransactionInput {
                previous_outpoint: mock(),
                signature_script: Hash::mock().as_bytes().to_vec(),
                sequence: mock(),
                sig_op_count: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcTransactionOutputVerboseData {
        fn mock() -> Self {
            RpcTransactionOutputVerboseData { script_public_key_type: RpcScriptClass::PubKey, script_public_key_address: mock() }
        }
    }

    impl Mock for RpcTransactionOutput {
        fn mock() -> Self {
            RpcTransactionOutput { value: mock(), script_public_key: mock(), verbose_data: mock() }
        }
    }

    impl Mock for RpcTransactionVerboseData {
        fn mock() -> Self {
            RpcTransactionVerboseData {
                transaction_id: mock(),
                hash: mock(),
                compute_mass: mock(),
                block_hash: mock(),
                block_time: mock(),
            }
        }
    }

    impl Mock for RpcTransaction {
        fn mock() -> Self {
            RpcTransaction {
                version: mock(),
                inputs: mock(),
                outputs: mock(),
                lock_time: mock(),
                subnetwork_id: mock(),
                gas: mock(),
                payload: Hash::mock().as_bytes().to_vec(),
                mass: mock(),
                verbose_data: mock(),
            }
        }
    }

    impl Mock for RpcNodeId {
        fn mock() -> Self {
            RpcNodeId::new(Uuid::new_v4())
        }
    }

    impl Mock for IpAddr {
        fn mock() -> Self {
            IpAddr::V4(Ipv4Addr::new(mock(), mock(), mock(), mock()))
        }
    }

    impl Mock for IpAddress {
        fn mock() -> Self {
            IpAddress::new(mock())
        }
    }

    impl Mock for NetAddress {
        fn mock() -> Self {
            NetAddress::new(IpAddress::new(mock()), mock())
        }
    }

    impl Mock for ContextualNetAddress {
        fn mock() -> Self {
            ContextualNetAddress::new(mock(), mock())
        }
    }

    impl Mock for RpcPeerInfo {
        fn mock() -> Self {
            RpcPeerInfo {
                id: mock(),
                address: mock(),
                last_ping_duration: mock(),
                is_outbound: mock(),
                time_offset: mock(),
                user_agent: "0.4.2".to_string(),
                advertised_protocol_version: mock(),
                time_connected: mock(),
                is_ibd_peer: mock(),
            }
        }
    }

    impl Mock for RpcMempoolEntry {
        fn mock() -> Self {
            RpcMempoolEntry { fee: mock(), transaction: mock(), is_orphan: mock() }
        }
    }

    impl Mock for RpcMempoolEntryByAddress {
        fn mock() -> Self {
            RpcMempoolEntryByAddress { address: mock(), sending: mock(), receiving: mock() }
        }
    }

    impl Mock for ScriptPublicKey {
        fn mock() -> Self {
            let mut bytes: [u8; 36] = [0; 36];
            rand::thread_rng().fill(&mut bytes[..]);
            ScriptPublicKey::from_vec(0, bytes.to_vec())
        }
    }

    impl Mock for RpcUtxoEntry {
        fn mock() -> Self {
            RpcUtxoEntry { amount: mock(), script_public_key: mock(), block_daa_score: mock(), is_coinbase: true }
        }
    }

    impl Mock for RpcTransactionOutpoint {
        fn mock() -> Self {
            RpcTransactionOutpoint { transaction_id: mock(), index: mock() }
        }
    }

    impl Mock for RpcUtxosByAddressesEntry {
        fn mock() -> Self {
            RpcUtxosByAddressesEntry { address: mock(), outpoint: mock(), utxo_entry: mock() }
        }
    }

    impl Mock for ProcessMetrics {
        fn mock() -> Self {
            ProcessMetrics {
                resident_set_size: mock(),
                virtual_memory_size: mock(),
                core_num: mock(),
                cpu_usage: mock(),
                fd_num: mock(),
                disk_io_read_bytes: mock(),
                disk_io_write_bytes: mock(),
                disk_io_read_per_sec: mock(),
                disk_io_write_per_sec: mock(),
            }
        }
    }

    impl Mock for ConnectionMetrics {
        fn mock() -> Self {
            ConnectionMetrics {
                borsh_live_connections: mock(),
                borsh_connection_attempts: mock(),
                borsh_handshake_failures: mock(),
                json_live_connections: mock(),
                json_connection_attempts: mock(),
                json_handshake_failures: mock(),
                active_peers: mock(),
            }
        }
    }

    impl Mock for BandwidthMetrics {
        fn mock() -> Self {
            BandwidthMetrics {
                borsh_bytes_tx: mock(),
                borsh_bytes_rx: mock(),
                json_bytes_tx: mock(),
                json_bytes_rx: mock(),
                p2p_bytes_tx: mock(),
                p2p_bytes_rx: mock(),
                grpc_bytes_tx: mock(),
                grpc_bytes_rx: mock(),
            }
        }
    }

    impl Mock for ConsensusMetrics {
        fn mock() -> Self {
            ConsensusMetrics {
                node_blocks_submitted_count: mock(),
                node_headers_processed_count: mock(),
                node_dependencies_processed_count: mock(),
                node_bodies_processed_count: mock(),
                node_transactions_processed_count: mock(),
                node_chain_blocks_processed_count: mock(),
                node_mass_processed_count: mock(),
                node_database_blocks_count: mock(),
                node_database_headers_count: mock(),
                network_mempool_size: mock(),
                network_tip_hashes_count: mock(),
                network_difficulty: mock(),
                network_past_median_time: mock(),
                network_virtual_parent_hashes_count: mock(),
                network_virtual_daa_score: mock(),
            }
        }
    }

    impl Mock for StorageMetrics {
        fn mock() -> Self {
            StorageMetrics { storage_size_bytes: mock() }
        }
    }

    // --------------------------------------------
    // implementations for all the rpc request
    // and response data structures.

    impl Mock for SubmitBlockRequest {
        fn mock() -> Self {
            SubmitBlockRequest { block: mock(), allow_non_daa_blocks: true }
        }
    }

    test!(SubmitBlockRequest);

    impl Mock for SubmitBlockResponse {
        fn mock() -> Self {
            SubmitBlockResponse { report: SubmitBlockReport::Success }
        }
    }

    test!(SubmitBlockResponse);

    impl Mock for GetBlockTemplateRequest {
        fn mock() -> Self {
            GetBlockTemplateRequest { pay_address: mock(), extra_data: vec![4, 2] }
        }
    }

    test!(GetBlockTemplateRequest);

    impl Mock for GetBlockTemplateResponse {
        fn mock() -> Self {
            GetBlockTemplateResponse { block: mock(), is_synced: true }
        }
    }

    test!(GetBlockTemplateResponse);

    impl Mock for GetBlockRequest {
        fn mock() -> Self {
            GetBlockRequest { hash: mock(), include_transactions: true }
        }
    }

    test!(GetBlockRequest);

    impl Mock for GetBlockResponse {
        fn mock() -> Self {
            GetBlockResponse { block: mock() }
        }
    }

    test!(GetBlockResponse);

    impl Mock for GetInfoRequest {
        fn mock() -> Self {
            GetInfoRequest {}
        }
    }

    test!(GetInfoRequest);

    impl Mock for GetInfoResponse {
        fn mock() -> Self {
            GetInfoResponse {
                p2p_id: Hash::mock().to_string(),
                mempool_size: mock(),
                server_version: "0.4.2".to_string(),
                is_utxo_indexed: true,
                is_synced: false,
                has_notify_command: true,
                has_message_id: false,
            }
        }
    }

    test!(GetInfoResponse);

    impl Mock for GetCurrentNetworkRequest {
        fn mock() -> Self {
            GetCurrentNetworkRequest {}
        }
    }

    test!(GetCurrentNetworkRequest);

    impl Mock for GetCurrentNetworkResponse {
        fn mock() -> Self {
            GetCurrentNetworkResponse { network: NetworkType::Mainnet }
        }
    }

    test!(GetCurrentNetworkResponse);

    impl Mock for GetPeerAddressesRequest {
        fn mock() -> Self {
            GetPeerAddressesRequest {}
        }
    }

    test!(GetPeerAddressesRequest);

    impl Mock for GetPeerAddressesResponse {
        fn mock() -> Self {
            GetPeerAddressesResponse { known_addresses: mock(), banned_addresses: mock() }
        }
    }

    test!(GetPeerAddressesResponse);

    impl Mock for GetSinkRequest {
        fn mock() -> Self {
            GetSinkRequest {}
        }
    }

    test!(GetSinkRequest);

    impl Mock for GetSinkResponse {
        fn mock() -> Self {
            GetSinkResponse { sink: mock() }
        }
    }

    test!(GetSinkResponse);

    impl Mock for GetMempoolEntryRequest {
        fn mock() -> Self {
            GetMempoolEntryRequest { transaction_id: mock(), include_orphan_pool: true, filter_transaction_pool: false }
        }
    }

    test!(GetMempoolEntryRequest);

    impl Mock for GetMempoolEntryResponse {
        fn mock() -> Self {
            GetMempoolEntryResponse { mempool_entry: RpcMempoolEntry { fee: mock(), transaction: mock(), is_orphan: false } }
        }
    }

    test!(GetMempoolEntryResponse);

    impl Mock for GetMempoolEntriesRequest {
        fn mock() -> Self {
            GetMempoolEntriesRequest { include_orphan_pool: true, filter_transaction_pool: false }
        }
    }

    test!(GetMempoolEntriesRequest);

    impl Mock for GetMempoolEntriesResponse {
        fn mock() -> Self {
            GetMempoolEntriesResponse { mempool_entries: mock() }
        }
    }

    test!(GetMempoolEntriesResponse);

    impl Mock for GetConnectedPeerInfoRequest {
        fn mock() -> Self {
            GetConnectedPeerInfoRequest {}
        }
    }

    test!(GetConnectedPeerInfoRequest);

    impl Mock for GetConnectedPeerInfoResponse {
        fn mock() -> Self {
            GetConnectedPeerInfoResponse { peer_info: mock() }
        }
    }

    test!(GetConnectedPeerInfoResponse);

    impl Mock for AddPeerRequest {
        fn mock() -> Self {
            AddPeerRequest { peer_address: mock(), is_permanent: mock() }
        }
    }

    test!(AddPeerRequest);

    impl Mock for AddPeerResponse {
        fn mock() -> Self {
            AddPeerResponse {}
        }
    }

    test!(AddPeerResponse);

    impl Mock for SubmitTransactionRequest {
        fn mock() -> Self {
            SubmitTransactionRequest { transaction: mock(), allow_orphan: mock() }
        }
    }

    test!(SubmitTransactionRequest);

    impl Mock for SubmitTransactionResponse {
        fn mock() -> Self {
            SubmitTransactionResponse { transaction_id: mock() }
        }
    }

    test!(SubmitTransactionResponse);

    impl Mock for GetSubnetworkRequest {
        fn mock() -> Self {
            GetSubnetworkRequest { subnetwork_id: mock() }
        }
    }

    test!(GetSubnetworkRequest);

    impl Mock for GetSubnetworkResponse {
        fn mock() -> Self {
            GetSubnetworkResponse { gas_limit: mock() }
        }
    }

    test!(GetSubnetworkResponse);

    impl Mock for GetVirtualChainFromBlockRequest {
        fn mock() -> Self {
            GetVirtualChainFromBlockRequest { start_hash: mock(), include_accepted_transaction_ids: mock() }
        }
    }

    test!(GetVirtualChainFromBlockRequest);

    impl Mock for RpcAcceptedTransactionIds {
        fn mock() -> Self {
            RpcAcceptedTransactionIds { accepting_block_hash: mock(), accepted_transaction_ids: mock() }
        }
    }

    impl Mock for GetVirtualChainFromBlockResponse {
        fn mock() -> Self {
            GetVirtualChainFromBlockResponse {
                removed_chain_block_hashes: mock(),
                added_chain_block_hashes: mock(),
                accepted_transaction_ids: mock(),
            }
        }
    }

    test!(GetVirtualChainFromBlockResponse);

    impl Mock for GetBlocksRequest {
        fn mock() -> Self {
            GetBlocksRequest { low_hash: mock(), include_blocks: mock(), include_transactions: mock() }
        }
    }

    test!(GetBlocksRequest);

    impl Mock for GetBlocksResponse {
        fn mock() -> Self {
            GetBlocksResponse { block_hashes: mock(), blocks: mock() }
        }
    }

    test!(GetBlocksResponse);

    impl Mock for GetBlockCountRequest {
        fn mock() -> Self {
            GetBlockCountRequest {}
        }
    }

    test!(GetBlockCountRequest);

    impl Mock for BlockCount {
        fn mock() -> Self {
            BlockCount { header_count: mock(), block_count: mock() }
        }
    }

    test!(BlockCount);

    impl Mock for GetBlockDagInfoRequest {
        fn mock() -> Self {
            GetBlockDagInfoRequest {}
        }
    }

    test!(GetBlockDagInfoRequest);

    impl Mock for GetBlockDagInfoResponse {
        fn mock() -> Self {
            GetBlockDagInfoResponse {
                network: NetworkType::Mainnet.try_into().unwrap(),
                block_count: mock(),
                header_count: mock(),
                tip_hashes: mock(),
                difficulty: mock(),
                past_median_time: mock(),
                virtual_parent_hashes: mock(),
                pruning_point_hash: mock(),
                virtual_daa_score: mock(),
                sink: mock(),
            }
        }
    }

    test!(GetBlockDagInfoResponse);

    impl Mock for ResolveFinalityConflictRequest {
        fn mock() -> Self {
            ResolveFinalityConflictRequest { finality_block_hash: mock() }
        }
    }

    test!(ResolveFinalityConflictRequest);

    impl Mock for ResolveFinalityConflictResponse {
        fn mock() -> Self {
            ResolveFinalityConflictResponse {}
        }
    }

    test!(ResolveFinalityConflictResponse);

    impl Mock for ShutdownRequest {
        fn mock() -> Self {
            ShutdownRequest {}
        }
    }

    test!(ShutdownRequest);

    impl Mock for ShutdownResponse {
        fn mock() -> Self {
            ShutdownResponse {}
        }
    }

    test!(ShutdownResponse);

    impl Mock for GetHeadersRequest {
        fn mock() -> Self {
            GetHeadersRequest { start_hash: mock(), limit: mock(), is_ascending: mock() }
        }
    }

    test!(GetHeadersRequest);

    impl Mock for GetHeadersResponse {
        fn mock() -> Self {
            GetHeadersResponse { headers: mock() }
        }
    }

    test!(GetHeadersResponse);

    impl Mock for GetBalanceByAddressRequest {
        fn mock() -> Self {
            GetBalanceByAddressRequest { address: mock() }
        }
    }

    test!(GetBalanceByAddressRequest);

    impl Mock for GetBalanceByAddressResponse {
        fn mock() -> Self {
            GetBalanceByAddressResponse { balance: mock() }
        }
    }

    test!(GetBalanceByAddressResponse);

    impl Mock for GetBalancesByAddressesRequest {
        fn mock() -> Self {
            GetBalancesByAddressesRequest { addresses: mock() }
        }
    }

    test!(GetBalancesByAddressesRequest);

    impl Mock for RpcBalancesByAddressesEntry {
        fn mock() -> Self {
            RpcBalancesByAddressesEntry { address: mock(), balance: mock() }
        }
    }

    impl Mock for GetBalancesByAddressesResponse {
        fn mock() -> Self {
            GetBalancesByAddressesResponse { entries: mock() }
        }
    }

    test!(GetBalancesByAddressesResponse);

    impl Mock for GetSinkBlueScoreRequest {
        fn mock() -> Self {
            GetSinkBlueScoreRequest {}
        }
    }

    test!(GetSinkBlueScoreRequest);

    impl Mock for GetSinkBlueScoreResponse {
        fn mock() -> Self {
            GetSinkBlueScoreResponse { blue_score: mock() }
        }
    }

    test!(GetSinkBlueScoreResponse);

    impl Mock for GetUtxosByAddressesRequest {
        fn mock() -> Self {
            GetUtxosByAddressesRequest { addresses: mock() }
        }
    }

    test!(GetUtxosByAddressesRequest);

    impl Mock for GetUtxosByAddressesResponse {
        fn mock() -> Self {
            GetUtxosByAddressesResponse { entries: mock() }
        }
    }

    test!(GetUtxosByAddressesResponse);

    impl Mock for BanRequest {
        fn mock() -> Self {
            BanRequest { ip: mock() }
        }
    }

    test!(BanRequest);

    impl Mock for BanResponse {
        fn mock() -> Self {
            BanResponse {}
        }
    }

    test!(BanResponse);

    impl Mock for UnbanRequest {
        fn mock() -> Self {
            UnbanRequest { ip: mock() }
        }
    }

    test!(UnbanRequest);

    impl Mock for UnbanResponse {
        fn mock() -> Self {
            UnbanResponse {}
        }
    }

    test!(UnbanResponse);

    impl Mock for EstimateNetworkHashesPerSecondRequest {
        fn mock() -> Self {
            EstimateNetworkHashesPerSecondRequest { window_size: mock(), start_hash: mock() }
        }
    }

    test!(EstimateNetworkHashesPerSecondRequest);

    impl Mock for EstimateNetworkHashesPerSecondResponse {
        fn mock() -> Self {
            EstimateNetworkHashesPerSecondResponse { network_hashes_per_second: mock() }
        }
    }

    test!(EstimateNetworkHashesPerSecondResponse);

    impl Mock for GetMempoolEntriesByAddressesRequest {
        fn mock() -> Self {
            GetMempoolEntriesByAddressesRequest { addresses: mock(), include_orphan_pool: true, filter_transaction_pool: false }
        }
    }

    test!(GetMempoolEntriesByAddressesRequest);

    impl Mock for GetMempoolEntriesByAddressesResponse {
        fn mock() -> Self {
            GetMempoolEntriesByAddressesResponse { entries: mock() }
        }
    }

    test!(GetMempoolEntriesByAddressesResponse);

    impl Mock for GetCoinSupplyRequest {
        fn mock() -> Self {
            GetCoinSupplyRequest {}
        }
    }

    test!(GetCoinSupplyRequest);

    impl Mock for GetCoinSupplyResponse {
        fn mock() -> Self {
            GetCoinSupplyResponse { max_sompi: mock(), circulating_sompi: mock() }
        }
    }

    test!(GetCoinSupplyResponse);

    impl Mock for PingRequest {
        fn mock() -> Self {
            PingRequest {}
        }
    }

    test!(PingRequest);

    impl Mock for PingResponse {
        fn mock() -> Self {
            PingResponse {}
        }
    }

    test!(PingResponse);

    impl Mock for GetConnectionsRequest {
        fn mock() -> Self {
            GetConnectionsRequest { include_profile_data: false }
        }
    }

    test!(GetConnectionsRequest);

    impl Mock for GetConnectionsResponse {
        fn mock() -> Self {
            GetConnectionsResponse { clients: mock(), peers: mock(), profile_data: None }
        }
    }

    test!(GetConnectionsResponse);

    impl Mock for GetSystemInfoRequest {
        fn mock() -> Self {
            GetSystemInfoRequest {}
        }
    }

    test!(GetSystemInfoRequest);

    impl Mock for GetSystemInfoResponse {
        fn mock() -> Self {
            GetSystemInfoResponse {
                version: "1.2.3".to_string(),
                system_id: mock(),
                git_hash: mock(),
                cpu_physical_cores: mock(),
                total_memory: mock(),
                fd_limit: mock(),
            }
        }
    }

    test!(GetSystemInfoResponse);

    impl Mock for GetMetricsRequest {
        fn mock() -> Self {
            GetMetricsRequest {
                process_metrics: true,
                connection_metrics: true,
                bandwidth_metrics: true,
                consensus_metrics: true,
                storage_metrics: true,
                custom_metrics: false,
            }
        }
    }

    test!(GetMetricsRequest);

    impl Mock for GetMetricsResponse {
        fn mock() -> Self {
            GetMetricsResponse {
                server_time: mock(),
                process_metrics: mock(),
                connection_metrics: mock(),
                bandwidth_metrics: mock(),
                consensus_metrics: mock(),
                storage_metrics: mock(),
                custom_metrics: None,
            }
        }
    }

    test!(GetMetricsResponse);

    impl Mock for GetServerInfoRequest {
        fn mock() -> Self {
            GetServerInfoRequest {}
        }
    }

    test!(GetServerInfoRequest);

    impl Mock for GetServerInfoResponse {
        fn mock() -> Self {
            GetServerInfoResponse {
                rpc_api_version: mock(),
                rpc_api_revision: mock(),
                server_version: "0.4.2".to_string(),
                network_id: NetworkType::Mainnet.try_into().unwrap(),
                has_utxo_index: true,
                is_synced: false,
                virtual_daa_score: mock(),
            }
        }
    }

    test!(GetServerInfoResponse);

    impl Mock for GetSyncStatusRequest {
        fn mock() -> Self {
            GetSyncStatusRequest {}
        }
    }

    test!(GetSyncStatusRequest);

    impl Mock for GetSyncStatusResponse {
        fn mock() -> Self {
            GetSyncStatusResponse { is_synced: true }
        }
    }

    test!(GetSyncStatusResponse);

    impl Mock for GetDaaScoreTimestampEstimateRequest {
        fn mock() -> Self {
            GetDaaScoreTimestampEstimateRequest { daa_scores: mock() }
        }
    }

    test!(GetDaaScoreTimestampEstimateRequest);

    impl Mock for GetDaaScoreTimestampEstimateResponse {
        fn mock() -> Self {
            GetDaaScoreTimestampEstimateResponse { timestamps: mock() }
        }
    }

    test!(GetDaaScoreTimestampEstimateResponse);

    impl Mock for NotifyBlockAddedRequest {
        fn mock() -> Self {
            NotifyBlockAddedRequest { command: Command::Start }
        }
    }

    test!(NotifyBlockAddedRequest);

    impl Mock for NotifyBlockAddedResponse {
        fn mock() -> Self {
            NotifyBlockAddedResponse {}
        }
    }

    test!(NotifyBlockAddedResponse);

    impl Mock for BlockAddedNotification {
        fn mock() -> Self {
            BlockAddedNotification { block: mock() }
        }
    }

    test!(BlockAddedNotification);

    impl Mock for NotifyVirtualChainChangedRequest {
        fn mock() -> Self {
            NotifyVirtualChainChangedRequest { command: Command::Start, include_accepted_transaction_ids: true }
        }
    }

    test!(NotifyVirtualChainChangedRequest);

    impl Mock for NotifyVirtualChainChangedResponse {
        fn mock() -> Self {
            NotifyVirtualChainChangedResponse {}
        }
    }

    test!(NotifyVirtualChainChangedResponse);

    impl Mock for VirtualChainChangedNotification {
        fn mock() -> Self {
            VirtualChainChangedNotification {
                removed_chain_block_hashes: mock(),
                added_chain_block_hashes: mock(),
                accepted_transaction_ids: mock(),
            }
        }
    }

    test!(VirtualChainChangedNotification);

    impl Mock for NotifyFinalityConflictRequest {
        fn mock() -> Self {
            NotifyFinalityConflictRequest { command: Command::Start }
        }
    }

    test!(NotifyFinalityConflictRequest);

    impl Mock for NotifyFinalityConflictResponse {
        fn mock() -> Self {
            NotifyFinalityConflictResponse {}
        }
    }

    test!(NotifyFinalityConflictResponse);

    impl Mock for FinalityConflictNotification {
        fn mock() -> Self {
            FinalityConflictNotification { violating_block_hash: mock() }
        }
    }

    test!(FinalityConflictNotification);

    impl Mock for NotifyFinalityConflictResolvedRequest {
        fn mock() -> Self {
            NotifyFinalityConflictResolvedRequest { command: Command::Start }
        }
    }

    test!(NotifyFinalityConflictResolvedRequest);

    impl Mock for NotifyFinalityConflictResolvedResponse {
        fn mock() -> Self {
            NotifyFinalityConflictResolvedResponse {}
        }
    }

    test!(NotifyFinalityConflictResolvedResponse);

    impl Mock for FinalityConflictResolvedNotification {
        fn mock() -> Self {
            FinalityConflictResolvedNotification { finality_block_hash: mock() }
        }
    }

    test!(FinalityConflictResolvedNotification);

    impl Mock for NotifyUtxosChangedRequest {
        fn mock() -> Self {
            NotifyUtxosChangedRequest { addresses: mock(), command: Command::Start }
        }
    }

    test!(NotifyUtxosChangedRequest);

    impl Mock for NotifyUtxosChangedResponse {
        fn mock() -> Self {
            NotifyUtxosChangedResponse {}
        }
    }

    test!(NotifyUtxosChangedResponse);

    impl Mock for UtxosChangedNotification {
        fn mock() -> Self {
            UtxosChangedNotification { added: mock(), removed: mock() }
        }
    }

    test!(UtxosChangedNotification);

    impl Mock for NotifySinkBlueScoreChangedRequest {
        fn mock() -> Self {
            NotifySinkBlueScoreChangedRequest { command: Command::Start }
        }
    }

    test!(NotifySinkBlueScoreChangedRequest);

    impl Mock for NotifySinkBlueScoreChangedResponse {
        fn mock() -> Self {
            NotifySinkBlueScoreChangedResponse {}
        }
    }

    test!(NotifySinkBlueScoreChangedResponse);

    impl Mock for SinkBlueScoreChangedNotification {
        fn mock() -> Self {
            SinkBlueScoreChangedNotification { sink_blue_score: mock() }
        }
    }

    test!(SinkBlueScoreChangedNotification);

    impl Mock for NotifyVirtualDaaScoreChangedRequest {
        fn mock() -> Self {
            NotifyVirtualDaaScoreChangedRequest { command: Command::Start }
        }
    }

    test!(NotifyVirtualDaaScoreChangedRequest);

    impl Mock for NotifyVirtualDaaScoreChangedResponse {
        fn mock() -> Self {
            NotifyVirtualDaaScoreChangedResponse {}
        }
    }

    test!(NotifyVirtualDaaScoreChangedResponse);

    impl Mock for VirtualDaaScoreChangedNotification {
        fn mock() -> Self {
            VirtualDaaScoreChangedNotification { virtual_daa_score: mock() }
        }
    }

    test!(VirtualDaaScoreChangedNotification);

    impl Mock for NotifyPruningPointUtxoSetOverrideRequest {
        fn mock() -> Self {
            NotifyPruningPointUtxoSetOverrideRequest { command: Command::Start }
        }
    }

    test!(NotifyPruningPointUtxoSetOverrideRequest);

    impl Mock for NotifyPruningPointUtxoSetOverrideResponse {
        fn mock() -> Self {
            NotifyPruningPointUtxoSetOverrideResponse {}
        }
    }

    test!(NotifyPruningPointUtxoSetOverrideResponse);

    impl Mock for PruningPointUtxoSetOverrideNotification {
        fn mock() -> Self {
            PruningPointUtxoSetOverrideNotification {}
        }
    }

    test!(PruningPointUtxoSetOverrideNotification);

    impl Mock for NotifyNewBlockTemplateRequest {
        fn mock() -> Self {
            NotifyNewBlockTemplateRequest { command: Command::Start }
        }
    }

    test!(NotifyNewBlockTemplateRequest);

    impl Mock for NotifyNewBlockTemplateResponse {
        fn mock() -> Self {
            NotifyNewBlockTemplateResponse {}
        }
    }

    test!(NotifyNewBlockTemplateResponse);

    impl Mock for NewBlockTemplateNotification {
        fn mock() -> Self {
            NewBlockTemplateNotification {}
        }
    }

    test!(NewBlockTemplateNotification);

    impl Mock for SubscribeResponse {
        fn mock() -> Self {
            SubscribeResponse::new(mock())
        }
    }

    test!(SubscribeResponse);

    impl Mock for UnsubscribeResponse {
        fn mock() -> Self {
            UnsubscribeResponse {}
        }
    }

    test!(UnsubscribeResponse);

    struct Misalign;

    impl Mock for Misalign {
        fn mock() -> Self {
            Misalign
        }
    }

    impl Serializer for Misalign {
        fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
            store!(u32, &1, writer)?;
            store!(u32, &2, writer)?;
            store!(u32, &3, writer)?;
            Ok(())
        }
    }

    impl Deserializer for Misalign {
        fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
            let version: u32 = load!(u32, reader)?;
            assert_eq!(version, 1);
            Ok(Self)
        }
    }

    #[test]
    fn test_misalignment() {
        test::<Misalign>("Misalign");
    }
}
