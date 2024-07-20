use crate::error::Error;
use crate::prelude::*;
use crate::pskt::{Inner as PSKTInner, PSKT};
use crate::wasm::result;

use kaspa_addresses::Address;
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};

use hex;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script, pay_to_script_hash_script};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub inner_list: Vec<PSKTInner>,
}

impl<ROLE> From<PSKT<ROLE>> for Bundle {
    fn from(pskt: PSKT<ROLE>) -> Self {
        Bundle { inner_list: vec![pskt.deref().clone()] }
    }
}

impl<ROLE> From<Vec<PSKT<ROLE>>> for Bundle {
    fn from(pskts: Vec<PSKT<ROLE>>) -> Self {
        let inner_list = pskts.into_iter().map(|pskt| pskt.deref().clone()).collect();
        Bundle { inner_list }
    }
}

impl Bundle {
    pub fn new() -> Self {
        Self { inner_list: Vec::new() }
    }

    /// Adds an Inner instance to the bundle
    pub fn add_inner(&mut self, inner: PSKTInner) {
        self.inner_list.push(inner);
    }

    /// Adds a PSKT instance to the bundle
    pub fn add_pskt<ROLE>(&mut self, pskt: PSKT<ROLE>) {
        self.inner_list.push(pskt.deref().clone());
    }

    /// Merges another bundle into the current bundle
    pub fn merge(&mut self, other: Bundle) {
        for inner in other.inner_list {
            self.inner_list.push(inner);
        }
    }

    pub fn to_hex(&self) -> Result<String, Error> {
        match TypeMarked::new(self, Marker::Pskb) {
            Ok(type_marked) => match serde_json::to_string(&type_marked) {
                Ok(result) => Ok(hex::encode(result)),
                Err(e) => Err(Error::PskbSerializeToHexError(e.to_string())),
            },
            Err(e) => Err(Error::PskbSerializeToHexError(e.to_string())),
        }
    }

    pub fn from_hex(hex_data: &str) -> Result<Self, Error> {
        let bundle: TypeMarked<Bundle> = serde_json::from_slice(hex::decode(hex_data)?.as_slice())?;
        Ok(bundle.data)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
enum Marker {
    Pskb,
}

impl Marker {
    fn as_str(&self) -> &str {
        match self {
            Marker::Pskb => "pskb",
        }
    }

    fn from_str(marker: &str) -> Result<Self, Error> {
        match marker {
            "pskb" => Ok(Marker::Pskb),
            _ => Err("Invalid pskb type marker".into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct TypeMarked<T> {
    type_marker: String,
    #[serde(flatten)]
    data: T,
}

impl<T> TypeMarked<T> {
    fn new(data: T, marker: Marker) -> Result<Self, Error> {
        let type_marker = marker.as_str().to_string();
        if Marker::from_str(&type_marker)? == marker {
            Ok(Self { type_marker, data })
        } else {
            Err("Invalid pskb type marker".into())
        }
    }
}

impl TryFrom<String> for Bundle {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Bundle::from_hex(&value)
    }
}

impl TryFrom<&str> for Bundle {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Error> {
        Bundle::from_hex(value)
    }
}
impl TryFrom<Bundle> for String {
    type Error = Error;
    fn try_from(value: Bundle) -> Result<String, Error> {
        match Bundle::to_hex(&value) {
            Ok(output) => Ok(output.to_owned()),
            Err(e) => Err(Error::PskbSerializeError(e.to_string())),
        }
    }
}

impl Default for Bundle {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lock_script_sig_templating(payload: String, pubkey_bytes: Option<&[u8]>) -> Result<Vec<u8>, Error> {
    let mut payload_bytes: Vec<u8> = hex::decode(payload)?;

    if let Some(pubkey) = pubkey_bytes {
        let placeholder = b"{{pubkey}}";

        // Search for the placeholder in payload bytes to be replaced by public key.
        if let Some(pos) = payload_bytes.windows(placeholder.len()).position(|window| window == placeholder) {
            payload_bytes.splice(pos..pos + placeholder.len(), pubkey.iter().cloned());
        }
    }
    Ok(payload_bytes)
}

pub fn script_sig_to_address(script_sig: &[u8], prefix: kaspa_addresses::Prefix) -> Result<Address, Error> {
    extract_script_pub_key_address(&pay_to_script_hash_script(script_sig), prefix).map_err(Error::P2SHExtractError)
}

pub fn unlock_utxos_as_pskb(
    utxo_references: Vec<(UtxoEntry, TransactionOutpoint)>,
    recipient: &Address,
    script_sig: Vec<u8>,
    priority_fee_sompi_per_transaction: u64,
) -> Result<Bundle, Error> {
    // Fee per transaction.
    // Check if each UTXO's amounts can cover priority fee.
    utxo_references
        .iter()
        .map(|(entry, _)| {
            if entry.amount <= priority_fee_sompi_per_transaction {
                return Err(Error::ExcessUnlockFeeError);
            }
            Ok(())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let recipient_spk = pay_to_address_script(recipient);
    let (successes, errors): (Vec<_>, Vec<_>) = utxo_references
        .into_iter()
        .map(|(utxo_entry, outpoint)| {
            unlock_utxo(&utxo_entry, &outpoint, &recipient_spk, &script_sig, priority_fee_sompi_per_transaction)
        })
        .partition(Result::is_ok);

    let successful_bundles: Vec<_> = successes.into_iter().filter_map(Result::ok).collect();
    let error_list: Vec<_> = errors.into_iter().filter_map(Result::err).collect();

    if !error_list.is_empty() {
        return Err(Error::MultipleUnlockUtxoError(error_list));
    }

    let merged_bundle = successful_bundles.into_iter().fold(None, |acc: Option<Bundle>, bundle| match acc {
        Some(mut merged_bundle) => {
            merged_bundle.merge(bundle);
            Some(merged_bundle)
        }
        None => Some(bundle),
    });

    match merged_bundle {
        None => Err("Generating an empty PSKB".into()),
        Some(bundle) => Ok(bundle),
    }
}

pub fn unlock_utxo(
    utxo_entry: &UtxoEntry,
    outpoint: &TransactionOutpoint,
    script_public_key: &ScriptPublicKey,
    script_sig: &[u8],
    priority_fee_sompi: u64,
) -> Result<Bundle, Error> {
    let input = InputBuilder::default()
        .utxo_entry(utxo_entry.to_owned())
        .previous_outpoint(outpoint.to_owned())
        .sig_op_count(1)
        .redeem_script(script_sig.to_vec())
        .build()?;

    let output = OutputBuilder::default()
        .amount(utxo_entry.amount - priority_fee_sompi)
        .script_public_key(script_public_key.clone())
        .build()?;

    let pskt: PSKT<Constructor> = PSKT::<Creator>::default().constructor().input(input).output(output);
    Ok(pskt.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::role::Creator;
    use crate::role::*;
    // hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValues},
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};
    use kaspa_txscript::{multisig_redeem_script, pay_to_script_hash_script};
    // use kaspa_txscript::{multisig_redeem_script, opcodes::codes::OpData65, pay_to_script_hash_script, script_builder::ScriptBuilder};
    use rmp_serde::{decode, encode};
    use secp256k1::Secp256k1;
    use secp256k1::{rand::thread_rng, Keypair};
    use std::str::FromStr;
    use std::sync::Once;

    static INIT: Once = Once::new();
    static mut CONTEXT: Option<Box<([Keypair; 2], Vec<u8>)>> = None;

    fn mock_context() -> &'static ([Keypair; 2], Vec<u8>) {
        unsafe {
            INIT.call_once(|| {
                let kps = [Keypair::new(&Secp256k1::new(), &mut thread_rng()), Keypair::new(&Secp256k1::new(), &mut thread_rng())];
                let redeem_script: Vec<u8> = multisig_redeem_script(kps.iter().map(|pk| pk.x_only_public_key().0.serialize()), 2)
                    .expect("Test multisig redeem script");

                CONTEXT = Some(Box::new((kps, redeem_script)));
            });

            CONTEXT.as_ref().unwrap()
        }
    }

    // Mock multisig PSKT from example
    fn mock_pskt_constructor() -> PSKT<Constructor> {
        let (_, redeem_script) = mock_context();
        let pskt = PSKT::<Creator>::default().inputs_modifiable().outputs_modifiable();
        let input_0 = InputBuilder::default()
            .utxo_entry(UtxoEntry {
                amount: 12793000000000,
                script_public_key: pay_to_script_hash_script(redeem_script),
                block_daa_score: 36151168,
                is_coinbase: false,
            })
            .previous_outpoint(TransactionOutpoint {
                transaction_id: TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap(),
                index: 0,
            })
            .sig_op_count(2)
            .redeem_script(redeem_script.to_owned())
            .build()
            .expect("Mock PSKT constructor");

        pskt.constructor().input(input_0)
    }

    #[test]
    fn test_serialization() {
        let constructor = mock_pskt_constructor();
        let bundle = Bundle::from(constructor.clone());

        // Serialize to MessagePack
        let mut buf = Vec::new();
        encode::write(&mut buf, &bundle).expect("Serialize PSKB");
        println!("Serialized: {:?}", buf);

        assert!(!bundle.inner_list.is_empty());

        // todo: discuss why deserializing from MessagePack errors
        match decode::from_slice::<Bundle>(&buf) {
            Ok(bundle_constructor_deser) => {
                println!("Deserialized: {:?}", bundle_constructor_deser);
                let pskt_constructor_deser: Option<PSKT<Constructor>> =
                    bundle_constructor_deser.inner_list.first().map(|inner| PSKT::from(inner.clone()));
                match pskt_constructor_deser {
                    Some(_) => println!("PSKT<Constructor> deserialized successfully"),
                    None => println!("No elements in inner_list to deserialize"),
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize: {}", e);
                panic!()
            }
        }
    }

    #[test]
    fn test_bundle_creation() {
        let bundle = Bundle::new();
        assert!(bundle.inner_list.is_empty());
    }

    #[test]
    fn test_new_with_pskt() {
        let pskt = PSKT::<Creator>::default();
        let bundle = Bundle::from(pskt);
        assert_eq!(bundle.inner_list.len(), 1);
    }

    #[test]
    fn test_add_pskt() {
        let mut bundle = Bundle::new();
        let pskt = PSKT::<Creator>::default();
        bundle.add_pskt(pskt);
        assert_eq!(bundle.inner_list.len(), 1);
    }

    #[test]
    fn test_merge_bundles() {
        let mut bundle1 = Bundle::new();
        let mut bundle2 = Bundle::new();

        let inner1 = PSKTInner::default();
        let inner2 = PSKTInner::default();

        bundle1.add_inner(inner1.clone());
        bundle2.add_inner(inner2.clone());

        bundle1.merge(bundle2);

        assert_eq!(bundle1.inner_list.len(), 2);
    }
}
