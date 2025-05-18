use crate::kaspawalletd::{Outpoint, ScriptPublicKey, UtxoEntry, UtxosByAddressesEntry};
use crate::protoserialization;
use crate::protoserialization::{PartiallySignedInput, PartiallySignedTransaction, TransactionMessage, TransactionOutput};
use kaspa_bip32::secp256k1::PublicKey;
use kaspa_bip32::{DerivationPath, Error, ExtendedKey, ExtendedPublicKey};
use kaspa_rpc_core::{
    RpcScriptPublicKey, RpcScriptVec, RpcSubnetworkId, RpcTransaction, RpcTransactionInput, RpcTransactionOutpoint,
    RpcTransactionOutput,
};
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_wallet_core::api::{ScriptPublicKeyWrapper, TransactionOutpointWrapper, UtxoEntryWrapper};
use kaspa_wallet_core::derivation::ExtendedPublicKeySecp256k1;
use prost::Message;
use std::num::TryFromIntError;
use std::str::FromStr;
use tonic::Status;

/// Deserializes a vector of transaction byte arrays into RpcTransaction.
///
/// # Arguments
/// * `txs` - Vector of transaction byte arrays to deserialize
/// * `is_domain` - Boolean flag indicating whether the transactions are domain transactions
///
/// # Returns
/// * `Result<Vec<RpcTransaction>, Status>` - Vector of deserialized transactions or error status
pub fn deserialize_txs(txs: Vec<Vec<u8>>, is_domain: bool, ecdsa: bool) -> Result<Vec<RpcTransaction>, Status> {
    txs.into_iter()
        .map(|tx| if is_domain { deserialize_domain_tx(tx.as_slice()) } else { extract_tx(tx.as_slice(), ecdsa) })
        .collect::<Result<Vec<_>, Status>>()
}

/// Deserializes a domain transaction from bytes into an RpcTransaction.
///
/// # Arguments
/// * `tx` - Byte slice containing the domain transaction data
///
/// # Returns
/// * `Result<RpcTransaction, Status>` - Deserialized transaction or error status
fn deserialize_domain_tx(tx: &[u8]) -> Result<RpcTransaction, Status> {
    let tx = TransactionMessage::decode(tx).map_err(|err| Status::invalid_argument(err.to_string()))?;
    RpcTransaction::try_from(tx)
}

/// Extracts and deserializes a partially signed transaction from bytes into an RpcTransaction.
///
/// # Arguments
/// * `tx` - Byte slice containing the partially signed transaction data
///
/// # Returns
/// * `Result<RpcTransaction, Status>` - Deserialized transaction or error status
fn extract_tx(tx: &[u8], ecdsa: bool) -> Result<RpcTransaction, Status> {
    let tx = PartiallySignedTransaction::decode(tx).map_err(|err| Status::invalid_argument(err.to_string()))?;
    let tx_message = extract_tx_deserialized(tx, ecdsa).map_err(|err| Status::invalid_argument(err.to_string()))?;
    RpcTransaction::try_from(tx_message)
}

/// Extracts and processes a partially signed transaction into a regular transaction message.
/// Handles both single-signature and multi-signature inputs, constructing appropriate signature scripts.
fn extract_tx_deserialized(partially_signed_tx: PartiallySignedTransaction, ecdsa: bool) -> Result<TransactionMessage, Status> {
    let Some(mut tx) = partially_signed_tx.tx else { return Err(Status::invalid_argument("missing transaction")) };
    if partially_signed_tx.partially_signed_inputs.len() > tx.inputs.len() {
        return Err(Status::invalid_argument("unbalanced inputs"));
    }
    for (idx, (signed_input, tx_input)) in partially_signed_tx.partially_signed_inputs.iter().zip(&mut tx.inputs).enumerate() {
        let mut script_builder = ScriptBuilder::new();
        match signed_input.pub_key_signature_pairs.len() {
            0 => return Err(Status::invalid_argument("missing signature")),
            1 => {
                let sig_script = script_builder
                    .add_data(signed_input.pub_key_signature_pairs[0].signature.as_slice())
                    .map_err(|err| Status::invalid_argument(err.to_string()))?
                    .drain();
                tx_input.signature_script = sig_script;
            }
            pairs_len /*multisig*/ => {
                for pair in signed_input.pub_key_signature_pairs.iter() {
                    script_builder.add_data(pair.signature.as_slice()).map_err(|err| Status::invalid_argument(err.to_string()))?;
                }
                if pairs_len < signed_input.minimum_signatures as usize {
                    return Err(Status::invalid_argument(format!("missing {} signatures on input: {idx}", signed_input.minimum_signatures as usize - pairs_len)));
                }
                let redeem_script = partially_signed_input_multisig_redeem_script(signed_input, ecdsa, "m")?;
                    script_builder.add_data(redeem_script.as_slice()).map_err(|err| Status::invalid_argument(err.to_string()))?;
                tx_input.signature_script = script_builder.drain();
            }
        }
    }
    Ok(tx)
}

/// Generates a multi-signature redeem script for a partially signed input.
/// Supports both ECDSA and Schnorr signature schemes based on the ecdsa parameter.
fn partially_signed_input_multisig_redeem_script(input: &PartiallySignedInput, ecdsa: bool, path: &str) -> Result<Vec<u8>, Status> {
    let extended_pub_keys: &[ExtendedPublicKey<PublicKey>] = &input
        .pub_key_signature_pairs
        .iter()
        .map(|pair| {
            let extended_key =
                ExtendedKey::from_str(pair.extended_pub_key.as_str()).map_err(|err| Status::invalid_argument(err.to_string()))?;
            let derived_key: ExtendedPublicKeySecp256k1 =
                extended_key.try_into().map_err(|err: Error| Status::invalid_argument(err.to_string()))?;
            derived_key
                .derive_path(&path.parse::<DerivationPath>().map_err(|err| Status::invalid_argument(err.to_string()))?)
                .map_err(|err| Status::invalid_argument(err.to_string()))
        })
        .collect::<Result<Vec<_>, Status>>()?;

    if ecdsa {
        multisig_redeem_script_ecdsa(extended_pub_keys, input.minimum_signatures as usize)
    } else {
        multisig_redeem_script(extended_pub_keys, input.minimum_signatures as usize)
    }
}

/// Creates a Schnorr-based multisig redeem script from a list of public keys.
/// The script requires at least `minimum_signatures` valid signatures to spend.
fn multisig_redeem_script(extended_pub_keys: &[ExtendedPublicKey<PublicKey>], minimum_signatures: usize) -> Result<Vec<u8>, Status> {
    let serialized_keys = extended_pub_keys.iter().map(|key| key.public_key.x_only_public_key().0.serialize());
    let redeem_script = kaspa_txscript::multisig_redeem_script(serialized_keys, minimum_signatures)
        .map_err(|err| Status::invalid_argument(err.to_string()))?;
    Ok(redeem_script)
}

/// Creates an ECDSA-based multisig redeem script from a list of public keys.
/// The script requires at least `minimum_signatures` valid signatures to spend.
fn multisig_redeem_script_ecdsa(
    extended_pub_keys: &[ExtendedPublicKey<PublicKey>],
    minimum_signatures: usize,
) -> Result<Vec<u8>, Status> {
    let serialized_ecdsa_keys = extended_pub_keys.iter().map(|key| key.public_key.serialize());
    let redeem_script = kaspa_txscript::multisig_redeem_script_ecdsa(serialized_ecdsa_keys, minimum_signatures)
        .map_err(|err| Status::invalid_argument(err.to_string()))?;
    Ok(redeem_script)
}

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

impl TryFrom<TransactionMessage> for RpcTransaction {
    type Error = Status;

    fn try_from(
        // protoserialization::TransactionMessage { version, inputs, outputs, lock_time, subnetwork_id, gas, payload }: protoserialization::TransactionMessage,
        value: TransactionMessage,
    ) -> Result<Self, Self::Error> {
        let version: u16 = value.version.try_into().map_err(|e: TryFromIntError| Status::invalid_argument(e.to_string()))?;
        let inputs: Result<Vec<RpcTransactionInput>, Status> = value
            .inputs
            .into_iter()
            .map(|i| RpcTransactionInput::try_from(i).map_err(|e| Status::invalid_argument(e.to_string())))
            .collect();
        let outputs: Result<Vec<RpcTransactionOutput>, Status> = value
            .outputs
            .into_iter()
            .map(|i| RpcTransactionOutput::try_from(i).map_err(|e| Status::invalid_argument(e.to_string())))
            .collect();

        let subnetwork_id =
            RpcSubnetworkId::try_from(value.subnetwork_id.ok_or(Status::invalid_argument("missing subnetwork_id"))?.bytes.as_slice())
                .map_err(|e| Status::invalid_argument(e.to_string()))?;

        Ok(RpcTransaction {
            version,
            inputs: inputs?,
            outputs: outputs?,
            lock_time: value.lock_time,
            subnetwork_id,
            gas: value.gas,
            payload: value.payload,
            mass: 0,
            verbose_data: None,
        })
    }
}

impl TryFrom<protoserialization::TransactionInput> for RpcTransactionInput {
    type Error = Status;
    fn try_from(value: protoserialization::TransactionInput) -> Result<Self, Self::Error> {
        let previous_outpoint = value.previous_outpoint.ok_or(Status::invalid_argument("missing previous outpoint"))?.try_into()?;
        let sig_op_count: u8 = value.sig_op_count.try_into().map_err(|e: TryFromIntError| Status::invalid_argument(e.to_string()))?;
        Ok(RpcTransactionInput {
            previous_outpoint,
            signature_script: value.signature_script,
            sequence: value.sequence,
            sig_op_count,
            verbose_data: None,
        })
    }
}

impl TryFrom<TransactionOutput> for RpcTransactionOutput {
    type Error = Status;

    fn try_from(value: TransactionOutput) -> Result<Self, Self::Error> {
        Ok(RpcTransactionOutput {
            value: value.value,
            script_public_key: value.script_public_key.ok_or(Status::invalid_argument("missing script public key"))?.try_into()?,
            verbose_data: None,
        })
    }
}

impl TryFrom<protoserialization::ScriptPublicKey> for RpcScriptPublicKey {
    type Error = Status;

    fn try_from(value: protoserialization::ScriptPublicKey) -> Result<Self, Self::Error> {
        let version: u16 = value.version.try_into().map_err(|e: TryFromIntError| Status::invalid_argument(e.to_string()))?;
        Ok(RpcScriptPublicKey::new(version, RpcScriptVec::from(value.script)))
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
