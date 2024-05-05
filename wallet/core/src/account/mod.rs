//!
//! Generic wallet [`Account`] trait implementation used
//! by different types of accounts.
//!

pub mod descriptor;
pub mod kind;
pub mod variants;
pub use kind::*;
pub use variants::*;

use crate::derivation::build_derivate_paths;
use crate::derivation::AddressDerivationManagerTrait;
use crate::imports::*;
use crate::storage::account::AccountSettings;
use crate::storage::AccountMetadata;
use crate::storage::{PrvKeyData, PrvKeyDataId};
use crate::tx::PaymentOutput;
use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, PaymentDestination, PendingTransaction, Signer};
use crate::utxo::balance::{AtomicBalance, BalanceStrings};
use crate::utxo::UtxoContextBinding;
use kaspa_bip32::{ChildNumber, ExtendedPrivateKey, PrivateKey};
use kaspa_consensus_client::UtxoEntryReference;
use kaspa_wallet_keys::derivation::gen0::WalletDerivationManagerV0;
use workflow_core::abortable::Abortable;

/// Notification callback type used by [`Account::sweep`] and [`Account::send`].
/// Allows tracking in-flight transactions during transaction generation.
pub type GenerationNotifier = Arc<dyn Fn(&PendingTransaction) + Send + Sync>;
/// Scan notification callback type used by [`DerivationCapableAccount::derivation_scan`].
/// Provides derivation discovery scan progress information.
pub type ScanNotifier = Arc<dyn Fn(usize, usize, u64, Option<TransactionId>) + Send + Sync>;

/// General-purpose wrapper around [`AccountSettings`] (managed by [`Inner`]).
pub struct Context {
    pub settings: AccountSettings,
}

impl Context {
    pub fn new(settings: AccountSettings) -> Self {
        Self { settings }
    }

    pub fn settings(&self) -> &AccountSettings {
        &self.settings
    }
}

/// Account `Inner` struct used by most account types.
pub struct Inner {
    context: Mutex<Context>,
    id: AccountId,
    storage_key: AccountStorageKey,
    wallet: Arc<Wallet>,
    utxo_context: UtxoContext,
}

impl Inner {
    pub fn new(wallet: &Arc<Wallet>, id: AccountId, storage_key: AccountStorageKey, settings: AccountSettings) -> Self {
        let utxo_context = UtxoContext::new(wallet.utxo_processor(), UtxoContextBinding::AccountId(id));

        let context = Context { settings };
        Inner { context: Mutex::new(context), id, storage_key, wallet: wallet.clone(), utxo_context: utxo_context.clone() }
    }

    pub fn from_storage(wallet: &Arc<Wallet>, storage: &AccountStorage) -> Self {
        Self::new(wallet, storage.id, storage.storage_key, storage.settings.clone())
    }

    pub fn context(&self) -> MutexGuard<Context> {
        self.context.lock().unwrap()
    }

    pub fn store(&self) -> &Arc<dyn Interface> {
        self.wallet.store()
    }
}

/// Generic wallet [`Account`] trait implementation used
/// by different types of accounts.
#[async_trait]
pub trait Account: AnySync + Send + Sync + 'static {
    fn inner(&self) -> &Arc<Inner>;

    fn context(&self) -> MutexGuard<Context> {
        self.inner().context.lock().unwrap()
    }

    fn id(&self) -> &AccountId {
        &self.inner().id
    }

    fn storage_key(&self) -> &AccountStorageKey {
        &self.inner().storage_key
    }

    fn account_kind(&self) -> AccountKind;

    fn wallet(&self) -> &Arc<Wallet> {
        &self.inner().wallet
    }

    fn utxo_context(&self) -> &UtxoContext {
        &self.inner().utxo_context
    }

    fn balance(&self) -> Option<Balance> {
        self.utxo_context().balance()
    }

    fn balance_as_strings(&self, padding: Option<usize>) -> Result<BalanceStrings> {
        Ok(BalanceStrings::from((self.balance().as_ref(), &self.wallet().network_id()?.into(), padding)))
    }

    fn name(&self) -> Option<String> {
        self.context().settings.name.clone()
    }

    fn name_or_id(&self) -> String {
        if let Some(name) = self.name() {
            if name.is_empty() {
                self.id().short()
            } else {
                name
            }
        } else {
            self.id().short()
        }
    }

    fn name_with_id(&self) -> String {
        if let Some(name) = self.name() {
            if name.is_empty() {
                self.id().short()
            } else {
                format!("{name} {}", self.id().short())
            }
        } else {
            self.id().short()
        }
    }

    async fn rename(&self, wallet_secret: &Secret, name: Option<&str>) -> Result<()> {
        {
            let mut context = self.context();
            context.settings.name = name.map(String::from);
        }

        let account = self.to_storage()?;
        self.wallet().store().as_account_store()?.store_single(&account, None).await?;

        self.wallet().store().commit(wallet_secret).await?;
        Ok(())
    }

    fn get_list_string(&self) -> Result<String> {
        let name = style(self.name_with_id()).blue();
        let balance = self.balance_as_strings(None)?;
        let mature_utxo_size = self.utxo_context().mature_utxo_size();
        let pending_utxo_size = self.utxo_context().pending_utxo_size();
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

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        // TODO - change to AssocPrvKeyDataIds
        Err(Error::ResidentAccount)
    }

    async fn prv_key_data(&self, wallet_secret: Secret) -> Result<PrvKeyData> {
        let prv_key_data_id = self.prv_key_data_id()?;

        let keydata = self
            .wallet()
            .store()
            .as_prv_key_data_store()?
            .load_key_data(&wallet_secret, prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(*prv_key_data_id))?;
        Ok(keydata)
    }

    fn to_storage(&self) -> Result<AccountStorage>;
    fn metadata(&self) -> Result<Option<AccountMetadata>>;
    fn descriptor(&self) -> Result<descriptor::AccountDescriptor>;

    async fn scan(self: Arc<Self>, window_size: Option<usize>, extent: Option<u32>) -> Result<()> {
        self.utxo_context().clear().await?;

        let current_daa_score = self.wallet().current_daa_score().ok_or(Error::NotConnected)?;
        let balance = Arc::new(AtomicBalance::default());

        match self.clone().as_derivation_capable() {
            Ok(account) => {
                let derivation = account.derivation();

                let extent = match extent {
                    Some(depth) => ScanExtent::Depth(depth),
                    None => ScanExtent::EmptyWindow,
                };

                let scans = [
                    Scan::new_with_address_manager(
                        derivation.receive_address_manager(),
                        &balance,
                        current_daa_score,
                        window_size,
                        Some(extent),
                    ),
                    Scan::new_with_address_manager(
                        derivation.change_address_manager(),
                        &balance,
                        current_daa_score,
                        window_size,
                        Some(extent),
                    ),
                ];

                let futures = scans.iter().map(|scan| scan.scan(self.utxo_context())).collect::<Vec<_>>();

                join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
            }
            Err(_) => {
                let mut address_set = HashSet::<Address>::new();
                address_set.insert(self.receive_address()?);
                address_set.insert(self.change_address()?);

                let scan = Scan::new_with_address_set(address_set, &balance, current_daa_score);
                scan.scan(self.utxo_context()).await?;
            }
        }

        self.utxo_context().update_balance().await?;

        Ok(())
    }

    fn sig_op_count(&self) -> u8;

    fn minimum_signatures(&self) -> u16;

    fn receive_address(&self) -> Result<Address>;

    fn change_address(&self) -> Result<Address>;

    /// Start Account service task
    async fn start(self: Arc<Self>) -> Result<()> {
        self.connect().await?;
        Ok(())
    }

    /// Stop Account service task
    async fn stop(self: Arc<Self>) -> Result<()> {
        self.utxo_context().clear().await?;
        self.disconnect().await?;
        Ok(())
    }

    /// handle connection event
    async fn connect(self: Arc<Self>) -> Result<()> {
        let vacated = self.wallet().active_accounts().insert(self.clone().as_dyn_arc());
        if vacated.is_none() && self.wallet().is_connected() {
            self.scan(None, None).await?;
        }
        Ok(())
    }

    /// handle disconnection event
    async fn disconnect(&self) -> Result<()> {
        self.wallet().active_accounts().remove(self.id());
        Ok(())
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account>;

    /// Aggregate all account UTXOs into the change address.
    /// Also known as "compounding".
    async fn sweep(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));
        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), PaymentDestination::Change, Fees::None, None)?;
        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    /// Send funds to a [`PaymentDestination`] comprised of one or multiple [`PaymentOutputs`](crate::tx::PaymentOutputs)
    /// or [`PaymentDestination::Change`] variant that will forward funds to the change address.
    async fn send(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let settings = GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, priority_fee_sompi, payload)?;

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    /// Execute a transfer to another wallet account.
    async fn transfer(
        self: Arc<Self>,
        destination_account_id: AccountId,
        transfer_amount_sompi: u64,
        priority_fee_sompi: Fees,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<kaspa_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let destination_account = self
            .wallet()
            .get_account_by_id(&destination_account_id)
            .await?
            .ok_or_else(|| Error::AccountNotFound(destination_account_id))?;

        let destination_address = destination_account.receive_address()?;
        let final_transaction_destination = PaymentDestination::from(PaymentOutput::new(destination_address, transfer_amount_sompi));
        let final_transaction_payload = None;

        let settings = GeneratorSettings::try_new_with_account(
            self.clone().as_dyn_arc(),
            final_transaction_destination,
            priority_fee_sompi,
            final_transaction_payload,
        )?
        .utxo_context_transfer(destination_account.utxo_context());

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn estimate(
        self: Arc<Self>,
        destination: PaymentDestination,
        priority_fee_sompi: Fees,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<GeneratorSummary> {
        let settings = GeneratorSettings::try_new_with_account(self.as_dyn_arc(), destination, priority_fee_sompi, payload)?;

        let generator = Generator::try_new(settings, None, Some(abortable))?;

        let mut stream = generator.stream();
        while let Some(_transaction) = stream.try_next().await? {
            yield_executor().await;
        }

        Ok(generator.summary())
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountAddressDerivationCaps)
    }

    fn as_legacy_account(self: Arc<Self>) -> Result<Arc<dyn AsLegacyAccount>> {
        Err(Error::InvalidAccountKind)
    }
}

downcast_sync!(dyn Account);

/// Account trait used by legacy account types (BIP32 account types with the `'972` derivation path).
#[async_trait]
pub trait AsLegacyAccount: Account {
    async fn create_private_context(
        &self,
        _wallet_secret: &Secret,
        _payment_secret: Option<&Secret>,
        _index: Option<u32>,
    ) -> Result<()>;

    async fn clear_private_context(&self) -> Result<()>;
}

/// Account trait used by derivation capable account types (BIP32, MultiSig, etc.)
#[async_trait]
pub trait DerivationCapableAccount: Account {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait>;

    fn account_index(&self) -> u64;

    async fn derivation_scan(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        start: usize,
        extent: usize,
        window: usize,
        sweep: bool,
        abortable: &Abortable,
        notifier: Option<ScanNotifier>,
    ) -> Result<()> {
        if let Ok(legacy_account) = self.clone().as_legacy_account() {
            legacy_account.create_private_context(&wallet_secret, payment_secret.as_ref(), None).await?;
        }

        let derivation = self.derivation();

        let prv_key_data = self.prv_key_data(wallet_secret).await?;
        let payload = prv_key_data.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;

        let receive_address_manager = derivation.receive_address_manager();
        let change_address_manager = derivation.change_address_manager();

        let change_address_index = change_address_manager.index();
        let change_address_keypair =
            derivation.get_range_with_keys(true, change_address_index..change_address_index + 1, false, &xkey).await?;

        let rpc = self.wallet().rpc_api();
        let notifier = notifier.as_ref();

        let mut index: usize = start;
        let mut last_notification = 0;
        let mut aggregate_balance = 0;
        let mut aggregate_utxo_count = 0;

        let change_address = change_address_keypair[0].0.clone();

        while index < extent && !abortable.is_aborted() {
            let first = index as u32;
            let last = (index + window) as u32;
            index = last as usize;

            let (mut keys, addresses) = if sweep {
                let mut keypairs = derivation.get_range_with_keys(false, first..last, false, &xkey).await?;
                let change_keypairs = derivation.get_range_with_keys(true, first..last, false, &xkey).await?;
                keypairs.extend(change_keypairs);
                let mut keys = vec![];
                let addresses = keypairs
                    .iter()
                    .map(|(address, key)| {
                        keys.push(key.to_bytes());
                        address.clone()
                    })
                    .collect::<Vec<_>>();
                keys.push(change_address_keypair[0].1.to_bytes());
                (keys, addresses)
            } else {
                let mut addresses = receive_address_manager.get_range_with_args(first..last, false)?;
                let change_addresses = change_address_manager.get_range_with_args(first..last, false)?;
                addresses.extend(change_addresses);
                (vec![], addresses)
            };

            let utxos = rpc.get_utxos_by_addresses(addresses.clone()).await?;
            let balance = utxos.iter().map(|utxo| utxo.utxo_entry.amount).sum::<u64>();
            aggregate_utxo_count += utxos.len();

            if balance > 0 {
                aggregate_balance += balance;

                if sweep {
                    let utxos = utxos.into_iter().map(UtxoEntryReference::from).collect::<Vec<_>>();

                    let settings = GeneratorSettings::try_new_with_iterator(
                        self.wallet().network_id()?,
                        Box::new(utxos.into_iter()),
                        change_address.clone(),
                        1,
                        1,
                        PaymentDestination::Change,
                        Fees::None,
                        None,
                        None,
                    )?;

                    let generator = Generator::try_new(settings, None, Some(abortable))?;

                    let mut stream = generator.stream();
                    while let Some(transaction) = stream.try_next().await? {
                        transaction.try_sign_with_keys(&keys)?;
                        let id = transaction.try_submit(&rpc).await?;
                        if let Some(notifier) = notifier {
                            notifier(index, aggregate_utxo_count, balance, Some(id));
                        }
                        yield_executor().await;
                    }
                } else {
                    if let Some(notifier) = notifier {
                        notifier(index, aggregate_utxo_count, aggregate_balance, None);
                    }
                    yield_executor().await;
                }
            }

            if index > last_notification + 1_000 {
                last_notification = index;
                if let Some(notifier) = notifier {
                    notifier(index, aggregate_utxo_count, aggregate_balance, None);
                }
                yield_executor().await;
            }

            keys.zeroize();
        }

        if index > last_notification {
            if let Some(notifier) = notifier {
                notifier(index, aggregate_utxo_count, aggregate_balance, None);
            }
        }

        if let Ok(legacy_account) = self.as_legacy_account() {
            legacy_account.clear_private_context().await?;
        }

        Ok(())
    }

    async fn new_receive_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().receive_address_manager().new_address()?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;

        let metadata = self.metadata()?.expect("derivation accounts must provide metadata");
        let store = self.wallet().store().as_account_store()?;
        store.update_metadata(vec![metadata]).await?;

        self.wallet().notify(Events::AccountUpdate { account_descriptor: self.descriptor()? }).await?;

        Ok(address)
    }

    async fn new_change_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().change_address_manager().new_address()?;
        self.utxo_context().register_addresses(&[address.clone()]).await?;

        let metadata = self.metadata()?.expect("derivation accounts must provide metadata");
        let store = self.wallet().store().as_account_store()?;
        store.update_metadata(vec![metadata]).await?;

        self.wallet().notify(Events::AccountUpdate { account_descriptor: self.descriptor()? }).await?;

        Ok(address)
    }

    fn cosigner_index(&self) -> u32 {
        0
    }

    fn create_private_keys<'l>(
        &self,
        key_data: &PrvKeyData,
        payment_secret: &Option<Secret>,
        receive: &[(&'l Address, u32)],
        change: &[(&'l Address, u32)],
    ) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
        let payload = key_data.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;
        create_private_keys(&self.account_kind(), self.cosigner_index(), self.account_index(), &xkey, receive, change)
    }
}

downcast_sync!(dyn DerivationCapableAccount);

pub(crate) fn create_private_keys<'l>(
    account_kind: &AccountKind,
    cosigner_index: u32,
    account_index: u64,
    xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    receive: &[(&'l Address, u32)],
    change: &[(&'l Address, u32)],
) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
    let paths = build_derivate_paths(account_kind, account_index, cosigner_index)?;
    let mut private_keys = vec![];
    if matches!(account_kind.as_ref(), LEGACY_ACCOUNT_KIND) {
        let (private_key, attrs) = WalletDerivationManagerV0::derive_key_by_path(xkey, paths.0)?;
        for (address, index) in receive.iter() {
            let (private_key, _) =
                WalletDerivationManagerV0::derive_private_key(&private_key, &attrs, ChildNumber::new(*index, true)?)?;
            private_keys.push((*address, private_key));
        }
        let (private_key, attrs) = WalletDerivationManagerV0::derive_key_by_path(xkey, paths.1)?;
        for (address, index) in change.iter() {
            let (private_key, _) =
                WalletDerivationManagerV0::derive_private_key(&private_key, &attrs, ChildNumber::new(*index, true)?)?;
            private_keys.push((*address, private_key));
        }
    } else {
        let receive_xkey = xkey.clone().derive_path(&paths.0)?;
        let change_xkey = xkey.clone().derive_path(&paths.1)?;

        for (address, index) in receive.iter() {
            private_keys.push((*address, *receive_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
        }
        for (address, index) in change.iter() {
            private_keys.push((*address, *change_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
        }
    }

    Ok(private_keys)
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::create_private_keys;
    use super::ExtendedPrivateKey;
    use crate::imports::LEGACY_ACCOUNT_KIND;
    use kaspa_addresses::Address;
    use kaspa_addresses::Prefix;
    use kaspa_bip32::secp256k1::SecretKey;
    use kaspa_bip32::PrivateKey;
    use kaspa_bip32::SecretKeyExt;
    use kaspa_wallet_keys::derivation::gen0::PubkeyDerivationManagerV0;
    use std::str::FromStr;

    fn gen0_receive_addresses() -> Vec<&'static str> {
        vec![
            "kaspatest:qqnapngv3zxp305qf06w6hpzmyxtx2r99jjhs04lu980xdyd2ulwwmx9evrfz",
            "kaspatest:qqfwmv2jm7dsuju9wz27ptdm4e28qh6evfsm66uf2vf4fxmpxfqgym4m2fcyp",
            "kaspatest:qpcerqk4ltxtyprv9096wrlzjx5mnrlw4fqce6hnl3axy7tkvyjxypjc5dyqs",
            "kaspatest:qr9m4h44ghmyz4wagktx8kgmh9zj8h8q0f6tc87wuad5xvzkdlwd6uu9plg2c",
            "kaspatest:qrkxylqkyjtkjr5zs4z5wjmhmj756e84pa05amcw3zn8wdqjvn4tcc2gcqhrw",
            "kaspatest:qp3w5h9hp9ude4vjpllsm4qpe8rcc5dmeealkl0cnxlgtj4ly7rczqxcdamvr",
            "kaspatest:qpqen78dezzj4w7rae4n6kvahlr6wft7jy3lcul78709asxksgxc2kr9fgv6j",
            "kaspatest:qq7upgj3g8klaylc4etwhlmr70t24wu4n4qrlayuw44yd8wx40seje27ah2x7",
            "kaspatest:qqt2jzgzwy04j8np6ne4g0akmq4gj3fha0gqupr2mjj95u5utzxqvv33mzpcu",
            "kaspatest:qpcnt3vscphae5q8h576xkufhtuqvntg0ves8jnthgfaxy8ajek8zz3jcg4de",
            "kaspatest:qz7wzgzvnadgp6v4u6ua9f3hltaa3cv8635mvzlepa63ttt72c6m208g48q0p",
            "kaspatest:qpqtsd4flc0n4g720mjwk67tnc46xv9ns5xs2khyvlvszy584ej4xq9adw9h9",
            "kaspatest:qq4uy92hzh9eauypps060g2k7zv2xv9fsgc5gxkwgsvlhc7tw4a3gk5rnpc0k",
            "kaspatest:qqgfhd3ur2v2xcf35jggre97ar3awl0h62qlmmaaq28dfrhwzgjnxntdugycr",
            "kaspatest:qzuflj6tgzwjujsym9ap6dvqz9zfwnmkta68fjulax09clh8l4rfslj9j9nnt",
            "kaspatest:qz6645a8rrf0hmrdvyr9uj673lrr9zwhjvvrytqpjsjdet23czvc784e84lfe",
            "kaspatest:qz2fvhmk996rmmg44ht0s79gnw647ehu8ncmpf3sf6txhkfmuzuxssceg9sw0",
            "kaspatest:qr9aflwylzdu99z2z25lzljyeszhs7j02zhfdazydgahq2vg6x8w7nfp3juqq",
            "kaspatest:qzen7nh0lmzvujlye5sv3nwgwdyew2zp9nz5we7pay65wrt6kfxd6khwja56q",
            "kaspatest:qq74jrja2mh3wn6853g8ywpfy9nlg0uuzchvpa0cmnvds4tfnpjj5tqgnqm4f",
        ]
    }

    fn gen0_receive_keys() -> Vec<&'static str> {
        vec![
            "269b7650e8a3b37472b353e6e8331a4427c6f081ff51ba6adf9ef203aa346845",
            "b8e4a2ee20e0c9c0d380c89ffc3d84c8fef6f768cd4c9ac7778aa783a9c70aa4",
            "ce7a150989f19d4fc00f44e88c55d40f7416364e265e4561063b0b0d753a72a1",
            "3459868739a23c6a6157ab20300dcb6714c2c4977b07721ca143d4d214b15ff2",
            "40c8e90184d2f2c2721b80a34cbea60e07bdf396c0367005e904bb8caee5ec63",
            "a02968ebbe44f9a1543c46cf93a41da1e2cd6d4f2176c6bba7b871ae52bd40fd",
            "8a9bd04793504af4d146f66fbc3b4b91f8d44c36eff41ac2b6650e1760099506",
            "a8c38dc42ee94dd569fc0baa115832a2ecd49c970058e703e650565bffb4e30f",
            "11f96f263a50a8f7a8d434635ec8026db9e2ffc8cadd996ad4c4a7af5ecebbc3",
            "b2f7aa8fd4c171865d517765485b3f5ab6c76a51a22e51c6bfc3099e08a533db",
            "6af8dc2a19abbbb2aa3d08b53481c5d88991463d4aacb2d7f4b2fc76368ee90c",
            "510d09240cd33ba17ef2e3bac206c59f6f0f604e8fe7766b989ee0fa651307e6",
            "d3d3e41bd7b764fc940af5b82ddc91e8a717f404e41839081f86860841954b1d",
            "aa0268ab215e1df65f4697f13af20d5d4a896d8ed98ad4764d079c4cad6142d3",
            "12bed3b829a881a50d5fa8a8a6a9fd28f8f2f2dc5cdb3f31d4e8c41a405ba7bb",
            "c71614ecb6f369b0379566cceba6bed0fd90336a298cd460a6b305879c3ec884",
            "65717bf1c74589e6f98f157434ec45c606c76f5ba882e6e6943e272ac159a5d3",
            "be6815e86d1df8823c93037e7b179fc4d57929cd49681c59eeacf4b0904ed844",
            "513f5e59508c6cde7a404d45394f32537872e8e9093dd5fd1768c8fbcac07dc0",
            "f0af3b29f2074838d394288a4a3bcd1cd00dc045e8f15e70eb3e70b4d5856075",
        ]
    }

    fn gen0_change_addresses() -> Vec<&'static str> {
        vec![
            "kaspatest:qrc0xjaq00fq8qzvrudfuk9msag7whnd72nefwq5d07ks4j4d97kzm0x3ertv",
            "kaspatest:qpf00utzmaa2u8w9353ssuazsv7fzs605eg00l9luyvcwzwj9cx0z4m8n9p5j",
            "kaspatest:qrkxek2q6eze7lhg8tq0qw9h890lujvjhtnn5vllrkgj2rgudl6xv3ut9j5mu",
            "kaspatest:qrn0ga4lddypp9w8eygt9vwk92lagr55e2eqjgkfr09az90632jc6namw09ll",
            "kaspatest:qzga696vavxtrg0heunvlta5ghjucptll9cfs5x0m2j05s55vtl36uhpauwuk",
            "kaspatest:qq8ernhu26fgt3ap73jalhzl5u5zuergm9f0dcsa8uy7lmcx875hwl3r894fp",
            "kaspatest:qrauma73jdn0yfwspr7yf39recvjkk3uy5e4309vjc82qq7sxtskjphgwu0sx",
            "kaspatest:qzk7yd3ep4def7sv7yhl8m0mr7p75zclycrv0x0jfm0gmwte23k0u5f9dclzy",
            "kaspatest:qzvm7mnhpkrw52c4p85xd5scrpddxnagzmhmz4v8yt6nawwzgjtavu84ft88x",
            "kaspatest:qq4feppacdug6p6zk2xf4rw400ps92c9h78gctfcdlucvzzjwzyz7j650nw52",
            "kaspatest:qryepg9agerq4wdzpv39xxjdytktga53dphvs6r4fdjc0gfyndhk7ytpnl5tv",
            "kaspatest:qpywh5galz3dd3ndkx96ckpvvf5g8t4adaf0k58y4kgf8w06jt5myjrpluvk6",
            "kaspatest:qq32grys34737mfe5ud5j2v03cjefynuym27q7jsdt28qy72ucv3sv0teqwvm",
            "kaspatest:qper47ahktzf9lv67a5e9rmfk35pq4xneufhu97px6tlzd0d4qkaklx7m3f7w",
            "kaspatest:qqal0t8w2y65a4lm5j5y4maxyy4nuwxj6u364eppj5qpxz9s4l7tknfw0u6r3",
            "kaspatest:qr7p66q7lmdqcf2vnyus38efx3l4apvqvv5sff66n808mtclef2w7vxh3afnn",
            "kaspatest:qqx4xydd58qe5csedz3l3q7v02e49rwqnydc425d6jchv02el2gdv4055vh0y",
            "kaspatest:qzyc9l5azcae7y3yltgnl5k2dzzvngp90a0glsepq0dnz8dvp4jyveezpqse8",
            "kaspatest:qq705x6hl9qdvr03n0t65esevpvzkkt2xj0faxp6luvd2hk2gr76chxw8xhy5",
            "kaspatest:qzufchm3cy2ej6f4cjpxpnt3g7c2gn77c320qhrnrjqqskpn7vnzsaxg6z0kd",
        ]
    }

    fn gen0_change_keys() -> Vec<&'static str> {
        vec![
            "585820ee35dea0fae75c2bcc101875e32e0a73a7a72eb837a266f2970901b4d1",
            "5c65fda0e8ca7f1c1d1bb0347c010e881fc3e1b8550d8695b02d631568ca67c2",
            "4d04d7bdaf6308a5100497cc43e62c86b970153aff585bcccdbc9e9272b8a6c7",
            "98dcc3d7a83e6bab4005587868beda4364865b4c93f2727de5cf750e2ebb8cb8",
            "294a4087dc41a755afb47ebd23e6cc253e4d3c502cc79d5556105cd521c54a8f",
            "0645287dfd9c325505993a7d824f23ffb6002c4cfac19df29cc12a5b263fee5b",
            "bda676ee28af75b5b59bbadd823315b53b7e9e3d20c222bd307a3c49b76e2b47",
            "4e7626010f65d21bca852eb9d316a3a1088a04d95f4f9d7c94942eb461e2f660",
            "2d07da1b5599a58116ff12b090e458376e4abec4318f6ffdd7209ada7375e495",
            "8fac76d8267f453b11733fc7edd61f89cb59112112dea1b6f07a6928f8173b55",
            "fa2b253b20158ccb09dcb8fb4b0186e9a7c7a58096e1a3cadc382fae581abc07",
            "c6515d1e078295c93c2b0ca8c85c5910b5af79e8bdaa8bb2e86db449e988d074",
            "37c29e6d16573e613b216e38e57b719499244cf05d6dd0160f9144434002fc4d",
            "97cc07d5e059f53070a22eb4cc9cd96be302da44712667ae84fa5c526d39f2d0",
            "ec2d44d434be19e71d669e54b9f2cf23872e07ad73f0eb03d2011358a98342a3",
            "c14826adc08300e154733cc1b3f2631e95504bcc9bc0ef177088aacc6c19658a",
            "5c0284f38aed77a50836b1b4425bd8c650ddbcdacfb6f2b9d9cf9962f2ba128c",
            "178a20f914a3215aa18166431859aac30c478d14610171e51dea41e5af35c03c",
            "762763953155c98f42c29210a04866873be366b801904add1dd023fce39a7b81",
            "0e7b3c72aff5cb6e3a963a4a89240082c1fc87b3bfbc964969c5c2aeb86f4490",
        ]
    }

    fn bytes_str(bytes: &[u8]) -> String {
        let mut hex = [0u8; 64];
        faster_hex::hex_encode(bytes, &mut hex).expect("The output is exactly twice the size of the input");
        unsafe { std::str::from_utf8_unchecked(&hex) }.to_string()
    }

    #[tokio::test]
    async fn gen0_prv_keys() {
        let receive_addresses = gen0_receive_addresses()
            .iter()
            .enumerate()
            .map(|(index, str)| (Address::try_from(*str).unwrap(), index as u32))
            .collect::<Vec<(Address, u32)>>();

        let change_addresses = gen0_change_addresses()
            .iter()
            .enumerate()
            .map(|(index, str)| (Address::try_from(*str).unwrap(), index as u32))
            .collect::<Vec<(Address, u32)>>();

        let receive_addresses = receive_addresses.iter().map(|(a, index)| (a, *index)).collect::<Vec<(&Address, u32)>>();
        let change_addresses = change_addresses.iter().map(|(a, index)| (a, *index)).collect::<Vec<(&Address, u32)>>();

        let key = "xprv9s21ZrQH143K2SDYtUz6dphDH3yRLAC7Jc552GYiXai3STvqgc3JBZxH2M4KaKhriaZDSS9KL7zUi5kYpggFspkiZBYWNCxbp27CCcnsJUs";
        let xkey = ExtendedPrivateKey::<SecretKey>::from_str(key).unwrap();

        let receive_keys = gen0_receive_keys();
        let change_keys = gen0_change_keys();

        let keys = create_private_keys(&LEGACY_ACCOUNT_KIND.into(), 0, 0, &xkey, &receive_addresses, &[]).unwrap();
        for (index, (a, key)) in keys.iter().enumerate() {
            let address = PubkeyDerivationManagerV0::create_address(&key.get_public_key(), Prefix::Testnet, false).unwrap();
            assert_eq!(*a, &address, "receive address at {index} failed");
            assert_eq!(bytes_str(&key.to_bytes()), receive_keys[index], "receive key at {index} failed");
        }

        let keys = create_private_keys(&LEGACY_ACCOUNT_KIND.into(), 0, 0, &xkey, &[], &change_addresses).unwrap();
        for (index, (a, key)) in keys.iter().enumerate() {
            let address = PubkeyDerivationManagerV0::create_address(&key.get_public_key(), Prefix::Testnet, false).unwrap();
            assert_eq!(*a, &address, "change address at {index} failed");
            assert_eq!(bytes_str(&key.to_bytes()), change_keys[index], "change key at {index} failed");
        }
    }
}
