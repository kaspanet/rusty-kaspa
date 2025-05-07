use crate::kaspawalletd::{Outpoint, ScriptPublicKey, UtxoEntry, UtxosByAddressesEntry};
use crate::protoserialization;
use kaspa_rpc_core::{RpcTransaction, RpcTransactionInput, RpcTransactionOutpoint};
use kaspa_wallet_core::api::{ScriptPublicKeyWrapper, TransactionOutpointWrapper, UtxoEntryWrapper};
// use std::num::TryFromIntError;
use tonic::Status;

impl From<TransactionOutpointWrapper> for Outpoint {
    fn from(wrapper: kaspa_wallet_core::api::TransactionOutpointWrapper) -> Self {
        Outpoint { transaction_id: wrapper.transaction_id.to_string(), index: wrapper.index }
    }
}

impl From<ScriptPublicKeyWrapper> for ScriptPublicKey {
    fn from(script_pub_key: ScriptPublicKeyWrapper) -> Self {
        ScriptPublicKey { script_public_key: script_pub_key.script_public_key, version: script_pub_key.version.into() }
    }
}

impl From<UtxoEntryWrapper> for UtxosByAddressesEntry {
    fn from(wrapper: UtxoEntryWrapper) -> Self {
        UtxosByAddressesEntry {
            address: wrapper.address.map(|addr| addr.to_string()).unwrap_or_default(),
            outpoint: Some(wrapper.outpoint.into()),
            utxo_entry: Some(UtxoEntry {
                amount: wrapper.amount,
                script_public_key: Some(wrapper.script_public_key.into()),
                block_daa_score: wrapper.block_daa_score,
                is_coinbase: wrapper.is_coinbase,
            }),
        }
    }
}

impl TryFrom<protoserialization::TransactionMessage> for RpcTransaction {
    type Error = Status;

    fn try_from(
        // protoserialization::TransactionMessage { version, inputs, outputs, lock_time, subnetwork_id, gas, payload }: protoserialization::TransactionMessage,
        _: protoserialization::TransactionMessage,
    ) -> Result<Self, Self::Error> {
        todo!()
        // Ok(RpcTransaction {
        //     version: version.try_into().map_err(|err: TryFromIntError| Status::invalid_argument(err.to_string()))?,
        //     inputs: vec![],
        //     outputs: vec![],
        //     lock_time: 0,
        //     subnetwork_id: Default::default(),
        //     gas: 0,
        //     payload: vec![],
        //     mass: 0,
        //     verbose_data: None,
        // })
    }
}

impl TryFrom<protoserialization::TransactionInput> for RpcTransactionInput {
    type Error = Status;

    fn try_from(_value: protoserialization::TransactionInput) -> Result<Self, Self::Error> {
        todo!()
        // RpcTransactionInput{
        //     previous_outpoint: RpcTransactionOutpoint {},
        //     signature_script: vec![],
        //     sequence: 0,
        //     sig_op_count: 0,
        //     verbose_data: None,
        // }
    }
}

impl TryFrom<protoserialization::Outpoint> for RpcTransactionOutpoint {
    type Error = Status;

    fn try_from(
        _: protoserialization::Outpoint, /*protoserialization::Outpoint{ transaction_id, index }: protoserialization::Outpoint*/
    ) -> Result<Self, Self::Error> {
        todo!()
        // Ok(RpcTransactionOutpoint { transaction_id: Default::default(), index: 0 })
    }
}
