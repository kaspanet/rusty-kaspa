use crate::result::Result;
use crate::tx::Generator;
use crate::utxo::UtxoEntryReference;
use crate::DynRpcApi;
use kaspa_addresses::Address;
use kaspa_consensus_core::sign::sign_with_multiple;
use kaspa_consensus_core::tx::{SignableTransaction, Transaction, TransactionId};
use kaspa_rpc_core::{RpcTransaction, RpcTransactionId};
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use workflow_log::log_info;

struct Inner {
    generator: Generator,
    utxo_entries: Vec<UtxoEntryReference>,
    mutable_tx: Mutex<SignableTransaction>,
    addresses: Vec<Address>,
    is_committed: AtomicBool,
}
pub struct PendingTransaction {
    inner: Arc<Inner>,
}

impl PendingTransaction {
    pub fn try_new(
        generator: &Generator,
        transaction: Transaction,
        utxo_entries: Vec<UtxoEntryReference>,
        addresses: Vec<Address>,
    ) -> Result<Self> {
        let entries = utxo_entries.iter().map(|e| e.utxo.entry.clone()).collect::<Vec<_>>();
        let mutable_tx = Mutex::new(SignableTransaction::with_entries(transaction, entries));
        Ok(Self {
            inner: Arc::new(Inner {
                generator: generator.clone(),
                mutable_tx,
                utxo_entries,
                addresses,
                is_committed: AtomicBool::new(false),
            }),
        })
    }

    pub fn id(&self) -> TransactionId {
        self.inner.mutable_tx.lock().unwrap().id()
    }

    /// Addresses used by the pending transaction
    pub fn addresses(&self) -> &Vec<Address> {
        &self.inner.addresses
    }

    fn commit(&self) -> Result<()> {
        self.inner.is_committed.load(Ordering::SeqCst).then(|| {
            panic!("PendingTransaction::commit() called multiple times");
        });
        self.inner.is_committed.store(true, Ordering::SeqCst);
        if let Some(utxo_context) = self.inner.generator.utxo_context() {
            // utxo_context.consume(&self.inner.mutable_transaction.lock().unwrap().entries)?;
            utxo_context.consume(&self.inner.utxo_entries)?;
        }
        Ok(())
    }

    pub fn rpc_transaction(&self) -> Result<RpcTransaction> {
        Ok(self.inner.mutable_tx.lock().unwrap().tx.as_ref().into())
    }

    /// Submit the transaction on the supplied rpc
    pub async fn try_submit(&self, rpc: &Arc<DynRpcApi>) -> Result<RpcTransactionId> {
        self.commit()?; // commit transactions only if we are submitting
        let rpc_transaction: RpcTransaction = self.rpc_transaction()?;
        Ok(rpc.submit_transaction(rpc_transaction, true).await?)
    }

    pub async fn log(&self) -> Result<()> {
        log_info!("pending transaction: {:?}", self.rpc_transaction()?);
        Ok(())
    }

    pub fn try_sign(&self) -> Result<()> {
        let signer = self.inner.generator.signer().as_ref().expect("no signer in tx generator");
        let signed_tx = signer.try_sign(self.inner.mutable_tx.lock()?.clone(), self.addresses())?;
        *self.inner.mutable_tx.lock().unwrap() = signed_tx;
        Ok(())
    }

    pub fn try_sign_with_keys(&self, privkeys: Vec<[u8; 32]>) -> Result<()> {
        let mutable_tx = self.inner.mutable_tx.lock()?.clone();
        let _signed = sign_with_multiple(mutable_tx, privkeys);
        Ok(())
    }
}
