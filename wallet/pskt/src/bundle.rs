use crate::error::Error;
use crate::prelude::*;
use crate::pskt::{Inner as PSKTInner, PSKT};
// use crate::wasm::result;

use kaspa_addresses::{Address, Prefix};
// use kaspa_bip32::Prefix;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};

use hex;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script, pay_to_script_hash_script};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle(pub Vec<PSKTInner>);

impl<ROLE> From<PSKT<ROLE>> for Bundle {
    fn from(pskt: PSKT<ROLE>) -> Self {
        Bundle(vec![pskt.deref().clone()])
    }
}

impl<ROLE> From<Vec<PSKT<ROLE>>> for Bundle {
    fn from(pskts: Vec<PSKT<ROLE>>) -> Self {
        let inner_list = pskts.into_iter().map(|pskt| pskt.deref().clone()).collect();
        Bundle(inner_list)
    }
}

impl Bundle {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Adds an Inner instance to the bundle
    pub fn add_inner(&mut self, inner: PSKTInner) {
        self.0.push(inner);
    }

    /// Adds a PSKT instance to the bundle
    pub fn add_pskt<ROLE>(&mut self, pskt: PSKT<ROLE>) {
        self.0.push(pskt.deref().clone());
    }

    /// Merges another bundle into the current bundle
    pub fn merge(&mut self, other: Bundle) {
        for inner in other.0 {
            self.0.push(inner);
        }
    }

    /// Iterator over the inner PSKT instances
    pub fn iter(&self) -> std::slice::Iter<PSKTInner> {
        self.0.iter()
    }

    pub fn serialize(&self) -> Result<String, Error> {
        Ok(format!("PSKB{}", hex::encode(serde_json::to_string(self)?)))
    }

    pub fn deserialize(hex_data: &str) -> Result<Self, Error> {
        if let Some(hex_data) = hex_data.strip_prefix("PSKB") {
            Ok(serde_json::from_slice(hex::decode(hex_data)?.as_slice())?)
        } else {
            Err(Error::PskbPrefixError)
        }
    }

    pub fn display_format<F>(&self, network_id: NetworkId, sompi_formatter: F) -> String
    where
        F: Fn(u64, &NetworkType) -> String,
    {
        let mut result = "".to_string();

        for (pskt_index, bundle_inner) in self.0.iter().enumerate() {
            let pskt: PSKT<Signer> = PSKT::<Signer>::from(bundle_inner.to_owned());

            result.push_str(&format!("\r\nPSKT #{:02}\r\n", pskt_index + 1));

            for (key_inner, input) in pskt.clone().inputs.iter().enumerate() {
                result.push_str(&format!("Input #{:02}\r\n", key_inner + 1));

                if let Some(utxo_entry) = &input.utxo_entry {
                    result.push_str(&format!("  amount: {}\r\n", sompi_formatter(utxo_entry.amount, &NetworkType::from(network_id))));
                    result.push_str(&format!(
                        "  address: {}\r\n",
                        extract_script_pub_key_address(&utxo_entry.script_public_key, Prefix::from(network_id))
                            .expect("Input address")
                    ));
                }
            }

            result.push_str("---\r\n");

            for (key_inner, output) in pskt.clone().outputs.iter().enumerate() {
                result.push_str(&format!("Output #{:02}\r\n", key_inner + 1));
                result.push_str(&format!("  amount: {}\r\n", sompi_formatter(output.amount, &NetworkType::from(network_id))));
                result.push_str(&format!(
                    "  address: {}\r\n",
                    extract_script_pub_key_address(&output.script_public_key, Prefix::from(network_id)).expect("Input address")
                ));
            }
        }
        result
    }
}

impl AsRef<[PSKTInner]> for Bundle {
    fn as_ref(&self) -> &[PSKTInner] {
        self.0.as_slice()
    }
}

impl TryFrom<String> for Bundle {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Bundle::deserialize(&value)
    }
}

impl TryFrom<&str> for Bundle {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Error> {
        Bundle::deserialize(value)
    }
}
impl TryFrom<Bundle> for String {
    type Error = Error;
    fn try_from(value: Bundle) -> Result<String, Error> {
        match Bundle::serialize(&value) {
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
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};
    use kaspa_txscript::{multisig_redeem_script, pay_to_script_hash_script};
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
    fn test_pskb_serialization() {
        let constructor = mock_pskt_constructor();
        let bundle = Bundle::from(constructor.clone());

        println!("Bundle: {}", serde_json::to_string(&bundle).unwrap());

        // Serialize Bundle
        let serialized = bundle.serialize().map_err(|err| format!("Unable to serialize bundle: {err}")).unwrap();
        println!("Serialized: {}", serialized);

        assert!(!bundle.0.is_empty());

        match Bundle::deserialize(&serialized) {
            Ok(bundle_constructor_deser) => {
                println!("Deserialized: {:?}", bundle_constructor_deser);
                let pskt_constructor_deser: Option<PSKT<Constructor>> =
                    bundle_constructor_deser.0.first().map(|inner| PSKT::from(inner.clone()));
                match pskt_constructor_deser {
                    Some(_) => println!("PSKT<Constructor> deserialized successfully"),
                    None => println!("No elements in the inner list to deserialize"),
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize: {}", e);
                panic!()
            }
        }
    }

    #[test]
    fn test_pskb_bundle_creation() {
        let bundle = Bundle::new();
        assert!(bundle.0.is_empty());
    }

    #[test]
    fn test_pskb_new_with_pskt() {
        let pskt = PSKT::<Creator>::default();
        let bundle = Bundle::from(pskt);
        assert_eq!(bundle.0.len(), 1);
    }

    #[test]
    fn test_pskb_add_pskt() {
        let mut bundle = Bundle::new();
        let pskt = PSKT::<Creator>::default();
        bundle.add_pskt(pskt);
        assert_eq!(bundle.0.len(), 1);
    }

    #[test]
    fn test_pskb_merge_bundles() {
        let mut bundle1 = Bundle::new();
        let mut bundle2 = Bundle::new();

        let inner1 = PSKTInner::default();
        let inner2 = PSKTInner::default();

        bundle1.add_inner(inner1.clone());
        bundle2.add_inner(inner2.clone());

        bundle1.merge(bundle2);

        assert_eq!(bundle1.0.len(), 2);
    }
}
