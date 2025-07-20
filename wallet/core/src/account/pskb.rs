//!
//! Tools for interfacing wallet accounts with PSKBs.
//! (Partial Signed Kaspa Transaction Bundles).
//!

pub use crate::error::Error;
use crate::imports::*;
use crate::tx::PaymentOutput;
use crate::tx::PaymentOutputs;
use futures::stream;
use kaspa_bip32::{DerivationPath, KeyFingerprint, PrivateKey};
use kaspa_consensus_client::UtxoEntry as ClientUTXO;
use kaspa_consensus_core::hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync};
use kaspa_consensus_core::tx::VerifiableTransaction;
use kaspa_consensus_core::tx::{TransactionInput, UtxoEntry};
use kaspa_txscript::extract_script_pub_key_address;
use kaspa_txscript::opcodes::codes::OpData65;
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_wallet_core::tx::{Generator, GeneratorSettings, PaymentDestination, PendingTransaction};
pub use kaspa_wallet_pskt::bundle::Bundle;
use kaspa_wallet_pskt::bundle::{script_sig_to_address, unlock_utxo_outputs_as_batch_transaction_pskb};
use kaspa_wallet_pskt::prelude::lock_script_sig_templating_bytes;
use kaspa_wallet_pskt::prelude::KeySource;
use kaspa_wallet_pskt::prelude::{Finalizer, Inner, SignInputOk, Signature, Signer};
pub use kaspa_wallet_pskt::pskt::{Creator, PSKT};
use secp256k1::schnorr;
use secp256k1::{Message, PublicKey};
use std::iter;

struct PSKBSignerInner {
    keydata: PrvKeyData,
    account: Arc<dyn Account>,
    payment_secret: Option<Secret>,
    keys: Mutex<AHashMap<Address, [u8; 32]>>,
}

pub struct PSKBSigner {
    inner: Arc<PSKBSignerInner>,
}

impl PSKBSigner {
    pub fn new(account: Arc<dyn Account>, keydata: PrvKeyData, payment_secret: Option<Secret>) -> Self {
        Self { inner: Arc::new(PSKBSignerInner { keydata, account, payment_secret, keys: Mutex::new(AHashMap::new()) }) }
    }

    pub fn ingest(&self, addresses: &[Address]) -> Result<()> {
        let mut keys = self.inner.keys.lock()?;

        // Skip addresses that are already present in the key map.
        let addresses = addresses.iter().filter(|a| !keys.contains_key(a)).collect::<Vec<_>>();
        if !addresses.is_empty() {
            // let account = self.inner.account.clone().as_derivation_capable().expect("expecting derivation capable account");
            // let (receive, change) = account.derivation().addresses_indexes(&addresses)?;
            // let private_keys = account.create_private_keys(&self.inner.keydata, &self.inner.payment_secret, &receive, &change)?;
            let private_keys = self.inner.account.clone().create_address_private_keys(
                &self.inner.keydata,
                &self.inner.payment_secret,
                addresses.as_slice(),
            )?;
            for (address, private_key) in private_keys {
                keys.insert(address.clone(), private_key.to_bytes());
            }
        }
        Ok(())
    }

    fn public_key(&self, for_address: &Address) -> Result<PublicKey> {
        let keys = self.inner.keys.lock()?;
        match keys.get(for_address) {
            Some(private_key) => {
                let kp = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key)?;
                Ok(kp.public_key())
            }
            None => Err(Error::from("PSKBSigner address coverage error")),
        }
    }

    fn sign_schnorr(&self, for_address: &Address, message: Message) -> Result<schnorr::Signature> {
        let keys = self.inner.keys.lock()?;
        match keys.get(for_address) {
            Some(private_key) => {
                let schnorr_key = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, private_key)?;
                Ok(schnorr_key.sign_schnorr(message))
            }
            None => Err(Error::from("PSKBSigner address coverage error")),
        }
    }
}

pub struct PSKTGenerator {
    generator: Generator,
    signer: Arc<PSKBSigner>,
    prefix: Prefix,
}

impl PSKTGenerator {
    pub fn new(generator: Generator, signer: Arc<PSKBSigner>, prefix: Prefix) -> Self {
        Self { generator, signer, prefix }
    }

    pub fn stream(&self) -> impl Stream<Item = Result<PSKT<Signer>, Error>> {
        PSKTStream::new(self.generator.clone(), self.signer.clone(), self.prefix)
    }
}

struct PSKTStream {
    generator_stream: Pin<Box<dyn Stream<Item = Result<PendingTransaction, Error>> + Send>>,
    signer: Arc<PSKBSigner>,
    prefix: Prefix,
}

impl PSKTStream {
    fn new(generator: Generator, signer: Arc<PSKBSigner>, prefix: Prefix) -> Self {
        let generator_stream = generator.stream().map_err(Error::from);
        Self { generator_stream: Box::pin(generator_stream), signer, prefix }
    }
}

impl Stream for PSKTStream {
    type Item = Result<PSKT<Signer>, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_ref();

        let _prefix = this.prefix;
        let _signer = this.signer.clone();

        match self.get_mut().generator_stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(pending_tx))) => {
                let pskt = convert_pending_tx_to_pskt(pending_tx);
                Poll::Ready(Some(pskt))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

fn convert_pending_tx_to_pskt(pending_tx: PendingTransaction) -> Result<PSKT<Signer>, Error> {
    let signable_tx = pending_tx.signable_transaction();
    let verifiable_tx = signable_tx.as_verifiable();
    let populated_inputs: Vec<(&TransactionInput, &UtxoEntry)> = verifiable_tx.populated_inputs().collect();
    let pskt_inner = Inner::try_from((pending_tx.transaction(), populated_inputs.to_owned()))?;
    Ok(PSKT::<Signer>::from(pskt_inner))
}

pub async fn bundle_from_pskt_generator(generator: PSKTGenerator) -> Result<Bundle, Error> {
    let mut bundle: Bundle = Bundle::new();
    let mut stream = generator.stream();

    while let Some(pskt_result) = stream.next().await {
        match pskt_result {
            Ok(pskt) => bundle.add_pskt(pskt),
            Err(e) => return Err(e),
        }
    }

    Ok(bundle)
}
pub async fn pskb_signer_for_address(
    bundle: &Bundle,
    signer: Arc<PSKBSigner>,
    network_id: NetworkId,
    sign_for_address: Option<&Address>,
    derivation_path: Option<DerivationPath>,
    key_fingerprint: Option<KeyFingerprint>,
) -> Result<Bundle, Error> {
    let mut signed_bundle = Bundle::new();

    // If sign_for_address is provided, we'll use it for all signatures
    // Otherwise, collect addresses per PSKT
    let addresses_per_pskt: Vec<Vec<Address>> = if sign_for_address.is_some() {
        // Create a vec of single-address vecs
        bundle.iter().map(|_| vec![sign_for_address.unwrap().clone()]).collect()
    } else {
        // Collect addresses for each PSKT separately
        bundle
            .iter()
            .map(|inner| {
                inner
                    .inputs
                    .iter()
                    .filter_map(|input| input.utxo_entry.as_ref())
                    .filter_map(|utxo_entry| {
                        extract_script_pub_key_address(&utxo_entry.script_public_key.clone(), network_id.into()).ok()
                    })
                    .collect()
            })
            .collect()
    };

    // Prepare the signer with all unique addresses
    let all_addresses: Vec<Address> = addresses_per_pskt.iter().flat_map(|addresses| addresses.iter().cloned()).collect();
    signer.ingest(all_addresses.as_slice())?;

    // in case of keypair account, we don't have a derivation path,
    // so we need to skip the key source
    let mut key_source = None;
    if let Some(key_fingerprint) = key_fingerprint {
        if let Some(derivation_path) = derivation_path {
            key_source = Some(KeySource { key_fingerprint, derivation_path: derivation_path.clone() });
        }
    }

    // Process each PSKT in the bundle
    for (pskt_idx, pskt_inner) in bundle.iter().cloned().enumerate() {
        let pskt: PSKT<Signer> = PSKT::from(pskt_inner);
        let current_addresses = &addresses_per_pskt[pskt_idx];

        // Create new reused values for each PSKT
        let reused_values = SigHashReusedValuesUnsync::new();

        let sign = |signer_pskt: PSKT<Signer>| -> Result<PSKT<Signer>, Error> {
            signer_pskt
                .pass_signature_sync(|tx, sighash| -> Result<Vec<SignInputOk>, String> {
                    tx.tx
                        .inputs
                        .iter()
                        .enumerate()
                        .map(|(input_idx, _input)| {
                            let hash = calc_schnorr_signature_hash(&tx.as_verifiable(), input_idx, sighash[input_idx], &reused_values);
                            let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).map_err(|e| e.to_string())?;

                            // Get the appropriate address for this input
                            let address = if let Some(sign_addr) = sign_for_address {
                                sign_addr
                            } else {
                                current_addresses.get(input_idx).ok_or_else(|| format!("No address found for input {}", input_idx))?
                            };

                            let pub_key = signer.public_key(address).map_err(|e| format!("Failed to get public key: {}", e))?;

                            let signature = signer.sign_schnorr(address, msg).map_err(|e| format!("Failed to sign: {}", e))?;

                            Ok(SignInputOk { signature: Signature::Schnorr(signature), pub_key, key_source: key_source.clone() })
                        })
                        .collect()
                })
                .map_err(Error::from)
        };

        let signed_pskt = sign(pskt)?;
        signed_bundle.add_pskt(signed_pskt);
    }

    Ok(signed_bundle)
}

pub fn finalize_pskt_one_or_more_sig_and_redeem_script(pskt: PSKT<Finalizer>) -> Result<PSKT<Finalizer>, Error> {
    let result = pskt.finalize_sync(|inner: &Inner| -> Result<Vec<Vec<u8>>, String> {
        Ok(inner
            .inputs
            .iter()
            .map(|input| -> Vec<u8> {
                let signatures: Vec<_> = input
                    .partial_sigs
                    .clone()
                    .into_iter()
                    .flat_map(|(_, signature)| iter::once(OpData65).chain(signature.into_bytes()).chain([input.sighash_type.to_u8()]))
                    .collect();

                signatures
                    .into_iter()
                    .chain(
                        input
                            .redeem_script
                            .as_ref()
                            .map(|redeem_script| ScriptBuilder::new().add_data(redeem_script.as_slice()).unwrap().drain().to_vec())
                            .unwrap_or_default(),
                    )
                    .collect()
            })
            .collect())
    });

    match result {
        Ok(finalized_pskt) => Ok(finalized_pskt),
        Err(e) => Err(Error::from(e.to_string())),
    }
}

pub fn finalize_pskt_no_sig_and_redeem_script(pskt: PSKT<Finalizer>) -> Result<PSKT<Finalizer>, Error> {
    let result = pskt.finalize_sync(|inner: &Inner| -> Result<Vec<Vec<u8>>, String> {
        Ok(inner
            .inputs
            .iter()
            .map(|input| -> Vec<u8> {
                input
                    .redeem_script
                    .as_ref()
                    .map(|redeem_script| ScriptBuilder::new().add_data(redeem_script.as_slice()).unwrap().drain().to_vec())
                    .unwrap_or_default()
            })
            .collect())
    });

    match result {
        Ok(finalized_pskt) => Ok(finalized_pskt),
        Err(e) => Err(Error::from(e.to_string())),
    }
}

pub fn bundle_to_finalizer_stream(bundle: &Bundle) -> impl Stream<Item = Result<PSKT<Finalizer>, Error>> + Send {
    stream::iter(bundle.iter().cloned().collect::<Vec<_>>()).map(move |pskt_inner| {
        let pskt: PSKT<Creator> = PSKT::from(pskt_inner);
        let pskt_finalizer = pskt.constructor().updater().signer().finalizer();
        finalize_pskt_one_or_more_sig_and_redeem_script(pskt_finalizer)
    })
}

pub fn pskt_to_pending_transaction(
    finalized_pskt: PSKT<Finalizer>,
    network_id: NetworkId,
    change_address: Address,
    source_utxo_context: Option<UtxoContext>,
) -> Result<PendingTransaction, Error> {
    let inner_pskt = finalized_pskt.deref();
    let (utxo_entries_ref, aggregate_input_value): (Vec<UtxoEntryReference>, u64) = inner_pskt
        .inputs
        .iter()
        .filter_map(|input| {
            input.utxo_entry.as_ref().map(|ue| {
                (
                    UtxoEntryReference {
                        utxo: Arc::new(ClientUTXO {
                            address: Some(extract_script_pub_key_address(&ue.script_public_key, network_id.into()).unwrap()),
                            amount: ue.amount,
                            outpoint: input.previous_outpoint.into(),
                            script_public_key: ue.script_public_key.clone(),
                            block_daa_score: ue.block_daa_score,
                            is_coinbase: ue.is_coinbase,
                        }),
                    },
                    ue.amount,
                )
            })
        })
        .fold((Vec::new(), 0), |(mut vec, sum), (entry, amount)| {
            vec.push(entry);
            (vec, sum + amount)
        });
    let signed_tx = match finalized_pskt.extractor() {
        Ok(extractor) => match extractor.extract_tx(&network_id.into()) {
            Ok(tx) => tx.tx,
            Err(e) => return Err(Error::PendingTransactionFromPSKTError(e.to_string())),
        },
        Err(e) => return Err(Error::PendingTransactionFromPSKTError(e.to_string())),
    };
    let output: &Vec<kaspa_consensus_core::tx::TransactionOutput> = &signed_tx.outputs;
    if output.is_empty() {
        return Err(Error::Custom("0 outputs pskt is not supported".to_string()));
        // todo support 0 outputs
    }
    let recipient = extract_script_pub_key_address(&output[0].script_public_key, network_id.into())?;
    let fee_u: u64 = 0;

    let utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static> =
        Box::new(utxo_entries_ref.clone().into_iter());

    let final_transaction_destination = PaymentDestination::PaymentOutputs(PaymentOutputs::from((recipient, output[0].value)));

    let settings = GeneratorSettings {
        network_id,
        multiplexer: None,
        sig_op_count: 1,
        minimum_signatures: 1,
        change_address: change_address.clone(),
        utxo_iterator,
        priority_utxo_entries: None,
        source_utxo_context,
        destination_utxo_context: None,
        fee_rate: None,
        final_transaction_priority_fee: fee_u.into(),
        final_transaction_destination,
        final_transaction_payload: None,
    };

    // Create the Generator
    let generator = Generator::try_new(settings, None, None)?;

    let aggregate_output_value = output.iter().map(|output| output.value).sum::<u64>();

    let (change_output_index, change_output_value) = output
        .iter()
        .enumerate()
        .find_map(|(idx, output)| {
            if let Ok(address) = extract_script_pub_key_address(&output.script_public_key, change_address.prefix) {
                if address == change_address {
                    Some((Some(idx), output.value))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or((None, 0));

    // Create PendingTransaction (WIP)
    let addresses = utxo_entries_ref.iter().filter_map(|a| a.address()).collect();
    // todo where the source of mass and fees. why does it equal to zero?
    let pending_tx = PendingTransaction::try_new(
        &generator,
        signed_tx,
        utxo_entries_ref,
        addresses,
        Some(aggregate_output_value),
        change_output_index,
        change_output_value,
        aggregate_input_value,
        aggregate_output_value,
        1,
        0,
        0,
        kaspa_wallet_core::tx::DataKind::Final,
    )?;

    Ok(pending_tx)
}

// Allow creation of atomic commit reveal operation with two
// different parameters sets.
pub enum CommitRevealBatchKind {
    Manual { hop_payment: PaymentDestination, destination_payment: PaymentDestination },
    Parameterized { address: Address, commit_amount_sompi: u64 },
}

struct BundleCommitRevealConfig {
    pub address_commit: Address,
    pub addresses_reveal: Vec<Address>,
    pub commit_destination: PaymentDestination,
    pub redeem_script: Vec<u8>,
    pub payment_outputs: PaymentOutputs,
}

// Create signed atomic commit reveal PSKB.
pub async fn commit_reveal_batch_bundle(
    batch_config: CommitRevealBatchKind,
    reveal_fee_sompi: u64,
    script_sig: Vec<u8>,
    payload: Option<Vec<u8>>,
    fee_rate: Option<f64>,
    account: Arc<dyn Account>,
    wallet_secret: Secret,
    payment_secret: Option<Secret>,
    abortable: &Abortable,
) -> Result<Bundle, Error> {
    let network_id = account.wallet().clone().network_id()?;

    // Configure atomic batch of commit reveal transactions
    let conf: BundleCommitRevealConfig = match batch_config {
        CommitRevealBatchKind::Manual { hop_payment, destination_payment } => {
            let addr_commit = match hop_payment.clone() {
                PaymentDestination::Change => Err(Error::CommitRevealInvalidPaymentDestination),
                PaymentDestination::PaymentOutputs(payment_outputs) => {
                    payment_outputs.outputs.first().map(|out| out.address.clone()).ok_or(Error::CommitRevealEmptyPaymentOutputs)
                }
            }?;

            let (addresses, payment_outputs) = match destination_payment {
                PaymentDestination::Change => Err(Error::CommitRevealInvalidPaymentDestination),
                PaymentDestination::PaymentOutputs(payment_outputs) => {
                    Ok((payment_outputs.outputs.iter().map(|out| out.address.clone()).collect(), payment_outputs))
                }
            }?;

            BundleCommitRevealConfig {
                address_commit: addr_commit,
                addresses_reveal: addresses,
                commit_destination: hop_payment,
                redeem_script: script_sig,
                payment_outputs,
            }
        }
        CommitRevealBatchKind::Parameterized { address, commit_amount_sompi } => {
            let redeem_script = lock_script_sig_templating_bytes(script_sig.to_vec(), Some(&address.payload))
                .map_err(|_| Error::RevealRedeemScriptTemplateError)?;

            let lock_address = script_sig_to_address(&redeem_script, network_id.into())?;

            let amt_reveal: u64 = commit_amount_sompi - reveal_fee_sompi;

            BundleCommitRevealConfig {
                address_commit: lock_address.clone(),
                addresses_reveal: vec![address.clone()],
                commit_destination: PaymentDestination::from(PaymentOutput::new(lock_address, commit_amount_sompi)),
                redeem_script,
                payment_outputs: PaymentOutputs { outputs: vec![PaymentOutput::new(address.clone(), amt_reveal)] },
            }
        }
    };

    // Generate commit transaction
    let settings = GeneratorSettings::try_new_with_account(
        account.clone().as_dyn_arc(),
        conf.commit_destination.clone(),
        fee_rate.or(Some(1.0)),
        0u64.into(),
        payload,
    )
    .map_err(|e| Error::PSKTGenerationError(e.to_string()))?;

    let signer = Arc::new(PSKBSigner::new(
        account.clone().as_dyn_arc(),
        account.prv_key_data(wallet_secret.clone()).await?,
        payment_secret.clone(),
    ));

    let generator = Generator::try_new(settings, None, Some(abortable)).map_err(|e| Error::PSKTGenerationError(e.to_string()))?;

    let pskt_generator = PSKTGenerator::new(generator, signer, account.wallet().address_prefix()?);

    let bundle_commit = bundle_from_pskt_generator(pskt_generator).await.map_err(|e| Error::PSKTGenerationError(e.to_string()))?;

    // Generate reveal transaction
    let bundle_unlock = unlock_utxo_outputs_as_batch_transaction_pskb(
        conf.commit_destination.amount().unwrap(),
        &conf.address_commit,
        &conf.redeem_script,
        conf.payment_outputs.outputs.into_iter().map(|i| (i.address.clone(), i.amount)).collect(),
    )
    .map_err(|e| Error::PSKTGenerationError(e.to_string()))?;

    // Sign and finalize commit transaction
    let (mut merge_bundle, commit_transaction_id) = {
        let signed_pskb = account
            .clone()
            .pskb_sign(&bundle_commit, wallet_secret.clone(), payment_secret.clone(), None)
            .await
            .map_err(|_| Error::CommitTransactionSigningError)?;

        let merge_bundle = Bundle::deserialize(&signed_pskb.serialize()?).map_err(|_| Error::CommitRevealBundleMergeError)?;

        let pskt: PSKT<Signer> = PSKT::<Signer>::from(signed_pskb.as_ref()[0].to_owned());
        let finalizer = pskt.finalizer();

        let pskt_finalizer = finalize_pskt_one_or_more_sig_and_redeem_script(finalizer).map_err(|_| Error::PSKTFinalizationError)?;

        let transaction_id = pskt_to_pending_transaction(
            pskt_finalizer.clone(),
            network_id,
            account.change_address()?,
            account.utxo_context().clone().into(),
        )
        .map_err(|_| Error::CommitTransactionIdExtractionError)?
        .id();
        (merge_bundle, transaction_id)
    };

    // Set commit transaction ID in reveal batch transaction input
    let reveal_pskt: PSKT<Signer> = PSKT::<Signer>::from(bundle_unlock.as_ref()[0].to_owned());
    let unorphaned_bundle_unlock = Bundle::from(reveal_pskt.set_input_prev_transaction_id(commit_transaction_id));

    // Try signing with each reveal address
    for reveal_address in &conf.addresses_reveal {
        if let Ok(signed_pskb) = account
            .clone()
            .pskb_sign(&unorphaned_bundle_unlock, wallet_secret.clone(), payment_secret.clone(), Some(reveal_address))
            .await
        {
            merge_bundle.merge(signed_pskb);
            return Ok(merge_bundle);
        }
    }

    Err(Error::NoQualifiedRevealSignerFound)
}
