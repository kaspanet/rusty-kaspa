use crate::accounts::AddressGeneratorTrait;
use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use crate::storage::{self, PrvKeyDataId, PubKeyData};
use crate::utxo::{UtxoEntryReference, UtxoOrdering, UtxoSet};
use crate::DynRpcApi;
use async_trait::async_trait;
use kaspa_notify::listener::ListenerId;
use kaspa_notify::scope::{Scope, UtxosChangedScope};
use kaspa_rpc_core::api::notifications::Notification;
use kaspa_rpc_core::notify::connection::ChannelConnection;
use std::sync::atomic::{AtomicBool, AtomicU64};
use workflow_core::channel::Channel;

#[derive(Default, Clone)]
pub struct Estimate {
    pub total_sompi: u64,
    pub fees_sompi: u64,
    pub utxos: usize,
    pub transactions: usize,
}

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
    pub listener_id: ListenerId,
    pub stored: storage::Account,
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
    rpc: Arc<DynRpcApi>,
    is_connected: AtomicBool,
    #[wasm_bindgen(js_name = "accountKind")]
    pub account_kind: AccountKind,
    pub account_index: u32,
    #[wasm_bindgen(skip)]
    pub prv_key_data_id: Option<PrvKeyDataId>,
    pub ecdsa: bool,
}

impl Account {
    pub fn new_with_args(
        rpc_api: Arc<DynRpcApi>,
        name: &str,
        title: &str,
        account_kind: AccountKind,
        account_index: u32,
        prv_key_data_id: Option<PrvKeyDataId>,
        pub_key_data: PubKeyData,
        ecdsa: bool,
    ) -> Self {
        let channel = Channel::<Notification>::unbounded();
        let listener_id = rpc_api.register_new_listener(ChannelConnection::new(channel.sender));

        // rpc_api.register_new_listener();

        let stored = storage::Account::new(
            name.to_string(),
            title.to_string(),
            account_kind,
            account_index,
            false,
            pub_key_data,
            prv_key_data_id,
            ecdsa,
            1,
            0,
        );

        let inner = Inner { listener_id, stored };

        Account {
            utxos: UtxoSet::default(),
            balance: AtomicU64::new(0),
            // _generator: Arc::new(config.clone()),
            rpc: rpc_api.clone(),
            is_connected: AtomicBool::new(false),
            // -
            inner: Arc::new(Mutex::new(inner)),
            // -
            account_kind,
            account_index,
            prv_key_data_id,
            ecdsa: false,
        }
    }
    pub fn new_from_storage(rpc_api: Arc<DynRpcApi>, stored: &storage::Account) -> Self {
        let channel = Channel::<Notification>::unbounded();
        let listener_id = rpc_api.register_new_listener(ChannelConnection::new(channel.sender));

        // rpc_api.register_new_listener();

        let inner = Inner { listener_id, stored: stored.clone() };

        Account {
            utxos: UtxoSet::default(),
            balance: AtomicU64::new(0),
            // _generator: Arc::new(config.clone()),
            rpc: rpc_api.clone(),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind: stored.account_kind,
            account_index: stored.account_index,
            prv_key_data_id: stored.prv_key_data_id,
            ecdsa: stored.ecdsa,
        }
    }

    pub async fn subscribe(&self) {
        // TODO query account interface
        let addresses = vec![];
        let utxos_changed_scope = UtxosChangedScope { addresses };
        let id = self.inner.lock().unwrap().listener_id;
        let _ = self.rpc.start_notify(id, Scope::UtxosChanged(utxos_changed_scope)).await;
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
        self.inner.lock().unwrap().stored.name.clone()
    }

    pub fn get_ls_string(&self) -> String {
        let name = self.name();
        let balance = self.balance.load(std::sync::atomic::Ordering::SeqCst);
        format!("{balance} - {name}")
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub async fn scan_utxos(&self) -> Result<u64> {
        self.utxos.clear();

        let scan_depth: usize = 1024;
        let window_size: usize = 128;
        let mut balance = 0u64;
        let mut cursor = 0;
        while cursor < scan_depth {
            let first = cursor;
            let last = cursor + window_size;
            cursor = last;

            log_info!("first: {}, last: {}", first, last);

            // - TODO - populate address range from derivators/generators
            let _addresses = Vec::<Address>::new();

            let resp = self.rpc.get_utxos_by_addresses(_addresses.clone()).await?;

            let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();

            balance += refs.iter().map(|r| r.utxo.utxo_entry.amount).sum::<u64>();
            for r in refs.iter() {
                balance += r.as_ref().amount();
            }

            self.utxos.extend(&refs);
        }

        self.utxos.order(UtxoOrdering::AscendingAmount)?;

        Ok(balance)
    }

    pub async fn estimate(&self, _address: &Address, _amount_sompi: u64, _priority_fee_sompi: u64) -> Result<Estimate> {
        todo!()
        // Ok(())
    }

    pub async fn send(
        &self,
        _address: &Address,
        _amount_sompi: u64,
        _priority_fee_sompi: u64,
        _payment_secret: Option<Secret>,
    ) -> Result<()> {
        todo!()
        // Ok(())
    }

    pub async fn address(&self) -> Result<Address> {
        todo!()
    }

    #[allow(dead_code)]
    fn receive_generator(&self) -> Result<Arc<dyn AddressGeneratorTrait>> {
        todo!()
        // Ok(self.account()?.receive_wallet())
    }

    fn change_generator(&self) -> Result<Arc<dyn AddressGeneratorTrait>> {
        todo!()
        // Ok(self.account()?.change_wallet())
    }

    pub async fn new_receive_address(&self) -> Result<String> {
        todo!()
        // let address = self.receive_wallet()?.new_address().await?;
        // Ok(address.into())
    }

    pub async fn new_change_address(self: &Arc<Self>) -> Result<String> {
        let address = self.change_generator()?.new_address().await?;
        Ok(address.into())
    }

    pub async fn sign(&self) -> Result<()> {
        Ok(())
    }

    pub async fn sweep(&self) -> Result<()> {
        Ok(())
    }

    pub async fn create_unsigned_transaction(&self) -> Result<()> {
        Ok(())
    }
}

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(getter)]
    pub fn balance(&self) -> u64 {
        self.balance.load(std::sync::atomic::Ordering::SeqCst)
    }
}
