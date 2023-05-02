use crate::consensus::test_consensus::TestConsensus;
use itertools::Itertools;
use kaspa_consensus_core::{
    api::ConsensusApi,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    config::{params::MAINNET_PARAMS, ConfigBuilder},
    tx::{ScriptPublicKey, ScriptVec},
    BlockHashSet,
};

#[tokio::test]
async fn template_mining_sanity_test() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let rounds = 10;
    let width = 3;
    let mut tips = BlockHashSet::from_iter([config.genesis.hash]);

    let miner_data = new_miner_data();
    for _ in 0..rounds {
        let templates = (0..width)
            .map(|j| {
                let mut t = consensus.build_block_template(miner_data.clone(), Default::default()).unwrap();
                t.block.header.nonce = j as u64;
                t.block.header.finalize();
                t
            })
            .collect_vec();
        let prev_tips = std::mem::take(&mut tips);
        for t in templates {
            assert_eq!(prev_tips, BlockHashSet::from_iter(t.block.header.direct_parents().iter().copied()));
            tips.insert(t.block.header.hash);
            let status = consensus.validate_and_insert_block(t.block.to_immutable()).await.unwrap();
            assert!(status.is_utxo_valid_or_pending());
        }
        assert_eq!(width, tips.len());
    }

    // Assert that at least one body tip was resolved with valid UTXO
    assert!(consensus.body_tips().iter().copied().any(|h| consensus.block_status(h) == BlockStatus::StatusUTXOValid));

    consensus.shutdown(wait_handles);
}

fn new_miner_data() -> MinerData {
    let secp = secp256k1::Secp256k1::new();
    let mut rng = rand::thread_rng();
    let (_sk, pk) = secp.generate_keypair(&mut rng);
    let script = ScriptVec::from_slice(&pk.serialize());
    MinerData::new(ScriptPublicKey::new(0, script), vec![])
}
