#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::address::{build_derivate_paths, AddressManager};
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AtomicBalance, Balance, BalanceStrings, Wallet};
use crate::secret::Secret;
use crate::signer::sign_mutable_transaction;
use crate::storage::interface::AccessContext;
use crate::storage::{self, AccessContextT, PrvKeyData, PrvKeyDataId, PubKeyData};
use crate::tx::{
    Generator, GeneratorSettings, GeneratorSummary, LimitCalcStrategy, PaymentDestination, PaymentOutputs, PendingTransaction,
    VirtualTransactionV1,
};
use crate::utxo::UtxoContext;
// use crate::utxo::UtxoStream;
use crate::AddressDerivationManager;
use faster_hex::hex_string;
use futures::future::join_all;
// use futures::pin_mut;
use kaspa_bip32::{ChildNumber, PrivateKey};
use kaspa_notify::listener::ListenerId;
use separator::Separatable;
use serde::Serializer;
use std::hash::Hash;
use std::str::FromStr;
use workflow_core::abortable::Abortable;
use workflow_core::enums::u8_try_from;

pub const DEFAULT_AMOUNT_PADDING: usize = 19;

pub type GenerationNotifier = Arc<dyn Fn(&PendingTransaction) + Send + Sync>;

// #[derive(Default, Clone, Debug)]
// pub struct Estimate {
//     pub final_amount_including_fees: u64,
//     pub aggregate_fees: u64,
//     pub aggregate_utxos: usize,
//     pub transactions: usize,
// }

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

// #[derive(Hash)]
#[derive(BorshSerialize)]
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
        let data = AccountIdHashData { prv_key_data_id: *prv_key_data_id, ecdsa, account_kind: *account_kind, account_index };
        AccountId(xxh3_64(data.try_to_vec().unwrap().as_slice()))
    }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        // format!("{}..{}", &hex[0..4], &hex[hex.len() - 4..])
        format!("[{}]", &hex[0..4])
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
    utxo_context: UtxoContext,
    // balance: Mutex<Option<Balance>>,
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
        let utxo_context = UtxoContext::new(wallet.utxo_processor());
        let account = Arc::new(Account {
            id: AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index),
            wallet: wallet.clone(),
            utxo_context: utxo_context.clone(),
            // balance: Mutex::new(None), // Arc::new(AtomicU64::new(0)),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind,
            account_index,
            prv_key_data_id,
            ecdsa: false,
            derivation,
        });

        utxo_context.bind_to_account(&account);

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
        let utxo_context = UtxoContext::new(wallet.utxo_processor());
        let account = Arc::new(Account {
            id: AccountId::new(&stored.prv_key_data_id, stored.ecdsa, &stored.account_kind, stored.account_index),
            wallet: wallet.clone(),
            utxo_context: utxo_context.clone(),
            // balance: Mutex::new(None), //Arc::new(AtomicU64::new(0)),
            is_connected: AtomicBool::new(false),
            inner: Arc::new(Mutex::new(inner)),
            account_kind: stored.account_kind,
            account_index: stored.account_index,
            prv_key_data_id: stored.prv_key_data_id,
            ecdsa: stored.ecdsa,
            derivation,
        });

        utxo_context.bind_to_account(&account);

        Ok(account)
    }

    pub fn id(&self) -> &AccountId {
        &self.id
    }

    pub fn utxo_context(&self) -> &UtxoContext {
        &self.utxo_context
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

    pub async fn rename(&self, secret: Secret, name: &str) -> Result<()> {
        let stored = {
            let inner = self.inner();
            let mut stored = inner.stored.clone();
            stored.name = name.to_string();
            stored
        };

        self.wallet.store().as_account_store()?.store(&[&stored]).await?;

        let ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(secret));
        self.wallet.store().commit(&ctx).await?;
        Ok(())
    }

    pub fn balance(&self) -> Option<Balance> {
        self.utxo_context().balance()
    }

    pub fn balance_as_strings(&self, padding: Option<usize>) -> Result<BalanceStrings> {
        Ok(BalanceStrings::from((&self.balance(), &self.wallet.network_id()?.into(), padding)))
    }

    pub fn get_list_string(&self) -> Result<String> {
        let name = style(self.name_or_id()).blue();
        let balance = self.balance_as_strings(None)?;
        let mature_utxo_size = self.utxo_context.mature_utxo_size();
        let pending_utxo_size = self.utxo_context.pending_utxo_size();
        let info = match (mature_utxo_size, pending_utxo_size) {
            (0, 0) => "".to_string(),
            (_, 0) => {
                format!("{} UTXOs", mature_utxo_size.separated_string())
            }
            (0, _) => {
                format!("{} UTXOs pending", pending_utxo_size.separated_string())
            }
            _ => {
                format!("{} UTXOs, {} UTXOs pending", mature_utxo_size.separated_string(), pending_utxo_size.separated_string())
            }
        };
        Ok(format!("{name}: {balance}   {}", style(info).dim()))
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub async fn scan(self: &Arc<Self>, window_size: Option<usize>, extent: Option<u32>) -> Result<()> {
        self.utxo_context().clear().await?;

        let current_daa_score = self.wallet.current_daa_score().ok_or(Error::NotConnected)?;
        let balance = Arc::new(AtomicBalance::default());

        let extent = match extent {
            Some(depth) => ScanExtent::Depth(depth),
            None => ScanExtent::EmptyWindow,
        };

        let scans = vec![
            Scan::new_with_args(self.derivation.receive_address_manager(), window_size, extent, &balance, current_daa_score),
            Scan::new_with_args(self.derivation.change_address_manager(), window_size, extent, &balance, current_daa_score), //.scan(self.utxo_context()),
        ];

        let futures = scans.iter().map(|scan| scan.scan(self.utxo_context())).collect::<Vec<_>>();

        join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;

        self.utxo_context().update_balance().await?;

        Ok(())
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
        self.utxo_context().register_addresses(&[address.clone()]).await?;
        Ok(address.into())
    }

    pub async fn new_change_address(self: &Arc<Self>) -> Result<String> {
        let address = self.change_address_manager()?.new_address().await?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;
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
        // if self.wallet.is_connected() {
        self.connect().await?;
        // }

        Ok(())
    }

    /// Stop Account service task
    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.utxo_context().clear().await?;
        self.disconnect().await?;
        Ok(())
    }

    /// handle connection event
    pub async fn connect(self: &Arc<Self>) -> Result<()> {
        if self.wallet.is_connected() && self.wallet.active_accounts().insert(self.clone()).is_none() {
            self.scan(None, None).await?;
        }
        Ok(())
    }

    /// handle disconnection event
    pub async fn disconnect(&self) -> Result<()> {
        self.wallet.active_accounts().remove(&self.id);
        // self.is_connected.store(false, Ordering::SeqCst);
        // self.unregister_notification_listener().await?;
        Ok(())
    }

    pub async fn send_v1(
        &self,
        outputs: &PaymentOutputs,
        priority_fee_sompi: Option<u64>,
        _include_fees_in_amount: bool,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
    ) -> Result<Vec<kaspa_hashes::Hash>> {
        let mut ctx = self.utxo_context().create_selection_context();

        let change_address = self.change_address().await?;
        let payload = vec![];
        let sig_op_count = self.inner().stored.pub_key_data.keys.len() as u8;
        let minimum_signatures = self.inner().stored.minimum_signatures;
        let vt = VirtualTransactionV1::try_new(
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

    pub async fn send(
        &self,
        destination: PaymentDestination,
        priority_fee_sompi: Option<u64>,
        include_fees_in_amount: bool,
        payload: Option<Vec<u8>>,
        _wallet_secret: Secret,
        _payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<Vec<kaspa_hashes::Hash>> {
        // todo!()

        let settings = GeneratorSettings::try_new_with_account(
            self,
            destination,
            priority_fee_sompi,
            include_fees_in_amount,
            payload,
            // wallet_secret,
            // payment_secret,
            // abortable,
        )
        .await?;
        // .generator(abortable);

        let generator = Generator::new(settings, abortable);
        // pin_mut!(generator);

        // ---
        // let mut stream = generator.stream();
        // while let Some(transaction) = stream.try_next().await? {
        // let mut iterator = generator.iter();
        // while let Some(transaction) = iterator.next() {
        // ---

        for transaction in generator.iter() {
            let transaction = transaction?;
            // -- WIP
            // -- WIP
            // -- WIP
            // - TODO - sign & submit
            // -- WIP
            // -- WIP
            // -- WIP

            // transaction.submit(self.wallet.rpc()).await?;

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }

            transaction.log().await?;

            yield_executor().await;
        }

        Ok(vec![])
    }

    pub async fn estimate(
        &self,
        destination: PaymentDestination,
        priority_fee_sompi: Option<u64>,
        include_fees_in_amount: bool,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<GeneratorSummary> {
        // todo!()

        let settings =
            GeneratorSettings::try_new_with_account(self, destination, priority_fee_sompi, include_fees_in_amount, payload).await?;

        let generator = Generator::new(settings, abortable);

        // let mut iterator = generator.iter();
        let mut stream = generator.stream();
        while let Some(_transaction) = stream.try_next().await? {
            _transaction.log().await?;

            yield_executor().await;
        }

        // for _ in generator.iter() {
        //     yield_executor().await;
        // }

        // let aggregate_fees = generator.aggregate_fees();
        // let aggregate_utxos = generator.aggregate_utxos();
        // let estimate = Estimate {
        //     final_amount_including_fees: 0, // TODO
        //     aggregate_fees,
        //     aggregate_utxos,
        //     transactions,
        // };

        Ok(generator.summary())
    }
}
