use crate::result::Result;
use crate::tx::{Generator, MutableTransaction, Transaction};
use crate::utxo::UtxoEntryReference;
use crate::DynRpcApi;
use kaspa_consensus_core::tx::TransactionId;
// use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
use crate::tx::calc_schnorr_signature_hash;
use crate::tx::SigHashReusedValues;
use crate::tx::SignableTransaction;
use crate::tx::SIG_HASH_ALL;
use itertools::Itertools;
use kaspa_consensus_core::tx;
use kaspa_rpc_core::{RpcTransaction, RpcTransactionId};
use kaspa_utils::hex::ToHex;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use workflow_log::log_info;

struct Inner {
    generator: Generator,
    mutable_transaction: Mutex<MutableTransaction>,
    is_committed: AtomicBool,
}
pub struct PendingTransaction {
    inner: Arc<Inner>,
}

impl PendingTransaction {
    // pub fn new(generator: Generator, transaction: MutableTransaction, ) -> Self {
    pub fn new(generator: &Generator, transaction: Transaction, utxo_entries: Vec<UtxoEntryReference>) -> Self {
        let mutable_transaction = Mutex::new(MutableTransaction::new(transaction, utxo_entries.into()));

        Self { inner: Arc::new(Inner { generator: generator.clone(), mutable_transaction, is_committed: AtomicBool::new(false) }) }
    }

    pub fn id(&self) -> TransactionId {
        self.inner.mutable_transaction.lock().unwrap().id()
    }

    fn commit(&self) -> Result<()> {
        self.inner.is_committed.load(Ordering::SeqCst).then(|| {
            panic!("PendingTransaction::commit() called multiple times");
        });
        self.inner.is_committed.store(true, Ordering::SeqCst);
        if let Some(utxo_context) = self.inner.generator.utxo_context() {
            utxo_context.consume(&self.inner.mutable_transaction.lock().unwrap().entries)?;
        }
        Ok(())
    }

    pub fn rpc_transaction(&self) -> Result<RpcTransaction> {
        let transaction = self.inner.mutable_transaction.lock().unwrap();
        // TODO - remove this clone if feasible
        transaction.clone().try_into()
    }

    pub async fn submit(&self, rpc: &Arc<DynRpcApi>) -> Result<RpcTransactionId> {
        self.commit()?;

        let rpc_transaction: RpcTransaction = self.rpc_transaction()?; //transaction().try_into()?;

        Ok(rpc.submit_transaction(rpc_transaction, true).await?)
    }

    pub async fn log(&self) -> Result<()> {
        log_info!("pending transaction: {:?}", self.rpc_transaction()?);
        Ok(())
    }

    /// Sign a transaction using schnorr
    pub fn sign(&self, privkeys: &Vec<[u8; 32]>) -> Result<()> {
        use core::iter::once;
        let mut map = BTreeMap::new();
        for privkey in privkeys {
            let schnorr_key = secp256k1::KeyPair::from_seckey_slice(secp256k1::SECP256K1, privkey)?;
            let schnorr_public_key = schnorr_key.public_key().x_only_public_key().0;
            let script_pub_key_script = once(0x20).chain(schnorr_public_key.serialize().into_iter()).chain(once(0xac)).collect_vec();
            // TODO: Use script builder when available to create p2pk properly

            //map.insert(schnorr_public_key.serialize(), schnorr_key);
            map.insert(script_pub_key_script.to_hex(), schnorr_key);
            //println!("schnorr_key.public_key().serialize(): {:x?}", schnorr_public_key.serialize())
        }
        let mutable_tx = &*self.inner.mutable_transaction.lock()?;
        let mtx: tx::MutableTransaction<tx::Transaction> = mutable_tx.try_into()?;
        let inputs = mutable_tx.inputs()?;
        let mut reused_values = SigHashReusedValues::new();
        let entries = mutable_tx.entries.items();
        for i in 0..inputs.len() {
            let script = entries[i].as_ref().entry.script_public_key.script().to_hex();
            if let Some(schnorr_key) = map.get(&script) {
                let sig_hash = calc_schnorr_signature_hash(&mtx.as_verifiable(), i, SIG_HASH_ALL, &mut reused_values);
                let msg = secp256k1::Message::from_slice(sig_hash.as_bytes().as_slice()).unwrap();
                let sig: [u8; 64] = *schnorr_key.sign_schnorr(msg).as_ref();
                // This represents OP_DATA_65 <SIGNATURE+SIGHASH_TYPE> (since signature length is 64 bytes and SIGHASH_TYPE is one byte)
                inputs[i].inner().signature_script = std::iter::once(65u8).chain(sig).chain([SIG_HASH_ALL.to_u8()]).collect();
            }
        }
        Ok(())
    }

    // pub fn sign_with_signer(&mut self, signer: &Signer, verify_sig: bool) -> crate::Result<()> {
    //     // let
    //     signer.sign(self.mtx);
    //     let mut transactions = vec![];
    //     for mtx in self.transactions.clone() {
    //         transactions.push(signer.sign_transaction(mtx, verify_sig)?);
    //     }
    //     self.transactions = transactions;
    //     Ok(())
    // }
}
