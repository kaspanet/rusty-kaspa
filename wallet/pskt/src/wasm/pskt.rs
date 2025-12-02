use crate::pskt::{Input, SignInputOk, PSKT as Native};
use crate::role::*;
use crate::wasm::signer::PrivateKeyArrayT;
use kaspa_consensus_core::network::{NetworkId, NetworkIdT, NetworkType, NetworkTypeT};
use kaspa_consensus_core::tx::{TransactionId, VerifiableTransaction};
use wasm_bindgen::prelude::*;
// use js_sys::Object;
use crate::prelude::Signature;
use crate::pskt::Inner;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_client::{Transaction, TransactionInput, TransactionInputT, TransactionOutput, TransactionOutputT};
use kaspa_consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync};
use kaspa_txscript::extract_script_pub_key_address;
use kaspa_wallet_keys::privatekey::PrivateKey;
use secp256k1::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use workflow_wasm::{
    convert::{Cast, CastFromJs, TryCastFromJs},
    // extensions::object::*,
    // error::Error as CastError,
};

use super::error::*;
use super::result::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", content = "payload")]
pub enum State {
    NoOp(Option<Inner>),
    Creator(Native<Creator>),
    Constructor(Native<Constructor>),
    Updater(Native<Updater>),
    Signer(Native<Signer>),
    Combiner(Native<Combiner>),
    Finalizer(Native<Finalizer>),
    Extractor(Native<Extractor>),
}

impl AsRef<State> for State {
    fn as_ref(&self) -> &State {
        self
    }
}

impl State {
    // this is not a Display trait intentionally
    pub fn display(&self) -> &'static str {
        match self {
            State::NoOp(_) => "Init",
            State::Creator(_) => "Creator",
            State::Constructor(_) => "Constructor",
            State::Updater(_) => "Updater",
            State::Signer(_) => "Signer",
            State::Combiner(_) => "Combiner",
            State::Finalizer(_) => "Finalizer",
            State::Extractor(_) => "Extractor",
        }
    }
}

impl From<State> for PSKT {
    fn from(state: State) -> Self {
        PSKT { state: Arc::new(Mutex::new(Some(state))) }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "PSKT | Transaction | string | undefined")]
    pub type CtorT;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Payload {
    data: String,
}

impl<T> TryFrom<Payload> for Native<T> {
    type Error = Error;

    fn try_from(value: Payload) -> Result<Self> {
        let Payload { data } = value;
        if data.starts_with("PSKT") {
            unimplemented!("PSKT binary serialization")
        } else {
            Ok(serde_json::from_str(&data).map_err(|err| format!("Invalid JSON: {err}"))?)
        }
    }
}

#[wasm_bindgen(inspectable)]
#[derive(Clone, CastFromJs)]
pub struct PSKT {
    state: Arc<Mutex<Option<State>>>,
}

impl TryCastFromJs for PSKT {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if JsValue::is_undefined(value.as_ref()) {
                Ok(PSKT::from(State::Creator(Native::<Creator>::default())))
            } else if let Some(data) = value.as_ref().as_string() {
                let pskt_inner: Inner = serde_json::from_str(&data).map_err(|_| Error::InvalidPayload)?;
                Ok(PSKT::from(State::NoOp(Some(pskt_inner))))
            } else if let Ok(transaction) = Transaction::try_owned_from(value) {
                let pskt_inner: Inner = transaction.try_into()?;
                Ok(PSKT::from(State::NoOp(Some(pskt_inner))))
            } else {
                Err(Error::InvalidPayload)
            }
        })
    }
}

#[wasm_bindgen]
impl PSKT {
    #[wasm_bindgen(constructor)]
    pub fn new(payload: CtorT) -> Result<PSKT> {
        PSKT::try_owned_from(payload.unchecked_into::<JsValue>().as_ref()).map_err(|err| Error::Ctor(err.to_string()))
    }

    #[wasm_bindgen(getter, js_name = "role")]
    pub fn role_getter(&self) -> String {
        self.state().as_ref().unwrap().display().to_string()
    }

    #[wasm_bindgen(getter, js_name = "payload")]
    pub fn payload_getter(&self) -> JsValue {
        let state = self.state();
        workflow_wasm::serde::to_value(state.as_ref().unwrap()).unwrap()
    }

    pub fn serialize(&self) -> String {
        let state = self.state();
        serde_json::to_string(state.as_ref().unwrap()).unwrap()
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, Option<State>> {
        self.state.lock().unwrap()
    }

    fn take(&self) -> State {
        self.state.lock().unwrap().take().unwrap()
    }

    fn replace(&self, state: State) -> Result<PSKT> {
        self.state.lock().unwrap().replace(state);
        Ok(self.clone())
    }

    /// Change role to `CREATOR`
    /// #[wasm_bindgen(js_name = toCreator)]
    pub fn creator(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => match inner {
                None => State::Creator(Native::default()),
                Some(_) => Err(Error::CreateNotAllowed)?,
            },
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    /// Change role to `CONSTRUCTOR`
    #[wasm_bindgen(js_name = toConstructor)]
    pub fn constructor(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Constructor(inner.ok_or(Error::NotInitialized)?.into()),
            State::Creator(pskt) => State::Constructor(pskt.constructor()),
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    /// Change role to `UPDATER`
    #[wasm_bindgen(js_name = toUpdater)]
    pub fn updater(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Updater(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(constructor) => State::Updater(constructor.updater()),
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    /// Change role to `SIGNER`
    #[wasm_bindgen(js_name = toSigner)]
    pub fn signer(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Signer(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(pskt) => State::Signer(pskt.signer()),
            State::Updater(pskt) => State::Signer(pskt.signer()),
            State::Combiner(pskt) => State::Signer(pskt.signer()),
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    /// Change role to `COMBINER`
    #[wasm_bindgen(js_name = toCombiner)]
    pub fn combiner(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Combiner(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(pskt) => State::Combiner(pskt.combiner()),
            State::Updater(pskt) => State::Combiner(pskt.combiner()),
            State::Signer(pskt) => State::Combiner(pskt.combiner()),
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    /// Change role to `FINALIZER`
    #[wasm_bindgen(js_name = toFinalizer)]
    pub fn finalizer(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Finalizer(inner.ok_or(Error::NotInitialized)?.into()),
            State::Combiner(pskt) => State::Finalizer(pskt.finalizer()),
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    /// Change role to `EXTRACTOR`
    #[wasm_bindgen(js_name = toExtractor)]
    pub fn extractor(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Extractor(inner.ok_or(Error::NotInitialized)?.into()),
            State::Finalizer(pskt) => State::Extractor(pskt.extractor()?),
            state => Err(Error::state(state))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = fallbackLockTime)]
    pub fn fallback_lock_time(&self, lock_time: u64) -> Result<PSKT> {
        let state = match self.take() {
            State::Creator(pskt) => State::Creator(pskt.fallback_lock_time(lock_time)),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = inputsModifiable)]
    pub fn inputs_modifiable(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::Creator(pskt) => State::Creator(pskt.inputs_modifiable()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = outputsModifiable)]
    pub fn outputs_modifiable(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::Creator(pskt) => State::Creator(pskt.outputs_modifiable()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = noMoreInputs)]
    pub fn no_more_inputs(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::Constructor(pskt) => State::Constructor(pskt.no_more_inputs()),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = noMoreOutputs)]
    pub fn no_more_outputs(&self) -> Result<PSKT> {
        let state = match self.take() {
            State::Constructor(pskt) => State::Constructor(pskt.no_more_outputs()),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = inputAndRedeemScript)]
    pub fn input_with_redeem(&self, input: &TransactionInputT, data: &JsValue) -> Result<PSKT> {
        let obj = js_sys::Object::from(data.clone());

        let input = TransactionInput::try_owned_from(input)?;
        let mut input: Input = input.try_into()?;
        let redeem_script = js_sys::Reflect::get(&obj, &"redeemScript".into())
            .expect("Missing redeemscript field")
            .as_string()
            .expect("redeemscript must be a string");
        input.redeem_script =
            Some(hex::decode(redeem_script).map_err(|e| Error::custom(format!("Redeem script is not a hex string: {}", e)))?);
        let state = match self.take() {
            State::Constructor(pskt) => State::Constructor(pskt.input(input)),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    pub fn input(&self, input: &TransactionInputT) -> Result<PSKT> {
        let input = TransactionInput::try_owned_from(input)?;
        let state = match self.take() {
            State::Constructor(pskt) => State::Constructor(pskt.input(input.try_into()?)),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    pub fn output(&self, output: &TransactionOutputT) -> Result<PSKT> {
        let output = TransactionOutput::try_owned_from(output)?;
        let state = match self.take() {
            State::Constructor(pskt) => State::Constructor(pskt.output(output.try_into()?)),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = setSequence)]
    pub fn set_sequence(&self, n: u64, input_index: usize) -> Result<PSKT> {
        let state = match self.take() {
            State::Updater(pskt) => State::Updater(pskt.set_sequence(n, input_index)?),
            _ => Err(Error::expected_state("Updater"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = calculateId)]
    pub fn calculate_id(&self) -> Result<TransactionId> {
        let state = self.state();
        match state.as_ref().unwrap() {
            State::Signer(pskt) => Ok(pskt.calculate_id()),
            _ => Err(Error::expected_state("Signer"))?,
        }
    }

    #[wasm_bindgen(js_name = calculateMass)]
    pub fn calculate_mass(&self, data: &JsValue) -> Result<u64> {
        let obj = js_sys::Object::from(data.clone());
        let network_id = js_sys::Reflect::get(&obj, &"networkId".into())
            .map_err(|_| Error::custom("networkId is missing"))?
            .as_string()
            .ok_or_else(|| Error::custom("networkId must be a string"))?;

        let network_id = NetworkType::from_str(&network_id).map_err(|e| Error::custom(format!("Invalid networkId: {}", e)))?;

        let cloned_pskt = self.clone();

        let extractor = {
            let finalizer = cloned_pskt.finalizer()?;

            let finalizer_state = finalizer.state().clone().unwrap();

            match finalizer_state {
                State::Finalizer(pskt) => {
                    for input in pskt.inputs.iter() {
                        if input.redeem_script.is_some() {
                            return Err(Error::custom("Mass calculation is not supported for inputs with redeem scripts"));
                        }
                    }
                    let pskt = pskt
                        .finalize_sync(|inner: &Inner| -> Result<Vec<Vec<u8>>> { Ok(vec![vec![0u8, 65]; inner.inputs.len()]) })
                        .map_err(|e| Error::custom(format!("Failed to finalize PSKT: {e}")))?;
                    pskt.extractor()?
                }
                _ => panic!("Finalizer state is not valid"),
            }
        };
        let tx = extractor
            .extract_tx_unchecked(&network_id.into())
            .map_err(|e| Error::custom(format!("Failed to extract transaction: {e}")))?;
        Ok(tx.tx.mass())
    }

    /// Extracts all input addresses from the PSKT.
    /// This is useful for figuring out which private keys are required for signing.
    #[wasm_bindgen]
    pub fn addresses(&self, network_id: &NetworkIdT) -> Result<Vec<Address>> {
        let network_id = NetworkId::try_cast_from(network_id)?.into_owned();
        let prefix: Prefix = network_id.into();

        let state_guard = self.state();
        let inner = match state_guard.as_ref().unwrap() {
            State::NoOp(Some(inner)) => inner,
            State::Creator(pskt) => pskt.deref(),
            State::Constructor(pskt) => pskt.deref(),
            State::Updater(pskt) => pskt.deref(),
            State::Signer(pskt) => pskt.deref(),
            State::Combiner(pskt) => pskt.deref(),
            State::Finalizer(pskt) => pskt.deref(),
            State::Extractor(pskt) => pskt.deref(),
            _ => return Err(Error::Custom("PSKT is not initialized".to_string())),
        };

        let addresses = inner
            .inputs
            .iter()
            .filter_map(|input| input.utxo_entry.as_ref())
            .filter_map(|utxo_entry| extract_script_pub_key_address(&utxo_entry.script_public_key, prefix).ok())
            .collect::<Vec<_>>();

        Ok(addresses)
    }

    /// Sign the PSKT with the provided private keys.
    /// The method will find the inputs corresponding to the private keys and sign them.
    /// This method performs partial signing, so if no private keys are provided, it will
    /// return an unmodified PSKT.
    #[wasm_bindgen]
    pub fn sign(&self, private_keys: PrivateKeyArrayT, network_type: &NetworkTypeT) -> Result<PSKT> {
        let prefix: Prefix = network_type.try_into()?;

        let private_keys: Vec<PrivateKey> =
            private_keys.try_into().map_err(|e| Error::Custom(format!("Invalid private keys: {:?}", e)))?;

        let mut key_map: HashMap<Address, PrivateKey> = HashMap::new();
        for pk in private_keys {
            // TODO: address this unwrap
            key_map.insert(pk.to_address(network_type).unwrap(), pk);
        }

        let signer_pskt: Native<Signer> = match self.take() {
            State::NoOp(inner) => inner.ok_or(Error::NotInitialized)?.into(),
            State::Creator(pskt) => pskt.constructor().signer(),
            State::Constructor(pskt) => pskt.signer(),
            State::Updater(pskt) => pskt.signer(),
            State::Signer(pskt) => pskt,
            State::Combiner(pskt) => pskt.signer(),
            state => return Err(Error::state(state))?,
        };

        let reused_values = SigHashReusedValuesUnsync::new();
        let signed_pskt = signer_pskt.pass_signature_sync::<_, Error>(|tx, sighash| {
            let signatures = tx
                .as_verifiable()
                .populated_inputs()
                .enumerate()
                .filter_map(|(idx, (_, utxo_entry))| {
                    extract_script_pub_key_address(&utxo_entry.script_public_key, prefix).ok().and_then(|address| {
                        key_map.get(&address).map(|private_key| {
                            let hash = calc_schnorr_signature_hash(&tx.as_verifiable(), idx, sighash[idx], &reused_values);
                            let msg =
                                Message::from_digest_slice(hash.as_bytes().as_slice()).map_err(|e| Error::Custom(e.to_string()))?;

                            let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &private_key.secret_bytes())
                                .map_err(|e| Error::Custom(e.to_string()))?;

                            Ok(SignInputOk {
                                signature: Signature::Schnorr(keypair.sign_schnorr(msg)),
                                pub_key: keypair.public_key(),
                                key_source: None,
                            })
                        })
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            Ok(signatures)
        })?;

        self.replace(State::Signer(signed_pskt))
    }
}

impl PSKT {
    pub fn inner(&self) -> Result<Inner> {
        let state_guard = self.state();
        let state = state_guard.as_ref().ok_or(Error::NotInitialized)?;

        let inner_clone = match state {
            State::NoOp(Some(inner)) => inner.clone(),
            State::Creator(pskt) => pskt.deref().clone(),
            State::Constructor(pskt) => pskt.deref().clone(),
            State::Updater(pskt) => pskt.deref().clone(),
            State::Signer(pskt) => pskt.deref().clone(),
            State::Combiner(pskt) => pskt.deref().clone(),
            State::Finalizer(pskt) => pskt.deref().clone(),
            State::Extractor(pskt) => pskt.deref().clone(),
            State::NoOp(None) => return Err(Error::NotInitialized),
        };
        Ok(inner_clone)
    }
}

#[cfg(test)]
mod tests {
    use js_sys::Array;
    use kaspa_addresses::Version;
    use kaspa_consensus_core::{
        tx::{TransactionOutpoint, UtxoEntry},
        Hash,
    };
    use kaspa_txscript::pay_to_address_script;
    use kaspa_wallet_keys::prelude::PublicKey;
    use wasm_bindgen_test::*;

    use crate::pskt::{Global, Inner as PSKTInner, InputBuilder};

    use super::*;

    fn _address_from_private_key(private_key: &PrivateKey) -> Address {
        let public_key = secp256k1::PublicKey::from_secret_key(
            secp256k1::SECP256K1,
            &secp256k1::SecretKey::from_slice(private_key.secret_bytes().as_slice()).unwrap(),
        );
        let (x_only_public_key, _) = public_key.x_only_public_key();
        let payload = x_only_public_key.serialize();
        Address::new(Prefix::Testnet, Version::PubKey, &payload)
    }

    fn _mock_pskt_inner(private_key: &PrivateKey) -> PSKTInner {
        let script_public_key = pay_to_address_script(&_address_from_private_key(private_key));

        let utxo_entry = UtxoEntry { amount: 1000, script_public_key, block_daa_score: 0, is_coinbase: false };

        let input = InputBuilder::default()
            .previous_outpoint(TransactionOutpoint::new(
                Hash::from_str("4bb07535dfd58e0b3cd64fd7155280872a0471bcf83095526ace0e38c6000000").unwrap(),
                4294967291,
            ))
            .utxo_entry(utxo_entry)
            .build()
            .unwrap();

        PSKTInner { global: Global::default(), inputs: vec![input], outputs: vec![] }
    }

    #[wasm_bindgen_test]
    fn _test_pskt_addresses() {
        let sk = secp256k1::SecretKey::new(&mut secp256k1::rand::thread_rng());
        let pk: PrivateKey = PrivateKey::from(&sk);
        let inner = _mock_pskt_inner(&pk);
        let address = _address_from_private_key(&pk);

        let pskt = PSKT::from(State::NoOp(Some(inner)));

        let network_id_t: NetworkIdT = JsValue::from_str("testnet-10").into();

        let addresses = pskt.addresses(&network_id_t).unwrap();

        assert_eq!(addresses.len(), 1);
        assert_eq!(addresses[0], address);
    }

    #[wasm_bindgen_test]
    fn _test_pskt_sign() {
        let sk = secp256k1::SecretKey::new(&mut secp256k1::rand::thread_rng());
        let pk: PrivateKey = PrivateKey::from(&sk);

        let inner = _mock_pskt_inner(&pk);
        let pskt = PSKT::from(State::NoOp(Some(inner)));

        // Check that there are no partial sigs initially
        let state: State = serde_wasm_bindgen::from_value(pskt.payload_getter()).unwrap();
        if let State::NoOp(Some(inner_before_sign)) = state {
            assert!(inner_before_sign.inputs[0].partial_sigs.is_empty());
        } else {
            panic!("Unexpected initial state");
        }

        let keys = Array::new();
        keys.push(&JsValue::from(pk.clone()));
        let keys_t: PrivateKeyArrayT = JsValue::from(keys).into();

        let signed_pskt = pskt.sign(keys_t, &JsValue::from_str("testnet-10").into()).unwrap();

        let signed_state: State = serde_wasm_bindgen::from_value(signed_pskt.payload_getter()).unwrap();

        match signed_state {
            State::Signer(native_pskt) => {
                let signed_inner = native_pskt.deref();
                assert_eq!(signed_inner.inputs[0].partial_sigs.len(), 1);
                let (pub_key, _signature) = signed_inner.inputs[0].partial_sigs.iter().next().unwrap();

                let wasm_pk: PublicKey = pub_key.into();
                assert_eq!(wasm_pk.to_string(), pk.to_public_key().unwrap().to_string());
            }
            _ => panic!("PSKT is not in Signer state after signing"),
        }
    }
}
