// use crate::accounts::{WalletAccountTrait, WalletAccountV0};

use crate::result::Result;
use crate::storage::StoredWalletAccount;
use crate::utxo::UtxoSet;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::{atomic::AtomicU64, Arc};
use borsh::{BorshDeserialize, BorshSerialize};
// use kaspa_notify::connection::ChannelConnection;
use kaspa_notify::listener::ListenerId;
// use kaspa_notify::notification::Notification; 
use kaspa_rpc_core::api::notifications::Notification;
// use kaspa_notify::notification::Notification;
use kaspa_notify::scope::{Scope, UtxosChangedScope};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::notify::connection::ChannelConnection;
use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use workflow_core::channel::Channel;
use crate::DynRpcApi;
// use notify::{collector::RpcCoreCollector, connection::ChannelConnection},


#[derive(Debug, Clone, Copy, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[wasm_bindgen]
pub enum AccountKind {
    V0,
    Bip32,
    MultiSig,
}

impl Default for AccountKind {
    fn default() -> Self {
        AccountKind::Bip32
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub kind: AccountKind,
}

impl AccountConfig {
    pub fn new(kind: AccountKind) -> Self {
        Self { kind }
    }
}

#[async_trait]
pub trait AccountT {
    // async fn connect(&self);
    // async fn disconnect(&self);
    // async fn reset();
}



pub struct Inner {
    listener_id : ListenerId,
    name : String,
    title : String,
}

/// Wallet `Account` data structure. An account is typically a single
/// HD-key derivation (derived from a wallet or from an an external secret)
#[wasm_bindgen(inspectable)]
pub struct Account {
    // TODO bind with accounts/ primitives
    // _generator: Arc<dyn WalletAccountTrait>,
    inner: Arc<Mutex<Inner>>,
    utxos: UtxoSet,
    balance: AtomicU64,
    rpc_api : Arc<DynRpcApi>,
    is_connected : AtomicBool,
    pub account_kind : AccountKind,
    // index of the private key in the wallet store
    private_key_index : u32,
}

impl Account {

    // pub fn new(rpc_api : Arc<DynRpcApi>, config : AccountConfig) -> Self {
    pub fn new(rpc_api : Arc<DynRpcApi>, stored : &StoredWalletAccount) -> Self {

        // let generator = match config.kind {
        //     AccountKind::V0 => WalletAccountV0,//Arc::new(V0Account::new(rpc_api.clone())),
        //     AccountKind::Bip32 => Arc::new(Bip32Account::new(rpc_api.clone())),
        //     AccountKind::MultiSig => Arc::new(MultiSigAccount::new(rpc_api.clone())),
        // };
        let channel = Channel::<Notification>::unbounded();
        let listener_id = rpc_api.register_new_listener(ChannelConnection::new(channel.sender));

        // rpc_api.register_new_listener();

        let inner = Inner {
            listener_id, 
            name : stored.name.clone(),
            title : stored.title.clone(),
        };

        Account {
            utxos: UtxoSet::default(),
            balance: AtomicU64::new(0),
            // _generator: Arc::new(config.clone()),
            rpc_api : rpc_api.clone(),
            is_connected : AtomicBool::new(false),
            inner : Arc::new(Mutex::new(inner)),
            account_kind : stored.account_kind,
            private_key_index : stored.private_key_index,
        }
    }

    pub fn subscribe(&self) {
        // TODO query account interface
        let addresses = vec![];
        let utxos_changed_scope = UtxosChangedScope { addresses };
        self.rpc_api.start_notify(self.inner.lock().unwrap().listener_id, Scope::UtxosChanged(utxos_changed_scope));
    }

    pub async fn update_balance(&mut self) -> Result<u64> {
        let balance = self.utxos.calculate_balance().await?;
        self.balance.store(self.utxos.calculate_balance().await?, std::sync::atomic::Ordering::SeqCst);
        Ok(balance)
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().name.clone()
    }

    pub fn get_ls_string(&self) -> String {
        let name = self.name();
        let balance = self.balance.load(std::sync::atomic::Ordering::SeqCst);
        format!("{balance} - {name}")
    }


}

// impl AccountT for Account {
//     fn connect(&self) {
//         self.is_connected.store(true, std::sync::atomic::Ordering::SeqCst);
//     }
    
//     fn disconnect(&self) {
//         self.is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
//     }

    // fn reset() {

    // }
// }

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> u64 {
        self.balance.load(std::sync::atomic::Ordering::SeqCst)
    }
}
