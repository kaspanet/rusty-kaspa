use crate::RpcHash;
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

/// Identifies an accepting block by its hash or its exact selected-chain DAA score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RpcAcceptingBlock {
    Hash(#[serde(with = "kaspa_utils::serde_bytes_fixed_ref")] RpcHash),
    DaaScore(u64),
}

impl Serializer for RpcAcceptingBlock {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        match self {
            Self::Hash(hash) => {
                store!(u8, &0, writer)?;
                store!(RpcHash, hash, writer)?;
            }
            Self::DaaScore(score) => {
                store!(u8, &1, writer)?;
                store!(u64, score, writer)?;
            }
        }
        Ok(())
    }
}

impl Deserializer for RpcAcceptingBlock {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        match load!(u8, reader)? {
            0 => Ok(Self::Hash(load!(RpcHash, reader)?)),
            1 => Ok(Self::DaaScore(load!(u64, reader)?)),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid accepting block selector")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GetTransactionsByAcceptingBlockRequest, GetTransactionsByAcceptingBlockResponse, RpcOptionalTransaction};
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{
            CovenantBinding, ScriptPublicKey, SignableTransaction, Transaction, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry,
        },
    };
    use serde_json::json;

    #[test]
    fn accepting_block_request_roundtrip() {
        let hash = RpcHash::from_bytes([0xab; 32]);
        for (selector, json_selector) in [
            (RpcAcceptingBlock::Hash(hash), json!({ "hash": hash.to_string() })),
            (RpcAcceptingBlock::DaaScore(0), json!({ "daaScore": 0 })),
            (RpcAcceptingBlock::DaaScore(u64::MAX), json!({ "daaScore": u64::MAX })),
        ] {
            let request = GetTransactionsByAcceptingBlockRequest::new(selector, vec![hash]);
            let value = serde_json::to_value(&request).unwrap();
            assert_eq!(value, json!({ "acceptingBlock": json_selector, "transactionIds": [hash.to_string()] }));
            assert_eq!(serde_json::from_value::<GetTransactionsByAcceptingBlockRequest>(value).unwrap(), request);
            let mut bytes = Vec::new();
            serialize!(GetTransactionsByAcceptingBlockRequest, &request, &mut bytes).unwrap();
            let decoded = deserialize!(GetTransactionsByAcceptingBlockRequest, &mut bytes.as_slice()).unwrap();
            assert_eq!(decoded, request);
        }
        for value in [json!({}), json!({ "hash": hash.to_string(), "daaScore": 1 }), json!({ "hash": "invalid" })] {
            assert!(serde_json::from_value::<RpcAcceptingBlock>(value).is_err());
        }
        let error = deserialize!(RpcAcceptingBlock, &mut [1u8, 2].as_slice()).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn populated_rpc_response_roundtrip() {
        let covenant_id = RpcHash::from_bytes([0xcd; 32]);
        let script = ScriptPublicKey::from_vec(0, vec![0x51]);
        let entry = UtxoEntry::new(12345, script.clone(), 100, false, Some(covenant_id));
        let tx = Transaction::new(
            1,
            vec![TransactionInput::new_with_compute_budget(TransactionOutpoint::new(covenant_id, 2), vec![1, 2, 3], 9, 456)],
            vec![TransactionOutput::with_covenant(12300, script, Some(CovenantBinding { authorizing_input: 0, covenant_id }))],
            7,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![4, 5, 6],
        );
        tx.set_storage_mass(789);
        let populated = SignableTransaction::with_entries(tx, vec![entry.clone()]);
        let response = GetTransactionsByAcceptingBlockResponse { transactions: vec![RpcOptionalTransaction::from(&populated)] };
        let mut bytes = Vec::new();
        serialize!(GetTransactionsByAcceptingBlockResponse, &response, &mut bytes).unwrap();
        let binary = deserialize!(GetTransactionsByAcceptingBlockResponse, &mut bytes.as_slice()).unwrap();
        let json = serde_json::to_value(&response).unwrap();
        let from_json: GetTransactionsByAcceptingBlockResponse = serde_json::from_value(json.clone()).unwrap();
        for decoded in [binary, from_json] {
            assert_eq!(serde_json::to_value(&decoded).unwrap(), json);
            let transaction = decoded.transactions.into_iter().next().unwrap();
            let verbose = transaction.verbose_data.as_ref().unwrap();
            assert_eq!(verbose.transaction_id, Some(populated.id()));
            assert!(verbose.block_hash.is_none());
            assert!(verbose.block_time.is_none());
            let input_entry: UtxoEntry =
                transaction.inputs[0].verbose_data.as_ref().unwrap().utxo_entry.clone().unwrap().try_into().unwrap();
            assert_eq!(input_entry, entry);
            let restored: Transaction = transaction.try_into().unwrap();
            assert_eq!(restored, populated.tx);
        }
    }
}
