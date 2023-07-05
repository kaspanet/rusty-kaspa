#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::address::{build_derivate_paths, AddressManager};
use crate::imports::*;
use crate::result::Result;
use crate::runtime::scan::Scan;
use crate::runtime::{AtomicBalance, Balance, BalanceStrings, Wallet};
use crate::secret::Secret;
use crate::signer::sign_mutable_transaction;
use crate::storage::interface::AccessContext;
use crate::storage::{self, AccessContextT, PrvKeyData, PrvKeyDataId, PubKeyData};
use crate::tx::{LimitCalcStrategy, PaymentOutputs, VirtualTransaction};
use crate::utxo::{UtxoEntryReference, UtxoProcessor};
use crate::AddressDerivationManager;
use faster_hex::hex_string;
use futures::future::join_all;
use kaspa_bip32::{ChildNumber, PrivateKey};
use kaspa_notify::listener::ListenerId;
use serde::Serializer;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use workflow_core::abortable::Abortable;
use workflow_core::enums::u8_try_from;

use super::scan::{ScanExtent, DEFAULT_WINDOW_SIZE};
pub const DEFAULT_AMOUNT_PADDING: usize = 19;

#[derive(Default, Clone)]
pub struct Estimate {
    pub total_sompi: u64,
    pub fees_sompi: u64,
    pub utxos: usize,
    pub transactions: usize,
}

u8_try_from! {
    #[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Hash)]
    #[serde(rename_all = "lowercase")]
    #[wasm_bindgen]
    pub enum AccountKind {
        Legacy,
        #[default]
        Bip32,
        MultiSig,
    }
}

impl ToString for AccountKind {
    fn to_string(&self) -> String {
        match self {
            AccountKind::Legacy => "legacy".to_string(),
            AccountKind::Bip32 => "bip32".to_string(),
            AccountKind::MultiSig => "multisig".to_string(),
        }
    }
}

impl FromStr for AccountKind {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "legacy" => Ok(AccountKind::Legacy),
            "bip32" => Ok(AccountKind::Bip32),
            "multisig" => Ok(AccountKind::MultiSig),
            _ => Err(Error::InvalidAccountKind),
        }
    }
}

#[derive(Hash)]
struct AccountIdHashData {
    prv_key_data_id: PrvKeyDataId,
    ecdsa: bool,
    account_kind: AccountKind,
    account_index: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccountId(pub(crate) u64);

impl AccountId {
    pub(crate) fn new(prv_key_data_id: &PrvKeyDataId, ecdsa: bool, account_kind: &AccountKind, account_index: u64) -> AccountId {
        let mut hasher = DefaultHasher::new();
        AccountIdHashData { prv_key_data_id: *prv_key_data_id, ecdsa, account_kind: *account_kind, account_index }.hash(&mut hasher);
        AccountId(hasher.finish())
    }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("{}..{}", &hex[0..4], &hex[hex.len() - 4..])
    }
}

impl ToHex for AccountId {
    fn to_hex(&self) -> String {
        format!("{:x}", self.0)
    }
}

impl Serialize for AccountId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0.to_be_bytes()))
    }
}

impl<'de> Deserialize<'de> for AccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = <std::string::String as Deserialize>::deserialize(deserializer)?;
        let mut out = [0u8; 8];
        let mut input = [b'0'; 16];
        let start = input.len() - hex_str.len();
        input[start..].copy_from_slice(hex_str.as_bytes());
        faster_hex::hex_decode(&input, &mut out).map_err(serde::de::Error::custom)?;
        Ok(AccountId(u64::from_be_bytes(out)))
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex_string(&self.0.to_be_bytes()))
    }
}

pub struct Inner {
    pub listener_id: Option<ListenerId>,
    pub stored: storage::Account,
}

/// Wallet `Account` data structure. An account is typically a single
/// HD-key derivation (derived from a wallet or from an an external secret)
pub struct Account {
    pub id: AccountId,
    inner: Arc<Mutex<Inner>>,
    wallet: Arc<Wallet>,
    utxo_processor: Arc<UtxoProcessor>,
    balance: Mutex<Option<Balance>>,
    is_connected: AtomicBool,
    pub account_kind: AccountKind,
    pub account_index: u64,
    pub prv_key_data_id: PrvKeyDataId,
    pub ecdsa: bool,
    pub derivation: Arc<AddressDerivationManager>,
}

impl Account {
    pub async fn try_new_arc_with_args(
        wallet: &Arc<Wallet>,
        name: &str,
        title: &str,
        account_kind: AccountKind,
        account_index: u64,
        prv_key_data_id: PrvKeyDataId,
        pub_key_data: PubKeyData,
        ecdsa: bool,
    ) -> Result<Arc<Self>> {
        let minimum_signatures = pub_key_data.minimum_signatures.unwrap_or(1) as usize;
        let derivation =
            AddressDerivationManager::new(wallet, account_kind, &pub_key_data, ecdsa, minimum_signatures, None, None).await?;

        let stored = storage::Account::new(
            name.to_string(),
            title.to_string(),
            account_kind,
            account_index,
            false,
            pub_key_data.clone(),
            prv_key_data_id,
            ecdsa,
            pub_key_data.minimum_signatures.unwrap_or(1),
            pub_key_data.cosigner_index.unwrap_or(0),
        );

        let inner = Inner { listener_id: None, stored };
        let utxo_processor = Arc::new(UtxoProcessor::new(wallet.utxo_processor_core()));
        let account = Arc::new(Account {
            id: AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index),
            wallet: wallet.clone(),
            utxo_processor: utxo_processor.clone(),
            balance: Mutex::new(None), // Arc::new(AtomicU64::new(0)),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind,
            account_index,
            prv_key_data_id,
            ecdsa: false,
            derivation,
        });

        utxo_processor.bind_to_account(&account);

        Ok(account)
    }

    pub async fn try_new_arc_from_storage(wallet: &Arc<Wallet>, stored: &storage::Account) -> Result<Arc<Self>> {
        let minimum_signatures = stored.pub_key_data.minimum_signatures.unwrap_or(1) as usize;
        let derivation = AddressDerivationManager::new(
            wallet,
            stored.account_kind,
            &stored.pub_key_data,
            stored.ecdsa,
            minimum_signatures,
            None,
            None,
        )
        .await?;

        let inner = Inner { listener_id: None, stored: stored.clone() };
        let utxo_processor = Arc::new(UtxoProcessor::new(wallet.utxo_processor_core()));
        let account = Arc::new(Account {
            id: AccountId::new(&stored.prv_key_data_id, stored.ecdsa, &stored.account_kind, stored.account_index),
            wallet: wallet.clone(),
            utxo_processor: utxo_processor.clone(),
            balance: Mutex::new(None), //Arc::new(AtomicU64::new(0)),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind: stored.account_kind,
            account_index: stored.account_index,
            prv_key_data_id: stored.prv_key_data_id,
            ecdsa: stored.ecdsa,
            derivation,
        });

        utxo_processor.bind_to_account(&account);

        Ok(account)
    }

    pub fn id(&self) -> &AccountId {
        &self.id
    }

    pub fn utxo_processor(&self) -> &Arc<UtxoProcessor> {
        &self.utxo_processor
    }

    pub fn is_connected(&self) -> bool {
        self.is_connected.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().stored.name.clone()
    }

    pub fn name_or_id(&self) -> String {
        let name = self.inner.lock().unwrap().stored.name.clone();
        if name.is_empty() {
            self.id.short()
        } else {
            name
        }
    }

    pub fn balance(&self) -> Option<Balance> {
        self.balance.lock().unwrap().clone()
    }

    pub fn balance_as_strings(&self) -> Result<BalanceStrings> {
        Ok(BalanceStrings::from((&self.balance(), &self.wallet.network()?, Some(DEFAULT_AMOUNT_PADDING))))
    }

    pub fn get_ls_string(&self) -> Result<String> {
        let name = self.name();
        let balance = self.balance_as_strings()?;
        Ok(format!("{name} - {balance}"))
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub async fn scan_address_manager(self: &Arc<Self>, scan: Scan) -> Result<()> {
        let mut cursor = 0;

        // let mut balance = 0;
        let mut last_address_index = std::cmp::max(scan.address_manager.index()?, scan.window_size);

        'scan: loop {
            let first = cursor;
            let last = if cursor == 0 { last_address_index + 1 } else { cursor + scan.window_size };
            cursor = last;

            // log_info!("first: {}, last: {}", first, last);

            let addresses = scan.address_manager.get_range(first..last).await?;
            // log_info!("{}", format!("scanning addresses:\n{:#?}", addresses).replace('\n', "\r\n"));

            // log_info!("get_utxos_by_address start");
            let resp = self.wallet.rpc().get_utxos_by_addresses(addresses).await?;
            // log_info!("get_utxos_by_address done");
            let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
            // println!("{}", format!("addresses:{:#?}", address_str).replace('\n', "\r\n"));
            //println!("{}", format!("resp:{:#?}", resp.get(0).and_then(|a|a.address.clone())).replace('\n', "\r\n"));

            for utxo_ref in refs.iter() {
                if let Some(address) = utxo_ref.utxo.address.as_ref() {
                    if let Some(utxo_address_index) = scan.address_manager.inner().address_to_index_map.get(address) {
                        if last_address_index < *utxo_address_index {
                            last_address_index = *utxo_address_index;
                        }
                    } else {
                        panic!("Account::scan_address_manager() has received an unknown address: `{address}`");
                    }
                }
            }
            // log_info!("scan updating balance");

            let balance: Balance = refs.iter().fold(Balance::default(), |mut balance, r| {
                let entry_balance = r.as_ref().balance(scan.current_daa_score);
                balance.mature += entry_balance.mature;
                balance.pending += entry_balance.pending;
                balance
            });

            self.utxo_processor().extend(refs, scan.current_daa_score).await?;

            if !balance.is_empty() {
                scan.balance.add(balance);
            } else {
                match &scan.extent {
                    ScanExtent::EmptyWindow => {
                        if cursor > last_address_index + scan.window_size {
                            break 'scan;
                        }
                    }
                    ScanExtent::Depth(depth) => {
                        if &cursor > depth {
                            break 'scan;
                        }
                    }
                }
            }

            // yield_executor().await;
        }

        // log_info!("scan - secondary pass");

        let mut cursor = 0;
        while cursor <= last_address_index {
            let first = cursor;
            let last = std::cmp::min(cursor + scan.window_size, last_address_index + 1);
            cursor = last;
            let addresses = scan.address_manager.get_range(first..last).await?;
            // log_info!("starting scan address registration...");
            self.utxo_processor().register_addresses(addresses).await?;
        }

        scan.address_manager.set_index(last_address_index)?;

        Ok(())
    }

    pub async fn scan_utxos(self: &Arc<Self>, window_size: Option<u32>, extent: Option<u32>) -> Result<()> {
        self.utxo_processor().clear().await?;

        let current_daa_score = self.wallet.current_daa_score();
        let balance = Arc::new(AtomicBalance::default());

        let window_size = window_size.unwrap_or(DEFAULT_WINDOW_SIZE);
        let extent = match extent {
            Some(depth) => ScanExtent::Depth(depth),
            None => ScanExtent::EmptyWindow,
        };

        let scans = vec![
            self.scan_address_manager(Scan::new_with_args(
                self.derivation.receive_address_manager(),
                window_size,
                extent,
                &balance,
                current_daa_score,
            )),
            self.scan_address_manager(Scan::new_with_args(
                self.derivation.change_address_manager(),
                window_size,
                extent,
                &balance,
                current_daa_score,
            )),
        ];

        join_all(scans).await.into_iter().collect::<Result<Vec<_>>>()?;

        self.utxo_processor().update_balance().await?;

        Ok(())
    }

    pub async fn estimate(&self, _address: &Address, _amount_sompi: u64, _priority_fee_sompi: u64) -> Result<Estimate> {
        todo!()
        // Ok(())
    }

    pub async fn send(
        &self,
        outputs: &PaymentOutputs,
        priority_fee_sompi: Option<u64>,
        _include_fees_in_amount: bool,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
    ) -> Result<Vec<kaspa_hashes::Hash>> {
        let mut ctx = self.utxo_processor().create_selection_context();

        let change_address = self.change_address().await?;
        let payload = vec![];
        let sig_op_count = self.inner().stored.pub_key_data.keys.len() as u8;
        let minimum_signatures = self.inner().stored.minimum_signatures;
        let vt = VirtualTransaction::new(
            sig_op_count,
            minimum_signatures,
            &mut ctx,
            outputs,
            &change_address,
            priority_fee_sompi,
            payload,
            LimitCalcStrategy::inputs(80),
            abortable,
        )
        .await?;

        let addresses = ctx.addresses();
        let indexes = self.derivation.addresses_indexes(&addresses)?;
        let receive_indexes = indexes.0;
        let change_indexes = indexes.1;

        let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
        let keydata = self
            .wallet
            .store()
            .as_prv_key_data_store()?
            .load_key_data(&access_ctx, &self.prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(self.prv_key_data_id.to_hex()))?;

        let private_keys = self.create_private_keys(keydata, payment_secret, receive_indexes, change_indexes)?;
        let private_keys = &private_keys.iter().map(|k| k.to_bytes()).collect::<Vec<_>>();
        let mut tx_ids = vec![];
        for mtx in vt.transactions().clone() {
            let mtx = sign_mutable_transaction(mtx, private_keys, true)?;
            let id = self.wallet.rpc().submit_transaction(mtx.try_into()?, false).await?;
            //println!("id: {id}\r\n");
            tx_ids.push(id);
        }

        ctx.commit()?;

        Ok(tx_ids)
    }

    fn create_private_keys(
        &self,
        keydata: PrvKeyData,
        payment_secret: Option<Secret>,
        receive_indexes: Vec<u32>,
        change_indexes: Vec<u32>,
    ) -> Result<Vec<secp256k1::SecretKey>> {
        let payload = keydata.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;

        let cosigner_index = self.inner().stored.pub_key_data.cosigner_index.unwrap_or(0);
        let paths = build_derivate_paths(self.account_kind, self.account_index, cosigner_index)?;
        let receive_xkey = xkey.clone().derive_path(paths.0)?;
        let change_xkey = xkey.derive_path(paths.1)?;

        let mut private_keys = vec![];
        for index in receive_indexes {
            private_keys.push(*receive_xkey.derive_child(ChildNumber::new(index, false)?)?.private_key());
        }
        for index in change_indexes {
            private_keys.push(*change_xkey.derive_child(ChildNumber::new(index, false)?)?.private_key());
        }

        Ok(private_keys)
    }

    pub async fn receive_address(&self) -> Result<Address> {
        self.receive_address_manager()?.current_address().await
    }

    pub async fn change_address(&self) -> Result<Address> {
        self.change_address_manager()?.current_address().await
    }

    pub fn receive_address_manager(&self) -> Result<Arc<AddressManager>> {
        Ok(self.derivation.receive_address_manager())
    }

    pub fn change_address_manager(&self) -> Result<Arc<AddressManager>> {
        Ok(self.derivation.change_address_manager())
    }

    pub async fn new_receive_address(self: &Arc<Self>) -> Result<String> {
        let address = self.receive_address_manager()?.new_address().await?;
        self.utxo_processor().register_addresses(vec![address.clone()]).await?;
        Ok(address.into())
    }

    pub async fn new_change_address(self: &Arc<Self>) -> Result<String> {
        let address = self.change_address_manager()?.new_address().await?;
        self.utxo_processor().register_addresses(vec![address.clone()]).await?;
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

    // -

    /// Start Account service task
    pub async fn start(self: &Arc<Self>) -> Result<()> {
        // self.start_task().await
        if self.wallet.is_connected() {
            self.connect().await?;
        }

        Ok(())
    }

    /// Stop Account service task
    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        // self.stop_task().await
        // self.unsubscribe_utxos_changed(vec![]).await?;
        self.utxo_processor().clear().await?;
        self.disconnect().await?;
        Ok(())
    }

    /// handle connection event
    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        self.wallet.active_accounts().insert(self.clone());
        // self.is_connected.store(true, Ordering::SeqCst);
        // self.register_notification_listener().await?;
        self.scan_utxos(None, None).await?;
        Ok(())
    }

    /// handle disconnection event
    pub async fn disconnect(&self) -> Result<()> {
        self.wallet.active_accounts().remove(&self.id);
        // self.is_connected.store(false, Ordering::SeqCst);
        // self.unregister_notification_listener().await?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct AccountMap(Arc<Mutex<HashMap<AccountId, Arc<Account>>>>);

impl AccountMap {
    pub fn inner(&self) -> MutexGuard<HashMap<AccountId, Arc<Account>>> {
        self.0.lock().unwrap()
    }

    pub fn clear(&self) {
        self.inner().clear();
    }

    pub fn get(&self, account_id: &AccountId) -> Option<Arc<Account>> {
        self.inner().get(account_id).cloned()
    }

    pub fn extend(&self, accounts: Vec<Arc<Account>>) {
        let mut map = self.inner();
        let accounts = accounts.into_iter().map(|a| (a.id, a)); //.collect::<Vec<_>>();
        map.extend(accounts);
    }

    pub fn insert(&self, account: Arc<Account>) {
        self.inner().insert(account.id, account);
    }

    pub fn remove(&self, id: &AccountId) {
        self.inner().remove(id);
    }

    pub fn cloned_flat_list(&self) -> Vec<Arc<Account>> {
        self.inner().values().cloned().collect()
    }
}
