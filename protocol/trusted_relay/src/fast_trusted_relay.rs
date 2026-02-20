use crate::servers::udp_transport::runtime::TransportRuntime;


pub struct FastTrustedRelay {
    udp_runtime: Option<TransportRuntime>,
    tcp_runtime: ControlRuntime,
}

impl FastTrustedRelay {
// TODO
}

/*
#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::tx::Transaction;

    use super::*;
    use crate::params::{DecodingParams, TransportParams};
    use crate::sharding::config::ShardingConfig;

    fn generate_test_block() -> Block {
        use kaspa_hashes::Hash;
        // Construct a minimal but valid header and transactions rather than
        // deserializing arbitrary bytes.
        let header = Arc::new(Header::from_precomputed_hash(Hash::from_bytes([0u8; 32]), Vec::new()));
        let tx =
            Transaction::new(0, Vec::new(), Vec::new(), 0, kaspa_consensus_core::subnets::SubnetworkId::from_byte(0), 0, Vec::new());
        let transactions = Arc::new(vec![tx; 10]);
        Block { header, transactions }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_adaptor_broadcast_block_noop_when_inactive() {
        let params = TrustedRelayParams {
            sharding: ShardingConfig::new(4, 2, 1024),
            transport: TransportParams { hmac_workers: 1, ..TransportParams::default() },
            decoding: DecodingParams { block_reassembly_workers: 1, num_workers: 1, ..DecodingParams::default() },
        };
        let listen_addr = "127.0.0.1:0".parse().unwrap();
        let secret = b"test-secret".to_vec();

        let adaptor = FastTrustedRelay::start(params, listen_addr, secret).await.unwrap();
        // Relay inactive by default; broadcast_block should be a no-op and return Ok.
        let hash = kaspa_hashes::Hash::from_bytes([1u8; 32]);
        // send pre-serialized bytes via convenience helper
        let res = adaptor.broadcast_block(hash, generate_test_block()).await;
        assert!(res.is_ok());

        // Shutdown the adaptor to clean up tasks.
        adaptor.shutdown().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_adaptor_broadcast_block_when_active() {
        let params = TrustedRelayParams {
            sharding: ShardingConfig::new(4, 2, 1024),
            transport: TransportParams { hmac_workers: 1, ..TransportParams::default() },
            decoding: DecodingParams { block_reassembly_workers: 1, num_workers: 1, ..DecodingParams::default() },
        };
        let listen_addr = "127.0.0.1:0".parse().unwrap();
        let secret = b"test-secret".to_vec();

        let adaptor = FastTrustedRelay::start(params, listen_addr, secret).await.unwrap();
        // Activate relay and broadcast a block — should enqueue successfully.
        adaptor.start_fast_relay().await.unwrap();
        let hash = kaspa_hashes::Hash::from_bytes([2u8; 32]);
        // send pre-serialized bytes via convenience helper
        let res = adaptor.broadcast_block(hash, generate_test_block()).await;
        assert!(res.is_ok());

        adaptor.shutdown().await;
    }
}
*/
