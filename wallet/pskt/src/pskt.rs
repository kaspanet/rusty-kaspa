use kaspa_bip32::{secp256k1, DerivationPath, KeyFingerprint};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::{collections::BTreeMap, fmt::Display, fmt::Formatter, future::Future, marker::PhantomData, ops::Deref};

pub use crate::error::Error;
pub use crate::global::{Global, GlobalBuilder};
pub use crate::input::{Input, InputBuilder};
pub use crate::output::{Output, OutputBuilder};
pub use crate::role::{Combiner, Constructor, Creator, Extractor, Finalizer, Signer, Updater};
use kaspa_consensus_core::tx::UtxoEntry;
use kaspa_consensus_core::{
    hashing::sighash_type::SigHashType,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{MutableTransaction, SignableTransaction, Transaction, TransactionId, TransactionInput, TransactionOutput},
};
use kaspa_txscript::{caches::Cache, TxScriptEngine};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Inner {
    /// The global map.
    pub global: Global,
    /// The corresponding key-value map for each input in the unsigned transaction.
    pub inputs: Vec<Input>,
    /// The corresponding key-value map for each output in the unsigned transaction.
    pub outputs: Vec<Output>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Version {
    #[default]
    Zero = 0,
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::Zero => write!(f, "{}", Version::Zero as u8),
        }
    }
}

/// Full information on the used extended public key: fingerprint of the
/// master extended public key and a derivation path from it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeySource {
    #[serde(with = "kaspa_utils::serde_bytes_fixed")]
    pub key_fingerprint: KeyFingerprint,
    pub derivation_path: DerivationPath,
}

impl KeySource {
    pub fn new(key_fingerprint: KeyFingerprint, derivation_path: DerivationPath) -> Self {
        Self { key_fingerprint, derivation_path }
    }
}

pub type PartialSigs = BTreeMap<secp256k1::PublicKey, Signature>;

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "camelCase")]
pub enum Signature {
    ECDSA(secp256k1::ecdsa::Signature),
    Schnorr(secp256k1::schnorr::Signature),
}

impl Signature {
    pub fn into_bytes(self) -> [u8; 64] {
        match self {
            Signature::ECDSA(s) => s.serialize_compact(),
            Signature::Schnorr(s) => s.serialize(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PSKT<ROLE> {
    #[serde(flatten)]
    inner_pskt: Inner,
    #[serde(skip_serializing, default)]
    role: PhantomData<ROLE>,
}

impl<ROLE> From<Inner> for PSKT<ROLE> {
    fn from(inner_pskt: Inner) -> Self {
        PSKT { inner_pskt, role: Default::default() }
    }
}

impl<ROLE> Clone for PSKT<ROLE> {
    fn clone(&self) -> Self {
        PSKT { inner_pskt: self.inner_pskt.clone(), role: Default::default() }
    }
}

impl<ROLE> Deref for PSKT<ROLE> {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner_pskt
    }
}

impl<R> PSKT<R> {
    fn unsigned_tx(&self) -> SignableTransaction {
        let tx = Transaction::new(
            self.global.tx_version,
            self.inputs
                .iter()
                .map(|Input { previous_outpoint, sequence, sig_op_count, .. }| TransactionInput {
                    previous_outpoint: *previous_outpoint,
                    signature_script: vec![],
                    sequence: sequence.unwrap_or(u64::MAX),
                    sig_op_count: sig_op_count.unwrap_or(0),
                })
                .collect(),
            self.outputs
                .iter()
                .map(|Output { amount, script_public_key, .. }: &Output| TransactionOutput {
                    value: *amount,
                    script_public_key: script_public_key.clone(),
                })
                .collect(),
            self.determine_lock_time(),
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let entries = self.inputs.iter().filter_map(|Input { utxo_entry, .. }| utxo_entry.clone()).collect();
        SignableTransaction::with_entries(tx, entries)
    }

    fn calculate_id_internal(&self) -> TransactionId {
        self.unsigned_tx().tx.id()
    }

    fn determine_lock_time(&self) -> u64 {
        self.inputs.iter().map(|input: &Input| input.min_time).max().unwrap_or(self.global.fallback_lock_time).unwrap_or(0)
    }

    pub fn to_hex(&self) -> Result<String, Error> {
        Ok(format!("PSKT{}", hex::encode(serde_json::to_string(self)?)))
    }

    pub fn from_hex(hex_data: &str) -> Result<Self, Error> {
        if let Some(hex_data) = hex_data.strip_prefix("PSKT") {
            Ok(serde_json::from_slice(hex::decode(hex_data)?.as_slice())?)
        } else {
            Err(Error::PsktPrefixError)
        }
    }
}

impl Default for PSKT<Creator> {
    fn default() -> Self {
        PSKT { inner_pskt: Default::default(), role: Default::default() }
    }
}

impl PSKT<Creator> {
    /// Sets the fallback lock time.
    pub fn fallback_lock_time(mut self, fallback: u64) -> Self {
        self.inner_pskt.global.fallback_lock_time = Some(fallback);
        self
    }

    // todo generic const
    /// Sets the inputs modifiable bit in the transaction modifiable flags.
    pub fn inputs_modifiable(mut self) -> Self {
        self.inner_pskt.global.inputs_modifiable = true;
        self
    }
    // todo generic const
    /// Sets the outputs modifiable bit in the transaction modifiable flags.
    pub fn outputs_modifiable(mut self) -> Self {
        self.inner_pskt.global.outputs_modifiable = true;
        self
    }

    pub fn constructor(self) -> PSKT<Constructor> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }
}

impl PSKT<Constructor> {
    // todo generic const
    /// Marks that the `PSKT` can not have any more inputs added to it.
    pub fn no_more_inputs(mut self) -> Self {
        self.inner_pskt.global.inputs_modifiable = false;
        self
    }
    // todo generic const
    /// Marks that the `PSKT` can not have any more outputs added to it.
    pub fn no_more_outputs(mut self) -> Self {
        self.inner_pskt.global.outputs_modifiable = false;
        self
    }

    /// Adds an input to the PSKT.
    pub fn input(mut self, input: Input) -> Self {
        self.inner_pskt.inputs.push(input);
        self.inner_pskt.global.input_count += 1;
        self
    }

    /// Adds an output to the PSKT.
    pub fn output(mut self, output: Output) -> Self {
        self.inner_pskt.outputs.push(output);
        self.inner_pskt.global.output_count += 1;
        self
    }

    /// Returns a PSKT [`Updater`] once construction is completed.
    pub fn updater(self) -> PSKT<Updater> {
        let pskt = self.no_more_inputs().no_more_outputs();
        PSKT { inner_pskt: pskt.inner_pskt, role: Default::default() }
    }

    pub fn signer(self) -> PSKT<Signer> {
        self.updater().signer()
    }

    pub fn combiner(self) -> PSKT<Combiner> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }
}

impl PSKT<Updater> {
    pub fn set_sequence(mut self, n: u64, input_index: usize) -> Result<Self, Error> {
        self.inner_pskt.inputs.get_mut(input_index).ok_or(Error::OutOfBounds)?.sequence = Some(n);
        Ok(self)
    }

    pub fn signer(self) -> PSKT<Signer> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }

    pub fn combiner(self) -> PSKT<Combiner> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }
}

impl PSKT<Signer> {
    // todo use iterator instead of vector
    pub fn pass_signature_sync<SignFn, E>(mut self, sign_fn: SignFn) -> Result<Self, E>
    where
        E: Display,
        SignFn: FnOnce(SignableTransaction, Vec<SigHashType>) -> Result<Vec<SignInputOk>, E>,
    {
        let unsigned_tx = self.unsigned_tx();
        let sighashes = self.inputs.iter().map(|input| input.sighash_type).collect();
        self.inner_pskt.inputs.iter_mut().zip(sign_fn(unsigned_tx, sighashes)?).for_each(
            |(input, SignInputOk { signature, pub_key, key_source })| {
                input.bip32_derivations.insert(pub_key, key_source);
                input.partial_sigs.insert(pub_key, signature);
            },
        );

        Ok(self)
    }
    // todo use iterator instead of vector
    pub async fn pass_signature<SignFn, Fut, E>(mut self, sign_fn: SignFn) -> Result<Self, E>
    where
        E: Display,
        Fut: Future<Output = Result<Vec<SignInputOk>, E>>,
        SignFn: FnOnce(SignableTransaction, Vec<SigHashType>) -> Fut,
    {
        let unsigned_tx = self.unsigned_tx();
        let sighashes = self.inputs.iter().map(|input| input.sighash_type).collect();
        self.inner_pskt.inputs.iter_mut().zip(sign_fn(unsigned_tx, sighashes).await?).for_each(
            |(input, SignInputOk { signature, pub_key, key_source })| {
                input.bip32_derivations.insert(pub_key, key_source);
                input.partial_sigs.insert(pub_key, signature);
            },
        );
        Ok(self)
    }

    pub fn calculate_id(&self) -> TransactionId {
        self.calculate_id_internal()
    }

    pub fn finalizer(self) -> PSKT<Finalizer> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }

    pub fn combiner(self) -> PSKT<Combiner> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInputOk {
    pub signature: Signature,
    pub pub_key: secp256k1::PublicKey,
    pub key_source: Option<KeySource>,
}

impl<R> std::ops::Add<PSKT<R>> for PSKT<Combiner> {
    type Output = Result<Self, CombineError>;

    fn add(mut self, mut rhs: PSKT<R>) -> Self::Output {
        self.inner_pskt.global = (self.inner_pskt.global + rhs.inner_pskt.global)?;
        macro_rules! combine {
            ($left:expr, $right:expr, $err: ty) => {
                if $left.len() > $right.len() {
                    $left.iter_mut().zip($right.iter_mut()).try_for_each(|(left, right)| -> Result<(), $err> {
                        *left = (std::mem::take(left) + std::mem::take(right))?;
                        Ok(())
                    })?;
                    $left
                } else {
                    $right.iter_mut().zip($left.iter_mut()).try_for_each(|(left, right)| -> Result<(), $err> {
                        *left = (std::mem::take(left) + std::mem::take(right))?;
                        Ok(())
                    })?;
                    $right
                }
            };
        }
        // todo add sort to build deterministic combination
        self.inner_pskt.inputs = combine!(self.inner_pskt.inputs, rhs.inner_pskt.inputs, crate::input::CombineError);
        self.inner_pskt.outputs = combine!(self.inner_pskt.outputs, rhs.inner_pskt.outputs, crate::output::CombineError);
        Ok(self)
    }
}

impl PSKT<Combiner> {
    pub fn signer(self) -> PSKT<Signer> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }
    pub fn finalizer(self) -> PSKT<Finalizer> {
        PSKT { inner_pskt: self.inner_pskt, role: Default::default() }
    }
}

impl PSKT<Finalizer> {
    pub fn finalize_sync<E: Display>(
        self,
        final_sig_fn: impl FnOnce(&Inner) -> Result<Vec<Vec<u8>>, E>,
    ) -> Result<Self, FinalizeError<E>> {
        let sigs = final_sig_fn(&self);
        self.finalize_internal(sigs)
    }

    pub async fn finalize<F, Fut, E>(self, final_sig_fn: F) -> Result<Self, FinalizeError<E>>
    where
        E: Display,
        F: FnOnce(&Inner) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<u8>>, E>>,
    {
        let sigs = final_sig_fn(&self).await;
        self.finalize_internal(sigs)
    }

    pub fn id(&self) -> Option<TransactionId> {
        self.global.id
    }

    pub fn extractor(self) -> Result<PSKT<Extractor>, TxNotFinalized> {
        if self.global.id.is_none() {
            Err(TxNotFinalized {})
        } else {
            Ok(PSKT { inner_pskt: self.inner_pskt, role: Default::default() })
        }
    }

    fn finalize_internal<E: Display>(mut self, sigs: Result<Vec<Vec<u8>>, E>) -> Result<Self, FinalizeError<E>> {
        let sigs = sigs?;
        if sigs.len() != self.inputs.len() {
            return Err(FinalizeError::WrongFinalizedSigsCount { expected: self.inputs.len(), actual: sigs.len() });
        }
        self.inner_pskt.inputs.iter_mut().enumerate().zip(sigs).try_for_each(|((idx, input), sig)| {
            if sig.is_empty() {
                return Err(FinalizeError::EmptySignature(idx));
            }
            input.sequence = Some(input.sequence.unwrap_or(u64::MAX)); // todo discussable
            input.final_script_sig = Some(sig);
            Ok(())
        })?;
        self.inner_pskt.global.id = Some(self.calculate_id_internal());
        Ok(self)
    }
}

impl PSKT<Extractor> {
    pub fn extract_tx_unchecked(self) -> Result<impl FnOnce(u64) -> (Transaction, Vec<Option<UtxoEntry>>), TxNotFinalized> {
        let tx = self.unsigned_tx();
        let entries = tx.entries;
        let mut tx = tx.tx;
        tx.inputs.iter_mut().zip(self.inner_pskt.inputs).try_for_each(|(dest, src)| {
            dest.signature_script = src.final_script_sig.ok_or(TxNotFinalized {})?;
            Ok(())
        })?;
        Ok(move |mass| {
            tx.set_mass(mass);
            (tx, entries)
        })
    }

    pub fn extract_tx(self) -> Result<impl FnOnce(u64) -> (Transaction, Vec<Option<UtxoEntry>>), ExtractError> {
        let (tx, entries) = self.extract_tx_unchecked()?(0);

        let tx = MutableTransaction::with_entries(tx, entries.into_iter().flatten().collect());
        use kaspa_consensus_core::tx::VerifiableTransaction;
        {
            let tx = tx.as_verifiable();
            let cache = Cache::new(10_000);
            let reused_values = SigHashReusedValuesUnsync::new();

            tx.populated_inputs().enumerate().try_for_each(|(idx, (input, entry))| {
                TxScriptEngine::from_transaction_input(&tx, input, idx, entry, &reused_values, &cache)?.execute()?;
                <Result<(), ExtractError>>::Ok(())
            })?;
        }
        let entries = tx.entries;
        let tx = tx.tx;
        let closure = move |mass| {
            tx.set_mass(mass);
            (tx, entries)
        };
        Ok(closure)
    }
}

/// Error combining pskt.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error(transparent)]
    Global(#[from] crate::global::CombineError),
    #[error(transparent)]
    Inputs(#[from] crate::input::CombineError),
    #[error(transparent)]
    Outputs(#[from] crate::output::CombineError),
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum FinalizeError<E> {
    #[error("Signatures count mismatch")]
    WrongFinalizedSigsCount { expected: usize, actual: usize },
    #[error("Signatures at index: {0} is empty")]
    EmptySignature(usize),
    #[error(transparent)]
    FinalaziCb(#[from] E),
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    #[error(transparent)]
    TxScriptError(#[from] kaspa_txscript_errors::TxScriptError),
    #[error(transparent)]
    TxNotFinalized(#[from] TxNotFinalized),
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("Transaction is not finalized")]
pub struct TxNotFinalized {}

#[cfg(test)]
mod tests {

    // #[test]
    // fn it_works() {
    //     let result = add(2, 2);
    //     assert_eq!(result, 4);
    // }
}
