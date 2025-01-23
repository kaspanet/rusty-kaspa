use std::ops::Deref;

use super::error::*;
use super::result::*;
use crate::bundle::Bundle;
use crate::pskt::Combiner;
use crate::pskt::Constructor;
use crate::pskt::Creator;
use crate::pskt::Extractor;
use crate::pskt::Finalizer;
use crate::pskt::Inner;
use crate::pskt::Signer;
use crate::pskt::Updater;
use crate::pskt::PSKT as Native;
use crate::wasm::pskt::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone)]
#[derive(Debug)]
pub struct PsktBundle(Bundle);

impl Clone for Bundle {
    fn clone(&self) -> Self {
        Bundle(self.0.clone())
    }
}

#[wasm_bindgen]
impl PsktBundle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<PsktBundle> {
        let bundle = Bundle::new();
        Ok(PsktBundle(bundle))
    }

    #[wasm_bindgen]
    pub fn serialize(&self) -> Result<String> {
        self.0.serialize().map_err(Error::from)
    }

    #[wasm_bindgen]
    pub fn deserialize(hex_data: &str) -> Result<PsktBundle> {
        let bundle = Bundle::deserialize(hex_data).map_err(Error::from)?;
        Ok(PsktBundle(bundle))
    }

    #[wasm_bindgen]
    pub fn pskt_count(&self) -> usize {
        self.0 .0.len()
    }

    #[wasm_bindgen]
    pub fn add_pskt(&mut self, pskt: &PSKT) -> Result<()> {
        let payload = pskt.payload_getter();

        let payload_str: String = payload.as_string().unwrap_or_else(|| "Unable to convert to string".to_string());
        println!("{}", payload_str);

        // Try multiple deserialization methods
        if let Ok(inner) = serde_wasm_bindgen::from_value::<Inner>(payload.clone()) {
            self.0.add_inner(inner);
            Ok(())
        } else if let Ok(state) = serde_wasm_bindgen::from_value::<State>(payload) {
            match state {
                State::Creator(native_pskt) => {
                    self.0.add_inner(native_pskt.deref().clone());
                    Ok(())
                }
                State::Combiner(native_pskt) => {
                    self.0.add_inner(native_pskt.deref().clone());
                    Ok(())
                }
                State::Constructor(native_pskt) => {
                    self.0.add_inner(native_pskt.deref().clone());
                    Ok(())
                }
                State::Extractor(native_pskt) => {
                    self.0.add_inner(native_pskt.deref().clone());
                    Ok(())
                }
                State::Finalizer(native_pskt) => {
                    self.0.add_inner(native_pskt.deref().clone());
                    Ok(())
                }
                _ => Err(Error::Custom("Unsupported PSKT state".into())),
            }
        } else {
            Err(Error::Custom("Failed to deserialize PSKT payload".into()))
        }
    }

    #[wasm_bindgen]
    pub fn merge(&mut self, other: &PsktBundle) {
        self.0.merge(other.0.clone());
    }
}

impl From<Bundle> for PsktBundle {
    fn from(bundle: Bundle) -> Self {
        PsktBundle(bundle)
    }
}

impl From<PsktBundle> for Bundle {
    fn from(bundle: PsktBundle) -> Self {
        bundle.0
    }
}

impl AsRef<Bundle> for PsktBundle {
    fn as_ref(&self) -> &Bundle {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::pskt::Creator;

    use super::*;
    use serde_json::json;
    use wasm_bindgen::convert::IntoWasmAbi;
    use wasm_bindgen_test::wasm_bindgen_test;
    use wasm_bindgen_test::*;
    use workflow_wasm::convert::TryCastFromJs;
    // use web_sys::console;

    #[wasm_bindgen_test]
    fn test_pskt_bundle_creation() {
        let bundle = PsktBundle::new().expect("Failed to create PsktBundle");
        assert_eq!(bundle.pskt_count(), 0, "New bundle should have zero PSKTs");
    }

    #[wasm_bindgen_test]
    fn test_empty_pskt_bundle_serialize_deserialize() {
        // Create a bundle
        let mut bundle: PsktBundle = PsktBundle::new().expect("Failed to create PsktBundle");

        // Serialize the bundle
        let serialized = bundle.serialize().expect("Failed to serialize bundle");

        // Deserialize the bundle
        let deserialized_bundle = PsktBundle::deserialize(&serialized).expect("Failed to deserialize bundle");

        // Check that the deserialized bundle has the same number of PSKTs
        assert_eq!(bundle.pskt_count(), deserialized_bundle.pskt_count(), "Deserialized bundle should have same PSKT count");
    }

    #[wasm_bindgen_test]
    fn test_bundle_add() {
        let pskt_json = json!({
            "global": {
                "version": 0,
                "txVersion": 0,
                "fallbackLockTime": null,
                "inputsModifiable": false,
                "outputsModifiable": false,
                "inputCount": 0,
                "outputCount": 0,
                "xpubs": {},
                "id": null,
                "proprietaries": {}
            },
            "inputs": [
                {
                    "utxoEntry": {
                        "amount": 468928887,
                        "scriptPublicKey": "0000202d8a1414e62e081fb6bcf644e648c18061c2855575cac722f86324cad91dd0faac",
                        "blockDaaScore": 84981186,
                        "isCoinbase": false
                    },
                    "previousOutpoint": {
                        "transactionId": "69155d0e3380e8816dffe2671294ad104f0b3776f35bce1a22f0c21b1f908500",
                        "index": 0
                    },
                    "sequence": null,
                    "minTime": null,
                    "partialSigs": {},
                    "sighashType": 1,
                    "redeemScript": null,
                    "sigOpCount": 1,
                    "bip32Derivations": {},
                    "finalScriptSig": null,
                    "proprietaries": {}
                }
            ],
            "outputs": [
                {
                    "amount": 1500000000,
                    "scriptPublicKey": "0000",
                    "redeemScript": null,
                    "bip32Derivations": {},
                    "proprietaries": {}
                }
            ]
        });
        let json_str = pskt_json.to_string();
        let inner: Inner = serde_json::from_str(&json_str).expect("Failed to parse JSON");

        let native = Native::<Finalizer>::from(inner);

        let wasm_pskt = PSKT::from(State::Finalizer(native));

        // Create a bundle
        let mut bundle: PsktBundle = PsktBundle::new().expect("Failed to create PsktBundle");
        bundle.add_pskt(&wasm_pskt).expect("add pskt");

        // Serialize the bundle
        let serialized = bundle.serialize().expect("Failed to serialize bundle");
        // println!("{}", serialized);
        console_log!("{}", serialized);

        // Deserialize the bundle
        let deserialized_bundle = PsktBundle::deserialize(&serialized).expect("Failed to deserialize bundle");

        // Check that the deserialized bundle has the same number of PSKTs
        assert_eq!(bundle.pskt_count(), deserialized_bundle.pskt_count(), "Deserialized bundle should have same PSKT count");
    }
}
