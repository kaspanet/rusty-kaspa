use kaspa_bip32::{secp256k1, DerivationPath, KeyFingerprint};
use kaspa_consensus_core::{
    hashing::sighash_type::SigHashType,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{SignableTransaction, Transaction, TransactionInput, TransactionOutput},
};
use std::{collections::BTreeMap, fmt::Display, fmt::Formatter, future::Future, marker::PhantomData, ops::Deref};

mod error;
mod global;
mod input;

mod output;

mod role;

pub use error::Error;
pub use global::Global;
pub use input::Input;
pub use output::Output;
pub use role::{Combiner, Constructor, Creator, Signer, Updater};

#[derive(Default)]
pub struct Inner {
    /// The global map.
    pub global: Global,
    /// The corresponding key-value map for each input in the unsigned transaction.
    pub inputs: Vec<Input>,
    /// The corresponding key-value map for each output in the unsigned transaction.
    pub outputs: Vec<Output>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
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
pub type KeySource = (KeyFingerprint, DerivationPath);

pub type PartialSigs = BTreeMap<secp256k1::PublicKey, Signature>;

pub enum Signature {
    ECDSA(secp256k1::ecdsa::Signature),
    Schnorr(secp256k1::schnorr::Signature),
}

pub struct PSKT<ROLE> {
    inner_pskt: Inner,
    role: PhantomData<ROLE>,
}

impl<ROLE> Deref for PSKT<ROLE> {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner_pskt
    }
}

impl Default for PSKT<Creator> {
    fn default() -> Self {
        PSKT { inner_pskt: Default::default(), role: Default::default() }
    }
}

impl<R> PSKT<R> {
    pub fn determine_lock_time(&self) -> u64 {
        self.inputs.iter().map(|input: &Input| input.min_time).max().unwrap_or(self.global.fallback_lock_time).unwrap_or(0)
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

    /// Returns a PSBT [`Updater`] once construction is completed.
    pub fn updater(self) -> PSKT<Updater> {
        let pskt = self.no_more_inputs().no_more_outputs();
        PSKT { inner_pskt: pskt.inner_pskt, role: Default::default() }
    }

    pub fn signer(self) -> PSKT<Signer> {
        self.updater().signer()
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
}

impl PSKT<Signer> {
    fn unsigned_tx(&self) -> SignableTransaction {
        let tx = Transaction::new(
            self.global.tx_version,
            self.inputs
                .iter()
                .map(|Input { previous_outpoint, sequence, sig_op_count, .. }| TransactionInput {
                    previous_outpoint: *previous_outpoint,
                    signature_script: vec![],
                    sequence: sequence.unwrap_or(self.global.fallback_sequence.unwrap_or(0)),
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
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        let entries = self.inputs.iter().filter_map(|Input { utxo_entry, .. }| utxo_entry.clone()).collect();
        SignableTransaction::with_entries(tx, entries)
    }
    pub fn pass_signature_sync<SignFn, E>(&mut self, sign_fn: SignFn) -> Result<(), E>
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

        Ok(())
    }
    pub async fn pass_signature<SignFn, Fut, E>(&mut self, sign_fn: SignFn) -> Result<(), E>
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
        Ok(())
    }
}

pub struct SignInputOk {
    pub signature: Signature,
    pub pub_key: secp256k1::PublicKey,
    pub key_source: KeySource,
}

impl std::ops::Add for PSKT<Combiner> {
    type Output = Result<Self, CombineError>;

    fn add(mut self, mut rhs: Self) -> Self::Output {
        self.inner_pskt.global = (std::mem::take(&mut self.inner_pskt.global) + std::mem::take(&mut rhs.inner_pskt.global))?;
        macro_rules! combine {
            ($left:expr, $right:expr, $err: ty) => {
                if $left.len() >  $right.len() {
                    $left.iter_mut().zip($right.iter_mut()).try_for_each(
                        |(left, right)| -> Result<(), $err> {
                            *left = (std::mem::take(left) + std::mem::take(right))?;
                            Ok(())
                        },
                    )?;
                    $left
                } else {
                     $right.iter_mut().zip($left.iter_mut()).try_for_each(
                        |(left, right)| -> Result<(), $err> {
                            *left = (std::mem::take(left) + std::mem::take(right))?;
                            Ok(())
                        },
                    )?;
                    $right
                }
            };
        }
        self.inner_pskt.inputs = combine!(self.inner_pskt.inputs, rhs.inner_pskt.inputs, input::CombineError);
        self.inner_pskt.outputs = combine!(self.inner_pskt.outputs, rhs.inner_pskt.outputs, output::CombineError);
        Ok(self)
    }
}

/// Error combining pskt.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error(transparent)]
    Global(#[from] global::CombineError),
    #[error(transparent)]
    Inputs(#[from] input::CombineError),
    #[error(transparent)]
    Outputs(#[from] output::CombineError),
}

#[cfg(test)]
mod tests {

    // #[test]
    // fn it_works() {
    //     let result = add(2, 2);
    //     assert_eq!(result, 4);
    // }
}
