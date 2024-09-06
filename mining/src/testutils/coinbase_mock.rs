use kaspa_consensus_core::{
    coinbase::{CoinbaseData, CoinbaseTransactionTemplate, MinerData},
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    subnets::SUBNETWORK_ID_COINBASE,
    tx::{Transaction, TransactionOutput},
};

const LENGTH_OF_BLUE_SCORE: usize = size_of::<u64>();
const LENGTH_OF_SUBSIDY: usize = size_of::<u64>();

pub(super) struct CoinbaseManagerMock {}

impl CoinbaseManagerMock {
    pub(super) fn new() -> Self {
        Self {}
    }

    pub(super) fn expected_coinbase_transaction(&self, miner_data: MinerData) -> CoinbaseTransactionTemplate {
        const SUBSIDY: u64 = 500 * SOMPI_PER_KASPA;
        let output = TransactionOutput::new(SUBSIDY, miner_data.script_public_key.clone());

        let payload = self.serialize_coinbase_payload(&CoinbaseData { blue_score: 1, subsidy: SUBSIDY, miner_data });

        CoinbaseTransactionTemplate {
            tx: Transaction::new(TX_VERSION, vec![], vec![output], 0, SUBNETWORK_ID_COINBASE, 0, payload),
            has_red_reward: false,
        }
    }

    pub(super) fn serialize_coinbase_payload(&self, data: &CoinbaseData) -> Vec<u8> {
        let script_pub_key_len = data.miner_data.script_public_key.script().len();
        let payload: Vec<u8> = data.blue_score.to_le_bytes().iter().copied()                    // Blue score                   (u64)
            .chain(data.subsidy.to_le_bytes().iter().copied())                                  // Subsidy                      (u64)
            .chain(data.miner_data.script_public_key.version().to_le_bytes().iter().copied())   // Script public key version    (u16)
            .chain((script_pub_key_len as u8).to_le_bytes().iter().copied())                    // Script public key length     (u8)
            .chain(data.miner_data.script_public_key.script().iter().copied())                  // Script public key            
            .chain(data.miner_data.extra_data.iter().copied())                                  // Extra data
            .collect();

        payload
    }

    pub fn modify_coinbase_payload(&self, mut payload: Vec<u8>, miner_data: &MinerData) -> Vec<u8> {
        let script_pub_key_len = miner_data.script_public_key.script().len();
        payload.truncate(LENGTH_OF_BLUE_SCORE + LENGTH_OF_SUBSIDY);
        payload.extend(
            miner_data.script_public_key.version().to_le_bytes().iter().copied() // Script public key version (u16)
                .chain((script_pub_key_len as u8).to_le_bytes().iter().copied()) // Script public key length  (u8)
                .chain(miner_data.script_public_key.script().iter().copied())    // Script public key
                .chain(miner_data.extra_data.iter().copied()), // Extra data
        );

        payload
    }
}
