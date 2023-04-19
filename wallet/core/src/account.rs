// use crate::accounts::{WalletAccountTrait, WalletAccountV0};
use crate::imports::*;
use crate::result::Result;
use crate::storage;
use crate::utxo::UtxoSet;
use crate::DynRpcApi;
use async_trait::async_trait;
use kaspa_notify::listener::ListenerId;
use kaspa_notify::scope::{Scope, UtxosChangedScope};
use kaspa_rpc_core::api::notifications::Notification;
use kaspa_rpc_core::notify::connection::ChannelConnection;
use std::sync::atomic::{AtomicBool, AtomicU64};
use workflow_core::channel::Channel;

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[wasm_bindgen]
pub enum AccountKind {
    V0,
    #[default]
    Bip32,
    MultiSig,
}

#[async_trait]
pub trait AccountT {
    // async fn connect(&self);
    // async fn disconnect(&self);
    // async fn reset();
}

pub struct Inner {
    listener_id: ListenerId,
    name: String,
    #[allow(dead_code)] //TODO: remove me
    title: String,
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
    rpc_api: Arc<DynRpcApi>,
    is_connected: AtomicBool,
    #[wasm_bindgen(js_name = "accountKind")]
    pub account_kind: AccountKind,
    // index of the private key in the wallet store
    #[allow(dead_code)] //TODO: remove me
    // #[wasm_bindgen(js_name = "privateKeyIndex")]
    keydata_id: storage::KeydataId,
}

impl Account {
    // pub fn new(rpc_api : Arc<DynRpcApi>, config : AccountConfig) -> Self {
    pub fn new(rpc_api: Arc<DynRpcApi>, stored: &storage::Account) -> Self {
        // let generator = match config.kind {
        //     AccountKind::V0 => WalletAccountV0,//Arc::new(V0Account::new(rpc_api.clone())),
        //     AccountKind::Bip32 => Arc::new(Bip32Account::new(rpc_api.clone())),
        //     AccountKind::MultiSig => Arc::new(MultiSigAccount::new(rpc_api.clone())),
        // };
        let channel = Channel::<Notification>::unbounded();
        let listener_id = rpc_api.register_new_listener(ChannelConnection::new(channel.sender));

        // rpc_api.register_new_listener();

        let inner = Inner { listener_id, name: stored.name.clone(), title: stored.title.clone() };

        Account {
            utxos: UtxoSet::default(),
            balance: AtomicU64::new(0),
            // _generator: Arc::new(config.clone()),
            rpc_api: rpc_api.clone(),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind: stored.account_kind,
            keydata_id: stored.keydata_id,
        }
    }

    pub async fn subscribe(&self) {
        // TODO query account interface
        let addresses = vec![];
        let utxos_changed_scope = UtxosChangedScope { addresses };
        let id = self.inner.lock().unwrap().listener_id;
        let _ = self.rpc_api.start_notify(id, Scope::UtxosChanged(utxos_changed_scope)).await;
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

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> u64 {
        self.balance.load(std::sync::atomic::Ordering::SeqCst)
    }
}
