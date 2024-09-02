//!
//! Pending transaction encapsulating a
//! transaction generated by the [`Generator`].
//!

use crate::imports::*;
use crate::result::Result;
use crate::rpc::DynRpcApi;
use crate::tx::{DataKind, Generator};
use crate::utxo::{UtxoContext, UtxoEntryId, UtxoEntryReference};
use kaspa_consensus_core::hashing::sighash_type::SigHashType;
use kaspa_consensus_core::sign::{sign_input, sign_with_multiple_v2, Signed};
use kaspa_consensus_core::tx::{SignableTransaction, Transaction, TransactionId};
use kaspa_rpc_core::{RpcTransaction, RpcTransactionId};

pub(crate) struct PendingTransactionInner {
    /// Generator that produced the transaction
    pub(crate) generator: Generator,
    /// UtxoEntryReferences of the pending transaction
    pub(crate) utxo_entries: AHashMap<UtxoEntryId, UtxoEntryReference>,
    /// Transaction Id (cached in pending to avoid mutex lock)
    pub(crate) id: TransactionId,
    /// Signable transaction (actual transaction that will be signed and sent)
    pub(crate) signable_tx: Mutex<SignableTransaction>,
    /// UTXO addresses used by this transaction
    pub(crate) addresses: Vec<Address>,
    /// Whether the transaction has been committed to the mempool via RPC
    pub(crate) is_submitted: AtomicBool,
    /// Payment value of the transaction (transaction destination amount)
    pub(crate) payment_value: Option<u64>,
    /// The index (position) of the change output in the transaction
    pub(crate) change_output_index: Option<usize>,
    /// Change value of the transaction (transaction change amount)
    pub(crate) change_output_value: u64,
    /// Total aggregate value of all inputs
    pub(crate) aggregate_input_value: u64,
    /// Total aggregate value of all outputs
    pub(crate) aggregate_output_value: u64,
    /// Minimum number of signatures required for the transaction
    /// (passed in during transaction creation). This value is used
    /// to estimate the mass of the transaction.
    pub(crate) minimum_signatures: u16,
    // Transaction mass
    pub(crate) mass: u64,
    /// Fees of the transaction
    pub(crate) fees: u64,
    /// Indicates the type of the transaction
    pub(crate) kind: DataKind,
}

impl std::fmt::Debug for PendingTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let transaction = self.transaction();
        f.debug_struct("PendingTransaction")
            .field("utxo_entries", &self.inner.utxo_entries)
            .field("addresses", &self.inner.addresses)
            .field("payment_value", &self.inner.payment_value)
            .field("change_output_index", &self.inner.change_output_index)
            .field("change_output_value", &self.inner.change_output_value)
            .field("aggregate_input_value", &self.inner.aggregate_input_value)
            .field("minimum_signatures", &self.inner.minimum_signatures)
            .field("mass", &self.inner.mass)
            .field("fees", &self.inner.fees)
            .field("kind", &self.inner.kind)
            .field("transaction", &transaction)
            .finish()
    }
}

/// Meta transaction encapsulating a transaction generated by the [`Generator`].
/// Contains auxiliary information about the transaction such as aggregate
/// input/output amounts, fees, etc.
#[derive(Clone)]
pub struct PendingTransaction {
    pub(crate) inner: Arc<PendingTransactionInner>,
}

impl PendingTransaction {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        generator: &Generator,
        transaction: Transaction,
        utxo_entries: Vec<UtxoEntryReference>,
        addresses: Vec<Address>,
        payment_value: Option<u64>,
        change_output_index: Option<usize>,
        change_output_value: u64,
        aggregate_input_value: u64,
        aggregate_output_value: u64,
        minimum_signatures: u16,
        mass: u64,
        fees: u64,
        kind: DataKind,
    ) -> Result<Self> {
        let id = transaction.id();
        let entries = utxo_entries.iter().map(|e| e.utxo.as_ref().into()).collect::<Vec<_>>();
        let signable_tx = Mutex::new(SignableTransaction::with_entries(transaction, entries));
        let utxo_entries = utxo_entries.into_iter().map(|entry| (entry.id(), entry)).collect::<AHashMap<_, _>>();
        Ok(Self {
            inner: Arc::new(PendingTransactionInner {
                generator: generator.clone(),
                id,
                signable_tx,
                utxo_entries,
                addresses,
                is_submitted: AtomicBool::new(false),
                payment_value,
                change_output_index,
                change_output_value,
                aggregate_input_value,
                aggregate_output_value,
                minimum_signatures,
                mass,
                fees,
                kind,
            }),
        })
    }

    pub fn id(&self) -> TransactionId {
        self.inner.id
    }

    pub fn generator(&self) -> &Generator {
        &self.inner.generator
    }

    pub fn source_utxo_context(&self) -> &Option<UtxoContext> {
        self.inner.generator.source_utxo_context()
    }

    pub fn destination_utxo_context(&self) -> &Option<UtxoContext> {
        self.inner.generator.destination_utxo_context()
    }

    /// Addresses used by the pending transaction
    pub fn addresses(&self) -> &Vec<Address> {
        &self.inner.addresses
    }

    /// Get UTXO entries [`AHashSet<UtxoEntryReference>`] of the pending transaction
    pub fn utxo_entries(&self) -> &AHashMap<UtxoEntryId, UtxoEntryReference> {
        &self.inner.utxo_entries
    }

    pub fn fees(&self) -> u64 {
        self.inner.fees
    }

    pub fn mass(&self) -> u64 {
        self.inner.mass
    }

    pub fn minimum_signatures(&self) -> u16 {
        self.inner.minimum_signatures
    }

    pub fn aggregate_input_value(&self) -> u64 {
        self.inner.aggregate_input_value
    }

    pub fn aggregate_output_value(&self) -> u64 {
        self.inner.aggregate_output_value
    }

    pub fn payment_value(&self) -> Option<u64> {
        self.inner.payment_value
    }

    pub fn change_output_index(&self) -> Option<usize> {
        self.inner.change_output_index
    }

    pub fn change_value(&self) -> u64 {
        self.inner.change_output_value
    }

    pub fn is_final(&self) -> bool {
        self.inner.kind.is_final()
    }

    pub fn is_batch(&self) -> bool {
        !self.inner.kind.is_final()
    }

    pub fn network_type(&self) -> NetworkType {
        self.inner.generator.network_type()
    }

    pub fn transaction(&self) -> Transaction {
        self.inner.signable_tx.lock().unwrap().tx.clone()
    }

    pub fn signable_transaction(&self) -> SignableTransaction {
        self.inner.signable_tx.lock().unwrap().clone()
    }

    pub fn rpc_transaction(&self) -> RpcTransaction {
        self.inner.signable_tx.lock().unwrap().tx.as_ref().into()
    }

    /// Submit the transaction on the supplied rpc
    pub async fn try_submit(&self, rpc: &Arc<DynRpcApi>) -> Result<RpcTransactionId> {
        // sanity check to prevent multiple invocations (for API use)
        self.inner.is_submitted.load(Ordering::SeqCst).then(|| {
            panic!("PendingTransaction::try_submit() called multiple times");
        });
        self.inner.is_submitted.store(true, Ordering::SeqCst);

        let rpc_transaction: RpcTransaction = self.rpc_transaction();

        // if we are running under UtxoProcessor
        if let Some(utxo_context) = self.inner.generator.source_utxo_context() {
            // lock UtxoProcessor notification ingest
            let _lock = utxo_context.processor().notification_lock().await;

            // register pending UTXOs with UtxoProcessor
            utxo_context.register_outgoing_transaction(self).await?;

            // try to submit transaction
            match rpc.submit_transaction(rpc_transaction, false).await {
                Ok(id) => {
                    // on successful submit, create a notification
                    utxo_context.notify_outgoing_transaction(self).await?;
                    Ok(id)
                }
                Err(error) => {
                    // in case of failure, remove transaction UTXOs from the consumed list
                    utxo_context.cancel_outgoing_transaction(self).await?;
                    Err(error.into())
                }
            }
        } else {
            // No UtxoProcessor present (API etc)
            Ok(rpc.submit_transaction(rpc_transaction, false).await?)
        }
    }

    pub async fn log(&self) -> Result<()> {
        log_info!("pending transaction: {:?}", self.rpc_transaction());
        Ok(())
    }

    pub fn try_sign(&self) -> Result<()> {
        let signer = self.inner.generator.signer().as_ref().expect("no signer in tx generator");
        let signed_tx = signer.try_sign(self.inner.signable_tx.lock()?.clone(), self.addresses())?;
        *self.inner.signable_tx.lock().unwrap() = signed_tx;
        Ok(())
    }

    pub fn create_input_signature(&self, input_index: usize, private_key: &[u8; 32], hash_type: SigHashType) -> Result<Vec<u8>> {
        let mutable_tx = self.inner.signable_tx.lock()?.clone();
        let verifiable_tx = mutable_tx.as_verifiable();

        Ok(sign_input(&verifiable_tx, input_index, private_key, hash_type))
    }

    pub fn fill_input(&self, input_index: usize, signature_script: Vec<u8>) -> Result<()> {
        let mut mutable_tx = self.inner.signable_tx.lock()?.clone();
        mutable_tx.tx.inputs[input_index].signature_script = signature_script;
        *self.inner.signable_tx.lock().unwrap() = mutable_tx;

        Ok(())
    }

    pub fn sign_input(&self, input_index: usize, private_key: &[u8; 32], hash_type: SigHashType) -> Result<()> {
        let mut mutable_tx = self.inner.signable_tx.lock()?.clone();

        let signature_script = {
            let verifiable_tx = &mutable_tx.as_verifiable();
            sign_input(verifiable_tx, input_index, private_key, hash_type)
        };

        mutable_tx.tx.inputs[input_index].signature_script = signature_script;
        *self.inner.signable_tx.lock().unwrap() = mutable_tx;

        Ok(())
    }

    pub fn try_sign_with_keys(&self, privkeys: &[[u8; 32]], check_fully_signed: Option<bool>) -> Result<()> {
        let mutable_tx = self.inner.signable_tx.lock()?.clone();
        let signed = sign_with_multiple_v2(mutable_tx, privkeys);

        let signed_tx = match signed {
            Signed::Fully(tx) => tx,
            Signed::Partially(_) => {
                if check_fully_signed.unwrap_or(true) {
                    signed.fully_signed()?
                } else {
                    signed.unwrap()
                }
            }
        };

        *self.inner.signable_tx.lock().unwrap() = signed_tx;
        Ok(())
    }
}
