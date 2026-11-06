use super::error::ConversionError;
use crate::pb as protowire;
use kaspa_consensus_core::{block::Block, tx::Transaction};
type BlockBody = Vec<Transaction>;
use prost::Message;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Block> for protowire::BlockMessage {
    fn from(block: &Block) -> Self {
        Self { header: Some(block.header.as_ref().into()), transactions: block.transactions.iter().map(|tx| tx.into()).collect() }
    }
}
impl From<&BlockBody> for protowire::BlockBodyMessage {
    fn from(block_body: &BlockBody) -> Self {
        Self { transactions: block_body.iter().map(|tx| tx.into()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

const MAX_BLOCK_BODY_SIZE: usize = 2 * 1024 * 1024; // 2MB

impl TryFrom<protowire::BlockMessage> for Block {
    type Error = ConversionError;

    fn try_from(block: protowire::BlockMessage) -> Result<Self, Self::Error> {
        if block.encoded_len().saturating_sub(block.header.as_ref().map(|h| h.encoded_len()).unwrap_or(0)) > MAX_BLOCK_BODY_SIZE {
            return Err(ConversionError::Size);
        }

        let header = block.header.ok_or(ConversionError::NoneValue)?;
        Ok(Self::new(
            header.try_into()?,
            block.transactions.into_iter().map(|i| i.try_into()).collect::<Result<Vec<Transaction>, Self::Error>>()?,
        ))
    }
}

impl TryFrom<protowire::BlockBodyMessage> for BlockBody {
    type Error = ConversionError;
    fn try_from(body_message: protowire::BlockBodyMessage) -> Result<Self, Self::Error> {
        if body_message.encoded_len() > MAX_BLOCK_BODY_SIZE {
            return Err(ConversionError::Size);
        }

        let blk_body: BlockBody =
            body_message.transactions.into_iter().map(|i| i.try_into()).collect::<Result<Vec<Transaction>, ConversionError>>()?;
        Ok(blk_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::header::Header;

    #[test]
    fn test_block_message_oversized_rejected() {
        let tx = protowire::TransactionMessage { payload: vec![0u8; 800 * 1024], ..Default::default() };
        let block = protowire::BlockMessage { header: None, transactions: vec![tx.clone(), tx.clone(), tx] };
        assert!(block.encoded_len() > MAX_BLOCK_BODY_SIZE);
        assert!(matches!(Block::try_from(block), Err(ConversionError::Size)));
    }

    #[test]
    fn test_block_body_message_oversized_rejected() {
        let tx = protowire::TransactionMessage { payload: vec![0u8; 800 * 1024], ..Default::default() };
        let body = protowire::BlockBodyMessage { transactions: vec![tx.clone(), tx.clone(), tx] };
        assert!(body.encoded_len() > MAX_BLOCK_BODY_SIZE);
        assert!(matches!(BlockBody::try_from(body), Err(ConversionError::Size)));
    }

    #[test]
    fn test_block_message_roundtrip() {
        let header = Header::new_finalized(
            2,
            vec![vec![1.into()]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            1,
            2,
            3,
            4,
            5.into(),
            6,
            Default::default(),
        );
        let block = Block::new(header, vec![]);
        let wire: protowire::BlockMessage = (&block).into();
        assert!(wire.encoded_len() <= MAX_BLOCK_BODY_SIZE);
        let decoded = Block::try_from(wire).unwrap();
        assert_eq!(decoded.header.hash, block.header.hash);
        assert_eq!(decoded.transactions.len(), 0);
    }

    #[test]
    fn test_block_body_message_roundtrip() {
        let body: BlockBody = vec![];
        let wire: protowire::BlockBodyMessage = (&body).into();
        assert!(wire.encoded_len() <= MAX_BLOCK_BODY_SIZE);
        let decoded = BlockBody::try_from(wire).unwrap();
        assert_eq!(decoded.len(), 0);
    }

    #[test]
    fn test_block_message_missing_header_returns_none_value() {
        let block = protowire::BlockMessage { header: None, transactions: vec![] };
        assert!(block.encoded_len() <= MAX_BLOCK_BODY_SIZE);
        assert!(matches!(Block::try_from(block), Err(ConversionError::NoneValue)));
    }

    #[test]
    fn test_p2p_max_block_body_size_larger_than_consensus() {
        use kaspa_consensus_core::{
            config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS},
            constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
            mass::transaction_estimated_serialized_size,
            subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_SIZE},
            tx::{CovenantBinding, ScriptPublicKey, TransactionOutput},
        };
        use kaspa_hashes::{HASH_SIZE, Hash};

        for (name, params) in
            [("mainnet", &MAINNET_PARAMS), ("testnet", &TESTNET_PARAMS), ("devnet", &DEVNET_PARAMS), ("simnet", &SIMNET_PARAMS)]
        {
            // 1. Non-coinbase transactions are capped by the block's transient mass limit:
            //    transient_mass = size * TRANSIENT_BYTE_TO_MASS_FACTOR (4)
            let max_non_coinbase_bytes = (params.block_mass_limits.transient / TRANSIENT_BYTE_TO_MASS_FACTOR) as usize;

            // 2. Compute the maximum pre-virtual valid coinbase transaction size from Params and constants.
            //    Before virtual validation (which only determines qualification for the virtual chain),
            //    a version 1 coinbase transaction may have:
            //    - No inputs (coinbase rule: 0 inputs)
            //    - Up to K + 2 outputs (checked in check_coinbase_in_isolation)
            //    - Up to coinbase_payload_script_public_key_max_len bytes script per output (150 bytes)
            //    - A covenant binding on every output in version 1 (authorizing_input: u16 + covenant_id: Hash)
            //    - An arbitrary u64 lock time (finality succeeds with 0 inputs)
            //    - Up to max_coinbase_payload_len bytes payload (204 bytes)
            //    - Zero gas, zero storage mass, and SUBNETWORK_ID_COINBASE.
            let max_coinbase_outputs = params.ghostdag_k() as usize + 2;

            // Compute the consensus estimated serialized size from its constituent summands:
            let output_size = 8 // value (u64)
                + 2 // ScriptPublicKey.version (u16)
                + 8 // ScriptPublicKey length (u64)
                + params.coinbase_payload_script_public_key_max_len as usize // 150 bytes
                + 2 // covenant authorizing_input (u16)
                + HASH_SIZE; // covenant_id (32 bytes)

            let coinbase_tx_fixed_size = 2 // version (u16)
                + 8 // inputs count (u64)
                + 8 // outputs count (u64)
                + 8 // lock_time (u64)
                + SUBNETWORK_ID_SIZE // 20 bytes
                + 8 // gas (u64)
                + HASH_SIZE // payload hash (32 bytes)
                + 8 // payload length (u64)
                + params.max_coinbase_payload_len; // 204 bytes

            // Construct the maximal valid coinbase transaction to verify the analytical calculation
            // and determine its Protobuf wire encoding size.
            let max_coinbase_tx = Transaction::new(
                1,
                vec![],
                (0..max_coinbase_outputs)
                    .map(|_| {
                        TransactionOutput::with_covenant(
                            u64::MAX,
                            ScriptPublicKey::from_vec(0, vec![0u8; params.coinbase_payload_script_public_key_max_len as usize]),
                            Some(CovenantBinding { authorizing_input: u16::MAX, covenant_id: Hash::from_bytes([0xff; 32]) }),
                        )
                    })
                    .collect(),
                u64::MAX,
                SUBNETWORK_ID_COINBASE,
                0,
                vec![0u8; params.max_coinbase_payload_len],
            );

            assert_eq!(
                usize::try_from(transaction_estimated_serialized_size(&max_coinbase_tx)).unwrap(),
                coinbase_tx_fixed_size + max_coinbase_outputs * output_size,
                "[{name}] analytical coinbase size must match the consensus estimator"
            );

            let max_coinbase_proto_bytes = protowire::TransactionMessage::from(&max_coinbase_tx).encoded_len();

            // Total maximum consensus block body size (combining non-coinbase transient limit + coinbase tx)
            let consensus_max_block_body_size = max_non_coinbase_bytes + max_coinbase_proto_bytes;

            // Ensure P2P MAX_BLOCK_BODY_SIZE has at least a 2x slack over the consensus maximum
            assert!(
                MAX_BLOCK_BODY_SIZE >= 2 * consensus_max_block_body_size,
                "[{name}] P2P MAX_BLOCK_BODY_SIZE ({MAX_BLOCK_BODY_SIZE}) must be at least 2x consensus limit ({consensus_max_block_body_size})"
            );
        }
    }
}
